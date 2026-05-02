//! B-3 — ADR-008 check 7: `RocksDbSnapshot::save` / `load` round-trip on
//! a synthetic 256-byte payload (per Batch C execution note v0.3 Risk
//! Decision 6).
//!
//! The journal crate's S-1..S-7 unit tests already exercise the snapshot
//! API exhaustively; this smoke test exists for the CI-level "is RocksDB
//! still working?" gate.

use rust_lmax_mev_journal::RocksDbSnapshot;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SmokePayload {
    label: String,
    bytes: Vec<u8>,
}

#[test]
fn rocksdb_snapshot_save_load_round_trip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let snapshot = RocksDbSnapshot::open(dir.path().join("snapshot")).expect("open snapshot");

    let original = SmokePayload {
        label: "smoke".to_string(),
        bytes: (0..=255u8).collect(), // 256-byte deterministic payload
    };
    let key: &[u8] = b"smoke-key";

    snapshot.save(key, &original).expect("save");

    let loaded: Option<SmokePayload> = snapshot.load(key).expect("load");
    assert_eq!(
        loaded.as_ref(),
        Some(&original),
        "loaded value must equal saved value byte-for-byte"
    );

    let stats = snapshot.stats();
    assert_eq!(stats.saved_total, 1, "exactly one save");
    assert_eq!(stats.loaded_total, 1, "exactly one successful load");
}
