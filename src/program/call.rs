use enum_dispatch::enum_dispatch;
use enum_downcast::EnumDowncast;
use postcard::to_allocvec;
use serde::{Deserialize, Serialize};
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
        let mut bytes = to_allocvec(&self.nr).unwrap();
        bytes.extend(self.args.iter().flat_map(|arg| arg.to_exec_bytes()));
        if let Some(id) = &self.result {
            bytes.extend(to_allocvec(id).unwrap());
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
        to_allocvec(&self.0).unwrap()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PointerArg {
    Addr(u64),
    Res(Box<Arg>),
}

impl PointerArg {
    pub fn from_addr(addr: u64) -> Self {
        Self::Addr(addr)
    }

    pub fn from_res(res: Arg) -> Self {
        Self::Res(Box::new(res))
    }
}

impl ToExecBytes for PointerArg {
    fn to_exec_bytes(&self) -> Vec<u8> {
        match &self {
            PointerArg::Addr(addr) => to_allocvec(addr).unwrap(),
            PointerArg::Res(res) => res.to_exec_bytes(),
        }
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
            DataArg::In(data) => to_allocvec(data).unwrap(),
            DataArg::Out(size) => to_allocvec(size).unwrap(),
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
        match self {
            ResultArg::Ref(other_id) => id == *other_id,
            _ => false,
        }
    }
}

impl ToExecBytes for ResultArg {
    fn to_exec_bytes(&self) -> Vec<u8> {
        match &self {
            ResultArg::Ref(id) => to_allocvec(id).unwrap(),
            ResultArg::Literal(literal) => to_allocvec(literal).unwrap(),
        }
    }
}
