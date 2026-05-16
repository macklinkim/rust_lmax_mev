# Phase 6b Production Gate Boundary Spec

## Section 1 -- Status + Scope

Phase 6b runtime contract; sibling to `docs/specs/phase-6a-boundary.md`. Lands as a P6B-A deliverable after Codex APPROVED HIGH on the P6B-A pre-impl pack (plan v0.3 at commit `2ddba8a`) AND explicit user re-authorization for the ADR-001 scope-lift describing-only amendment.

This doc captures the **Phase 6b unlock contract** for each of the live-action gates (`live_send=true`, `eth_sendBundle` runtime, actual relay submission) and the **per-batch unlock sequence** P6B-A..F. Phase 6b is the ONLY path to live action per ADR-001 + `docs/specs/execution-safety.md` + `docs/specs/production-signer.md`.

**At P6B-A close (this doc landing) the workspace remains at the Phase 6a fail-closed baseline.** Phase 6a HSI-1..HSI-11 stay UNCHANGED at P6B-A close. P6B-A unlocks NO live-action gate. The relaxations enumerated in Section 2 happen only in later batches.

Parent context:

- `docs/specs/phase-6a-boundary.md` -- the canonical Phase 6a contract (HSI-1..HSI-11; G1..G11). Phase 6b inherits from it.
- `docs/specs/production-signer.md` -- the production-signer design contract (HSM/KMS-only custody; never-in-memory key material; auditability; threat model with host-compromise residual + Phase 6b control point).
- `docs/specs/execution-safety.md` -- the parent safety policy. Section "Funded Key / Prod Signer Ban" stays in force at P6B-A close; the operational scope-lift is the cumulative effect of P6B-B + P6B-C + P6B-D + P6B-E landing in sequence.

## Section 2 -- Phase 6a HSI inheritance + relaxation map

| HSI | Gate | Status at `phase-6a-complete` | Relaxed in | Under what control | Status at P6B-A close |
|---|---|---|---|---|---|
| HSI-1 | G1 | `eth_sendBundle` doc-comment-only in `crates/` (5 `//!` hits) | P6B-E | New runtime call sites documented per file:line in Section 5; each guarded by the G12 chain INHERITING G13 | UNCHANGED (5 doc-comment hits) |
| HSI-2 | G2a | 0 hits of forbidden signer-symbol set in `crates/*.rs` | NEVER | HSM/KMS-backed signer impl MUST NOT introduce any forbidden symbol; key material lives in HSM/KMS, not in source | UNCHANGED (0 hits) |
| HSI-3 | G2b | 0 hits of forbidden signer-dep set in `crates/**/Cargo.toml` | P6B-B (only if the HSM/KMS client library is in the banned set; v0.3 plan RECOMMENDS against) | Codex review at P6B-B plan time | UNCHANGED (0 hits) |
| HSI-4 | G2c / G2d | Signer-symbol allow-list = `crates/signer/` + 3 approved files | P6B-B | Allow-list grows by ONE file (the new production signer impl module inside `crates/signer/`) | UNCHANGED |
| HSI-5 | G2e | 2 `signer = { path = "../signer" }` dep edges | NEVER (v0.3 plan RECOMMENDS housing the new impl inside `crates/signer/`, keeping dep-edge count at 2) | -- | UNCHANGED (2 edges) |
| HSI-6 | G3 | 0 `submit_bundle(` callers in `crates/app/src/` | P6B-E | Each new caller documented per file:line in Section 5; guarded by the **full 7-step G12 chain INHERITING G13** (kill-switch + signer Ok + local-sim Ok + relay-sim Ok + comparator Match + bundle-byte equality + runtime `live_send/profile/signer_kind` assertion) | UNCHANGED (0 callers) |
| HSI-7 | G4 | 0 `dyn BundleRelay` / `Arc<dyn BundleRelay>` in `crates/app/src/` | P6B-E | Per-callsite decision recorded at P6B-E plan time; documented in Section 5 | UNCHANGED (0 hits) |
| HSI-8 | G5 | `live_send=true` config-validation rejected for ALL profiles | P6B-D | Reject relaxed ONLY for the operator-controlled production profile AND ONLY when paired with `signer_kind == HsmKms`; dev/test/shadow continue to reject unconditionally | UNCHANGED (rejected for all profiles) |
| HSI-9 | G6 / G7 | `api_key` never in `tracing::*!`; `#[ignore]` count = 1 (P2-C carry-forward) | NEVER (G6); per-batch with explicit user approval (G7) | -- (G6); any live-network test added must be `#[ignore]`-gated + env-overlay opt-in | UNCHANGED |
| HSI-10 | G8 / G9 | No workspace dep cycles; `KillSwitch` allow-list = `bundle-relay` + `app` + `relay-clients` | Per-batch as needed | All `KillSwitch` additions stay in the existing allow-list unless Section 5 extends it | UNCHANGED |
| HSI-11 | G10 / G11 | G10: per-adapter `submit_bundle` first-statement kill-switch guard; G11: single `sign_tx` production call site at `crates/execution/src/lib.rs:238` | G10 REINFORCED (never relaxed) in P6B-E; G11 grows by additional callsites in P6B-E | G10: `Ok(_)` return path in P6B-E runs only after the guard passes + remains inactive throughout submission; G11: each new callsite documented in Section 5; routing through `&dyn Signer` preserved | UNCHANGED |

ALL ELEVEN HSI stay UNCHANGED at P6B-A close.

## Section 3 -- Per-batch unlock contract (P6B-A..F)

| Batch | Unlock-gate (this batch lands) | Prerequisite controls landed in earlier batches | Fail-closed posture for remaining batches |
|---|---|---|---|
| P6B-A | None. Boundary doc + ADR-001 describing-only amendment. | -- | Every HSI stays UNCHANGED. No live-action gate is unlocked. |
| P6B-B | HSM/KMS-backed `Signer` impl that can return `Ok(SignedTxBytes)` from `sign_tx`; at least one Section 2.5 host-compromise control wired (not merely documented). | P6B-A boundary doc + ADR amendment. | `submit_bundle` continues to return `Err(KillSwitchActive)` (KS active) or `Err(SubmitDisabled)` (KS inactive). G3 + G4 + HSI-8 G5 + HSI-1 G1 stay fail-closed. |
| P6B-C | Funded key material wired operationally to the HSM/KMS. NO private-key bytes in repo, tests, fixtures, configs, env-examples, build artifacts, or runtime memory at any point. Production `Signer` impl reachable only under config-gated production profile (which is rejected in dev/test/shadow). | P6B-A + P6B-B. | `live_send` reject + `submit_bundle` fail-closed both preserved. |
| P6B-D | `live_send=true` capability flip in `crates/config` validation. The Phase 4 P4-E DP-E9 reject is relaxed ONLY when: (a) `config.active_profile == Production`, AND (b) `config.signer_kind == HsmKms`. Dev / test / shadow profiles continue to reject `live_send=true` unconditionally. | P6B-A + P6B-B + P6B-C. | `submit_bundle` still returns `Err(SubmitDisabled)` because P6B-E has not landed. G3 stays 0. |
| P6B-E | `eth_sendBundle` runtime path + actual relay submission. `submit_bundle` returns `Ok(SubmissionReceipt)` only after the full 7-step G12 chain INHERITING G13 passes. G3 + G4 relax per-callsite-documented (Section 5); G10 REINFORCED; G11 grows. v0.3 plan Q-A1..Q-A7 + Q-P6B-G recommend splitting into P6B-E1 (local-wiremock-only dress rehearsal binding to 127.0.0.1) + P6B-E2 (live submission with user go/no-go after E1 closes). | All of P6B-A..D. | Live-action gate finally opens here, fully gated. |
| P6B-F | Phase 6b DoD audit + `phase-6b-complete` annotated tag. | All of P6B-A..E. | No new gate; audit + tag only. |

**Reordering is forbidden.** Skipping a batch is forbidden. The user-authorization-per-non-goal model (Phase 6b overview v0.2 prerequisite table item #1) enforces the order at the human level; the per-batch Codex pre-impl review enforces the order at the artifact level.

## Section 4 -- New Phase 6b G-gates

### G12 -- submit_bundle caller pre-check chain (G12 INHERITS G13)

**G12 INHERITS G13.** The runtime `live_send + production-profile + signer_kind` check from G13 MUST hold at runtime before any of the G12 per-call pre-checks below are evaluated. A `submit_bundle` caller that fails G13 must never execute the G12 chain.

Verbatim ripgrep:

```text
rg -n --type rust -B 0 -A 30 'submit_bundle\(' crates/app/src/
```

At P6B-A close: **0 hits** (HSI-6 unchanged baseline).

At P6B-E close: every `submit_bundle(` caller in `crates/app/src/` MUST be preceded within the same function (manual inspection at audit) by ALL seven steps in order:

1. **Kill-switch check.** `kill_switch.is_active()` returning `false`; short-circuit if active. (Reinforces P6-D G10.)
2. **Signer Ok.** `signer.sign_tx(...) -> Ok(SignedTxBytes)` (NOT `Err(SignerError::SignerDisabled)`).
3. **Local-sim Ok.** local-simulator `simulate(...) -> Ok(...)`.
4. **Relay-sim Ok.** relay-simulator `simulate_bundle(...) -> Ok(RelaySimulationOutcome)` with non-error fields.
5. **Comparator Match.** P4-E `compare_result(...) -> Match` (NOT `Mismatch(_)`).
6. **Bundle-byte equality.** signed bundle bytes match the simulated artifact byte-for-byte.
7. **G13 inheritance runtime assertion.** `config.relay.live_send == true && config.active_profile == Production && config.signer_kind == HsmKms` holds at the moment of submission. The static `crates/config` startup validation enforces this at boot; the runtime in-loop assertion is the defensive guard against any code path bypassing the static guard.

Each P6B-E callsite is documented per file:line in Section 5 with BOTH the G12 chain step locations AND the G13 inheritance assertion site.

### G13 -- live_send=true profile scope

Verbatim ripgrep:

```text
rg -n --type rust 'live_send' crates/config/src/
```

At P6B-A close: same as `phase-6a-complete` baseline -- `crates/config/src/lib.rs:282` field declaration, `:295` default `false`, `:426` error variant, `:535` reject guard, surrounding doc comments. **No change at P6B-A close.**

At P6B-D close (PROPOSED logic; LOCKED at P6B-D plan time):

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

The exact gating logic is finalized at P6B-D plan time. G13 locks the contract that ANY relaxation of the `live_send` reject MUST gate on (a) `active_profile == Production` AND (b) `signer_kind == HsmKms`. Dev / test / shadow continue to reject `live_send=true` unconditionally.

### G14 -- eth_sendBundle runtime call documentation

Verbatim ripgrep:

```text
rg -n --type rust 'eth_sendBundle' crates/
```

At P6B-A close: 5 `//!` doc-comment hits (HSI-1 baseline). **No change at P6B-A close.**

At P6B-E close: every non-doc-comment runtime reference in `crates/` MUST be documented per file:line in Section 5 + guarded by the G12 chain. The 5 existing `//!` doc-comment hits stay; their text may be updated to reflect Phase 6b unlock semantics.

## Section 5 -- Per-callsite documentation requirement

This section is **empty at P6B-A close**. P6B-A introduces no new caller / no new runtime reference / no new production sign_tx site.

When P6B-E lands, this section MUST grow to include:

- Every new `submit_bundle(` caller in `crates/app/src/` -- documented as: `file_path:line_number` + the G12 chain step locations (steps 1..7 in the same function within reasonable visual scope) + the G13 inheritance assertion site. Format: one row per caller in a table.
- Every new `eth_sendBundle` runtime reference (non-doc-comment) in `crates/` -- documented as: `file_path:line_number` + the G12 chain step locations.
- Every new production `sign_tx` call site beyond the existing `crates/execution/src/lib.rs:238` -- documented as: `file_path:line_number` + the routing through `&dyn Signer` + the surrounding `BundleConstructor`-private context (or successor structural context defined at P6B-B/E plan time).

This centralization (per Q-A5 in the v0.3 plan) makes a single audit grep sufficient: any new caller / reference / call site NOT documented here is a P6B-F audit blocker.

## Section 6 -- Phase 6b hard forbids

Carried forward from `docs/specs/phase-6a-boundary.md` Section 5 + new Phase 6b-specific forbids:

- No private-key bytes in repo / tests / fixtures / configs / env-examples / build artifacts / runtime memory at any point in Phase 6b.
- No production signer impl outside the P6B-B-documented module inside `crates/signer/`. The HSM/KMS client library SDK is named at P6B-B plan time and Codex-reviewed against G2b.
- No `live_send=true` outside the operator-controlled production profile AND outside `signer_kind == HsmKms` (after P6B-D lands). Dev / test / shadow profiles continue to reject `live_send=true` unconditionally.
- No `eth_sendBundle` runtime path outside the P6B-E-documented sites in Section 5. The 5 `//!` doc-comment hits inherited from Phase 6a may have their text updated; the count of non-doc-comment hits in `crates/` is bounded by Section 5.
- No `submit_bundle` `Ok(_)` return outside the P6B-E-documented sites in Section 5. Every caller satisfies the full 7-step G12 chain INHERITING G13.
- No live-network test enabled by default. `#[ignore]`-gated + env-overlay opt-in + explicit user approval per test.
- No paid live API dependency enabled in CI by default.
- No reordering of P6B-A..F. No skipping. No batching of two live-action gates in a single batch.
- No Phase 6a HSI relaxation outside the per-batch unlock contract in Section 3.
- No `.claude/` / `AGENTS.md` / `fixture_output.txt` / `hook_toast.md` staging.
- No destructive git (force push, reset --hard, branch delete, tag overwrite).
- No asset (WETH/USDC) / venue (UniV2 + UniV3 0.05% + Sushi V2) / V3-fee-tier widening.

## Section 7 -- Reordering ban + fail-closed default

**Reordering ban.** P6B-A..F batches MUST land in order: A -> B -> C -> D -> E -> F. Skipping a batch is forbidden. Reordering (e.g., flipping `live_send=true` before the HSM/KMS signer impl lands) is explicitly forbidden. The user-authorization-per-non-goal model (Phase 6b overview v0.2 prerequisite table item #1) enforces the order at the human level; the per-batch Codex pre-impl review enforces the order at the artifact level.

**Fail-closed default.** A failure in any Phase 6b batch RE-LOCKS all gates that had been relaxed by earlier batches. Specifically:

- If P6B-B fails its DoD audit after landing (e.g., a host-compromise control is discovered to be trivial), the production `Signer` impl MUST be reverted; `DisabledSigner` becomes the only reachable impl again.
- If P6B-C fails (e.g., funded key wiring discovered to leak), the key wiring is operationally severed; the production `Signer` profile becomes unreachable.
- If P6B-D fails (e.g., the `live_send=true` config flip is discovered to leak into a non-production profile), the config-validation reject is restored to its Phase 6a state.
- If P6B-E fails (e.g., a `submit_bundle` caller is discovered to bypass the G12 chain), the new caller is removed; `submit_bundle` continues to return `Err(SubmitDisabled)`.

**Rollback target.** The Phase 6a fail-closed baseline at `phase-6a-complete` (commit `bd0a53c`; tag object `3c9faaf`) is the rollback target if Phase 6b is abandoned at any point. The boundary doc itself is informative; a Phase 6b abandonment may revert this doc + the ADR amendment, or leave them in place as design records, depending on user direction at abandonment time.

## Section 8 -- Cross-references

- `docs/specs/phase-6a-boundary.md` -- the Phase 6a fail-closed safety contract this doc inherits from (HSI-1..HSI-11; G1..G11; Phase 6a hard forbids).
- `docs/specs/execution-safety.md` -- the parent safety policy (`submit_bundle` ban, `live_send` default, funded-key ban, gas-bidding policy, kill switch). The Section "Funded Key / Prod Signer Ban" remains in force at P6B-A close; the operational scope-lift is the cumulative effect of P6B-B + P6B-C + P6B-D + P6B-E.
- `docs/specs/production-signer.md` -- the Phase 6b unlock contract for production signing (HSM/KMS-only key custody, never-in-memory key material, auditability, key rotation + lifecycle, threat model with host-compromise residual + Phase 6b control point requirement).
- `docs/adr/ADR-001-vertical-slice-replay-hooks-gate-policy.md` -- the parent gate-policy ADR; amended in P6B-A to describe the Phase 6a/6b split.
- `docs/superpowers/plans/2026-05-16-phase-6b-overview-execution.md` -- Phase 6b overview v0.2 APPROVED HIGH at commit `49123e9`; P6B-A..F batch breakdown + 6 unlock prerequisites + per-gate fail-closed table.
