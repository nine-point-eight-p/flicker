mod generation;
mod mutation;

use libafl_bolts::rands::Rand;

use enum_dispatch::enum_dispatch;
use syzlang_parser::parser::{
    ArgOpt, ArgType, Argument, Direction as ParserDirection, Flag, Function, IdentType, Identifier,
    Parsed, Resource, Struct, Union, Value,
};

use super::call::{Arg, Call};
use super::context::Context;

pub use generation::ArgGenerator;
pub use mutation::ArgMutator;

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
}

#[derive(Debug, Clone)]
pub struct FlagType {
    values: Vec<u64>,
    is_bitmask: bool,
}

impl FlagType {
    fn from_flag(flag: &Flag, ctx: &Parsed) -> Self {
        let mut values: Vec<u64> = flag
            .args()
            .map(|arg| value_to_u64_flatten(arg, ctx).unwrap())
            .collect();
        values.sort();
        let is_bitmask = is_bitmask(&values);
        Self { values, is_bitmask }
    }

    fn from_argument(argument: &Argument, ctx: &Parsed) -> Self {
        let flag_name = find_ident(&argument.opts, ctx).expect("No flag name for flag type");
        let flag = ctx.get_flag(flag_name).unwrap();
        Self::from_flag(flag, ctx)
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

#[derive(Debug, Clone)]
pub struct ResourceType {
    name: String,
    values: Vec<u64>,
}

impl ResourceType {
    pub fn from_resource(resource: &Resource, ctx: &Parsed) -> Self {
        let values: Vec<u64> = resource
            .consts
            .iter()
            .map(|val| value_to_u64_flatten(val, ctx).unwrap())
            .collect();
        assert!(
            !values.is_empty(),
            "Resource type has to provide at least one value as default"
        );
        Self {
            name: resource.name.name.clone(),
            values,
        }
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
    arg_opts.iter().find_map(|opt| match opt {
        ArgOpt::Ident(ident) => Some(ident),
        _ => None,
    })
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

/// Check if the values are a bitmask (i.e. no two values have the same bit set).
/// Assume the values are sorted.
fn is_bitmask(values: &[u64]) -> bool {
    if values.is_empty() || values[0] == 0 {
        // 0 can't be part of a bitmask
        return false;
    }
    let mut combined = 0;
    for v in values {
        if v & combined != 0 {
            return false;
        }
        combined |= v;
    }
    true
}