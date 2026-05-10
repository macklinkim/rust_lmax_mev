//! P5-C DP-C3 + DP-C10 boundary types.
//!
//! [`BundleTx`] is the structured transaction-shape input to
//! [`crate::Signer::sign_tx`] per overview Q-P5-4. The `bundle_correlation_id`
//! field MUST carry the same `u64` as the upstream
//! `EventEnvelope.correlation_id` in `crates/types` so journaled signing
//! events can be cross-linked to the comparator chain. P5-C does NOT
//! introduce a `CorrelationId` newtype; `crates/types` exposes no such
//! type today and the field is plain `u64`.
//!
//! [`SignedTxBytes`] is a transparent `Vec<u8>` newtype that Phase 6b
//! will populate with real signed bytes. Phase 5 never produces a
//! `SignedTxBytes` value outside tests.

use alloy_primitives::{Address, U256};

/// Structured transaction-shape input to a `Signer::sign_tx` call.
/// `#[non_exhaustive]` so future Phase 6 fields (e.g., access list,
/// max-fee parameters, blob-tx fields) can be added without breaking
/// downstream callers; use [`BundleTx::new`] to construct.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleTx {
    pub from: Address,
    pub to: Address,
    pub value_wei: U256,
    pub data: Vec<u8>,
    pub gas_limit: u64,
    pub nonce: u64,
    pub chain_id: u64,
    /// MUST be the same `u64` as the upstream
    /// `rust_lmax_mev_types::EventEnvelope::correlation_id` so journaled
    /// signing events are cross-linkable to the comparator chain.
    pub bundle_correlation_id: u64,
}

impl BundleTx {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        from: Address,
        to: Address,
        value_wei: U256,
        data: Vec<u8>,
        gas_limit: u64,
        nonce: u64,
        chain_id: u64,
        bundle_correlation_id: u64,
    ) -> Self {
        Self {
            from,
            to,
            value_wei,
            data,
            gas_limit,
            nonce,
            chain_id,
            bundle_correlation_id,
        }
    }
}

/// Transparent `Vec<u8>` newtype for signed transaction bytes.
/// Public boundary type; Phase 6b populates with real signed bytes.
/// No `Default` (zero-byte signed payload is meaningless); no `Display`
/// (avoid accidental log of raw bytes — Phase 6 may add hex helpers).
#[repr(transparent)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedTxBytes(pub Vec<u8>);
