use std::marker::PhantomData;

use libafl::HasMetadata;
use libafl::{generators::Generator, state::HasRand, Error};
use libafl_bolts::rands::Rand;

use crate::input::SyscallInput;
use crate::program::{
    call::{Arg, Call},
    context::Context,
    syscall::{ArgGenerator, Field, Syscall},
};

pub struct SyscallGenerator<S>
where
    S: HasRand + HasMetadata,
{
    max_size: usize,
    context: Context,
    _phantom: PhantomData<S>,
}

impl<S> Generator<SyscallInput, S> for SyscallGenerator<S>
where
    S: HasRand + HasMetadata,
{
    fn generate(&mut self, state: &mut S) -> Result<SyscallInput, Error> {
        let rand = state.rand_mut();
        let mut calls = vec![];
        let mut size = 0;
        while size < self.max_size {
            let syscall = rand.choose(self.context.syscalls().iter()).unwrap().clone();
            let new_calls = generate_call(rand, &mut self.context, &syscall);
            size += new_calls.len();
            calls.extend(new_calls);
        }
        Ok(SyscallInput::new(calls))
    }
}

impl<S> SyscallGenerator<S>
where
    S: HasRand + HasMetadata,
{
    pub fn new(max_size: usize, context: Context) -> Self {
        Self {
            max_size,
            context,
            _phantom: PhantomData,
        }
    }
}

pub fn generate_call<R: Rand>(rand: &mut R, ctx: &mut Context, syscall: &Syscall) -> Vec<Call> {
    let (args, mut calls) = generate_args(rand, ctx, syscall.fields());
    let id = syscall
        .return_type()
        .filter(|ty| ty.is_resource())
        .map(|ty| ctx.add_result(ty))
        .map(|res| res.id());
    let new_call = Call::new(syscall.number(), args, id);
    calls.push(new_call);
    calls
}

pub fn generate_args<R: Rand>(
    rand: &mut R,
    ctx: &mut Context,
    fields: &[Field],
) -> (Vec<Arg>, Vec<Call>) {
    let (args, calls): (Vec<Arg>, Vec<Vec<Call>>) = fields
        .iter()
        .map(|field| generate_arg(rand, ctx, field))
        .unzip();
    let calls = calls.into_iter().flatten().collect();
    (args, calls)
}

pub fn generate_arg<R: Rand>(rand: &mut R, ctx: &mut Context, field: &Field) -> (Arg, Vec<Call>) {
    field.generate(rand, ctx)
}
