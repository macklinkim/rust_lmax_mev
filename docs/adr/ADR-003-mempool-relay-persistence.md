# ADR-003: Mempool / Relay / Persistence Strategy

**Date:** 2026-04-24
**Status:** Accepted

## Context

The engine requires three infrastructure decisions that are tightly coupled:

1. **Mempool access:** How pending transactions are observed before block inclusion.
2. **Bundle relay:** How constructed MEV bundles are submitted to block builders.
3. **Persistence:** How market snapshots and event journals are stored for replay and audit.

These choices affect latency, operational complexity, cost, and the ability to replay historical data deterministically.

## Decision

### Mempool
Hybrid approach: Geth local node (primary, lowest latency) + one external feed as secondary.

External feed options (choose one at deployment time):
- bloXroute BDN
- Chainbound Fiber

The external feed supplements the local node for transactions that propagate via private channels not visible to the public p2p network. Both feeds must be behind a common `MempoolSource` trait so they are swappable without changes to downstream consumers.

### Relay
Multi-relay submission using an adapter pattern:

- **Primary:** Flashbots relay (`eth_sendBundle`)
- **Secondary:** bloXroute relay (`eth_sendBundle` or equivalent BDN endpoint)
- Additional relays added via the same `BundleRelay` trait adapter without core changes.

Bundles are submitted to all configured relays concurrently; the first inclusion wins.

### Persistence
Two-tier storage:

- **RocksDB embedded KV store:** Snapshots of on-chain state (pool reserves, block context). Keyed by block number + pool address. Used for replay initialization.
- **Append-only binary journal:** Ordered log of all domain events (mempool arrivals, block arrivals, simulation results, bundle outcomes). Written sequentially; never mutated in place. Used for deterministic replay and audit.

Journal format: `rkyv`-serialized event records with a 4-byte length prefix and CRC32 checksum per record.

## Rationale

- A local Geth node provides the lowest-latency mempool view and removes dependency on a single external vendor for the critical path.
- The external feed hedges against transactions that bypass the public p2p network (e.g., private mempools, direct-to-builder transactions). Hybrid is strictly better than either alone.
- Multi-relay submission maximizes bundle inclusion probability at negligible extra cost (concurrent HTTP sends).
- The adapter pattern for both mempool sources and relays isolates vendor-specific logic and enables testing with mock implementations.
- RocksDB is a well-understood embedded KV store with good Rust bindings (`rocksdb` crate). It is appropriate for random-access snapshot reads.
- An append-only journal is the simplest correct persistence model for an ordered event stream: it cannot corrupt existing records and is trivially replayable.
- Separating snapshots (random access) from events (sequential append) matches their access patterns exactly.

## Revisit Trigger

RocksDB write amplification exceeds 10x measured on the CI benchmark workload, OR total CI compile time for the persistence crate exceeds 5 minutes on the standard CI runner.

## Consequences

- A `MempoolSource` trait must be defined in P1; Geth WS subscription is the first implementation.
- A `BundleRelay` trait must be defined before P4 bundle submission work begins.
- The journal writer must be a dedicated component on the event bus hot path; it must never block event processing.
- RocksDB tuning (block cache size, compaction settings) is deferred to P4 performance work.
- Journal replay must be tested in CI (see ADR-008).
