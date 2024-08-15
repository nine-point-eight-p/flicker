#[cfg(target_os = "linux")]
mod fuzzer;

#[cfg(target_os = "linux")]
mod runner;

#[cfg(target_os = "linux")]
mod cli;

#[cfg(target_os = "linux")]
pub fn main() {
    use cli::Commands;
    use env_logger;

    let cli = cli::parse();

    env_logger::init();

    match cli.command {
        Commands::Fuzz(options) => fuzzer::fuzz(options),
        Commands::Reproduce(options) => runner::reproduce(options),
    }
}

#[cfg(not(target_os = "linux"))]
pub fn main() {
    panic!("qemu-user and libafl_qemu is only supported on linux!");
}
