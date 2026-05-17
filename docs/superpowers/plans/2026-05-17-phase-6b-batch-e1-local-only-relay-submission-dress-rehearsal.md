# Phase 6b Batch E1 -- Local-only relay submission dress rehearsal (planning only)

**Date:** 2026-05-17 KST
**Status:** Draft v0.1. PRE-IMPL PLAN. ASCII-only. No `.rs` / `Cargo.toml` / `Cargo.lock` / config / fixture / env-example / ADR / spec edits in this turn. No commit, no push.
**Awaiting:** manual Codex review of v0.1.

## Predecessors

- P6B-D closed at `36e3b5a` (APPROVED HIGH). `Config::validate()` now permits `live_send=true` for `(Profile::Production, KeyBackend::HsmKms, non-empty audit_key_id)` ONLY.
- `master` HEAD `36e3b5a`. Pre-P6B-E1 targeted baseline (signer + config + app): **67 passed + 0 ignored**.
- Boundary doc `docs/specs/phase-6b-boundary.md` Section 3 row for P6B-E states P6B-E may be split into E1 (local-wiremock-only dress rehearsal binding to 127.0.0.1) + E2 (live submission with user go/no-go after E1). This batch is E1.

## Authorization basis

User authorized P6B-E1 implementation in turn with explicit `MAXIMUM coding / MINIMUM plan` mode + the hard limit "If any code could submit to a non-localhost endpoint by default, stop and fix it fail-closed." Investigation by Explore subagent surfaced 10 architectural decisions; this minimum plan locks them and stops for Codex review before any source-level change. The two-gate pattern (plan -> Codex review -> implementation) from every prior P6B batch is preserved.

## Scope (v0.1)

P6B-E1 ships **the local-wiremock-only dress rehearsal of `submit_bundle -> Ok(SubmissionReceipt)`**. After E1 close the workspace can submit a real `eth_sendBundle` JSON-RPC POST to an in-process mock relay bound to `127.0.0.1` and observe the full G12 7-step chain INHERITING G13 fire under the kill-switch + comparator-pass + matched-bundle preconditions. **No non-localhost endpoint can be submitted to by ANY code path** -- enforced by two redundant fail-closed gates (config-validate-time + adapter-runtime).

P6B-E2 (NOT this batch; locked) ships the operator-controlled flip that permits non-localhost (production-mainnet) relay endpoints. E2 requires its own separate user re-authorization + Codex review.

## 10 architectural locks (Investigation A..J)

The Explore subagent investigation report (this conversation) listed 10 open questions A..J. v0.1 LOCKS each:

### A. New caller location -- LOCKED as new `submission_driver` task

`wire_phase4` (`crates/app/src/lib.rs`) spawns a NEW `submission_driver` task in parallel to the existing `comparator_driver`. Rationale: keeps the P4-E `comparator_driver` body bit-for-bit unchanged; separates the G12 7-step chain into a single auditable function; matches the existing per-driver kill-switch guard pattern. NOT inline within `comparator_driver`.

### B. G12 7-step chain wiring -- LOCKED

NEW broadcast channel `submission_tx: broadcast::Sender<SubmissionAttempt>` where `SubmissionAttempt = { candidate: BundleCandidate, signed_bytes: SignedBundle, sim_outcome: SimulationOutcomeWithFingerprint, relay_outcome: RelaySimulationOutcome }`. `comparator_driver` builds + broadcasts `SubmissionAttempt` ONLY when `compare_result(...) == Match` AND no mismatch path was taken. `submission_driver` subscribes; its iteration body verifies the 7-step chain:

1. **Kill-switch**: first-statement guard `if kill_switch.is_active() { continue; }`.
2. **Signer Ok**: structural -- the presence of `SubmissionAttempt.signed_bytes` proves the signer returned Ok upstream.
3. **Local-sim Ok**: assert `sim_outcome.status == Success`.
4. **Relay-sim Ok**: assert `relay_outcome.status == Match-eligible non-error variant`.
5. **Comparator Match**: structural -- the `SubmissionAttempt` is only broadcast on Match (other variants drop the attempt).
6. **Bundle-byte equality**: v0.1 SIMPLIFICATION (E1 scope): assert `signed_bytes.0.is_empty() == false` AND `signed_bytes.0.len() >= 64` (minimum-valid-tx sanity). True keccak-comparison against relay echo is P6B-E2 scope when production relay responses are observable.
7. **G13 inheritance runtime assertion**: synchronous read of `config.relay.live_send && config.active_profile == Production && config.relay.key_backend == HsmKms`. Any false -> skip iteration with WARN. This is the NEW app-side read of `config.relay.live_send` (breaks P6B-D's 0-hits-in-`crates/app/src/` invariant; documented in boundary doc Section 5 per-callsite requirement).

### C. Localhost-only enforcement -- LOCKED as defense-in-depth (config + adapter)

Two fail-closed gates:

1. **Config validate-time**: NEW `ConfigError::LiveSendRequiresLocalhostEndpoint` rejects ANY config with `live_send == true` AND a relay endpoint whose URL host is not in `{"127.0.0.1", "localhost", "::1"}` (exact-string match on `url::Url::host_str()`). Fires AFTER the P6B-D `LiveSendRequires*` branches; preserves the validation evaluation order.
2. **Adapter runtime**: NEW pre-submit check in `FlashbotsRelay::submit_bundle(...)` (and Bloxroute symmetric) -- after the kill-switch guard but before the HTTP POST, parse the configured endpoint URL and verify host is in the localhost set; fail-closed with NEW `BundleRelayError::SubmitDisabledNonLocalhost`. Redundant with (1); intentional defense-in-depth.

### D. Bundle-byte equality (Step 6) -- LOCKED SIMPLIFIED for E1

E1 implementation: `signed_bytes.0.len() >= 64`. Rationale: a true keccak-against-relay-echo requires either (i) a production relay actually echoing the bundle hash (E2 scope) or (ii) the test wiremock returning a hash matching the workspace's pre-submit keccak (would be tautological -- workspace would compute its own hash, send it, and verify the echo matches the hash it just computed; structurally validates the JSON-RPC wiring but not relay-side equality). E1 punt to non-empty + minimum-length sanity; E2 lifts to true bundle-byte equality.

### E. New types -- LOCKED minimal

- **NEW** `SubmissionAttempt` struct in `crates/bundle-relay/src/lib.rs`: fields enumerated under (B). `Debug + Clone`. NOT serializable -- in-process broadcast only.
- **EXISTING** `SubmissionReceipt` (`crates/bundle-relay/src/lib.rs:59-74`): reused as-is. Adapter HTTP success path constructs from the relay response.

### F. Test relay wiremock spawning -- LOCKED test-harness-only

The wiremock `MockServer::start()` lives ONLY in `#[tokio::test]` functions in `crates/relay-clients/tests/` and `crates/app/tests/`. NOT spawned by `wire_phase4` in any code path. Operators in dev/test/shadow have no use for it; production operators set `enabled_relays[0].endpoint` to their own deployment URL when their HSM/KMS wiring is ready. NO new dev-only feature flag.

### G. Data threading -- LOCKED via `submission_tx` broadcast

`comparator_driver` extends to capture the `signed_bytes` (currently has `BundleCandidate` only). To do this without invoking the signer twice, the execution_driver chain must produce `signed_bytes` and broadcast them via the existing `sim_tx` (`tokio::sync::broadcast::Sender<SimResult>`) OR a new sibling channel. v0.1 LOCKS: extend `SimResult` to include `signed_bytes: Option<SignedBundle>` (optional so non-execution paths can broadcast None). `comparator_driver` then propagates the bytes into the `SubmissionAttempt` it builds on Match.

**Alternative considered + rejected**: have `submission_driver` re-invoke the signer. Rejected because (1) it would double-emit the G15 audit + counter events (one per signing attempt, not one per submission attempt); (2) it would race with `comparator_driver` on the same `BundleCandidate`; (3) it would multiply runtime G11 callsites which P6B-CD locked at 1.

### H. Submission receipt journaling -- LOCKED

NEW `FileJournal<SubmissionReceipt>` opened in `wire_phase4`. `submission_driver` appends-and-flushes the receipt BEFORE acknowledging success in the loop (matches the P4-E mismatch journal append-before-broadcast pattern). Journal path: NEW `JournalConfig::submission_journal_path: PathBuf` field with default `"/var/lib/lmax/submission.log"`. NO retroactive serde change to existing config files (the field is `#[serde(default)]` with the path default).

### I. Adapter flip scope -- LOCKED narrow

**Flashbots**: `submit_bundle` body flips to return `Ok(SubmissionReceipt)` ONLY when ALL of: kill_switch inactive AND endpoint host is localhost AND HTTP POST succeeds. Otherwise: `Err(KillSwitchActive)`, `Err(SubmitDisabledNonLocalhost)`, OR a new `Err(SubmitHttpFailed)` variant. The pre-existing `Err(SubmitDisabled)` becomes unreachable in the new body but the variant stays in `BundleRelayError` for forward compat.

**Bloxroute**: NOT touched in P6B-E1. Continues to return `Err(SubmitDisabled)` / `Err(KillSwitchActive)` per P6B-D close. Rationale: E1 is single-adapter dress rehearsal; multi-adapter parity is P6B-E2 scope. The boundary doc P4-E "at most 1 relay" config validation already bounds E1 to a single adapter; pick Flashbots.

### J. P6B-E1 vs P6B-E2 boundary -- LOCKED

At P6B-E1 close:
- `submit_bundle -> Ok(SubmissionReceipt)` IS reachable from `crates/app/src/submission_driver` (G3 grows from 0 to 1).
- Localhost endpoint validation rejects non-localhost endpoints at boot (config validate) AND at runtime (adapter check).
- `eth_sendBundle` JSON-RPC runtime call IS present (G14 grows from 5 doc-comment hits to 5 doc-comment + 1 runtime hit; the new runtime hit is documented per-file:line in boundary doc Section 5).
- Multi-adapter relay submission, non-localhost endpoint flow, P6B-F audit + `phase-6b-complete` tag: ALL REMAIN LOCKED.

P6B-E2 (NOT THIS BATCH) ships the non-localhost adapter flip + (optionally) multi-adapter parity. P6B-F (NOT THIS BATCH) ships the audit + tag.

## Deliverables (D-E1-1..D-E1-9)

| ID | Scope |
|---|---|
| D-E1-1 | `ConfigError::LiveSendRequiresLocalhostEndpoint` + `Config::validate()` localhost-endpoint check. |
| D-E1-2 | `BundleRelayError::SubmitDisabledNonLocalhost` + `BundleRelayError::SubmitHttpFailed` (both payload-free). |
| D-E1-3 | `FlashbotsRelay::submit_bundle` body rewrite: kill-switch + localhost check + HTTP POST + receipt parse + Ok-return. |
| D-E1-4 | `eth_sendBundle` JSON-RPC body builder fn in `crates/relay-clients/src/send_bundle.rs` (new file) sibling to `call_bundle.rs`. |
| D-E1-5 | NEW `SubmissionAttempt` struct in `crates/bundle-relay/src/lib.rs`. Extend `SimResult` (or sibling type) with `signed_bytes: Option<SignedBundle>`. |
| D-E1-6 | NEW `submission_driver` task in `crates/app/src/lib.rs::wire_phase4`. Implements G12 7-step chain. Owns 1 production runtime `submit_bundle(` call site (G3 := 1). |
| D-E1-7 | NEW `FileJournal<SubmissionReceipt>` + `JournalConfig::submission_journal_path`. Append-and-flush before any submission-driver loop continue. |
| D-E1-8 | Boundary doc additive: Section 3 P6B-E1 amendment paragraph + Section 5 per-callsite entry for the new `submit_bundle(` caller + the new `eth_sendBundle` runtime reference. |
| D-E1-9 | Targeted tests (D-T-E1-1..D-T-E1-7; see "Tests" below). |

## Tests (7 minimum targeted; v0.1 LOCKED)

| ID | File / location | Asserts |
|---|---|---|
| D-T-E1-1 | `crates/config/src/lib.rs` `#[cfg(test)]` | Production + HsmKms + live_send=true + non-localhost endpoint (e.g., `https://relay.flashbots.net`) -> `Err(LiveSendRequiresLocalhostEndpoint)`. |
| D-T-E1-2 | `crates/config/src/lib.rs` `#[cfg(test)]` | Production + HsmKms + live_send=true + endpoint `http://127.0.0.1:9999` -> `Ok(_)`. Variants: `http://localhost:9999`, `http://[::1]:9999` also pass. |
| D-T-E1-3 | `crates/relay-clients/tests/submit_disabled.rs` (extend) | KillSwitch active + localhost endpoint -> `Err(KillSwitchActive)` (precedence preserved). |
| D-T-E1-4 | `crates/relay-clients/tests/flashbots.rs` (extend) | Adapter with non-localhost endpoint + KillSwitch inactive -> `Err(SubmitDisabledNonLocalhost)` (defense-in-depth at adapter runtime). |
| D-T-E1-5 | `crates/relay-clients/tests/flashbots.rs` (extend) | Wiremock at `127.0.0.1:<random>`, `eth_sendBundle` POST received, body shape verified by `serde_json::Value` equality (mirrors the P6B-C RC-F-6 pattern); `submit_bundle` returns `Ok(SubmissionReceipt)`. |
| D-T-E1-6 | `crates/app/tests/wire_phase4_e1.rs` (new) | `wire_phase4` with Production + HsmKms + live_send=true + localhost endpoint + wiremock-bound 127.0.0.1 + active comparator-pass path -> `submission_driver` invokes `submit_bundle` once, journal receives 1 `SubmissionReceipt` entry. |
| D-T-E1-7 | `crates/app/tests/wire_phase4_e1.rs` (new) | `wire_phase4` with `live_send=false` -> G13 inheritance fails -> `submission_driver` SKIPS iteration on every comparator-pass; 0 calls to mock relay. Kill-switch-active variant of same test: same skip behavior. |

Pre-P6B-E1 baseline: 67 passed. P6B-E1 target: **67 + 7 = 74 passed + 0 ignored**.

NO new live-network / live-KMS / `#[ignore]` test. The wiremock relay is in-process; the workspace never reaches any external endpoint.

## Gates at P6B-E1 close (deltas vs P6B-D close `36e3b5a`)

| Gate | Pre-P6B-E1 | Post-P6B-E1 |
|---|---|---|
| G2a / G2b / G2c / G2d / G2e / G2f / G2g | 0 / 0 / 13 / 13 / 2 / 0 / 0 | UNCHANGED |
| G3 (`submit_bundle(` callers in `crates/app/src/`) | 0 | **1** (documented per file:line in boundary doc Section 5; the new `submission_driver` task body) |
| G4 (`dyn BundleRelay` in `crates/app/src/`) | 0 | **1** (the new submission_driver's `Arc<dyn BundleRelay>` handle; documented in Section 5) |
| G5 (config rejects) | 5 (P6B-D set) | 6 (`LiveSendRequiresLocalhostEndpoint` added) |
| G6 / G7 / G8 / G9 / G10 | 0 / 1 / 0 / 3 / enforced | UNCHANGED |
| G11 (production sign_tx call site) | 1 (test-only hook) | UNCHANGED at 1 (signer invocation still happens inside `BundleConstructor`; `submission_driver` consumes the existing `signed_bytes` carried in the broadcast envelope) |
| G12 (submit_bundle pre-check chain) | vacuously 0 callers | **ENFORCED** for the 1 new caller; 7-step chain verified per-iteration |
| G13 (live_send=true profile scope) | ENFORCED at config-validate boot | UNCHANGED at config-validate boot + the new submission_driver iterates the runtime inheritance assertion (step 7) on every comparator-pass |
| G14 (`eth_sendBundle` runtime) | 5 doc-comment hits | **5 doc-comments + 1 runtime hit** at the new `send_bundle.rs` JSON-RPC builder; the new runtime hit is documented per file:line in Section 5 |
| G15 (production-signer audit-surface) | 4-piece surface + 7-label set | UNCHANGED. P6B-E1 adds NO new sign attempts (the existing `BundleConstructor` invocation path is preserved). |
| NEW: localhost-endpoint gate | n/a | Config validate rejects non-localhost when live_send=true; adapter rejects non-localhost at runtime. Defense-in-depth verified by D-T-E1-1 + D-T-E1-4. |

## Hard forbids at P6B-E1 close

- NO real external relay URL anywhere in repo / tests / fixtures / configs / env-examples.
- NO mainnet / live relay submission. Any HTTP POST exits exclusively to `127.0.0.1` / `localhost` / `::1` per the dual gate.
- NO live-network test enabled by default. NO `#[ignore]`-gated live-network test added.
- NO live AWS / KMS test.
- NO private-key material / `SecretKey` / `SigningKey` / `test_key` byte literal anywhere (G2g preserved at 0 hits absolute).
- NO config example file (`config/base/`, `config/dev/`, `config/test/`) modified to enable `live_send=true`. Examples stay fail-closed.
- NO `phase-6b-complete` tag (P6B-F scope).
- NO P6B-E2 work (non-localhost adapter flip).
- NO change to `Profile` / `KeyBackend` enums.
- NO change to `RelayConfig::live_send`, `audit_key_id`, `signing_audit_alert` field types.
- NO `Cargo.toml` workspace-dep change. (Local additions allowed: `wiremock` is already a dev-dep in `crates/relay-clients`; will be added as dev-dep in `crates/app` for D-T-E1-6 + D-T-E1-7.)
- NO change to `crates/signer/`, `crates/execution/`, `crates/state/`, `crates/opportunity/`, `crates/risk/`, `crates/simulator/`, `crates/state-fetcher/`, `crates/relay-sim/`. The signer + sim pipeline is UNCHANGED.
- NO change to `crates/bundle-relay/` other than the 2 new error variants + the new `SubmissionAttempt` struct.
- NO ADR amendment. ADR-001 Amendment 1 already describes the P6B-E unlock at the high level; E1 is the first half of the unlock and does not require its own ADR amendment.
- NO touch to `docs/specs/production-signer.md`, `docs/specs/execution-safety.md`, `docs/specs/phase-6a-boundary.md`.
- NO `.coordination/` staging (gitignored). NO `AGENTS.md`, `fixture_output.txt`, `hook_toast.md` staging.

## File-touch summary (for the future P6B-E1 impl turn)

| File | Change kind |
|---|---|
| `crates/config/src/lib.rs` | Substantive: NEW `LiveSendRequiresLocalhostEndpoint` variant + validate() body check + test. Existing `live_send=true` branches UNCHANGED. |
| `crates/bundle-relay/src/lib.rs` | Substantive: 2 NEW `BundleRelayError` variants (`SubmitDisabledNonLocalhost`, `SubmitHttpFailed`) + NEW `SubmissionAttempt` struct. `BundleRelay` trait + `SubmissionReceipt` UNCHANGED. |
| `crates/relay-clients/src/flashbots.rs` | Substantive: `submit_bundle` body rewrite to localhost-only Ok-path. New helper to parse + check endpoint host. |
| `crates/relay-clients/src/send_bundle.rs` | NEW. `eth_sendBundle` JSON-RPC body builder sibling to `call_bundle.rs`. |
| `crates/relay-clients/src/lib.rs` | Additive: `mod send_bundle`. |
| `crates/relay-clients/tests/submit_disabled.rs` | Additive: extend with localhost / non-localhost / kill-switch precedence tests. |
| `crates/relay-clients/tests/flashbots.rs` | Additive: wiremock eth_sendBundle test. |
| `crates/app/src/lib.rs` | Substantive: NEW `submission_driver` task spawned in `wire_phase4`; NEW `submission_tx` broadcast; NEW `Arc<dyn BundleRelay>` field on AppHandle4. `comparator_driver` extended to broadcast `SubmissionAttempt` on Match. Carry-forward extension: `SimResult` (or sibling) carries `signed_bytes`. |
| `crates/app/tests/wire_phase4_e1.rs` | NEW. 2 integration tests (D-T-E1-6 + D-T-E1-7). |
| `crates/journal/src/lib.rs` | UNCHANGED. The existing `FileJournal<T>` is generic over the payload type; `FileJournal<SubmissionReceipt>` works without library change. `SubmissionReceipt` gains `Archive + Serialize/Deserialize` derives via the existing `rkyv_compat` adapter pattern -- additive only. |
| `crates/app/Cargo.toml` | Minimal: add `wiremock` as dev-dependency. |
| `crates/bundle-relay/Cargo.toml` | Possibly minimal: add `rkyv` dep if not already present for the `SubmissionReceipt` derives. |
| `docs/specs/phase-6b-boundary.md` | Additive: Section 3 P6B-E1 amendment + Section 5 per-callsite entries for the new submit_bundle caller + the new eth_sendBundle runtime reference. |

NO touch in P6B-E1: `crates/signer/`, `crates/execution/`, `crates/state/`, `crates/opportunity/`, `crates/risk/`, `crates/simulator/`, `crates/state-fetcher/`, `crates/relay-sim/`, `crates/bundle-relay/` beyond the listed additions, `crates/relay-clients/src/bloxroute.rs`, `crates/app/src/main.rs`, `crates/app/tests/wire_phase4.rs`, `docs/adr/`, `docs/specs/production-signer.md`, `docs/specs/execution-safety.md`, `docs/specs/phase-6a-boundary.md`, all earlier frozen plan files.

## Plan execution checklist (after Codex APPROVED + explicit user re-authorization)

- [ ] **Step 1**: User explicitly re-authorizes P6B-E1 implementation.
- [ ] **Step 2**: `crates/config/src/lib.rs` -- add `LiveSendRequiresLocalhostEndpoint` variant + validate() check + D-T-E1-1, D-T-E1-2.
- [ ] **Step 3**: `crates/bundle-relay/src/lib.rs` -- add 2 NEW error variants + `SubmissionAttempt` struct.
- [ ] **Step 4**: `crates/relay-clients/src/send_bundle.rs` -- NEW file with JSON-RPC body builder.
- [ ] **Step 5**: `crates/relay-clients/src/flashbots.rs` -- rewrite `submit_bundle` body for the Ok-path under localhost gate. Add D-T-E1-3, D-T-E1-4, D-T-E1-5.
- [ ] **Step 6**: `crates/app/src/lib.rs` -- extend `SimResult` carry-forward + add `submission_tx` broadcast + add `submission_driver` task + add `FileJournal<SubmissionReceipt>` + add new field on `AppHandle4`. Extend `comparator_driver` to broadcast `SubmissionAttempt` on Match.
- [ ] **Step 7**: `crates/app/tests/wire_phase4_e1.rs` -- NEW. Add D-T-E1-6, D-T-E1-7.
- [ ] **Step 8**: `docs/specs/phase-6b-boundary.md` -- Section 3 P6B-E1 amendment + Section 5 per-callsite entries.
- [ ] **Step 9**: Targeted self-check: `cargo fmt --check`; `cargo clippy -p rust-lmax-mev-signer -p rust-lmax-mev-config -p rust-lmax-mev-app -p rust-lmax-mev-bundle-relay -p rust-lmax-mev-relay-clients --all-targets -- -D warnings`; targeted test count reaches 67 + 7 = 74. P6B-E1 ripgrep gates from "Gates at P6B-E1 close" table.
- [ ] **Step 10**: Commit + push as `feat(p6b-e1): add local-only relay submission dress rehearsal`.
- [ ] **Step 11**: Emit P6B-E1 closeout to `.coordination/claude_outbox.md`.

## Open questions for Codex at v0.1

1. **Q-E1-1**: Caller location -- new `submission_driver` task vs inline within `comparator_driver`. v0.1 LOCKS new task per (A). Codex verdict.
2. **Q-E1-2**: Bundle-byte equality simplification at E1 -- non-empty + length >= 64 vs true keccak equality against relay echo. v0.1 LOCKS simplification per (D). Codex verdict.
3. **Q-E1-3**: Localhost enforcement -- config-only / adapter-only / hybrid. v0.1 LOCKS hybrid per (C). Codex verdict.
4. **Q-E1-4**: Adapter flip scope -- single adapter (Flashbots) vs both (Flashbots + Bloxroute). v0.1 LOCKS single per (I). Codex verdict.
5. **Q-E1-5**: Data threading -- extend `SimResult` to carry `signed_bytes: Option<SignedBundle>` vs re-invoke signer in submission_driver. v0.1 LOCKS extend per (G). Codex verdict on whether the carry-forward approach correctly preserves G11 = 1 and G15 audit-once-per-attempt.
6. **Q-E1-6**: Journal -- new `FileJournal<SubmissionReceipt>` with `submission_journal_path` config field vs reuse the existing `MismatchAbort` journal vs no submission journal at E1. v0.1 LOCKS new dedicated journal per (H).
7. **Q-E1-7**: AppHandle4 surface -- add `Arc<dyn BundleRelay>` field + getter, or hide inside submission_driver task only. v0.1 LOCKS hidden inside task (no AppHandle4 surface change) to minimize G4 footprint. Codex verdict.

## Process (v0.1; two-gate carried forward)

Plan approval and implementation authorization are TWO SEPARATE gates.

1. Claude writes v0.1 plan + emits the v0.1 Codex review pack to `.coordination/claude_outbox.md`.
2. Claude STOPS for manual Codex review.
3. User pastes pack to Codex; relays verdict.
4. Claude records verdict in `.coordination/codex_review.md`.
5. **APPROVED** -> commit + push v0.1 plan to `master`; STOP. P6B-E1 implementation does NOT begin in the same turn. Separate explicit user re-authorization required for the impl turn.
6. **REVISION REQUIRED** -> revise plan in place + re-emit pack as v0.2.
7. **Scope / ADR change required beyond what this plan proposes** -> HALT to user.

## Verdict shapes Claude expects (v0.1)

- **APPROVED** -> commit + push v0.1 plan; STOP for separate user re-authorization.
- **REVISION REQUIRED** -> revise + re-emit as v0.2.
- **Scope / ADR change required** -> HALT to user.
