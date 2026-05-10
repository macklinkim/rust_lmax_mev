# Phase 5 Overview — Safety Gate (planning + design + fail-closed wiring; NO live action)

**Date:** 2026-05-10 KST
**Status:** Draft v0.3 (revised after manual Codex REVISION REQUIRED HIGH on v0.2, 2026-05-10 KST). One residual R-P5-3 fragment fixed in P5-B Scope (replaced "ADR-006 §Gas bidding updated to record the Phase 5+ unlock" with the user-supplied no-amendment wording). v0.1 R-P5-1..R-P5-4 + Codex Q-P5-1..7 standing answers carried unchanged. Awaiting manual Codex re-review.
**Predecessor:** `phase-4-complete` annotated tag at `e5f13ea` (Phase 4 fully closed; Codex closeout NO ACTION HIGH).

## Phase 5 user-approval basis

User explicitly authorized **Phase 5 kickoff** (planning + design) 2026-05-10 KST. User did NOT authorize live trading, funded keys, production signer use, `live_send=true`, or actual relay submission. Phase 5 is treated as **Safety Gate design + fail-closed wiring only**; Phase 6b Production Gate remains the only path to live action per `docs/specs/execution-safety.md`.

## Baseline at Phase 5 start

- Workspace 206 passed + 1 ignored at `e5f13ea`.
- All Phase 4 hard safety invariants 1..16 verified (recorded in P4-G DoD audit).
- `LocalSimulator::simulate_with_fingerprint` runs real revm against recorded fixtures (P4-C2); `ProfitSource::RevmComputed` stamped.
- `comparator_driver` wired; `submit_bundle` always `Err(SubmitDisabled)`; `Arc<dyn RelaySimulator>` only in `crates/app`; `live_send=true` config-rejected; multi-relay rejected; mismatch journal append+flush before broadcast; full secret redaction.
- `ExternalMempoolSource` fail-closed; `MempoolSourceKind` runtime selector.
- Production `LocalSimulator::prefetch_for` integration deferred to Phase 5 (P4-G boundary).
- `wire_phase4` in production constructs `LocalSimulator` with no fixtures → simulate returns `Setup` → entire downstream chain inert (no `MismatchAbort` records, no `submit_bundle` reachability).

## Safety assumptions and non-goals

### Safety assumptions (carried from Phase 4)

- `docs/specs/execution-safety.md` ban remains in force: **funded private key OR production signer is banned until Phase 6b Production Gate**.
- `live_send = false` is the default and Phase 5 keeps the config-validation reject of `live_send=true`.
- `eth_sendBundle` remains forbidden in Phase 5.
- Actual relay submission (any tx broadcast through any path) remains forbidden in Phase 5.
- No paid live API dependency in CI.
- No live-network tests enabled by default.
- No `.claude/` / `AGENTS.md` staging; no destructive git; no force-push.
- WETH/USDC asset scope held; Sushi V2 + UniV2 + UniV3 0.05% venue scope held; no widening.

### Non-goals for Phase 5

- Phase 5 does **NOT** implement a production signer.
- Phase 5 does **NOT** implement actual relay submission (`submit_bundle` stays `Err(SubmitDisabled)`).
- Phase 5 does **NOT** flip `live_send` capability.
- Phase 5 does **NOT** populate `RelaySimRequest::txs` with real signed bytes (would require signing infra).
- Phase 5 does **NOT** ship Flashbots `X-Flashbots-Signature` auth header signing.
- Phase 5 does **NOT** widen asset scope or venue set.
- Phase 5 does **NOT** open Phase 6 Production Gate work.

If any of the above non-goals are reconsidered mid-phase, HALT and require explicit user authorization for that scope expansion.

## Provisional batch breakdown

Foundation-first ordering. Each row is INDEPENDENT of submission path.

| Batch | Goal | Pre-impl review |
|---|---|---|
| P5-A | Production `LocalSimulator::prefetch_for` wiring in **shadow mode** (archive-RPC-gated; fail-closed when archive absent/stale/rate-limited; per-block fixture cache + freshness contract; interior-mutability redesign of `LocalSimulator` for `Arc`-shared use). NO submission path change. | YES |
| P5-B | **Dynamic gas bidding** strategy infrastructure per ADR-006 §"Gas bidding" Phase 5+ unlock. Add `BidStrategy` trait + at least one strategy (EIP-1559 base-fee+tip-aware). Existing fixed-fraction stays as the default strategy implementation for safety. NO submission path change. | YES |
| P5-C | **Signer boundary design + fail-closed stub crate** (NEW `crates/signer`). Trait surface only; every method returns `Err(SignerDisabled)`. Documents the Phase 6b unlock contract. No real signer impl. No funded key. No key material in repo/tests/configs/env examples. | YES |
| P5-D | **Kill switch** wiring per `docs/specs/execution-safety.md` §"Kill Switch": config toggle (`relay.execution_disabled` already present from P4-E) + runtime toggle (Atomic flag accessible via `AppHandle4`). Asserts: even with future submission code, the kill switch suppresses every potential submit call site. Adds a hard `Result::Err` guard at any future submit boundary. | **YES** (R-P5-1: kill switch is a core execution-safety.md device; Q-P5-5 placement is open; not routine) |
| P5-E | **Final wiring + Phase 5 DoD audit + `phase-5-complete` tag**. Audit P5-A..D completion. Run all carry-forward Phase 4 safety grep gates + new Phase 5 invariants. Tag draft. | NO (audit/tag-only per Phase 4 P4-G precedent) |

Batches are independent enough that A and B can be reordered; C must precede any future signing-using work; D should be last before E to validate the kill switch against the new code paths.

## Per-batch detail

### P5-A — production `prefetch_for` wiring (shadow mode)

**Scope**:

- Restructure `LocalSimulator` to hold fixtures behind interior mutability (likely `Mutex<Option<FixtureSet>>` on `&self` API). The current `&mut self` `load_fixture` / `prefetch_for` becomes `&self` so the existing `Arc<LocalSimulator>` shared from `wire_phase4` works.
- Wire `simulator_driver` to call `prefetch_for(...)` per inbound `RiskCheckedOpportunity` IFF `config.node.archive_rpc.is_some()` AND `config.simulator.prefetch_enabled = true` (NEW config field, default `false`).
- Per-block fixture cache: keyed by `(block_hash, source_pool, sink_pool)` with bounded LRU; cache hit short-circuits the prefetch path. Eviction on block boundary.
- Fail-closed semantics: archive `ArchiveNotConfigured` / `Transport` / `Timeout` errors → driver logs at WARN + continues; **never silently substitutes stale state**.
- Freshness contract: a fixture is considered stale after N blocks (config; default 1, i.e., per-block re-fetch) — stale entries are evicted before use.
- NO submission path change. NO sigining. NO new live network surface beyond the existing P4-A archive client.

**Forbidden in P5-A**:
- Funded key, signer, submission, `live_send=true`.
- Wiring prefetch into the GethWS path (only the archive RPC path).
- Caching across process restarts.

**Tests** (lean):
- Disabled-by-default: with `prefetch_enabled = false`, `simulator_driver` path unchanged from Phase 4.
- Fail-closed: with archive RPC unconfigured, driver logs WARN + drops event; no fixture loaded.
- Cache hit: per-`(block_hash, pools)` re-call returns from cache (mock fetcher counts calls).
- Stale eviction: after N blocks, cache entry is evicted before reuse.

**Gates**: standard fmt / build / test / clippy / deny + cycle gate + safety greps carry-forward.

### P5-B — dynamic gas bidding strategy

**Scope**:

- Add `BidStrategy` trait in `crates/execution`: `fn compute_bid(&self, outcome: &SimulationOutcome, ctx: &BidContext) -> U256`.
- `BidContext` carries the live block context (base fee, prior block tip distribution, etc.). Phase 5 fields are minimal; Phase 6+ extends.
- Two impls in P5-B:
  1. `FixedFractionBidStrategy` — preserves existing P3-F default behavior verbatim.
  2. `Eip1559BasefeeAwareBidStrategy` — bid = `min(profit * fixed_fraction, base_fee * gas_used + tip_floor)`.
- `BundleConstructor::new(...)` takes `Arc<dyn BidStrategy>`; default = `FixedFractionBidStrategy::default()` so existing call sites unchanged.
- The P5-B execution note records that ADR-006 already allows dynamic gas bidding in Phase 5+; ADR-006 text is not modified unless separately authorized by the user.

**Forbidden in P5-B**:
- ML / adaptive feedback strategies (Phase 6+ per ADR-006).
- Reading external bid feeds (no live API).

**Tests** (lean):
- Both strategies: known-input known-output unit tests.
- Default behavior unchanged when `BundleConstructor::default()` is used (regression guard).

### P5-C — signer boundary design + fail-closed stub

**Scope**:

- NEW `crates/signer` skeleton crate with:
  - `pub trait Signer: Send + Sync + 'static { async fn sign_tx(&self, ...) -> Result<Vec<u8>, SignerError>; }`
  - `pub enum SignerError { SignerDisabled, ... }` (`#[non_exhaustive]`)
  - Display message on `SignerDisabled` MUST contain "Phase 6b Production Gate" (BR-3-style spec-drift guard).
  - One impl: `DisabledSigner` returning `Err(SignerDisabled)` unconditionally.
  - NO real signer impl. NO funded key. NO key derivation. NO `secp256k1` / `Wallet` / `PrivateKey` symbols.
- Documentation of the Phase 6b unlock contract (what production signer must satisfy: HSM/KMS-only, never-in-memory-key, audit log, etc.).
- Optional: a `crates/signer` module-level `forbid!` macro that asserts at compile time that no key material is present (defense-in-depth).

**Forbidden in P5-C**:
- Any real signer impl (test-ephemeral or otherwise).
- Any private key material in repo / tests / configs / env examples / fixtures.
- Importing `secp256k1` / `k256` / `alloy-signer` / `ethers-signers` / `Wallet` / `PrivateKey` types.
- `tracing::*` calls that log signer state beyond `signer_set: bool`.

**P5-C grep-gate redefinition (R-P5-2)**: P5-C deliberately introduces a `Signer` trait + `DisabledSigner` impl in the new `crates/signer`. The Phase 4 zero-hit `Signer` grep gate must be redefined to allow these specific symbols and forbid everything else:
- **Allowed Signer symbols** (whitelist): `crates/signer::Signer` (trait), `crates/signer::DisabledSigner`, `crates/signer::SignerError`. Any `Signer` token outside `crates/signer/` remains forbidden.
- **Forbidden grep set** (replaces the Phase 4 `Signer` zero-hit gate from P5-C onward): `Wallet|PrivateKey|secp256k1|k256|alloy-signer|ethers-signers|sign_transaction|funded` — zero hits anywhere in `crates/`.
- The `funded` token catches any future `funded_key` / `funded_signer` / `funded_wallet` mention; intentional doc comments referencing "funded private key is banned" must use a different phrasing (e.g., `"production key material"`) to keep the grep clean.
- The G2 grep documented in the P4-G DoD audit (`Signer\|Wallet\|PrivateKey\|secp256k1\|sign_transaction`) is **superseded** for Phase 5+ by the redefined gate above; the P5-E DoD audit must record this transition.

**Tests** (lean):
- `SignerDisabled` Display contains "Phase 6b Production Gate".
- `DisabledSigner::sign_tx` always returns `Err(SignerDisabled)`.
- Grep gate at batch close: zero forbidden imports/symbols.

### P5-D — kill switch wiring

**Scope**:

- Promote `relay.execution_disabled` from per-config-read (where Phase 5+ adds checks) to a process-wide `Arc<AtomicBool>` accessible via `AppHandle4`.
- Wire the atomic into a hard `Result::Err(KillSwitchActive)` guard at every potential submission boundary. In Phase 5 the boundaries are: (a) the `comparator_driver` would-be submission point (still SubmitDisabled at trait level — the kill switch is the second layer); (b) any future `submit_bundle` impl would consume the guard.
- Operator-toggle method: `AppHandle4::set_execution_disabled(bool)` for runtime toggle without restart.
- Default `false` (matching `relay.execution_disabled` default).

**Forbidden in P5-D**:
- Code that ACTUALLY submits — the kill switch is preventive, not enabling.
- A "kill" mode that bypasses other safety gates (kill switch is layered on top, not replacing).

**Tests** (lean):
- Toggle: `set_execution_disabled(true)` flips the atomic; subsequent guard reads return `Err`.
- Default: `false` matches `relay.execution_disabled` default.

### P5-E — final wiring + Phase 5 DoD audit + tag

**Scope**:

- Audit P5-A..D completion against this overview.
- Run all Phase 4 carry-forward safety grep gates (G1..G8) PLUS new Phase 5 invariants (P5-C signer-symbol grep gate; P5-D kill-switch presence; P5-A archive-gated default).
- DoD audit doc per the Phase 4 P4-G template.
- Create `phase-5-complete` annotated tag.

**Forbidden**:
- Tag without all gates passing.
- Tag if any safety ambiguity surfaced — write outbox for Codex review instead.

## Explicit treatment of cross-cutting items

| Item | Phase 5 disposition |
|---|---|
| Production `prefetch_for` | P5-A wires it BEHIND a config flag (default off), archive-RPC-gated, fail-closed |
| Signer / key policy | P5-C ships fail-closed stub + design doc; NO real signer; NO key material |
| Signed tx bytes for `eth_callBundle` | NOT shipped in Phase 5. `RelaySimRequest::txs` stays empty → adapter short-circuits with `UnsignedBundleUnavailable` (P4-E behavior). Real signing requires Phase 6b. |
| `submit_bundle` | Stays `Err(SubmitDisabled)`. Phase 5 adds the kill-switch second layer (P5-D). |
| `live_send` | Stays config-validation rejected. NO change in Phase 5. |
| Kill switch | P5-D wires it (config + runtime toggle). |
| Secret redaction | All Phase 4 redaction tests carry forward; no regression allowed. |
| Journal/abort policy | Mismatch journal append+flush before broadcast (P4-E R-E9) carries forward; no regression. |
| Dynamic gas bidding | P5-B unlocks per ADR-006 §"Gas bidding" Phase 5+ language; default strategy preserves existing behavior. **R-P5-3**: ADR-006 itself is NOT amended in P5-B — the unlock is recorded in the P5-B execution note only. ADR text changes require separate explicit user approval. |
| Asset scope | WETH/USDC only. UniV2 + UniV3 0.05% + Sushi V2. NO widening. |

## Hard forbids during all of Phase 5

- No funded key.
- No production signer.
- No private key material in repo / tests / fixtures / configs / env examples.
- No `live_send = true`.
- No `eth_sendBundle`.
- No actual relay submission.
- No real paid API dependency in CI.
- No live-network test enabled by default.
- No Phase 6 Production Gate work.
- No `.claude/` / `AGENTS.md` / `fixture_output.txt` staging.
- No destructive git, no force-push.
- No asset-scope widening; no extra V3 fee tiers; no extra venues.

## Phase 6 boundary (kept explicit)

Phase 6 (NOT Phase 5) ships:
- 6a Pre-Production Gate: shadow-run with full submission-shape validation against test relays
- 6b Production Gate: funded key / production signer / `live_send=true` / actual `eth_sendBundle`

Anything that touches a real signer or actual submission is a Phase 6 item.

## Codex Q-P5 standing answers (v0.2)

All seven v0.1 open questions received Codex verdicts. Encoded inline as standing decisions; future per-batch plans inherit these unless re-opened.

- **Q-P5-1 — APPROVED batch ordering A→B→C→D→E**. Rationale: P5-A is independent of signing/submission and closes the most prominent Phase 4 → Phase 5 boundary (P4-G's explicit `prefetch_for` deferral).
- **Q-P5-2 — defer to P5-A pre-impl plan**; default `parking_lot::Mutex<Option<FixtureSet>>` for `LocalSimulator` interior mutability. Revisit only if P5-A profiling shows contention.
- **Q-P5-3 — `BidStrategy` metric**: P5-B documents the metric contract only (canonical counter name + label set). Implementation deferred (P5-E or Phase 6).
- **Q-P5-4 — `Signer::sign_tx` shape**: structured `BundleTx { from, to, value, data, gas_limit, nonce, chain_id, bundle_correlation_id, ... }` (the `bundle_correlation_id` is REQUIRED so journaled signing events can be cross-linked to the upstream comparator chain).
- **Q-P5-5 — kill switch placement**: BOTH per-driver AND per-adapter. P5-D pre-impl review required (R-P5-1 lifted P5-D's review status).
- **Q-P5-6 — `prefetch_enabled` default**: `false` even when `archive_rpc.is_some()`. Operator must opt in to incur live archive RPC cost (no auto-on).
- **Q-P5-7 — ADR-006 amendment**: NO ADR-006 edit. P5-B records the dynamic-gas-bidding unlock in its execution note ONLY. ADR text changes require separate explicit user approval (R-P5-3).

## Recommended first implementation batch after Codex review

**P5-A — production `prefetch_for` wiring (shadow mode)**, contingent on Codex APPROVED on this overview + the P5-A pre-impl plan that follows. Rationale: P4-G left `prefetch_for` as the explicit deferred item; landing it first closes the most prominent Phase 4 ↔ Phase 5 boundary, is independent of signing/submission work, and exercises only the existing P4-A archive RPC surface (no new live network class).

If Codex prefers a different ordering (Q-P5-1), Claude follows the verdict.

## Process

Per the 2026-05-04 routine-closeout policy + 2026-05-04 22:20 KST manual-Codex-review-mode + the user's explicit Phase 5 kickoff instruction (overview stays UNCOMMITTED on disk until Codex APPROVED):

1. Claude writes this overview to disk (UNCOMMITTED) + emits the review pack to `.coordination/claude_outbox.md`.
2. Claude STOPS and reports "manual Codex review required".
3. User pastes pack to Codex; relays verdict.
4. Claude records verdict in `.coordination/codex_review.md`.
5. **APPROVED** → commit + push this overview as a routine doc commit; THEN draft the P5-A pre-impl plan; THEN await Codex P5-A verdict before any code change.
6. **REVISION REQUIRED** → revise overview in place + re-emit pack.
7. **Scope/ADR change required** → HALT to user (any item from Phase 5 hard-forbids list, any ADR text change, any signer/submission/`live_send` capability addition).

No code or `Cargo.toml` edits in this turn. No commit. No push. No tag.
