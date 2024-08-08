use std::rc::Rc;

use enum_dispatch::enum_dispatch;
use enum_downcast::EnumDowncast;
use postcard::to_allocvec;
use serde::{Deserialize, Serialize};

use super::syscall::{Direction, Syscall, Type};

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
    res: Option<CallResultId>,
}

impl Call {
    pub fn new(nr: u32, args: Vec<Arg>, res: Option<CallResultId>) -> Self {
        Self { nr, args, res }
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

    pub fn result(&self) -> Option<CallResultId> {
        self.res
    }

    pub fn result_mut(&mut self) -> Option<&mut CallResultId> {
        self.res.as_mut()
    }
}

impl ToExecBytes for Call {
    fn to_exec_bytes(&self) -> Vec<u8> {
        let mut bytes = to_allocvec(&self.nr).unwrap();
        bytes.extend(self.args.iter().flat_map(|arg| arg.to_exec_bytes()));
        bytes
    }
}

#[derive(Debug)]
pub struct CallResult {
    ty: Type,
    id: CallResultId,
}

impl CallResult {
    pub fn new(ty: Type, id: CallResultId) -> Self {
        Self { ty, id }
    }

    pub fn ty(&self) -> &Type {
        &self.ty
    }

    pub fn id(&self) -> CallResultId {
        self.id
    }
}

pub type CallResultId = usize;

#[enum_dispatch(ToExecBytes)]
#[derive(Debug, Clone, Serialize, Deserialize, EnumDowncast)]
pub enum Arg {
    ConstArg,
    PointerArg,
    GroupArg,
    ResultArg,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub struct PointerArg {
    addr: u64,
    res: Box<Arg>,
}

impl PointerArg {
    pub fn new(addr: u64, res: Box<Arg>) -> Self {
        Self { addr, res }
    }
}

impl ToExecBytes for PointerArg {
    fn to_exec_bytes(&self) -> Vec<u8> {
        to_allocvec(&self.addr).unwrap()
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
pub struct ResultArg(ResultArgInner);

#[derive(Debug, Clone, Serialize, Deserialize)]
enum ResultArgInner {
    Ref(CallResultId),
    Literal(u64),
}

impl ResultArg {
    pub fn from_result(id: CallResultId) -> Self {
        Self(ResultArgInner::Ref(id))
    }

    pub fn from_literal(literal: u64) -> Self {
        Self(ResultArgInner::Literal(literal))
    }
}

impl ToExecBytes for ResultArg {
    fn to_exec_bytes(&self) -> Vec<u8> {
        match &self.0 {
            ResultArgInner::Ref(id) => to_allocvec(id).unwrap(),
            ResultArgInner::Literal(literal) => to_allocvec(literal).unwrap(),
        }
    }
}
