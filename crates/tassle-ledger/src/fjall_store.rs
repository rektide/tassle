use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use fjall::{Config, Database, Keyspace, KeyspaceCreateOptions, PersistMode};
use moka::sync::Cache;

use crate::store::{Cursor, LedgerError, LedgerStore, Result, StoredEvent};

/// Event log keyspace: key = big-endian `Cursor`, value = JSON `StoredEvent`.
const EVENTS: &str = "events_by_cursor";
/// Meta keyspace: holds the snapshot blob and the last-folded cursor.
const META: &str = "meta";
const META_SNAPSHOT: &[u8] = b"snapshot";
const META_CURSOR: &[u8] = b"cursor";

/// One actor DID's fjall database: the ordered event log plus a meta keyspace
/// holding the materialized snapshot and last-folded cursor.
struct DidDb {
    db: Database,
    events: Keyspace,
    meta: Keyspace,
}

impl DidDb {
    fn open(root: &Path, did: &str) -> std::result::Result<Self, LedgerError> {
        let dir = root.join(did_dir(did));
        std::fs::create_dir_all(&dir)?;
        let db = Database::open(Config::new(&dir)).map_err(|source| LedgerError::Open {
            did: did.to_owned(),
            source,
        })?;
        let events = db.keyspace(EVENTS, KeyspaceCreateOptions::default)?;
        let meta = db.keyspace(META, KeyspaceCreateOptions::default)?;
        Ok(Self { db, events, meta })
    }

    fn flush(&self) -> Result<()> {
        self.db.persist(PersistMode::SyncAll)?;
        Ok(())
    }
}

/// Tuning for the bounded pool of open per-DID databases.
#[derive(Clone, Debug)]
pub struct OpenPoolConfig {
    /// Maximum number of per-DID databases kept open at once. Beyond this,
    /// least-recently-used databases are flushed and closed.
    pub max_open: u64,
    /// Close a DID's database after this long without a touch.
    pub idle_timeout: Duration,
}

impl Default for OpenPoolConfig {
    fn default() -> Self {
        Self {
            max_open: 256,
            idle_timeout: Duration::from_secs(300),
        }
    }
}

/// Per-DID fjall ledger store.
///
/// One fjall database per actor DID under `root` (`<root>/<did-dir>/`), so each
/// actor can be locked, snapshotted, backed up, deleted, or rebuilt
/// independently — the per-repo sovereignty model in miniature. That cuts
/// against fjall's "one database, many keyspaces" grain, so the cost is bounded
/// by a [`moka`] pool: databases are opened lazily on touch and the
/// least-recently-used ones are flushed and dropped once `max_open` or
/// `idle_timeout` is hit.
///
/// Concurrency note: [`Cache::try_get_with_by_ref`] serializes initialization
/// per key, so concurrent first-touches of the same DID share one database
/// rather than racing two opens (which would collide on fjall's per-dir lock).
/// A caller should fetch a DID's handle once per operation and reuse it; the
/// only remaining open-collision window is re-opening a DID while an evicted
/// handle is still held live elsewhere, which the single-threaded listener fold
/// does not hit. See ticket `tass-ledgerstore`.
pub struct FjallLedgerStore {
    root: PathBuf,
    open: Cache<String, Arc<DidDb>>,
}

impl FjallLedgerStore {
    /// Open (creating if needed) a store rooted at `root`.
    pub fn new(root: impl Into<PathBuf>, pool: OpenPoolConfig) -> Result<Self> {
        let root = root.into();
        std::fs::create_dir_all(&root)?;
        let open = Cache::builder()
            .max_capacity(pool.max_open)
            .time_to_idle(pool.idle_timeout)
            .eviction_listener(|did: Arc<String>, db: Arc<DidDb>, _cause| {
                if let Err(e) = db.flush() {
                    eprintln!("tassle-ledger: flush on evict failed for {did}: {e}");
                }
            })
            .build();
        Ok(Self { root, open })
    }

    /// Fetch (lazily opening) the database handle for `did`.
    fn handle(&self, did: &str) -> Result<Arc<DidDb>> {
        let root = self.root.clone();
        self.open
            .try_get_with_by_ref(did, || DidDb::open(&root, did).map(Arc::new))
            .map_err(|e: Arc<LedgerError>| LedgerError::Pool(e.to_string()))
    }
}

impl LedgerStore for FjallLedgerStore {
    fn append_events(&self, did: &str, events: &[StoredEvent]) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }
        let h = self.handle(did)?;
        let mut batch = h.db.batch();
        for ev in events {
            batch.insert(&h.events, ev.cursor.to_be_bytes(), serde_json::to_vec(ev)?);
        }
        batch.commit()?;
        h.db.persist(PersistMode::SyncAll)?;
        Ok(())
    }

    fn read_events(&self, did: &str) -> Result<Vec<StoredEvent>> {
        let h = self.handle(did)?;
        let mut out = Vec::new();
        for guard in h.events.iter() {
            let value = guard.value()?;
            out.push(serde_json::from_slice(&value)?);
        }
        Ok(out)
    }

    fn snapshot(&self, did: &str) -> Result<Option<Vec<u8>>> {
        let h = self.handle(did)?;
        Ok(h.meta.get(META_SNAPSHOT)?.map(|v| v.to_vec()))
    }

    fn cursor(&self, did: &str) -> Result<Option<Cursor>> {
        let h = self.handle(did)?;
        let raw = h.meta.get(META_CURSOR)?;
        Ok(raw.and_then(|v| <[u8; 8]>::try_from(v.as_ref()).ok().map(u64::from_be_bytes)))
    }

    fn commit_fold(
        &self,
        did: &str,
        events: &[StoredEvent],
        snapshot: Option<&[u8]>,
        cursor: Cursor,
    ) -> Result<()> {
        let h = self.handle(did)?;
        let mut batch = h.db.batch();
        for ev in events {
            batch.insert(&h.events, ev.cursor.to_be_bytes(), serde_json::to_vec(ev)?);
        }
        if let Some(snap) = snapshot {
            batch.insert(&h.meta, META_SNAPSHOT, snap);
        }
        batch.insert(&h.meta, META_CURSOR, cursor.to_be_bytes());
        batch.commit()?;
        h.db.persist(PersistMode::SyncAll)?;
        Ok(())
    }

    fn forget(&self, did: &str) -> Result<()> {
        // Drop the cached handle and let moka run the eviction (flush) before we
        // remove the directory out from under any live database.
        self.open.invalidate(did);
        self.open.run_pending_tasks();
        let dir = self.root.join(did_dir(did));
        if dir.exists() {
            std::fs::remove_dir_all(dir)?;
        }
        Ok(())
    }
}

/// Map a DID to a filesystem-safe directory name.
///
/// v1: reserved characters become `_`. This is legible (good for `btrfs
/// subvolume` names, see ticket `tass-btrfs-subvol`) but not provably
/// collision-free for exotic `did:web` values that differ only in reserved
/// characters. Revisit with a reversible encoding or appended hash if real
/// collisions appear.
fn did_dir(did: &str) -> String {
    did.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' => c,
            _ => '_',
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    fn store() -> (tempfile::TempDir, FjallLedgerStore) {
        let dir = tempdir().unwrap();
        let store = FjallLedgerStore::new(dir.path(), OpenPoolConfig::default()).unwrap();
        (dir, store)
    }

    fn ev(cursor: Cursor, collection: &str) -> StoredEvent {
        StoredEvent {
            cursor,
            uri: format!("at://did:plc:alice/{collection}/rk{cursor}"),
            cid: Some(format!("bafy{cursor}")),
            collection: collection.to_owned(),
            rkey: format!("rk{cursor}"),
            action: "create".to_owned(),
            created_at: Some("2026-06-27T00:00:00Z".to_owned()),
            record: Some(json!({ "n": cursor })),
        }
    }

    #[test]
    fn append_and_read_in_cursor_order() {
        let (_d, store) = store();
        let did = "did:plc:alice";
        // insert out of order; the BE-cursor key must sort them back.
        store
            .append_events(did, &[ev(2, "com.superbfowle.tass.meditate")])
            .unwrap();
        store
            .append_events(did, &[ev(1, "com.superbfowle.tass.node")])
            .unwrap();
        let got = store.read_events(did).unwrap();
        assert_eq!(got.iter().map(|e| e.cursor).collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(
            got,
            vec![
                ev(1, "com.superbfowle.tass.node"),
                ev(2, "com.superbfowle.tass.meditate")
            ]
        );
    }

    #[test]
    fn commit_fold_is_atomic_and_advances_cursor() {
        let (_d, store) = store();
        let did = "did:plc:bob";
        assert_eq!(store.cursor(did).unwrap(), None);
        assert_eq!(store.snapshot(did).unwrap(), None);

        let events = [
            ev(10, "com.superbfowle.tass.node"),
            ev(11, "com.superbfowle.tass.tassilize"),
        ];
        store
            .commit_fold(did, &events, Some(b"snapshot-v1"), 11)
            .unwrap();

        assert_eq!(store.cursor(did).unwrap(), Some(11));
        assert_eq!(
            store.snapshot(did).unwrap().as_deref(),
            Some(&b"snapshot-v1"[..])
        );
        assert_eq!(store.read_events(did).unwrap().len(), 2);
    }

    #[test]
    fn snapshot_recomputable_from_log_after_reopen() {
        let dir = tempdir().unwrap();
        let did = "did:plc:carol";
        {
            let store = FjallLedgerStore::new(dir.path(), OpenPoolConfig::default()).unwrap();
            store
                .commit_fold(did, &[ev(1, "com.superbfowle.tass.node")], Some(b"snap"), 1)
                .unwrap();
        }
        // reopen: state must survive a fresh process/store.
        let store = FjallLedgerStore::new(dir.path(), OpenPoolConfig::default()).unwrap();
        assert_eq!(store.cursor(did).unwrap(), Some(1));
        assert_eq!(store.read_events(did).unwrap().len(), 1);
    }

    #[test]
    fn forget_removes_did_state() {
        let (_d, store) = store();
        let did = "did:plc:dave";
        store
            .commit_fold(did, &[ev(1, "com.superbfowle.tass.node")], Some(b"s"), 1)
            .unwrap();
        store.forget(did).unwrap();
        assert_eq!(store.cursor(did).unwrap(), None);
        assert_eq!(store.read_events(did).unwrap().len(), 0);
    }
}
