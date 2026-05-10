//! Phase 4 P4-D per-crate rkyv-with adapters for the alloy-primitives
//! types embedded in `RelaySimRequest` / `RelaySimulationOutcome` /
//! `LocalBundleShape`. Same shape as the existing
//! `crates/simulator/src/rkyv_compat.rs` adapters; adapters cannot be
//! shared across crates because rkyv-with bindings are tied to the
//! crate that defines the deriving struct.
//!
//! Implementation lands in the comparator commit alongside the type
//! definitions (D-3.c per the P4-D execution note v0.4).
