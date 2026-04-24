# Event Model Specification

## EventEnvelope\<T\>

All events on the internal bus are wrapped in `EventEnvelope<T>`. The envelope carries 7 fields:

| Field | Type | Description |
|-------|------|-------------|
| `sequence` | `u64` | Monotonic counter, assigned by the bus at publish time |
| `timestamp_ns` | `u64` | Wall clock at publish (nanoseconds since Unix epoch), assigned by the bus |
| `source` | `EventSource` | Identifies the pipeline stage that published the event |
| `chain_context` | `ChainContext` | Block-level context at time of publish |
| `event_version` | `u16` | Schema version; increment on any breaking change |
| `correlation_id` | `u64` | Trace linkage across pipeline stages |
| `payload` | `T` | Concrete event variant |

### EventSource Enum

```
Ingress
Normalizer
StateEngine
OpportunityEngine
RiskEngine
Simulator
Execution
Relay
```

### ChainContext

```
chain_id:     u64
block_number: u64
block_hash:   [u8; 32]
```

## Required Derives

Every `EventEnvelope<T>` and its payload types must derive:

```rust
Clone, Debug, PartialEq,
rkyv::{Archive, Serialize, Deserialize},
serde::{Serialize, Deserialize}
```

## PublishMeta

The caller provides:

- `source`
- `chain_context`
- `event_version`
- `correlation_id`

The bus assigns:

- `sequence` (monotonic, bus-global)
- `timestamp_ns` (wall clock at publish)

## Versioning Rule

`event_version` is a `u16`. It must be incremented on any schema change that alters field layout, types, or semantics. Old versions must remain deserializable, or a documented migration path must be provided before the old version is retired.
