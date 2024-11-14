use std::borrow::Cow;
use std::iter;

use libafl::{
    corpus::Corpus,
    inputs::UsesInput,
    mutators::{MutationResult, Mutator},
    random_corpus_id,
    state::{HasCorpus, HasMaxSize, HasRand},
    Error,
};
use libafl_bolts::{
    rands::Rand,
    tuples::{tuple_list, tuple_list_type},
    HasLen, Named,
};

use log::debug;

use crate::program::context::Context;
use crate::{generator::generate_call, program::syscall::MutateArg};
use crate::{input::SyscallInput, program::metadata::SyscallMetadata};

pub struct SyscallSpliceMutator;

impl<S> Mutator<SyscallInput, S> for SyscallSpliceMutator
where
    S: UsesInput<Input = SyscallInput>
        + HasRand
        + HasCorpus<Corpus: Corpus<Input = SyscallInput>>
        + HasMaxSize,
{
    /// Splice syscalls from the corpus into the input
    fn mutate(&mut self, state: &mut S, input: &mut SyscallInput) -> Result<MutationResult, Error> {
        if state.corpus().count() == 0 || input.len() == 0 || input.len() > state.max_size() {
            return Ok(MutationResult::Skipped);
        }

        // Choose a random corpus entry and position to splice
        let id = random_corpus_id!(state.corpus(), state.rand_mut());
        let pos = state.rand_mut().below(input.len().try_into().unwrap());

        // Get the calls from the corpus entry
        let other = state.corpus().get(id)?;
        let other = other.borrow().input().as_ref().cloned().unwrap();

        // Replace input calls after the position with the calls from the corpus entry
        input.splice(pos, other.take().into_iter());

        // Truncate calls to the max size
        input.splice(state.max_size(), iter::empty());

        debug!(
            "[SyscallSpliceMutator::mutate] Spliced calls from corpus entry {} at position {}",
            id, pos
        );
        Ok(MutationResult::Mutated)
    }
}

impl Named for SyscallSpliceMutator {
    fn name(&self) -> &Cow<'static, str> {
        static NAME: Cow<'static, str> = Cow::Borrowed("SyscallSpliceMutator");
        &NAME
    }
}

pub struct SyscallInsertMutator {
    metadata: SyscallMetadata,
}

impl<S> Mutator<SyscallInput, S> for SyscallInsertMutator
where
    S: HasRand + HasMaxSize,
{
    /// Insert a random syscall into the input
    fn mutate(&mut self, state: &mut S, input: &mut SyscallInput) -> Result<MutationResult, Error> {
        if input.len() >= state.max_size() {
            return Ok(MutationResult::Skipped);
        }

        // Choose a random position to insert the new syscalls
        let pos = state.rand_mut().below(input.len().try_into().unwrap());

        // Create context at the insertion point
        let mut context = Context::with_calls(self.metadata.clone(), &input.calls()[..pos]);

        // Choose a random syscall to insert
        let idx = state
            .rand_mut()
            .below(self.metadata.syscalls().len().try_into().unwrap());
        let syscall = &self.metadata.syscalls()[idx];

        // Generate syscall
        let new_calls = generate_call(state.rand_mut(), &mut context, syscall);

        // Insert new calls
        input.insert(pos, new_calls.into_iter());

        // Truncate calls to the max size
        input.splice(state.max_size(), iter::empty());

        debug!("[SyscallInsertMutator::mutate] Inserted call at position {pos}");
        Ok(MutationResult::Mutated)
    }
}

impl Named for SyscallInsertMutator {
    fn name(&self) -> &Cow<'static, str> {
        static NAME: Cow<'static, str> = Cow::Borrowed("SyscallInsertMutator");
        &NAME
    }
}

pub struct SyscallRandMutator {
    metadata: SyscallMetadata,
}

impl<S> Mutator<SyscallInput, S> for SyscallRandMutator
where
    S: HasRand + HasMaxSize,
{
    /// Mutate a random argument of a random syscall
    fn mutate(&mut self, state: &mut S, input: &mut SyscallInput) -> Result<MutationResult, Error> {
        if input.len() == 0 {
            return Ok(MutationResult::Skipped);
        }

        // Choose a random call to mutate
        let call_pos = state.rand_mut().below(input.len().try_into().unwrap());
        let mut ctx = Context::with_calls(self.metadata.clone(), &input.calls()[..call_pos]);
        let call = input.get_mut(call_pos);
        let syscall = self
            .metadata
            .find_number(call.number())
            .expect("Syscall not found");

        // Choose a random argument to mutate
        if syscall.fields().len() == 0 {
            return Ok(MutationResult::Skipped);
        }
        let arg_pos = state
            .rand_mut()
            .below(syscall.fields().len().try_into().unwrap());
        let field = &syscall.fields()[arg_pos];
        let arg = &mut call.args_mut()[arg_pos];
        let calls = field.mutate(state.rand_mut(), &mut ctx, arg);

        // Insert new calls if any
        input.insert(call_pos, calls.into_iter());

        // Truncate calls to the max size
        input.splice(state.max_size(), iter::empty());

        debug!("[SyscallRandMutator::mutate] Mutated arg {arg_pos} of call at position {call_pos}");
        Ok(MutationResult::Mutated)
    }
}

impl Named for SyscallRandMutator {
    fn name(&self) -> &Cow<'static, str> {
        static NAME: Cow<'static, str> = Cow::Borrowed("SyscallRandMutator");
        &NAME
    }
}

pub struct SyscallRemoveMutator {
    metadata: SyscallMetadata,
}

impl<S> Mutator<SyscallInput, S> for SyscallRemoveMutator
where
    S: HasRand,
{
    /// Remove a random syscall from the input
    fn mutate(&mut self, state: &mut S, input: &mut SyscallInput) -> Result<MutationResult, Error> {
        if input.len() == 0 {
            return Ok(MutationResult::Skipped);
        }

        let pos = state.rand_mut().below(input.len().try_into().unwrap());
        input.remove(pos, &self.metadata);

        debug!("[SyscallRemoveMutator::mutate] Removed call at position {pos}",);
        Ok(MutationResult::Mutated)
    }
}

impl Named for SyscallRemoveMutator {
    fn name(&self) -> &Cow<'static, str> {
        static NAME: Cow<'static, str> = Cow::Borrowed("SyscallRemoveMutator");
        &NAME
    }
}

pub fn syscall_mutations(
    metadata: SyscallMetadata,
) -> tuple_list_type!(
    SyscallSpliceMutator,
    SyscallInsertMutator,
    SyscallRandMutator,
    SyscallRemoveMutator
) {
    tuple_list!(
        SyscallSpliceMutator,
        SyscallInsertMutator {
            metadata: metadata.clone()
        },
        SyscallRandMutator {
            metadata: metadata.clone()
        },
        SyscallRemoveMutator { metadata },
    )
}
