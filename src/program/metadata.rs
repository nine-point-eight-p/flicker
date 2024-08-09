use syzlang_parser::parser::{Arch, Parsed};

use crate::program::syscall::Syscall;

const ARCH: Arch = Arch::Riscv64;

#[derive(Debug, Clone)]
pub struct SyscallMetadata(Vec<Syscall>);

impl SyscallMetadata {
    /// Create a new `SyscallMetadata` from a list of syscalls.
    pub fn new(syscalls: Vec<Syscall>) -> Self {
        Self(syscalls)
    }

    /// Create a new `SyscallMetadata` from a parsed syzlang file.
    pub fn from_parsed(parsed: Parsed) -> Self {
        let syscalls = parsed
            .functions()
            .map(|func| {
                let nr = find_sysno_with_default(&parsed, &func.name.name);
                Syscall::from_function(nr, func, &parsed)
            })
            .collect();
        Self::new(syscalls)
    }

    pub fn syscalls(&self) -> &[Syscall] {
        &self.0
    }

    pub fn find_number(&self, nr: u32) -> Option<&Syscall> {
        self.0.iter().find(|s| s.number() == nr)
    }
}

fn find_sysno_with_default(parsed: &Parsed, name: &str) -> u32 {
    if let Some(nr) = parsed.consts().find_sysno(&name, &ARCH) {
        // Use the syscall number for the arch if available
        nr as u32
    } else {
        // Use the default syscall number if not specified
        parsed
            .consts()
            .find_sysno_for_any(name)
            .iter()
            .find(|c| c.arch.is_empty())
            .map(|c| c.as_uint().unwrap() as u32)
            .expect(&format!("Syscall number not found: {}", name))
    }
}
