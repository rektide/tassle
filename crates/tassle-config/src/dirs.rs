//! On-disk locations for tassle, per the XDG Base Directory spec.
//!
//! Four roots, each resolving to `<base>/<appname>`:
//!
//! - [`config_dir`] — `XDG_CONFIG_HOME` (`~/.config`) — TOML config + drop-ins
//! - [`data_dir`]   — `XDG_DATA_HOME` (`~/.local/share`) — portable user data
//! - [`state_dir`]  — `XDG_STATE_HOME` (`~/.local/state`) — durable local state
//!   (the turso auth/session DB lives here — it is *state*, not config)
//! - [`cache_dir`]  — `XDG_CACHE_HOME` (`~/.cache`) — regenerable caches
//!
//! [`appname`] is the leaf directory under each base: `TASSLE_APPNAME` if set,
//! else `"tassle"`. Each root additionally honours a full-path override env
//! (`TASSLE_CONFIG_DIR` / `TASSLE_DATA_DIR` / `TASSLE_STATE_DIR` /
//! `TASSLE_CACHE_DIR`); when set it is used verbatim (the appname is *not*
//! appended), letting ops relocate a single root wholesale. Per the XDG spec,
//! an `XDG_*_HOME` value that is not an absolute path is ignored and the `$HOME`
//! fallback is used instead.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Built-in application directory name; overridden by `TASSLE_APPNAME`.
const DEFAULT_APPNAME: &str = "tassle";

/// Process-wide directory overrides (typically from CLI flags), taking
/// precedence over env/XDG. Install once at startup with [`set_overrides`].
#[derive(Debug, Default, Clone)]
pub struct Overrides {
    /// Override the appname leaf directory (retargets every root).
    pub appname: Option<String>,
    /// Override the config root wholesale (verbatim, no appname appended).
    pub config_dir: Option<PathBuf>,
    /// Override the state root wholesale (verbatim, no appname appended).
    pub state_dir: Option<PathBuf>,
}

static OVERRIDES: OnceLock<Overrides> = OnceLock::new();

/// Install process-wide directory overrides. Intended to be called once at
/// startup, before any directory resolution; later calls are ignored.
pub fn set_overrides(overrides: Overrides) {
    let _ = OVERRIDES.set(overrides);
}

fn overrides() -> &'static Overrides {
    OVERRIDES.get_or_init(Overrides::default)
}

/// The leaf directory used under each XDG base. Precedence: the process
/// [`Overrides::appname`] > `TASSLE_APPNAME` > the built-in `"tassle"`
/// (empty/whitespace is treated as unset).
pub fn appname() -> String {
    if let Some(a) = overrides()
        .appname
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return a.to_string();
    }
    std::env::var("TASSLE_APPNAME")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_APPNAME.to_string())
}

/// A process override path, else a non-empty environment value.
fn flag_or_env(flag: Option<&Path>, env_key: &str) -> Option<OsString> {
    flag.map(|p| p.as_os_str().to_owned())
        .or_else(|| env_path(env_key))
}

/// The user's XDG config root (`~/.config/<appname>` by default). Precedence:
/// [`Overrides::config_dir`] > `TASSLE_CONFIG_DIR` > `XDG_CONFIG_HOME` > `$HOME`.
pub fn config_dir() -> miette::Result<PathBuf> {
    resolve_from(
        flag_or_env(overrides().config_dir.as_deref(), "TASSLE_CONFIG_DIR"),
        env_path("XDG_CONFIG_HOME"),
        home().as_deref(),
        &[".config"],
        &appname(),
    )
}

/// The user's XDG data root (`~/.local/share/<appname>` by default).
pub fn data_dir() -> miette::Result<PathBuf> {
    resolve_from(
        env_path("TASSLE_DATA_DIR"),
        env_path("XDG_DATA_HOME"),
        home().as_deref(),
        &[".local", "share"],
        &appname(),
    )
}

/// The user's XDG state root (`~/.local/state/<appname>` by default). Home of
/// the turso auth/session DB. Precedence: [`Overrides::state_dir`] >
/// `TASSLE_STATE_DIR` > `XDG_STATE_HOME` > `$HOME`.
pub fn state_dir() -> miette::Result<PathBuf> {
    resolve_from(
        flag_or_env(overrides().state_dir.as_deref(), "TASSLE_STATE_DIR"),
        env_path("XDG_STATE_HOME"),
        home().as_deref(),
        &[".local", "state"],
        &appname(),
    )
}

/// The user's XDG cache root (`~/.cache/<appname>` by default).
pub fn cache_dir() -> miette::Result<PathBuf> {
    resolve_from(
        env_path("TASSLE_CACHE_DIR"),
        env_path("XDG_CACHE_HOME"),
        home().as_deref(),
        &[".cache"],
        &appname(),
    )
}

/// Path to a named turso store DB: `state_dir()/store/<stem>.db`. The single
/// place the DB filesystem layout is defined; naming/selection policy (shared
/// vs per-profile) is resolved in [`crate::config::resolve_store_path`].
pub fn store_path(stem: &str) -> miette::Result<PathBuf> {
    Ok(state_dir()?.join("store").join(format!("{stem}.db")))
}

/// `$HOME` as a path, if set.
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// A non-empty environment value as a path.
fn env_path(key: &str) -> Option<OsString> {
    std::env::var_os(key).filter(|s| !s.is_empty())
}

/// Pure resolution, factored out so it is testable without mutating process env:
/// explicit `override_val` (verbatim) > absolute `xdg_val`/`appname` >
/// `home/fallback/appname`.
fn resolve_from(
    override_val: Option<OsString>,
    xdg_val: Option<OsString>,
    home: Option<&Path>,
    fallback: &[&str],
    appname: &str,
) -> miette::Result<PathBuf> {
    if let Some(dir) = override_val {
        return Ok(PathBuf::from(dir));
    }
    if let Some(base) = xdg_val.map(PathBuf::from).filter(|b| b.is_absolute()) {
        return Ok(base.join(appname));
    }
    let home = home
        .ok_or_else(|| miette::miette!("HOME is unset; cannot resolve XDG directories"))?;
    let mut p = home.to_path_buf();
    for seg in fallback {
        p.push(seg);
    }
    p.push(appname);
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn os(s: &str) -> Option<OsString> {
        Some(OsString::from(s))
    }

    #[test]
    fn override_wins_verbatim_without_appname() {
        let got = resolve_from(os("/srv/tassle-cfg"), os("/xdg"), Some(Path::new("/home/u")), &[".config"], "tassle").unwrap();
        assert_eq!(got, PathBuf::from("/srv/tassle-cfg"));
    }

    #[test]
    fn xdg_base_gets_appname_appended() {
        let got = resolve_from(None, os("/xdg/config"), Some(Path::new("/home/u")), &[".config"], "tassle").unwrap();
        assert_eq!(got, PathBuf::from("/xdg/config/tassle"));
    }

    #[test]
    fn relative_xdg_is_ignored_falls_back_to_home() {
        let got = resolve_from(None, os("relative/nope"), Some(Path::new("/home/u")), &[".local", "state"], "tassle").unwrap();
        assert_eq!(got, PathBuf::from("/home/u/.local/state/tassle"));
    }

    #[test]
    fn appname_override_retargets_leaf() {
        let got = resolve_from(None, None, Some(Path::new("/home/u")), &[".config"], "custom").unwrap();
        assert_eq!(got, PathBuf::from("/home/u/.config/custom"));
    }

    #[test]
    fn missing_home_without_bases_errors() {
        assert!(resolve_from(None, None, None, &[".config"], "tassle").is_err());
    }
}
