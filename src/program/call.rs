use std::rc::Rc;

use enum_dispatch::enum_dispatch;
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

    pub fn result(&self) -> Option<CallResultId> {
        self.res
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

#[enum_dispatch]
pub trait ArgOperation {}

#[enum_dispatch(ArgOperation)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Arg {
    ConstArg,
    PointerArg,
    GroupArg,
    ResultArg,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstArg(u64);

impl ConstArg {
    pub fn new(value: u64) -> Self {
        Self(value)
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupArg(Vec<Arg>);

impl GroupArg {
    pub fn new(args: Vec<Arg>) -> Self {
        Self(args)
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
