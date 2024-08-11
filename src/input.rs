use std::hash::{BuildHasher, Hasher};

use libafl::inputs::{HasTargetBytes, Input};
use libafl_bolts::{ownedref::OwnedSlice, HasLen};

use ahash::RandomState;
use postcard::to_allocvec;
use serde::{Deserialize, Serialize};

use crate::program::{
    call::{Arg, Call, ToExecBytes},
    metadata::SyscallMetadata,
    syscall::ArgGenerator,
};

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

    pub fn get(&self, idx: usize) -> &Call {
        &self.calls[idx]
    }

    pub fn get_mut(&mut self, idx: usize) -> &mut Call {
        &mut self.calls[idx]
    }

    pub fn take(self) -> Vec<Call> {
        self.calls
    }

    /// Splice calls from the given iterator into the input at the given position.
    /// This will remove any calls after the given position.
    pub fn splice(&mut self, idx: usize, calls: impl IntoIterator<Item = Call>) {
        // Because any calls after the position are removed, we can simply
        // truncate the vector without taking care of the removed calls,
        // like we would have to do with `remove`.
        self.calls.truncate(idx);
        self.calls.extend(calls);
    }

    /// Insert calls from the given iterator into the input at the given position.
    pub fn insert(&mut self, idx: usize, calls: impl IntoIterator<Item = Call>) {
        self.calls.splice(idx..idx, calls);
    }

    /// Remove the call at the given index.
    pub fn remove(&mut self, idx: usize, metadata: &SyscallMetadata) {
        let call = self.calls.remove(idx);

        // Remove any result arguments that use the result of the removed call
        if let Some(id) = call.result() {
            let return_type = metadata
                .find_number(call.number())
                .unwrap()
                .return_type()
                .unwrap();
            debug_assert!(return_type.is_resource());
            // TODO: Check recursively for group args
            self.calls[idx..]
                .iter_mut()
                .flat_map(|c| c.args_mut())
                .for_each(|arg| match arg {
                    Arg::ResultArg(inner) if inner.uses_result(id) => {
                        *arg = return_type.default();
                    }
                    _ => {}
                });
        }
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
