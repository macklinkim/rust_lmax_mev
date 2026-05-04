//! Phase 4 P4-B archive-state loader feeding real revm.
//!
//! Per the manual-Codex-APPROVED P4-B v0.2 execution note
//! (`docs/superpowers/plans/2026-05-04-phase-4-batch-b-state-fetcher-execution.md`).
//!
//! - `StateFetcher` trait: object-safe; one async `fetch_pool` method
//!   keyed by `(pool, block_hash, layout)`.
//! - `ArchiveStateFetcher` production impl: wraps `Arc<NodeProvider>` +
//!   bounded LRU caches keyed by `(Address, BlockHash)` for code and
//!   `(Address, BlockHash, U256)` for storage.
//! - `PoolSlotLayout` two-phase resolver: `base_slots` returns the
//!   unconditional list; `derived_slots(already_fetched)` returns
//!   slots derived from prior values (capped at depth 3 to avoid loops).
//!   `CallerSuppliedSlots` + `NoExtraSlots` ship in P4-B; verified
//!   `UniswapV2Layout` / `UniswapV3Fee005Layout` impls land in P4-C
//!   against recorded mainnet fixtures.
//! - `FetchedPoolState` flat-bytes shape (per DP-4): `Bytes` for code,
//!   `Vec<(U256, B256)>` for storage. P4-C translates into
//!   `revm::db::CacheDB`. Auxiliary contract state (ERC-20 / callback
//!   receivers / EOA balances) is the P4-C concern — see the P4-B v0.2
//!   §"P4-C handoff caveat".
//! - Metrics per ADR-008 + Codex 2026-05-04 22:48 Q-B7:
//!   `state_fetcher_*_cache_{hits,misses}_total` +
//!   `state_fetcher_archive_calls_total{method=...}` +
//!   `state_fetcher_fetch_pool_errors_total{kind=...}`. Public
//!   `CacheStats` snapshot retained for unit-test determinism.
//! - DP-1 no-fallback: archive errors propagate directly; the fetcher
//!   never falls back to non-archive `NodeProvider` paths (P4-A
//!   policy).

pub mod storage_key;

use std::num::NonZeroUsize;
use std::sync::Arc;

use alloy::eips::BlockId;
use alloy_primitives::{Address, Bytes, B256, U256};
use async_trait::async_trait;
use lru::LruCache;
use parking_lot::Mutex;
use rust_lmax_mev_node::{NodeError, NodeProvider};
use rust_lmax_mev_state::PoolId;

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("node error: {0}")]
    Node(#[from] NodeError),
    #[error("invalid block_hash zero")]
    InvalidBlockHash,
    #[error("derived-slot loop exceeded max depth ({0})")]
    DerivedSlotsTooDeep(u8),
    #[error("internal: {0}")]
    Internal(String),
}

/// Maximum number of `PoolSlotLayout::derived_slots` recursive passes.
/// 3 covers UniV3's slot0→tickBitmap→ticks chain with one head-room
/// pass; pathological loops abort with `FetchError::DerivedSlotsTooDeep`.
pub const DERIVED_SLOTS_MAX_DEPTH: u8 = 3;

/// LRU bounds. Defaults: 4096 bytecode entries (~40 MB at 10 KB avg);
/// 65536 storage entries (~6 MB at 96 B per entry).
#[derive(Debug, Clone)]
pub struct StateFetcherConfig {
    pub bytecode_cache_capacity: NonZeroUsize,
    pub storage_cache_capacity: NonZeroUsize,
}

impl StateFetcherConfig {
    pub fn defaults() -> Self {
        Self {
            bytecode_cache_capacity: NonZeroUsize::new(4096).expect("4096 != 0"),
            storage_cache_capacity: NonZeroUsize::new(65536).expect("65536 != 0"),
        }
    }
}

/// Two-phase per-pool slot resolver. P4-C ships verified Uniswap
/// V2/V3 impls; P4-B exercises the loop with `CallerSuppliedSlots`
/// + `NoExtraSlots`.
pub trait PoolSlotLayout: Send + Sync {
    /// Unconditional slot list — fetched first.
    fn base_slots(&self, pool: &PoolId) -> Vec<U256>;
    /// Slots derived from already-fetched `(slot, value)` pairs.
    /// Returning empty terminates the derivation loop. May be called
    /// repeatedly, capped at `DERIVED_SLOTS_MAX_DEPTH`.
    fn derived_slots(&self, pool: &PoolId, already_fetched: &[(U256, B256)]) -> Vec<U256>;
}

/// Always returns the same caller-supplied slot list. Tests + callers
/// who already know the exact slot set.
pub struct CallerSuppliedSlots(pub Vec<U256>);

impl PoolSlotLayout for CallerSuppliedSlots {
    fn base_slots(&self, _pool: &PoolId) -> Vec<U256> {
        self.0.clone()
    }
    fn derived_slots(&self, _pool: &PoolId, _already: &[(U256, B256)]) -> Vec<U256> {
        Vec::new()
    }
}

/// Fetch pool bytecode only; no storage slots.
pub struct NoExtraSlots;

impl PoolSlotLayout for NoExtraSlots {
    fn base_slots(&self, _pool: &PoolId) -> Vec<U256> {
        Vec::new()
    }
    fn derived_slots(&self, _pool: &PoolId, _already: &[(U256, B256)]) -> Vec<U256> {
        Vec::new()
    }
}

/// One auxiliary contract: `(address, code, storage slots)`. P4-B
/// layouts always emit empty auxiliary; P4-C populates this with
/// ERC-20 / callback receiver / EOA state per the v0.2 §"P4-C handoff
/// caveat".
pub type AuxiliaryContract = (Address, Bytes, Vec<(U256, B256)>);

/// Flat per-pool snapshot at a pinned block. P4-C translates into a
/// `revm::db::CacheDB`. `pool_storage` is sorted by slot ascending for
/// determinism (S-F-2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedPoolState {
    pub pool: PoolId,
    pub block_hash: B256,
    pub pool_code: Bytes,
    pub pool_storage: Vec<(U256, B256)>,
    pub auxiliary: Vec<AuxiliaryContract>,
}

/// Cache hit/miss counters snapshot for unit-test determinism.
/// `metrics::counter!` emissions ALSO go to the global `metrics`
/// registry per ADR-008 — this snapshot is the test-friendly mirror.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStats {
    pub bytecode_hits: u64,
    pub bytecode_misses: u64,
    pub storage_hits: u64,
    pub storage_misses: u64,
}

/// Object-safe public API.
#[async_trait]
pub trait StateFetcher: Send + Sync {
    async fn fetch_pool(
        &self,
        pool: &PoolId,
        block_hash: B256,
        layout: &dyn PoolSlotLayout,
    ) -> Result<FetchedPoolState, FetchError>;
}

/// Private archive transport — abstracts the three P4-A archive
/// methods so tests substitute `MockArchiveBackend` without spinning
/// up a `NodeProvider`. Production impl is `NodeProviderBackend`.
#[async_trait]
trait ArchiveBackend: Send + Sync {
    async fn get_storage_at(
        &self,
        address: Address,
        slot: U256,
        block_hash: B256,
    ) -> Result<B256, NodeError>;
    async fn get_code(&self, address: Address, block_hash: B256) -> Result<Bytes, NodeError>;
}

struct NodeProviderBackend(Arc<NodeProvider>);

#[async_trait]
impl ArchiveBackend for NodeProviderBackend {
    async fn get_storage_at(
        &self,
        address: Address,
        slot: U256,
        block_hash: B256,
    ) -> Result<B256, NodeError> {
        self.0
            .eth_get_storage_at(address, slot, BlockId::Hash(block_hash.into()))
            .await
    }
    async fn get_code(&self, address: Address, block_hash: B256) -> Result<Bytes, NodeError> {
        self.0
            .eth_get_code(address, BlockId::Hash(block_hash.into()))
            .await
    }
}

type CodeKey = (Address, B256);
type StorageKey = (Address, B256, U256);

pub struct ArchiveStateFetcher {
    backend: Arc<dyn ArchiveBackend>,
    code_cache: Mutex<LruCache<CodeKey, Bytes>>,
    storage_cache: Mutex<LruCache<StorageKey, B256>>,
    stats: Mutex<CacheStats>,
}

impl std::fmt::Debug for ArchiveStateFetcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArchiveStateFetcher")
            .field("stats", &*self.stats.lock())
            .finish_non_exhaustive()
    }
}

impl ArchiveStateFetcher {
    pub fn new(node: Arc<NodeProvider>, cfg: StateFetcherConfig) -> Self {
        Self::with_backend(Arc::new(NodeProviderBackend(node)), cfg)
    }

    fn with_backend(backend: Arc<dyn ArchiveBackend>, cfg: StateFetcherConfig) -> Self {
        Self {
            backend,
            code_cache: Mutex::new(LruCache::new(cfg.bytecode_cache_capacity)),
            storage_cache: Mutex::new(LruCache::new(cfg.storage_cache_capacity)),
            stats: Mutex::new(CacheStats::default()),
        }
    }

    pub fn cache_stats(&self) -> CacheStats {
        *self.stats.lock()
    }

    async fn get_code_cached(
        &self,
        address: Address,
        block_hash: B256,
    ) -> Result<Bytes, FetchError> {
        let key = (address, block_hash);
        if let Some(hit) = self.code_cache.lock().get(&key).cloned() {
            self.stats.lock().bytecode_hits += 1;
            metrics::counter!("state_fetcher_bytecode_cache_hits_total").increment(1);
            return Ok(hit);
        }
        self.stats.lock().bytecode_misses += 1;
        metrics::counter!("state_fetcher_bytecode_cache_misses_total").increment(1);
        metrics::counter!("state_fetcher_archive_calls_total", "method" => "get_code").increment(1);
        let code = self
            .backend
            .get_code(address, block_hash)
            .await
            .map_err(|e| {
                self.bump_error(&e);
                FetchError::Node(e)
            })?;
        self.code_cache.lock().put(key, code.clone());
        Ok(code)
    }

    async fn get_storage_cached(
        &self,
        address: Address,
        slot: U256,
        block_hash: B256,
    ) -> Result<B256, FetchError> {
        let key = (address, block_hash, slot);
        if let Some(hit) = self.storage_cache.lock().get(&key).copied() {
            self.stats.lock().storage_hits += 1;
            metrics::counter!("state_fetcher_storage_cache_hits_total").increment(1);
            return Ok(hit);
        }
        self.stats.lock().storage_misses += 1;
        metrics::counter!("state_fetcher_storage_cache_misses_total").increment(1);
        metrics::counter!("state_fetcher_archive_calls_total", "method" => "get_storage_at")
            .increment(1);
        let value = self
            .backend
            .get_storage_at(address, slot, block_hash)
            .await
            .map_err(|e| {
                self.bump_error(&e);
                FetchError::Node(e)
            })?;
        self.storage_cache.lock().put(key, value);
        Ok(value)
    }

    fn bump_error(&self, e: &NodeError) {
        let kind = match e {
            NodeError::ArchiveNotConfigured => "archive_not_configured",
            NodeError::Transport(_) => "transport",
            NodeError::Decode(_) => "decode",
            _ => "other",
        };
        metrics::counter!("state_fetcher_fetch_pool_errors_total", "kind" => kind).increment(1);
    }
}

#[async_trait]
impl StateFetcher for ArchiveStateFetcher {
    async fn fetch_pool(
        &self,
        pool: &PoolId,
        block_hash: B256,
        layout: &dyn PoolSlotLayout,
    ) -> Result<FetchedPoolState, FetchError> {
        if block_hash == B256::ZERO {
            return Err(FetchError::InvalidBlockHash);
        }

        let pool_code = self.get_code_cached(pool.address, block_hash).await?;

        let mut all_fetched: Vec<(U256, B256)> = Vec::new();
        let mut to_fetch: Vec<U256> = layout.base_slots(pool);
        let mut depth: u8 = 0;

        loop {
            // Dedup against already-fetched slots so derived layouts can
            // safely return overlapping sets without inflating the loop.
            to_fetch.sort();
            to_fetch.dedup();
            to_fetch.retain(|s| !all_fetched.iter().any(|(prev, _)| prev == s));
            if to_fetch.is_empty() {
                break;
            }
            for slot in to_fetch.drain(..) {
                let value = self
                    .get_storage_cached(pool.address, slot, block_hash)
                    .await?;
                all_fetched.push((slot, value));
            }
            depth += 1;
            let next = layout.derived_slots(pool, &all_fetched);
            if next.is_empty() {
                break;
            }
            if depth >= DERIVED_SLOTS_MAX_DEPTH {
                return Err(FetchError::DerivedSlotsTooDeep(DERIVED_SLOTS_MAX_DEPTH));
            }
            to_fetch = next;
        }

        all_fetched.sort_by_key(|(slot, _)| *slot);

        Ok(FetchedPoolState {
            pool: pool.clone(),
            block_hash,
            pool_code,
            pool_storage: all_fetched,
            auxiliary: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_lmax_mev_state::PoolKind;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};

    type StorageMap = HashMap<(Address, B256, U256), Result<B256, NodeError>>;
    type CodeMap = HashMap<(Address, B256), Result<Bytes, NodeError>>;

    /// Counts backend invocations so cache-hit tests can assert zero
    /// new calls on the second pass.
    struct MockArchiveBackend {
        storage: Mutex<StorageMap>,
        code: Mutex<CodeMap>,
        storage_calls: AtomicU64,
        code_calls: AtomicU64,
        /// If set, every backend call returns this error (S-F-5/6).
        force_error: Mutex<Option<NodeError>>,
    }

    impl MockArchiveBackend {
        fn new() -> Self {
            Self {
                storage: Mutex::new(HashMap::new()),
                code: Mutex::new(HashMap::new()),
                storage_calls: AtomicU64::new(0),
                code_calls: AtomicU64::new(0),
                force_error: Mutex::new(None),
            }
        }
        fn put_storage(&self, addr: Address, block: B256, slot: U256, val: B256) {
            self.storage.lock().insert((addr, block, slot), Ok(val));
        }
        fn put_code(&self, addr: Address, block: B256, code: Bytes) {
            self.code.lock().insert((addr, block), Ok(code));
        }
        fn force(&self, e: NodeError) {
            *self.force_error.lock() = Some(e);
        }
    }

    fn clone_node_err(e: &NodeError) -> NodeError {
        // NodeError doesn't impl Clone (intentional — variants own
        // String). Test-only shallow clone.
        match e {
            NodeError::Transport(s) => NodeError::Transport(s.clone()),
            NodeError::ArchiveNotConfigured => NodeError::ArchiveNotConfigured,
            NodeError::Rpc(s) => NodeError::Rpc(s.clone()),
            NodeError::Decode(s) => NodeError::Decode(s.clone()),
            NodeError::WsConnect(s) => NodeError::WsConnect(s.clone()),
            NodeError::Closed => NodeError::Closed,
            _ => NodeError::Rpc("unknown variant in test clone".into()),
        }
    }

    #[async_trait]
    impl ArchiveBackend for MockArchiveBackend {
        async fn get_storage_at(
            &self,
            address: Address,
            slot: U256,
            block_hash: B256,
        ) -> Result<B256, NodeError> {
            self.storage_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(e) = self.force_error.lock().as_ref() {
                return Err(clone_node_err(e));
            }
            match self.storage.lock().get(&(address, block_hash, slot)) {
                Some(Ok(v)) => Ok(*v),
                Some(Err(e)) => Err(clone_node_err(e)),
                None => Err(NodeError::Rpc(format!(
                    "no fixture for storage ({address}, {block_hash}, {slot})"
                ))),
            }
        }
        async fn get_code(&self, address: Address, block_hash: B256) -> Result<Bytes, NodeError> {
            self.code_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(e) = self.force_error.lock().as_ref() {
                return Err(clone_node_err(e));
            }
            match self.code.lock().get(&(address, block_hash)) {
                Some(Ok(v)) => Ok(v.clone()),
                Some(Err(e)) => Err(clone_node_err(e)),
                None => Err(NodeError::Rpc(format!(
                    "no fixture for code ({address}, {block_hash})"
                ))),
            }
        }
    }

    fn fetcher_with(backend: Arc<MockArchiveBackend>) -> ArchiveStateFetcher {
        ArchiveStateFetcher::with_backend(backend, StateFetcherConfig::defaults())
    }

    fn pool() -> PoolId {
        PoolId {
            kind: PoolKind::UniswapV2,
            address: alloy_primitives::address!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        }
    }

    fn b256_from_u8(b: u8) -> B256 {
        B256::from([b; 32])
    }

    /// S-F-1 happy: caller-supplied slots → code + 3 sorted entries.
    #[tokio::test]
    async fn archive_state_fetcher_fetch_with_caller_supplied_slots_returns_code_and_values() {
        let backend = Arc::new(MockArchiveBackend::new());
        let p = pool();
        let block = b256_from_u8(0x11);
        backend.put_code(p.address, block, Bytes::from_static(&[0x60, 0x00]));
        backend.put_storage(p.address, block, U256::from(2u64), b256_from_u8(0xaa));
        backend.put_storage(p.address, block, U256::from(0u64), b256_from_u8(0xbb));
        backend.put_storage(p.address, block, U256::from(1u64), b256_from_u8(0xcc));

        let fetcher = fetcher_with(backend);
        let layout =
            CallerSuppliedSlots(vec![U256::from(2u64), U256::from(0u64), U256::from(1u64)]);
        let out = fetcher.fetch_pool(&p, block, &layout).await.unwrap();
        assert_eq!(out.pool_code.as_ref(), &[0x60, 0x00]);
        assert_eq!(
            out.pool_storage,
            vec![
                (U256::from(0u64), b256_from_u8(0xbb)),
                (U256::from(1u64), b256_from_u8(0xcc)),
                (U256::from(2u64), b256_from_u8(0xaa)),
            ],
            "storage must be sorted by slot ascending"
        );
        assert!(out.auxiliary.is_empty());
    }

    /// S-F-2 determinism: same inputs → byte-identical output.
    #[tokio::test]
    async fn archive_state_fetcher_fetch_pool_byte_identical_across_two_calls() {
        let backend = Arc::new(MockArchiveBackend::new());
        let p = pool();
        let block = b256_from_u8(0x22);
        backend.put_code(p.address, block, Bytes::from_static(&[0x00]));
        for s in 0..5u64 {
            backend.put_storage(p.address, block, U256::from(s), b256_from_u8(s as u8));
        }
        let fetcher = fetcher_with(backend);
        let layout = CallerSuppliedSlots((0..5u64).map(U256::from).collect());
        let a = fetcher.fetch_pool(&p, block, &layout).await.unwrap();
        let b = fetcher.fetch_pool(&p, block, &layout).await.unwrap();
        assert_eq!(a, b, "deterministic same-input output");
    }

    /// S-F-3 cache hit: second call does zero new backend invocations.
    #[tokio::test]
    async fn archive_state_fetcher_second_fetch_hits_cache_and_skips_backend() {
        let backend = Arc::new(MockArchiveBackend::new());
        let p = pool();
        let block = b256_from_u8(0x33);
        backend.put_code(p.address, block, Bytes::from_static(&[0x42]));
        backend.put_storage(p.address, block, U256::from(7u64), b256_from_u8(0x77));
        backend.put_storage(p.address, block, U256::from(8u64), b256_from_u8(0x88));

        let backend_for_fetcher: Arc<dyn ArchiveBackend> = backend.clone();
        let fetcher =
            ArchiveStateFetcher::with_backend(backend_for_fetcher, StateFetcherConfig::defaults());
        let layout = CallerSuppliedSlots(vec![U256::from(7u64), U256::from(8u64)]);
        let _ = fetcher.fetch_pool(&p, block, &layout).await.unwrap();
        let storage_after_first = backend.storage_calls.load(Ordering::SeqCst);
        let code_after_first = backend.code_calls.load(Ordering::SeqCst);
        assert_eq!(storage_after_first, 2);
        assert_eq!(code_after_first, 1);

        let _ = fetcher.fetch_pool(&p, block, &layout).await.unwrap();
        assert_eq!(
            backend.storage_calls.load(Ordering::SeqCst),
            storage_after_first,
            "second fetch must not invoke storage backend"
        );
        assert_eq!(
            backend.code_calls.load(Ordering::SeqCst),
            code_after_first,
            "second fetch must not invoke code backend"
        );
        let stats = fetcher.cache_stats();
        assert!(stats.bytecode_hits >= 1);
        assert_eq!(stats.storage_hits, 2);
    }

    /// S-F-4 cache miss new block: A then B both invoke backend.
    #[tokio::test]
    async fn archive_state_fetcher_different_block_hash_misses_cache() {
        let backend = Arc::new(MockArchiveBackend::new());
        let p = pool();
        let block_a = b256_from_u8(0x44);
        let block_b = b256_from_u8(0x55);
        backend.put_code(p.address, block_a, Bytes::from_static(&[0xa1]));
        backend.put_code(p.address, block_b, Bytes::from_static(&[0xb1]));
        backend.put_storage(p.address, block_a, U256::from(0u64), b256_from_u8(0x01));
        backend.put_storage(p.address, block_b, U256::from(0u64), b256_from_u8(0x02));
        let backend_for_fetcher: Arc<dyn ArchiveBackend> = backend.clone();
        let fetcher =
            ArchiveStateFetcher::with_backend(backend_for_fetcher, StateFetcherConfig::defaults());
        let layout = CallerSuppliedSlots(vec![U256::from(0u64)]);
        let _ = fetcher.fetch_pool(&p, block_a, &layout).await.unwrap();
        let _ = fetcher.fetch_pool(&p, block_b, &layout).await.unwrap();
        assert_eq!(backend.code_calls.load(Ordering::SeqCst), 2);
        assert_eq!(backend.storage_calls.load(Ordering::SeqCst), 2);
    }

    /// S-F-5 abort no-config: ArchiveNotConfigured propagates.
    #[tokio::test]
    async fn archive_state_fetcher_propagates_archive_not_configured() {
        let backend = Arc::new(MockArchiveBackend::new());
        backend.force(NodeError::ArchiveNotConfigured);
        let fetcher = fetcher_with(backend);
        let p = pool();
        let block = b256_from_u8(0x66);
        let layout = NoExtraSlots;
        let err = fetcher
            .fetch_pool(&p, block, &layout)
            .await
            .expect_err("must propagate ArchiveNotConfigured");
        assert!(
            matches!(err, FetchError::Node(NodeError::ArchiveNotConfigured)),
            "got {err:?}"
        );
    }

    /// S-F-6 abort transport no-fallback: Transport(_) propagates directly.
    #[tokio::test]
    async fn archive_state_fetcher_propagates_transport_error_no_fallback() {
        let backend = Arc::new(MockArchiveBackend::new());
        backend.force(NodeError::Transport("simulated archive down".into()));
        let fetcher = fetcher_with(backend.clone());
        let p = pool();
        let block = b256_from_u8(0x77);
        let layout = CallerSuppliedSlots(vec![U256::from(0u64)]);
        let err = fetcher
            .fetch_pool(&p, block, &layout)
            .await
            .expect_err("must propagate Transport");
        assert!(
            matches!(err, FetchError::Node(NodeError::Transport(_))),
            "got {err:?}"
        );
        // No retry — single invocation reached the backend (the code
        // call) before erroring.
        assert_eq!(backend.code_calls.load(Ordering::SeqCst), 1);
        assert_eq!(backend.storage_calls.load(Ordering::SeqCst), 0);
    }

    /// S-F-7 boundary zero block_hash rejected (per Codex 22:48
    /// non-blocking note: NonZeroUsize is type-enforced so we cannot
    /// test zero-cache-capacity at runtime; substitute zero-block-hash).
    #[tokio::test]
    async fn archive_state_fetcher_rejects_zero_block_hash() {
        let backend = Arc::new(MockArchiveBackend::new());
        let fetcher = fetcher_with(backend.clone());
        let p = pool();
        let layout = NoExtraSlots;
        let err = fetcher
            .fetch_pool(&p, B256::ZERO, &layout)
            .await
            .expect_err("zero block hash must be rejected");
        assert!(matches!(err, FetchError::InvalidBlockHash), "got {err:?}");
        assert_eq!(
            backend.code_calls.load(Ordering::SeqCst),
            0,
            "zero block hash must reject before any backend call"
        );
        assert_eq!(backend.storage_calls.load(Ordering::SeqCst), 0);
    }

    /// S-F-8 cache eviction: capacity = 2; insert 3 distinct entries;
    /// oldest evicted (re-fetch is a miss).
    #[tokio::test]
    async fn archive_state_fetcher_lru_evicts_oldest_when_capacity_full() {
        let backend = Arc::new(MockArchiveBackend::new());
        let p = pool();
        let blocks = [b256_from_u8(0xa0), b256_from_u8(0xa1), b256_from_u8(0xa2)];
        for b in blocks.iter() {
            backend.put_code(p.address, *b, Bytes::from_static(&[0x00]));
        }
        let fetcher = ArchiveStateFetcher::with_backend(
            backend.clone() as Arc<dyn ArchiveBackend>,
            StateFetcherConfig {
                bytecode_cache_capacity: NonZeroUsize::new(2).unwrap(),
                storage_cache_capacity: NonZeroUsize::new(65536).unwrap(),
            },
        );
        let layout = NoExtraSlots;
        for b in blocks.iter() {
            let _ = fetcher.fetch_pool(&p, *b, &layout).await.unwrap();
        }
        assert_eq!(
            backend.code_calls.load(Ordering::SeqCst),
            3,
            "3 distinct misses → 3 calls"
        );
        // Re-fetch the OLDEST block — should miss because capacity=2
        // evicted it when block #3 came in.
        let _ = fetcher.fetch_pool(&p, blocks[0], &layout).await.unwrap();
        assert_eq!(
            backend.code_calls.load(Ordering::SeqCst),
            4,
            "oldest entry must have been evicted; re-fetch is a miss",
        );
    }
}
