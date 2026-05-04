# Phase 3 Batch D — `crates/risk` Sizing + Budget Gate + Topology Design

**Date:** 2026-05-04
**Status:** Draft v0.2 (revised after Codex 2026-05-04 17:20:11 +09:00 REVISION REQUIRED MEDIUM: added R-8 daily-gas-cap and R-9 canary-capital tests; topology Option A clarified to require fail-closed handling of `broadcast::RecvError::Lagged`. DP-1/DP-2/DP-3 confirmed acceptable as written.) — v0.1 (pre-impl review request widening Phase-3-overview scope to include the P3-B-deferred ingress_bus topology revisit per user 2026-05-04 directive).
**Predecessor:** P3-C closed at `a70b8a2` (Codex APPROVED MEDIUM 2026-05-04 17:04:39).

## Scope

Two deliverables in this batch:

1. **New crate `crates/risk`** — pure-function `RiskGate` consuming `OpportunityEvent` (from P3-C), emits `RiskCheckedOpportunity` (passed; possibly size-clamped) or `OpportunityAborted { category }`. Implements every cap from `docs/specs/risk-budget.md` plus the ADR-006 zero-tolerance posture. State-mutating side (recording realized loss, marking a bundle live, day-boundary rollovers) is shaped today but not exercised in P3 — Phase 3 is shadow-only per ADR-002 so no real submission ever increments these counters.

2. **Topology design doc only** — document the multi-consumer fanout options for `ingress_bus` (the P3-B-deferred Codex 16:00:13 obligation). NO actual `wire_phase3` change. Implementation lands in P3-F when the final `wire_phase4` (or extended `wire_phase3`) is built — at that point the state-engine driver consumer + the new opportunity engine driver become live, and `ingress_bus` needs a real second consumer.

## New crate + API surface (`crates/risk`)

```rust
#[derive(Debug, Clone, PartialEq, Eq, ...derives... )]
pub struct RiskBudgetConfig {
    pub per_bundle_max_notional_wei: U256,        // ABS cap (0.1 ETH)
    pub per_bundle_max_notional_relative_bps: u16, // REL cap (100 = 1% of strategy cap)
    pub daily_realized_loss_cap_wei: U256,        // ABS cap (0.05 ETH)
    pub daily_realized_loss_cap_relative_bps: u16, // REL cap (300 = 3%)
    pub max_gas_spend_per_day_wei: U256,          // ABS cap (0.03 ETH)
    pub max_concurrent_live_bundles: u32,         // 1
    pub max_resubmits_per_opportunity: u32,       // 2
    pub initial_canary_capital_wei: U256,         // 0.5 ETH
    pub strategy_capital_wei: Option<U256>,       // None until set; relative caps gated on this
}

pub struct RiskBudgetState {
    pub daily_realized_loss_wei: U256,
    pub gas_spend_today_wei: U256,
    pub concurrent_live_bundles: u32,
    pub resubmits_per_opportunity: HashMap<OpportunityKey, u32>,
    pub day_started_unix_ns: u64,
    /// Remaining canary capital. Initialized to `config.initial_canary_capital_wei`.
    /// `evaluate` reads this; the P4 state mutator decrements when a bundle lands.
    pub canary_remaining_wei: U256,
}

#[derive(...derives..., #[non_exhaustive])]
pub enum AbortCategory {
    PerBundleNotionalCapExceeded,
    DailyLossCapWouldBeExceeded,
    DailyGasCapWouldBeExceeded,
    ConcurrencyCapExceeded,
    ResubmitCapExceeded,
    InsufficientCanaryCapital,
}

pub struct RiskCheckedOpportunity { /* opportunity + clamped size_wei */ }
pub struct OpportunityAborted { /* opportunity + AbortCategory */ }

pub struct RiskGate { config: RiskBudgetConfig, state: Arc<RwLock<RiskBudgetState>> }

impl RiskGate {
    pub fn new(config: RiskBudgetConfig) -> Self;
    pub fn with_state(config: RiskBudgetConfig, state: RiskBudgetState) -> Self; // test ctor

    /// Read-side: applies all caps in the spec'd order; returns Approved
    /// (possibly clamped) or Aborted. Does NOT mutate state.
    pub fn evaluate(&self, opp: &OpportunityEvent) -> Result<RiskCheckedOpportunity, OpportunityAborted>;
}
```

Effective cap rule (per spec): `effective = min(absolute, relative_if_strategy_capital_is_some)`. Absolute always applies; relative kicks in only when `strategy_capital_wei.is_some()`.

`OpportunityKey` = `(block_number, source_pool.address, sink_pool.address)` so resubmit counting is per-opportunity-instance and survives across `evaluate` calls within the same block.

## Decision points (defaults; Codex pre-impl review confirms or revises)

- **DP-1 (state mutators in P3-D)** — No state-mutating API in P3-D (`record_realized_loss`, `mark_live`, `day_rollover`). Phase 3 is shadow-only per ADR-002; no submission ever increments the counters. State-side API lands in P4 alongside `BundleRelay`. P3-D ships ONLY `evaluate()` (read-side) + a test-only `with_state` ctor for seeding.
- **DP-2 (clamping vs reject on per-bundle notional)** — Default: CLAMP. If `opp.optimal_amount_in_wei > effective_per_bundle_cap`, return `RiskCheckedOpportunity { size_wei: effective_per_bundle_cap }` (downstream sim verifies the clamped size). Alternative: REJECT with `PerBundleNotionalCapExceeded`. Default chosen because the opportunity is still real at the smaller size; the downstream simulator + execution will compute actual profit with the clamped amount.
- **DP-3 (resubmit counting key)** — `(block_number, source_pool.address, sink_pool.address)`. Two opportunities for the same pool pair at the same block are the same instance; if one was aborted-then-re-submitted, the resubmit count grows. Caveat: resubmit counting is meaningful only when paired with the state mutator (DP-1) so it remains a P4 primitive even though the data structure exists in P3.
- **DP-4 (RiskBudgetConfig defaults)** — Hardcoded constants `pub const DEFAULT_*` mirror the risk-budget.md table verbatim. No TOML wiring in P3-D (config plumbing is `crates/app`-side; can land additively in P3-F).
- **DP-5 (test fixtures)** — Deterministic small-integer fixtures only. No live trading, no funded key, no relay, no submission anywhere. `with_state` ctor seeds the necessary `RiskBudgetState` for the state-driven cap tests.

## Test matrix (lean per `feedback_phase2_doc_volume.md`)

`crates/risk` (9 tests, v0.2 expanded per Codex 17:20:11 to cover every `AbortCategory` variant):
- **R-1 happy** `evaluate_returns_approved_for_small_safe_opportunity` — 0.005 ETH opp under all caps → `Ok(RiskCheckedOpportunity { size_wei: 0.005 ETH })`.
- **R-2 clamp** `evaluate_clamps_oversized_opportunity_to_per_bundle_cap` — 1 ETH opp + 0.1 ETH cap → `Ok(RiskCheckedOpportunity { size_wei: 0.1 ETH })`.
- **R-3 abort daily loss** `evaluate_aborts_when_daily_loss_at_cap` — seed state with `daily_realized_loss_wei == cap`; new opp → `Err(DailyLossCapWouldBeExceeded)`.
- **R-4 abort concurrency** `evaluate_aborts_when_concurrent_live_at_cap` — seed state with `concurrent_live_bundles == max` → `Err(ConcurrencyCapExceeded)`.
- **R-5 abort resubmits** `evaluate_aborts_when_resubmits_at_cap` — seed `resubmits_per_opportunity[opp_key] == max` → `Err(ResubmitCapExceeded)`.
- **R-6 boundary** `evaluate_aborts_when_strategy_capital_unset_and_relative_required` — config requires relative cap but `strategy_capital_wei == None` → defaults to absolute-only (per spec "Absolute caps always apply, even when strategy capital is undefined"). Asserts no panic + falls through to absolute cap.
- **R-7 rkyv** `risk_checked_opportunity_envelope_round_trips` — wraps `RiskCheckedOpportunity` in `EventEnvelope<RiskCheckedOpportunity>`, rkyv round-trip, asserts equality. Mirrors P3-A/P3-C pattern.
- **R-8 abort daily gas (NEW v0.2)** `evaluate_aborts_when_daily_gas_at_cap` — seed state with `gas_spend_today_wei == max_gas_spend_per_day_wei`; new opp's `gas_estimate * gas_price_proxy` would exceed → `Err(DailyGasCapWouldBeExceeded)`. Phase 3 gas_price_proxy is a per-eval const (e.g., 30 gwei); P5 wires real gas-price feed.
- **R-9 abort canary capital (NEW v0.2)** `evaluate_aborts_when_size_exceeds_canary_capital` — config has `initial_canary_capital_wei = 0.5 ETH` and the opportunity's effective size (post-clamp) would consume more than the remaining canary balance (state-tracked) → `Err(InsufficientCanaryCapital)`. The canary balance state is part of `RiskBudgetState` (additive in v0.2).

Total 9 new tests; workspace cumulative: 86 → **95** in CI (+1 ignored unchanged).

## Workspace + per-crate dependency deltas

`Cargo.toml`: add `"crates/risk"` to `[workspace] members`.

`crates/risk/Cargo.toml` runtime deps:
- `rust-lmax-mev-types` (path), `rust-lmax-mev-state` (path; brings `PoolId`), `rust-lmax-mev-opportunity` (path; brings `OpportunityEvent`), `alloy-primitives` (workspace), `serde` (workspace), `rkyv` (workspace), `thiserror` (workspace), `parking_lot` (workspace; for `RwLock<RiskBudgetState>`).
- Dev: `tempfile = "3"` (likely unused; included for symmetry).

No workspace-level changes.

## Topology design doc (P3-B-deferred Codex 16:00:13 obligation)

Three options for adding a second consumer to `ingress_bus` (which today carries only the journal-drain consumer):

- **Option A (default)**: insert a `tokio::sync::broadcast` rebroadcast layer between the producer task and downstream consumers. Producer publishes once; broadcast fans out to N receivers. Lossless when all consumers keep up; has built-in `RecvError::Lagged(skipped)` for slow consumers. Idiomatic with the existing tokio producer task. **Cost:** one extra channel hop + reformatting the consume_loop to take a `broadcast::Receiver<EventEnvelope<T>>` instead of `CrossbeamConsumer`.

  **v0.2 fail-closed lag policy (per Codex 17:20:11)**: in P3-F, ANY consumer that observes `broadcast::RecvError::Lagged(n)` MUST treat the lag as fatal for that pipeline path — abort the consumer task with a tracing error, surface the lag count in metrics, and rely on the supervising `wire_phase4` runtime drop to tear the pipeline down. **No silent skip, no continue.** The journal-drain consumer in particular cannot tolerate gaps because replay determinism (P2-C EXIT gate) depends on every published envelope reaching the journal. The state-engine driver and opportunity-engine driver consumers similarly cannot tolerate missed `BlockEvent`s because per-block snapshot keying depends on full coverage. P3-F's batch-close evidence pack will include explicit test coverage of the Lagged → fail-closed path before any merge.
- **Option B**: a crossbeam "tee" thread that reads from `ingress_bus.consumer_handle` and forwards each envelope to N downstream queues (each its own `CrossbeamBoundedBus<IngressEvent>`). Keeps `consume_loop`'s shape unchanged. **Cost:** an extra thread per fanout point; backpressure semantics get muddier (slow downstream backs up the tee, which backs up everything).
- **Option C**: refactor `crates/event-bus::CrossbeamBoundedBus` to support multi-consumer natively. **Cost:** breaks the Phase 1 freeze on `crates/event-bus` (forbidden without ADR change).

**Recommendation:** Option A. Lands in P3-F when the state-engine driver consumer + opportunity-engine driver consumer + journal-drain consumer all need to read `ingress_bus`. P3-D records the choice but does not modify `crates/app` or `crates/event-bus`.

## Commit grouping (4 commits)

1. `docs: add Phase 3 Batch D risk + topology execution note` — this file.
2. `chore(workspace): scaffold crates/risk` — workspace member + Cargo.toml + placeholder lib.rs + rkyv_compat.rs.
3. `feat(risk): RiskGate + RiskBudgetConfig/State + AbortCategory + evaluate (R-1..R-6)` — full body + 6 non-rkyv tests.
4. `test(risk): R-7 rkyv envelope round-trip` — separate commit so the spec-compliance test mirrors P3-A/P3-C structure.
5. (optional) `chore(batch-p3-d): pick up fmt + Cargo.lock drift at batch close` — only if needed.

Targeted `cargo test -p rust-lmax-mev-risk` per code commit; full workspace gates ONLY at batch close + tail-summary append.

## Forbidden delta (only NEW)

- No state-mutating API in P3-D (DP-1; deferred to P4).
- No `wire_phase3` / `wire_phase4` / app integration in P3-D — strictly the math crate + topology DOC (P3-F wires).
- No `BundleRelay` / `RelaySimulator` / `revm` / submission / live mainnet / funded key.
- No edits to `crates/event-bus` (Phase 1 freeze; topology Option C requires ADR).
- All standing forbids carry over.

## Question for Codex (pre-impl, v0.2)

v0.2 incorporates Codex 17:20:11 (REVISION REQUIRED MEDIUM):
- Test matrix expanded 7 → 9: R-8 daily gas cap abort + R-9 canary capital abort. Workspace target now 86 → 95.
- `RiskBudgetState.canary_remaining_wei` field added (initialized from config) so R-9 has somewhere to seed.
- Topology Option A clarified: `broadcast::RecvError::Lagged` MUST be fail-closed/fatal in P3-F; no silent skip.
- DP-1 (state mutators → P4), DP-2 (clamp default), DP-3 (`(block_number, source_pool, sink_pool)` key encodes direction) confirmed acceptable per Codex 17:20:11.

Open question: anything else needed before the 4-5 commit ladder runs?

If APPROVED: execute the ladder + batch-close evidence pack with auto_check.md tail-summary refresh. If REVISION: revise + re-emit. If ADR/scope/freeze change required: HALT to user.
