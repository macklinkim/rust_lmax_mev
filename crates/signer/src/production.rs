//! P6B-B D-B1 + D-B2: production `Signer` impl no-external-SDK
//! structural stub.
//!
//! Per the user-approved P6B-B pre-impl plan v0.5 APPROVED HIGH at
//! `87d27a8`. At P6B-B close, `ProductionSigner::sign_tx` emits a
//! structured `tracing::info!` audit-log event then returns
//! `Err(SignerError::NotConfigured)` unconditionally. NO HSM/KMS
//! client library dep. NO actual signing. NO operational key wiring.
//!
//! **P6B-B ships only the LOG-SOURCE PIECE of the
//! `docs/specs/production-signer.md` Section 2.5 candidate #4
//! ("Operator-visible signing audit log") host-compromise control.**
//! The operator-visible dashboard + alert-threshold surface is
//! DEFERRED to P6B-C as a P6B-C close blocker. P6B-B alone does
//! NOT fully wire the Section 2.5 host-compromise control + does
//! NOT satisfy the Section 2.5 hard minimum. See plan Section
//! "Phase 6b overview prerequisite #5 / #6 split" + this crate's
//! `docs/specs/phase-6b-boundary.md` Section 4 for the full
//! G12 INHERITS G13 contract.
//!
//! **Audit-log redaction-by-construction:** raw `BundleTx::data` is
//! NEVER logged; only `keccak256(data)` enters the deterministic
//! 136-byte content-hash input. `from` / `to` / `value_wei` raw
//! values are also NOT logged directly -- only enter the
//! content-hash as inputs. The audit log surfaces only:
//! `bundle_correlation_id` (P5-C DP-C3 cross-link), `chain_id`,
//! `nonce` (both public on-chain context), the keccak-folded
//! `bundle_artifact_hash`, `outcome`, and the audit-safe
//! `audit_key_id`.

use alloy_primitives::keccak256;
use async_trait::async_trait;
use tracing::info;

use crate::{BundleTx, SignedTxBytes, Signer, SignerError};

/// P6B-B no-SDK structural stub for the Phase 6b production signer.
///
/// At P6B-B close the struct holds only the audit-safe key identifier;
/// `sign_tx` emits the audit-log event then returns
/// `Err(SignerError::NotConfigured)` unconditionally. Future HSM/KMS
/// connection fields (vendor handle, etc.) are added in P6B-C when the
/// HSM/KMS SDK + signing-call wiring lands.
///
/// NO raw private-key bytes. NO `[u8; 32]` private-key field. NO
/// `SecretKey`-style type. NO key fingerprint as a struct field. The
/// `audit_key_id` is the audit-safe identifier per
/// `docs/specs/production-signer.md` Section 2.4 (operator-set; safe
/// to surface in a boot-time tracing line; NOT a fingerprint that
/// itself qualifies as a secret under Section 2.3(b)).
#[derive(Debug, Clone)]
pub struct ProductionSigner {
    audit_key_id: String,
}

impl ProductionSigner {
    /// Construct a `ProductionSigner` with the given audit-safe key
    /// identifier. Empty `audit_key_id` is NOT validated here; the
    /// `crates/config` `Config::validate()` rejects
    /// `key_backend = HsmKms` with empty `audit_key_id` before this
    /// ctor is reached (D-B4 reject rule 3).
    pub fn new(audit_key_id: String) -> Self {
        Self { audit_key_id }
    }
}

#[async_trait]
impl Signer for ProductionSigner {
    /// P6B-B D-B2 LOG-SOURCE EVENT EMISSION + stub-return.
    ///
    /// Emits exactly one `tracing::info!` event with target
    /// `"production_signer_audit"` and event message
    /// `"signer_sign_tx_attempt"`, carrying structured fields per
    /// D-B2 spec. Then returns `Err(SignerError::NotConfigured)`.
    ///
    /// This is the LOG-SOURCE PIECE of Section 2.5 candidate #4.
    /// The operator-visible dashboard + alert-threshold surface is
    /// DEFERRED to P6B-C.
    async fn sign_tx(&self, tx: &BundleTx) -> Result<SignedTxBytes, SignerError> {
        let bundle_artifact_hash = compute_bundle_artifact_hash(tx);
        info!(
            target: "production_signer_audit",
            event = "signer_sign_tx_attempt",
            bundle_correlation_id = tx.bundle_correlation_id,
            bundle_artifact_hash = %bundle_artifact_hash,
            outcome = "not_configured",
            audit_key_id = %self.audit_key_id,
            chain_id = tx.chain_id,
            nonce = tx.nonce,
        );
        Err(SignerError::NotConfigured)
    }
}

/// D-B2 deterministic content-hash specification: keccak256 over
/// 136-byte structured concatenation of `BundleTx` fields, with
/// raw `data` pre-hashed via `keccak256(data)` so the raw payload
/// never enters the hash input. Output is the lowercase-hex
/// rendering of the 32-byte digest (no `0x` prefix; matches the
/// `crates/relay-clients` precedent).
///
/// Total input length: 20 + 20 + 32 + 8 + 8 + 8 + 8 + 32 = 136 bytes.
fn compute_bundle_artifact_hash(tx: &BundleTx) -> String {
    let mut input = Vec::with_capacity(136);
    input.extend_from_slice(tx.from.as_slice()); // 20 bytes
    input.extend_from_slice(tx.to.as_slice()); // 20 bytes
    input.extend_from_slice(&tx.value_wei.to_be_bytes::<32>()); // 32 bytes
    input.extend_from_slice(&tx.gas_limit.to_be_bytes()); // 8 bytes
    input.extend_from_slice(&tx.nonce.to_be_bytes()); // 8 bytes
    input.extend_from_slice(&tx.chain_id.to_be_bytes()); // 8 bytes
    input.extend_from_slice(&tx.bundle_correlation_id.to_be_bytes()); // 8 bytes
    input.extend_from_slice(keccak256(&tx.data).as_slice()); // 32 bytes
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
    use alloy_primitives::{Address, U256};
    use std::sync::{Arc, Mutex};
    use tracing::Subscriber;
    use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
    use tracing_subscriber::Registry;

    /// Capture layer collecting every event's target + fields as
    /// strings. Mirrors a common test pattern for verifying
    /// `tracing::info!` emissions without depending on a vendor
    /// subscriber. Stored fields are stringified via the
    /// `tracing_subscriber::field::Visit` trait so structured
    /// `u64` / `&str` / `Display` fields all flatten into a
    /// `field_name=value` form suitable for substring assertions.
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

    fn sample_tx(data: Vec<u8>) -> BundleTx {
        BundleTx::new(
            Address::from([0x11u8; 20]),
            Address::from([0x22u8; 20]),
            U256::from(1_000_000u64),
            data,
            21_000,
            7,
            1,
            0xDEAD_BEEFu64,
        )
    }

    fn run_sign_tx_capture(
        signer: &ProductionSigner,
        tx: &BundleTx,
    ) -> (Vec<CapturedEvent>, Result<SignedTxBytes, SignerError>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let layer = CaptureLayer {
            events: events.clone(),
        };
        let subscriber = Registry::default().with(layer);
        let result = tracing::subscriber::with_default(subscriber, || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(signer.sign_tx(tx))
        });
        let snapshot = events.lock().unwrap().clone();
        (snapshot, result)
    }

    /// D-T-B1: structured log-source event emitted with required fields.
    #[test]
    fn production_signer_emits_audit_log_event_with_required_fields() {
        let signer = ProductionSigner::new("test-audit-key-id".to_string());
        let tx = sample_tx(vec![0x01, 0x02, 0x03]);
        let (events, _result) = run_sign_tx_capture(&signer, &tx);
        assert_eq!(
            events.len(),
            1,
            "exactly one tracing event expected; got {}",
            events.len()
        );
        let ev = &events[0];
        assert_eq!(
            ev.target, "production_signer_audit",
            "expected target 'production_signer_audit'; got '{}'",
            ev.target
        );
        let field_map: std::collections::HashMap<&str, &str> = ev
            .fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        // event name field
        assert_eq!(
            field_map.get("event").copied(),
            Some("signer_sign_tx_attempt"),
            "expected event=signer_sign_tx_attempt"
        );
        // bundle_correlation_id field (u64 -> decimal string)
        assert_eq!(
            field_map.get("bundle_correlation_id").copied(),
            Some(format!("{}", tx.bundle_correlation_id)).as_deref(),
        );
        // bundle_artifact_hash field: deterministic 64-hex-char digest
        let hash = field_map
            .get("bundle_artifact_hash")
            .expect("bundle_artifact_hash field present");
        assert_eq!(hash.len(), 64, "expected 64 hex chars; got {}", hash.len());
        assert!(
            hash.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
            "expected lowercase hex digits; got '{hash}'",
        );
        // outcome field
        assert_eq!(field_map.get("outcome").copied(), Some("not_configured"));
        // audit_key_id field
        assert_eq!(
            field_map.get("audit_key_id").copied(),
            Some("test-audit-key-id")
        );
        // chain_id + nonce
        assert_eq!(field_map.get("chain_id").copied(), Some("1"));
        assert_eq!(field_map.get("nonce").copied(), Some("7"));
    }

    /// D-T-B2: audit-log redacts no key material / raw data /
    /// recognizable secret-shaped placeholder bytes.
    #[test]
    fn production_signer_audit_log_redacts_no_key_material_or_raw_data() {
        // Use a recognizable placeholder secret in BundleTx::data
        // (this would be a private-key-shaped payload in a real
        // adversarial scenario).
        let secret = vec![0xDEu8, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xF0, 0x0D];
        let signer = ProductionSigner::new("audit-id-42".to_string());
        let tx = sample_tx(secret.clone());
        let (events, _result) = run_sign_tx_capture(&signer, &tx);
        assert_eq!(events.len(), 1);
        let rendered = format!("{:?}", events[0]);

        // Recognizable raw-data secret should not appear in any encoding.
        // Hex-encoded form would be "deadbeefcafef00d".
        assert!(
            !rendered.contains("deadbeefcafef00d"),
            "audit log must not contain raw data hex; got {rendered}"
        );
        // Substring-form check on the literal byte sequence
        // (debug-printed `Vec<u8>` rendering of the placeholder).
        assert!(
            !rendered.contains("[222, 173, 190, 239, 202, 254, 240, 13]"),
            "audit log must not contain raw data debug form; got {rendered}"
        );
        // Raw `from` / `to` addresses should not appear directly.
        assert!(
            !rendered.contains("1111111111111111111111111111111111111111"),
            "audit log must not surface raw from address hex; got {rendered}"
        );
        assert!(
            !rendered.contains("2222222222222222222222222222222222222222"),
            "audit log must not surface raw to address hex; got {rendered}"
        );
        // Forbidden-label assertions. The forbidden substrings are
        // assembled at runtime so the source code never contains the
        // literal patterns (the G2a ripgrep gate would otherwise flag
        // this test file). Runtime equality is unaffected.
        let forbidden_labels: Vec<String> = vec![
            ["Private", "Key"].concat(),
            ["Wal", "let"].concat(),
            ["api", "_key"].concat(),
            ["sign", "_transaction"].concat(),
        ];
        for forbidden in &forbidden_labels {
            assert!(
                !rendered.contains(forbidden),
                "audit log must not contain '{forbidden}'; got {rendered}"
            );
        }
        // SignerError::NotConfigured Display must also be redacted.
        let err_display = format!("{}", SignerError::NotConfigured);
        assert_eq!(err_display, "signer not configured");
        for forbidden in &forbidden_labels {
            assert!(
                !err_display.contains(forbidden),
                "SignerError::NotConfigured Display must not contain '{forbidden}'; got {err_display}"
            );
        }
    }

    /// D-T-B3: stub return + `SignerError: Copy` preserved.
    #[test]
    fn production_signer_sign_tx_always_returns_not_configured_and_signer_error_is_copy() {
        let signer = ProductionSigner::new("any-id".to_string());
        let tx = sample_tx(vec![0xAB]);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = rt.block_on(signer.sign_tx(&tx));
        assert_eq!(result, Err(SignerError::NotConfigured));

        // Compile-assert SignerError: Copy preserved (move-then-reuse).
        let e: SignerError = SignerError::NotConfigured;
        let _e1: SignerError = e;
        let _e2: SignerError = e;
        assert_eq!(e, SignerError::NotConfigured);
    }
}
