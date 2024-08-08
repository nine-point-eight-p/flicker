use std::hash::{BuildHasher, Hasher};

use libafl::inputs::{HasTargetBytes, Input};
use libafl_bolts::{ownedref::OwnedSlice, HasLen};

use ahash::RandomState;
use postcard::to_allocvec;
use serde::{Deserialize, Serialize};

use crate::program::call::{Call, ToExecBytes};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyscallInput {
    calls: Vec<Call>,
}

impl SyscallInput {
    pub fn new(calls: Vec<Call>) -> Self {
        Self { calls }
    }

    pub fn calls(&self) -> &[Call] {
        &self.calls
    }

    pub fn calls_mut(&mut self) -> &mut Vec<Call> {
        &mut self.calls
    }
}

impl Input for SyscallInput {
    fn generate_name(&self, idx: usize) -> String {
        let mut hasher = RandomState::with_seeds(0, 0, 0, 0).build_hasher();
        hasher.write(&self.target_bytes());
        format!("{:016x}", hasher.finish())
    }
}

impl HasLen for SyscallInput {
    fn len(&self) -> usize {
        self.calls.len()
    }
}

impl HasTargetBytes for SyscallInput {
    fn target_bytes(&self) -> OwnedSlice<u8> {
        let len = self.calls.len() as u32;
        let mut bytes = to_allocvec(&len).unwrap();
        bytes.extend(self.calls.iter().flat_map(|c| c.to_exec_bytes()));
        bytes.into()
    }
}
