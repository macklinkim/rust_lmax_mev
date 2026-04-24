# ADR-001: Vertical Slice + Replay Hooks + Gate Policy

**Date:** 2026-04-24
**Status:** Accepted

## Context

The project must choose an overall development strategy for building a Rust LMAX-style MEV engine. Two broad approaches were considered:

- **Approach A (Vertical Slice + Replay Hooks):** Build a thin end-to-end path first across all architectural layers, then widen and harden incrementally. Mandatory quality gates block phase transitions.
- **Approach B (Horizontal Layer-by-Layer):** Build and stabilize each horizontal layer in isolation before integrating upward.

Phase 0 is documentation-only. Phases 1–6 cover implementation, testing, and production hardening. A gate policy is needed to ensure correctness is verified before scope expands.

## Decision

Adopt Approach A: Vertical Slice + Replay Hooks.

Phase structure:
- **P0:** Documentation and architecture freeze (current phase).
- **P1–P3:** Build the thin end-to-end path (single strategy, single pool pair, shadow mode).
- **P4–P6:** Widen scope and harden for production.

Four mandatory gates:
1. **Replay Gate** — must pass at P2 exit: replay of captured events produces deterministic output.
2. **State Correctness Gate** — must pass at P2 exit: simulated state matches observed on-chain state within tolerance.
3. **Safety Gate** — must pass at P5 exit: all abort paths exercised, no unhandled panic in stress run.
4. **Production Gate** — must pass at P6a exit: latency, reliability, and profit thresholds met in shadow production run.

Phase dependency chain: P0 → P1 → P2 → [Replay Gate + State Correctness Gate] → P3 → P4 → P5 → [Safety Gate] → P6a → [Production Gate] → P6b.

No phase may begin until all gates for the preceding checkpoint have been signed off.

## Rationale

- Vertical slices surface integration risk early. Horizontal layering delays integration until late, when rework is most expensive.
- Replay hooks are required for deterministic testing of a live-data system; baking them in from P1 avoids retrofitting.
- Mandatory gates prevent "works in isolation" from masquerading as production-ready.
- The gate policy is explicit and binary — pass/fail — to eliminate subjective phase-exit criteria.

## Revisit Trigger

The thin path approach fails to produce end-to-end results (captured event → simulated profit signal → bundle construction) by the end of Phase 3.

## Consequences

- Phase 1 must instrument replay hooks from day one, not as an afterthought.
- Scope expansion (new pools, new strategies) is blocked until P4, even if P1–P3 complete ahead of schedule.
- Gate failures halt the phase queue; the team must resolve blockers before proceeding.
- Approach B (horizontal layers) is off the table unless the revisit trigger fires.
