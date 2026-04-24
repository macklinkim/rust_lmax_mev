# ADR-007: Node Topology & Fallback RPC

**Date:** 2026-04-24
**Status:** Accepted

## Context

The engine requires reliable, low-latency access to Ethereum node data for three distinct purposes:

1. **Streaming subscriptions:** `newHeads`, `newPendingTransactions`, `logs` — require persistent WebSocket connections.
2. **State reads:** `eth_call`, `eth_getStorageAt`, `eth_getBalance` — can use stateless HTTP.
3. **Archive access:** Historical state at arbitrary block numbers — needed for backtesting and replay initialization.

A single node topology decision must cover: which client software to run locally, which transport protocols to use for which operations, how to handle local node degradation, and when archive access is needed.

## Decision

### Primary node
**Geth** (go-ethereum), self-hosted.

- **WS endpoint:** Required. Used for `newHeads`, `newPendingTransactions`, and `logs` subscriptions.
- **HTTP endpoint:** Required. Used for `eth_call`, `eth_getStorageAt`, and other request-response RPC calls.

### Fallback RPC
Minimum one external HTTP provider must be configured at startup. Acceptable providers: Alchemy, Infura (or equivalent tier-1 provider).

Failover behavior: when the local Geth HTTP endpoint returns errors or times out on state read calls, the engine automatically retries the same call against the fallback provider. The WS subscription path does not fail over (reconnect to local Geth only; if Geth is down, block processing pauses until it recovers).

### Transport assignment
| Operation | Transport | Primary | Fallback |
|---|---|---|---|
| `newHeads` | WS | Local Geth | None (reconnect only) |
| `newPendingTransactions` | WS | Local Geth | None (reconnect only) |
| `logs` | WS | Local Geth | None (reconnect only) |
| `eth_call` | HTTP | Local Geth | Alchemy/Infura |
| `eth_getStorageAt` | HTTP | Local Geth | Alchemy/Infura |
| `eth_getBalance` | HTTP | Local Geth | Alchemy/Infura |

### Archive access
Archive node access is **optional through Phase 3**. Beginning at Phase 4 (scope widening), archive access is required for backtesting over historical blocks. The local Geth node should be run in full-sync mode (not archive) through Phase 3 to minimize disk requirements; archive mode or a separate archive provider is added at Phase 4.

## Rationale

- Geth is the most widely deployed Ethereum execution client with mature Rust-compatible WS and HTTP APIs. It is the lowest-risk choice for a primary node.
- Separating WS (streaming) from HTTP (request-response) matches the operational characteristics of each: WS connections are long-lived and multiplexed; HTTP calls are stateless and easy to retry.
- A single fallback HTTP provider is sufficient for state reads during P1–P3 (shadow mode, low call volume). Multiple fallback providers add operational complexity not justified until P4+.
- Not failing over WS subscriptions to external providers is intentional: external subscription streams have different latency profiles and could introduce subtle ordering bugs. Pausing on local node failure is safer and more predictable.
- Deferring archive access avoids the significant disk cost (~2 TB+) until it is actually needed for historical backtesting.
- Reth (Rust Ethereum client) is a known alternative with potentially better performance, but it is less mature than Geth. The decision to use Geth is revisitable if benchmark evidence warrants it.

## Revisit Trigger

Reth demonstrates a performance advantage greater than 30% over Geth on the Phase 4 node throughput benchmark (measured as blocks processed per second or mempool transaction ingestion rate).

## Consequences

- Geth must be running and synced before any Phase 1 integration tests can execute against mainnet state.
- The `NodeProvider` abstraction must expose separate WS and HTTP client handles; they must not share a connection pool.
- At least one fallback HTTP provider URL must be present in the engine config; startup fails if it is missing or unreachable.
- Archive mode enablement at Phase 4 must be tracked as a deployment task, not a code change.
- WS reconnection logic (exponential backoff, max retry count) must be implemented in Phase 1 as part of the `NodeProvider` component.
