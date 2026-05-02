//! Phase 1 binary entrypoint library for the LMAX-style MEV engine.
//!
//! Per Batch B execution note (`docs/superpowers/plans/2026-05-02-phase-1-
//! batch-b-app-execution.md`). This crate wires the foundation crates
//! (`rust-lmax-mev-config`, `rust-lmax-mev-observability`,
//! `rust-lmax-mev-journal`, `rust-lmax-mev-event-bus`,
//! `rust-lmax-mev-types`) into a runnable Phase 1 process.
//!
//! Behavior is added in the next commit (`feat(app): run() wiring +
//! AppError + 3 integration tests`); this scaffold compiles but exposes
//! no public surface.
