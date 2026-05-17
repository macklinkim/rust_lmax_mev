//! Phase 5 P5-C signer boundary + Phase 6b P6B-B/P6B-C
//! pre-activation infrastructure.
//!
//! Per the user-approved P5-C execution note v0.3 §DP-C1..DP-C11 +
//! SC-1..5 + Codex APPROVED HIGH 2026-05-10 KST, extended by
//! P6B-B v0.5 + P6B-C v0.3 APPROVED HIGH at commit `3e803c2` +
//! explicit user re-authorization for P6B-C implementation.
//!
//! `crates/signer` exposes the [`Signer`] trait, the `Phase 6a`
//! fail-closed [`DisabledSigner`] impl, and the P6B-B / P6B-C
//! pre-activation production signer [`ProductionSigner`] whose
//! `sign_tx` is STILL fail-closed: it returns
//! [`SignerError::AddressMismatch`] (KMS-built path with mismatched
//! `from`) or [`SignerError::NotConfigured`] (every other case). It
//! NEVER returns `Ok(_)` at P6B-C close. The HSM/KMS `sign_digest`
//! call is wired on a `pub(crate)` trait but is NEVER invoked from
//! `sign_tx`; the future approved sign-activation batch flips it.
//!
//! No production signer that returns `Ok(_)`, no key derivation, no
//! ECDSA-library or wallet-library symbols anywhere in the workspace,
//! no key material in repo / tests / fixtures / configs / env
//! examples, no app live-action surface. Phase 6b sign-activation
//! batch is the only path to a real `Ok(SignedTxBytes)` return.
//! (See plan DP-C7 G2a/G2b/G2d for the enforcement greps.)

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
