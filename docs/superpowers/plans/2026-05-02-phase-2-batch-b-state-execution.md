# Phase 2 Batch B — State Engine

**Date:** 2026-05-02
**Status:** Draft v0.4 (Codex 2026-05-02 20:58:55 +09:00 HIGH REVISION REQUIRED — 2 wording-only fixes: stale "`StateEngine` holds `Arc<NodeProvider>` directly" sentence in Risk Decision 1 contradicted v0.3's `caller: Arc<dyn EthCaller>` storage; Scope said "`NodeProvider::eth_call`" but should reference the new `eth_call_at_block` per Risk Decision 2)
**Predecessor:** P2-A closed at `9487cce`.

## Scope

New crate `crates/state`: per-block reserves snapshot for WETH/USDC on
Uniswap V2 + V3 (0.05%) via the new
[`NodeProvider::eth_call_at_block`] (additive P2-A API per Risk
Decision 2), persisted to [`RocksDbSnapshot`]. Owns half of the
Phase 2 EXIT (State Correctness Gate). No `crates/app` wiring (P2-D).

## New types & API surface

- `PoolKind { UniV2, UniV3Fee0_05 }`, `PoolId { kind: PoolKind, address: Address }`.
- `PoolState` enum:
  - `UniV2 { reserve0: U256, reserve1: U256, block_timestamp_last: u32 }` (from `getReserves()` selector `0x0902f1ac`).
  - `UniV3 { sqrt_price_x96: U256, tick: i32, liquidity: u128 }` (from `slot0()` selector `0x3850c7bd` + `liquidity()` selector `0x1a686502`).
- `StateUpdateEvent { block_number, block_hash, pool: PoolId, state: PoolState }` — payload type emitted by the State Engine on the state→opportunity bus (Phase 2 has no consumer past P2-D).
- ```rust
  #[async_trait::async_trait]
  pub trait EthCaller: Send + Sync {
      /// Pinned at `block_id`; same retry-then-fallback policy as
      /// `NodeProvider::eth_call_at_block`. Returns the raw 32-byte-
      /// word ABI return bytes; `StateEngine` is responsible for
      /// selector dispatch + decode.
      async fn eth_call_at_block(
          &self,
          req: alloy::rpc::types::eth::TransactionRequest,
          block_id: alloy::eips::BlockId,
      ) -> Result<alloy_primitives::Bytes, rust_lmax_mev_node::NodeError>;
  }
  ```
  Production: `NodeEthCaller(Arc<NodeProvider>)` impl forwards to `provider.eth_call_at_block(...)`. Tests / replay fixtures: implement directly with canned `(selector, pool, block_hash) -> Bytes` lookups.
- `StateEngine { caller: Arc<dyn EthCaller>, snapshot: Arc<RocksDbSnapshot>, pools: Vec<PoolId> }`:
  - `pub fn new(provider: Arc<NodeProvider>, snapshot: Arc<RocksDbSnapshot>, pools: Vec<PoolId>) -> Self` — production ctor; wraps `NodeProvider` in `NodeEthCaller` internally.
  - `pub fn with_caller(caller: Arc<dyn EthCaller>, snapshot: Arc<RocksDbSnapshot>, pools: Vec<PoolId>) -> Self` — test/replay ctor.
  - `pub async fn refresh_block(&self, block_number: u64, block_hash: B256) -> Result<Vec<StateUpdateEvent>, StateError>` — for each pool: build `TransactionRequest` (to=pool, data=selector), call `self.caller.eth_call_at_block(req, BlockId::Hash(block_hash.into()))`, decode → bincode-persist via `snapshot.save(key, &state)` → push event. Returns the events (caller publishes to bus).
- `StateError` (`#[non_exhaustive]`): `Node(NodeError)` (with `#[from]`), `Decode(String)`, `Snapshot(JournalError)` (with `#[from]`), `UnknownPool(Address)`.

## Risk decisions (NEW to this batch)

1. **Provider abstraction**: production construction accepts `Arc<NodeProvider>` (no per-state-crate alloy client; single connection pool per process, matches Phase 1 app-wiring pattern), but `StateEngine` STORES `Arc<dyn EthCaller>`; `pub fn new(provider, snapshot, pools)` wraps the provider in `NodeEthCaller` internally. The call surface trait is `pub trait EthCaller: Send + Sync` (NOT `pub(crate)` — P2-C gate tests in `crates/replay/tests/` need cross-crate access; `cfg(test)` does not fire for downstream crates). Production `NodeEthCaller(Arc<NodeProvider>)` impl forwards to `provider.eth_call_at_block`; tests construct a `MockEthCaller` keyed by `(selector, pool_address, block_hash)`. `pub fn StateEngine::with_caller(caller, snapshot, pools) -> Self` is the test/replay constructor; `pub fn StateEngine::new(provider, snapshot, pools)` is the production constructor. Both are `pub`.
2. **Block pinning by `block_hash`** (reorg-safe), not `block_number`. `alloy::rpc::types::eth::BlockId::Hash(block_hash.into())` passed to a NEW additive API on `NodeProvider`:

   ```rust
   pub async fn eth_call_at_block(
       &self,
       req: TransactionRequest,
       block_id: BlockId,
   ) -> Result<Bytes, NodeError>
   ```

   **Required additive scope exception**: P2-A's `eth_call(req)` defaults to `BlockNumberOrTag::Pending` and does not carry a `BlockId`. v0.2 admits this as the SOLE additive P2-A API change in Batch B, with the same retry-then-fallback policy as the existing `eth_call`. Implementation = call `provider.call(&req).block(block_id)` (alloy's `EthCall` builder accepts `.block`). `eth_call(req)` stays unchanged so existing callers and tests are not affected.

   Determinism is required for the State Correctness Gate (P2-C): a re-run at the same `block_hash` MUST produce byte-identical `PoolState`.
3. **Snapshot key shape: `[u8; 28]` = `block_number_be (8) || pool_address (20)`** — fixed-width, lex-orderable by block then pool. Avoids the `RESERVED_KEY_PREFIX` (`b"\0rust_lmax_mev:snapshot:"`) trivially because the first byte is the high byte of a u64 (zero only for blocks < 2^56, never reserved-prefix). `PoolKind` encoded inside the bincoded value, not the key.
4. **`[state]` config schema**:
   ```toml
   [[state.pools]]
   kind = "uniswap_v2"
   address = "0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc"
   [[state.pools]]
   kind = "uniswap_v3_fee_005"
   address = "0x88e6A0c2dDD26FEEb64F039a2c41296FcB3f5640"
   ```
   `Config::validate` enforces `state.pools.len() >= 1` and unique pool addresses.
5. **Decoding strategy**: hand-rolled byte slicing for the 3 selectors (their return shapes are stable ABI). Avoids pulling `alloy-sol-types` / `alloy-contract` features. All ABI return values are 32-byte big-endian words, **left-padded** for unsigned integers and **sign-extended** for signed integers; the decode helpers must validate the padding bytes are correct or return `StateError::Decode(_)`.
   - **UniV2 `getReserves()`** returns 3×32 = 96 bytes: word 0 = `uint112 reserve0` (high 14 bytes MUST be zero); word 1 = `uint112 reserve1` (high 14 bytes MUST be zero); word 2 = `uint32 blockTimestampLast` (high 28 bytes MUST be zero, low 4 bytes BE = the value).
   - **UniV3 `slot0()`** returns 7×32 = 224 bytes; only words 0 and 1 are persisted (the rest — observationIndex, observationCardinality, observationCardinalityNext, feeProtocol, unlocked — are intentionally read past + ignored). Word 0 = `uint160 sqrtPriceX96` (high 12 bytes MUST be zero, low 20 bytes BE). Word 1 = `int24 tick` (signed): low 3 bytes = magnitude; high 29 bytes MUST be either all `0x00` (positive) or all `0xff` (negative two's-complement sign-extended); decode validates the high bytes match the sign of the low 3 bytes interpreted as `i24`. The 24-bit tick value is then sign-extended to `i32` for the `PoolState::UniV3.tick` field.
   - **UniV3 `liquidity()`** returns 1×32 = 32 bytes: `uint128`, **left-padded** (high 16 bytes MUST be zero, low 16 bytes BE = the `u128` value). Decoding validates the high bytes are zero before reading the low 16 as a `u128`.

   `PoolState` only carries the fields actually persisted: `UniV2 { reserve0: U256, reserve1: U256, block_timestamp_last: u32 }` and `UniV3 { sqrt_price_x96: U256, tick: i32, liquidity: u128 }`. (Phase 2 stores `reserve0`/`reserve1` as `U256` for convenient downstream math even though the on-wire type is `uint112`; they are validated to fit `uint112` at decode time.)
6. **`StateError::Snapshot(JournalError)` `#[from]`**: yes; `crates/journal::JournalError` is already `#[non_exhaustive]` so additive variants in journal won't cascade-break P2-B.
7. **Determinism for the Gate test**: addressed in Risk Decision 1 above — `EthCaller` is `pub` and `StateEngine::with_caller(...)` is `pub` so P2-C's gate test (in `crates/replay/tests/`, a downstream crate) can drive the engine with recorded fixtures. `pub(crate)` + `#[cfg(test)]` was rejected in v0.2 per Codex 20:48:30 because `cfg(test)` doesn't fire for downstream crates.

## Test matrix (lean per `feedback_phase2_doc_volume.md`)

`crates/state` (4 tests):
- **S-1 happy** `state_engine_refresh_decodes_univ2_reserves` — mock EthCaller returns canned 96-byte `getReserves` response for one V2 pool; assert returned event has `PoolState::UniV2 { reserve0, reserve1, block_timestamp_last }` matching expected values; assert `RocksDbSnapshot::load` returns the same state byte-equal.
- **S-2 happy** `state_engine_refresh_decodes_univ3_slot0_and_liquidity` — mock returns canned 224-byte `slot0` + canned 32-byte `liquidity` for one V3 pool; assert `PoolState::UniV3 { sqrt_price_x96, tick, liquidity }` matches; snapshot round-trip byte-equal.
- **S-3 boundary** `state_engine_refresh_persists_per_pool_independently` — 2 pools (1 V2 + 1 V3); after `refresh_block`, both keys present in `RocksDbSnapshot`; the V2 key's value decodes as `UniV2`, the V3 key's value decodes as `UniV3`; cross-key load returns the right variant.
- **S-4 failure** `state_engine_refresh_returns_decode_error_on_malformed_abi` (NEW v0.3 per Codex 20:55:13) — `MockEthCaller` returns 96 bytes for `getReserves` whose word 0 has non-zero high bytes (uint112 padding violation); `refresh_block` returns `Err(StateError::Decode(_))` for that pool AND no entry is written to `RocksDbSnapshot` for that key (snapshot.load returns `Ok(None)`). Asserts the chosen error-vs-panic posture from Risk Decision 5.

Total: 4 new tests; workspace cumulative target 59 → **63**.

## Workspace + per-crate dependency deltas

Workspace `[workspace.dependencies]`: no new entries (alloy already pinned in P2-A).

`crates/state/Cargo.toml`:
- Runtime: `rust-lmax-mev-types = { path = "../types" }`, `rust-lmax-mev-node = { path = "../node" }`, `rust-lmax-mev-journal = { path = "../journal" }`, `rust-lmax-mev-config = { path = "../config" }`, `alloy = { workspace = true }`, `alloy-primitives = { workspace = true, features = ["serde"] }`, `serde = { workspace = true }`, `bincode = { workspace = true }`, `thiserror = { workspace = true }`, `tracing = { workspace = true }`, `async-trait = "0.1"`.
- Dev: `tokio = { workspace = true, features = ["test-util", "macros"] }`, `tempfile = "3"`.

`crates/config/Cargo.toml`: no change (alloy-primitives already added in P2-A commit 2 with `serde` feature).

`crates/config/src/lib.rs` additive:
- New `StateConfig { pools: Vec<PoolConfig> }`, `PoolConfig { kind: PoolKind, address: Address }`, `PoolKind { UniswapV2, UniswapV3Fee005 }` — `#[serde(deny_unknown_fields)]`, `#[serde(rename_all = "snake_case")]` on the enum.
- `Config` gains `pub state: StateConfig`.
- `Config::validate` adds `EmptyStatePools` and `DuplicatePoolAddress`.

## Commit grouping (4–5 commits)

1. **docs: add Phase 2 Batch B state-engine execution note** — this file.
2. **feat(node): add eth_call_at_block additive API** — single additive method on `NodeProvider` per Risk Decision 2; same retry-then-fallback policy as `eth_call`. 1 new test (`eth_call_at_block_pins_block_id`). Stays inside the `crates/node` source tree because the API legitimately belongs to NodeProvider; no API renames or removals.
3. **chore(workspace): scaffold crates/state + state config** — new crate scaffolding + config additive section.
4. **feat(state): PoolKind/PoolState/StateUpdateEvent + decode helpers + StateEngine::refresh_block (S-1..S-3)** — full state-engine impl with `pub EthCaller` trait + 3 tests.
5. **chore(batch-p2-b): final fmt/clippy cleanup** (only if drift surfaces at batch close).

Per `feedback_verification_cadence.md`: targeted `cargo test -p <crate>` per commit; full workspace gates ONLY at batch close.

## Forbidden delta (only NEW)

- No live-mainnet calls (mock EthCaller in tests).
- No `revm` (Phase 3).
- No alloy `sol!` macros / contract bindings — hand-rolled selector decode keeps the compile surface lean.
- The single additive `NodeProvider::eth_call_at_block` API is the ONLY P2-A-crate edit allowed in this batch; no API renames, no removals, no behavior changes to existing methods.
- All standing Phase 1 + Phase 2 forbids carry over.

## Question for Codex (pre-impl)

v0.3 incorporates Codex 20:55:13 (3 targeted items): `StateEngine` field is `caller: Arc<dyn EthCaller>` (production wraps via `NodeEthCaller`), exact `EthCaller` trait signature documented inline, S-4 decode-failure test added.

Open question: anything else needed before the 5-commit ladder runs?

If APPROVED: execute the 5-commit ladder. If REVISION REQUIRED: edit + re-emit. If ADR/scope change: HALT.
