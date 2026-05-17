//! P5-C DP-C4 error type + P6B-B + P6B-C additive variants.
//!
//! `SignerError` is `#[non_exhaustive]` + payload-free.
//! `Clone + Copy + PartialEq + Eq` derives let tests compare via
//! `assert_eq!(...)` directly without `matches!`.
//!
//! P6B-C v0.3 D-C2 adds two payload-free variants
//! (`ClientInit`, `AddressMismatch`) to keep `Copy` preserved.
//! `Display` text contains no key material, no payload, no AWS
//! credential / endpoint detail per `production-signer.md`
//! Section 2.3(b).

/// Signer error set. `Display` for every variant is fixed text;
/// no payload field renders user-controlled or HSM/KMS-side content.
///
/// SC-3 pins `SignerDisabled` Display to contain the literal phrase
/// "Phase 6b Production Gate".
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SignerError {
    #[error("signer disabled — production signing requires Phase 6b Production Gate")]
    SignerDisabled,
    /// P6B-B D-B5: signer is structurally configured (via the
    /// `KeyBackend::HsmKms` config path) but `sign_tx` remains
    /// fail-closed at P6B-C close. A future approved sign-activation
    /// batch (after `BundleTx` fee-field extension + ECDSA recovery-id
    /// resolution) flips this return to `Ok(SignedTxBytes)`. Display
    /// contains no key material per `production-signer.md` Section 2.3(b).
    #[error("signer not configured")]
    NotConfigured,
    /// P6B-C D-C2: `ProductionSigner::from_aws_kms` factory failed to
    /// initialize the KMS client OR to parse the returned
    /// `GetPublicKey` DER. Display is fixed text; no payload field
    /// renders any AWS SDK error body, public-key bytes, or
    /// AWS-credential-shaped substring.
    #[error("production signer client init failed")]
    ClientInit,
    /// P6B-C D-C3: `sign_tx` was invoked with a `BundleTx` whose
    /// `from` address does not match the boot-time-derived address
    /// associated with the configured HSM/KMS key. Detected by the
    /// pre-sign address-consistency check inside
    /// `ProductionSigner::sign_tx`; fires before `NotConfigured`.
    /// Display contains no address bytes (the mismatched values are
    /// in the upstream `bundle_correlation_id`-linked audit chain).
    #[error("signer address mismatch")]
    AddressMismatch,
}
