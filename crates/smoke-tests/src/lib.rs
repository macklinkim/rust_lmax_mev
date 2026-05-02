//! Phase 1 integration smoke tests for the LMAX-style MEV engine.
//!
//! Per Batch C execution note (`docs/superpowers/plans/2026-05-02-phase-1-
//! batch-c-tests-ci-execution.md`). All tests live under `tests/` so the
//! crate's lib target is intentionally empty — cargo requires a lib or
//! bin target for `tests/` integration tests to compile.
//!
//! - `tests/bus_smoke.rs` — ADR-008 check 5 (100k events with deterministic
//!   backpressure verification, non-deadlocking cleanup).
//! - `tests/journal_round_trip.rs` — ADR-008 check 6 (1024-event write +
//!   reread, bit-exact equality).
//! - `tests/snapshot_smoke.rs` — ADR-008 check 7 (RocksDbSnapshot save +
//!   load round-trip on a 256-byte payload).
