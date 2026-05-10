//! Phase 4 P4-D relay sim comparator infrastructure.
//!
//! Per the user-approved P4-D execution note v0.4 (manual Codex
//! APPROVED HIGH 2026-05-10 KST). Ships ADR-006 §"Abort policy"
//! types + a zero-tolerance comparator + a `RelaySimulator` async
//! trait + an in-memory `MockRelaySimulator` for tests.
//!
//! **HONEST CLOSEOUT CLAIM**: P4-D ships the comparator + type
//! infrastructure required by ADR-006 §"Abort policy". It does NOT
//! complete ADR-006's relay-sim mandate — the actual `eth_callBundle`
//! HTTP client and the relay-sim → comparator wiring into the
//! producer chain land in P4-E. P4-D's comparator is exercised
//! against the in-memory `MockRelaySimulator` only.
//!
//! Forbids reaffirmed: no `eth_sendBundle`, no production key material, no
//! signing, no `live_send=true`, no relay submission, no live
//! network tests, no `tracing::*` macros.

pub mod rkyv_compat;

use std::sync::Mutex;

use alloy_primitives::{B256, U256};
use async_trait::async_trait;
use rust_lmax_mev_simulator::{
    LocalStateFingerprint, SimStatus, SimulationOutcome, StateObservation,
};
use rust_lmax_mev_types::MismatchCategory;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------
// Type re-exports for ergonomics.
// ---------------------------------------------------------------

/// Per execution note v0.4 §R10: `RelaySimStatus` re-exports
/// `SimStatus` from `crates/simulator`. Single enum, single
/// maintenance point — the comparator's Revert detection is a
/// clean enum-equality check.
pub use rust_lmax_mev_simulator::SimStatus as RelaySimStatus;

// ---------------------------------------------------------------
// Local-side comparator inputs (peer to local SimulationOutcome).
// ---------------------------------------------------------------

/// Local-side bundle-shape inputs the comparator needs alongside
/// `SimulationOutcome` numeric fields. Per execution note v0.4 §R2.
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
pub struct LocalBundleShape {
    pub expected_inclusion_index: u32,
    #[rkyv(with = crate::rkyv_compat::U256AsBytes)]
    pub expected_coinbase_transfer_wei: U256,
}

// ---------------------------------------------------------------
// Relay-side request + outcome shapes.
// ---------------------------------------------------------------

/// Relay sim request payload. P4-D ships the shape; the test mock is
/// the only writer. The HTTP client in P4-E is the first real
/// producer (from the deferred `eth_callBundle` wiring).
///
/// `txs` is `Vec<Vec<u8>>` — see `rkyv_compat::txs`-rationale comment
/// (rkyv-friendly representation; the P4-E HTTP client converts
/// to/from `alloy_primitives::Bytes` at its own boundary).
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
pub struct RelaySimRequest {
    #[rkyv(with = crate::rkyv_compat::B256AsBytes)]
    pub block_hash: B256,
    pub state_block_number: u64,
    pub txs: Vec<Vec<u8>>,
}

/// Relay sim response payload. Mirrors local `SimulationOutcome`
/// shape (gas + status + profit) and adds bundle-shape fields per
/// §R2 (`inclusion_index` + `coinbase_transfer_wei`) plus the relay's
/// own observation set (sparse — relays only report what they touched).
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
pub struct RelaySimulationOutcome {
    pub gas_used: u64,
    pub status: SimStatus,
    #[rkyv(with = crate::rkyv_compat::U256AsBytes)]
    pub measured_profit_wei: U256,
    pub state_observations: Vec<StateObservation>,
    pub inclusion_index: u32,
    #[rkyv(with = crate::rkyv_compat::U256AsBytes)]
    pub coinbase_transfer_wei: U256,
}

/// Relay sim transport / configuration error. `Clone + PartialEq +
/// Eq` per §R15 (the `MockRelaySimulator` returns cloned programmed
/// results from a `Mutex<Result<...>>` cell on every call).
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RelaySimError {
    #[error("relay sim not configured")]
    NotConfigured,
    #[error("relay sim transport failure: {0}")]
    Transport(String),
    #[error("relay sim returned unrecognized payload: {0}")]
    UnrecognizedResponse(String),
    /// P4-E (R-E1): caller has no signed transactions to send (no
    /// signing infrastructure exists in P4-E). The adapter MUST
    /// surface this BEFORE issuing any HTTP request. Comparator
    /// classifies as `MismatchCategory::Unknown` via `compare_result`.
    /// Payload-free by design (DP-E11) — no string field that could
    /// leak secrets.
    #[error("unsigned bundle unavailable (P4-E has no signer)")]
    UnsignedBundleUnavailable,
}

/// Async, object-safe relay sim trait. The HTTP impl lands in P4-E
/// (Flashbots / bloXroute adapters using `eth_callBundle`); P4-D
/// ships only the trait + the in-memory `MockRelaySimulator`.
#[async_trait]
pub trait RelaySimulator: Send + Sync + 'static {
    async fn simulate_bundle(
        &self,
        req: RelaySimRequest,
    ) -> Result<RelaySimulationOutcome, RelaySimError>;
}

// ---------------------------------------------------------------
// Comparator — pure, zero-tolerance per ADR-006 §"Abort policy".
// ---------------------------------------------------------------

/// Inputs the comparator collects at the call site. Carries all the
/// local-side data needed for the five reachable
/// `MismatchCategory` checks (per `compare`'s precedence — see
/// `compare`'s docs).
pub struct ComparatorInputs<'a> {
    pub local: &'a SimulationOutcome,
    pub local_shape: &'a LocalBundleShape,
    pub local_fingerprint: &'a LocalStateFingerprint,
}

/// Pure-data comparator. Returns `Err(MismatchAbort)` per the
/// deterministic precedence chain documented in execution note v0.4
/// §DP-D17:
///
/// 1. `Revert` — `local.status == Success` XOR `relay.status == Success`.
/// 2. `BundleOutcome` — `inclusion_index` differs OR `coinbase_transfer_wei` differs.
/// 3. `StateDependency` — any slot in the intersection of `local_fingerprint.observations` × `relay.state_observations` disagrees on `value`. (Relay-omitted slots are NOT a mismatch — DP-D10.)
/// 4. `Profitability` — `local.simulated_profit_wei != relay.measured_profit_wei`.
/// 5. `Gas` — `local.gas_used != relay.gas_used`.
///
/// Rationale: "diagnose the cause, not the symptom" — Revert
/// short-circuits all numeric checks (one side reverted, comparing
/// gas/profit is meaningless); BundleOutcome means we're comparing
/// different bundles (inclusion order or coinbase delta differs);
/// StateDependency is the upstream cause of any downstream profit/
/// gas drift, so flag the cause rather than the symptom. CMP-10
/// constructs an input where ALL FIVE categories would individually
/// match and asserts `Revert` is returned per priority.
///
/// `Unknown` is NOT reachable from `compare` — only `compare_result`
/// can yield it, exclusively from a relay-side `RelaySimError`.
pub fn compare(
    inputs: ComparatorInputs<'_>,
    relay: &RelaySimulationOutcome,
) -> Result<(), Box<MismatchAbort>> {
    let local = inputs.local;
    let local_shape = inputs.local_shape;
    let local_fingerprint = inputs.local_fingerprint;

    let local_success = matches!(local.status, SimStatus::Success);
    let relay_success = matches!(relay.status, SimStatus::Success);

    // 1. Revert — liveness mismatch overshadows numeric checks.
    if local_success != relay_success {
        return Err(Box::new(MismatchAbort {
            category: MismatchCategory::Revert,
            detail: format!(
                "local.status (Success={local_success}) != relay.status (Success={relay_success})",
            ),
            local: local.clone(),
            local_shape: local_shape.clone(),
            local_fingerprint: local_fingerprint.clone(),
            relay: Some(relay.clone()),
        }));
    }

    // 2. BundleOutcome — inclusion order or coinbase transfer differs.
    if local_shape.expected_inclusion_index != relay.inclusion_index {
        return Err(Box::new(MismatchAbort {
            category: MismatchCategory::BundleOutcome,
            detail: format!(
                "expected_inclusion_index {} != relay.inclusion_index {}",
                local_shape.expected_inclusion_index, relay.inclusion_index,
            ),
            local: local.clone(),
            local_shape: local_shape.clone(),
            local_fingerprint: local_fingerprint.clone(),
            relay: Some(relay.clone()),
        }));
    }
    if local_shape.expected_coinbase_transfer_wei != relay.coinbase_transfer_wei {
        return Err(Box::new(MismatchAbort {
            category: MismatchCategory::BundleOutcome,
            detail: format!(
                "expected_coinbase_transfer_wei {} != relay.coinbase_transfer_wei {}",
                local_shape.expected_coinbase_transfer_wei, relay.coinbase_transfer_wei,
            ),
            local: local.clone(),
            local_shape: local_shape.clone(),
            local_fingerprint: local_fingerprint.clone(),
            relay: Some(relay.clone()),
        }));
    }

    // 3. StateDependency — intersection-only, per DP-D10. Slot keys
    //    present in only one side are NOT a mismatch (relay-omitted
    //    slots are inherently sparse).
    for local_obs in &local_fingerprint.observations {
        if let Some(relay_obs) = relay
            .state_observations
            .iter()
            .find(|r| r.account == local_obs.account && r.slot == local_obs.slot)
        {
            if relay_obs.value != local_obs.value {
                return Err(Box::new(MismatchAbort {
                    category: MismatchCategory::StateDependency,
                    detail: format!(
                        "slot {}@{:?}: local={:?}, relay={:?}",
                        local_obs.slot, local_obs.account, local_obs.value, relay_obs.value,
                    ),
                    local: local.clone(),
                    local_shape: local_shape.clone(),
                    local_fingerprint: local_fingerprint.clone(),
                    relay: Some(relay.clone()),
                }));
            }
        }
    }

    // 4. Profitability — zero-tolerance per ADR-006.
    if local.simulated_profit_wei != relay.measured_profit_wei {
        return Err(Box::new(MismatchAbort {
            category: MismatchCategory::Profitability,
            detail: format!(
                "local.simulated_profit_wei {} != relay.measured_profit_wei {}",
                local.simulated_profit_wei, relay.measured_profit_wei,
            ),
            local: local.clone(),
            local_shape: local_shape.clone(),
            local_fingerprint: local_fingerprint.clone(),
            relay: Some(relay.clone()),
        }));
    }

    // 5. Gas — zero-tolerance.
    if local.gas_used != relay.gas_used {
        return Err(Box::new(MismatchAbort {
            category: MismatchCategory::Gas,
            detail: format!(
                "local.gas_used {} != relay.gas_used {}",
                local.gas_used, relay.gas_used,
            ),
            local: local.clone(),
            local_shape: local_shape.clone(),
            local_fingerprint: local_fingerprint.clone(),
            relay: Some(relay.clone()),
        }));
    }

    Ok(())
}

/// Wrapper that classifies a relay-side error as
/// `MismatchCategory::Unknown` and otherwise delegates to `compare`.
/// This is the entry point the producer chain calls (the relay sim
/// returns `Result<RelaySimulationOutcome, RelaySimError>` and we
/// must turn EITHER outcome into an `Ok(())` or a `MismatchAbort`).
pub fn compare_result(
    inputs: ComparatorInputs<'_>,
    relay_result: Result<&RelaySimulationOutcome, &RelaySimError>,
) -> Result<(), Box<MismatchAbort>> {
    match relay_result {
        Ok(outcome) => compare(inputs, outcome),
        Err(err) => Err(Box::new(MismatchAbort {
            category: MismatchCategory::Unknown,
            detail: format!("relay sim error: {err}"),
            local: inputs.local.clone(),
            local_shape: inputs.local_shape.clone(),
            local_fingerprint: inputs.local_fingerprint.clone(),
            relay: None,
        })),
    }
}

/// Single canonical journalable abort record. Carries the category +
/// human-debuggable detail + the full local inputs (so the abort is
/// self-contained for the journal) + the relay outcome IFF the
/// abort came from `compare` (Unknown-from-error has `relay = None`).
/// Per execution note v0.4 §DP-D5 (R4 fix).
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
pub struct MismatchAbort {
    pub category: MismatchCategory,
    pub detail: String,
    pub local: SimulationOutcome,
    pub local_shape: LocalBundleShape,
    pub local_fingerprint: LocalStateFingerprint,
    pub relay: Option<RelaySimulationOutcome>,
}

// ---------------------------------------------------------------
// In-memory mock for tests + future P4-G driver wiring.
// ---------------------------------------------------------------

/// In-memory `RelaySimulator` impl. Programmed by the test caller;
/// returns the programmed `Result` (cloned) on every `simulate_bundle`
/// call. Default starts with `Err(NotConfigured)` (fail-closed
/// invariant per CMP-8).
pub struct MockRelaySimulator {
    programmed: Mutex<Result<RelaySimulationOutcome, RelaySimError>>,
}

impl Default for MockRelaySimulator {
    fn default() -> Self {
        Self {
            programmed: Mutex::new(Err(RelaySimError::NotConfigured)),
        }
    }
}

impl MockRelaySimulator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the `Result` future `simulate_bundle` calls will clone-out.
    pub fn program(&self, result: Result<RelaySimulationOutcome, RelaySimError>) {
        let mut slot = self.programmed.lock().expect("mock relay mutex poisoned");
        *slot = result;
    }
}

#[async_trait]
impl RelaySimulator for MockRelaySimulator {
    async fn simulate_bundle(
        &self,
        _req: RelaySimRequest,
    ) -> Result<RelaySimulationOutcome, RelaySimError> {
        let slot = self.programmed.lock().expect("mock relay mutex poisoned");
        slot.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::Address;
    use rust_lmax_mev_simulator::ProfitSource;

    // -------- Builders for canonical "matching" inputs ---------------

    fn sample_observation(account: u8, slot: u64, value: u8) -> StateObservation {
        StateObservation {
            account: Address::from([account; 20]),
            slot: U256::from(slot),
            value: B256::from([value; 32]),
        }
    }

    fn make_local(gas: u64, status: SimStatus, profit: u64) -> SimulationOutcome {
        SimulationOutcome {
            opportunity_block_number: 22_000_000,
            gas_used: gas,
            status,
            simulated_profit_wei: U256::from(profit),
            profit_source: ProfitSource::RevmComputed,
        }
    }

    fn make_local_shape() -> LocalBundleShape {
        LocalBundleShape {
            expected_inclusion_index: 0,
            expected_coinbase_transfer_wei: U256::ZERO,
        }
    }

    fn make_fingerprint() -> LocalStateFingerprint {
        LocalStateFingerprint {
            block_hash: B256::from([0xCD; 32]),
            observations: vec![
                sample_observation(0x10, 1, 0x01),
                sample_observation(0x10, 2, 0x02),
                sample_observation(0x20, 7, 0x07),
            ],
        }
    }

    fn make_relay(gas: u64, status: SimStatus, profit: u64) -> RelaySimulationOutcome {
        RelaySimulationOutcome {
            gas_used: gas,
            status,
            measured_profit_wei: U256::from(profit),
            state_observations: vec![
                sample_observation(0x10, 1, 0x01),
                sample_observation(0x10, 2, 0x02),
                sample_observation(0x20, 7, 0x07),
            ],
            inclusion_index: 0,
            coinbase_transfer_wei: U256::ZERO,
        }
    }

    fn inputs_for<'a>(
        local: &'a SimulationOutcome,
        local_shape: &'a LocalBundleShape,
        local_fingerprint: &'a LocalStateFingerprint,
    ) -> ComparatorInputs<'a> {
        ComparatorInputs {
            local,
            local_shape,
            local_fingerprint,
        }
    }

    // -------- CMP-1..10 tests ---------------------------------------

    /// CMP-1: identical local + relay outcomes → Ok(()).
    #[test]
    fn cmp_1_identical_inputs_ok() {
        let local = make_local(100_000, SimStatus::Success, 5_000);
        let shape = make_local_shape();
        let fp = make_fingerprint();
        let relay = make_relay(100_000, SimStatus::Success, 5_000);
        assert!(compare(inputs_for(&local, &shape, &fp), &relay).is_ok());
    }

    /// CMP-2: profit differs by 1 wei → Profitability (zero
    /// tolerance per ADR-006).
    #[test]
    fn cmp_2_one_wei_profit_delta_yields_profitability() {
        let local = make_local(100_000, SimStatus::Success, 5_000);
        let shape = make_local_shape();
        let fp = make_fingerprint();
        let relay = make_relay(100_000, SimStatus::Success, 5_001);
        let err = compare(inputs_for(&local, &shape, &fp), &relay).expect_err("CMP-2 must abort");
        assert_eq!(err.category, MismatchCategory::Profitability);
        assert!(err.relay.is_some());
    }

    /// CMP-3: gas differs (profit/state/shape equal) → Gas.
    #[test]
    fn cmp_3_gas_delta_yields_gas() {
        let local = make_local(100_000, SimStatus::Success, 5_000);
        let shape = make_local_shape();
        let fp = make_fingerprint();
        let relay = make_relay(100_001, SimStatus::Success, 5_000);
        let err = compare(inputs_for(&local, &shape, &fp), &relay).expect_err("CMP-3 must abort");
        assert_eq!(err.category, MismatchCategory::Gas);
    }

    /// CMP-4a: local Success vs relay Reverted → Revert.
    #[test]
    fn cmp_4a_local_success_relay_reverted_yields_revert() {
        let local = make_local(100_000, SimStatus::Success, 5_000);
        let shape = make_local_shape();
        let fp = make_fingerprint();
        let relay = make_relay(
            100_000,
            SimStatus::Reverted {
                reason_hex: "0x".into(),
            },
            5_000,
        );
        let err = compare(inputs_for(&local, &shape, &fp), &relay).expect_err("CMP-4a must abort");
        assert_eq!(err.category, MismatchCategory::Revert);
    }

    /// CMP-4b: local Reverted vs relay Success → Revert.
    #[test]
    fn cmp_4b_local_reverted_relay_success_yields_revert() {
        let local = make_local(
            100_000,
            SimStatus::Reverted {
                reason_hex: "0x".into(),
            },
            0,
        );
        let shape = make_local_shape();
        let fp = make_fingerprint();
        let relay = make_relay(100_000, SimStatus::Success, 0);
        let err = compare(inputs_for(&local, &shape, &fp), &relay).expect_err("CMP-4b must abort");
        assert_eq!(err.category, MismatchCategory::Revert);
    }

    /// CMP-5: one slot in the intersection disagrees on value →
    /// StateDependency.
    #[test]
    fn cmp_5_intersected_slot_value_disagrees_yields_state_dependency() {
        let local = make_local(100_000, SimStatus::Success, 5_000);
        let shape = make_local_shape();
        let fp = make_fingerprint();
        let mut relay = make_relay(100_000, SimStatus::Success, 5_000);
        // Mutate the relay's value at the same (account, slot) the
        // local fingerprint observed.
        relay.state_observations[1].value = B256::from([0xFF; 32]);
        let err = compare(inputs_for(&local, &shape, &fp), &relay).expect_err("CMP-5 must abort");
        assert_eq!(err.category, MismatchCategory::StateDependency);
    }

    /// CMP-5b (DP-D10 boundary): local fingerprint observed slot S;
    /// relay omitted S (different slot keys); nothing else differs →
    /// Ok(()). Relay-omitted slots are NOT a mismatch.
    #[test]
    fn cmp_5b_relay_omitted_slot_is_not_mismatch() {
        let local = make_local(100_000, SimStatus::Success, 5_000);
        let shape = make_local_shape();
        let fp = make_fingerprint();
        let mut relay = make_relay(100_000, SimStatus::Success, 5_000);
        // Drop relay observations entirely → relay reports zero slot
        // observations. Comparator must NOT flag this; it should
        // only compare the intersection (which is now empty).
        relay.state_observations.clear();
        assert!(
            compare(inputs_for(&local, &shape, &fp), &relay).is_ok(),
            "relay-omitted slots must not be a mismatch (DP-D10)"
        );
    }

    /// CMP-6a: inclusion_index differs → BundleOutcome.
    #[test]
    fn cmp_6a_inclusion_index_delta_yields_bundle_outcome() {
        let local = make_local(100_000, SimStatus::Success, 5_000);
        let shape = make_local_shape();
        let fp = make_fingerprint();
        let mut relay = make_relay(100_000, SimStatus::Success, 5_000);
        relay.inclusion_index = 1;
        let err = compare(inputs_for(&local, &shape, &fp), &relay).expect_err("CMP-6a must abort");
        assert_eq!(err.category, MismatchCategory::BundleOutcome);
    }

    /// CMP-6b: coinbase_transfer_wei differs → BundleOutcome.
    #[test]
    fn cmp_6b_coinbase_transfer_delta_yields_bundle_outcome() {
        let local = make_local(100_000, SimStatus::Success, 5_000);
        let shape = make_local_shape();
        let fp = make_fingerprint();
        let mut relay = make_relay(100_000, SimStatus::Success, 5_000);
        relay.coinbase_transfer_wei = U256::from(1u64);
        let err = compare(inputs_for(&local, &shape, &fp), &relay).expect_err("CMP-6b must abort");
        assert_eq!(err.category, MismatchCategory::BundleOutcome);
    }

    /// RS-N-1 (P4-E R-E1 + R-E11): UnsignedBundleUnavailable variant
    /// has stable Display message + is classified by compare_result as
    /// MismatchCategory::Unknown with MismatchAbort.relay = None. NO
    /// rkyv/serde round-trip — RelaySimError is not a journal payload.
    #[test]
    fn rs_n_1_unsigned_bundle_unavailable_classification() {
        // Display message stable (catches accidental wording changes).
        let display = format!("{}", RelaySimError::UnsignedBundleUnavailable);
        assert!(
            display.contains("unsigned bundle unavailable"),
            "Display wording must remain stable; got {display:?}"
        );

        // compare_result classifies as Unknown; MismatchAbort.relay is None.
        let local = make_local(100_000, SimStatus::Success, 5_000);
        let shape = make_local_shape();
        let fp = make_fingerprint();
        let err_in = RelaySimError::UnsignedBundleUnavailable;
        let err = compare_result(inputs_for(&local, &shape, &fp), Err(&err_in))
            .expect_err("RS-N-1: UnsignedBundleUnavailable must classify as Unknown abort");
        assert_eq!(err.category, MismatchCategory::Unknown);
        assert!(err.relay.is_none());
        assert!(
            err.detail.contains("unsigned bundle unavailable"),
            "MismatchAbort.detail must surface the relay-sim error wording; got {:?}",
            err.detail
        );
    }

    /// CMP-7: compare_result with relay Err(Transport(...)) →
    /// MismatchCategory::Unknown; relay is None on the abort.
    #[test]
    fn cmp_7_compare_result_transport_error_yields_unknown() {
        let local = make_local(100_000, SimStatus::Success, 5_000);
        let shape = make_local_shape();
        let fp = make_fingerprint();
        let err_in = RelaySimError::Transport("network down".into());
        let err = compare_result(inputs_for(&local, &shape, &fp), Err(&err_in))
            .expect_err("CMP-7 must abort");
        assert_eq!(err.category, MismatchCategory::Unknown);
        assert!(err.relay.is_none());
        assert!(err.detail.contains("network down"));
    }

    /// CMP-8: MockRelaySimulator::default() returns Err(NotConfigured)
    /// (fail-closed invariant); compare_result classifies as Unknown.
    #[test]
    fn cmp_8_mock_default_not_configured_yields_unknown() {
        let mock = MockRelaySimulator::default();
        let req = RelaySimRequest {
            block_hash: B256::from([0; 32]),
            state_block_number: 0,
            txs: vec![],
        };
        // Drive the async mock without a real runtime — it's a
        // single-future no-op so a noop_waker poll suffices.
        let fut = mock.simulate_bundle(req);
        let res = futures_block_on(fut);
        assert_eq!(res, Err(RelaySimError::NotConfigured));

        let local = make_local(100_000, SimStatus::Success, 5_000);
        let shape = make_local_shape();
        let fp = make_fingerprint();
        let err_in = RelaySimError::NotConfigured;
        let err = compare_result(inputs_for(&local, &shape, &fp), Err(&err_in))
            .expect_err("CMP-8 must abort");
        assert_eq!(err.category, MismatchCategory::Unknown);
        assert!(err.relay.is_none());
    }

    /// CMP-9: rkyv archive round-trip on a fully-populated
    /// MismatchAbort preserves byte-identical equality. Journal
    /// correctness gate.
    #[test]
    fn cmp_9_mismatch_abort_rkyv_round_trip() {
        let local = make_local(100_000, SimStatus::Success, 5_000);
        let shape = make_local_shape();
        let fp = make_fingerprint();
        let relay = make_relay(99_999, SimStatus::Success, 5_001);
        let original = MismatchAbort {
            category: MismatchCategory::Gas,
            detail: "test detail".into(),
            local,
            local_shape: shape,
            local_fingerprint: fp,
            relay: Some(relay),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("rkyv serialize");
        let decoded: MismatchAbort = rkyv::from_bytes::<MismatchAbort, rkyv::rancor::Error>(&bytes)
            .expect("rkyv deserialize");
        assert_eq!(original, decoded);
    }

    /// CMP-10 (R14 precedence): an input where ALL FIVE
    /// `compare`-reachable categories would individually match must
    /// return `Revert` per the documented precedence chain
    /// (`Revert > BundleOutcome > StateDependency > Profitability >
    /// Gas`).
    #[test]
    fn cmp_10_precedence_revert_short_circuits_all_others() {
        // Build inputs where every category would match:
        // - Revert: local Success, relay Reverted.
        // - BundleOutcome: inclusion_index differs (1 vs 0).
        // - StateDependency: intersected slot value differs.
        // - Profitability: profit differs.
        // - Gas: gas differs.
        let local = make_local(100_000, SimStatus::Success, 5_000);
        let shape = make_local_shape();
        let fp = make_fingerprint();
        let mut relay = make_relay(
            100_001, // gas differs
            SimStatus::Reverted {
                reason_hex: "0x".into(),
            },
            5_001, // profit differs
        );
        relay.inclusion_index = 1; // bundle-outcome differs
        relay.state_observations[0].value = B256::from([0xFF; 32]); // state differs
        relay.coinbase_transfer_wei = U256::from(7u64); // bundle-outcome again

        let err = compare(inputs_for(&local, &shape, &fp), &relay).expect_err("CMP-10 must abort");
        assert_eq!(
            err.category,
            MismatchCategory::Revert,
            "precedence: Revert must short-circuit BundleOutcome / StateDependency / Profitability / Gas"
        );
    }

    /// Trivial in-test future executor — avoids pulling tokio into
    /// the relay-sim test surface (the comparator + mock are
    /// runtime-agnostic; the production driver provides its own
    /// runtime in P4-E or P4-G).
    fn futures_block_on<F: std::future::Future>(mut fut: F) -> F::Output {
        use std::pin::Pin;
        use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

        // SAFETY: the noop waker is a no-op and the future is a
        // single-step async fn that never yields.
        const VTABLE: RawWakerVTable = RawWakerVTable::new(
            |_| RawWaker::new(std::ptr::null(), &VTABLE),
            |_| {},
            |_| {},
            |_| {},
        );
        let raw = RawWaker::new(std::ptr::null(), &VTABLE);
        // SAFETY: VTABLE has no-op fns matching the contract.
        let waker = unsafe { Waker::from_raw(raw) };
        let mut cx = Context::from_waker(&waker);
        // SAFETY: fut is owned by this stack frame; no movement after pin.
        let mut pinned = unsafe { Pin::new_unchecked(&mut fut) };
        loop {
            match pinned.as_mut().poll(&mut cx) {
                Poll::Ready(v) => return v,
                Poll::Pending => continue,
            }
        }
    }
}
