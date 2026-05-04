# Phase 3 Overview

**Date:** 2026-05-04
**Status:** Draft v0.1 — first Phase 3 planning artifact, mirrors Phase 2 overview structure. Pre-impl Codex review requested for the overview content (architectural risk: 6-stage pipeline carve-up + spec-compliance repair scope).
**Predecessor:** `phase-2-complete` at `b5ed4cd`.

## Phase 3 EXIT (per ADR-001)

ADR-001 defines NO mandatory binary EXIT gate for Phase 3 (Replay + State Correctness are P2; Safety is P5; Production is P6a). Phase 3 closure is **revisit-trigger-driven** per ADR-001 line 43:

> The thin path approach fails to produce end-to-end results (captured event → simulated profit signal → bundle construction) by the end of Phase 3.

Therefore Phase 3 ships when:
1. The mempool / state event stream is journaled deterministically (`FileJournal<IngressEvent>` + `FileJournal<StateUpdateEvent>` consumers running).
2. An arbitrage opportunity engine emits candidate `OpportunityEvent`s from `StateUpdateEvent` deltas.
3. A risk engine sizes + filters candidates per ADR-006 + risk-budget spec.
4. A `revm` local pre-simulation produces a profit signal per candidate (relay sim + submission stay Phase 4 per ADR-002 shadow-only).
5. Bundle CONSTRUCTION (not submission) produces a `BundleCandidate` artifact suitable for Phase 4 relay handoff.

## Phase 2 → Phase 3 prerequisite repair

**Spec-compliance repair (mandatory P3-A first):** `docs/specs/event-model.md` mandates that every `EventEnvelope<T>` and its payload types derive `Clone, Debug, PartialEq, rkyv::{Archive, Serialize, Deserialize}, serde::{Serialize, Deserialize}`. P2-A's `IngressEvent` / `MempoolEvent` / `BlockEvent` and P2-B's `PoolState` / `StateUpdateEvent` shipped without the rkyv derives. This is a spec-compliance miss, not a free design call — Phase 3 P3-A repairs it before any consumer wiring. The repair is strictly additive (no API removal), so the standard "frozen P2-A/P2-B crate src" rule does not block it; the repair brings those crates into compliance.

## Batch breakdown (proposal — 6 batches)

| Batch | New / touched | Goal |
|---|---|---|
| **P3-A** | `crates/ingress` + `crates/state` (additive derives only); `crates/types` if `EventSource` enum needs to add new variants | Spec-compliance repair: add `rkyv + serde` derives on `IngressEvent`/`MempoolEvent`/`BlockEvent`/`PoolState`/`StateUpdateEvent` per `event-model.md`. Add missing `EventSource` variants if the existing enum is short. Unblocks P3-B journal wiring. |
| **P3-B** | `crates/app` (additive); journal-drain consumer threads | Spawn `FileJournal<IngressEvent>` + `FileJournal<StateUpdateEvent>` consumer threads in `wire_phase3` (extends `wire_phase2`); spawn `GethWsMempool::stream()` producer task. Replay-deterministic via existing `FileJournal` infra. |
| **P3-C** | new `crates/opportunity` | Arbitrage detection across `StateUpdateEvent` snapshots (UniV2 vs UniV3 0.05% delta). Emits `OpportunityEvent { pool_a, pool_b, expected_profit_wei, optimal_amount_in }`. Pure function on snapshot pairs; no I/O. |
| **P3-D** | new `crates/risk` | Sizing + budget gate per `docs/specs/risk-budget.md`. Filters opportunities; emits `RiskCheckedOpportunity` or `OpportunityAborted { category }`. |
| **P3-E** | new `crates/simulator` (`revm` local pre-sim only); `crates/types` for `SimulationOutcome` | revm sim against `RocksDbSnapshot` state; emits `SimulationOutcome { local_profit_wei, gas_used, mismatch_category: Option<MismatchCategory> }`. Per ADR-006 + ADR-002, P3 ships LOCAL ONLY — no `RelaySimulator`, no submission. |
| **P3-F** | new `crates/execution` (bundle construction only); `crates/app` (final wiring); P3 DoD audit + `phase-3-complete` tag draft | Bundle CONSTRUCTION emits `BundleCandidate` artifacts; no relay submission per ADR-002 shadow-only. Final `crates/app::wire_phase3` wires the full 6-stage pipeline. Final DoD audit + tag draft mirrors P2-D. |

`revm` enters via P3-E only (workspace dep, ADR-004 exact-minor pin, feature-narrowed iteratively as in P2-A's alloy onboarding). All other batches stay free of `revm`.

## Crate freeze policy (Phase 3)

- **Phase 1 frozen** (`types e2911cf`, `event-bus bb2e020`, `journal 9c81e27`, `observability 587211f`, `smoke-tests ad8de57`) stay source-frozen except `crates/types` if `EventSource` / `EventEnvelope` shape needs additive variants for new payload types (per `event-model.md` `EventSource` enum already lists `OpportunityEngine`/`RiskEngine`/`Simulator`/`Execution`/`Relay`, so likely no edit).
- **P2-A frozen** (`crates/node`, `crates/ingress`) — additive-derives-only edit allowed in P3-A per spec-compliance carve-out above; no API change beyond derives.
- **P2-B frozen** (`crates/state`) — same additive-derives-only carve-out in P3-A.
- **P2-C frozen** (`crates/replay`) — no edit; replay impl already uses the spec-mandated payload types.
- **P2-D `crates/app`** — additive `wire_phase3` constructor + `AppHandle3` per Phase 2 `wire_phase2` precedent; existing `wire`/`wire_phase2` stay byte-identical.
- **`crates/config`** — additive struct fields only (precedent: `BusConfig`/`IngressConfig`/`StateConfig`).

## Forbidden additions for Phase 3

- No `RelaySimulator` / `BundleRelay` / bundle submission (ADR-002 + ADR-006: Phase 4+).
- No live-mainnet calls in `cargo test --workspace` (replay fixtures + mocks only).
- No `live_send = true` ever; no funded key ever (execution-safety spec).
- No external mempool feed (bloXroute / Chainbound) — Phase 4 deployment-time (ADR-003).
- No archive-mode RPC integration — Phase 4 (ADR-007).
- No additional pools / venues / chains / pairs — thin path is frozen through Phase 3 (ADR-002 + thin-path-scope spec).
- No dynamic / EIP-1559 / ML gas bidding — Phase 5+ (ADR-006).
- All standing Phase 1 + Phase 2 forbids carry over (no push/tag/CLAUDE.md/AGENTS.md/.claude/ staging without explicit user approval; though per 2026-05-04 KST protocol update, routine docs/plan/impl commits + post-Codex-APPROVED master pushes proceed without re-confirmation).

## Architectural risks needing pre-impl Codex review

Per `feedback_phase2_doc_volume.md` lean-doc policy, pre-impl review is requested ONLY for batches with architectural risk:

- **P3-A: YES** — first-ever edit to a P2-A/P2-B-frozen crate after their batch close, even if strictly additive. Wants Codex sign-off on the freeze-carve-out shape + the exact derive set vs. spec.
- **P3-B: YES** — first-ever real producer spawn + journal-drain consumer in `wire_phase3`; threading model + shutdown ordering matter.
- **P3-C: NO** — pure-function arbitrage math; standard test-driven build.
- **P3-D: NO** — risk gate is a sequence of bounded checks against the spec; standard test-driven build.
- **P3-E: YES** — first `revm` integration; feature pinning + state-snapshot adapter + ADR-006 mismatch-category enum shape need sign-off.
- **P3-F: NO** — bundle construction is shape-driven; final app wiring follows the P2-D precedent.

## Process

Per 2026-05-04 KST protocol update from user, routine docs / plan / implementation commits + post-Codex-APPROVED master pushes proceed without explicit user re-confirmation. `phase-complete` tag creation/push also proceeds when the tag target + message is pre-recorded in the relevant execution note and Codex has APPROVED. User explicit OK is reserved for: destructive git ops, force push, branch/remote changes, ADR/scope/frozen-decision changes, live trading / funded key / relay submission, `.claude/`/`AGENTS.md` staging, and Codex REVISION REQUIRED / LOW confidence outcomes.

After Codex APPROVAL of this overview content:
1. Claude commits this note as `docs: add Phase 3 overview` and pushes per the new protocol.
2. P3-A: draft batch execution note → Codex pre-impl review → ladder + close review.
3. P3-B: same pattern.
4. P3-C, P3-D: skip pre-impl review per architectural-risk assessment above.
5. P3-E: pre-impl review.
6. P3-F: no pre-impl; batch-close + final P3 DoD audit + `phase-3-complete` tag draft.

## Question for Codex (pre-implementation overview review)

1. Is the 6-batch breakdown (P3-A spec repair → P3-B journal/producer wiring → P3-C opportunity → P3-D risk → P3-E revm sim → P3-F execution + app wiring + tag) aligned with ADR-001 vertical-slice ordering and ADR-002 shadow-only constraint? Should P3-D + P3-E merge?
2. Is treating P3-A's additive `rkyv + serde` derives on `IngressEvent`/`MempoolEvent`/`BlockEvent`/`PoolState`/`StateUpdateEvent` as a spec-compliance repair (not a freeze breach) acceptable, given `event-model.md` mandates them?
3. Is the LOCAL-ONLY revm scope for P3-E correct (no `RelaySimulator`, no submission — all P4) per ADR-002 + ADR-006?
4. Should `MismatchCategory` (per ADR-006 6-variant enum) live in `crates/types` (Phase 1 frozen — additive variants only) or in the new `crates/simulator`?
5. Anything else needed before P3-A drafting begins?

If APPROVED: commit + push this overview, then draft P3-A note. If REVISION: edit + re-emit. If scope/ADR change required: HALT to user.
