# Phase 6 Batch F -- Phase 6a DoD audit + `phase-6a-complete` annotated tag

**Date:** 2026-05-16 KST
**Status:** Draft v0.2 (revised after Codex REVISION REQUIRED HIGH on v0.1, 2026-05-16 KST). Five v0.1 -> v0.2 fixes:
(1) **Tag-target model clarified.** v0.1 hard-coded the tag target as `e0ec00f` while also prescribing that the P6-F plan be committed BEFORE the audit runs. That is internally inconsistent: the plan commit advances `master`, so a tag at `e0ec00f` would sit one commit behind `master` and omit the P6-F plan from the tag's history. v0.2 fixes this with a single consistent model: `e0ec00f` is the **pre-P6-F baseline** (Phase 6a's pre-audit state); the **`phase-6a-complete` annotated tag is created at the post-plan-commit HEAD** (i.e., at the SHA produced when this plan commits cleanly to `master`). The plan commit is the LAST file change of Phase 6a; the tag immediately follows.
(2) **D-F1 / D-F3 / checklist / outbox wording normalized** to "pre-P6-F baseline `e0ec00f`; audit/tag target is the approved P6-F plan commit HEAD". No more hard-coded `e0ec00f` as the tag target anywhere.
(3) **D-F5 wording fixed** to account for the allowed P6-F plan commit. The audit/tag *execution* touches zero code/spec/Cargo/ADR files, but the *repository-state changes* include (a) the P6-F plan commit (NEW file `docs/superpowers/plans/2026-05-16-phase-6-batch-f-...`), and (b) the `phase-6a-complete` annotated tag object. v0.2 D-F5 reframes from "ZERO file changes" to "ZERO `crates/` / `Cargo.toml` / `docs/specs/` / `docs/adr/` change; ONE new file under `docs/superpowers/plans/` (this plan); ONE new annotated tag object". Mirror fix in outbox `file-touch summary`.
(4) **Tag-message contradiction resolved.** v0.1 Q-F3 said summary in tag + verbatim in outbox, while v0.1 checklist Step 5 said "actual HSI verbatim results inlined". v0.2 locks the policy: **summary HSI results in the tag message body; full verbatim HSI command output in the closeout outbox**. Checklist Step 5 reworded accordingly. The tag message stays concise + human-readable (Phase 5 P5-E precedent).
(5) **`git tag -v` replaced with annotated-tag content verification.** v0.1 checklist Step 6 used `git tag -v phase-6a-complete` which fails on unsigned annotated tags (the repo does not configure tag signing per the Phase 1..5 precedent). v0.2 replaces with `git show phase-6a-complete` (verifies the annotated-tag object's message + target) and `git for-each-ref refs/tags/phase-6a-complete --format='%(objecttype) %(objectname) %(*objectname)'` (confirms `tag` object pointing at a commit). A conditional `git tag -v phase-6a-complete` MAY be added later if the project enables tag signing.
Awaiting manual Codex re-review.
**Predecessors:**

- Phase 6 overview v0.3 at `c08db38` (pushed). P6-F batch row: "Phase 6a DoD audit + `phase-6a-complete` annotated tag. Audit P6-A..E completion. Run all Phase 5 carry-forward safety grep gates G1..G9 + new Phase 6a invariants (G10 kill-switch-first; G11 signer-routing-fail-closed). Tag draft. **NO** pre-impl review (audit/tag-only per P4-G / P5-E precedent)."
- P6-A: pre-impl plan `4c4c0dd`; boundary spec `64ffaee` / `19e263a` / `a7367b7` -- FULLY CLOSED.
- P6-B: pre-impl plan `9a6ebd2`; impl `b27d01a`; closeout v3 `a7367b7` -- FULLY CLOSED.
- P6-C: pre-impl plan v0.3 `07a0256`; impl `93803b2` -- FULLY CLOSED.
- P6-D: plan v0.2 `98ab10c`; plan v0.5 `e8ca1c5`; impl `d88693b` -- FULLY CLOSED.
- P6-E: plan v0.3 `8511b02`; impl `e0ec00f` -- FULLY CLOSED.
- **Pre-P6-F baseline:** HEAD `e0ec00f` (P6-E impl close). Workspace baseline (P6-D verified): **239 passed + 1 ignored**. The `phase-6a-complete` tag target is the **post-plan-commit HEAD** that results when this P6-F plan commits cleanly to `master`; the tag SHA is determined at audit time and recorded in the closeout outbox.

## Scope

P6-F is **audit + tag only**. NO feature work. NO spec change. NO ADR amendment. The batch runs a fixed verification set, verifies the Phase 6a hard safety invariants HSI-1..N are intact, drafts the `phase-6a-complete` annotated tag message, and pushes the tag. Per the overview, P6-F is overview-flagged "NO pre-impl review" -- but the user has explicitly requested Codex pre-impl review of this plan before any audit step runs. The plan is therefore drafted to disk uncommitted; **stop and await Codex review**; no audit run, no commit, no push, no tag in this turn.

Per-Phase precedent: Phase 1 / Phase 2 / Phase 3 / Phase 4 / Phase 5 each closed with an audit + annotated tag in a single final batch (Task 19 / P2-D / P3-F / P4-G / P5-E). P6-F mirrors that shape.

**Phase 6a invariants explicitly preserved through audit:**

- NO production signer impl (G2a / G2b / G2d zero-hit; G2c 3-file allow-list; G2e dep-edge count = 2).
- NO funded key / NO private key material / NO key derivation / NO `secp256k1` / `k256` / `alloy-signer` / `ethers-signers` / `Wallet` / `PrivateKey` / `sign_transaction` / `funded` symbol anywhere in `crates/`.
- NO `eth_sendBundle` runtime path; G1 doc-comment-only `//!` hits.
- NO actual relay submission. G3 + G4 zero hits in `crates/app/src/`.
- NO `live_send = true` capability.
- NO live relay tests; NO env-gated test paths beyond the P2-C carry-forward.
- NO new `#[ignore]` test additions (G7 stays at P2-C baseline = 1).
- NO new Cargo dep / dev-dep / feature flag.
- NO `.rs` change in P6-F itself (audit only).
- NO `docs/adr/` text amendment in P6-F.
- NO `docs/specs/` text change in P6-F.

## What P6-F does NOT do

- Does NOT add any new feature, refactor, or test.
- Does NOT amend any ADR.
- Does NOT amend `docs/specs/execution-safety.md`, `docs/specs/phase-6a-boundary.md`, `docs/specs/production-signer.md`, or any other spec file.
- Does NOT amend any `Cargo.toml`.
- Does NOT touch any `.rs` file in `crates/`.
- Does NOT begin any Phase 6b planning.
- Does NOT relax any HSI; if the audit discovers a violation, see Section "Blocker handling" below.

## Why P6-F exists

Phase tag creation is the single point in the workflow where the full safety contract is re-verified end-to-end. Per the Phase 1..5 precedent, the tag is the durable artifact that future Phase 6b / Phase 7 work can reference as "everything before this tag was Phase 6a-compliant". The audit step also catches drift introduced by accidental in-batch changes that earlier per-batch gate runs missed.

## Deliverables

### D-F1 -- Full verification set

Run, in this order, with all results recorded in the closeout outbox:

1. **`git log --oneline -10`** -- confirm HEAD is the **post-plan-commit P6-F SHA** (advanced one commit from pre-P6-F baseline `e0ec00f`); confirm linear master since `phase-5-complete` (`55679a4`). Record the exact post-plan-commit SHA in the closeout outbox.
2. **`git status --short`** -- expect only persistent scratch (`?? AGENTS.md`, `?? fixture_output.txt`, `?? hook_toast.md`). Anything else is a blocker.
3. **`cargo fmt --check`** -- expect zero diff.
4. **`cargo clippy --workspace --all-targets -- -D warnings`** -- expect zero warnings (any warning is a blocker).
5. **`cargo test --workspace`** -- expect **239 passed + 1 ignored** (inherited from P6-D; first re-verification since P6-D close). Any divergence is a blocker.
6. **`cargo deny check`** -- expect clean against the v2 schema rules from Phase 1; the carry-forward `RUSTSEC-2025-0141` (bincode 1.3 unmaintained) ignore per ADR-004 must still be the only ignored advisory.
7. **`cargo tree -d`** -- expect no duplicate dependency edges flagged. (Acceptable carry-forwards from Phase 5 if any; record verbatim.)
8. **G1..G11 ripgrep gates** per `docs/specs/phase-6a-boundary.md` Section 4 (verbatim copy-paste commands). Each gate's expected result is enumerated in HSI-1..HSI-11 below.

### D-F2 -- Phase 6a Hard Safety Invariants (HSI-1..HSI-11) verification

Each HSI must hold at the post-plan-commit P6-F HEAD (i.e., one commit forward of pre-P6-F baseline `e0ec00f`; the audit runs after the P6-F plan commit lands cleanly on `master`). The plan commit only adds a `docs/superpowers/plans/` file and cannot break any HSI by construction; HSI re-verification at the post-plan-commit HEAD is identical in expected results to the pre-P6-F baseline. Any failure is a blocker.

| HSI | Gate(s) | Expected |
|---|---|---|
| HSI-1 | G1 | `rg -n --type rust 'eth_sendBundle' crates/` returns only doc-comment / `//!` "NO" assertions (5 hits at P6-D close baseline). No runtime call site. |
| HSI-2 | G2a | `rg -n --type rust -e 'Wallet' -e 'PrivateKey' -e 'secp256k1' -e '\bk256\b' -e 'sign_transaction' -e 'funded' crates/` returns 0 hits. (`\bk256\b` word boundary excludes the `keccak256` substring per P6-B D-B0.) |
| HSI-3 | G2b | `rg -n --glob 'crates/**/Cargo.toml' -e 'alloy-signer' -e 'ethers-signers' -e 'secp256k1' -e 'k256'` returns 0 hits. |
| HSI-4 | G2c / G2d | `rg -n --type rust -e 'Signer' -e 'DisabledSigner' -e 'SignerError' -e 'SignerDisabled' crates/` hits live only under `crates/signer/` + 3 approved file entries (`crates/execution/src/lib.rs`, `crates/app/src/lib.rs`, `crates/app/tests/wire_phase4.rs`). G2d returns 0 hits outside the allow-list. |
| HSI-5 | G2e | `rg -n --glob 'crates/**/Cargo.toml' 'signer = \{ path = "../signer" \}'` returns exactly 2 hits (`crates/execution/Cargo.toml` + `crates/app/Cargo.toml`). |
| HSI-6 | G3 | `rg -n --type rust 'submit_bundle\(' crates/app/src/` returns 0 hits. |
| HSI-7 | G4 | `rg -n --type rust -e 'dyn BundleRelay' -e 'Arc<dyn BundleRelay>' crates/app/src/` returns 0 hits. |
| HSI-8 | G5 | Config-validation reject of `live_send = true` preserved (no `live_send = true` capability anywhere). Verified by manual inspection of `crates/config/src/` + a `rg -n 'live_send' crates/` walk-through. |
| HSI-9 | G6 / G7 | G6: `api_key` field-access only, never inside `tracing::*!`. G7: `#[ignore]` count = 1 in source (the P2-C `g_state_live` carry-forward); `cargo test --workspace` summary reports `1 ignored`. |
| HSI-10 | G8 / G9 | G8 workspace has no cycles. G9 `rg -n --type rust 'KillSwitch' crates/` hits live only under `crates/bundle-relay/`, `crates/app/`, and `crates/relay-clients/` (post-P6-D extended allow-list per boundary-spec Section G9). Zero hits outside. |
| HSI-11 | G10 / G11 | G10: each `impl BundleRelay for ... { fn submit_bundle }` body's FIRST non-trivia statement is `if self.kill_switch.is_active() { return Err(BundleRelayError::KillSwitchActive); }` (P6-D enforcement). G11: single `sign_tx` production call site at `crates/execution/src/lib.rs:238` routed through `&dyn Signer` inside the `BundleConstructor`-private signing-request hook (P6-B enforcement). |

If any HSI fails, see "Blocker handling" below.

### D-F3 -- `phase-6a-complete` annotated tag

After D-F1 + D-F2 pass cleanly, create an **annotated** tag named `phase-6a-complete` **at the post-plan-commit HEAD** (the SHA produced when this plan commits cleanly to `master`; recorded by D-F1 step 1). The tag message structure (drafted at audit time; this plan locks the structure, not the prose). **Summary HSI results in the tag message; full verbatim HSI command output in the closeout outbox** (v0.2 lock per Q-F3):

```text
phase-6a-complete

Phase 6a Pre-Production Gate closed. Six batches CLOSED:
  - P6-A overview + boundary spec
  - P6-B signing-request pipeline (fail-closed)
  - P6-C relay eth_callBundle wiremock body-shape tests
  - P6-D per-adapter kill-switch wiring (G10 enforcement)
  - P6-E production-signer design doc (Phase 6b unlock contract)
  - P6-F DoD audit + this tag

Workspace tests: 239 passed + 1 ignored.

Hard Safety Invariants HSI-1..HSI-11 verified at this tag
(verbatim listing per the P6-F audit report on .coordination/
claude_outbox.md, P6-F closeout pack).

Phase 6b Production Gate is NOT touched here. Phase 6b
requires fresh explicit user authorization + a separate Phase 6b
overview document + a separate Codex review. The
SignerError::SignerDisabled Display literal "Phase 6b Production
Gate" is the canonical forward-link from runtime code to the
gate that would unlock real signing.

Predecessor tag: phase-5-complete at 55679a4.
```

Tag SHA recorded in the closeout outbox.

### D-F4 -- Push tag + master

`git push origin master` (already in sync at impl time; defensive); `git push origin phase-6a-complete` (annotated tag push).

### D-F5 -- NO `.rs` / Cargo / spec / ADR change in P6-F audit/tag execution

The P6-F **audit + tag execution** touches **zero** files in `crates/`, **zero** `Cargo.toml`, **zero** `docs/specs/`, **zero** `docs/adr/`. The repository-state changes from P6-F are:

1. **ONE new file** under `docs/superpowers/plans/`: this plan itself (`2026-05-16-phase-6-batch-f-phase-6a-dod-audit-tag-execution.md`), committed as a routine doc commit AFTER Codex APPROVED and BEFORE the audit runs.
2. **ONE new annotated tag object** `phase-6a-complete` at the post-plan-commit HEAD, created AFTER the audit passes.
3. (Optional, user-gated) A `CLAUDE.md` Phase 6a wrap-up commit if the user explicitly requests it AFTER the tag lands. Not auto-landed at P6-F close.

The audit step itself is read-only at the file level; the only writes in P6-F are the plan commit (item 1, before audit) and the tag object (item 2, after audit).

## Tests

**N/A -- P6-F is audit + tag only.** No new Rust test; no test-file edit; `cargo test --workspace` is part of the audit (D-F1 step 5) and expected to report **239 passed + 1 ignored** unchanged.

## Reused

- The verbatim G1..G11 grep commands from `docs/specs/phase-6a-boundary.md` Section 4 (and Section 5 hard-forbids list).
- The Phase 5 P5-E tag-creation precedent (`phase-5-complete` at `55679a4` / tag object `be98681`).
- The Phase 4 P4-G tag-creation precedent (`phase-4-complete`).
- `cargo deny check` rules from Phase 1 Task 18 (`deny.toml` v2 schema, `RUSTSEC-2025-0141` ignore).

## Gates at P6-F close (deltas vs P6-E close / pre-P6-F baseline `e0ec00f`)

All gate results UNCHANGED (no `crates/` change; no Cargo change; no spec change). P6-F is the verification batch -- gates are RUN, not changed.

| Gate | Result | Notes |
|---|---|---|
| G1..G11 | unchanged from P6-D enforcement state; re-verified at P6-F audit. | The first end-to-end re-verification since P6-D close. |

Workspace tests at P6-F close: **239 passed + 1 ignored** (re-verified, not inherited).

## Blocker handling

If any D-F1 / D-F2 step fails, P6-F **HALTS** and does NOT create the tag. The failure is recorded in `.coordination/claude_outbox.md` with:

1. The failing gate / HSI ID + exact command output.
2. The minimal remediation candidate (e.g., "P6-B D-T1 introduced an extra `Signer` reference at `crates/execution/src/lib.rs:N` that the G2d allow-list doesn't cover; needs either allow-list extension OR symbol removal").
3. A flag that the remediation requires fresh Codex pre-impl review under a new "P6-F-remediation" batch ID (NOT a relaxation of HSI; the remediation must restore the invariant or change the boundary spec under separate explicit user authorization).

Tag creation is **strictly blocked** until every HSI passes.

## Forbidden in P6-F

- Any `.rs` change anywhere in `crates/`.
- Any `Cargo.toml` change anywhere.
- Any ADR text amendment.
- Any `docs/specs/` text change (including `phase-6a-boundary.md`, `execution-safety.md`, `production-signer.md`).
- Any `docs/superpowers/plans/` change beyond this plan file itself.
- Any new test; any test removal; any `#[ignore]` toggle.
- Any silent HSI relaxation; any "we'll fix that in Phase 6b" carve-out at audit time.
- Any destructive git operation (no `git reset --hard`, no `git push --force`, no `git rebase` of pushed commits, no `git tag -f` overwriting an existing tag).
- Any tag creation that is NOT annotated.
- Any tag push without an immediately preceding successful audit log.
- Any `.claude/` / `AGENTS.md` / `fixture_output.txt` / `hook_toast.md` staging.
- Any asset / V3-fee-tier / venue widening.

## Plan execution checklist (audit + tag only)

- [ ] **Step 1: Confirm post-plan-commit state.** After Codex APPROVED and the P6-F plan commit lands on `master`: `git log --oneline -10` shows the new post-plan-commit SHA as HEAD (one commit forward of pre-P6-F baseline `e0ec00f`); `git status --short` shows only persistent scratch. Record the post-plan-commit SHA -- this IS the `phase-6a-complete` tag target.
- [ ] **Step 2: Run D-F1 verification set in order.** Record exact output for each step in the closeout outbox.
- [ ] **Step 3: Run D-F2 HSI-1..HSI-11 verification.** Record exact ripgrep output and pass/fail for each HSI.
- [ ] **Step 4: If any step in 2..3 fails, HALT** and proceed to "Blocker handling" emit. Skip Step 5..7 entirely.
- [ ] **Step 5: If all gates pass, draft the `phase-6a-complete` annotated tag message** per D-F3 structure. **Summary HSI results in the tag message body** (one short line per HSI: pass/fail + count); **full verbatim HSI command output in the closeout outbox** (Step 8). Tag message stays concise + human-readable per Phase 5 P5-E precedent.
- [ ] **Step 6: Create the annotated tag** at the post-plan-commit HEAD (recorded in Step 1) with `git tag -a phase-6a-complete -m "..."`. **Verify the tag object** with `git show phase-6a-complete` (message + target commit content check) and `git for-each-ref refs/tags/phase-6a-complete --format='%(objecttype) %(objectname) %(*objectname)'` (confirms an annotated `tag` object pointing at the expected commit). `git tag -v phase-6a-complete` is NOT used (the project does not configure tag signing; `-v` would fail on unsigned annotated tags per v0.2 Codex item 5). A conditional `git tag -v` may be added later if tag signing is enabled.
- [ ] **Step 7: Push the tag and master.** `git push origin master && git push origin phase-6a-complete`.
- [ ] **Step 8: Emit the P6-F closeout report** to `.coordination/claude_outbox.md` with: HEAD SHA, tag SHA, full audit log, HSI verbatim results, "Phase 6a FULLY CLOSED" status, and a note that Phase 6b is **NOT STARTED** and requires fresh user authorization before any Phase 6b planning begins.
- [ ] **Step 9: Update `CLAUDE.md` Phase 6 status block** with the `phase-6a-complete` tag SHA + Phase 6a completion summary. **Only if user requests it**; not auto-landed at P6-F close. (Phase 1..5 precedent: the CLAUDE.md update is a separate doc commit after the tag.)

## Risks + open questions

- **Q-F1 -- Does P6-F require Codex pre-impl review at all?** The Phase 6 overview explicitly says "NO pre-impl review (audit/tag-only per P4-G / P5-E precedent)". The user has overridden the overview for THIS turn by requesting "Emit `.coordination/claude_outbox.md` as the Codex review pack. Stop after drafting the plan; no commit, push, or tag until Codex approval." v0.1 honors the user override. Codex verdict: APPROVE the plan as written, or flag any audit step / HSI definition that requires correction before the audit runs?
- **Q-F2 -- Tag-creation timing relative to `cargo test --workspace`.** v0.1 recommends running `cargo test --workspace` **before** tag creation (D-F1 step 5 is part of the gate sequence that must pass before D-F3 tag-message drafting). Phase 5 P5-E precedent ran tests first. Codex verdict on the ordering?
- **Q-F3 -- Tag-message detail level.** v0.2 LOCKED: **summary HSI results (one short line per HSI: pass/fail + count) in the tag message body; full verbatim command output in the closeout outbox.** The outbox is the durable audit log; the tag stays concise + human-readable per Phase 5 P5-E precedent. Checklist Step 5 + D-F3 are now consistent with this lock (v0.1 had Step 5 saying "verbatim inlined" -- fixed at v0.2 per Codex item 4). Codex verdict ratifying?
- **Q-F4 -- Blocker-handling severity.** v0.1 says any HSI failure HALTS and requires a fresh "P6-F-remediation" batch with Codex review. This is intentionally strict: relaxing an HSI at audit time without explicit boundary-spec change + user authorization would compromise the entire Phase 6a safety story. Codex verdict on the strictness?
- **Q-F5 -- Should P6-F also re-run the P6-D D-T-D1..D-T-D4 tests in isolation** (targeted `cargo test -p rust-lmax-mev-relay-clients --test submit_disabled`) before the full workspace run? v0.1 recommends **NO** -- the workspace run covers it; isolated runs add no signal. Codex verdict?

## Process

Per the 2026-05-04 routine-closeout policy + the overview Section Process:

1. Claude writes this pre-impl plan to disk (UNCOMMITTED) + emits the review pack to `.coordination/claude_outbox.md`.
2. Claude STOPS and reports "manual Codex pre-impl review required for P6-F audit/tag plan". **No `.rs` / `Cargo.toml` / ADR / `docs/specs/` edits in this turn. No audit run. No tag creation. No push.**
3. User pastes pack to Codex; relays verdict.
4. Claude records verdict in `.coordination/codex_review.md`.
5. **APPROVED** -> commit + push this plan as a routine doc commit; THEN execute per Section "Plan execution checklist" Step 1..7; THEN emit closeout outbox.
6. **REVISION REQUIRED** -> revise plan in place + re-emit pack.
7. **Scope / ADR change required** -> HALT to user.
