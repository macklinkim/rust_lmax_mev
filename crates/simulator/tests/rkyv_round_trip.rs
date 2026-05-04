//! Phase 3 P3-E test S-4: rkyv envelope round-trip on
//! `SimulationOutcome` per the user-approved DP-S1 + the
//! `docs/specs/event-model.md` mandatory derives. Mirrors the P3-A
//! ingress/state/replay round-trip pattern, the P3-C opportunity test,
//! and the P3-D RiskCheckedOpportunity test; proves the spec-compliance
//! derives and the per-crate `rkyv_compat::U256AsBytes` adapter work
//! end-to-end through `EventEnvelope<T>`.

use alloy_primitives::U256;
use rust_lmax_mev_simulator::{ProfitSource, SimStatus, SimulationOutcome};
use rust_lmax_mev_types::{ChainContext, EventEnvelope, EventSource, PublishMeta};

#[test]
fn simulation_outcome_envelope_round_trips() {
    let outcome = SimulationOutcome {
        opportunity_block_number: 18_000_000,
        gas_used: 21_002,
        status: SimStatus::Success,
        simulated_profit_wei: U256::from(50_000_000_000_000u128),
        profit_source: ProfitSource::HeuristicPassthrough,
    };

    let meta = PublishMeta {
        source: EventSource::Simulator,
        chain_context: ChainContext {
            chain_id: 1,
            block_number: 18_000_000,
            block_hash: [0xAB; 32],
        },
        event_version: 1,
        correlation_id: 7,
    };
    let envelope =
        EventEnvelope::seal(meta, outcome.clone(), 99, 1_700_000_000_000_000_000).unwrap();

    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&envelope).expect("rkyv serialize");
    let round_tripped: EventEnvelope<SimulationOutcome> =
        rkyv::from_bytes::<EventEnvelope<SimulationOutcome>, rkyv::rancor::Error>(&bytes)
            .expect("rkyv deserialize");

    assert_eq!(envelope, round_tripped);
    assert_eq!(round_tripped.payload(), &outcome);
}
