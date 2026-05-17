//! P6B-C D-C2 real AWS KMS impl of `KmsSigningClient`.
//!
//! Wraps `aws_sdk_kms::Client`. The SDK performs ECDSA signing
//! SERVER-SIDE; raw key bytes physically live in AWS-managed
//! HSM-backed key storage and never enter the workspace process.
//!
//! G2b safety: this module imports `aws_sdk_kms` + `aws_config` only.
//! The banned signer-symbol set enumerated by G2a / G2b
//! (Phase 6a boundary doc Section 4) does NOT appear in this file --
//! verified by the G2a `rg` gate at P6B-C close (0 hits absolute).
//! The add-then-verify guard in P6B-C v0.3 D-C1 also runs
//! `cargo tree -e features -p rust-lmax-mev-signer | rg ...` and
//! expects zero hits against the banned dep set.

use async_trait::async_trait;
use aws_sdk_kms::primitives::Blob;
use aws_sdk_kms::types::{MessageType, SigningAlgorithmSpec};

use crate::kms_client::{KmsClientError, KmsSigningClient};

/// Real AWS KMS impl. Held by `ProductionSigner` as
/// `Arc<dyn KmsSigningClient>` so tests can substitute a mock without
/// reaching the network.
#[derive(Debug, Clone)]
pub(crate) struct AwsKmsSigningClient {
    inner: aws_sdk_kms::Client,
}

impl AwsKmsSigningClient {
    pub(crate) fn new(inner: aws_sdk_kms::Client) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl KmsSigningClient for AwsKmsSigningClient {
    async fn get_public_key_der(&self, key_id: &str) -> Result<Vec<u8>, KmsClientError> {
        let resp = self
            .inner
            .get_public_key()
            .key_id(key_id)
            .send()
            .await
            .map_err(|_| KmsClientError::GetPublicKeyFailed)?;
        let blob: Blob = resp.public_key.ok_or(KmsClientError::GetPublicKeyFailed)?;
        Ok(blob.into_inner())
    }

    async fn sign_digest(
        &self,
        key_id: &str,
        digest: &[u8; 32],
    ) -> Result<Vec<u8>, KmsClientError> {
        let resp = self
            .inner
            .sign()
            .key_id(key_id)
            .message_type(MessageType::Digest)
            .signing_algorithm(SigningAlgorithmSpec::EcdsaSha256)
            .message(Blob::new(digest.to_vec()))
            .send()
            .await
            .map_err(|_| KmsClientError::SignFailed)?;
        let blob: Blob = resp.signature.ok_or(KmsClientError::SignFailed)?;
        Ok(blob.into_inner())
    }
}
