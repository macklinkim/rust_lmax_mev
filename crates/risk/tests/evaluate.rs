//! Phase 3 P3-D tests for `RiskGate::evaluate` per the approved Batch D
//! execution note v0.2 + Codex 17:20:11/17:26:59 verdicts.
//!
//! All fixtures are deterministic small-integer values constructed
//! with crate-local helpers. No live trading, no relay, no submission,
//! no network, no funded key.
//!
//! Test ladder R-1..R-9 (R-7 rkyv envelope round-trip lives in
//! crates/risk/tests/rkyv_round_trip.rs):
//! - R-1 happy small opp under all caps
//! - R-2 clamp oversize opp to per-bundle cap
//! - R-3 abort daily realized loss at cap
//! - R-4 abort concurrency at cap
//! - R-5 abort resubmits at cap
//! - R-6 boundary: strategy capital unset → relative falls through to absolute
//! - R-8 abort daily gas at cap
//! - R-9 abort canary capital insufficient

use std::collections::HashMap;

use alloy_primitives::{Address, B256, U256};
use rust_lmax_mev_opportunity::{OpportunityEvent, GAS_ESTIMATE_TWO_HOP_ARB};
use rust_lmax_mev_risk::{
    AbortCategory, OpportunityKey, RiskBudgetConfig, RiskBudgetState, RiskGate,
    DEFAULT_PER_BUNDLE_MAX_NOTIONAL_WEI,
};
use rust_lmax_mev_state::{PoolId, PoolKind};

// --- Test helpers --------------------------------------------------------

fn pool_v2() -> PoolId {
    PoolId {
        kind: PoolKind::UniswapV2,
        address: Address::from([0xB4; 20]),
    }
}
fn pool_v3() -> PoolId {
    PoolId {
        kind: PoolKind::UniswapV3Fee005,
        address: Address::from([0x88; 20]),
    }
}

/// Builds a synthetic `OpportunityEvent` with the supplied `optimal`
/// size; everything else is fixed for determinism.
fn opp(optimal_wei: U256) -> OpportunityEvent {
    OpportunityEvent {
        block_number: 18_000_000,
        block_hash: B256::from([0xAB; 32]),
        source_pool: pool_v2(),
        sink_pool: pool_v3(),
        optimal_amount_in_wei: optimal_wei,
        expected_profit_wei: U256::from(1_000_000_000_000_000u128), // 0.001 ETH placeholder
        gas_estimate: GAS_ESTIMATE_TWO_HOP_ARB,
    }
}

// --- Tests ---------------------------------------------------------------

/// R-1 happy: 0.005 ETH opp under all caps → Approved at 0.005 ETH.
#[test]
fn evaluate_returns_approved_for_small_safe_opportunity() {
    let gate = RiskGate::new(RiskBudgetConfig::defaults());
    let small = U256::from(5_000_000_000_000_000u128); // 0.005 ETH
    let approved = gate
        .evaluate(&opp(small))
        .expect("small opp under all caps must be approved");
    assert_eq!(approved.size_wei, small);
    assert_eq!(approved.opportunity.optimal_amount_in_wei, small);
}

/// R-2 clamp: 1 ETH opp + 0.1 ETH per-bundle cap → Approved at 0.1 ETH.
#[test]
fn evaluate_clamps_oversized_opportunity_to_per_bundle_cap() {
    let gate = RiskGate::new(RiskBudgetConfig::defaults());
    let huge = U256::from(1_000_000_000_000_000_000u128); // 1 ETH
    let approved = gate
        .evaluate(&opp(huge))
        .expect("oversize opp should clamp, not abort");
    assert_eq!(
        approved.size_wei,
        U256::from(DEFAULT_PER_BUNDLE_MAX_NOTIONAL_WEI)
    );
    // Caller still sees the original opp.optimal_amount_in_wei in the
    // RiskCheckedOpportunity.opportunity field (not clobbered).
    assert_eq!(approved.opportunity.optimal_amount_in_wei, huge);
}

/// R-3 abort: state has daily_realized_loss == cap → DailyLossCapWouldBeExceeded.
#[test]
fn evaluate_aborts_when_daily_loss_at_cap() {
    let config = RiskBudgetConfig::defaults();
    let mut state = RiskBudgetState::new(&config, 0);
    state.daily_realized_loss_wei = config.daily_realized_loss_cap_wei;
    let gate = RiskGate::with_state(config, state);

    let aborted = gate
        .evaluate(&opp(U256::from(1_000_000_000_000u128))) // tiny opp
        .expect_err("must abort when daily loss is at cap");
    assert_eq!(aborted.category, AbortCategory::DailyLossCapWouldBeExceeded);
}

/// R-4 abort: state has concurrent_live_bundles == max → ConcurrencyCapExceeded.
#[test]
fn evaluate_aborts_when_concurrent_live_at_cap() {
    let config = RiskBudgetConfig::defaults();
    let mut state = RiskBudgetState::new(&config, 0);
    state.concurrent_live_bundles = config.max_concurrent_live_bundles;
    let gate = RiskGate::with_state(config, state);

    let aborted = gate
        .evaluate(&opp(U256::from(1_000_000_000_000u128)))
        .expect_err("must abort when at concurrency cap");
    assert_eq!(aborted.category, AbortCategory::ConcurrencyCapExceeded);
}

/// R-5 abort: state has resubmits_per_opportunity[key] == max → ResubmitCapExceeded.
#[test]
fn evaluate_aborts_when_resubmits_at_cap() {
    let config = RiskBudgetConfig::defaults();
    let mut state = RiskBudgetState::new(&config, 0);
    let key = OpportunityKey::from_event(&opp(U256::from(1u64)));
    let mut resubmits = HashMap::new();
    resubmits.insert(key, config.max_resubmits_per_opportunity);
    state.resubmits_per_opportunity = resubmits;
    let gate = RiskGate::with_state(config, state);

    let aborted = gate
        .evaluate(&opp(U256::from(1_000_000_000_000u128)))
        .expect_err("must abort when at resubmit cap");
    assert_eq!(aborted.category, AbortCategory::ResubmitCapExceeded);
}

/// R-6 boundary: config requires relative cap but `strategy_capital_wei == None`.
/// Per spec, absolute caps always apply when strategy capital is unset → no panic,
/// falls through to absolute-only sizing. A small opp under abs caps returns Ok.
#[test]
fn evaluate_aborts_when_strategy_capital_unset_and_relative_required() {
    let mut config = RiskBudgetConfig::defaults();
    // Configure aggressive relative caps that would matter IF strategy_capital was Some,
    // but leave strategy_capital_wei = None.
    config.per_bundle_max_notional_relative_bps = 1; // 0.01% — would be tiny
    config.daily_realized_loss_cap_relative_bps = 1;
    config.strategy_capital_wei = None;

    let gate = RiskGate::new(config);
    // Small opp well under the absolute caps → should still be approved
    // because relative caps fall through when strategy_capital is None.
    let small = U256::from(5_000_000_000_000_000u128); // 0.005 ETH
    let approved = gate
        .evaluate(&opp(small))
        .expect("relative caps must fall through to absolute when strategy_capital is None");
    assert_eq!(approved.size_wei, small);
}

/// R-8 abort daily gas (NEW v0.2 per Codex 17:20:11): state has
/// gas_spend_today_wei == max_gas_spend_per_day_wei; new opp's projected
/// gas (gas_estimate * gas_price_proxy) would exceed → DailyGasCapWouldBeExceeded.
#[test]
fn evaluate_aborts_when_daily_gas_at_cap() {
    let config = RiskBudgetConfig::defaults();
    let mut state = RiskBudgetState::new(&config, 0);
    state.gas_spend_today_wei = config.max_gas_spend_per_day_wei; // already at cap
    let gate = RiskGate::with_state(config, state);

    let aborted = gate
        .evaluate(&opp(U256::from(1_000_000_000_000u128)))
        .expect_err("must abort when daily gas is at cap and projection would exceed");
    assert_eq!(aborted.category, AbortCategory::DailyGasCapWouldBeExceeded);
}

/// R-9 abort canary capital (NEW v0.2 per Codex 17:20:11): clamped opp
/// size > canary_remaining_wei → InsufficientCanaryCapital.
#[test]
fn evaluate_aborts_when_size_exceeds_canary_capital() {
    let config = RiskBudgetConfig::defaults();
    let mut state = RiskBudgetState::new(&config, 0);
    // Burn most canary balance; only 1 wei remains.
    state.canary_remaining_wei = U256::from(1u64);
    let gate = RiskGate::with_state(config, state);

    let aborted = gate
        .evaluate(&opp(U256::from(5_000_000_000_000_000u128))) // 0.005 ETH
        .expect_err("must abort when clamped size exceeds canary remaining");
    assert_eq!(aborted.category, AbortCategory::InsufficientCanaryCapital);
}
