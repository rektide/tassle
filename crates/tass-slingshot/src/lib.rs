//! A [`tass_engine::Hydrator`] backed by microcosm's
//! [Slingshot](https://slingshot.microcosm.blue/) record cache.
//!
//! Spacedust hands us a pointer (`source_record` at-uri); to read the command
//! text we need the record body. Slingshot is a convenience cache that serves
//! the standard `com.atproto.repo.getRecord`, so we point an unauthenticated
//! jacquard client at a Slingshot instance and reuse [`tass_repo::get_record`].
//! The alternative — fetching from the owning PDS directly — is tracked by
//! `tass-hydrate-pds`.

use jacquard::client::BasicClient;
use jacquard_common::deps::fluent_uri::Uri;
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::xrpc::XrpcClient;
use tass_engine::{parse_at_uri, Hydrator};

/// The public microcosm Slingshot instance.
pub const DEFAULT_BASE: &str = "https://slingshot.microcosm.blue";

/// The tass-slingshot config fragment. A service composes this into its own
/// config (e.g. `[service.listen.slingshot]`) rather than re-declaring it.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SlingshotConfig {
    /// Slingshot base URL. Defaults to the public instance ([`DEFAULT_BASE`]).
    #[serde(default = "default_base")]
    pub base: String,
}

fn default_base() -> String {
    DEFAULT_BASE.to_string()
}

impl Default for SlingshotConfig {
    fn default() -> Self {
        Self {
            base: default_base(),
        }
    }
}

/// Hydrates records by calling `getRecord` against a Slingshot instance.
#[derive(Debug, Clone)]
pub struct SlingshotHydrator {
    base: String,
}

impl SlingshotHydrator {
    /// Point at a specific Slingshot base URL (e.g. a self-hosted instance).
    pub fn new(base: impl Into<String>) -> Self {
        Self { base: base.into() }
    }

    /// The public microcosm instance ([`DEFAULT_BASE`]).
    pub fn public() -> Self {
        Self::new(DEFAULT_BASE)
    }

    /// Build from a [`SlingshotConfig`] fragment.
    pub fn from_config(config: SlingshotConfig) -> Self {
        Self::new(config.base)
    }
}

impl Default for SlingshotHydrator {
    fn default() -> Self {
        Self::public()
    }
}

/// Errors hydrating a record via Slingshot.
#[derive(Debug, thiserror::Error)]
pub enum SlingshotError {
    #[error("malformed at-uri: {0}")]
    BadUri(String),
    #[error("invalid Slingshot base URL {0}: {1}")]
    BadBase(String, String),
    #[error("getRecord failed: {0}")]
    Fetch(String),
    #[error("record not found: {0}")]
    NotFound(String),
}

impl Hydrator for SlingshotHydrator {
    type Error = SlingshotError;

    async fn hydrate(&self, at_uri: &str) -> Result<serde_json::Value, SlingshotError> {
        let (did, collection, rkey) =
            parse_at_uri(at_uri).ok_or_else(|| SlingshotError::BadUri(at_uri.to_string()))?;

        // Unauthenticated read pointed at Slingshot (a public cache, no auth).
        let client = BasicClient::unauthenticated();
        let base = Uri::parse(self.base.clone())
            .map_err(|(e, _)| SlingshotError::BadBase(self.base.clone(), e.to_string()))?;
        client.set_base_uri(base).await;

        let repo =
            AtIdentifier::new_owned(did).map_err(|e| SlingshotError::BadUri(e.to_string()))?;

        let record = tass_repo::get_record(&client, repo, collection, rkey)
            .await
            .map_err(|e| SlingshotError::Fetch(e.to_string()))?
            .ok_or_else(|| SlingshotError::NotFound(at_uri.to_string()))?;

        Ok(record.value)
    }
}
