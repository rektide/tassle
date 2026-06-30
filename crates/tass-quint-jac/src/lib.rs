//! Jacquard-backed read/write for [`tass_quint`] against the mage block of an
//! `actor.rpg.stats` record.
//!
//! [`QuintClient`] is generic over any `jacquard` client implementing
//! [`XrpcClient`] — read works unauthenticated (a public getRecord), write
//! requires whatever authenticated client the caller supplies (jacquard carries
//! auth in `CallOptions.auth`, so the same trait serves both). The crate does
//! not resolve DIDs/handles to a PDS itself: point the client's `base_uri` at
//! the actor's PDS first, the way `tassle-cli`'s `mage list` does.
//!
//! ```no_run
//! # use jacquard::client::BasicClient;
//! use tass_quint_jac::QuintClient;
//! # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
//! let qc = QuintClient::new(BasicClient::unauthenticated());
//! // point base_uri at the actor's PDS first, then:
//! let _current = qc.read("did:plc:…", "mage").await?;
//! # Ok(()) }
//! ```
//!
//! See the `tass-quint` crate for the resolution / floor / replication rules.
//! Per `doc/ledger.md`, sheet writes through [`QuintClient::write`] are an
//! explicit command — the ledger never silently mutates `actor.rpg.stats`.

use chrono::Utc;
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::types::string::{Nsid, RecordKey};
use jacquard_common::types::value::Data;
use jacquard_common::DefaultStr;
use jacquard_common::xrpc::atproto::{
    GetRecord, GetRecordError, GetRecordOutput, PutRecord,
};
use jacquard_common::xrpc::{XrpcClient, XrpcError};
use serde_json::{Map, Value};
use tass_quint::Quint;

/// Default collection (the rpg.actor host record).
pub const STATS_COLLECTION: &str = "actor.rpg.stats";
/// Default rkey for the canonical modern mage record.
pub const DEFAULT_RKEY: &str = "mage";

pub type Result<T> = std::result::Result<T, QuintError>;

/// Errors from [`QuintClient`]. Hand-rolled (no thiserror) to keep the
/// dependency surface minimal, matching `tassle-ledger`.
#[derive(Debug)]
pub enum QuintError {
    /// A DID/rkey/collection string failed AT-Proto syntax validation.
    Ident(jacquard_common::types::string::AtStrError),
    /// A jacquard XRPC transport/typed error, stringified so this crate stays
    /// generic over the caller's client without leaking its error type params.
    Xrpc(String),
    /// The record has no mage block to patch (write only).
    NoMageBlock,
    /// (De)serializing the record body failed.
    Serde(serde_json::Error),
}

impl std::fmt::Display for QuintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuintError::Ident(e) => write!(f, "invalid identifier: {e}"),
            QuintError::Xrpc(e) => write!(f, "xrpc error: {e}"),
            QuintError::NoMageBlock => write!(f, "record has no mage block to patch"),
            QuintError::Serde(e) => write!(f, "record (de)serialize error: {e}"),
        }
    }
}

impl std::error::Error for QuintError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            QuintError::Ident(e) => Some(e),
            QuintError::Serde(e) => Some(e),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for QuintError {
    fn from(e: serde_json::Error) -> Self {
        QuintError::Serde(e)
    }
}

/// Read/write access to mage pattern-quintessence on `actor.rpg.stats`.
///
/// Generic over any `C: XrpcClient`. Pass an unauthenticated client (e.g.
/// `BasicClient::unauthenticated()`) for public reads; pass an authenticated
/// session/agent for writes. Whether a write succeeds is enforced by the PDS
/// based on the client's auth, not by this type.
pub struct QuintClient<C> {
    client: C,
}

impl<C: XrpcClient + Sync> QuintClient<C> {
    /// Wrap a jacquard client. The caller is responsible for pointing the
    /// client's `base_uri` at the actor's PDS before calling read/write.
    pub fn new(client: C) -> Self {
        Self { client }
    }

    /// Borrow the underlying client.
    pub fn client(&self) -> &C {
        &self.client
    }

    /// Read the mage pattern-quintessence for `repo`/`rkey`.
    ///
    /// Returns `Ok(None)` when the record is absent or carries no resolvable
    /// quintessence. Prefers the `milliQuintessence` extension field and hydrates from
    /// `quintessence * 1000` for legacy sheets (see [`tass_quint::resolve`]).
    pub async fn read(&self, repo: &str, rkey: &str) -> Result<Option<Quint>> {
        let Some(record) = self.get_record(repo, rkey).await? else {
            return Ok(None);
        };
        let value = serde_json::to_value(&record.value)?;
        Ok(extract_quint(&value))
    }

    /// Read-modify-write the mage block: sets `milliQuintessence` to `q.millis()` and
    /// replicates the floored value into `quintessence` (see
    /// [`tass_quint::sheet_patch`]). All other mage fields and the record
    /// envelope are preserved; `updatedAt` is bumped. Requires an authenticated
    /// client or the PDS will reject the putRecord.
    ///
    /// Returns the applied [`Quint`] on success.
    pub async fn write(&self, repo: &str, rkey: &str, q: Quint) -> Result<Quint> {
        let Some(record) = self.get_record(repo, rkey).await? else {
            return Err(QuintError::NoMageBlock);
        };
        let mut value = serde_json::to_value(&record.value)?;
        let mage = mage_block_mut(&mut value).ok_or(QuintError::NoMageBlock)?;
        apply_quint(mage, q);
        // Bump the record-level updatedAt (LWW marker), not the mage block's.
        if let Some(root) = value.as_object_mut() {
            root.insert("updatedAt".to_string(), Value::from(Utc::now().to_rfc3339()));
        }
        let data: Data<DefaultStr> = serde_json::from_value(value)?;
        self.put_record(repo, rkey, data).await?;
        Ok(q)
    }

    async fn get_record(&self, repo: &str, rkey: &str) -> Result<Option<GetRecordOutput>> {
        let request = GetRecord::<DefaultStr> {
            repo: AtIdentifier::new_owned(repo).map_err(QuintError::Ident)?,
            collection: Nsid::new_static(STATS_COLLECTION).map_err(QuintError::Ident)?,
            rkey: RecordKey::any_owned(rkey).map_err(QuintError::Ident)?,
            cid: None,
        };
        let response = self
            .client
            .send(request)
            .await
            .map_err(|e| QuintError::Xrpc(e.to_string()))?;
        match response.into_output() {
            Ok(output) => Ok(Some(output)),
            Err(XrpcError::Xrpc(GetRecordError::RecordNotFound(_))) => Ok(None),
            Err(e) => Err(QuintError::Xrpc(e.to_string())),
        }
    }

    async fn put_record(&self, repo: &str, rkey: &str, record: Data<DefaultStr>) -> Result<()> {
        let request = PutRecord::<DefaultStr> {
            repo: AtIdentifier::new_owned(repo).map_err(QuintError::Ident)?,
            collection: Nsid::new_static(STATS_COLLECTION).map_err(QuintError::Ident)?,
            rkey: RecordKey::any_owned(rkey).map_err(QuintError::Ident)?,
            record,
            swap_commit: None,
            swap_record: None,
            validate: None,
        };
        self.client
            .send(request)
            .await
            .map_err(|e| QuintError::Xrpc(e.to_string()))?;
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Pure helpers — testable without a client.
// ─────────────────────────────────────────────────────────────────────────────

/// Locate the mage field map inside a record value.
///
/// Handles the two real production shapes (mirroring `tassle-cli`'s
/// `extract_mage`): the per-system envelope `{ system: "mage", data: {…} }`
/// and the legacy inline `{ mage: {…} }`.
pub(crate) fn mage_block(value: &Value) -> Option<&Map<String, Value>> {
    if value.get("system").and_then(Value::as_str) == Some("mage")
        && let Some(data) = value.get("data")
        && let Some(obj) = data.as_object()
    {
        return Some(obj);
    }
    value.get("mage").and_then(Value::as_object)
}

/// Mutable counterpart of [`mage_block`].
pub(crate) fn mage_block_mut(value: &mut Value) -> Option<&mut Map<String, Value>> {
    let is_envelope = value.get("system").and_then(Value::as_str) == Some("mage");
    if is_envelope {
        return value.get_mut("data").and_then(Value::as_object_mut);
    }
    value.get_mut("mage").and_then(Value::as_object_mut)
}

/// Read `milliQuintessence`/`quintessence` out of a record value and resolve to a [`Quint`].
pub(crate) fn extract_quint(value: &Value) -> Option<Quint> {
    let mage = mage_block(value)?;
    let milli_quintessence = mage.get("milliQuintessence").and_then(Value::as_i64);
    let quintessence = mage.get("quintessence").and_then(Value::as_i64);
    tass_quint::resolve(milli_quintessence, quintessence)
}

/// Write the [`tass_quint::sheet_patch`] fields into a mage field map.
pub(crate) fn apply_quint(mage: &mut Map<String, Value>, q: Quint) {
    let patch = tass_quint::sheet_patch(q);
    mage.insert(
        "milliQuintessence".to_string(),
        Value::from(patch.milli_quintessence),
    );
    mage.insert(
        "quintessence".to_string(),
        Value::from(patch.quintessence),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn envelope(mage_fields: Value) -> Value {
        json!({ "system": "mage", "data": mage_fields, "$type": "actor.rpg.stats" })
    }

    fn inline(mage_fields: Value) -> Value {
        json!({ "mage": mage_fields, "$type": "actor.rpg.stats" })
    }

    #[test]
    fn extract_prefers_milli_field() {
        let v = envelope(json!({ "milliQuintessence": 1500, "quintessence": 9 }));
        assert_eq!(extract_quint(&v), Some(Quint::from_millis(1500)));
        assert_eq!(extract_quint(&v).unwrap().points(), 1);
    }

    #[test]
    fn extract_hydrates_from_legacy_quintessence() {
        let v = envelope(json!({ "quintessence": 9 }));
        assert_eq!(extract_quint(&v), Some(Quint::from_points(9)));
    }

    #[test]
    fn extract_works_for_inline_legacy_shape() {
        let v = inline(json!({ "Quintessence": 7 }));
        // Capitalized alias is NOT handled here (mage.rs normalizes case);
        // the crate speaks the lexicon-canonical lowercase wire field.
        assert_eq!(extract_quint(&v), None);
        let v2 = inline(json!({ "quintessence": 7 }));
        assert_eq!(extract_quint(&v2), Some(Quint::from_points(7)));
    }

    #[test]
    fn extract_none_when_no_mage_block() {
        let v = json!({ "system": "vampire", "data": {} });
        assert_eq!(extract_quint(&v), None);
    }

    #[test]
    fn extract_none_when_both_fields_absent() {
        let v = envelope(json!({ "arete": 3 }));
        assert_eq!(extract_quint(&v), None);
    }

    #[test]
    fn apply_replicates_into_legacy_field() {
        let mut v = envelope(json!({ "arete": 3, "quintessence": 5 }));
        let mage = mage_block_mut(&mut v).unwrap();
        apply_quint(mage, Quint::from_millis(1_500));
        let mage = mage_block(&v).unwrap();
        assert_eq!(mage.get("milliQuintessence").and_then(Value::as_i64), Some(1500));
        assert_eq!(mage.get("quintessence").and_then(Value::as_i64), Some(1));
        // sibling field preserved
        assert_eq!(mage.get("arete").and_then(Value::as_i64), Some(3));
    }

    #[test]
    fn apply_preserves_envelope_and_type() {
        let mut v = envelope(json!({ "quintessence": 0 }));
        {
            let mage = mage_block_mut(&mut v).unwrap();
            apply_quint(mage, Quint::from_points(2));
        }
        // envelope structure untouched
        assert_eq!(v.get("system").and_then(Value::as_str), Some("mage"));
        assert_eq!(v.get("$type").and_then(Value::as_str), Some("actor.rpg.stats"));
        assert_eq!(
            v.get("data").unwrap().get("milliQuintessence").and_then(Value::as_i64),
            Some(2000)
        );
    }

    #[test]
    fn apply_to_inline_shape_patches_mage_key() {
        let mut v = inline(json!({ "quintessence": 0 }));
        {
            let mage = mage_block_mut(&mut v).unwrap();
            apply_quint(mage, Quint::from_points(4));
        }
        assert_eq!(
            v.get("mage").unwrap().get("milliQuintessence").and_then(Value::as_i64),
            Some(4000)
        );
        assert_eq!(
            v.get("mage").unwrap().get("quintessence").and_then(Value::as_i64),
            Some(4)
        );
    }
}
