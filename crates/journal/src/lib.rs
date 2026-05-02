//! Phase 1 append-only journal + RocksDB snapshot for the LMAX-style MEV engine.
//!
//! This crate ships two primitives that pair with the `crates/event-bus`
//! consumer:
//!
//! - [`FileJournal<T>`] — append-only file-backed log of `EventEnvelope<T>`
//!   records framed as `[u32 length LE][payload][u32 CRC32 LE]` after an
//!   8-byte file header. See spec sections 4.5 and B.1 for the byte layout.
//! - [`RocksDbSnapshot`] — keyed `save<V>` / `load<V>` over RocksDB, plus a
//!   reserved `last_sequence` watermark used by replay to skip entries the
//!   snapshot already covers. See spec sections 5.2 and B.2.5-B.2.7.
//!
//! Both primitives are synchronous and blocking by design. Non-blocking
//! pipeline behavior is satisfied at app-wiring time (Task 16) by running
//! the journal as a dedicated event-bus consumer thread; see spec section
//! 4.6 for the ADR-003 reconciliation.
//!
//! Module layout (split-modules-from-start per spec section X.11):
//!
//! - [`error`] — [`error::JournalError`] enum (15 variants per spec section B.4)
//! - [`frame`] — frame encode/decode helpers and file-header constants (spec section B.1)
//! - [`journal`] — [`journal::FileJournal`] and [`journal::JournalStats`]
//! - [`snapshot`] — [`snapshot::RocksDbSnapshot`] and [`snapshot::SnapshotStats`]
//!
//! The Gate 3 scaffold ships these modules empty; subsequent Gate 5
//! implementation commits fill each module per the spec section B / C plan.

pub mod error;
pub mod frame;
pub mod journal;
pub mod snapshot;

pub use error::JournalError;
pub use journal::{FileJournal, JournalStats};
