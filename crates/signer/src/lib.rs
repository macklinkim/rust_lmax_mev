//! Phase 5 P5-C signer boundary + Phase 6b P6B-B / P6B-C / P6B-CD
//! sign-activation infrastructure.
//!
//! Per the user-approved P5-C execution note v0.3 + SC-1..5 + Codex
//! APPROVED HIGH 2026-05-10 KST, extended by P6B-B v0.5 + P6B-C v0.3 +
//! P6B-CD v0.4 APPROVED HIGH at commit `c9451c2` + explicit user
//! re-authorization for P6B-CD implementation including ADR-001
//! Amendment 2 (narrow recovery-only carve-out).
//!
//! `crates/signer` exposes the [`Signer`] trait, the Phase 6a
//! fail-closed [`DisabledSigner`] impl, and the P6B-B / P6B-C / P6B-CD
//! production signer [`ProductionSigner`]. After P6B-CD,
//! `ProductionSigner::sign_tx` MAY return `Ok(SignedTxBytes)` when ALL
//! of: signer was built via `from_aws_kms*`; `tx.from == derived_address`;
//! `max_fee_per_gas >= max_priority_fee_per_gas`; KMS `sign_digest`
//! succeeds; DER parses; low-s normalization completes; trial-recovery
//! against the boot-time public key finds a matching `yParity in {0, 1}`.
//! Every other path returns one of [`SignerError::AddressMismatch`],
//! [`SignerError::NotConfigured`], [`SignerError::InvalidBundleTx`],
//! [`SignerError::KmsSignFailed`], [`SignerError::InvalidSignatureBytes`],
//! [`SignerError::SignatureRecoveryFailed`]. Every return path emits the
//! G15 audit + counter outcome exactly once.
//!
//! **At P6B-CD close, the `Ok(_)` return path is reachable ONLY through
//! the existing `#[cfg(test)] pub(crate) async fn invoke_signer_for_test`
//! hook in `crates/execution`.** G11 production runtime sign_tx call site
//! count stays at 1; the runtime caller path is P6B-E scope.
//! **P6B-D (`live_send=true`) and P6B-E (live relay submission +
//! runtime relay submission call) REMAIN LOCKED** behind separate user
//! re-authorization per Phase 6b overview prerequisite #1.
//!
//! ECDSA recovery uses the narrow recovery-only crypto-library
//! carve-out per ADR-001 Amendment 2. The dep is named in
//! `crates/signer/Cargo.toml` and may be imported ONLY by
//! `crates/signer/src/recovery.rs` (G2f single-file import gate); the
//! permitted-symbol allow-list is enumerated in
//! `docs/specs/phase-6b-boundary.md` Section G2f. G2g continues to
//! forbid any signing-key constructor or test-key byte literal
//! anywhere in `crates/` (including `#[cfg(test)]` files); no
//! private-key material enters the workspace at any point. See
//! `docs/specs/phase-6b-boundary.md` Sections G2f + G2g + G15 + the
//! Section 3 amendment for the verbatim enforcement greps.

mod bundle_tx;
mod disabled;
mod error;
mod kms_aws;
mod kms_client;
pub mod production;
mod recovery;
mod rlp;
mod signer_trait;

pub use bundle_tx::{BundleTx, SignedTxBytes};
pub use disabled::DisabledSigner;
pub use error::SignerError;
pub use production::{ProductionSigner, SigningAuditThresholds};
pub use signer_trait::Signer;

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{Address, U256};
    use std::sync::Arc;

    fn sample_tx() -> BundleTx {
        BundleTx::new(
            Address::ZERO,
            Address::ZERO,
            U256::ZERO,
            Vec::new(),
            21_000,
            0,
            1,
            42,
            U256::ZERO, // max_priority_fee_per_gas
            U256::ZERO, // max_fee_per_gas
        )
    }

    /// SC-1: fail-closed contract.
    #[tokio::test]
    async fn sc1_disabled_signer_always_errors() {
        let via_default: DisabledSigner = Default::default();
        let via_literal = DisabledSigner;
        let tx = sample_tx();
        assert_eq!(
            via_default.sign_tx(&tx).await,
            Err(SignerError::SignerDisabled)
        );
        assert_eq!(
            via_literal.sign_tx(&tx).await,
            Err(SignerError::SignerDisabled)
        );
    }

    /// SC-2: concurrent fail-closed across `Arc`-shared signer.
    #[tokio::test]
    async fn sc2_disabled_signer_concurrent_errors() {
        let signer: Arc<DisabledSigner> = Arc::new(DisabledSigner);
        let tx = sample_tx();
        let (a, b, c) = tokio::join!(
            signer.sign_tx(&tx),
            signer.sign_tx(&tx),
            signer.sign_tx(&tx),
        );
        assert_eq!(a, Err(SignerError::SignerDisabled));
        assert_eq!(b, Err(SignerError::SignerDisabled));
        assert_eq!(c, Err(SignerError::SignerDisabled));
    }

    /// SC-3: spec-drift guard.
    #[test]
    fn sc3_signer_disabled_display_pins_phase_6b_phrase() {
        let rendered = format!("{}", SignerError::SignerDisabled);
        assert!(
            rendered.contains("Phase 6b Production Gate"),
            "SignerError::SignerDisabled Display must contain \
             'Phase 6b Production Gate' (BR-3-style spec-drift guard); \
             got: {rendered}",
        );
    }

    /// SC-4: trait object-safety.
    #[tokio::test]
    async fn sc4_signer_is_object_safe() {
        let boxed: Box<dyn Signer> = Box::new(DisabledSigner);
        let tx = sample_tx();
        assert_eq!(boxed.sign_tx(&tx).await, Err(SignerError::SignerDisabled));
    }

    /// SC-5: boundary types round-trip.
    #[test]
    fn sc5_boundary_types_round_trip() {
        let tx = sample_tx();
        let cloned = tx.clone();
        assert_eq!(tx, cloned);

        let bytes = SignedTxBytes(vec![0u8, 1, 2, 3]);
        let cloned_bytes = bytes.clone();
        assert_eq!(bytes, cloned_bytes);
    }
}
