// tassle: the Rust CLI.

mod commands;
mod config;
mod profile_config;

use clap::{Parser, Subcommand};
use commands::OutputFormat;
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(
    name = "tassle",
    version,
    about = "Tassle — Mage: The Ascension quintessence/tass energy ledger"
)]
struct Cli {
    /// Output format (global; accepted on every subcommand).
    #[arg(global = true, long, value_enum, default_value_t = OutputFormat::Table)]
    format: OutputFormat,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Profile and authentication commands
    Auth(commands::auth::AuthArgs),
    /// figment-backed config: profiles, loaded sources, active values
    Config(commands::config::ConfigArgs),
    /// Generate records (node, tassilize, etc.) as JSON or CBOR
    Generate(commands::generate::GenerateArgs),
    /// Mage character sheet commands
    Mage(commands::mage::MageArgs),
    /// Read public repository records through Jacquard XRPC
    Repo(commands::repo::RepoArgs),
    /// Inspect self-rkey aggregate records
    #[command(name = "self")]
    SelfRecord(commands::self_record::SelfArgs),
    /// Generate example records into the lexicon corpus
    Samples(commands::samples::SamplesArgs),
}

#[tokio::main]
async fn main() -> miette::Result<ExitCode> {
    let cli = Cli::parse();
    let format = cli.format;
    match cli.command {
        Command::Auth(args) => commands::auth::run(args, format).await,
        Command::Config(args) => commands::config::run(args, format),
        Command::Generate(args) => match args.kind {
            commands::generate::GenerateKind::Node(a) => commands::generate::node::run(a, format),
        },
        Command::Mage(args) => commands::mage::run(args, format).await,
        Command::Repo(args) => commands::repo::run(args, format).await,
        Command::SelfRecord(args) => commands::self_record::run(args, format).await,
        Command::Samples(args) => commands::samples::run(args, format),
    }
}
