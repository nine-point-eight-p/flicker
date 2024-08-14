use std::marker::PhantomData;

use libafl::HasMetadata;
use libafl::{generators::Generator, state::HasRand, Error};
use libafl_bolts::rands::Rand;

use log::info;

use crate::input::SyscallInput;
use crate::program::{
    call::{Arg, Call},
    context::Context,
    syscall::{Field, GenerateArg, Syscall, Type},
};
use crate::utility::binary;

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
        // Reset the context before generating new calls
        self.context.reset();

        // Generate calls until reaching the max size
        let rand = state.rand_mut();
        let mut calls = vec![];
        let mut size = 0;
        while size < self.max_size {
            let idx = rand.below(self.context.syscalls().len());
            let syscall = self.context.syscalls()[idx].clone();
            let new_calls = generate_call(rand, &mut self.context, &syscall);
            size += new_calls.len();
            calls.extend(new_calls);
        }

        // Truncate calls if the size exceeds the max size.
        // It is find to simply truncate, the reason is the same for SyscallInput::splice.
        if size > self.max_size {
            calls.truncate(self.max_size);
        }

        info!(
            "[SyscallGenerator::generate] Generated {} calls",
            calls.len()
        );
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
        .map(|ty| ctx.add_result(ty));
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
        .map(|field| generate_arg(rand, ctx, &field.ty))
        .unzip();
    let calls = calls.into_iter().flatten().collect();
    (args, calls)
}

pub fn generate_arg<R: Rand>(rand: &mut R, ctx: &mut Context, ty: &Type) -> (Arg, Vec<Call>) {
    if ty.attr().optional && binary(rand) {
        // Use default values for optional fields with 1/2 probability
        let arg = match ty {
            Type::Resource(res) => res.choose_fallback(rand),
            other => other.default(),
        };
        (arg, vec![])
    } else {
        ty.generate(rand, ctx)
    }
}
