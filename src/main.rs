#[cfg(target_os = "linux")]
use env_logger;

#[cfg(target_os = "linux")]
mod fuzzer;

#[cfg(target_os = "linux")]
mod option;

#[cfg(target_os = "linux")]
pub fn main() {
    let options = option::parse();

    env_logger::init();
    fuzzer::fuzz(options);
}

#[cfg(not(target_os = "linux"))]
pub fn main() {
    panic!("qemu-user and libafl_qemu is only supported on linux!");
}
