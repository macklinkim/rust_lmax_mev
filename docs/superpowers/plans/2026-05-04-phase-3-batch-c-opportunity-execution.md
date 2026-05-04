# Phase 3 Batch C — `crates/opportunity` Arbitrage Detection

**Date:** 2026-05-04
**Status:** Draft v0.1. **No pre-impl Codex review** per Phase 3 overview §"Architectural risks" (P3-C: pure-function arbitrage math; standard test-driven build). Codex batch-close review at the end.
**Predecessor:** P3-B closed at `8dee524` (Codex APPROVED MEDIUM 2026-05-04 16:22:10).

## Scope

New crate `crates/opportunity`. Pure function: given the latest `PoolState` for the WETH/USDC pair on Uniswap V2 and on Uniswap V3 0.05% at the same `(block_number, block_hash)`, compute the cross-venue arbitrage and emit an `OpportunityEvent` if a positive-EV opportunity exists.

No I/O, no spawn, no bus wiring. P3-D wires this engine into the pipeline; P3-B's deferred topology question (multi-consumer fanout on `ingress_bus` + StateEngine driver consumer) is also a P3-D concern, NOT a P3-C concern. P3-C only ships the math.

## New crate + API surface

```rust
// crates/opportunity/src/lib.rs

pub struct OpportunityEngine {
    weth: Address,    // from config.ingress.tokens
    usdc: Address,    // from config.ingress.tokens
}

impl OpportunityEngine {
    pub fn new(tokens: &IngressTokens) -> Self;

    /// Returns Some(event) iff cross-venue arb has positive EV after
    /// the conservative gas floor; None otherwise. Pure function — no
    /// I/O, no allocation beyond the returned struct.
    pub fn check(
        &self,
        chain_context: &ChainContext,
        pool_a: &PoolId, state_a: &PoolState,
        pool_b: &PoolId, state_b: &PoolState,
    ) -> Option<OpportunityEvent>;
}

#[derive(Debug, Clone, PartialEq, Eq,
         rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
         serde::Serialize, serde::Deserialize)]
pub struct OpportunityEvent {
    pub block_number: u64,
    #[rkyv(with = crate::rkyv_compat::B256AsBytes)]
    pub block_hash: B256,
    pub source_pool: PoolId,        // buy side (cheap)
    pub sink_pool:   PoolId,        // sell side (expensive)
    #[rkyv(with = crate::rkyv_compat::U256AsBytes)]
    pub optimal_amount_in_wei: U256,
    #[rkyv(with = crate::rkyv_compat::U256AsBytes)]
    pub expected_profit_wei: U256,
    pub gas_estimate: u64,
}

pub mod rkyv_compat;  // same B256AsBytes/U256AsBytes pattern as P3-A
```

## Math (thin-path approximation)

- **UniV2 mid-price**: `price_v2 = reserve_usdc / reserve_weth` (USDC per WETH; both as U256, normalized for decimals at use site).
- **UniV3 0.05% mid-price**: `price_v3 = (sqrt_price_x96 / 2^96)^2`. The integer `sqrt_price_x96` is a Q64.96 fixed-point square root of `token1/token0`; the squared form is the spot price at the current tick. Per ADR-006 thin-path, ignore concentrated-liquidity tick-crossing for now; treat the spot price as locally accurate.
- **Arb direction**: if `price_v2 < price_v3 - epsilon`, buy WETH on V2, sell on V3 (`source_pool = v2`, `sink_pool = v3`); if `price_v2 > price_v3 + epsilon`, the opposite. `epsilon` covers float-conversion noise + a conservative gas floor.
- **Optimal amount in (CPMM closed form for V2 → V3)**: using V2 reserves and V3 reserves-equivalent (derived from sqrt_price + liquidity), apply the standard two-pool arbitrage formula: `amount_in = sqrt(r_a_in * r_a_out * r_b_in * r_b_out / fee_factor) - r_a_in`. Phase 3 thin-path computes this with U256 fixed-point math; precision-loss is acceptable since the value is only a hint — `crates/risk` (P3-D) caps the size and `crates/simulator` (P3-E) verifies via revm.
- **Expected profit**: `(amount_out_at_sink - amount_in - fees) - gas_cost_in_wei_eq`.
- **Gas estimate**: hardcoded const for the thin path (`GAS_ESTIMATE_TWO_HOP_ARB: u64 = 350_000`); ADR-006 covers refinement in P5.

The spec's `ADR-006::Profitability` mismatch category bites at P3-E (revm sim) — P3-C just emits the predicted profit and downstream verifies.

## Workspace + per-crate dependency deltas

`Cargo.toml`: add `"crates/opportunity"` to `[workspace] members`.

`crates/opportunity/Cargo.toml` runtime deps:
- `rust-lmax-mev-types` (path), `rust-lmax-mev-state` (path; brings `PoolId`/`PoolState`), `rust-lmax-mev-config` (path; brings `IngressTokens`), `alloy-primitives` (workspace), `serde` (workspace), `rkyv` (workspace), `thiserror` (workspace), `tracing` (workspace).
- Dev: `tempfile = "3"` (likely unused; included for symmetry).

No workspace-level changes.

## Test matrix (lean per `feedback_phase2_doc_volume.md`)

`crates/opportunity` (5 tests):
- **O-1 happy** `v2_cheap_v3_expensive_emits_opportunity_v2_to_v3` — V2 reserves give price_v2 = $X; V3 sqrt_price gives price_v3 = $X * 1.005 (50 bps over fees+gas threshold) → emit `OpportunityEvent { source_pool: v2_id, sink_pool: v3_id, expected_profit_wei > 0 }`.
- **O-2 happy** `v2_expensive_v3_cheap_emits_opportunity_v3_to_v2` — symmetric.
- **O-3 boundary** `equal_price_returns_none` — exact equality (price_v2 == price_v3) → None.
- **O-4 boundary** `delta_below_gas_floor_returns_none` — 5 bps spread (below the gas floor) → None.
- **O-5 rkyv** `opportunity_event_envelope_round_trips` — wraps `OpportunityEvent` in `EventEnvelope<OpportunityEvent>`, rkyv round-trip, asserts equality (mirrors P3-A test pattern; proves the spec-compliance derives work for the new payload type).

Total 5 new tests; workspace cumulative: 79 → **84** in CI (+1 ignored unchanged).

## Commit grouping (4 commits)

1. `docs: add Phase 3 Batch C opportunity engine execution note` — this file.
2. `chore(workspace): scaffold crates/opportunity` — root `Cargo.toml` member + `crates/opportunity/Cargo.toml` + placeholder `lib.rs` + `rkyv_compat.rs`.
3. `feat(opportunity): OpportunityEngine + OpportunityEvent + UniV2/UniV3 arb math (O-1..O-4)` — full math impl + 4 tests.
4. `test(opportunity): O-5 rkyv envelope round-trip` — separate commit so the rkyv-spec-compliance test mirrors P3-A's structure.
5. (optional) `chore(batch-p3-c): pick up fmt + Cargo.lock drift at batch close`.

Targeted `cargo test -p rust-lmax-mev-opportunity` per code commit; full workspace gates ONLY at batch close + tail-summary append.

## Forbidden delta (only NEW)

- No bus wiring / `wire_phase4` / app integration in P3-C — strictly the math crate (P3-D wires it).
- No multi-consumer fanout / StateEngine driver work — that's P3-D.
- No `revm` (P3-E).
- No live-mainnet / no relay / no submission (carried over).
- All standing forbids carry over.

## Codex action

P3-C has no pre-impl review per Phase 3 overview. Routine policy applies: Claude commits + pushes the docs note, runs the 4-commit ladder, then emits the batch-close evidence pack with auto_check.md tail-summary refresh for Codex batch-close review. If math choices in §"Math" prove load-bearing for downstream P3-D/E correctness, batch-close review will surface that.
