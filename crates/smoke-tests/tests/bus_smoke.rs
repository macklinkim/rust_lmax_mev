//! B-1 — ADR-008 check 5: 100k events through `CrossbeamBoundedBus` with
//! deterministic backpressure observation and non-deadlocking cleanup
//! (per Batch C execution note v0.3 Risk Decision 4).
//!
//! Pattern mirrors `crates/event-bus`'s T2 unit test scaled to 100k:
//! 1. Capacity 64; producer thread publishes 100_000 envelopes.
//! 2. Main polls `backpressure_total > 0` with 5s deadline; sets
//!    `timed_out` flag instead of panicking inline.
//! 3. ALWAYS spawn consumer + drain so producer can complete to 100k
//!    regardless of timeout state — this is what eliminates the
//!    cleanup-path deadlock risk Codex flagged on v0.2.
//! 4. Join producer; consumer exits after observing exactly TOTAL_EVENTS
//!    so we can read final stats before dropping the bus.
//! 5. AFTER joins succeed, panic on `timed_out` or assert on success.

use std::sync::Arc;
use std::time::{Duration, Instant};

use rust_lmax_mev_event_bus::{CrossbeamBoundedBus, EventBus, EventConsumer};
use rust_lmax_mev_types::{ChainContext, EventSource, PublishMeta, SmokeTestPayload};

const CAPACITY: usize = 64;
const TOTAL_EVENTS: u64 = 100_000;
const BACKPRESSURE_DEADLINE: Duration = Duration::from_secs(5);

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

fn payload(nonce: u64) -> SmokeTestPayload {
    SmokeTestPayload {
        nonce,
        data: [0xCD; 32],
    }
}

#[test]
fn bus_handles_100k_events_with_backpressure() {
    let (bus, consumer) =
        CrossbeamBoundedBus::<SmokeTestPayload>::new(CAPACITY).expect("capacity 64 valid");
    let bus = Arc::new(bus);

    // Step 1+2: producer publishes TOTAL_EVENTS sequentially. First CAPACITY
    // succeed instantly; next blocks at the crossbeam send.
    let bus_p = Arc::clone(&bus);
    let producer = std::thread::Builder::new()
        .name("smoke-producer".to_string())
        .spawn(move || {
            for n in 0..TOTAL_EVENTS {
                bus_p
                    .publish(payload(n), meta())
                    .expect("publish must succeed (channel closed only on consumer drop)");
            }
        })
        .expect("spawn producer");

    // Step 3: poll backpressure_total > 0 with deadline. Set timed_out flag
    // on expiry but DO NOT panic — fall through so cleanup runs.
    let mut timed_out = false;
    let deadline = Instant::now() + BACKPRESSURE_DEADLINE;
    loop {
        if bus.stats().backpressure_total > 0 {
            break;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            break;
        }
        std::thread::yield_now();
    }

    // Step 4: ALWAYS spawn consumer + drain regardless of timeout state.
    // Consumer exits after observing exactly TOTAL_EVENTS so the producer
    // can complete and we can read final stats before the bus is dropped.
    // This unblocks the producer no matter how full the channel is, which
    // eliminates the v0.2 cleanup-deadlock risk (single-slot drains are
    // insufficient at 100k scale).
    let consumer_thread = std::thread::Builder::new()
        .name("smoke-consumer".to_string())
        .spawn(move || {
            let mut received = 0u64;
            while received < TOTAL_EVENTS {
                consumer
                    .recv()
                    .expect("consumer recv must succeed for TOTAL_EVENTS");
                received += 1;
            }
            received
        })
        .expect("spawn consumer");

    // Step 5: join producer (it completes after publishing all because
    // consumer is now draining); join consumer (it exits after counting
    // TOTAL_EVENTS).
    producer.join().expect("producer thread panicked");
    let received = consumer_thread.join().expect("consumer thread panicked");

    // Snapshot stats while the bus is still alive in main. The
    // consumed_total counter is shared via Arc with the consumer, so
    // its increments are reflected here.
    let stats = bus.stats();
    drop(bus);

    // Step 6: inspect timed_out AFTER both joins succeed (no thread leaks
    // on either path).
    assert!(
        !timed_out,
        "backpressure_total never reached >= 1 within {:?}; received {} events; final stats: {:?}",
        BACKPRESSURE_DEADLINE, received, stats
    );
    assert_eq!(received, TOTAL_EVENTS, "consumer count");
    assert_eq!(stats.published_total, TOTAL_EVENTS, "published_total");
    assert_eq!(stats.consumed_total, TOTAL_EVENTS, "consumed_total");
    assert!(
        stats.backpressure_total >= 1,
        "backpressure_total must be >= 1, got {}",
        stats.backpressure_total
    );
    assert_eq!(stats.current_depth, 0, "current_depth at quiesce");
}
