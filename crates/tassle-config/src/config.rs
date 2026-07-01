//! figment2-backed configuration: config dir, generic profile selection, and the
//! [`Login`] shape.
//!
//! A **profile** here is the figment sense — a named config bucket selected by
//! the `profile = "..."` key / a `config.toml.d/<name>.toml` drop-in. It is
//! generic: a profile may carry a [`Login`] (an account identity), service
//! config, or anything else. "profile" is never a synonym for "login".
//!
//! Model (ported from tassle-cli's `config.rs`): a base `config.toml` carries
//! the active profile selector (`profile = "..."`) plus shared defaults;
//! `config.toml.d/<name>.toml` are profile-gated drop-ins that load into the
//! selected profile. Composed with the figments-rs `select_profile_from_config`
//! and `DropIns` operators.
//!
//! This is the figment-native profile model. The older hand-rolled
//! `config.toml.d/<did>.toml` fragment-per-DID bridge (tassle-cli's
//! `profile_config.rs`) is being retired in favour of it; this crate exposes
//! only the figment model.

use std::path::PathBuf;

use figment2::ops::operators::{select_profile_from_config, DropIns};
use figment2::providers::{Format, Serialized, Toml};
use figment2::Figment;
use serde::{Deserialize, Serialize};

/// The tassle XDG config dir. Delegates to [`crate::dirs::config_dir`] — the
/// single source of truth for on-disk locations (`$XDG_CONFIG_HOME/<appname>`
/// or `~/.config/<appname>`, `TASSLE_APPNAME`-aware).
pub fn tassle_config_dir() -> miette::Result<PathBuf> {
    crate::dirs::config_dir()
}

/// `config.toml` — base config: the `profile = "..."` selector + flat defaults.
pub fn config_file() -> miette::Result<PathBuf> {
    Ok(tassle_config_dir()?.join("config.toml"))
}

/// `config.toml.d/` — one drop-in fragment per profile (`<name>.toml`).
pub fn dropins_dir() -> miette::Result<PathBuf> {
    Ok(tassle_config_dir()?.join("config.toml.d"))
}

/// A **login**: the account identity configured in a profile. All fields
/// optional — defaults flow from the base `config.toml`; the selected profile's
/// drop-in overrides them.
///
/// "Login" is the broad heading over both auth kinds (app-password and, later,
/// oauth); [`auth_mode`](Self::auth_mode) names which. This is *not* the profile
/// — a profile is a generic config bucket that happens to carry a login.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Login {
    pub did: Option<String>,
    pub handle: Option<String>,
    pub pds: Option<String>,
    /// `"app_password"` (MVP default) or `"oauth"` (deferred).
    pub auth_mode: Option<String>,
    /// Which session within the account (jacquard `SessionKey.session_id`).
    pub session_id: Option<String>,
    /// Optional per-profile store path override.
    pub store_path: Option<PathBuf>,
}

impl Login {
    /// True when this login has enough to attempt an authenticated session
    /// (a target account + a PDS to talk to).
    pub fn is_resolvable(&self) -> bool {
        (self.did.is_some() || self.handle.is_some()) && self.pds.is_some()
    }

    /// The account identifier to log in as: the DID if present, else the handle.
    /// (The shape jacquard's `SessionHint::from_optional_input` wants.)
    pub fn account(&self) -> Option<&str> {
        self.did.as_deref().or(self.handle.as_deref())
    }
}

/// Build the tassle figment. If `profile_override` is given (from `--profile`
/// or `TASSLE_PROFILE`), it is injected as the `profile` key.
pub fn build_figment(profile_override: Option<&str>) -> miette::Result<Figment> {
    let config = config_file()?;
    let dropins = dropins_dir()?;

    let mut figment = Figment::new();
    if config.exists() {
        figment = figment.merge(Toml::file(&config));
    }
    if let Some(name) = profile_override.filter(|s| !s.trim().is_empty()) {
        figment = figment.merge(Serialized::default("profile", name));
    }
    figment = figment.derive(select_profile_from_config("profile"));
    figment = figment.derive(DropIns::new(dropins).profile_gated().operator::<Toml>());
    Ok(figment)
}

/// The active profile name from `TASSLE_PROFILE`, if set.
pub fn profile_from_env() -> Option<String> {
    std::env::var("TASSLE_PROFILE")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

/// The active figment, with CLI/env overrides applied (`cli_profile` > env > file).
pub fn active_figment(cli_profile: Option<&str>) -> miette::Result<Figment> {
    let override_name = cli_profile
        .map(str::to_string)
        .or_else(profile_from_env);
    build_figment(override_name.as_deref())
}

/// Extract the active profile's [`Login`] from a figment.
pub fn active_login(figment: &Figment) -> miette::Result<Login> {
    figment
        .extract::<Login>()
        .map_err(|e| miette::miette!("failed to extract tassle login: {e}"))
}

/// The selected profile name, or `"default"` if none.
pub fn active_name(figment: &Figment) -> String {
    let p = figment.profile();
    if p == figment2::Profile::Default {
        "default".to_string()
    } else {
        p.as_str().to_string()
    }
}

/// Available profile names = drop-in fragment stems (one fragment per profile).
pub fn available_profiles() -> miette::Result<Vec<String>> {
    use miette::IntoDiagnostic;
    let dir = dropins_dir()?;
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut stems: Vec<String> = std::fs::read_dir(dir)
        .into_diagnostic()?
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("toml") {
                p.file_stem()?.to_str().map(String::from)
            } else {
                None
            }
        })
        .collect();
    stems.sort();
    Ok(stems)
}
