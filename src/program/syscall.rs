use libafl_bolts::rands::Rand;

use enum_dispatch::enum_dispatch;

use super::call::{Arg, Call, CallResult, ConstArg, GroupArg, ResultArg};
use super::context::{Context, ResourceDesc};
use crate::generator::{generate_arg, generate_args, generate_call};

#[derive(Debug, Clone)]
pub struct Syscall {
    nr: u32,
    name: String,
    fields: Vec<Field>,
    ret: Option<Type>,
}

impl Syscall {
    pub fn number(&self) -> u32 {
        self.nr
    }

    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    pub fn return_type(&self) -> Option<&Type> {
        self.ret.as_ref()
    }
}

#[enum_dispatch]
pub trait ArgGenerator {
    /// Generate a new argument for this field type
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>);
}

#[enum_dispatch]
pub trait ArgMutator {
    /// Mutate an existing argument for this field type
    fn mutate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>);
}

#[derive(Debug, Clone)]
pub struct Field {
    name: String,
    ty: Type,
    dir: Direction,
}

impl ArgGenerator for Field {
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        self.ty.generate(rand, ctx)
    }
}

#[enum_dispatch(ArgGenerator)]
#[derive(Debug, Clone)]
pub enum Type {
    IntType,
    FlagType,
    ArrayType,
    PointerType,
    StructType,
    UnionType,
    ResourceType,
}

impl Type {
    pub fn is_integer(&self) -> bool {
        matches!(self, Type::IntType(_) | Type::FlagType(_))
    }

    pub fn is_resource(&self) -> bool {
        matches!(self, Type::ResourceType(_))
    }

    pub fn is_compatible_resource(&self, desc: &ResourceDesc) -> bool {
        matches!(self, Type::ResourceType(inner) if inner.desc.name == desc.name)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    In,
    Out,
    InOut,
}

#[derive(Debug, Clone)]
pub struct IntType {
    begin: u64,
    end: u64,
}

impl ArgGenerator for IntType {
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        let val = rand.between(self.begin.try_into().unwrap(), self.end.try_into().unwrap());
        (ConstArg::new(val as u64).into(), vec![])
    }
}

#[derive(Debug, Clone)]
pub struct FlagType {
    values: Vec<u64>,
}

impl ArgGenerator for FlagType {
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        let val = self.values[rand.below(self.values.len())];
        (ConstArg::new(val).into(), vec![])
    }
}

#[derive(Debug, Clone)]
pub struct ArrayType {
    elem: Box<Type>,
    begin: u64,
    end: u64,
    dir: Direction,
}

impl ArgGenerator for ArrayType {
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        let size = rand.between(self.begin.try_into().unwrap(), self.end.try_into().unwrap());
        assert!(size > 0);
        let elem_field = Field {
            name: "array_elem".to_string(), // doesn't matter
            ty: self.elem.as_ref().clone(),
            dir: self.dir,
        };
        let (args, calls): (Vec<Arg>, Vec<Vec<Call>>) = (0..size)
            .map(|_| generate_arg(rand, ctx, &elem_field))
            .unzip();
        let arg = GroupArg::new(args).into();
        let calls = calls.into_iter().flatten().collect();
        (arg, calls)
    }
}

#[derive(Debug, Clone)]
pub struct PointerType {
    elem: Box<Type>,
    dir: Direction,
}

impl ArgGenerator for PointerType {
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        todo!("PointerType::generate")
    }
}

#[derive(Debug, Clone)]
pub struct StructType {
    fields: Vec<Field>,
}

impl ArgGenerator for StructType {
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        let (args, calls) = generate_args(rand, ctx, &self.fields);
        let arg = GroupArg::new(args).into();
        (arg, calls)
    }
}

#[derive(Debug, Clone)]
pub struct UnionType {
    fields: Vec<Field>,
}

impl ArgGenerator for UnionType {
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        let field = &self.fields[rand.below(self.fields.len())];
        generate_arg(rand, ctx, field)
    }
}

#[derive(Debug, Clone)]
pub struct ResourceType {
    desc: ResourceDesc,
}

impl ResourceType {
    /// Create a new resource type
    pub fn new(desc: ResourceDesc) -> Self {
        Self { desc }
    }

    /// Create a new resource by generating a syscall
    fn create_resource<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        // Find a syscall that creates this resource
        let resource_creators = ctx
            .syscalls()
            .iter()
            .filter(|s| {
                s.return_type()
                    .is_some_and(|ty| ty.is_compatible_resource(&self.desc))
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
    fn load_resource<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> Arg {
        todo!()
    }

    /// Use an existing resource
    fn use_existing_resource<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> Option<Arg> {
        // Find compatible resources
        let results = ctx
            .results()
            .iter()
            .filter(|res| res.ty().is_compatible_resource(&self.desc));
        // Randomly choose one
        rand.choose(results)
            .map(|res| ResultArg::from_result(res.id()).into())
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
        if can_recurse && rand.coinflip(0.8) || !can_recurse && rand.coinflip(0.95) {
            if let Some(arg) = self.use_existing_resource(rand, ctx) {
                return (arg, vec![]);
            }
        }
        // Create a new resource if we can recurse
        if can_recurse && rand.coinflip(0.8) {
            return self.create_resource(rand, ctx);
        }
        // Fallback: use special values
        let val = self.desc.values[rand.below(self.desc.values.len())];
        (ResultArg::from_literal(val).into(), vec![])
    }
}
