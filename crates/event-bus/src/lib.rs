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
