use std::rc::Rc;

use super::call::{CallResult, CallResultId};
use super::syscall::{Syscall, Type};

pub struct Context {
    // Static data
    syscalls: Vec<Rc<Syscall>>,

    // Dynamic data
    pub generating_resource: bool,
    results: Vec<CallResult>,
    result_allocator: IndexAllocator,
}

impl Context {
    pub fn new(syscalls: Vec<Syscall>) -> Self {
        let syscalls = syscalls.into_iter().map(Rc::new).collect();
        Self {
            syscalls,
            generating_resource: false,
            results: vec![],
            result_allocator: IndexAllocator::new(),
        }
    }

    pub fn syscalls(&self) -> &[Rc<Syscall>] {
        &self.syscalls
    }

    pub fn results(&self) -> &[CallResult] {
        &self.results
    }

    pub fn add_result(&mut self, ty: &Type) -> &CallResult {
        let id = self.result_allocator.alloc();
        let result = CallResult::new(ty.clone(), id);
        self.results.push(result);
        &self.results[id]
    }
}

#[derive(Debug, Clone)]
pub struct ResourceDesc {
    pub name: String,
    pub values: Vec<u64>,
}

pub struct FlagDesc {
    pub name: String,
    pub values: Vec<u64>,
}

struct IndexAllocator(usize);

impl IndexAllocator {
    pub fn new() -> Self {
        Self(0)
    }

    pub fn alloc(&mut self) -> usize {
        let idx = self.0;
        self.0 += 1;
        idx
    }
}
