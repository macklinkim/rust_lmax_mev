//! `RocksDbSnapshot` — keyed save/load primitive over RocksDB.
//!
//! Gate 3 scaffold placeholder. The `RocksDbSnapshot` struct, its
//! `open` / `save<V>` / `load<V>` / `last_sequence` / `set_last_sequence` /
//! `stats` impls, and `SnapshotStats` land during Gate 5 implementation per
//! spec sections 5.2 and B.2.5-B.2.7. The reserved-key prefix
//! `b"\0rust_lmax_mev:snapshot:"` (and the derived `LAST_SEQUENCE_KEY`) and
//! the bincode 1.x serde adapter encoding are enforced per spec sections X.4
//! and 5.2.
//!
//! The `rocksdb = { workspace = true }` dependency is staged-deferred per
//! spec v0.7 amendment; it is added to `crates/journal/Cargo.toml` at the
//! time this module's `RocksDbSnapshot` impl lands during Gate 5.
