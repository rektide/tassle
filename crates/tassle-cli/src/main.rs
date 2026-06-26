// tassle: the Rust CLI.

mod commands;
mod profile_config;

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
    /// Profile and authentication commands
    Auth(commands::auth::AuthArgs),
    /// Generate records (node, tassilize, etc.) as JSON or CBOR
    Generate(commands::generate::GenerateArgs),
    /// Mage character sheet and energy-state commands
    Mage(commands::mage::MageArgs),
    /// Read public repository records through Jacquard XRPC
    Repo(commands::repo::RepoArgs),
    /// Inspect self-rkey aggregate records
    #[command(name = "self")]
    SelfRecord(commands::self_record::SelfArgs),
    /// Generate example records into samples/
    Samples(commands::samples::SamplesArgs),
}

#[tokio::main]
async fn main() -> miette::Result<ExitCode> {
    let cli = Cli::parse();
    match cli.command {
        Command::Auth(args) => commands::auth::run(args).await,
        Command::Generate(args) => match args.kind {
            commands::generate::GenerateKind::Node(a) => {
                commands::generate::node::run(a)
            }
        },
        Command::Mage(args) => commands::mage::run(args).await,
        Command::Repo(args) => commands::repo::run(args).await,
        Command::SelfRecord(args) => commands::self_record::run(args).await,
        Command::Samples(args) => commands::samples::run(args),
    }
}
