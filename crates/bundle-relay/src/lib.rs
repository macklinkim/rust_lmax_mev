//! Phase 4 P4-E `BundleRelay` trait + payload types + `SubmitDisabled`
//! invariant. Per the user-approved P4-E execution note v0.6 (manual
//! Codex APPROVED HIGH 2026-05-10 KST).
//!
//! **HARD INVARIANT (DP-E1)**: every concrete-adapter
//! `BundleRelay::submit_bundle` impl in P4-E returns
//! `Err(BundleRelayError::SubmitDisabled)` UNCONDITIONALLY.
//!
//! **NO `crates/app` call site** invokes `submit_bundle` in P4-E
//! (CW-3 grep gate at batch close); the producer/comparator wiring
//! holds only `Arc<dyn RelaySimulator>` and never constructs
//! `Arc<dyn BundleRelay>` (DP-E6 v0.5 + DP-E13 v0.3).
//!
//! Phase 5 Safety Gate is the only path to enabling real submission.

pub mod rkyv_compat;

use async_trait::async_trait;
use rust_lmax_mev_relay_sim::RelaySimulator;
use serde::{Deserialize, Serialize};

use alloy_primitives::{Address, U256};

/// Carrier of one signed bundle ready for submission. P4-E ships
/// the type; the only writers in P4-E are tests + the deliberately
/// fail-closed adapter `submit_bundle` impls. Real signers + funded
/// keys land in Phase 5 Safety Gate.
///
/// Per-field rkyv adapters (R-E12): `Address` + `U256` cannot derive
/// rkyv natively. `block_hash` is `[u8; 32]` directly (rkyv-native).
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    Serialize,
    Deserialize,
)]
pub struct SignedBundle {
    pub block_hash: [u8; 32],
    pub state_block_number: u64,
    pub signed_txs: Vec<Vec<u8>>,
    #[rkyv(with = crate::rkyv_compat::AddressAsBytes)]
    pub coinbase_recipient: Address,
    #[rkyv(with = crate::rkyv_compat::U256AsBytes)]
    pub coinbase_transfer_wei: U256,
    pub validity_block_min: u64,
    pub validity_block_max: u64,
}

/// Receipt from a submission. P4-E ships the type; no writer exists
/// (every `submit_bundle` returns `Err(SubmitDisabled)`).
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    Serialize,
    Deserialize,
)]
pub struct SubmissionReceipt {
    pub relay_name: String,
    pub bundle_hash: String,
    pub submitted_at_unix_ns: u64,
}

/// `BundleRelay` operation errors. `#[non_exhaustive]` so future
/// submit-path codes land additively.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BundleRelayError {
    /// HARD INVARIANT (P4-E §DP-E1): every adapter `submit_bundle`
    /// returns this variant. There is no caller in P4-E that invokes
    /// `submit_bundle` (CW-3 grep gate). Phase 5 Safety Gate is the
    /// only path to enabling real submission. The Display message
    /// MUST contain the substring "Phase 5 Safety Gate" so a future
    /// PR that loosens the invariant is forced to also update test
    /// text + spec docs (BR-3 spec-drift guard).
    #[error("submit_bundle disabled in this build (Phase 5 Safety Gate required)")]
    SubmitDisabled,
}

/// Object-safe async trait for relay endpoints that expose both the
/// simulation and submission surfaces. P4-E adapters implement both
/// `RelaySimulator` and `BundleRelay`, but P4-E app/comparator wiring
/// stores them only as `Arc<dyn RelaySimulator>`; no `dyn BundleRelay`
/// object and no trait-object upcast is constructed in `crates/app`.
/// The `dyn BundleRelay` shape exists for concrete-adapter
/// submit-disabled tests and Phase 5+ submission consumers.
#[async_trait]
pub trait BundleRelay: RelaySimulator + Send + Sync + 'static {
    fn name(&self) -> &str;

    /// HARD INVARIANT (P4-E §DP-E1): every impl in P4-E returns
    /// `Err(BundleRelayError::SubmitDisabled)`. No call site exists
    /// in P4-E that invokes this method. Phase 5 Safety Gate is the
    /// only path to enabling real submission.
    async fn submit_bundle(
        &self,
        bundle: &SignedBundle,
    ) -> Result<SubmissionReceipt, BundleRelayError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_bundle() -> SignedBundle {
        SignedBundle {
            block_hash: [0xAB; 32],
            state_block_number: 22_000_000,
            signed_txs: vec![vec![0x01, 0x02, 0x03], vec![0xAA, 0xBB]],
            coinbase_recipient: Address::from([0xCD; 20]),
            coinbase_transfer_wei: U256::from(1_000_000u64),
            validity_block_min: 22_000_000,
            validity_block_max: 22_000_005,
        }
    }

    fn sample_receipt() -> SubmissionReceipt {
        SubmissionReceipt {
            relay_name: "flashbots".into(),
            bundle_hash: "0xdeadbeef".into(),
            submitted_at_unix_ns: 1_700_000_000_000_000_000,
        }
    }

    /// BR-1 (transitive via concrete-adapter tests in P4-E): trait
    /// is object-safe. Compile-asserted by the placeholder dyn
    /// reference below + the adapter trait-object construction in
    /// `crates/relay-clients/tests/submit_disabled.rs`.
    #[allow(dead_code)]
    fn br_1_object_safety_compile_check(_relay: &dyn BundleRelay) {}

    /// BR-2: SignedBundle + SubmissionReceipt rkyv + serde round-trip.
    #[test]
    fn br_2_signed_bundle_and_receipt_round_trip() {
        let original = sample_bundle();
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("rkyv serialize");
        let decoded: SignedBundle = rkyv::from_bytes::<SignedBundle, rkyv::rancor::Error>(&bytes)
            .expect("rkyv deserialize");
        assert_eq!(original, decoded);
        let bin = bincode::serialize(&original).expect("bincode serialize");
        let from_bin: SignedBundle = bincode::deserialize(&bin).expect("bincode deserialize");
        assert_eq!(original, from_bin);

        let receipt = sample_receipt();
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&receipt).expect("rkyv serialize");
        let decoded: SubmissionReceipt =
            rkyv::from_bytes::<SubmissionReceipt, rkyv::rancor::Error>(&bytes)
                .expect("rkyv deserialize");
        assert_eq!(receipt, decoded);
        let bin = bincode::serialize(&receipt).expect("bincode serialize");
        let from_bin: SubmissionReceipt = bincode::deserialize(&bin).expect("bincode deserialize");
        assert_eq!(receipt, from_bin);
    }

    /// BR-3 (spec-drift guard): SubmitDisabled Display message
    /// contains "Phase 5 Safety Gate" so loosening the invariant
    /// forces a test-text + spec-doc update.
    #[test]
    fn br_3_submit_disabled_display_contains_phase_5_safety_gate() {
        let err = BundleRelayError::SubmitDisabled;
        let display = format!("{err}");
        assert!(
            display.contains("Phase 5 Safety Gate"),
            "BR-3: SubmitDisabled Display must contain 'Phase 5 Safety Gate'; got {display:?}"
        );
    }
}
