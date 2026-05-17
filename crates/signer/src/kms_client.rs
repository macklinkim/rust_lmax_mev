//! P6B-C D-C2 signer-private HSM/KMS client trait.
//!
//! `KmsSigningClient` is the abstraction over the HSM/KMS vendor SDK
//! (P6B-C v0.3 selects AWS KMS via `aws-sdk-kms = "1.65"`). The trait
//! stays `pub(crate)` so the SDK choice never leaks into
//! `crates/signer`'s public surface; the public surface is the
//! `ProductionSigner::from_aws_kms(...)` factory only.
//!
//! At P6B-C close:
//!
//! - `get_public_key_der` IS reached at runtime (called once from the
//!   factory body to derive the workspace's `derived_address`).
//! - `sign_digest` is NOT invoked from `ProductionSigner::sign_tx`. It
//!   is held on the trait so the boundary is reviewable before the
//!   future approved sign-activation batch flips `sign_tx` to
//!   `Ok(SignedTxBytes)`. A targeted test exercises this method via
//!   the mock client so it is not dead code.
//!
//! `KmsClientError` is the internal error type. `Display` is fixed text
//! with no payload-bytes rendering. Variants are payload-free so the
//! type stays `Copy + Eq` for ergonomic test assertions.

use async_trait::async_trait;

/// Internal HSM/KMS client abstraction.
///
/// Stays `pub(crate)` so the SDK never appears in `crates/signer`'s
/// public API. The two concrete impls in P6B-C are
/// `AwsKmsSigningClient` (real, in `kms_aws.rs`) and a test mock
/// (in this crate's `#[cfg(test)]` modules).
#[async_trait]
pub(crate) trait KmsSigningClient: Send + Sync + std::fmt::Debug {
    /// Returns the DER-encoded SubjectPublicKeyInfo (RFC 5480) for the
    /// HSM/KMS-backed key referenced by `key_id`. The signer factory
    /// parses this DER to extract the SEC1 uncompressed point and
    /// derive the active Ethereum address.
    async fn get_public_key_der(&self, key_id: &str) -> Result<Vec<u8>, KmsClientError>;

    /// Sign a 32-byte message digest. Returns DER-encoded ECDSA
    /// signature bytes on success. **Not invoked from
    /// `ProductionSigner::sign_tx` in P6B-C**; held on the trait for
    /// the future approved sign-activation batch. Exercised by D-T-C1
    /// via the mock client so the trait method is not dead code at
    /// the workspace test level. `#[allow(dead_code)]` is required
    /// because the non-test crate build does not reach this method.
    #[allow(dead_code)]
    async fn sign_digest(&self, key_id: &str, digest: &[u8; 32])
        -> Result<Vec<u8>, KmsClientError>;
}

/// Internal client error variants. Payload-free so the type stays
/// `Copy + Eq`. `Display` is fixed text per variant and never renders
/// AWS SDK error bodies, public-key bytes, signature bytes, or
/// credential-shaped substrings.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum KmsClientError {
    /// `get_public_key_der` returned an error from the HSM/KMS or
    /// the response was empty. Folded into `SignerError::ClientInit`
    /// at the `ProductionSigner` boundary.
    #[error("kms get_public_key failed")]
    GetPublicKeyFailed,
    /// `sign_digest` returned an error from the HSM/KMS or the
    /// response was empty. **Not reached from `sign_tx` in P6B-C**;
    /// surfaces only when a P6B-C test exercises the mock's
    /// `sign_digest` path directly. `#[allow(dead_code)]` required
    /// for the same reason as `KmsSigningClient::sign_digest`.
    #[error("kms sign_digest failed")]
    #[allow(dead_code)]
    SignFailed,
}
