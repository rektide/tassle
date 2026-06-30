//! Tassle config + profile + auth bundle — the foundation the app bootstraps
//! from.
//!
//! This crate consolidates the three concerns that today live scattered across
//! `tassle-cli`'s `config.rs` / `profile_config.rs` / `auth.rs`:
//!
//! - **config** ([`config`]): figment2 config dir, active-profile selection, the
//!   [`Profile`] shape.
//! - **auth** ([`auth`], behind the `auth-store` feature): an [`AuthedClient`]
//!   resumed from the active profile's fjall app-password store and pointed at
//!   its PDS — the single path write paths compose.
//!
//! ```no_run
//! # #[cfg(feature = "auth-store")] {
//! use tassle_config::AuthedClient;
//! # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
//! let c = AuthedClient::for_active_profile().await?;
//! // c.session() is a jacquard XrpcClient ready for write calls.
//! # Ok(()) } }
//! ```
//!
//! This is an initial **spike** to evaluate the layering and API; it duplicates
//! the figment profile model from `tassle-cli::config` rather than extracting
//! it yet. Migration (make `tassle-cli` depend on this crate and delete its
//! copies) is the follow-up once the design settles.

pub mod config;

#[cfg(feature = "auth-store")]
pub mod auth;

pub use config::Profile;

/// Resolve the active [`Profile`] from figment (CLI/env override > file), in one
/// call. Convenience over [`config::active_figment`] + [`config::active_profile`].
pub fn active() -> miette::Result<Profile> {
    let figment = config::active_figment(None)?;
    config::active_profile(&figment)
}

#[cfg(feature = "auth-store")]
pub use auth::{AuthError, AuthedClient, AppPasswordSession};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_is_resolvable_needs_account_and_pds() {
        let neither = Profile::default();
        assert!(!neither.is_resolvable());

        let with_did = Profile {
            did: Some("did:plc:abc".into()),
            ..Default::default()
        };
        assert!(!with_did.is_resolvable()); // no pds

        let full = Profile {
            did: Some("did:plc:abc".into()),
            pds: Some("https://pds.example".into()),
            ..Default::default()
        };
        assert!(full.is_resolvable());

        let handle_only = Profile {
            handle: Some("foo.bar".into()),
            pds: Some("https://pds.example".into()),
            ..Default::default()
        };
        assert!(handle_only.is_resolvable());
    }

    // Mutates process-global env (XDG_CONFIG_HOME, TASSLE_PROFILE), so it must
    // not run alongside any other env-mutating test. Kept as the single such
    // test here; env::set_var/remove_var are unsafe under edition 2024.
    #[test]
    fn build_figment_tolerates_missing_config() {
        let tmp = tempfile::tempdir().unwrap();
        // SAFETY: no other test in this crate mutates these vars, so there is
        // no concurrent access while this test runs.
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", tmp.path());
            std::env::remove_var("TASSLE_PROFILE");
        }
        let fig = config::build_figment(None);
        assert!(fig.is_ok(), "build_figment should tolerate a missing config");
        unsafe {
            std::env::remove_var("XDG_CONFIG_HOME");
        }
    }
}
