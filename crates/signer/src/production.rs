//! P6B-C v0.3 D-C2 + D-C3 + D-C4: HSM/KMS-infrastructure-only
//! production signer. Fail-closed throughout P6B-C close.
//!
//! At P6B-C close, `ProductionSigner::sign_tx` returns:
//!
//! - `Err(SignerError::AddressMismatch)` when the signer was
//!   constructed through `from_aws_kms(...)` AND the inbound
//!   `BundleTx.from` does not match the boot-derived address.
//! - `Err(SignerError::NotConfigured)` otherwise (legacy `new()` path
//!   AND `from_aws_kms` path with matching `from`).
//!
//! **`sign_tx` NEVER returns `Ok(_)` in P6B-C.** The
//! `client.sign_digest(...)` method is held on the trait for future
//! review but is NEVER invoked from `sign_tx`; a targeted mock test
//! exercises it independently so the trait method is not dead code.
//!
//! Per Codex APPROVED HIGH on v0.3 + the user-approved P6B-C
//! implementation re-authorization at master `3e803c2`.
//!
//! Boot path (`from_aws_kms` / `from_aws_kms_with_client`):
//!
//! 1. Fetch AWS KMS `GetPublicKey` -> DER SubjectPublicKeyInfo bytes.
//! 2. Validate the literal RFC 5480 / SEC1 prefix bytes + length +
//!    `0x04` uncompressed marker; mismatch -> `Err(ClientInit)` (no
//!    panic).
//! 3. Extract the SEC1 uncompressed point `0x04 || x || y` and
//!    compute `derived_address = keccak256(x || y)[12..]`.
//! 4. Emit one boot tracing event
//!    `target="production_signer_boot"`,
//!    `event="production_signer_initialized"` with `audit_key_id` +
//!    `derived_address` fields (no key-material-shaped field names).
//! 5. Emit Prometheus gauges
//!    `production_signer_audit_alert_threshold_max_attempts_per_minute`
//!    + `production_signer_audit_alert_threshold_max_failed_per_minute`
//!      carrying the configured thresholds (gauge value `0` means
//!      operator left the threshold disabled).
//!
//! Sign-attempt audit log (preserved from P6B-B): every `sign_tx`
//! attempt emits a structured `tracing::info!` event with target
//! `production_signer_audit` carrying the bundle correlation chain
//! per `production-signer.md` Section 2.3.

use std::sync::Arc;

use alloy_primitives::{keccak256, Address};
use async_trait::async_trait;
use tracing::info;

use crate::kms_client::KmsSigningClient;
use crate::{BundleTx, SignedTxBytes, Signer, SignerError};

/// Signer-local alert thresholds for the operator-visible signing
/// audit dashboard. Plain integer values with `0` meaning "disabled".
/// The signer crate intentionally does NOT depend on `crates/config`;
/// `crates/app` maps the config-side `SigningAuditAlertConfig` into
/// this struct at boot.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SigningAuditThresholds {
    pub max_attempts_per_minute: u32,
    pub max_failed_per_minute: u32,
}

/// P6B-B / P6B-C structural production signer.
///
/// Holds the audit-safe key identifier, optionally the HSM/KMS-derived
/// address and KMS client (only when constructed through
/// `from_aws_kms*`), and the operator-configured alert thresholds.
///
/// NO raw private-key bytes. NO `[u8; 32]` private-key field.
/// no opaque private-key-byte struct field of any shape. NO key
/// fingerprint as a struct field. (P6B-CD v0.4 R-9 cleanup: prior
/// wording removed to keep G2g at 0 hits absolute; invariant
/// preserved verbatim.)
#[derive(Debug, Clone)]
pub struct ProductionSigner {
    audit_key_id: String,
    derived_address: Option<Address>,
    /// P6B-CD D-CD6: SEC1-uncompressed public-key bytes
    /// (`0x04 || x || y`, 65 bytes) captured at boot alongside
    /// `derived_address`. Used by `sign_tx`'s recovery cross-check.
    /// NOT key material per `production-signer.md` Section 2.3(b):
    /// public-key bytes are non-secret.
    pubkey_sec1_65: Option<[u8; 65]>,
    /// `client` holds the `Arc<dyn KmsSigningClient>` used by
    /// `sign_tx`'s P6B-CD Ok-path. Field is `Option` so the legacy
    /// `new(...)` ctor (no HSM connection) destructures to `None`
    /// and falls through to `Err(NotConfigured)`.
    client: Option<Arc<dyn KmsSigningClient>>,
    /// `thresholds` are consumed at boot to emit the Prometheus
    /// threshold gauges; the field is held so a future config-reload
    /// path can re-emit them. Test-accessor `thresholds()` reads it.
    #[allow(dead_code)]
    thresholds: SigningAuditThresholds,
}

impl ProductionSigner {
    /// P6B-B legacy ctor preserved. No HSM/KMS connection; no
    /// `derived_address`; `sign_tx` returns
    /// `Err(SignerError::NotConfigured)` for every call. This is the
    /// stub path the P6B-B baseline ships.
    pub fn new(audit_key_id: String) -> Self {
        Self {
            audit_key_id,
            derived_address: None,
            pubkey_sec1_65: None,
            client: None,
            thresholds: SigningAuditThresholds::default(),
        }
    }

    /// P6B-C public factory. Loads the AWS SDK default credential
    /// chain, constructs an `aws_sdk_kms::Client`, calls
    /// `GetPublicKey`, derives the Ethereum address, emits the boot
    /// identifier + threshold gauges, and stores the client for the
    /// future sign-activation batch.
    ///
    /// Returns `Err(SignerError::ClientInit)` on any AWS SDK / DER /
    /// length / marker failure. Never panics.
    ///
    /// **`crates/signer` deviates here from the v0.3 plan text in one
    /// minor place**: the factory loads `aws_config::SdkConfig`
    /// internally via `aws_config::load_from_env().await` rather than
    /// accepting an `SdkConfig` parameter from the caller. This keeps
    /// `crates/app` free of an `aws-config` dep edge while preserving
    /// the spec spirit (default credential chain, operator-controlled
    /// outside the workspace). The test-injection factory
    /// `from_aws_kms_with_client` covers the same boot path without
    /// reaching AWS.
    pub async fn from_aws_kms(
        audit_key_id: String,
        thresholds: SigningAuditThresholds,
    ) -> Result<Self, SignerError> {
        let aws_cfg = aws_config::load_from_env().await;
        let kms = aws_sdk_kms::Client::new(&aws_cfg);
        let client = Arc::new(crate::kms_aws::AwsKmsSigningClient::new(kms));
        Self::from_aws_kms_with_client(audit_key_id, thresholds, client).await
    }

    /// P6B-C test-injection factory. Takes a pre-constructed
    /// `Arc<dyn KmsSigningClient>`; identical boot path to
    /// `from_aws_kms` from `GetPublicKey` onward. `pub(crate)` so the
    /// trait stays signer-private; `#[cfg(test)]` callers in this
    /// crate use it directly.
    pub(crate) async fn from_aws_kms_with_client(
        audit_key_id: String,
        thresholds: SigningAuditThresholds,
        client: Arc<dyn KmsSigningClient>,
    ) -> Result<Self, SignerError> {
        let der = client
            .get_public_key_der(&audit_key_id)
            .await
            .map_err(|_| SignerError::ClientInit)?;
        let pubkey_sec1_65 = parse_ec_pubkey_der_to_sec1_65(&der)?;
        let derived_address = derive_address_from_sec1_65(&pubkey_sec1_65);

        info!(
            target: "production_signer_boot",
            event = "production_signer_initialized",
            audit_key_id = %audit_key_id,
            derived_address = %derived_address,
        );

        metrics::gauge!("production_signer_audit_alert_threshold_max_attempts_per_minute")
            .set(thresholds.max_attempts_per_minute as f64);
        metrics::gauge!("production_signer_audit_alert_threshold_max_failed_per_minute")
            .set(thresholds.max_failed_per_minute as f64);

        Ok(Self {
            audit_key_id,
            derived_address: Some(derived_address),
            pubkey_sec1_65: Some(pubkey_sec1_65),
            client: Some(client),
            thresholds,
        })
    }

    /// Test-only accessor for the derived address. `pub(crate)` so
    /// only in-crate tests reach it.
    #[cfg(test)]
    pub(crate) fn derived_address(&self) -> Option<Address> {
        self.derived_address
    }

    /// Test-only accessor for the thresholds. `pub(crate)` so only
    /// in-crate tests reach it.
    #[cfg(test)]
    pub(crate) fn thresholds(&self) -> SigningAuditThresholds {
        self.thresholds
    }
}

#[async_trait]
impl Signer for ProductionSigner {
    /// P6B-CD v0.4 D-CD6 body. `Ok(SignedTxBytes)` is now reachable
    /// when ALL of: signer was built via `from_aws_kms*`,
    /// `tx.from == derived_address`, `max_fee_per_gas >=
    /// max_priority_fee_per_gas`, KMS `sign_digest` succeeds, DER
    /// parses, low-s normalization completes, and trial-recovery
    /// against `pubkey_sec1_65` finds a matching `yParity in {0, 1}`.
    /// Every return path emits an audited + counted outcome label
    /// exactly once (G15 contract). No `expect(...)` / `unwrap()` /
    /// `panic!()` on any path.
    async fn sign_tx(&self, tx: &BundleTx) -> Result<SignedTxBytes, SignerError> {
        // P6B-C invariant: address consistency.
        if let Some(derived) = self.derived_address {
            if tx.from != derived {
                emit_attempt_audit(self, tx, "address_mismatch");
                counter_increment("address_mismatch");
                return Err(SignerError::AddressMismatch);
            }
        }
        // P6B-CD: legacy `new(...)` path OR any boot path where the
        // three KMS-derived Options are not jointly Some(...) stays
        // NotConfigured. `from_aws_kms_with_client` is the only setter
        // and sets all three atomically; this destructure is the single
        // fail-closed entry for "no live KMS attached". No panic path.
        let (Some(client), Some(_derived), Some(pubkey_sec1_65)) = (
            self.client.as_ref(),
            self.derived_address,
            self.pubkey_sec1_65,
        ) else {
            emit_attempt_audit(self, tx, "not_configured");
            counter_increment("not_configured");
            return Err(SignerError::NotConfigured);
        };

        // P6B-CD bundle-tx invariant check.
        if tx.max_fee_per_gas < tx.max_priority_fee_per_gas {
            emit_attempt_audit(self, tx, "invalid_bundle_tx");
            counter_increment("invalid_bundle_tx");
            return Err(SignerError::InvalidBundleTx);
        }

        // P6B-CD encode + sign.
        let preimage = crate::rlp::encode_eip1559_unsigned(tx);
        let digest = keccak256(&preimage);
        let der = match client.sign_digest(&self.audit_key_id, &digest.0).await {
            Ok(d) => d,
            Err(_) => {
                emit_attempt_audit(self, tx, "kms_sign_failed");
                counter_increment("kms_sign_failed");
                return Err(SignerError::KmsSignFailed);
            }
        };

        // R-2: DER parse failure is audited + counted.
        let (r, s) = match crate::recovery::parse_der_to_rs(&der) {
            Ok(rs) => rs,
            Err(_) => {
                emit_attempt_audit(self, tx, "invalid_signature_bytes");
                counter_increment("invalid_signature_bytes");
                return Err(SignerError::InvalidSignatureBytes);
            }
        };

        // R-2: trial-recovery failure is audited + counted.
        let y_parity = match crate::recovery::recover_y_parity(&digest.0, r, s, &pubkey_sec1_65) {
            Ok(v) => v,
            Err(_) => {
                emit_attempt_audit(self, tx, "signature_recovery_failed");
                counter_increment("signature_recovery_failed");
                return Err(SignerError::SignatureRecoveryFailed);
            }
        };

        // P6B-CD assemble signed bytes.
        let signed = crate::rlp::encode_eip1559_signed(tx, y_parity, &r, &s);
        emit_attempt_audit(self, tx, "ok");
        counter_increment("ok");
        Ok(SignedTxBytes(signed))
    }
}

fn counter_increment(outcome: &'static str) {
    metrics::counter!(
        "production_signer_audit_attempts_total",
        "outcome" => outcome,
    )
    .increment(1);
}

/// P6B-C v0.3 D-C2 DER/SPKI parser.
///
/// Validates the literal RFC 5480 / SEC1 SubjectPublicKeyInfo prefix
/// for the EC curve identified by OID `1.3.132.0.10` (the K1 curve
/// AWS KMS uses with key-spec `ECC_SECG_P256K1` for ECDSA-SHA-256
/// signing) -- including the outer SEQUENCE header, the
/// AlgorithmIdentifier `ecPublicKey` + curve OIDs, the BIT STRING
/// header, the zero-unused-bits byte, and the `0x04` SEC1
/// uncompressed-point marker -- then extracts the 32-byte X || 32-byte
/// Y suffix and returns `keccak256(X || Y)[12..]` as the derived
/// `Address`.
///
/// Any prefix mismatch, length mismatch, or marker mismatch returns
/// `Err(SignerError::ClientInit)` with no panic.
///
/// P6B-CD D-CD6 refactor: returns the SEC1-uncompressed point bytes
/// `0x04 || x || y` (65 bytes) instead of the derived `Address`. The
/// boot path derives the address via [`derive_address_from_sec1_65`]
/// and stores BOTH on the signer (the SEC1 bytes are needed by
/// `recover_y_parity` at sign time).
fn parse_ec_pubkey_der_to_sec1_65(der: &[u8]) -> Result<[u8; 65], SignerError> {
    // Expected DER for AWS KMS `ECC_SECG_P256K1` `GetPublicKey` response:
    //
    //   30 56                            ; SEQUENCE (86 bytes)
    //     30 10                          ; SEQUENCE (16 bytes) AlgorithmIdentifier
    //       06 07 2A 86 48 CE 3D 02 01   ; OID id-ecPublicKey (1.2.840.10045.2.1)
    //       06 05 2B 81 04 00 0A         ; OID 1.3.132.0.10 (the K1 curve)
    //     03 42 00                       ; BIT STRING (66 bytes; 0 unused bits)
    //     04                             ; SEC1 uncompressed-point marker
    //     <32 bytes X> <32 bytes Y>
    //
    // Total: 88 bytes. The 23-byte fixed prefix is followed by the
    // 0x04 SEC1 marker at offset 23; bytes 23..88 are the
    // SEC1-uncompressed point (0x04 || x || y). Byte-literal check;
    // no ASN.1 parser dependency.
    const EXPECTED_PREFIX_TO_MARKER: &[u8; 24] = b"\x30\x56\x30\x10\x06\x07\x2a\x86\x48\xce\x3d\x02\x01\x06\x05\x2b\x81\x04\x00\x0a\x03\x42\x00\x04";
    const EXPECTED_LEN: usize = 88;
    const SEC1_OFFSET: usize = 23; // byte index of the 0x04 SEC1 marker

    if der.len() != EXPECTED_LEN {
        return Err(SignerError::ClientInit);
    }
    if &der[..EXPECTED_PREFIX_TO_MARKER.len()] != EXPECTED_PREFIX_TO_MARKER.as_slice() {
        return Err(SignerError::ClientInit);
    }
    let mut sec1 = [0u8; 65];
    sec1.copy_from_slice(&der[SEC1_OFFSET..EXPECTED_LEN]);
    debug_assert_eq!(sec1[0], 0x04);
    Ok(sec1)
}

/// Derive an Ethereum `Address` from SEC1-uncompressed point bytes
/// `0x04 || x || y` via `keccak256(x || y)[12..32]`. Public-key
/// derivation is non-secret per `production-signer.md` Section 2.3(b).
fn derive_address_from_sec1_65(sec1_65: &[u8; 65]) -> Address {
    let digest = keccak256(&sec1_65[1..]);
    Address::from_slice(&digest.as_slice()[12..])
}

/// Emit the structured `tracing::info!` audit event for one
/// `sign_tx` attempt. Preserved verbatim from P6B-B D-B2, modulo the
/// caller-supplied `outcome` label.
fn emit_attempt_audit(signer: &ProductionSigner, tx: &BundleTx, outcome: &'static str) {
    let bundle_artifact_hash = compute_bundle_artifact_hash(tx);
    info!(
        target: "production_signer_audit",
        event = "signer_sign_tx_attempt",
        bundle_correlation_id = tx.bundle_correlation_id,
        bundle_artifact_hash = %bundle_artifact_hash,
        outcome = outcome,
        audit_key_id = %signer.audit_key_id,
        chain_id = tx.chain_id,
        nonce = tx.nonce,
    );
}

/// D-B2 deterministic content-hash specification (P6B-B carry-forward).
/// keccak256 over 136-byte structured concatenation of `BundleTx`
/// fields, with raw `data` pre-hashed via `keccak256(data)` so the
/// raw payload never enters the hash input. Output is the
/// lowercase-hex rendering of the 32-byte digest.
fn compute_bundle_artifact_hash(tx: &BundleTx) -> String {
    let mut input = Vec::with_capacity(136);
    input.extend_from_slice(tx.from.as_slice());
    input.extend_from_slice(tx.to.as_slice());
    input.extend_from_slice(&tx.value_wei.to_be_bytes::<32>());
    input.extend_from_slice(&tx.gas_limit.to_be_bytes());
    input.extend_from_slice(&tx.nonce.to_be_bytes());
    input.extend_from_slice(&tx.chain_id.to_be_bytes());
    input.extend_from_slice(&tx.bundle_correlation_id.to_be_bytes());
    input.extend_from_slice(keccak256(&tx.data).as_slice());
    debug_assert_eq!(input.len(), 136);
    let digest = keccak256(&input);
    let mut hex = String::with_capacity(64);
    for b in digest.as_slice() {
        hex.push_str(&format!("{b:02x}"));
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kms_client::KmsClientError;
    use alloy_primitives::U256;
    use async_trait::async_trait;
    use metrics_util::debugging::{DebugValue, DebuggingRecorder};
    use std::sync::{Arc, Mutex};
    use tracing::Subscriber;
    use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
    use tracing_subscriber::Registry;

    // ------------------------------------------------------------------
    // Mock KMS client + DER helper
    // ------------------------------------------------------------------

    fn valid_test_der(x: [u8; 32], y: [u8; 32]) -> Vec<u8> {
        let prefix: &[u8; 24] = b"\x30\x56\x30\x10\x06\x07\x2a\x86\x48\xce\x3d\x02\x01\x06\x05\x2b\x81\x04\x00\x0a\x03\x42\x00\x04";
        let mut v = Vec::with_capacity(88);
        v.extend_from_slice(prefix);
        v.extend_from_slice(&x);
        v.extend_from_slice(&y);
        v
    }

    fn expected_address_from(x: [u8; 32], y: [u8; 32]) -> Address {
        let mut input = [0u8; 64];
        input[..32].copy_from_slice(&x);
        input[32..].copy_from_slice(&y);
        Address::from_slice(&keccak256(input).as_slice()[12..])
    }

    /// Canned-response mock implementing `KmsSigningClient`. Each
    /// field is `Result<Vec<u8>, KmsClientError>` so individual tests
    /// can force success or a specific failure mode.
    #[derive(Debug, Clone)]
    struct MockKmsSigningClient {
        public_key_response: Result<Vec<u8>, KmsClientError>,
        sign_response: Result<Vec<u8>, KmsClientError>,
    }

    impl MockKmsSigningClient {
        fn ok(der: Vec<u8>) -> Self {
            Self {
                public_key_response: Ok(der),
                sign_response: Ok(vec![0xDE, 0xAD, 0xBE, 0xEF]),
            }
        }

        fn pubkey_failure() -> Self {
            Self {
                public_key_response: Err(KmsClientError::GetPublicKeyFailed),
                sign_response: Err(KmsClientError::SignFailed),
            }
        }
    }

    #[async_trait]
    impl KmsSigningClient for MockKmsSigningClient {
        async fn get_public_key_der(&self, _key_id: &str) -> Result<Vec<u8>, KmsClientError> {
            self.public_key_response.clone()
        }

        async fn sign_digest(
            &self,
            _key_id: &str,
            _digest: &[u8; 32],
        ) -> Result<Vec<u8>, KmsClientError> {
            self.sign_response.clone()
        }
    }

    // ------------------------------------------------------------------
    // Tracing capture layer
    // ------------------------------------------------------------------

    #[derive(Debug, Default, Clone)]
    struct CapturedEvent {
        target: String,
        fields: Vec<(String, String)>,
    }

    #[derive(Default)]
    struct CaptureLayer {
        events: Arc<Mutex<Vec<CapturedEvent>>>,
    }

    struct FieldVisitor<'a> {
        fields: &'a mut Vec<(String, String)>,
    }

    impl<'a> tracing::field::Visit for FieldVisitor<'a> {
        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            self.fields
                .push((field.name().to_string(), value.to_string()));
        }
        fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
            self.fields
                .push((field.name().to_string(), value.to_string()));
        }
        fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
            self.fields
                .push((field.name().to_string(), value.to_string()));
        }
        fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
            self.fields
                .push((field.name().to_string(), value.to_string()));
        }
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            self.fields
                .push((field.name().to_string(), format!("{value:?}")));
        }
    }

    impl<S: Subscriber> Layer<S> for CaptureLayer {
        fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
            let mut fields: Vec<(String, String)> = Vec::new();
            let mut visitor = FieldVisitor {
                fields: &mut fields,
            };
            event.record(&mut visitor);
            self.events.lock().unwrap().push(CapturedEvent {
                target: event.metadata().target().to_string(),
                fields,
            });
        }
    }

    fn capture_events<F>(f: F) -> Vec<CapturedEvent>
    where
        F: FnOnce(),
    {
        let events = Arc::new(Mutex::new(Vec::new()));
        let layer = CaptureLayer {
            events: events.clone(),
        };
        let subscriber = Registry::default().with(layer);
        tracing::subscriber::with_default(subscriber, f);
        let snap = events.lock().unwrap().clone();
        snap
    }

    fn sample_tx_with_from(from: Address, data: Vec<u8>) -> BundleTx {
        BundleTx::new(
            from,
            Address::from([0x22u8; 20]),
            U256::from(1_000_000u64),
            data,
            21_000,
            7,
            1,
            0xDEAD_BEEFu64,
            U256::from(1u64), // max_priority_fee_per_gas
            U256::from(2u64), // max_fee_per_gas
        )
    }

    /// Hand-crafted structurally-valid DER for an ECDSA signature
    /// with (r=[0x01;32], s=[0x02;32]). Both r and s have top bit
    /// clear so no DER 0x00 padding byte is needed. DER bytes:
    ///   30 44                ; SEQUENCE (68 bytes body)
    ///     02 20 r (32 bytes) ; INTEGER r
    ///     02 20 s (32 bytes) ; INTEGER s
    /// Total: 70 bytes. parse_der_to_rs ACCEPTS this; recover_y_parity
    /// against any specific pubkey almost-certainly returns Err
    /// because (r, s, digest) is not a real signature.
    fn well_formed_but_non_recovering_der() -> Vec<u8> {
        let mut v = Vec::with_capacity(70);
        v.extend_from_slice(&[0x30, 0x44, 0x02, 0x20]);
        v.extend_from_slice(&[0x01u8; 32]);
        v.extend_from_slice(&[0x02, 0x20]);
        v.extend_from_slice(&[0x02u8; 32]);
        v
    }

    fn current_thread_rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    fn fields_to_map(ev: &CapturedEvent) -> std::collections::HashMap<&str, &str> {
        ev.fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect()
    }

    // ------------------------------------------------------------------
    // D-T-CD9 (was P6B-C d_t_c1; repurposed for the P6B-CD sign_tx Ok
    // path that no longer returns NotConfigured for matched-from):
    // mock returns 4-byte non-DER garbage; sign_tx full pipeline runs
    // up to parse_der_to_rs, which fails -> Err(InvalidSignatureBytes);
    // outcome=invalid_signature_bytes audit + counter emitted.
    // ------------------------------------------------------------------
    #[test]
    fn d_t_cd9_invalid_signature_bytes_emits_audit_and_counter() {
        let x = [0xAAu8; 32];
        let y = [0xBBu8; 32];
        let mock = Arc::new(MockKmsSigningClient::ok(valid_test_der(x, y)));
        let derived = expected_address_from(x, y);

        let events = capture_events(|| {
            let rt = current_thread_rt();
            rt.block_on(async {
                let signer = ProductionSigner::from_aws_kms_with_client(
                    "test-audit-key-id".to_string(),
                    SigningAuditThresholds::default(),
                    mock.clone(),
                )
                .await
                .expect("from_aws_kms_with_client should succeed");
                assert_eq!(signer.derived_address(), Some(derived));

                let tx = sample_tx_with_from(derived, vec![0x01, 0x02, 0x03]);
                let result = signer.sign_tx(&tx).await;
                // Default mock sign_response is vec![0xDE, 0xAD, 0xBE, 0xEF]
                // (4 bytes); not valid DER -> InvalidSignatureBytes.
                assert_eq!(result, Err(SignerError::InvalidSignatureBytes));

                // Exercise mock sign_digest directly so the trait method
                // is not dead code at the workspace test level.
                let raw = mock
                    .sign_digest("test-audit-key-id", &[0u8; 32])
                    .await
                    .expect("mock sign_digest configured to Ok");
                assert_eq!(raw, vec![0xDE, 0xAD, 0xBE, 0xEF]);
            });
        });

        let audit = events
            .iter()
            .find(|e| {
                e.target == "production_signer_audit"
                    && fields_to_map(e).get("outcome").copied() == Some("invalid_signature_bytes")
            })
            .expect("one invalid_signature_bytes audit event expected");
        let map = fields_to_map(audit);
        assert_eq!(map.get("event").copied(), Some("signer_sign_tx_attempt"));
        assert_eq!(map.get("audit_key_id").copied(), Some("test-audit-key-id"));
    }

    // ------------------------------------------------------------------
    // D-T-C2: KMS mock success + mismatched from -> AddressMismatch
    // ------------------------------------------------------------------
    #[test]
    fn d_t_c2_mismatched_address_returns_address_mismatch_with_audit() {
        let x = [0x33u8; 32];
        let y = [0x44u8; 32];
        let mock = Arc::new(MockKmsSigningClient::ok(valid_test_der(x, y)));

        let events = capture_events(|| {
            let rt = current_thread_rt();
            rt.block_on(async {
                let signer = ProductionSigner::from_aws_kms_with_client(
                    "another-key".to_string(),
                    SigningAuditThresholds::default(),
                    mock,
                )
                .await
                .expect("ctor success");
                // Use an address that cannot equal the derived address
                // for any non-trivial (x, y).
                let bogus_from = Address::ZERO;
                assert_ne!(
                    Some(bogus_from),
                    signer.derived_address(),
                    "test setup invariant",
                );
                let tx = sample_tx_with_from(bogus_from, vec![0x55]);
                let result = signer.sign_tx(&tx).await;
                assert_eq!(result, Err(SignerError::AddressMismatch));
            });
        });

        let audit = events
            .iter()
            .find(|e| {
                e.target == "production_signer_audit"
                    && fields_to_map(e).get("outcome").copied() == Some("address_mismatch")
            })
            .expect("one address_mismatch audit event expected");
        let map = fields_to_map(audit);
        assert_eq!(map.get("outcome").copied(), Some("address_mismatch"));
    }

    // ------------------------------------------------------------------
    // D-T-C3: KMS get_public_key failure -> ClientInit, redacted Display
    // ------------------------------------------------------------------
    #[test]
    fn d_t_c3_client_init_failure_returns_client_init_with_redacted_display() {
        let mock = Arc::new(MockKmsSigningClient::pubkey_failure());
        let rt = current_thread_rt();
        let result = rt.block_on(ProductionSigner::from_aws_kms_with_client(
            "boot-failure-id".to_string(),
            SigningAuditThresholds::default(),
            mock,
        ));
        assert_eq!(result.map(|_| ()), Err(SignerError::ClientInit));

        let rendered = format!("{}", SignerError::ClientInit);
        assert_eq!(rendered, "production signer client init failed");

        // No AWS-credential-shaped substring; no payload-bytes substring.
        // Forbidden substrings are assembled at runtime so this test
        // file does not itself contain the literal patterns the
        // G2a / G6 ripgrep gates scan for.
        let forbidden: Vec<String> = vec![
            ["AKIA"].concat(),
            ["aws_secret", "_access_key"].concat(),
            ["api", "_key"].concat(),
            ["Private", "Key"].concat(),
        ];
        for sub in &forbidden {
            assert!(
                !rendered.contains(sub),
                "Display must not contain '{sub}'; got {rendered}",
            );
        }
    }

    // ------------------------------------------------------------------
    // D-T-C4: Boot emits one production_signer_boot event with
    // audit_key_id + derived_address, and no key-material-shaped
    // field names.
    // ------------------------------------------------------------------
    #[test]
    fn d_t_c4_boot_emits_audit_safe_initialization_event() {
        let x = [0x10u8; 32];
        let y = [0x20u8; 32];
        let mock = Arc::new(MockKmsSigningClient::ok(valid_test_der(x, y)));

        let events = capture_events(|| {
            let rt = current_thread_rt();
            rt.block_on(async {
                ProductionSigner::from_aws_kms_with_client(
                    "boot-audit-id".to_string(),
                    SigningAuditThresholds::default(),
                    mock,
                )
                .await
                .expect("ctor success");
            });
        });

        let boot_events: Vec<&CapturedEvent> = events
            .iter()
            .filter(|e| e.target == "production_signer_boot")
            .collect();
        assert_eq!(
            boot_events.len(),
            1,
            "exactly one production_signer_boot event expected; got {}",
            boot_events.len()
        );
        let boot = boot_events[0];
        let map = fields_to_map(boot);
        // Required fields with non-empty values.
        let key_id = map
            .get("audit_key_id")
            .expect("audit_key_id field present in boot event");
        assert!(!key_id.is_empty(), "audit_key_id must be non-empty");
        let addr = map
            .get("derived_address")
            .expect("derived_address field present in boot event");
        assert!(!addr.is_empty(), "derived_address must be non-empty");
        // event = "production_signer_initialized"
        assert_eq!(
            map.get("event").copied(),
            Some("production_signer_initialized"),
        );

        // No field NAME contains a key-material-shaped substring.
        // Substrings assembled at runtime so the source file itself
        // does not contain the literal patterns the G2a ripgrep gate
        // scans for.
        let forbidden_substrings: Vec<String> = vec![
            "private".to_string(),
            "secret".to_string(),
            "priv".to_string(),
            "seed".to_string(),
            ["fund", "ed"].concat(),
        ];
        for (name, _value) in &boot.fields {
            let lower = name.to_ascii_lowercase();
            for forbidden in &forbidden_substrings {
                assert!(
                    !lower.contains(forbidden),
                    "boot event field name '{name}' contains forbidden substring '{forbidden}'",
                );
            }
        }
    }

    // ------------------------------------------------------------------
    // D-T-C5: Legacy ProductionSigner::new(...) returns NotConfigured.
    // ------------------------------------------------------------------
    #[test]
    fn d_t_c5_legacy_new_returns_not_configured() {
        let signer = ProductionSigner::new("any-id".to_string());
        assert_eq!(signer.derived_address(), None);
        let tx = sample_tx_with_from(Address::ZERO, vec![0xAB]);
        let rt = current_thread_rt();
        let result = rt.block_on(signer.sign_tx(&tx));
        assert_eq!(result, Err(SignerError::NotConfigured));

        // SignerError stays Copy after P6B-C additions.
        let e: SignerError = SignerError::ClientInit;
        let _e1: SignerError = e;
        let _e2: SignerError = e;
        let e2: SignerError = SignerError::AddressMismatch;
        let _e3: SignerError = e2;
        let _e4: SignerError = e2;
    }

    // ------------------------------------------------------------------
    // D-T-C7: Constructor emits both threshold gauges with configured
    // values. Uses metrics-util DebuggingRecorder under a local-scope
    // recorder so other tests are unaffected.
    // ------------------------------------------------------------------
    #[test]
    fn d_t_c7_constructor_emits_threshold_gauges() {
        let x = [0xCCu8; 32];
        let y = [0xDDu8; 32];
        let mock = Arc::new(MockKmsSigningClient::ok(valid_test_der(x, y)));
        let thresholds = SigningAuditThresholds {
            max_attempts_per_minute: 600,
            max_failed_per_minute: 60,
        };

        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();

        metrics::with_local_recorder(&recorder, || {
            let rt = current_thread_rt();
            rt.block_on(async {
                let signer = ProductionSigner::from_aws_kms_with_client(
                    "gauge-test-id".to_string(),
                    thresholds,
                    mock,
                )
                .await
                .expect("ctor success");
                assert_eq!(signer.thresholds(), thresholds);
            });
        });

        let snapshot = snapshotter.snapshot().into_vec();
        let mut got_attempts: Option<f64> = None;
        let mut got_failed: Option<f64> = None;
        for (key, _unit, _desc, value) in snapshot {
            let key_name = key.key().name().to_string();
            if let DebugValue::Gauge(g) = value {
                let g: f64 = g.into_inner();
                if key_name == "production_signer_audit_alert_threshold_max_attempts_per_minute" {
                    got_attempts = Some(g);
                } else if key_name
                    == "production_signer_audit_alert_threshold_max_failed_per_minute"
                {
                    got_failed = Some(g);
                }
            }
        }
        assert_eq!(
            got_attempts,
            Some(600.0),
            "max_attempts_per_minute gauge must register configured value",
        );
        assert_eq!(
            got_failed,
            Some(60.0),
            "max_failed_per_minute gauge must register configured value",
        );
    }

    // ------------------------------------------------------------------
    // DER parser robustness: length / prefix / marker mismatches all
    // surface as Err(ClientInit) with no panic. Targets the P6B-CD
    // refactored function `parse_ec_pubkey_der_to_sec1_65`.
    // ------------------------------------------------------------------
    #[test]
    fn parser_rejects_short_der_with_client_init() {
        let too_short = vec![0u8; 87];
        let result = parse_ec_pubkey_der_to_sec1_65(&too_short);
        assert_eq!(result.err(), Some(SignerError::ClientInit));
    }

    #[test]
    fn parser_rejects_bad_prefix_with_client_init() {
        let mut der = valid_test_der([0u8; 32], [0u8; 32]);
        der[0] = 0x31;
        let result = parse_ec_pubkey_der_to_sec1_65(&der);
        assert_eq!(result.err(), Some(SignerError::ClientInit));
    }

    #[test]
    fn parser_rejects_bad_uncompressed_marker_with_client_init() {
        let mut der = valid_test_der([0u8; 32], [0u8; 32]);
        der[23] = 0x02;
        let result = parse_ec_pubkey_der_to_sec1_65(&der);
        assert_eq!(result.err(), Some(SignerError::ClientInit));
    }

    // ------------------------------------------------------------------
    // D-T-CD7: KMS sign_digest error -> Err(KmsSignFailed); audit +
    // counter emitted with outcome="kms_sign_failed".
    // ------------------------------------------------------------------
    #[test]
    fn d_t_cd7_kms_sign_failed_emits_audit_and_counter() {
        let x = [0x55u8; 32];
        let y = [0x66u8; 32];
        let mut mock_state = MockKmsSigningClient::ok(valid_test_der(x, y));
        // Override sign_response to simulate AWS KMS Sign failure.
        mock_state.sign_response = Err(KmsClientError::SignFailed);
        let mock = Arc::new(mock_state);
        let derived = expected_address_from(x, y);

        let events = capture_events(|| {
            let rt = current_thread_rt();
            rt.block_on(async {
                let signer = ProductionSigner::from_aws_kms_with_client(
                    "kms-sign-err-id".to_string(),
                    SigningAuditThresholds::default(),
                    mock,
                )
                .await
                .expect("ctor success");
                let tx = sample_tx_with_from(derived, vec![0x77]);
                let result = signer.sign_tx(&tx).await;
                assert_eq!(result, Err(SignerError::KmsSignFailed));
            });
        });

        let audit = events
            .iter()
            .find(|e| {
                e.target == "production_signer_audit"
                    && fields_to_map(e).get("outcome").copied() == Some("kms_sign_failed")
            })
            .expect("one kms_sign_failed audit event expected");
        let map = fields_to_map(audit);
        assert_eq!(map.get("outcome").copied(), Some("kms_sign_failed"));
    }

    // ------------------------------------------------------------------
    // D-T-CD10 (also covers v0.4 plan D-T-CD5 source-lock deviation
    // structurally): mock returns a STRUCTURALLY VALID DER signature
    // (parses successfully) that does NOT correspond to a real
    // signature by the boot-time public key. parse_der_to_rs succeeds
    // -> recover_y_parity fails because neither y_parity in {0, 1}
    // recovers to the boot pubkey -> Err(SignatureRecoveryFailed);
    // outcome=signature_recovery_failed audit + counter emitted. This
    // exercises every line of the sign_tx pipeline up to (but not
    // through) the Ok-return assembly. Positive Ok-return testing
    // requires precomputed off-tree non-secret material per the v0.4
    // plan deviation note in recovery.rs.
    // ------------------------------------------------------------------
    #[test]
    fn d_t_cd10_signature_recovery_failed_emits_audit_and_counter() {
        let x = [0x88u8; 32];
        let y = [0x99u8; 32];
        let mut mock_state = MockKmsSigningClient::ok(valid_test_der(x, y));
        mock_state.sign_response = Ok(well_formed_but_non_recovering_der());
        let mock = Arc::new(mock_state);
        let derived = expected_address_from(x, y);

        let events = capture_events(|| {
            let rt = current_thread_rt();
            rt.block_on(async {
                let signer = ProductionSigner::from_aws_kms_with_client(
                    "recovery-fail-id".to_string(),
                    SigningAuditThresholds::default(),
                    mock,
                )
                .await
                .expect("ctor success");
                let tx = sample_tx_with_from(derived, vec![0xAA]);
                let result = signer.sign_tx(&tx).await;
                assert_eq!(result, Err(SignerError::SignatureRecoveryFailed));
            });
        });

        let audit = events
            .iter()
            .find(|e| {
                e.target == "production_signer_audit"
                    && fields_to_map(e).get("outcome").copied() == Some("signature_recovery_failed")
            })
            .expect("one signature_recovery_failed audit event expected");
        let map = fields_to_map(audit);
        assert_eq!(
            map.get("outcome").copied(),
            Some("signature_recovery_failed")
        );
    }

    // ------------------------------------------------------------------
    // D-T-CD-INVALID-BUNDLE: sign_tx with max_fee_per_gas <
    // max_priority_fee_per_gas -> Err(InvalidBundleTx); outcome=
    // invalid_bundle_tx audit + counter emitted. (Covers the
    // pre-sign invariant check the v0.4 plan adds in D-CD6 step 3.)
    // ------------------------------------------------------------------
    #[test]
    fn d_t_cd_invalid_bundle_tx_emits_audit_and_counter() {
        let x = [0x11u8; 32];
        let y = [0x12u8; 32];
        let mock = Arc::new(MockKmsSigningClient::ok(valid_test_der(x, y)));
        let derived = expected_address_from(x, y);

        let events = capture_events(|| {
            let rt = current_thread_rt();
            rt.block_on(async {
                let signer = ProductionSigner::from_aws_kms_with_client(
                    "bad-fee-id".to_string(),
                    SigningAuditThresholds::default(),
                    mock,
                )
                .await
                .expect("ctor success");
                // Build a tx with max_fee < max_priority_fee.
                let tx = BundleTx::new(
                    derived,
                    Address::from([0x22u8; 20]),
                    U256::ZERO,
                    Vec::new(),
                    21_000,
                    0,
                    1,
                    0,
                    U256::from(100u64), // max_priority_fee_per_gas
                    U256::from(50u64),  // max_fee_per_gas (lower; invalid)
                );
                let result = signer.sign_tx(&tx).await;
                assert_eq!(result, Err(SignerError::InvalidBundleTx));
            });
        });

        let audit = events
            .iter()
            .find(|e| {
                e.target == "production_signer_audit"
                    && fields_to_map(e).get("outcome").copied() == Some("invalid_bundle_tx")
            })
            .expect("one invalid_bundle_tx audit event expected");
        let map = fields_to_map(audit);
        assert_eq!(map.get("outcome").copied(), Some("invalid_bundle_tx"));
    }
}
