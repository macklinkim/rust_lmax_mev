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

**Phase 4: NOT STARTED** — relay submission + `BundleRelay` + funded-key + ADR-006 strict revm against current state snapshot + ADR-007 archive node integration + dynamic gas bidding (Phase 5+ per ADR-006). Wait for explicit user prompt to begin.

## Resume Instructions

1. Read `.coordination/task_state.md`, `.coordination/claude_outbox.md`, and `.coordination/codex_review.md` first; they describe the current gate and live handoff state.
2. Phase 1/2/3 closed at `phase-1-complete`/`phase-2-complete`/`phase-3-complete`. Do not re-open frozen Phase 1 / P2-A / P2-B / P2-C / P3 crates without an ADR/spec change. The P3-E ADR-006 deferral is documented in the `phase-3-complete` tag annotation; Phase 4 plan must reference that deferral and explicitly land the strict-revm requirement.
3. To begin Phase 4 work: draft a Phase 4 plan under `docs/superpowers/plans/` mirroring the Phase 3 lean-batching pattern. Phase 4 owns ADR-007 archive node integration, real Uniswap V2/V3 bytecode + state-fetcher (flipping `ProfitSource::HeuristicPassthrough` → `RevmComputed`), `MismatchCategory` enum + relay sim comparator alongside `BundleRelay`, funded-key + signing infrastructure (gated by Safety Gate per ADR-001), and Sushiswap WETH/USDC inclusion per ADR-002 Phase 4 unlock.
4. Use `superpowers:subagent-driven-development` for Phase 4 implementation work once a plan is user-approved.

## Key Decisions (frozen in ADRs)

- **Approach:** Vertical Slice - Phase 1-3 thin e2e path, Phase 4-6 widen/harden
- **Stack:** alloy, revm, tokio, rkyv(hot)/bincode(cold), RocksDB, crossbeam bounded
- **Thin Path:** Ethereum mainnet, WETH/USDC, Uniswap V2+V3 0.05%, shadow-only through Phase 3
- **EventBus:** Single-consumer bounded queue (Phase 1), multi-consumer deferred to Phase 2+
- **Pipeline (Phase 3):** 6-stage with PipelineOutcome<T> generic immutable pattern
- **Config:** TOML, primary node Geth, fallback RPC 1+

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
