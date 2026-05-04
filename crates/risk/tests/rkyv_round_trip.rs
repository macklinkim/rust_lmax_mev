//! Phase 3 P3-D test R-7: rkyv envelope round-trip on
//! `RiskCheckedOpportunity` per the approved Batch D execution note v0.2
//! + `docs/specs/event-model.md` mandatory derives. Mirrors the P3-A
//! ingress/state/replay round-trip pattern + the P3-C opportunity
//! envelope test; proves the spec-compliance derives + per-crate
//! `rkyv_compat::U256AsBytes` adapter work end-to-end through
//! `EventEnvelope<T>`.

use alloy_primitives::{Address, B256, U256};
use rust_lmax_mev_opportunity::{OpportunityEvent, GAS_ESTIMATE_TWO_HOP_ARB};
use rust_lmax_mev_risk::RiskCheckedOpportunity;
use rust_lmax_mev_state::{PoolId, PoolKind};
use rust_lmax_mev_types::{ChainContext, EventEnvelope, EventSource, PublishMeta};

#[test]
fn risk_checked_opportunity_envelope_round_trips() {
    let opp = OpportunityEvent {
        block_number: 18_000_000,
        block_hash: B256::from([0xAB; 32]),
        source_pool: PoolId {
            kind: PoolKind::UniswapV2,
            address: Address::from([0xB4; 20]),
        },
        sink_pool: PoolId {
            kind: PoolKind::UniswapV3Fee005,
            address: Address::from([0x88; 20]),
        },
        optimal_amount_in_wei: U256::from(10_000_000_000_000_000u128), // 0.01 ETH
        expected_profit_wei: U256::from(50_000_000_000_000u128),       // 0.00005 ETH
        gas_estimate: GAS_ESTIMATE_TWO_HOP_ARB,
    };
    let checked = RiskCheckedOpportunity {
        opportunity: opp,
        size_wei: U256::from(10_000_000_000_000_000u128),
    };

    let meta = PublishMeta {
        source: EventSource::RiskEngine,
        chain_context: ChainContext {
            chain_id: 1,
            block_number: 18_000_000,
            block_hash: [0xAB; 32],
        },
        event_version: 1,
        correlation_id: 7,
    };
    let envelope =
        EventEnvelope::seal(meta, checked.clone(), 99, 1_700_000_000_000_000_000).unwrap();

    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&envelope).expect("rkyv serialize");
    let round_tripped: EventEnvelope<RiskCheckedOpportunity> =
        rkyv::from_bytes::<EventEnvelope<RiskCheckedOpportunity>, rkyv::rancor::Error>(&bytes)
            .expect("rkyv deserialize");

    assert_eq!(envelope, round_tripped);
    assert_eq!(round_tripped.payload(), &checked);
}
