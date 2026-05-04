//! Phase 3 P3-A spec-compliance test A-2: rkyv round-trip for
//! `PoolState::UniV2` / `PoolState::UniV3` and the full
//! `StateUpdateEvent` wrapped in `EventEnvelope<StateUpdateEvent>`.
//! Confirms the new derives + per-crate `rkyv_compat` adapters survive
//! a full envelope serialize/deserialize cycle.

use alloy_primitives::{Address, B256, U256};
use rust_lmax_mev_config::PoolKind;
use rust_lmax_mev_state::{PoolId, PoolState, StateUpdateEvent};
use rust_lmax_mev_types::{ChainContext, EventEnvelope, EventSource, PublishMeta};

fn meta() -> PublishMeta {
    PublishMeta {
        source: EventSource::StateEngine,
        chain_context: ChainContext {
            chain_id: 1,
            block_number: 18_000_010,
            block_hash: [0xAB; 32],
        },
        event_version: 1,
        correlation_id: 42,
    }
}

fn rkyv_round_trip<T>(value: &T) -> T
where
    T: rkyv::Archive
        + for<'a> rkyv::Serialize<
            rkyv::api::high::HighSerializer<
                rkyv::util::AlignedVec,
                rkyv::ser::allocator::ArenaHandle<'a>,
                rkyv::rancor::Error,
            >,
        >,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(value).expect("rkyv serialize");
    rkyv::from_bytes::<T, rkyv::rancor::Error>(&bytes).expect("rkyv deserialize")
}

#[test]
fn univ2_state_update_envelope_round_trips() {
    let payload = StateUpdateEvent {
        block_number: 18_000_010,
        block_hash: B256::from([0x11; 32]),
        pool: PoolId {
            kind: PoolKind::UniswapV2,
            address: Address::from([0x22; 20]),
        },
        state: PoolState::UniV2 {
            reserve0: U256::from(1_000_000_000_000_000_000_u128),
            reserve1: U256::from(2_500_000_000_u64),
            block_timestamp_last: 1_700_000_000,
        },
    };
    let envelope =
        EventEnvelope::seal(meta(), payload.clone(), 200, 1_700_000_010_000_000_000).unwrap();
    let round_tripped = rkyv_round_trip(&envelope);
    assert_eq!(envelope, round_tripped);
    assert_eq!(round_tripped.payload(), &payload);
}

#[test]
fn univ3_state_update_envelope_round_trips() {
    let payload = StateUpdateEvent {
        block_number: 18_000_011,
        block_hash: B256::from([0x33; 32]),
        pool: PoolId {
            kind: PoolKind::UniswapV3Fee005,
            address: Address::from([0x44; 20]),
        },
        state: PoolState::UniV3 {
            sqrt_price_x96: U256::from(0x123456789abcdef_u64) << 32,
            tick: -123_456,
            liquidity: 9_876_543_210_123_456_789_u128,
        },
    };
    let envelope =
        EventEnvelope::seal(meta(), payload.clone(), 201, 1_700_000_011_000_000_000).unwrap();
    let round_tripped = rkyv_round_trip(&envelope);
    assert_eq!(envelope, round_tripped);
    assert_eq!(round_tripped.payload(), &payload);
}
