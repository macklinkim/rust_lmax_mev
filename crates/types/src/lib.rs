// Crate-level docstring is added in Task 10. EventEnvelope and TypesError
// are added in later tasks.

pub type BlockHash = [u8; 32];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum EventSource {
    Ingress,
    Normalizer,
    StateEngine,
    OpportunityEngine,
    RiskEngine,
    Simulator,
    Execution,
    Relay,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ChainContext {
    pub chain_id: u64,
    pub block_number: u64,
    pub block_hash: BlockHash,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PublishMeta {
    pub source: EventSource,
    pub chain_context: ChainContext,
    pub event_version: u16,
    pub correlation_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct JournalPosition {
    pub sequence: u64,
    pub byte_offset: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct SmokeTestPayload {
    pub nonce: u64,
    pub data: [u8; 32],
}

#[derive(Debug, thiserror::Error)]
pub enum TypesError {
    #[error("invalid envelope: field={field}, reason={reason}")]
    InvalidEnvelope {
        field: &'static str,
        reason: &'static str,
    },
    #[error("unsupported event_version: found={found}, max_supported={max_supported}")]
    UnsupportedEventVersion {
        found: u16,
        max_supported: u16,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct EventEnvelope<T> {
    sequence: u64,
    timestamp_ns: u64,
    source: EventSource,
    chain_context: ChainContext,
    event_version: u16,
    correlation_id: u64,
    payload: T,
}

impl<T> EventEnvelope<T> {
    /// Seals an envelope with bus-assigned `sequence` and `timestamp_ns`.
    ///
    /// **STUB — Task 6 replaces this with the real invariant-checking
    /// implementation. Do not consume in production code while this stub
    /// is in place.**
    pub fn seal(
        meta: PublishMeta,
        payload: T,
        sequence: u64,
        timestamp_ns: u64,
    ) -> Result<Self, TypesError> {
        Ok(Self {
            sequence,
            timestamp_ns,
            source: meta.source,
            chain_context: meta.chain_context,
            event_version: meta.event_version,
            correlation_id: meta.correlation_id,
            payload,
        })
    }

    /// Re-validates Phase 1 invariants. **STUB — Task 7 replaces this.**
    pub fn validate(&self) -> Result<(), TypesError> {
        Ok(())
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn timestamp_ns(&self) -> u64 {
        self.timestamp_ns
    }

    pub fn source(&self) -> EventSource {
        self.source
    }

    pub fn event_version(&self) -> u16 {
        self.event_version
    }

    pub fn correlation_id(&self) -> u64 {
        self.correlation_id
    }

    pub fn chain_context(&self) -> &ChainContext {
        &self.chain_context
    }

    pub fn payload(&self) -> &T {
        &self.payload
    }

    pub fn into_payload(self) -> T {
        self.payload
    }
}
