//! P5-C DP-C2 signer trait surface.
//!
//! Object-safe `async` trait via `async-trait`. Single method
//! `sign_tx`. Phase 5 ships exactly one impl ([`crate::DisabledSigner`]).
//! Phase 6b populates with a real HSM/KMS-backed implementation.

use crate::{BundleTx, SignedTxBytes, SignerError};

#[async_trait::async_trait]
pub trait Signer: Send + Sync + std::fmt::Debug + 'static {
    async fn sign_tx(&self, tx: &BundleTx) -> Result<SignedTxBytes, SignerError>;
}
