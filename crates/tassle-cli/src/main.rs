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

    /// Profile (login) to use for this invocation, overriding TASSLE_PROFILE and
    /// the config file's `profile` selector. Global; accepted on every
    /// subcommand (honoured by the figment-backed `auth`/`config` commands).
    #[arg(global = true, long)]
    profile: Option<String>,

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
}

#[tokio::main]
async fn main() -> miette::Result<ExitCode> {
    let cli = Cli::parse();
    let format = cli.format;
    let profile = cli.profile.as_deref();
    match cli.command {
        Command::Auth(args) => commands::auth::run(args, format, profile).await,
        Command::Config(args) => commands::config::run(args, format, profile),
        Command::Generate(args) => match args.kind {
            commands::generate::GenerateKind::Node(a) => commands::generate::node::run(a, format),
        },
        Command::Mage(args) => commands::mage::run(args, format).await,
        Command::Repo(args) => commands::repo::run(args, format).await,
        Command::SelfRecord(args) => commands::self_record::run(args, format).await,
    }
}
