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

impl<T> EventBus<T> for CrossbeamBoundedBus<T>
where
    T: Send + 'static,
{
    fn publish(&self, _payload: T, _meta: PublishMeta) -> Result<PublishAck, BusError> {
        unimplemented!("publish lands in Task 5")
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
        unimplemented!("recv lands in Task 5")
    }

    fn try_recv(&self) -> Result<Option<EventEnvelope<T>>, BusError> {
        unimplemented!("try_recv lands in Task 5")
    }

    fn len(&self) -> usize {
        self.receiver.len()
    }
}
