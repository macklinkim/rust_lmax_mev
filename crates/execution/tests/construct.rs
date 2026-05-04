//! Phase 3 P3-F tests for `BundleConstructor::construct` per the
//! approved Batch F execution note v0.1.
//!
//! All fixtures are deterministic small-integer values constructed
//! via crate-local helpers. No live network, no funded key.
//!
//! Test ladder:
//! - E-1 happy: successful sim + positive profit + non-zero bid → BundleCandidate.
//! - E-2 abort: status != Success → Err(Aborted { SimulationNotSuccess }).

use alloy_primitives::U256;
use rust_lmax_mev_execution::{
    AbortReason, BundleConfig, BundleConstructor, ExecutionError, DEFAULT_FIXED_BID_FRACTION_BPS,
    DEFAULT_VALIDITY_BLOCK_WINDOW,
};
use rust_lmax_mev_simulator::{ProfitSource, SimStatus, SimulationOutcome};

fn outcome_success(profit_wei: U256) -> SimulationOutcome {
    SimulationOutcome {
        opportunity_block_number: 18_000_000,
        gas_used: 21_002,
        status: SimStatus::Success,
        simulated_profit_wei: profit_wei,
        profit_source: ProfitSource::HeuristicPassthrough,
    }
}

fn outcome_failed(status: SimStatus) -> SimulationOutcome {
    SimulationOutcome {
        opportunity_block_number: 18_000_000,
        gas_used: 21_000,
        status,
        simulated_profit_wei: U256::from(50_000_000_000_000u128),
        profit_source: ProfitSource::HeuristicPassthrough,
    }
}

/// E-1 happy: 0.001 ETH profit + 90% fixed bid → BundleCandidate with
/// gas_bid_wei == 0.0009 ETH. Validity window starts at the opp block
/// and spans `DEFAULT_VALIDITY_BLOCK_WINDOW` blocks. profit_source is
/// passed through verbatim from the upstream SimulationOutcome.
#[test]
fn construct_returns_candidate_for_successful_sim() {
    let ctor = BundleConstructor::new(BundleConfig::defaults()).unwrap();
    let profit = U256::from(1_000_000_000_000_000u128); // 0.001 ETH
    let candidate = ctor
        .construct(&outcome_success(profit))
        .expect("successful sim with positive profit must produce a candidate");

    assert_eq!(candidate.opportunity_block_number, 18_000_000);
    assert_eq!(candidate.gas_used, 21_002);
    assert_eq!(candidate.simulated_profit_wei, profit);
    // 0.001 ETH * 9_000 / 10_000 = 0.0009 ETH = 9e14 wei.
    assert_eq!(candidate.gas_bid_wei, U256::from(900_000_000_000_000u128));
    assert_eq!(candidate.validity_block_min, 18_000_000);
    assert_eq!(
        candidate.validity_block_max,
        18_000_000 + DEFAULT_VALIDITY_BLOCK_WINDOW - 1
    );
    assert_eq!(candidate.profit_source, ProfitSource::HeuristicPassthrough);
    // Sanity: default bid fraction is 9_000 bps (90%).
    assert_eq!(
        BundleConfig::defaults().fixed_bid_fraction_bps,
        DEFAULT_FIXED_BID_FRACTION_BPS
    );
}

/// E-2 abort: SimulationOutcome with non-Success status → no bundle.
#[test]
fn construct_aborts_when_sim_not_success() {
    let ctor = BundleConstructor::new(BundleConfig::defaults()).unwrap();
    let err = ctor
        .construct(&outcome_failed(SimStatus::OutOfGas))
        .expect_err("non-Success status must abort");
    assert!(matches!(
        err,
        ExecutionError::Aborted {
            reason: AbortReason::SimulationNotSuccess
        }
    ));
}

/// Bonus boundary: zero profit → NonPositiveProfit (extra coverage of
/// the cap-evaluation order; not numbered in the v0.1 plan but lean +
/// deterministic).
#[test]
fn construct_aborts_on_zero_profit() {
    let ctor = BundleConstructor::new(BundleConfig::defaults()).unwrap();
    let err = ctor
        .construct(&outcome_success(U256::ZERO))
        .expect_err("zero profit must abort");
    assert!(matches!(
        err,
        ExecutionError::Aborted {
            reason: AbortReason::NonPositiveProfit
        }
    ));
}

/// Bonus boundary: tiny profit where bid rounds to zero → BidRoundsToZero.
/// At fraction 9_000 bps and profit = 1 wei, bid = 1 * 9_000 / 10_000 = 0.
#[test]
fn construct_aborts_when_bid_rounds_to_zero() {
    let ctor = BundleConstructor::new(BundleConfig::defaults()).unwrap();
    let err = ctor
        .construct(&outcome_success(U256::from(1u64)))
        .expect_err("bid rounding to zero must abort");
    assert!(matches!(
        err,
        ExecutionError::Aborted {
            reason: AbortReason::BidRoundsToZero
        }
    ));
}
