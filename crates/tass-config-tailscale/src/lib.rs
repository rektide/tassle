//! Tailscale config fragment for tassle services.
//!
//! This crate owns only the *configuration shape* for running Tailscale alongside
//! a tassle service. It mirrors the pattern used by `tass-spacedust` and
//! `tass-slingshot`: the crate defines a `TailscaleConfig` fragment that a
//! consuming service composes into its own `[service.<variant>]` config bucket.
//!
//! The actual Tailscale daemon lifecycle (spawning `tailscaled`, waiting for
//! readiness, running `tailscale up`, graceful shutdown) lives in the consuming
//! crate (`tass-web` initially).
//!
//! Example TOML:
//!
//! ```toml
//! [service.tailscale]
//! enabled = true
//! auth_key = "tskey-auth-..."
//! hostname = "tass-web"
//! extra_args = "--advertise-exit-node"
//!
//! state_path = "/var/lib/tailscale/tailscaled.state"
//! socket_path = "/var/run/tailscale/tailscaled.sock"
//! bin_dir = "./bin"
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Default Tailscale hostname when none is configured.
pub const DEFAULT_HOSTNAME: &str = "tass-web";

/// Default directory for Tailscale state files.
pub const DEFAULT_STATE_DIR: &str = "/var/lib/tailscale";

/// Default directory for Tailscale runtime sockets.
pub const DEFAULT_SOCKET_DIR: &str = "/var/run/tailscale";

/// Default directory to look for the bundled `tailscale` and `tailscaled`
/// binaries.
pub const DEFAULT_BIN_DIR: &str = "./bin";

/// Tailscale config fragment. Designed to be composed into a service config
/// bucket (e.g. `[service.tailscale]`) via `serde` deserialization.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct TailscaleConfig {
    /// Whether Tailscale should be started at all. Default `true`.
    pub enabled: bool,
    /// Tailscale auth key (the `TS_AUTHKEY` value). When `None`, the daemon is
    /// started but `tailscale up` is skipped; the node will sit unauthenticated.
    pub auth_key: Option<String>,
    /// Hostname to advertise on the tailnet. Default [`DEFAULT_HOSTNAME`].
    pub hostname: String,
    /// Path to the `tailscaled` state file. Defaults to
    /// `/var/lib/tailscale/tailscaled.state`.
    #[serde(with = "pathbuf_to_string")]
    pub state_path: PathBuf,
    /// Path to the `tailscaled` control socket. Defaults to
    /// `/var/run/tailscale/tailscaled.sock`.
    #[serde(with = "pathbuf_to_string")]
    pub socket_path: PathBuf,
    /// Directory containing the bundled `tailscale` and `tailscaled` binaries.
    /// Defaults to `./bin`.
    #[serde(with = "pathbuf_to_string")]
    pub bin_dir: PathBuf,
    /// Extra arguments passed to `tailscale up` verbatim. Useful for flags like
    /// `--advertise-exit-node` or `--advertise-routes=...`.
    pub extra_args: Option<String>,
}

impl Default for TailscaleConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auth_key: None,
            hostname: DEFAULT_HOSTNAME.to_string(),
            state_path: PathBuf::from(DEFAULT_STATE_DIR).join("tailscaled.state"),
            socket_path: PathBuf::from(DEFAULT_SOCKET_DIR).join("tailscaled.sock"),
            bin_dir: PathBuf::from(DEFAULT_BIN_DIR),
            extra_args: None,
        }
    }
}

impl TailscaleConfig {
    /// Path to the `tailscaled` binary.
    pub fn tailscaled_path(&self) -> PathBuf {
        self.bin_dir.join("tailscaled")
    }

    /// Path to the `tailscale` CLI binary.
    pub fn tailscale_path(&self) -> PathBuf {
        self.bin_dir.join("tailscale")
    }

    /// Extra arguments for `tailscale up`, split on whitespace.
    pub fn extra_args(&self) -> Vec<String> {
        self.extra_args
            .as_deref()
            .map(|s| shell_words::split(s).unwrap_or_default())
            .unwrap_or_default()
    }

    /// The control socket path as a `Path`.
    pub fn socket(&self) -> &Path {
        &self.socket_path
    }

    /// The daemon state file as a `Path`.
    pub fn state_file(&self) -> &Path {
        &self.state_path
    }
}

/// Serde adapter that (de)serializes `PathBuf` through its string form.
///
/// This keeps TOML/JSON config human-readable while letting the fragment store
/// typed paths internally.
mod pathbuf_to_string {
    use std::path::PathBuf;

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(path: &PathBuf, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&path.to_string_lossy())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<PathBuf, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(PathBuf::from(s))
    }
}

#[cfg(test)]
mod tests {
    use figment2::providers::{Format, Toml};
    use figment2::Figment;

    use super::*;

    #[test]
    fn defaults_are_sensible() {
        let cfg = TailscaleConfig::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.hostname, "tass-web");
        assert_eq!(cfg.auth_key, None);
        assert_eq!(
            cfg.state_path,
            PathBuf::from("/var/lib/tailscale/tailscaled.state")
        );
        assert_eq!(
            cfg.socket_path,
            PathBuf::from("/var/run/tailscale/tailscaled.sock")
        );
        assert_eq!(cfg.bin_dir, PathBuf::from("./bin"));
        assert_eq!(cfg.extra_args, None);
    }

    #[test]
    fn resolves_binary_paths() {
        let cfg = TailscaleConfig::default();
        assert_eq!(cfg.tailscale_path(), PathBuf::from("./bin/tailscale"));
        assert_eq!(cfg.tailscaled_path(), PathBuf::from("./bin/tailscaled"));
    }

    #[test]
    fn splits_extra_args() {
        let cfg = TailscaleConfig {
            extra_args: Some("--advertise-exit-node --advertise-routes=10.0.0.0/8".to_string()),
            ..Default::default()
        };
        assert_eq!(
            cfg.extra_args(),
            vec!["--advertise-exit-node", "--advertise-routes=10.0.0.0/8"]
        );
    }

    #[test]
    fn deserializes_from_toml() {
        let toml = r#"
[tailscale]
enabled = true
auth_key = "tskey-auth-test"
hostname = "tass-web-prod"
state_path = "/data/tailscale.state"
socket_path = "/tmp/tailscaled.sock"
bin_dir = "/opt/tailscale/bin"
extra_args = "--advertise-exit-node"
"#;
        // The config fragment is intended to be read from `[service.tailscale]`
        // via `extract_cascade`; here we read it directly from `[tailscale]`.
        let figment = Figment::new().merge(Toml::string(toml));
        let cfg: TailscaleConfig = figment.extract_inner("tailscale").unwrap();

        assert!(cfg.enabled);
        assert_eq!(cfg.auth_key.as_deref(), Some("tskey-auth-test"));
        assert_eq!(cfg.hostname, "tass-web-prod");
        assert_eq!(cfg.state_path, PathBuf::from("/data/tailscale.state"));
        assert_eq!(cfg.socket_path, PathBuf::from("/tmp/tailscaled.sock"));
        assert_eq!(cfg.bin_dir, PathBuf::from("/opt/tailscale/bin"));
        assert_eq!(
            cfg.tailscale_path(),
            PathBuf::from("/opt/tailscale/bin/tailscale")
        );
    }

    #[test]
    fn absent_config_uses_defaults() {
        let figment = Figment::new();
        let cfg: TailscaleConfig = figment.extract_inner("tailscale").unwrap_or_default();
        assert_eq!(cfg.hostname, "tass-web");
        assert!(cfg.enabled);
    }
}
