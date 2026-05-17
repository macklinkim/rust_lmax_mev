# Phase 6b Batch D -- `live_send=true` capability flip (planning only)

**Date:** 2026-05-17 KST
**Status:** Draft v0.4 (revised after Codex REVISION REQUIRED HIGH on v0.3; 2 still-open items + 1 newly-required process-label update + advisory cleanup). PRE-IMPL PLAN. ASCII-only. No `.rs` / `Cargo.toml` / `Cargo.lock` / config-example / fixture / env-example / ADR / spec edits in this turn. No commit, no push.
**Awaiting:** manual Codex re-review of v0.4.

## v0.3 -> v0.4 changelog

| Codex item | v0.3 issue | v0.4 fix |
|---|---|---|
| R-D7 STILL OPEN | The v0.3 copy-exact G2a + G2g commands used `--glob '!**/signer/src/recovery.rs'`. A dry-run from the repo root with search path `crates/` STILL returns `crates/signer/src/recovery.rs` hits because ripgrep's glob matching is relative to the search root in a way that the bare-prefix path `signer/src/recovery.rs` does not match `crates/signer/src/recovery.rs` when invoked from the repo root. Expected "0 hits mechanically" was not actually mechanical. | Both G2a and G2g exclusion globs changed to `--glob '!**/signer/src/recovery.rs'` (full-path-match form). Dry-run from the repo root with search path `crates/` now correctly excludes `crates/signer/src/recovery.rs`; expected 0 hits is mechanically true. Plan + outbox wording updated to name the corrected glob form. |
| R-D3 STILL OPEN | One earlier "Boundary doc reconciliation" subsection under "Architectural design" + Q-D5 still describe the old 2-edit scope ("Section 3 + Section 4"). | Both updated to **3 boundary-doc edits**: (1) Section 2 HSI-8/G5 row update, (2) Section 3 P6B-D amendment paragraph, (3) Section 4 G13 ENFORCED subsection. Q-D5 wording rewritten to ask Codex's verdict on the 3-edit scope (not the 2-edit scope). |
| R-D8 REQUIRED | Process + Verdict shapes section labels still said "v0.2" / "v0.3" with stale plan-version + outbox-cycle wording. | All process labels updated: heading "## Process (v0.4; two-gate from P6B-CD carried forward)"; Step 1 names v0.4 plan + outbox; APPROVED branch -> "commit + push v0.4 plan"; REVISION REQUIRED branch -> "re-emit pack as v0.5". Verdict shapes heading + branches updated identically. |

Advisory items also folded into v0.4:

- "Scope (v0.1)" heading updated to "Scope (v0.4; carried forward from v0.1)" -- the scope decision was locked at v0.1 and has not changed.
- "Tests (7 net targeted; v0.1 LOCKED)" heading updated to "Tests (7 net targeted; carried forward from v0.1)".
- "Copy-exact gate commands (R-D1 fix; v0.2 LOCKED)" heading updated to "Copy-exact gate commands (R-D1 / R-D6 / R-D7 corrections; v0.4 LOCKED)".

## v0.2 -> v0.3 changelog

| Codex item | v0.2 issue | v0.3 fix |
|---|---|---|
| R-D4 STILL OPEN | The v0.2 changelog said "additive ABI-compatible wording is removed", but a stale paragraph in the "ConfigError variant changes" section under "Architectural design" still said `#[non_exhaustive]` made the removal "an additive ABI-compatible change for non-crate-internal callers". Two contradictory positions in the same plan. | Stale paragraph REMOVED. The plan now has ONE position only: the variant rename/removal is a **deliberate workspace-local source-level break**; impl Step 2 verifies via `rg -n 'LiveSendForbidden' crates/` -> 0 hits after the rename. |
| R-D3 STILL OPEN | Two locations still described the boundary-doc scope as "2 additive edits" or "Section 3 + Section 4" only: (1) the "Boundary-doc reconciliation summary" subsection just before the file-touch summary, and (2) the stale heading "v0.1 file-touch summary". | Both updated to **3 boundary-doc edits** matching D-D4 + Step 6 + outbox: (1) Section 2 HSI-8/G5 row update, (2) Section 3 P6B-D amendment paragraph, (3) Section 4 G13 ENFORCED subsection. The "v0.1 file-touch summary" heading is updated to "v0.3 file-touch summary". |
| R-D5 BLOCKING | The validation matrix in "Architectural design" listed rows like `Dev + HsmKms + _any_` -> `HsmKmsRequiresProductionProfile` (P6B-B carry-forward) and `Production + Disabled + _any_` -> `ProductionProfileRequiresHsmKms` (P6B-B). With the LOCKED live-send-first evaluation order, those rows are wrong when `live_send=true`: Dev+HsmKms+live_send=true hits `LiveSendRequiresProductionProfile` FIRST (the P6B-B reject below is unreachable); Production+Disabled+live_send=true hits `LiveSendRequiresHsmKms` FIRST. | The matrix is REWRITTEN to split rows by `live_send` axis. Each `(Profile, KeyBackend)` pair now has up to two rows (`live_send=false`, `live_send=true`) with the actual error variant the live-send-first order produces. The P6B-B reject names continue to apply for the `live_send=false` rows (their evaluation path unchanged); `LiveSendRequires*` variants apply for the `live_send=true` rows. |
| R-D6 BLOCKING | Copy-exact G11 command `rg -n 'sign_tx\(' crates/ --glob '*.rs'` returns many baseline hits (trait method definition, signer-crate test calls, etc.). The "Copy-exact" block's stated expected result "1 production runtime call site" was not mechanically true. | G11 narrowed to scope only `crates/app/src/` + `crates/execution/src/` (production source dirs; excludes signer + test integration files) AND uses `.sign_tx\(` (method call site only, not trait declaration). Expected: 1 hit at `crates/execution/src/lib.rs` (the existing `invoke_signer_for_test` hook). Doc text in the gate table is updated to match. |
| R-D7 REQUIRED | Copy-exact G2a command emitted the allowed `k256` import hits inside `crates/signer/src/recovery.rs` (the P6B-CD G2c/G2d allow-list file). The "Copy-exact" block's expected "0 outside recovery.rs" outcome was not mechanically checkable from a single command. | G2a command now uses `--glob '!**/signer/src/recovery.rs'` to exclude the allow-list file; expected result becomes mechanically 0 hits. Same exclusion applied to the G2g command for consistency (G2g also has allowed wording inside `recovery.rs` doc comments referencing the banned token names). A second follow-up grep on recovery.rs alone is documented as a positive presence check. |

Advisory items also folded into v0.3:

- "What Codex is asked to verdict at v0.1" heading updated to "What Codex is asked to verdict at v0.3".
- "v0.1 LOCKS ..." phrases describing CURRENT decisions in the Q-D1..Q-D7 section + the architectural body rewritten as "carried forward from v0.1" or "v0.1-locked, carried forward". Each phrase now reads as either (a) "Decision X was locked at v0.1 and carries forward to v0.3 unchanged" or (b) the new v0.3 framing if the decision changed.

## v0.1 -> v0.2 changelog

| Codex item | v0.1 issue | v0.2 fix |
|---|---|---|
| R-D1 | Executable ripgrep gate commands used `\|` (escaped pipe) inside `'...'` strings. In ripgrep regex syntax `\|` is a LITERAL pipe, not alternation; a dry-run against `Wallet` does not match. The `\|` in the v0.1 plan was a Markdown table-cell escape that got passed through into the actual command unchanged. | All executable ripgrep gates rewritten to use **multiple `-e` arguments** (each pattern in its own `-e 'PATTERN'`), which sidesteps Markdown pipe-escaping AND is unambiguous ripgrep syntax. A fenced "Copy-exact gate commands" code block under the gate table contains the full set of commands in one place that implementer / Codex can copy verbatim. |
| R-D2 | Step 7 verification command was `cargo test -p rust-lmax-mev-config -p rust-lmax-mev-app` but expected the **signer + config + app** subset to rise from 61 to 67. The signer crate was missing from `-p` despite being part of the baseline-count denominator. | Step 7 command now reads `cargo test -p rust-lmax-mev-signer -p rust-lmax-mev-config -p rust-lmax-mev-app` so the count stays comparable to the P6B-CD 61-test baseline. Expected count UNCHANGED at 67. |
| R-D3 | Boundary-doc edit scope was inconsistent across the plan. D-D4, the file-touch summary, and Step 6 named only Section 3 + Section 4 G13. Q-D7 and the outbox additionally locked a Section 2 HSI-8 / G5 row update. Four documents disagreed. The plan also did not reconcile the existing boundary doc's stale pseudo-field name `signer_kind` against the actual config field name `key_backend`. | D-D4 + file-touch summary + Step 6 + outbox all now uniformly list **3 boundary-doc edits**: Section 2 HSI-8 row update (G5 RELAXED at P6B-D), Section 3 amendment paragraph, Section 4 G13 ENFORCED subsection. The Section 4 G13 subsection's wording uses the actual config field name `key_backend` (not the boundary doc's pseudo-field `signer_kind`); the existing PROPOSED block at Section 4 G13 retains the `signer_kind` wording labeled "PROPOSED at P6B-A authoring time; the operative field name in `crates/config/src/lib.rs::RelayConfig` is `key_backend`". |
| R-D4 | The v0.1 plan called the variant rename "a source-level breaking change" in one place and "additive ABI-compatible because `ConfigError` is `#[non_exhaustive]`" in another. `#[non_exhaustive]` makes ADDING variants additive but does NOT make REMOVING a variant ABI-compatible: any downstream match that names the removed variant fails to compile. | v0.2 LOCKS the variant rename (option (b) from Codex advisory) and frames it as a **deliberate workspace-local source-level break**. The plan now requires a workspace-wide ripgrep verification at Step 2 that `LiveSendForbidden` has 0 remaining references after the rename. The "additive ABI-compatible" wording is removed. v0.2 stops short of option (a) (keep `LiveSendForbidden`) because the existing variant's `Display` literal "until Phase 6b Production Gate" becomes semantically stale at P6B-D close. |

Advisory items folded into v0.2:

- D-D3 + the test-table prefatory text now state explicitly that **existing P6B-B validation tests (`config_validate_rejects_all_5_illegal_profile_keybackend_audit_combos` + `profile_and_key_backend_serde_defaults`) continue to cover the invalid `Production + non-HsmKms` and `non-Production + HsmKms` combinations with `live_send=false`**. D-T-D1..D-T-D7 covers ONLY the `live_send=true` axis; the `live_send=false` matrix is the P6B-B carry-forward.
- The "NO new app-side read of `config.relay.live_send`" invariant is reaffirmed; P6B-E owns the runtime G12 step 7 / G13 assertion site (acknowledged Codex advisory).

## Predecessors

- `phase-6a-complete` at `bd0a53c` (tag object `3c9faaf`).
- Phase 6b overview v0.2 APPROVED HIGH at `49123e9`.
- P6B-A boundary doc + ADR-001 Amendment 1 at `1c490de`.
- P6B-B no-SDK ProductionSigner stub + log-source at `df96ac8`.
- P6B-C HSM/KMS infrastructure + signer audit surface at `b77241a` (+ doc-only YAML fix `ff2edbc`).
- P6B-CD sign-activation at `a68641d` (+ follow-up fix `247e7ad`; both APPROVED HIGH).
- `master` HEAD `247e7ad`. Pre-P6B-D targeted baseline (signer + config + app): **61 passed + 0 ignored**.

## Authorization basis

Phase 6b overview v0.2 prerequisite #1 ("Fresh explicit user authorization per non-goal") REQUIRES a separate user re-authorization for P6B-D implementation even after Codex APPROVES this plan. P6B-D is the **first batch that lifts a Phase 6a hard forbid at the runtime safety contract level**: it relaxes `ConfigError::LiveSendForbidden` so the operator-controlled production profile can pass config validation with `live_send=true`. The P6B-A ADR-001 Amendment 1 already DESCRIBES this exact unlock at the high level (boundary doc Section 4 G13 PROPOSED logic; ADR-001 amendment text "After P6B-D lands, `live_send = true` becomes permissible only for the operator-controlled production profile when paired with the HSM/KMS-backed signer from P6B-B"); P6B-D EFFECTUATES that description.

This plan describes WHAT the implementation would do once authorized. It does NOT implement anything.

## Scope (v0.4; carried forward from v0.1)

P6B-D is a **single-purpose config-validation semantics flip**:

- **Today** (`master` HEAD `247e7ad`): `Config::validate()` rejects `live_send=true` for ALL profiles with `ConfigError::LiveSendForbidden`. The error message reads "relay.live_send=true is forbidden until Phase 6b Production Gate". This reject is the only safety mechanism keeping the flag from being set; the flag is NOT plumbed to any code path elsewhere.
- **At P6B-D close**: `Config::validate()` rejects `live_send=true` UNLESS `(active_profile == Profile::Production) AND (key_backend == KeyBackend::HsmKms) AND (audit_key_id is non-empty)`. The one legal combo passes validation. Dev/Test/Shadow profiles continue to reject `live_send=true` UNCONDITIONALLY (independent of `key_backend`).

Hard scope guard: **NO submission path is unlocked at P6B-D close.** The validation flip is necessary-but-not-sufficient. The downstream invariants remain in force:

- `submit_bundle` adapter impls continue to return `Err(KillSwitchActive)` or `Err(SubmitDisabled)` -- adapter code is NOT touched in P6B-D.
- NO `submit_bundle(` caller exists in `crates/app/src/`. G3 stays at 0.
- NO `eth_sendBundle` runtime call. G14 stays at 5 doc-comment hits.
- NO new runtime caller of `ProductionSigner::sign_tx`. G11 stays at 1 (the existing test-only `invoke_signer_for_test` hook in `crates/execution`).
- The `live_send` flag continues to NOT be plumbed into any non-config code path. P6B-D adds NO new read site of `config.relay.live_send` anywhere in `crates/app/src/` or `crates/execution/`.

P6B-E is the batch that lands `submit_bundle -> Ok(SubmissionReceipt)` + `eth_sendBundle` runtime, gated by the full G12 chain INHERITING G13 per the boundary doc Section 4. P6B-F is the final audit + `phase-6b-complete` tag. Both REMAIN LOCKED behind P6B-D close + separate user re-authorization per non-goal.

## Architectural design

### Validation matrix at P6B-D close (v0.3 R-D5 corrected; aligned to live-send-first eval order)

The two NEW `live_send=true` reject branches fire BEFORE the P6B-B reject chain. Therefore the error variant returned for a given `(Profile, KeyBackend, live_send, audit_key_id)` depends on whether `live_send=true` or `live_send=false`. Rows split accordingly:

#### live_send = false (P6B-B carry-forward; eval reaches the P6B-B chain unchanged)

| `active_profile` | `key_backend` | `audit_key_id` | Result |
|---|---|---|---|
| Dev (default)    | Disabled (default) | (any)        | OK |
| Dev              | HsmKms             | (any)        | REJECT `HsmKmsRequiresProductionProfile` (P6B-B) |
| Test             | Disabled           | (any)        | OK |
| Test             | HsmKms             | (any)        | REJECT `HsmKmsRequiresProductionProfile` (P6B-B) |
| Shadow           | Disabled           | (any)        | OK |
| Shadow           | HsmKms             | (any)        | REJECT `HsmKmsRequiresProductionProfile` (P6B-B) |
| Production       | Disabled           | (any)        | REJECT `ProductionProfileRequiresHsmKms` (P6B-B) |
| Production       | HsmKms             | empty        | REJECT `HsmKmsRequiresNonEmptyAuditKeyId` (P6B-B) |
| Production       | HsmKms             | non-empty    | OK (P6B-CD baseline) |

#### live_send = true (P6B-D NEW; live-send-first branches fire BEFORE the P6B-B chain)

| `active_profile` | `key_backend` | `audit_key_id` | Result |
|---|---|---|---|
| Dev              | (any)              | (any)        | **REJECT** `LiveSendRequiresProductionProfile` (P6B-D NEW; fires before any P6B-B check) |
| Test             | (any)              | (any)        | **REJECT** `LiveSendRequiresProductionProfile` (P6B-D NEW) |
| Shadow           | (any)              | (any)        | **REJECT** `LiveSendRequiresProductionProfile` (P6B-D NEW) |
| Production       | Disabled           | (any)        | **REJECT** `LiveSendRequiresHsmKms` (P6B-D NEW; fires before `ProductionProfileRequiresHsmKms` per live-send-first order) |
| Production       | HsmKms             | empty        | REJECT `HsmKmsRequiresNonEmptyAuditKeyId` (P6B-B; both P6B-D branches are inert because `live_send && Production && HsmKms` satisfies neither `!= Production` nor `!= HsmKms`; eval falls through to the P6B-B chain) |
| Production       | HsmKms             | non-empty    | **OK** (P6B-D NEW; the single legal `live_send=true` combo) |

**Single legal `live_send=true` combo at P6B-D close:** `(Profile::Production, KeyBackend::HsmKms, non-empty audit_key_id)`. Every other `live_send=true` row rejects.

### Validate() evaluation order

Current code (`crates/config/src/lib.rs` lines 640-660 approx) evaluates:

1. `live_send` -> `LiveSendForbidden` (the absolute reject this batch relaxes)
2. `Production && !HsmKms` -> `ProductionProfileRequiresHsmKms`
3. `HsmKms && !Production` -> `HsmKmsRequiresProductionProfile`
4. `HsmKms && empty audit_key_id` -> `HsmKmsRequiresNonEmptyAuditKeyId`

At P6B-D, the live_send check splits into two reject branches AND keeps the P6B-B reject chain (which becomes the rest of the validation). The proposed new order:

```text
// P6B-D: live_send=true requires Production profile.
if self.relay.live_send && self.active_profile != Profile::Production {
    return Err(ConfigError::LiveSendRequiresProductionProfile);
}
// P6B-D: live_send=true requires HsmKms signer (only fires for Production + non-HsmKms,
// which would have been caught by ProductionProfileRequiresHsmKms below; this branch
// is the precise P6B-B G13 contract from the boundary doc Section 4).
if self.relay.live_send
    && self.active_profile == Profile::Production
    && self.relay.key_backend != KeyBackend::HsmKms
{
    return Err(ConfigError::LiveSendRequiresHsmKms);
}
// P6B-B carry-forward rejects (UNCHANGED ORDER):
if self.active_profile == Profile::Production
    && self.relay.key_backend != KeyBackend::HsmKms { return Err(ProductionProfileRequiresHsmKms); }
if self.relay.key_backend == KeyBackend::HsmKms
    && self.active_profile != Profile::Production { return Err(HsmKmsRequiresProductionProfile); }
if self.relay.key_backend == KeyBackend::HsmKms
    && self.relay.audit_key_id.trim().is_empty() { return Err(HsmKmsRequiresNonEmptyAuditKeyId); }
```

Note on evaluation-order subtlety: the second branch `live_send && Production && !HsmKms` is technically redundant with `ProductionProfileRequiresHsmKms` from the P6B-B chain below. The decision to keep the dedicated `LiveSendRequiresHsmKms` variant (rather than letting the P6B-B reject swallow it) was carried forward from v0.1 because:

1. The error message is operator-readable: a Production-profile operator who set `live_send=true` but forgot the HsmKms wiring sees "live_send=true requires HsmKms signer" rather than the more generic "Production profile requires key_backend=HsmKms". Operationally clearer.
2. The G13 boundary doc Section 4 contract explicitly enumerates this branch as a separate gate. Keeping it dedicated keeps the code's structure aligned with the spec.

Codex verdict on this design decision in Q-D1 below.

### `ConfigError` variant changes

`LiveSendForbidden` is REMOVED (rename + repurpose). Two NEW variants ADDED:

```text
#[error("relay.live_send=true requires Production profile")]
LiveSendRequiresProductionProfile,
#[error("relay.live_send=true requires key_backend=HsmKms")]
LiveSendRequiresHsmKms,
```

The variant rename/removal is a **deliberate workspace-local source-level break** (see R-D4 framing in v0.1 -> v0.2 changelog + D-D1 below). `#[non_exhaustive]` permits ADDING variants additively but does NOT make REMOVING a variant ABI-compatible: any in-tree `match` arm or `matches!` macro that names `LiveSendForbidden` fails to compile post-rename. The in-tree consumer scan is: `crates/app::AppError::Config(#[from] ConfigError)` (the `#[from]` impl blankets every variant; not affected) + the in-tree test `cfg_live_send_1_rejects_true` (co-edited in D-D3 step 4). Impl Step 2 runs `rg -n 'LiveSendForbidden' crates/` and expects 0 hits post-rename; any unexpected hit halts the impl turn pending re-review.

Codex verdict on variant rename vs keep-and-rewrite-Display in Q-D2.

### Runtime impact

**ZERO.** The validation flip changes only `Config::validate()` semantics. The runtime path:

- `crates/app/src/lib.rs::run` constructs `ProductionSigner::from_aws_kms(...)` when `key_backend == HsmKms` (P6B-C). The HsmKms arm is reachable only with `Production`+`HsmKms`+non-empty `audit_key_id`. With `live_send=true` now also permissible under that triple, the runtime path is structurally unchanged (no branch on `live_send`).
- `wire_phase4` signature UNCHANGED.
- `crates/app/tests/wire_phase4.rs` UNCHANGED.
- Relay adapters (`crates/relay-clients/`, `crates/bundle-relay/`) UNCHANGED. `submit_bundle` continues to return `Err(...)` from every adapter.
- `crates/execution/`, `crates/signer/` UNCHANGED. The test-only `invoke_signer_for_test` hook is the only `sign_tx` caller; G11 stays at 1.

### Boundary doc reconciliation (3 edits per v0.3 R-D3 unified)

`docs/specs/phase-6b-boundary.md`:

- **Section 2 HSI-8 / G5 row update**: change the "Status at later close" wording for HSI-8 (G5) from "rejected for all profiles" to "RELAXED at P6B-D: `live_send=true` permissible only for `(Profile::Production, KeyBackend::HsmKms, non-empty audit_key_id)`; dev/test/shadow continue to reject unconditionally". The "Relaxed in" column already names P6B-D in the doc-as-written.
- **Section 3 amendment**: NEW paragraph at the bottom describing P6B-D's flip ("config-validation gate now permits the single legal `live_send=true` combo; the runtime safety chain G12 still blocks live submission via the `submit_bundle Err` return and the 0-caller invariant"). Pattern matches the P6B-CD Section 3 amendment.
- **Section 4 G13 ENFORCED subsection**: the PROPOSED logic block (lines ~84-95 currently) gets a NEW companion "At P6B-D close (ENFORCED)" block using the actual config-side names (`key_backend`). The plan adds a small additive paragraph capturing this enforcement (decision carried forward from v0.1); the existing PROPOSED block is explicitly relabeled "PROPOSED at P6B-A authoring time; operative field name in `crates/config/src/lib.rs::RelayConfig` is `key_backend`".

NO edit to:

- `docs/specs/production-signer.md` (Section 2 contract is satisfied; P6B-D is a config validation flip, not a signer-impl change).
- `docs/specs/execution-safety.md` (the "Funded Key / Prod Signer Ban" section's operational scope-lift is the CUMULATIVE effect of all P6B-A..E batches per Amendment 1; P6B-D is one step in that chain. No standalone amendment to execution-safety.md is required.)
- `docs/specs/phase-6a-boundary.md` (Phase 6a contract is inherited, not modified.)
- `docs/adr/ADR-001-*.md` (Amendment 1 already describes the P6B-D unlock; no new amendment needed in P6B-D.)

### Config example files

`config/base/`, `config/dev/`, `config/test/` config files: **UNCHANGED**. NO config-example edits in P6B-D (decision carried forward from v0.1). Examples MUST stay fail-closed (no `live_send=true` set anywhere). Operators set `live_send=true` in their own deployment config when their HSM/KMS wiring is operationally ready; the workspace ships no example showing it.

`config/examples/signing-audit-alert.yaml`: UNCHANGED (operator-side Alertmanager YAML; P6B-CD already narrowed the failure-rule matcher to the 7-label outcome set; P6B-D doesn't change outcome labels).

## Deliverables (D-D1..D-D5)

### D-D1 -- `ConfigError` variant rename + add

`crates/config/src/lib.rs`. Remove `ConfigError::LiveSendForbidden`. Add `ConfigError::LiveSendRequiresProductionProfile` + `ConfigError::LiveSendRequiresHsmKms`. Both NEW variants are payload-free + preserve `ConfigError: thiserror::Error + Debug + Display`.

**v0.2 R-D4 framing**: this variant rename is a **deliberate workspace-local source-level break**. `#[non_exhaustive]` on `ConfigError` makes ADDING variants additive but does NOT make REMOVING a variant ABI-compatible: any downstream `match` arm or `matches!` macro that names `LiveSendForbidden` fails to compile post-rename. The break is workspace-local because `crates/config` is `publish = false` and the only in-tree consumers are:

- `crates/app::AppError::Config(#[from] ConfigError)` -- the `#[from]` impl blankets every `ConfigError` variant, so the variant rename does NOT affect `AppError` matching semantics.
- The in-tree test `cfg_live_send_1_rejects_true` -- the test names `LiveSendForbidden` and is co-edited in D-D3 step 4.

P6B-D Step 2 (plan execution checklist below) includes a workspace-wide ripgrep verification that 0 `LiveSendForbidden` references remain after the rename (`rg -n 'LiveSendForbidden' crates/`). Any unexpected hit halts the impl turn pending re-review.

Alternative considered + rejected (option (a) from Codex R-D4 advisory): keep `LiveSendForbidden` (rewriting only its Display) and add only `LiveSendRequiresHsmKms`. v0.2 rejects (a) because the existing variant's Display literal `"relay.live_send=true is forbidden until Phase 6b Production Gate"` becomes semantically stale at P6B-D close (P6B-D IS the Production-Gate moment for the live_send-flip), and the naming `LiveSendForbidden` no longer matches the variant's actual purpose under (a) (the variant would represent "live_send=true requires Production profile", which is what the new name `LiveSendRequiresProductionProfile` says cleanly).

### D-D2 -- `Config::validate()` body update

`crates/config/src/lib.rs`. Replace the existing single `live_send` reject with the 2-branch P6B-D reject (Section "Validate() evaluation order" above). Keep the P6B-B reject chain UNCHANGED (order preserved; lines 645+ continue as today).

### D-D3 -- Existing test rewrite + 6 new tests

`crates/config/src/lib.rs` `#[cfg(test)] mod tests`. D-T-D1..D-T-D7 cover ONLY the `live_send=true` axis of the validation matrix. The existing P6B-B tests (`config_validate_rejects_all_5_illegal_profile_keybackend_audit_combos` + `profile_and_key_backend_serde_defaults`) continue to cover the orthogonal `live_send=false` combined with invalid `(Profile, KeyBackend, audit_key_id)` combinations -- those tests are NOT rewritten in P6B-D because their behavior is unchanged (the P6B-B reject chain at lines 645+ stays bit-for-bit identical, only the live_send branch above it is replaced).

- Existing test `cfg_live_send_1_rejects_true` (currently asserts `ConfigError::LiveSendForbidden` for `live_send=true` with default Dev profile): REWRITTEN to assert `ConfigError::LiveSendRequiresProductionProfile`. The test's TOML body is unchanged (only the expected error variant).
- NEW tests (D-T-D1..D-T-D7 -- the v0.2 plan's minimum test set; 6 new + 1 rewrite = 7 net validation-matrix coverage for the live_send=true axis):

| ID | Profile + KeyBackend + live_send + audit_key_id combo | Expected result |
|---|---|---|
| D-T-D1 (rewrite of `cfg_live_send_1_rejects_true`) | `(Dev, Disabled, true, "")` | `Err(LiveSendRequiresProductionProfile)` |
| D-T-D2 | `(Test, Disabled, true, "")` | `Err(LiveSendRequiresProductionProfile)` |
| D-T-D3 | `(Shadow, Disabled, true, "")` | `Err(LiveSendRequiresProductionProfile)` |
| D-T-D4 | `(Production, Disabled, true, "")` | `Err(LiveSendRequiresHsmKms)` (NOTE: this fires before `ProductionProfileRequiresHsmKms` because the live_send-specific branch is evaluated FIRST per the ordering decision in Q-D1) |
| D-T-D5 | `(Production, HsmKms, true, "k1")` | `Ok(_)` -- the single legal `live_send=true` combo PASSES validation |
| D-T-D6 | `(Production, HsmKms, false, "k1")` | `Ok(_)` -- P6B-CD baseline preserved (sanity check that `live_send=false` continues to pass) |

Plus 1 carry-forward sanity test:

| ID | Combo | Expected |
|---|---|---|
| D-T-D7 | All-defaults TOML (`active_profile` omitted -> `Dev`, `live_send` omitted -> `false`, `key_backend` omitted -> `Disabled`, `audit_key_id` omitted -> `""`) | `Ok(_)` AND `cfg.relay.live_send == false` (defaults preserved) |

Targeted count delta at P6B-D close: pre-P6B-D `cargo test -p signer -p config -p app` = 61 passed. Adding 6 new tests + 1 rewrite (rewrite stays counted; net +6) = **67 passed + 0 ignored**.

### D-D4 -- Boundary doc additive amendment (v0.2 R-D3: 3 edits)

`docs/specs/phase-6b-boundary.md`. v0.2 unifies the edit scope across this Section, the file-touch summary, Step 6, and the outbox: **3 additive edits** in P6B-D.

1. **Section 2 HSI-8 / G5 row update**: change "Status at P6B-A close" / "Status at later close" wording for HSI-8 (G5) from "rejected for all profiles" to "RELAXED at P6B-D: `live_send=true` permissible only for `(Profile::Production, KeyBackend::HsmKms, non-empty audit_key_id)`; dev/test/shadow continue to reject unconditionally". The "Relaxed in" column for HSI-8 already says P6B-D (per the boundary doc as-written); v0.2 changes the "Status at P6B-A close" sentinel to a per-batch progression note acknowledging P6B-D as the relaxation event.
2. **Section 3 amendment block**: ADD a new paragraph at the bottom of the existing Section 3 amendment block (which currently has P6B-C v0.3 + P6B-CD v0.4 entries). The new paragraph names P6B-D, the validation flip, the single legal combo, and the still-locked downstream chain (`submit_bundle Err` + 0-callers + no `eth_sendBundle` runtime).
3. **Section 4 G13 "ENFORCED" subsection**: ADD an additive subsection "**At P6B-D close (ENFORCED):**" containing the actual code-level reject names (`LiveSendRequiresProductionProfile`, `LiveSendRequiresHsmKms`) AND using the actual `crates/config` field name `key_backend` (NOT the boundary doc's pre-P6B-D pseudo-field `signer_kind`). The existing "PROPOSED" block above (currently lines 84-95) stays as historical reference but is **explicitly relabeled** "PROPOSED at P6B-A authoring time; the operative field name in `crates/config/src/lib.rs::RelayConfig` is `key_backend`" so reader confusion between `signer_kind` and `key_backend` is bounded.

The 3 edits are jointly the scope of `docs/specs/phase-6b-boundary.md` change in P6B-D. No other boundary-doc section is touched.

### D-D5 -- NO ADR amendment

ADR-001 Amendment 1 already describes the P6B-D unlock. P6B-D effectuates the description without requiring its own ADR amendment. NO `docs/adr/` edit in P6B-D.

NO new ADR-001 Amendment 3.

## Out of scope (explicitly NOT P6B-D)

- `submit_bundle -> Ok(SubmissionReceipt)`. P6B-E.
- `eth_sendBundle` runtime call site. P6B-E.
- New runtime caller of `submit_bundle(` in `crates/app/src/`. P6B-E.
- Any plumbing of `config.relay.live_send` into `crates/app/`, `crates/execution/`, `crates/relay-clients/`, `crates/bundle-relay/`. P6B-E (the boundary doc Section 4 G12 step 7 "G13 inheritance runtime assertion" lives at the eventual P6B-E `submit_bundle(` caller; P6B-D adds no such read site).
- `KillSwitch` extensions. NOT IN PHASE 6 SCOPE.
- `Phase 6b overview prerequisite #5` re-verification (P6B-CD already satisfied it at the design level; positive Ok-path runtime exercise is P6B-E's responsibility).
- `phase-6b-complete` tag creation. P6B-F.
- Per-callsite documentation in `docs/specs/phase-6b-boundary.md` Section 5. P6B-E adds the actual call sites; Section 5 stays empty at P6B-D close.

## Tests (7 net targeted; carried forward from v0.1)

See D-D3 table above. 6 new + 1 rewrite = 7 total. All in `crates/config/src/lib.rs::tests`. NO new live-network, live-KMS, or `#[ignore]` test in P6B-D. G7 (ignore count) UNCHANGED at 1 (P2-C carry-forward).

`crates/signer/` and `crates/app/` test suites: UNCHANGED. No new tests in those crates because their behavior is structurally unchanged by P6B-D.

## Gates at P6B-D close (deltas vs P6B-CD close `247e7ad`)

| Gate | Status at P6B-CD close (`247e7ad`) | Status at P6B-D close |
|---|---|---|
| G2a (signer-symbol literals in `crates/*.rs`) | 0 hits outside `crates/signer/src/recovery.rs` allow-list | UNCHANGED |
| G2b (signer-dep literals in `crates/**/Cargo.toml`) | 0 | UNCHANGED |
| G2c/G2d (signer-symbol allow-list) | 13 files | UNCHANGED at 13 (P6B-D adds no new file under the allow-list) |
| G2e (signer dep edges) | 2 | UNCHANGED |
| G2f (narrow k256 surface) | 0 | UNCHANGED |
| G2g (signing-key constructors / test-key ban) | 0 | UNCHANGED |
| G3 (`submit_bundle(` callers in `crates/app/src/`) | 0 | UNCHANGED at 0 |
| G4 (`dyn BundleRelay` in `crates/app/src/`) | 0 | UNCHANGED at 0 |
| G5 (config-validation rejects) | `LiveSendForbidden` + 3 P6B-B rejects + 2 P4-E rejects | **MODIFIED**: `LiveSendForbidden` removed; `LiveSendRequiresProductionProfile` + `LiveSendRequiresHsmKms` added; P6B-B + P4-E rejects UNCHANGED. Single legal `live_send=true` combo now passes. |
| G6 (`api_key` in `tracing::*!`) | 0 | UNCHANGED |
| G7 (`#[ignore]` count) | 1 | UNCHANGED at 1 |
| G8 (cargo tree -d cycles) | 0 | UNCHANGED |
| G9 (KillSwitch allow-list) | 3 files | UNCHANGED |
| G10 (per-adapter submit_bundle first-statement KS guard) | enforced | UNCHANGED |
| G11 (production `sign_tx` call site count) | 1 (test-only hook) | UNCHANGED at 1 |
| G12 (submit_bundle pre-check chain) | vacuously satisfied (0 callers) | UNCHANGED |
| G13 (live_send=true profile scope) | PROPOSED in boundary doc; ABSOLUTE reject in code | **ENFORCED**: the boundary doc PROPOSED logic is now in `Config::validate()`. |
| G14 (`eth_sendBundle` runtime) | 5 doc-comment hits | UNCHANGED at 5 |
| G15 (production-signer audit-surface contract) | 4-piece surface + 7-label set | UNCHANGED |

**NEW gate-style invariant at P6B-D close (not a numbered G-gate; P6B-E will use this as a prerequisite for G12 step 7):**

No app-side read of `config.relay.live_send` (P6B-D introduces no plumbing). Expected: 0 hits at P6B-D close. P6B-E lifts this to a "verbatim documented call site" requirement when the runtime G12 step 7 assertion lands.

### Copy-exact gate commands (R-D1 / R-D6 / R-D7 corrections; v0.4 LOCKED)

The commands below use **multiple `-e` arguments** (each pattern in its own `-e 'PATTERN'`) instead of pipe-alternation inside a single regex. This sidesteps Markdown table-cell pipe escaping AND is unambiguous ripgrep syntax. The implementer + Codex copy these commands verbatim at the verification step.

```sh
# G2a (signer-symbol literals; v0.4 R-D7 corrected: excludes the recovery.rs
#  allow-list file via --glob '!**/signer/src/recovery.rs' (full-path-match form
#  so it works from the repo root with search path crates/); expected 0 hits
#  mechanically; verified by dry-run.)
rg -n -e 'Wallet' -e 'PrivateKey' -e 'secp256k1' -e '\bk256\b' -e 'sign_transaction' -e '\bfunded\b' crates/ --glob '*.rs' --glob '!**/signer/src/recovery.rs'

# G2a positive presence check (recovery.rs MUST contain the permitted k256 imports;
#  expected >= 1 hit)
rg -n 'use k256::' crates/signer/src/recovery.rs

# G2b (signer-dep literals in Cargo.toml; expected 0)
rg -n --glob 'crates/**/Cargo.toml' -e 'alloy-signer' -e 'ethers-signers' -e 'secp256k1'

# G2f (narrow k256 surface; expected 0)
rg -n -e 'k256::SecretKey' -e 'k256::ecdsa::SigningKey' -e 'k256::Scalar' -e 'k256::FieldElement' -e 'k256::ProjectivePoint' -e 'k256::AffinePoint' -e 'k256::elliptic_curve' crates/ --glob '*.rs'

# G2g (signing-key constructors + test-key literals; v0.3 R-D7: excludes recovery.rs
#  for the same allow-list reason as G2a -- the recovery.rs file's doc comments
#  reference the banned token names by string for documentation purposes;
#  expected 0 hits outside recovery.rs)
rg -n -e 'test_key' -e 'TEST_KEY' -e 'TEST_PRIV' -e 'TEST_PRIVATE' -e 'SecretKey' -e 'SigningKey' -e 'from_bytes_be' -e 'from_slice_be' -e '::random\(' -e '::generate\(' crates/ --glob '*.rs' --glob '!**/signer/src/recovery.rs'

# G3 (submit_bundle callers in app/src; expected 0)
rg -n 'submit_bundle\(' crates/app/src/

# G4 (BundleRelay trait objects in app/src; expected 0)
rg -n -e 'dyn BundleRelay' -e 'Arc<dyn BundleRelay>' crates/app/src/

# G5 (P6B-D config-error variant census; expected: 0 LiveSendForbidden refs + at least 1 ref to each NEW variant)
rg -n -e 'LiveSendForbidden' -e 'LiveSendRequiresProductionProfile' -e 'LiveSendRequiresHsmKms' crates/

# G7 (#[ignore] count; expected 1, the P2-C carry-forward)
rg -n '#\[ignore\]' crates/ --glob '*.rs'

# G11 (production sign_tx call site count; v0.3 R-D6: narrowed to production-source
#  directories ONLY (crates/app/src + crates/execution/src) AND uses '.sign_tx\('
#  to match method call sites only (not the trait definition in
#  crates/signer/src/signer_trait.rs nor signer in-crate test calls).
#  Expected: 1 hit at crates/execution/src/lib.rs (the existing
#  #[cfg(test)] pub(crate) async fn invoke_signer_for_test hook body).)
rg -n '\.sign_tx\(' crates/app/src/ crates/execution/src/

# G13 (live_send references in crates/config/src/; expected declaration + default + 2 reject branches + carry-forward refs)
rg -n 'live_send' crates/config/src/lib.rs

# G14 (eth_sendBundle runtime; expected 5 doc-comment hits)
rg -n 'eth_sendBundle' crates/ --glob '*.rs'

# P6B-D NEW invariant: NO app-side read of config.relay.live_send (expected 0)
rg -n -e 'config\.relay\.live_send' -e '\.live_send' crates/app/src/

# R-D4 verification: 0 remaining LiveSendForbidden references after the rename
rg -n 'LiveSendForbidden' crates/
```

## Hard forbids at P6B-D close

- NO `submit_bundle -> Ok(_)`. Relay adapter impls remain unchanged.
- NO `submit_bundle(` caller in `crates/app/src/`. G3 stays at 0.
- NO `eth_sendBundle` runtime call site. G14 stays at 5 doc-comment hits.
- NO actual relay submission.
- NO new runtime call site of `ProductionSigner::sign_tx` outside the existing test-only `invoke_signer_for_test` hook.
- NO live-network test enabled by default. NO live-KMS test by default. NO new `#[ignore]`-gated test.
- NO private-key bytes / seed / wallet / raw secret / env-example key material / test-key literal / signing-key constructor anywhere (G2a + G2g UNCHANGED).
- NO change to `crates/signer/`, `crates/execution/`, `crates/relay-clients/`, `crates/bundle-relay/`, `crates/relay-sim/`, `crates/app/src/main.rs`, `crates/app/tests/`, `crates/app/src/lib.rs::wire_phase4`. Decision carried forward from v0.1: `crates/app/src/lib.rs` is NOT touched at all in P6B-D; if a future review surfaces a needed app-side change, the plan revises forward.
- NO change to `Profile` / `KeyBackend` enums.
- NO new field on `RelayConfig` or `Config`.
- NO change to `config/base/`, `config/dev/`, `config/test/`, or `config/examples/`. Example configs stay fail-closed.
- NO change to `Cargo.toml` or `Cargo.lock`. P6B-D adds no dependencies.
- NO ADR-001 amendment. Amendment 1 already describes the P6B-D unlock.
- NO change to `docs/specs/production-signer.md`, `docs/specs/execution-safety.md`, `docs/specs/phase-6a-boundary.md`.
- NO `.coordination/` staging (gitignored). NO `AGENTS.md`, `fixture_output.txt`, `hook_toast.md` staging.
- NO destructive git. NO `phase-6b-complete` tag (P6B-F scope).

## Boundary-doc reconciliation summary

`docs/specs/phase-6b-boundary.md` is the ONLY spec doc touched by P6B-D. **3 additive edits** (R-D3 v0.3 unified):

1. **Section 2 HSI-8 / G5 row update**: change the "Status at later close" wording for HSI-8 (G5) from "rejected for all profiles" to "RELAXED at P6B-D: `live_send=true` permissible only for `(Profile::Production, KeyBackend::HsmKms, non-empty audit_key_id)`; dev/test/shadow continue to reject unconditionally". The "Relaxed in" column already names P6B-D in the doc-as-written.
2. **Section 3 amendment block**: NEW paragraph for P6B-D below the existing P6B-CD entry. Names the validation flip, the single legal combo, and the still-locked downstream chain (`submit_bundle Err`, 0-callers, no `eth_sendBundle` runtime).
3. **Section 4 G13 ENFORCED subsection**: NEW "At P6B-D close (ENFORCED)" block using the actual `crates/config` field name `key_backend` (NOT the boundary doc's pre-P6B-D pseudo-field `signer_kind`). The existing PROPOSED block above (currently "At P6B-D close (PROPOSED logic; LOCKED at P6B-D plan time)") is **explicitly relabeled** "PROPOSED at P6B-A authoring time; operative field name in `crates/config/src/lib.rs::RelayConfig` is `key_backend`".

NO edit to `production-signer.md`, `execution-safety.md`, `phase-6a-boundary.md`, or any `docs/adr/` file.

## v0.3 file-touch summary (for the future P6B-D impl turn)

| File | Change kind |
|---|---|
| `crates/config/src/lib.rs` | Substantive: `LiveSendForbidden` variant removed (deliberate workspace-local source-level break per R-D4 framing); `LiveSendRequiresProductionProfile` + `LiveSendRequiresHsmKms` added; `Config::validate()` body updated with 2-branch live_send reject; existing `cfg_live_send_1_rejects_true` test rewritten to name the new variant; 6 new D-T-D2..D-T-D7 tests added. |
| `docs/specs/phase-6b-boundary.md` | Additive (3 edits per v0.2 R-D3): Section 2 HSI-8 / G5 row update + Section 3 P6B-D amendment paragraph + Section 4 G13 ENFORCED subsection. The Section 4 ENFORCED wording uses the actual `crates/config` field name `key_backend`; the existing PROPOSED block is relabeled to point at the operative field name. |

NO touch in P6B-D:

- `crates/app/`, `crates/signer/`, `crates/execution/`, `crates/relay-clients/`, `crates/bundle-relay/`, `crates/relay-sim/`, all other crates.
- `Cargo.toml`, `Cargo.lock`.
- `config/base/`, `config/dev/`, `config/test/`, `config/examples/`.
- `docs/specs/production-signer.md`, `docs/specs/execution-safety.md`, `docs/specs/phase-6a-boundary.md`.
- `docs/adr/`.
- All previously-frozen plan files under `docs/superpowers/plans/`.

## Plan execution checklist (after Codex APPROVED + explicit user re-authorization)

- [ ] **Step 1**: User explicitly re-authorizes the P6B-D implementation.
- [ ] **Step 2 (R-D4 verification)**: Edit `crates/config/src/lib.rs`: remove `LiveSendForbidden`; add `LiveSendRequiresProductionProfile` + `LiveSendRequiresHsmKms` (both payload-free + thiserror-Error). Run `rg -n 'LiveSendForbidden' crates/` and expect **0 hits** (deliberate workspace-local source-level break verified). Any unexpected hit -> halt impl turn pending re-review.
- [ ] **Step 3**: Update `Config::validate()` body to insert the 2-branch live_send reject BEFORE the P6B-B reject chain (preserving the P6B-B chain's existing order). The 2 new branches use the live-send-first ordering per Q-D1: `LiveSendRequiresProductionProfile` then `LiveSendRequiresHsmKms`.
- [ ] **Step 4**: Rewrite the existing `cfg_live_send_1_rejects_true` test to assert `ConfigError::LiveSendRequiresProductionProfile`.
- [ ] **Step 5**: Add D-T-D2..D-T-D7 tests per the D-D3 matrix.
- [ ] **Step 6 (R-D3 unified scope)**: Update `docs/specs/phase-6b-boundary.md` per D-D4 with **3 edits**: (a) Section 2 HSI-8 / G5 row update describing the P6B-D relaxation, (b) Section 3 P6B-D amendment paragraph, (c) Section 4 G13 "ENFORCED" subsection using `key_backend` (NOT `signer_kind`) for the operative field name + the historical PROPOSED block relabeled.
- [ ] **Step 7 (R-D2 corrected)**: Targeted self-check: `cargo fmt --check`; `cargo clippy -p rust-lmax-mev-signer -p rust-lmax-mev-config -p rust-lmax-mev-app --all-targets -- -D warnings`; `cargo test -p rust-lmax-mev-signer -p rust-lmax-mev-config -p rust-lmax-mev-app` (expect signer + config + app subset count to reach 61 + 6 = **67 passed + 0 ignored** per D-D3); P6B-D targeted ripgrep gates per the "Copy-exact gate commands" fenced block above.
- [ ] **Step 8**: Commit + push as `feat(p6b-d): relax live_send=true for Production+HsmKms profile only`.
- [ ] **Step 9**: Emit P6B-D closeout report to `.coordination/claude_outbox.md` naming the new ConfigError variants, the gate G13 ENFORCED status, the 3 boundary-doc additions, and the still-locked P6B-E/F items.

## Risks + open questions (Q-D1..Q-D7)

- **Q-D1 (validation evaluation order)**: live-send-first is carried forward from v0.1. The `LiveSendRequiresHsmKms` branch is evaluated BEFORE the P6B-B `ProductionProfileRequiresHsmKms` branch so a Production-profile operator who set `live_send=true` but forgot HsmKms wiring sees the live-send-specific error message. Codex verdict: should the order instead be P6B-B-first (which would yield `ProductionProfileRequiresHsmKms` for the same input), making `LiveSendRequiresHsmKms` reachable only when P6B-B's check is somehow disabled? The validation matrix in "Architectural design" (v0.3 R-D5 corrected) reflects live-send-first; switching to P6B-B-first would change the matrix.
- **Q-D2 (variant rename vs Display rewrite)**: v0.1 REMOVES `LiveSendForbidden` and ADDS two new variants. The alternative is to keep `LiveSendForbidden` (rewriting only its Display + repurposing the variant for the dev/test/shadow case) and add `LiveSendRequiresHsmKms`. Variant rename has cleaner naming + matches the P6B-B bidirectional reject pattern but is a 1-variant ABI change. Codex verdict.
- **Q-D3 (NO config example with live_send=true)**: NO config example file in `config/` is updated to demonstrate the legal `live_send=true` combo (decision carried forward from v0.1). Examples stay fail-closed; operators read the boundary doc + Display text to learn the legal combo. Codex verdict: should P6B-D ship a documented example, or is fail-closed the right default?
- **Q-D4 (zero app touch)**: `crates/app/src/lib.rs` is NOT touched at all in P6B-D (decision carried forward from v0.1). The HsmKms construction at `run()` does NOT branch on `live_send` (it doesn't need to: HsmKms construction is already guarded by `key_backend == HsmKms`, which is the same set as the legal `live_send=true` combo). Codex verdict: does ANY app-side change become necessary in P6B-D, or is "validation-only" the right scoping?
- **Q-D5 (boundary doc edits scope)**: v0.4 LOCKS **3 additive edits** to `phase-6b-boundary.md` -- (1) Section 2 HSI-8/G5 row update, (2) Section 3 P6B-D amendment paragraph, (3) Section 4 G13 ENFORCED subsection. The 3-edit scope is consistent across D-D4, Step 6, the Boundary-doc reconciliation summary, the file-touch summary, and the outbox. Codex verdict on whether the 3-edit scope is correct + the exact wording for each edit.
- **Q-D6 (test count + naming)**: v0.1 proposes 6 new + 1 rewrite (D-T-D1 reuses the slot of the rewritten `cfg_live_send_1_rejects_true`). Total +6 net. Codex verdict: is D-T-D2..D-T-D7 sufficient, or is any case missing (e.g., env-overlay override of `live_send` from `RUST_LMAX_MEV__RELAY__LIVE_SEND=true` -- which would go through the same validate() path so should be covered by D-T-D1..D-T-D7 indirectly)?
- **Q-D7 (G5 documentation in boundary doc)**: The boundary doc Section 4 G13 already specifies the post-P6B-D state. Does the G5 row in the Section 2 HSI inheritance table need updating from "HSI-8 G5: rejected for all profiles" to "HSI-8 G5 RELAXED at P6B-D: production-only"? YES, carried forward from v0.1 (D-D4 includes Section 4 G13 update + Section 3 amendment + Section 2 HSI-8 row update; v0.3 R-D3 unifies this scope across all plan sections). Codex verdict on the exact wording.

## Process (v0.4; two-gate from P6B-CD carried forward)

Plan approval and implementation authorization are TWO SEPARATE gates. Codex APPROVAL on this plan is approval to COMMIT THE PLAN ONLY; it is NOT authorization to begin Steps 1..9. Implementation requires a SECOND user gesture after the plan commit lands on `master`.

1. Claude writes this v0.4 plan to disk (UNCOMMITTED) + re-emits the v0.4 Codex review pack to `.coordination/claude_outbox.md`.
2. Claude STOPS for manual Codex re-review.
3. User pastes pack to Codex; relays verdict.
4. Claude records verdict in `.coordination/codex_review.md` (gitignored handoff file).
5. **APPROVED** -> commit + push the v0.4 plan to `master`; STOP. P6B-D implementation does NOT begin in the same turn. A SEPARATE subsequent user message must explicitly re-authorize Steps 1..9 before any `.rs` / `Cargo.toml` / `Cargo.lock` / config / spec / boundary-doc / ADR change.
6. **REVISION REQUIRED** -> revise plan in place + re-emit pack as v0.5.
7. **Scope / ADR change required beyond what this plan proposes** -> HALT to user.

## What Codex is asked to verdict at v0.3

1. Q-D1: validation evaluation order (live-send-first vs P6B-B-first).
2. Q-D2: variant rename (remove `LiveSendForbidden`, add two) vs keep-and-rewrite.
3. Q-D3: NO example config showing `live_send=true` (operator-discovers via spec).
4. Q-D4: ZERO app touch (validation-only batch).
5. Q-D5: boundary-doc edit scope (Section 3 + Section 4 G13 + Section 2 HSI-8 row).
6. Q-D6: test count (6 new + 1 rewrite = 7 total in `crates/config`).
7. Q-D7: HSI-8 G5 row wording in Section 2.
8. Whether the v0.1 plan correctly preserves every P6B-CD invariant (G3/G4/G11/G14/G15 unchanged; sign_tx Ok still test-only; no relay submission unlocked).
9. Whether the chain of locks (P6B-D -> P6B-E -> P6B-F) is correctly stated.
10. Whether the "NO new app-side read of `config.relay.live_send`" pre-condition for P6B-E G12 step 7 is the right place to land that invariant.

## Verdict shapes Claude expects (v0.4)

- **APPROVED** -> commit + push the v0.4 plan to `master`; STOP. P6B-D implementation does NOT begin in the same turn. A SEPARATE subsequent user message must explicitly re-authorize Steps 1..9 before any source-level change.
- **REVISION REQUIRED** -> revise plan in place + re-emit pack as v0.5.
- **Scope / ADR change required beyond what this plan proposes** -> HALT to user.
