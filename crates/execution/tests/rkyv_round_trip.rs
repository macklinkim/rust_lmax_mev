//! Phase 3 P3-F test E-3: rkyv envelope round-trip on
//! `BundleCandidate` per the approved Batch F execution note v0.1
//! and the `docs/specs/event-model.md` mandatory derives. Mirrors the
//! P3-A ingress/state/replay round-trip pattern, the P3-C opportunity
//! envelope test, the P3-D RiskCheckedOpportunity test, and the P3-E
//! SimulationOutcome test; proves the spec-compliance derives plus
//! the per-crate `rkyv_compat::U256AsBytes` adapter work end-to-end
//! through `EventEnvelope<T>`.

use alloy_primitives::U256;
use rust_lmax_mev_execution::BundleCandidate;
use rust_lmax_mev_simulator::ProfitSource;
use rust_lmax_mev_types::{ChainContext, EventEnvelope, EventSource, PublishMeta};

#[test]
fn bundle_candidate_envelope_round_trips() {
    let candidate = BundleCandidate {
        opportunity_block_number: 18_000_000,
        gas_used: 21_002,
        simulated_profit_wei: U256::from(1_000_000_000_000_000u128), // 0.001 ETH
        gas_bid_wei: U256::from(900_000_000_000_000u128),            // 0.0009 ETH (90%)
        validity_block_min: 18_000_000,
        validity_block_max: 18_000_004,
        profit_source: ProfitSource::HeuristicPassthrough,
    };

    let meta = PublishMeta {
        source: EventSource::Execution,
        chain_context: ChainContext {
            chain_id: 1,
            block_number: 18_000_000,
            block_hash: [0xAB; 32],
        },
        event_version: 1,
        correlation_id: 7,
    };
    let envelope =
        EventEnvelope::seal(meta, candidate.clone(), 99, 1_700_000_000_000_000_000).unwrap();

    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&envelope).expect("rkyv serialize");
    let round_tripped: EventEnvelope<BundleCandidate> =
        rkyv::from_bytes::<EventEnvelope<BundleCandidate>, rkyv::rancor::Error>(&bytes)
            .expect("rkyv deserialize");

    assert_eq!(envelope, round_tripped);
    assert_eq!(round_tripped.payload(), &candidate);
}
