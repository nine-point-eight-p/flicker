use std::hash::{BuildHasher, Hasher};

use libafl::inputs::Input;
use libafl_bolts::HasLen;

use ahash::RandomState;
use serde::{Deserialize, Serialize};

use crate::program::call::Call;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyscallInput {
    calls: Vec<Call>,
}

impl SyscallInput {
    pub fn new(calls: Vec<Call>) -> Self {
        Self { calls }
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
