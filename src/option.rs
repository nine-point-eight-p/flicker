use clap::Parser;

#[derive(Parser)]
#[clap(trailing_var_arg = true)]
pub struct FuzzerOption {
    #[arg(short, long, default_value_t = 3)]
    pub timeout: u64,
    #[arg(short, long, default_value_t = 1337)]
    pub broker_port: u16,
    #[arg(short, long, default_value = "1")]
    pub cores: String,
    #[arg(short, long, default_value = "./corpus/init")]
    pub init_corpus_dir: String,
    #[arg(short, long, default_value = "./corpus/gen")]
    pub gen_corpus_dir: String,
    #[arg(short, long, default_value = "./crashes")]
    pub objective_dir: String,
    #[clap(num_args = 0.., allow_hyphen_values = true)]
    pub run_args: Vec<String>,
}

pub fn parse() -> FuzzerOption {
    FuzzerOption::parse()
}
