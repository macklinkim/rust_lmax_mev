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
| HSI-8 | G5 | `live_send=true` config-validation rejected for ALL profiles | P6B-D | Reject relaxed ONLY for the operator-controlled production profile AND ONLY when paired with `key_backend == HsmKms` and non-empty `audit_key_id`; dev/test/shadow continue to reject unconditionally | RELAXED at P6B-D: `live_send=true` permissible only for `(Profile::Production, KeyBackend::HsmKms, non-empty audit_key_id)`; dev/test/shadow continue to reject unconditionally |
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

### Section 3 reconciliation (added at P6B-C v0.3 close, Codex APPROVED HIGH)

The table row for P6B-B in this section was authored before Codex review surfaced two architectural blockers (`BundleTx` lacks EIP-1559 fee fields needed for unsigned-tx RLP construction; AWS KMS returns DER ECDSA signatures without an Ethereum recovery id, computing `v` requires either a G2b-approved recovery-only crypto-library carve-out or a different signer service). As a result:

- **P6B-B (closed at `df96ac8`) and P6B-C (closed at the implementation commit for this v0.3 plan) are jointly classified as PRE-ACTIVATION signer infrastructure batches.** Neither batch alone, nor both together, gives `ProductionSigner::sign_tx` an `Ok(SignedTxBytes)` return path. At the close of each, `sign_tx` returns `Err(SignerError::NotConfigured)` (legacy `new(...)` path AND `from_aws_kms(...)` path with matching `from`) or `Err(SignerError::AddressMismatch)` (`from_aws_kms(...)` path with mismatched `from`).
- **A future approved sign-activation batch (name + scope locked at that batch's plan time) MUST close before P6B-D or P6B-E may begin.** That batch must extend `BundleTx` with EIP-1559 fee fields, land an RLP encoder, resolve the DER -> `(r, s, v)` recovery question without violating G2b, and flip `sign_tx` to `Ok(_)` under matched-address + successful-sign conditions. The batch requires fresh explicit user re-authorization per the Phase 6b overview prerequisite #1 ("Fresh explicit user authorization per non-goal").
- **P6B-D `live_send=true` capability flip and P6B-E `eth_sendBundle` runtime + actual relay submission BOTH remain locked behind the sign-activation batch.** The chain of locks is: P6B-C (HSM-wired ctor, no sign) -> future sign-activation batch (`sign_tx -> Ok`) -> P6B-D (`live_send=true` relaxation) -> P6B-E (`submit_bundle -> Ok` + `eth_sendBundle` runtime caller).
- Reordering this revised chain is forbidden under the same Section 7 ban that governs the original P6B-A..F sequence.

### Section 3 amendment (added at P6B-CD v0.4 close, per APPROVED plan at `c9451c2`)

P6B-CD lands the sign-activation infrastructure: `BundleTx` EIP-1559 fee fields, an `alloy-rlp = "0.3"` unsigned + signed encoder, and a narrow `k256 = "0.13"` recovery-only carve-out per ADR-001 Amendment 2. `ProductionSigner::sign_tx` may now return `Ok(SignedTxBytes)` when ALL of: signer constructed via `from_aws_kms*`, `tx.from == derived_address`, `max_fee_per_gas >= max_priority_fee_per_gas`, KMS `sign_digest` succeeds, DER parses, low-s normalization completes, trial-recovery against the boot-time public key finds a matching `yParity in {0, 1}`. Every failure path emits an audited + counted outcome label exactly once (G15 contract preserved).

**P6B-D `live_send=true` and P6B-E `submit_bundle Ok` + `eth_sendBundle` runtime remain locked behind P6B-CD impl close + separate user re-authorization per non-goal.** The `submit_bundle` adapters continue to return `Err(KillSwitchActive)` or `Err(SubmitDisabled)`. G3 + G4 stay at 0. G11 production runtime sign_tx call site count stays at 1 (the existing `#[cfg(test)] pub(crate) async fn invoke_signer_for_test` hook in `crates/execution/src/lib.rs`); the runtime caller path is P6B-E scope.

### Section 3 amendment (added at P6B-D close, per APPROVED plan v0.4 at `dc5ca55`)

P6B-D effectuates the boundary doc Section 4 G13 contract sketched at P6B-A. `Config::validate()` now permits `live_send=true` for the SINGLE legal combo `(Profile::Production, KeyBackend::HsmKms, non-empty audit_key_id)`; every other combination rejects. Two NEW `ConfigError` variants land:

- `LiveSendRequiresProductionProfile` -- fires when `live_send=true` is set with any non-Production profile (Dev/Test/Shadow).
- `LiveSendRequiresHsmKms` -- fires when `live_send=true` is set with `Profile::Production` but `key_backend != HsmKms`.

The two new branches evaluate BEFORE the P6B-B reject chain (live-send-first ordering) so the operator sees the live-send-specific error message rather than the generic P6B-B Profile/KeyBackend guard. The P6B-B reject chain (`ProductionProfileRequiresHsmKms`, `HsmKmsRequiresProductionProfile`, `HsmKmsRequiresNonEmptyAuditKeyId`) continues to fire for the orthogonal `live_send=false` axis bit-for-bit unchanged.

**P6B-E (`submit_bundle Ok` + `eth_sendBundle` runtime + actual relay submission) and P6B-F (Phase 6b DoD audit + `phase-6b-complete` annotated tag) REMAIN LOCKED.** The runtime safety chain at P6B-D close: `submit_bundle` adapters still return `Err(KillSwitchActive)` or `Err(SubmitDisabled)`; G3 + G4 stay at 0; G11 production runtime sign_tx call site count stays at 1 (test-only `invoke_signer_for_test` hook); G14 stays at 5 doc-comment hits; NO new app-side read of `config.relay.live_send` anywhere in `crates/app/src/`. The validation flip is necessary-but-not-sufficient for live submission; P6B-E owns the runtime G12 step 7 / G13 inheritance assertion site.

### Section 3 amendment (added at P6B-E1 close, per APPROVED plan v0.1 at `4ca6abd`)

P6B-E1 ships the **local-wiremock-only dress rehearsal** of `submit_bundle -> Ok(SubmissionReceipt)`. After E1 close the workspace can submit a real `eth_sendBundle` JSON-RPC POST to an in-process mock relay bound to `127.0.0.1` and observe the full G12 7-step chain INHERITING G13 fire under the kill-switch + comparator-pass + matched-bundle preconditions. **No non-localhost endpoint can be submitted to by ANY code path** -- enforced by two redundant fail-closed gates (`ConfigError::LiveSendRequiresLocalhostEndpoint` at validate-time + `BundleRelayError::SubmitDisabledNonLocalhost` at adapter runtime).

Deliverables landed (D-E1-1..D-E1-9 per the v0.1 plan):

- D-E1-1: `ConfigError::LiveSendRequiresLocalhostEndpoint` + validate-body check after the P6B-D live-send-first branches.
- D-E1-2: NEW `BundleRelayError::SubmitDisabledNonLocalhost` + `SubmitHttpFailed` variants.
- D-E1-3: `FlashbotsRelay::submit_bundle` Ok-path rewrite under localhost gate. **Bloxroute UNCHANGED at E1** (single-adapter scope per v0.1 plan lock (I); Bloxroute remains `Err(SubmitDisabled)`).
- D-E1-4: NEW `crates/relay-clients/src/send_bundle.rs` JSON-RPC body builder + localhost-URL helper. Sibling to the P4-E `call_bundle.rs` `eth_callBundle` helper.
- D-E1-5: `SimulationOutcomeWithFingerprint.signed_bundle: Option<SignedBundle>` extension + NEW `SubmissionAttempt` struct in `crates/bundle-relay`.
- D-E1-6: NEW `submission_driver` task in `wire_phase4`, parallel to `comparator_driver`. Subscribes to `submission_tx` and on each `SubmissionAttempt` runs the G12 7-step pre-check chain INHERITING G13. One `Arc<dyn BundleRelay>` parameter (G4 grows 0 -> 1); one `.submit_bundle(` method call site (G3 grows 0 -> 1).
- D-E1-7: NEW `FileJournal<SubmissionReceipt>` opened from `config.journal.submission_journal_path`. Append-and-flush BEFORE acknowledging success in the loop (DP-E8 v0.4 pattern).
- D-E1-8: THIS Section 3 amendment + Section 5 per-callsite entries below.
- D-E1-9: 7 targeted tests (D-T-E1-1..D-T-E1-7) -- 2 config validate (localhost reject + accept), 1 relay-clients adapter runtime non-localhost reject (`rc_f_4` rewritten), 1 relay-clients localhost wiremock happy path (`d_t_e1_5_submit_bundle_ok_on_local_wiremock`), 2 `submission_driver` integration tests in `crates/app/tests/submission_driver_e1.rs` (happy path + G13 inheritance fail skip). Plus 2 existing `submit_disabled_1` / `submit_disabled_5` updated to reflect the new `SubmitDisabledNonLocalhost` shape.

**v0.1 plan lock (D) SIMPLIFICATION**: G12 step 6 ("bundle-byte equality") is approximated at E1 as `signed_bundle.signed_txs.iter().all(|t| t.len() >= 64) && !signed_txs.is_empty()`. True keccak-against-relay-echo bundle-byte equality is **deferred to P6B-E2** when production-relay echo responses are observable.

**P6B-E2 (non-localhost endpoint adapter flip + multi-adapter parity + true bundle-byte equality) and P6B-F (Phase 6b DoD audit + `phase-6b-complete` annotated tag) REMAIN LOCKED.** Runtime safety chain at P6B-E1 close: relay adapters return `Ok(SubmissionReceipt)` ONLY when the endpoint host is in `{"127.0.0.1", "localhost", "::1"}` AND the kill-switch is inactive AND the HTTP POST succeeds; every other combination returns `Err(...)` per the documented PRECEDENCE (`KillSwitchActive` > `SubmitDisabledNonLocalhost` > `SubmitHttpFailed`). The `comparator_driver` -> `submission_tx` -> `submission_driver` path is structurally compiled but only fires in tests because `SimulationOutcomeWithFingerprint.signed_bundle` is ALWAYS `None` in production runtime (G11 = 1 carried forward from P6B-CD; no production signer call site upstream).

### Section 3 amendment (added at P6B-E2 close, per APPROVED plan v0.1)

P6B-E2 ships the **live submission unlock**: non-localhost Ok-path under operator opt-in + Bloxroute parity + true keccak bundle-byte equality + production-runtime `sign_tx` call site. The runtime fail-closed posture is preserved at the default: `wire_phase4` continues to inject `DisabledSigner`, so the new production `sign_tx` call site returns `Err(SignerError::SignerDisabled)` on every iteration -> `simulator_driver` skips -> no `SubmissionAttempt` is broadcast -> no `submit_bundle` invocation. The unlock is reachable only by an operator who (a) wires a real `ProductionSigner` AND (b) flips `RelayConfig::allow_non_localhost_endpoint = true` (legal only paired with `live_send=true` + Production + HsmKms).

Deliverables landed (D-E2-1..D-E2-9 per the v0.1 plan):

- D-E2-1: `RelayConfig::allow_non_localhost_endpoint: bool` (default `false`; serde default). `Config::validate()` keeps the `LiveSendRequiresLocalhostEndpoint` reject UNLESS the flag is `true`. D-T-E2-1a + D-T-E2-1b cover both branches.
- D-E2-2: NEW `BundleRelayError::BundleHashMismatch` (payload-free; Display literal `"bundle hash mismatch: relay-returned hash != local keccak"`). Synthesized by `submission_driver` (not the adapter) after a successful HTTP parse + keccak compare.
- D-E2-3: `SubmissionReceipt::local_bundle_hash: String` (additive). Populated by `submission_driver` on both match (informational) and mismatch (audit). Sample fixtures + the `br_2` rkyv+serde round-trip test updated.
- D-E2-4: `FlashbotsRelay::new(cfg, ks, allow_non_localhost)` + `BloxrouteRelay::new(cfg, ks, allow_non_localhost)` (BREAKING ctor signature change). Bloxroute `submit_bundle` flipped from unconditional `Err(SubmitDisabled)` to the same PRECEDENCE chain as Flashbots via the shared `crates/relay-clients/src/send_bundle.rs` helper. `RC-B-4` + `bloxroute_kill_switch_inactive_baseline_*` + `submit_disabled_2` updated to expect `SubmitDisabledNonLocalhost`.
- D-E2-5: `submission_driver` G12 step 6 keccak compare. Computes `keccak256(concat(signed_bundle.signed_txs))` lowercase-hex; compares to the relay-returned `bundle_hash` (host-tolerant `0x` normalization). On mismatch: warn-log naming `BundleRelayError::BundleHashMismatch` + journal mismatch record. On match: existing Ok-path receipt journal append with `local_bundle_hash` populated for audit.
- D-E2-6: NEW `BundleConstructor::sign_for_outcome(&self, outcome) -> Result<SignedTxBytes, SignerError>` in `crates/execution/src/lib.rs`. Production runtime `sign_tx` call site (G11 grows 1 -> 2). `simulator_driver` invokes this after `simulate_with_fingerprint` Ok; `Err(_)` -> iteration-skip + WARN (unsigned bundles never flow downstream past the simulator boundary).
- D-E2-7: Audit-note-1 visibility narrowing -- `DEFAULT_FLASHBOTS_ENDPOINT` + `DEFAULT_BLOXROUTE_ENDPOINT` flipped to `pub(crate)` + `#[doc(hidden)]`. Crate-public surface no longer leaks the literal URLs.
- D-E2-8: THIS Section 3 amendment + Section 4 G14 count update + Section 5 per-callsite expansion below.
- D-E2-9: 5 targeted tests (D-T-E2-1a/1b in `crates/config/src/lib.rs`; D-T-E2-2 in `crates/relay-clients/tests/bloxroute.rs`; D-T-E2-3 + D-T-E2-4 + D-T-E2-5 in `crates/app/tests/submission_driver_e2.rs`). Plus carry-forward updates: `rc_f_4` + `rc_b_4_*` + `submit_disabled_*` ctor + variant updates.

**P6B-F (Phase 6b DoD audit + `phase-6b-complete` annotated tag) REMAINS LOCKED** behind P6B-E2 close + a separate explicit user re-authorization per the Phase 6b overview's per-non-goal prerequisite chain.

At P6B-E2 close (default config, no operator overrides):

- `allow_non_localhost_endpoint = false` -> `LiveSendRequiresLocalhostEndpoint` reject preserved.
- `key_backend = Disabled` -> `wire_phase4` injects `DisabledSigner` -> `sign_for_outcome` returns `Err(SignerDisabled)` -> `simulator_driver` skips -> no envelope flows downstream -> no submission attempt.
- `submit_bundle` runtime path is the only Ok-source for `SubmissionReceipt`; the keccak check at the driver guarantees the journaled record is either a verified match or an audited mismatch.

**P6B-F note-1 (Bloxroute operator wiring contract)**: `BloxrouteRelay::submit_bundle` Ok-path requires the adapter to have been constructed with a non-empty `api_key`. When the operator omits `api_key` from `RelayEndpointConfig`, the adapter ctor still succeeds (the inner `http: Option<Client>` stays `None`) but every `submit_bundle` call returns `Err(SubmitHttpFailed)` -- a third fail-closed layer beneath the kill-switch + localhost gates. Operators wiring a real HSM/KMS-backed `ProductionSigner` for Bloxroute MUST also configure `api_key` as an environment secret (per the Phase 6b operator-runbook conventions referenced by `production-signer.md` Section 2.4); without it, the chain stops at `SubmitHttpFailed` and no live submission is possible regardless of the other gates.

**P6B-F note-4 (placeholder BundleTx as a 2-step safety chain)**: `BundleConstructor::sign_for_outcome` constructs a placeholder `BundleTx::new(Address::ZERO, Address::ZERO, ...)` and feeds it to `signer.sign_tx`. Together with the default `DisabledSigner` injection in `wire_phase4`, this gives a 2-step safety chain:
- **Step 1** (default fail-closed): `DisabledSigner::sign_tx` returns `Err(SignerError::SignerDisabled)` unconditionally -> `simulator_driver` iteration-skip.
- **Step 2** (defense-in-depth when operator wires a real signer): `ProductionSigner::sign_tx` rejects with `Err(SignerError::AddressMismatch)` because `tx.from == Address::ZERO != derived_address` -- the audited per-attempt counter records `address_mismatch`. No `Ok(SignedTxBytes)` can flow until the placeholder is replaced.

A future batch that introduces a real `BundleTx::from` (real builder coinbase + populated calldata) removes Step 2; that batch MUST simultaneously add a different control point (e.g., transaction-shape validator, explicit operator confirmation, or a per-attempt rate-limit) so the post-replacement chain stays as defensive as the pre-replacement chain.

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

PROPOSED at P6B-A authoring time (operative field name in `crates/config/src/lib.rs::RelayConfig` is `key_backend`, NOT `signer_kind`; the pseudo-field below is the boundary doc's earlier draft wording, preserved for historical reference):

```text
// PROPOSED P6B-A boundary-contract sketch (operative names corrected in
// the ENFORCED subsection below):
// if self.relay.live_send && self.active_profile != Profile::Production {
//     return Err(...);
// }
// if self.relay.live_send && self.active_profile == Profile::Production
//    && self.signer_kind != SignerKind::HsmKms {
//     return Err(...);  // production-profile live_send requires HSM/KMS signer
// }
```

The exact gating logic was finalized at P6B-D plan time + landed at P6B-D close (see the ENFORCED subsection below). G13 locks the contract that ANY relaxation of the `live_send` reject MUST gate on (a) `active_profile == Production` AND (b) HSM/KMS-backed signer.

**At P6B-D close (ENFORCED at commit `dc5ca55`-derived impl):**

`Config::validate()` in `crates/config/src/lib.rs` evaluates the live-send-first reject pair BEFORE the P6B-B reject chain:

```rust
// P6B-D D-D2 (replaces the P4-E absolute LiveSendForbidden reject):
if self.relay.live_send && self.active_profile != Profile::Production {
    return Err(ConfigError::LiveSendRequiresProductionProfile);
}
if self.relay.live_send
    && self.active_profile == Profile::Production
    && self.relay.key_backend != KeyBackend::HsmKms
{
    return Err(ConfigError::LiveSendRequiresHsmKms);
}
// ... P6B-B reject chain unchanged below ...
```

Note the operative field name is `key_backend` (NOT the boundary doc's pre-P6B-D pseudo-field `signer_kind`). The single legal `live_send=true` combo is `(Profile::Production, KeyBackend::HsmKms, non-empty audit_key_id)`; the `audit_key_id` non-empty constraint is enforced by the P6B-B `HsmKmsRequiresNonEmptyAuditKeyId` reject that fires AFTER the two new live-send-first branches (the Production+HsmKms+empty+live_send=true row falls through both new branches because both predicates are false, and hits the P6B-B reject). Dev / test / shadow continue to reject `live_send=true` unconditionally regardless of `key_backend`.

`ConfigError::LiveSendForbidden` was REMOVED at P6B-D close as a deliberate workspace-local source-level break; the post-rename `rg -n 'LiveSendForbidden' crates/` returns 0 hits.

### G14 -- eth_sendBundle runtime call documentation

Verbatim ripgrep:

```text
rg -n --type rust 'eth_sendBundle' crates/
```

At P6B-A close: 5 `//!` doc-comment hits (HSI-1 baseline). **No change at P6B-A close.**

At P6B-E close: every non-doc-comment runtime reference in `crates/` MUST be documented per file:line in Section 5 + guarded by the G12 chain. The 5 existing `//!` doc-comment hits stay; their text may be updated to reflect Phase 6b unlock semantics.

### G15 -- production-signer audit-surface contract (added at P6B-C v0.3 close)

Per `docs/specs/production-signer.md` Section 2.4 + Section 2.5 candidate #4 ("Operator-visible signing audit log"). At P6B-C close the workspace ships the audit-log SURFACE -- the operator's dashboard + Alertmanager stack scrape it. The four required surfaces below MUST stay in place across any later Phase 6b batch:

1. **Boot-time audit-safe identifier.** `ProductionSigner::from_aws_kms(...)` emits exactly one `tracing::info!` event at construction with `target = "production_signer_boot"`, `event = "production_signer_initialized"`, and structured fields `audit_key_id` + `derived_address`. No field name contains a key-material-shaped substring (`private`, `secret`, `priv`, `seed`, `funded`). Section 2.4 satisfaction.
2. **Per-attempt counter.** Every `ProductionSigner::sign_tx` call increments `production_signer_audit_attempts_total{outcome}` exactly once. Outcomes at P6B-CD close (7-label set): `not_configured`, `address_mismatch`, `invalid_bundle_tx`, `kms_sign_failed`, `invalid_signature_bytes`, `signature_recovery_failed`, `ok`. The metric name + label key stay stable across later batches; the per-batch outcome-label-set expansion paragraph below tracks any future additions.
3. **Threshold gauges.** `ProductionSigner::from_aws_kms*` emits the two gauges `production_signer_audit_alert_threshold_max_attempts_per_minute` + `production_signer_audit_alert_threshold_max_failed_per_minute` carrying the operator-configured values from `[relay.signing_audit_alert]`. Gauge value `0` = operator left the threshold disabled.
4. **Sample Alertmanager rule.** `config/examples/signing-audit-alert.yaml` references the threshold gauges in PromQL alert expressions (NOT hardcoded scalars). Operators copy + adapt to their stack. The sample is illustrative; the workspace does not ship dashboard rendering.

Verbatim ripgrep + presence gates at P6B-C close:

```text
rg -n 'production_signer_boot' crates/signer/src/                         # >= 1 hit
rg -n 'production_signer_audit_attempts_total' crates/signer/src/         # >= 1 hit
rg -n 'production_signer_audit_alert_threshold_max_attempts_per_minute' crates/signer/src/   # >= 1 hit
rg -n 'production_signer_audit_alert_threshold_max_failed_per_minute' crates/signer/src/     # >= 1 hit
rg -n 'signing_audit_alert' crates/config/src/                            # >= 1 hit
ls config/examples/signing-audit-alert.yaml                               # file exists
```

Any later Phase 6b batch that removes any of these four surfaces re-opens Section 2.5 candidate #4 and is a P6B-F audit blocker.

**P6B-CD v0.4 outcome-label expansion**: the per-attempt counter's outcome-label set grows by 5 labels at P6B-CD close: `ok`, `kms_sign_failed`, `invalid_bundle_tx`, `invalid_signature_bytes`, `signature_recovery_failed`. The full 7-label set at P6B-CD close is `{not_configured, address_mismatch, invalid_bundle_tx, kms_sign_failed, invalid_signature_bytes, signature_recovery_failed, ok}`. The sample Alertmanager YAML's `SigningFailureRateHigh` matcher narrows to the 5-failure positive enumeration `{outcome=~"address_mismatch|invalid_bundle_tx|kms_sign_failed|invalid_signature_bytes|signature_recovery_failed"}` (excludes `ok` and `not_configured`); `SigningAttemptRateHigh` continues to scrape the full 7-label set as the operator-attack-surface signal.

### G2f -- narrow k256 surface allow-list (added at P6B-CD v0.4 close)

Permitted k256 symbols inside `crates/signer/src/recovery.rs`: `k256::ecdsa::{Signature, RecoveryId, VerifyingKey}` and `k256::PublicKey`. Forbidden absolute (in ANY `crates/*.rs` file): `k256::SecretKey`, `k256::ecdsa::SigningKey`, `k256::Scalar`, `k256::FieldElement`, `k256::ProjectivePoint`, `k256::AffinePoint`, `k256::elliptic_curve::*`.

Verbatim ripgrep gate (must return 0 hits absolute):

```text
rg -n -e 'k256::(SecretKey|ecdsa::SigningKey|Scalar|FieldElement|ProjectivePoint|AffinePoint|elliptic_curve)' crates/ --glob '*.rs'
```

Positive grep (must return >= 1 hit):

```text
rg -n 'use k256::' crates/signer/src/recovery.rs
```

### G2g -- absolute ban on signing-key constructors / test-key bytes (added at P6B-CD v0.4 close, R-7)

Verbatim ripgrep gate (must return 0 hits absolute, including `#[cfg(test)]` files):

```text
rg -n -e 'test_key|TEST_KEY|TEST_PRIV|TEST_PRIVATE|SecretKey|SigningKey|from_bytes_be|from_slice_be|::random\(|::generate\(' crates/ --glob '*.rs'
```

Tests that need a known-good ECDSA recovery vector consume ONLY non-secret material (preimage, digest, signed-tx bytes, `r`, `s`, `y_parity`, precomputed-off-tree SEC1 public-key bytes, DER signature bytes); private-key derivation -- even from a published source -- happens OFF-TREE and only the non-secret outputs are pasted into the repo. Relaxing G2g requires its own ADR-001 amendment.

## Section 5 -- Per-callsite documentation requirement

### P6B-E1 close: 1 new `submit_bundle(` caller + 1 new `eth_sendBundle` runtime reference

Per the v0.1 plan lock (J), exactly ONE new caller per gate is documented here at P6B-E1 close:

| Gate | File:line | Callsite |
|---|---|---|
| G3 / G4 (new `Arc<dyn BundleRelay>` + `.submit_bundle(` call) | `crates/app/src/lib.rs` -- inside the body of `pub async fn submission_driver(...)` -- the line `match relay.submit_bundle(&attempt.signed_bundle).await { ... }`. The `Arc<dyn BundleRelay>` is the function parameter `relay: Option<Arc<dyn BundleRelay>>` (None branch yields a structurally inert task). | G12 7-step chain INHERITING G13 verified in the same loop body BEFORE the `.submit_bundle(` invocation: (1) `kill_switch.is_active()` short-circuit; (2) structural (presence of `attempt.signed_bundle` proves the upstream signer Ok); (3)-(5) structural (comparator_driver only broadcasts SubmissionAttempt on Match); (6) `signed_bundle.signed_txs` non-empty + each tx >= 64 bytes (v0.1 D simplification); (7) `gate.permits_submission()` synchronous assertion `live_send && Production && HsmKms` against the boot-time `SubmissionGate` snapshot. |
| G14 (new `eth_sendBundle` runtime reference) | `crates/relay-clients/src/send_bundle.rs` -- inside the body of `pub(crate) async fn submit_eth_send_bundle(...)` -- the JSON-RPC body literal `method: "eth_sendBundle"`. | This runtime reference is reachable ONLY through the chain `submission_driver -> FlashbotsRelay::submit_bundle -> send_bundle::submit_eth_send_bundle`. The 2 gates above (G3 + G4 documented chain) cover the upstream preconditions. The adapter itself also re-checks kill-switch + localhost-only before invoking this function (adapter-runtime defense-in-depth per `BundleRelayError::SubmitDisabledNonLocalhost`). |



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
