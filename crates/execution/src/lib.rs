//! Phase 3 P3-F bundle construction per the approved Batch F execution
//! note v0.1.
//!
//! Pure-function `BundleConstructor::construct` consumes a
//! `SimulationOutcome` (from P3-E) and emits a `BundleCandidate`
//! representing the *intent* to submit. NO relay submission, NO
//! signing, NO production key material, NO `BundleRelay` — those land in Phase 4
//! per ADR-002 + ADR-006.
//!
//! Per ADR-006 §"Gas bidding" Phase 4 thin-path uses conservative
//! fixed gas bidding `bid = profit * fixed_bid_fraction` (default 0.90).
//! Phase 5+ swaps in dynamic / EIP-1559 / ML strategies; the API
//! surface in P3-F is shaped to allow that swap without breakage.

pub mod bid_strategy;
pub mod rkyv_compat;

pub use bid_strategy::{
    BidContext, BidStrategy, BidStrategyRef, Eip1559BasefeeAwareBidStrategy,
    FixedFractionBidStrategy,
};

use std::sync::Arc;

use alloy_primitives::{Address, U256};
use rust_lmax_mev_simulator::{ProfitSource, SimStatus, SimulationOutcome};
use serde::{Deserialize, Serialize};

/// ADR-006 default `fixed_bid_fraction` (0.90 = bid 90% of profit).
/// Stored as basis points (9_000 / 10_000) so we can stay in U256
/// arithmetic instead of round-tripping through f64.
pub const DEFAULT_FIXED_BID_FRACTION_BPS: u16 = 9_000;

/// Default validity window: bundle valid from `opp.block_number` to
/// `opp.block_number + window - 1` (5 blocks). Phase 4 may tune
/// per-relay; default is conservative.
pub const DEFAULT_VALIDITY_BLOCK_WINDOW: u64 = 5;

/// Bundle construction config. `fixed_bid_fraction_bps` lives in basis
/// points (9_000 = 90%, 10_000 = 100%) so the `evaluate` arithmetic
/// stays U256-only. `coinbase_recipient` is a placeholder until
/// Phase 4 wires the real builder coinbase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleConfig {
    pub fixed_bid_fraction_bps: u16,
    pub coinbase_recipient: Address,
    pub validity_block_window: u64,
}

impl BundleConfig {
    /// Spec-defaults constructor. `coinbase_recipient` defaults to
    /// `Address::ZERO` (Phase 4 must override before any submission).
    pub fn defaults() -> Self {
        Self {
            fixed_bid_fraction_bps: DEFAULT_FIXED_BID_FRACTION_BPS,
            coinbase_recipient: Address::ZERO,
            validity_block_window: DEFAULT_VALIDITY_BLOCK_WINDOW,
        }
    }
}

/// Bundle abort reasons. `#[non_exhaustive]` so future caps land
/// additively. Phase 4 expands with relay-specific reject reasons.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AbortReason {
    /// `outcome.status != SimStatus::Success`.
    SimulationNotSuccess,
    /// `outcome.simulated_profit_wei == 0`.
    NonPositiveProfit,
    /// `profit * fixed_bid_fraction_bps / 10_000` rounds to zero.
    BidRoundsToZero,
}

/// Bundle construction failure surface.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum ExecutionError {
    #[error("bundle aborted: {reason:?}")]
    Aborted { reason: AbortReason },
    #[error("invalid BundleConfig: {0}")]
    Setup(String),
}

/// Phase 3 bundle candidate: the *intent* to submit. Phase 4
/// `BundleRelay` consumes this + signs + submits to relays. P3-F
/// fields are deliberately the irreducible minimum needed by P4
/// (gas bid + validity window + provenance) — no signed-tx hash, no
/// relay endpoints, no signer-id (those land in P4 alongside production
/// key material + signing infrastructure).
///
/// Per `event-model.md` derives the spec-mandated `Clone, Debug,
/// PartialEq, Eq, rkyv::*, serde::*`.
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
pub struct BundleCandidate {
    pub opportunity_block_number: u64,
    pub gas_used: u64,
    #[rkyv(with = crate::rkyv_compat::U256AsBytes)]
    pub simulated_profit_wei: U256,
    #[rkyv(with = crate::rkyv_compat::U256AsBytes)]
    pub gas_bid_wei: U256,
    pub validity_block_min: u64,
    pub validity_block_max: u64,
    pub profit_source: ProfitSource,
}

/// Stateless bundle constructor. Construct once per process from the
/// `BundleConfig`; `construct(...)` is `&self` and allocation-free
/// beyond the returned struct.
///
/// Phase 5 P5-B: holds an `Arc<dyn BidStrategy>` so the gas-bid
/// math is a pluggable strategy. `BundleConstructor::new(cfg)` keeps
/// its P3-F signature + behavior byte-identical (default strategy is
/// `FixedFractionBidStrategy::new(cfg.fixed_bid_fraction_bps)?`).
/// New `with_strategy(cfg, strategy)` accepts an explicit strategy
/// (e.g., `Eip1559BasefeeAwareBidStrategy`).
pub struct BundleConstructor {
    cfg: BundleConfig,
    strategy: BidStrategyRef,
}

impl BundleConstructor {
    /// Validates `BundleConfig` and returns a ready-to-construct
    /// engine using the default `FixedFractionBidStrategy` per
    /// `cfg.fixed_bid_fraction_bps`. `Err(ExecutionError::Setup)`
    /// for `validity_block_window == 0` or
    /// `fixed_bid_fraction_bps > 10_000`. P3-F byte-identical
    /// behavior preserved (BS-4 regression-guards this).
    pub fn new(cfg: BundleConfig) -> Result<Self, ExecutionError> {
        if cfg.validity_block_window == 0 {
            return Err(ExecutionError::Setup(
                "validity_block_window must be non-zero".to_string(),
            ));
        }
        // Strategy ctor itself validates `bps ≤ 10_000` (R-B2);
        // propagate the same Setup wording the previous P3-F new()
        // returned.
        let strategy: BidStrategyRef =
            Arc::new(FixedFractionBidStrategy::new(cfg.fixed_bid_fraction_bps)?);
        Ok(Self { cfg, strategy })
    }

    /// Phase 5 P5-B (DP-B5): explicit-strategy constructor. The
    /// strategy is responsible for its own validation; this ctor
    /// validates only `cfg.validity_block_window`. Existing tests
    /// using `BundleConstructor::new(cfg)` are unaffected.
    pub fn with_strategy(
        cfg: BundleConfig,
        strategy: BidStrategyRef,
    ) -> Result<Self, ExecutionError> {
        if cfg.validity_block_window == 0 {
            return Err(ExecutionError::Setup(
                "validity_block_window must be non-zero".to_string(),
            ));
        }
        Ok(Self { cfg, strategy })
    }

    pub fn cfg(&self) -> &BundleConfig {
        &self.cfg
    }

    pub fn strategy_name(&self) -> &'static str {
        self.strategy.name()
    }

    /// Pure: returns `Ok(BundleCandidate)` iff the simulation succeeded
    /// AND profit > 0 AND the bid is non-zero after the strategy is
    /// applied. Otherwise `Err(ExecutionError::Aborted { reason })`.
    ///
    /// P5-B (DP-B6): internally constructs `BidContext::for_legacy_outcome`
    /// (R-B10) and delegates to `construct_with_context`. The legacy
    /// context sets `block_base_fee_wei = U256::ZERO` so the
    /// `wire_phase4` execution_driver's existing call site preserves
    /// P3-F byte-identical behavior with the default
    /// `FixedFractionBidStrategy`.
    pub fn construct(
        &self,
        outcome: &SimulationOutcome,
    ) -> Result<BundleCandidate, ExecutionError> {
        let ctx = BidContext::for_legacy_outcome(outcome);
        self.construct_with_context(outcome, &ctx)
    }

    /// Phase 5 P5-B (DP-B6): explicit-context construct API. The
    /// caller supplies the `BidContext` (e.g., live block base fee
    /// + gas estimate); the strategy uses it to compute the bid.
    pub fn construct_with_context(
        &self,
        outcome: &SimulationOutcome,
        ctx: &BidContext,
    ) -> Result<BundleCandidate, ExecutionError> {
        let aborted = |reason| ExecutionError::Aborted { reason };

        if outcome.status != SimStatus::Success {
            return Err(aborted(AbortReason::SimulationNotSuccess));
        }
        if outcome.simulated_profit_wei.is_zero() {
            return Err(aborted(AbortReason::NonPositiveProfit));
        }

        let bid = self.strategy.compute_bid(outcome, ctx);
        if bid.is_zero() {
            return Err(aborted(AbortReason::BidRoundsToZero));
        }

        let validity_min = outcome.opportunity_block_number;
        let validity_max = validity_min.saturating_add(self.cfg.validity_block_window - 1);

        Ok(BundleCandidate {
            opportunity_block_number: outcome.opportunity_block_number,
            gas_used: outcome.gas_used,
            simulated_profit_wei: outcome.simulated_profit_wei,
            gas_bid_wei: bid,
            validity_block_min: validity_min,
            validity_block_max: validity_max,
            profit_source: outcome.profit_source,
        })
    }
}

#[cfg(test)]
mod construct_tests {
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

    /// BS-4: `BundleConstructor::new(BundleConfig::defaults()).construct(&outcome)`
    /// produces a `BundleCandidate` byte-identical to the P3-F formula
    /// — the `wire_phase4` execution_driver path is unchanged. This
    /// is the regression guard: if a future PR accidentally swaps
    /// the default strategy, this test breaks.
    #[test]
    fn bs_4_default_strategy_p3f_byte_identical() {
        let cfg = BundleConfig::defaults();
        let ctor = BundleConstructor::new(cfg.clone()).expect("ctor ok");
        // profit = 1_000_000 wei → bid = 900_000 (bps 9_000).
        let out = outcome(U256::from(1_000_000u64), 100_000);
        let cand = ctor.construct(&out).expect("construct ok");
        assert_eq!(cand.gas_bid_wei, U256::from(900_000u64));
        assert_eq!(cand.opportunity_block_number, 22_000_000);
        assert_eq!(cand.gas_used, 100_000);
        assert_eq!(cand.simulated_profit_wei, U256::from(1_000_000u64));
        // validity_block_window default = 5 → max = 22_000_000 + 4.
        assert_eq!(cand.validity_block_min, 22_000_000);
        assert_eq!(cand.validity_block_max, 22_000_004);
        assert_eq!(ctor.strategy_name(), "fixed_fraction");
    }

    /// BS-5 (R-B4): `with_strategy(eip1559).construct_with_context`
    /// produces a `gas_bid_wei` capped per DP-B4. Uses an explicit
    /// `BidContext` with NONZERO `block_base_fee_wei` so the EIP-1559
    /// cap actually binds (legacy `construct(&outcome)` would set
    /// base fee 0 and the cap would not exercise).
    #[test]
    fn bs_5_with_strategy_eip1559_capped_via_construct_with_context() {
        let cfg = BundleConfig::defaults();
        let strategy: BidStrategyRef = Arc::new(Eip1559BasefeeAwareBidStrategy::default());
        let ctor = BundleConstructor::with_strategy(cfg, strategy).expect("ctor ok");
        // profit = 1e18 wei → fixed-fraction = 9e17.
        // base_fee = 30 gwei, tip = 1 gwei (default), gas = 100_000.
        // cap = (3e10 + 1e9) * 1e5 = 3.1e15. 9e17 > 3.1e15 → cap.
        let out = outcome(U256::from(1_000_000_000_000_000_000u128), 100_000);
        let ctx = BidContext::new(U256::from(30_000_000_000u64), 100_000);
        let cand = ctor
            .construct_with_context(&out, &ctx)
            .expect("construct_with_context ok");
        let expected_cap = U256::from(31_000_000_000u64) * U256::from(100_000u64);
        assert_eq!(cand.gas_bid_wei, expected_cap, "EIP-1559 cap binds");
        assert!(
            cand.gas_bid_wei < U256::from(900_000_000_000_000_000u128),
            "cap < fixed-fraction bid"
        );
        assert_eq!(ctor.strategy_name(), "eip1559_basefee_aware");
    }
}
