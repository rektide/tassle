// Command modules. Each file exposes an Args struct (clap) and a run(args).

pub mod auth;
pub mod config;
pub mod generate;
pub mod mage;
pub mod repo;
pub mod samples;
pub mod self_record;

use clap::ValueEnum;
use miette::IntoDiagnostic;
use serde::{Deserialize, Serialize};
use std::process::ExitCode;

/// Output format for record-producing commands.
#[derive(Debug, Clone, Copy, ValueEnum, Serialize, Deserialize, PartialEq, Eq)]
pub enum OutputFormat {
    /// Pretty-printed JSON to stdout (default)
    Json,
    /// DAG-CBOR bytes (atproto wire format)
    Cbor,
}

/// Emit a serializable record in the requested format.
/// Centralizes the json/cbor dispatch so every gen subcommand shares it.
pub fn emit<S>(record: &S, format: OutputFormat) -> miette::Result<ExitCode>
where
    S: serde::Serialize,
{
    match format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(record).into_diagnostic()?;
            println!("{json}");
            Ok(ExitCode::SUCCESS)
        }
        OutputFormat::Cbor => {
            use std::io::Write;
            let bytes = serde_ipld_dagcbor::to_vec(record).into_diagnostic()?;
            let stdout = std::io::stdout();
            let mut lock = stdout.lock();
            lock.write_all(&bytes).into_diagnostic()?;
            Ok(ExitCode::SUCCESS)
        }
    }
}
