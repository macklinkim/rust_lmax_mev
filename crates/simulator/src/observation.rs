//! Phase 4 P4-D observation types: `StateObservation` +
//! `LocalStateFingerprint`.
//!
//! Owned by `crates/simulator` (the read-set producer) per the P4-D
//! execution note v0.4 §R7. `crates/relay-sim` consumes these via
//! the existing `relay-sim → simulator` path-dep; no reverse edge,
//! no crate cycle.

use alloy_primitives::{Address, B256, U256};
use serde::{Deserialize, Serialize};

/// One observed storage read during local simulation. Carries the
/// `(account, slot, value)` triple revm reported when reading that
/// slot. `value` is normalized to a 32-byte big-endian word
/// (`B256`) so the comparator's intersection check is a clean byte
/// equality regardless of how either side encodes the U256 → bytes
/// rendering.
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
pub struct StateObservation {
    #[rkyv(with = crate::rkyv_compat::AddressAsBytes)]
    pub account: Address,
    #[rkyv(with = crate::rkyv_compat::U256AsBytes)]
    pub slot: U256,
    #[rkyv(with = crate::rkyv_compat::B256AsBytes)]
    pub value: B256,
}

/// Sparse read-set captured during ONE
/// `LocalSimulator::simulate_with_fingerprint` call. The `block_hash`
/// pins the snapshot the reads were taken against; the comparator
/// later intersects `observations` with the relay's own observation
/// list to detect `MismatchCategory::StateDependency` divergences.
///
/// "Sparse" is load-bearing: the relay's observation set is also
/// sparse (relays only report slots they themselves touched), so
/// the comparator only flags a mismatch when BOTH sides report the
/// same `(account, slot)` and disagree on `value`. Slots reported by
/// only one side are not a mismatch (see comparator DP-D10).
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
pub struct LocalStateFingerprint {
    #[rkyv(with = crate::rkyv_compat::B256AsBytes)]
    pub block_hash: B256,
    pub observations: Vec<StateObservation>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_observation() -> StateObservation {
        StateObservation {
            account: Address::from([0x11u8; 20]),
            slot: U256::from(0x42u64),
            value: B256::from([0xAAu8; 32]),
        }
    }

    fn sample_fingerprint() -> LocalStateFingerprint {
        LocalStateFingerprint {
            block_hash: B256::from([0xCDu8; 32]),
            observations: vec![
                sample_observation(),
                StateObservation {
                    account: Address::from([0x22u8; 20]),
                    slot: U256::from(0x100u64),
                    value: B256::from([0xBBu8; 32]),
                },
            ],
        }
    }

    /// OBS-1: `StateObservation` rkyv archive round-trip preserves all
    /// three fields (account / slot / value) byte-identically.
    #[test]
    fn obs_1_state_observation_rkyv_round_trip() {
        let original = sample_observation();
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("rkyv serialize");
        let decoded: StateObservation =
            rkyv::from_bytes::<StateObservation, rkyv::rancor::Error>(&bytes)
                .expect("rkyv deserialize");
        assert_eq!(original, decoded);
    }

    /// OBS-2: `LocalStateFingerprint` rkyv archive round-trip preserves
    /// `block_hash` + the full `observations` Vec.
    #[test]
    fn obs_2_local_state_fingerprint_rkyv_round_trip() {
        let original = sample_fingerprint();
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("rkyv serialize");
        let decoded: LocalStateFingerprint =
            rkyv::from_bytes::<LocalStateFingerprint, rkyv::rancor::Error>(&bytes)
                .expect("rkyv deserialize");
        assert_eq!(original, decoded);
        assert_eq!(decoded.observations.len(), 2);
    }
}
