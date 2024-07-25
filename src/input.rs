use std::hash::{BuildHasher, Hasher};

use libafl::inputs::{HasTargetBytes, Input};
use libafl_bolts::{impl_serdeany, ownedref::OwnedSlice, HasLen};

use ahash::RandomState;
use serde::{Deserialize, Serialize};

use crate::program::call::{Call, HasBytes};

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
        for call in &self.calls {
            // TODO: syscall number is not enough
            hasher.write(&call.number().to_le_bytes());
        }
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
        let mut bytes = Vec::new();
        for call in &self.calls {
            bytes.extend(call.number().to_le_bytes().into_iter());
            for arg in call.args() {
                bytes.extend(arg.bytes().into_iter());
            }
        }
        OwnedSlice::from(bytes)
    }
}
