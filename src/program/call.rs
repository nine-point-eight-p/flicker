use enum_dispatch::enum_dispatch;
use enum_downcast::EnumDowncast;
use enum_index::EnumIndex;
use postcard::to_stdvec;
use serde::{Deserialize, Serialize};
use syscall2struct_helpers::{Pointer, Result};
use uuid::Uuid;

/// Serialize to testcase bytes for execution on the target.
/// Used as a helper for [`libafl::inputs::HasTargetBytes`].
/// Currently should be implemented with [`postcard`] for harness
/// to deserialize the bytes.
#[enum_dispatch]
pub trait ToExecBytes {
    /// Byte representation of this object.
    fn to_exec_bytes(&self) -> Vec<u8>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Call {
    nr: u32,
    args: Vec<Arg>,
    result: Option<Uuid>,
}

impl Call {
    pub fn new(nr: u32, args: Vec<Arg>, result: Option<Uuid>) -> Self {
        Self { nr, args, result }
    }

    pub fn number(&self) -> u32 {
        self.nr
    }

    pub fn args(&self) -> &[Arg] {
        &self.args
    }

    pub fn args_mut(&mut self) -> &mut Vec<Arg> {
        &mut self.args
    }

    pub fn result(&self) -> Option<Uuid> {
        self.result
    }
}

impl ToExecBytes for Call {
    fn to_exec_bytes(&self) -> Vec<u8> {
        let mut bytes = to_stdvec(&self.nr).unwrap();
        bytes.extend(self.args.iter().flat_map(|arg| arg.to_exec_bytes()));
        if let Some(id) = &self.result {
            bytes.extend(to_stdvec(id).unwrap());
        }
        bytes
    }
}

#[enum_dispatch(ToExecBytes)]
#[derive(Debug, Clone, Serialize, Deserialize, EnumDowncast)]
pub enum Arg {
    ConstArg,
    PointerArg,
    DataArg,
    GroupArg,
    ResultArg,
}

// impl Arg {
//     pub fn for_each_subarg<F>(&self, mut f: F)
//     where
//         F: FnMut(&Arg),
//     {
//         match self {
//             Arg::GroupArg(group) => {
//                 for arg in group.0.iter() {
//                     arg.for_each_subarg(&mut f);
//                 }
//             }
//             _ => f(self),
//         }
//     }
// }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConstArg(pub u64);

impl ConstArg {
    pub fn new(value: u64) -> Self {
        Self(value)
    }
}

impl ToExecBytes for ConstArg {
    fn to_exec_bytes(&self) -> Vec<u8> {
        to_stdvec(&self.0).unwrap()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PointerArg {
    /// Special address
    Addr(u64),
    /// Pointee
    Data(Box<Arg>),
}

impl PointerArg {
    pub fn from_addr(addr: u64) -> Self {
        assert_eq!(addr, 0, "Only null pointer is supported for now");
        Self::Addr(addr)
    }

    pub fn from_res(res: Arg) -> Self {
        Self::Data(Box::new(res))
    }
}

impl Default for PointerArg {
    fn default() -> Self {
        Self::Addr(0)
    }
}

impl ToExecBytes for PointerArg {
    fn to_exec_bytes(&self) -> Vec<u8> {
        // HACK: Pointers in harness are represented by `Pointer<T>` enum.
        // We should ensure that harness can deserialize to get `Pointer<T>`.
        // One way is to convert `PointerArg` to `Pointer<T>` directly, but it is hard
        // when `T` is some struct type, which we don't have in this crate.
        // So we serialize directly to bytes according to postcard's format,
        // with u32 enum index followed by data.
        let idx = match &self {
            // Dummy values for the type `T` and data
            PointerArg::Addr(_) => Pointer::<u32>::Addr(0).enum_index() as u32,
            PointerArg::Data(_) => Pointer::Data(0).enum_index() as u32,
        };
        let idx = to_stdvec(&idx).unwrap();

        let data = match &self {
            PointerArg::Addr(addr) => to_stdvec(addr).unwrap(),
            PointerArg::Data(data) => data.to_exec_bytes(),
        };

        [idx, data].concat()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataArg {
    In(Vec<u8>),
    Out(u64),
}

impl ToExecBytes for DataArg {
    fn to_exec_bytes(&self) -> Vec<u8> {
        match &self {
            DataArg::In(data) => to_stdvec(data.as_slice()).unwrap(),
            // DataArg::Out(size) => to_stdvec(size).unwrap(),
            DataArg::Out(_) => todo!("Serialize DataArg::Out"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupArg(Vec<Arg>);

impl GroupArg {
    pub fn new(args: Vec<Arg>) -> Self {
        Self(args)
    }
}

impl ToExecBytes for GroupArg {
    fn to_exec_bytes(&self) -> Vec<u8> {
        self.0.iter().flat_map(|arg| arg.to_exec_bytes()).collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResultArg {
    Ref(Uuid),
    Literal(u64),
}

impl ResultArg {
    pub fn from_result(id: Uuid) -> Self {
        Self::Ref(id)
    }

    pub fn from_literal(literal: u64) -> Self {
        Self::Literal(literal)
    }

    pub fn uses_result(&self, id: Uuid) -> bool {
        matches!(self, ResultArg::Ref(self_id) if id == *self_id)
    }
}

impl ToExecBytes for ResultArg {
    fn to_exec_bytes(&self) -> Vec<u8> {
        // HACK: This is the similar to `PointerArg` serialization.
        let idx = match &self {
            ResultArg::Ref(_) => Result::Ref(Uuid::default()).enum_index() as u32,
            ResultArg::Literal(_) => Result::Value(0).enum_index() as u32,
        };
        let idx = to_stdvec(&idx).unwrap();

        let data = match &self {
            ResultArg::Ref(id) => to_stdvec(id).unwrap(),
            ResultArg::Literal(literal) => to_stdvec(literal).unwrap(),
        };

        [idx, data].concat()
    }
}
