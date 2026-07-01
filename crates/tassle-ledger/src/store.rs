use serde::{Deserialize, Serialize};

/// Source ingestion order (e.g. a Hydrant event id).
///
/// Drives ingestion, idempotency, and exactly-once local processing. This is
/// deliberately distinct from a record's `createdAt` order, which drives the
/// game-state fold — see the "two independent orderings" note in
/// `doc/ledger.md`.
pub type Cursor = u64;

pub type Result<T> = std::result::Result<T, LedgerError>;

/// Errors from a [`LedgerStore`].
///
/// Hand-rolled (no `thiserror`) to keep the dependency surface minimal.
#[derive(Debug)]
pub enum LedgerError {
    /// Opening a DID's database failed.
    Open { did: String, source: fjall::Error },
    /// A fjall operation failed.
    Fjall(fjall::Error),
    /// (De)serializing a stored event or snapshot failed.
    Serde(serde_json::Error),
    /// A filesystem operation on the data dir failed.
    Io(std::io::Error),
    /// The open-handle pool failed to produce a handle (init error, surfaced as
    /// a string because moka wraps the original error in an `Arc`).
    Pool(String),
}

impl std::fmt::Display for LedgerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LedgerError::Open { did, source } => {
                write!(f, "opening ledger store for {did}: {source}")
            }
            LedgerError::Fjall(e) => write!(f, "fjall error: {e}"),
            LedgerError::Serde(e) => write!(f, "serialization error: {e}"),
            LedgerError::Io(e) => write!(f, "ledger store io error: {e}"),
            LedgerError::Pool(e) => write!(f, "ledger store pool error: {e}"),
        }
    }
}

impl std::error::Error for LedgerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LedgerError::Open { source, .. } => Some(source),
            LedgerError::Fjall(e) => Some(e),
            LedgerError::Serde(e) => Some(e),
            LedgerError::Io(e) => Some(e),
            LedgerError::Pool(_) => None,
        }
    }
}

impl From<fjall::Error> for LedgerError {
    fn from(e: fjall::Error) -> Self {
        LedgerError::Fjall(e)
    }
}

impl From<serde_json::Error> for LedgerError {
    fn from(e: serde_json::Error) -> Self {
        LedgerError::Serde(e)
    }
}

impl From<std::io::Error> for LedgerError {
    fn from(e: std::io::Error) -> Self {
        LedgerError::Io(e)
    }
}

/// One normalized record event in a DID's ledger log.
///
/// This is the storage envelope and is deliberately lexicon-agnostic: the fold
/// (see ticket `tass-ledger-fold`) is what interprets `collection`/`record`
/// into Node/Tass balances. Keeping the store dumb lets the fold and the
/// lexicons evolve without a storage migration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoredEvent {
    /// Source ingestion order; also the primary key in the event log.
    pub cursor: Cursor,
    /// AT-URI of the record (`at://did/collection/rkey`).
    pub uri: String,
    /// Record CID, when known.
    pub cid: Option<String>,
    /// Collection NSID, e.g. `at.telluri.act.meditate`.
    pub collection: String,
    /// Record key.
    pub rkey: String,
    /// `create` | `update` | `delete`.
    pub action: String,
    /// Record `createdAt`, when present and parseable.
    pub created_at: Option<String>,
    /// Raw/normalized record body. `None` for deletes.
    pub record: Option<serde_json::Value>,
}

/// Durable, per-DID storage for the ledger fold.
///
/// Implementations key everything by actor DID. A DID's state is its ordered
/// event log plus a materialized balance snapshot and the last-folded cursor.
pub trait LedgerStore: Send + Sync {
    /// Append events to a DID's log (durable). Use [`commit_fold`] when the
    /// snapshot and cursor advance together with the events.
    ///
    /// [`commit_fold`]: LedgerStore::commit_fold
    fn append_events(&self, did: &str, events: &[StoredEvent]) -> Result<()>;

    /// Read a DID's full event log in cursor (ingestion) order.
    fn read_events(&self, did: &str) -> Result<Vec<StoredEvent>>;

    /// The materialized balance snapshot for a DID, if one has been written.
    /// Opaque bytes: the fold owns the snapshot encoding.
    fn snapshot(&self, did: &str) -> Result<Option<Vec<u8>>>;

    /// The last cursor folded into a DID's snapshot.
    fn cursor(&self, did: &str) -> Result<Option<Cursor>>;

    /// Atomic, durable fold commit: append `events`, replace the snapshot when
    /// `snapshot` is `Some`, and advance the stored cursor — all in one batch,
    /// so a crash mid-fold can never leave the snapshot ahead of the log or the
    /// cursor ahead of the snapshot.
    fn commit_fold(
        &self,
        did: &str,
        events: &[StoredEvent],
        snapshot: Option<&[u8]>,
        cursor: Cursor,
    ) -> Result<()>;

    /// Drop all state for a DID (close its handle and remove its data dir).
    fn forget(&self, did: &str) -> Result<()>;
}
