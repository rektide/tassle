//! figment2-backed configuration: active profile selection + drop-in fragments.
//!
//! Model: a base `config.toml` carries the active profile selector (`profile`)
//! plus shared defaults; `config.toml.d/<name>.toml` are profile-gated drop-ins
//! that load into the selected profile. Composed with the figments-rs
//! `select_profile_from_config` and `DropIns` operators, which record every
//! load/skip in the operation log (printed by `tassle config files`).
//!
//! This is the figment-native profile model. The older hand-rolled
//! [`crate::profile_config`] (config.toml.d/<did>.toml fragment-per-DID) is kept
//! as a bridge until the auth commands migrate onto this module.

use std::path::PathBuf;

use figment2::ops::operators::{select_profile_from_config, DropIns};
use figment2::providers::{Format, Serialized, Toml};
use figment2::Figment;
use serde::{Deserialize, Serialize};

/// The conventional tassle XDG config dir (`$XDG_CONFIG_HOME/tassle` or
/// `~/.config/tassle`).
pub fn tassle_config_dir() -> miette::Result<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(dir).join("tassle"));
    }
    let home = std::env::var_os("HOME")
        .ok_or_else(|| miette::miette!("HOME is unset; cannot resolve XDG config directory"))?;
    Ok(PathBuf::from(home).join(".config").join("tassle"))
}

/// `config.toml` — base config: the `profile = "..."` selector + flat defaults.
pub fn config_file() -> miette::Result<PathBuf> {
    Ok(tassle_config_dir()?.join("config.toml"))
}

/// `config.toml.d/` — one drop-in fragment per profile (`<name>.toml`).
pub fn dropins_dir() -> miette::Result<PathBuf> {
    Ok(tassle_config_dir()?.join("config.toml.d"))
}

/// A single login profile. All fields optional — defaults flow from the base
/// `config.toml`; the selected profile's drop-in overrides per-profile.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct LoginProfile {
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

/// Build the tassle figment. If `profile_override` is given (from `--profile`
/// or `TASSLE_PROFILE`), it is injected as the `profile` key so the selector
/// records it in the audit log.
pub fn build_figment(profile_override: Option<&str>) -> miette::Result<Figment> {
    let config = config_file()?;
    let dropins = dropins_dir()?;

    let mut figment = Figment::new();
    // Base config (non-nested): top-level `profile = "..."` selector + flat
    // defaults load into the Default profile. Missing file is fine (fresh system).
    if config.exists() {
        figment = figment.merge(Toml::file(&config));
    }
    // Explicit override (CLI flag or env) wins over the file's selector.
    if let Some(name) = profile_override.filter(|s| !s.trim().is_empty()) {
        figment = figment.merge(Serialized::default("profile", name));
    }
    // Select the profile named by the `profile` key (recorded audit step).
    figment = figment.derive(select_profile_from_config("profile"));
    // Load the matching drop-in fragment into the selected profile (profile-gated).
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

/// Extract the active [`LoginProfile`] from the figment.
pub fn active_profile(figment: &Figment) -> miette::Result<LoginProfile> {
    figment
        .extract::<LoginProfile>()
        .map_err(|e| miette::miette!("failed to extract tassle profile: {e}"))
}

/// The selected profile name, or `"default"` if none.
pub fn active_name(figment: &Figment) -> String {
    let p = figment.profile();
    if p == &figment2::Profile::Default {
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
