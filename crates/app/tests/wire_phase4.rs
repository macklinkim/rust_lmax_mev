//! Phase 3 P3-F tests for `wire_phase4` per the approved Batch F
//! execution note v0.1.
//!
//! - **W-1 deterministic shutdown**: bogus URL → wire_phase4 returns
//!   `Err(AppError::Node | Io)` within `tokio::time::timeout(5s)`. Same
//!   pattern as P2-D D-1 / P3-B B-1 (no live node needed).
//! - **W-2 broadcast Lagged fail-closed**: subscribe a journal-drain
//!   consumer to a small `tokio::sync::broadcast` channel; flood it
//!   past capacity without recv'ing; assert the consumer task exits
//!   on the next recv via `Lagged` (per P3-D v0.2 fail-closed policy).

mod common;

use std::time::Duration;

use alloy_primitives::{Address, Bytes, B256, U256};
use rust_lmax_mev_app::{journal_drain_loop, wire_phase4, AppError, WireOptions};
use rust_lmax_mev_ingress::{IngressEvent, MempoolEvent};
use rust_lmax_mev_journal::FileJournal;
use rust_lmax_mev_types::{ChainContext, EventEnvelope, EventSource, PublishMeta};
use tokio::sync::broadcast;

/// W-1 deterministic shutdown: bogus geth_http_url → Err(Node|Io)
/// within 5s. NodeProvider::connect parses URL synchronously so this
/// never touches the network.
#[tokio::test(flavor = "multi_thread")]
async fn wire_phase4_returns_error_for_bogus_geth_url() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = common::make_config(dir.path());
    config.node.geth_http_url = "not-a-url".to_string();

    let result = tokio::time::timeout(
        Duration::from_secs(5),
        wire_phase4(
            &config,
            WireOptions {
                init_observability: false,
            },
        ),
    )
    .await
    .expect("wire_phase4 must complete within 5s for an unparseable URL");

    match result {
        Err(AppError::Node(_)) | Err(AppError::Io(_)) => {}
        other => panic!("expected AppError::Node or AppError::Io, got {other:?}"),
    }
}

/// W-2 broadcast Lagged fail-closed: drive `journal_drain_loop` on a
/// small broadcast channel; flood past capacity without consuming;
/// then drive ONE recv → consumer must exit (not loop) per the v0.2
/// fail-closed policy. Asserts the consumer task `JoinHandle` resolves
/// within 2s after the lag is observed.
#[tokio::test(flavor = "multi_thread")]
async fn journal_drain_consumer_exits_on_broadcast_lagged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ingress_lag.log");
    let journal: FileJournal<IngressEvent> = FileJournal::open(&path).unwrap();

    // Capacity 4 channel — small so we can deliberately overflow it.
    let (tx, rx) = broadcast::channel::<EventEnvelope<IngressEvent>>(4);

    // Spawn the consumer. It will block on rx.recv() inside
    // journal_drain_loop until tx sends or drops.
    let consumer = tokio::spawn(journal_drain_loop("test-lag", rx, journal));

    // Send 16 envelopes WITHOUT giving the consumer a chance to recv;
    // tokio::broadcast overruns the receiver, marking it lagged.
    for i in 0..16u64 {
        let env = make_envelope(i);
        let _ = tx.send(env);
        // No await between sends; even if the consumer does manage to
        // recv a few, total send count >> capacity guarantees Lagged.
    }

    // Bound the join with a 2s timeout.
    let join = tokio::time::timeout(Duration::from_secs(2), consumer).await;
    assert!(
        join.is_ok(),
        "journal_drain_loop must exit within 2s after Lagged; instead the JoinHandle did not resolve"
    );
    join.unwrap()
        .expect("journal_drain_loop task itself must not panic on Lagged");
}

fn make_envelope(seq: u64) -> EventEnvelope<IngressEvent> {
    let payload = IngressEvent::Mempool(MempoolEvent {
        tx_hash: B256::from([0x01; 32]),
        from: Address::from([0x02; 20]),
        to: Some(Address::from([0x03; 20])),
        value: U256::from(1u64),
        input: Bytes::from(vec![0x04]),
        gas_limit: 21_000,
        max_fee: 1,
        observed_at_ns: 1,
    });
    let meta = PublishMeta {
        source: EventSource::Ingress,
        chain_context: ChainContext {
            chain_id: 1,
            block_number: 0,
            block_hash: [0; 32],
        },
        event_version: 1,
        correlation_id: 0,
    };
    EventEnvelope::seal(meta, payload, seq.max(1), 1).unwrap()
}
