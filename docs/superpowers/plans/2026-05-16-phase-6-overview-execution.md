# Phase 6 Overview — Production Gate (6a Pre-Production + 6b Production)

**Date:** 2026-05-16 KST
**Status:** Draft v0.3 (revised after manual Codex REVISION REQUIRED HIGH on v0.2, 2026-05-16 KST). Four v0.2→v0.3 fixes: (1) Q-P6-F wording corrected to "G2c allow-list expanded by exactly TWO sites" (`crates/execution` + `crates/app`); (2) P6-A signer-boundary wording corrected to name `crates/execution` as the primary `Signer`-using boundary alongside `crates/app`; (3) P6-B test/gate wording reworded — G2d is an explicit allow-list gate (allowed file:line pairs in `crates/execution` and `crates/app`) with zero hits **outside** that allow-list; (4) batch-independence + Q-P6-H wording updated: P6-B introduces TWO signer dep edges, and P6-C no longer depends on P6-B for `BundleTx` shape since `BundleTx` already exists in `crates/signer` (P5-C DP-C3). v0.1 → v0.2 six-item revisions retained. Uncommitted. Awaiting manual Codex re-review.
**Predecessor:** `phase-5-complete` annotated tag at `55679a4` (Phase 5 Safety Gate fully closed; tag object `be98681`).

## Phase 6 user-approval basis

User explicitly authorized **Phase 6 PLANNING ONLY** on 2026-05-16 KST. User did NOT authorize any of:

- Production signer impl.
- Funded private key material in repo / tests / fixtures / configs / env / runtime.
- `live_send = true` capability flip.
- `eth_sendBundle` runtime call path.
- Actual relay submission (any bundle broadcast through any path).
- Paid live API dependency enabled in CI by default.
- Live-network tests enabled by default.
- ADR text amendment.
- Asset / venue / V3-fee-tier widening.

Phase 6 is split into two formally distinct gates: **6a Pre-Production** (this overview's scope for implementation planning) and **6b Production** (entirely separate; requires fresh explicit user approval before any planning starts). 6a remains **fail-closed** unless the user explicitly approves otherwise mid-phase.

## Baseline at Phase 6 start (verified 2026-05-16)

- Workspace **231 passed + 1 ignored** at `55679a4` (the one ignored test is the P2-C `g_state_live` env-contract live-smoke stub; carry-forward).
- 20 workspace crates: `types`, `event-bus`, `journal`, `config`, `observability`, `app`, `smoke-tests`, `node`, `ingress`, `state`, `replay`, `opportunity`, `risk`, `simulator`, `execution`, `state-fetcher`, `relay-sim`, `bundle-relay`, `relay-clients`, `signer`.
- All Phase 5 hard safety invariants HSI-1..11 verified at the tag (P5-E DoD audit at `55679a4`).
- `crates/signer` is an isolated leaf crate; `crates/app` does NOT depend on it. The only reachable code path is the in-crate unit tests.
- `KillSwitch` is wired through `wire_phase4` → `AppHandle4::{kill_switch(), set_execution_disabled(bool)}` → `comparator_driver` per-driver top-of-iteration guard. Per-adapter wiring deferred to Phase 6 per Q-P5-5 / DP-D8.
- Every relay adapter's `submit_bundle` returns `Err(SubmitDisabled)` unconditionally. `crates/app` holds adapters as `Arc<dyn RelaySimulator>` only (DP-E13 upcast prevention); zero `submit_bundle(` call sites in `crates/app/src/`.
- `LocalSimulator::prefetch_for` shadow-mode wiring is opt-in via `config.simulator.prefetch_enabled` (default `false`) AND archive-RPC-gated AND fail-closed.
- `BidStrategy` trait + `FixedFractionBidStrategy` + `Eip1559BasefeeAwareBidStrategy` shipped; default behavior preserved.
- Hard `submit_bundle` call-site bans + four-gate G2a/G2b/G2c/G2d signer grep gates + G9 `KillSwitch` leak gate all clean at the tag.

## Phase 6a safety assumptions and non-goals

### Safety assumptions (carried from Phase 5)

- `docs/specs/execution-safety.md` ban remains in force: **funded private key OR production signer is banned until Phase 6b Production Gate**.
- `live_send = false` is the default and Phase 6a keeps the config-validation reject of `live_send=true`.
- `eth_sendBundle` remains forbidden in Phase 6a.
- Actual relay submission (any tx broadcast) remains forbidden in Phase 6a.
- No paid live API dependency enabled in CI.
- No live-network tests enabled by default.
- No `.claude/` / `AGENTS.md` / `fixture_output.txt` / `hook_toast.md` staging; no destructive git; no force-push.
- WETH/USDC asset scope held; UniV2 + UniV3 0.05% + Sushi V2 venue scope held; no widening.

### Non-goals for Phase 6a

- Phase 6a does **NOT** ship a production signer impl.
- Phase 6a does **NOT** introduce funded key material into the workspace.
- Phase 6a does **NOT** flip `live_send` capability.
- Phase 6a does **NOT** call `eth_sendBundle` from any runtime path.
- Phase 6a does **NOT** perform actual relay submission. `submit_bundle` may grow per-adapter kill-switch and signer-routing precedence rules but MUST continue to terminate with `Err(SubmitDisabled)` (or `Err(KillSwitchActive)`, or `Err(SignerDisabled)` propagated as the appropriate boundary variant).
- Phase 6a does **NOT** open Phase 6b Production Gate work.
- Phase 6a does **NOT** widen asset scope or venue set.

If any of these non-goals are reconsidered mid-phase, **HALT** and require explicit user authorization for that scope expansion.

## Phase 6a provisional batch breakdown

Foundation-first ordering. Each row is independent of actual submission and remains fail-closed.

| Batch | Goal | Pre-impl review |
|---|---|---|
| P6-A | Phase 6a safety boundary refinement: write `docs/specs/phase-6a-boundary.md` capturing the exact contract between submission boundary (`submit_bundle`), kill-switch (`KillSwitch`), and signer trait (`Signer`); enumerate the Phase 6a `Result::Err` ordering rules (kill switch FIRST, signer second, submit-disabled last); enumerate the Phase 6a grep-gate set carried + extended from Phase 5. No code change. | **YES** (overview-adjacent; boundary spec is the basis for every subsequent batch) |
| P6-B | Signing-request pipeline, still fail-closed. **Reuse** existing `crates/signer::BundleTx` (P5-C DP-C3) and `crates/signer::SignedTxBytes` (P5-C DP-C10) — NO new boundary type in `crates/types`. Add a `BundleConstructor`-internal signing-request hook that **always** routes to the `DisabledSigner` from `crates/signer` and **always** returns `Err(SignerError::SignerDisabled)`. `crates/execution` gains a `signer = { path = "../signer" }` dep edge (this is the new `Signer`-using site). `crates/app` ALSO gains the edge for the `wire_phase4` ctor injection. Both crates extend G2c/G2d allow-list. `wire_phase4` accepts `Arc<dyn Signer>` wired to `Arc::new(DisabledSigner::default())`. No production signer; no key material. | **YES** (introduces the FIRST `crates/execution` → `crates/signer` and `crates/app` → `crates/signer` dependency edges; trait-shape decisions cascade into P6-C and P6-E) |
| P6-C | Relay `eth_callBundle` with **signed-tx-bytes placeholder**, read-only simulation only. **Existing API** (NOT a migration): `RelaySimulator::simulate_bundle(req: RelaySimRequest) -> ...` with `RelaySimRequest::txs: Vec<Vec<u8>>`. In Phase 6a, the bytes vector is **always empty** because the signer is `DisabledSigner` — adapters MUST short-circuit with `Err(UnsignedBundleUnavailable)` (P4-E behavior) and never reach the network. Adds **wiremock-only** adapter tests that, when fed fixture pre-computed test-vector RLP bytes (no key material implied), assert the adapter formats `eth_callBundle` correctly. NO live relay test code in Phase 6a (Q-P6-D resolved below). No real signing. No `eth_sendBundle`. | **YES** (interacts with the existing `relay-sim` comparator + relay-clients adapters; mis-wiring here is a submission-path risk) |
| P6-D | Per-adapter kill switch wiring + submission-boundary hard guards. Thread the existing cloneable `KillSwitch` newtype (P5-D `pub struct KillSwitch(Arc<AtomicBool>)`, `#[derive(Clone)]`) **directly** into Flashbots + bloXroute adapter constructors — NO `Arc<KillSwitch>` (the type is already internally `Arc<AtomicBool>`; double-Arc is unjustified). `submit_bundle` impls MUST check `kill_switch.is_active()` BEFORE any other branch and return `Err(KillSwitchActive)` first (trait-doc PRECEDENCE rule from P5-D). Add G10 (per-adapter `submit_bundle` first-statement is kill-switch check) grep gate. NO new `submit_bundle` callers in `crates/app/src/`. | **YES** (R-P5-1 carry-forward: kill switch is an execution-safety.md device; per-adapter wiring is the deferred P5-D item) |
| P6-E | Production signer design review, disabled by default. Document the Phase 6b unlock contract (HSM/KMS-only, never-in-memory key material, audit log shape, key rotation, lifecycle, threat model). NO impl. NO `secp256k1`/`k256`/`alloy-signer`/`ethers-signers`/`Wallet`/`PrivateKey` symbols added. `DisabledSigner` remains the only impl. Document MUST land at `docs/specs/production-signer.md` and be referenced from `docs/specs/execution-safety.md`. | **YES** (spec-class change; the design itself is the deliverable and must be reviewed before any 6b impl planning) |
| P6-F | Phase 6a DoD audit + `phase-6a-complete` annotated tag. Audit P6-A..E completion. Run all Phase 5 carry-forward safety grep gates G1..G9 + new Phase 6a invariants (G10 kill-switch-first; G11 signer-routing-fail-closed). Tag draft. | NO (audit/tag-only per P4-G / P5-E precedent) |

Batch independence: P6-A is a pure-doc batch and MUST land first. P6-B introduces **two** signer dep edges — `crates/execution` → `crates/signer` (primary, the `Signer`-using site) and `crates/app` → `crates/signer` (secondary, the ctor-injection site) — and the signing-request hook. P6-C does **NOT** depend on P6-B for the `BundleTx` shape (Codex v0.2 verdict, item 4: `BundleTx` already exists in `crates/signer` at P5-C DP-C3; P6-C only references the existing `RelaySimRequest::txs: Vec<Vec<u8>>` API and the pre-computed test-vector RLP fixtures). P6-C can in principle land before P6-B; the recommended ordering keeps B before C so the integration test "with `DisabledSigner`, chain terminates at signer before relay-sim path" has its dependency in place. P6-D is independent of B/C and could run in parallel, but lands after them to keep the per-adapter-kill-switch wiring tests aligned with the post-B/C trait surfaces. P6-E is a doc-only batch and can land any time after P6-A but is most useful after P6-B/C/D so the design doc can cite real boundary shapes. P6-F is closeout-only.

## Per-batch detail

### P6-A — Phase 6a safety boundary refinement

**Scope**:

- NEW `docs/specs/phase-6a-boundary.md` capturing:
  - Submission boundary: `submit_bundle` PRECEDENCE rule (kill switch first → signer disabled second → submit disabled last). Lock the ordering as a trait-doc invariant + a grep-asserted invariant.
  - Signer boundary: **`crates/execution` is the primary `Signer`-using boundary** (the `BundleConstructor::with_signer` ctor + internal signing-request hook invoke `Signer::sign_tx`). `crates/app` is the secondary boundary (ctor-injection only, via `wire_phase4` accepting `Arc<dyn Signer>`). Both depend on `crates/signer` as `Arc<dyn Signer>` only; concrete `DisabledSigner` is the only impl Phase 6a ships.
  - Kill switch boundary: per-driver (P5-D) + per-adapter (P6-D) layered guards; both must be operable independently.
  - Grep-gate set: carry-forward G1..G9 from P5-E DoD; new G10 (per-adapter `submit_bundle` first-statement kill-switch check); new G11 (signer-routing fail-closed at `BundleConstructor` boundary).
- Cross-link `docs/specs/execution-safety.md` to the new boundary doc.

**Forbidden in P6-A**:

- Any `.rs` edit.
- Any `Cargo.toml` edit.
- Any ADR amendment.
- Any new dependency.

**Tests**: none (doc-only batch).

**Gates**: doc-only; the existing CI gates run on the doc commit but no test delta is expected.

### P6-B — signing-request pipeline (fail-closed)

**Scope**:

- **Reuse** existing `crates/signer::BundleTx` (P5-C DP-C3 boundary type at `crates/signer/src/bundle_tx.rs:23`) — Q-P5-4 standing was already implemented in P5-C. **NO** duplicate `BundleTx` in `crates/types`.
- **Reuse** existing `crates/signer::SignedTxBytes` (P5-C DP-C10 transparent `Vec<u8>` newtype at `crates/signer/src/bundle_tx.rs:68`) for the signed-bytes output type.
- Extend `BundleConstructor` (`crates/execution/src/lib.rs:127`) with a `with_signer(Arc<dyn Signer>)` ctor that stores the signer; a `BundleConstructor`-internal signing-request hook calls `Signer::sign_tx(&BundleTx) -> Result<SignedTxBytes, SignerError>`. In Phase 6a, every call returns `Err(SignerError::SignerDisabled)` because the injected signer is `DisabledSigner`.
- `Cargo.toml` of **`crates/execution`** adds `signer = { path = "../signer" }` — this is the new `Signer`-using site.
- `Cargo.toml` of `crates/app` ALSO adds `signer = { path = "../signer" }` — the ctor-injection site.
- `wire_phase4` accepts `Arc<dyn Signer>` and threads it into `BundleConstructor::with_signer(...)`. Existing `with_strategy` chain compatible.
- G2c allow-list is extended by **two** sites: (a) `crates/execution` (the `BundleConstructor::with_signer` ctor + the internal signing-request hook); (b) `crates/app` (the `wire_phase4` `Arc<dyn Signer>` injection).
- G2d is **redefined for Phase 6a as an explicit allow-list gate**: the previously zero-hit `Signer` / `DisabledSigner` / `SignerError` / `SignerDisabled` token leak gate becomes a positive allow-list of approved file:line pairs in `crates/execution/` and `crates/app/`. The P6-B pre-impl plan enumerates the allow-listed file:line pairs; G2d at P6-F asserts (i) **zero hits outside the allow-list** (i.e., outside `crates/signer/` and the approved `crates/execution` / `crates/app` sites) AND (ii) every approved site is still in place. Removing or relocating an approved site without updating the allow-list is a gate failure.

**Forbidden in P6-B**:

- Any production signer impl (test-ephemeral or otherwise).
- Any private key material.
- Importing `secp256k1` / `k256` / `alloy-signer` / `ethers-signers` / `Wallet` / `PrivateKey`.
- `tracing::*` calls that log signer state beyond `signer_set: bool`.
- Populating `RelaySimRequest::txs` with non-empty bytes (P6-C scope).
- Any duplicate `BundleTx` or `SignedTxBytes` type in `crates/types` or `crates/execution` — reuse from `crates/signer` only.

**Tests** (lean):

- `BundleConstructor::with_signer(DisabledSigner)` → signing-request hook returns `Err(SignerError::SignerDisabled)` on invocation.
- `wire_phase4` injects `Arc<DisabledSigner>` by default; no other impl reachable.
- G2c allow-list updated for **both** `crates/execution` and `crates/app` sites.
- G2d (redefined per Scope above) re-verified: zero hits **outside** the explicit allow-list of approved file:line pairs in `crates/signer/` + the two new sites in `crates/execution/` and `crates/app/`; every approved site present.

### P6-C — relay `eth_callBundle` with signed-tx-bytes placeholder (read-only, wiremock-only)

**Scope**:

- **No API migration**: the existing `RelaySimulator::simulate_bundle(req: RelaySimRequest)` trait method at `crates/relay-sim/src/lib.rs:145-146` and the existing `RelaySimRequest::txs: Vec<Vec<u8>>` field at `crates/relay-sim/src/lib.rs:86-90` are the API. P6-C documents the contract: `txs` carries RLP-encoded signed-transaction wire bytes produced by the future production signer.
- In Phase 6a, `BundleConstructor::with_signer(DisabledSigner)` propagates `Err(SignerError::SignerDisabled)` BEFORE the relay-sim path is reached. The relay adapters (Flashbots at `crates/relay-clients/src/flashbots.rs:109`, bloXroute at `crates/relay-clients/src/bloxroute.rs:118`) MUST also short-circuit with `Err(UnsignedBundleUnavailable)` (P4-E behavior) when the `txs` vector is empty, as a second-layer guard.
- Add **wiremock-only** adapter tests that, when fed fixture pre-computed test-vector RLP bytes (a public test vector with no key material implied — pre-computed and committed as a fixture in `crates/relay-clients/tests/fixtures/`), assert the adapter formats `eth_callBundle` correctly.
- **NO live relay test code in Phase 6a**, env-gated or otherwise. Q-P6-D resolved to wiremock-only; any live test relay integration requires separate explicit user approval and a new batch (not a P6-C extension).

**Forbidden in P6-C**:

- Any real `eth_sendBundle` call.
- Any live-network test code, env-gated or otherwise (resolved Q-P6-D).
- Any signed-tx bytes generated at runtime by a real signer.
- Populating production credentials into any test or CI secret.
- Renaming `simulate_bundle` → `sim_bundle` or changing `RelaySimRequest::txs` from `Vec<Vec<u8>>` to `Vec<Bytes>` (API migration is out of scope for Phase 6a; if Codex wants the migration, it requires its own pre-impl plan).

**Tests** (lean, wiremock-only):

- Adapter formats `eth_callBundle` request body correctly with fixture-signed bytes (wiremock).
- Adapter returns `Err(UnsignedBundleUnavailable)` when `RelaySimRequest::txs` is empty.
- End-to-end through `BundleConstructor::with_signer(DisabledSigner)`: chain terminates at the signer with `Err(SignerError::SignerDisabled)`; relay-sim path never invoked.

### P6-D — per-adapter kill switch wiring + submission-boundary hard guards

**Scope**:

- Thread the existing cloneable `KillSwitch` newtype directly into Flashbots + bloXroute adapter constructors. Per `crates/bundle-relay/src/kill_switch.rs:17` (`pub struct KillSwitch(Arc<AtomicBool>); #[derive(Clone)]`), the type is **already** internally `Arc<AtomicBool>` and `Clone` is cheap (Arc clone). Both adapters take `kill_switch: KillSwitch` as a constructor arg — **NOT** `Arc<KillSwitch>` (double-Arc is unjustified).
- `submit_bundle` impls in both adapters: FIRST statement is `if self.kill_switch.is_active() { return Err(BundleRelayError::KillSwitchActive); }`. Existing `Err(SubmitDisabled)` second.
- Add G10 grep gate: every `impl BundleRelay for ... { fn submit_bundle(...) }` first non-trivia statement is the kill-switch check. Implementation: ripgrep pattern asserting the literal `kill_switch.is_active()` appears on the first statement line of each `submit_bundle` impl.
- `wire_phase4` clones the same `KillSwitch` instance used by `comparator_driver` (P5-D) into the adapter constructors — single source of truth; `set_active` flips all layers simultaneously (Arc-shared atomic).

**Forbidden in P6-D**:

- Introducing any `submit_bundle` caller in `crates/app/src/` (G3 must remain zero-hit).
- Bypassing the kill switch via any helper or feature flag.
- Adding any `submit_bundle` code path that returns `Ok(_)` (even with fixture bytes).

**Tests** (lean):

- Per-adapter unit test: `kill_switch.set(true); adapter.submit_bundle(...)` returns `Err(KillSwitchActive)`.
- Per-adapter unit test: `kill_switch.set(false); adapter.submit_bundle(...)` returns `Err(SubmitDisabled)` (PRECEDENCE second).
- G10 grep gate green.
- G3 / G4 unchanged from P5-E (zero hits in `crates/app/src/`).

### P6-E — production signer design review (disabled by default)

**Scope**:

- NEW `docs/specs/production-signer.md` capturing the Phase 6b unlock contract:
  - HSM/KMS-only key material; never in process memory; never in env vars; never in repo.
  - Audit log shape: structured event per signing op including `bundle_correlation_id`, `chain_id`, signing-key fingerprint (NOT the key), and a monotonic counter.
  - Key rotation policy + zero-downtime rotation.
  - Lifecycle (initialization, ready/disabled state, drain on shutdown).
  - Threat model: key extraction, rogue operator, supply-chain dependency.
  - Explicit list of allowed crates (HSM/KMS SDK only; NO `secp256k1` / `k256` / `alloy-signer` / `ethers-signers` direct use).
- Cross-link from `docs/specs/execution-safety.md` §"Funded Key / Prod Signer Ban".
- The doc itself MUST contain a self-describing review checklist for the Phase 6b kickoff.

**Forbidden in P6-E**:

- Any `.rs` change.
- Any `Cargo.toml` change.
- Any code-level signer impl.
- Any key material (even placeholder / test-vector public-key without private counterpart is discouraged; if needed for the doc, use a fully synthetic non-key string and mark it as such).

**Tests**: none (doc-only batch).

### P6-F — Phase 6a DoD audit + `phase-6a-complete` tag

**Scope**:

- Audit P6-A..E completion against this overview.
- Run all Phase 5 carry-forward safety grep gates G1..G9 PLUS new Phase 6a invariants G10 (per-adapter kill-switch first) + G11 (signer-routing fail-closed).
- DoD audit doc per the P5-E template.
- Create + push `phase-6a-complete` annotated tag.

**Forbidden**:

- Tag if any safety ambiguity surfaced — write outbox for Codex review instead.
- Tag before all P6-A..E gates pass.
- Any scope leak into Phase 6b territory.

## Explicit treatment of cross-cutting items

| Item | Phase 6a disposition |
|---|---|
| Production signer impl | **NOT** shipped. `DisabledSigner` remains the only impl. Design doc only (P6-E). |
| Funded key material | **BANNED** throughout Phase 6a. |
| Signed tx bytes at runtime | **NOT** generated by a real signer. P6-C uses pre-computed public test-vector fixtures (no key material) for adapter format assertion. |
| `submit_bundle` runtime call | Stays `Err(SubmitDisabled)` (or `Err(KillSwitchActive)` if kill-switch set). G3 remains zero-hit in `crates/app/src/`. |
| `live_send = true` | Stays config-validation rejected. NO change in Phase 6a. |
| `eth_sendBundle` | **BANNED** throughout Phase 6a. Phase 6b only. |
| `eth_callBundle` | Documented + adapter-format asserted in P6-C via **wiremock-only** tests. NO live relay test code in Phase 6a (Q-P6-D resolved v0.2). |
| Kill switch | P5-D per-driver guard + P6-D per-adapter guard; both layers operable. |
| Secret redaction | Phase 4 R-E20 redaction tests carry forward; no regression allowed. |
| Journal/abort policy | P4-E R-E9 mismatch journal append+flush before broadcast carries forward; no regression. |
| Dynamic gas bidding | P5-B `BidStrategy` carries forward; no further widening in 6a. |
| Asset scope | WETH/USDC only. UniV2 + UniV3 0.05% + Sushi V2. NO widening. |
| ADR amendment | NONE. Phase 6a unlock language (if any) recorded in batch execution notes only. Open question Q-P6-C below. |

## Hard forbids during all of Phase 6a

- No production signer impl.
- No funded key.
- No private key material in repo / tests / fixtures / configs / env examples / runtime.
- No `live_send = true`.
- No `eth_sendBundle`.
- No actual relay submission.
- No real paid API dependency enabled in CI by default.
- No live-network test enabled by default.
- No Phase 6b Production Gate work.
- No `.claude/` / `AGENTS.md` / `fixture_output.txt` / `hook_toast.md` staging.
- No destructive git, no force-push.
- No asset-scope widening; no extra V3 fee tiers; no extra venues.
- No ADR text amendment without separate explicit user approval.

## Phase 6b Production Gate (kept entirely separate)

Phase 6b is **NOT** in scope for this overview. Phase 6b will require:

- A separate Phase 6b overview doc (`docs/superpowers/plans/<date>-phase-6b-overview-execution.md`).
- Fresh **explicit user authorization** with the literal phrase "Phase 6b kickoff" or equivalent unambiguous wording.
- Reference to the P6-E `docs/specs/production-signer.md` design as the contract being implemented.
- A separate Codex review pass before any 6b implementation work.

Phase 6b owns:

- Production signer impl (HSM/KMS-backed `Signer` trait impl in `crates/signer`).
- Funded key material wiring (operationally; never in repo).
- `live_send = true` capability (config-validation flip).
- `eth_sendBundle` runtime call path.
- Actual relay submission (`submit_bundle` returning `Ok(_)` for the first time in project history).
- Live submission monitoring + post-submission journal contract.

None of the above may be touched in Phase 6a.

## Codex Q-P6 open questions

- **Q-P6-A — First Phase 6a implementation batch.** Recommend **P6-A (boundary spec)** as the first batch, since it locks the contract that all subsequent batches must satisfy. Alternatives: P6-D (per-adapter kill switch) is also a strong candidate because it closes the P5-D deferred item and is independent of signer-shape decisions. Codex to verdict.
- **Q-P6-B — Pre-impl Codex review on which batches.** Default: P6-A, P6-B, P6-C, P6-D, P6-E ALL require pre-impl review (safety-relevant). P6-F is audit/tag-only (NO). Codex to confirm or relax.
- **Q-P6-C — ADR amendment required?** Recommend **NO**. Phase 6a unlock language (if any — most batches are pure code/doc additions within existing ADR scope) recorded in batch execution notes only. ADR-001 already names Phase 6 gates explicitly; ADR-006 already allows the dynamic-bidding unlock per P5-B. Codex to confirm or flag any ADR section needing amendment.
- **Q-P6-D — Live relay simulation: wiremock-only, or env-gated live test relay too?** **RESOLVED (v0.2, post-Codex REVISION REQUIRED)**: **wiremock/mock-only** for Phase 6a. Any live relay test code, even env-gated, requires **separate explicit user approval** and a new batch (not a P6-C extension). P6-C ships zero live-network test code.
- **Q-P6-E — Production signer in Phase 6a: entirely stubbed or scaffold-only?** Recommend **entirely stubbed**. `DisabledSigner` is the only impl shipped; the design doc (P6-E) is the only forward-looking artifact. NO scaffolding of an HSM/KMS-backed impl in Phase 6a.
- **Q-P6-F — Exact safety grep gates carrying from Phase 5.** Recommend carrying all of G1..G9 verbatim from P5-E DoD audit. New Phase 6a gates: G10 (per-adapter `submit_bundle` first-statement kill-switch check) + G11 (signer-routing fail-closed at `BundleConstructor::with_signer` boundary). **G2c allow-list expanded by exactly TWO sites**: (a) `crates/execution` (the `BundleConstructor::with_signer` ctor + internal signing-request hook — the primary `Signer`-using site); (b) `crates/app` (the `wire_phase4` `Arc<dyn Signer>` injection — the ctor-injection site). G2d redefined to a positive allow-list gate (P6-B Scope): zero hits **outside** the approved file:line pairs in `crates/signer/` + the two `crates/execution` / `crates/app` sites. Codex to verdict on the precise G10/G11/G2d wording.
- **Q-P6-G — Phase 6a tag policy.** Recommend `phase-6a-complete` annotated tag at P6-F, matching `phase-{1..5}-complete` precedent. Phase 6b will get its own `phase-6b-complete` tag when (and only when) the Production Gate passes.
- **Q-P6-H — Signer dependency edges.** P6-B introduces **two** new signer dep edges for the first time: (a) `crates/execution` → `crates/signer` (primary, the `Signer`-using site at `BundleConstructor::with_signer`); (b) `crates/app` → `crates/signer` (secondary, ctor-injection only). Codex to verdict whether both edges should be direct `signer = { path = "../signer" }`, or routed through a shim crate (overhead probably not worth it for these sites — recommend direct on both).

## Recommended first implementation batch after Codex review

**P6-A — Phase 6a safety boundary refinement** (doc-only). Rationale: every subsequent batch's correctness depends on the boundary contract being explicit and reviewed. P6-A produces no code change, so it is the lowest-risk first step and lets Codex verdict the boundary shape before any `.rs` edit lands.

If Codex prefers a different ordering (Q-P6-A), Claude follows the verdict.

## Untracked-file housekeeping

At Phase 6 planning start (this turn), the working tree contains these untracked items:

- `AGENTS.md` — coordination artifact, **never-stage** (per CLAUDE.md policy).
- `fixture_output.txt` — CI fixture scratch, **never-stage**.
- `hook_toast.md` — **CLASSIFIED v0.2 as never-stage** (coordination scratch in the same category as `AGENTS.md`). Codex/user to confirm at v0.2 review; if reclassified, the v0.3 overview must reflect that disposition.
- `.claude/` — already gitignored; **never-stage**.
- `docs/superpowers/plans/2026-05-16-phase-6-overview-execution.md` — **this overview doc**. **UNCOMMITTED** in this turn by design (the Phase 5 overview ac07024 precedent: overview committed only after Codex APPROVED). On APPROVED verdict, this doc is the first thing committed + pushed as a routine doc commit.

Expected post-APPROVED `git status` set: `AGENTS.md`, `fixture_output.txt`, `hook_toast.md`, `.claude/` (all unchanged; the overview doc moves from untracked → tracked at commit).

## Process

Per the 2026-05-04 routine-closeout policy + the 2026-05-04 22:20 KST manual-Codex-review-mode + the user's 2026-05-16 explicit Phase 6 planning-only instruction (overview stays UNCOMMITTED on disk until Codex APPROVED):

1. Claude writes this overview to disk (UNCOMMITTED) + emits the review pack to `.coordination/claude_outbox.md`.
2. Claude STOPS and reports "manual Codex review required".
3. User pastes pack to Codex; relays verdict.
4. Claude records verdict in `.coordination/codex_review.md`.
5. **APPROVED** → commit + push this overview as a routine doc commit; THEN draft the P6-A pre-impl plan (if P6-A is the agreed first batch); THEN await Codex P6-A verdict before any code change.
6. **REVISION REQUIRED** → revise overview in place + re-emit pack.
7. **Scope / ADR change required** → HALT to user (any item from Phase 6a hard-forbids list, any ADR text change, any Phase 6b kickoff request).

No code or `Cargo.toml` edits in this turn. No commit. No push. No tag.
