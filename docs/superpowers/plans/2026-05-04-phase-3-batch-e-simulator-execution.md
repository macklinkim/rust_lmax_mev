# Phase 3 Batch E — `crates/simulator` revm LOCAL Pre-Sim

**Date:** 2026-05-04
**Status:** Draft v0.2 (revised after Codex 2026-05-04 18:03:59 +09:00 REVISION REQUIRED HIGH: DP-S1 reframed as a "P3-E simulator pipeline smoke / provenance shim", NOT ADR-006-strict local sim; explicit user sign-off block added because ADR-006 scope deferral is on the user-gate list per task_state.md routine policy; revm pin recorded in this note before any Cargo edit; S-3 tightened to deterministic OutOfGas expectation. DP-S2 / DP-S3 unchanged. ProfitSource::HeuristicPassthrough kept; MismatchCategory still excluded from P3-E.) — v0.1 (pre-impl review request surfacing the scope tension).
**Predecessor:** P3-D closed at `33370ed` (Codex APPROVED MEDIUM 2026-05-04 17:39:19).

## Scope

New crate `crates/simulator`. Pure-function-ish `LocalSimulator` that takes a `RiskCheckedOpportunity` (from P3-D) and runs a deterministic `revm` evaluation, returning a `SimulationOutcome { gas_used, simulated_profit_wei, status }`. NO relay simulation, NO bundle submission, NO live mainnet calls, NO funded key — those are all Phase 4+.

P3-F wires the simulator into the pipeline (consumer of `RiskCheckedOpportunity`, producer of `SimulationOutcome`). P3-E ships only the engine + the deterministic revm setup.

## Scope reduction relative to ADR-006 (load-bearing — needs Codex sign-off)

ADR-006 §"Simulation pipeline" mandates: **"Local pre-simulation (revm against the current state snapshot)"**. Strict reading: revm executes the actual user transaction (or bundle calldata) against a snapshot containing every EVM storage slot the transaction touches, including all Uniswap pool contract storage.

Phase 3 thin-path **cannot** satisfy that strict reading because:

1. **No full-state snapshot.** P2-B's `RocksDbSnapshot` stores only the *reserves summary* per pool (`PoolState::UniV2 { reserve0, reserve1, ts }` / `UniV3 { sqrt_price_x96, tick, liquidity }`), not the raw EVM storage slots that revm needs to load Uniswap V2/V3 contract code + execute a real `swap()`.
2. **No archive node integration.** Phase 2 overview §"Forbidden additions" explicitly says "No archive node integration — Phase 4 (ADR-007)". revm-against-real-state requires an archive node (or equivalent eth_getProof / state-fetcher) to populate storage slots on demand.
3. **No real Uniswap bytecode loading.** P3-E cannot load the deployed V2 + V3 bytecode without (2).

Three scope options for P3-E to satisfy the Phase 3 EXIT trigger ("captured event → simulated profit signal → bundle construction") under these constraints:

- **DP-S1 (default — Phase 3 simulator pipeline smoke / provenance shim, NOT ADR-006-strict local sim)**: `LocalSimulator` constructs a deterministic revm `Evm` instance, deploys a tiny test contract (≤30 lines of opcodes; in-tree bytecode constant — no Solidity compiler), executes a single transaction against it, captures `gas_used` + status. `simulated_profit_wei` is **passed through** from the upstream P3-C heuristic (`risk_checked.opportunity.expected_profit_wei`, possibly clamped) and stamped with `ProfitSource::HeuristicPassthrough`. **DP-S1 explicitly defers ADR-006-strict "revm against the current state snapshot" to Phase 4.** Honest framing: P3-E delivers "revm wired and ready, pipeline determinism proven" — NOT "revm-validated profit". The profit number is the P3-C heuristic; revm validates only that the bytecode pipeline runs deterministically with stable gas accounting. P4 swaps in real Uniswap bytecode + state-fetcher and flips `ProfitSource` to `RevmComputed` while keeping the API surface unchanged.
- **DP-S2**: hand-write minimal AMM bytecode for a "fake CPMM" contract (2 storage slots = `reserve0`, `reserve1`; one entrypoint = `swap(amount_in_token0)` returning `amount_out_token1` per `x*y=k`); deploy two instances; execute two-hop swap; `simulated_profit_wei = balance_after - balance_before`. Adds maintenance burden (hand-coded EVM bytecode is brittle) but produces a real revm-computed profit number, not a heuristic pass-through.
- **DP-S3**: full real-Uniswap revm — **FORBIDDEN** by Phase 2 overview "no archive node". Would require an ADR change.

**Default = DP-S1.** Rationale: ADR-001 line 43 needs *a* simulated profit signal, not necessarily a precision-grade one; P3-C's heuristic + P3-E's revm pipeline-shim together satisfy "captured event → simulated profit signal → bundle construction" end-to-end. `SimulationOutcome.profit_source: ProfitSource::HeuristicPassthrough` makes the provenance visible; P4 changes that variant to `ProfitSource::RevmComputed`.

## ADR-006 deferral — REQUIRES EXPLICIT USER APPROVAL (v0.2 per Codex 18:03:59)

DP-S1 is an **ADR/scope/frozen-decision deferral**: ADR-006 §"Simulation pipeline" mandates "Local pre-simulation (`revm` against the current state snapshot)" for every bundle before submission. DP-S1 ships a deterministic revm pipeline shim that does **not** execute the actual opportunity calldata against the actual current EVM storage — it executes test bytecode against an in-memory database and passes the heuristic profit through.

Per `task_state.md` "Protocol update authorized 2026-05-04 KST" the user-question list includes "ADR/scope/frozen decision 변경". Codex 18:03:59 explicitly says: do not approve DP-S1 without explicit user sign-off on the deferral.

**Required user statement (verbatim or equivalent) before P3-E implementation begins:**

> Approve deferring ADR-006 strict "revm against the current state snapshot" local simulation to Phase 4 (alongside archive node integration per ADR-007). Phase 3 P3-E ships the DP-S1 pipeline shim with `ProfitSource::HeuristicPassthrough`; Phase 4 will land DP-S3 (real Uniswap bytecode + state) and flip `ProfitSource` to `RevmComputed` — at which point the ADR-006 contract is fully met.

Without that statement, Claude HALTS at P3-E and waits.

If the user instead chooses DP-S2 (hand-coded minimal CPMM bytecode + revm-computed profit) the deferral framing changes — Phase 3 ships a real revm-computed number, but for a SYNTHETIC pool, not the real Uniswap pool. Strict ADR-006 still not met (real bytecode/state still requires P4) but the heuristic-passthrough framing goes away. Claude can proceed under DP-S2 with the same user-approved ADR deferral, just with a richer Phase 3 deliverable.

If the user wants strict ADR-006 in Phase 3 (i.e., DP-S3 = archive node + real bytecode in Phase 3), that's a Phase 2 overview "Forbidden additions" reversal AND an effective Phase 3/4 scope swap — much larger user decision. Claude HALTS at P3-E in that case until the scope swap is documented.

## API surface (DP-S1 baseline)

```rust
// crates/simulator/src/lib.rs

pub struct LocalSimulator {
    cfg: SimConfig,
    // Deterministic revm setup; pre-deployed test contract address +
    // pre-funded EOA. No mempool, no live state.
    deployed_test_contract: Address,
    eoa_signer: Address,
}

#[derive(Debug, Clone, PartialEq, Eq, ...derives...)]
pub struct SimConfig {
    pub chain_id: u64,             // 1 (per ADR-002)
    pub gas_limit_per_sim: u64,    // 30_000_000 default
    pub base_fee_wei: U256,        // 30 gwei default
    pub eoa_initial_balance_wei: U256,
}

impl LocalSimulator {
    pub fn new(cfg: SimConfig) -> Result<Self, SimulationError>;

    /// Runs the deterministic revm pipeline for the given checked
    /// opportunity. Pure-function-ish: same input → byte-identical
    /// SimulationOutcome (S-2 determinism test).
    pub fn simulate(
        &self,
        risk_checked: &RiskCheckedOpportunity,
    ) -> Result<SimulationOutcome, SimulationError>;
}

#[derive(Debug, Clone, PartialEq, Eq, ...derives...)]
pub struct SimulationOutcome {
    pub opportunity_block_number: u64,
    pub gas_used: u64,
    pub status: SimStatus,
    pub simulated_profit_wei: U256,         // DP-S1: passthrough; P4: revm-computed
    pub profit_source: ProfitSource,        // makes provenance explicit
}

#[derive(...derives..., #[non_exhaustive])]
pub enum SimStatus {
    Success,
    Reverted { reason_hex: String },
    OutOfGas,
    HaltedOther { reason: String },
}

#[derive(...derives..., #[non_exhaustive])]
pub enum ProfitSource {
    /// P3-E: revm pipeline ran; profit value is heuristic-passthrough
    /// from P3-C; revm validated only the bytecode pipeline.
    HeuristicPassthrough,
    /// P4+: simulated_profit_wei is the actual revm-computed delta.
    RevmComputed,
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum SimulationError {
    #[error("revm setup failed: {0}")]
    Setup(String),
    #[error("revm execution failed: {0}")]
    Execution(String),
}
```

## Risk decisions (P3-E specific)

1. **revm version pin (v0.2 per Codex 18:03:59)**: workspace dep `revm = { version = "14", default-features = false, features = ["std"] }`. Pin chosen because 14.x is the latest stable revm series with `alloy-primitives` interop matching the workspace's `alloy = 0.8`. If the actual `cargo build` at scaffold time surfaces a version-resolution conflict (e.g., revm 14.x requires alloy-primitives 0.7 or pulls a transitive incompatibility), Claude HALTS at scaffold + revises this note to record the actual compatible pin BEFORE any source code lands. Per ADR-004 exact-minor policy. Minimal feature set; narrow on demand.
2. **No `revm-database-interface` / no `MainnetState`** in P3-E. We use `revm::InMemoryDB` (or equivalent) seeded with the test contract bytecode + EOA balance only. No state fetching from Geth.
3. **Test contract bytecode**: hand-coded constant `&[u8]` for a stack-balance-zero "no-op" contract that returns immediately (≤10 opcodes). DP-S2 fallback would replace this with a hand-coded CPMM AMM (still ≤200 bytes).
4. **Determinism**: chain_id, base_fee, timestamp, EOA address, contract address, calldata are ALL constants in `SimConfig` defaults. Same `RiskCheckedOpportunity` input → byte-identical `SimulationOutcome` (S-2 test).
5. **Profit pass-through**: `simulated_profit_wei = risk_checked.size_wei * (heuristic_profit_per_wei)`. Specifically reuse `risk_checked.opportunity.expected_profit_wei` directly (already P3-C-computed). No double-math.
6. **Gas accounting honest**: `gas_used` IS the actual revm-reported gas for the test transaction — that's the legitimately-measured value in P3-E. NOT a heuristic.
7. **MismatchCategory NOT in P3-E**: per ADR-006 the 6 mismatch categories compare LOCAL vs RELAY sim. P3-E ships only LOCAL; P4 (with relay) wires the comparator + the `MismatchCategory` enum (which lives in `crates/types` per Codex 15:03:59 Q4 if cross-crate consumed).

## Test matrix (lean per `feedback_phase2_doc_volume.md`)

`crates/simulator` (5 tests):
- **S-1 happy** `simulate_returns_success_for_valid_opportunity` — simulate a known-shape `RiskCheckedOpportunity` → `Ok(SimulationOutcome { status: Success, gas_used > 0, profit_source: HeuristicPassthrough })`.
- **S-2 determinism** `simulate_is_byte_identical_for_repeated_call` — two `simulate()` calls with same input → equal `SimulationOutcome` (mirrors P3-C O-7).
- **S-3 OOG (tightened v0.2 per Codex 18:03:59)** `simulate_returns_out_of_gas_for_tiny_gas_limit` — `SimConfig.gas_limit_per_sim` set to a value strictly below the test contract's measured baseline gas usage (impl picks the value after measuring S-1's `gas_used`); simulate → `Ok(SimulationOutcome { status: SimStatus::OutOfGas, .. })`. **Exact match on `OutOfGas`**, NO "or Execution error" alternative — if the impl's revm setup raises `Execution` for OOG instead of `OutOfGas`, the wrapper logic in `LocalSimulator::simulate` MUST normalize that case to `SimStatus::OutOfGas` before returning. The point of S-3 is to prove the gate's status mapping is deterministic, not to accept ambiguity.
- **S-4 rkyv** `simulation_outcome_envelope_round_trips` — wraps `SimulationOutcome` in `EventEnvelope<SimulationOutcome>`, rkyv round-trip, asserts equality. Mirrors P3-A/P3-C/P3-D pattern.
- **S-5 setup-failure** `new_returns_error_on_invalid_config` — e.g., `chain_id == 0` or `eoa_initial_balance_wei == 0` → `Err(SimulationError::Setup(_))` (one specific failure mode chosen during impl).

Total 5 new tests; workspace cumulative: 95 → **100** in CI (+1 ignored unchanged).

## Workspace + per-crate dependency deltas

`Cargo.toml`: add `"crates/simulator"` to `[workspace] members`; add `revm = { version = "<TBD>", default-features = false, features = ["std"] }` to `[workspace.dependencies]`.

`crates/simulator/Cargo.toml` runtime deps:
- `rust-lmax-mev-types` (path), `rust-lmax-mev-state` (path; brings `PoolId`), `rust-lmax-mev-opportunity` (path; brings `OpportunityEvent`), `rust-lmax-mev-risk` (path; brings `RiskCheckedOpportunity`), `alloy-primitives` (workspace), `serde` (workspace), `rkyv` (workspace), `thiserror` (workspace), `tracing` (workspace), `revm` (workspace).
- Dev: `tempfile = "3"` (likely unused; included for symmetry).

## Commit grouping (4-5 commits)

1. `docs: add Phase 3 Batch E simulator execution note` — this file.
2. `chore(workspace): scaffold crates/simulator + revm workspace dep` — workspace member + revm pin + Cargo.toml + placeholder `lib.rs` + `rkyv_compat.rs`.
3. `feat(simulator): LocalSimulator + SimulationOutcome + revm pipeline (S-1..S-3, S-5)` — full body + 4 non-rkyv tests.
4. `test(simulator): S-4 rkyv envelope round-trip` — separate commit so the spec-compliance test mirrors P3-A/P3-C/P3-D structure.
5. (optional) `chore(batch-p3-e): pick up fmt + Cargo.lock drift at batch close` — only if needed.

Targeted `cargo test -p rust-lmax-mev-simulator` per code commit; full workspace gates ONLY at batch close + tail-summary append.

## Forbidden delta (only NEW)

- No relay simulation, no `eth_callBundle`, no `BundleRelay`, no submission, no live mainnet, no funded key.
- No archive node integration (Phase 4 per ADR-007).
- No real Uniswap V2/V3 bytecode/state loading in P3-E.
- No `MismatchCategory` enum in P3-E (P4 alongside relay sim).
- No `wire_phase3` / `wire_phase4` / app integration (P3-F).
- No edits to `crates/event-bus` / Phase-1-frozen / P2-A / P2-B / P2-C / P3-A / P3-B / P3-C / P3-D crate src.
- All standing forbids carry over.

## Question for user (v0.2 — ADR-006 deferral; user-gated per protocol)

The DP-S1 vs DP-S2 vs DP-S3 choice is an ADR-006 scope deferral, which `task_state.md` Protocol routes to user, not Codex. Codex 18:03:59 explicitly declined to approve DP-S1 without user sign-off. **No P3-E source/Cargo edit happens until the user replies with one of:**

- **(A) Approve DP-S1**: ship the pipeline shim + heuristic passthrough + `ProfitSource::HeuristicPassthrough`; ADR-006 strict local sim deferred to Phase 4 alongside ADR-007 archive-node integration.
- **(B) Approve DP-S2**: hand-coded minimal CPMM bytecode + revm-computed profit on a SYNTHETIC pool (real Uniswap bytecode/state still Phase 4). ADR-006 deferral framing changes but is still active.
- **(C) Push to DP-S3 (Phase 3 scope expansion)**: archive-node integration + real Uniswap bytecode + state in Phase 3. Requires reversing Phase 2 overview "no archive node" + significant scope swap. Claude HALTS until the user documents the swap.

## Open question for Codex (v0.2 — non-scope items only)

After the user picks A/B/C, Claude does not need another Codex review for the scope question per Codex 18:03:59. Remaining concrete questions on the v0.2 plan content (already addressed in v0.2; listed here for verification at batch close):

1. revm 14 pin compatible with workspace alloy 0.8 — verified at scaffold; HALT + revise if not.
2. `SimulationOutcome` shape `{ opportunity_block_number, gas_used, status, simulated_profit_wei, profit_source }` complete — confirm at batch close.
3. `SimStatus` 4-variant `{ Success | Reverted { reason_hex } | OutOfGas | HaltedOther { reason } }` sufficient — confirm at batch close.
4. S-1..S-5 deterministic; S-3 tightened to exact `OutOfGas` match (no "or Execution") — implemented per spec.

If user picks A or B and v0.2 plan items above hold: execute the 4-5 commit ladder + batch-close evidence pack. If user picks C: HALT and wait for the documented scope swap.
