//! # rust-lmax-mev-event-bus
//!
//! Phase 1 single-consumer bounded event bus for the LMAX-style MEV engine.
//!
//! ## Topology
//!
//! Each domain pipeline stage boundary owns one `(CrossbeamBoundedBus<T>,
//! CrossbeamConsumer<T>)` pair. There is **no global engine-wide queue** —
//! the number of bus instances is decided by the wiring crate (Task 16,
//! `crates/app`) and each instance carries one ordered stream.
//!
//! Multi-consumer cursors, broadcast semantics, and `subscribe(name)`-style
//! APIs are explicitly forbidden in Phase 1 and are deferred to Phase 2 per
//! ADR-005 §"Phase 1 baseline" + `PHASE_1_DETAIL_REVISION` §3.1.
//!
//! ## Sequence and timestamp ownership
//!
//! `EventEnvelope::sequence` and `EventEnvelope::timestamp_ns` are
//! **bus-assigned**. Callers pass `PublishMeta` and a payload to
//! [`EventBus::publish`]; the bus internally calls
//! [`rust_lmax_mev_types::EventEnvelope::seal`] with values it owns and
//! returns a [`PublishAck`] containing the assigned sequence and timestamp.
//! Envelope itself flows only to the consumer side — the publish path is
//! clone-free.
//!
//! `timestamp_ns` is captured at publish attempt time before a potentially
//! blocking send. It may therefore be earlier than the actual enqueue/receive
//! time. Ordering is defined by `sequence`, not timestamp monotonicity.
//!
//! ## Publish-path serialization
//!
//! The publish path is serialized end-to-end by a `parking_lot::Mutex`. The
//! lock is held across the optional blocking `send`, which is intentional:
//! it ensures sequence assignment order matches channel arrival order, and
//! it propagates backpressure to all publishers via the mutex. The mutex
//! does **not** guarantee strict FIFO acquisition fairness.
//!
//! [`EventBus::stats`] **must not** acquire the publish-state mutex. It
//! reads only atomics and the channel `len`/`capacity`. This is a hard
//! correctness requirement: tests (and operational diagnostics) must be
//! able to call `stats()` while another thread holds the lock inside a
//! blocking `send`.
//!
//! ## Sequence-exhaustion policy
//!
//! `u64::MAX` is reserved as the exhaustion sentinel and is **never
//! published**. Phase 1 publishable sequence range is `0..u64::MAX`
//! (half-open); the maximum published value is `u64::MAX - 1`. When
//! `next_sequence` reaches `u64::MAX`, [`EventBus::publish`] returns
//! [`BusError::SequenceExhausted`] terminally — the bus instance must be
//! discarded. Silent wrap is forbidden.
//!
//! ## Retry safety
//!
//! `state.next_sequence` advances **only on publish success**. All failed
//! publish paths leave the counter unchanged, so the next publish attempt
//! reuses the would-have-been sequence. Among the failure modes:
//!
//! - [`BusError::ClockUnavailable`] and [`BusError::Envelope`] are
//!   **retryable** — the underlying problem (system clock or caller-supplied
//!   `PublishMeta`) can be corrected and the same publish re-attempted.
//! - [`BusError::Closed`] is **terminal** — the consumer has dropped and no
//!   further publish on this bus can succeed; the caller should discard the
//!   bus instance. The "does not consume a sequence" rule still applies, so
//!   a Closed-failed publish leaves `next_sequence` unchanged for the
//!   bookkeeping symmetry.
//! - [`BusError::SequenceExhausted`] is also **terminal** — the bus has
//!   reached `u64::MAX` and must be discarded.
//!
//! This is the foundation of the retry-safety invariant verified by T6.
//!
//! ## Phase 2 extension policy
//!
//! [`BusStats`] and [`BusError`] both carry `#[non_exhaustive]` so future
//! Phase 2 work (e.g. backpressure timeout variants, additional metric
//! fields) can land additively. New impls of [`EventBus`] / [`EventConsumer`]
//! (e.g. lock-free ring buffer, cursor-aware multi-consumer) are also
//! additive — gated on benchmark proof per ADR-005.

use rust_lmax_mev_types::TypesError;

/// Returned from `publish` on success.
///
/// Carries only the bus-assigned identity of the published event; the
/// envelope itself flows exclusively to the consumer side. Keeping the
/// publish path clone-free is the reason `publish` does not return
/// `EventEnvelope<T>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublishAck {
    pub sequence: u64,
    pub timestamp_ns: u64,
}

/// In-process readable bus snapshot.
///
/// Comprised of three counters (`published_total`, `consumed_total`,
/// `backpressure_total`), one gauge (`current_depth`), and one structural
/// quantity (`capacity`).
///
/// Each field is read independently from its underlying atomic or channel,
/// so the returned value is an **observability sample, not a linearizable
/// transaction snapshot** — concurrent publish / recv may interleave between
/// the per-field reads.
///
/// Marked `#[non_exhaustive]` so downstream callers cannot rely on
/// exhaustive construction or matching; Phase 2 may add fields while
/// callers continue to read the values through `stats()` and pattern-match
/// with `..` rest patterns. (Note: `#[non_exhaustive]` on a public struct
/// forbids struct-literal construction outside this crate, which is the
/// desired contract here — `BusStats` is produced only by
/// [`EventBus::stats`].)
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusStats {
    pub published_total: u64,
    pub consumed_total: u64,
    pub backpressure_total: u64,
    pub current_depth: usize,
    pub capacity: usize,
}

/// Errors returned by [`EventBus`] / [`EventConsumer`] operations.
///
/// Marked `#[non_exhaustive]` so Phase 2 may add variants (e.g.
/// `BackpressureTimeout`, `MetricsBackendUnavailable`) without breaking
/// downstream `match` consumers — they will route the new variants
/// through the `_ =>` arm.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum BusError {
    #[error("invalid capacity: {0} (must be > 0)")]
    InvalidCapacity(usize),

    #[error("system clock unavailable, pre-epoch, or timestamp out of range")]
    ClockUnavailable,

    #[error("sequence counter exhausted (u64::MAX reached)")]
    SequenceExhausted,

    #[error("envelope construction rejected: {0}")]
    Envelope(#[from] TypesError),

    #[error("channel closed: peer dropped")]
    Closed,
}

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crossbeam_channel::{Receiver, Sender};
use parking_lot::Mutex;

use rust_lmax_mev_types::{EventEnvelope, PublishMeta};

// === Traits ===

/// Producer-side handle to a single ordered event stream.
///
/// Domain events block on a full queue; telemetry-channel saturation drops
/// are the responsibility of a separate channel (Task 15) and are not
/// modeled here. See ADR-005 for the policy.
pub trait EventBus<T>: Send + Sync
where
    T: Send + 'static,
{
    /// Publishes `payload` with the caller-supplied `meta`. Returns the
    /// bus-assigned identity. The envelope itself flows only to the
    /// consumer; the publish path is clone-free.
    ///
    /// On success, `state.next_sequence` is advanced by 1. On any error,
    /// the counter is unchanged — see the crate-level "Retry safety" section.
    fn publish(&self, payload: T, meta: PublishMeta) -> Result<PublishAck, BusError>;

    /// Returns the current channel depth (`sender.len()`).
    fn len(&self) -> usize;

    /// Returns the configured channel capacity (set at `new()` time).
    fn capacity(&self) -> usize;

    /// Returns an in-process snapshot of the four metrics + structural
    /// quantities. **Must not acquire the publish-state mutex** — reads
    /// only the atomic counters and the channel `len`/`capacity`.
    fn stats(&self) -> BusStats;
}

/// Consumer-side handle to a single ordered event stream.
///
/// `CrossbeamConsumer<T>` (the only Phase 1 impl) does not implement
/// `Clone`. The single-consumer contract is enforced by construction:
/// `CrossbeamBoundedBus::new` returns exactly one consumer per bus.
///
/// Sharing a consumer across multiple worker threads (which would
/// distribute order-dependent processing) is **outside** the Phase 1
/// contract. Producer-side sharing across threads is supported via
/// `Arc<CrossbeamBoundedBus<T>>`.
pub trait EventConsumer<T>: Send + Sync
where
    T: Send + 'static,
{
    /// Blocks until an envelope is available or the bus is dropped.
    fn recv(&self) -> Result<EventEnvelope<T>, BusError>;

    /// Returns immediately. `Ok(None)` indicates an empty queue and is
    /// **not an error**; `consumed_total` is not advanced on this branch.
    fn try_recv(&self) -> Result<Option<EventEnvelope<T>>, BusError>;

    /// Returns the current channel depth (`receiver.len()`).
    fn len(&self) -> usize;
}

// === Internal state ===

struct PublishState {
    next_sequence: u64,
}

// === Concrete impls ===

pub struct CrossbeamBoundedBus<T> {
    sender: Sender<EventEnvelope<T>>,
    state: Mutex<PublishState>,
    published_total: AtomicU64,
    backpressure_total: AtomicU64,
    consumed_total: Arc<AtomicU64>,
    capacity: usize,
}

pub struct CrossbeamConsumer<T> {
    receiver: Receiver<EventEnvelope<T>>,
    consumed_total: Arc<AtomicU64>,
}

fn now_ns() -> Result<u64, BusError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| u64::try_from(d.as_nanos()).ok())
        .filter(|&ns| ns != 0)
        .ok_or(BusError::ClockUnavailable)
}

/// Centralized `usize → f64` cast for the `current_depth` gauge.
///
/// Phase 1 capacities are well below 2^53, so precision loss does not apply
/// (per spec §8.1). Centralizing the cast keeps the `#[allow(...)]` rationale
/// in one place rather than scattered across publish/recv call sites.
#[allow(clippy::cast_precision_loss)]
fn depth_as_f64(depth: usize) -> f64 {
    depth as f64
}

impl<T> CrossbeamBoundedBus<T>
where
    T: Send + 'static,
{
    /// Constructs a paired `(bus, consumer)`.
    ///
    /// `capacity == 0` is rejected with `BusError::InvalidCapacity(0)`.
    /// Crossbeam's zero-capacity rendezvous semantics are deliberately
    /// excluded — they conflict with the Phase 1 metric definitions of
    /// `current_depth`, `capacity`, and `backpressure_total`.
    pub fn new(capacity: usize) -> Result<(Self, CrossbeamConsumer<T>), BusError> {
        if capacity == 0 {
            return Err(BusError::InvalidCapacity(0));
        }
        let (sender, receiver) = crossbeam_channel::bounded(capacity);
        let consumed_total = Arc::new(AtomicU64::new(0));
        let bus = CrossbeamBoundedBus {
            sender,
            state: Mutex::new(PublishState { next_sequence: 0 }),
            published_total: AtomicU64::new(0),
            backpressure_total: AtomicU64::new(0),
            consumed_total: Arc::clone(&consumed_total),
            capacity,
        };
        let consumer = CrossbeamConsumer {
            receiver,
            consumed_total,
        };
        Ok((bus, consumer))
    }
}

impl<T> EventBus<T> for CrossbeamBoundedBus<T>
where
    T: Send + 'static,
{
    fn publish(&self, payload: T, meta: PublishMeta) -> Result<PublishAck, BusError> {
        let mut state = self.state.lock();

        let timestamp_ns = now_ns()?;

        let sequence = state.next_sequence;
        if sequence == u64::MAX {
            return Err(BusError::SequenceExhausted);
        }

        let envelope = EventEnvelope::seal(meta, payload, sequence, timestamp_ns)?;

        match self.sender.try_send(envelope) {
            Ok(()) => { /* fast path */ }
            Err(crossbeam_channel::TrySendError::Full(env)) => {
                self.backpressure_total.fetch_add(1, Ordering::Relaxed);
                metrics::counter!("event_bus_backpressure_total").increment(1);
                self.sender.send(env).map_err(|_| BusError::Closed)?;
            }
            Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                return Err(BusError::Closed);
            }
        }

        state.next_sequence = sequence + 1;
        self.published_total.fetch_add(1, Ordering::Relaxed);
        metrics::counter!("event_bus_published_total").increment(1);
        metrics::gauge!("event_bus_current_depth").set(depth_as_f64(self.sender.len()));

        Ok(PublishAck {
            sequence,
            timestamp_ns,
        })
    }

    fn len(&self) -> usize {
        self.sender.len()
    }

    fn capacity(&self) -> usize {
        self.capacity
    }

    fn stats(&self) -> BusStats {
        BusStats {
            published_total: self.published_total.load(Ordering::Relaxed),
            consumed_total: self.consumed_total.load(Ordering::Relaxed),
            backpressure_total: self.backpressure_total.load(Ordering::Relaxed),
            current_depth: self.sender.len(),
            capacity: self.capacity,
        }
    }
}

impl<T> EventConsumer<T> for CrossbeamConsumer<T>
where
    T: Send + 'static,
{
    fn recv(&self) -> Result<EventEnvelope<T>, BusError> {
        match self.receiver.recv() {
            Ok(env) => {
                self.consumed_total.fetch_add(1, Ordering::Relaxed);
                metrics::counter!("event_bus_consumed_total").increment(1);
                metrics::gauge!("event_bus_current_depth").set(depth_as_f64(self.receiver.len()));
                Ok(env)
            }
            Err(_) => Err(BusError::Closed),
        }
    }

    fn try_recv(&self) -> Result<Option<EventEnvelope<T>>, BusError> {
        use crossbeam_channel::TryRecvError;
        match self.receiver.try_recv() {
            Ok(env) => {
                self.consumed_total.fetch_add(1, Ordering::Relaxed);
                metrics::counter!("event_bus_consumed_total").increment(1);
                metrics::gauge!("event_bus_current_depth").set(depth_as_f64(self.receiver.len()));
                Ok(Some(env))
            }
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(BusError::Closed),
        }
    }

    fn len(&self) -> usize {
        self.receiver.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_lmax_mev_types::{ChainContext, EventSource, SmokeTestPayload};

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
    fn new_rejects_zero_capacity() {
        // Note: Result::expect_err would require the Ok variant
        // (CrossbeamBoundedBus<T>, CrossbeamConsumer<T>) to implement Debug,
        // which is not part of the Phase 1 contract. Use an explicit match
        // instead so the test does not silently demand a Debug derive on the
        // bus/consumer pair.
        let err = match CrossbeamBoundedBus::<SmokeTestPayload>::new(0) {
            Err(err) => err,
            Ok(_) => panic!("capacity 0 must reject"),
        };
        assert!(matches!(err, BusError::InvalidCapacity(0)));
    }

    #[test]
    fn publish_assigns_sequence_nonzero_timestamp_and_preserves_envelope() {
        let (bus, consumer) = CrossbeamBoundedBus::<SmokeTestPayload>::new(8)
            .expect("capacity 8 valid");

        let m = meta();
        let p = payload(7);

        let ack0 = bus.publish(payload(0), m.clone()).expect("publish 0");
        let ack1 = bus.publish(payload(1), m.clone()).expect("publish 1");
        let ack2 = bus.publish(p.clone(), m.clone()).expect("publish 2");

        assert_eq!([ack0.sequence, ack1.sequence, ack2.sequence], [0, 1, 2]);
        assert!(ack0.timestamp_ns != 0);
        assert!(ack1.timestamp_ns != 0);
        assert!(ack2.timestamp_ns != 0);
        // No timestamp monotonicity assertion - wall clock may move backward.

        // Drain and verify the third envelope matches its ack and preserves all
        // meta + payload.
        let _e0 = consumer.recv().expect("recv 0");
        let _e1 = consumer.recv().expect("recv 1");
        let e2 = consumer.recv().expect("recv 2");

        assert_eq!(e2.sequence(), ack2.sequence);
        assert_eq!(e2.timestamp_ns(), ack2.timestamp_ns);
        assert_eq!(e2.source(), m.source);
        assert_eq!(e2.event_version(), m.event_version);
        assert_eq!(e2.correlation_id(), m.correlation_id);
        assert_eq!(e2.chain_context(), &m.chain_context);
        assert_eq!(e2.payload(), &p);

        let stats = bus.stats();
        assert_eq!(stats.published_total, 3);
        assert_eq!(stats.consumed_total, 3);
        assert_eq!(stats.backpressure_total, 0);
        assert_eq!(stats.current_depth, 0);
        assert_eq!(stats.capacity, 8);
    }

    #[test]
    fn publish_registers_backpressure_when_full_and_completes_after_recv() {
        use std::sync::Arc;
        use std::time::{Duration, Instant};

        let (bus, consumer) = CrossbeamBoundedBus::<SmokeTestPayload>::new(2)
            .expect("capacity 2 valid");
        let bus = Arc::new(bus);

        let ack0 = bus.publish(payload(0), meta()).expect("publish 0");
        let ack1 = bus.publish(payload(1), meta()).expect("publish 1");
        assert_eq!([ack0.sequence, ack1.sequence], [0, 1]);
        assert_eq!(bus.len(), 2);
        assert_eq!(bus.stats().backpressure_total, 0);

        let bus_t = Arc::clone(&bus);
        let handle = std::thread::spawn(move || bus_t.publish(payload(2), meta()));

        // backpressure_total advances inside the publish lock just before the
        // blocking send, so a non-zero value implies the publisher thread has
        // entered the full-queue branch. yield_now() avoids burning CPU; the
        // 3s deadline is a bug guard only.
        //
        // Cleanup-on-failure: if the deadline expires (e.g., during the TDD red
        // phase, where the implementation does not yet increment
        // backpressure_total and the publisher is genuinely blocked inside
        // send()), drain a slot and join the spawned thread before panicking.
        // This ensures the test does not leak a parked thread when it fails.
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if bus.stats().backpressure_total > 0 {
                break;
            }
            if Instant::now() >= deadline {
                // Drain one slot so the publisher's blocking send completes,
                // then join.
                let _ = consumer.recv();
                let _ = handle.join();
                panic!("publisher thread never registered backpressure within 3s");
            }
            std::thread::yield_now();
        }
        assert_eq!(bus.stats().backpressure_total, 1);

        let env0 = consumer.recv().expect("drain 0");
        assert_eq!(env0.sequence(), 0);

        let ack2 = handle
            .join()
            .expect("publisher thread did not panic")
            .expect("publish 2 succeeded after drain");
        assert_eq!(ack2.sequence, 2);

        let env1 = consumer.recv().expect("drain 1");
        let env2 = consumer.recv().expect("drain 2");
        assert_eq!(env1.sequence(), 1);
        assert_eq!(env2.sequence(), 2);

        let stats = bus.stats();
        assert_eq!(stats.published_total, 3);
        assert_eq!(stats.consumed_total, 3);
        assert_eq!(stats.backpressure_total, 1);
        assert_eq!(stats.current_depth, 0);
        assert_eq!(stats.capacity, 2);
    }

    #[test]
    fn publish_after_consumer_drop_returns_closed() {
        let (bus, consumer) = CrossbeamBoundedBus::<SmokeTestPayload>::new(4)
            .expect("capacity 4 valid");
        drop(consumer);

        let err = bus.publish(payload(0), meta()).expect_err("publish must fail");
        assert!(matches!(err, BusError::Closed));

        // Failed publish does not advance the sequence counter.
        assert_eq!(bus.stats().published_total, 0);
    }

    #[test]
    fn try_recv_empty_returns_none_and_recv_after_bus_drop_returns_closed() {
        let (bus, consumer) = CrossbeamBoundedBus::<SmokeTestPayload>::new(2)
            .expect("capacity 2 valid");

        // (a) try_recv on empty queue: Ok(None), consumed_total stays at 0.
        assert!(matches!(consumer.try_recv().expect("try_recv ok"), None));
        assert_eq!(consumer.len(), 0);
        assert_eq!(bus.stats().consumed_total, 0);

        // (b) After one publish, try_recv returns Ok(Some(_)) and consumed_total advances.
        let ack = bus.publish(payload(0), meta()).expect("publish");
        let env = consumer.try_recv().expect("try_recv ok").expect("event present");
        assert_eq!(env.sequence(), ack.sequence);
        assert_eq!(bus.stats().consumed_total, 1);

        // (c) After dropping the bus, recv returns BusError::Closed.
        drop(bus);
        let err = consumer.recv().expect_err("recv must be closed");
        assert!(matches!(err, BusError::Closed));
    }

    #[test]
    fn publish_rejects_invalid_meta_without_consuming_sequence() {
        let (bus, consumer) = CrossbeamBoundedBus::<SmokeTestPayload>::new(2)
            .expect("capacity 2 valid");

        // Invalid meta: chain_id = 0 violates Phase 1 envelope invariant.
        let mut bad_meta = meta();
        bad_meta.chain_context.chain_id = 0;

        let err = bus
            .publish(payload(0), bad_meta)
            .expect_err("invalid meta must reject");
        assert!(matches!(err, BusError::Envelope(_)));
        assert_eq!(bus.stats().published_total, 0);

        // Valid retry must reuse sequence = 0.
        let ack = bus.publish(payload(0), meta()).expect("valid publish");
        assert_eq!(ack.sequence, 0);

        let env = consumer.recv().expect("recv");
        assert_eq!(env.sequence(), 0);
        assert_eq!(bus.stats().published_total, 1);
    }

    #[test]
    fn sequence_exhausted_does_not_wrap() {
        let (bus, consumer) = CrossbeamBoundedBus::<SmokeTestPayload>::new(2)
            .expect("capacity 2 valid");

        // Force the boundary: next publish would attempt sequence == u64::MAX.
        bus.state.lock().next_sequence = u64::MAX;

        let err = bus
            .publish(payload(0), meta())
            .expect_err("must be SequenceExhausted at u64::MAX");
        assert!(matches!(err, BusError::SequenceExhausted));

        // No advance, no envelope sent, and crucially no progress past step 4 of
        // section 7.3: backpressure_total and current_depth must both be 0 to
        // confirm the publish path returned before reaching seal/try_send.
        assert_eq!(bus.state.lock().next_sequence, u64::MAX);
        assert!(matches!(consumer.try_recv().expect("try_recv ok"), None));
        let stats = bus.stats();
        assert_eq!(stats.published_total, 0);
        assert_eq!(stats.backpressure_total, 0);
        assert_eq!(stats.current_depth, 0);
    }
}
