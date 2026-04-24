# ADR-004: RPC/EVM Stack Selection

**Date:** 2026-04-24
**Status:** Accepted

## Context

The engine requires a set of Rust library choices for:

1. **RPC/EVM library:** Ethereum JSON-RPC client, ABI encoding, type primitives.
2. **Local EVM simulation:** Execute transactions against captured state without submitting to the network.
3. **Async runtime:** Drive the event loop, timers, and I/O.
4. **Hot-path serialization:** Zero-copy encode/decode for the event bus inner loop.
5. **Cold-path serialization:** Snapshots, config, and RocksDB values.

These choices have long-lived consequences: migrating an RPC library or async runtime mid-project is extremely disruptive.

## Decision

| Concern | Library | Notes |
|---|---|---|
| RPC / EVM / type primitives | `alloy` | Paradigm's `ethers-rs` successor; actively maintained |
| Local EVM simulation | `revm` | Rust EVM, used by Foundry and Reth |
| Async runtime | `tokio` | Industry standard; `alloy` and `revm` both integrate with it |
| Hot-path serialization | `rkyv` | Zero-copy deserialization; no parse step on read |
| Cold-path serialization | `bincode` | Compact binary; simple schema; appropriate for snapshots and config |

**JSON is forbidden on the hot path.** Any code path that executes more than once per block must not allocate a JSON string or invoke a JSON parser. JSON is permitted only for configuration file loading and developer tooling (CLI output, debug dumps).

## Rationale

- `alloy` is the designated successor to `ethers-rs` from the same authors. It has cleaner generics, better provider abstractions, and first-class support for EIP-4337 and beyond. Starting with `alloy` avoids a future migration from `ethers-rs`.
- `revm` is the most accurate Rust EVM implementation, used in production by Foundry and Reth. It supports fork-mode state access, making it suitable for simulating bundles against a live-state snapshot.
- `tokio` is the only async runtime with mature integration across all selected libraries. `async-std` is not compatible with the `alloy` provider model.
- `rkyv` achieves zero-copy deserialization by casting a byte slice directly to a typed reference; this eliminates allocation and parse latency on the event bus inner loop where microseconds matter.
- `bincode` is simpler than `rkyv` and sufficient for cold-path data (snapshots written once per block, config loaded at startup). Its schema evolution story is adequate for these use cases.
- The JSON hot-path ban prevents accidental performance regressions as new contributors add code.

## Revisit Triggers

- `alloy` breaking change blocks implementation progress for more than 3 consecutive days.
- `revm` state divergence from mainnet exceeds 0.1% of simulated transactions in the Phase 3 shadow log.
- `rkyv` schema layout is broken by a dependency update more than 2 times during Phase 2.

## Consequences

- All event bus message types must derive `rkyv::Archive`, `rkyv::Serialize`, and `rkyv::Deserialize`.
- Snapshot types must derive `bincode::Encode` and `bincode::Decode`.
- CI must include a `cargo deny` check to ensure no transitive JSON-on-hot-path dependencies are silently introduced.
- `alloy` version must be pinned in `Cargo.toml`; upgrades require explicit review.
- `revm` fork-state provider must be configured to pull storage slots lazily from the local Geth node to avoid loading the full world state into memory.
