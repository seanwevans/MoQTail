use clap::Parser;
use moqtail_core::hello;

#[derive(Parser)]
struct Cli {}

fn main() {
    println!("{}", hello());
}
