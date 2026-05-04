# Phase 4 Batch B — State-Fetcher (Archive-State Loader for Real revm)

**Date:** 2026-05-04
**Status:** Draft v0.2 (revised after manual Codex 2026-05-04 22:35 +09:00 REVISION REQUIRED HIGH; six required revisions encoded — slot-constant claims dropped, storage-key derivation documented, UniV3 two-phase explicit, S-F-7 replaced, P4-C auxiliary-state caveat added, Q-B7 answered with metrics counters).
**Predecessor:** P4-A archive node integration CLOSED at `e2b6704` (manual Codex APPROVED MEDIUM 22:20:00).

## Scope

Land the state-fetcher feeding Phase 4's real-revm pipeline (P4-C). The fetcher consumes the P4-A archive RPC surface (`NodeProvider::eth_get_code` / `eth_get_storage_at` / `eth_get_proof`) and produces a deterministic, cached, revm-shaped `FetchedPoolState` for one pool at one pinned block.

P4-B ships the **fetcher infrastructure**: trait, production impl, LRU caching, metrics, mock-driven test matrix, and a `PoolSlotLayout` trait that pool-kind-specific layouts will implement in P4-C. **P4-B does NOT ship verified Uniswap V2 / V3 slot constants** — those land in P4-C against recorded mainnet fixtures (per Codex 22:35 revision #1: the v0.1 claim of "UniV2 reserves at slot 8, token0 at 6, token1 at 7" was unverified and load-bearing for P4-C correctness, so it is removed from P4-B scope).

1. **New crate `crates/state-fetcher`** (additive; keeps `crates/state` Phase 2 freeze intact). Public API:
   - `pub trait StateFetcher` — object-safe; one `fetch_pool` async method.
   - `pub struct ArchiveStateFetcher` — production impl wrapping `Arc<NodeProvider>` + LRU caches.
   - `pub struct FetchedPoolState` — bytecode + per-slot storage values + the pinning `BlockHash`.
   - `pub trait PoolSlotLayout` — caller (P4-C) supplies the `Vec<U256>` of slots to fetch for a given pool. P4-B ships a `CallerSuppliedSlots(Vec<U256>)` impl plus a `NoExtraSlots` impl; `UniswapV2Layout` / `UniswapV3Fee005Layout` impls land in P4-C.
2. **LRU cache layer** keyed by:
   - **Bytecode cache**: `(Address, BlockHash)` → `Bytes`. Bytecode is contract-immutable post-deployment in the absence of `SELFDESTRUCT`/`CREATE2` redeploy, but we still pin per `BlockHash` for correctness against historical re-orgs.
   - **Storage cache**: `(Address, BlockHash, U256 slot)` → `B256`. Per-block — different blocks reflect different storage state.
   - Both caches are bounded LRU (capacity in config; defaults 4096 bytecode entries, 65536 storage entries).
   - **Metrics**: each cache emits `state_fetcher_<class>_cache_hits_total` + `_misses_total` counters via `metrics::counter!` per ADR-008 (Codex 22:35 Q-B7 answer; archive reads are cost-sensitive so cache observability is operationally important). Public `CacheStats` snapshot retained for deterministic unit-test assertions.
3. **No archive→primary fallback** (inherits P4-A DP-1). Fetcher errors map to a `FetchError` enum that downstream pipeline handles authoritatively.
4. **Determinism contract**: same `(pool, block_hash, slot list)` → byte-identical `FetchedPoolState` across two `fetch_pool` calls (S-F-2 test mirrors P3-E S-2 + P2-C G-Replay pattern). This is the load-bearing invariant for P4-C real-revm replay.

P4-B does NOT yet wire the fetcher into `LocalSimulator` (P4-C's job) and does NOT flip `ProfitSource::HeuristicPassthrough → RevmComputed` (P4-C's job).

## Decision points (defaults; Codex pre-impl review confirms)

- **DP-1 (cache invalidation policy)**: per-block keys are immutable forever — the (Address, BlockHash, slot) tuple uniquely identifies a historical archive read. LRU eviction is the only removal mechanism. New blocks insert new keys; old keys age out under capacity pressure. Rationale: archive RPC reads at a fixed historical block are functionally pure; caching them indefinitely is correct.
- **DP-2 (selective vs proof-based slot fetching)** — Codex Q-B1 ANSWERED: P4-B uses **direct `eth_get_storage_at` per slot**, NOT `eth_get_proof` + Merkle reconstruction. Rationale:
  - `eth_get_storage_at` is cheaper (single value vs proof) and matches the slot-by-slot consumption pattern of revm's `Database::storage()` interface.
  - `eth_get_proof` is held in reserve for a P4 follow-up where we may want trie-verified state for stronger correctness claims; P4-B doesn't need the verification layer.
  - Per-pool slot list is small (UniV2: ~3-5 slots; UniV3: ~10-15 slots within the active tick window; concrete counts land in P4-C). At 5-10 pools × ~10 slots × 1-2 blocks/sec = 50-200 archive calls/sec, well within Alchemy/Infura archive-tier sustained limits.
- **DP-3 (UniV3 two-phase fetch shape)** — Codex Q-B2 ANSWERED, expanded per revision #2 + #3: V3 swap state requires reading `slot0` (which contains the current `tick` packed in bits 160..184) BEFORE the per-tick storage slots can be derived. P4-B's fetcher therefore supports a **two-phase fetch shape** at the `PoolSlotLayout` boundary:
  - **Phase 1**: `PoolSlotLayout::base_slots()` returns the unconditional slot list (e.g., for V3: `slot0`, `liquidity`, `feeGrowthGlobal0X128`, `feeGrowthGlobal1X128`).
  - **Phase 2**: `PoolSlotLayout::derived_slots(base_values: &[(U256, B256)])` returns additional slots derived from Phase 1 values (e.g., for V3: parse `tick` from `slot0`, compute the 3 `tickBitmap` word positions around it, and after fetching those, the tick-mapping slots for the set bits).
  - The fetcher loop runs Phase 1 → Phase 2 → optional Phase 3 (`derived_slots` may itself be called recursively until it returns empty, capped at a small max-depth = 3 to avoid pathological loops).
  - P4-B implements this loop generically; P4-C ships the `UniswapV3Fee005Layout` impl that drives it. The `CallerSuppliedSlots` and `NoExtraSlots` layouts in P4-B return their slot list from `base_slots` and an empty `derived_slots`, exercising the single-phase path.
  - Fixed 3-word `tickBitmap` window remains the v0.2 strategy. Widening (Codex Q-B2 caveat) is a P4-C/P4-D data-driven follow-up if revm reports out-of-range tick reverts on recorded fixtures.
- **DP-4 (`FetchedPoolState` shape — flat-bytes vs revm-DB)** — Codex Q-B3 ANSWERED: P4-B emits a **flat-bytes shape** (`Bytes` for code; `Vec<(U256 slot, B256 value)>` for storage; plus a `Vec<(Address, Bytes, Vec<(U256, B256)>)>` for any auxiliary contracts). P4-C is responsible for translating that into `revm::db::CacheDB`. Rationale: keeps the fetcher independent of revm's exact in-memory DB type, so future revm version bumps don't cascade through the cache layer; also makes serialization for fixture-recording trivial.
- **DP-5 (cache capacity defaults)**: 4096 bytecode entries × ~10 KB avg = ~40 MB ceiling; 65536 storage entries × 96 B = ~6 MB ceiling. Combined ~46 MB process-resident, well under any reasonable deployment budget. Configurable via `StateFetcherConfig` for operators who want tighter or looser bounds.
- **DP-6 (fixture recording strategy for tests)**: P4-B tests use **inline `const`-byte fixtures** (mirrors P2-B `MockEthCaller` + P2-C `RecordedEthCaller` pattern). No JSON files, no `serde_json` dep. A `MockArchiveBackend` test type implements a private fetcher backend trait and returns canned `(slot → value)` + `(address → bytecode)` maps. Phase 4 follow-up may add an env-gated live-archive smoke (deferred to P4-C or later).
- **DP-7 (re-org safety)** — Codex Q-B4 ANSWERED: state-fetcher reads at a caller-supplied `BlockHash` (NOT `BlockNumber`). Fetcher is **hash-pure** — the archive node's response for a fixed `BlockHash` is the historical state at that hash regardless of subsequent re-orgs. The orchestrator (P4-C/D) owns the policy decision of "should the engine re-execute against the new canonical hash if a re-org demoted the original?" P4-B emits no `BlockReorged` warning.

## Storage-key derivation (Codex 22:35 revision #2 — REQUIRED)

Solidity mapping storage at slot `M` for key `K` lives at `keccak256(abi.encode(K, M))`. P4-B ships a small `storage_key` helper module with the canonical derivation functions:

```rust
// crates/state-fetcher/src/storage_key.rs

use alloy_primitives::{B256, U256, keccak256};

/// Storage slot for `mapping(<keytype> => _) value` at slot `mapping_slot`,
/// indexed by `key`. `key` is left-padded to 32 bytes per ABI rules.
/// Used for UniV3 `ticks: mapping(int24 => Tick.Info)` and
/// `tickBitmap: mapping(int16 => uint256)` derivations in P4-C.
pub fn mapping_slot_u256(mapping_slot: U256, key: B256) -> U256 {
    let mut buf = [0u8; 64];
    buf[..32].copy_from_slice(key.as_slice());
    buf[32..].copy_from_slice(&mapping_slot.to_be_bytes::<32>());
    U256::from_be_bytes(keccak256(buf).0)
}

/// Storage slot for the `i`-th element of an `array` declared at slot
/// `array_slot` with element size `element_words` (number of 32-byte words
/// per element; 1 for scalar arrays, more for struct arrays).
pub fn array_element_slot(array_slot: U256, i: U256, element_words: u32) -> U256 {
    let base = U256::from_be_bytes(keccak256(array_slot.to_be_bytes::<32>()).0);
    base + i * U256::from(element_words)
}
```

**P4-B ships the helper functions + their unit tests** (S-K-1 mapping derivation matches a known-good vector; S-K-2 array derivation matches a known-good vector). **P4-B does NOT ship Uniswap-specific slot CONSTANTS**: those land in P4-C inside the `UniswapV2Layout` / `UniswapV3Fee005Layout` impls and are validated against recorded mainnet pool fixtures.

The two unit tests use known-good vectors from the Solidity documentation (`solc` storage layout examples) so they verify the ABI/`keccak` math rather than a Uniswap-specific layout claim.

## API surface

```rust
// crates/state-fetcher/src/lib.rs

use alloy_primitives::{Address, B256, Bytes, U256};
use rust_lmax_mev_node::{NodeError, NodeProvider};
use rust_lmax_mev_state::PoolId;

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("node error: {0}")]
    Node(#[from] NodeError),
    #[error("invalid block_hash zero")]
    InvalidBlockHash,
    #[error("invalid config: {0}")]
    InvalidConfig(String),
    #[error("derived-slot loop exceeded max depth ({0})")]
    DerivedSlotsTooDeep(u8),
    #[error("internal: {0}")]
    Internal(String),
}

#[derive(Debug, Clone)]
pub struct StateFetcherConfig {
    pub bytecode_cache_capacity: std::num::NonZeroUsize,
    pub storage_cache_capacity: std::num::NonZeroUsize,
}

impl StateFetcherConfig {
    pub fn defaults() -> Self;
}

/// Two-phase slot resolver — Phase 1 returns unconditional slots; Phase 2
/// returns slots derived from Phase 1 values (capped at max-depth 3).
pub trait PoolSlotLayout: Send + Sync {
    fn base_slots(&self, pool: &PoolId) -> Vec<U256>;
    fn derived_slots(
        &self,
        pool: &PoolId,
        already_fetched: &[(U256, B256)],
    ) -> Vec<U256>;
}

/// Always returns the same caller-supplied slot list. Used by tests and
/// by callers who already know the exact slot set they need.
pub struct CallerSuppliedSlots(pub Vec<U256>);
impl PoolSlotLayout for CallerSuppliedSlots { /* ... */ }

/// Fetch pool bytecode only; no storage slots.
pub struct NoExtraSlots;
impl PoolSlotLayout for NoExtraSlots { /* ... */ }

/// Flat per-pool snapshot at a pinned block. P4-C translates this into
/// a revm `CacheDB` for actual simulation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedPoolState {
    pub pool: PoolId,
    pub block_hash: B256,
    pub pool_code: Bytes,
    pub pool_storage: Vec<(U256, B256)>,   // sorted by slot for determinism
    pub auxiliary: Vec<(Address, Bytes, Vec<(U256, B256)>)>,
}

#[async_trait::async_trait]
pub trait StateFetcher: Send + Sync {
    async fn fetch_pool(
        &self,
        pool: &PoolId,
        block_hash: B256,
        layout: &dyn PoolSlotLayout,
    ) -> Result<FetchedPoolState, FetchError>;
}

pub struct ArchiveStateFetcher { /* node, caches, cfg */ }

impl ArchiveStateFetcher {
    pub fn new(node: std::sync::Arc<NodeProvider>, cfg: StateFetcherConfig) -> Self;
    pub fn cache_stats(&self) -> CacheStats;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStats {
    pub bytecode_hits: u64,
    pub bytecode_misses: u64,
    pub storage_hits: u64,
    pub storage_misses: u64,
}
```

Internally a private `ArchiveBackend` trait abstracts the three archive methods so tests can substitute a `MockArchiveBackend` without `NodeProvider`. The production impl is a thin `Arc<NodeProvider>` wrapper.

## Metrics (per ADR-008 + Codex 22:35 Q-B7)

Counters emitted via the `metrics` facade:
- `state_fetcher_bytecode_cache_hits_total`
- `state_fetcher_bytecode_cache_misses_total`
- `state_fetcher_storage_cache_hits_total`
- `state_fetcher_storage_cache_misses_total`
- `state_fetcher_archive_calls_total{method="get_code|get_storage_at"}` (for cost-tracking; archive calls are the most expensive class)
- `state_fetcher_fetch_pool_errors_total{kind="archive_not_configured|transport|decode|other"}` (failure-mode visibility)

The public `CacheStats` snapshot is retained for unit-testing (S-F-3 / S-F-4 / S-F-8 assert on it directly without depending on the global `metrics` registry).

## P4-C handoff caveat (Codex 22:35 revision #5 — REQUIRED)

`FetchedPoolState` carries pool bytecode + the slot list returned by the supplied `PoolSlotLayout`, plus an `auxiliary: Vec<(Address, Bytes, Vec<(U256, B256)>)>` field for additional contract state. **P4-B fetches `auxiliary` only if the layout populates it** (the v0.2 layouts `CallerSuppliedSlots` and `NoExtraSlots` always emit empty auxiliary).

**Real Uniswap V2/V3 swap execution under revm will likely require auxiliary state** that P4-B does NOT load:

- **ERC-20 token contracts**: `WETH9` + `USDC` bytecode + the swapper-account balance / allowance slots. UniV2 `swap()` calls `IERC20(token0).transfer(to, amount0Out)` etc.; if the revm DB has no `token0` account info, the EXTCODESIZE check inside `_safeTransfer` will revert.
- **Callback receivers**: UniV3 `swap()` invokes `IUniswapV3SwapCallback(msg.sender).uniswapV3SwapCallback(...)` on the caller, which means the simulated swapper account must either be an EOA with no code (revm short-circuits the callback) or a deployed test-router contract whose bytecode is in the DB.
- **Per-account state**: simulated EOA balance must cover `gas * gas_price + value` and any `transferFrom` source accounts must hold sufficient token balance + allowance.

P4-B's deferral is **explicit and recorded here**: P4-C is responsible for either (a) extending `PoolSlotLayout` to a richer `PoolFetchLayout` that returns auxiliary contract addresses + their slot lists, OR (b) adding a separate `RevmDbBuilder` step in P4-C that loads the swapper-account / token / callback state from chain (or constructs a test EOA + skips ERC-20 transfers via a mock router). The P4-C execution note will choose explicitly.

## Test matrix (10 new in P4-B; workspace cumulative 113 → 123 in CI)

`crates/state-fetcher` (8 fetcher tests + 2 storage-key derivation tests):

- **S-K-1** `mapping_slot_u256_matches_solidity_known_good_vector` — verify `mapping_slot_u256(mapping_slot=1, key=0x...0042) == keccak256(abi.encode(0x...0042, 1))` against a hardcoded expected hash from the Solidity docs.
- **S-K-2** `array_element_slot_matches_solidity_known_good_vector` — verify `array_element_slot(array_slot=2, i=5, element_words=1) == keccak256(2) + 5` against a hardcoded expected slot.
- **S-F-1 happy** `archive_state_fetcher_fetch_with_caller_supplied_slots_returns_code_and_values` — `MockArchiveBackend` stubs return canned bytecode + N slot values; `CallerSuppliedSlots(vec![U256::from(0), U256::from(1), U256::from(2)])`; assert `pool_code` matches; `pool_storage` has exactly 3 entries sorted by slot ascending.
- **S-F-2 determinism** `archive_state_fetcher_fetch_pool_byte_identical_across_two_calls` — same `(pool, block_hash, layout)` two consecutive `fetch_pool` calls return `FetchedPoolState` with `==`. Critical for P4-C real-revm replay determinism.
- **S-F-3 cache hit** `archive_state_fetcher_second_fetch_hits_cache_and_skips_backend` — mock backend counts call invocations; second `fetch_pool` for the same `(pool, block_hash, layout)` does ZERO new backend calls; `cache_stats().bytecode_hits ≥ 1` and `storage_hits == N_slots`.
- **S-F-4 cache miss new block** `archive_state_fetcher_different_block_hash_misses_cache` — `fetch_pool` at `block_hash=A` then at `block_hash=B`; both miss → both invoke backend; total backend calls = 2 × (N_slots + 1).
- **S-F-5 abort no-config** `archive_state_fetcher_propagates_archive_not_configured` — `MockArchiveBackend` returns `NodeError::ArchiveNotConfigured` for any call; `fetch_pool` returns `Err(FetchError::Node(NodeError::ArchiveNotConfigured))` — verifies P4-A no-fallback DP-1 propagation.
- **S-F-6 abort transport no-fallback** `archive_state_fetcher_propagates_transport_error_no_fallback` — backend returns `NodeError::Transport(_)`; fetcher returns `Err(FetchError::Node(NodeError::Transport(_)))` directly. NO retry inside fetcher (P4-A's primary-retry policy applies to P2-A `eth_call` only; archive paths have no fallback by design).
- **S-F-7 boundary zero-cache-capacity rejected** (replaces v0.1 unsupported-pool-kind shim per Codex 22:35 revision #4) — `StateFetcherConfig { bytecode_cache_capacity: 0, .. }` is rejected at `ArchiveStateFetcher::new` → returns `Err(FetchError::InvalidConfig(_))`. Uses `NonZeroUsize` at the type level so the error is the parser surface (config struct uses raw `usize` to allow this test; `NonZeroUsize` enforcement happens inside `new()`). **Alternative if NonZeroUsize is type-enforced earlier**: substitute `archive_state_fetcher_rejects_zero_block_hash` testing `fetch_pool` with `B256::ZERO` returns `Err(FetchError::InvalidBlockHash)`. Pick whichever fits cleaner once the API is in code; v0.2 declares both as acceptable boundaries.
- **S-F-8 cache eviction** `archive_state_fetcher_lru_evicts_oldest_when_capacity_full` — config with `bytecode_cache_capacity = NonZeroUsize::new(2)`; insert 3 distinct `(addr, block_hash)` entries; assert oldest is evicted (next fetch for it is a miss, `bytecode_misses` increments).

NO LIVE TESTS in CI. An env-gated archive-smoke is deferred to P4-C or later (per overview Q8).

## Workspace + per-crate dependency deltas

`[workspace.dependencies]`: add `lru = "0.12"` (already used by `crates/ingress` at non-workspace pin; promote to workspace). Add `metrics = "0.23"` is already in workspace deps (verified — used by `crates/event-bus`/`crates/journal` per ADR-008); state-fetcher consumes the workspace pin.

`crates/state-fetcher/Cargo.toml`:
- runtime: `rust-lmax-mev-state` (for `PoolId`/`PoolKind`), `rust-lmax-mev-node` (for `NodeProvider`/`NodeError`), `alloy = { workspace = true }` (for `BlockId`), `alloy-primitives = { workspace = true }`, `lru = { workspace = true }`, `parking_lot = { workspace = true }`, `metrics = { workspace = true }`, `async-trait = "0.1"`, `thiserror = { workspace = true }`.
- dev: `tokio = { workspace = true }` (test-util/macros).

`crates/ingress/Cargo.toml`: bump `lru = "0.12"` direct dep to `lru = { workspace = true }` (cleanup, not strictly required for P4-B but fixes the duplicate-pin smell).

## Commit grouping (4-5 commits)

1. `docs: add Phase 4 Batch B state-fetcher execution note` — this file.
2. `chore(workspace): scaffold crates/state-fetcher + promote lru to workspace dep` — Cargo.toml edits, placeholder lib.rs, ingress lru-dep cleanup.
3. `feat(state-fetcher): StateFetcher trait + ArchiveStateFetcher + LRU caches + slot resolver + storage-key helpers + metrics` — types + impl + storage_key module + metrics emission. Targeted `cargo test -p rust-lmax-mev-state-fetcher` per code commit.
4. `test(state-fetcher): S-K-1..S-K-2 + S-F-1..S-F-8 storage-key derivation + archive fetch + cache + abort + determinism tests`.
5. (optional) `chore(batch-p4-b): pick up fmt + Cargo.lock drift at batch close`.

Per `feedback_verification_cadence.md`: targeted per-commit; full workspace gates ONLY at batch close + tail-summary append.

## Forbidden delta (only NEW)

- No archive→primary fallback (DP-1; inherited from P4-A).
- No `eth_get_proof` consumption in P4-B (DP-2; held in reserve for later).
- No revm DB type leakage into `FetchedPoolState` (DP-4 flat-bytes).
- **No verified Uniswap V2/V3 slot constants in P4-B** (Codex 22:35 revision #1; deferred to P4-C with recorded fixtures).
- **No downstream test-only `PoolKind` variant** (Codex 22:35 revision #4; do not modify upstream `crates/state` `PoolKind` enum).
- No live-network tests in CI.
- No new ADR / no scope creep into P4-C real-revm flip (`ProfitSource` stays `HeuristicPassthrough` until P4-C).
- All Phase 4 forbids carry over (no `eth_sendBundle`, no funded key, no `live_send=true`, no `.claude/`/`AGENTS.md` staging).

## Q8 hardening verified at batch close

- (a) **No funded key / signing infra**: P4-B touches no signing surface; `crates/state-fetcher` has no `Signer`/`Wallet`/`PrivateKey`/`secp256k1` types. Verified by `grep -nE "Signer|Wallet|PrivateKey|secp256k1" crates/state-fetcher/`.
- (b) **No `submit_bundle` wiring**: P4-B does not touch `BundleRelay` (P4-E concern); no submit-path code added.
- (c) **No `live_send` toggle**: P4-B does not edit config schema beyond optionally promoting `lru` to workspace dep. `live_send` is not introduced until P4-E.
- (d) **Env-gated `#[ignore]` tests only**: NO live tests in P4-B (mocked-only sufficient for boundary coverage). Env-gated archive-smoke deferred to a later batch.
- (e) **Secret redaction**: `crates/state-fetcher/src/lib.rs` will perform zero `tracing::*` logging in v0.2 (mirrors `crates/node` / `crates/state` Phase 2-3 zero-logging precedent). The `metrics::counter!` emissions carry no URL or API-key payload (only counter labels like `method="get_code"`). Verified by grep at batch close.
- (f) **Fail-closed defaults**: `StateFetcherConfig::defaults()` provides bounded LRU capacities (no unbounded growth); zero-capacity is rejected at construction time. `ArchiveNotConfigured` propagates through `FetchError::Node(_)` so downstream sees the same fail-closed signal as direct `NodeProvider` callers.

## Codex 22:35 v0.2 verdicts (encoded; standing answers unless revised)

- **Q-B1 (storage-fetch method)**: `eth_get_storage_at` per slot. Defer proof-based fetch.
- **Q-B2 (V3 tickBitmap window)**: fixed 3-word window OK because mapping derivation + two-phase fetch are documented (revisions #2 + #3). Widening = P4-C/P4-D follow-up.
- **Q-B3 (`FetchedPoolState` shape)**: flat-bytes; do not couple to revm DB types.
- **Q-B4 (re-org policy)**: fetcher hash-pure; orchestrator owns reorg.
- **Q-B5 (S-F-7 boundary)**: NO downstream test-only `PoolKind` variant; v0.2 substitutes zero-cache-capacity rejection (or zero-block-hash rejection as fallback).
- **Q-B6 (test count)**: 8 fetcher tests OK + 2 derivation tests = 10.
- **Q-B7 (metrics)**: emit `metrics::counter!` for hits/misses + retain public `CacheStats` for unit-test determinism.

## Question for Codex (v0.2 — non-scope items only)

v0.2 encodes Codex 22:35 verdicts on Q-B1..Q-B7 + addresses required revisions #1..#6. Open: confirmation-at-approval only.

If APPROVED: 4-5 commit ladder + batch-close evidence pack with auto_check.md tail-summary refresh. If REVISION: revise + re-emit. If ADR/scope/freeze change required: HALT to user.
