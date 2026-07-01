// Command modules. Each file exposes an Args struct (clap) and a run(args).

pub mod auth;
pub mod config;
pub mod generate;
pub mod mage;
#[cfg(feature = "auth-store")]
pub mod quint;
pub mod repo;
pub mod self_record;

use clap::ValueEnum;
use miette::IntoDiagnostic;
use serde::{Deserialize, Serialize};
use std::process::ExitCode;

/// Output format. Selected globally via `--format` (default `table`).
#[derive(Debug, Clone, Copy, ValueEnum, Serialize, Deserialize, PartialEq, Eq)]
pub enum OutputFormat {
    /// Human-readable (tables for lists; pretty JSON for single records).
    Table,
    /// Machine-readable JSON.
    Json,
    /// DAG-CBOR bytes (atproto wire format; only meaningful for records).
    Cbor,
}

impl OutputFormat {
    /// True when the caller should emit JSON (machine-readable).
    pub fn is_json(self) -> bool {
        matches!(self, OutputFormat::Json)
    }
}

/// Acquire the client read commands use.
///
/// With `auth-store`, this resolves the active profile's `auth` selector
/// (`@active-if-available` by default) to a [`tass_config::ReadClient`] — reads
/// run over the active session when one is present, else unauthenticated. The
/// returned client implements `XrpcClient + IdentityResolver`, so `tass_repo`
/// consumes it exactly like the plain `BasicClient` did.
#[cfg(feature = "auth-store")]
pub async fn acquire_read_client(
    profile: Option<&str>,
) -> miette::Result<tass_config::ReadClient> {
    let figment = tass_config::config::active_figment(profile)?;
    let selector = tass_config::config::auth_selector(&figment)?;
    tass_config::read_client(&selector, profile)
        .await
        .map_err(|e| miette::miette!("{e}"))
}

/// Without `auth-store`, reads are always unauthenticated (the lean build pulls
/// no session-store deps). Same signature as the authed variant so call sites
/// stay cfg-agnostic.
#[cfg(not(feature = "auth-store"))]
pub async fn acquire_read_client(
    _profile: Option<&str>,
) -> miette::Result<jacquard::client::BasicClient> {
    Ok(jacquard::client::BasicClient::unauthenticated())
}

/// Emit a serializable record in the requested format.
/// `Table` has no tabular form for a single record, so it falls back to
/// pretty-printed JSON (the same as `Json`).
pub fn emit<S>(record: &S, format: OutputFormat) -> miette::Result<ExitCode>
where
    S: serde::Serialize,
{
    match format {
        OutputFormat::Json | OutputFormat::Table => {
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
