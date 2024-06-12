#[cfg(all(target_os = "linux", feature = "fuzzer"))]
mod fuzzer;

#[cfg(all(target_os = "linux", feature = "runtime"))]
mod runner;

#[cfg(target_os = "linux")]
mod parse;

#[cfg(target_os = "linux")]
pub fn main() {
    let args = parse::parse();

    #[cfg(feature = "fuzzer")]
    fuzzer::fuzz(args);

    #[cfg(feature = "runtime")]
    runner::run(args);
}

#[cfg(not(target_os = "linux"))]
pub fn main() {
    panic!("qemu-user and libafl_qemu is only supported on linux!");
}
