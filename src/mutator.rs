use std::borrow::Cow;
use std::cmp::min;

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

use crate::input::SyscallInput;
use crate::program::{context::Context, syscall::Syscall};
use crate::{generator::generate_call, program::syscall::ArgMutator};

pub struct SyscallSpliceMutator;

impl<S> Mutator<SyscallInput, S> for SyscallSpliceMutator
where
    S: UsesInput<Input = SyscallInput> + HasRand + HasCorpus + HasMaxSize,
{
    /// Splice syscalls from the corpus into the input
    fn mutate(&mut self, state: &mut S, input: &mut SyscallInput) -> Result<MutationResult, Error> {
        if state.corpus().count() == 0 || input.len() == 0 || input.len() > state.max_size() {
            return Ok(MutationResult::Skipped);
        }

        // Choose a random corpus entry and position to splice
        let id = random_corpus_id!(state.corpus(), state.rand_mut());
        let pos = state.rand_mut().below(input.len());

        // Get the calls from the corpus entry
        let other = state.corpus().get(id)?.borrow();
        let other = other.input().as_ref().unwrap();

        // Replace input calls after the position with the calls from the corpus entry
        input.calls_mut().truncate(pos);
        input.calls_mut().extend_from_slice(other.calls());

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
    syscalls: Vec<Syscall>,
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
        let pos = state.rand_mut().below(input.len());

        // Create context at the insertion point
        let mut context = Context::from_calls(self.syscalls.clone(), &input.calls()[..pos]);
        let result_size = context.results().len();

        // Choose a random syscall to insert
        let syscall = state.rand_mut().choose(self.syscalls.iter()).unwrap();

        // Generate syscall
        let new_calls = generate_call(state.rand_mut(), &mut context, syscall);
        let offset = context.results().len() - result_size;

        // Add offset to the results after the insertion point
        input.calls_mut()[pos..].iter_mut().for_each(|call| {
            call.result_mut().map(|res| *res += offset);
        });

        // Insert new calls
        input.calls_mut().splice(pos..pos, new_calls);

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
    syscalls: Vec<Syscall>,
}

impl<S> Mutator<SyscallInput, S> for SyscallRandMutator
where
    S: HasRand,
{
    /// Mutate a random argument of a random syscall
    fn mutate(&mut self, state: &mut S, input: &mut SyscallInput) -> Result<MutationResult, Error> {
        if input.len() == 0 {
            return Ok(MutationResult::Skipped);
        }

        // Choose a random call to mutate
        let pos = state.rand_mut().below(input.len());
        let mut ctx = Context::from_calls(self.syscalls.clone(), &input.calls()[..pos]);
        let call = &mut input.calls_mut()[pos];
        let syscall = self
            .syscalls
            .iter()
            .find(|s| s.number() == call.number())
            .expect("Syscall not found");

        // Choose a random argument to mutate
        let pos = state.rand_mut().below(syscall.fields().len());
        let field = &syscall.fields()[pos];
        let arg = &mut call.args_mut()[pos];
        field.mutate(state.rand_mut(), &mut ctx, arg);

        Ok(MutationResult::Mutated)
    }
}

impl Named for SyscallRandMutator {
    fn name(&self) -> &Cow<'static, str> {
        static NAME: Cow<'static, str> = Cow::Borrowed("SyscallRandMutator");
        &NAME
    }
}

pub struct SyscallRemoveMutator;

impl<S> Mutator<SyscallInput, S> for SyscallRemoveMutator
where
    S: HasRand,
{
    /// Remove a random syscall from the input
    fn mutate(&mut self, state: &mut S, input: &mut SyscallInput) -> Result<MutationResult, Error> {
        if input.len() == 0 {
            return Ok(MutationResult::Skipped);
        }

        let pos = state.rand_mut().below(input.len());
        input.calls_mut().remove(pos);
        // TODO: Check if removed syscall is a resource generator

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
    syscalls: Vec<Syscall>,
) -> tuple_list_type!(
    SyscallSpliceMutator,
    SyscallInsertMutator,
    SyscallRandMutator,
    SyscallRemoveMutator
) {
    tuple_list!(
        SyscallSpliceMutator,
        SyscallInsertMutator {
            syscalls: syscalls.clone()
        },
        SyscallRandMutator { syscalls },
        SyscallRemoveMutator
    )
}
