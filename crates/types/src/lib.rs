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

fn check_envelope_invariants(
    timestamp_ns: u64,
    event_version: u16,
    chain_id: u64,
) -> Result<(), TypesError> {
    if timestamp_ns == 0 {
        return Err(TypesError::InvalidEnvelope {
            field: "timestamp_ns",
            reason: "must be non-zero",
        });
    }
    if event_version == 0 {
        return Err(TypesError::InvalidEnvelope {
            field: "event_version",
            reason: "must be non-zero",
        });
    }
    if chain_id == 0 {
        return Err(TypesError::InvalidEnvelope {
            field: "chain_context.chain_id",
            reason: "must be non-zero",
        });
    }
    Ok(())
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
    /// **Intended caller: EventBus implementations only.** Downstream
    /// consumers receive sealed envelopes and access fields via getters.
    ///
    /// Validates Phase 1 invariants:
    /// - `timestamp_ns != 0`
    /// - `meta.event_version != 0` (event_version = 0 is reserved per
    ///   Phase 1 policy; see crate-level docs).
    /// - `meta.chain_context.chain_id != 0`
    ///
    /// `sequence`, `block_number`, `correlation_id` are accepted as-is.
    pub fn seal(
        meta: PublishMeta,
        payload: T,
        sequence: u64,
        timestamp_ns: u64,
    ) -> Result<Self, TypesError> {
        check_envelope_invariants(
            timestamp_ns,
            meta.event_version,
            meta.chain_context.chain_id,
        )?;
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

    /// Re-validates Phase 1 invariants without reconstructing the envelope.
    ///
    /// Use this at deserialization boundaries (journal decode, replay,
    /// wire decode) to confirm a decoded envelope still satisfies the
    /// invariants `seal()` enforced at construction time. `serde::Deserialize`
    /// and `rkyv::Deserialize` reconstruct fields directly, **bypassing
    /// `seal()`** — without this method, a corrupted frame could produce
    /// an envelope with `timestamp_ns = 0`, `event_version = 0`, or
    /// `chain_context.chain_id = 0`.
    ///
    /// Checks the same three invariants as `seal()`:
    /// - `timestamp_ns != 0`
    /// - `event_version != 0`
    /// - `chain_context.chain_id != 0`
    ///
    /// Journal, replay, and decoder consumers MUST call `validate()`
    /// after any deserialization, before passing the envelope to
    /// downstream pipeline stages.
    pub fn validate(&self) -> Result<(), TypesError> {
        check_envelope_invariants(
            self.timestamp_ns,
            self.event_version,
            self.chain_context.chain_id,
        )
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

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_envelope() -> EventEnvelope<SmokeTestPayload> {
        let meta = PublishMeta {
            source: EventSource::Ingress,
            chain_context: ChainContext {
                chain_id: 1,
                block_number: 18_000_000,
                block_hash: [0xAB; 32],
            },
            event_version: 1,
            correlation_id: 42,
        };
        let payload = SmokeTestPayload {
            nonce: 7,
            data: [0xCD; 32],
        };
        EventEnvelope::seal(meta, payload, 100, 1_700_000_000_000_000_000)
            .expect("valid envelope should seal")
    }

    #[test]
    fn seal_enforces_phase_1_invariants() {
        let valid_meta = || PublishMeta {
            source: EventSource::Ingress,
            chain_context: ChainContext {
                chain_id: 1,
                block_number: 18_000_000,
                block_hash: [0xAB; 32],
            },
            event_version: 1,
            correlation_id: 42,
        };
        let valid_payload = || SmokeTestPayload {
            nonce: 7,
            data: [0xCD; 32],
        };

        // 1. timestamp_ns = 0 must reject
        let err = EventEnvelope::seal(valid_meta(), valid_payload(), 100, 0)
            .expect_err("timestamp_ns=0 must reject");
        assert!(matches!(
            err,
            TypesError::InvalidEnvelope { field: "timestamp_ns", .. }
        ));

        // 2. event_version = 0 must reject
        let mut bad_meta = valid_meta();
        bad_meta.event_version = 0;
        let err = EventEnvelope::seal(bad_meta, valid_payload(), 100, 1_700_000_000_000_000_000)
            .expect_err("event_version=0 must reject");
        assert!(matches!(
            err,
            TypesError::InvalidEnvelope { field: "event_version", .. }
        ));

        // 3. chain_id = 0 must reject
        let mut bad_meta = valid_meta();
        bad_meta.chain_context.chain_id = 0;
        let err = EventEnvelope::seal(bad_meta, valid_payload(), 100, 1_700_000_000_000_000_000)
            .expect_err("chain_id=0 must reject");
        assert!(matches!(
            err,
            TypesError::InvalidEnvelope { field: "chain_context.chain_id", .. }
        ));

        // 4. happy path - seal succeeds and getters return inputs verbatim
        let env = EventEnvelope::seal(
            valid_meta(),
            valid_payload(),
            100,
            1_700_000_000_000_000_000,
        )
        .expect("valid envelope must seal");
        assert_eq!(env.sequence(), 100);
        assert_eq!(env.timestamp_ns(), 1_700_000_000_000_000_000);
        assert_eq!(env.source(), EventSource::Ingress);
        assert_eq!(env.event_version(), 1);
        assert_eq!(env.correlation_id(), 42);
        assert_eq!(env.chain_context().chain_id, 1);
        assert_eq!(env.payload().nonce, 7);

        // 5. happy envelope must also pass validate() (cross-check that
        //    seal() and validate() accept the same valid inputs)
        env.validate().expect("happy envelope must pass validate()");
    }

    #[test]
    fn validate_rejects_decoded_envelope_violations() {
        let valid_chain = ChainContext {
            chain_id: 1,
            block_number: 18_000_000,
            block_hash: [0xAB; 32],
        };
        let valid_payload = SmokeTestPayload {
            nonce: 7,
            data: [0xCD; 32],
        };

        // Case 1: timestamp_ns = 0 (simulates corrupted decoded frame).
        // Direct struct literal is permitted in test module per spec section 5.3.
        let bad_ts = EventEnvelope::<SmokeTestPayload> {
            sequence: 100,
            timestamp_ns: 0,
            source: EventSource::Ingress,
            chain_context: valid_chain.clone(),
            event_version: 1,
            correlation_id: 42,
            payload: valid_payload.clone(),
        };
        assert!(matches!(
            bad_ts.validate(),
            Err(TypesError::InvalidEnvelope { field: "timestamp_ns", .. })
        ));

        // Case 2: event_version = 0
        let bad_ver = EventEnvelope::<SmokeTestPayload> {
            sequence: 100,
            timestamp_ns: 1_700_000_000_000_000_000,
            source: EventSource::Ingress,
            chain_context: valid_chain.clone(),
            event_version: 0,
            correlation_id: 42,
            payload: valid_payload.clone(),
        };
        assert!(matches!(
            bad_ver.validate(),
            Err(TypesError::InvalidEnvelope { field: "event_version", .. })
        ));

        // Case 3: chain_context.chain_id = 0
        let bad_chain = EventEnvelope::<SmokeTestPayload> {
            sequence: 100,
            timestamp_ns: 1_700_000_000_000_000_000,
            source: EventSource::Ingress,
            chain_context: ChainContext {
                chain_id: 0,
                ..valid_chain.clone()
            },
            event_version: 1,
            correlation_id: 42,
            payload: valid_payload,
        };
        assert!(matches!(
            bad_chain.validate(),
            Err(TypesError::InvalidEnvelope { field: "chain_context.chain_id", .. })
        ));
    }
}
