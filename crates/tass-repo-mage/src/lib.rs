//! Jacquard-backed DAO for the `actor.rpg.stats/mage` record.
//!
//! The mage record data-access layer: it owns reading and writing the mage
//! block of an `actor.rpg.stats` record over jacquard. Its first hosted
//! operation is the [`tass_quint`] pattern-quintessence field RMW
//! ([`QuintClient`]); the crate is the home for mage-record I/O generally, not
//! quint-specific. (Renamed from `tass-quint-jac`.)
//!
//! [`QuintClient`] is generic over any `jacquard` client implementing
//! [`XrpcClient`] — read works unauthenticated (a public getRecord), write
//! requires whatever authenticated client the caller supplies (jacquard carries
//! auth in `CallOptions.auth`, so the same trait serves both). The crate does
//! not resolve DIDs/handles to a PDS itself: point the client's `base_uri` at
//! the actor's PDS first, the way `tass-cli`'s `mage list` does.
//!
//! ```no_run
//! # use jacquard::client::BasicClient;
//! use tass_repo_mage::QuintClient;
//! # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
//! let client = BasicClient::unauthenticated();
//! let qc = QuintClient::new(&client);
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
use tass_mage::{mage_block, mage_block_mut};
use tass_quint::{
    coherent_quint, sheet_patch, Coherence, MilliIsTruthCoherence, Quint, ReadReport,
    SheetFields, SheetPatch,
};

/// Default collection (the rpg.actor host record).
pub const STATS_COLLECTION: &str = "actor.rpg.stats";
/// Default rkey for the canonical modern mage record.
pub const DEFAULT_RKEY: &str = "mage";

/// One page of `actor.rpg.stats` records, each summarized by shape (see
/// [`tass_stats::summarize_record`]).
pub struct StatsPage {
    pub cursor: Option<String>,
    pub records: Vec<tass_stats::StatsSummary>,
}

/// Read one `actor.rpg.stats` record by `rkey`, or `Ok(None)` if absent.
///
/// The repo-touching read half of the mage DAO: it owns the collection NSID so
/// callers don't. The client must already be pointed at the actor's PDS.
pub async fn get_stats_record<C: XrpcClient + Sync + ?Sized>(
    client: &C,
    repo: AtIdentifier,
    rkey: &str,
) -> tass_repo::Result<Option<tass_repo::RecordEnvelope>> {
    tass_repo::get_record(client, repo, STATS_COLLECTION, rkey).await
}

/// List an actor's `actor.rpg.stats` records, each summarized by shape. The
/// client must already be pointed at the actor's PDS.
pub async fn list_stats_records<C: XrpcClient + Sync + ?Sized>(
    client: &C,
    repo: AtIdentifier,
    limit: Option<i64>,
    cursor: Option<String>,
    reverse: bool,
) -> tass_repo::Result<StatsPage> {
    let page =
        tass_repo::list_records(client, repo, STATS_COLLECTION, limit, cursor, reverse).await?;
    let records = page
        .records
        .into_iter()
        .map(|env| tass_stats::summarize_record(&env.uri, env.cid.as_deref(), &env.value))
        .collect();
    Ok(StatsPage {
        cursor: page.cursor,
        records,
    })
}

pub type Result<T> = std::result::Result<T, QuintError>;

/// Errors from [`QuintClient`]. Hand-rolled (no thiserror) to keep the
/// dependency surface minimal, matching `tass-ledger`.
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

/// How [`QuintClient::write_with`] stamps `milliQuintessenceUpdatedAt`.
///
/// The common case is [`Stamp::Now`]: the write layer generates the timestamp
/// itself. Callers backdating or reproducing a value supply [`Stamp::At`], and
/// callers who don't want the field touched use [`Stamp::None`].
#[derive(Debug, Clone)]
enum Stamp {
    /// Generate the stamp from the wall clock (the default).
    Now,
    /// Use a caller-supplied ISO-8601 timestamp.
    At(String),
    /// Don't write `milliQuintessenceUpdatedAt`.
    None,
}

/// Options for [`QuintClient::write_with`].
///
/// Defaults to **stamping "now"** — the write generates the
/// `milliQuintessenceUpdatedAt` timestamp unless the caller opts out or
/// supplies their own:
///
/// ```
/// use tass_repo_mage::WriteOpts;
/// let _default_now = WriteOpts::default();                       // stamp now
/// let _at = WriteOpts::default().at("2026-06-30T00:00:00Z");     // caller time
/// let _off = WriteOpts::default().unstamped();                   // no stamp
/// ```
#[derive(Debug, Clone)]
pub struct WriteOpts {
    stamp: Stamp,
}

impl Default for WriteOpts {
    fn default() -> Self {
        Self { stamp: Stamp::Now }
    }
}

impl WriteOpts {
    /// Stamp with a caller-supplied ISO-8601 time instead of generating "now".
    pub fn at(mut self, ts: impl Into<String>) -> Self {
        self.stamp = Stamp::At(ts.into());
        self
    }

    /// Don't write `milliQuintessenceUpdatedAt` at all.
    pub fn unstamped(mut self) -> Self {
        self.stamp = Stamp::None;
        self
    }
}

/// Read/write access to mage pattern-quintessence on `actor.rpg.stats`.
///
/// **Borrows** the jacquard client (`&'c C`), so the caller keeps owning it.
/// Pass `&basic_client` for unauthenticated public reads, or `&authed_session`
/// for writes — whether a write succeeds is enforced by the PDS based on the
/// client's auth, not by this type. Because the session is borrowed, a single
/// live session can be shared by many concurrent `QuintClient` uses (the safe
/// model — see the crate docs and the `tass-config-session-source` ticket).
///
/// The coherence policy is pluggable via the `Co` type parameter (default
/// [`MilliIsTruthCoherence`]); swap it with [`with_coherence`](Self::with_coherence).
/// See `doc/microquint.md` and the `tass-quint-stale-sync` ticket.
pub struct QuintClient<'c, C: XrpcClient + Sync + ?Sized, Co: Coherence = MilliIsTruthCoherence> {
    client: &'c C,
    coherence: Co,
}

/// Construction — fixed to the default coherence ([`MilliIsTruthCoherence`]).
impl<'c, C: XrpcClient + Sync + ?Sized> QuintClient<'c, C, MilliIsTruthCoherence> {
    /// Borrow a jacquard client with the default coherence policy. The caller
    /// is responsible for pointing the client's `base_uri` at the actor's PDS
    /// before calling read/write.
    pub fn new(client: &'c C) -> Self {
        QuintClient {
            client,
            coherence: MilliIsTruthCoherence,
        }
    }
}

/// Operations — generic over the coherence policy.
impl<'c, C: XrpcClient + Sync + ?Sized, Co: Coherence> QuintClient<'c, C, Co> {
    /// Swap in a different coherence policy. Consumes `self` and returns a
    /// `QuintClient` carrying the new policy — the borrowed client is carried
    /// across, so callers chain this right after [`new`](Self::new):
    ///
    /// ```
    /// # use jacquard::client::BasicClient;
    /// use tass_quint::MilliIsTruthCoherence;
    /// use tass_repo_mage::QuintClient;
    /// # async fn demo() {
    /// let client = BasicClient::unauthenticated();
    /// let qc = QuintClient::new(&client).with_coherence(MilliIsTruthCoherence);
    /// # }
    /// ```
    ///
    /// This is the seam by which a chronicle opts into a non-default drift
    /// rule (e.g. a future `QuintessenceIsTruthCoherence` from the
    /// `tass-quint-sync-direction` ticket). The default [`new`](Self::new)
    /// path is unaffected.
    pub fn with_coherence<Co2: Coherence>(self, coherence: Co2) -> QuintClient<'c, C, Co2> {
        QuintClient {
            client: self.client,
            coherence,
        }
    }

    /// The borrowed client reference.
    pub fn client(&self) -> &'c C {
        self.client
    }

    /// The active coherence policy.
    pub fn coherence(&self) -> &Co {
        &self.coherence
    }

    /// Read the mage pattern-quintessence for `repo`/`rkey`.
    ///
    /// Returns `Ok(None)` when the record is absent or carries no resolvable
    /// quintessence. The returned [`Quint`] is the **coherent** one: inline
    /// drift detection (via the active coherence policy; default
    /// [`MilliIsTruthCoherence`]) runs before the value leaves this method,
    /// so callers see the source-of-truth value even when the sheet's two
    /// fields (or their timestamps) have drifted. A read MUST NOT issue a
    /// write to repair the sheet — the next `write_with` is what repairs
    /// storage; this method just returns the coherent view of what's on the
    /// wire.
    ///
    /// Drift is silent on this path (the value is repaired in the returned
    /// [`Quint`]). Use [`read_report`](Self::read_report) to surface the
    /// [`tass_quint::SyncDecision`] alongside the value.
    pub async fn read(&self, repo: &str, rkey: &str) -> Result<Option<Quint>> {
        Ok(self.read_report(repo, rkey).await?.map(|r| r.quint))
    }

    /// Like [`read`](Self::read) but returns the [`tass_quint::ReadReport`] so
    /// callers can observe drift (the [`tass_quint::SyncDecision`] the inline
    /// coherence check made) alongside the coherent [`Quint`].
    ///
    /// This is the read variant for callers that log/audit sheet coherence.
    /// It is **not** a sync verb — it shares the same single fetch +
    /// classify-with-the-active-coherence path as [`read`](Self::read); the
    /// only difference is that the decision is returned instead of discarded.
    pub async fn read_report(&self, repo: &str, rkey: &str) -> Result<Option<ReadReport>> {
        let Some(record) = self.get_record(repo, rkey).await? else {
            return Ok(None);
        };
        let value = serde_json::to_value(&record.value)?;
        let Some(fields) = extract_fields(&value) else {
            return Ok(None);
        };
        let decision = self.coherence.classify(&fields);
        let quint = match coherent_quint(&fields, decision) {
            Some(q) => q,
            None => return Ok(None),
        };
        Ok(Some(ReadReport { quint, decision }))
    }

    /// Read-modify-write the mage block, **stamping `milliQuintessenceUpdatedAt`
    /// with "now"** (the common case). Equivalent to
    /// [`write_with`](Self::write_with) with [`WriteOpts::default`].
    ///
    /// Requires an authenticated client or the PDS will reject the putRecord.
    /// Returns the applied [`Quint`] on success.
    pub async fn write(&self, repo: &str, rkey: &str, q: Quint) -> Result<Quint> {
        self.write_with(repo, rkey, q, WriteOpts::default()).await
    }

    /// Read-modify-write with full control over the timestamp stamp.
    ///
    /// Sets `milliQuintessence` to `q.millis()`, replicates the floor into
    /// `quintessence`, and stamps `milliQuintessenceUpdatedAt` per `opts`
    /// (now by default, a caller-supplied time via [`WriteOpts::at`], or none
    /// via [`WriteOpts::unstamped`]). The record-level `updatedAt` always bumps
    /// to "now" — it marks when the write happened, independent of the milli
    /// provenance stamp. All other mage fields and the record envelope are
    /// preserved.
    pub async fn write_with(
        &self,
        repo: &str,
        rkey: &str,
        q: Quint,
        opts: WriteOpts,
    ) -> Result<Quint> {
        let Some(record) = self.get_record(repo, rkey).await? else {
            return Err(QuintError::NoMageBlock);
        };
        let mut value = serde_json::to_value(&record.value)?;
        let now = Utc::now().to_rfc3339();
        let patch = build_patch(q, &opts, &now);
        {
            let mage = mage_block_mut(&mut value).ok_or(QuintError::NoMageBlock)?;
            apply_quint(mage, &patch);
        }
        // Record-level updatedAt always bumps to "now" (the write happened now).
        if let Some(root) = value.as_object_mut() {
            root.insert("updatedAt".to_string(), Value::from(now.as_str()));
        }
        let data: Data<DefaultStr> = serde_json::from_value(value)?;
        self.put_record(repo, rkey, data).await?;
        Ok(q)
    }

    /// Read-modify-write: add `delta` to the current value (defaulting to 0 when
    /// absent) and write it back, stamping "now". Equivalent to
    /// [`adjust_with`](Self::adjust_with) with [`WriteOpts::default`].
    ///
    /// **Non-atomic**: the read and the write are separate XRPC calls, so a
    /// concurrent writer between them can be clobbered. Fine for a single-user
    /// CLI; not safe under concurrent writers without optimistic concurrency
    /// (swap records / `swapCommit`), which this does not do yet.
    pub async fn adjust(&self, repo: &str, rkey: &str, delta: Quint) -> Result<Quint> {
        self.adjust_with(repo, rkey, delta, WriteOpts::default()).await
    }

    /// Read-modify-write with full control over the timestamp stamp.
    pub async fn adjust_with(
        &self,
        repo: &str,
        rkey: &str,
        delta: Quint,
        opts: WriteOpts,
    ) -> Result<Quint> {
        let current = self
            .read(repo, rkey)
            .await?
            .unwrap_or_else(|| Quint::from_millis(0));
        let next = current.add_millis(delta.millis());
        self.write_with(repo, rkey, next, opts).await
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
//
// Mage-block location lives in `tass_mage` (shared with the CLI); the helpers
// below are the quint/coherence bridge over that block.
// ─────────────────────────────────────────────────────────────────────────────

/// Read `milliQuintessence`/`quintessence` out of a record value and resolve
/// to a [`Quint`], bypassing the coherence seam. Kept for tests and as an
/// escape hatch for future callers that want the raw resolved value without
/// the inline drift check; the production read path is
/// [`QuintClient::read`](QuintClient::read) / [`QuintClient::read_report`](QuintClient::read_report).
#[allow(dead_code)]
pub(crate) fn extract_quint(value: &Value) -> Option<Quint> {
    let mage = mage_block(value)?;
    let milli_quintessence = mage.get("milliQuintessence").and_then(Value::as_i64);
    let quintessence = mage.get("quintessence").and_then(Value::as_i64);
    tass_quint::resolve(milli_quintessence, quintessence)
}

/// Pull the four raw fields the [`Coherence`] seam looks at out of a record
/// value: `milliQuintessence`, `quintessence`, `milliQuintessenceUpdatedAt`
/// (narrow stamp, from the mage block), and record-level `updatedAt` (broad
/// stamp, from the record root). Returns `None` when no mage block is present.
///
/// Field-level `None`s are preserved and surface as `None` on the
/// [`SheetFields`] (e.g. a legacy sheet with no milli field — the coherence
/// default treats that as `InSync` and defers to [`tass_quint::resolve`]).
pub(crate) fn extract_fields(value: &Value) -> Option<SheetFields<'_>> {
    let mage = mage_block(value)?;
    Some(SheetFields {
        milli_quintessence: mage.get("milliQuintessence").and_then(Value::as_i64),
        quintessence: mage.get("quintessence").and_then(Value::as_i64),
        milli_quintessence_updated_at: mage
            .get("milliQuintessenceUpdatedAt")
            .and_then(Value::as_str),
        updated_at: value.get("updatedAt").and_then(Value::as_str),
    })
}

/// Write a [`SheetPatch`] into a mage field map: `milliQuintessence`, the
/// replicated `quintessence` floor, and `milliQuintessenceUpdatedAt` when the
/// patch carries one.
pub(crate) fn apply_quint(mage: &mut Map<String, Value>, patch: &SheetPatch) {
    mage.insert(
        "milliQuintessence".to_string(),
        Value::from(patch.milli_quintessence),
    );
    mage.insert(
        "quintessence".to_string(),
        Value::from(patch.quintessence),
    );
    if let Some(ts) = &patch.milli_quintessence_updated_at {
        mage.insert(
            "milliQuintessenceUpdatedAt".to_string(),
            Value::from(ts.as_str()),
        );
    }
}

/// Build the [`SheetPatch`] for `q` under stamp `opts`, where `now` is the
/// write layer's current wall-clock time (used for [`Stamp::Now`]). Pure, so
/// the three stamp modes are unit-testable without a client.
pub(crate) fn build_patch(q: Quint, opts: &WriteOpts, now: &str) -> SheetPatch {
    match &opts.stamp {
        Stamp::Now => sheet_patch(q).with_updated_at(now),
        Stamp::At(ts) => sheet_patch(q).with_updated_at(ts.as_str()),
        Stamp::None => sheet_patch(q),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tass_quint::SyncDecision;

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
        apply_quint(mage, &sheet_patch(Quint::from_millis(1_500)));
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
            apply_quint(mage, &sheet_patch(Quint::from_points(2)));
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
            apply_quint(mage, &sheet_patch(Quint::from_points(4)));
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

    #[test]
    fn apply_writes_updated_at_when_stamped() {
        let mut v = envelope(json!({ "quintessence": 0 }));
        let mage = mage_block_mut(&mut v).unwrap();
        let patch = sheet_patch(Quint::from_points(1)).with_updated_at("2026-06-29T21:00:00Z");
        apply_quint(mage, &patch);
        let mage = mage_block(&v).unwrap();
        assert_eq!(
            mage.get("milliQuintessenceUpdatedAt").and_then(Value::as_str),
            Some("2026-06-29T21:00:00Z")
        );
    }

    #[test]
    fn apply_omits_updated_at_when_unstamped() {
        let mut v = envelope(json!({ "quintessence": 0 }));
        let mage = mage_block_mut(&mut v).unwrap();
        apply_quint(mage, &sheet_patch(Quint::from_points(1)));
        let mage = mage_block(&v).unwrap();
        assert!(mage.get("milliQuintessenceUpdatedAt").is_none());
    }

    #[test]
    fn build_patch_default_stamps_now() {
        let patch = build_patch(Quint::from_points(2), &WriteOpts::default(), "2026-06-30T00:00:00Z");
        assert_eq!(patch.milli_quintessence, 2_000);
        assert_eq!(patch.quintessence, 2);
        assert_eq!(
            patch.milli_quintessence_updated_at.as_deref(),
            Some("2026-06-30T00:00:00Z")
        );
    }

    #[test]
    fn build_patch_at_uses_caller_time() {
        let opts = WriteOpts::default().at("1999-01-01T00:00:00Z");
        let patch = build_patch(Quint::from_points(1), &opts, "2026-06-30T00:00:00Z");
        // caller time wins over "now"
        assert_eq!(
            patch.milli_quintessence_updated_at.as_deref(),
            Some("1999-01-01T00:00:00Z")
        );
    }

    #[test]
    fn build_patch_unstamped_omits_timestamp() {
        let opts = WriteOpts::default().unstamped();
        let patch = build_patch(Quint::from_points(1), &opts, "2026-06-30T00:00:00Z");
        assert!(patch.milli_quintessence_updated_at.is_none());
    }

    // — extract_fields + coherence on the read path ———————————

    #[test]
    fn extract_fields_pulls_all_four_fields_from_envelope() {
        let v = json!({
            "system": "mage",
            "data": { "milliQuintessence": 1500, "quintessence": 1, "milliQuintessenceUpdatedAt": "2026-06-29T10:00:00Z" },
            "$type": "actor.rpg.stats",
            "updatedAt": "2026-06-29T10:00:00Z"
        });
        let f = extract_fields(&v).unwrap();
        assert_eq!(f, SheetFields {
            milli_quintessence: Some(1500),
            quintessence: Some(1),
            milli_quintessence_updated_at: Some("2026-06-29T10:00:00Z"),
            updated_at: Some("2026-06-29T10:00:00Z"),
        });
    }

    #[test]
    fn extract_fields_returns_none_without_mage_block() {
        let v = json!({ "system": "vampire", "data": {} });
        assert!(extract_fields(&v).is_none());
    }

    #[test]
    fn extract_fields_preserves_absent_fields_as_none() {
        let v = envelope(json!({ "quintessence": 7 }));
        let f = extract_fields(&v).unwrap();
        assert_eq!(f.milli_quintessence, None);
        assert_eq!(f.quintessence, Some(7));
        assert_eq!(f.milli_quintessence_updated_at, None);
        assert_eq!(f.updated_at, None);
    }

    #[test]
    fn classify_in_sync_on_clean_envelope() {
        let v = json!({
            "system": "mage",
            "data": { "milliQuintessence": 1500, "quintessence": 1, "milliQuintessenceUpdatedAt": "2026-06-29T10:00:00Z" },
            "$type": "actor.rpg.stats",
            "updatedAt": "2026-06-29T10:00:00Z"
        });
        let f = extract_fields(&v).unwrap();
        assert_eq!(MilliIsTruthCoherence.classify(&f), SyncDecision::InSync);
    }

    #[test]
    fn classify_refresh_floor_on_drifted_envelope() {
        // milli=1500 (floor 1) but quintessence=9 — drifted
        let v = envelope(json!({ "milliQuintessence": 1500, "quintessence": 9 }));
        let f = extract_fields(&v).unwrap();
        assert_eq!(MilliIsTruthCoherence.classify(&f), SyncDecision::RefreshFloor);
    }

    #[test]
    fn classify_refresh_floor_on_timestamp_drift() {
        // floor agrees, but record updatedAt advanced past the milli stamp
        let v = json!({
            "system": "mage",
            "data": { "milliQuintessence": 1500, "quintessence": 1, "milliQuintessenceUpdatedAt": "2026-06-29T10:00:00Z" },
            "$type": "actor.rpg.stats",
            "updatedAt": "2026-06-30T10:00:00Z"
        });
        let f = extract_fields(&v).unwrap();
        assert_eq!(MilliIsTruthCoherence.classify(&f), SyncDecision::RefreshFloor);
    }

    #[test]
    fn classify_in_sync_on_legacy_only_sheet() {
        let v = envelope(json!({ "quintessence": 7 }));
        let f = extract_fields(&v).unwrap();
        assert_eq!(MilliIsTruthCoherence.classify(&f), SyncDecision::InSync);
    }

#[test]
    fn coherent_quint_on_envelope_returns_milli_value_for_refresh_floor() {
        let v = envelope(json!({ "milliQuintessence": 1500, "quintessence": 9 }));
        let f = extract_fields(&v).unwrap();
        let d = MilliIsTruthCoherence.classify(&f);
        assert_eq!(coherent_quint(&f, d), Some(Quint::from_millis(1500)));
    }

    // — with_coherence seam: a custom policy routes through read_report —

    /// A custom coherence that always says "drift, hydrate from legacy" —
    /// stands in for a future QuintessenceIsTruth impl. Proves the seam is
    /// exercisable end-to-end (construct -> classify -> coherent_quint).
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    struct AlwaysHydrateFromLegacy;

    impl Coherence for AlwaysHydrateFromLegacy {
        fn classify(&self, fields: &SheetFields<'_>) -> SyncDecision {
            if fields.quintessence.is_some() {
                SyncDecision::HydrateMilli
            } else {
                SyncDecision::InSync
            }
        }
    }

    #[test]
    fn with_coherence_swaps_the_active_policy() {
        // a sheet where milli=1500 and quintessence=9 — under the default
        // (MilliIsTruth) this is RefreshFloor and coherent_quint returns 1500;
        // under AlwaysHydrateFromLegacy it's HydrateMilli and coherent_quint
        // returns 9000 (hydrated from the legacy integer).
        let v = envelope(json!({ "milliQuintessence": 1500, "quintessence": 9 }));
        let f = extract_fields(&v).unwrap();

        let default_decision = MilliIsTruthCoherence.classify(&f);
        assert_eq!(default_decision, SyncDecision::RefreshFloor);
        assert_eq!(coherent_quint(&f, default_decision), Some(Quint::from_millis(1500)));

        let custom_decision = AlwaysHydrateFromLegacy.classify(&f);
        assert_eq!(custom_decision, SyncDecision::HydrateMilli);
        assert_eq!(coherent_quint(&f, custom_decision), Some(Quint::from_points(9)));
    }
}
