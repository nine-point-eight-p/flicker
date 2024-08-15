//! Command line interface for flicker

use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run the fuzzer
    Fuzz(FuzzOption),
    /// Reproduce a crash
    Reproduce(ReproduceOption),
}

/// Fuzzing options
#[derive(Args)]
#[clap(trailing_var_arg = true)]
pub struct FuzzOption {
    /// Time limit for each run of the target
    #[arg(short, long, default_value_t = 3)]
    pub timeout: u64,

    /// Broker port
    #[arg(short, long, default_value_t = 1337)]
    pub port: u16,

    /// Number of cores used by fuzzer
    #[arg(short, long, default_value = "1")]
    pub cores: String,

    /// Path to the directory of initial provided corpus
    #[arg(long, default_value = "./corpus/init")]
    pub init_corpus: String,

    /// Path to the directory of generated corpus
    #[arg(long, default_value = "./corpus/gen")]
    pub gen_corpus: String,

    /// Path to the directory of crashes
    #[arg(long, default_value = "./crashes")]
    pub crash: String,

    /// Path to the description file
    #[arg(long)]
    pub desc: String,

    /// Path to the constants file
    #[arg(long)]
    pub r#const: String,

    /// Max number of calls per run
    #[arg(long, default_value = "30")]
    pub max_calls: usize,

    /// Arguments passed to Qemu
    #[arg(num_args = 0.., allow_hyphen_values = true)]
    pub run_args: Vec<String>,
}

/// Reproduction options
#[derive(Args)]
pub struct ReproduceOption {
    /// Path to the testcase file
    pub testcase: String,

    /// Time limit for each run of the target
    #[arg(short, long, default_value_t = 3)]
    pub timeout: u64,

    /// Arguments passed to Qemu
    #[arg(num_args = 0.., allow_hyphen_values = true)]
    pub run_args: Vec<String>,
}

pub fn parse() -> Cli {
    Cli::parse()
}
