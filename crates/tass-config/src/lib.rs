//! Tassle config + profile + auth bundle — the foundation the app bootstraps
//! from.
//!
//! This crate owns the config/auth concerns that used to live in `tass-cli`:
//!
//! - **config** ([`config`]): figment2 config dir, generic profile selection, the
//!   [`Login`] shape. `tass-cli` consumes this module directly (its old
//!   `config.rs` copy was deleted); the legacy did-keyed `profile_config.rs`
//!   write helpers still live in the CLI pending their own retirement.
//! - **auth** ([`auth`], behind the `auth-store` feature): an [`AuthedClient`]
//!   resumed from the active profile's turso app-password store and pointed at
//!   its PDS — the single path write paths compose.
//!
//! ```no_run
//! # #[cfg(feature = "auth-store")] {
//! use tass_config::AuthedClient;
//! # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
//! let authed = AuthedClient::for_active_profile().await?;
//! // authed.session() lends &CredentialSession — pass it to a QuintClient::new(&…):
//! let _session = authed.session();
//! # Ok(()) } }
//! ```
//!
//! The figment profile model is now the single copy: `tass-cli` depends on
//! this crate and its duplicate `config.rs` was removed (tass-cli-config-dedup).
//! "profile" is generic (a figment config bucket); the account identity it may
//! carry is a [`Login`] (tass-config-profile-generic). The [`service`] module
//! adds the non-login `[service]` / `[service.oauth]` config for the axum OAuth
//! web flow (tass-config-service-shape); next is a login-kind model —
//! app-password | oauth (tass-config-login-kinds).

pub mod config;
pub mod dirs;
pub mod service;

#[cfg(feature = "auth-store")]
pub mod auth;

#[cfg(feature = "auth-store")]
pub mod read;

pub use config::{auth_selector, CredentialSelector, Login};
pub use service::{OAuthConfig, ServiceConfig};

/// Resolve the active profile's [`Login`] from figment (CLI/env override > file),
/// in one call. Convenience over [`config::active_figment`] + [`config::active_login`].
pub fn active_login() -> miette::Result<Login> {
    let figment = config::active_figment(None)?;
    config::active_login(&figment)
}

#[cfg(feature = "auth-store")]
pub use auth::{stored_access_jwt, AppPasswordSession, AuthError, AuthedClient, LoginOutcome};

#[cfg(feature = "auth-store")]
pub use read::{read_client, OAuthReadSession, ReadClient};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_is_resolvable_needs_account_and_pds() {
        let neither = Login::default();
        assert!(!neither.is_resolvable());

        let with_did = Login {
            did: Some("did:plc:abc".into()),
            ..Default::default()
        };
        assert!(!with_did.is_resolvable()); // no pds

        let full = Login {
            did: Some("did:plc:abc".into()),
            pds: Some("https://pds.example".into()),
            ..Default::default()
        };
        assert!(full.is_resolvable());
        assert_eq!(full.account(), Some("did:plc:abc"));

        let handle_only = Login {
            handle: Some("foo.bar".into()),
            pds: Some("https://pds.example".into()),
            ..Default::default()
        };
        assert!(handle_only.is_resolvable());
        assert_eq!(handle_only.account(), Some("foo.bar"));
    }

    // Mutates process-global env (XDG_CONFIG_HOME, TASS_PROFILE), so it must
    // not run alongside any other env-mutating test. Kept as the single such
    // test here; env::set_var/remove_var are unsafe under edition 2024.
    #[test]
    fn build_figment_tolerates_missing_config() {
        let tmp = tempfile::tempdir().unwrap();
        // SAFETY: no other test in this crate mutates these vars, so there is
        // no concurrent access while this test runs.
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", tmp.path());
            std::env::remove_var("TASS_PROFILE");
        }
        let fig = config::build_figment(None);
        assert!(fig.is_ok(), "build_figment should tolerate a missing config");
        unsafe {
            std::env::remove_var("XDG_CONFIG_HOME");
        }
    }

    // Compile-checks that the auth types exist and are constructible from the
    // crate root. Gated by `auth-store` since they only exist under it.
    #[cfg(feature = "auth-store")]
    #[test]
    fn auth_types_are_exported() {
        // Forces the re-exports to resolve; "used" via a dead-code take.
        fn _check() {
            let _ = std::any::TypeId::of::<crate::AuthedClient>();
            let _ = std::any::TypeId::of::<crate::AppPasswordSession>();
        }
    }
}
