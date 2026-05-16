# Phase 6b Batch A -- Boundary doc + ADR-001 amendment proposal (planning only)

**Date:** 2026-05-16 KST
**Status:** Draft v0.3 (revised after Codex REVISION REQUIRED HIGH on v0.2, 2026-05-16 KST). Three v0.2 -> v0.3 stale-text fixes inside the Plan execution checklist Step 2 + Step 6 (v0.2 changes to deliverables D-BA3 removal were not fully propagated into the checklist text):
(0a) Step 2 wording "Steps 3..5 do NOT begin" -> **"Steps 3, 4, 6, and 7 do NOT begin"** (Step 5 was removed at v0.2; the dependent-step list must skip 5).
(0b) Step 6 self-check `awk` line "all three potentially-touched files ... optionally `docs/specs/execution-safety.md`" -> **"the exactly two touched files (`docs/specs/phase-6b-boundary.md` + `docs/adr/ADR-001-...md`)"** (D-BA3 removed; `execution-safety.md` is no longer a P6B-A target).
(0c) Step 6 `git diff --stat` wording "2-3 touched files" -> **"exactly the two touched files"** with explicit named pair + an explicit "no `execution-safety.md` change" negative claim.
This is **strictly a text consistency fix** to propagate the v0.2 D-BA3 removal into the checklist. No deliverable change. No new content. v0.2 substantive content (G12 INHERITS G13, ADR scope-lift contradiction resolution, path typo fix, D-BA3 removal) UNCHANGED.

**v0.1 -> v0.2 changelog retained verbatim below for traceability.** Four v0.1 -> v0.2 fixes:
(1) **ADR-001 scope-lift contradiction resolved.** v0.1 D-BA2 said the `execution-safety.md` ban is NOT lifted in P6B-A, but the proposed ADR paragraph text said the ban "is scope-lifted ONLY for ... P6B-B HSM/KMS-backed signer impl reviewed". The "is scope-lifted" wording implied P6B-A itself effectuated the lift, contradicting both the overview prerequisite ordering (lift happens operationally only when P6B-B + P6B-C + P6B-D all land) AND P6B-A's stated "unlocks NO live-action gate" invariant. v0.2 reworks the proposed ADR paragraph to **describe the Phase 6b unlock PATH without claiming the lift has occurred**: the ADR amendment names where the unlock contract is documented (`docs/specs/phase-6b-boundary.md`) and what conditions Phase 6b unlock requires, but it explicitly states that the funded-key + prod-signer ban from `execution-safety.md` REMAINS IN FORCE at the moment the ADR amendment lands. The operational scope-lift is the cumulative effect of P6B-B (signer impl) + P6B-C (key wiring) + P6B-D (config flip) + P6B-E (live submission) all landing in sequence with their respective reviews; no single batch in Phase 6b "lifts the ban" by itself.
(2) **G12 live_send / production-profile gating added.** v0.1 G12 enumerated a 6-step pre-check chain (kill-switch + signer + local-sim + relay-sim + comparator-match + bundle-byte equality) but did NOT name the `live_send = true` runtime capability check. Codex correctly flagged: without G13 holding at runtime, a `submit_bundle` caller could in principle execute even if `live_send = false` (the config-validation reject runs at startup but not necessarily at every call). v0.2 G12 adds **step 7: `config.relay.live_send == true && config.active_profile == Production && config.signer_kind == HsmKms` runtime check** as the LAST step before `submit_bundle` is invoked. Alternatively (and equivalently for the audit), v0.2 states explicitly that **G12 depends on G13 being satisfied**: the boundary-doc Section 4 G12 entry now opens with "G12 INHERITS G13: the live_send + production-profile + signer_kind check from G13 MUST hold at runtime before any of the six G12 pre-checks are evaluated". The verbatim ripgrep command grows to also locate the runtime live_send/profile check.
(3) **Path typo fixed.** v0.1 line 31 said `crates/specs/execution-safety.md`. The correct path is `docs/specs/execution-safety.md`. v0.2 corrects the typo.
(4) **D-BA3 removed from P6B-A scope.** v0.1 listed D-BA3 (optional single-line cross-reference back-fill in `execution-safety.md`) as a P6B-A deliverable pending Q-A2 verdict. Codex correctly flagged: the required P6B-A deliverables are the NEW `phase-6b-boundary.md` + the ADR-001 amendment; the optional third-file edit adds unnecessary scope to a batch whose entire point is to land the boundary contract on paper. v0.2 **removes D-BA3 from P6B-A entirely**. The back-fill (if it ever makes sense) becomes a separate future doc-cleanup batch that can be planned independently. v0.2 Q-A2 reframed accordingly.
Awaiting manual Codex re-review.

**v0.1 status retained for traceability:** Draft v0.1 (this plan file) was UNCOMMITTED on disk before v0.2 edits. ASCII-only. **This is a PRE-IMPL PLAN: it proposes the future content of `docs/specs/phase-6b-boundary.md` and the future ADR-001 amendment text. NEITHER lands in this turn.** Per the Phase 6b overview v0.2 prerequisite table item #4, the ADR-001 amendment lands as a commit only AFTER (a) explicit user re-authorization for the ADR scope-lift, AND (b) Codex APPROVED on this pre-impl pack.
**Predecessors:**

- `phase-6a-complete` annotated tag at `bd0a53c` (tag object `3c9faaf`).
- Phase 6b overview v0.2 APPROVED HIGH at `49123e9`.
- `master` HEAD `49123e9`.
- Workspace baseline (inherited from `phase-6a-complete`): **239 passed + 1 ignored** (not re-verified this turn per testing policy).

## Scope

P6B-A is a **doc-only** batch with TWO deliverables, both reviewed-before-landing:

1. **NEW spec file** `docs/specs/phase-6b-boundary.md` capturing the Phase 6b runtime contract (per-batch unlock contract, per-callsite documentation requirements for P6B-E new sites, new G-gates G12..G14 replacing relaxed Phase 6a gates, reordering ban, fail-closed default).
2. **ADR-001 amendment text** (proposed in this plan; landed as a commit only after explicit user re-authorization). The amendment scopes the Phase 6 Production Gate definition to acknowledge the Phase 6a-vs-6b split and cross-references the new boundary doc + the existing `docs/specs/production-signer.md`.

**P6B-A itself unlocks NO live-action gate.** This batch lands docs only. The fail-closed posture at `phase-6a-complete` is preserved bit-for-bit at P6B-A close; all live-action gates (HSI-1 G1 runtime / HSI-6 G3 / HSI-7 G4 / HSI-8 G5 `live_send=true` / HSI-11 G11 runtime signer-invocation) stay locked until their respective later batches (P6B-D for HSI-8; P6B-E for HSI-1/HSI-6/HSI-7/HSI-11-runtime).

**Phase 6b non-goals explicitly NOT touched in P6B-A:**

- NO production signer impl. The `Signer` trait surface stays unchanged; `DisabledSigner` remains the only impl in the workspace.
- NO key material / funded key wiring. No env-example, no config-example, no fixture, no runtime path that loads any key.
- NO `live_send = true` enablement. The Phase 4 P4-E DP-E9 config-validation reject stays in force; `live_send = true` continues to be rejected by `crates/config` for every profile.
- NO new `eth_sendBundle` runtime path. The existing 5 doc-comment-only `//!` hits in `crates/` stay as-is.
- NO actual relay submission. `submit_bundle` continues to return `Err(KillSwitchActive)` (when KS active per P6-D) or `Err(SubmitDisabled)` (when KS inactive) in every adapter.
- NO live-network tests. No `#[ignore]` test added. G7 count stays at 1 (P2-C carry-forward).
- NO `Cargo.toml` change. NO `.rs` change. NO new workspace crate.
- NO asset (WETH/USDC) / venue (UniV2 + UniV3 0.05% + Sushi V2) / V3-fee-tier widening.
- NO change to `docs/specs/execution-safety.md` (v0.1 path typo `crates/specs/` -> `docs/specs/` fixed at v0.2). v0.2 also DROPS the optional D-BA3 back-fill from P6B-A scope entirely; no edit to `execution-safety.md` in this batch (the existing P6-E D-E2 cross-reference to `production-signer.md` already provides forward navigation from `execution-safety.md` into the Phase 6b doc chain; the new `phase-6b-boundary.md` cross-references back to `execution-safety.md` one-way; that asymmetry is acceptable and adding a second back-fill is a separate doc-cleanup batch).
- NO change to `docs/specs/phase-6a-boundary.md`. The Phase 6a boundary doc stays the canonical Phase 6a contract; the new `phase-6b-boundary.md` is a SIBLING doc, not a successor.

## Why P6B-A exists

Two reasons:

1. **The Phase 6b runtime contract must be locked on paper BEFORE any code-changing Phase 6b batch begins.** P6B-B (signer impl), P6B-C (key wiring), P6B-D (`live_send=true` flip), and P6B-E (live submission) all need a fixed reference for: which Phase 6a HSIs they relax, which Phase 6a HSIs they preserve, which new G-gates they enforce, what the per-callsite documentation contract is for new `submit_bundle` callers / new `sign_tx` callers / new `eth_sendBundle` runtime sites.
2. **ADR-001 must be amended to acknowledge the Phase 6a-vs-6b split.** ADR-001 line 28 currently defines a single "Production Gate -- must pass at P6a exit". With `phase-6a-complete` shipped, that definition is operationally satisfied but conceptually incomplete: the ADR doesn't yet describe what Phase 6b looks like under the post-P6a structure. The amendment provides that description + the cross-references to `phase-6b-boundary.md` + `production-signer.md` so future Phase 6b readers have a single authoritative entry point at the ADR level.

## Deliverables

### D-BA1 -- NEW `docs/specs/phase-6b-boundary.md` (proposed structure; NOT authored this turn)

The actual file content is authored at impl time (after Codex APPROVED on this plan). The plan locks the SECTION STRUCTURE + INVARIANT WORDING + the new G-gate definitions. The impl-time author may not deviate from the structure without revising this plan.

**Section structure (8 sections; target <= 250 lines, ASCII-only):**

1. **Section 1 -- Status + Scope.** Header noting "Phase 6b runtime contract; sibling to `phase-6a-boundary.md`; lands as a P6B-A deliverable after Codex APPROVED + user re-authorization on the ADR-001 amendment text". Cross-reference to `docs/specs/phase-6a-boundary.md` (the Phase 6a contract this doc inherits from), `docs/specs/production-signer.md` (the unlock contract this doc references), `docs/specs/execution-safety.md` (the parent safety policy).

2. **Section 2 -- Phase 6a HSI inheritance + relaxation map.** Each Phase 6a HSI (HSI-1..HSI-11) gets a row: which Phase 6b batch (if any) relaxes it, under what control, with what new G-gate replacing the relaxed Phase 6a gate. The HSI rows that STAY fail-closed throughout Phase 6b are explicitly named. See "v0.1 HSI inheritance map" below for the full table.

3. **Section 3 -- Per-batch unlock contract (P6B-A..F).** One row per batch with: unlock-gate, prerequisite controls landed in earlier batches, fail-closed posture for the remaining batches. Reordering is forbidden.

4. **Section 4 -- New Phase 6b G-gates.** G12, G13, G14 defined with verbatim ripgrep commands + expected results. See "v0.1 new G-gate definitions" below.

5. **Section 5 -- Per-callsite documentation requirement.** Every new `submit_bundle(` caller in `crates/app/src/` (P6B-E) MUST be documented in this section by file:line + the guard chain it satisfies (G12). Every new `eth_sendBundle` runtime reference in `crates/` (P6B-E) MUST be documented by file:line + the guard chain. Every new production `sign_tx` call site beyond the existing `crates/execution/src/lib.rs:238` (P6B-E) MUST be documented by file:line + the routing through `&dyn Signer`.

6. **Section 6 -- Phase 6b hard forbids.** Carried forward from `phase-6a-boundary.md` Section 5 + new Phase 6b-specific forbids: no `live_send = true` outside the operator-controlled production profile; no `eth_sendBundle` runtime path outside the P6B-E-documented sites; no `submit_bundle` `Ok(_)` return outside the P6B-E-documented sites; no production signer impl outside the P6B-B-documented site; no key material in repo at any point.

7. **Section 7 -- Reordering ban + fail-closed default.** Explicit prose that:
   - P6B-A..F batches MUST land in order.
   - Skipping a batch is forbidden.
   - A failure in any batch RE-LOCKS all gates that had been relaxed by earlier batches.
   - The Phase 6a fail-closed baseline (`phase-6a-complete` at `bd0a53c`) is the rollback target if Phase 6b is abandoned at any point.

8. **Section 8 -- Cross-references.** Pointers to `docs/specs/phase-6a-boundary.md`, `docs/specs/execution-safety.md`, `docs/specs/production-signer.md`, `docs/adr/ADR-001-vertical-slice-replay-hooks-gate-policy.md`, `docs/superpowers/plans/2026-05-16-phase-6b-overview-execution.md`.

**Hard text invariants for `phase-6b-boundary.md`:**

- ASCII-only (P6-E v0.3 lesson).
- No Rust code-fence introducing forbidden symbols (`Wallet` / `PrivateKey` / `secp256k1` / `\bk256\b` / `alloy-signer` / `ethers-signers` / `sign_transaction` / `funded`).
- No vendor SDK code snippet (HSM/KMS vendor names allowed only in prose as illustrative examples, per `production-signer.md` Section 2.1 precedent).
- No example private key. No example signed-tx with a recognizable test-vector key.
- Target <= 250 lines including the new G12..G14 grep tables.

### D-BA2 -- ADR-001 amendment text (proposed; landed only after user re-authorization)

The amendment edits `docs/adr/ADR-001-vertical-slice-replay-hooks-gate-policy.md` in TWO locations:

**Location 1: Section "Gate Policy" (line 28 area; current text "Production Gate -- must pass at P6a exit").** Replaces the single "Production Gate" bullet with a TWO-stage description:

```text
4. **Production Gate** -- two-stage gate per the post-Phase-6a project structure:
   - **Phase 6a Pre-Production Gate** (CLOSED at `phase-6a-complete` annotated tag, commit `bd0a53c`, 2026-05-16). Latency, reliability, and fail-closed safety thresholds met in shadow + comparator runs; per-adapter kill-switch enforced (G10); signer routing fail-closed (G11). See `docs/specs/phase-6a-boundary.md`.
   - **Phase 6b Production Gate** (NOT STARTED). Live-action unlock sequence per `docs/specs/phase-6b-boundary.md` (P6B-A..F batches). Unlock requires explicit user re-authorization per non-goal + per-batch Codex pre-impl review + at least one non-trivial host-compromise control per `docs/specs/production-signer.md` Section 2.5.
```

**Location 2: Section "Decision" or "Consequences" (final section of the ADR).** Adds a new paragraph naming the cross-references:

```text
**Phase 6b scope context (added 2026-05-XX per user authorization on P6B-A pre-impl pack APPROVED HIGH at <SHA>):** The Phase 6b Production Gate is the only path to live action (funded key, production signer, `live_send=true`, `eth_sendBundle` runtime, actual relay submission). The Phase 6b unlock CONTRACT lives in `docs/specs/phase-6b-boundary.md`; the production-signer design contract lives in `docs/specs/production-signer.md`. **At the moment this amendment lands, the funded-key + prod-signer ban from `docs/specs/execution-safety.md` Section "Funded Key / Prod Signer Ban" REMAINS IN FORCE for all profiles, including the future operator-controlled production profile.** The eventual operational scope-lift is the cumulative effect of P6B-B (HSM/KMS-backed signer impl + host-compromise control) + P6B-C (key wiring) + P6B-D (config-validation flip restricted to the production profile + HSM/KMS signer) + P6B-E (live submission, fully gated) all landing IN SEQUENCE with their respective Codex pre-impl reviews and explicit user re-authorizations; no single batch in Phase 6b -- and specifically NOT P6B-A -- lifts the ban by itself. After P6B-D lands, `live_send = true` becomes permissible only for the operator-controlled production profile when paired with the HSM/KMS-backed signer from P6B-B; dev/test/shadow profiles continue to reject `live_send = true` unconditionally. After P6B-E lands, `submit_bundle` may return `Ok(SubmissionReceipt)` only through the G12 pre-check chain; outside that chain `submit_bundle` continues to return `Err(KillSwitchActive)` or `Err(SubmitDisabled)` per the Phase 6a PRECEDENCE. This amendment DESCRIBES the unlock PATH; it does NOT effectuate any unlock.
```

**Hard text invariants for the ADR-001 amendment:**

- ASCII-only.
- Does NOT lift the funded-key / prod-signer ban in execution-safety.md (that ban stays in force; the amendment merely cross-references the Phase 6b context).
- Does NOT introduce any new gate, decision, or scope expansion at ADR level beyond naming the Phase 6a-vs-6b split.
- The actual SHA filled into "P6B-A pre-impl pack APPROVED HIGH at <SHA>" is recorded at amendment-landing time (post-Codex-APPROVED-on-this-plan, post-user-re-authorization).
- No vendor name (HSM/KMS vendors are not named in the ADR; they live in `production-signer.md` Section 2.1 prose).

### D-BA3 -- REMOVED FROM P6B-A SCOPE (v0.2 per Codex item 4)

v0.1 proposed an OPTIONAL single-line cross-reference back-fill in `docs/specs/execution-safety.md` Section "Funded Key / Prod Signer Ban" pointing at the new `docs/specs/phase-6b-boundary.md`. Codex v0.1 verdict item 4: the required P6B-A deliverables are the NEW boundary doc + the ADR-001 amendment; the optional third-file edit adds unnecessary scope. v0.2 DROPS D-BA3 entirely from P6B-A. The forward-navigation chain at P6B-A close is:

- `execution-safety.md` Section "Funded Key / Prod Signer Ban" already points at `production-signer.md` (P6-E D-E2 cross-reference).
- `phase-6b-boundary.md` Section 8 points back at `execution-safety.md` + `production-signer.md` + `phase-6a-boundary.md` + ADR-001 + the Phase 6b overview (one-way reference).

If a future doc-cleanup batch wants to add a `execution-safety.md` -> `phase-6b-boundary.md` back-reference, it can be planned independently. **P6B-A touches exactly two files: NEW `docs/specs/phase-6b-boundary.md` + amendment-edit to `docs/adr/ADR-001-vertical-slice-replay-hooks-gate-policy.md`.** Nothing else.

### D-BA4 -- NO `.rs` / Cargo / runtime / config / fixture change

P6B-A touches zero `crates/`, zero `Cargo.toml`, zero `config/`, zero test fixtures.

## v0.1 HSI inheritance map (proposed Section 2 content of `phase-6b-boundary.md`)

| Phase 6a HSI | Gate | Status at `phase-6a-complete` | Relaxed in (Phase 6b batch) | Under what control | Status at P6B-A close |
|---|---|---|---|---|---|
| HSI-1 | G1 | `eth_sendBundle` doc-comment-only in `crates/` (5 `//!` hits) | P6B-E | New runtime call sites documented per file:line in Section 5; each guarded by G12 chain | **UNCHANGED** at P6B-A close (5 doc-comment hits; no new runtime path) |
| HSI-2 | G2a | 0 hits of forbidden signer-symbol set in `crates/*.rs` | NEVER | -- (HSM/KMS-backed signer impl must NOT introduce any forbidden symbol; key material lives in HSM/KMS not in source) | **UNCHANGED** at P6B-A close (0 hits) |
| HSI-3 | G2b | 0 hits of forbidden signer-dep set in `crates/**/Cargo.toml` | P6B-B (only if the HSM/KMS client library is in the banned set, which v0.1 RECOMMENDS against) | Codex review at P6B-B plan time | **UNCHANGED** at P6B-A close (0 hits) |
| HSI-4 | G2c / G2d | Signer-symbol allow-list = `crates/signer/` + 3 approved files | P6B-B | Allow-list grows by ONE file (the new production signer impl module) | **UNCHANGED** at P6B-A close (allow-list unchanged) |
| HSI-5 | G2e | 2 `signer = { path = "../signer" }` dep edges | P6B-B (new file lives inside `crates/signer/`; dep-edge count stays at 2 unless the impl moves to a new crate) | v0.1 RECOMMENDS keeping the dep-edge count at 2 by housing the new impl inside the existing `crates/signer/` crate | **UNCHANGED** at P6B-A close (2 edges) |
| HSI-6 | G3 | 0 `submit_bundle(` callers in `crates/app/src/` | P6B-E | Each new caller documented per file:line in Section 5; guarded by the **G12 7-step chain INHERITING G13** (kill-switch + signer Ok + local-sim Ok + relay-sim Ok + comparator Match + bundle-byte equality + runtime `live_send/profile/signer_kind` assertion) | **UNCHANGED** at P6B-A close (0 callers) |
| HSI-7 | G4 | 0 `dyn BundleRelay` / `Arc<dyn BundleRelay>` in `crates/app/src/` | P6B-E | Per-callsite decision recorded at P6B-E plan time | **UNCHANGED** at P6B-A close (0 hits) |
| HSI-8 | G5 | `live_send = true` config-validation rejected for ALL profiles | P6B-D | Reject relaxed ONLY for the operator-controlled production profile; dev/test/shadow continue to reject | **UNCHANGED** at P6B-A close (rejected for all profiles) |
| HSI-9 | G6 / G7 | `api_key` never in tracing; `#[ignore]` count = 1 | NEVER (G6); per-batch with explicit user approval (G7) | -- (G6); any live-network test added must be `#[ignore]`-gated + env-overlay opt-in (G7) | **UNCHANGED** at P6B-A close |
| HSI-10 | G8 / G9 | No workspace dep cycles; `KillSwitch` allow-list = `bundle-relay` + `app` + `relay-clients` | Per-batch as needed | All `KillSwitch` additions stay within the existing allow-list unless the boundary doc Section 5 extends it | **UNCHANGED** at P6B-A close |
| HSI-11 | G10 / G11 | G10: per-adapter `submit_bundle` first-statement kill-switch guard; G11: single `sign_tx` production call site at `crates/execution/src/lib.rs:238` | G10 REINFORCED (never relaxed) in P6B-E; G11 grows by additional callsites in P6B-E | G10: `Ok(_)` return path in P6B-E runs only after the guard passes + remains inactive throughout submission; G11: each new callsite documented per file:line in Section 5; routing through `&dyn Signer` preserved | **UNCHANGED** at P6B-A close |

**Summary: ALL ELEVEN HSI stay UNCHANGED at P6B-A close.** P6B-A unlocks NO live-action gate. The relaxations enumerated above happen ONLY in later batches.

## v0.1 new G-gate definitions (proposed Section 4 content of `phase-6b-boundary.md`)

### G12 -- submit_bundle caller pre-check chain (G12 INHERITS G13)

**G12 INHERITS G13.** The runtime `live_send + production-profile + signer_kind` check from G13 MUST hold at runtime before any of the G12 per-call pre-checks below are evaluated. The G13 inheritance is structural: a `submit_bundle` caller that fails G13 must never execute the G12 chain in the first place. The boundary-doc Section 4 G12 entry opens with this inheritance statement; the per-callsite documentation in `phase-6b-boundary.md` Section 5 names BOTH the G13-satisfaction path AND the G12 chain for every new caller.

**Verbatim ripgrep command:**

```text
rg -n --type rust -B 0 -A 30 'submit_bundle\(' crates/app/src/
```

(The window is widened from `-A 25` to `-A 30` to capture the additional step-7 runtime check below.)

**Expected result at P6B-A close: 0 hits** (HSI-6 unchanged).

**Expected result at P6B-E close:** every `submit_bundle(` caller in `crates/app/src/` MUST be preceded within the same function (within reasonable visual scope; manual inspection at audit) by ALL of the SEVEN steps below, in order. Step 7 is the G13-inheritance runtime check; steps 1..6 are the per-call pre-check chain.

1. **Kill-switch check.** `kill_switch.is_active()` returning `false`; short-circuit if active. (Reinforces P6-D G10.)
2. **Signer Ok.** A successful `signer.sign_tx(...)` returning `Ok(SignedTxBytes)` (NOT `Err(SignerError::SignerDisabled)`).
3. **Local-sim Ok.** A successful local-simulator `simulate(...)` returning `Ok(...)`.
4. **Relay-sim Ok.** A successful relay-simulator `simulate_bundle(...)` returning `Ok(RelaySimulationOutcome)` with non-error fields.
5. **Comparator Match.** A sim-mismatch-comparator equality check (P4-E `compare_result(...)`) returning `Match` (NOT `Mismatch(_)`).
6. **Bundle-byte equality.** A check confirming the signed bundle bytes match the simulated artifact byte-for-byte.
7. **G13 inheritance runtime check.** A runtime assertion that `config.relay.live_send == true && config.active_profile == Production && config.signer_kind == HsmKms` holds at the moment of submission. The static `crates/config` validation enforces this at startup; the runtime assertion is the defensive in-loop check that prevents any code path from bypassing the static guard (e.g., via test-only injection, dynamic config reload, profile mutation). Equivalent to "G12 INHERITS G13" stated declaratively in code.

The verbatim grep command + manual inspection at audit time enforces the chain. Each P6B-E callsite is documented per file:line in `phase-6b-boundary.md` Section 5 with both the G12 chain step locations AND the G13 inheritance assertion site.

### G13 -- live_send=true profile scope

**Verbatim ripgrep command:**

```text
rg -n --type rust 'live_send' crates/config/src/
```

**Expected result at P6B-A close: same as `phase-6a-complete` baseline** (line 282 field, line 295 default `false`, line 426 error variant, line 535 reject guard, plus surrounding doc comments).

**Expected result at P6B-D close:** the reject guard at `crates/config/src/lib.rs:535` (current `if self.relay.live_send { return Err(...) }`) becomes profile-aware:

```text
// PROPOSED P6B-D logic (NOT THIS BATCH; documented here for boundary contract only):
// if self.relay.live_send && self.active_profile != Profile::Production {
//     return Err(...);
// }
// if self.relay.live_send && self.active_profile == Profile::Production
//    && self.signer_kind != SignerKind::HsmKms {
//     return Err(...);  // production-profile live_send requires HSM/KMS signer
// }
```

The exact gating logic is finalized at P6B-D plan time; this G13 entry locks the contract that ANY relaxation of the live_send reject MUST also gate on (a) profile == Production AND (b) signer_kind == HsmKms (or equivalent enum naming). Dev / test / shadow profiles continue to reject `live_send = true` unconditionally.

### G14 -- eth_sendBundle runtime call documentation

**Verbatim ripgrep command:**

```text
rg -n --type rust 'eth_sendBundle' crates/
```

**Expected result at P6B-A close: 5 `//!` doc-comment hits** (HSI-1 baseline).

**Expected result at P6B-E close:** every `eth_sendBundle` runtime reference (non-doc-comment) in `crates/` MUST be documented per file:line in `phase-6b-boundary.md` Section 5 + guarded by the G12 pre-check chain. The 5 existing `//!` doc-comment hits stay (their text may be updated to reflect Phase 6b unlock).

## Tests

**N/A -- P6B-A is doc-only.** No new Rust test; no test-file edit; `cargo test --workspace` count unchanged. Workspace baseline at P6B-A close: **239 passed + 1 ignored** (inherited from `phase-6a-complete`; not re-verified at P6B-A close per the P6-E precedent for doc-only batches).

## Reused (no duplication)

- `docs/specs/phase-6a-boundary.md` -- the canonical Phase 6a contract. The new `phase-6b-boundary.md` is a SIBLING doc, not a successor; it references the Phase 6a contract for the inheritance baseline.
- `docs/specs/production-signer.md` -- the Phase 6b unlock contract authored in P6-E. The new boundary doc references it for Section 2 (HSM/KMS-only custody, never-in-memory key material, etc.) and Section 2.5 (host-compromise residual + Phase 6b control point).
- `docs/specs/execution-safety.md` -- the parent safety policy. The new boundary doc references its Section "Funded Key / Prod Signer Ban" for the scope-lift context.
- ADR-001 line 28 "Production Gate" bullet -- the amendment text in D-BA2 amends this exact bullet.
- `docs/superpowers/plans/2026-05-16-phase-6b-overview-execution.md` v0.2 -- the overview that locks the P6B-A..F batch sequence + the six unlock prerequisites; this plan inherits the overview's structure and adds the per-batch boundary contract.
- The P6-A `phase-6a-boundary.md` G1..G11 ripgrep gates are inherited verbatim; G12..G14 are NEW.

## Gates at P6B-A close (deltas vs `phase-6a-complete` baseline)

All gate results UNCHANGED. P6B-A is doc-only; no `crates/` change; no Cargo change; no test change. The new G12..G14 grep commands are DEFINED at P6B-A close (so they exist as audit targets for later batches) but their expected results at P6B-A close are the inherited Phase 6a baseline (0 hits for G12; baseline for G13; 5 doc-comment hits for G14).

Workspace tests at P6B-A close: **239 passed + 1 ignored** (inherited, not re-verified).

## Hard-forbids in P6B-A

- No production signer impl, design sketch, or stub.
- No private key material referenced by name, fingerprint, or fixture.
- No funded key wiring code, config example, or env-example.
- No CODE / CONFIG / RUNTIME enablement of `live_send = true`. Prose discussion in the new boundary doc is allowed (mirrors `execution-safety.md` precedent).
- No new `eth_sendBundle` runtime path or executable code in `crates/`. Prose mentions in the new boundary doc allowed (mirrors existing `docs/specs/*.md` precedent).
- No actual relay submission code, mock, or test.
- No live-network test.
- No paid live API.
- No `Cargo.toml` change. No `.rs` change. No `config/` change. No fixture change.
- No edit to `docs/specs/phase-6a-boundary.md`.
- No edit to `docs/specs/production-signer.md`.
- No edit to `docs/specs/execution-safety.md` at all (v0.2 dropped the v0.1 optional D-BA3 back-fill from P6B-A scope per Codex item 4).
- No new ADR file. No edit to ADR-002..ADR-008.
- The ADR-001 amendment text (D-BA2) is PROPOSED in this plan and REVIEWED by Codex as part of this pre-impl pack. The amendment text lands as a commit only AFTER (a) explicit user re-authorization for the ADR scope-lift, AND (b) Codex APPROVED on this plan. NO ADR text amendment in this plan-drafting turn.
- No new workspace crate.
- No widening of asset / venue / V3-fee-tier scope.
- No `.claude/` / `AGENTS.md` / `fixture_output.txt` / `hook_toast.md` staging.
- No destructive git.

## P6B-A unlocks NO live-action gate (overview-mandated item 13)

Explicit confirmation: P6B-A close advances the project state by adding (a) one new spec file `docs/specs/phase-6b-boundary.md` and (b) an ADR-001 amendment (after user re-authorization). NEITHER unlocks any live-action capability. The fail-closed posture at `phase-6a-complete` is preserved bit-for-bit through P6B-A close. The next batch with any unlock potential is P6B-B (HSM/KMS-backed `Signer` impl + host-compromise control), and even P6B-B does NOT unlock live submission -- the unlock chain is: P6B-B (signer impl) -> P6B-C (key wiring, operationally only) -> P6B-D (config flip for production profile) -> P6B-E (live submission, fully gated). P6B-A is purely the contract-on-paper batch.

## Plan execution checklist (doc-only, lightweight gate set)

- [ ] **Step 1: Confirm predecessor state.** `git log --oneline -3` shows HEAD `49123e9` (Phase 6b overview v0.2); `git status --short` shows only persistent scratch.
- [ ] **Step 2: After this plan is Codex-APPROVED + this plan commit lands**, await explicit user re-authorization for the ADR-001 amendment scope-lift. Without that re-authorization, **Steps 3, 4, 6, and 7 do NOT begin** (Step 5 was removed at v0.2 per Codex item 4).
- [ ] **Step 3: Author `docs/specs/phase-6b-boundary.md`** per the 8-section structure in D-BA1. ASCII-only. <= 250 lines. Include G12..G14 definitions with verbatim ripgrep commands per the v0.1 templates above.
- [ ] **Step 4: Edit `docs/adr/ADR-001-vertical-slice-replay-hooks-gate-policy.md`** per the D-BA2 two-location amendment text. ASCII-only. Fill in the actual P6B-A pre-impl pack APPROVED HIGH SHA (this plan's commit SHA after Step 2 completes) into the "added 2026-05-XX per user authorization on ... at <SHA>" placeholder.
- [ ] **Step 5 (v0.2): REMOVED.** v0.1 had an optional D-BA3 back-fill in `execution-safety.md`; v0.2 dropped per Codex item 4. Skip this step entirely.
- [ ] **Step 6: Self-check (minimum-checks-only per testing policy).**
  - `awk '/[\x80-\xFF]/'` on the **exactly two touched files** (`docs/specs/phase-6b-boundary.md` + `docs/adr/ADR-001-vertical-slice-replay-hooks-gate-policy.md`) returns 0 hits.
  - `wc -l docs/specs/phase-6b-boundary.md` confirms <= 250.
  - Targeted `rg -n --type rust 'submit_bundle\(' crates/app/src/` returns 0 (G12 baseline at P6B-A close).
  - Targeted `rg -n --type rust 'eth_sendBundle' crates/` returns 5 doc-comment-only hits (G14 baseline at P6B-A close).
  - `git diff --stat` confirms **exactly the two touched files** (`docs/specs/phase-6b-boundary.md` NEW + `docs/adr/ADR-001-vertical-slice-replay-hooks-gate-policy.md` modified); no `.rs` / no `Cargo.toml` / no other `docs/specs/` / no other ADR edits; no `docs/specs/execution-safety.md` change.
- [ ] **Step 7: Commit + push** as a single routine `docs(p6b-a)` commit. Suggested message: `docs(p6b-a): Phase 6b boundary doc + ADR-001 amendment (Phase 6a/6b split)`.
- [ ] **Step 8: Emit P6B-A closeout report** to `.coordination/claude_outbox.md` with full self-check results + tag of "P6B-A closed; P6B-B planning may now begin pending explicit user re-authorization for the P6B-B non-goal (HSM/KMS-backed signer impl)".

## Risks + open questions

- **Q-A1 -- ADR-001 amendment scope: minimal (D-BA2 two-location edit) vs broader?** v0.1 RECOMMENDS minimal: amend ONLY the "Gate Policy" Section 4 bullet + add one paragraph at the Decision/Consequences section. Broader amendment (e.g., rewriting the Phase 6 narrative entirely) would risk drifting from the original ADR-001 voice + requires more user authorization surface. Codex verdict?
- **Q-A2 -- v0.2 LOCKED: D-BA3 REMOVED from P6B-A scope per Codex v0.1 item 4.** The back-fill (if it ever makes sense) is a separate future doc-cleanup batch, NOT a P6B-A deliverable. P6B-A touches exactly two files: NEW `phase-6b-boundary.md` + ADR-001 amendment. Codex verdict ratifying the removal?
- **Q-A3 -- G13 logic preview in the boundary doc.** v0.1 includes a "PROPOSED P6B-D logic" code-style block in the G13 definition for clarity. Codex verdict: keep the illustrative block, drop it (leave G13 wording at the prose-only level), or move it to a P6B-D-specific section that lands when P6B-D itself lands?
- **Q-A4 -- Boundary-doc line cap.** v0.1 RECOMMENDS <= 250 lines (versus `phase-6a-boundary.md`'s 131-line baseline + `production-signer.md`'s 119-line baseline). The Phase 6b boundary doc is structurally larger (HSI inheritance table + 3 new G-gates + per-batch unlock contract + per-callsite documentation requirement). Codex verdict on the cap?
- **Q-A5 -- Per-callsite documentation requirement: in `phase-6b-boundary.md` Section 5 vs in each batch's pre-impl plan?** v0.1 RECOMMENDS centralizing in `phase-6b-boundary.md` Section 5 (so a single audit grep covers it) + cross-referenced from each batch's pre-impl plan. Alternative: distribute across per-batch plans. Codex verdict?
- **Q-A6 -- ADR amendment SHA placeholder.** The amendment text in D-BA2 says "P6B-A pre-impl pack APPROVED HIGH at <SHA>". v0.1 RECOMMENDS filling this with THIS plan's commit SHA (post-Step-2 + user-re-authorization) so the ADR record names the specific pre-impl pack that scoped the lift. Alternative: use `phase-6b-complete` tag SHA, which doesn't exist yet at P6B-A close. Codex verdict?
- **Q-A7 -- Workspace re-verification at P6B-A close.** v0.1 says NO (doc-only batch; inherit baseline per P6-E precedent). The user testing policy for this turn explicitly says "Future P6B-A execution should remain doc-only; full cargo verification can be deferred unless the plan explicitly justifies it." v0.1 RECOMMENDS no justification needed; defer full cargo to P6B-B (first code-changing batch). Codex verdict ratifying?

## Process

Per the 2026-05-04 routine-closeout policy + the overview Section "Process":

1. Claude writes this pre-impl plan to disk (UNCOMMITTED) + emits the review pack to `.coordination/claude_outbox.md`.
2. Claude STOPS and reports "manual Codex review required for P6B-A pre-impl plan". **No `.rs` / `Cargo.toml` / ADR / `docs/specs/` edits in this turn. No boundary doc authored. No ADR amendment landed. No commit. No push. No tag.**
3. User pastes pack to Codex; relays verdict.
4. Claude records verdict in `.coordination/codex_review.md`.
5. **APPROVED + explicit user re-authorization for ADR scope-lift** -> commit + push this plan as a routine doc commit; THEN execute per Section "Plan execution checklist" Step 3 + Step 4 + Step 6 + Step 7 (author boundary doc; edit ADR-001 per D-BA2; **skip Step 5 -- removed at v0.2**; self-check; commit + push impl); THEN emit closeout outbox.
6. **APPROVED without user re-authorization for ADR scope-lift** -> commit + push this plan as a routine doc commit; AWAIT explicit user re-authorization before Step 3..7 begin.
7. **REVISION REQUIRED** -> revise plan in place + re-emit pack.
8. **Scope / ADR change required beyond what this plan proposes** -> HALT to user.
