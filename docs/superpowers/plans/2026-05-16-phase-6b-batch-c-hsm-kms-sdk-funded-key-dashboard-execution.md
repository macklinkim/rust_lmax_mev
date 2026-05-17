# Phase 6b Batch C -- HSM/KMS infrastructure-only + operator-visible audit surface

**Date:** 2026-05-16 KST
**Status:** Draft v0.3 (revised after manual Codex REVISION REQUIRED on v0.2). This is a PRE-IMPL PLAN. No implementation is authorized by this document alone.

## Manual Codex v0.2 review result

Codex verdict on v0.2: **REVISION REQUIRED / Confidence MEDIUM-HIGH**.

Required v0.3 fixes:

1. Remove residual `alloy-rlp` and pre-add `cargo tree -p aws-sdk-kms` wording from P6B-C. RLP remains deferred to the future sign-activation batch.
2. Make the `ProductionSigner::from_aws_kms` public signature consistent everywhere, including AWS config and alert-threshold input.
3. Avoid a signer -> config crate dependency for alert thresholds; pass signer-local threshold values from `crates/app`.
4. Preserve `ProductionSigner::new(...)` P6B-B behavior. The new address-consistency check must apply only to the AWS-KMS-constructed signer, not to the legacy no-client stub path.
5. Make the sample Alertmanager YAML reference the configured threshold gauges, not hardcoded constants or future-only `hsm_error` labels.
6. Reconcile the approved Phase 6b boundary/overview sequencing with the new reality that `sign_tx -> Ok(_)` is deferred past P6B-C. P6B-C implementation must include a small boundary-doc amendment recording this split before any later P6B-D/P6B-E work.
7. Fix app error mapping to match the current codebase, which has no existing `AppError::Signer` variant.

Advisory items also folded into v0.3:

- DER/SPKI parsing validates literal expected prefix bytes and returns `Err(SignerError::ClientInit)` on any mismatch, length mismatch, or uncompressed-marker mismatch. No panic.
- The boot-event test checks for required fields and absence of key-material-shaped field names; it does not assert that tracing metadata has no extra fields.
- `sign_digest` is exercised by a mock-client test path so the trait method is not left untested in P6B-C.

## Predecessors

- Phase 6b overview v0.2 APPROVED HIGH at `49123e9`.
- P6B-A fully closed at `2ddba8a` / `1c490de`.
- P6B-B fully closed at `87d27a8` / `df96ac8`.
- Current baseline: `master` HEAD `df96ac8`; targeted workspace baseline reported as **244 passed + 1 ignored**.

## Scope

P6B-C v0.3 is an **HSM/KMS infrastructure-only batch plus operator-visible signing-audit surface**.

At P6B-C close:

- `ProductionSigner::sign_tx` remains fail-closed and never returns `Ok(_)`.
- AWS KMS `GetPublicKey` is reachable through the public factory to derive and audit the active address.
- AWS KMS `Sign` is represented in the internal client trait for future activation, but is not invoked from `sign_tx`.
- No RLP encoder is added.
- No DER-to-`(r,s,v)` conversion is added.
- No `BundleTx` fee-field change is made.
- No `live_send=true` enablement is made.
- No `eth_sendBundle` runtime path is made.
- No actual relay submission is made.
- No live-network test is added.

This plan intentionally defers `sign_tx -> Ok(SignedTxBytes)` to a future approved sign-activation batch after both architectural blockers are resolved:

- `BundleTx` currently lacks EIP-1559 fee fields needed to construct a valid unsigned transaction for RLP signing.
- AWS KMS returns DER ECDSA signatures without an Ethereum recovery id; computing `v` requires either an explicit G2b-approved recovery-only crypto-library carve-out or a different signer service that returns Ethereum-compatible `(r,s,v)` directly.

## Boundary Reconciliation

P6B-C v0.3 changes the original Phase 6b batch assumptions. The committed `docs/specs/phase-6b-boundary.md` and `docs/superpowers/plans/2026-05-16-phase-6b-overview-execution.md` still describe P6B-B as the batch where a signer can return `Ok(_)`, with P6B-C only wiring the funded key operationally.

Because Codex found that `Ok(_)` is structurally unsafe to plan before the `BundleTx` and recovery-id blockers are solved, P6B-C implementation must include a small doc reconciliation:

- Amend `docs/specs/phase-6b-boundary.md` Section 3 to record that P6B-B and P6B-C together are now **pre-activation** signer infrastructure batches.
- Record that the future sign-activation batch, name locked by that future plan, must land before P6B-D and P6B-E can begin.
- Record that P6B-D `live_send=true` and P6B-E actual relay submission remain blocked until `sign_tx -> Ok(_)` is reviewed-closed in that future batch.
- Keep `docs/specs/production-signer.md` unchanged.
- Keep `docs/adr/` unchanged.

The boundary reconciliation is doc-only and does not unlock any live-action gate.

## Deliverables

### D-C1 -- AWS KMS SDK Integration, Infrastructure-Only

Vendor selected: AWS KMS through `aws-sdk-kms` and `aws-config`.

Implementation-time dependency edits:

- `crates/signer/Cargo.toml` gains `aws-sdk-kms = "1.65"`.
- `crates/signer/Cargo.toml` gains `aws-config = "1.6"`.
- `crates/signer/Cargo.toml` gains `metrics = { workspace = true }` if not already present.
- `crates/signer/Cargo.toml` keeps `tracing = { workspace = true }`.
- `alloy-rlp` is **not** added in P6B-C.

Verification protocol:

1. Add the dependencies.
2. Run `cargo tree -e features -p rust-lmax-mev-signer 2>&1 | rg 'secp256k1|k256|alloy-signer|ethers-signers'`.
3. Expected result: zero matches.
4. If any match appears, revert the dependency edit and halt for Codex/user review.

### D-C2 -- Signer-Private KMS Client Trait

Add `crates/signer/src/kms_client.rs` with signer-private trait:

```text
#[async_trait]
pub(crate) trait KmsSigningClient: Send + Sync + std::fmt::Debug {
    async fn get_public_key_der(&self, key_id: &str) -> Result<Vec<u8>, KmsClientError>;

    async fn sign_digest(
        &self,
        key_id: &str,
        digest: &[u8; 32],
    ) -> Result<Vec<u8>, KmsClientError>;
}
```

`sign_digest` is not called from `sign_tx` in P6B-C. It exists so the client boundary is reviewed before the future activation batch.

Add `crates/signer/src/kms_aws.rs` with `AwsKmsSigningClient`, plus test-only mock support. The trait remains `pub(crate)` and is never imported by `crates/app`.

### D-C3 -- Public Factory and Threshold Input

Add a signer-local threshold struct to avoid coupling `crates/signer` to `crates/config`:

```text
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SigningAuditThresholds {
    pub max_attempts_per_minute: u32,
    pub max_failed_per_minute: u32,
}
```

Public constructor:

```text
pub async fn from_aws_kms(
    audit_key_id: String,
    aws_config: aws_config::SdkConfig,
    thresholds: SigningAuditThresholds,
) -> Result<Self, SignerError>;
```

Test-injection constructor:

```text
pub(crate) async fn from_aws_kms_with_client(
    audit_key_id: String,
    thresholds: SigningAuditThresholds,
    client: Arc<dyn KmsSigningClient>,
) -> Result<Self, SignerError>;
```

`ProductionSigner` stores:

```text
pub struct ProductionSigner {
    audit_key_id: String,
    derived_address: Option<Address>,
    client: Option<Arc<dyn KmsSigningClient>>,
    thresholds: SigningAuditThresholds,
}
```

`ProductionSigner::new(audit_key_id)` is preserved for the P6B-B no-client stub path:

- `derived_address = None`.
- `client = None`.
- `thresholds = SigningAuditThresholds::default()`.
- `sign_tx` returns `Err(SignerError::NotConfigured)` as before.

The AWS-KMS factory path:

1. Constructs `aws_sdk_kms::Client` from `aws_config`.
2. Wraps it in `AwsKmsSigningClient`.
3. Calls `get_public_key_der(&audit_key_id).await`.
4. Parses SubjectPublicKeyInfo to extract the SEC1 uncompressed point `0x04 || x || y`. The parser validates the expected RFC 5480 / SEC1 prefix bytes literally, validates length, validates the `0x04` uncompressed marker, and returns `Err(SignerError::ClientInit)` on any mismatch. It must not panic.
5. Computes `Address = keccak256(pubkey_uncompressed[1..])[12..32]`.
6. Emits one boot tracing event: `target="production_signer_boot"`, `event="production_signer_initialized"`, `audit_key_id`, `derived_address`.
7. Stores `derived_address = Some(address)` and `client = Some(client)`.

New payload-free `SignerError` variants:

- `ClientInit`
- `AddressMismatch`

Both preserve `SignerError: Copy` and render fixed redacted display text.

### D-C4 -- `sign_tx` Remains Fail-Closed

P6B-C `sign_tx` shape:

```text
async fn sign_tx(&self, tx: &BundleTx) -> Result<SignedTxBytes, SignerError> {
    if let Some(derived_address) = self.derived_address {
        if tx.from != derived_address {
            emit_audit("address_mismatch", tx);
            metrics::counter!("production_signer_audit_attempts_total", "outcome" => "address_mismatch").increment(1);
            return Err(SignerError::AddressMismatch);
        }
    }

    emit_audit("not_configured", tx);
    metrics::counter!("production_signer_audit_attempts_total", "outcome" => "not_configured").increment(1);
    Err(SignerError::NotConfigured)
}
```

This preserves P6B-B behavior for `ProductionSigner::new(...)` and adds the address-consistency invariant only when the signer was constructed through AWS KMS.

Hard invariants:

- No `Ok(_)` return path.
- No `client.sign_digest(...)` invocation from `sign_tx`.
- No RLP encoding.
- No DER signature parsing.
- No recovery-id computation.
- No `secp256k1`, `k256`, `alloy-signer`, `ethers-signers`, `Wallet`, `PrivateKey`, `sign_transaction`, or `funded` symbol introduced in `crates/*.rs`.

### D-C5 -- Config and App Wiring

Add config-only threshold fields:

```text
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, default)]
pub struct SigningAuditAlertConfig {
    pub max_attempts_per_minute: u32,
    pub max_failed_per_minute: u32,
}
```

Add `RelayConfig::signing_audit_alert: SigningAuditAlertConfig`, defaulting to zeros.

`crates/app/src/lib.rs::run` maps config to signer thresholds:

```text
let thresholds = SigningAuditThresholds {
    max_attempts_per_minute: config.relay.signing_audit_alert.max_attempts_per_minute,
    max_failed_per_minute: config.relay.signing_audit_alert.max_failed_per_minute,
};
```

For `KeyBackend::HsmKms`, app constructs:

```text
let aws_config = runtime.block_on(aws_config::load_from_env());
let signer = runtime
    .block_on(ProductionSigner::from_aws_kms(
        config.relay.audit_key_id.clone(),
        aws_config,
        thresholds,
    ))
    .map_err(AppError::ProductionSignerInit)?;
```

Add `AppError::ProductionSignerInit(SignerError)` because the current `AppError` enum has no `Signer` variant.

`wire_phase4` signature remains unchanged.

### D-C6 -- Operator-Visible Audit Surface

Emit:

- Counter: `production_signer_audit_attempts_total{outcome}`.
- Gauge: `production_signer_audit_alert_threshold_max_attempts_per_minute`.
- Gauge: `production_signer_audit_alert_threshold_max_failed_per_minute`.

The gauges are emitted from the signer constructor using the resolved threshold values passed from config. Gauge value `0` means disabled.

Add `config/examples/signing-audit-alert.yaml` with gauge-referenced expressions, not hardcoded thresholds:

```text
groups:
  - name: production-signer-audit
    rules:
      - alert: SigningAttemptRateHigh
        expr: rate(production_signer_audit_attempts_total[1m]) * 60 > on() production_signer_audit_alert_threshold_max_attempts_per_minute
        for: 1m
        labels:
          severity: warning
      - alert: SigningFailureRateHigh
        expr: rate(production_signer_audit_attempts_total{outcome!="not_configured"}[1m]) * 60 > on() production_signer_audit_alert_threshold_max_failed_per_minute
        for: 1m
        labels:
          severity: critical
```

This YAML is a sample for operator infrastructure. The workspace does not ship Grafana or Alertmanager runtime.

### D-C7 -- Boundary-Doc Audit Amendment

Amend `docs/specs/phase-6b-boundary.md` only as needed to:

- Record the P6B-B/P6B-C pre-activation split.
- Record that a future sign-activation batch is required before P6B-D/P6B-E.
- Add the Section 2.5 candidate #4 dashboard contract: counter, gauges, sample rule, and audit-safe boot identifier.

No edit to `docs/specs/production-signer.md`, `docs/specs/execution-safety.md`, `docs/specs/phase-6a-boundary.md`, or `docs/adr/`.

## Tests

Seven new targeted tests:

| ID | Target | Assertion |
|---|---|---|
| D-T-C1 | signer | `from_aws_kms_with_client` succeeds; matching `tx.from` returns `Err(NotConfigured)`; audit outcome `not_configured`; counter increments. |
| D-T-C2 | signer | mismatched `tx.from` returns `Err(AddressMismatch)`; audit outcome `address_mismatch`; counter increments. |
| D-T-C3 | signer | mock public-key failure returns `Err(ClientInit)`; display text contains no payload or AWS-credential-shaped substring. |
| D-T-C4 | signer | boot emits exactly one `production_signer_boot` event containing non-empty `audit_key_id` and `derived_address`; test also asserts no field name contains key-material-shaped substrings such as `private`, `secret`, `priv`, or `seed`. |
| D-T-C5 | signer | legacy `ProductionSigner::new(...)` returns `Err(NotConfigured)`, preserving P6B-B behavior. |
| D-T-C6 | config | `[relay.signing_audit_alert]` defaults to `0/0`, non-zero values parse, P6B-B reject rules remain unchanged. |
| D-T-C7 | signer | constructor emits both threshold gauges with configured values. |

D-T-C1 also invokes the mock client's `sign_digest` method directly with a canned digest and canned non-secret DER-shaped response. `ProductionSigner::sign_tx` still must not invoke `sign_digest`; this direct mock call exists only to keep the trait boundary exercised before the future activation batch.

No `#[ignore]` test is added.
No live AWS KMS call is added.

## Targeted Checks

Planning/doc-only turns: ASCII checks only.

Implementation close should run:

- `cargo fmt --check`
- `cargo clippy -p rust-lmax-mev-signer -p rust-lmax-mev-config -p rust-lmax-mev-app --all-targets -- -D warnings`
- `cargo test -p rust-lmax-mev-signer -p rust-lmax-mev-config -p rust-lmax-mev-app`
- `cargo tree -e features -p rust-lmax-mev-signer 2>&1 | rg 'secp256k1|k256|alloy-signer|ethers-signers'` expecting zero matches
- Targeted rg gates for G1/G2a/G2b/G3/G4/G5/G7/G10/G11 plus the new audit-surface checks

Full workspace test/clippy/deny/tree sweep remains deferred to P6B-F unless a real blocker requires it.

## File Touch Summary for Future Implementation

Allowed in the later implementation turn, after Codex APPROVED and explicit user re-authorization:

- `crates/signer/src/kms_client.rs` new.
- `crates/signer/src/kms_aws.rs` new.
- `crates/signer/src/production.rs` additive/substantive.
- `crates/signer/src/error.rs` additive.
- `crates/signer/src/lib.rs` additive exports.
- `crates/signer/Cargo.toml` additive AWS/metrics deps only; no `alloy-rlp`.
- `crates/config/src/lib.rs` additive config fields/tests.
- `crates/app/src/lib.rs` additive construction/error mapping.
- `config/examples/signing-audit-alert.yaml` new.
- `docs/specs/phase-6b-boundary.md` additive reconciliation/audit-surface text.

Not allowed:

- No `crates/signer/src/{signer_trait,disabled,bundle_tx}.rs` edit.
- No `Cargo.lock` edit before implementation authorization.
- No `crates/app/src/main.rs` edit.
- No `crates/app/tests/wire_phase4.rs` edit.
- No `docs/adr/` edit.
- No `docs/specs/production-signer.md` edit.
- No `live_send=true` enablement.
- No `eth_sendBundle` runtime path.
- No relay submission.
- No key material.

## Process

1. Claude writes this v0.3 plan and re-emits `.coordination/claude_outbox.md`.
2. Claude stops for manual Codex re-review.
3. If Codex APPROVED: record verdict, commit/push the approved plan only, then stop.
4. P6B-C implementation requires a later explicit user re-authorization after the approved plan is committed.
5. If Codex REVISION REQUIRED: revise plan only, re-emit outbox, ASCII-check both files, and stop.
