# CLAUDE.md - Rust LMAX MEV Project

## Project Overview

Rust LMAX Disruptor-style MEV detection and execution engine for Ethereum mainnet. Solo developer + AI agents.

## Current Status

**Phase 0: COMPLETE** (git tag: `phase-0-complete`)
- 8 ADRs written and committed (`docs/adr/ADR-001` through `ADR-008`)
- 4 frozen spec docs written (`docs/specs/`)
- Documentation-only phase

**Phase 1: COMPLETE** (git tag: `phase-1-complete`)
- All Tasks 10–19 shipped via the Task 11–13 per-task pattern + Batch A (Foundation) + Batch B (App) + Batch C (Smoke tests + CI) + Batch D (final audit) lean-batching policy.
- 7 workspace crates: `types`, `event-bus`, `journal`, `config`, `observability`, `app`, `smoke-tests`.
- 52 workspace tests passing (event-bus 7 + journal 30 + types 4 + config 4 + observability 1 + app 3 + smoke-tests 3).
- CI: `.github/workflows/ci.yml` runs `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo deny check` on `ubuntu-latest`. ADR-008 checks 5+6+7 (bus 100k smoke, journal round-trip, snapshot smoke) exercised inside the test job.
- `deny.toml` v2 schema (cargo-deny 0.18+); RUSTSEC-2025-0141 (bincode 1.3 unmaintained) ignored per ADR-004 cold-path serializer choice.

**Phase 2: COMPLETE** (git tag: `phase-2-complete` at `b5ed4cd`, pushed to `origin`)
- All four batches CLOSED via the lean-batching policy:
  - P2-A node + ingress (`d9e7d48..9487cce`)
  - P2-B state engine (`9311d8d..310f6c7`)
  - P2-C replay + EXIT gates (`8f297ed..239ea86`)
  - P2-D `crates/app` producer wiring + final DoD audit + tag draft (`8192439..b5ed4cd`)
- 11 workspace crates: Phase 1 7 + new `node`, `ingress`, `state`, `replay`. `crates/config` and `crates/app` were the only previously-frozen crates touched (additive only).
- 71 workspace tests passing in CI (52 P1 baseline + 6 P2-A + 6 P2-B + 5 P2-C + 2 P2-D), plus 1 ignored live-smoke env-contract stub.
- ADR-001 Phase 2 EXIT gates passing in CI:
  - **Replay Gate** — `crates/replay/tests/g_replay.rs` byte-identical assertion across two runs.
  - **State Correctness Gate** — `crates/replay/tests/g_state.rs` + `g_pin.rs` (3 cases: non-Hash BlockId, unknown-hash, missing-fixture-no-witness).
- `master` and `phase-2-complete` tag pushed to `origin`.

**Phase 3: COMPLETE** (git tag: `phase-3-complete` at `e2a9c19`, pushed to `origin`)
- All six batches CLOSED via the lean-batching policy:
  - Phase 3 overview (`c755ccb`)
  - P3-A spec-compliance repair — additive `rkyv + serde` derives on `IngressEvent`/`MempoolEvent`/`BlockEvent`/`PoolState`/`StateUpdateEvent`/`PoolKind` via per-crate `rkyv_compat` adapters (`6e5de50..ae2fc59`)
  - P3-B `wire_phase3` + dual journal-drain consumer threads (`0933c2c..8dee524`)
  - P3-C `crates/opportunity` UniV2 vs UniV3 0.05% Q64 arb math (`4b6f798..a70b8a2`)
  - P3-D `crates/risk` sizing + budget gate per `docs/specs/risk-budget.md` + topology Option A design doc (`abc5bbc..33370ed`)
  - P3-E `crates/simulator` revm LOCAL pre-sim shim per DP-S1 (`65da50e..f9560c0`)
  - P3-F `crates/execution` bundle construction + `wire_phase4` final wiring (`38d14da..e2a9c19`)
- 15 workspace crates: Phase 2 11 + new `opportunity`, `risk`, `simulator`, `execution`. Existing Phase 1/2 crates touched only via the spec-compliance carve-out (P3-A additive rkyv derives) and `crates/app` `wire_phase4` additive constructor; `wire`/`wire_phase2`/`wire_phase3` stay byte-identical.
- 107 workspace tests passing in CI (52 P1 baseline + 6 P2-A + 6 P2-B + 5 P2-C + 2 P2-D + 6 P3-A + 2 P3-B runtime + 7 P3-C + 9 P3-D + 5 P3-E + 7 P3-F), plus 1 ignored live-smoke env-contract stub.
- ADR-001 line 43 revisit-trigger conditions ALL satisfied: captured event journaled (P2-A + P3-A + P3-B + P3-F broadcast tee) → simulated profit signal (P3-C heuristic + P3-E revm shim) → bundle construction (P3-F).
- **ADR-006 deferral on permanent record**: P3-E ships a deterministic revm pipeline shim (in-tree STOP test bytecode + in-memory `CacheDB`) with `simulated_profit_wei` heuristic-passthrough from upstream, stamped `ProfitSource::HeuristicPassthrough`. Phase 4 lands ADR-007 archive node + real Uniswap V2/V3 bytecode + state-fetcher and flips `ProfitSource` → `RevmComputed`; `MismatchCategory` + relay sim comparator land alongside `BundleRelay`. User-approved deferral 2026-05-04.
- Topology Option A (`tokio::sync::broadcast` rebroadcast) implemented in P3-F `wire_phase4` with the v0.2 fail-closed `RecvError::Lagged` policy from P3-D documented design (W-2 test asserts the consumer task exits within 2s on `Lagged`).
- `master` and `phase-3-complete` tag pushed to `origin` (tag object `7298660`).

**Phase 4: COMPLETE** (git tag: `phase-4-complete` at `e5f13ea`, pushed to `origin`)
- All seven batches CLOSED via the lean-batching policy (P4-A archive node integration; P4-B `crates/state-fetcher`; P4-C real revm against current state; P4-D `crates/relay-sim` comparator + `MismatchCategory`; P4-E `crates/bundle-relay` + `crates/relay-clients` Flashbots + bloXroute adapters with `submit_bundle` returning `Err(SubmitDisabled)` unconditionally + comparator_driver wired with mismatch journal append-before-broadcast; P4-F `MempoolSourceKind` runtime selector + `ExternalMempoolSource` fail-closed; P4-G final wiring + DoD audit + tag).
- 19 workspace crates: Phase 3 15 + new `state-fetcher`, `relay-sim`, `bundle-relay`, `relay-clients`.
- 206 workspace tests passing in CI + 1 ignored live-smoke env-contract stub.
- Hard P4 invariants: `submit_bundle` returns `Err(SubmitDisabled)` unconditionally in every adapter; `crates/app` holds adapters as `Arc<dyn RelaySimulator>` only (DP-E13 type-system upcast prevention); zero `submit_bundle(` call sites in `crates/app/src/`; `live_send=true` config-validation rejected; multi-relay rejected; mismatch journal append+flush BEFORE broadcast (R-E9); full secret redaction at relay error rendering (R-E20).
- `ProfitSource::RevmComputed` flipped from `HeuristicPassthrough` (P4-C2 SR-1 + FP-1 + T-USDC-1 fixtures).
- Production `LocalSimulator::prefetch_for` integration deferred to Phase 5 P5-A per the P4-G fail-closed boundary.
- `master` and `phase-4-complete` tag pushed to `origin`.

**Phase 5: COMPLETE** (git tag: `phase-5-complete` at `55679a4`, pushed to `origin`)
- Safety Gate scope: planning + design + fail-closed wiring; **NO live action** per the user-approved Phase 5 overview.
- All five batches CLOSED via the lean-batching policy:
  - Phase 5 overview (`ac07024`)
  - P5-A `LocalSimulator::prefetch_for` shadow-mode wiring — archive-RPC-gated, fail-closed when archive unconfigured/stale, per-block fixture cache, interior-mutability redesign of `LocalSimulator` (`0c28d5c`).
  - P5-B `crates/execution` dynamic gas bidding strategy infrastructure — `BidStrategy` trait + `FixedFractionBidStrategy` + `Eip1559BasefeeAwareBidStrategy` + `BundleConstructor::with_strategy(...)` explicit-strategy ctor per ADR-006 §"Gas bidding" Phase 5+ unlock; **no ADR text amendment** per Q-P5-7 / R-P5-3 (`84283d9`).
  - P5-C boundary-only `crates/signer` workspace member — `Signer` trait + `DisabledSigner` fail-closed impl + `SignerError::SignerDisabled` whose `Display` pins the literal `"Phase 6b Production Gate"`; Phase 4 G2 zero-hit `Signer` grep promoted to four-gate G2a/G2b/G2c/G2d redefined gate per R-P5-2 (`db6a7b8`).
  - P5-D `KillSwitch` (`Arc<AtomicBool>` newtype) in `crates/bundle-relay` + `BundleRelayError::KillSwitchActive` variant + `AppHandle4::{kill_switch(), set_execution_disabled(bool)}` surface + `comparator_driver` per-driver guard (top-of-iteration `kill_switch.is_active()` short-circuit). G9 leak gate added. Per-adapter wiring deferred to Phase 6 per Q-P5-5 / DP-D8 (`e3e11bb`).
  - P5-E final wiring + Phase 5 DoD audit + `phase-5-complete` annotated tag (`55679a4`).
- 20 workspace crates: Phase 4 19 + new `signer`.
- 231 workspace tests passing in CI + 1 ignored live-smoke env-contract stub (P2-C carry-forward).
- Hard P5 invariants HSI-1..11 verified at the `phase-5-complete` tag: NO production signer / NO funded key / NO key material / NO `secp256k1|k256|alloy-signer|ethers-signers|Wallet|PrivateKey|sign_transaction|funded` symbols anywhere in `crates/` (G2a + G2b + G2d zero-hit); NO `eth_sendBundle` runtime call (G1 only doc/`//!` "NO" assertions); NO `live_send=true` capability (config-validation reject preserved); NO actual relay submission (`submit_bundle` returns `Err(SubmitDisabled)` unchanged; G3 + G4 zero hits in `crates/app/src/`); NO new live-network surface (P5-A `prefetch_for` opt-in via `config.simulator.prefetch_enabled` default `false` AND archive-RPC-gated AND fail-closed); NO ADR text amendment; NO asset/venue/V3 fee-tier widening.
- `crates/signer` is an isolated leaf crate — `crates/app` does NOT depend on it; the signer is reachable only by direct unit tests in `crates/signer/`. Phase 6b Production Gate is the only path to enabling real signing.
- `master` and `phase-5-complete` tag pushed to `origin`.

**Phase 6a: COMPLETE** (git tag: `phase-6a-complete` at `bd0a53c`, pushed to `origin`; tag object `3c9faaf`)
- Pre-Production Gate scope: docs + fail-closed wiring + tests; **NO live action** per the Phase 6 overview's "Phase 6b Production Gate (kept entirely separate)" carve-out.
- All six batches CLOSED via the lean-batching policy:
  - P6-A overview + boundary spec at `docs/specs/phase-6a-boundary.md` (`4c4c0dd` / `64ffaee` / `19e263a` / `a7367b7`).
  - P6-B signing-request pipeline fail-closed — `BundleConstructor::with_signer(Arc<dyn Signer>)` + `crates/execution::invoke_signer_for_test` `#[cfg(test)] pub(crate)` hook + `wire_phase4` `Arc::new(DisabledSigner::default())` injection (`9a6ebd2` / `b27d01a` / `a7367b7`). G2c allow-list expanded by THREE file entries (`crates/execution/src/lib.rs`, `crates/app/src/lib.rs`, `crates/app/tests/wire_phase4.rs`). G2e dep-edge count flipped 0 → 2 (`crates/execution/Cargo.toml` + `crates/app/Cargo.toml`).
  - P6-C wiremock body-shape tests for `eth_callBundle` on Flashbots + bloXroute adapters with synthetic placeholder bytes (NOT RLP / NOT key-material-derived) (`07a0256` / `93803b2`). Workspace 233 → 235.
  - P6-D per-adapter kill-switch wiring — `KillSwitch` field added to `FlashbotsRelay` + `BloxrouteRelay`; ctor sig changed to `::new(cfg, kill_switch: KillSwitch)` (BREAKING; 17 in-crate + 2 `crates/app/src/lib.rs::build_relay_sim_from_config` callsites updated); two-line first-statement guard added to each `submit_bundle` body. G10 promoted DOCUMENTED → ENFORCED. G9 allow-list extended to also include `crates/relay-clients/`. Lean 4-test matrix per user override (D-T-D1..D-T-D4: Flashbots + bloXroute PRECEDENCE, Flashbots shared-state, bloXroute inactive-baseline) (`98ab10c` / `e8ca1c5` / `d88693b`). Workspace 235 → 239.
  - P6-E production-signer design doc — new `docs/specs/production-signer.md` (119 lines, ASCII-only) capturing the Phase 6b unlock contract: HSM/KMS-only key custody, never-in-memory key material (necessary-but-not-sufficient framing on the `BundleTx → Result<SignedTxBytes, SignerError>` trait shape; HSM/KMS custody + Phase 6b impl review enforce), positive auditability requirement (correlation to existing opportunity/bundle chain + no key material in the event; exact event shape Phase 6b-locked), key rotation + lifecycle, threat model with host-compromise-requested malicious-signing residual + Phase 6b control point. Single-line cross-reference appended to `docs/specs/execution-safety.md` (`8511b02` / `e0ec00f`). No `crates/` change. No new impl.
  - P6-F final DoD audit + `phase-6a-complete` annotated tag (`bd0a53c`; tag object `3c9faaf`). Full `cargo fmt --check` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test --workspace` (239 + 1 ignored) / `cargo deny check` / `cargo tree -d` set re-verified at the post-plan-commit HEAD.
- 20 workspace crates: Phase 5 20 unchanged. Phase 6a added 1 new spec doc (`docs/specs/production-signer.md`), 4 new tests in `crates/relay-clients/tests/submit_disabled.rs` + `crates/relay-clients/src/bloxroute.rs` `#[cfg(test)]`, and the signer dep edges from `crates/execution` + `crates/app` into `crates/signer`. No new workspace member.
- 239 workspace tests passing + 1 ignored live-smoke env-contract stub (P2-C carry-forward).
- Hard P6a invariants HSI-1..11 verified at `phase-6a-complete`: NO production signer / NO funded key / NO key material / NO `secp256k1|k256|alloy-signer|ethers-signers|Wallet|PrivateKey|sign_transaction|funded` symbols anywhere in `crates/` (G2a + G2b + G2d zero-hit; G2c 5-`crates/signer/` + 3-approved-file allow-list; G2e dep-edge count = 2); NO `eth_sendBundle` runtime call (G1 only doc/`//!` "NO" assertions, 5 hits); NO `live_send=true` capability (config-validation reject preserved with `"relay.live_send=true is forbidden until Phase 6b Production Gate"` Display literal); NO actual relay submission (`submit_bundle` returns `Err(KillSwitchActive)` or `Err(SubmitDisabled)` per the PRECEDENCE; G3 + G4 zero hits in `crates/app/src/`); per-adapter G10 enforced (each `submit_bundle` body's FIRST non-trivia statement is the kill-switch guard); single G11 `sign_tx` production call site at `crates/execution/src/lib.rs:238` routed through `&dyn Signer`; NO ADR text amendment; NO asset/venue/V3 fee-tier widening.
- `crates/signer` reachable from production code only via the P6-B `BundleConstructor::with_signer` boundary; the only impl reachable in Phase 6a is `DisabledSigner`. The `SignerError::SignerDisabled` `Display` literal `"Phase 6b Production Gate"` is the canonical forward-link from runtime code to the gate that would unlock real signing.
- `master` and `phase-6a-complete` tag pushed to `origin`.

**Phase 6b: NOT STARTED** — Production Gate (funded key / production signer impl / `live_send=true` capability / actual `eth_sendBundle` / actual relay submission). Phase 6b requires ALL of: (1) fresh explicit user authorization lifting the Phase 6 overview's Phase 6b non-goals; (2) a Phase 6b overview document under `docs/superpowers/plans/`; (3) a separate Codex review against `docs/specs/production-signer.md` Section 2 contract; (4) at least one non-trivial host-compromise control per `docs/specs/production-signer.md` Section 2.5 residual; (5) a Phase 6b boundary document under `docs/specs/` (separate from `phase-6a-boundary.md`); (6) an ADR-001 amendment user-authorized to lift the funded-key / prod-signer ban for the scoped Phase 6b context. Until ALL six are satisfied, the workspace at `phase-6a-complete` remains the fail-closed baseline. Wait for explicit user prompt to begin.

## Resume Instructions

1. Read `.coordination/task_state.md`, `.coordination/claude_outbox.md`, and `.coordination/codex_review.md` first; they describe the current gate and live handoff state.
2. Phase 1/2/3/4/5/6a closed at `phase-1-complete`/`phase-2-complete`/`phase-3-complete`/`phase-4-complete`/`phase-5-complete`/`phase-6a-complete`. Do not re-open frozen Phase 1 / P2-A..D / P3 / P4 / P5 / P6-A..F crates or specs without an ADR/spec change. The P3-E ADR-006 deferral, P4-G `prefetch_for` deferral, Phase 5 Safety Gate scope (NO live action), and Phase 6a Pre-Production Gate scope (signer routing fail-closed + per-adapter kill-switch + design-doc-only production signer) are documented in their respective phase tag annotations.
3. **Phase 6b is NOT STARTED** and is the ONLY path to live action per ADR-001 + `docs/specs/execution-safety.md` + `docs/specs/production-signer.md`. Phase 6b requires ALL SIX prerequisites enumerated in the "Phase 6b: NOT STARTED" status block above (fresh explicit user authorization + Phase 6b overview + Codex review against `production-signer.md` Section 2 + at least one host-compromise control per Section 2.5 residual + Phase 6b boundary doc + ADR-001 amendment). Do NOT draft a Phase 6b plan unprompted; wait for the user to explicitly authorize each prerequisite, in order.
4. Use `superpowers:subagent-driven-development` for any future Phase 6b implementation work once a Phase 6b plan is user-approved.

## Key Decisions (frozen in ADRs)

- **Approach:** Vertical Slice - Phase 1-3 thin e2e path, Phase 4-6 widen/harden
- **Stack:** alloy, revm, tokio, rkyv(hot)/bincode(cold), RocksDB, crossbeam bounded
- **Thin Path:** Ethereum mainnet, WETH/USDC, Uniswap V2+V3 0.05%, shadow-only through Phase 3
- **EventBus:** Single-consumer bounded queue (Phase 1), multi-consumer deferred to Phase 2+
- **Pipeline (Phase 3):** 6-stage with PipelineOutcome<T> generic immutable pattern
- **Config:** TOML, primary node Geth, fallback RPC 1+

## Task Checklist (Phase 6a — all CLOSED)

- [x] P6-A: Phase 6 overview (`c08db38`) + Phase 6a safety boundary refinement at `docs/specs/phase-6a-boundary.md` (`4c4c0dd` / `64ffaee` / `19e263a` / `a7367b7`) — three safety devices (`submit_bundle`, `Signer`, `KillSwitch`), §3 PRECEDENCE rule, §4 G1..G11 verbatim grep gates, §5 hard forbids, §6 Phase 6b boundary kept explicit.
- [x] P6-B: signing-request pipeline fail-closed — `BundleConstructor::with_signer(Arc<dyn Signer>)` + `#[cfg(test)] pub(crate) async fn invoke_signer_for_test` hook in `crates/execution` + `wire_phase4` `Arc::new(DisabledSigner::default())` injection. G2c allow-list expanded by 3 file entries; G2e dep edges 0→2 (`crates/execution` + `crates/app` → `crates/signer`). `DisabledSigner` returns `Err(SignerError::SignerDisabled)` with Display literal `"Phase 6b Production Gate"` (`9a6ebd2` / `b27d01a` / `a7367b7`).
- [x] P6-C: wiremock-only adapter tests for `eth_callBundle` body-shape on Flashbots + bloXroute with synthetic placeholder bytes (NOT RLP / NOT key-material). `rc_f_6_*` + `rc_b_6_*` integration tests assert exact `serde_json::Value` equality of the JSON-RPC envelope including `blockNumber` / `stateBlockNumber` = `0x14fb180` (`07a0256` / `93803b2`). Workspace 233 → 235.
- [x] P6-D: per-adapter kill-switch wiring (G10 enforcement). `KillSwitch` field on `FlashbotsRelay` + `BloxrouteRelay`; ctor sig BREAKING `::new(cfg, kill_switch: KillSwitch)`; two-line first-statement guard `if self.kill_switch.is_active() { return Err(BundleRelayError::KillSwitchActive); }` BEFORE `Err(SubmitDisabled)`. `crates/app/src/lib.rs::build_relay_sim_from_config` threads `kill_switch.clone()` from the `AppHandle4`-owned instance into both adapter ctors; G3 + G4 preserved at 0. Lean 4-test matrix per user override: D-T-D1/D-T-D2 (Flashbots + bloXroute PRECEDENCE), D-T-D3 (Flashbots shared-state clone-and-flip), D-T-D4 (bloXroute inactive-baseline regression) (`98ab10c` / `e8ca1c5` / `d88693b`). Workspace 235 → 239.
- [x] P6-E: production-signer design doc — new `docs/specs/production-signer.md` (119 lines, ASCII-only) capturing the Phase 6b unlock contract; single-line cross-reference appended to `docs/specs/execution-safety.md` (`8511b02` / `e0ec00f`). NO `crates/` change; NO new impl.
- [x] P6-F: Phase 6a DoD audit + `phase-6a-complete` annotated tag (`bd0a53c`; tag object `3c9faaf`). HSI-1..HSI-11 verified verbatim in `.coordination/claude_outbox.md` P6-F closeout; workspace `cargo fmt --check` / `cargo clippy -D warnings` / `cargo test --workspace` (239 + 1 ignored) / `cargo deny check` / `cargo tree -d` all PASS.

## Task Checklist (Phase 3 — all CLOSED)

- [x] P3-A: spec-compliance repair — additive `rkyv + serde` derives on `IngressEvent`/`MempoolEvent`/`BlockEvent`/`PoolState`/`StateUpdateEvent`/`PoolKind` per `docs/specs/event-model.md` mandate; per-crate `rkyv_compat` adapters for alloy-primitives types.
- [x] P3-B: `wire_phase3` + dual journal-drain consumer threads (`FileJournal<IngressEvent>` + `FileJournal<StateUpdateEvent>`); async `AppHandle3::shutdown` with `producer_task.abort(); .await` BEFORE bus drop / consumer join.
- [x] P3-C: `crates/opportunity` UniV2 vs UniV3 0.05% Q64 cross-venue arb math; `OpportunityEngine::check` pure function emits `OpportunityEvent` iff price delta exceeds gas-floor threshold.
- [x] P3-D: `crates/risk` sizing + budget gate per `docs/specs/risk-budget.md`; 6-variant `AbortCategory`; topology Option A (`tokio::sync::broadcast` with v0.2 fail-closed `RecvError::Lagged`) documented for P3-F implementation.
- [x] P3-E: `crates/simulator` revm LOCAL pre-sim shim (DP-S1) per user-approved ADR-006 deferral; `LocalSimulator` deterministic pipeline + `ProfitSource::HeuristicPassthrough` provenance.
- [x] P3-F: `crates/execution` pure-function `BundleConstructor` (intent-only; no signing/submission) + `wire_phase4` final wiring with topology Option A broadcast tee + the full opportunity → risk → simulator → execution driver chain + Phase 3 DoD audit + `phase-3-complete` annotated tag.

## Task Checklist (Phase 2 — all CLOSED)

- [x] P2-A: `crates/node` + `crates/ingress` (NodeProvider WS+HTTP+fallback per ADR-007; MempoolSource trait + GethWsMempool per ADR-003).
- [x] P2-B: `crates/state` (UniV2 + UniV3 0.05% reserves snapshot, block-hash-pinned `eth_call_at_block`, persisted to `RocksDbSnapshot`).
- [x] P2-C: `crates/replay` (Replayer trait + StateReplayer + RecordedEthCaller; G-Replay + G-State + G-Pin EXIT gate tests + ignored live smoke).
- [x] P2-D: `crates/app` producer-side wiring (`wire_phase2` + `AppHandle2` + `AppError::Node|State`), final DoD audit, `phase-2-complete` annotated tag at `b5ed4cd`.

## Task Checklist (Phase 1)

- [x] Task 10: Workspace scaffold (Cargo.toml, configs)
- [x] Task 11: crates/types (EventEnvelope<T>, primitives, events, error)
- [x] Task 12: crates/event-bus (EventBus trait, CrossbeamBoundedBus)
- [x] Task 13: crates/journal (FileJournal, RocksDbSnapshot)
- [x] Task 14: crates/config (TOML loading, env overlay, BusConfig)
- [x] Task 15: crates/observability (tracing, Prometheus)
- [x] Task 16: crates/app (binary entrypoint, wiring, AppError, integration tests)
- [x] Task 17: Integration smoke tests (100k bus + backpressure, journal round-trip, snapshot)
- [x] Task 18: CI pipeline (.github/workflows/ci.yml + deny.toml v2)
- [x] Task 19: Final verification + phase-1-complete tag

## Important Notes for AI Agents

- rkyv 0.8 has breaking API changes from 0.7. If derives do not work with alloy-primitives, use `[u8; N]` field types or fall back to bincode-only for Phase 1.
- `consumed_total` metric must be shared via `Arc<AtomicU64>` between bus and consumer.
- Event-bus emits three counters (`event_bus_published_total`, `event_bus_consumed_total`, `event_bus_backpressure_total`) plus one gauge (`event_bus_current_depth`). Journal and snapshot emit counters only (no gauges). All emit through the `metrics` facade for Prometheus export per ADR-008.
- Backpressure test must be fully implemented (not stub).
- Config crate needs `tempfile = "3"` in dev-dependencies.
- Task 13 uses `rocksdb = { workspace = true }` only when the approved implementation plan reaches the `RocksDbSnapshot` task. Do not add it during earlier journal tasks.
- `clang`, `LIBCLANG_PATH`, and `libclang.dll` must be available before any build that activates the RocksDB dependency.

## File Structure Reference

```text
docs/adr/          # 8 ADRs (frozen)
docs/specs/        # 4 spec docs (frozen)
docs/superpowers/  # task specs and plans
config/            # base/dev/test TOML configs
crates/            # Rust workspace members
```

## Agent Coordination Protocol

The `.coordination/` directory is the file-based handoff channel between Claude (implementer) and Codex (reviewer). When that directory exists, the following rules apply project-wide:

- Repo files are the source of truth; Claude's per-conversation memory is only a hint and must not override the repo when the two disagree.
- Claude writes task reports, questions, and review requests to `.coordination/claude_outbox.md`, not chat alone.
- Codex reviews `.coordination/claude_outbox.md`, `.coordination/auto_check.md`, the working-tree diff, and relevant source/spec files, then writes verdicts to `.coordination/codex_review.md`.
- Watcher output in `.coordination/auto_check.md` is mechanical verification only and is not approval.
- The API reviewer watcher is a coordination/gate reviewer, not a full code reviewer. It may approve routine in-flight gates when its context includes sufficient repo evidence; high-risk implementation review may still need manual Codex review.
- Start or restart the API reviewer watcher with `.coordination/scripts/start_codex_api_reviewer.ps1` (use `-Restart` to replace an existing watcher). Defaults: 180s poll, 600s reviewer timeout, `gpt-5.5`, reasoning `medium`.
- `AGENTS.md` and `.claude/` are never staged.
- Per the 2026-05-04 routine-closeout policy update, Codex APPROVED + an execution-note-documented target/scope is sufficient authorization for routine docs/plan/implementation commits, `git push origin master`, `phase-complete` annotated tag creation + push, `CLAUDE.md` phase wrap-up commits, and coordination-file updates. No user re-confirmation needed for those routine actions.
- User explicit approval IS still required for: destructive git operations (force push / reset / rebase), branch or remote changes, ADR/scope/frozen-decision changes, live trading / relay submission / funded key / `live_send = true`, `.claude/` or `AGENTS.md` staging, Codex `REVISION REQUIRED` or `LOW` confidence outcomes, and the scope-defining first start of any new phase.
- Normal workflow: Claude writes the current bounded report/request to `.coordination/claude_outbox.md`; Codex/API reviewer writes a verdict to `.coordination/codex_review.md`; Claude follows that verdict. Keep both files compact and live-state oriented.
