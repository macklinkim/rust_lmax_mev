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
