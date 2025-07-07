use clap::{Parser, Subcommand};
use moqtail_core::compile;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile and print a subscription selector
    Sub {
        /// Query selector string
        query: String,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Sub { query } => match compile(&query) {
            Ok(sel) => println!("{:?}", sel),
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        },
    }
}
