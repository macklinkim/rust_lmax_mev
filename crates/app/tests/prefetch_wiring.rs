//! Phase 5 P5-A PA-1 / PA-4 wiring tests for `simulator_driver`'s
//! optional `prefetch_for(...)` dispatch.
//!
//! - PA-1: with `prefetch_fetcher = None` (the disabled-by-default
//!   path), the driver behaves exactly like P4-G — no archive call
//!   is attempted (no fetcher even exists), and `simulate_with_fingerprint`
//!   returns `Setup` (no fixtures loaded) → no `SimulationOutcomeWithFingerprint`
//!   envelope is emitted on `sim_tx`.
//! - PA-4: with `prefetch_fetcher = Some(_)` returning `Err(FetchError)`,
//!   the driver logs WARN + drops the event; subsequent events are
//!   not blocked (the loop continues).

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use alloy_primitives::{Address, B256, U256};
use rust_lmax_mev_app::{simulator_driver, SimulationOutcomeWithFingerprint};
use rust_lmax_mev_opportunity::{OpportunityEvent, GAS_ESTIMATE_TWO_HOP_ARB};
use rust_lmax_mev_risk::RiskCheckedOpportunity;
use rust_lmax_mev_simulator::{LocalSimulator, SimConfig};
use rust_lmax_mev_state::{PoolId, PoolKind};
use rust_lmax_mev_state_fetcher::{
    FetchError, FetchedAccount, FetchedPoolState, PoolSlotLayout, StateFetcher,
};
use rust_lmax_mev_types::{ChainContext, EventEnvelope, EventSource, PublishMeta};
use tokio::sync::broadcast;

fn weth_addr() -> Address {
    "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
        .parse()
        .unwrap()
}
fn usdc_addr() -> Address {
    "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
        .parse()
        .unwrap()
}

fn synthetic_risk_checked() -> RiskCheckedOpportunity {
    let opp = OpportunityEvent {
        block_number: 22_000_000,
        block_hash: B256::from([0xAB; 32]),
        source_pool: PoolId {
            kind: PoolKind::UniswapV3Fee005,
            address: Address::from([0x88; 20]),
        },
        sink_pool: PoolId {
            kind: PoolKind::UniswapV2,
            address: Address::from([0xB4; 20]),
        },
        optimal_amount_in_wei: U256::from(10_000_000_000_000_000u128),
        expected_profit_wei: U256::ZERO,
        gas_estimate: GAS_ESTIMATE_TWO_HOP_ARB,
    };
    RiskCheckedOpportunity {
        opportunity: opp,
        size_wei: U256::from(10_000_000_000_000_000u128),
    }
}

fn seal(rc: RiskCheckedOpportunity) -> EventEnvelope<RiskCheckedOpportunity> {
    let meta = PublishMeta {
        source: EventSource::RiskEngine,
        chain_context: ChainContext {
            chain_id: 1,
            block_number: 22_000_000,
            block_hash: [0xAB; 32],
        },
        event_version: 1,
        correlation_id: 99,
    };
    EventEnvelope::seal(meta, rc, 1, 1_700_000_000_000_000_000).expect("seal")
}

/// PA-1 (disabled-by-default): with `prefetch_fetcher = None`,
/// `simulator_driver` matches P4-G behavior exactly. Simulator has
/// no fixtures loaded; `simulate_with_fingerprint` returns
/// `Setup` error; no `SimulationOutcomeWithFingerprint` envelope is
/// emitted on `sim_tx`. Verified by asserting the broadcast receiver
/// times out (no message arrives) within a 500ms window.
#[tokio::test(flavor = "multi_thread")]
async fn pa_1_disabled_by_default_no_event_emitted() {
    let (risk_tx, risk_rx) = broadcast::channel(8);
    let (sim_tx, mut sim_rx) =
        broadcast::channel::<EventEnvelope<SimulationOutcomeWithFingerprint>>(8);
    let sim = Arc::new(LocalSimulator::new(SimConfig::defaults()).expect("sim ok"));

    let driver = tokio::spawn(simulator_driver(
        risk_rx,
        sim_tx,
        Arc::clone(&sim),
        None, // P5-A: prefetch_enabled = false → no fetcher
        weth_addr(),
        usdc_addr(),
        Arc::new(
            rust_lmax_mev_execution::BundleConstructor::new(
                rust_lmax_mev_execution::BundleConfig::defaults(),
            )
            .expect("bundle constructor"),
        ),
    ));

    risk_tx
        .send(seal(synthetic_risk_checked()))
        .expect("publish");

    let result = tokio::time::timeout(Duration::from_millis(500), sim_rx.recv()).await;
    assert!(
        result.is_err(),
        "PA-1: with prefetch_fetcher=None and no fixtures, simulator_driver MUST NOT emit SimulationOutcomeWithFingerprint; got {result:?}"
    );

    drop(risk_tx);
    let _ = driver.await;
}

/// Mock fetcher that fails every call with `FetchError::ArchiveNotConfigured`.
struct AlwaysFailFetcher {
    pool_calls: AtomicUsize,
}

impl AlwaysFailFetcher {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            pool_calls: AtomicUsize::new(0),
        })
    }
    fn pool_calls(&self) -> usize {
        self.pool_calls.load(Ordering::Relaxed)
    }
}

#[async_trait::async_trait]
impl StateFetcher for AlwaysFailFetcher {
    async fn fetch_pool(
        &self,
        _pool: &PoolId,
        _block_hash: B256,
        _layout: &dyn PoolSlotLayout,
    ) -> Result<FetchedPoolState, FetchError> {
        self.pool_calls.fetch_add(1, Ordering::Relaxed);
        Err(FetchError::Internal("simulated archive failure".into()))
    }
    async fn fetch_account(
        &self,
        _address: Address,
        _slots: &[U256],
        _block_hash: B256,
    ) -> Result<FetchedAccount, FetchError> {
        Err(FetchError::Internal("simulated archive failure".into()))
    }
}

/// PA-4 (fail-closed on archive error): when prefetch_for fails with
/// `ArchiveNotConfigured`, the driver logs WARN + drops the event;
/// no `SimulationOutcomeWithFingerprint` envelope is emitted; AND
/// subsequent events are not blocked (the driver continues).
#[tokio::test(flavor = "multi_thread")]
async fn pa_4_fail_closed_on_archive_error() {
    let (risk_tx, risk_rx) = broadcast::channel(8);
    let (sim_tx, mut sim_rx) =
        broadcast::channel::<EventEnvelope<SimulationOutcomeWithFingerprint>>(8);
    let sim = Arc::new(LocalSimulator::new(SimConfig::defaults()).expect("sim ok"));
    let raw = AlwaysFailFetcher::new();
    let fetcher: Arc<dyn StateFetcher> = raw.clone();

    let driver = tokio::spawn(simulator_driver(
        risk_rx,
        sim_tx,
        Arc::clone(&sim),
        Some(fetcher),
        weth_addr(),
        usdc_addr(),
        Arc::new(
            rust_lmax_mev_execution::BundleConstructor::new(
                rust_lmax_mev_execution::BundleConfig::defaults(),
            )
            .expect("bundle constructor"),
        ),
    ));

    // Publish first event → driver attempts prefetch → fetcher fails →
    // event dropped.
    risk_tx
        .send(seal(synthetic_risk_checked()))
        .expect("publish 1");
    let r1 = tokio::time::timeout(Duration::from_millis(500), sim_rx.recv()).await;
    assert!(
        r1.is_err(),
        "PA-4: archive error must drop the event (no SimulationOutcomeWithFingerprint emitted)"
    );

    // Publish a second event → driver did NOT block; another fetch
    // attempt occurs.
    risk_tx
        .send(seal(synthetic_risk_checked()))
        .expect("publish 2");
    let r2 = tokio::time::timeout(Duration::from_millis(500), sim_rx.recv()).await;
    assert!(
        r2.is_err(),
        "PA-4: subsequent event also drops cleanly (driver still running)"
    );
    assert!(
        raw.pool_calls() >= 2,
        "PA-4: driver attempted prefetch on each event; got {} attempts",
        raw.pool_calls()
    );

    drop(risk_tx);
    let _ = driver.await;
}
