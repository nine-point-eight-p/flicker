use libafl_bolts::rands::Rand;

use enum_dispatch::enum_dispatch;
use enum_downcast::EnumDowncast;

use super::utility::*;
use super::{
    ArgGenerator, ArrayType, ByteBuffer, Field, FilenameBuffer, FlagType, IntType, PointerType,
    ResourceType, StringBuffer, StructType, UnionType,
};
use crate::program::{
    call::{Arg, Call, ConstArg},
    context::Context,
};

#[enum_dispatch]
pub trait ArgMutator {
    /// Mutate an existing argument for this field type
    fn mutate<R: Rand>(&self, rand: &mut R, ctx: &mut Context, arg: &mut Arg) -> Vec<Call>;
}

impl ArgMutator for Field {
    fn mutate<R: Rand>(&self, rand: &mut R, ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
        self.ty.mutate(rand, ctx, arg)
    }
}

impl ArgMutator for IntType {
    fn mutate<R: Rand>(&self, rand: &mut R, _ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
        let arg = arg.enum_downcast_mut::<ConstArg>().unwrap();

        arg.0 = if binary(rand) {
            self.generate_impl(rand)
        } else {
            if one_of(rand, 3) {
                arg.0.wrapping_add(rand.below(4) as u64 + 1)
            } else if one_of(rand, 2) {
                arg.0.wrapping_sub(rand.below(4) as u64 + 1)
            } else {
                arg.0 ^ (1 << rand.below(self.bits as usize))
            }
        };

        vec![]
    }
}

impl ArgMutator for FlagType {
    fn mutate<R: Rand>(&self, rand: &mut R, ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
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

impl ArgMutator for ArrayType {
    fn mutate<R: Rand>(&self, rand: &mut R, ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
        todo!("ArrayType::mutate")
    }
}

impl ArgMutator for PointerType {
    fn mutate<R: Rand>(&self, rand: &mut R, ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
        todo!("PointerType::mutate")
    }
}

impl ArgMutator for StringBuffer {
    fn mutate<R: Rand>(&self, rand: &mut R, ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
        todo!("StringBuffer::mutate")
    }
}

impl ArgMutator for FilenameBuffer {
    fn mutate<R: Rand>(&self, rand: &mut R, ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
        todo!("FilenameBuffer::mutate")
    }
}

impl ArgMutator for ByteBuffer {
    fn mutate<R: Rand>(&self, rand: &mut R, ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
        todo!("ByteBuffer::mutate")
    }
}

impl ArgMutator for StructType {
    fn mutate<R: Rand>(&self, rand: &mut R, ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
        todo!("StructType::mutate")
    }
}

impl ArgMutator for UnionType {
    fn mutate<R: Rand>(&self, rand: &mut R, ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
        todo!("UnionType::mutate")
    }
}

impl ArgMutator for ResourceType {
    fn mutate<R: Rand>(&self, rand: &mut R, ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
        // TODO: What to do with the old resource?
        let (new_arg, new_calls) = self.generate(rand, ctx);
        *arg = new_arg;
        new_calls
    }
}
