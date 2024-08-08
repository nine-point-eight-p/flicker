use clap::Parser;

#[derive(Parser)]
#[clap(trailing_var_arg = true)]
pub struct FuzzerOption {
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
    #[clap(long)]
    pub desc: String,

    /// Path to the constants file
    #[clap(long)]
    pub r#const: String,

    /// Arguments passed to Qemu
    #[clap(num_args = 0.., allow_hyphen_values = true)]
    pub run_args: Vec<String>,
}

pub fn parse() -> FuzzerOption {
    FuzzerOption::parse()
}
