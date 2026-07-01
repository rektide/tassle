// Command modules. Each file exposes an Args struct (clap) and a run(args).

pub mod auth;
pub mod config;
pub mod generate;
pub mod mage;
#[cfg(feature = "auth-store")]
pub mod quint;
pub mod repo;

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

/// Map the clap-facing flag onto the clap-free [`tass_output::Format`] the
/// output layer consumes.
impl From<OutputFormat> for tass_output::Format {
    fn from(f: OutputFormat) -> Self {
        match f {
            OutputFormat::Table => tass_output::Format::Table,
            OutputFormat::Json => tass_output::Format::Json,
            OutputFormat::Cbor => tass_output::Format::Cbor,
        }
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
pub async fn acquire_read_client(profile: Option<&str>) -> miette::Result<tass_config::ReadClient> {
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

/// Resolve `actor` and point the returned client at its PDS, ready to read.
///
/// The cross-PDS guard lives here: identity resolution (PLC/DNS) runs first and
/// is safe, then — before pointing at the actor's PDS — an authed client that
/// belongs to a *different* identity is downgraded to unauthenticated
/// ([`ReadClient::for_target`](tass_config::ReadClient::for_target)), so a
/// session's bearer token is never sent to another actor's PDS. Reading your
/// own repo stays authenticated.
#[cfg(feature = "auth-store")]
pub async fn resolve_read(
    client: tass_config::ReadClient,
    actor: &str,
) -> miette::Result<(tass_config::ReadClient, tass_repo::Resolved)> {
    let resolved = tass_repo::resolve(&client, actor)
        .await
        .map_err(|e| miette::miette!("{e}"))?;
    let client = client.for_target(resolved.did.as_str());
    tass_repo::point(&client, &resolved)
        .await
        .map_err(|e| miette::miette!("{e}"))?;
    Ok((client, resolved))
}

/// Without `auth-store` the client is already unauthenticated, so there is
/// nothing to guard: resolve + point in one step.
#[cfg(not(feature = "auth-store"))]
pub async fn resolve_read(
    client: jacquard::client::BasicClient,
    actor: &str,
) -> miette::Result<(jacquard::client::BasicClient, tass_repo::Resolved)> {
    let resolved = tass_repo::resolve_and_point(&client, actor)
        .await
        .map_err(|e| miette::miette!("{e}"))?;
    Ok((client, resolved))
}

/// Emit a serializable record in the requested format, through the shared
/// [`tass_output`] layer. `Table` has no tabular form for a single record, so it
/// falls back to pretty-printed JSON (the same as `Json`).
pub fn emit<S>(record: &S, format: OutputFormat) -> miette::Result<ExitCode>
where
    S: serde::Serialize,
{
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    tass_output::render_value(record, format.into(), &mut lock).into_diagnostic()?;
    Ok(ExitCode::SUCCESS)
}
