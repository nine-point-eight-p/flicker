use std::rc::Rc;

use enum_dispatch::enum_dispatch;
use enum_downcast::EnumDowncast;
use serde::{Deserialize, Serialize};

use super::syscall::{Direction, Syscall, Type};

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

/// Can be represented with a slice of bytes.
/// Used as a helper for [`libafl::inputs::HasTargetBytes`].
#[enum_dispatch]
pub trait HasBytes {
    /// Byte representation of this object.
    fn bytes(&self) -> Vec<u8>;
}

#[enum_dispatch(HasBytes)]
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

impl HasBytes for ConstArg {
    fn bytes(&self) -> Vec<u8> {
        self.0.to_le_bytes().to_vec()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointerArg {
    addr: u64,
    res: Box<Arg>,
}

impl HasBytes for PointerArg {
    fn bytes(&self) -> Vec<u8> {
        self.addr.to_le_bytes().to_vec()
    }
}

impl PointerArg {
    pub fn new(addr: u64, res: Box<Arg>) -> Self {
        Self { addr, res }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupArg(Vec<Arg>);

impl GroupArg {
    pub fn new(args: Vec<Arg>) -> Self {
        Self(args)
    }
}

impl HasBytes for GroupArg {
    fn bytes(&self) -> Vec<u8> {
        self.0.iter().flat_map(|arg| arg.bytes()).collect()
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

impl HasBytes for ResultArg {
    fn bytes(&self) -> Vec<u8> {
        match &self.0 {
            ResultArgInner::Ref(id) => id.to_le_bytes().to_vec(),
            ResultArgInner::Literal(literal) => literal.to_le_bytes().to_vec(),
        }
    }
}
