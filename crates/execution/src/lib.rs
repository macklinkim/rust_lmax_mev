//! Phase 3 P3-F bundle construction per the approved Batch F execution
//! note v0.1.
//!
//! Pure-function `BundleConstructor::construct` consumes a
//! `SimulationOutcome` (from P3-E) and emits a `BundleCandidate`
//! representing the *intent* to submit. NO relay submission, NO
//! signing, NO funded key, NO `BundleRelay` — those land in Phase 4
//! per ADR-002 + ADR-006.
//!
//! Per ADR-006 §"Gas bidding" Phase 4 thin-path uses conservative
//! fixed gas bidding `bid = profit * fixed_bid_fraction` (default 0.90).
//! Phase 5+ swaps in dynamic / EIP-1559 / ML strategies; the API
//! surface in P3-F is shaped to allow that swap without breakage.

pub mod rkyv_compat;

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
/// relay endpoints, no signer-id (those land in P4 alongside funded
/// key + signing infrastructure).
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
pub struct BundleConstructor {
    cfg: BundleConfig,
}

impl BundleConstructor {
    /// Validates `BundleConfig` and returns a ready-to-construct
    /// engine. `Err(ExecutionError::Setup)` for obvious bad-config
    /// (validity_block_window == 0, fixed_bid_fraction_bps > 10_000).
    pub fn new(cfg: BundleConfig) -> Result<Self, ExecutionError> {
        if cfg.validity_block_window == 0 {
            return Err(ExecutionError::Setup(
                "validity_block_window must be non-zero".to_string(),
            ));
        }
        if cfg.fixed_bid_fraction_bps > 10_000 {
            return Err(ExecutionError::Setup(format!(
                "fixed_bid_fraction_bps must be in 0..=10_000, got {}",
                cfg.fixed_bid_fraction_bps
            )));
        }
        Ok(Self { cfg })
    }

    pub fn cfg(&self) -> &BundleConfig {
        &self.cfg
    }

    /// Pure: returns `Ok(BundleCandidate)` iff the simulation succeeded
    /// AND profit > 0 AND the bid is non-zero after `fixed_bid_fraction`
    /// is applied. Otherwise `Err(ExecutionError::Aborted { reason })`.
    pub fn construct(
        &self,
        outcome: &SimulationOutcome,
    ) -> Result<BundleCandidate, ExecutionError> {
        let aborted = |reason| ExecutionError::Aborted { reason };

        if outcome.status != SimStatus::Success {
            return Err(aborted(AbortReason::SimulationNotSuccess));
        }
        if outcome.simulated_profit_wei.is_zero() {
            return Err(aborted(AbortReason::NonPositiveProfit));
        }

        // gas_bid = profit * fraction_bps / 10_000. U256-safe; both
        // operands fit U256, multiplication is U256-multiplication.
        let bid = outcome.simulated_profit_wei
            * U256::from(self.cfg.fixed_bid_fraction_bps)
            / U256::from(10_000u32);
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
