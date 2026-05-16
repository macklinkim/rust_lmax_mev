//! Phase 5 P5-C signer boundary + fail-closed stub.
//!
//! Per the user-approved P5-C execution note v0.3 §DP-C1..DP-C11 +
//! SC-1..5 + Codex APPROVED HIGH 2026-05-10 KST.
//!
//! Boundary-only crate. Defines the signer surface that Phase 6b
//! Production Gate will populate with a real HSM/KMS-backed
//! implementation. Phase 5 ships exactly one impl, [`DisabledSigner`],
//! whose every signing attempt returns
//! [`SignerError::SignerDisabled`] unconditionally.
//!
//! No production signer, no key derivation, no ECDSA-library or
//! wallet-library symbols anywhere in the workspace, no key material
//! in repo / tests / fixtures / configs / env examples, no app wiring.
//! Phase 6b Production Gate is the only path to a real signer.
//! (See plan DP-C7 G2a/G2b/G2d for the enforcement greps.)

mod bundle_tx;
mod disabled;
mod error;
pub mod production;
mod signer_trait;

pub use bundle_tx::{BundleTx, SignedTxBytes};
pub use disabled::DisabledSigner;
pub use error::SignerError;
pub use production::ProductionSigner;
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
        )
    }

    /// SC-1: fail-closed contract. Exercises both `Default::default()` ctor
    /// path (DP-C5: `DisabledSigner` derives `Default`) and the unit-struct
    /// literal path. Uses an `assert_eq!` directly because `SignerError`
    /// derives `PartialEq + Eq` per DP-C4 / R-C8.
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

    /// SC-3: spec-drift guard — `Display` MUST contain the literal
    /// "Phase 6b Production Gate".
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

    /// SC-4: trait object-safety — `Box<dyn Signer>` constructible
    /// and usable through the trait object.
    #[tokio::test]
    async fn sc4_signer_is_object_safe() {
        let boxed: Box<dyn Signer> = Box::new(DisabledSigner);
        let tx = sample_tx();
        assert_eq!(boxed.sign_tx(&tx).await, Err(SignerError::SignerDisabled));
    }

    /// SC-5: `BundleTx::new(...)` ctor accessibility despite
    /// `#[non_exhaustive]`; `BundleTx` + `SignedTxBytes` round-trip
    /// through `Clone + PartialEq + Eq`.
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
