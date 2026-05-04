//! Phase 3 P3-B tests for `wire_phase3` per the approved Batch B
//! execution note v0.2 (Codex APPROVED HIGH 2026-05-04 16:06:52).
//!
//! - **B-1 failure** asserts `wire_phase3` returns `Err(AppError::Node | Io)`
//!   within a bounded timeout when `geth_http_url` is unparseable.
//! - **B-2 deterministic shutdown** drives `consume_loop` directly on a
//!   temp journal + bus, drops the bus, and asserts the consumer thread
//!   joins within 2 seconds. Proves the load-bearing shutdown semantic
//!   `wire_phase3` depends on without needing a NodeProvider mock.
//! - **B-3 boundary** is a compile-time assertion that
//!   `AppHandle3::shutdown` returns a `Future<Output = Result<(), AppError>>`,
//!   guarding the async-shutdown contract from accidental refactors.

mod common;

use std::sync::mpsc as std_mpsc;
use std::thread;
use std::time::{Duration, Instant};

use alloy_primitives::{Address, Bytes, B256, U256};
use rust_lmax_mev_app::{consume_loop, wire_phase3, AppError, AppHandle3, WireOptions};
use rust_lmax_mev_event_bus::{CrossbeamBoundedBus, EventBus};
use rust_lmax_mev_ingress::{IngressEvent, MempoolEvent};
use rust_lmax_mev_journal::FileJournal;
use rust_lmax_mev_types::{ChainContext, EventSource, PublishMeta};

/// B-1 failure: bogus `geth_http_url` (no scheme) → `wire_phase3`
/// returns `Err(AppError::Node | Io)` within `Duration::from_secs(5)`.
/// `NodeProvider::connect` does URL parse synchronously so this never
/// touches a network.
#[tokio::test(flavor = "multi_thread")]
async fn wire_phase3_returns_error_for_bogus_geth_url() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = common::make_config(dir.path());
    config.node.geth_http_url = "not-a-url".to_string();

    let result = tokio::time::timeout(
        Duration::from_secs(5),
        wire_phase3(
            &config,
            WireOptions {
                init_observability: false,
            },
        ),
    )
    .await
    .expect("wire_phase3 must complete within 5s for an unparseable URL");

    match result {
        Err(AppError::Node(_)) | Err(AppError::Io(_)) => {}
        other => panic!("expected AppError::Node or AppError::Io, got {other:?}"),
    }
}

/// B-2 deterministic shutdown: drive `consume_loop` directly on a temp
/// journal + bus, publish a couple of envelopes, drop the bus producer
/// handles, then assert the consumer thread joins within 2 seconds.
/// Proves the load-bearing shutdown semantic `wire_phase3` depends on
/// without a NodeProvider mock or live network.
#[test]
fn journal_drain_consumer_joins_after_bus_drop() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ingress.log");
    let journal: FileJournal<IngressEvent> = FileJournal::open(&path).unwrap();

    let (bus, consumer) = CrossbeamBoundedBus::<IngressEvent>::new(8).unwrap();

    // Publish 2 envelopes BEFORE handing the consumer to the thread, so
    // the loop has work to do on its first iterations.
    let meta = || PublishMeta {
        source: EventSource::Ingress,
        chain_context: ChainContext {
            chain_id: 1,
            block_number: 0,
            block_hash: [0; 32],
        },
        event_version: 1,
        correlation_id: 0,
    };
    let payload_a = IngressEvent::Mempool(MempoolEvent {
        tx_hash: B256::from([0x11; 32]),
        from: Address::from([0x22; 20]),
        to: Some(Address::from([0x33; 20])),
        value: U256::from(1u64),
        input: Bytes::from(vec![0x01]),
        gas_limit: 21_000,
        max_fee: 1,
        observed_at_ns: 1,
    });
    let payload_b = payload_a.clone();
    bus.publish(payload_a, meta()).expect("publish a");
    bus.publish(payload_b, meta()).expect("publish b");

    let join = thread::Builder::new()
        .name("p3b-consume-loop".to_string())
        .spawn(move || consume_loop(consumer, journal))
        .unwrap();

    // Drop the only producer-side bus handle. Consumer's recv() will
    // return Err(Closed) once the in-flight envelopes are drained.
    drop(bus);

    // Poll join() with a 2-second deadline. We can't directly time-box
    // std::thread::JoinHandle::join, so we wrap it via a helper thread
    // that signals on completion.
    let (tx, rx) = std_mpsc::channel();
    thread::spawn(move || {
        let _ = join.join();
        let _ = tx.send(());
    });

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut joined = false;
    while Instant::now() < deadline {
        if rx.try_recv().is_ok() {
            joined = true;
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        joined,
        "consume_loop thread did not join within 2s after bus drop"
    );
}

/// B-3 boundary: compile-time assertion that `AppHandle3::shutdown`
/// returns a `Future<Output = Result<(), AppError>>`. Catches accidental
/// refactors that demote shutdown back to a sync method (which would
/// re-introduce the Codex 16:00:13 hang risk).
#[allow(dead_code)]
fn _assert_async_shutdown(h: AppHandle3) -> impl std::future::Future<Output = Result<(), AppError>> {
    h.shutdown()
}
