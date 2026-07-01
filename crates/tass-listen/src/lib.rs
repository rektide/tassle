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
use tass_slingshot::{SlingshotConfig, SlingshotHydrator};
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

/// The `[service.listen]` config block — **composed from the fragments each
/// crate owns**, not re-declared here:
///
/// - the Spacedust connection fields (`account`, `endpoint`, `wanted_sources`,
///   `instant`) are [`tass_spacedust::SpacedustConfig`], flattened so they sit
///   directly under `[service.listen]`;
/// - the hydrator settings are [`tass_slingshot::SlingshotConfig`] under
///   `[service.listen.slingshot]`.
///
/// ```toml
/// [service.listen]
/// account  = "did:plc:mage"
/// endpoint = "wss://spacedust.microcosm.blue/subscribe"
///
/// [service.listen.slingshot]
/// base = "https://slingshot.microcosm.blue"
/// ```
///
/// Per-verb tables (`[service.listen.<verb>]`) fall through to these via
/// `extract_cascade` once verbs exist.
#[derive(Debug, Clone, Deserialize)]
pub struct ListenConfig {
    /// Spacedust connection fragment (owned by `tass-spacedust`).
    #[serde(flatten)]
    pub spacedust: SpacedustConfig,
    /// Hydrator fragment (owned by `tass-slingshot`).
    #[serde(default)]
    pub slingshot: SlingshotConfig,
}

/// Build the config (profile figment → `[service.listen]` → CLI overrides) and
/// run the listener until the stream ends.
pub async fn run(args: ListenArgs, profile: Option<&str>) -> miette::Result<()> {
    let figment = tass_config::config::active_figment(profile)?;
    let mut cfg: ListenConfig =
        tass_config::config::extract_cascade(&figment, &["service.listen"])?;

    // CLI flags override individual fragment fields.
    if let Some(account) = args.account {
        cfg.spacedust.account = Some(account);
    }
    if let Some(endpoint) = args.endpoint {
        cfg.spacedust.endpoint = endpoint;
    }
    if let Some(slingshot) = args.slingshot {
        cfg.slingshot.base = slingshot;
    }

    if cfg.spacedust.account.is_none() {
        miette::bail!(
            "no account to listen for: set [service.listen].account or pass --account"
        );
    }

    let hydrator = SlingshotHydrator::from_config(cfg.slingshot);

    tracing::info!(
        account = cfg.spacedust.account.as_deref().unwrap_or_default(),
        endpoint = %cfg.spacedust.endpoint,
        "starting listener",
    );

    let source = SpacedustSource::connect(&cfg.spacedust, hydrator)
        .await
        .map_err(|e| miette::miette!("failed to connect to spacedust: {e}"))?;

    // Verbs (tass-act-*) register here. Empty for now → a read-only tail.
    let dispatcher = Dispatcher::new();

    tass_engine::run(source, dispatcher).await;
    tracing::info!("listener stream ended");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use figment2::providers::{Format, Toml};
    use figment2::Figment;

    #[test]
    fn listen_config_composes_fragments() {
        let toml = r#"
            [service.listen]
            account  = "did:plc:mage"
            endpoint = "wss://spacedust.example/subscribe"
            instant  = true

            [service.listen.slingshot]
            base = "https://slingshot.example"
        "#;
        let figment = Figment::from(Toml::string(toml));
        let cfg: ListenConfig =
            tass_config::config::extract_cascade(&figment, &["service.listen"]).unwrap();

        // Spacedust fragment (flattened) + slingshot fragment (nested).
        assert_eq!(cfg.spacedust.account.as_deref(), Some("did:plc:mage"));
        assert_eq!(cfg.spacedust.endpoint, "wss://spacedust.example/subscribe");
        assert!(cfg.spacedust.instant);
        assert_eq!(cfg.slingshot.base, "https://slingshot.example");
    }

    #[test]
    fn absent_config_uses_fragment_defaults() {
        let cfg: ListenConfig =
            tass_config::config::extract_cascade(&Figment::new(), &["service.listen"]).unwrap();
        assert_eq!(cfg.spacedust.account, None); // required later; validated in run()
        assert_eq!(cfg.spacedust.endpoint, tass_spacedust::DEFAULT_ENDPOINT);
        assert_eq!(cfg.slingshot.base, tass_slingshot::DEFAULT_BASE);
    }
}
