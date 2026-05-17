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
/// `#[non_exhaustive]` so future fields (e.g., access list, blob-tx
/// fields) can be added without breaking downstream callers; use
/// [`BundleTx::new`] to construct.
///
/// P6B-CD D-CD1 added the two EIP-1559 fee fields
/// `max_priority_fee_per_gas` and `max_fee_per_gas` (BREAKING
/// `BundleTx::new(...)` 8 args -> 10 args). NO `access_list` field;
/// the RLP encoder hardcodes an empty access list. Non-empty access
/// lists remain future scope.
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
    /// P6B-CD D-CD1: EIP-1559 priority-fee bid (wei per gas). Must be
    /// `<= max_fee_per_gas`; `ProductionSigner::sign_tx` rejects with
    /// `Err(SignerError::InvalidBundleTx)` if the invariant is violated.
    pub max_priority_fee_per_gas: U256,
    /// P6B-CD D-CD1: EIP-1559 max total fee (wei per gas).
    pub max_fee_per_gas: U256,
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
        max_priority_fee_per_gas: U256,
        max_fee_per_gas: U256,
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
            max_priority_fee_per_gas,
            max_fee_per_gas,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// D-T-CD1: `BundleTx::new` accepts the 10-arg P6B-CD signature
    /// (8 P5-C args + 2 new EIP-1559 fee fields); field accessors
    /// return the values passed in.
    #[test]
    fn d_t_cd1_bundletx_eip1559_field_construction() {
        let tx = BundleTx::new(
            Address::from([0x11u8; 20]),
            Address::from([0x22u8; 20]),
            U256::from(1_000u64),
            vec![0xAA, 0xBB],
            21_000,
            7,
            1,
            0xDEAD_BEEFu64,
            U256::from(2u64),
            U256::from(10u64),
        );
        assert_eq!(tx.from, Address::from([0x11u8; 20]));
        assert_eq!(tx.to, Address::from([0x22u8; 20]));
        assert_eq!(tx.value_wei, U256::from(1_000u64));
        assert_eq!(tx.data, vec![0xAA, 0xBB]);
        assert_eq!(tx.gas_limit, 21_000);
        assert_eq!(tx.nonce, 7);
        assert_eq!(tx.chain_id, 1);
        assert_eq!(tx.bundle_correlation_id, 0xDEAD_BEEFu64);
        assert_eq!(tx.max_priority_fee_per_gas, U256::from(2u64));
        assert_eq!(tx.max_fee_per_gas, U256::from(10u64));
    }
}
