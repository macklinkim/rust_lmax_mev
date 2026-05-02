# Phase 2 Overview

**Date:** 2026-05-02
**Status:** Draft v0.2 (Codex 2026-05-02 19:07:45 +09:00 HIGH REVISION REQUIRED — P2-B reclassified to pre-impl YES because it owns the State Correctness Gate; Process section clarified that Codex doc-commit authorization rests on the standing routine-in-flight-gates policy + user's explicit "Phase 2 시작" prompt this turn, not on Codex approval alone)
**Predecessor:** `phase-1-complete` at `a93dd91`.

## Phase 2 EXIT (per ADR-001)

Two binary gates:
1. **Replay Gate** — same journal + same code → byte-identical emitted state events.
2. **State Correctness Gate** — engine reserves snapshot for WETH/USDC on UniV2 + UniV3 0.05% at block N matches a fresh `eth_call` at the same block; tolerance = 0.

## Batch breakdown

| Batch | New crates | Touched | Goal |
|---|---|---|---|
| **P2-A** | `node`, `ingress` | `config` (additive), workspace deps (`alloy`) | Geth WS+HTTP + fallback (ADR-007); `MempoolSource` trait + Geth WS impl (ADR-003); normalize raw txs into `MempoolEvent` + `BlockEvent`. |
| **P2-B** | `state` | `config` (additive pool addrs) | UniV2 + UniV3 0.05% WETH/USDC pool models; per-block reserves via `eth_call`; persist to `RocksDbSnapshot` keyed by `(block, pool)`; emit `StateUpdateEvent`. |
| **P2-C** | `replay` | — | `Replayer` trait + journal-backed impl; the two EXIT gate tests under `crates/replay/tests/` (recorded fixtures for CI determinism, manual live-network smoke separate). |
| **P2-D** | — | `crates/app` (producer-side wiring) | End-to-end runnable; DoD audit; `phase-2-complete` tag draft (creation user-gated). |

## Crate freeze policy

Phase 1 frozen crates (`types e2911cf`, `event-bus bb2e020`, `journal 9c81e27`, `observability 587211f`, `app 2b82272`, `smoke-tests ad8de57`) stay source-frozen except:
- `crates/config`: additive struct fields only (precedent: `BusConfig` in Batch B).
- `crates/app`: producer-side wiring updated in P2-D (this is the explicit Phase 2 endpoint).

Per-batch event payload types live in their producing crate (`MempoolEvent`/`BlockEvent` in `ingress`, `StateUpdateEvent` in `state`) so `crates/types` stays frozen.

## Forbidden additions for Phase 2

- No live-mainnet calls inside `cargo test --workspace` (CI must stay deterministic; live-network smoke is dev-host only).
- No bundle submission, no `BundleRelay`, no relay sim (Phase 4 per ADR-002 + ADR-003 + ADR-006).
- No external mempool feed (`bloXroute` / `Chainbound`) — Phase 4 deployment-time choice per ADR-003.
- No `revm` dependency — Phase 3.
- No archive node integration — Phase 4 per ADR-007.
- All standing Phase 1 forbids carry over (no push/tag/CLAUDE.md/AGENTS.md/.claude/ staging without explicit user approval; `live_send = true` never; funded key never).

## Architectural risks needing pre-implementation review

Per `feedback_phase2_doc_volume.md`, pre-impl Codex review is requested ONLY for batches with architectural risk. Initial assessment:

- **P2-A: YES** — first new crate boundaries since Phase 1, first `alloy` integration, `MempoolSource` trait shape sets pattern for `BundleRelay` later. → request Codex pre-impl review of the P2-A batch execution note.
- **P2-B: YES** (revised v0.2 per Codex 19:07:45) — owns half of the Phase 2 EXIT (State Correctness Gate). Architectural decisions belonging to P2-B and not yet settled: provider-abstraction usage from `crates/state` (own client vs. shared NodeProvider handle), block-pinning strategy for `eth_call` (block-hash vs. block-number-with-reorg-tolerance), pool identity + `[state]` config schema, `RocksDbSnapshot` key shape `(block, pool)`, `StateUpdateEvent` payload contract, deterministic-fixture strategy for the gate test. → request Codex pre-impl review of the P2-B batch execution note.
- **P2-C: YES** — the two EXIT gates ARE the Phase 2 deliverable; gate-test design needs sign-off before code. → request Codex pre-impl review of the P2-C batch execution note.
- **P2-D: NO** — wiring follows the Phase 1 Batch B precedent; final audit + tag draft mirrors Phase 1 Batch D. → no pre-impl review; batch-close review only.

## Process

Authorization basis for the docs commit of this overview note: per `.coordination/task_state.md` "User policy update (2026-05-02 KST)", routine `spec/ADR doc commits` are Codex-approvable for in-flight gates. The user's explicit prompt this turn ("Phase 2 시작 ... Phase 2 execution note 초안부터 작성하세요") is the explicit Phase-2-commit-gate authorization that complements Codex's content approval. Push, tag, and CLAUDE.md edits remain separate user-approval gates (per the standing forbidden list) — Codex approval of this note alone does not authorize any of those.

After Codex APPROVAL of this overview content:
1. Claude commits this note as `docs: add Phase 2 overview`.
2. P2-A: draft batch execution note (≤150 lines) → Codex pre-impl review (architectural risk) → 4-commit ladder → batch-close review.
3. P2-B: draft batch note → Codex pre-impl review (State Correctness Gate ownership) → 4-commit ladder → batch-close review.
4. P2-C: draft batch note → Codex pre-impl review (gate design) → 4-commit ladder → batch-close review.
5. P2-D: draft batch note → 4-commit ladder → batch-close review → user-approval question for `phase-2-complete` tag.

## Question for Codex (pre-implementation overview review — architectural risk only)

1. Is the 4-batch breakdown's in/out cut between Phase 2 and Phase 3 aligned with ADR-001 + ADR-002?
2. Are `node` / `ingress` / `state` / `replay` the right crate seams (vs. e.g. `node`+`ingress` as one crate, or `state`+`replay` merged)?
3. Is the additive-`config`-only carve-out from Phase 1 freeze acceptable?
4. With v0.2 reclassifying P2-B → YES pre-impl review, are the THREE pre-impl gates (P2-A, P2-B, P2-C) sufficient, or should P2-D also be pre-reviewed?

If APPROVED: commit + draft P2-A batch note next. If REVISION REQUIRED: edit overview in place + re-emit. If scope/ADR change needed: HALT to user.
