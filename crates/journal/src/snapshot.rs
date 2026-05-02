//! `RocksDbSnapshot` — keyed `save<V>` / `load<V>` over RocksDB.
//!
//! Per spec §5.2 (public API surface), §5.6 (success-only counter rule),
//! §B.2.5 (`save`), §B.2.6 (`load`), §B.2.7 (`last_sequence` /
//! `set_last_sequence` — added in Task 10), §X.4 (bincode 1.x serde
//! adapter), §X.12 (0 is NOT a sentinel for `last_sequence`).
//!
//! Task 9 lands `RocksDbSnapshot` (struct + open + save + load + stats),
//! `SnapshotStats`, the reserved-key prefix constants, and the 5 S-tests
//! (S-1, S-2, S-5, S-6, S-7) per spec §B.5.3. The remaining 2 S-tests
//! (S-3, S-4) and the `last_sequence` / `set_last_sequence` methods land
//! in Task 10. The cross-module I-1 integration test lands in Task 11.

// `loaded_total` is populated by `load` in this task but not yet read
// outside the in-tests `stats()` call; `LAST_SEQUENCE_KEY` is defined here
// for use by Task 10's last_sequence / set_last_sequence. The module-level
// annotation matches the plan v0.3 dead-code policy and is removed in Task
// 12 once every constant and method has at least one non-test caller.
#![allow(dead_code)]

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use rocksdb::DB;

use crate::error::JournalError;

/// Reserved snapshot key prefix per spec §5.2 + §B.2.5. Any user-supplied
/// key starting with this prefix is rejected at the `save` / `load`
/// boundary BEFORE any RocksDB call. The leading null byte makes
/// accidental collision with human-readable user keys effectively
/// impossible.
pub(crate) const RESERVED_KEY_PREFIX: &[u8] = b"\0rust_lmax_mev:snapshot:";

/// Reserved key holding the `last_sequence` watermark per spec §B.2.7.
/// Wired by Task 10; declared here so Task 9's reserved-prefix check has
/// a stable target.
pub(crate) const LAST_SEQUENCE_KEY: &[u8] = b"\0rust_lmax_mev:snapshot:last_sequence";

/// Keyed save/load over RocksDB for snapshot data (per spec §5.2).
///
/// Phase 1 wraps a single `Arc<rocksdb::DB>` shared with downstream
/// consumers (the `Arc` is preserved for Task 17 smoke harness sharing).
/// `save` / `load` use bincode 1.x serde-adapter encoding per spec §X.4
/// and ADR-004 (the latter wording was fixed in commit `f9e42fe`).
///
/// `#[derive(Debug)]` is included for ergonomic test diagnostics; the
/// `rocksdb::DB` field's `Debug` impl is inherited via the `Arc`.
#[derive(Debug)]
pub struct RocksDbSnapshot {
    db: Arc<DB>,
    saved_total: AtomicU64,
    loaded_total: AtomicU64,
}

/// In-process counter snapshot returned from `RocksDbSnapshot::stats(&self)`.
///
/// Mirrors the `metrics::counter!` emissions documented in spec §B.3:
/// `event_snapshot_saved_total` and `event_snapshot_loaded_total`. No
/// gauge surface per CLAUDE.md ("Journal and snapshot emit counters
/// only — no gauges").
///
/// `#[non_exhaustive]` per spec §5.2 so Phase 2 may add fields additively.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotStats {
    pub saved_total: u64,
    pub loaded_total: u64,
}

impl RocksDbSnapshot {
    /// Opens (or creates) a RocksDB instance at `path` per spec §B.2.5.
    ///
    /// Uses default `Options::default().create_if_missing(true)` so a
    /// missing directory is created on first open. Errors from `DB::open`
    /// surface as `JournalError::RocksDb(...)` via the `#[from]` impl.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, JournalError> {
        let mut opts = rocksdb::Options::default();
        opts.create_if_missing(true);
        let db = DB::open(&opts, path)?;
        Ok(Self {
            db: Arc::new(db),
            saved_total: AtomicU64::new(0),
            loaded_total: AtomicU64::new(0),
        })
    }

    /// Saves `value` under `key`. Per spec §B.2.5:
    /// 1. Reject if `key` starts with `RESERVED_KEY_PREFIX` →
    ///    `Err(JournalError::ReservedKey(key.to_vec()))` BEFORE any
    ///    RocksDB call.
    /// 2. bincode-serialize `value` via the 1.x serde adapter (per spec
    ///    §X.4); on encode failure → `Err(BincodeSerialize(...))` (NO
    ///    `#[from]` per spec §X.4 — explicit `.map_err`).
    /// 3. `db.put(key, encoded)`; on RocksDB error → `Err(RocksDb(...))`.
    /// 4. Success-only: `saved_total += 1` (atomic + metrics).
    pub fn save<V>(&self, key: &[u8], value: &V) -> Result<(), JournalError>
    where
        V: serde::Serialize,
    {
        if key.starts_with(RESERVED_KEY_PREFIX) {
            return Err(JournalError::ReservedKey(key.to_vec()));
        }
        let encoded = bincode::serialize(value).map_err(JournalError::BincodeSerialize)?;
        self.db.put(key, encoded)?;
        self.saved_total.fetch_add(1, Ordering::Relaxed);
        metrics::counter!("event_snapshot_saved_total").increment(1);
        Ok(())
    }

    /// Loads the value at `key`. Per spec §B.2.6:
    /// 1. Reject reserved-prefix keys per the same boundary as `save`.
    /// 2. `db.get(key)`; on RocksDB error → `Err(RocksDb(...))`.
    /// 3. If `None`: return `Ok(None)` — NO counter increment per spec
    ///    §5.6 (success-only is on a successful decode-of-present, not
    ///    on absence).
    /// 4. If `Some(bytes)`: bincode-deserialize via 1.x serde adapter; on
    ///    decode failure → `Err(BincodeDeserialize(...))` (NO `#[from]`
    ///    per spec §X.4); on success increment `loaded_total` and return
    ///    `Ok(Some(value))`.
    pub fn load<V>(&self, key: &[u8]) -> Result<Option<V>, JournalError>
    where
        V: serde::de::DeserializeOwned,
    {
        if key.starts_with(RESERVED_KEY_PREFIX) {
            return Err(JournalError::ReservedKey(key.to_vec()));
        }
        let bytes = self.db.get(key)?;
        match bytes {
            None => Ok(None),
            Some(raw) => {
                let value: V =
                    bincode::deserialize(&raw).map_err(JournalError::BincodeDeserialize)?;
                self.loaded_total.fetch_add(1, Ordering::Relaxed);
                metrics::counter!("event_snapshot_loaded_total").increment(1);
                Ok(Some(value))
            }
        }
    }

    /// Returns an in-process snapshot of the two counter atomics per spec
    /// §B.3. Reads via `Ordering::Relaxed` because the consumer (operator
    /// dashboard or test) tolerates eventual consistency.
    pub fn stats(&self) -> SnapshotStats {
        SnapshotStats {
            saved_total: self.saved_total.load(Ordering::Relaxed),
            loaded_total: self.loaded_total.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_lmax_mev_types::SmokeTestPayload;

    fn sample_payload(nonce: u64) -> SmokeTestPayload {
        SmokeTestPayload {
            nonce,
            data: [0xCD; 32],
        }
    }

    /// S-1 (TDD red→green; Task 9): save then load returns the equal value.
    #[test]
    fn snapshot_save_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let snap = RocksDbSnapshot::open(dir.path().join("rocks")).unwrap();
        let val = sample_payload(7);
        snap.save(b"key1", &val).unwrap();
        let loaded: Option<SmokeTestPayload> = snap.load(b"key1").unwrap();
        assert_eq!(loaded, Some(val));

        let stats = snap.stats();
        assert_eq!(stats.saved_total, 1);
        assert_eq!(stats.loaded_total, 1);
    }

    /// S-2 (test-first; Task 9): `load` of an absent key returns
    /// `Ok(None)` and does NOT increment `loaded_total` per spec §5.6 +
    /// §B.2.6 (success-only-on-decode-of-present).
    #[test]
    fn snapshot_load_absent_key_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let snap = RocksDbSnapshot::open(dir.path().join("rocks")).unwrap();
        let loaded: Option<SmokeTestPayload> = snap.load(b"missing").unwrap();
        assert_eq!(loaded, None);
        let stats = snap.stats();
        assert_eq!(stats.saved_total, 0);
        assert_eq!(
            stats.loaded_total, 0,
            "absent-key load must NOT increment loaded_total per spec §5.6"
        );
    }

    /// S-5 (test-first; Task 9): `save` with reserved-prefix key is
    /// rejected BEFORE any RocksDB write. The DB instance remains
    /// untouched (verifiable via subsequent `load` of any non-reserved
    /// key returning `Ok(None)`).
    #[test]
    fn snapshot_save_under_reserved_prefix_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let snap = RocksDbSnapshot::open(dir.path().join("rocks")).unwrap();
        let val = sample_payload(7);

        let mut reserved_key = RESERVED_KEY_PREFIX.to_vec();
        reserved_key.extend_from_slice(b"foo");

        let err = snap.save(&reserved_key, &val).unwrap_err();
        match err {
            JournalError::ReservedKey(returned) => {
                assert_eq!(returned, reserved_key);
            }
            other => panic!("expected ReservedKey, got {other:?}"),
        }

        // No counter bump — rejection is BEFORE the put per spec §B.2.5.
        let stats = snap.stats();
        assert_eq!(stats.saved_total, 0);

        // RocksDB untouched: a fresh load of the same reserved key would
        // also reject (testing the boundary symmetrically), but the
        // important invariant is no put landed. Probe a non-reserved key
        // to confirm the DB is empty.
        let probe: Option<SmokeTestPayload> = snap.load(b"any-other-key").unwrap();
        assert_eq!(probe, None);
    }

    /// S-6 (test-first; Task 9): `load` with reserved-prefix key is
    /// rejected BEFORE any RocksDB read.
    #[test]
    fn snapshot_load_under_reserved_prefix_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let snap = RocksDbSnapshot::open(dir.path().join("rocks")).unwrap();

        let mut reserved_key = RESERVED_KEY_PREFIX.to_vec();
        reserved_key.extend_from_slice(b"foo");

        let result: Result<Option<SmokeTestPayload>, _> = snap.load(&reserved_key);
        let err = result.unwrap_err();
        match err {
            JournalError::ReservedKey(returned) => {
                assert_eq!(returned, reserved_key);
            }
            other => panic!("expected ReservedKey, got {other:?}"),
        }
        assert_eq!(snap.stats().loaded_total, 0);
    }

    /// S-7 (test-first; Task 9): aggregate counter behavior across
    /// save+save+load(absent)+load(present): final stats shows
    /// `saved_total == 2`, `loaded_total == 1` per spec §B.3.
    #[test]
    fn snapshot_save_load_increment_counters_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let snap = RocksDbSnapshot::open(dir.path().join("rocks")).unwrap();

        snap.save(b"key1", &sample_payload(1)).unwrap();
        snap.save(b"key2", &sample_payload(2)).unwrap();
        let absent: Option<SmokeTestPayload> = snap.load(b"missing").unwrap();
        assert_eq!(absent, None);
        let present: Option<SmokeTestPayload> = snap.load(b"key1").unwrap();
        assert_eq!(present, Some(sample_payload(1)));

        let stats = snap.stats();
        assert_eq!(stats.saved_total, 2, "two successful saves");
        assert_eq!(
            stats.loaded_total, 1,
            "only the present-load increments; absent-load does not"
        );
    }
}
