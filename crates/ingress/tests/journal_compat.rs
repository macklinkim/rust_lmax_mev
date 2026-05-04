//! Phase 3 P3-A spec-compliance test A-3: prove
//! `FileJournal<EventEnvelope<IngressEvent>>` open + append + iter_all
//! round-trips a `MempoolEvent` envelope. Confirms the journal's
//! `T: rkyv::Archive + Serialize<...>` bound is now satisfied for
//! ingress payloads — i.e., Phase 3 can wire a journal-drain consumer
//! on the ingress→state bus.

use alloy_primitives::{Address, Bytes, B256, U256};
use rust_lmax_mev_ingress::{IngressEvent, MempoolEvent};
use rust_lmax_mev_journal::FileJournal;
use rust_lmax_mev_types::{ChainContext, EventEnvelope, EventSource, PublishMeta};

#[test]
fn file_journal_round_trips_ingress_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ingress_journal.log");

    let payload = IngressEvent::Mempool(MempoolEvent {
        tx_hash: B256::from([0x77; 32]),
        from: Address::from([0x88; 20]),
        to: Some(Address::from([0x99; 20])),
        value: U256::from(987_654_321_u64),
        input: Bytes::from(vec![0xCA, 0xFE, 0xBA, 0xBE]),
        gas_limit: 50_000,
        max_fee: 50_000_000_000_u128,
        observed_at_ns: 1_700_000_001_000_000_000,
    });
    let meta = PublishMeta {
        source: EventSource::Ingress,
        chain_context: ChainContext {
            chain_id: 1,
            block_number: 18_000_002,
            block_hash: [0xCC; 32],
        },
        event_version: 1,
        correlation_id: 99,
    };
    let envelope = EventEnvelope::seal(meta, payload, 200, 1_700_000_001_000_000_000).unwrap();

    let mut journal: FileJournal<IngressEvent> = FileJournal::open(&path).unwrap();
    journal.append(&envelope).expect("append");
    journal.flush().expect("flush");

    let read: Vec<EventEnvelope<IngressEvent>> = journal
        .iter_all()
        .map(|r| r.expect("iter ok"))
        .collect();
    assert_eq!(read.len(), 1);
    assert_eq!(read[0], envelope);
    read[0].validate().expect("envelope passes Phase 1 invariants after round-trip");
}
