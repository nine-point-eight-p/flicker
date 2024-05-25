#[cfg(target_os = "linux")]
mod fuzzer;

#[cfg(target_os = "linux")]
mod parse;

#[cfg(target_os = "linux")]
pub fn main() {
    let args = parse::parse();
    fuzzer::fuzz(args);
}

#[cfg(not(target_os = "linux"))]
pub fn main() {
    panic!("qemu-user and libafl_qemu is only supported on linux!");
}
