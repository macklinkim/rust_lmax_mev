//! Phase 3 P3-D risk gate per the approved Batch D execution note v0.2
//! (`docs/superpowers/plans/2026-05-04-phase-3-batch-d-risk-execution.md`).
//!
//! Scaffold lands here in commit 2 of the P3-D ladder; full body +
//! tests land in commits 3-4. Per the approved API surface, this crate
//! exposes:
//!
//! - `RiskGate` — stateless wrt the engine itself; carries an
//!   `Arc<RwLock<RiskBudgetState>>` so a future P4 state mutator can
//!   share the same state with `evaluate()`.
//! - `RiskBudgetConfig` — every cap from `docs/specs/risk-budget.md`
//!   verbatim, including `canary_remaining_wei` initializer.
//! - `RiskBudgetState` — per-day counters + per-opportunity resubmit
//!   counts + canary balance.
//! - `AbortCategory` — six `#[non_exhaustive]` variants, one per cap.
//! - `RiskCheckedOpportunity` / `OpportunityAborted` — the
//!   `Result<_, _>` payload pair `evaluate()` returns.
//!
//! No I/O, no spawn, no bus wiring. P3-F wires the gate into the
//! pipeline. Per ADR-002 + ADR-006, Phase 3 is shadow-only — no actual
//! submission ever increments the state counters; the state-mutating
//! API lands in P4 alongside `BundleRelay`.

pub mod rkyv_compat;
