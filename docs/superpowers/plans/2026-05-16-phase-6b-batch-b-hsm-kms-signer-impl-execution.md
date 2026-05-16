# Phase 6b Batch B -- HSM/KMS-backed Signer stub + structured audit log-source event (planning only)

**Date:** 2026-05-16 KST
**Status:** Draft v0.5 (revised after Codex REVISION REQUIRED HIGH on v0.4, 2026-05-16 KST). Four v0.4 -> v0.5 fixes (strict text-consistency + scope-honesty):

(I) **Stale `144-byte` wording purged from live-spec positions.** The locked hash input is **136 bytes** everywhere in the plan body. Remaining `144-byte` mentions live ONLY inside historical changelog context (this v0.5 changelog block describing what v0.4 fixed; the carried-verbatim v0.3 -> v0.4 changelog below). Plan-body live-spec hits = 0. Permitted per Codex v0.4 item 1 "except historical changelog context".

(II) **Overclaim language purged throughout the plan**: title, Scope, D-B2 header + body, Q-B2, and the "Why P6B-B exists" section all reframed to say P6B-B ships the **log-source piece only** of the Section 2.5 candidate #4 control. v0.5 explicitly: P6B-B does NOT, by itself, fully wire the Section 2.5 host-compromise control + does NOT satisfy the Section 2.5 hard minimum.

(III) **Explicit "P6B-B does NOT satisfy Section 2.5 hard minimum at close"** statement landed in Scope, D-B2 body, the new "Phase 6b overview prerequisite #5 / #6 split" section, and Q-B2.

(IV) **NEW section "Phase 6b overview prerequisite #5 / #6 split"** added to plan body (between Scope and "Why P6B-B exists"). States explicitly:
- Overview v0.2 prerequisite #5 (Section 2 Codex review) PARTIALLY satisfied at P6B-B pre-impl review; operationally-meaningful Section 2 review (against an impl that can return `Ok`) DEFERRED to P6B-C.
- Overview v0.2 prerequisite #6 (non-trivial host-compromise control wired in P6B-B) PARTIALLY satisfied at P6B-B close (log-source piece only); full satisfaction lands at P6B-C close.
- P6B-B closeout report MUST explicitly state #5 / #6 NOT fully satisfied at P6B-B close.
- **P6B-C close-blocker contract LOCKED at v0.5**: P6B-C MUST land BOTH (a) HSM/KMS SDK + signing-call wiring AND (b) operator-visible dashboard + alert-threshold surface. Omitting (b) = REVISION REQUIRED. `sign_tx` returning `Ok(_)` is FORBIDDEN until (b) lands.

Awaiting manual Codex re-review.

**v0.3 -> v0.4 changelog retained verbatim below for traceability.** Three v0.3 -> v0.4 fixes:

(A) **Serde / Default implementation LOCKED.** v0.3 said `#[serde(default)]` on the new `Profile` and `KeyBackend` fields without specifying how the default is sourced. Inspection of `crates/config/src/lib.rs`: `Config` is `#[serde(deny_unknown_fields)]` with NO `impl Default for Config`; `RelayConfig` is `#[serde(deny_unknown_fields, default)]` with explicit `impl Default for RelayConfig` at line 290. v0.4 LOCKS the exact defaulting mechanism:
- ADD `impl Default for Profile { fn default() -> Self { Profile::Dev } }` (type-level Default; enables `#[serde(default)]` on the `Config::active_profile` field to produce `Profile::Dev` when absent from TOML).
- ADD `impl Default for KeyBackend { fn default() -> Self { KeyBackend::Disabled } }` (type-level Default; required because `RelayConfig` has struct-level `#[serde(default)]` which routes through `RelayConfig::default()`, and the manual `impl Default for RelayConfig` body must initialize the new field).
- UPDATE the existing `impl Default for RelayConfig` at line 290 to include the two new fields in the struct literal:
  ```text
  // EXISTING body keeps enabled_relays/simulate_timeout_ms/live_send/execution_disabled fields.
  // v0.4 ADDS:
  key_backend: KeyBackend::Disabled,
  audit_key_id: String::new(),
  ```
  No removal of existing default fields; strictly additive.
- ADD `#[serde(default)]` on the `pub active_profile: Profile` field declaration in `Config` (since `Config` has no struct-level Default; per-field default routes through `Profile::Default` impl above).
- The `pub key_backend: KeyBackend` + `pub audit_key_id: String` fields on `RelayConfig` do NOT need per-field `#[serde(default)]` because the struct-level `#[serde(deny_unknown_fields, default)]` already routes missing fields through `RelayConfig::default()` (which v0.4 updates to include them).

(B) **Hash byte count corrected to 136 bytes.** v0.3 D-B2 stated "Total input length: 144 bytes". Re-counting: `from (20) + to (20) + value_wei (32) + gas_limit (8) + nonce (8) + chain_id (8) + bundle_correlation_id (8) + keccak256(data) (32) = 136 bytes`. The "144" was an arithmetic error; the field set is unchanged. v0.4 corrects D-B2 spec to "136 bytes". The hash itself is still a deterministic 32-byte keccak256 digest.

(C) **Section 2.5 satisfaction claim corrected -- log-source piece only at P6B-B; operator-visible dashboard surface DEFERRED.** v0.3 claimed P6B-B's `tracing::info!` emission "satisfies the Section 2.5 hard minimum". Codex correctly flagged: `production-signer.md` Section 2.5 candidate #4 reads "Operator-visible signing audit log. Every signing attempt linked to the opportunity/bundle chain per Section 2.3, **surfaced in an operator dashboard with a configurable alert threshold**." A bare `tracing::info!` event is the LOG SOURCE; the operator-visible DASHBOARD + ALERT-THRESHOLD surface is separate observability infrastructure that P6B-B does NOT ship. v0.4 reframes accurately:
- P6B-B ships ONLY the **structured log-source event** (`tracing::info!` with target `"production_signer_audit"` + the locked field set). This piece is reviewable + unit-testable at the signer-module level.
- The **operator-visible dashboard + alert-threshold surface** is **DEFERRED** to **P6B-C** (or a Phase 6b infrastructure sub-batch named at P6B-C plan time). P6B-C is the appropriate target because:
  - P6B-C is when the HSM/KMS SDK actually connects and `sign_tx` can transition from `Err(NotConfigured)` to `Ok(SignedTxBytes)` -- i.e., when real audit-log events with `outcome="ok"` (or HSM/KMS errors) first start emitting at runtime.
  - The `production-signer.md` Section 2.5 hard minimum reads "MUST add at minimum one of the following non-trivial host-compromise controls **before any production signer replaces `DisabledSigner`**". At P6B-B close, `ProductionSigner` is reachable (via the `KeyBackend::HsmKms` config path) but `sign_tx` only returns `Err(NotConfigured)` -- no signing can happen, so the host-compromise risk is structurally zero. The full Section 2.5 hard minimum (log-source + operator-visible surface) MUST be in place before P6B-C close (when `sign_tx` can return `Ok`).
- v0.4 explicitly states: **at P6B-B close, the Section 2.5 hard minimum is PARTIALLY satisfied (log-source only); P6B-C close is the latest point at which the operator-visible dashboard surface MUST land**. P6B-C's pre-impl plan MUST include the operator-dashboard deliverable as a prerequisite for that batch closing (not just the HSM/KMS SDK + signing-call wiring).
- This framing preserves the safety story: no live signing can happen between P6B-B close and P6B-C close because `sign_tx` returns `Err(NotConfigured)` throughout that interval. The full Section 2.5 hard minimum is in force the moment real signing becomes possible.

Awaiting manual Codex re-review.

**v0.2 -> v0.3 changelog retained verbatim below for traceability.** Five v0.2 -> v0.3 fixes:

(1) **G2c/G2d allow-list strictness preserved via RENAME** to avoid `Signer` substring outside approved files. v0.2 added `SignerKind::DisabledSigner` to `crates/config/src/lib.rs`, which would have added `Signer` symbol hits there + forced the allow-list to extend by an additional file. Codex correctly flagged: "Keep the allow-list exact by avoiding uppercase `Signer` symbols outside approved files". v0.3 renames at the type-system level:
- `SignerKind` enum -> `KeyBackend` (no "Signer" substring).
- Variant `DisabledSigner` -> `Disabled` (no "Signer" substring).
- Variant `HsmKms` UNCHANGED (no "Signer" substring).
- Field `RelayConfig.signer_kind` -> `RelayConfig.key_backend`.
- Field `RelayConfig.signer_audit_key_id` -> `RelayConfig.audit_key_id`.
- `ConfigError` variant names: `ProductionProfileRequiresHsmKms` + `HsmKmsRequiresProductionProfile` + (NEW item 5) `HsmKmsRequiresNonEmptyAuditKeyId`. All three avoid "Signer" substring.
- `Profile` enum UNCHANGED (no "Signer" substring).
- Result: `crates/config/src/lib.rs` stays OUT of the G2c/G2d allow-list; the v0.2 expectation of "extends by exactly 1 file" is preserved -- the ONE new file is `crates/signer/src/production.rs`.

(2) **App callsite LOCKED to `crates/app/src/lib.rs::run` (line 174-185), NOT `main.rs`.** Codex correctly flagged: the existing P6-B-approved signer construction is at `crates/app/src/lib.rs:184` inside `pub fn run(config_path: ...)`, where line 184 currently reads `let signer: Arc<dyn Signer> = Arc::new(DisabledSigner);`. v0.3 LOCKS the v0.2 "caller-side wiring" to this exact callsite. `crates/app/src/lib.rs` is already in the G2c/G2d allow-list (P6-B approved). The construction site becomes a match arm on `config.relay.key_backend`. The use statement at `crates/app/src/lib.rs:30` extends from `use rust_lmax_mev_signer::{DisabledSigner, Signer};` to `use rust_lmax_mev_signer::{DisabledSigner, ProductionSigner, Signer};`. `main.rs` remains UNTOUCHED.

(3) **Cargo-dependency contradiction resolved.** v0.2 claimed "no new Cargo dep at all in P6B-B" but `ProductionSigner::sign_tx` uses `tracing::info!` (for the audit-log control) and D-T-B1 captures tracing events (which requires a `tracing-subscriber` Layer at test time). `crates/signer/Cargo.toml` currently has only `alloy-primitives`, `async-trait`, `thiserror` as `[dependencies]` and `tokio` as `[dev-dependencies]`. Neither `tracing` nor `tracing-subscriber` is present. v0.3 LOCKS the Cargo additions:
- `crates/signer/Cargo.toml` `[dependencies]`: add `tracing = { workspace = true }` (workspace dep already declared at root `Cargo.toml:53` as `tracing = "0.1"`).
- `crates/signer/Cargo.toml` `[dev-dependencies]`: add `tracing-subscriber = { workspace = true }` (workspace dep already declared at root `Cargo.toml:54` with features `["json", "env-filter"]`; the dev-dependency inherits the workspace features and is sufficient for D-T-B1 / D-T-B2 capture).
- Neither dep is in the G2b banned set (`alloy-signer`, `ethers-signers`, `secp256k1`, `k256`); G2b stays at 0 hits.
- Neither dep changes the workspace dep graph cyclicity (both are leaf-utility crates already in widespread use); G8 stays clean.

(4) **Audit-log fields LOCKED to actual `BundleTx` shape.** v0.2 used `tx.opportunity_id` and `BundleTx::signed_payload()`, neither of which exists. Inspection of `crates/signer/src/bundle_tx.rs` confirms the actual fields are: `from: Address`, `to: Address`, `value_wei: U256`, `data: Vec<u8>`, `gas_limit: u64`, `nonce: u64`, `chain_id: u64`, `bundle_correlation_id: u64`. v0.3 LOCKS the audit-log field set + the deterministic hash input:
- **Audit-log structured fields emitted by `tracing::info!`**:
  - `bundle_correlation_id: u64` (from `BundleTx::bundle_correlation_id`; the existing P5-C DP-C3 cross-link to the `EventEnvelope::correlation_id` chain).
  - `bundle_artifact_hash: String` (hex-encoded deterministic 32-byte hash; computed inline; specification below).
  - `outcome: &'static str` (`"not_configured"` at P6B-B close).
  - `audit_key_id: String` (from `ProductionSigner.audit_key_id`).
  - `chain_id: u64` (from `BundleTx::chain_id`; safe to log -- public on-chain context, not secret).
  - `nonce: u64` (from `BundleTx::nonce`; safe to log -- per-account public counter, not secret).
  - Event name (the message arg): `"signer_sign_tx_attempt"`.
  - Tracing target: `"production_signer_audit"`.
- **Deterministic content-hash specification.** `bundle_artifact_hash` is the hex-encoded `alloy_primitives::keccak256` of the byte concatenation: `from (20 bytes) || to (20 bytes) || value_wei.to_be_bytes::<32>() || gas_limit.to_be_bytes() (8 bytes) || nonce.to_be_bytes() (8 bytes) || chain_id.to_be_bytes() (8 bytes) || bundle_correlation_id.to_be_bytes() (8 bytes) || keccak256(data) (32 bytes)`. **Raw `data` bytes are NOT logged**; only `keccak256(data)` enters the artifact-hash. `alloy_primitives::keccak256` is already in the `crates/signer/` dependency tree (transitively via `alloy-primitives`); no new dep. Note: `keccak256` does NOT trigger G2a `\bk256\b` per the P6-B D-B0 word-boundary fix (verified: the substring `k256` inside `keccak256` lacks a word boundary before `k`).
- **Fields explicitly NOT logged**: `from` / `to` / `data` raw bytes (potential transaction-pattern fingerprint surface; only enter the artifact-hash as inputs); `value_wei` (could leak strategy bid-size). Conservative redaction-by-omission: only fields with established public-on-chain semantics (`chain_id`, `nonce`) leak into the audit log directly; everything else is keccak-folded.

(5) **NEW non-empty `audit_key_id` validation when `key_backend == HsmKms`.** v0.2 left `audit_key_id` as a plain `String` with default `""`; if `Production + HsmKms` validates but `audit_key_id` is empty, the operator-visible signing audit-log control loses its "audit-safe key identifier" field (Section 2.4 of `production-signer.md`). v0.3 LOCKS a THIRD validation rule: `if config.relay.key_backend == KeyBackend::HsmKms && config.relay.audit_key_id.trim().is_empty() { return Err(ConfigError::HsmKmsRequiresNonEmptyAuditKeyId); }` with Display literal `"key_backend=HsmKms requires non-empty audit_key_id"`. D-T-B4 extends to cover this third reject case (single test, now 5 illegal sub-cases instead of 4).

Awaiting manual Codex re-review.

**Predecessors:**

- `phase-6a-complete` annotated tag at `bd0a53c` (tag object `3c9faaf`).
- Phase 6b overview v0.2 APPROVED HIGH at `49123e9`.
- P6B-A pre-impl plan v0.3 APPROVED HIGH at `2ddba8a`; impl at `1c490de`.
- `master` HEAD `1c490de`.
- Workspace baseline (inherited): **239 passed + 1 ignored**.

## Scope

P6B-B adds a **no-external-SDK structural stub** for the production `Signer` impl module inside `crates/signer/`. The module implements the `Signer` trait (returning `Err(SignerError::NotConfigured)` always at P6B-B close), holds the field shape for future HSM/KMS connection (added in P6B-C), ships the **structured log-source event piece** (`tracing::info!` with the locked field set) of the Section 2.5 candidate #4 host-compromise control, and gates reachability on a NEW `Profile::Production` config field paired with a NEW `KeyBackend::HsmKms` setting. NO HSM/KMS client library dep. NO live HSM/KMS connection. NO actual signing. **P6B-B does NOT ship the operator-visible dashboard + alert-threshold surface**, so the full Section 2.5 host-compromise control is **NOT** fully wired at P6B-B close (only the log-source piece is). The operator-visible surface is **DEFERRED to P6B-C** as a P6B-C close blocker -- see Section "Phase 6b overview prerequisite #5/#6 split" below.

**P6B-B itself unlocks NO live submission.** After P6B-B lands:

- `submit_bundle` continues to return `Err(KillSwitchActive)` (KS active) or `Err(SubmitDisabled)` (KS inactive) in every adapter. G3 + G4 stay 0 in `crates/app/src/`.
- `live_send=true` config-validation reject stays in force for ALL profiles (HSI-8 unchanged at P6B-B close). The flip happens only in P6B-D.
- The new production `Signer` impl is reachable ONLY via the existing P6-B `BundleConstructor::with_signer` boundary, and even then `Signer::sign_tx` returns `Err(NotConfigured)` (stub). HSI-11 G11 production-runtime-callsite count stays at 1.
- NO funded key. NO HSM/KMS connection. NO HSM/KMS SDK Cargo dep.

**Phase 6b non-goals NOT touched in P6B-B:**

- NO HSM/KMS client library dep added (deferred to P6B-C).
- NO funded private key material. NO env-example with a key. NO config-example with a key. NO fixture with a key. NO test that loads a key.
- NO `live_send=true` enablement.
- NO `eth_sendBundle` runtime path.
- NO actual relay submission.
- NO live-network test enabled by default.
- NO paid live API in CI.
- NO `docs/specs/` edit. NO `docs/adr/` edit.
- NO change to `crates/signer/src/signer_trait.rs` (`Signer` trait surface stays locked at P5-C).
- NO change to `crates/signer/src/disabled.rs` (`DisabledSigner` remains the default impl).
- NO change to `crates/signer/src/bundle_tx.rs` (`BundleTx` surface unchanged; v0.3 audit-log uses ONLY existing fields).
- NO drop of `Copy` derive on `SignerError`.
- NO change to `wire_phase4` signature (`Arc<dyn Signer>` parameter unchanged).
- NO change to `crates/app/tests/wire_phase4.rs`.
- NO change to `crates/app/src/main.rs`.
- NO asset / venue / V3-fee-tier widening.

## Phase 6b overview prerequisite #5 / #6 split (DELIBERATE; LOCKED at v0.5)

The Phase 6b overview v0.2 (`49123e9`) prerequisite table item #5 reads "A Codex review against `docs/specs/production-signer.md` Section 2 contract ... Reviewer verifies that the proposed production `Signer` impl satisfies every Section 2 requirement before it can replace `DisabledSigner`. Satisfied at the P6B-B pre-impl review." Item #6 reads "At least one non-trivial host-compromise control per `docs/specs/production-signer.md` Section 2.5 residual. ... Selected in the P6B-B pre-impl plan + landed as code in P6B-B itself."

**P6B-B v0.5 does NOT satisfy items #5 / #6 in full.** The Codex v0.3 -> v0.4 verdict (item C, on the live-overclaim wording) + the Codex v0.4 -> v0.5 verdict (item 4, demanding an explicit prereq-split acknowledgement) jointly forced the recognition that:

- **Item #5 (Section 2 review)**: The Codex pre-impl review of this plan IS the Section 2 review for the PROPOSED impl shape; that piece is satisfied at this review cycle. However, item #5 also says "verifies that the proposed production `Signer` impl satisfies every Section 2 requirement BEFORE it can replace `DisabledSigner`". P6B-B's impl returns `Err(NotConfigured)` always; it cannot replace `DisabledSigner` in any operational sense at P6B-B close. The full Section 2 review (against an impl that can return `Ok(SignedTxBytes)`) is **DEFERRED to P6B-C** -- P6B-C's pre-impl review is when the HSM/KMS SDK + signing-call wiring is reviewed against Section 2 in operationally-meaningful form.
- **Item #6 (host-compromise control wired)**: P6B-B ships only the log-source piece of Section 2.5 candidate #4. The operator-visible dashboard + alert-threshold surface is **DEFERRED to P6B-C** as a P6B-C close blocker. The full item #6 satisfaction (non-trivial host-compromise control wired in full) lands at P6B-C close, not P6B-B close.

**Closeout claim discipline at P6B-B**: the P6B-B closeout report MUST explicitly state that prerequisites #5 / #6 are NOT fully satisfied at P6B-B close. The closeout MUST NOT claim Phase 6b is moving forward against the overview prerequisite table as if these items were ticked. The closeout report names the P6B-C deliverables that finish the split.

**P6B-C close-blocker contract LOCKED at v0.5**: P6B-C MUST land BOTH:
1. The HSM/KMS SDK + signing-call wiring (so `sign_tx` can return `Ok(SignedTxBytes)`).
2. The operator-visible dashboard + alert-threshold surface for the Section 2.5 candidate #4 control.

If P6B-C's pre-impl plan omits (2), the P6B-C pre-impl review MUST be REVISION REQUIRED. P6B-C close-time `sign_tx` returning `Ok(_)` is FORBIDDEN until (2) lands.

**Safety story preserved**: at P6B-B close, `sign_tx` returns `Err(NotConfigured)` unconditionally. The host-compromise risk that Section 2.5 candidate #4 mitigates is structurally zero at P6B-B close because no signing can happen. The full Section 2.5 hard minimum (log-source + operator-visible surface) is in force at P6B-C close, which is the first point real signing becomes possible.

## Why P6B-B exists (as a stub)

The Phase 6b overview v0.2 names P6B-B as the batch that lands the HSM/KMS-backed production `Signer` impl + at least one host-compromise control. Codex v0.1 flagged that without vendor + SDK lock at plan time, G2b reviewability + the pre-sign-comparator-gating feasibility broke. v0.2 split the work:

- **P6B-B (this plan): no-SDK structural stub.** Lands module structure + **log-source piece only** of the Section 2.5 candidate #4 control (operator-visible dashboard + alert-threshold surface DEFERRED to P6B-C) + config-gating + new `SignerError` variant + `Profile` + `KeyBackend` + bidirectional + non-empty-audit-key-id rejects. Reviewable now. **P6B-B by itself does NOT fully wire the Section 2.5 host-compromise control + does NOT satisfy the Section 2.5 hard minimum** (see "Phase 6b overview prerequisite #5 / #6 split" above).
- **P6B-C (future): HSM/KMS SDK integration + funded-key wiring.** Adds the HSM/KMS client library dep (Codex-reviewed against G2b at P6B-C plan time), switches `sign_tx` from `Err(NotConfigured)` to `Ok(SignedTxBytes)` under proper config.
- **P6B-E (future): live submission + pre-sign mismatch-comparator gating.** The G12 chain INHERITING G13 enforces pre-sign comparator + bundle-byte equality at the runtime callsite (the only structural point where comparator outputs are available).

## Deliverables

### D-B1 -- NEW `crates/signer/src/production.rs` (no-SDK structural stub)

NEW file `crates/signer/src/production.rs`:

- Declares `pub struct ProductionSigner { audit_key_id: String }` (only field at P6B-B close; future HSM/KMS connection fields added in P6B-C).
- `pub fn new(audit_key_id: String) -> Self` constructor (no validation here -- empty `audit_key_id` is caught by `crates/config/src/lib.rs` validation D-B4 below before this ctor is reached).
- Implements `crates/signer::Signer` trait per the P5-C surface (`BundleTx -> Result<SignedTxBytes, SignerError>`). At P6B-B close, the impl body emits the audit-log event (D-B2) then returns `Err(SignerError::NotConfigured)`.
- NO raw key bytes. NO `[u8; 32]` private-key field. NO `SecretKey`-style type. NO key fingerprint as a struct field.
- ASCII-only file content. NO forbidden symbols (`Wallet` / `PrivateKey` / `secp256k1` / `\bk256\b` (word-boundary; `keccak256` substring not matched) / `alloy-signer` / `ethers-signers` / `sign_transaction` / `funded`).
- Re-exported from `crates/signer/src/lib.rs` as `pub mod production;` + `pub use production::ProductionSigner;`.

### D-B2 -- Structured log-source event (PARTIAL host-compromise control; log-source piece only of Section 2.5 candidate #4)

The audit-log emission inside `ProductionSigner::sign_tx` ships the **log-source piece** of the Section 2.5 candidate #4 ("Operator-visible signing audit log") host-compromise control. The full Section 2.5 candidate #4 also requires "surfaced in an operator dashboard with a configurable alert threshold" -- that operator-visible dashboard + alert-threshold surface is **DEFERRED to P6B-C** and is a P6B-C close blocker before `sign_tx` may ever return `Ok(_)`. **P6B-B does NOT, by itself, fully wire the Section 2.5 host-compromise control. P6B-B does NOT, by itself, satisfy the Section 2.5 hard minimum.**

The log-source spec for what P6B-B does land:

- **Emission mechanism.** `tracing::info!(target = "production_signer_audit", event = "signer_sign_tx_attempt", bundle_correlation_id = tx.bundle_correlation_id, bundle_artifact_hash = %hex_hash, outcome = "not_configured", audit_key_id = %self.audit_key_id, chain_id = tx.chain_id, nonce = tx.nonce);` (exact macro form locked at impl time; the field set is locked here).
- **Required structured fields (LOCKED v0.3)**:
  - `bundle_correlation_id: u64` -- from `BundleTx::bundle_correlation_id`. The P5-C DP-C3 cross-link to the `EventEnvelope::correlation_id` chain.
  - `bundle_artifact_hash: String` -- hex-encoded deterministic 32-byte hash, computed inline per D-B2 hash specification below.
  - `outcome: &'static str` -- `"not_configured"` at P6B-B close (stub return). P6B-C+ extends to `"ok"` / `"hsm_error"` / etc.
  - `audit_key_id: String` -- from `ProductionSigner.audit_key_id` (the audit-safe identifier per `production-signer.md` Section 2.4).
  - `chain_id: u64` -- from `BundleTx::chain_id`. Public on-chain context; not secret.
  - `nonce: u64` -- from `BundleTx::nonce`. Public on-chain per-account counter; not secret.
- **Hash specification.** `bundle_artifact_hash` is the hex-encoded `alloy_primitives::keccak256` of the byte concatenation:
  ```text
  from        (20 bytes; Address::as_slice())
  || to       (20 bytes)
  || value_wei.to_be_bytes::<32>()  (32 bytes; U256 big-endian)
  || gas_limit.to_be_bytes()        (8 bytes; u64 big-endian)
  || nonce.to_be_bytes()            (8 bytes)
  || chain_id.to_be_bytes()         (8 bytes)
  || bundle_correlation_id.to_be_bytes()  (8 bytes)
  || keccak256(data)                (32 bytes; pre-hash so raw data does not enter the hash input)
  ```
  Total input length: **136 bytes** (= 20 + 20 + 32 + 8 + 8 + 8 + 8 + 32) -> single keccak256 -> 32-byte digest -> hex-encoded. The `keccak256` function comes from `alloy_primitives::keccak256` which is already in `crates/signer/`'s dependency tree (transitively via `alloy-primitives`); no new dep. `keccak256` does NOT trigger G2a `\bk256\b` per the P6-B D-B0 word-boundary fix.
- **Fields explicitly NOT logged**: `from` / `to` / `data` raw bytes (only enter the artifact-hash as inputs); `value_wei` raw (only enters the artifact-hash). Redaction-by-omission. The hash is deterministic + reproducible by a forensic operator who has access to the journal chain, but the audit log itself does not leak transaction-pattern fingerprint surface beyond the public-on-chain fields (`chain_id`, `nonce`) and the keccak-folded artifact hash.
- **Hard text invariants (per `production-signer.md` Section 2.3(b))**. The event MUST NOT contain: private-key bytes (none in `BundleTx` anyway); key derivative; key fingerprint; derivable secret; HSM/KMS-internal handle; `api_key`-like field. Verified by D-T-B2.

### D-B3 -- G2c/G2d allow-list extension by EXACTLY ONE file

Allow-list extends by EXACTLY one new file at P6B-B close: `crates/signer/src/production.rs`. No vendor submodule. No additional approved file.

**Why config / app stay out of the allow-list extension:** v0.3 renames `SignerKind` -> `KeyBackend`, `DisabledSigner` (variant) -> `Disabled`, `signer_kind` -> `key_backend`, `signer_audit_key_id` -> `audit_key_id`. None of these names contain the substring `Signer`. The `ConfigError` variants `ProductionProfileRequiresHsmKms`, `HsmKmsRequiresProductionProfile`, `HsmKmsRequiresNonEmptyAuditKeyId` also avoid `Signer`. The app callsite at `crates/app/src/lib.rs:184` already has `Signer` hits (existing P6-B approved file); extending it to also reference `ProductionSigner` keeps the symbol concentration inside an already-approved file.

Allow-list at P6B-B close: **9 files exactly** = 5 baseline `crates/signer/` + 1 P6B-B new (`crates/signer/src/production.rs`) + 3 P6-B approved (`crates/execution/src/lib.rs`, `crates/app/src/lib.rs`, `crates/app/tests/wire_phase4.rs`).

### D-B4 -- `Profile` + `KeyBackend` enums + 3 NEW `Config` fields + 3 NEW `ConfigError` variants + 3 NEW validation rules + caller-side wiring at `crates/app/src/lib.rs::run`

**`crates/config/src/lib.rs` changes (all additive; NO `Signer` substring anywhere):**

1. NEW enum at file scope + explicit `impl Default for Profile`:
   ```text
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
   #[serde(rename_all = "lowercase")]
   pub enum Profile {
       Dev,
       Test,
       Shadow,
       Production,
   }

   impl Default for Profile {
       fn default() -> Self { Profile::Dev }
   }
   ```
   Default `Profile::Dev` is provided at the TYPE level, so the `Config::active_profile` field declaration uses `#[serde(default)]` (no per-field default-fn pointer needed).

2. NEW enum at file scope + explicit `impl Default for KeyBackend`:
   ```text
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
   #[serde(rename_all = "lowercase")]
   pub enum KeyBackend {
       Disabled,
       HsmKms,
   }

   impl Default for KeyBackend {
       fn default() -> Self { KeyBackend::Disabled }
   }
   ```
   Default `KeyBackend::Disabled` at the TYPE level is REQUIRED because `RelayConfig` has the struct-level `#[serde(deny_unknown_fields, default)]` attribute which routes missing fields through `RelayConfig::default()` -- the manual `impl Default for RelayConfig` body uses `KeyBackend::Disabled` directly (see item 6 below); the `Default` impl makes this idiom-correct. **No `Signer` substring** in enum name or variant names.

3. NEW field `pub active_profile: Profile` on top-level `Config` (line 35; sibling to `node`, `journal`, `relay`, etc.). Per-field `#[serde(default)]` because `Config` has NO struct-level `default` attribute (only `#[serde(deny_unknown_fields)]`). The per-field default routes through `Profile::default()` -> `Profile::Dev`.

4. NEW field `pub key_backend: KeyBackend` on `RelayConfig` (line 270; sibling to `live_send`, `enabled_relays`). NO per-field `#[serde(default)]` needed (covered by the struct-level `#[serde(deny_unknown_fields, default)]` already on `RelayConfig` -- missing fields route through `RelayConfig::default()`).

5. NEW field `pub audit_key_id: String` on `RelayConfig`. Same covered-by-struct-level-default logic as item 4. `String::default()` is `""`. (Empty valid for `Disabled`; rejected for `HsmKms` per rule 3 in item 8 below.)

6. UPDATE the existing `impl Default for RelayConfig` at line 290 to include the two new fields in the struct literal:
   ```text
   impl Default for RelayConfig {
       fn default() -> Self {
           Self {
               enabled_relays: Vec::new(),     // existing
               simulate_timeout_ms: 2_000,     // existing
               live_send: false,               // existing
               execution_disabled: false,      // existing
               key_backend: KeyBackend::Disabled,  // v0.4 NEW
               audit_key_id: String::new(),        // v0.4 NEW
           }
       }
   }
   ```
   Strictly additive; no removal of existing default fields. (`Config` does NOT have an `impl Default`, so no equivalent update is needed there; the `active_profile` field's `#[serde(default)]` is sufficient.)

7. THREE NEW `ConfigError` variants (extending `pub enum ConfigError` at line 363, all `#[error("...")]` derived, all payload-free):
   - `ProductionProfileRequiresHsmKms` with Display `"Production profile requires key_backend=HsmKms"`.
   - `HsmKmsRequiresProductionProfile` with Display `"key_backend=HsmKms requires Production profile"`.
   - `HsmKmsRequiresNonEmptyAuditKeyId` with Display `"key_backend=HsmKms requires non-empty audit_key_id"`.

8. THREE NEW validation rules inside the existing `Config::validate()` (or equivalent function called at startup; exact callsite enumerated at impl time):
   - If `config.active_profile == Profile::Production && config.relay.key_backend != KeyBackend::HsmKms` -> `Err(ConfigError::ProductionProfileRequiresHsmKms)`.
   - If `config.relay.key_backend == KeyBackend::HsmKms && config.active_profile != Profile::Production` -> `Err(ConfigError::HsmKmsRequiresProductionProfile)`.
   - If `config.relay.key_backend == KeyBackend::HsmKms && config.relay.audit_key_id.trim().is_empty()` -> `Err(ConfigError::HsmKmsRequiresNonEmptyAuditKeyId)`.

**`crates/app/src/lib.rs` changes (additive only; allow-listed file):**

- Line 30 use-statement extends from `use rust_lmax_mev_signer::{DisabledSigner, Signer};` to `use rust_lmax_mev_signer::{DisabledSigner, ProductionSigner, Signer};`.
- Line 184 (inside `pub fn run`) currently reads `let signer: Arc<dyn Signer> = Arc::new(DisabledSigner);`. v0.3 replaces this single line with a `match config.relay.key_backend` arm:
  ```text
  // Existing line 184 replaced with the match below; wire_phase4 sig UNCHANGED.
  let signer: Arc<dyn Signer> = match config.relay.key_backend {
      rust_lmax_mev_config::KeyBackend::Disabled =>
          Arc::new(DisabledSigner),
      rust_lmax_mev_config::KeyBackend::HsmKms =>
          Arc::new(ProductionSigner::new(config.relay.audit_key_id.clone())),
  };
  let handle = runtime.block_on(wire_phase4(&config, WireOptions::default(), signer))?;
  ```
  (Exact match shape locked at impl time; the structural intent is locked here. The `KeyBackend::HsmKms` arm is reachable ONLY when the bidirectional reject in `Config::validate()` allowed the combo through, which requires `active_profile == Production` AND non-empty `audit_key_id`.)
- NO other change to `crates/app/src/lib.rs`. NO change to `wire_phase4`'s signature. NO change to `main.rs`. NO change to `crates/app/tests/wire_phase4.rs`.

### D-B5 -- `SignerError::NotConfigured` payload-free variant (preserves Copy derive)

Add to `crates/signer/src/error.rs:13` (the existing `#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)] #[non_exhaustive] pub enum SignerError`):

```text
/// P6B-B D-B5: signer is structurally configured (via the
/// `KeyBackend::HsmKms` config arm) but operationally not yet wired
/// to a live HSM/KMS connection. P6B-C wires the HSM/KMS connection
/// and switches this return path to `Ok(SignedTxBytes)` under proper
/// config. Display literal `"signer not configured"` -- ASCII, no
/// key material, no fingerprint, no HSM-internal handle (per
/// `production-signer.md` Section 2.3(b)).
#[error("signer not configured")]
NotConfigured,
```

- Payload-free. Preserves `Copy` derive.
- ASCII Display `"signer not configured"`. No key material.

### D-B6 -- Tests (5 minimal targeted; v0.3 locked count)

| ID | File | Test | Asserts |
|---|---|---|---|
| D-T-B1 | `crates/signer/src/production.rs` `#[cfg(test)] mod tests` | `production_signer_emits_audit_log_event_with_required_fields` | When `ProductionSigner::sign_tx` is invoked with a synthetic `BundleTx` (constructed with `BundleTx::new` from public API), exactly one `tracing` event is emitted with target `"production_signer_audit"` and event message `"signer_sign_tx_attempt"`, carrying structured fields `bundle_correlation_id` (matches `BundleTx::bundle_correlation_id`), `bundle_artifact_hash` (32-byte hex string; deterministic from the BundleTx inputs), `outcome="not_configured"`, `audit_key_id` (matches `ProductionSigner::new` ctor argument), `chain_id`, `nonce`. Verified via a custom `tracing_subscriber::Layer` capture (test-only). |
| D-T-B2 | `crates/signer/src/production.rs` `#[cfg(test)] mod tests` | `production_signer_audit_log_redacts_no_key_material_or_raw_data` | The captured tracing event from D-T-B1 (with a synthetic `BundleTx` whose `data` field contains recognizable secret-shaped placeholder bytes `0xDEADBEEFCAFEF00D` repeated) does NOT contain: the placeholder substring in any form (hex / utf8 / debug); `from` / `to` raw address strings; `value_wei` raw value; the literal substrings `"PrivateKey"` / `"Wallet"` / `"api_key"` / `"sign_transaction"`. The `SignerError::NotConfigured` Display string `"signer not configured"` also asserted to contain no key material. |
| D-T-B3 | `crates/signer/src/production.rs` `#[cfg(test)] mod tests` | `production_signer_sign_tx_always_returns_not_configured_and_signer_error_is_copy` | `ProductionSigner::sign_tx` always returns `Err(SignerError::NotConfigured)` at P6B-B close. Compile-asserts `SignerError: Copy` is preserved via a move-then-reuse pattern: `let e = SignerError::NotConfigured; let _e1 = e; let _e2 = e;`. |
| D-T-B4 | `crates/config/src/lib.rs` `#[cfg(test)] mod tests` | `config_validate_rejects_all_5_illegal_profile_keybackend_audit_combos` | Config validation rejects all FIVE illegal combos with the expected error variant: `(Production, Disabled, _)` -> `ProductionProfileRequiresHsmKms`; `(Dev, HsmKms, _)` + `(Test, HsmKms, _)` + `(Shadow, HsmKms, _)` -> `HsmKmsRequiresProductionProfile`; `(Production, HsmKms, "")` (or whitespace-only) -> `HsmKmsRequiresNonEmptyAuditKeyId`. Single test, 5 sub-cases. |
| D-T-B5 | `crates/config/src/lib.rs` `#[cfg(test)] mod tests` | `profile_and_key_backend_serde_defaults` | Omitting `active_profile` from a TOML config gives `Profile::Dev`; omitting `key_backend` from `[relay]` gives `KeyBackend::Disabled`; omitting `audit_key_id` gives `""`. The `(Dev, Disabled, "")` default triple passes validation. Parallels the existing `live_send: false` default-test pattern. |

**5 new tests.** NO `#[ignore]` added. NO live-network test. G7 stays at 1.

Workspace test total at P6B-B close: **239 + 5 = 244 passed + 1 ignored** (target; verified at P6B-B close via targeted `cargo test -p ...` runs; full workspace run deferred to P6B-F).

### D-B7 -- Static / ripgrep gates at P6B-B close (lean-batching policy; targeted cargo only)

- `cargo fmt --check` on the workspace.
- Targeted `cargo clippy -p rust-lmax-mev-signer -p rust-lmax-mev-config -p rust-lmax-mev-app -- -D warnings`.
- Targeted `cargo test -p rust-lmax-mev-signer -p rust-lmax-mev-config -p rust-lmax-mev-app`. Expected delta: +5 passes vs `phase-6a-complete`. Workspace summation target = 244 + 1 ignored. Full workspace `cargo test --workspace` deferred to P6B-F audit per lean-batching policy.
- G2a: `rg -nE 'Wallet|PrivateKey|secp256k1|\bk256\b|sign_transaction|funded' crates/` -> **0 hits** (HSI-2 UNCHANGED).
- G2b: `rg -n --glob 'crates/**/Cargo.toml' -e 'alloy-signer' -e 'ethers-signers' -e 'secp256k1' -e 'k256'` -> **0 hits** (HSI-3 UNCHANGED; only `tracing` + `tracing-subscriber` added to `crates/signer/Cargo.toml`, neither in banned set).
- G2c/G2d: `rg -n --type rust -e 'Signer' -e 'DisabledSigner' -e 'SignerError' -e 'SignerDisabled' crates/` -> hits in EXACTLY 9 files (5 baseline `crates/signer/` + 1 NEW `crates/signer/src/production.rs` + 3 P6-B approved). **Zero hits outside.** `crates/config/src/lib.rs` MUST return zero matches (verifies the rename strategy from item 1).
- G2e: `rg -n --glob 'crates/**/Cargo.toml' 'signer = \{ path = "../signer" \}'` -> **2 hits UNCHANGED** (production module lives inside `crates/signer/`; no new dep edge into signer from another crate).
- G3 / G4: 0 in `crates/app/src/` (HSI-6 + HSI-7 UNCHANGED).
- G5: `live_send=true` reject UNCHANGED + 3 NEW config rejects verified by D-T-B4.
- G6 / G7 / G8 / G9 / G10: UNCHANGED.
- G11: `rg -n 'sign_tx' crates/execution/src/` -> **1 production call site at `crates/execution/src/lib.rs:238`** UNCHANGED.
- G12 / G13 / G14: UNCHANGED.

### D-B8 -- Cargo deps locked: `tracing` (deps) + `tracing-subscriber` (dev-deps) in `crates/signer/Cargo.toml`

Two NEW lines added to `crates/signer/Cargo.toml`:

- `[dependencies]`: `tracing = { workspace = true }` (workspace dep already present at root `Cargo.toml:53` `tracing = "0.1"`).
- `[dev-dependencies]`: `tracing-subscriber = { workspace = true }` (workspace dep already present at root `Cargo.toml:54` `tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }`).

Neither dep is in the G2b banned set. Both are already in the workspace dep graph (used widely in `crates/app`, `crates/observability`, etc.). G8 stays clean (no cycle). G2e stays at 2 (these deps are tracing crates, NOT path-deps on `signer`).

### D-B9 -- Negative invariants

- NO HSM/KMS client library Cargo dep (deferred to P6B-C).
- NO new workspace crate.
- NO `Wallet` / `PrivateKey` / `secp256k1` / `k256` / `alloy-signer` / `ethers-signers` / `sign_transaction` / `funded` symbol anywhere in `crates/`.
- NO funded private key in repo / tests / fixtures / configs / env-examples / build artifacts / runtime memory.
- NO `live_send=true` enablement.
- NO `eth_sendBundle` runtime path.
- NO actual relay submission.
- NO live-network test.
- NO paid live API.
- NO `docs/specs/` edit. NO `docs/adr/` edit.
- NO change to `crates/signer/src/{signer_trait,disabled,bundle_tx}.rs`.
- NO drop of `Copy` derive on `SignerError`.
- NO change to `wire_phase4` signature.
- NO change to `crates/app/tests/wire_phase4.rs`. NO change to `crates/app/src/main.rs`.
- NO change to `crates/relay-clients/`, `crates/relay-sim/`, `crates/bundle-relay/`, `crates/execution/`, `crates/state-fetcher/`, etc.
- NO asset / venue / V3-fee-tier widening.

## Gates at P6B-B close (deltas vs P6B-A close baseline `1c490de`)

| Gate | Result |
|---|---|
| G1 | UNCHANGED (5 `//!` doc-comment hits). |
| G2a | UNCHANGED at 0. |
| G2b | UNCHANGED at 0. New deps `tracing` + `tracing-subscriber` are not in banned set. |
| G2c / G2d | EXTENDED by EXACTLY 1 file (`crates/signer/src/production.rs`). Allow-list = 9 files exact. **Zero hits in `crates/config/`** (verified by rename strategy). Zero hits anywhere outside allow-list. |
| G2e | UNCHANGED at 2. |
| G3 / G4 | UNCHANGED at 0. |
| G5 | `live_send=true` reject UNCHANGED + 3 NEW config rejects (TIGHTENING). |
| G6 / G7 / G8 / G9 / G10 | UNCHANGED. |
| G11 | UNCHANGED at 1 production call site. |
| G12 / G13 / G14 | UNCHANGED. |

Workspace tests: 239 -> 244 passed + 1 ignored (target).

## Q-B1..Q-B7 (all LOCKED at v0.3; pending Codex ratification)

- Q-B1: LOCKED -- no HSM/KMS vendor / SDK in P6B-B; deferred to P6B-C.
- Q-B2: LOCKED -- host-compromise control candidate selected = Section 2.5 candidate #4 ("Operator-visible signing audit log"). **P6B-B ships ONLY the log-source piece** (`tracing::info!` structured event); the operator-visible dashboard + alert-threshold surface is DEFERRED to P6B-C. Pre-sign mismatch-comparator gating (candidate #3) deferred to P6B-E. **P6B-B by itself does NOT fully wire the Section 2.5 host-compromise control + does NOT satisfy the Section 2.5 hard minimum.**
- Q-B3: LOCKED -- `Profile` + `KeyBackend` enums; bidirectional + non-empty `audit_key_id` validation reject (3 rules).
- Q-B4: LOCKED -- `wire_phase4` signature UNCHANGED; caller-side match-arm wiring at `crates/app/src/lib.rs:184` inside `pub fn run`. `main.rs` UNCHANGED.
- Q-B5: LOCKED -- `SignerError::NotConfigured` payload-free; preserves Copy derive.
- Q-B6: LOCKED -- G2c/G2d allow-list extends by EXACTLY 1 file (`crates/signer/src/production.rs`). Allow-list count = 9. `crates/config/` stays out of allow-list via rename strategy.
- Q-B7: LOCKED -- targeted `cargo test -p ...` at P6B-B close (3 crates); full workspace at P6B-F.

## Plan execution checklist (after Codex APPROVED on this v0.5 plan + explicit user re-authorization for the P6B-B non-goal)

- [ ] **Step 1: Confirm predecessor state.** `git log --oneline -3` shows HEAD `1c490de`; `git status --short` only persistent scratch.
- [ ] **Step 2: Add `SignerError::NotConfigured`** to `crates/signer/src/error.rs`. Add `tracing = { workspace = true }` to `crates/signer/Cargo.toml` `[dependencies]`. Add `tracing-subscriber = { workspace = true }` to `[dev-dependencies]`. Add `pub mod production; pub use production::ProductionSigner;` to `crates/signer/src/lib.rs`. NEW file `crates/signer/src/production.rs` per D-B1 + D-B2.
- [ ] **Step 3: Add `Profile` + `KeyBackend` enums + `Config.active_profile` field + `RelayConfig.key_backend` field + `RelayConfig.audit_key_id` field + 3 NEW `ConfigError` variants + 3 NEW validation rules** to `crates/config/src/lib.rs` per D-B4.
- [ ] **Step 4: Extend `crates/app/src/lib.rs:30` use-statement and replace `crates/app/src/lib.rs:184` single-line signer construction with the `match config.relay.key_backend` arm** per D-B4 caller-side. `wire_phase4` sig UNCHANGED. `main.rs` UNCHANGED. `tests/wire_phase4.rs` UNCHANGED.
- [ ] **Step 5: Add D-T-B1..D-T-B5 tests** per D-B6.
- [ ] **Step 6: Targeted self-check** per D-B7. `cargo fmt --check`; targeted clippy + test on 3 crates; G2a / G2b / G2c / G2d / G2e / G3 / G4 / G5 / G6 / G7 / G10 / G11 / G12 / G13 / G14 ripgrep gates. ASCII-only check on ALL touched files. **Verify G2c/G2d returns hits in EXACTLY 9 files**: 5 baseline `crates/signer/` + 1 NEW (`production.rs`) + 3 P6-B approved (`crates/execution/src/lib.rs`, `crates/app/src/lib.rs`, `crates/app/tests/wire_phase4.rs`). **Verify `crates/config/src/lib.rs` returns zero matches** (the rename strategy from item 1).
- [ ] **Step 7: Commit + push** as a single routine `feat(p6b-b)` commit.
- [ ] **Step 8: Emit P6B-B closeout report** to `.coordination/claude_outbox.md`. Draft P6B-C pre-impl plan.

## Process

1. Claude writes this v0.5 plan to disk (UNCOMMITTED) + emits the review pack to `.coordination/claude_outbox.md`.
2. Claude STOPS and reports "manual Codex re-review required for P6B-B v0.5".
3. User pastes pack to Codex; relays verdict.
4. Claude records verdict in `.coordination/codex_review.md`.
5. **APPROVED + explicit user re-authorization for the P6B-B non-goal** -> commit + push this plan; execute Steps 2..8 of the checklist.
6. **APPROVED without user re-authorization** -> commit + push plan; AWAIT explicit user re-authorization.
7. **REVISION REQUIRED** -> revise plan in place + re-emit pack.
8. **Scope / ADR change required** -> HALT to user.
