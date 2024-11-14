use std::num::NonZeroUsize;

use libafl::{
    inputs::{BytesInput, HasMutatorBytes},
    mutators::{havoc_mutations_no_crossover, MutationResult, MutatorsTuple},
    state::{HasRand, NopState},
};
use libafl_bolts::{nonzero, rands::Rand};

use enum_dispatch::enum_dispatch;
use enum_downcast::EnumDowncast;
use libafl_bolts::HasLen;

use super::generation::{rand_filename_length, MAX_BUFFER_LENGTH};
use super::{
    ArrayType, ByteBuffer, Field, FilenameBuffer, FlagType, IntType, PointerType, ResourceType,
    StringBuffer, StructType, Type, UnionType,
};
use crate::generator::generate_arg;
use crate::program::{
    call::{Arg, Call, ConstArg, DataArg, PointerArg},
    context::Context,
};
use crate::utility::*;

#[enum_dispatch]
pub trait MutateArg {
    /// Mutate an existing argument for this field type
    #[must_use = "The newly generated calls during mutation should be put back to original testcase"]
    fn mutate<R: Rand>(&self, rand: &mut R, ctx: &mut Context, arg: &mut Arg) -> Vec<Call>;
}

impl MutateArg for Field {
    fn mutate<R: Rand>(&self, rand: &mut R, ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
        self.ty.mutate(rand, ctx, arg)
    }
}

impl MutateArg for IntType {
    fn mutate<R: Rand>(&self, rand: &mut R, _ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
        let arg = arg.enum_downcast_mut::<ConstArg>().unwrap();

        arg.0 = if binary(rand) {
            self.generate_impl(rand)
        } else {
            // Refactored, but the probability is the same as syzkaller implementation
            match rand.below(nonzero!(5)) {
                0 => arg.0.wrapping_add(rand.below(nonzero!(4)) as u64 + 1),
                1 => arg.0.wrapping_sub(rand.below(nonzero!(4)) as u64 + 1),
                _ => {
                    // bits is verified when constructed
                    let shift =
                        rand.below(unsafe { NonZeroUsize::new_unchecked(self.bits as usize) });
                    arg.0 ^ (1 << shift)
                }
            }
        };

        vec![]
    }
}

impl MutateArg for FlagType {
    fn mutate<R: Rand>(&self, rand: &mut R, _ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
        let arg = arg.enum_downcast_mut::<ConstArg>().unwrap();

        loop {
            let val = self.generate_impl(rand, arg.0);
            if arg.0 != val {
                arg.0 = val;
                return vec![];
            }
        }
    }
}

impl MutateArg for ArrayType {
    fn mutate<R: Rand>(&self, _rand: &mut R, _ctx: &mut Context, _arg: &mut Arg) -> Vec<Call> {
        todo!("ArrayType::mutate")
    }
}

impl MutateArg for PointerType {
    fn mutate<R: Rand>(&self, rand: &mut R, ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
        let pointer_arg = arg.enum_downcast_mut::<PointerArg>().unwrap();

        let regenerate = one_of(rand, 3);
        match pointer_arg {
            PointerArg::Data(data) if !regenerate => self.elem.mutate(rand, ctx, data), // Mutate inner
            _ => {
                // Regenerate
                let (new_arg, new_calls) = generate_arg(rand, ctx, &Type::Pointer(self.clone()));
                *arg = new_arg;
                new_calls
            }
        }
    }
}

impl MutateArg for StringBuffer {
    fn mutate<R: Rand>(&self, rand: &mut R, ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
        let arg = arg.enum_downcast_mut::<DataArg>().unwrap();

        match arg {
            DataArg::In(data) => {
                if !self.values.is_empty() {
                    // Regenerate
                    let mut bytes = self.generate_string(rand, ctx).into_bytes();
                    bytes.truncate(MAX_BUFFER_LENGTH as usize);
                    *data = bytes;
                } else {
                    // Mutate
                    mutate_bytes(data, None);
                }
            }
            DataArg::Out(len) => {
                mutate_buffer_length(rand, len, None);
            }
        }

        vec![]
    }
}

impl MutateArg for FilenameBuffer {
    fn mutate<R: Rand>(&self, rand: &mut R, ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
        let arg = arg.enum_downcast_mut::<DataArg>().unwrap();

        match arg {
            DataArg::In(data) => {
                let mut bytes = self.generate_filename(rand, ctx).into_bytes();
                bytes.truncate(MAX_BUFFER_LENGTH as usize);
            }
            DataArg::Out(len) => {
                if one_of(rand, 100) {
                    *len = rand_filename_length(rand);
                } else {
                    mutate_buffer_length(rand, len, None);
                }
            }
        }

        vec![]
    }
}

impl MutateArg for ByteBuffer {
    fn mutate<R: Rand>(&self, rand: &mut R, _ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
        let arg = arg.enum_downcast_mut::<DataArg>().unwrap();

        match arg {
            DataArg::In(data) => {
                mutate_bytes(data, self.range);
                assert!(data.len() as u64 <= MAX_BUFFER_LENGTH);
            }
            DataArg::Out(len) => mutate_buffer_length(rand, len, self.range),
        }

        vec![]
    }
}

impl MutateArg for StructType {
    fn mutate<R: Rand>(&self, _rand: &mut R, _ctx: &mut Context, _arg: &mut Arg) -> Vec<Call> {
        todo!("StructType::mutate")
    }
}

impl MutateArg for UnionType {
    fn mutate<R: Rand>(&self, _rand: &mut R, _ctx: &mut Context, _arg: &mut Arg) -> Vec<Call> {
        todo!("UnionType::mutate")
    }
}

impl MutateArg for ResourceType {
    fn mutate<R: Rand>(&self, rand: &mut R, ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
        // TODO: What to do with the old resource?
        let (new_arg, new_calls) = generate_arg(rand, ctx, &Type::Resource(self.clone()));
        *arg = new_arg;
        new_calls
    }
}

/// Mutate some bytes with mutators from [`libafl::mutators::havoc_mutations_no_crossover`].
fn mutate_bytes(bytes: &mut Vec<u8>, range: Option<(u64, u64)>) {
    // TODO: Maybe use `static` to avoid re-creating the mutators and state every time
    let mut mutators = havoc_mutations_no_crossover();
    let mut nop_state = NopState::<BytesInput>::new();
    let mut input = BytesInput::new(bytes.to_vec());

    loop {
        let index = nop_state
            .rand_mut()
            .below(mutators.len().try_into().unwrap());
        let result = mutators
            .get_and_mutate(index.into(), &mut nop_state, &mut input)
            .unwrap();
        if result == MutationResult::Mutated && one_of(nop_state.rand_mut(), 3) {
            break;
        }
    }

    // Resize if it can not fitted into the range
    if let Some((min, max)) = range {
        let len = input.len() as u64;
        if len < min {
            input.resize(min as usize, 0);
        } else if len > max {
            input.resize(max as usize, 0);
        }
    }
    if input.len() > MAX_BUFFER_LENGTH as usize {
        input.resize(MAX_BUFFER_LENGTH as usize, 0);
    }

    bytes.resize(input.len(), 0);
    bytes.copy_from_slice(input.bytes());
}

/// Mutate the length of a buffer.
fn mutate_buffer_length<R: Rand>(rand: &mut R, old_len: &mut u64, range: Option<(u64, u64)>) {
    let (min, max) = range.unwrap_or((0, MAX_BUFFER_LENGTH));
    let mut new_len = *old_len;

    while new_len == *old_len {
        new_len += rand.below(nonzero!(33)) as u64 - 16; // [0, 33) -> [-16, 17)
        if (new_len as i64) < (min as i64) {
            new_len = min;
        }
        if new_len > max {
            new_len = max;
        }
    }

    *old_len = new_len;
}
