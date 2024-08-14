use std::iter;
use std::ops::Neg;

use libafl_bolts::rands::Rand;

use enum_dispatch::enum_dispatch;
use log::debug;
use uuid::Uuid;

use super::{
    ArrayType, ByteBuffer, Direction, Field, FilenameBuffer, FlagType, IntType, PointerType,
    ResourceType, StringBuffer, StructType, UnionType,
};
use crate::generator::{generate_arg, generate_args, generate_call};
use crate::program::{
    call::{Arg, Call, ConstArg, DataArg, GroupArg, PointerArg, ResultArg},
    context::Context,
};
use crate::utility::*;

pub const MAX_ARRAY_LENGTH: u64 = 10;
pub const MAX_BUFFER_LENGTH: u64 = 0x1000;

#[enum_dispatch]
pub trait GenerateArg {
    /// Generate a new argument for this field type
    #[must_use]
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>);

    /// Generate the default value for this field type
    #[must_use]
    fn default(&self) -> Arg;
}

impl GenerateArg for Field {
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        self.ty.generate(rand, ctx)
    }

    fn default(&self) -> Arg {
        self.ty.default()
    }
}

impl IntType {
    pub(super) fn generate_impl<R: Rand>(&self, rand: &mut R) -> u64 {
        if let Some((min, max)) = self.range {
            rand.between(min as usize, max as usize) as u64
        } else {
            rand_int(rand, self.bits)
        }
    }
}

impl GenerateArg for IntType {
    fn generate<R: Rand>(&self, rand: &mut R, _ctx: &mut Context) -> (Arg, Vec<Call>) {
        let val = self.generate_impl(rand);
        (ConstArg::new(val as u64).into(), vec![])
    }

    fn default(&self) -> Arg {
        ConstArg::default().into()
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

impl GenerateArg for FlagType {
    fn generate<R: Rand>(&self, rand: &mut R, _ctx: &mut Context) -> (Arg, Vec<Call>) {
        let val = self.generate_impl(rand, 0);
        (ConstArg::new(val).into(), vec![])
    }

    fn default(&self) -> Arg {
        ConstArg::default().into()
    }
}

impl GenerateArg for ArrayType {
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        // Generate a random length
        let mut len = if let Some((min, max)) = self.range {
            rand.between(min as usize, max as usize) as u64
        } else {
            rand_array_length(rand)
        };

        // Generate at least one element if we are generating a resource
        if ctx.generating_resource && len == 0 {
            len = 1;
        }

        // Generate the elements
        let (args, calls): (Vec<Arg>, Vec<Vec<Call>>) =
            iter::repeat_with(|| generate_arg(rand, ctx, &self.elem))
                .take(len as usize)
                .unzip();
        let arg = GroupArg::new(args).into();
        let calls = calls.into_iter().flatten().collect();
        (arg, calls)
    }

    fn default(&self) -> Arg {
        let args = self
            .range
            .map(|(min, _)| vec![self.elem.default(); min as usize])
            .unwrap_or_default();
        GroupArg::new(args).into()
    }
}

impl GenerateArg for PointerType {
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        // The resource we are trying to generate may be in the pointer,
        // so don't try to create an empty special pointer during resource generation.
        if !ctx.generating_resource && one_of(rand, 1000) {
            (PointerArg::from_addr(0).into(), vec![])
        } else {
            let (arg, calls) = generate_arg(rand, ctx, &self.elem);
            (PointerArg::from_res(arg).into(), calls)
        }
    }

    fn default(&self) -> Arg {
        PointerArg::default().into()
    }
}

impl StringBuffer {
    pub(super) fn generate_string<R: Rand>(&self, rand: &mut R, ctx: &Context) -> String {
        let mut string = if !self.values.is_empty() {
            // Choose a special value
            self.values[rand.below(self.values.len())].clone()
        } else if binary(rand) {
            // Use existing strings
            sample_from_iter(rand, ctx.strings().iter())
                .cloned()
                .unwrap_or_else(|| rand_string(rand))
        } else {
            // Generate a new one
            rand_string(rand)
        };

        // The null byte will appear/disappear unexpectedly with a probability of 1/100
        if one_of(rand, 100) == self.no_zero {
            string.push('\0');
        };

        string
    }
}

impl GenerateArg for StringBuffer {
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        let string = self.generate_string(rand, ctx);
        let arg = match self.attr.dir {
            Direction::In | Direction::InOut => DataArg::In(string.into_bytes()),
            Direction::Out => DataArg::Out(string.len() as u64),
        };
        (arg.into(), vec![])
    }

    fn default(&self) -> Arg {
        let arg = match self.attr.dir {
            Direction::In | Direction::InOut => {
                if self.values.len() == 1 {
                    DataArg::In(self.values[0].clone().into_bytes())
                } else {
                    DataArg::In(Vec::default())
                }
            }
            Direction::Out => DataArg::Out(u64::default()),
        };
        arg.into()
    }
}

impl FilenameBuffer {
    pub(super) fn generate_filename<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> String {
        const SPECIAL_FILENAMES: [&str; 2] = ["", "."];

        let mut filename = if one_of(rand, 100) {
            // Use a special filename
            SPECIAL_FILENAMES[rand.below(SPECIAL_FILENAMES.len())].to_string()
        } else if n_out_of(rand, 9, 10) {
            // Use an existing filename
            sample_from_iter(rand, ctx.filenames().iter())
                .cloned()
                .unwrap_or_else(|| rand_filename(rand, ctx))
        } else {
            // Generate a new one
            rand_filename(rand, ctx)
        };
        if !self.no_zero {
            filename.push('\0');
        }
        filename
    }
}

impl GenerateArg for FilenameBuffer {
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        let arg = match self.attr.dir {
            Direction::In | Direction::InOut => {
                DataArg::In(self.generate_filename(rand, ctx).into_bytes())
            }
            Direction::Out => {
                // We consider the filename length to be variable
                let len = if n_out_of(rand, 1, 3) {
                    rand.below(100) as u64
                } else {
                    rand_filename_length(rand)
                };
                DataArg::Out(len)
            }
        };
        (arg.into(), vec![])
    }

    fn default(&self) -> Arg {
        let arg = match self.attr.dir {
            Direction::In | Direction::InOut => DataArg::In(Vec::default()),
            Direction::Out => DataArg::Out(u64::default()),
        };
        arg.into()
    }
}

impl GenerateArg for ByteBuffer {
    fn generate<R: Rand>(&self, rand: &mut R, _ctx: &mut Context) -> (Arg, Vec<Call>) {
        let len = if let Some((min, max)) = self.range {
            rand.between(min as usize, max as usize) as u64
        } else {
            rand_buffer_length(rand)
        };
        assert!(len <= MAX_BUFFER_LENGTH);
        let arg = match self.attr.dir {
            Direction::In | Direction::InOut => {
                let data = iter::repeat_with(|| rand.below(256) as u8)
                    .take(len as usize)
                    .collect();
                DataArg::In(data)
            }
            Direction::Out => DataArg::Out(len),
        };
        (arg.into(), vec![])
    }

    fn default(&self) -> Arg {
        let arg = match self.attr.dir {
            Direction::In | Direction::InOut => {
                if !is_var_len(self.range) {
                    DataArg::In(vec![0; self.range.unwrap().0 as usize])
                } else {
                    DataArg::In(Vec::default())
                }
            }
            Direction::Out => DataArg::Out(u64::default()),
        };
        arg.into()
    }
}

impl GenerateArg for StructType {
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

impl GenerateArg for UnionType {
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        let field = &self.fields[rand.below(self.fields.len())];
        generate_arg(rand, ctx, &field.ty)
    }

    fn default(&self) -> Arg {
        let field = &self.fields[0];
        field.default()
    }
}

impl ResourceType {
    /// Create a new resource by generating a syscall
    fn create_resource<R: Rand>(
        &self,
        rand: &mut R,
        ctx: &mut Context,
    ) -> Option<(Arg, Vec<Call>)> {
        // Find a syscall that creates this resource
        let resource_creators: Vec<_> = ctx
            .syscalls()
            .iter()
            .filter(|s| {
                s.return_type()
                    .is_some_and(|ty| ty.is_compatible_resource(&self.name))
            })
            .collect();
        if resource_creators.is_empty() {
            return None;
        }
        let idx = rand.below(resource_creators.len());
        let syscall = resource_creators[idx].clone();

        // Generate the syscall and the argument
        let calls = generate_call(rand, ctx, &syscall);
        let id = calls.last().unwrap().result().unwrap();
        let arg = ResultArg::from_result(id).into();

        debug!("[ResourceType] Create resource, id: {}", id);

        Some((arg, calls))
    }

    /// Create a resource by loading the initializations from the corpus
    fn load_resource<R: Rand>(&self, _rand: &mut R, _ctx: &mut Context) -> Option<(Arg, Vec<Call>)> {
        // TODO: Implement ResourceType::load_resource
        None
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
        if results.is_empty() {
            return None;
        }
        let id = *results[rand.below(results.len())];
        debug!("[ResourceType] Use existing resource, id: {}", id);
        Some(ResultArg::from_result(id).into())
    }

    /// Choose a fallback value
    pub fn choose_fallback<R: Rand>(&self, rand: &mut R) -> Arg {
        let val = self.values[rand.below(self.values.len())];
        debug!("[ResourceType] Choose fallback value: {}", val);
        ResultArg::from_literal(val).into()
    }
}

impl GenerateArg for ResourceType {
    fn generate<R: Rand>(&self, rand: &mut R, ctx: &mut Context) -> (Arg, Vec<Call>) {
        // Check if we can recurse
        let old_generating_resource = ctx.generating_resource;
        let can_recurse = if ctx.generating_resource {
            false
        } else {
            ctx.generating_resource = true;
            true
        };

        // Use an existing resource with a high probability
        if can_recurse && n_out_of(rand, 4, 5) || !can_recurse && n_out_of(rand, 19, 20) {
            if let Some(arg) = self.use_existing_resource(rand, ctx) {
                ctx.generating_resource = old_generating_resource;
                return (arg, vec![]);
            }
        }
        // Create a new resource if we can recurse
        if can_recurse {
            if one_of(rand, 4) {
                if let Some((arg, calls)) = self.load_resource(rand, ctx) {
                    ctx.generating_resource = old_generating_resource;
                    return (arg, calls);
                }
            }
            if n_out_of(rand, 4, 5) {
                if let Some((arg, calls)) = self.create_resource(rand, ctx) {
                    ctx.generating_resource = old_generating_resource;
                    return (arg, calls);
                }
            }
        }
        // Fallback: use special values
        ctx.generating_resource = old_generating_resource;
        (self.choose_fallback(rand), vec![])
    }

    fn default(&self) -> Arg {
        ResultArg::from_literal(self.values[0]).into()
    }
}

// Helper functions

/// Returns a random int in range [0..n),
/// probability of n-1 is k times higher than probability of 0.
fn biased_rand<R: Rand>(rand: &mut R, n: usize, k: usize) -> u64 {
    let nf = n as f64;
    let kf = k as f64;
    let rf = nf * (kf / 2.0 + 1.0) * rand.next_float();
    let bf = (-1.0 + (1.0 + 2.0 * kf * rf / nf).sqrt()) * nf / kf;
    bf as u64
}

/// Generate a random integer.
fn rand_int<R: Rand>(rand: &mut R, bits: u8) -> u64 {
    let mut val = rand.next();

    // Set the value into a range
    if n_out_of(rand, 100, 182) {
        val %= 10;
    } else if bits >= 8 && n_out_of(rand, 50, 82) {
    } else if n_out_of(rand, 10, 32) {
        val %= 256;
    } else if n_out_of(rand, 10, 22) {
        val %= 0x1000;
    } else if n_out_of(rand, 10, 12) {
        val %= 0x10000;
    } else {
        val %= 0x8000_0000;
    }

    // Negate or shift the value
    if n_out_of(rand, 100, 107) {
        // Do nothing
    } else if n_out_of(rand, 5, 7) {
        val = (val as i64).neg() as u64;
    } else {
        val <<= rand.below(bits as usize);
    }

    // Truncate value to the number of bits
    val & ((1 << bits) - 1)
}

/// Generate a random array length.
fn rand_array_length<R: Rand>(rand: &mut R) -> u64 {
    let n = MAX_ARRAY_LENGTH + 1;
    (biased_rand(rand, n as usize, 10) + 1) % n
}

/// Generate a random string.
fn rand_string<R: Rand>(rand: &mut R) -> String {
    const PUNCT: [char; 23] = [
        '!', '@', '#', '$', '%', '^', '&', '*', '(', ')', '-', '+', '\\', '/', ':', '.', ',', '-',
        '\'', '[', ']', '{', '}',
    ];

    let mut buf = String::new();
    while n_out_of(rand, 3, 4) {
        if n_out_of(rand, 10, 11) {
            buf.push(PUNCT[rand.below(PUNCT.len())]);
        } else {
            buf.push(rand.below(256) as u8 as char);
        }
    }
    buf
}

/// Generate a random filename.
fn rand_filename<R: Rand>(rand: &mut R, ctx: &Context) -> String {
    let mut dir = if binary(rand) {
        sample_from_iter(rand, ctx.filenames().iter())
            .cloned()
            .unwrap_or(".".to_string())
    } else {
        ".".to_string()
    };
    if dir.ends_with("\0") {
        dir.pop();
    }
    if one_of(rand, 10)
        && path_clean::clean(&dir)
            .into_os_string()
            .into_string()
            .unwrap()
            != "."
    {
        dir.push_str("/..");
    }

    let mut i = 1;
    loop {
        let mut name = format!("{}/file{}", dir, i);
        if one_of(rand, 100) {
            // Vary the length
            let len = rand_filename_length(rand) as usize;
            if len > name.len() {
                name += str::repeat("a", len - name.len()).as_str();
            }
        }
        if !ctx.filenames().contains(&name) {
            return name;
        }
        i += 1;
    }
}

/// Generate a random filename length.
pub(super) fn rand_filename_length<R: Rand>(rand: &mut R) -> u64 {
    // TODO: Configure for different targets
    const SPECIAL_FILE_LENGTHS: [u64; 1] = [4096];

    let off = biased_rand(rand, 10, 5);
    let len = SPECIAL_FILE_LENGTHS[rand.below(SPECIAL_FILE_LENGTHS.len())];
    let res = if binary(rand) {
        len + off
    } else {
        len.checked_sub(off).unwrap_or(0)
    };
    res
}

/// Generate a random buffer length.
fn rand_buffer_length<R: Rand>(rand: &mut R) -> u64 {
    if n_out_of(rand, 50, 56) {
        rand.below(256) as u64
    } else if n_out_of(rand, 5, 6) {
        MAX_BUFFER_LENGTH
    } else {
        0
    }
}

#[inline]
fn sample_from_iter<T, I, R>(rand: &mut R, iter: I) -> Option<T>
where
    T: Clone,
    I: Iterator<Item = T>,
    R: Rand,
{
    let vec: Vec<_> = iter.collect();
    (!vec.is_empty()).then(|| vec[rand.below(vec.len())].clone())
}

#[inline]
fn is_var_len(range: Option<(u64, u64)>) -> bool {
    range.map_or(true, |(min, max)| min != max)
}
