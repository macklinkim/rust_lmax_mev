//! Phase 4 P4-C2 LocalSimulator real-revm pipeline.
//!
//! `LocalSimulator` runs real Uniswap V2 / V3 0.05% swap calldata
//! against recorded mainnet bytecode + storage loaded into a
//! `StrictMissingDb`-wrapped `CacheDB`. `simulate(&risk_checked)`
//! executes a 2-hop atomic arb (swap-1 sells WETH at the expensive
//! `sink_pool`; swap-2 buys WETH at the cheap `source_pool`) via the
//! mock router + measures the router's WETH balance delta. The
//! returned `SimulationOutcome` stamps `ProfitSource::RevmComputed` on
//! every successful path. Per the user-approved P4-C v0.3 plan +
//! 2026-05-09 P4-C1/C2 split amendment.
//!
//! Fixture loading:
//! - `LocalSimulator::new(cfg)` — sync constructor; no fixtures
//!   loaded. `simulate` returns `SimulationError::Setup` until
//!   `load_fixture` (test) or `prefetch_for` (production async) is
//!   called.
//! - `load_fixture(...)` — test path; takes pre-recorded
//!   `FetchedPoolState`s + `FetchedAccount`s.
//! - `prefetch_for(&fetcher, opportunity)` — production async path;
//!   calls `StateFetcher::fetch_pool` + `fetch_account` for the
//!   opportunity's source/sink + WETH9 + USDC proxy + USDC impl.
//!   NOT wired into `wire_phase4` per plan v0.3 §DP-C14 — runtime
//!   integration is P4-G's job.
//!
//! No relay sim, no submission, no live mainnet, no production key material.
//! Mock router holds NO key, signs NO tx, is invoked by a code-less
//! test EOA caller.

pub mod cache_db_builder;
pub mod fixtures;
pub mod mock_router;
pub mod observation;
pub mod reconcile;
pub mod recording_db;
pub mod rkyv_compat;
pub mod strict_db;
pub mod swap_calldata;

pub use observation::{LocalStateFingerprint, StateObservation};
pub use recording_db::RecordingDb;

use std::sync::Arc;

use alloy_primitives::{Address, Bytes, B256, I256, U256};
use revm::primitives::{
    AccountInfo, Bytes as RevmBytes, ExecutionResult, HaltReason, TxKind, KECCAK_EMPTY,
    U256 as RevmU256,
};
use revm::{Database, DatabaseCommit, Evm};
use rust_lmax_mev_opportunity::OpportunityEvent;
use rust_lmax_mev_risk::RiskCheckedOpportunity;
use rust_lmax_mev_state::{PoolId, PoolKind};
use rust_lmax_mev_state_fetcher::storage_key::{address_key, mapping_slot_u256};
use rust_lmax_mev_state_fetcher::uniswap::{UniswapV2Layout, UniswapV3Fee005Layout};
use rust_lmax_mev_state_fetcher::{FetchError, FetchedAccount, FetchedPoolState, StateFetcher};
use serde::{Deserialize, Serialize};

use cache_db_builder::{build_prepared, AuxiliaryAccounts};
use mock_router::{
    MOCK_ROUTER_ADDRESS, MOCK_ROUTER_RUNTIME, SELECTOR_EXEC_V2_SWAP, SELECTOR_EXEC_V3_SWAP,
};
use swap_calldata::{max_sqrt_ratio_minus_one, min_sqrt_ratio_plus_one, uniswap_v2_get_amount_out};

/// Default gas limit per simulation: 30_000_000 (mainnet block-gas-limit
/// scale; conservative upper bound for any single-bundle local sim).
pub const DEFAULT_GAS_LIMIT_PER_SIM: u64 = 30_000_000;

/// Default base fee: 30 gwei.
pub const DEFAULT_BASE_FEE_WEI_U128: u128 = 30_000_000_000;

/// Default EOA initial balance: 100 ETH.
pub const DEFAULT_EOA_INITIAL_BALANCE_WEI_U128: u128 = 100_000_000_000_000_000_000;

/// WETH/USDC token ordering (canonical address-ascending): token0=USDC,
/// token1=WETH. Used by both V2 and V3 swap calldata builders to pick
/// `(amount0Out, amount1Out)` and `zeroForOne`.
pub const USDC_IS_TOKEN0: bool = true;

/// WETH9 `balances` mapping declaration slot.
pub const WETH9_BALANCES_SLOT: u64 = 3;
/// FiatTokenV2_2 `balances` mapping declaration slot.
pub const USDC_BALANCES_SLOT: u64 = 9;
/// V2 packed reserves slot.
pub const V2_RESERVES_SLOT: u64 = 8;

/// Code-less test caller. The mock router has bytecode, so it can NOT
/// be the outer `tx.caller` (revm EIP-3607 `RejectCallerWithCode`).
/// This EOA calls `MockRouter::execV{2,3}Swap`, which then calls the
/// pool with `msg.sender = router` (so V3 callbacks return to the
/// router's bytecode).
pub const TEST_EOA_CALLER: Address = Address::new([0xee; 20]);

/// Canonical mainnet UniswapV2Factory address. Used by `prefetch_for`
/// to fetch the factory account fixture; mirrored in
/// `dump_fixture.rs`.
pub const V2_FACTORY_ADDRESS: Address = Address::new([
    0x5c, 0x69, 0xbe, 0xe7, 0x01, 0xef, 0x81, 0x4a, 0x2b, 0x6a, 0x3e, 0xdd, 0x4b, 0x16, 0x52, 0xcb,
    0x9c, 0xc5, 0xaa, 0x6f,
]);

/// Deterministic LOCAL simulation configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimConfig {
    pub chain_id: u64,
    pub gas_limit_per_sim: u64,
    pub base_fee_wei: U256,
    pub eoa_initial_balance_wei: U256,
}

impl SimConfig {
    /// One-line spec-defaults constructor (chain_id=1 per ADR-002).
    pub fn defaults() -> Self {
        Self {
            chain_id: 1,
            gas_limit_per_sim: DEFAULT_GAS_LIMIT_PER_SIM,
            base_fee_wei: U256::from(DEFAULT_BASE_FEE_WEI_U128),
            eoa_initial_balance_wei: U256::from(DEFAULT_EOA_INITIAL_BALANCE_WEI_U128),
        }
    }
}

/// Provenance of `SimulationOutcome.simulated_profit_wei`. P4-C2 emits
/// `RevmComputed` on every outcome (real revm-measured WETH delta).
/// `HeuristicPassthrough` is retained for rkyv archive forward-compat
/// with the P3-E shim era; never emitted by the P4-C2 code path.
/// (Marked `#[allow(deprecated)]` on the enum because the rkyv
/// `Archive` derive auto-references both variants; the P3-E variant is
/// soft-deprecated by documentation only.)
#[allow(deprecated)]
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum ProfitSource {
    /// P3-E DP-S1 era variant — never emitted by P4-C2; retained for
    /// rkyv archive forward-compat only.
    HeuristicPassthrough,
    /// `simulated_profit_wei` is the real revm-computed
    /// `router_weth_post - router_weth_pre` delta (saturating-sub at
    /// zero on loss).
    RevmComputed,
}

/// Discrete simulation outcome status.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum SimStatus {
    Success,
    Reverted { reason_hex: String },
    OutOfGas,
    HaltedOther { reason: String },
}

/// Output of one local pre-sim. Per `event-model.md` derives the
/// spec-mandated `Clone, Debug, PartialEq, Eq, rkyv::*, serde::*`.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct SimulationOutcome {
    pub opportunity_block_number: u64,
    pub gas_used: u64,
    pub status: SimStatus,
    #[rkyv(with = crate::rkyv_compat::U256AsBytes)]
    pub simulated_profit_wei: U256,
    pub profit_source: ProfitSource,
}

/// Setup / execution failures.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum SimulationError {
    #[error("invalid SimConfig: {0}")]
    Setup(String),
    #[error("revm execution failed: {0}")]
    Execution(String),
    #[error("state fetch failed: {0}")]
    Fetch(#[from] FetchError),
}

/// Recorded fixture set for one cross-venue arb opportunity at one
/// pinned block. Loaded into `LocalSimulator` before `simulate`.
#[derive(Debug, Clone)]
pub struct FixtureSet {
    pub block_hash: B256,
    pub source: FetchedPoolState,
    pub sink: FetchedPoolState,
    pub weth: FetchedAccount,
    pub usdc_proxy: FetchedAccount,
    pub usdc_impl: FetchedAccount,
    /// UniswapV2Factory account. Required so V2 swap's `_mintFee`
    /// external CALL `IUniswapV2Factory(factory).feeTo()` resolves
    /// cleanly under StrictMissingDb. Carries factory code + slot 0
    /// (`feeTo` address). Always required even if neither pool is V2
    /// — the cost of always-loading is one extra account; the cost
    /// of conditionally loading is significant complexity.
    pub v2_factory: FetchedAccount,
}

/// LOCAL real-revm pre-sim engine.
///
/// Phase 5 P5-A interior-mutability redesign per execution-note v0.3
/// §DP-A1 + §R-A2: `fixtures` moved behind `parking_lot::Mutex` so
/// the existing `Arc<LocalSimulator>` shared from `wire_phase4` can
/// drive `prefetch_for(...)` (which now takes `&self`) per inbound
/// `RiskCheckedOpportunity`. The mutex protects both the active
/// fixture slot AND the per-block LRU cache (DP-A3 cache-hit
/// semantics). simulate paths CLONE the active fixture out of the
/// lock, drop the lock, then run revm.
pub struct LocalSimulator {
    cfg: SimConfig,
    state: parking_lot::Mutex<SimulatorState>,
}

struct SimulatorState {
    active: Option<FixtureSet>,
    cache: lru::LruCache<FixtureKey, FixtureSet>,
    freshness_window_blocks: u64,
    last_block_seen: Option<u64>,
}

/// Cache key per DP-A3 / Q-A2: `(block_hash, source_pool, sink_pool)`
/// only — `optimal_amount_in_wei` is excluded because the fixture
/// set depends on chain state at `block_hash` for the two pools, not
/// on the input size (probe-size variance affects swap calldata, not
/// the fixture).
#[derive(Hash, PartialEq, Eq, Clone, Debug)]
struct FixtureKey {
    block_hash: B256,
    source_pool: Address,
    sink_pool: Address,
}

impl FixtureKey {
    fn from_fixture(f: &FixtureSet) -> Self {
        Self {
            block_hash: f.block_hash,
            source_pool: f.source.pool.address,
            sink_pool: f.sink.pool.address,
        }
    }
}

impl std::fmt::Debug for LocalSimulator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self.state.lock();
        f.debug_struct("LocalSimulator")
            .field("cfg", &self.cfg)
            .field("fixtures_loaded", &s.active.is_some())
            .field("cache_len", &s.cache.len())
            .finish_non_exhaustive()
    }
}

impl LocalSimulator {
    fn validate_cfg(cfg: &SimConfig) -> Result<(), SimulationError> {
        if cfg.chain_id == 0 {
            return Err(SimulationError::Setup(
                "chain_id must be non-zero".to_string(),
            ));
        }
        if cfg.gas_limit_per_sim == 0 {
            return Err(SimulationError::Setup(
                "gas_limit_per_sim must be non-zero".to_string(),
            ));
        }
        if cfg.eoa_initial_balance_wei.is_zero() {
            return Err(SimulationError::Setup(
                "eoa_initial_balance_wei must be non-zero".to_string(),
            ));
        }
        Ok(())
    }

    /// Validates `SimConfig` and returns a sim engine with NO fixtures
    /// loaded and a minimal cache (capacity 1, retention window 1 —
    /// effectively no cache benefit; equivalent to the pre-P5-A
    /// behavior). Existing test callers using `LocalSimulator::new`
    /// keep working unchanged. Production wiring uses [`with_cache`].
    pub fn new(cfg: SimConfig) -> Result<Self, SimulationError> {
        // Per Q-A1 standing answer: separate `with_cache` ctor; `new`
        // is the lean test-friendly ctor with capacity 1.
        Self::with_cache(cfg, std::num::NonZeroUsize::new(1).expect("1 > 0"), 1)
    }

    /// Production constructor (P5-A) — explicit cache capacity +
    /// retention window per DP-A8 + DP-A11. Capacity is
    /// `NonZeroUsize` so a `0` is statically impossible; the wire-
    /// shape `usize` in `SimulatorConfig` is validated to `> 0` by
    /// `Config::validate` BEFORE this ctor is called.
    pub fn with_cache(
        cfg: SimConfig,
        cache_capacity: std::num::NonZeroUsize,
        freshness_window_blocks: u64,
    ) -> Result<Self, SimulationError> {
        Self::validate_cfg(&cfg)?;
        Ok(Self {
            cfg,
            state: parking_lot::Mutex::new(SimulatorState {
                active: None,
                cache: lru::LruCache::new(cache_capacity),
                freshness_window_blocks,
                last_block_seen: None,
            }),
        })
    }

    pub fn cfg(&self) -> &SimConfig {
        &self.cfg
    }

    /// P5-A R-A2: cheap loaded-check; locks briefly to read the
    /// active slot's `is_some()`. Replaces the v0.4 `fixtures(&self)
    /// -> Option<&FixtureSet>` API which was structurally impossible
    /// behind a `Mutex` (cannot lend a borrow across the lock guard).
    pub fn fixtures_loaded(&self) -> bool {
        self.state.lock().active.is_some()
    }

    /// P5-A R-A2: clone-out snapshot of the active fixture slot.
    /// Returns `None` if no fixture is loaded.
    pub fn fixtures_snapshot(&self) -> Option<FixtureSet> {
        self.state.lock().active.clone()
    }

    /// Test path. Loads pre-recorded fixtures into the engine. All
    /// inputs must share `block_hash`; otherwise returns
    /// `SimulationError::Setup`.
    ///
    /// P5-A: changed from `&mut self` to `&self` per DP-A1 +
    /// interior mutability.
    pub fn load_fixture(
        &self,
        source: FetchedPoolState,
        sink: FetchedPoolState,
        weth: FetchedAccount,
        usdc_proxy: FetchedAccount,
        usdc_impl: FetchedAccount,
        v2_factory: FetchedAccount,
    ) -> Result<(), SimulationError> {
        let bh = source.block_hash;
        for (name, hash) in [
            ("sink", sink.block_hash),
            ("weth", weth.block_hash),
            ("usdc_proxy", usdc_proxy.block_hash),
            ("usdc_impl", usdc_impl.block_hash),
            ("v2_factory", v2_factory.block_hash),
        ] {
            if hash != bh {
                return Err(SimulationError::Setup(format!(
                    "{name} fixture block_hash {hash:?} != source block_hash {bh:?}",
                )));
            }
        }
        let fix = FixtureSet {
            block_hash: bh,
            source,
            sink,
            weth,
            usdc_proxy,
            usdc_impl,
            v2_factory,
        };
        let key = FixtureKey::from_fixture(&fix);
        let mut s = self.state.lock();
        s.cache.put(key, fix.clone());
        s.active = Some(fix);
        Ok(())
    }

    /// Production async path (P5-A wired into `simulator_driver` when
    /// `simulator.prefetch_enabled = true` in config).
    ///
    /// Per DP-A3 (cache-hit semantics): three cases under per-call
    /// mutex acquisitions:
    /// 1. Cache hit + active slot already matches the same key → no-op.
    /// 2. Cache hit + active slot does NOT match → clone cached
    ///    `FixtureSet` into active slot inside the critical section.
    /// 3. Cache miss → drop the lock, fetch via `StateFetcher` (async
    ///    I/O), re-acquire the lock + insert + clone-load.
    ///
    /// Per DP-A11 (retention-only semantics): cache key includes
    /// `block_hash`, so a different block produces a different key by
    /// construction — the cache CANNOT serve stale state under any
    /// `freshness_window_blocks` value. When `block_number` advances
    /// past the configured window, the cache is fully cleared as a
    /// retention/eviction policy.
    ///
    /// Changed from `&mut self` to `&self` per DP-A1 + R-A2.
    pub async fn prefetch_for(
        &self,
        fetcher: &Arc<dyn StateFetcher>,
        opp: &OpportunityEvent,
        weth_address: Address,
        usdc_proxy_address: Address,
    ) -> Result<(), SimulationError> {
        let key = FixtureKey {
            block_hash: opp.block_hash,
            source_pool: opp.source_pool.address,
            sink_pool: opp.sink_pool.address,
        };

        // Phase 1: cache lookup + retention/eviction under lock.
        {
            let mut s = self.state.lock();
            // DP-A11 retention/eviction: when the block_number
            // advances past the window, evict the entire cache.
            // (Keying by block_hash means we can never serve stale
            // state; this is purely an LRU-size policy.)
            if let Some(last) = s.last_block_seen {
                if opp.block_number > last
                    && opp.block_number.saturating_sub(last) >= s.freshness_window_blocks
                {
                    s.cache.clear();
                }
            }
            s.last_block_seen = Some(
                s.last_block_seen
                    .map_or(opp.block_number, |last| last.max(opp.block_number)),
            );

            // Cache hit?
            let cached = s.cache.get(&key).cloned();
            if let Some(fix) = cached {
                let active_matches =
                    s.active.as_ref().map(FixtureKey::from_fixture) == Some(key.clone());
                if !active_matches {
                    // Case 2: clone-load cache → active.
                    s.active = Some(fix);
                }
                // Case 1 (active matches) OR Case 2 (just loaded) →
                // return Ok.
                return Ok(());
            }
            // Cache miss → drop lock + fall through to fetch.
        }

        // Phase 2: async fetch out of the lock.
        let source = layout_dispatch(fetcher, &opp.source_pool, opp.block_hash).await?;
        let sink = layout_dispatch(fetcher, &opp.sink_pool, opp.block_hash).await?;
        let weth_slots = vec![
            mapping_slot_u256(
                U256::from(WETH9_BALANCES_SLOT),
                address_key(MOCK_ROUTER_ADDRESS),
            ),
            mapping_slot_u256(
                U256::from(WETH9_BALANCES_SLOT),
                address_key(opp.source_pool.address),
            ),
            mapping_slot_u256(
                U256::from(WETH9_BALANCES_SLOT),
                address_key(opp.sink_pool.address),
            ),
        ];
        let weth = fetcher
            .fetch_account(weth_address, &weth_slots, opp.block_hash)
            .await?;
        let usdc_slots = build_usdc_proxy_slots(&opp.source_pool.address, &opp.sink_pool.address);
        let usdc_proxy = fetcher
            .fetch_account(usdc_proxy_address, &usdc_slots, opp.block_hash)
            .await?;
        let impl_addr = parse_usdc_impl_address(&usdc_proxy).ok_or_else(|| {
            SimulationError::Setup(
                "USDC proxy impl slot is zero in both EIP-1967 and ZeppelinOS layouts".into(),
            )
        })?;
        let usdc_impl = fetcher
            .fetch_account(impl_addr, &[], opp.block_hash)
            .await?;
        let v2_factory = fetcher
            .fetch_account(V2_FACTORY_ADDRESS, &[U256::from(0u64)], opp.block_hash)
            .await?;

        let fix = FixtureSet {
            block_hash: opp.block_hash,
            source,
            sink,
            weth,
            usdc_proxy,
            usdc_impl,
            v2_factory,
        };

        // Phase 3: re-acquire lock + insert into cache + clone-load
        // into active slot.
        let mut s = self.state.lock();
        s.cache.put(key, fix.clone());
        s.active = Some(fix);
        Ok(())
    }

    /// Runs the real-revm pipeline for the given checked opportunity.
    /// Returns `SimulationError::Setup` if no fixture is loaded or if
    /// the opportunity's pool addresses don't match the loaded
    /// fixture's source/sink. Same input → byte-identical
    /// `SimulationOutcome`.
    pub fn simulate(
        &self,
        risk_checked: &RiskCheckedOpportunity,
    ) -> Result<SimulationOutcome, SimulationError> {
        let opp = &risk_checked.opportunity;
        let fixtures = self.state.lock().active.clone().ok_or_else(|| {
            SimulationError::Setup(
                "no fixtures loaded; call load_fixture or prefetch_for first".into(),
            )
        })?;

        if fixtures.block_hash != opp.block_hash {
            return Err(SimulationError::Setup(format!(
                "fixture block_hash {:?} != opportunity block_hash {:?}",
                fixtures.block_hash, opp.block_hash
            )));
        }
        if fixtures.source.pool != opp.source_pool {
            return Err(SimulationError::Setup(format!(
                "loaded source pool {:?} != opportunity source_pool {:?}",
                fixtures.source.pool, opp.source_pool
            )));
        }
        if fixtures.sink.pool != opp.sink_pool {
            return Err(SimulationError::Setup(format!(
                "loaded sink pool {:?} != opportunity sink_pool {:?}",
                fixtures.sink.pool, opp.sink_pool
            )));
        }

        let weth_address = fixtures.weth.address;
        let usdc_proxy_address = fixtures.usdc_proxy.address;

        // Pre-fund router WETH balance: insert/override the
        // balanceOf[router] slot in the WETH fixture.
        let router_weth_balance_slot = mapping_slot_u256(
            U256::from(WETH9_BALANCES_SLOT),
            address_key(MOCK_ROUTER_ADDRESS),
        );
        let mut weth_fixture = fixtures.weth.clone();
        let prefund_value = B256::from(opp.optimal_amount_in_wei.to_be_bytes::<32>());
        if let Some(entry) = weth_fixture
            .storage
            .iter_mut()
            .find(|(s, _)| *s == router_weth_balance_slot)
        {
            entry.1 = prefund_value;
        } else {
            weth_fixture
                .storage
                .push((router_weth_balance_slot, prefund_value));
        }

        let extra_accounts = [
            weth_fixture,
            fixtures.usdc_proxy.clone(),
            fixtures.usdc_impl.clone(),
            fixtures.v2_factory.clone(),
        ];

        let aux = AuxiliaryAccounts {
            mock_router_address: MOCK_ROUTER_ADDRESS,
            mock_router_bytecode: Bytes::from_static(MOCK_ROUTER_RUNTIME),
            mock_router_eth_balance_wei: self.cfg.eoa_initial_balance_wei,
            mock_router_weth_balance_wei: opp.optimal_amount_in_wei,
        };

        let prepared = build_prepared(
            &fixtures.source,
            &fixtures.sink,
            &extra_accounts,
            &aux,
            weth_address,
            router_weth_balance_slot,
        )?;
        let mut db = prepared.db;
        let pre_router_weth = prepared.pre_router_weth_wei;

        // Insert coinbase + EOA caller (both code-less).
        db.insert_account(Address::ZERO, code_less_account(U256::ZERO));
        db.insert_account(
            TEST_EOA_CALLER,
            code_less_account(self.cfg.eoa_initial_balance_wei),
        );

        // ---- Build calldata for swap-1 (sell WETH at sink) ----
        let weth_in = opp.optimal_amount_in_wei;
        let swap1_calldata = build_router_calldata_sell_weth(
            &fixtures.sink,
            weth_in,
            weth_address,
            usdc_proxy_address,
        )?;

        // ---- Execute swap-1 ----
        // block.timestamp = V2 pair's blockTimestampLast so
        // UniswapV2Pair._update's `if (timeElapsed > 0)` branch is
        // skipped (slots 9 + 10 are not read in that case). The V2
        // fixture is one of source/sink — pick whichever is V2; if
        // both are V3, fall back to opp.block_number for timestamp
        // (V3 swap doesn't read blockTimestampLast in the same way).
        let block_timestamp = v2_block_timestamp_last(&fixtures.source)
            .or_else(|| v2_block_timestamp_last(&fixtures.sink));
        let mut evm = build_evm(db, &self.cfg, opp.block_number, block_timestamp);
        let (swap1_status, swap1_gas) = exec_router_call(&mut evm, swap1_calldata, 0)?;
        if !matches!(swap1_status, SimStatus::Success) {
            return Ok(SimulationOutcome {
                opportunity_block_number: opp.block_number,
                gas_used: swap1_gas,
                status: swap1_status,
                simulated_profit_wei: U256::ZERO,
                profit_source: ProfitSource::RevmComputed,
            });
        }

        // Read intermediate USDC balance for swap-2 input. The slot
        // is populated by T-USDC-1's recorded fixture (router slot
        // pre-seeded to 0) + the swap-1 commit (which writes the
        // received USDC to the same slot via the proxy's storage
        // context). Either way StrictMissingDb sees it as populated.
        let router_usdc_balance_slot = mapping_slot_u256(
            U256::from(USDC_BALANCES_SLOT),
            address_key(MOCK_ROUTER_ADDRESS),
        );
        let usdc_in = match evm
            .context
            .evm
            .db
            .storage(usdc_proxy_address, router_usdc_balance_slot)
        {
            Ok(v) => v,
            Err(e) => {
                return Ok(SimulationOutcome {
                    opportunity_block_number: opp.block_number,
                    gas_used: swap1_gas,
                    status: SimStatus::Reverted {
                        reason_hex: format!("read USDC balance after swap-1: {e:?}"),
                    },
                    simulated_profit_wei: U256::ZERO,
                    profit_source: ProfitSource::RevmComputed,
                });
            }
        };

        // ---- Build calldata for swap-2 (buy WETH at source) ----
        let swap2_calldata = build_router_calldata_buy_weth(
            &fixtures.source,
            usdc_in,
            weth_address,
            usdc_proxy_address,
        )?;

        // ---- Execute swap-2 ----
        let (swap2_status, swap2_gas) = exec_router_call(&mut evm, swap2_calldata, 1)?;
        if !matches!(swap2_status, SimStatus::Success) {
            return Ok(SimulationOutcome {
                opportunity_block_number: opp.block_number,
                gas_used: swap1_gas.saturating_add(swap2_gas),
                status: swap2_status,
                simulated_profit_wei: U256::ZERO,
                profit_source: ProfitSource::RevmComputed,
            });
        }

        // ---- Measure post-swap router WETH balance ----
        let post_router_weth = match evm
            .context
            .evm
            .db
            .storage(weth_address, router_weth_balance_slot)
        {
            Ok(v) => v,
            Err(e) => {
                return Ok(SimulationOutcome {
                    opportunity_block_number: opp.block_number,
                    gas_used: swap1_gas.saturating_add(swap2_gas),
                    status: SimStatus::Reverted {
                        reason_hex: format!("read post WETH balance: {e:?}"),
                    },
                    simulated_profit_wei: U256::ZERO,
                    profit_source: ProfitSource::RevmComputed,
                });
            }
        };
        let profit = post_router_weth.saturating_sub(pre_router_weth);

        Ok(SimulationOutcome {
            opportunity_block_number: opp.block_number,
            gas_used: swap1_gas.saturating_add(swap2_gas),
            status: SimStatus::Success,
            simulated_profit_wei: profit,
            profit_source: ProfitSource::RevmComputed,
        })
    }

    /// Phase 4 P4-D peer to `simulate(...)` that additionally records
    /// the storage read-set into a `LocalStateFingerprint`. Same
    /// inputs, same execution path, same `SimulationOutcome` (FP-1
    /// parity invariant); the only difference is the database is
    /// wrapped in `RecordingDb<StrictMissingDb>` so every successful
    /// `Database::storage` call is captured. The wrapper lives
    /// entirely on the call's stack frame — `LocalSimulator` does
    /// NOT gain interior state.
    ///
    /// Used by the (deferred-to-P4-E/G) relay-sim comparator to
    /// detect `MismatchCategory::StateDependency` divergences against
    /// the relay's own observation set. Per the P4-D execution note
    /// v0.4 §DP-D9'/§DP-D16/§R8/§R13.
    pub fn simulate_with_fingerprint(
        &self,
        risk_checked: &RiskCheckedOpportunity,
    ) -> Result<(SimulationOutcome, LocalStateFingerprint), SimulationError> {
        let opp = &risk_checked.opportunity;
        let fixtures = self.state.lock().active.clone().ok_or_else(|| {
            SimulationError::Setup(
                "no fixtures loaded; call load_fixture or prefetch_for first".into(),
            )
        })?;

        if fixtures.block_hash != opp.block_hash {
            return Err(SimulationError::Setup(format!(
                "fixture block_hash {:?} != opportunity block_hash {:?}",
                fixtures.block_hash, opp.block_hash
            )));
        }
        if fixtures.source.pool != opp.source_pool {
            return Err(SimulationError::Setup(format!(
                "loaded source pool {:?} != opportunity source_pool {:?}",
                fixtures.source.pool, opp.source_pool
            )));
        }
        if fixtures.sink.pool != opp.sink_pool {
            return Err(SimulationError::Setup(format!(
                "loaded sink pool {:?} != opportunity sink_pool {:?}",
                fixtures.sink.pool, opp.sink_pool
            )));
        }

        let block_hash = fixtures.block_hash;
        let weth_address = fixtures.weth.address;
        let usdc_proxy_address = fixtures.usdc_proxy.address;

        let router_weth_balance_slot = mapping_slot_u256(
            U256::from(WETH9_BALANCES_SLOT),
            address_key(MOCK_ROUTER_ADDRESS),
        );
        let mut weth_fixture = fixtures.weth.clone();
        let prefund_value = B256::from(opp.optimal_amount_in_wei.to_be_bytes::<32>());
        if let Some(entry) = weth_fixture
            .storage
            .iter_mut()
            .find(|(s, _)| *s == router_weth_balance_slot)
        {
            entry.1 = prefund_value;
        } else {
            weth_fixture
                .storage
                .push((router_weth_balance_slot, prefund_value));
        }

        let extra_accounts = [
            weth_fixture,
            fixtures.usdc_proxy.clone(),
            fixtures.usdc_impl.clone(),
            fixtures.v2_factory.clone(),
        ];

        let aux = AuxiliaryAccounts {
            mock_router_address: MOCK_ROUTER_ADDRESS,
            mock_router_bytecode: Bytes::from_static(MOCK_ROUTER_RUNTIME),
            mock_router_eth_balance_wei: self.cfg.eoa_initial_balance_wei,
            mock_router_weth_balance_wei: opp.optimal_amount_in_wei,
        };

        let prepared = build_prepared(
            &fixtures.source,
            &fixtures.sink,
            &extra_accounts,
            &aux,
            weth_address,
            router_weth_balance_slot,
        )?;
        let mut inner = prepared.db;
        let pre_router_weth = prepared.pre_router_weth_wei;

        inner.insert_account(Address::ZERO, code_less_account(U256::ZERO));
        inner.insert_account(
            TEST_EOA_CALLER,
            code_less_account(self.cfg.eoa_initial_balance_wei),
        );

        // Wrap in RecordingDb. From here on every Database::storage
        // call is captured; basic/code/block_hash are pass-through.
        let db = RecordingDb::new(inner);

        let weth_in = opp.optimal_amount_in_wei;
        let swap1_calldata = build_router_calldata_sell_weth(
            &fixtures.sink,
            weth_in,
            weth_address,
            usdc_proxy_address,
        )?;

        let block_timestamp = v2_block_timestamp_last(&fixtures.source)
            .or_else(|| v2_block_timestamp_last(&fixtures.sink));
        let mut evm = build_evm(db, &self.cfg, opp.block_number, block_timestamp);
        let (swap1_status, swap1_gas) = exec_router_call(&mut evm, swap1_calldata, 0)?;
        if !matches!(swap1_status, SimStatus::Success) {
            let observations = evm.context.evm.db.observations();
            return Ok((
                SimulationOutcome {
                    opportunity_block_number: opp.block_number,
                    gas_used: swap1_gas,
                    status: swap1_status,
                    simulated_profit_wei: U256::ZERO,
                    profit_source: ProfitSource::RevmComputed,
                },
                LocalStateFingerprint {
                    block_hash,
                    observations,
                },
            ));
        }

        let router_usdc_balance_slot = mapping_slot_u256(
            U256::from(USDC_BALANCES_SLOT),
            address_key(MOCK_ROUTER_ADDRESS),
        );
        let usdc_in = match evm
            .context
            .evm
            .db
            .storage(usdc_proxy_address, router_usdc_balance_slot)
        {
            Ok(v) => v,
            Err(e) => {
                let observations = evm.context.evm.db.observations();
                return Ok((
                    SimulationOutcome {
                        opportunity_block_number: opp.block_number,
                        gas_used: swap1_gas,
                        status: SimStatus::Reverted {
                            reason_hex: format!("read USDC balance after swap-1: {e:?}"),
                        },
                        simulated_profit_wei: U256::ZERO,
                        profit_source: ProfitSource::RevmComputed,
                    },
                    LocalStateFingerprint {
                        block_hash,
                        observations,
                    },
                ));
            }
        };

        let swap2_calldata = build_router_calldata_buy_weth(
            &fixtures.source,
            usdc_in,
            weth_address,
            usdc_proxy_address,
        )?;

        let (swap2_status, swap2_gas) = exec_router_call(&mut evm, swap2_calldata, 1)?;
        if !matches!(swap2_status, SimStatus::Success) {
            let observations = evm.context.evm.db.observations();
            return Ok((
                SimulationOutcome {
                    opportunity_block_number: opp.block_number,
                    gas_used: swap1_gas.saturating_add(swap2_gas),
                    status: swap2_status,
                    simulated_profit_wei: U256::ZERO,
                    profit_source: ProfitSource::RevmComputed,
                },
                LocalStateFingerprint {
                    block_hash,
                    observations,
                },
            ));
        }

        let post_router_weth = match evm
            .context
            .evm
            .db
            .storage(weth_address, router_weth_balance_slot)
        {
            Ok(v) => v,
            Err(e) => {
                let observations = evm.context.evm.db.observations();
                return Ok((
                    SimulationOutcome {
                        opportunity_block_number: opp.block_number,
                        gas_used: swap1_gas.saturating_add(swap2_gas),
                        status: SimStatus::Reverted {
                            reason_hex: format!("read post WETH balance: {e:?}"),
                        },
                        simulated_profit_wei: U256::ZERO,
                        profit_source: ProfitSource::RevmComputed,
                    },
                    LocalStateFingerprint {
                        block_hash,
                        observations,
                    },
                ));
            }
        };
        let profit = post_router_weth.saturating_sub(pre_router_weth);
        let observations = evm.context.evm.db.observations();

        Ok((
            SimulationOutcome {
                opportunity_block_number: opp.block_number,
                gas_used: swap1_gas.saturating_add(swap2_gas),
                status: SimStatus::Success,
                simulated_profit_wei: profit,
                profit_source: ProfitSource::RevmComputed,
            },
            LocalStateFingerprint {
                block_hash,
                observations,
            },
        ))
    }
}

/// Maps revm's `HaltReason` to `SimStatus`. `HaltReason::OutOfGas(_)`
/// normalizes to `SimStatus::OutOfGas` per Codex 18:03:59 S-3
/// tightening; everything else falls into `HaltedOther`.
fn classify_halt(reason: HaltReason) -> SimStatus {
    match reason {
        HaltReason::OutOfGas(_) => SimStatus::OutOfGas,
        other => SimStatus::HaltedOther {
            reason: format!("{other:?}"),
        },
    }
}

fn hex_lower(bytes: &RevmBytes) -> String {
    let mut s = String::with_capacity(2 + bytes.len() * 2);
    s.push_str("0x");
    for b in bytes.iter() {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn code_less_account(balance: U256) -> AccountInfo {
    AccountInfo {
        balance,
        nonce: 0,
        code_hash: KECCAK_EMPTY,
        code: None,
    }
}

fn build_evm<'a, DB: Database>(
    db: DB,
    cfg: &SimConfig,
    block_number: u64,
    block_timestamp: Option<u32>,
) -> Evm<'a, (), DB> {
    let chain_id = cfg.chain_id;
    let gas_limit = cfg.gas_limit_per_sim;
    let base_fee = RevmU256::from_be_bytes(cfg.base_fee_wei.to_be_bytes::<32>());
    let timestamp = block_timestamp
        .map(|t| RevmU256::from(t as u64))
        .unwrap_or_else(|| RevmU256::from(block_number));
    Evm::builder()
        .with_db(db)
        .modify_cfg_env(|c| {
            c.chain_id = chain_id;
        })
        .modify_block_env(|b| {
            b.basefee = base_fee;
            b.gas_limit = RevmU256::from(gas_limit);
            b.number = RevmU256::from(block_number);
            b.timestamp = timestamp;
        })
        .modify_tx_env(|tx| {
            tx.caller = TEST_EOA_CALLER;
            tx.transact_to = TxKind::Call(MOCK_ROUTER_ADDRESS);
            tx.gas_limit = gas_limit;
            tx.gas_price = base_fee;
            tx.value = RevmU256::ZERO;
            tx.chain_id = Some(chain_id);
            tx.nonce = Some(0);
        })
        .build()
}

/// Execute one router call through `evm`. The tx fields (caller +
/// transact_to) are baked in by `build_evm`; only `tx.data` and
/// `tx.nonce` are mutated per-call.
fn exec_router_call<DB>(
    evm: &mut Evm<'_, (), DB>,
    calldata: Bytes,
    nonce: u64,
) -> Result<(SimStatus, u64), SimulationError>
where
    DB: Database + DatabaseCommit,
    <DB as Database>::Error: std::fmt::Debug,
{
    evm.tx_mut().data = RevmBytes::copy_from_slice(&calldata);
    evm.tx_mut().nonce = Some(nonce);

    let result = match evm.transact_commit() {
        Ok(r) => r,
        Err(e) => {
            // Per plan v0.3 §DP-C8: missing fixture state surfaces as
            // Reverted (visible in status), not as a typed
            // SimulationError::Execution. The reason carries the full
            // detail so the operator can extend dump_fixture and
            // re-record at the same block hash.
            let detail = format!("{e:?}");
            if detail.contains("MissingStorage") || detail.contains("MissingAccount") {
                return Ok((
                    SimStatus::Reverted {
                        reason_hex: format!("missing fixture state: {detail}"),
                    },
                    0,
                ));
            }
            return Err(SimulationError::Execution(detail));
        }
    };
    Ok(match result {
        ExecutionResult::Success { gas_used, .. } => (SimStatus::Success, gas_used),
        ExecutionResult::Revert { gas_used, output } => (
            SimStatus::Reverted {
                reason_hex: hex_lower(&output),
            },
            gas_used,
        ),
        ExecutionResult::Halt { reason, gas_used } => (classify_halt(reason), gas_used),
    })
}

/// Build calldata for `MockRouter::execV{2,3}Swap(...)` to sell
/// `weth_in` WETH at `pool` and receive USDC. WETH/USDC ordering:
/// USDC=token0, WETH=token1.
fn build_router_calldata_sell_weth(
    pool: &FetchedPoolState,
    weth_in: U256,
    weth_address: Address,
    _usdc_address: Address,
) -> Result<Bytes, SimulationError> {
    match pool.pool.kind {
        // P4-F: SushiswapV2 reuses the V2 swap calldata path
        // (Sushi V2 is a UniV2 fork — same getReserves() shape +
        // constant-product math).
        PoolKind::UniswapV2 | PoolKind::SushiswapV2 => {
            let (reserve0_usdc, reserve1_weth) = decode_v2_reserves(pool).ok_or_else(|| {
                SimulationError::Setup(format!(
                    "V2 pool {:?} reserves slot 8 not in fixture",
                    pool.pool.address
                ))
            })?;
            let usdc_out = uniswap_v2_get_amount_out(weth_in, reserve1_weth, reserve0_usdc);
            // amount0Out = USDC out, amount1Out = 0.
            Ok(build_exec_v2_swap_calldata(
                pool.pool.address,
                weth_address,
                weth_in,
                usdc_out,
                U256::ZERO,
            ))
        }
        PoolKind::UniswapV3Fee005 => {
            // Selling token1=WETH for token0=USDC → zeroForOne = false.
            let amount_specified = I256::try_from(weth_in)
                .map_err(|_| SimulationError::Setup("weth_in exceeds I256".into()))?;
            Ok(build_exec_v3_swap_calldata(
                pool.pool.address,
                /* zeroForOne */ false,
                amount_specified,
                max_sqrt_ratio_minus_one(),
                weth_address,
            ))
        }
    }
}

/// Build calldata for `MockRouter::execV{2,3}Swap(...)` to spend
/// `usdc_in` USDC at `pool` and receive WETH.
fn build_router_calldata_buy_weth(
    pool: &FetchedPoolState,
    usdc_in: U256,
    _weth_address: Address,
    usdc_address: Address,
) -> Result<Bytes, SimulationError> {
    match pool.pool.kind {
        // P4-F: SushiswapV2 reuses the V2 path (Sushi V2 is a UniV2 fork).
        PoolKind::UniswapV2 | PoolKind::SushiswapV2 => {
            let (reserve0_usdc, reserve1_weth) = decode_v2_reserves(pool).ok_or_else(|| {
                SimulationError::Setup(format!(
                    "V2 pool {:?} reserves slot 8 not in fixture",
                    pool.pool.address
                ))
            })?;
            let weth_out = uniswap_v2_get_amount_out(usdc_in, reserve0_usdc, reserve1_weth);
            // amount0Out = 0, amount1Out = WETH out.
            Ok(build_exec_v2_swap_calldata(
                pool.pool.address,
                usdc_address,
                usdc_in,
                U256::ZERO,
                weth_out,
            ))
        }
        PoolKind::UniswapV3Fee005 => {
            // Selling token0=USDC for token1=WETH → zeroForOne = true.
            let amount_specified = I256::try_from(usdc_in)
                .map_err(|_| SimulationError::Setup("usdc_in exceeds I256".into()))?;
            Ok(build_exec_v3_swap_calldata(
                pool.pool.address,
                /* zeroForOne */ true,
                amount_specified,
                min_sqrt_ratio_plus_one(),
                usdc_address,
            ))
        }
    }
}

fn build_exec_v2_swap_calldata(
    pool: Address,
    input_token: Address,
    amount_in: U256,
    amount0_out: U256,
    amount1_out: U256,
) -> Bytes {
    let mut buf = Vec::with_capacity(4 + 32 * 5);
    buf.extend_from_slice(&SELECTOR_EXEC_V2_SWAP);
    extend_address_padded(&mut buf, pool);
    extend_address_padded(&mut buf, input_token);
    extend_u256_be(&mut buf, amount_in);
    extend_u256_be(&mut buf, amount0_out);
    extend_u256_be(&mut buf, amount1_out);
    Bytes::from(buf)
}

fn build_exec_v3_swap_calldata(
    pool: Address,
    zero_for_one: bool,
    amount_specified: I256,
    sqrt_price_limit_x96: U256,
    input_token: Address,
) -> Bytes {
    let mut buf = Vec::with_capacity(4 + 32 * 5);
    buf.extend_from_slice(&SELECTOR_EXEC_V3_SWAP);
    extend_address_padded(&mut buf, pool);
    let mut bool_word = [0u8; 32];
    bool_word[31] = u8::from(zero_for_one);
    buf.extend_from_slice(&bool_word);
    buf.extend_from_slice(&amount_specified.to_be_bytes::<32>());
    extend_u256_be(&mut buf, sqrt_price_limit_x96);
    extend_address_padded(&mut buf, input_token);
    Bytes::from(buf)
}

fn extend_address_padded(buf: &mut Vec<u8>, a: Address) {
    buf.extend_from_slice(&[0u8; 12]);
    buf.extend_from_slice(a.as_slice());
}

fn extend_u256_be(buf: &mut Vec<u8>, v: U256) {
    buf.extend_from_slice(&v.to_be_bytes::<32>());
}

/// Parse `blockTimestampLast` (uint32) from V2's packed slot 8. Returns
/// `None` if the pool is not V2 or slot 8 isn't in the fixture.
fn v2_block_timestamp_last(state: &FetchedPoolState) -> Option<u32> {
    // P4-F: SushiV2 has the same packed reserves slot 8 (V2 fork).
    if !matches!(state.pool.kind, PoolKind::UniswapV2 | PoolKind::SushiswapV2) {
        return None;
    }
    let value = state
        .pool_storage
        .iter()
        .find(|(s, _)| *s == U256::from(V2_RESERVES_SLOT))
        .map(|(_, v)| v)?;
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&value.0[0..4]);
    Some(u32::from_be_bytes(buf))
}

/// Decode V2 packed reserves slot (`slot 8`):
/// `(uint112 reserve0 || uint112 reserve1 || uint32 blockTimestampLast)`
/// stored as 14+14+4 bytes BE. Returns `(reserve0_usdc, reserve1_weth)`.
fn decode_v2_reserves(state: &FetchedPoolState) -> Option<(U256, U256)> {
    let value = state
        .pool_storage
        .iter()
        .find(|(s, _)| *s == U256::from(V2_RESERVES_SLOT))
        .map(|(_, v)| v)?;
    // Layout (BE bytes, MSB first):
    //   bytes 0..4   = blockTimestampLast (uint32)
    //   bytes 4..18  = reserve1 (uint112, 14 bytes BE)
    //   bytes 18..32 = reserve0 (uint112, 14 bytes BE)
    let mut r1_buf = [0u8; 32];
    r1_buf[18..32].copy_from_slice(&value.0[4..18]);
    let reserve1_weth = U256::from_be_bytes(r1_buf);
    let mut r0_buf = [0u8; 32];
    r0_buf[18..32].copy_from_slice(&value.0[18..32]);
    let reserve0_usdc = U256::from_be_bytes(r0_buf);
    Some((reserve0_usdc, reserve1_weth))
}

async fn layout_dispatch(
    fetcher: &Arc<dyn StateFetcher>,
    pool: &PoolId,
    block_hash: B256,
) -> Result<FetchedPoolState, SimulationError> {
    let result = match pool.kind {
        // P4-F: SushiV2 reuses the V2 layout (Sushi V2 is a UniV2 fork).
        PoolKind::UniswapV2 | PoolKind::SushiswapV2 => {
            fetcher.fetch_pool(pool, block_hash, &UniswapV2Layout).await
        }
        PoolKind::UniswapV3Fee005 => {
            fetcher
                .fetch_pool(pool, block_hash, &UniswapV3Fee005Layout)
                .await
        }
    };
    Ok(result?)
}

/// EIP-1967 + ZeppelinOS slot keys used by `prefetch_for` to read the
/// USDC implementation address from the proxy. Defined here mirroring
/// `dump_fixture.rs` so the production prefetch path produces the same
/// slot list T-USDC-1 validated.
const EIP1967_IMPL_SLOT_BE: [u8; 32] = [
    0x36, 0x08, 0x94, 0xa1, 0x3b, 0xa1, 0xa3, 0x21, 0x06, 0x67, 0xc8, 0x28, 0x49, 0x2d, 0xb9, 0x8d,
    0xca, 0x3e, 0x20, 0x76, 0xcc, 0x37, 0x35, 0xa9, 0x20, 0xa3, 0xca, 0x50, 0x5d, 0x38, 0x2b, 0xbc,
];
const ZEPPELINOS_IMPL_SLOT_BE: [u8; 32] = [
    0x70, 0x50, 0xc9, 0xe0, 0xf4, 0xca, 0x76, 0x9c, 0x69, 0xbd, 0x3a, 0x8e, 0xf7, 0x40, 0xbc, 0x37,
    0x93, 0x4f, 0x8e, 0x2c, 0x03, 0x6e, 0x5a, 0x72, 0x3f, 0xd8, 0xee, 0x04, 0x8e, 0xd3, 0xf8, 0xc3,
];
const ZEPPELINOS_ADMIN_SLOT_BE: [u8; 32] = [
    0x10, 0xd6, 0xa5, 0x4a, 0x47, 0x54, 0xc8, 0x86, 0x9d, 0x68, 0x86, 0xb5, 0xf5, 0xd7, 0xfb, 0xfa,
    0x5b, 0x45, 0x22, 0x23, 0x7e, 0xa5, 0xc6, 0x0d, 0x11, 0xbc, 0x4e, 0x7a, 0x1f, 0xf9, 0x39, 0x0b,
];

fn build_usdc_proxy_slots(source_pool: &Address, sink_pool: &Address) -> Vec<U256> {
    let mut slots = vec![
        U256::from_be_bytes(EIP1967_IMPL_SLOT_BE),
        U256::from_be_bytes(ZEPPELINOS_IMPL_SLOT_BE),
        U256::from_be_bytes(ZEPPELINOS_ADMIN_SLOT_BE),
        mapping_slot_u256(
            U256::from(USDC_BALANCES_SLOT),
            address_key(MOCK_ROUTER_ADDRESS),
        ),
        mapping_slot_u256(U256::from(USDC_BALANCES_SLOT), address_key(*source_pool)),
        mapping_slot_u256(U256::from(USDC_BALANCES_SLOT), address_key(*sink_pool)),
    ];
    slots.extend((0u64..16).map(U256::from));
    slots
}

fn parse_usdc_impl_address(proxy: &FetchedAccount) -> Option<Address> {
    let read = |slot_be: [u8; 32]| -> Option<B256> {
        proxy
            .storage
            .iter()
            .find(|(s, _)| *s == U256::from_be_bytes(slot_be))
            .map(|(_, v)| *v)
    };
    let value = read(EIP1967_IMPL_SLOT_BE)
        .filter(|v| *v != B256::ZERO)
        .or_else(|| read(ZEPPELINOS_IMPL_SLOT_BE).filter(|v| *v != B256::ZERO))?;
    Some(Address::from_slice(&value.0[12..32]))
}
