use libafl_bolts::rands::Rand;

use enum_dispatch::enum_dispatch;
use enum_downcast::EnumDowncast;
use syzlang_parser::parser::{
    ArgOpt, ArgType, Argument, Direction as ParserDirection, Flag, Function, IdentType, Identifier, Parsed, Resource, Struct, StructAttr, Union, Value
};

use log::warn;

use super::call::{Arg, Call, CallResult, ConstArg, GroupArg, ResultArg};
use super::context::Context;
use crate::generator::{generate_arg, generate_args, generate_call};

#[derive(Debug, Clone)]
pub struct Syscall {
    nr: u32,
    name: String,
    fields: Vec<Field>,
    ret: Option<Type>,
}

impl Syscall {
    pub fn from_function(nr: u32, function: &Function, ctx: &Parsed) -> Self {
        let ret_arg = Argument::new_fake(function.output.clone(), vec![]);
        Self {
            nr,
            name: function.name.name.clone(),
            fields: function
                .args
                .iter()
                .map(|arg| Field::from_argument(arg, ctx))
                .collect(),
            ret: Type::from_argument(&ret_arg, ctx),
        }
    }

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
    fn mutate<R: Rand>(&self, rand: &mut R, ctx: &mut Context, arg: &mut Arg) -> Vec<Call>;
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub ty: Type,
    pub dir: Direction,
}

impl Field {
    pub fn from_argument(argument: &Argument, ctx: &Parsed) -> Self {
        Self {
            name: argument.name.name.clone(),
            ty: Type::from_argument(argument, ctx).unwrap(),
            dir: argument.direction().into(),
        }
    }
}

impl ArgGenerator for Field {
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        self.ty.generate(rand, ctx)
    }
}

impl ArgMutator for Field {
    fn mutate<R: Rand>(&self, rand: &mut R, ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
        self.ty.mutate(rand, ctx, arg)
    }
}

#[enum_dispatch(ArgGenerator, ArgMutator)]
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
    pub fn from_argument(argument: &Argument, ctx: &Parsed) -> Option<Self> {
        match &argument.argtype {
            ArgType::Int8 | ArgType::Int16 | ArgType::Int32 | ArgType::Int64 | ArgType::Intptr => {
                Some(IntType::from_argument(argument).into())
            }
            ArgType::Flags => Some(FlagType::from_argument(argument, ctx).into()),
            ArgType::Array => Some(ArrayType::from_argument(argument, ctx).into()),
            ArgType::Ident(ident) => {
                let ident_type = ctx
                    .identifier_to_ident_type(&ident)
                    .expect("Unknown ident type");
                let ty = match ident_type {
                    IdentType::Struct => {
                        StructType::from_struct(ctx.get_struct(&ident).unwrap(), ctx).into()
                    }
                    IdentType::Union => {
                        UnionType::from_union(ctx.get_union(&ident).unwrap(), ctx).into()
                    }
                    IdentType::Resource => {
                        ResourceType::from_resource(ctx.get_resource(&ident).unwrap(), ctx).into()
                    }
                    _ => unimplemented!("Unsupported ident type: {:?}", ident_type),
                };
                Some(ty)
            }
            ArgType::Void => None,
            _ => unimplemented!("Unsupported argument type: {:?}", argument.argtype),
        }
    }

    pub fn is_integer(&self) -> bool {
        matches!(self, Type::IntType(_) | Type::FlagType(_))
    }

    pub fn is_resource(&self) -> bool {
        matches!(self, Type::ResourceType(_))
    }

    pub fn is_compatible_resource(&self, name: &str) -> bool {
        matches!(self, Type::ResourceType(inner) if inner.name == name)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    In,
    Out,
    InOut,
}

impl From<ParserDirection> for Direction {
    fn from(value: ParserDirection) -> Self {
        match value {
            ParserDirection::In => Self::In,
            ParserDirection::Out => Self::Out,
            ParserDirection::InOut => Self::InOut,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IntType {
    begin: u64,
    end: u64,
}

impl IntType {
    pub fn from_argument(argument: &Argument) -> Self {
        // Check if there is a range specified
        if let Some((begin, end)) = find_range(&argument.opts) {
            return Self { begin, end };
        }
        // Default range
        let max = match argument.argtype {
            ArgType::Int8 => u8::MAX as u64,
            ArgType::Int16 => u16::MAX as u64,
            ArgType::Int32 => u32::MAX as u64,
            ArgType::Int64 => u64::MAX as u64,
            ArgType::Intptr => usize::MAX as u64,
            _ => unreachable!("Invalid argument type for int type"),
        };
        Self { begin: 0, end: max }
    }

    fn generate_impl<R: Rand>(&self, rand: &mut R) -> u64 {
        rand.between(self.begin.try_into().unwrap(), self.end.try_into().unwrap()) as u64
    }
}

impl ArgGenerator for IntType {
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        let val = self.generate_impl(rand);
        (ConstArg::new(val as u64).into(), vec![])
    }
}

impl ArgMutator for IntType {
    fn mutate<R: Rand>(&self, rand: &mut R, _ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
        let arg = arg.enum_downcast_mut::<ConstArg>().unwrap();

        arg.0 = if rand.below(2) == 0 {
            self.generate_impl(rand)
        } else {
            match rand.below(3) {
                0 => arg.0.wrapping_sub(rand.below(4) as u64 + 1),
                1 => arg.0.wrapping_add(rand.below(4) as u64 + 1),
                2 => arg.0 ^ (1 << rand.below(64)),
                _ => unreachable!("Invalid random value"),
            }
        };

        vec![]
    }
}

#[derive(Debug, Clone)]
pub struct FlagType {
    values: Vec<u64>,
}

impl FlagType {
    fn from_flag(flag: &Flag, ctx: &Parsed) -> Self {
        let values = flag
            .args()
            .map(|arg| value_to_u64_flatten(arg, ctx).unwrap())
            .collect();
        Self { values }
    }

    fn from_argument(argument: &Argument, ctx: &Parsed) -> Self {
        let flag_name = find_ident(&argument.opts, ctx).expect("No flag name for flag type");
        let flag = ctx.get_flag(flag_name).unwrap();
        Self::from_flag(flag, ctx)
    }

    fn generate_impl<R: Rand>(&self, rand: &mut R) -> u64 {
        match rand.below(5) {
            0 => 0,
            1 => rand.next(),
            _ => self.values[rand.below(self.values.len())],
        }
    }
}

impl ArgGenerator for FlagType {
    fn generate<R: Rand>(&self, rand: &mut R, _ctx: &mut Context) -> (Arg, Vec<Call>) {
        let val = self.generate_impl(rand);
        (ConstArg::new(val).into(), vec![])
    }
}

impl ArgMutator for FlagType {
    fn mutate<R: Rand>(&self, rand: &mut R, ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
        let arg = arg.enum_downcast_mut::<ConstArg>().unwrap();

        loop {
            let val = self.generate_impl(rand);
            if arg.0 != val {
                arg.0 = val;
                return vec![];
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ArrayType {
    elem: Box<Type>,
    begin: u64,
    end: u64,
    dir: Direction,
}

impl ArrayType {
    pub fn from_argument(argument: &Argument, ctx: &Parsed) -> Self {
        let subarg = ArgOpt::get_subarg(&argument.opts).unwrap();
        let elem = Box::new(Type::from_argument(subarg, ctx).unwrap());
        let (begin, end) = find_length(&argument.opts).unwrap_or((0, usize::MAX as u64));
        let dir = find_dir(&argument.opts);
        Self {
            elem,
            begin,
            end,
            dir,
        }
    }
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

impl ArgMutator for ArrayType {
    fn mutate<R: Rand>(&self, rand: &mut R, ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
        todo!("ArrayType::mutate")
    }
}

#[derive(Debug, Clone)]
pub struct PointerType {
    elem: Box<Type>,
    dir: Direction,
}

impl PointerType {
    pub fn from_argument(argument: &Argument, ctx: &Parsed) -> Self {
        let subarg = ArgOpt::get_subarg(&argument.opts).expect("No underlying type for pointer");
        let elem = Box::new(Type::from_argument(subarg, ctx).unwrap());
        let dir = find_dir(&argument.opts);
        Self { elem, dir }
    }
}

impl ArgGenerator for PointerType {
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        todo!("PointerType::generate")
    }
}

impl ArgMutator for PointerType {
    fn mutate<R: Rand>(&self, rand: &mut R, ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
        todo!("PointerType::mutate")
    }
}

#[derive(Debug, Clone)]
pub struct StructType {
    fields: Vec<Field>,
}

impl StructType {
    pub fn from_struct(st: &Struct, ctx: &Parsed) -> Self {
        Self {
            fields: st
                .args()
                .map(|arg| Field::from_argument(arg, ctx))
                .collect(),
        }
    }
}

impl ArgGenerator for StructType {
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        let (args, calls) = generate_args(rand, ctx, &self.fields);
        let arg = GroupArg::new(args).into();
        (arg, calls)
    }
}

impl ArgMutator for StructType {
    fn mutate<R: Rand>(&self, rand: &mut R, ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
        todo!("StructType::mutate")
    }
}

#[derive(Debug, Clone)]
pub struct UnionType {
    fields: Vec<Field>,
}

impl UnionType {
    pub fn from_union(union: &Union, ctx: &Parsed) -> Self {
        Self {
            fields: union
                .args()
                .map(|arg| Field::from_argument(arg, ctx))
                .collect(),
        }
    }
}

impl ArgGenerator for UnionType {
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        let field = &self.fields[rand.below(self.fields.len())];
        generate_arg(rand, ctx, field)
    }
}

impl ArgMutator for UnionType {
    fn mutate<R: Rand>(&self, rand: &mut R, ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
        todo!("UnionType::mutate")
    }
}

#[derive(Debug, Clone)]
pub struct ResourceType {
    name: String,
    values: Vec<u64>,
}

impl ResourceType {
    pub fn from_resource(resource: &Resource, ctx: &Parsed) -> Self {
        Self {
            name: resource.name.name.clone(),
            values: resource
                .consts
                .iter()
                .map(|val| value_to_u64_flatten(val, ctx).unwrap())
                .collect(),
        }
    }

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
    fn load_resource<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> Arg {
        todo!()
    }

    /// Use an existing resource
    fn use_existing_resource<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> Option<Arg> {
        // Find compatible resources
        let results = ctx
            .results()
            .iter()
            .filter(|res| res.ty().is_compatible_resource(&self.name));
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
        let val = self.values[rand.below(self.values.len())];
        (ResultArg::from_literal(val).into(), vec![])
    }
}

impl ArgMutator for ResourceType {
    fn mutate<R: Rand>(&self, rand: &mut R, ctx: &mut Context, arg: &mut Arg) -> Vec<Call> {
        todo!("ResourceType::mutate")
    }
}

fn find_dir(arg_opts: &[ArgOpt]) -> Direction {
    arg_opts
        .iter()
        .find_map(|opt| match opt {
            ArgOpt::Dir(dir) => Some(Direction::from(*dir)),
            _ => None,
        })
        .unwrap_or(Direction::In)
}

fn find_ident<'a>(arg_opts: &'a [ArgOpt], ctx: &'a Parsed) -> Option<&'a Identifier> {
    arg_opts
        .iter()
        .find_map(|opt| match opt {
            ArgOpt::Ident(ident) => Some(ident),
            _ => None,
        })
}

fn find_values(arg_opts: &[ArgOpt], ctx: &Parsed) -> Vec<u64> {
    arg_opts
        .iter()
        .filter_map(|opt| match opt {
            ArgOpt::Value(value) => Some(value_to_u64_flatten(value, ctx).unwrap()),
            _ => None,
        })
        .collect()
}

fn find_range(arg_opts: &[ArgOpt]) -> Option<(u64, u64)> {
    arg_opts.iter().find_map(|opt| match opt {
        ArgOpt::Range(begin, end, _step) => Some((value_to_u64(begin), value_to_u64(end))),
        _ => None,
    })
}

fn find_length(arg_opts: &[ArgOpt]) -> Option<(u64, u64)> {
    arg_opts.iter().find_map(|opt| match opt {
        ArgOpt::Len(begin, end) => Some((value_to_u64(begin), value_to_u64(end))),
        _ => None,
    })
}

fn value_to_u64(value: &Value) -> u64 {
    match value {
        Value::Int(val) => *val as u64,
        _ => panic!("Invalid integer value"),
    }
}

fn value_to_u64_flatten(value: &Value, ctx: &Parsed) -> Option<u64> {
    match value {
        Value::Int(val) => Some(*val as u64),
        Value::Ident(ident) => ctx
            .consts()
            .consts()
            .find(|c| c.name() == ident.name.as_str())
            .and_then(|c| c.as_uint().ok()),
        _ => None,
    }
}
