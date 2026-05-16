# Phase 6b Production Gate -- Overview (planning only)

**Date:** 2026-05-16 KST
**Status:** Draft v0.2 (revised after Codex REVISION REQUIRED HIGH on v0.1, 2026-05-16 KST). Five v0.1 -> v0.2 fixes:
(1) **Prerequisite list reframed as a non-sequential checklist.** v0.1 numbered six items "in order" but the numbering implied production-signer Codex review (item 3) + host-compromise control selection (item 4) BEFORE the P6B-A boundary doc + ADR amendment (items 5+6), which contradicted the actual P6B-A-first batch sequence. v0.2 renames the section "Phase 6b unlock prerequisites (non-sequential checklist)" and explicitly states the **landing order is the per-batch sequence (P6B-A first)**; the six prerequisites are an unordered set of conditions that ALL must be true before any Phase 6b live-action gate (P6B-D config flip or P6B-E live submission) unlocks. The actual prerequisite-completion timing is named per-prerequisite (e.g., "satisfied during P6B-A pre-impl review", "satisfied during P6B-B pre-impl review").
(2) **ADR-001 amendment review timing made consistent.** v0.1 said "reviewed at P6B-A close" in one place and "as a single pre-impl pack" in another -- contradictory. v0.2 locks the model: **ADR-001 amendment text is PROPOSED in the P6B-A pre-impl plan and REVIEWED by Codex as part of the P6B-A pre-impl review pack** (alongside the Phase 6b boundary doc). The amendment text lands as a commit only AFTER (a) explicit user re-authorization for the ADR scope-lift, and (b) Codex APPROVED on the P6B-A pre-impl pack. There is no "at P6B-A close" review of ADR text; close happens after the amendment is already approved + committed within P6B-A.
(3) **Overview-turn hard-limit wording fixed.** v0.1 said "No `live_send = true` reference outside the existing `execution-safety.md` ban + reject prose" which was self-contradictory (this overview necessarily discusses `live_send = true` in prose to describe the P6B-D unlock). v0.2 reframes the hard-limit as: forbid CODE / CONFIG / RUNTIME enablement of `live_send = true`; prose discussion of the capability + its unlock semantics is allowed in `docs/` (precedent: `docs/specs/execution-safety.md` line 32 + `docs/specs/phase-6a-boundary.md` Section 6 already discuss `live_send = true` in prose).
(4) **`docs/specs/` vs `docs/superpowers/plans/` path confusion fixed.** v0.1 hard-limit said "No `docs/specs/` edit other than this overview file's own commit" but the new overview lives under `docs/superpowers/plans/`, NOT `docs/specs/`. v0.2 states plainly: NO `docs/specs/` edits in the overview turn; the only new file is the overview itself under `docs/superpowers/plans/`.
(5) **P6B-E wiremock-only wording clarified.** v0.1 said "wiremock-only end-to-end against test relays" which read as "test-net relay infrastructure". v0.2 clarifies: **local-only wiremock MockServer instances binding to 127.0.0.1**; no live test-net relay endpoint, no Goerli/Sepolia relay infrastructure, no externally-reachable URL. Mirrors the P6-C v0.3 D-T precedent (`crates/relay-clients/tests/{flashbots,bloxroute,submit_disabled}.rs` all use `MockServer::start()` which binds to localhost).
Awaiting manual Codex re-review.

**v0.1 status retained for traceability:** Draft v0.1 (`docs/superpowers/plans/2026-05-16-phase-6b-overview-execution.md` on disk before v0.2 edits). UNCOMMITTED. ASCII-only. **This is an OVERVIEW, not a per-batch impl plan.** No batch in this overview is authorized to begin implementation; every Phase 6b batch requires its own pre-impl plan + separate Codex review + the explicit user authorizations enumerated in Section "Phase 6b unlock prerequisites (non-sequential checklist)" below.
**Predecessor:** `phase-6a-complete` annotated tag at `bd0a53c` (tag object `3c9faaf`); `master` HEAD `6fa108b`; workspace **239 passed + 1 ignored**.

## Phase 6b user-approval basis

The user explicitly authorized **Phase 6b OVERVIEW DRAFTING ONLY** on 2026-05-16 KST. This authorization does NOT cover any of the Phase 6b non-goals below, each of which requires a separate explicit user re-authorization at the batch-plan stage:

- Production signer impl in `crates/signer/` or anywhere else in the workspace.
- Funded private key material in repo / tests / fixtures / configs / env / runtime.
- `live_send = true` capability flip (config-validation un-rejection).
- `eth_sendBundle` runtime call path.
- Actual relay submission (any bundle broadcast through any path).
- Paid live API dependency enabled in CI by default.
- Live-network tests enabled by default.
- ADR-001 text amendment.
- Asset / venue / V3-fee-tier widening.

Phase 6b is the **ONLY** path to live action per ADR-001 + `docs/specs/execution-safety.md` + `docs/specs/production-signer.md`. Phase 6a remains the fail-closed baseline at `phase-6a-complete`; no Phase 6b batch may regress any Phase 6a Hard Safety Invariant (HSI-1..HSI-11) without that regression being the explicit point of a reviewed batch and the relaxation being captured in the Phase 6b boundary doc (see P6B-A below).

## Baseline at Phase 6b start

- `master` HEAD `6fa108b` (CLAUDE.md Phase 6a wrap-up); `phase-6a-complete` tag at `bd0a53c` (tag object `3c9faaf`); both pushed to `origin`.
- Workspace baseline: **239 passed + 1 ignored** (the ignored test is the carry-forward P2-C `g_state_live` env-contract live-smoke stub).
- 20 workspace crates unchanged from `phase-5-complete`.
- Phase 6a Hard Safety Invariants HSI-1..HSI-11 verified at the tag (full verbatim audit in `.coordination/claude_outbox.md` P6-F closeout):
  - HSI-1 (G1): `eth_sendBundle` doc-comment only in `crates/` (5 `//!` hits).
  - HSI-2 (G2a): 0 hits of `Wallet|PrivateKey|secp256k1|\bk256\b|sign_transaction|funded` in `crates/*.rs`.
  - HSI-3 (G2b): 0 hits of `alloy-signer|ethers-signers|secp256k1|k256` in `crates/**/Cargo.toml`.
  - HSI-4 (G2c/G2d): Signer-symbol allow-list = `crates/signer/` (5 files) + 3 approved file entries (`crates/execution/src/lib.rs`, `crates/app/src/lib.rs`, `crates/app/tests/wire_phase4.rs`); 0 hits outside.
  - HSI-5 (G2e): exactly 2 `signer = { path = "../signer" }` dep edges (`crates/execution/Cargo.toml` + `crates/app/Cargo.toml`).
  - HSI-6 (G3): 0 `submit_bundle(` callers in `crates/app/src/`.
  - HSI-7 (G4): 0 `dyn BundleRelay` / `Arc<dyn BundleRelay>` in `crates/app/src/`.
  - HSI-8 (G5): `live_send = true` config-validation reject preserved with Display literal `"relay.live_send=true is forbidden until Phase 6b Production Gate"`.
  - HSI-9 (G6/G7): `api_key` never in `tracing::*!`; `#[ignore]` count = 1 (P2-C carry-forward only).
  - HSI-10 (G8/G9): no workspace dep cycles; `KillSwitch` reach allow-list = `crates/bundle-relay/` + `crates/app/` + `crates/relay-clients/`.
  - HSI-11 (G10/G11): each adapter `submit_bundle` body FIRST non-trivia statement is the kill-switch guard (P6-D enforcement); single `sign_tx` production call site at `crates/execution/src/lib.rs:238` routed through `&dyn Signer` (P6-B enforcement).
- The `crates/signer::SignerError::SignerDisabled` `Display` literal `"Phase 6b Production Gate"` is the canonical forward-link from runtime code to this gate. Once Phase 6b production code lands, that literal will continue to surface from any code path that still reaches `DisabledSigner` (e.g., in tests, in unreachable branches, or in shadow paths).

## Phase 6b unlock prerequisites (non-sequential checklist)

Phase 6b requires ALL SIX prerequisites below to be satisfied before any **live-action gate** (P6B-D `live_send=true` config flip; P6B-E `eth_sendBundle` runtime + actual submission) may unlock. **This list is a checklist of conditions, NOT a sequential to-do list.** The actual landing order is the per-batch sequence in Section "Phase 6b provisional batch breakdown" (P6B-A boundary doc + ADR amendment FIRST; then P6B-B signer impl + host-compromise control; then P6B-C key wiring; then P6B-D live-send flip; then P6B-E live submission; then P6B-F audit + tag). Each prerequisite is satisfied at a specific point in that sequence, named per-prerequisite below.

| # | Prerequisite | Satisfied at |
|---|---|---|
| 1 | **Fresh explicit user authorization** with unambiguous wording for the specific Phase 6b non-goal being lifted (e.g., "approve P6B-C funded-key wiring" or "approve P6B-D `live_send=true` flip"). The Phase 6b overview drafting authorization (this turn) does NOT carry forward to any implementation batch. | At the START of each Phase 6b implementation batch (P6B-A..F). One re-authorization per batch. |
| 2 | **This Phase 6b overview document** under `docs/superpowers/plans/2026-05-16-phase-6b-overview-execution.md`. | Drafted in this turn (v0.2); requires Codex APPROVED before any per-batch pre-impl planning begins. |
| 3 | **A Phase 6b boundary document** under `docs/specs/phase-6b-boundary.md` (separate from `phase-6a-boundary.md`). Captures the runtime contract for `live_send`, `eth_sendBundle`, funded-key wiring, the per-gate fail-closed posture, and the new G-gates (e.g., G12) replacing relaxed Phase 6a gates. | Authored + reviewed + landed in **P6B-A** (the first Phase 6b implementation batch, BEFORE any other Phase 6b batch can begin planning). |
| 4 | **ADR-001 amendment text** user-authorized to lift the funded-key / prod-signer ban for the scoped Phase 6b context. The amendment scopes the lift to the specific HSM/KMS-backed impl from P6B-B + the specific control mix from P6B-B + the specific live-send config-flip mechanism from P6B-D. | **Proposed in the P6B-A pre-impl plan and reviewed by Codex as part of the P6B-A pre-impl review pack** (alongside the Phase 6b boundary doc). The amendment text lands as a commit only AFTER (a) explicit user re-authorization for the ADR scope-lift, AND (b) Codex APPROVED on the P6B-A pre-impl pack. Per CLAUDE.md, ADR-text amendments require explicit user approval. |
| 5 | **A Codex review against `docs/specs/production-signer.md` Section 2 contract** (Section 2.1 HSM/KMS-only key custody, Section 2.2 never-in-memory key material, Section 2.3 positive auditability requirement, Section 2.4 key rotation + lifecycle, Section 2.5 threat model). Reviewer verifies that the proposed production `Signer` impl satisfies every Section 2 requirement before it can replace `DisabledSigner`. | At the **P6B-B pre-impl review** (where the production `Signer` impl + host-compromise control are proposed). |
| 6 | **At least one non-trivial host-compromise control** per `docs/specs/production-signer.md` Section 2.5 residual. Phase 6b MUST land at least one of: per-bundle pre-sign attestation, request-authorization rate limits at the HSM/KMS, pre-sign mismatch-comparator gating, or operator-visible signing audit log linked to the opportunity/bundle chain per Section 2.3. | Selected in the **P6B-B pre-impl plan** + landed as code in P6B-B itself. v0.1 RECOMMENDS layering at least TWO controls if cost permits; ONE non-trivial control is the hard minimum. |

**Live-action gates stay fail-closed until ALL SIX are satisfied.** Specifically: P6B-D and P6B-E may not begin pre-impl planning until P6B-A is fully closed (#3 + #4 satisfied) AND P6B-B is fully closed (#5 + #6 satisfied) AND the funded-key wiring in P6B-C is reviewed-closed. The actual order of completion is the per-batch sequence below; the prerequisites above are simply the unordered set of conditions that must hold true at the moment a live-action gate unlocks.

## Phase 6b safety assumptions and non-goals (this overview only)

Even though Phase 6b is the live-action gate, NO live action is authorized by this overview alone. The hard limits for **this overview turn** are:

- No production signer impl drafted, sketched, or proposed beyond what `docs/specs/production-signer.md` Section 2 already locks. (Prose discussion of the impl's contract is allowed; code/struct/trait-impl snippets are NOT.)
- No private key material referenced by name, fingerprint, or fixture.
- No funded key wiring code, config example, or env-example.
- No CODE / CONFIG / RUNTIME enablement of `live_send = true`. Prose discussion of the `live_send = true` capability + its P6B-D unlock semantics is allowed in this overview (precedent: `docs/specs/execution-safety.md` line 32 + `docs/specs/phase-6a-boundary.md` Section 6 already discuss `live_send = true` in prose).
- No new `eth_sendBundle` runtime path or executable code in `crates/`. Prose mentions of `eth_sendBundle` + its P6B-E unlock semantics are allowed in this overview (precedent: `docs/specs/execution-safety.md` + `docs/specs/phase-6a-boundary.md` + the P6-F closeout already discuss `eth_sendBundle` in prose).
- No actual relay submission code, mock, or test that would issue a real HTTP request to a real relay.
- No live-network test enabled by default. Live-network tests added in Phase 6b batches (if any) require explicit user approval per batch and MUST be `#[ignore]`-gated by default with env-overlay opt-in.
- No paid live API dependency enabled in CI.
- No `Cargo.toml` change, no `.rs` change, no ADR text amendment, **no `docs/specs/` edit at all** in this overview turn. The only NEW file is the overview itself under `docs/superpowers/plans/2026-05-16-phase-6b-overview-execution.md`. No edit to any existing `docs/specs/*.md` file.
- No new workspace crate.
- No widening of asset (WETH/USDC) / venue (UniV2 + UniV3 0.05% + Sushi V2) / V3 fee-tier scope.

## Phase 6b unlock sequence (cross-references to production-signer.md)

The Phase 6b unlock sequence is structured so that **every gate that grants live-action capability is the SUBJECT of a separately-reviewed batch with explicit user re-authorization**. No two live-action gates may unlock in the same batch; each gate's batch is preceded by the controls that mitigate the risk it introduces.

| Gate to unlock | Risk introduced | Batch | Prerequisite controls landed in |
|---|---|---|---|
| HSM/KMS-backed `Signer` impl that returns `Ok(_)` from `sign_tx` (Section 2.1, Section 2.2) | Host can request signatures from HSM/KMS | P6B-B | P6B-A boundary doc + P6B-B itself lands at least one Section 2.5 host-compromise control |
| Funded key material operationally wired to the HSM/KMS (Section 2.1) | Real economic value at risk per signed bundle | P6B-C | P6B-A boundary + P6B-B signer impl + P6B-B control + ADR-001 amendment from P6B-A |
| `live_send = true` capability (config-validation flip on `crates/config`) | `submit_bundle` can no longer be statically proven to never broadcast | P6B-D | All of P6B-A + P6B-B + P6B-C + a per-environment live-send config-overlay rejection that defaults `live_send = false` everywhere except an operator-controlled production profile |
| `eth_sendBundle` runtime path (replaces `submit_bundle` returning `Err(SubmitDisabled)` with `Ok(_)`) | Actual bundle broadcast to a relay | P6B-E | All of P6B-A..D + P6B-E lands the final submission contract: `submit_bundle` -> `Result<SubmissionReceipt, BundleRelayError>` returns `Ok(_)` only after the kill-switch + signer + sim-mismatch-comparator pre-checks all pass. The G3 + G4 invariants are **explicitly relaxed in P6B-E for `crates/app/src/`** under the Phase 6b boundary doc; the relaxation is documented per-callsite. |
| Live-action production sign-and-submit end-to-end | The actual MEV bot is broadcasting bundles | P6B-F | DoD audit + `phase-6b-complete` annotated tag. NO new live-action gate unlocked in P6B-F; it is audit + tag only. |

This sequence is the **only** order in which the Phase 6b gates may unlock. Reordering (e.g., flipping `live_send = true` before the signer is HSM/KMS-backed) is explicitly forbidden.

## Phase 6b provisional batch breakdown

Each row is a separate batch with its own pre-impl plan + Codex review + user re-authorization. Numbering is `P6B-A..F` to parallel the Phase 6a `P6-A..F` shape.

| Batch | Goal | Pre-impl review | Lands what gate |
|---|---|---|---|
| P6B-A | **Boundary doc + ADR-001 amendment.** Authors `docs/specs/phase-6b-boundary.md` capturing: which Phase 6a HSIs are intentionally relaxed in Phase 6b (HSI-6 G3 + HSI-7 G4 in P6B-E only; HSI-8 G5 in P6B-D only; HSI-2 G2a stays at 0 hits in `crates/*.rs` since key material lives in HSM/KMS not in source); the per-batch unlock contract; the new G-gates that replace the relaxed Phase 6a gates (e.g., G12 = "every `submit_bundle(...)` caller in `crates/app/src/` is guarded by kill-switch + signer + sim-mismatch-comparator pre-checks"). ADR-001 amendment user-authorized to scope-lift the funded-key + prod-signer ban to the specific Phase 6b context. | **YES** (boundary spec is the contract every subsequent Phase 6b batch must satisfy; ADR amendment requires explicit user approval before the batch lands; Codex reviews both as a single pre-impl pack). | None (the boundary doc itself is the contract; no live action unlocked). |
| P6B-B | **HSM/KMS-backed `Signer` impl** (`crates/signer/src/production.rs` or similar) populating the existing P5-C `Signer` trait. The impl is HSM/KMS-only per Section 2.1; no private-key bytes touch Rust process memory at any point per Section 2.2; the impl includes **at least one** Section 2.5 host-compromise control (selected at plan time from: per-bundle pre-sign attestation, request-auth rate limits at HSM/KMS, pre-sign mismatch-comparator gating, operator-visible signing audit log). Vendor selection is a sub-question for the batch plan; the boundary doc from P6B-A locks the vendor-neutral contract. G2c allow-list extended by the new file; G2a continues to return 0 hits (no `secp256k1` / `k256` / etc. -- the HSM/KMS client library is named at batch plan time and reviewed against G2b). | **YES** (the impl is the safety-critical artifact of Phase 6b; pre-impl plan must show the full surface against `production-signer.md` Section 2 contract before any code lands; Codex must verify Section 2.5 host-compromise control is non-trivial and is actually wired into the impl, not merely documented). | HSM/KMS-backed `Signer` impl that can return `Ok(_)` from `sign_tx`. **No relay submission yet** -- `submit_bundle` still returns `Err(SubmitDisabled)` because P6B-D / P6B-E have not landed. |
| P6B-C | **Funded key material wiring.** Operationally connect the HSM/KMS-managed key to the running process via the boundary contract from P6B-A. **NO private-key bytes in repo, tests, fixtures, configs, env-examples, build artifacts, or runtime memory.** The key fingerprint surfaces only in a boot-time audit-safe identifier per Section 2.4. Workspace tests + CI MUST continue to use the existing `DisabledSigner` path; the production `Signer` impl from P6B-B is reachable only under a config-gated production profile that is rejected in dev/test/shadow profiles. | **YES** (this batch is the first that touches funded key material at operational time; the Codex review must verify the no-private-key-bytes-in-repo invariant + the test/CI continuation of `DisabledSigner`). | Funded key material connected to the HSM/KMS (operationally, not in repo). Production `Signer` impl reachable under config-gated profile. |
| P6B-D | **`live_send = true` capability flip** (`crates/config` validation un-reject for the production profile only). The config-validation reject from Phase 4 P4-E DP-E9 is relaxed to: "`live_send = true` is permitted only when the active profile is the operator-controlled production profile that also names the HSM/KMS-backed `Signer` impl from P6B-B"; all other profiles (dev / test / shadow) continue to reject `live_send = true`. | **YES** (the only capability flip in Phase 6b that changes static safety properties; the Codex review must verify per-profile rejection logic is correct and that dev/test/shadow profiles cannot accidentally inherit the production profile's flip). | `live_send = true` permitted under production profile only. **Still no actual submission** -- `submit_bundle` still returns `Err(SubmitDisabled)` because P6B-E has not landed. |
| P6B-E | **`eth_sendBundle` runtime path + actual relay submission.** `submit_bundle` returns `Ok(SubmissionReceipt)` only after: kill-switch inactive + signer returns `Ok(SignedTxBytes)` + local sim passed + relay sim passed + sim-mismatch-comparator equal + bundle bytes match the simulated artifact byte-for-byte. The G3 + G4 invariants are explicitly relaxed: `crates/app/src/` may now contain `submit_bundle(` callers in the comparator-driven execution path. G10 (per-adapter kill-switch first-statement guard) is REINFORCED, not relaxed: the `Ok(_)` return path runs only after the guard has passed and only if the guard remains inactive throughout the submission. | **YES** (this is the live-action gate; the pre-impl plan must enumerate every `submit_bundle` caller introduced; Codex must verify the pre-check ordering matches the boundary doc from P6B-A; the live-network test policy is locked at batch-plan time with explicit user approval if any live-network test is included). | Actual relay submission, fully gated. The `SignerError::SignerDisabled` Display literal `"Phase 6b Production Gate"` continues to surface from any unreached code path (e.g., tests, shadow paths). |
| P6B-F | **Phase 6b DoD audit + `phase-6b-complete` annotated tag.** Audit P6B-A..E completion. Re-verify the Phase 6a HSIs that are NOT explicitly relaxed by the boundary doc from P6B-A still pass. Verify the new Phase 6b G-gates (G12 etc.) introduced in P6B-A pass. Verify the host-compromise control from P6B-B is actually wired, not merely documented. Tag at the post-plan-commit HEAD per the v0.2 P6-F precedent. | NO pre-impl review (audit/tag-only per P4-G / P5-E / P6-F precedent); however the audit log + tag message draft are reviewed by Codex BEFORE the tag is created (same pattern as P6-F). | No new gate; closes Phase 6b. |

Batch independence: P6B-A is the boundary + ADR amendment doc-only batch and MUST land first. P6B-B introduces the new file for the HSM/KMS-backed `Signer` impl and the host-compromise control. P6B-C operationally wires the key. P6B-D flips the config capability. P6B-E lands the live submission path. P6B-F audits and tags. Each batch is gated by the explicit user re-authorization for the specific non-goal that batch lifts.

## Per-batch detail (drafted at batch-plan time)

This overview does NOT enumerate per-batch deliverables D-B1..D-Bn / D-C1..D-Cn / etc. Each batch's deliverables, file-touch table, hard-forbids, open questions, and gate verifications land in the batch-specific pre-impl plan when that batch is authorized. Per-batch pre-impl plans MUST:

1. Cross-reference this overview by SHA at the time of plan drafting.
2. Cross-reference `docs/specs/production-signer.md` Section 2 contract for the relevant invariants the batch is unlocking or preserving.
3. Cross-reference `docs/specs/phase-6b-boundary.md` (once P6B-A lands) for the gate-relaxation context.
4. Honor the lean-batching policy (small, reviewable, single-purpose).
5. Honor the verification cadence policy: minimum checks during planning, targeted tests during impl, full gate set only at batch close.

## Gates that remain fail-closed (per-batch unlock table)

Each row is a gate that remains fail-closed at Phase 6b start. Unlock happens only in the named batch, under the named control. Re-locking is the implicit default at any point a downstream batch fails -- a failure in P6B-C does NOT unlock P6B-D, regardless of intermediate progress.

| Gate | Fail-closed posture at Phase 6b start | Unlocks in | Under what control |
|---|---|---|---|
| HSI-2 (G2a) -- forbidden signer-symbol set 0 hits in `crates/*.rs` | **Stays at 0 hits** | -- | Not unlocked. HSM/KMS-backed signer must not introduce any forbidden symbol; the HSM/KMS client library is reviewed against G2b separately. |
| HSI-3 (G2b) -- forbidden signer-dep set 0 hits in `crates/**/Cargo.toml` | Reviewed in P6B-B | P6B-B (only if explicitly approved at batch plan time) | The HSM/KMS client library is named at P6B-B plan time and Codex-reviewed. If the chosen library is not in the G2b banned set, G2b stays at 0; if it is, the batch plan MUST justify the inclusion and Codex MUST approve. v0.1 RECOMMENDS the chosen library NOT be in the G2b banned set. |
| HSI-4 (G2c/G2d) -- Signer-symbol allow-list | Extended in P6B-B | P6B-B | Allow-list grows by ONE file (the new production signer impl); G2d still returns 0 hits outside the extended allow-list. |
| HSI-5 (G2e) -- signer dep edge count = 2 | Unchanged | -- | New HSM/KMS dep edge is on the new production signer impl module, NOT on a new `signer = { path = "../signer" }` edge. Dep-edge count stays 2. |
| HSI-6 (G3) -- 0 `submit_bundle(` callers in `crates/app/src/` | **Stays at 0** until P6B-E | P6B-E | Each new caller is documented per-callsite in `phase-6b-boundary.md` from P6B-A; each caller is guarded by the new G12 (kill-switch + signer + sim-mismatch-comparator pre-checks). |
| HSI-7 (G4) -- 0 `dyn BundleRelay` / `Arc<dyn BundleRelay>` in `crates/app/src/` | **Stays at 0** until P6B-E | P6B-E | The relaxation is documented per-callsite in the Phase 6b boundary doc; the new `submit_bundle` callsite in P6B-E may hold a concrete adapter or an `Arc<dyn BundleRelay>` -- decision recorded at batch plan time. |
| HSI-8 (G5) -- `live_send = true` config-validation rejected | **Stays rejected** until P6B-D | P6B-D | Reject relaxed only for the operator-controlled production profile; dev/test/shadow continue to reject. |
| HSI-1 (G1) -- `eth_sendBundle` doc-comment only | **Stays doc-comment only** until P6B-E | P6B-E | New runtime call sites are documented per-callsite in the Phase 6b boundary doc; each callsite is guarded by the G12 pre-check chain. |
| HSI-9 (G6) -- `api_key` never in `tracing::*!` | Stays at 0 hits | -- | No relaxation. |
| HSI-9 (G7) -- `#[ignore]` count = 1 | Reviewed at any batch that adds a live-network test | Per-batch, with explicit user approval | Any live-network test added in Phase 6b MUST be `#[ignore]`-gated by default with env-overlay opt-in; the G7 count grows by exactly 1 per added live test, and the test name is recorded in the batch plan. |
| HSI-10 (G8/G9) -- workspace dep cycles + `KillSwitch` reach | G8 stays clean; G9 allow-list may grow if the boundary doc adds new sites | Per-batch as needed | All `KillSwitch` additions stay in the existing allow-list (`crates/bundle-relay/` + `crates/app/` + `crates/relay-clients/`) unless the boundary doc extends the allow-list explicitly. |
| HSI-11 (G10) -- per-adapter `submit_bundle` first-statement kill-switch guard | **REINFORCED**, not relaxed | -- | The `Ok(_)` return path in P6B-E runs only after the guard passes; the guard remains the FIRST non-trivia statement of every adapter `submit_bundle` body. |
| HSI-11 (G11) -- single `sign_tx` production call site | Grows by additional callsites as the production runtime invokes `sign_tx` | P6B-E (when the runtime first invokes `sign_tx`) | Each new callsite is documented per-callsite in the Phase 6b boundary doc; the routing through `&dyn Signer` is preserved. |

**No live submission until P6B-E.** The fail-closed posture for HSI-6, HSI-7, HSI-1, HSI-11(G11-runtime) is preserved through P6B-A + P6B-B + P6B-C + P6B-D. Only P6B-E unlocks live submission, and only after all prior batches are reviewed-closed.

## Cross-cutting items

### Production-signer.md Section 2 cross-reference (explicit per overview-mandated item 1+2)

- **Section 2.1 (HSM/KMS-only key custody)**: P6B-A boundary doc names the vendor-neutral contract; P6B-B impl implements it; P6B-C operationally wires the funded key. NO raw private-key bytes in repo at any point. NO Rust `Wallet` / `PrivateKey` / `secp256k1` / `k256` / `alloy-signer` / `ethers-signers` / `sign_transaction` / `funded` identifier introduced in `crates/*.rs` (G2a stays 0).
- **Section 2.2 (never-in-memory key material)**: The `Signer::sign_tx` trait shape `BundleTx -> Result<SignedTxBytes, SignerError>` already locks the API surface against in-memory key material (necessary condition). Sufficiency comes from (a) HSM/KMS custody per Section 2.1 (raw key bytes never traverse the workspace process) + (b) the Codex review at P6B-B against the Section 2 contract. NO code path may load private-key bytes into Rust process memory at any time.

### Host-compromise residual (overview-mandated item 3)

Per `docs/specs/production-signer.md` Section 2.5: HSM/KMS prevents raw key extraction but does NOT prevent a compromised host from requesting malicious signatures. **At least one non-trivial control MUST land in P6B-B before any production signer can replace `DisabledSigner`.** The four candidate controls are:

1. **Per-bundle pre-sign attestation.** Operator-side cryptographic approval of each bundle before the HSM/KMS will sign.
2. **Request-authorization rate limits at the HSM/KMS.** Caps on signatures per minute / per block / per operator session enforced by the HSM/KMS, not by the workspace.
3. **Pre-sign mismatch-comparator gating.** The signer impl signs only if the just-completed local-simulator + relay-simulator comparison passed AND the bundle matches the simulated artifact byte-for-byte.
4. **Operator-visible signing audit log.** Every signing attempt linked to the opportunity/bundle chain per Section 2.3, surfaced in an operator dashboard with a configurable alert threshold.

**v0.1 recommends choosing the specific control at P6B-B plan time** (not at overview time), so that the choice is informed by the HSM/KMS vendor selection. v0.1 does NOT pre-commit a choice. The P6B-B pre-impl plan MUST select at least one, justify the choice against the threat model, and Codex MUST verify the choice is non-trivial (i.e., not just a TODO comment claiming the control will land later). v0.1 RECOMMENDS layering at least TWO controls if cost permits -- but ONE non-trivial control is the hard minimum.

### ADR-001 amendment (overview-mandated item 4)

ADR-001 currently bans funded key + prod signer "until Phase 6b Production Gate is formally passed". Phase 6b requires that ban be scope-lifted to the specific Phase 6b context. The amendment will:

- Scope the lift to the specific HSM/KMS-backed `Signer` impl from P6B-B.
- Scope the lift to the specific host-compromise control mix from P6B-B.
- Scope the lift to the specific live-send config-flip mechanism from P6B-D.
- Preserve the ban for any other context (dev/test/shadow profiles continue to reject; `DisabledSigner` remains the only impl reachable outside the operator-controlled production profile).

**The ADR-001 amendment text is PROPOSED in the P6B-A pre-impl plan and REVIEWED by Codex as part of the P6B-A pre-impl review pack** (alongside the new `docs/specs/phase-6b-boundary.md`, as a single pre-impl review pack). The amendment text lands as a commit only AFTER (a) explicit user re-authorization for the ADR scope-lift with unambiguous wording, AND (b) Codex APPROVED on the P6B-A pre-impl pack. ADR text amendments require explicit user approval per CLAUDE.md "User explicit approval IS still required for ... ADR/scope/frozen-decision changes". This matches the prerequisite-table item #4 wording in Section "Phase 6b unlock prerequisites (non-sequential checklist)".

### Phase 6b boundary doc (overview-mandated item 5)

Authored in P6B-A as `docs/specs/phase-6b-boundary.md`. The doc captures:

- The per-batch unlock contract (mirrors the "Gates that remain fail-closed" table above with the relaxation details for each gate).
- The per-callsite documentation requirement for new `submit_bundle` callers (P6B-E), new `eth_sendBundle` runtime references (P6B-E), and new `sign_tx` production callers (P6B-E).
- The new G-gates (G12 etc.) that replace the relaxed Phase 6a gates -- e.g., G12 = "every `submit_bundle(...)` caller in `crates/app/src/` is guarded by kill-switch + signer + sim-mismatch-comparator pre-checks; verbatim ripgrep command for the audit".
- The reordering ban (no Phase 6b batch may run out of order).
- The fail-closed default (every gate relaxed by a batch is re-locked the moment a downstream batch fails; the boundary doc is the source of truth for what "re-locked" means operationally).

### Per-batch Codex pre-impl review (overview-mandated item 6)

Every Phase 6b batch (P6B-A through P6B-E) requires its own pre-impl Codex review. P6B-F is audit/tag-only per the Phase 4 / Phase 5 / Phase 6a precedent and uses the same audit-log-review-before-tag pattern as P6-F (Codex reviews the audit log + tag draft before the tag is created). The lean-batching policy (small, reviewable, single-purpose) applies throughout.

### No live submission until P6B-E (overview-mandated item 8)

Reiterated: NO live submission, NO `eth_sendBundle` runtime call, NO `submit_bundle` returning `Ok(_)` from any caller in `crates/app/src/` until P6B-E lands and is reviewed-closed. P6B-A + P6B-B + P6B-C + P6B-D each preserve the fail-closed posture in different dimensions (boundary doc, signer impl, key wiring, config flip) without unlocking the actual submission path.

## Hard forbids during all of Phase 6b (carried forward from Phase 6a + new)

- No private-key bytes in repo / tests / fixtures / configs / env-examples / build artifacts / runtime memory at any point.
- No vendor SDK code snippet outside its specific batch (P6B-B); even there, the vendor is named at plan time and Codex-reviewed.
- No `Wallet` / `PrivateKey` / `secp256k1` / `k256` / `alloy-signer` / `ethers-signers` / `sign_transaction` / `funded` identifier in `crates/*.rs` (G2a stays at 0; the HSM/KMS client library must NOT use these identifier patterns in the production-signer-impl module either).
- No live-network test enabled by default. `#[ignore]`-gated with env-overlay opt-in; explicit user approval per batch.
- No paid live API dependency enabled in CI.
- No `.claude/` / `AGENTS.md` / `fixture_output.txt` / `hook_toast.md` staging.
- No destructive git (force push, reset --hard, branch delete, tag overwrite).
- No asset / venue / V3-fee-tier widening.
- No reordering of P6B-A..F batches.
- No Phase 6b non-goal lifted without explicit user re-authorization for that specific non-goal.

## Codex Q-P6B open questions

- **Q-P6B-A -- First Phase 6b implementation batch.** v0.1 recommends **P6B-A (boundary doc + ADR-001 amendment)** as the first batch -- it locks the contract every subsequent batch must satisfy + carries the only user-authorization-required artifact in Phase 6b (ADR amendment). Alternatives (P6B-B signer impl first) would require landing the impl before its contract is reviewed; v0.1 explicitly rejects that order. Codex verdict?
- **Q-P6B-B -- Pre-impl Codex review on which batches.** Default: P6B-A, P6B-B, P6B-C, P6B-D, P6B-E ALL require pre-impl review. P6B-F is audit/tag-only (NO pre-impl review). Codex verdict?
- **Q-P6B-C -- Host-compromise control selection timing.** v0.1 recommends choosing at **P6B-B plan time** (not at overview time), informed by HSM/KMS vendor selection. v0.1 also recommends **layering at least TWO controls if cost permits** with ONE non-trivial control as the hard minimum. Codex verdict on timing + minimum count?
- **Q-P6B-D -- HSM/KMS vendor selection.** v0.1 does NOT lock a vendor at overview time. The P6B-B pre-impl plan names the vendor + the SDK + the G2b dep-set check. v0.1 RECOMMENDS the chosen library NOT be in the G2b banned set (`alloy-signer`, `ethers-signers`, `secp256k1`, `k256`) so that HSI-3 stays at 0 hits. Codex verdict?
- **Q-P6B-E -- Live-network test policy.** v0.1 recommends NO live-network test enabled by default in any batch; any live-network test added MUST be `#[ignore]`-gated + env-overlay opt-in + explicit user approval per test. Codex verdict?
- **Q-P6B-F -- Workspace baseline tracking for Phase 6b batches.** Phase 6a verification cadence (minimum checks during planning, targeted tests during impl, full gate set at batch close) applies to Phase 6b. v0.1 RECOMMENDS each Phase 6b batch close re-runs `cargo test --workspace` to update the durable baseline; doc-only batches (P6B-A) may inherit from the prior close per the P6-E precedent. Codex verdict?
- **Q-P6B-G -- Should P6B-E include a local-wiremock-only dress rehearsal step before the first live submission?** v0.2 recommends YES: P6B-E plan should split the impl into two sub-batches: **P6B-E1 local-wiremock-only end-to-end** (`wiremock::MockServer::start()` binding to 127.0.0.1 only; mirrors the P6-C v0.3 D-T-C1..D-T-C2 + P6-D D-T-D7/D-T-D8 precedent that uses local wiremock; **NO live test-net relay endpoint, NO Goerli/Sepolia/mainnet-test relay infrastructure, NO externally-reachable URL**), then **P6B-E2 live submission** with user-controlled go/no-go after P6B-E1 dress rehearsal closes. This is a structural recommendation for P6B-E itself, not for this overview. Codex verdict?

## Recommended first implementation batch after Codex review

**P6B-A (Phase 6b boundary doc + ADR-001 amendment).** Doc-only at the file-touch level (NEW `docs/specs/phase-6b-boundary.md` + amendment edit to `docs/adr/ADR-001.md`); NO code change; NO signer impl; NO key material. The ADR amendment requires explicit user re-authorization at batch plan time per CLAUDE.md "User explicit approval IS still required for ... ADR/scope/frozen-decision changes". P6B-A close is the prerequisite for P6B-B planning to begin.

## Process

1. Claude writes this overview to disk (UNCOMMITTED) + emits the Codex review pack to `.coordination/claude_outbox.md`.
2. Claude STOPS and reports "manual Codex review required for Phase 6b overview v0.1". **No `.rs` / `Cargo.toml` / ADR / `docs/specs/` edits in this turn. No batch planning. No commit. No push. No tag.**
3. User pastes pack to Codex; relays verdict.
4. Claude records verdict in `.coordination/codex_review.md`.
5. **APPROVED** -> commit + push this overview as a routine doc commit; THEN await user explicit authorization to begin P6B-A planning (P6B-A is itself a pre-impl-review-required batch; the overview APPROVED status does NOT authorize P6B-A planning to start without a separate user prompt).
6. **REVISION REQUIRED** -> revise overview in place + re-emit pack.
7. **Scope / ADR change required** -> HALT to user (this is the default expected response if anything in this overview implies an ADR amendment landing before P6B-A; v0.1 is designed to flag ADR amendment as a P6B-A-internal deliverable, not an overview deliverable).
