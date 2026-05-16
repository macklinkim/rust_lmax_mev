# ADR-001: Vertical Slice + Replay Hooks + Gate Policy

**Date:** 2026-04-24
**Status:** Accepted

## Context

The project must choose an overall development strategy for building a Rust LMAX-style MEV engine. Two broad approaches were considered:

- **Approach A (Vertical Slice + Replay Hooks):** Build a thin end-to-end path first across all architectural layers, then widen and harden incrementally. Mandatory quality gates block phase transitions.
- **Approach B (Horizontal Layer-by-Layer):** Build and stabilize each horizontal layer in isolation before integrating upward.

Phase 0 is documentation-only. Phases 1--6 cover implementation, testing, and production hardening. A gate policy is needed to ensure correctness is verified before scope expands.

## Decision

Adopt Approach A: Vertical Slice + Replay Hooks.

Phase structure:
- **P0:** Documentation and architecture freeze (current phase).
- **P1--P3:** Build the thin end-to-end path (single strategy, single pool pair, shadow mode).
- **P4--P6:** Widen scope and harden for production.

Four mandatory gates:
1. **Replay Gate** -- must pass at P2 exit: replay of captured events produces deterministic output.
2. **State Correctness Gate** -- must pass at P2 exit: simulated state matches observed on-chain state within tolerance.
3. **Safety Gate** -- must pass at P5 exit: all abort paths exercised, no unhandled panic in stress run.
4. **Production Gate** -- two-stage gate per the post-Phase-6a project structure:
   - **Phase 6a Pre-Production Gate** (CLOSED at `phase-6a-complete` annotated tag, commit `bd0a53c`, 2026-05-16). Latency, reliability, and fail-closed safety thresholds met in shadow + comparator runs; per-adapter kill-switch enforced (G10); signer routing fail-closed (G11). See `docs/specs/phase-6a-boundary.md`.
   - **Phase 6b Production Gate** (NOT STARTED). Live-action unlock sequence per `docs/specs/phase-6b-boundary.md` (P6B-A..F batches). Unlock requires explicit user re-authorization per non-goal + per-batch Codex pre-impl review + at least one non-trivial host-compromise control per `docs/specs/production-signer.md` Section 2.5.

Phase dependency chain: P0 -> P1 -> P2 -> [Replay Gate + State Correctness Gate] -> P3 -> P4 -> P5 -> [Safety Gate] -> P6a -> [Production Gate] -> P6b.

No phase may begin until all gates for the preceding checkpoint have been signed off.

## Rationale

- Vertical slices surface integration risk early. Horizontal layering delays integration until late, when rework is most expensive.
- Replay hooks are required for deterministic testing of a live-data system; baking them in from P1 avoids retrofitting.
- Mandatory gates prevent "works in isolation" from masquerading as production-ready.
- The gate policy is explicit and binary -- pass/fail -- to eliminate subjective phase-exit criteria.

## Revisit Trigger

The thin path approach fails to produce end-to-end results (captured event -> simulated profit signal -> bundle construction) by the end of Phase 3.

## Consequences

- Phase 1 must instrument replay hooks from day one, not as an afterthought.
- Scope expansion (new pools, new strategies) is blocked until P4, even if P1--P3 complete ahead of schedule.
- Gate failures halt the phase queue; the team must resolve blockers before proceeding.
- Approach B (horizontal layers) is off the table unless the revisit trigger fires.

**Phase 6b scope context (added 2026-05-16 per user authorization on P6B-A pre-impl pack APPROVED HIGH at `2ddba8a`):** The Phase 6b Production Gate is the only path to live action (funded key, production signer, `live_send=true`, `eth_sendBundle` runtime, actual relay submission). The Phase 6b unlock CONTRACT lives in `docs/specs/phase-6b-boundary.md`; the production-signer design contract lives in `docs/specs/production-signer.md`. **At the moment this amendment lands, the funded-key + prod-signer ban from `docs/specs/execution-safety.md` Section "Funded Key / Prod Signer Ban" REMAINS IN FORCE for all profiles, including the future operator-controlled production profile.** The eventual operational scope-lift is the cumulative effect of P6B-B (HSM/KMS-backed signer impl + host-compromise control) + P6B-C (key wiring) + P6B-D (config-validation flip restricted to the production profile + HSM/KMS signer) + P6B-E (live submission, fully gated) all landing IN SEQUENCE with their respective Codex pre-impl reviews and explicit user re-authorizations; no single batch in Phase 6b -- and specifically NOT P6B-A -- lifts the ban by itself. After P6B-D lands, `live_send = true` becomes permissible only for the operator-controlled production profile when paired with the HSM/KMS-backed signer from P6B-B; dev/test/shadow profiles continue to reject `live_send = true` unconditionally. After P6B-E lands, `submit_bundle` may return `Ok(SubmissionReceipt)` only through the G12 pre-check chain (which INHERITS G13); outside that chain `submit_bundle` continues to return `Err(KillSwitchActive)` or `Err(SubmitDisabled)` per the Phase 6a PRECEDENCE. This amendment DESCRIBES the unlock PATH; it does NOT effectuate any unlock.
