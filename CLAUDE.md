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

**Phase 2: NOT STARTED** — mempool ingestion + replay hooks + simulation per ADR-001 vertical-slice ordering. Wait for explicit user prompt to begin.

## Resume Instructions

1. Read `.coordination/task_state.md`, `.coordination/claude_outbox.md`, and `.coordination/codex_review.md` first; they describe the current gate and live handoff state.
2. Phase 1 is closed at `phase-1-complete`; do not re-open frozen Phase 1 crates without an ADR/spec change.
3. To begin Phase 2 work: draft a Phase 2 plan/design under `docs/superpowers/specs/` and `docs/superpowers/plans/` mirroring the Phase 1 lean-batching pattern (ADR-001 vertical-slice ordering: mempool ingestion, replay hooks, simulation).
4. Use `superpowers:subagent-driven-development` for Phase 2 implementation work once a plan is user-approved.

## Key Decisions (frozen in ADRs)

- **Approach:** Vertical Slice - Phase 1-3 thin e2e path, Phase 4-6 widen/harden
- **Stack:** alloy, revm, tokio, rkyv(hot)/bincode(cold), RocksDB, crossbeam bounded
- **Thin Path:** Ethereum mainnet, WETH/USDC, Uniswap V2+V3 0.05%, shadow-only through Phase 3
- **EventBus:** Single-consumer bounded queue (Phase 1), multi-consumer deferred to Phase 2+
- **Pipeline (Phase 3):** 6-stage with PipelineOutcome<T> generic immutable pattern
- **Config:** TOML, primary node Geth, fallback RPC 1+

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
- No `git push` and no tag creation without explicit user approval.
- User explicit approval is still required for fundamental scope changes, backend swaps, `CLAUDE.md` commits, `AGENTS.md` staging, pushes, and tags.
- Codex approval is sufficient for routine in-flight gates already covered by the approved spec/plan: spec/ADR doc commits, workspace/scaffold commits, plan commits, and implementation commits per the approved plan.
- Normal workflow: Claude writes the current bounded report/request to `.coordination/claude_outbox.md`; Codex/API reviewer writes a verdict to `.coordination/codex_review.md`; Claude follows that verdict. Keep both files compact and live-state oriented.
