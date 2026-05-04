//! Phase 3 P3-E LOCAL revm pre-sim shim per the user-approved P3-E
//! execution note v0.2 (DP-S1: ADR-006 strict "revm against the
//! current state snapshot" deferred to Phase 4 alongside ADR-007
//! archive node integration).
//!
//! Scaffold lands here in commit 2 of the P3-E ladder; full body +
//! tests land in commits 3-4.

pub mod rkyv_compat;
