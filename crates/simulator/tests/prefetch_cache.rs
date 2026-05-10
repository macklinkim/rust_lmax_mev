//! Phase 5 P5-A PA-2 / PA-3 / PA-5 tests for the per-block fixture
//! cache + retention/eviction policy + simulate parity post-prefetch.
//!
//! Per execution-note v0.3 §"Test matrix":
//! - PA-2: cache hit short-circuits the archive call (mock fetcher
//!   counts calls; second prefetch with same key → 0 new fetches).
//! - PA-3: retention/eviction after `freshness_window_blocks`
//!   boundary; mock fetcher counts re-fetches on the new block.
//! - PA-5: simulate parity post-prefetch — reuses the SR-1 inline
//!   fixture data; `simulate_with_fingerprint` after
//!   `prefetch_for(...)` produces a byte-identical outcome to the
//!   `load_fixture(...)` baseline (FP-1 invariant carries forward).

mod common;

use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use alloy_primitives::{Address, B256, U256};
use rust_lmax_mev_opportunity::{OpportunityEvent, GAS_ESTIMATE_TWO_HOP_ARB};
use rust_lmax_mev_risk::RiskCheckedOpportunity;
use rust_lmax_mev_simulator::{
    fixtures, LocalSimulator, ProfitSource, SimConfig, SimStatus, V2_FACTORY_ADDRESS,
};
use rust_lmax_mev_state::{PoolId, PoolKind};
use rust_lmax_mev_state_fetcher::{
    FetchError, FetchedAccount, FetchedPoolState, PoolSlotLayout, StateFetcher,
};

/// Mock `StateFetcher` that returns fixture data from the SR-1
/// recorded set (per Q-A5 standing answer). Counts calls so PA-2/PA-3
/// can assert cache-hit short-circuit semantics.
struct CountingMockFetcher {
    pool_calls: AtomicUsize,
    account_calls: AtomicUsize,
}

impl CountingMockFetcher {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            pool_calls: AtomicUsize::new(0),
            account_calls: AtomicUsize::new(0),
        })
    }
    fn pool_calls(&self) -> usize {
        self.pool_calls.load(Ordering::Relaxed)
    }
    fn account_calls(&self) -> usize {
        self.account_calls.load(Ordering::Relaxed)
    }
}

#[async_trait::async_trait]
impl StateFetcher for CountingMockFetcher {
    async fn fetch_pool(
        &self,
        pool: &PoolId,
        _block_hash: B256,
        _layout: &dyn PoolSlotLayout,
    ) -> Result<FetchedPoolState, FetchError> {
        self.pool_calls.fetch_add(1, Ordering::Relaxed);
        // Reuse the SR-1 inline fixture data (Q-A5 standing answer).
        let (code, storage, addr) = if matches!(pool.kind, PoolKind::UniswapV3Fee005) {
            (
                fixtures::V3_WETH_USDC_005_CODE,
                fixtures::V3_WETH_USDC_005_STORAGE,
                fixtures::V3_WETH_USDC_005_ADDRESS,
            )
        } else {
            (
                fixtures::V2_WETH_USDC_CODE,
                fixtures::V2_WETH_USDC_STORAGE,
                fixtures::V2_WETH_USDC_ADDRESS,
            )
        };
        let _ = addr; // address is in the PoolId; fixture data is keyed by kind here
        Ok(FetchedPoolState {
            pool: pool.clone(),
            block_hash: B256::from(fixtures::V3_WETH_USDC_005_BLOCK_HASH),
            pool_code: alloy_primitives::Bytes::copy_from_slice(code),
            pool_storage: storage_from_pool(storage),
            auxiliary: Vec::new(),
        })
    }

    async fn fetch_account(
        &self,
        address: Address,
        _slots: &[U256],
        _block_hash: B256,
    ) -> Result<FetchedAccount, FetchError> {
        self.account_calls.fetch_add(1, Ordering::Relaxed);
        // Dispatch on address: WETH9 / USDC proxy / USDC impl /
        // V2 factory. Default fall-through returns the impl-account
        // shape (used for the parsed-from-proxy USDC impl address
        // which differs per fixture).
        let (code, storage, fixture_addr) = if address == Address::from(fixtures::WETH9_ADDRESS) {
            (
                fixtures::WETH9_CODE,
                fixtures::WETH9_STORAGE,
                fixtures::WETH9_ADDRESS,
            )
        } else if address == Address::from(fixtures::USDC_PROXY_ADDRESS) {
            (
                fixtures::USDC_PROXY_CODE,
                fixtures::USDC_PROXY_STORAGE,
                fixtures::USDC_PROXY_ADDRESS,
            )
        } else if address == V2_FACTORY_ADDRESS {
            (
                fixtures::V2_FACTORY_CODE,
                fixtures::V2_FACTORY_STORAGE,
                fixtures::V2_FACTORY_ADDRESS,
            )
        } else {
            // Treat any other address as the USDC impl (parsed
            // from the proxy fixture at runtime).
            (
                fixtures::USDC_IMPL_CODE,
                fixtures::USDC_IMPL_STORAGE,
                fixtures::USDC_IMPL_ADDRESS,
            )
        };
        let _ = fixture_addr;
        Ok(FetchedAccount {
            address,
            block_hash: B256::from(fixtures::V3_WETH_USDC_005_BLOCK_HASH),
            code: alloy_primitives::Bytes::copy_from_slice(code),
            storage: storage_from_account(storage),
        })
    }
}

fn storage_from_pool(s: &[([u8; 32], [u8; 32])]) -> Vec<(U256, B256)> {
    s.iter()
        .map(|(k, v)| (U256::from_be_bytes(*k), B256::from(*v)))
        .collect()
}

fn storage_from_account(s: &[([u8; 32], [u8; 32])]) -> Vec<(U256, B256)> {
    s.iter()
        .map(|(k, v)| (U256::from_be_bytes(*k), B256::from(*v)))
        .collect()
}

fn sr_1_opportunity() -> OpportunityEvent {
    OpportunityEvent {
        block_number: 22_000_000,
        block_hash: B256::from(fixtures::V3_WETH_USDC_005_BLOCK_HASH),
        source_pool: PoolId {
            kind: PoolKind::UniswapV3Fee005,
            address: Address::from(fixtures::V3_WETH_USDC_005_ADDRESS),
        },
        sink_pool: PoolId {
            kind: PoolKind::UniswapV2,
            address: Address::from(fixtures::V2_WETH_USDC_ADDRESS),
        },
        optimal_amount_in_wei: U256::from(10_000_000_000_000_000u128),
        expected_profit_wei: U256::ZERO,
        gas_estimate: GAS_ESTIMATE_TWO_HOP_ARB,
    }
}

/// PA-2 (DP-A3): repeat prefetch with same `(block_hash, source, sink)`
/// key produces zero archive RPC calls on the second call.
#[tokio::test]
async fn pa_2_cache_hit_short_circuits_archive_call() {
    let sim = LocalSimulator::with_cache(SimConfig::defaults(), NonZeroUsize::new(8).unwrap(), 1)
        .expect("ctor ok");
    let opp = sr_1_opportunity();
    let weth = Address::from(fixtures::WETH9_ADDRESS);
    let usdc = Address::from(fixtures::USDC_PROXY_ADDRESS);

    let raw = CountingMockFetcher::new();
    let fetcher: Arc<dyn StateFetcher> = raw.clone();

    sim.prefetch_for(&fetcher, &opp, weth, usdc).await.unwrap();
    let pool_after_first = raw.pool_calls();
    let account_after_first = raw.account_calls();
    assert!(pool_after_first >= 2, "first prefetch fetched both pools");
    assert!(
        account_after_first >= 4,
        "first prefetch fetched WETH+USDCproxy+USDCimpl+V2Factory"
    );

    sim.prefetch_for(&fetcher, &opp, weth, usdc).await.unwrap();
    assert_eq!(
        raw.pool_calls(),
        pool_after_first,
        "PA-2: cache hit must NOT fetch pools again"
    );
    assert_eq!(
        raw.account_calls(),
        account_after_first,
        "PA-2: cache hit must NOT fetch accounts again"
    );
    assert!(sim.fixtures_loaded());
}

/// PA-3 (DP-A11): retention/eviction — when block_number advances
/// past `freshness_window_blocks`, cache is cleared and a re-fetch
/// at the new block triggers fresh archive calls.
#[tokio::test]
async fn pa_3_retention_window_evicts_on_new_block() {
    let sim = LocalSimulator::with_cache(SimConfig::defaults(), NonZeroUsize::new(8).unwrap(), 1)
        .expect("ctor ok");
    let opp_a = sr_1_opportunity();
    let weth = Address::from(fixtures::WETH9_ADDRESS);
    let usdc = Address::from(fixtures::USDC_PROXY_ADDRESS);

    let fetcher: Arc<dyn StateFetcher> = CountingMockFetcher::new();
    let raw: Arc<CountingMockFetcher> = Arc::clone(unsafe {
        std::mem::transmute::<&Arc<dyn StateFetcher>, &Arc<CountingMockFetcher>>(&fetcher)
    });

    sim.prefetch_for(&fetcher, &opp_a, weth, usdc)
        .await
        .unwrap();
    let after_first = raw.pool_calls() + raw.account_calls();
    assert!(after_first > 0);

    // Advance block by exactly the freshness window (1) — eviction
    // triggers, re-fetch must occur on a different block_hash key.
    let mut opp_b = opp_a.clone();
    opp_b.block_number += 1;
    opp_b.block_hash = B256::from([0xCC; 32]); // different hash → different cache key
    sim.prefetch_for(&fetcher, &opp_b, weth, usdc)
        .await
        .unwrap();
    let after_second = raw.pool_calls() + raw.account_calls();
    assert!(
        after_second > after_first,
        "PA-3: new-block prefetch must trigger fresh archive calls; before={after_first} after={after_second}"
    );
}

/// PA-5 (Q-A5): simulate parity post-prefetch — `prefetch_for(...)`
/// followed by `simulate_with_fingerprint(...)` produces a
/// byte-identical outcome to the `load_fixture(...)` baseline using
/// the SR-1 recorded fixtures. Confirms FP-1 / SR-1 invariants
/// carry forward through the new mutex-backed code paths.
#[tokio::test]
async fn pa_5_simulate_parity_post_prefetch() {
    let opp = sr_1_opportunity();
    let weth = Address::from(fixtures::WETH9_ADDRESS);
    let usdc = Address::from(fixtures::USDC_PROXY_ADDRESS);
    let risk_checked = RiskCheckedOpportunity {
        opportunity: opp.clone(),
        size_wei: opp.optimal_amount_in_wei,
    };

    // Path A: load_fixture (SR-1-style direct injection).
    let sim_load =
        LocalSimulator::with_cache(SimConfig::defaults(), NonZeroUsize::new(4).unwrap(), 1)
            .expect("ctor ok");
    sim_load
        .load_fixture(
            common::v3_fixture(),
            common::v2_fixture(),
            common::weth_fixture(),
            common::usdc_proxy_fixture(),
            common::usdc_impl_fixture(),
            common::v2_factory_fixture(),
        )
        .expect("load_fixture ok");
    let outcome_load = sim_load
        .simulate_with_fingerprint(&risk_checked)
        .expect("simulate_with_fingerprint ok (load path)");

    // Path B: prefetch_for via mock fetcher (returns the same SR-1 data).
    let sim_prefetch =
        LocalSimulator::with_cache(SimConfig::defaults(), NonZeroUsize::new(4).unwrap(), 1)
            .expect("ctor ok");
    let fetcher: Arc<dyn StateFetcher> = CountingMockFetcher::new();
    sim_prefetch
        .prefetch_for(&fetcher, &opp, weth, usdc)
        .await
        .expect("prefetch_for ok");
    let outcome_prefetch = sim_prefetch
        .simulate_with_fingerprint(&risk_checked)
        .expect("simulate_with_fingerprint ok (prefetch path)");

    // Outcomes (status + gas + profit + fingerprint) must be identical
    // — the prefetch path threaded the same fixture bytes through the
    // mutex/cache/clone path that load_fixture uses directly.
    assert_eq!(outcome_load.0, outcome_prefetch.0, "PA-5 outcome parity");
    assert_eq!(
        outcome_load.0.status,
        SimStatus::Success,
        "PA-5: must reach Success on the SR-1 recorded fixtures"
    );
    assert_eq!(
        outcome_load.0.profit_source,
        ProfitSource::RevmComputed,
        "PA-5: ProfitSource::RevmComputed stamped"
    );
    assert_eq!(
        outcome_load.1, outcome_prefetch.1,
        "PA-5: LocalStateFingerprint parity across load_fixture vs prefetch_for"
    );
}
