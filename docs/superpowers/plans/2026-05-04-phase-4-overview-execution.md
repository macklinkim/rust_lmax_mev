# Phase 4 Overview

**Date:** 2026-05-04
**Status:** Draft v0.2 (revised after Codex 2026-05-04 21:24:10 +09:00 REVISION REQUIRED HIGH; Codex's verdict was procedural — full content not visible to the watcher — but it pre-supplied answers to all 8 questions, encoded below as Q1..Q8 verdicts. Substantive plan content unchanged from v0.1; v0.2 adds the answer encoding + Q8 hardening invariants.)
**Predecessor:** `phase-3-complete` at `e2a9c19` (Codex APPROVED MEDIUM 2026-05-04 19:56:18).

## Phase 4 user-approval basis

User explicitly authorized Phase 4 kickoff 2026-05-04 KST (verbatim: "Phase 4 kickoff를 승인한다. 이번에는 새 phase scope-defining start에 대한 사용자 승인을 명시적으로 부여한다. 더 이상 Phase 4 시작 여부로 idle하지 마라."). Phase 4 work proceeds under the 2026-05-04 routine-closeout policy + the standing forbidden list reaffirmed by user this turn:

> 여전히 금지: live trading / relay submission / funded key / live_send = true / destructive git / force push / `.claude/` or `AGENTS.md` staging.

Phase 4 ships INFRASTRUCTURE for relay submission, real revm, archive node, etc. — but does NOT actually flip `live_send = true`, does NOT use a funded key, does NOT call `eth_sendBundle`. Live-action gates remain Phase 5 Safety Gate (per ADR-001) and Phase 6a Production Gate.

## Scope (per ADR-002 Phase 4 unlock + ADR-006 + ADR-007 + Phase 3 deferrals)

Three distinct buckets of work, ordered by foundation-first dependency:

### Bucket A — ADR-006 deferral resolution (P3-E follow-up)

P3-E shipped DP-S1 with `ProfitSource::HeuristicPassthrough` per the user-approved ADR-006 deferral. Phase 4 lands the strict-revm path:

1. **Archive node integration** per ADR-007 §"Archive access" ("required" at Phase 4). Add archive-mode HTTP support to `NodeProvider`; expose `eth_getProof` / `eth_getStorageAt` / `eth_getCode` for state-fetcher consumption. Configurable archive endpoint separate from the existing primary/fallback HTTP for clean failure isolation.
2. **State-fetcher**: thin wrapper that on demand pulls real Uniswap V2/V3 contract bytecode + storage slots needed by revm to execute a real `swap()` against the snapshotted block.
3. **Real Uniswap bytecode integration**: load V2 (`UniswapV2Pair`) and V3 (`UniswapV3Pool` 0.05% tier) bytecode from chain via state-fetcher OR embed verified-bytecode constants. Either path must be deterministic + testable against a recorded fixture.
4. **Flip `LocalSimulator::ProfitSource::HeuristicPassthrough` → `RevmComputed`**. After P4 swap, `simulated_profit_wei` IS the real revm-computed delta (not heuristic-passthrough). API surface from P3-E stays unchanged; only the internal pipeline + the emitted `ProfitSource` variant change.
5. **Determinism contract**: same recorded chain state + same opportunity → byte-identical `SimulationOutcome` (extends P3-E S-2). Recorded fixtures land alongside.

### Bucket B — Relay simulation comparator (ADR-006 strict)

ADR-006 §"Simulation pipeline" mandates BOTH local revm pre-sim AND relay simulation (`eth_callBundle`) for every bundle, with zero-tolerance mismatch abort:

6. **Relay simulation client**: HTTP `eth_callBundle` adapter against Flashbots + bloXroute relay endpoints. Read-only; no signing, no funded key, no `eth_sendBundle`.
7. **`MismatchCategory` enum** in `crates/types` (per Codex 15:03:59 Q4 if cross-crate consumed) with the 6 ADR-006 variants: `Profitability` / `Gas` / `Revert` / `StateDependency` / `BundleOutcome` / `Unknown`. `#[non_exhaustive]`.
8. **`LocalRelayComparator`** (likely in `crates/simulator` or new `crates/sim_compare`): consumes `SimulationOutcome` (local) + `RelaySimulationOutcome` (relay) → emits `Result<(), MismatchAbort { category, ... }>` with zero tolerance per ADR-006.

### Bucket C — Bundle relay infrastructure (NO submission)

9. **`BundleRelay` trait** per ADR-003: object-safe trait that adapter implementations can target. Adapters: `FlashbotsRelay` + `BloxrouteRelay`. Trait method shape supports BOTH simulation (`simulate_bundle`) and submission (`submit_bundle`) but the engine wires ONLY simulation in Phase 4 — `submit_bundle` exists at the trait level so Phase 5+ can wire it without ABI breakage.
10. **`SignedBundle` / `BundleSubmission` payload types**: shape only; signing infrastructure is NOT implemented in Phase 4 (no funded key per user constraint). A `crates/signing` crate can be scaffolded with a stub signer that fails closed, OR the signing surface stays in `crates/execution` as a `todo!()` until Phase 5.
11. **Sushiswap WETH/USDC inclusion** per ADR-002 Phase 4 unlock: extend `crates/state` pool registry (additive `PoolKind::SushiswapV2` variant + decode adapter). Extends `crates/opportunity` to consider three-venue arb (UniV2 / UniV3 / SushiswapV2).
12. **External mempool feed adapter**: `MempoolSource` trait already exists per ADR-003; add a second impl (bloXroute BDN OR Chainbound Fiber) behind a feature flag or runtime selector. Deployment-time choice per ADR-003.
13. **Final wiring**: extend `wire_phase4` (or new `wire_phase5_pre_safety`) with the relay-sim driver + comparator + the bundle-relay trait wiring (still no submission).

## Codex 21:24:10 v0.2 verdicts (encoded; standing answers unless revised)

- **Q1 (bucket ordering)**: keep foundation-first A → B → C. No earlier `BundleRelay` lift.
- **Q2 (`MismatchCategory` location)**: place in `crates/types` if it crosses simulator/risk/app boundaries (Phase 4 use favors this); additive carve-out as in P3-A.
- **Q3 (Sushiswap timing)**: keep at P4-F. Earlier batches venue-parametric but do NOT widen scope before real revm (P4-C) is stable.
- **Q4 (external mempool feed binding)**: runtime config selector preferred. Compile features only for optional dependency isolation if truly needed.
- **Q5 (`live_send` enforcement)**: config-validation reject + submit-path hard `Result::Err` at the boundary. NO runtime panic as the primary guard. Compile-time `#[cfg]` only as a defense-in-depth supplement, not the primary mechanism.
- **Q6 (relay-sim interpretation)**: `eth_callBundle` is read-only relay simulation and proceeds under the stated forbids per ADR-006 §"Simulation pipeline". `eth_sendBundle`, funded signing, and live submission remain forbidden. Default proceed; no further user clarification needed.
- **Q7 (batch count)**: keep 7 (P4-A through P4-G). Do NOT merge A+B or D+E unless the per-batch detailed plans prove the boundary is trivial.
- **Q8 (hardening invariants — NEW v0.2)**: every Phase 4 batch close must include explicit verification of:
  - **No funded key** in the codebase (no key derivation that could yield a usable mainnet account; `cargo run` must not be a path to live submission).
  - **No signing infrastructure that produces a submittable signed transaction**.
  - **No `submit_bundle` call site wired** to the trait (`BundleRelay::submit_bundle` exists at the trait level — P4-E — but no caller invokes it; compile would require an explicit Phase 5 wiring change).
  - **No `live_send = true` ever** (default false; config validation rejects true; submit-path returns `Err` even if config slipped through).
  - **Env-gated `#[ignore]`'d network tests only** for any test that would otherwise touch a live RPC (mirrors `crates/replay/tests/g_state_live.rs` env-contract stub from P2-C).
  - **Secret redaction in logs** for any URL or header that could carry an API key (Alchemy/Infura/bloXroute API keys); `tracing` field-level redaction.
  - **Fail-closed config defaults**: every new config field defaults to the safe value (e.g., archive endpoint defaults to `None`, mempool feed selector defaults to local-Geth-only, `live_send` defaults `false`).

## Forbidden additions for Phase 4 (reaffirmed)

- **No `eth_sendBundle`**, no actual bundle submission to any relay, no signed transaction broadcast.
- **No funded key**, no production signer, no key derivation that could yield a usable mainnet account.
- **`live_send = false` enforced everywhere** — config schema, runtime check, integration-test asserts. Compile-time guard if achievable; else explicit runtime panic-on-true.
- **No `live_send = true` toggle** ever, including in dev/test profiles.
- **No live mainnet execution** of bundles even via dev funds.
- **Relay SIMULATION is allowed** (`eth_callBundle` is read-only, requires no signing, no key, no submission). This is required by ADR-006.
- **No destructive git** (force push / reset --hard / branch delete on shared refs / rebase of pushed commits).
- **No `.claude/` or `AGENTS.md` staging** ever.
- All standing Phase 1+2+3 forbids carry over; ADR-001 Safety Gate (P5 EXIT) and Production Gate (P6a EXIT) remain the only paths to live action.

## Provisional batch breakdown (proposal — pending Codex sign-off)

Foundation-first ordering (each row depends on the prior):

| Batch | Goal | Key deliverable |
|---|---|---|
| P4-A | ADR-007 archive node integration | `NodeProvider` archive HTTP support + `eth_getProof`/`getCode`/`getStorageAt`; config schema for the archive endpoint |
| P4-B | State-fetcher | Thin crate (or `crates/state` extension) that loads real Uniswap V2/V3 bytecode + needed storage slots into a revm-shaped DB |
| P4-C | Real revm + `ProfitSource::RevmComputed` | Replace P3-E STOP test bytecode with real Uniswap calldata path; flip provenance variant; recorded-fixture determinism test |
| P4-D | `MismatchCategory` + relay sim comparator | Add enum to `crates/types`; new `LocalRelayComparator` in `crates/simulator`; relay sim client (`eth_callBundle`) |
| P4-E | `BundleRelay` trait + Flashbots/bloXroute adapters | Trait + 2 adapters; `simulate_bundle` wired, `submit_bundle` declared but unwired |
| P4-F | Sushiswap WETH/USDC + external mempool feed | Additive `PoolKind::SushiswapV2` + 3-venue arb; second `MempoolSource` impl behind feature/selector |
| P4-G | Final wiring (`wire_phase5_pre_safety`) + DoD audit + `phase-4-complete` tag | Connect relay-sim comparator into the existing P3-F driver chain; abort path exercised on mismatch; tag draft |

Some merges may be appropriate (e.g., P4-A + P4-B if archive + state-fetcher are inseparable; P4-D + P4-E if the relay client is shared between sim and submission code paths). Codex pre-impl review should propose mergers.

## Architectural risks needing pre-implementation review

Phase 4 is widely-scoped; per the lean-doc policy, pre-impl review is requested ONLY for batches with architectural risk:

- **P4-A: YES** — first cross-network call class (archive node) introduces an external dependency; failure-mode policy + caching strategy + cost-control discussion (archive RPC quotas).
- **P4-B: YES** — state-fetcher determinism contract is load-bearing for the ADR-006 deferral resolution; test-fixture strategy + chain-tip-vs-recorded-block resolution policy needs sign-off.
- **P4-C: YES** — flips `ProfitSource::HeuristicPassthrough` → `RevmComputed`. Must surface any reconciliation mismatch (heuristic vs revm) for diagnostic, not silently overwrite.
- **P4-D: YES** — `MismatchCategory` shape choice + comparator policy (zero tolerance per ADR-006); `crates/types` edit (Phase-1-frozen carve-out) needs Codex sign-off as in P3-A.
- **P4-E: YES** — `BundleRelay` trait shape determines Phase 5+ submission code; getting the sim-vs-submit boundary right NOW saves a refactor later.
- **P4-F: NO** — Sushiswap is additive following established patterns; mempool feed is additive following ADR-003.
- **P4-G: NO** — final wiring follows established `wire_phase4` / topology Option A precedent; final DoD audit + tag draft mirrors Phase 1/2/3 close pattern.

## Process

Per `task_state.md` 2026-05-04 routine-closeout policy: Phase 4 docs/plan/impl commits + master push + per-batch `phase-N-complete` tag (when Codex APPROVED + execution-note-documented) + CLAUDE.md wrap-ups proceed without user re-confirmation. User explicit OK is reserved for: destructive git, branch/remote changes, ADR/scope/frozen-decision changes, live trading / funded key / `live_send = true`, `.claude/` or `AGENTS.md` staging, Codex `REVISION REQUIRED` or `LOW` confidence outcomes, and the scope-defining first start of any new phase (Phase 5 etc.).

After Codex APPROVAL of this overview content:
1. Claude commits + pushes this note as `docs: add Phase 4 overview` per the routine-doc policy.
2. P4-A: draft batch execution note → Codex pre-impl review → ladder + close review.
3. P4-B through P4-E: same pattern.
4. P4-F + P4-G: skip pre-impl review per architectural-risk assessment above.

## Question for Codex (v0.2 — non-scope items only)

v0.2 encodes Codex 21:24:10 verdicts on Q1..Q8 inline (see §"Codex 21:24:10 v0.2 verdicts"). All eight original questions have standing answers. The remaining open items are confirmation-at-approval, not new content:

- v0.2 is the full content Codex needs (provided inline in the outbox + on disk at `docs/superpowers/plans/2026-05-04-phase-4-overview-execution.md`).
- User-kickoff-approval evidence is the verbatim quote in the outbox top-of-section.

If APPROVED: per the routine policy, Claude commits + pushes this overview then drafts P4-A. If REVISION: revise + re-emit. If scope/ADR change required (e.g., a Q-answer needs override): HALT to user.
