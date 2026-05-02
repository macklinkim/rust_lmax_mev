# Phase 2 Batch A — Node + Ingress

**Date:** 2026-05-02
**Status:** Draft v0.4 (Codex 2026-05-02 19:33:58 +09:00 HIGH REVISION REQUIRED — 1 targeted item: `NodeError::Rpc(String)` variant added so authoritative JSON-RPC error responses do not collapse into `Transport` and incorrectly trigger fallback)
**Predecessor:** Phase 2 overview at `f5058b0`.

## Scope

Two new crates (`node`, `ingress`) + additive `crates/config` fields + workspace `alloy` dep. End state: a producer can subscribe to Geth WS streams, normalize raw txs, and publish `IngressEvent` to a typed bus. No `crates/app` wiring yet (P2-D).

## New types & API surface

- `crates/node`:
  - `NodeProvider` (struct) — owns `alloy` WS handle + primary HTTP handle + fallback HTTP handle. Ctor `NodeProvider::connect(&NodeConfig)`.
  - `provider.subscribe_new_heads()` / `subscribe_pending_txs()` / `subscribe_logs(filter)` → `Pin<Box<dyn Stream<Item = Result<T, NodeError>> + Send + 'static>>`. Reconnect handled inside the stream.
  - `provider.eth_call(req) -> Result<Bytes, NodeError>` — primary, fallback on `NodeError::Transport` only.
  - `provider.eth_get_transaction_by_hash(tx_hash) -> Result<Option<Transaction>, NodeError>` (NEW v0.2) — fallback policy (v0.3 disambiguated): fallback fires ONLY on primary `NodeError::Transport`. A primary `Ok(None)` (Geth knows nothing about this hash) is **authoritative** and returns `Ok(None)` immediately — fallback is NOT consulted, since a Geth `null` answer for a tx-by-hash query is the definitive negative answer for the local node's view.
  - `NodeError` (`#[non_exhaustive]`, v0.4):
    - `WsConnect(String)` — WebSocket handshake failure.
    - `Transport(String)` — TRUE transport failure only: connect refused, TCP timeout, socket reset, DNS failure. **The only variant that triggers HTTP fallback.**
    - `Rpc(String)` — JSON-RPC error response from the node (e.g., `eth_call` revert, invalid params, method not found). Authoritative; **never fails over** because the response IS the answer (negative or otherwise) and a different node would just give a different authoritative answer about the same query.
    - `Decode(String)` — response body decode/deserialize failure (malformed JSON, unexpected schema). Authoritative; never fails over.
    - `Closed` — channel/handle closed by caller-side drop.
- `crates/ingress`:
  - ```rust
    pub trait MempoolSource: Send + Sync {
        fn stream(&self) -> Pin<Box<dyn Stream<Item = Result<MempoolEvent, IngressError>> + Send + 'static>>;
    }
    ```
    Object-safe per Codex 19:20:44; `Pin<Box<dyn Stream + Send>>` is the explicit erased-stream contract so consumers can hold `Box<dyn MempoolSource>`. Future `BloXrouteMempool` impl plugs in unchanged.
  - `GethWsMempool { provider: Arc<NodeProvider> }` — implements `MempoolSource`. Internally: `provider.subscribe_pending_txs()` yields tx hashes; per hash call `provider.eth_get_transaction_by_hash` to fetch full tx; deduped by `tx_hash` via in-source LRU before yielding.
  - `Normalizer::filter(raw_tx, watched: &[Address]) -> Option<MempoolEvent>` — keeps a tx iff `raw_tx.to ∈ watched`. The `watched` list is `[ingress] watched_addresses` from config (Phase 2: Uniswap V2 + V3 0.05% WETH/USDC pool + router addresses; the config is operator-supplied, not hardcoded). Calldata-level decoding (e.g., `swap` selector matching) is OUT for P2-A; revisited in P3.
  - `IngressEvent` enum payload type: `Mempool(MempoolEvent) | Block(BlockEvent)`.
  - `MempoolEvent { tx_hash, from, to, value, input, gas_limit, max_fee, observed_at_ns }`.
  - `BlockEvent { block_number, block_hash, parent_hash, timestamp_ns }`.
  - `IngressError` (`#[non_exhaustive]`): `Node(NodeError)` (with `#[from]`), `Decode(String)`, `Closed`.

## Risk decisions (only NEW to this batch)

1. **`alloy` umbrella pin: `alloy = "0.8"`** at the same minor as the existing `alloy-primitives = "0.8"` to avoid version skew (ADR-004 exact-minor pin requirement). Required capabilities for P2-A: `ProviderBuilder` for WS + HTTP; `pubsub::Subscription` for `newHeads`/`newPendingTransactions`/`logs`; `network::Ethereum`; `transports::http::Http` + `transports::ws::WsConnect`; `rpc::types::eth::{BlockNumberOrTag, Filter, Log, Transaction}`. Feature selection: scaffold (commit 2) starts with `default-features = true` to unblock the build; commit 3/4 narrows features to the minimal subset that still builds + passes tests, recorded in commit 4's message. The build itself proves feature adequacy; no speculative feature pinning in v0.2.
2. **WS reconnect = exponential backoff 1s → 2s → 4s → … cap 60s, unlimited retries.** Per ADR-007 "WS reconnection logic ... must be implemented as part of NodeProvider." Block processing pauses while reconnecting (ADR-007 explicit). Reconnect logic lives inside the returned `Stream`, hidden from consumers.
3. **HTTP fallback fires only on `NodeError::Transport`** (connect refused, TCP timeout, socket reset, DNS failure). `NodeError::Rpc` (JSON-RPC error response: revert, invalid params, etc.) and `NodeError::Decode` are authoritative — fallback NOT consulted. Transport errors retry primary once before fallback. Implementation must classify `alloy` errors carefully so a JSON-RPC error response is never collapsed into `Transport`.
4. **Mempool filter at ingress.** Filter rule: keep a tx iff `tx.to ∈ [ingress] watched_addresses` (Vec<Address>, len ≥ 1). Operator-supplied at config time (Uniswap V2 + V3 0.05% WETH/USDC pool + router addresses). Calldata decoding deferred to P3. WETH/USDC token identities are config'd separately as typed `[ingress.tokens] { weth: Address, usdc: Address }` (Codex 19:20:44 #3 — typed roles instead of `Vec<Address>`); these are consumed by P2-B's pool-state code, not by P2-A's tx filter.
5. **Single ingress→state bus carrying `IngressEvent` sum type**, not two parallel buses. Per ADR-005 "each domain pipeline stage boundary owns one bus pair"; an ingress→state boundary is one boundary.
6. **`MempoolEvent.tx_hash` dedup at ingress.** Geth's `pending` subscription can repeat txs across reconnects; `GethWsMempool` keeps an LRU of the last 4096 seen hashes (`hashbrown` LRU; not a workspace dep yet — use `lru = "0.12"` as a `crates/ingress` dev-runtime dep to avoid pulling into other crates).

## Test matrix (lean per `feedback_phase2_doc_volume.md`)

`crates/node` (3 tests):
- N-1 happy: `NodeProvider::connect` against a mock `alloy` provider returns Ok; `eth_call` round-trips a known result.
- N-2 failure: primary HTTP returns transport error → fallback returns Ok; assert response from fallback. Companion check: primary returns `NodeError::Rpc` → method returns `Err(Rpc(_))` and fallback was NOT called (verified via mock call counter).
- N-3 boundary: WS stream emits 3 items, then a connection drop, then 3 more; consumer sees 6 items (reconnect transparent).

`crates/ingress` (3 tests):
- I-1 happy: `Normalizer::filter` with watched=[A,B] keeps a tx whose `to == A`, drops one whose `to == 0xC...`.
- I-2 failure: `GethWsMempool::stream` propagates `NodeError::Closed` as `IngressError::Closed`.
- I-3 boundary: 5 duplicate tx_hashes from the underlying stream → consumer sees 1 event (LRU dedup).

Total new tests: 6. Workspace cumulative target: 52 → **58**.

## Workspace + per-crate dependency deltas

Workspace `[workspace.dependencies]`: ADD `alloy = { version = "0.8", default-features = true }`. ADD `futures = "0.3"` (for `Stream` trait + `StreamExt`).

`crates/node/Cargo.toml`:
- Runtime: `rust-lmax-mev-config = { path = "../config" }`, `alloy = { workspace = true }`, `tokio = { workspace = true }`, `futures = { workspace = true }`, `tracing = { workspace = true }`, `thiserror = { workspace = true }`, `parking_lot = { workspace = true }`.
- Dev: `tokio = { workspace = true, features = ["test-util", "macros"] }`, `alloy = { workspace = true }` (mock provider features).

`crates/ingress/Cargo.toml`:
- Runtime: `rust-lmax-mev-types = { path = "../types" }`, `rust-lmax-mev-node = { path = "../node" }`, `rust-lmax-mev-config = { path = "../config" }`, `alloy = { workspace = true }`, `tokio = { workspace = true }`, `futures = { workspace = true }`, `tracing = { workspace = true }`, `thiserror = { workspace = true }`, `serde = { workspace = true }`, `rkyv = { workspace = true }`, `lru = "0.12"`.
- Dev: `tokio = { workspace = true, features = ["test-util", "macros"] }`.

`crates/config/src/lib.rs` additive (v0.2):
- New `IngressConfig { tokens: IngressTokens, watched_addresses: Vec<Address> }`.
- `IngressTokens { weth: Address, usdc: Address }` typed roles (replaces v0.1's `Vec<Address>`).
- `Address` deserialized via `alloy_primitives::Address`.
- `Config::validate` enforces `watched_addresses.len() >= 1` and `tokens.weth != tokens.usdc`.
- `[node]` reuses Phase 1's `fallback_rpc[]` — no new field.

`crates/config/Cargo.toml` (v0.3 explicit dep delta per Codex 19:27:20): ADD `alloy-primitives = { workspace = true }` to `[dependencies]`. Currently absent from this crate's manifest (workspace dep alone is not visible per-crate). Same `0.8` minor as the existing workspace pin.

## Commit grouping (4 commits)

1. **docs: add Phase 2 Batch A node+ingress execution note** — this file.
2. **chore(workspace): scaffold crates/node + crates/ingress + alloy workspace dep** — workspace `Cargo.toml` (members + alloy + futures), both crates' `Cargo.toml`, placeholder `lib.rs` files; `crates/config` extension for `[ingress]` section + validate.
3. **feat(node): NodeProvider + fallback HTTP + WS reconnect (N-1..N-3)** — full node behavior + 3 tests.
4. **feat(ingress): MempoolSource + GethWsMempool + Normalizer + IngressEvent (I-1..I-3)** — full ingress behavior + 3 tests.

Batch close runs full verification (fmt + build + test=58 + clippy + doc for both new crates + cargo deny check) per `feedback_verification_cadence.md`.

## Forbidden delta (only NEW)

- No live-mainnet calls in any test. Mock providers only in CI; live Geth is dev-host smoke (separate, not gated).
- No `BundleRelay`, no `revm`, no archive-mode RPC.
- All standing Phase 1 + Phase 2 forbids carry over.

## Question for Codex (pre-impl, architectural-risk)

v0.3 incorporates Codex 19:27:20 (2 targeted items). All prior architectural answers preserved:
- `Pin<Box<dyn Stream + Send + 'static>>` on subs + MempoolSource — confirmed correct.
- `tx.to ∈ watched_addresses` sufficient for P2-A; calldata decoding deferred to P3 — confirmed.
- `alloy` default-features → minimal-subset iteration through commits 2–4 — confirmed.

Open question: anything else needed before the 4-commit ladder runs?

If APPROVED: execute the 4-commit ladder. If REVISION REQUIRED: edit + re-emit. If scope/ADR change: HALT.
