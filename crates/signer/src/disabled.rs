//! P5-C DP-C5 fail-closed signer impl.
//!
//! [`DisabledSigner`] is the only [`crate::Signer`] impl Phase 5 ships.
//! `sign_tx` returns [`crate::SignerError::SignerDisabled`] for any
//! input, regardless of `BundleTx` contents. Phase 6b Production Gate
//! is the only path to a real signer.

use crate::{BundleTx, SignedTxBytes, Signer, SignerError};

#[derive(Debug, Clone, Copy, Default)]
pub struct DisabledSigner;

#[async_trait::async_trait]
impl Signer for DisabledSigner {
    async fn sign_tx(&self, _tx: &BundleTx) -> Result<SignedTxBytes, SignerError> {
        Err(SignerError::SignerDisabled)
    }
}
