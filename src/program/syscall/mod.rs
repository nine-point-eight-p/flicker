mod generation;
mod mutation;

use std::iter;

use libafl_bolts::rands::Rand;

use enum_common_fields::EnumCommonFields;
use enum_dispatch::enum_dispatch;
use syzlang_parser::parser::{
    ArgOpt, ArgType, Argument, Direction as ParserDirection, Flag, Function, IdentType, Identifier,
    Parsed, Resource, Struct, Union, Value,
};

use super::call::{Arg, Call};
use super::context::Context;
use super::metadata::ARCH;

pub use generation::GenerateArg;
pub use mutation::MutateArg;

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

#[enum_dispatch(GenerateArg, MutateArg)]
#[derive(Debug, Clone)]
pub enum Type {
    Int(IntType),
    Flag(FlagType),
    Array(ArrayType),
    Pointer(PointerType),
    Buffer(BufferType),
    Struct(StructType),
    Union(UnionType),
    Resource(ResourceType),
}

impl Type {
    pub fn from_argument(argument: &Argument, ctx: &Parsed) -> Option<Self> {
        match &argument.argtype {
            ArgType::Int8 | ArgType::Int16 | ArgType::Int32 | ArgType::Int64 | ArgType::Intptr => {
                Some(IntType::from_argument(argument).into())
            }
            ArgType::Flags => Some(FlagType::from_argument(argument, ctx).into()),
            ArgType::Ptr => Some(PointerType::from_argument(argument, ctx).into()),
            ArgType::Array => {
                let subarg = ArgOpt::get_subarg(&argument.opts).unwrap();
                let ty = if subarg.argtype == ArgType::Int8 {
                    // Special case: array[int8] is represented as buffer
                    BufferType::from_argument(argument, ctx).into()
                } else {
                    // Normal array
                    ArrayType::from_argument(argument, ctx).into()
                };
                Some(ty)
            }
            ArgType::String | ArgType::StringNoz => {
                Some(BufferType::from_argument(argument, ctx).into())
            }
            ArgType::Ident(ident) => {
                let attr = TypeAttr::from_opts(&argument.opts);
                let ident_type = ctx
                    .identifier_to_ident_type(&ident)
                    .expect("Unknown ident type");
                let ty = match ident_type {
                    IdentType::Struct => {
                        StructType::from_struct(ctx.get_struct(&ident).unwrap(), ctx, attr).into()
                    }
                    IdentType::Union => {
                        UnionType::from_union(ctx.get_union(&ident).unwrap(), ctx, attr).into()
                    }
                    IdentType::Resource => {
                        ResourceType::from_resource(ctx.get_resource(&ident).unwrap(), ctx, attr)
                            .into()
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
        matches!(self, Self::Int(_) | Self::Flag(_))
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Self::Buffer(BufferType::String(_)))
    }

    pub fn is_filename(&self) -> bool {
        matches!(self, Self::Buffer(BufferType::Filename(_)))
    }

    pub fn is_resource(&self) -> bool {
        matches!(self, Self::Resource(_))
    }

    pub fn is_compatible_resource(&self, name: &str) -> bool {
        matches!(self, Self::Resource(inner) if inner.name == name)
    }

    pub fn attr(&self) -> &TypeAttr {
        match self {
            Self::Int(inner) => &inner.attr,
            Self::Flag(inner) => &inner.attr,
            Self::Array(inner) => &inner.attr,
            Self::Pointer(inner) => &inner.attr,
            Self::Buffer(inner) => inner.attr(),
            Self::Struct(inner) => &inner.attr,
            Self::Union(inner) => &inner.attr,
            Self::Resource(inner) => &inner.attr,
        }
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
pub struct TypeAttr {
    pub dir: Direction,
    pub optional: bool,
}

impl TypeAttr {
    fn from_opts(arg_opts: &[ArgOpt]) -> Self {
        let dir = find_dir(arg_opts);
        let optional = arg_opts.iter().any(|opt| matches!(opt, ArgOpt::Opt));
        Self { dir, optional }
    }
}

#[derive(Debug, Clone)]
pub struct IntType {
    attr: TypeAttr,
    bits: u8,
    range: Option<(u64, u64)>,
}

impl IntType {
    fn from_argument(argument: &Argument) -> Self {
        let attr = TypeAttr::from_opts(&argument.opts);
        let arg_type = argument.arg_type();
        let bits = match argument.argtype {
            ArgType::Int8 | ArgType::Int16 | ArgType::Int32 | ArgType::Int64 | ArgType::Intptr => {
                (arg_type.evaluate_size(&ARCH).unwrap() * 8) as u8
            }
            _ => unreachable!("Invalid argument type for integer"),
        };
        let range = find_range(&argument.opts);
        Self { attr, bits, range }
    }
}

#[derive(Debug, Clone)]
pub struct FlagType {
    attr: TypeAttr,
    values: Vec<u64>,
    is_bitmask: bool,
}

impl FlagType {
    fn from_flag(flag: &Flag, ctx: &Parsed, attr: TypeAttr) -> Self {
        let mut values: Vec<u64> = flag
            .args()
            .map(|arg| value_to_u64_flatten(arg, ctx).unwrap())
            .collect();
        values.sort();
        let is_bitmask = is_bitmask(&values);
        Self {
            attr,
            values,
            is_bitmask,
        }
    }

    fn from_argument(argument: &Argument, ctx: &Parsed) -> Self {
        let flag_name = find_ident(&argument.opts).expect("No flag name for flag type");
        let flag = ctx.get_flag(flag_name).unwrap();
        let attr = TypeAttr::from_opts(&argument.opts);
        Self::from_flag(flag, ctx, attr)
    }
}

#[derive(Debug, Clone)]
pub struct ArrayType {
    attr: TypeAttr,
    elem: Box<Type>,
    range: Option<(u64, u64)>,
}

impl ArrayType {
    fn from_argument(argument: &Argument, ctx: &Parsed) -> Self {
        let attr = TypeAttr::from_opts(&argument.opts);
        let subarg = ArgOpt::get_subarg(&argument.opts).unwrap();
        let elem = Box::new(Type::from_argument(subarg, ctx).unwrap());
        let range = find_length(&argument.opts);
        Self { attr, elem, range }
    }
}

#[derive(Debug, Clone)]
pub struct PointerType {
    attr: TypeAttr,
    elem: Box<Type>,
}

impl PointerType {
    fn from_argument(argument: &Argument, ctx: &Parsed) -> Self {
        let attr = TypeAttr::from_opts(&argument.opts);
        let subarg = ArgOpt::get_subarg(&argument.opts).expect("No underlying type for pointer");
        let elem = Box::new(Type::from_argument(subarg, ctx).unwrap());
        Self { attr, elem }
    }
}

#[enum_dispatch(GenerateArg, MutateArg)]
#[derive(Debug, Clone, EnumCommonFields)]
#[common_field(attr: TypeAttr)]
pub enum BufferType {
    String(StringBuffer),
    Filename(FilenameBuffer),
    Byte(ByteBuffer),
}

impl BufferType {
    fn from_argument(argument: &Argument, ctx: &Parsed) -> Self {
        let arg_type = &argument.argtype;
        match arg_type {
            // string or string[filename]
            ArgType::String => {
                // Can't use arg_type.is_filename here since after post-processing
                // the parsed things, `filename` has been converted to `string[filename]`
                let is_filename = find_ident_value(&argument.opts)
                    .map(|ident| ident.name == "filename")
                    .unwrap_or(false);
                if is_filename {
                    FilenameBuffer::from_argument(argument).into()
                } else {
                    StringBuffer::from_argument(argument, ctx).into()
                }
            }
            // stringnoz
            ArgType::StringNoz => StringBuffer::from_argument(argument, ctx).into(),
            // array[int8], the underlying type should be ensured by the caller
            ArgType::Array => ByteBuffer::from_argument(argument).into(),
            _ => unreachable!("Invalid argument type for buffer kind"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StringBuffer {
    attr: TypeAttr,
    values: Vec<String>,
    no_zero: bool,
}

impl StringBuffer {
    fn from_argument(argument: &Argument, ctx: &Parsed) -> Self {
        let attr = TypeAttr::from_opts(&argument.opts);
        let values = if let Some(value) = find_string_value(&argument.opts) {
            // Use the const values if provided in the argument
            vec![value]
        } else {
            // If no values are provided, try to get them from the flag
            if let Some(flag_name) = find_ident(&argument.opts) {
                let flag = ctx
                    .get_flag(flag_name)
                    .expect(format!("Unknown string flag: {flag_name}").as_str());
                let mut values: Vec<String> = flag.args().map(value_to_string).collect();
                values.sort();
                values
            } else {
                vec![]
            }
        };
        let no_zero = argument.argtype == ArgType::StringNoz;

        Self {
            attr,
            values,
            no_zero,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FilenameBuffer {
    attr: TypeAttr,
    no_zero: bool,
}

impl FilenameBuffer {
    fn from_argument(argument: &Argument) -> Self {
        let attr = TypeAttr::from_opts(&argument.opts);
        let no_zero = argument.argtype == ArgType::StringNoz;
        Self { attr, no_zero }
    }
}

#[derive(Debug, Clone)]
pub struct ByteBuffer {
    attr: TypeAttr,
    range: Option<(u64, u64)>,
}

impl ByteBuffer {
    fn from_argument(argument: &Argument) -> Self {
        let attr = TypeAttr::from_opts(&argument.opts);
        let range = find_length(&argument.opts);
        Self { attr, range }
    }
}

#[derive(Debug, Clone)]
pub struct StructType {
    attr: TypeAttr,
    fields: Vec<Field>,
}

impl StructType {
    fn from_struct(st: &Struct, ctx: &Parsed, attr: TypeAttr) -> Self {
        Self {
            attr,
            fields: st
                .args()
                .map(|arg| Field::from_argument(arg, ctx))
                .collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UnionType {
    attr: TypeAttr,
    fields: Vec<Field>,
}

impl UnionType {
    fn from_union(union: &Union, ctx: &Parsed, attr: TypeAttr) -> Self {
        Self {
            attr,
            fields: union
                .args()
                .map(|arg| Field::from_argument(arg, ctx))
                .collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResourceType {
    attr: TypeAttr,
    name: String,
    values: Vec<u64>,
}

impl ResourceType {
    fn from_resource(resource: &Resource, ctx: &Parsed, attr: TypeAttr) -> Self {
        let path = ctx.resource_to_basics(&resource.name);
        let mut values: Vec<_> = path[..path.len() - 1]
            .iter()
            .map(|ty| match ty {
                ArgType::Ident(ident) => ctx.get_resource(&ident).expect("Unknown resource type"),
                _ => panic!("Invalid resource inheritance path"),
            })
            .chain(iter::once(resource)) // Add the current resource
            .flat_map(|res| {
                res.consts
                    .iter()
                    .map(|val| value_to_u64_flatten(val, ctx).unwrap())
            })
            .collect();
        assert!(
            !values.is_empty(),
            "Resource type has to provide at least one value as default"
        );
        values.sort();
        values.dedup();
        Self {
            attr,
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

fn find_ident<'a>(arg_opts: &'a [ArgOpt]) -> Option<&'a Identifier> {
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

fn find_string_value(arg_opts: &[ArgOpt]) -> Option<String> {
    arg_opts.iter().find_map(|opt| match opt {
        // Don't use `value_to_string` here because we don't need to panic
        // if the value is not a string
        ArgOpt::Value(Value::String(val)) => Some(val.clone()),
        _ => None,
    })
}

fn find_ident_value(arg_opts: &[ArgOpt]) -> Option<&Identifier> {
    arg_opts.iter().find_map(|opt| match opt {
        ArgOpt::Value(Value::Ident(ident)) => Some(ident),
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
            .find(|c| c.name() == ident.name)
            .and_then(|c| c.as_uint().ok()),
        _ => None,
    }
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(val) => val.clone(),
        _ => panic!("Invalid string value"),
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
