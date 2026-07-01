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

use std::path::{Path, PathBuf};

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

/// The `[store]` config bucket: where the turso auth/session DB lives. A bucket
/// *beside* [`Login`], not a field on it — storage is not identity.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct StoreConfig {
    /// Which DB to use, resolved to `state_dir()/store/<db>.db`. Two sentinels:
    /// [`STORE_DB_APPNAME`] (`"@appname"`) = the single **shared** `<appname>.db`
    /// — the default (also what an absent value means); [`STORE_DB_PER_PROFILE`]
    /// (`"@profile"`) = a **per-profile** `<profile>.db`. Any other value is a
    /// literal stem.
    pub db: Option<String>,
    /// Explicit full path to the DB file. When set it overrides [`db`](Self::db)
    /// resolution entirely (the escape hatch).
    pub path: Option<PathBuf>,
    /// Create the DB if it does not exist (default `true`). When `false`,
    /// opening a missing DB fails instead of creating it.
    pub create: Option<bool>,
    /// Migrate an out-of-date schema (default `true`). When `false`, opening a
    /// DB whose schema is older than the code should fail instead of migrating.
    pub update: Option<bool>,
}

impl StoreConfig {
    /// The open-time [`StoreLifecycle`] policy, applying the `true` defaults.
    pub fn lifecycle(&self) -> StoreLifecycle {
        StoreLifecycle {
            create: self.create.unwrap_or(true),
            update: self.update.unwrap_or(true),
        }
    }
}

/// Resolved DB open-time lifecycle policy from `[store]` (defaults applied).
#[derive(Debug, Clone, Copy)]
pub struct StoreLifecycle {
    /// Create the DB if it does not exist.
    pub create: bool,
    /// Migrate an out-of-date schema (vs. failing on a stale one).
    pub update: bool,
}

/// The [`StoreConfig::db`] sentinel selecting the shared, appname-named DB
/// (`<appname>.db`). The default (an absent `store.db` means the same).
pub const STORE_DB_APPNAME: &str = "@appname";

/// The [`StoreConfig::db`] sentinel selecting a per-profile DB (`<profile>.db`).
/// (Chosen over the more obscure "EPONYMOUS".)
pub const STORE_DB_PER_PROFILE: &str = "@profile";

/// Extract the `[store]` bucket from a figment, defaulting when it is absent.
pub fn store_config(figment: &Figment) -> miette::Result<StoreConfig> {
    if !figment.contains("store") {
        return Ok(StoreConfig::default());
    }
    figment
        .extract_inner::<StoreConfig>("store")
        .map_err(|e| miette::miette!("failed to extract [store] config: {e}"))
}

/// Resolve the turso auth-store DB path for the active profile.
///
/// Precedence: explicit `store.path` (verbatim) > the `store.db` selector under
/// `state_dir()/store/`. Absent `store.db` = the shared `<appname>.db`; the
/// sentinel [`STORE_DB_PER_PROFILE`] = the profile's own `<profile>.db`.
pub fn resolve_store_path(figment: &Figment, profile_name: &str) -> miette::Result<PathBuf> {
    let store = store_config(figment)?;
    if let Some(path) = store.path {
        return Ok(path);
    }
    crate::dirs::store_path(&store_stem(&store, profile_name))
}

/// The active profile's [`StoreLifecycle`] policy (`store.create` / `store.update`).
pub fn store_lifecycle(figment: &Figment) -> miette::Result<StoreLifecycle> {
    Ok(store_config(figment)?.lifecycle())
}

/// Enforce the store lifecycle policy before opening the DB at `path`.
///
/// - `create = false` + the DB is absent → error (do not create it).
/// - `update = false` (bail on stale schema) is **not yet enforceable**:
///   jac-store-fjall's `open_local` always migrates and does not yet expose a
///   schema-version / open-without-migrate API. The flag is honoured to the
///   extent possible (`update = true`, the default, is a no-op today); full
///   enforcement is tracked by tass-config-db-lifecycle's upstream coordination.
pub fn precheck_store(path: &Path, lifecycle: &StoreLifecycle) -> miette::Result<()> {
    if !lifecycle.create && !path.exists() {
        miette::bail!(
            "store DB does not exist and store.create = false: {}",
            path.display()
        );
    }
    let _ = lifecycle.update; // enforcement pending upstream schema introspection
    Ok(())
}

/// Extract `T` by cascading a chain of dotted-key config layers, later layers
/// overriding earlier ones **per key** (a deep merge, not a wholesale replace).
/// Missing layers are skipped, so `T`'s serde `default`s form the base of the
/// stack: `defaults < layer[0] < layer[1] < …`.
///
/// This is the mechanism behind hierarchical config such as `[service]` refined
/// by `[service.web]`:
///
/// ```ignore
/// let web: ServiceConfig = extract_cascade(&figment, &["service", "service.web"])?;
/// ```
///
/// (Figment already merges base `config.toml` under the selected profile's
/// drop-in, so the value found at each layer is itself the profile-resolved one;
/// this stacks the `[table]` → `[table.child]` refinement on top.)
pub fn extract_cascade<T>(figment: &Figment, layers: &[&str]) -> miette::Result<T>
where
    T: serde::de::DeserializeOwned,
{
    // Seed with an empty dict so the target key always exists even when every
    // layer is absent (then serde defaults fill `T`).
    let mut acc = Figment::new().merge(Serialized::default("layer", figment2::value::Dict::new()));
    for key in layers {
        if let Ok(value) = figment.find_value(key) {
            acc = acc.merge(Serialized::default("layer", value));
        }
    }
    acc.extract_inner::<T>("layer")
        .map_err(|e| miette::miette!("failed to extract cascaded config {layers:?}: {e}"))
}

/// The DB filename stem from a [`StoreConfig`]: the shared appname by default,
/// the profile name for the [`STORE_DB_PER_PROFILE`] sentinel, else the literal
/// `store.db` value.
fn store_stem(store: &StoreConfig, profile_name: &str) -> String {
    match store.db.as_deref() {
        None | Some(STORE_DB_APPNAME) => crate::dirs::appname(),
        Some(STORE_DB_PER_PROFILE) => profile_name.to_string(),
        Some(name) => name.to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_config_defaults_when_bucket_absent() {
        let sc = store_config(&Figment::new()).unwrap();
        assert!(sc.db.is_none() && sc.path.is_none());
    }

    #[test]
    fn store_config_reads_bucket() {
        let fig = Figment::from(Serialized::default(
            "store",
            StoreConfig { db: Some("shared".into()), ..Default::default() },
        ));
        assert_eq!(store_config(&fig).unwrap().db.as_deref(), Some("shared"));
    }

    // Feasibility proof for `[service]` + `[service.web]` hierarchical config:
    // a child table cascades over its parent (defaults < base < child), using
    // only figment2's public merge/coalesce (deep dict merge, later wins).
    #[test]
    fn service_child_cascades_over_base() {
        use figment2::providers::{Format, Toml};

        #[derive(Debug, Default, serde::Deserialize)]
        #[serde(default)]
        struct Svc {
            bind: Option<String>,
            public_url: Option<String>,
        }

        let toml = r#"
[service]
bind = "127.0.0.1:8080"
public_url = "https://base.example"

[service.web]
public_url = "https://web.example"

[service.reader]
bind = "127.0.0.1:9090"
"#;
        let fig = Figment::new().merge(Toml::string(toml));

        let web: Svc = extract_cascade(&fig, &["service", "service.web"]).unwrap();
        assert_eq!(web.bind.as_deref(), Some("127.0.0.1:8080")); // inherited from [service]
        assert_eq!(web.public_url.as_deref(), Some("https://web.example")); // [service.web] override

        let reader: Svc = extract_cascade(&fig, &["service", "service.reader"]).unwrap();
        assert_eq!(reader.bind.as_deref(), Some("127.0.0.1:9090")); // [service.reader] override
        assert_eq!(reader.public_url.as_deref(), Some("https://base.example")); // inherited

        // No layers present at all => the struct's serde defaults.
        let empty: Svc = extract_cascade(&fig, &["nope", "nope.child"]).unwrap();
        assert!(empty.bind.is_none() && empty.public_url.is_none());
    }

    #[test]
    fn lifecycle_defaults_true() {
        let lc = StoreConfig::default().lifecycle();
        assert!(lc.create && lc.update);
        let off = StoreConfig { create: Some(false), update: Some(false), ..Default::default() }
            .lifecycle();
        assert!(!off.create && !off.update);
    }

    #[test]
    fn precheck_bails_when_create_false_and_absent() {
        let missing = std::path::Path::new("/definitely/not/here.db");
        let no_create = StoreLifecycle { create: false, update: true };
        assert!(precheck_store(missing, &no_create).is_err());
        // create=true (default) tolerates an absent DB.
        let create = StoreLifecycle { create: true, update: true };
        assert!(precheck_store(missing, &create).is_ok());
    }

    #[test]
    fn store_stem_shared_profile_or_named() {
        // Absent => the shared appname DB (not per-profile).
        assert_eq!(store_stem(&StoreConfig::default(), "alice"), crate::dirs::appname());
        // @appname sentinel => same shared DB as the default.
        let app = StoreConfig { db: Some(STORE_DB_APPNAME.into()), ..Default::default() };
        assert_eq!(store_stem(&app, "alice"), crate::dirs::appname());
        // Sentinel => the profile's own DB.
        let per = StoreConfig { db: Some(STORE_DB_PER_PROFILE.into()), ..Default::default() };
        assert_eq!(store_stem(&per, "alice"), "alice");
        // Literal name => used verbatim.
        let named = StoreConfig { db: Some("custom".into()), ..Default::default() };
        assert_eq!(store_stem(&named, "alice"), "custom");
    }
}
