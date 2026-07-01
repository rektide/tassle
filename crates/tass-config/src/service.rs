//! Service (non-login) config: the `[service]` / `[service.oauth]` buckets for
//! the axum OAuth web flow. See `doc/oauth.md` ("Config shape") for the settled
//! design this implements.
//!
//! A profile is generic (see [`crate::config`]); `[service]` is the singleton
//! bucket describing how a running service instance behaves. It is read with
//! [`service_config`], which cascades `[service]` under an optional
//! `[service.<variant>]` refinement via [`crate::config::extract_cascade`].
//!
//! This module owns only the *config* — the typed knobs, path/URL resolution,
//! and the confidential-vs-public determination. Building jacquard's client
//! metadata + `OAuthWebConfig`, loading/generating the key material, and the
//! axum wiring live in the web crate (`tass-web-auth-crate`); route paths are
//! `OAuthWebConfig`'s, so `client_id`/`redirect_uris`/`jwks_uri` are derived
//! there, not here.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use figment2::Figment;
use serde::{Deserialize, Serialize};

/// `[service]` — how a running service instance behaves. A singleton bucket (not
/// per-login); refine per variant with `[service.<variant>]`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ServiceConfig {
    /// Local listen address (e.g. `127.0.0.1:3000`).
    pub bind: Option<SocketAddr>,
    /// Public HTTPS origin — the OAuth identity root (e.g. `https://telluri.at`).
    /// **Not** the same as [`bind`](Self::bind): this is where the world reaches
    /// the service (through a tunnel / reverse proxy), and every derived OAuth
    /// URL hangs off it.
    pub public_url: Option<String>,
    /// Cookie signing keys, ordered (`[0]` = active, the rest are validators once
    /// keyring verification lands). Bare names resolve under `state_dir()/cookie/`;
    /// absolute paths are used verbatim.
    pub cookie_paths: Vec<String>,
    /// OAuth client knobs.
    pub oauth: OAuthConfig,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            bind: None,
            public_url: None,
            cookie_paths: vec!["current".to_string()],
            oauth: OAuthConfig::default(),
        }
    }
}

/// `[service.oauth]` — the atproto OAuth client knobs jacquard-axum does *not*
/// own. Route paths live in its `OAuthWebConfig`; DPoP and client type are fixed
/// or emergent (from `public_url` + keyset presence), so there are no knobs for
/// them here.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct OAuthConfig {
    /// Scopes to request; `atproto` is always required. Default `["atproto"]`.
    pub scopes: Vec<String>,
    /// Client-auth ES256 keyset files, ordered (`[0]` = active signer). Bare
    /// names resolve under `state_dir()/keyset/`; absolute paths verbatim. An
    /// empty list means a **public / loopback** client (no confidential key).
    pub keyset_paths: Vec<String>,
    /// Human display name (`client_name`).
    pub client_name: Option<String>,
    /// Path joined to `public_url` for `logo_uri`.
    pub logo_path: Option<String>,
    /// Path joined to `public_url` for `tos_uri`.
    pub tos_path: Option<String>,
    /// Path joined to `public_url` for `privacy_policy_uri`.
    pub privacy_path: Option<String>,
}

impl Default for OAuthConfig {
    fn default() -> Self {
        Self {
            scopes: vec!["atproto".to_string()],
            keyset_paths: vec!["current".to_string()],
            client_name: None,
            logo_path: None,
            tos_path: None,
            privacy_path: None,
        }
    }
}

/// Read the service config, optionally refined by a `[service.<variant>]` child.
///
/// `None` reads just `[service]`; `Some("web")` cascades `[service]` <
/// `[service.web]` (child overrides per key). Absent tables fall back to
/// [`ServiceConfig::default`].
pub fn service_config(figment: &Figment, variant: Option<&str>) -> miette::Result<ServiceConfig> {
    match variant {
        None => crate::config::extract_cascade(figment, &["service"]),
        Some(v) => {
            let child = format!("service.{v}");
            crate::config::extract_cascade(figment, &["service", &child])
        }
    }
}

impl ServiceConfig {
    /// Resolve `cookie_paths` to absolute files under `state_dir()/cookie/`
    /// (`.key` assumed for bare names).
    pub fn cookie_files(&self) -> miette::Result<Vec<PathBuf>> {
        Ok(resolve_key_files(
            &crate::dirs::state_dir()?.join("cookie"),
            &self.cookie_paths,
            "key",
        ))
    }

    /// The public homepage URL (`client_uri`). Today just `public_url`.
    pub fn client_uri(&self) -> Option<String> {
        self.public_url.clone()
    }

    /// `logo_uri` = `public_url` + `oauth.logo_path` (both must be present).
    pub fn logo_uri(&self) -> Option<String> {
        self.join_public(self.oauth.logo_path.as_deref())
    }

    /// `tos_uri` = `public_url` + `oauth.tos_path`.
    pub fn tos_uri(&self) -> Option<String> {
        self.join_public(self.oauth.tos_path.as_deref())
    }

    /// `privacy_policy_uri` = `public_url` + `oauth.privacy_path`.
    pub fn privacy_uri(&self) -> Option<String> {
        self.join_public(self.oauth.privacy_path.as_deref())
    }

    fn join_public(&self, path: Option<&str>) -> Option<String> {
        match (self.public_url.as_deref(), path) {
            (Some(base), Some(p)) => Some(join_url(base, p)),
            _ => None,
        }
    }
}

impl OAuthConfig {
    /// Resolve `keyset_paths` to absolute files under `state_dir()/keyset/`
    /// (`.json` assumed for bare names). `[0]` is the active signer.
    pub fn keyset_files(&self) -> miette::Result<Vec<PathBuf>> {
        Ok(resolve_key_files(
            &crate::dirs::state_dir()?.join("keyset"),
            &self.keyset_paths,
            "json",
        ))
    }

    /// A confidential client (a keyset is configured) vs. a public/loopback one.
    pub fn is_confidential(&self) -> bool {
        !self.keyset_paths.is_empty()
    }
}

/// Resolve ordered key names to absolute files under `dir`. Absolute names are
/// used verbatim; a bare name with no extension gets `.<ext>` appended.
fn resolve_key_files(dir: &Path, names: &[String], ext: &str) -> Vec<PathBuf> {
    names
        .iter()
        .map(|n| {
            let named = if Path::new(n).extension().is_some() {
                n.clone()
            } else {
                format!("{n}.{ext}")
            };
            let p = PathBuf::from(&named);
            if p.is_absolute() {
                p
            } else {
                dir.join(named)
            }
        })
        .collect()
}

/// Join a base origin and a path with exactly one `/` between them.
fn join_url(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), path.trim_start_matches('/'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use figment2::providers::{Format, Toml};

    #[test]
    fn defaults_are_sensible() {
        let s = ServiceConfig::default();
        assert!(s.bind.is_none() && s.public_url.is_none());
        assert_eq!(s.cookie_paths, ["current"]);
        assert_eq!(s.oauth.scopes, ["atproto"]);
        assert_eq!(s.oauth.keyset_paths, ["current"]);
        assert!(s.oauth.is_confidential());
    }

    #[test]
    fn reads_service_and_cascades_variant() {
        let toml = r#"
[service]
bind = "127.0.0.1:3000"
public_url = "https://telluri.at"

[service.oauth]
scopes = ["atproto", "transition:generic"]
client_name = "Telluri.at"

[service.web]
public_url = "https://web.telluri.at"
"#;
        let fig = Figment::new().merge(Toml::string(toml));

        let base = service_config(&fig, None).unwrap();
        assert_eq!(base.bind.unwrap().to_string(), "127.0.0.1:3000");
        assert_eq!(base.public_url.as_deref(), Some("https://telluri.at"));
        assert_eq!(base.oauth.scopes, ["atproto", "transition:generic"]);
        assert_eq!(base.oauth.client_name.as_deref(), Some("Telluri.at"));

        // [service.web] overrides public_url, inherits bind + oauth.
        let web = service_config(&fig, Some("web")).unwrap();
        assert_eq!(web.public_url.as_deref(), Some("https://web.telluri.at"));
        assert_eq!(web.bind.unwrap().to_string(), "127.0.0.1:3000");
        assert_eq!(web.oauth.client_name.as_deref(), Some("Telluri.at"));
    }

    #[test]
    fn absent_service_is_default() {
        let fig = Figment::new();
        let s = service_config(&fig, None).unwrap();
        assert_eq!(s.cookie_paths, ["current"]);
        assert_eq!(s.oauth.scopes, ["atproto"]);
    }

    #[test]
    fn public_client_when_no_keyset() {
        let toml = "[service.oauth]\nkeyset_paths = []\n";
        let fig = Figment::new().merge(Toml::string(toml));
        let s = service_config(&fig, None).unwrap();
        assert!(!s.oauth.is_confidential());
    }

    #[test]
    fn key_file_resolution() {
        let dir = Path::new("/state/keyset");
        let files = resolve_key_files(
            dir,
            &["current".into(), "2026-07.json".into(), "/abs/k".into()],
            "json",
        );
        assert_eq!(files[0], PathBuf::from("/state/keyset/current.json"));
        assert_eq!(files[1], PathBuf::from("/state/keyset/2026-07.json"));
        assert_eq!(files[2], PathBuf::from("/abs/k.json")); // absolute, ext appended
    }

    #[test]
    fn branding_uris_join_public_url() {
        let s = ServiceConfig {
            public_url: Some("https://telluri.at/".to_string()),
            oauth: OAuthConfig {
                logo_path: Some("/logo.png".to_string()),
                tos_path: Some("tos".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(s.logo_uri().as_deref(), Some("https://telluri.at/logo.png"));
        assert_eq!(s.tos_uri().as_deref(), Some("https://telluri.at/tos"));
        assert_eq!(s.privacy_uri(), None); // no privacy_path
        assert_eq!(s.client_uri().as_deref(), Some("https://telluri.at/"));
    }
}
