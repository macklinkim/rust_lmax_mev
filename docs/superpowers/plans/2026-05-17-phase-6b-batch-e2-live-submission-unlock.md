# Phase 6b Batch E2 -- Live submission unlock (impl plan)

**Date:** 2026-05-17 KST
**Status:** Draft v0.1. ASCII-only. Pairs with the implementation turn re-authorized by the user on the same day. NO commit / push from this plan.
**Predecessor:** HEAD `78380ca` (P6B-E1 close). `.coordination/p6b-e1-audit.md` APPROVE-WITH-NOTES.
**Successor:** P6B-F (LOCKED until P6B-E2 user re-authorization closes).

## Scope (v0.1 LOCKED)

1. **Operator-controlled non-localhost adapter flip.** NEW `RelayConfig::allow_non_localhost_endpoint: bool` (default `false`; serde default). `Config::validate()` keeps the `LiveSendRequiresLocalhostEndpoint` reject UNLESS the flag is `true`. Adapter runtime check `SubmitDisabledNonLocalhost` also gated by the same flag (threaded into the adapter ctors as a third arg). Both gates remain explicit fail-closed without operator opt-in.
2. **Bloxroute submit_bundle Ok-path parity.** `BloxrouteRelay::submit_bundle` flips from unconditional `Err(SubmitDisabled)` to the same PRECEDENCE chain as Flashbots: `KillSwitchActive > SubmitDisabledNonLocalhost > SubmitHttpFailed | Ok(SubmissionReceipt)`. Both adapters share `crates/relay-clients/src/send_bundle.rs`.
3. **Bundle-byte equality (G12 step 6 lift).** `submission_driver` computes `keccak256(concat(signed_bundle.signed_txs))` and compares to the relay's returned `bundle_hash` (lowercase 0x-prefixed 32-byte hex). On mismatch -- NEW `BundleRelayError::BundleHashMismatch` synthesized -- the driver appends a mismatch `SubmissionReceipt` (with NEW `local_bundle_hash` field populated) to the submission journal + WARN logs. On match the existing Ok-path receipt is appended unchanged.
4. **Production-runtime `sign_tx` call site.** NEW `pub async fn BundleConstructor::sign_for_outcome(&self, outcome) -> Result<SignedTxBytes, SignerError>` in `crates/execution/src/lib.rs`. Internally calls `self.signer.sign_tx(&tx).await` where `tx` is a placeholder `BundleTx` derived from the outcome. `simulator_driver` in `crates/app/src/lib.rs` invokes this method after a successful `simulate_with_fingerprint`; Err -> iteration skip + WARN log. On Ok, `SimulationOutcomeWithFingerprint.signed_bundle` is populated. `wire_phase4` keeps injecting `DisabledSigner` -> default runtime continues to fail-closed (every iteration skips). G11 grep gate flips 1 -> 2 hits in `crates/execution/src/lib.rs`.

## Audit-note follow-ups

- **Note-1:** `DEFAULT_FLASHBOTS_ENDPOINT` + `DEFAULT_BLOXROUTE_ENDPOINT` -> `pub(crate)` + `#[doc(hidden)]`. Removes them from the public crate surface (audit-surface reduction).
- **Note-2:** `docs/specs/phase-6b-boundary.md` Section 4 G14 + Section 5 counts re-stated post-E2 (G14 doc-comments + 2 runtime; per-callsite chain G3/G4/G14 expanded).

## Locks A..L

- **A. allow_non_localhost_endpoint** -- LOCKED on `RelayConfig`. Default `false`. Threaded into Flashbots + Bloxroute adapter ctors as a third positional arg. Boundary-doc Section 5 documents the single read site at adapter ctor time.
- **B. Adapter ctor signature change** -- LOCKED breaking: `FlashbotsRelay::new(cfg, kill_switch, allow_non_localhost)` + `BloxrouteRelay::new(cfg, kill_switch, allow_non_localhost)`. Existing call sites (3 in `crates/app`, 8 in `crates/relay-clients` tests) updated to pass `false` except where the localhost wiremock test needs `false` to keep the localhost path active (Flashbots / Bloxroute happy-path tests use real localhost endpoints, so the flag stays `false` and the localhost check passes naturally).
- **C. PRECEDENCE order** -- LOCKED. `KillSwitchActive > SubmitDisabledNonLocalhost (when !allow_non_localhost) > SubmitHttpFailed | Ok(_)`. When `allow_non_localhost == true`, the non-localhost gate is bypassed; the kill-switch gate stays mandatory.
- **D. Bloxroute submit_bundle helper share** -- LOCKED. `submit_eth_send_bundle` in `crates/relay-clients/src/send_bundle.rs` is the shared helper; Bloxroute passes `self.http.as_ref().ok_or(BundleRelayError::SubmitHttpFailed)?` when api_key was absent at ctor time.
- **E. New BundleRelayError variant** -- LOCKED. `BundleHashMismatch` (payload-free; Display literal `"bundle hash mismatch: relay-returned hash != local keccak"`).
- **F. SubmissionReceipt additive field** -- LOCKED. NEW field `pub local_bundle_hash: String`. Populated on every Ok-path (verified-match) and on every mismatch-journaled record. Tests `br_2_signed_bundle_and_receipt_round_trip` + `sample_receipt` extended to populate the new field. Pre-P6B-E1 rkyv blobs do NOT exist (new journal added at E1); no backward-compat concern.
- **G. keccak helper** -- LOCKED via `alloy-primitives::keccak256`. Already in `crates/bundle-relay`'s dep set transitively; new direct dep added to `crates/app` if not already. The helper computes `keccak256(concat(signed_txs.iter().flatten()))` and renders as lowercase `0x<64hex>`.
- **H. Production sign_tx call site location** -- LOCKED inside `crates/execution::BundleConstructor::sign_for_outcome` (new method). G11 ripgrep `sign_tx` in `crates/execution/src/lib.rs` MUST count exactly 2 hits (existing `invoke_signer_for_test` line 238 + new `sign_for_outcome`).
- **I. simulator_driver wiring** -- LOCKED. New param `bundle_constructor: Arc<BundleConstructor>`. After `simulate_with_fingerprint` returns Ok, the driver calls `bundle_constructor.sign_for_outcome(&outcome).await`. On Err -> `tracing::warn!` + `continue`. On Ok -> emit `SimulationOutcomeWithFingerprint { signed_bundle: Some(...), ... }`.
- **J. wire_phase4 unchanged signer injection** -- LOCKED. `Arc::new(DisabledSigner::default())` continues to be the signer passed in. P6B-E2 enables the CALL site; the operator must subsequently land a real `ProductionSigner` to flip from skip-on-disabled to Ok.
- **K. G15 audit-once-per-attempt** -- LOCKED. Only `BundleConstructor::sign_for_outcome` reaches `signer.sign_tx`; `submission_driver` does NOT re-invoke. Each Ok / Err outcome inside `ProductionSigner::sign_tx` increments the counter exactly once (carried forward from P6B-CD).
- **L. No live network anywhere** -- LOCKED. All HTTP I/O in tests is bound to `127.0.0.1:<random>` via `wiremock`. NO live mainnet, NO live KMS. Defense-in-depth localhost gate stays effective for tests; the `allow_non_localhost` flag is only flipped to `true` in the D-T-E2-1 negative-config test (no HTTP I/O is performed because the test asserts only `Config::validate()` returns Ok).

## Deliverables (D-E2-1..D-E2-9)

| ID | Scope |
|---|---|
| D-E2-1 | `RelayConfig::allow_non_localhost_endpoint: bool` + `validate()` gating `LiveSendRequiresLocalhostEndpoint`. |
| D-E2-2 | `BundleRelayError::BundleHashMismatch` (payload-free). |
| D-E2-3 | `SubmissionReceipt::local_bundle_hash: String` (additive). Sample fixtures + `br_2` updated. |
| D-E2-4 | `FlashbotsRelay::new(cfg, ks, allow_non_localhost)` + `BloxrouteRelay::new(cfg, ks, allow_non_localhost)` + body of Bloxroute `submit_bundle` flipped to Ok-path via shared `submit_eth_send_bundle`. |
| D-E2-5 | `submission_driver` computes keccak + compares; mismatch synthesizes `BundleRelayError::BundleHashMismatch`, appends mismatch receipt, warn-logs. |
| D-E2-6 | `crates/execution::BundleConstructor::sign_for_outcome(&self, outcome) -> Result<SignedTxBytes, SignerError>` + `simulator_driver` wiring. |
| D-E2-7 | Audit-note-1 visibility narrowing: `DEFAULT_FLASHBOTS_ENDPOINT` + `DEFAULT_BLOXROUTE_ENDPOINT` flip to `pub(crate)` + `#[doc(hidden)]`. |
| D-E2-8 | Boundary doc Section 3 P6B-E2 amendment + Section 4 G14 count update + Section 5 per-callsite entries for the 2 new sign_tx site + 2 new runtime references (the Bloxroute send + the keccak compare). |
| D-E2-9 | Tests D-T-E2-1..D-T-E2-5 + carry-forward updates. |

## Tests (D-T-E2-1..D-T-E2-5)

| ID | File | Asserts |
|---|---|---|
| D-T-E2-1 | `crates/config/src/lib.rs` `#[cfg(test)]` | Production + HsmKms + live_send=true + `allow_non_localhost_endpoint=true` + non-localhost endpoint -> `Ok(_)`. Same shape with `allow_non_localhost_endpoint=false` -> `Err(LiveSendRequiresLocalhostEndpoint)` (regression of E1 behavior). |
| D-T-E2-2 | `crates/relay-clients/tests/bloxroute.rs` | Wiremock `127.0.0.1:<random>` with api_key configured; ctor with `allow_non_localhost=false` (localhost = ok regardless of flag); body shape verified via `serde_json::Value` equality on the `eth_sendBundle` envelope; `submit_bundle` returns `Ok(SubmissionReceipt)`. |
| D-T-E2-3 | `crates/app/tests/submission_driver_e2.rs` (new) | `submission_driver` against a localhost wiremock that returns a `bundleHash` distinct from `keccak256(concat(signed_txs))`. Asserts: (a) wiremock observed exactly 1 POST; (b) journal contains exactly 1 entry whose `local_bundle_hash != bundle_hash`. Implicit: the BundleHashMismatch error variant is referenced by the warn-log path. |
| D-T-E2-4 | `crates/app/tests/submission_driver_e2.rs` (new) | `simulator_driver` with a `DisabledSigner`-wired `BundleConstructor`: `sign_for_outcome` returns `Err(SignerDisabled)`; the simulator_driver path emits NO `SimulationOutcomeWithFingerprint` -> no comparator activity -> no submission. Asserts the wiremock observes 0 POSTs. |
| D-T-E2-5 | `crates/app/tests/submission_driver_e2.rs` (new) | Compile-time / source grep: `rg -c 'sign_tx' crates/execution/src/lib.rs` returns exactly 2. Implemented as `include_str!` + `.matches("sign_tx").count() == 2` assertion. |

Pre-P6B-E2 baseline (full workspace): 245-ish. Target: +5 net new + ~4 carry-forward updates passing.

## Hard forbids at P6B-E2 close

- NO real external relay URL in repo / tests / fixtures / configs / env-examples beyond the pre-existing P4-E const literals (which P6B-E2 demotes to `pub(crate) + #[doc(hidden)]`).
- NO live mainnet / live KMS / live network call by ANY code path. All HTTP I/O is wiremock-bound to `127.0.0.1:<random>` in tests.
- NO new `#[ignore]` test. Carry-forward `g_state_live_smoke_env_contract` stays the only ignored test (P2-C inheritance).
- NO new funded key / `SecretKey` / `SigningKey` / `test_key` / 64-hex private-key literal anywhere (G2g 0-hit baseline preserved).
- NO config example enabling `live_send=true`. `config/base|dev|test|examples` untouched on the live-send axis (operators add their own overlay).
- NO `phase-6b-complete` annotated tag / P6B-F work.
- NO touch to `crates/signer/`, `crates/state/`, `crates/opportunity/`, `crates/risk/`, `crates/state-fetcher/`, `crates/relay-sim/`, `crates/simulator/`. The only signer-side surface used is the existing `Signer` trait + `BundleTx` + `SignedTxBytes`.
- NO change to `wire_phase4`'s `signer: Arc<dyn Signer>` injection -- continues to receive `Arc::new(DisabledSigner::default())` from the test/prod entry point.
- NO ADR amendment. NO change to `docs/specs/production-signer.md` / `execution-safety.md` / `phase-6a-boundary.md`. Boundary doc additive only.
- NO `.coordination/` / `AGENTS.md` / `fixture_output.txt` / `hook_toast.md` staging.

## Open questions (Q-E2-N)

1. **Q-E2-1**: Should the `allow_non_localhost_endpoint` flag live on `RelayConfig` or per-endpoint on `RelayEndpointConfig`? v0.1 LOCKS `RelayConfig` (global; operator opts in once). Per-endpoint refinement deferred to a future batch.
2. **Q-E2-2**: Should the mismatch journal be a separate file or a flagged variant in the existing submission journal? v0.1 LOCKS additive `local_bundle_hash` field on `SubmissionReceipt` (single journal; mismatch records distinguished by `local_bundle_hash != bundle_hash`).
3. **Q-E2-3**: Should `BundleHashMismatch` carry the two hashes as payload? v0.1 LOCKS payload-free per workspace convention; the journal record IS the audit surface.
4. **Q-E2-4**: Where does the keccak compare live -- inside the adapter or inside `submission_driver`? v0.1 LOCKS inside `submission_driver` (single-source-of-truth audit + journal append flow stays in the same place).
5. **Q-E2-5**: Should `sign_for_outcome` accept `&BundleCandidate` instead of `&SimulationOutcome`? v0.1 LOCKS `&SimulationOutcome` (avoids a redundant `BundleCandidate` allocation in simulator_driver; the candidate is computed downstream by `execution_driver`).
6. **Q-E2-6**: Treatment of `Err(SignerDisabled)` -- skip vs emit-with-None? v0.1 LOCKS skip iteration + warn (user instruction "Err 분기는 iteration skip + warn log"). The default fail-closed posture is preserved because skipping yields no submission attempt. Tests that previously relied on `simulator_driver` emitting an envelope under DisabledSigner are updated to drive `comparator_driver` directly.
7. **Q-E2-7**: Should `coinbase_recipient` in the synthesized `BundleTx` be `Address::ZERO` or `cfg.coinbase_recipient`? v0.1 LOCKS `Address::ZERO` (consistent with the existing `BundleConfig::defaults` Phase 3 placeholder; production wiring of a real coinbase is P6B-F+ scope).
