// tassle: the Rust CLI.

mod commands;

use clap::{Parser, Subcommand};
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(
    name = "tassle",
    version,
    about = "Tassle — Mage: The Ascension quintessence/tass energy ledger"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Generate records (node, tassilize, etc.) as JSON or CBOR
    Generate(commands::generate::GenerateArgs),
    /// Generate example records into samples/
    Samples(commands::samples::SamplesArgs),
}

fn main() -> miette::Result<ExitCode> {
    let cli = Cli::parse();
    match cli.command {
        Command::Generate(args) => match args.kind {
            commands::generate::GenerateKind::Node(a) => {
                commands::generate::node::run(a)
            }
        },
        Command::Samples(args) => commands::samples::run(args),
    }
}
