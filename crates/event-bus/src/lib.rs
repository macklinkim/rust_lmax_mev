// Crate-level docstring is added in Task 11. Traits, concrete impls, and tests
// land in subsequent tasks.

use rust_lmax_mev_types::TypesError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublishAck {
    pub sequence: u64,
    pub timestamp_ns: u64,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusStats {
    pub published_total: u64,
    pub consumed_total: u64,
    pub backpressure_total: u64,
    pub current_depth: usize,
    pub capacity: usize,
}

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

pub trait EventBus<T>: Send + Sync
where
    T: Send + 'static,
{
    fn publish(&self, payload: T, meta: PublishMeta) -> Result<PublishAck, BusError>;
    fn len(&self) -> usize;
    fn capacity(&self) -> usize;
    fn stats(&self) -> BusStats;
}

pub trait EventConsumer<T>: Send + Sync
where
    T: Send + 'static,
{
    fn recv(&self) -> Result<EventEnvelope<T>, BusError>;
    fn try_recv(&self) -> Result<Option<EventEnvelope<T>>, BusError>;
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

        self.sender
            .send(envelope)
            .map_err(|_| BusError::Closed)?;

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
}
