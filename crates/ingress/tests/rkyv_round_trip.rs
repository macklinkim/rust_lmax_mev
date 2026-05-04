//! Phase 3 P3-A spec-compliance test A-1: rkyv round-trip for
//! `IngressEvent::Mempool(MempoolEvent)` and `IngressEvent::Block(BlockEvent)`
//! wrapped in `EventEnvelope<IngressEvent>`. Confirms the new derives +
//! `rkyv_compat` adapters survive a full envelope serialize/deserialize
//! cycle and reproduce the input byte-equal.

use alloy_primitives::{Address, Bytes, B256, U256};
use rust_lmax_mev_ingress::{BlockEvent, IngressEvent, MempoolEvent};
use rust_lmax_mev_types::{ChainContext, EventEnvelope, EventSource, PublishMeta};

fn meta() -> PublishMeta {
    PublishMeta {
        source: EventSource::Ingress,
        chain_context: ChainContext {
            chain_id: 1,
            block_number: 18_000_000,
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
fn mempool_event_envelope_round_trips() {
    let payload = IngressEvent::Mempool(MempoolEvent {
        tx_hash: B256::from([0x11; 32]),
        from: Address::from([0x22; 20]),
        to: Some(Address::from([0x33; 20])),
        value: U256::from(123_456_789_u64),
        input: Bytes::from(vec![0xDE, 0xAD, 0xBE, 0xEF]),
        gas_limit: 21_000,
        max_fee: 30_000_000_000_u128,
        observed_at_ns: 1_700_000_000_000_000_000,
    });
    let envelope =
        EventEnvelope::seal(meta(), payload.clone(), 100, 1_700_000_000_000_000_000).unwrap();
    let round_tripped = rkyv_round_trip(&envelope);
    assert_eq!(envelope, round_tripped);
    assert_eq!(round_tripped.payload(), &payload);
}

#[test]
fn block_event_envelope_round_trips() {
    let payload = IngressEvent::Block(BlockEvent {
        block_number: 18_000_001,
        block_hash: B256::from([0xAA; 32]),
        parent_hash: B256::from([0xBB; 32]),
        timestamp_ns: 1_700_000_000_500_000_000,
    });
    let envelope =
        EventEnvelope::seal(meta(), payload.clone(), 101, 1_700_000_000_500_000_000).unwrap();
    let round_tripped = rkyv_round_trip(&envelope);
    assert_eq!(envelope, round_tripped);
    assert_eq!(round_tripped.payload(), &payload);
}

#[test]
fn mempool_event_with_none_to_round_trips() {
    let payload = IngressEvent::Mempool(MempoolEvent {
        tx_hash: B256::ZERO,
        from: Address::ZERO,
        to: None,
        value: U256::ZERO,
        input: Bytes::new(),
        gas_limit: 0,
        max_fee: 0,
        observed_at_ns: 1,
    });
    let envelope = EventEnvelope::seal(meta(), payload.clone(), 102, 2).unwrap();
    let round_tripped = rkyv_round_trip(&envelope);
    assert_eq!(envelope, round_tripped);
    assert_eq!(round_tripped.payload(), &payload);
}
