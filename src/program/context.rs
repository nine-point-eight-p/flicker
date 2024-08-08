use syzlang_parser::parser::{Arch, Parsed};

use super::call::{Call, CallResult, CallResultId};
use super::metadata::SyscallMetadata;
use super::syscall::{Syscall, Type};

pub struct Context {
    // Static data
    metadata: SyscallMetadata,

    // Dynamic data
    pub generating_resource: bool,
    results: Vec<CallResult>,
    result_allocator: IndexAllocator,
}

impl Context {
    pub fn new(metadata: SyscallMetadata) -> Self {
        Self {
            metadata,
            generating_resource: false,
            results: vec![],
            result_allocator: IndexAllocator::new(),
        }
    }

    pub fn with_calls(metadata: SyscallMetadata, calls: &[Call]) -> Self {
        let mut results = vec![];
        for call in calls.iter() {
            if let Some(id) = call.result() {
                let ty = metadata
                    .syscalls()
                    .iter()
                    .find(|s| s.number() == call.number())
                    .unwrap()
                    .return_type()
                    .unwrap()
                    .clone();
                results.push(CallResult::new(ty, id));
            }
        }
        let result_allocator = IndexAllocator::from_index(results.len());

        Self {
            metadata,
            generating_resource: false,
            results,
            result_allocator,
        }
    }

    pub fn syscalls(&self) -> &[Syscall] {
        self.metadata.syscalls()
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

struct IndexAllocator(usize);

impl IndexAllocator {
    pub fn new() -> Self {
        Self(0)
    }

    pub fn from_index(index: usize) -> Self {
        Self(index)
    }

    pub fn alloc(&mut self) -> usize {
        let idx = self.0;
        self.0 += 1;
        idx
    }
}
