//! A-1 happy: wire → publish 3 SmokeTestPayload events → shutdown →
//! reopen FileJournal at the same path → iter_all reads back 3
//! envelopes in publish order with matching nonces.
//!
//! Skips observability init (`init_observability: false`) so this test
//! does not contend with the global tracing / metrics recorder.

mod common;

use rust_lmax_mev_app::{wire, AppError, WireOptions};
use rust_lmax_mev_event_bus::EventBus;
use rust_lmax_mev_journal::FileJournal;
use rust_lmax_mev_types::{ChainContext, EventSource, PublishMeta, SmokeTestPayload};

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

fn payload(nonce: u64) -> SmokeTestPayload {
    SmokeTestPayload {
        nonce,
        data: [0xCD; 32],
    }
}

#[test]
fn run_wires_journal_and_consumer_then_shuts_down_on_drop() -> Result<(), AppError> {
    let dir = tempfile::tempdir().unwrap();
    let cfg = common::make_config(dir.path());
    let (journal_path, _) = common::paths(dir.path());

    let handle = wire(
        &cfg,
        WireOptions {
            init_observability: false,
        },
    )?;

    for n in 0..3u64 {
        handle.bus().publish(payload(n), meta())?;
    }

    handle.shutdown()?;

    let reopened: FileJournal<SmokeTestPayload> = FileJournal::open(&journal_path)?;
    let read_back: Vec<_> = reopened
        .iter_all()
        .collect::<Result<Vec<_>, _>>()
        .expect("iter_all must succeed on a clean journal");

    assert_eq!(
        read_back.len(),
        3,
        "expected 3 envelopes, got {}",
        read_back.len()
    );
    for (i, env) in read_back.iter().enumerate() {
        assert_eq!(env.payload().nonce, i as u64, "nonce mismatch at index {i}");
        assert_eq!(env.sequence(), i as u64, "sequence mismatch at index {i}");
    }
    Ok(())
}
