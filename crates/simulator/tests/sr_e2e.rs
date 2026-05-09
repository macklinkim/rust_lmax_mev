//! Phase 4 P4-C2 SR e2e tests for the real-revm `LocalSimulator`
//! pipeline + `ProfitSource::RevmComputed` flip. Replaces the P3-E
//! STOP-bytecode shim integration tests (which are no longer
//! applicable now that `simulate` requires a fixture).
//!
//! - `new_returns_error_on_invalid_config` — `LocalSimulator::new`
//!   validation (migrated from the old simulate.rs).
//! - `sr_4_simulate_without_fixtures_returns_setup_error` — proves
//!   the no-fixture path returns the typed `Setup` error rather than
//!   silently producing a wrong answer (plan v0.3 §DP-C14).
//! - `sr_1_real_fixture_e2e_arb_emits_revm_computed` — exercises the
//!   full real-revm path against the recorded mainnet fixtures (V2
//!   pair, V3 0.05% pool, WETH9, USDC ZeppelinOS proxy + impl). The
//!   test does NOT assert positive profit (the recorded block may
//!   not have a profitable arb in the chosen direction); it asserts
//!   `Success` status, `ProfitSource::RevmComputed`, non-zero
//!   `gas_used`, AND determinism via byte-identical
//!   `SimulationOutcome` on repeat call.

use alloy_primitives::{Address, B256, U256};
use rust_lmax_mev_opportunity::{OpportunityEvent, GAS_ESTIMATE_TWO_HOP_ARB};
use rust_lmax_mev_risk::RiskCheckedOpportunity;
use rust_lmax_mev_simulator::{
    LocalSimulator, ProfitSource, SimConfig, SimStatus, SimulationError,
};
use rust_lmax_mev_state::{PoolId, PoolKind};
use rust_lmax_mev_state_fetcher::{FetchedAccount, FetchedPoolState};

#[test]
fn new_returns_error_on_invalid_config() {
    for bad in [
        SimConfig {
            chain_id: 0,
            ..SimConfig::defaults()
        },
        SimConfig {
            gas_limit_per_sim: 0,
            ..SimConfig::defaults()
        },
        SimConfig {
            eoa_initial_balance_wei: U256::ZERO,
            ..SimConfig::defaults()
        },
    ] {
        match LocalSimulator::new(bad) {
            Err(SimulationError::Setup(_)) => {}
            other => panic!("expected Setup error, got {other:?}"),
        }
    }
}

#[test]
fn sr_4_simulate_without_fixtures_returns_setup_error() {
    let sim = LocalSimulator::new(SimConfig::defaults()).expect("new ok");
    let opp = synthetic_opportunity_v2_to_v3();
    let risk_checked = RiskCheckedOpportunity {
        opportunity: opp,
        size_wei: U256::from(10_000_000_000_000_000u128),
    };
    match sim.simulate(&risk_checked) {
        Err(SimulationError::Setup(msg)) => {
            assert!(
                msg.contains("no fixtures loaded"),
                "expected 'no fixtures loaded' message, got: {msg}"
            );
        }
        other => panic!("expected Setup(no fixtures loaded), got {other:?}"),
    }
}

#[test]
fn sr_1_real_fixture_e2e_arb_emits_revm_computed() {
    use rust_lmax_mev_simulator::fixtures;

    // Build the cross-venue arb: source = V3 0.05% (cheap WETH side
    // typically; this assignment isn't guaranteed correct at every
    // block, but the test asserts only that simulation runs cleanly,
    // not that the chosen direction is profitable).
    let opp = OpportunityEvent {
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
        optimal_amount_in_wei: U256::from(10_000_000_000_000_000u128), // 0.01 WETH
        expected_profit_wei: U256::ZERO,
        gas_estimate: GAS_ESTIMATE_TWO_HOP_ARB,
    };
    let risk_checked = RiskCheckedOpportunity {
        opportunity: opp.clone(),
        size_wei: opp.optimal_amount_in_wei,
    };

    let mut sim = LocalSimulator::new(SimConfig::defaults()).expect("new ok");
    sim.load_fixture(
        // source = V3 (per the opportunity above)
        v3_fixture(),
        // sink = V2
        v2_fixture(),
        weth_fixture(),
        usdc_proxy_fixture(),
        usdc_impl_fixture(),
        v2_factory_fixture(),
    )
    .expect("load_fixture ok");

    // SR-1 asserts the full P4-C2 closure invariants:
    // - SimStatus::Success — the real-revm 2-hop arb executes cleanly
    //   against the recorded mainnet fixtures (V2 + V3 0.05% pools,
    //   WETH9, USDC ZeppelinOS proxy + impl, UniswapV2Factory).
    // - ProfitSource::RevmComputed — every outcome is stamped
    //   regardless of profit sign (P4-C2 ProfitSource flip).
    // - gas_used > 50_000 — non-trivial revm execution (real swap
    //   bytecode, not a STOP shim).
    // - block_number passthrough.
    // - Determinism: same inputs → byte-identical SimulationOutcome.
    //
    // If a future fixture re-record (different block hash) breaks any
    // of these, surface the new outcome via the eprintln! at end and
    // iterate via dump_fixture extension.
    let outcome_a = sim.simulate(&risk_checked).expect("simulate ok");
    eprintln!(
        "SR-1 outcome.status = {:?}; outcome.gas_used = {}; outcome.simulated_profit_wei = {}",
        outcome_a.status, outcome_a.gas_used, outcome_a.simulated_profit_wei
    );

    assert!(
        matches!(outcome_a.status, SimStatus::Success),
        "SR-1 must reach SimStatus::Success against the recorded fixtures; got {:?}",
        outcome_a.status,
    );
    assert_eq!(
        outcome_a.profit_source,
        ProfitSource::RevmComputed,
        "P4-C2 must stamp RevmComputed on every outcome"
    );
    assert!(
        outcome_a.gas_used > 50_000,
        "SR-1 e2e arb must consume > 50k gas (real swap bytecode); got {}",
        outcome_a.gas_used,
    );
    assert_eq!(
        outcome_a.opportunity_block_number, opp.block_number,
        "block_number passthrough"
    );

    // Second run: byte-identical (determinism). The pipeline must be
    // pure-functional given the same fixture set + opportunity.
    let outcome_b = sim.simulate(&risk_checked).expect("simulate ok again");
    assert_eq!(
        outcome_a, outcome_b,
        "SR-1 determinism: same inputs → byte-identical SimulationOutcome"
    );
}

// --- Synthetic opportunity for SR-4 (no-fixture Setup error) -------------

fn synthetic_opportunity_v2_to_v3() -> OpportunityEvent {
    OpportunityEvent {
        block_number: 18_000_000,
        block_hash: B256::from([0xAB; 32]),
        source_pool: PoolId {
            kind: PoolKind::UniswapV2,
            address: Address::from([0xB4; 20]),
        },
        sink_pool: PoolId {
            kind: PoolKind::UniswapV3Fee005,
            address: Address::from([0x88; 20]),
        },
        optimal_amount_in_wei: U256::from(10_000_000_000_000_000u128),
        expected_profit_wei: U256::ZERO,
        gas_estimate: GAS_ESTIMATE_TWO_HOP_ARB,
    }
}

// --- Fixture loaders ------------------------------------------------------

fn v2_fixture() -> FetchedPoolState {
    use rust_lmax_mev_simulator::fixtures;
    FetchedPoolState {
        pool: PoolId {
            kind: PoolKind::UniswapV2,
            address: Address::from(fixtures::V2_WETH_USDC_ADDRESS),
        },
        block_hash: B256::from(fixtures::V2_WETH_USDC_BLOCK_HASH),
        pool_code: alloy_primitives::Bytes::copy_from_slice(fixtures::V2_WETH_USDC_CODE),
        pool_storage: storage_from_pool(fixtures::V2_WETH_USDC_STORAGE),
        auxiliary: Vec::new(),
    }
}

fn v3_fixture() -> FetchedPoolState {
    use rust_lmax_mev_simulator::fixtures;
    FetchedPoolState {
        pool: PoolId {
            kind: PoolKind::UniswapV3Fee005,
            address: Address::from(fixtures::V3_WETH_USDC_005_ADDRESS),
        },
        block_hash: B256::from(fixtures::V3_WETH_USDC_005_BLOCK_HASH),
        pool_code: alloy_primitives::Bytes::copy_from_slice(fixtures::V3_WETH_USDC_005_CODE),
        pool_storage: storage_from_pool(fixtures::V3_WETH_USDC_005_STORAGE),
        auxiliary: Vec::new(),
    }
}

fn weth_fixture() -> FetchedAccount {
    use rust_lmax_mev_simulator::fixtures;
    FetchedAccount {
        address: Address::from(fixtures::WETH9_ADDRESS),
        block_hash: B256::from(fixtures::WETH9_BLOCK_HASH),
        code: alloy_primitives::Bytes::copy_from_slice(fixtures::WETH9_CODE),
        storage: storage_from_account(fixtures::WETH9_STORAGE),
    }
}

fn usdc_proxy_fixture() -> FetchedAccount {
    use rust_lmax_mev_simulator::fixtures;
    FetchedAccount {
        address: Address::from(fixtures::USDC_PROXY_ADDRESS),
        block_hash: B256::from(fixtures::USDC_PROXY_BLOCK_HASH),
        code: alloy_primitives::Bytes::copy_from_slice(fixtures::USDC_PROXY_CODE),
        storage: storage_from_account(fixtures::USDC_PROXY_STORAGE),
    }
}

fn usdc_impl_fixture() -> FetchedAccount {
    use rust_lmax_mev_simulator::fixtures;
    FetchedAccount {
        address: Address::from(fixtures::USDC_IMPL_ADDRESS),
        block_hash: B256::from(fixtures::USDC_IMPL_BLOCK_HASH),
        code: alloy_primitives::Bytes::copy_from_slice(fixtures::USDC_IMPL_CODE),
        storage: storage_from_account(fixtures::USDC_IMPL_STORAGE),
    }
}

fn v2_factory_fixture() -> FetchedAccount {
    use rust_lmax_mev_simulator::fixtures;
    FetchedAccount {
        address: Address::from(fixtures::V2_FACTORY_ADDRESS),
        block_hash: B256::from(fixtures::V2_FACTORY_BLOCK_HASH),
        code: alloy_primitives::Bytes::copy_from_slice(fixtures::V2_FACTORY_CODE),
        storage: storage_from_account(fixtures::V2_FACTORY_STORAGE),
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
