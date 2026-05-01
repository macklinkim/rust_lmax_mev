# CLAUDE.md — Rust LMAX MEV Project

## Project Overview

Rust LMAX Disruptor-style MEV detection and execution engine for Ethereum mainnet. Solo developer + AI agents.

## Current Status

**Phase 0: COMPLETE** (git tag: `phase-0-complete`)
- 8 ADRs written and committed (`docs/adr/ADR-001` ~ `ADR-008`)
- 4 frozen spec docs written (`docs/specs/`)
- No code — documentation only phase

**Phase 1: IN PROGRESS** — Task 13 (crates/journal) next
- Tasks 10, 11, 12 complete: workspace scaffold, `crates/types`, and `crates/event-bus` all merged on master.
- Tasks 13-19 remain (4 crates + smoke tests + CI + phase-1-complete tag).

## Resume Instructions

1. Read the Phase 0 plan/spec for context if needed: `docs/superpowers/specs/2026-04-24-phase-0-6-plan-design.md`, `docs/superpowers/plans/2026-04-24-phase-0-1-implementation.md`.
2. For per-task references, the Task 11 + Task 12 design specs and implementation plans live under `docs/superpowers/specs/` and `docs/superpowers/plans/` (`YYYY-MM-DD-task-N-<crate>-{design,implementation}.md`). Mirror the same naming for Task 13.
3. Resume from **Task 13: crates/journal** — draft a design spec, get user approval, draft the implementation plan, get user approval, then execute.
4. Use `superpowers:subagent-driven-development` skill to execute the Task 13 plan once both spec and plan are user-approved.
5. Tasks 14-19 follow sequentially after Task 13.

## Key Decisions (frozen in ADRs)

- **Approach:** Vertical Slice — Phase 1-3 thin e2e path, Phase 4-6 widen/harden
- **Stack:** alloy, revm, tokio, rkyv(hot)/bincode(cold), RocksDB, crossbeam bounded
- **Thin Path:** Ethereum mainnet, WETH/USDC, Uniswap V2+V3 0.05%, shadow-only through Phase 3
- **EventBus:** Single-consumer bounded queue (Phase 1), multi-consumer deferred to Phase 2+
- **Pipeline (Phase 3):** 6-stage with PipelineOutcome<T> generic immutable pattern
- **Config:** TOML, primary node Geth, fallback RPC 1+

## Task Checklist (Phase 1)

- [x] Task 10: Workspace scaffold (Cargo.toml, configs)
- [x] Task 11: crates/types (EventEnvelope<T>, primitives, events, error)
- [x] Task 12: crates/event-bus (EventBus trait, CrossbeamBoundedBus)
- [ ] Task 13: crates/journal (FileJournal, RocksDbSnapshot)
- [ ] Task 14: crates/config (TOML loading, env overlay)
- [ ] Task 15: crates/observability (tracing, Prometheus)
- [ ] Task 16: crates/app (binary entrypoint, wiring)
- [ ] Task 17: Integration smoke tests (100k bus, journal round-trip, snapshot)
- [ ] Task 18: CI pipeline (.github/workflows/ci.yml)
- [ ] Task 19: Final verification + phase-1-complete tag

## Important Notes for AI Agents

- rkyv 0.8 has breaking API changes from 0.7. If derives don't work with alloy-primitives, use [u8; N] field types or fall back to bincode-only for Phase 1.
- consumed_total metric must be shared via Arc<AtomicU64> between bus and consumer.
- Bus/journal must register metrics with `metrics::counter!`/`metrics::gauge!` macros for Prometheus export.
- Backpressure test must be fully implemented (not stub).
- Config crate needs `tempfile = "3"` in dev-dependencies.

## File Structure Reference

```
docs/adr/          # 8 ADRs (frozen)
docs/specs/        # 4 spec docs (frozen)
docs/superpowers/  # spec + plan documents
config/            # base/dev/test TOML configs
crates/            # Rust workspace members (6 crates)
```
