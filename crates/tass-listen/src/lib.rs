//! The listener daemon, as a composable unit.
//!
//! [`ListenArgs`] is a `clap::Args` block and [`run`] is the entry point, so
//! both the standalone `tass-listen` binary and `tass-cli` (`tassle listen`)
//! drive the exact same code. Configuration comes from `tass-config`'s
//! `[service.listen]` block (resolved through `extract_cascade`), with CLI
//! flags overriding individual fields.
//!
//! The pipeline: [`tass_spacedust::SpacedustSource`] (posts at us) + a
//! [`tass_slingshot::SlingshotHydrator`] (pointer → body) → [`tass_engine::run`]
//! (dispatch → Executor → wide-event). Verbs register into the [`Dispatcher`];
//! until `tass-act-*` crates land, the dispatcher is empty and the daemon is a
//! read-only tail.

use serde::Deserialize;

use tass_engine::Dispatcher;
use tass_slingshot::SlingshotHydrator;
use tass_spacedust::{SpacedustConfig, SpacedustSource};

/// CLI flags for the listener. Embeddable in `tass-cli` as a subcommand's args.
#[derive(Debug, Clone, clap::Args)]
pub struct ListenArgs {
    /// DID (or handle) to listen for. Overrides `[service.listen].account`.
    #[arg(long)]
    pub account: Option<String>,
    /// Spacedust WebSocket endpoint. Overrides `[service.listen].endpoint`.
    #[arg(long)]
    pub endpoint: Option<String>,
    /// Slingshot base URL used to hydrate records. Overrides
    /// `[service.listen].slingshot`.
    #[arg(long)]
    pub slingshot: Option<String>,
}

/// The `[service.listen]` config block. Per-verb tables
/// (`[service.listen.<verb>]`) fall through to these via `extract_cascade`
/// once verbs exist.
#[derive(Debug, Clone, Deserialize)]
pub struct ListenConfig {
    /// Spacedust endpoint. Defaults to the public instance.
    #[serde(default = "default_endpoint")]
    pub endpoint: String,
    /// The account we listen for (→ `wantedSubjectDids`). Required at runtime
    /// (config or `--account`).
    #[serde(default)]
    pub account: Option<String>,
    /// Slingshot base URL for hydration. Defaults to the public instance.
    #[serde(default = "default_slingshot")]
    pub slingshot: String,
    /// Optional `wantedSources` narrowing.
    #[serde(default)]
    pub wanted_sources: Vec<String>,
    /// Bypass Spacedust's delay buffer (default false).
    #[serde(default)]
    pub instant: bool,
}

fn default_endpoint() -> String {
    tass_spacedust::DEFAULT_ENDPOINT.to_string()
}
fn default_slingshot() -> String {
    tass_slingshot::DEFAULT_BASE.to_string()
}

/// Build the config (profile figment → `[service.listen]` → CLI overrides) and
/// run the listener until the stream ends.
pub async fn run(args: ListenArgs, profile: Option<&str>) -> miette::Result<()> {
    let figment = tass_config::config::active_figment(profile)?;
    let mut cfg: ListenConfig =
        tass_config::config::extract_cascade(&figment, &["service.listen"])?;

    // CLI flags override config fields.
    if let Some(account) = args.account {
        cfg.account = Some(account);
    }
    if let Some(endpoint) = args.endpoint {
        cfg.endpoint = endpoint;
    }
    if let Some(slingshot) = args.slingshot {
        cfg.slingshot = slingshot;
    }

    let account = cfg.account.ok_or_else(|| {
        miette::miette!("no account to listen for: set [service.listen].account or pass --account")
    })?;

    let spacedust = SpacedustConfig {
        endpoint: cfg.endpoint,
        account,
        wanted_sources: cfg.wanted_sources,
        instant: cfg.instant,
    };
    let hydrator = SlingshotHydrator::new(cfg.slingshot);

    tracing::info!(
        account = %spacedust.account,
        endpoint = %spacedust.endpoint,
        "starting listener",
    );

    let source = SpacedustSource::connect(&spacedust, hydrator)
        .await
        .map_err(|e| miette::miette!("failed to connect to spacedust: {e}"))?;

    // Verbs (tass-act-*) register here. Empty for now → a read-only tail.
    let dispatcher = Dispatcher::new();

    tass_engine::run(source, dispatcher).await;
    tracing::info!("listener stream ended");
    Ok(())
}
