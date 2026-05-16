# Phase 6 Batch C — Relay `eth_callBundle` wiremock-only adapter tests

**Date:** 2026-05-16 KST
**Status:** Draft v0.3 (revised after manual Codex REVISION REQUIRED HIGH on v0.2, 2026-05-16 KST). Four v0.2 → v0.3 fixes:
(1) **Block-number hex corrected to `0x14fb180`.** v0.2's expected JSON envelope in the outbox stated `"blockNumber": "0x150f900"` / `"stateBlockNumber": "0x150f900"`. That is wrong: `22_000_000 == 0x14fb180`, not `0x150f900`. The plan body §D-C2 was already correct at `0x14fb180`; the outbox was the divergence. v0.3 re-emits the outbox with `0x14fb180` and re-states the plan §D-C2 / §D-C3 expected JSON + the §93 inline comment with `0x14fb180` consistently. The checklist also references the same hex.
(2) **All "public test-vector RLP" / "fixture pre-computed test-vector RLP bytes" / "test-vector RLP" wording removed from §Scope and §Why-small.** v0.2 still referenced those phrases as part of "not adopting" / "v0.1 incorrectly described". v0.3 rewrites those sentences to describe v0.3 positively: the payload is synthetic placeholder bytes used to drive the adapter's hex-encoding path; no "test-vector" or "RLP" claim appears in either section. The historical v0.1 → v0.2 changelog block is also dropped from §Status to remove residual phrase carry-through; v0.3 §Status is now the only top-of-file changelog.
(3) **Stale `body_partial_json` mention removed from §Reused.** v0.2 §Reused parenthetical noted "`body_partial_json` is NOT used in v0.2". v0.3 locks the matcher strategy as `received_requests()` + exact `serde_json::Value` equality and the §Reused list mentions only what v0.3 actually consumes: `MockServer::received_requests()` + `wiremock` baseline. The matcher rationale lives in §D-C2 / Q-C4 only.
(4) **Dropped-e2e rationale reworded.** v0.2 §"What P6-C does NOT do" claimed the structural property "signer fail-closed before any relay-sim work" was **already proven by** G3 + G11 + P6-B D-T1 + §3 PRECEDENCE — that wording implied a runtime proof of the full signer-before-relay-sim chain. v0.3 reframes accurately: the signer-before-relay-sim ordering is (a) a **Phase 6b runtime contract** documented in §3 PRECEDENCE of `docs/specs/phase-6a-boundary.md`, and (b) enforced in Phase 6a by **separate static + unit-test invariants** — G3 (no `submit_bundle(` callers in `crates/app/src/`), G11 (single `sign_tx` call site under `#[cfg(test)]`), G4 (no `dyn BundleRelay` / `Arc<dyn BundleRelay>` in `crates/app/src/`), and the P6-B `crates/execution` unit test that proves `DisabledSigner` returns `Err(SignerError::SignerDisabled)` from the hook. No runtime end-to-end production-code chain exists in Phase 6a, so no runtime e2e can be written here; that does NOT mean the chain is "proven at runtime" — it means runtime e2e is **deferred to Phase 6b** when production wiring lands.
Awaiting manual Codex re-review.
**Predecessors:**

- Phase 6 overview v0.3 APPROVED HIGH at `c08db38` (pushed).
- P6-A pre-impl plan v0.3 APPROVED HIGH at `4c4c0dd` (pushed).
- P6-A boundary spec at `64ffaee`; D-B0 amendment at `19e263a`; closeout v3 wording fixes at `a7367b7` — all pushed.
- P6-B pre-impl plan v0.5 APPROVED HIGH at `9a6ebd2` (pushed).
- P6-B D-B1..D-B6 impl at `b27d01a` (pushed). HEAD.

## Scope

P6-C is a **test-only batch**. Adds wiremock-based JSON-RPC body-shape assertions for the `eth_callBundle` request the existing `FlashbotsRelay` + `BloxrouteRelay` adapters send, exercised with **synthetic placeholder bytes** (NO key material) as `RelaySimRequest::txs` payload. Closes the P6-C overview deliverable as adapted: wiremock-only adapter tests that, when fed an inline synthetic-placeholder byte payload, assert the adapter formats `eth_callBundle` correctly. The payload is a short byte sequence whose only role is to drive the adapter's hex-encoding path so the full wire envelope can be inspected; the wire-format-assertion intent of the overview is preserved.

**Phase 6a invariants explicitly preserved:**

- NO live relay test code (Q-P6-D resolved wiremock-only).
- NO `eth_sendBundle` runtime path; G1 stays doc-only.
- NO actual relay submission; `submit_bundle` returns `Err(SubmitDisabled)` unchanged in every adapter; G3 + G4 zero hits in `crates/app/src/`.
- NO production signer impl; NO key material; NO `live_send = true`.
- NO new `Signer` / `DisabledSigner` / `SignerError` / `SignerDisabled` symbol references anywhere (G2c allow-list unchanged from P6-B 3-file set).
- NO new `secp256k1` / `k256` / `alloy-signer` / `ethers-signers` / `Wallet` / `PrivateKey` / `sign_transaction` / `funded` symbols (G2a stays 0 hits).
- NO `#[ignore]` test additions (G7 stays at P2-C baseline).
- NO Cargo dep additions for `secp256k1` / `k256` / `alloy-signer` / `ethers-signers` (G2b stays 0).

## What P6-C does NOT do (overview-locked non-goals)

The overview's P6-C tests bullet "End-to-end through `BundleConstructor::with_signer(DisabledSigner)`: chain terminates at the signer with `Err(SignerError::SignerDisabled)`; relay-sim path never invoked" is **dropped from P6-C runtime scope**. Reason: at the landed P6-B HEAD `b27d01a`, production `BundleConstructor::construct(...)` / `construct_with_context(...)` do NOT invoke `Signer::sign_tx`. No production-code chain from `BundleConstructor` → signer → `simulate_bundle` exists in Phase 6a, so a runtime end-to-end test cannot be written here.

The signer-before-relay-sim ordering is intentionally split into two pieces:

- **Phase 6b runtime contract.** §3 PRECEDENCE in `docs/specs/phase-6a-boundary.md` documents the ordering that production code MUST follow once the runtime chain lands in Phase 6b: signer is invoked first, and `Err(SignerError::SignerDisabled)` short-circuits before any relay-sim work. P6-C does NOT prove this runtime contract — it cannot, because the chain does not yet exist in production code. The contract proof is **deferred to Phase 6b** when production wiring lands.
- **Phase 6a static + unit-test invariants.** Separately and independently, Phase 6a enforces a set of invariants that constrain how the eventual runtime chain can be wired: G3 (no `submit_bundle(` callers in `crates/app/src/`), G4 (no `dyn BundleRelay` / `Arc<dyn BundleRelay>` in `crates/app/src/`), G11 (single `sign_tx` call site inside the `#[cfg(test)]` hook in `crates/execution/src/lib.rs:238`), and the P6-B `crates/execution` unit test that proves `DisabledSigner` returns `Err(SignerError::SignerDisabled)` from the hook. These are static / unit-test invariants, NOT a runtime e2e proof of the ordering contract.

Adding a runtime end-to-end test in P6-C would require new production-code wiring that invokes the signer — which is **explicitly out of scope** for Phase 6a (Phase 6b unlock). **See Q-C1** for Codex confirmation; if Codex prefers a structural-only restatement test instead, v0.3 will add it.

## Why P6-C is small

The hard work landed at Phase 4 P4-E:

- `crates/relay-clients/src/call_bundle.rs` already encodes `eth_callBundle` JSON-RPC bodies with `txs` hex-encoded, `blockNumber` / `stateBlockNumber` formatted as `"0x<hex>"`, `id: 1`, `jsonrpc: "2.0"`, `method: "eth_callBundle"`.
- Both adapters (`FlashbotsRelay`, `BloxrouteRelay`) already implement `simulate_bundle` against this shared helper.
- The empty-txs short-circuit returning `Err(RelaySimError::UnsignedBundleUnavailable)` already lives in both adapters (covered by `rc_f_5_*` and `rc_b_5_*` tests).
- Existing wiremock tests `rc_f_1` / `rc_b_1` already verify **response parsing**.

What's missing — and what P6-C adds — is **request-body-shape assertion**: prove that the JSON-RPC body the adapter POSTs to the relay matches the `eth_callBundle` schema when given non-trivial bytes. The current `rc_f_1` / `rc_b_1` use `vec![0xDE, 0xAD, 0xBE, 0xEF]` as payload but only assert the parsed response; they do NOT inspect the request body the mock server received. P6-C closes that gap. The byte payload used is **synthetic placeholder bytes**; the only role of the bytes is to drive the adapter's hex-encoding path so the full wire envelope can be inspected.

## Deliverables

### D-C1 — Synthetic placeholder-byte fixture (test-only, NO key material, NOT valid RLP)

Inline a small constant byte sequence inside each test file. The bytes are **synthetic placeholders**, deliberately NOT a valid RLP-encoded signed transaction; their content has no semantic meaning beyond providing a non-empty `txs` payload so the adapter exercises its hex-encoding path. Q-C2 / Q-C3 resolved in v0.2 to: **synthetic + inline** (no `crates/relay-clients/tests/fixtures/` directory; no "RLP test-vector" claim).

Concretely each test file declares two paired constants:

```text
// P6-C D-C1: synthetic wire-format placeholder bytes — NOT a valid
// RLP-encoded signed transaction; NO key material implied; the only
// purpose of these bytes is to exercise the hex-encoding path inside
// `crates/relay-clients/src/call_bundle.rs`.
const FIXTURE_PLACEHOLDER_BYTES: &[u8] = &[0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xF0, 0x0D];
const FIXTURE_PLACEHOLDER_HEX: &str = "0xdeadbeefcafef00d";
```

The two constants are paired and kept in sync by hand; the test asserts the adapter hex-encodes `FIXTURE_PLACEHOLDER_BYTES` into exactly `FIXTURE_PLACEHOLDER_HEX` (the second constant is the expected wire output, not derived at runtime from the first). **No `hex` crate.** **No `hex::decode` call.** Raw byte-array literal + matching string literal only; no Cargo change.

**Hard invariants on the fixture:**

- No key material — no signing-derived bytes; no published-private-key reference; no plaintext private key.
- Not claimed as "RLP" / "RLP test vector" / "test-vector RLP" anywhere in code, comments, or test function names.
- Exact length and content fixed by the constant; test failure on accidental mutation is immediate.
- Lowercase-hex form in `FIXTURE_PLACEHOLDER_HEX` matches the lowercase format `call_bundle.rs` produces via `format!("{b:02x}")` at `crates/relay-clients/src/call_bundle.rs:105`.

### D-C2 — Flashbots wiremock body-shape test (new `rc_f_6_*`)

Add a new `#[tokio::test]` to `crates/relay-clients/tests/flashbots.rs`:

```text
rc_f_6_call_bundle_body_shape_matches_with_placeholder_bytes
```

Test body (v0.2 — `received_requests()` + exact `serde_json::Value` equality, NOT `body_partial_json`):

1. Start a `MockServer`.
2. Mount a permissive happy-path mock: `Mock::given(method("POST")).and(path("/"))` (no body matcher) responding with a minimal `eth_callBundle` happy-path result (same canned response as `rc_f_1`).
3. Construct `req = RelaySimRequest { block_hash: B256::from([0u8; 32]), state_block_number: 22_000_000u64 /* 0x14fb180 */, txs: vec![FIXTURE_PLACEHOLDER_BYTES.to_vec()] }`.
4. Call `relay.simulate_bundle(req).await.expect("happy path must succeed");`.
5. Fetch the received requests: `let received = server.received_requests().await.expect("wiremock recorded requests"); assert_eq!(received.len(), 1, "exactly one POST expected");`.
6. Parse the body: `let parsed: serde_json::Value = serde_json::from_slice(&received[0].body).expect("body is valid JSON");`.
7. Build the expected complete value:

   ```text
   let expected = serde_json::json!({
       "jsonrpc": "2.0",
       "id": 1,
       "method": "eth_callBundle",
       "params": [{
           "txs": [FIXTURE_PLACEHOLDER_HEX],
           "blockNumber": "0x14fb180",
           "stateBlockNumber": "0x14fb180"
       }]
   });
   ```

   (No `timestamp` key in expected — the adapter sends `None` → omitted via `#[serde(skip_serializing_if = "Option::is_none")]`.)
8. `assert_eq!(parsed, expected, "request body must match expected JSON-RPC envelope exactly");`.

Equality on `serde_json::Value` enforces both **presence** (every key in the expected envelope) AND **absence** (no extra keys: any future addition of `coinbase` / `gas_price` / `tx_index` etc. inside the adapter forces an explicit test update). Q-C6 strictness satisfied.

### D-C3 — bloXroute wiremock body-shape test (new `rc_b_6_*`)

Mirror of D-C2 for `crates/relay-clients/tests/bloxroute.rs`:

```text
rc_b_6_call_bundle_body_shape_matches_with_placeholder_bytes
```

Same fixture constants (copied verbatim into the second test file — v0.2 keeps each test file self-contained per existing relay-clients test-file pattern; no shared `common::` module added). Same expected JSON shape (the shared `call_bundle.rs` helper produces an identical body regardless of which adapter calls it; this test guards against future adapter divergence).

### D-C4 — DROPPED in v0.2

The v0.1 "optional" doc-comment edit to `crates/relay-clients/src/call_bundle.rs` is removed. P6-C is strictly **test-only**; no source-file edit anywhere in any crate. The boundary-spec cross-link is implicit via §3 + §4 G11 wording at HEAD `a7367b7`. A separate doc-only follow-up batch can land an explicit cross-link if Codex wants it later.

### D-C5 — NO new fixture-file directory, NO new dep, NO new feature flag, NO source-file edit

- No `crates/relay-clients/tests/fixtures/` directory.
- No new Cargo `[dependencies]` or `[dev-dependencies]` — `wiremock` + `tokio` + `serde_json` + `alloy-primitives` already present in `crates/relay-clients/Cargo.toml`. No `hex` crate.
- No new `[features]` block.
- No `.rs` change in `crates/relay-clients/src/` (or any other `src/` directory anywhere in the workspace). Only `crates/relay-clients/tests/flashbots.rs` + `crates/relay-clients/tests/bloxroute.rs` are touched.

## Tests

| ID | Crate / file | Test function | Kind | New / modified |
|---|---|---|---|---|
| D-T-C1 | `crates/relay-clients/tests/flashbots.rs` | `rc_f_6_call_bundle_body_shape_matches_with_placeholder_bytes` | `#[tokio::test]` integration | **new** |
| D-T-C2 | `crates/relay-clients/tests/bloxroute.rs` | `rc_b_6_call_bundle_body_shape_matches_with_placeholder_bytes` | `#[tokio::test]` integration | **new** |

Expected workspace test total at P6-C close: **233 + 2 = 235 passed + 1 ignored**.

No `#[ignore]` additions; no live-network tests; no env-gated test paths.

## Reused (no duplication)

- `RelaySimulator` trait + `RelaySimRequest` + `RelaySimError::UnsignedBundleUnavailable` + `RelaySimError::Transport` + `RelaySimError::UnrecognizedResponse` — all in `crates/relay-sim/src/lib.rs` since P4-E.
- `crates/relay-clients/src/call_bundle.rs` shared `eth_callBundle` JSON-RPC helper — unchanged.
- `FlashbotsRelay` + `BloxrouteRelay` ctors — unchanged.
- `MockServer` + `MockServer::received_requests()` — both from `wiremock` already declared as `[dev-dependencies]`. v0.3 matcher strategy: permissive happy-path `Mock` (no body matcher) + `received_requests()` + exact `serde_json::Value` equality. Rationale in §D-C2 / Q-C4.

## Gates at P6-C close (deltas vs P6-B close baseline at `a7367b7`)

| Gate | Delta | Notes |
|---|---|---|
| G1 | unchanged | doc-comment-only `eth_sendBundle` hits (5 sites; carry from P6-B). |
| G2a (POST-D-B0 form) | **0 hits absolute** | unchanged; no new signer-symbol set additions. |
| G2b | unchanged (0) | no new dep symbols. |
| G2c | **unchanged 3-file allow-list** | new relay-clients tests do NOT introduce `Signer` / `DisabledSigner` / `SignerError` / `SignerDisabled` symbol references. |
| G2d | unchanged | zero hits outside the union allow-list. |
| G2e | **unchanged at 2** | no new signer dep edges; `rust-lmax-mev-signer` not added to `crates/relay-clients/Cargo.toml`. |
| G3 | unchanged (0) | no new `submit_bundle(` callers in `crates/app/src/`. |
| G4 | unchanged (0) | no new `dyn BundleRelay` / `Arc<dyn BundleRelay>` in `crates/app/src/`. |
| G5 | unchanged | no `live_send` mutation. |
| G6 | unchanged | no new `api_key` log emission; no new tracing of secrets. |
| G7 | unchanged | no new `#[ignore]` tests. |
| G8 | unchanged shape | `wiremock` is a leaf dev-dep; no new workspace cycles. |
| G9 | unchanged | `KillSwitch` reach unchanged. |
| G10 | unchanged (documented; enforces P6-D) | per-adapter kill-switch first-statement lands at P6-D. |
| G11 | unchanged (1 site) | single `sign_tx` call site in `crates/execution/src/lib.rs:238`; relay-clients tests do not invoke it. |

Workspace tests at P6-C close: **235 passed + 1 ignored**.

## Forbidden in P6-C

- Any `.rs` change in `crates/execution/`, `crates/app/`, `crates/signer/`, `crates/relay-sim/`, `crates/relay-clients/src/` (no production code modification — including no doc-comment `//!` additions in v0.2; the v0.1 "optional D-C4" doc-comment edit is **explicitly forbidden** in v0.2 to remove the v0.1 contradiction Codex flagged).
- Any new `hex` crate addition or `hex::decode` usage (fixture uses paired raw-byte-array + lowercase-hex-string constants, no decode step).
- Any test function name or comment that claims the fixture bytes are "RLP" / "RLP test vector" / "test-vector RLP". The bytes are explicitly "placeholder bytes" — no RLP claim.
- Any new `eth_sendBundle` reference anywhere.
- Any live-network test code, env-gated or otherwise.
- Any signer-symbol use (`Signer`, `DisabledSigner`, `SignerError`, `SignerDisabled`) in the new tests.
- Any `secp256k1` / `k256` / `alloy-signer` / `ethers-signers` / `Wallet` / `PrivateKey` / `sign_transaction` / `funded` symbol additions.
- Any private key material in the fixture (the fixture is a synthetic placeholder byte sequence; explicitly NOT RLP-encoded; explicitly NO key derivation).
- Any new `RelaySimError` variant.
- Any new Cargo dep / dev-dep / feature flag.
- Any `submit_bundle` caller in `crates/app/src/` (G3 stays 0).
- Any `live_send = true` capability.
- Any new `#[ignore]` test.
- Any ADR amendment.
- Any edit to `docs/specs/execution-safety.md` or `docs/specs/phase-6a-boundary.md`.
- Any `.claude/` / `AGENTS.md` / `fixture_output.txt` / `hook_toast.md` staging.
- Any destructive git or force-push.
- Any asset / V3-fee-tier / venue widening.
- Any new `Cargo.toml` modification — `crates/relay-clients/Cargo.toml` already has all needed deps.

## Plan execution checklist (TDD-style)

- [ ] **Step 1: Confirm predecessor state.** `git log --oneline -5` shows `a7367b7` HEAD; workspace 233 passed + 1 ignored; G1..G11 all green at P6-B close.
- [ ] **Step 2: Red — write D-T-C1 first.** Add `rc_f_6_call_bundle_body_shape_matches_with_placeholder_bytes` to `crates/relay-clients/tests/flashbots.rs` per the v0.2 §D-C2 body (permissive happy-path mock + `received_requests()` + exact `serde_json::Value` equality + paired `FIXTURE_PLACEHOLDER_BYTES` / `FIXTURE_PLACEHOLDER_HEX` constants). Test fails initially because the expected `serde_json::Value` does not yet match (or test setup is incomplete). Confirm red state.
- [ ] **Step 3: Green — D-T-C1.** Tune the `expected` JSON value to exactly what `call_bundle.rs` produces. Test passes.
- [ ] **Step 4: Red → green — D-T-C2.** Mirror for `crates/relay-clients/tests/bloxroute.rs` `rc_b_6_call_bundle_body_shape_matches_with_placeholder_bytes`. Test passes.
- [ ] **Step 5 (v0.2): D-C4 dropped.** No source-file doc-comment edit in P6-C; skip entirely. (Explicit boundary-spec cross-link can land in a separate doc-only batch later if Codex wants it.)
- [ ] **Step 6: Full gate set.** Workspace `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (expect 235 passed + 1 ignored), `cargo deny check`, `cargo tree -d`, and all G1..G11 ripgrep gates from §"Gates at P6-C close".
- [ ] **Step 7: Commit + push** the two new tests as a single routine `test(p6-c)` commit. Suggested message: `test(p6-c): wiremock body-shape assertions for eth_callBundle on Flashbots + bloXroute adapters`. (No `crates/relay-clients/src/` modification — test-only commit.)
- [ ] **Step 8: Update `.coordination/codex_review.md` + `.coordination/claude_outbox.md`** with the P6-C closeout report; emit P6-D pre-impl plan draft.

## Risks + open questions

- **Q-C1 — Drop the runtime end-to-end test from overview-listed P6-C scope?** v0.2: still **recommend YES (drop)**. Property is structurally guaranteed by G3 + G11 + §3 PRECEDENCE at HEAD `a7367b7`; production code has no path that invokes `Signer::sign_tx` in Phase 6a, so a runtime e2e is impossible. Codex verdict?
- **Q-C2 — Fixture: inline byte literal vs committed file under `tests/fixtures/`?** v0.2: **inline locked**. No `tests/fixtures/` directory.
- **Q-C3 — Fixture bytes content: synthesized vs published EIP test vector?** v0.2: **synthesized locked** (paired `FIXTURE_PLACEHOLDER_BYTES` + `FIXTURE_PLACEHOLDER_HEX` constants; deliberately NOT a valid RLP-encoded signed transaction; explicitly labeled "placeholder bytes"). Test names + comments do NOT claim "RLP" or "test-vector RLP".
- **Q-C4 — `body_partial_json` vs `received_requests` introspection?** v0.2: **`received_requests()` + exact `serde_json::Value` equality locked**. `body_partial_json` cannot enforce field-absence (Codex v0.1 verdict item 2); exact equality on a full `serde_json::Value` enforces both presence and absence of every key, satisfying Q-C6 strictness.
- **Q-C5 — Test naming: extend existing `rc_f_N` / `rc_b_N` numbering vs new prefix?** v0.2: **extend** with `rc_f_6_call_bundle_body_shape_matches_with_placeholder_bytes` + `rc_b_6_call_bundle_body_shape_matches_with_placeholder_bytes`. Name explicitly says "placeholder_bytes", NOT "RLP" / "fixture_rlp".
- **Q-C6 — Should the body-shape matcher also assert absence of fields the adapter does NOT send?** v0.2: **YES**, satisfied via exact `serde_json::Value` equality. The `expected` JSON contains only the keys the adapter actually sends; any future addition of `coinbase` / `gas_price` / `tx_index` etc. will cause `assert_eq!(parsed, expected)` to fail and force an explicit test update.
- **Q-C7 — Two separate tests (one per adapter) vs one parameterized test?** Recommend **two separate tests** (parity with existing pattern). Both test files duplicate the fixture constants verbatim — no shared `common::` helper module added in v0.2 (matches existing relay-clients test-file structure where each test file is self-contained). Codex verdict?

## Process

Per the 2026-05-04 routine-closeout policy + the overview §Process:

1. Claude writes this pre-impl plan to disk (UNCOMMITTED) + emits the review pack to `.coordination/claude_outbox.md`.
2. Claude STOPS and reports "manual Codex review required". **No `.rs` / `Cargo.toml` / ADR / docs/specs edits in this turn.**
3. User pastes pack to Codex; relays verdict.
4. Claude records verdict in `.coordination/codex_review.md`.
5. **APPROVED** → commit + push this plan as a routine doc commit; THEN execute per §"Plan execution checklist"; THEN commit + push the impl; THEN draft P6-D pre-impl plan.
6. **REVISION REQUIRED** → revise plan in place + re-emit pack.
7. **Scope / ADR change required** → HALT to user.
