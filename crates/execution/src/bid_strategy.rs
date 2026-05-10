//! Phase 5 P5-B dynamic gas bidding strategy infrastructure per
//! ADR-006 §"Gas bidding" Phase 5+ language.
//!
//! Per execution-note v0.3 §DP-B1..DP-B9 + Q-B1..Q-B6 standing answers:
//! - `BidStrategy` trait: object-safe, sync, pure (no I/O).
//! - `BidContext`: minimal Phase 5 fields + `#[non_exhaustive]` +
//!   `new(...)` ctor + `for_legacy_outcome(...)` helper.
//! - `FixedFractionBidStrategy`: byte-identical to the P3-F formula
//!   (`bid = profit * bps / 10_000`); fallible ctor validates
//!   `bps ≤ 10_000`; default infallible.
//! - `Eip1559BasefeeAwareBidStrategy`: caps the bid at
//!   `(base_fee + tip_floor_per_gas) × gas`; fallible ctor with same
//!   bps validation; default infallible.
//! - All arithmetic uses `saturating_*` per Q-B5 / R-B6 (no panic).
//! - Metric counter contract documented; impl deferred to P5-E or
//!   Phase 6 per Q-P5-3 standing answer.
//!
//! No signing, no submission, no `live_send`, no `wire_phase4`
//! changes — the `BundleConstructor::new(cfg)` external surface is
//! byte-identical to P3-F (BS-4 regression-guards this).

use std::sync::Arc;

use alloy_primitives::U256;
use rust_lmax_mev_simulator::SimulationOutcome;

use crate::ExecutionError;

/// `BidStrategy` is the pure-function gas-bid policy abstraction
/// unlocked by ADR-006 §"Gas bidding" Phase 5+ language. Object-safe,
/// sync, pure: implementations MUST NOT perform I/O, panic on
/// extreme inputs, or read external state. `compute_bid` is invoked
/// per `BundleCandidate` construction; the return value is stored
/// in `BundleCandidate.gas_bid_wei`.
///
/// Future P5-E or Phase 6 wiring will emit a metric counter
/// `execution_bid_strategy_total{strategy = <name()>}` at the call
/// site (NOT inside `compute_bid` — strategies stay pure). The
/// canonical strategy names are `"fixed_fraction"` and
/// `"eip1559_basefee_aware"` (BS-6 spec-drift guard).
pub trait BidStrategy: Send + Sync + std::fmt::Debug + 'static {
    /// Stable canonical name for the metric label. Test BS-6 asserts
    /// the exact wording; loosening it forces a metric-doc update.
    fn name(&self) -> &'static str;

    /// Pure: returns the bid in wei. MUST NOT panic on extreme
    /// inputs — implementations use `saturating_*` arithmetic
    /// (BS-8 guards `U256::MAX` / `u64::MAX` extremes).
    fn compute_bid(&self, outcome: &SimulationOutcome, ctx: &BidContext) -> U256;
}

/// Per-construct context the strategy needs alongside the
/// `SimulationOutcome`. `#[non_exhaustive]` per R-B5 — Phase 6+ may
/// add fields (e.g., `prior_block_tip_distribution`) without
/// breaking external constructors.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BidContext {
    /// EIP-1559 block base fee (wei). `0` is the legacy P3-F value
    /// when the live base fee is not yet plumbed through the bus.
    pub block_base_fee_wei: U256,
    /// Gas-used estimate for the candidate bundle. P5-B sources this
    /// from `outcome.gas_used` (revm-measured) per Q-B3.
    pub gas_used_estimate: u64,
}

impl BidContext {
    pub fn new(block_base_fee_wei: U256, gas_used_estimate: u64) -> Self {
        Self {
            block_base_fee_wei,
            gas_used_estimate,
        }
    }

    /// Legacy helper used by `BundleConstructor::construct(&outcome)`
    /// to preserve P3-F byte-identical behavior of the FixedFraction
    /// default strategy. Sets `block_base_fee_wei = U256::ZERO` and
    /// `gas_used_estimate = outcome.gas_used`. Phase 6 wiring of the
    /// live base fee through the bus replaces this helper at the
    /// call site.
    pub fn for_legacy_outcome(outcome: &SimulationOutcome) -> Self {
        Self {
            block_base_fee_wei: U256::ZERO,
            gas_used_estimate: outcome.gas_used,
        }
    }
}

/// Phase 3 P3-F default strategy preserved as a `BidStrategy` impl.
/// `bid = profit.saturating_mul(U256::from(bps)) / U256::from(10_000u32)`.
#[derive(Debug, Clone)]
pub struct FixedFractionBidStrategy {
    fixed_bid_fraction_bps: u16,
}

impl FixedFractionBidStrategy {
    /// Fallible constructor: validates `bps ≤ 10_000`. Mirrors the
    /// existing `BundleConstructor::new` validation wording so an
    /// existing operator who passes the same bad value sees the same
    /// error message.
    pub fn new(fixed_bid_fraction_bps: u16) -> Result<Self, ExecutionError> {
        if fixed_bid_fraction_bps > 10_000 {
            return Err(ExecutionError::Setup(format!(
                "fixed_bid_fraction_bps must be in 0..=10_000, got {fixed_bid_fraction_bps}"
            )));
        }
        Ok(Self {
            fixed_bid_fraction_bps,
        })
    }

    pub fn fixed_bid_fraction_bps(&self) -> u16 {
        self.fixed_bid_fraction_bps
    }
}

impl Default for FixedFractionBidStrategy {
    fn default() -> Self {
        // DEFAULT_FIXED_BID_FRACTION_BPS = 9_000 is a known-good
        // constant (≤ 10_000), so unwrap is infallible.
        Self::new(crate::DEFAULT_FIXED_BID_FRACTION_BPS).expect("default 9_000 ≤ 10_000")
    }
}

impl BidStrategy for FixedFractionBidStrategy {
    fn name(&self) -> &'static str {
        "fixed_fraction"
    }

    fn compute_bid(&self, outcome: &SimulationOutcome, _ctx: &BidContext) -> U256 {
        outcome
            .simulated_profit_wei
            .saturating_mul(U256::from(self.fixed_bid_fraction_bps))
            / U256::from(10_000u32)
    }
}

/// Phase 5 P5-B EIP-1559-aware strategy. Caps the FixedFraction bid
/// at `(base_fee + tip_floor_per_gas) × gas` so an over-eager profit
/// estimate never overspends gas.
#[derive(Debug, Clone)]
pub struct Eip1559BasefeeAwareBidStrategy {
    fixed_bid_fraction_bps: u16,
    tip_floor_per_gas_wei: U256,
}

impl Eip1559BasefeeAwareBidStrategy {
    /// Fallible constructor — same `bps ≤ 10_000` validation as
    /// `FixedFractionBidStrategy::new`. `tip_floor_per_gas_wei` has
    /// no validation (any U256 is meaningful as a per-gas tip floor;
    /// `0` is a legitimate "no tip" value).
    pub fn new(
        fixed_bid_fraction_bps: u16,
        tip_floor_per_gas_wei: U256,
    ) -> Result<Self, ExecutionError> {
        if fixed_bid_fraction_bps > 10_000 {
            return Err(ExecutionError::Setup(format!(
                "fixed_bid_fraction_bps must be in 0..=10_000, got {fixed_bid_fraction_bps}"
            )));
        }
        Ok(Self {
            fixed_bid_fraction_bps,
            tip_floor_per_gas_wei,
        })
    }

    pub fn fixed_bid_fraction_bps(&self) -> u16 {
        self.fixed_bid_fraction_bps
    }

    pub fn tip_floor_per_gas_wei(&self) -> U256 {
        self.tip_floor_per_gas_wei
    }
}

impl Default for Eip1559BasefeeAwareBidStrategy {
    fn default() -> Self {
        // DEFAULT_FIXED_BID_FRACTION_BPS = 9_000 ≤ 10_000 + 1 gwei
        // tip floor are known-good constants → infallible.
        Self::new(
            crate::DEFAULT_FIXED_BID_FRACTION_BPS,
            U256::from(1_000_000_000u64), // 1 gwei per gas
        )
        .expect("default 9_000 bps ≤ 10_000")
    }
}

impl BidStrategy for Eip1559BasefeeAwareBidStrategy {
    fn name(&self) -> &'static str {
        "eip1559_basefee_aware"
    }

    fn compute_bid(&self, outcome: &SimulationOutcome, ctx: &BidContext) -> U256 {
        let fixed = outcome
            .simulated_profit_wei
            .saturating_mul(U256::from(self.fixed_bid_fraction_bps))
            / U256::from(10_000u32);
        let cap = ctx
            .block_base_fee_wei
            .saturating_add(self.tip_floor_per_gas_wei)
            .saturating_mul(U256::from(ctx.gas_used_estimate));
        fixed.min(cap)
    }
}

/// Convenience type alias used by `BundleConstructor::with_strategy`.
pub type BidStrategyRef = Arc<dyn BidStrategy>;

#[cfg(test)]
mod tests {
    use super::*;
    use rust_lmax_mev_simulator::{ProfitSource, SimStatus};

    fn outcome(profit_wei: U256, gas_used: u64) -> SimulationOutcome {
        SimulationOutcome {
            opportunity_block_number: 22_000_000,
            gas_used,
            status: SimStatus::Success,
            simulated_profit_wei: profit_wei,
            profit_source: ProfitSource::RevmComputed,
        }
    }

    /// BS-1: trait object-safety compile-asserted for both impls.
    /// Failing to compile this fn proves the trait is NOT object-safe.
    #[test]
    fn bs_1_trait_object_safety() {
        let _: Box<dyn BidStrategy> = Box::new(FixedFractionBidStrategy::default());
        let _: Box<dyn BidStrategy> = Box::new(Eip1559BasefeeAwareBidStrategy::default());
    }

    /// BS-2: FixedFraction parity with the P3-F formula. For
    /// `bps = 9_000`, `bid = profit * 9_000 / 10_000 = profit * 0.9`.
    #[test]
    fn bs_2_fixed_fraction_parity_with_p3f() {
        let strat = FixedFractionBidStrategy::default();
        let out = outcome(U256::from(1_000_000u64), 100_000);
        let ctx = BidContext::for_legacy_outcome(&out);
        let bid = strat.compute_bid(&out, &ctx);
        // 1_000_000 * 9_000 / 10_000 = 900_000.
        assert_eq!(bid, U256::from(900_000u64));
    }

    /// BS-3 (R-B8): EIP-1559 cap behavior — returns the LESSER of
    /// `(profit*bps/10_000)` and `((base_fee + tip_floor_per_gas) * gas)`.
    #[test]
    fn bs_3_eip1559_cap_behavior() {
        let strat = Eip1559BasefeeAwareBidStrategy::default(); // bps=9_000, tip=1 gwei

        // Case A: fixed-fraction wins (cap is huge).
        // profit = 1_000_000 wei → fixed = 900_000.
        // base_fee = 0, tip = 1 gwei = 1e9; gas = 1_000_000.
        // cap = (0 + 1e9) * 1_000_000 = 1e15. 900_000 < 1e15 → fixed.
        let out_a = outcome(U256::from(1_000_000u64), 1_000_000);
        let ctx_a = BidContext::new(U256::ZERO, 1_000_000);
        let bid_a = strat.compute_bid(&out_a, &ctx_a);
        assert_eq!(bid_a, U256::from(900_000u64), "fixed-fraction case");

        // Case B: cap binds.
        // profit = 1e18 wei → fixed = 9e17.
        // base_fee = 30 gwei = 3e10; tip = 1 gwei = 1e9; gas = 100_000.
        // cap = (3e10 + 1e9) * 100_000 = 31e9 * 1e5 = 3.1e15. 9e17 > 3.1e15 → cap.
        let out_b = outcome(U256::from(1_000_000_000_000_000_000u128), 100_000);
        let ctx_b = BidContext::new(U256::from(30_000_000_000u64), 100_000);
        let bid_b = strat.compute_bid(&out_b, &ctx_b);
        let expected_cap = U256::from(31_000_000_000u64) * U256::from(100_000u64);
        assert_eq!(bid_b, expected_cap, "cap-bound case");
        assert!(
            bid_b < U256::from(900_000_000_000_000_000u128),
            "cap < fixed"
        );
    }

    /// BS-6: strategy `name()` stability — spec-drift guard.
    #[test]
    fn bs_6_strategy_name_stability() {
        assert_eq!(FixedFractionBidStrategy::default().name(), "fixed_fraction");
        assert_eq!(
            Eip1559BasefeeAwareBidStrategy::default().name(),
            "eip1559_basefee_aware"
        );
    }

    /// BS-7 (extended R-B7): bps validation on BOTH strategies.
    #[test]
    fn bs_7_bps_validation_on_both_strategies() {
        match FixedFractionBidStrategy::new(11_000) {
            Err(ExecutionError::Setup(msg)) => {
                assert!(msg.contains("0..=10_000"), "msg = {msg}");
            }
            other => panic!("expected Setup error, got {other:?}"),
        }
        match Eip1559BasefeeAwareBidStrategy::new(11_000, U256::from(1_000_000_000u64)) {
            Err(ExecutionError::Setup(msg)) => {
                assert!(msg.contains("0..=10_000"), "msg = {msg}");
            }
            other => panic!("expected Setup error, got {other:?}"),
        }
    }

    /// BS-8 (R-B6): saturating arithmetic / no-panic on extreme
    /// inputs. Drives both strategies with `U256::MAX` profit,
    /// `u64::MAX` gas, `U256::MAX` base fee + tip; asserts both
    /// return SOME `U256` without panicking.
    #[test]
    fn bs_8_saturating_no_panic_on_extremes() {
        let extreme = outcome(U256::MAX, u64::MAX);
        let extreme_ctx = BidContext::new(U256::MAX, u64::MAX);

        let f = FixedFractionBidStrategy::default();
        let _bid_f = f.compute_bid(&extreme, &extreme_ctx);
        // No panic — assertion holds by reaching this line.

        let e = Eip1559BasefeeAwareBidStrategy::new(10_000, U256::MAX).expect("ctor ok");
        let _bid_e = e.compute_bid(&extreme, &extreme_ctx);
        // No panic — assertion holds by reaching this line.
    }
}
