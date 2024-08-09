use libafl_bolts::rands::Rand;

use enum_dispatch::enum_dispatch;
use uuid::Uuid;

use super::{
    ArrayType, Field, FlagType, IntType, PointerType, ResourceType, StructType, UnionType,
};
use crate::generator::{generate_arg, generate_args, generate_call};
use crate::program::{
    call::{Arg, Call, ConstArg, GroupArg, ResultArg},
    context::Context,
};

#[enum_dispatch]
pub trait ArgGenerator {
    /// Generate a new argument for this field type
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>);

    /// Generate the default value for this field type
    fn default(&self) -> Arg;
}

impl ArgGenerator for Field {
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        self.ty.generate(rand, ctx)
    }

    fn default(&self) -> Arg {
        self.ty.default()
    }
}

impl IntType {
    pub(super) fn generate_impl<R: Rand>(&self, rand: &mut R) -> u64 {
        rand.between(self.begin as usize, self.end as usize) as u64
    }
}

impl ArgGenerator for IntType {
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        let val = self.generate_impl(rand);
        (ConstArg::new(val as u64).into(), vec![])
    }

    fn default(&self) -> Arg {
        ConstArg::new(self.begin as u64).into()
    }
}

impl FlagType {
    // Ref: syzkaller/prog/rand.go
    pub(super) fn generate_impl<R: Rand>(&self, rand: &mut R, old_val: u64) -> u64 {
        if one_of(rand, 100) {
            return rand.next();
        }
        if one_of(rand, 50) {
            return 0;
        }

        // Slightly increment/decrement the old value
        if !self.is_bitmask && old_val != 0 && one_of(rand, 100) {
            let inc: u64 = if binary(rand) { 1 } else { u64::MAX };
            let mut val = old_val + inc;
            while binary(rand) {
                val += inc;
            }
            return val;
        }

        // Single value or 0
        if self.values.len() == 1 {
            return if binary(rand) { 0 } else { self.values[0] };
        }

        // Enumeration flags: randomly choose one
        if !self.is_bitmask && !one_of(rand, 10) {
            return self.values[rand.below(self.values.len())];
        }

        // Don't know why it returns 0 here...
        if one_of(rand, self.values.len() + 4) {
            return 0;
        }

        // Bitmask flags: flip random bits
        let mut val = if old_val != 0 && one_of(rand, 10) {
            0
        } else {
            old_val
        };
        for _ in 0..10 {
            if val != 0 && n_out_of(rand, 1, 3) {
                break;
            }
            let mut flag = self.values[rand.below(self.values.len())];
            if one_of(rand, 20) {
                // Try choosing adjacent bit values in case we forgot
                // to add all relevant flags to the descriptions
                if binary(rand) {
                    flag >>= 1;
                } else {
                    flag <<= 1;
                }
            }
            val ^= flag;
        }
        val
    }
}

impl ArgGenerator for FlagType {
    fn generate<R: Rand>(&self, rand: &mut R, _ctx: &mut Context) -> (Arg, Vec<Call>) {
        let val = self.generate_impl(rand, 0);
        (ConstArg::new(val).into(), vec![])
    }

    fn default(&self) -> Arg {
        ConstArg::new(0).into()
    }
}

impl ArgGenerator for ArrayType {
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        let size = rand.between(self.begin.try_into().unwrap(), self.end.try_into().unwrap());
        assert!(size > 0);
        let (args, calls): (Vec<Arg>, Vec<Vec<Call>>) =
            (0..size).map(|_| self.elem.generate(rand, ctx)).unzip();
        let arg = GroupArg::new(args).into();
        let calls = calls.into_iter().flatten().collect();
        (arg, calls)
    }

    fn default(&self) -> Arg {
        let args = if self.begin == self.end {
            vec![self.elem.default(); self.begin as usize]
        } else {
            vec![]
        };
        GroupArg::new(args).into()
    }
}

impl ArgGenerator for PointerType {
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        todo!("PointerType::generate")
    }

    fn default(&self) -> Arg {
        todo!("PointerType::default")
    }
}

impl ArgGenerator for StructType {
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        let (args, calls) = generate_args(rand, ctx, &self.fields);
        let arg = GroupArg::new(args).into();
        (arg, calls)
    }

    fn default(&self) -> Arg {
        let args: Vec<Arg> = self.fields.iter().map(|f| f.default()).collect();
        GroupArg::new(args).into()
    }
}

impl ArgGenerator for UnionType {
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        let field = &self.fields[rand.below(self.fields.len())];
        generate_arg(rand, ctx, field)
    }

    fn default(&self) -> Arg {
        let field = &self.fields[0];
        field.default()
    }
}

impl ResourceType {
    /// Create a new resource by generating a syscall
    fn create_resource<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        // Find a syscall that creates this resource
        let resource_creators = ctx
            .syscalls()
            .iter()
            .filter(|s| {
                s.return_type()
                    .is_some_and(|ty| ty.is_compatible_resource(&self.name))
            })
            .map(|s| s.clone());
        let syscall = rand
            .choose(resource_creators)
            .expect("No syscall to create resource");

        // Generate the syscall and the argument
        let calls = generate_call(rand, ctx, &syscall);
        let arg = ResultArg::from_result(calls.last().unwrap().result().unwrap()).into();

        (arg, calls)
    }

    /// Create a resource by loading the initializations from the corpus
    fn load_resource<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> Option<(Arg, Vec<Call>)> {
        None // TODO: Implement ResourceType::load_resource
    }

    /// Use an existing resource
    fn use_existing_resource<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> Option<Arg> {
        // Find compatible resources
        let results: Vec<&Uuid> = ctx
            .results()
            .filter(|(_, ty)| ty.is_compatible_resource(&self.name))
            .map(|(id, _)| id)
            .collect();
        // Randomly choose one if there is any
        (!results.is_empty())
            .then_some(ResultArg::from_result(*results[rand.below(results.len())]).into())
    }
}

impl ArgGenerator for ResourceType {
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        // Check if we can recurse
        let can_recurse = if ctx.generating_resource {
            false
        } else {
            ctx.generating_resource = true;
            true
        };

        // Use an existing resource with a high probability
        if can_recurse && n_out_of(rand, 4, 5) || !can_recurse && n_out_of(rand, 19, 20) {
            if let Some(arg) = self.use_existing_resource(rand, ctx) {
                return (arg, vec![]);
            }
        }
        // Create a new resource if we can recurse
        if can_recurse {
            if one_of(rand, 4) {
                if let Some((arg, calls)) = self.load_resource(rand, ctx) {
                    return (arg, calls);
                }
            }
            if n_out_of(rand, 4, 5) {
                return self.create_resource(rand, ctx);
            }
        }
        // Fallback: use special values
        let val = self.values[rand.below(self.values.len())];
        (ResultArg::from_literal(val).into(), vec![])
    }

    fn default(&self) -> Arg {
        ResultArg::from_literal(self.values[0]).into()
    }
}

#[inline]
fn binary<R: Rand>(rand: &mut R) -> bool {
    rand.below(2) == 0
}

#[inline]
fn one_of<R: Rand>(rand: &mut R, n: usize) -> bool {
    rand.below(n) == 0
}

#[inline]
fn n_out_of<R: Rand>(rand: &mut R, n: usize, total: usize) -> bool {
    debug_assert!(0 < n && n < total);
    rand.below(total) < n
}
