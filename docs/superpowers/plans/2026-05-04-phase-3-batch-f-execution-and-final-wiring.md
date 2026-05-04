# Phase 3 Batch F — `crates/execution` + Final Wiring + DoD Audit + `phase-3-complete` Tag

**Date:** 2026-05-04
**Status:** Draft v0.1. **No pre-impl Codex review** per Phase 3 overview §"Architectural risks" (P3-F is bundle-construction shape-driven; final app wiring follows the P2-D `wire_phase2` precedent + P3-D documented topology Option A). Batch-close Codex review at end.
**Predecessor:** P3-E closed at `f9560c0` (Codex APPROVED MEDIUM 2026-05-04 18:40:46).

## Scope

Three deliverables in this batch:

1. **New crate `crates/execution`** — pure-function `BundleConstructor` that consumes a `SimulationOutcome` (from P3-E) and emits a `BundleCandidate` (or `ExecutionError` if the simulation status disqualifies it). NO relay submission, NO bundle signing, NO funded key. Phase 4 wires `BundleRelay` to actually submit.
2. **App wiring upgrade** — new `wire_phase4` async constructor (additive; existing `wire`/`wire_phase2`/`wire_phase3` stay byte-identical) implementing the **topology Option A `tokio::sync::broadcast` rebroadcast layer** documented in P3-D, with the **fail-closed `RecvError::Lagged` policy** from P3-D v0.2. The journal-drain consumer + a state-engine driver consumer both subscribe to the broadcast tee. Per the user 2026-05-04 directive, the full opportunity → risk → simulator → execution driver chain is also wired in `wire_phase4` (each driver is a small async task spawned by `wire_phase4`).
3. **Phase 3 DoD audit + `phase-3-complete` annotated tag draft** — mirrors the P1 Batch D + P2 Batch D pattern. Tag creation + push proceed under the routine policy after Codex APPROVED.

## New crate + API surface (`crates/execution`)

```rust
pub struct BundleConstructor { cfg: BundleConfig }

#[derive(...derives..., #[serde(deny_unknown_fields)])]
pub struct BundleConfig {
    pub fixed_bid_fraction: f64,           // ADR-006 default 0.90
    pub coinbase_recipient: Address,       // builder coinbase target (Phase 4 = real)
    pub validity_block_window: u64,        // bundle valid-from/valid-to span (default 5 blocks)
}

impl BundleConstructor {
    pub fn new(cfg: BundleConfig) -> Result<Self, ExecutionError>;
    /// Pure: returns a BundleCandidate iff `outcome.status == Success`
    /// AND `outcome.simulated_profit_wei > 0` AND
    /// `expected_bid = profit * cfg.fixed_bid_fraction > 0`. Else
    /// returns `Err(ExecutionError::Aborted { category })`.
    pub fn construct(&self, outcome: &SimulationOutcome) -> Result<BundleCandidate, ExecutionError>;
}

#[derive(...derives...)]
pub struct BundleCandidate {
    pub opportunity_block_number: u64,
    pub gas_used: u64,
    pub simulated_profit_wei: U256,
    pub gas_bid_wei: U256,           // floor((profit * fixed_bid_fraction) / 1e18 * 1e18)
    pub validity_block_min: u64,     // opp.block_number
    pub validity_block_max: u64,     // opp.block_number + window - 1
    pub profit_source: ProfitSource, // mirrored from upstream (P3-E HeuristicPassthrough; P4 RevmComputed)
}

#[non_exhaustive] pub enum AbortReason {
    SimulationNotSuccess,    // status != Success
    NonPositiveProfit,
    BidRoundsToZero,
    InvalidConfig,
}
pub enum ExecutionError {
    Aborted { reason: AbortReason },
    Setup(String),
}
```

NO `BundleRelay`-shape members in `BundleCandidate` (signed-tx hash, relay endpoints, signer-id) — Phase 4 adds those alongside `BundleRelay`. The Phase 3 candidate is the *intent* to submit, not a submittable bundle.

## App wiring upgrade — `wire_phase4`

`wire_phase4` is **additive**: existing `wire` / `wire_phase2` / `wire_phase3` stay byte-identical. `run()` updates to call `wire_phase4` instead of `wire_phase3` so the binary entrypoint flows through the full chain.

Topology (Option A from P3-D, with v0.2 fail-closed `Lagged` policy):

```
GethWsMempool::stream() (tokio task)
  └─→ ingress_broadcast (tokio::sync::broadcast<EventEnvelope<IngressEvent>>)
        ├─→ ingress journal-drain task (spawn_blocking → FileJournal<IngressEvent>::append)
        └─→ state-engine driver task (filters BlockEvent → engine.refresh_block.await)
              └─→ state_broadcast (tokio::sync::broadcast<EventEnvelope<StateUpdateEvent>>)
                    ├─→ state journal-drain task
                    └─→ opportunity-engine driver task (pairs UniV2 + UniV3 snapshots → opp.check)
                          └─→ opp_broadcast
                                └─→ risk-gate driver task (risk.evaluate)
                                      └─→ risk_broadcast
                                            └─→ simulator driver task (sim.simulate)
                                                  └─→ sim_broadcast
                                                        └─→ execution driver task (constructor.construct)
                                                              └─→ exec_broadcast (no further consumer in P3; P4 attaches BundleRelay)
```

Every consumer task uses the canonical loop:
```rust
loop {
    match rx.recv().await {
        Ok(envelope) => { /* process; publish if success */ }
        Err(broadcast::error::RecvError::Lagged(n)) => {
            tracing::error!(skipped = n, consumer = "<name>",
                "broadcast lagged; aborting consumer per P3-D v0.2 fail-closed policy");
            return;  // task exits; supervising runtime tears down on shutdown.
        }
        Err(broadcast::error::RecvError::Closed) => return,
    }
}
```

`AppHandle4` carries the broadcast `Sender`s + every spawned task `JoinHandle`. `AppHandle4::shutdown` (async) aborts the producer task, awaits it, drops the broadcast Senders one by one (in reverse-pipeline order), awaits each driver task. Same load-bearing ordering pattern as P3-B `AppHandle3::shutdown`.

## Decision points (defaults; this is the lean shape — Phase 4 swaps in real BundleRelay + signer + funded key)

- **DP-1 driver test isolation**: each driver function is `pub async fn <name>_driver(rx, ..., tx)` testable in isolation by feeding crafted broadcast channels. Batch tests use this isolation; full end-to-end wiring `wire_phase4` is exercised only by W-1 (deterministic shutdown) + W-2 (Lagged fail-closed).
- **DP-2 opportunity-driver pairing**: opportunity engine needs TWO snapshots (UniV2 + UniV3 of the WETH/USDC pair) at the same block. The driver maintains a small in-process cache `HashMap<u64, Vec<StateUpdateEvent>>` keyed by block_number; on receiving a new event, looks up + invokes `opp_engine.check` for every distinct (V2, V3) pair, then emits `OpportunityEvent`(s).
- **DP-3 broadcast capacity**: `tokio::sync::broadcast::channel(config.bus.capacity)` for every fan-out point. Lagged threshold = same as bus capacity.
- **DP-4 NO state mutators in P3-F**: `RiskGate.evaluate` is read-only; per P3-D DP-1 the state-mutating side is P4. The risk driver in P3-F evaluates the gate but never increments concurrency / loss counters (no real bundle ever lands).
- **DP-5 execution driver writes to `exec_broadcast` but no downstream consumer**: P3-F ships the producer side only; P4 attaches `BundleRelay` as the consumer.

## Test matrix (lean per `feedback_phase2_doc_volume.md`)

`crates/execution` (3 tests):
- **E-1 happy** `construct_returns_candidate_for_successful_sim` — `SimulationOutcome { status: Success, simulated_profit_wei: 5e15, .. }` → `Ok(BundleCandidate { gas_bid_wei = 4.5e15, .. })` (90% of profit per default `fixed_bid_fraction`).
- **E-2 abort** `construct_aborts_when_sim_not_success` — `SimulationOutcome { status: OutOfGas, .. }` → `Err(ExecutionError::Aborted { reason: SimulationNotSuccess })`.
- **E-3 rkyv** `bundle_candidate_envelope_round_trips` — wraps `BundleCandidate` in `EventEnvelope<BundleCandidate>`, rkyv round-trip, asserts equality. Mirrors the P3-A/C/D/E pattern.

`crates/app` (2 tests, 9 total):
- **W-1 deterministic shutdown** `wire_phase4_shutdown_drains_broadcast_chain` — spawn `wire_phase4` against bogus URL (per the P3-B B-1 / P2-D D-1 pattern: `geth_http_url = "not-a-url"` returns `Err(AppError::Node)` within `tokio::time::timeout(5s)`). Compile-time assertion on `AppHandle4::shutdown` async signature.
- **W-2 broadcast Lagged fail-closed** `journal_drain_consumer_exits_on_broadcast_lagged` — direct unit test of the journal-drain consumer loop on a small `tokio::sync::broadcast` channel: subscribe a consumer, send `capacity + N` events without recv'ing, drive one recv → consumer exits with `Err(Lagged(N))` and the loop returns. Asserts the fail-closed semantic without standing up the full pipeline.

Total 5 new tests; workspace cumulative: 100 → **105** in CI (+1 ignored unchanged).

## Workspace + per-crate dependency deltas

`Cargo.toml`: add `"crates/execution"` to `[workspace] members`.

`crates/execution/Cargo.toml`: path-deps `rust-lmax-mev-types` + `rust-lmax-mev-state` + `rust-lmax-mev-opportunity` + `rust-lmax-mev-risk` + `rust-lmax-mev-simulator`; workspace deps `alloy-primitives`/`serde`/`rkyv`/`thiserror`/`tracing`. NO `revm`. NO new workspace deps.

`crates/app/Cargo.toml`: add path-deps `rust-lmax-mev-opportunity` + `rust-lmax-mev-risk` + `rust-lmax-mev-simulator` + `rust-lmax-mev-execution`. Workspace deps already carry `tokio` (which provides `sync::broadcast`).

`crates/app/src/lib.rs`: add `wire_phase4` + `AppHandle4` + the driver task functions; existing `wire`/`wire_phase2`/`wire_phase3` UNCHANGED. `run()` updates to `runtime.block_on(wire_phase4(...))`.

## Commit grouping (5-6 commits)

1. `docs: add Phase 3 Batch F execution + final wiring + tag execution note` — this file.
2. `chore(workspace): scaffold crates/execution` — workspace member + Cargo.toml + placeholder lib.rs + rkyv_compat.rs.
3. `feat(execution): BundleConstructor + BundleCandidate + construct (E-1, E-2)`.
4. `test(execution): E-3 rkyv envelope round-trip`.
5. `feat(app): wire_phase4 + topology Option A broadcast tee + driver chain (W-1, W-2)`.
6. (optional) `chore(batch-p3-f): pick up fmt + Cargo.lock drift at batch close`.

After all commits + batch-close gates green: `docs(claude): mark Phase 3 COMPLETE` + `phase-3-complete` annotated tag (commits 7-8 if user explicitly approves the CLAUDE.md edit; otherwise tag-only per protocol).

## Phase 3 DoD audit (against ADR-001 line 43 revisit trigger + Phase 3 overview)

| Item | Status | Evidence |
|---|---|---|
| Captured event journaled | ✅ | P2-A ingress + P3-A rkyv derives + P3-B `wire_phase3` journal-drain + P3-F `wire_phase4` broadcast tee |
| Simulated profit signal | ✅ | P3-C heuristic + P3-E revm pipeline shim with explicit `ProfitSource::HeuristicPassthrough` (ADR-006 strict deferred to Phase 4 per user-approved 2026-05-04) |
| Bundle construction | ✅ | P3-F `crates/execution::BundleConstructor::construct` |
| Topology fanout | ✅ | P3-F `wire_phase4` Option A `tokio::sync::broadcast` with v0.2 fail-closed `Lagged` policy |
| Per-batch CLOSED | ✅ × 6 | overview/A/B/C/D/E/F |
| Spec-compliance derives on every payload | ✅ | P3-A + each new payload type (Opp, Risk, Sim, BundleCandidate) |
| No frozen-crate src edits beyond carve-outs | ✅ | additive-only across config/app per Phase 3 overview |
| No relay / submission / live mainnet / funded key | ✅ | grep empty across crates |
| AGENTS.md / .claude/ never staged | ✅ | `git ls-files` empty |
| No push / tag without user approval (Phase 2 baseline) | ✅ | Phase 3 routine push policy is user-authorized 2026-05-04 |

## Draft `phase-3-complete` annotated tag message

```text
phase-3-complete: Phase 3 thin-path pipeline + revm shim + bundle construction shipped

Phase 3 ships the thin-path end-to-end MEV pipeline per ADR-001 line 43
revisit-trigger ("captured event → simulated profit signal → bundle
construction by P3 end").

New crates (Phase 3):
  - rust-lmax-mev-opportunity   (UniV2 vs UniV3 0.05% Q64 arb math)
  - rust-lmax-mev-risk          (sizing + budget gate per docs/specs/risk-budget.md)
  - rust-lmax-mev-simulator     (revm LOCAL pipeline shim per DP-S1 + ProfitSource::HeuristicPassthrough)
  - rust-lmax-mev-execution     (BundleConstructor + BundleCandidate; no relay submission)

Touched (additive only): crates/types (no edit needed; EventSource was complete),
crates/config (JournalConfig.{ingress,state}_journal_path additive in P3-B),
crates/app (wire_phase3 in P3-B + wire_phase4 in P3-F).

Frozen since phase-2-complete: crates/{types, event-bus, journal, observability,
smoke-tests, node, ingress, state, replay} except for the P3-A spec-compliance
rkyv derive carve-out (additive only).

ADR-006 deferral: Phase 3 ships ProfitSource::HeuristicPassthrough simulation
with an in-tree STOP test bytecode + in-memory CacheDB. Phase 4 lands ADR-007
archive node + real Uniswap V2/V3 bytecode + state-fetcher and flips
ProfitSource → RevmComputed; MismatchCategory + relay sim comparator land
alongside BundleRelay. User-approved deferral on record 2026-05-04.

Topology Option A (tokio::sync::broadcast rebroadcast) with fail-closed
RecvError::Lagged policy implemented in P3-F wire_phase4 per P3-D
documented design + Codex 17:20:11 v0.2 obligation.

Test count: 105 workspace tests in CI (52 P1 + 6 P2-A + 6 P2-B + 5 P2-C +
2 P2-D + 6 P3-A + 2 P3-B runtime + 7 P3-C + 9 P3-D + 5 P3-E + 5 P3-F),
+1 ignored = crates/replay/tests/g_state_live.rs env-contract stub.

Phase 4 (relay submission + BundleRelay + funded-key + ADR-006 strict
revm + dynamic gas bidding) is the next phase per ADR-002 + ADR-006.
```

## Forbidden delta (only NEW)

- No `BundleRelay` impl, no `eth_sendBundle`, no relay submission, no live mainnet, no funded key, no signed transaction.
- No `MismatchCategory` enum (P4 alongside relay sim).
- No archive node integration (Phase 4 per ADR-007).
- No edits to `crates/event-bus` (Phase 1 freeze).
- No edits to `crates/{types, journal, observability, smoke-tests, node, ingress, state, replay}` source.
- All standing forbids carry over.

## Codex action

P3-F has no pre-impl review per Phase 3 overview. Routine policy applies: Claude commits + pushes the docs note, runs the 5-6 commit ladder, then emits the batch-close evidence pack with auto_check.md tail-summary refresh for Codex batch-close review. After APPROVED + push: per the routine-tag policy authorized 2026-05-04, Claude creates + pushes `phase-3-complete` annotated tag against the post-batch-close HEAD using the draft message above.
