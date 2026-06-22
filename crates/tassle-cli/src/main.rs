// tassle: the Rust CLI for tassle.
//
// Composes tassle-lexicons (types + fluent builders) and tassle-validate
// (schema validation) in-process. For now, only `mint` exists, and it
// outputs the constructed record as JSON to stdout. Login/publish/cbor
// come later.
//
// Usage:
//   tassle mint "Crystal Spring" --rating 3 --resonance dynamic
//   tassle mint "Crystal Spring" --rating 3 --no-validate
//   tassle mint "Crystal Spring" --rating 3 --output cbor  # deferred

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
    /// Mint a new Node — a place where quintessence gathers
    Mint(commands::mint::MintArgs),
}

fn main() -> miette::Result<ExitCode> {
    let cli = Cli::parse();
    match cli.command {
        Command::Mint(args) => commands::mint::run(args),
    }
}
