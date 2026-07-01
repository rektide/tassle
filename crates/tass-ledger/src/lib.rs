//! Per-DID ledger store for Tassle.
//!
//! [`LedgerStore`] is the storage seam the ledger fold writes through, so the
//! physical layout stays swappable: fjall-per-DID today, a shared keyspace or a
//! relational backend (turso/libsql) later, without touching the fold. See
//! `doc/ledger.md` ("Storage Layout") for the layout rationale and tradeoffs.
//!
//! [`FjallLedgerStore`] is the first implementation: one fjall database per
//! actor DID under a data root, with a bounded pool of open handles so a large
//! network never keeps thousands of journals, file descriptors, and compaction
//! workers live at once.

mod fjall_store;
mod store;

pub use fjall_store::{FjallLedgerStore, OpenPoolConfig};
pub use store::{Cursor, LedgerError, LedgerStore, Result, StoredEvent};
