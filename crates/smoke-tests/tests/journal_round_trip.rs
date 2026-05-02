//! B-2 — ADR-008 check 6: write 1024 events to `FileJournal`, reread via
//! `iter_all`, assert bit-exact equality (per Batch C execution note v0.3
//! Risk Decision 5).
//!
//! 1024 events exercise the BufWriter flush boundary (default 8 KB
//! buffer) several times without the multi-MB I/O cost of 100k.

use rust_lmax_mev_journal::FileJournal;
use rust_lmax_mev_types::{
    ChainContext, EventEnvelope, EventSource, PublishMeta, SmokeTestPayload,
};

const TOTAL_EVENTS: u64 = 1024;

fn meta() -> PublishMeta {
    PublishMeta {
        source: EventSource::Ingress,
        chain_context: ChainContext {
            chain_id: 1,
            block_number: 18_000_000,
            block_hash: [0xAB; 32],
        },
        event_version: 1,
        correlation_id: 0,
    }
}

fn envelope(sequence: u64) -> EventEnvelope<SmokeTestPayload> {
    let mut data = [0u8; 32];
    data[0..8].copy_from_slice(&sequence.to_le_bytes());
    let payload = SmokeTestPayload {
        nonce: sequence,
        data,
    };
    // timestamp_ns is required != 0; use a deterministic value derived from
    // sequence so a corrupted-frame test would catch a swapped envelope.
    let timestamp_ns = 1_700_000_000_000_000_000u64 + sequence;
    EventEnvelope::seal(meta(), payload, sequence, timestamp_ns).expect("seal")
}

#[test]
fn file_journal_appends_and_iters_back_1024_events() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("journal.log");

    {
        let mut journal: FileJournal<SmokeTestPayload> =
            FileJournal::open(&path).expect("open new journal");
        for n in 0..TOTAL_EVENTS {
            journal.append(&envelope(n)).expect("append");
        }
        journal.flush().expect("flush");
        // journal drops here, releasing the file handle so reopen below
        // sees a fully-flushed file.
    }

    let reopened: FileJournal<SmokeTestPayload> = FileJournal::open(&path).expect("reopen");
    let read_back: Vec<_> = reopened
        .iter_all()
        .collect::<Result<Vec<_>, _>>()
        .expect("iter_all must succeed on a clean journal");

    assert_eq!(read_back.len(), TOTAL_EVENTS as usize, "envelope count");
    for (i, env) in read_back.iter().enumerate() {
        let n = i as u64;
        assert_eq!(env.sequence(), n, "sequence at index {i}");
        assert_eq!(env.payload().nonce, n, "nonce at index {i}");
        let mut expected = [0u8; 32];
        expected[0..8].copy_from_slice(&n.to_le_bytes());
        assert_eq!(env.payload().data, expected, "payload data at index {i}");
        assert_eq!(
            env.timestamp_ns(),
            1_700_000_000_000_000_000u64 + n,
            "timestamp_ns at index {i}"
        );
    }
}
