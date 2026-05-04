//! Phase 3 P3-D risk gate per the approved Batch D execution note v0.2
//! (`docs/superpowers/plans/2026-05-04-phase-3-batch-d-risk-execution.md`).
//!
//! Pure-function `RiskGate::evaluate` consuming `OpportunityEvent`,
//! emits `RiskCheckedOpportunity` (passed; possibly size-clamped) or
//! `OpportunityAborted { category }`. Implements every cap from
//! `docs/specs/risk-budget.md` plus the ADR-006 zero-tolerance posture.
//!
//! Phase 3 is shadow-only per ADR-002 — no actual submission ever
//! increments the state counters. The state-mutating side
//! (`record_realized_loss`, `mark_live`, `day_rollover`) is shaped by
//! `RiskBudgetState` but the public mutator API lands in P4 alongside
//! `BundleRelay`. P3-D ships only `evaluate()` (read-side) plus a
//! test-only `with_state` ctor for seeding fixtures.

pub mod rkyv_compat;

use std::collections::HashMap;
use std::sync::Arc;

use alloy_primitives::{Address, U256};
use parking_lot::RwLock;
use rust_lmax_mev_opportunity::OpportunityEvent;
use serde::{Deserialize, Serialize};

// ----- Defaults from docs/specs/risk-budget.md (verbatim) ----------------

/// Default per-bundle absolute cap: 0.1 ETH = 1e17 wei.
pub const DEFAULT_PER_BUNDLE_MAX_NOTIONAL_WEI: u128 = 100_000_000_000_000_000;
/// Default per-bundle relative cap: 100 bps (1% of strategy capital).
pub const DEFAULT_PER_BUNDLE_MAX_NOTIONAL_RELATIVE_BPS: u16 = 100;
/// Default daily realized loss absolute cap: 0.05 ETH = 5e16 wei.
pub const DEFAULT_DAILY_REALIZED_LOSS_CAP_WEI: u128 = 50_000_000_000_000_000;
/// Default daily realized loss relative cap: 300 bps (3% of strategy capital).
pub const DEFAULT_DAILY_REALIZED_LOSS_CAP_RELATIVE_BPS: u16 = 300;
/// Default max gas spend per day: 0.03 ETH = 3e16 wei.
pub const DEFAULT_MAX_GAS_SPEND_PER_DAY_WEI: u128 = 30_000_000_000_000_000;
/// Default max concurrent live bundles: 1 (per spec).
pub const DEFAULT_MAX_CONCURRENT_LIVE_BUNDLES: u32 = 1;
/// Default max resubmits per opportunity: 2 (per spec).
pub const DEFAULT_MAX_RESUBMITS_PER_OPPORTUNITY: u32 = 2;
/// Default initial canary capital: 0.5 ETH = 5e17 wei.
pub const DEFAULT_INITIAL_CANARY_CAPITAL_WEI: u128 = 500_000_000_000_000_000;
/// Phase-3 placeholder gas-price proxy used for daily-gas-cap projection
/// (`gas_estimate * proxy ≈ projected wei spend`). Real per-block gas-price
/// feed lands in P5 per ADR-006. 30 gwei = 3e10 wei.
pub const DEFAULT_GAS_PRICE_PROXY_WEI: u128 = 30_000_000_000;

// ----- Config + state types ---------------------------------------------

/// Risk budget configuration. Every field maps to a row in
/// `docs/specs/risk-budget.md`. Constants `DEFAULT_*` mirror the spec
/// for one-line construction in tests + binaries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskBudgetConfig {
    pub per_bundle_max_notional_wei: U256,
    pub per_bundle_max_notional_relative_bps: u16,
    pub daily_realized_loss_cap_wei: U256,
    pub daily_realized_loss_cap_relative_bps: u16,
    pub max_gas_spend_per_day_wei: U256,
    pub max_concurrent_live_bundles: u32,
    pub max_resubmits_per_opportunity: u32,
    pub initial_canary_capital_wei: U256,
    pub gas_price_proxy_wei: U256,
    /// Optional. When `Some`, relative caps apply via `min(absolute,
    /// relative)`. When `None`, absolute caps always apply per spec
    /// "Absolute caps always apply, even when strategy capital is
    /// undefined or unavailable."
    pub strategy_capital_wei: Option<U256>,
}

impl RiskBudgetConfig {
    /// One-line spec-defaults constructor.
    pub fn defaults() -> Self {
        Self {
            per_bundle_max_notional_wei: U256::from(DEFAULT_PER_BUNDLE_MAX_NOTIONAL_WEI),
            per_bundle_max_notional_relative_bps: DEFAULT_PER_BUNDLE_MAX_NOTIONAL_RELATIVE_BPS,
            daily_realized_loss_cap_wei: U256::from(DEFAULT_DAILY_REALIZED_LOSS_CAP_WEI),
            daily_realized_loss_cap_relative_bps: DEFAULT_DAILY_REALIZED_LOSS_CAP_RELATIVE_BPS,
            max_gas_spend_per_day_wei: U256::from(DEFAULT_MAX_GAS_SPEND_PER_DAY_WEI),
            max_concurrent_live_bundles: DEFAULT_MAX_CONCURRENT_LIVE_BUNDLES,
            max_resubmits_per_opportunity: DEFAULT_MAX_RESUBMITS_PER_OPPORTUNITY,
            initial_canary_capital_wei: U256::from(DEFAULT_INITIAL_CANARY_CAPITAL_WEI),
            gas_price_proxy_wei: U256::from(DEFAULT_GAS_PRICE_PROXY_WEI),
            strategy_capital_wei: None,
        }
    }
}

/// Per-opportunity-instance key for resubmit counting. Encodes
/// direction via source/sink ordering (Codex 17:20:11 #DP-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpportunityKey {
    pub block_number: u64,
    pub source_pool: Address,
    pub sink_pool: Address,
}

impl OpportunityKey {
    pub fn from_event(opp: &OpportunityEvent) -> Self {
        Self {
            block_number: opp.block_number,
            source_pool: opp.source_pool.address,
            sink_pool: opp.sink_pool.address,
        }
    }
}

/// Mutable risk-budget runtime state. P3-D ships ONLY the read-side
/// (`RiskGate::evaluate` + `with_state` test ctor); the state-mutating
/// API (`record_realized_loss`, `mark_live`, `day_rollover`) lands in
/// P4 alongside `BundleRelay`.
#[derive(Debug, Clone)]
pub struct RiskBudgetState {
    pub daily_realized_loss_wei: U256,
    pub gas_spend_today_wei: U256,
    pub concurrent_live_bundles: u32,
    pub resubmits_per_opportunity: HashMap<OpportunityKey, u32>,
    pub day_started_unix_ns: u64,
    /// Initialized to `config.initial_canary_capital_wei`. P4 mutator
    /// decrements when a bundle lands.
    pub canary_remaining_wei: U256,
}

impl RiskBudgetState {
    /// Fresh-day state seeded from the canary capital config. Clock is
    /// caller-supplied so tests stay deterministic.
    pub fn new(config: &RiskBudgetConfig, day_started_unix_ns: u64) -> Self {
        Self {
            daily_realized_loss_wei: U256::ZERO,
            gas_spend_today_wei: U256::ZERO,
            concurrent_live_bundles: 0,
            resubmits_per_opportunity: HashMap::new(),
            day_started_unix_ns,
            canary_remaining_wei: config.initial_canary_capital_wei,
        }
    }
}

/// Risk-gate abort categories. `#[non_exhaustive]` so future caps land
/// additively without breaking downstream `match`.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AbortCategory {
    PerBundleNotionalCapExceeded,
    DailyLossCapWouldBeExceeded,
    DailyGasCapWouldBeExceeded,
    ConcurrencyCapExceeded,
    ResubmitCapExceeded,
    InsufficientCanaryCapital,
}

/// Risk-checked opportunity: the original event plus the (possibly
/// clamped) approved trade size. Per `event-model.md` derives the
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
pub struct RiskCheckedOpportunity {
    pub opportunity: OpportunityEvent,
    #[rkyv(with = crate::rkyv_compat::U256AsBytes)]
    pub size_wei: U256,
}

/// Aborted opportunity: the original event + the abort category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpportunityAborted {
    pub opportunity: OpportunityEvent,
    pub category: AbortCategory,
}

// ----- Engine ------------------------------------------------------------

/// Stateless wrt the engine itself; carries `Arc<RwLock<RiskBudgetState>>`
/// so a future P4 state mutator can share state with `evaluate()`.
pub struct RiskGate {
    config: RiskBudgetConfig,
    state: Arc<RwLock<RiskBudgetState>>,
}

impl RiskGate {
    /// Production constructor: seeds fresh state from the config.
    pub fn new(config: RiskBudgetConfig) -> Self {
        let state = RiskBudgetState::new(&config, 0);
        Self {
            config,
            state: Arc::new(RwLock::new(state)),
        }
    }

    /// Test / replay constructor: caller-supplied state.
    pub fn with_state(config: RiskBudgetConfig, state: RiskBudgetState) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(state)),
        }
    }

    pub fn config(&self) -> &RiskBudgetConfig {
        &self.config
    }

    /// Read-only view of the current state. Useful for tests and
    /// future P4 state mutators.
    pub fn state(&self) -> Arc<RwLock<RiskBudgetState>> {
        Arc::clone(&self.state)
    }

    /// Read-side evaluation. Applies all caps in order; returns
    /// `Ok(RiskCheckedOpportunity)` (possibly clamped) or
    /// `Err(OpportunityAborted { category })`.
    ///
    /// Cap-evaluation order (chosen so the cheapest checks short-circuit
    /// first):
    /// 1. `ConcurrencyCapExceeded` — pure state read.
    /// 2. `ResubmitCapExceeded` — single hashmap lookup keyed by
    ///    `OpportunityKey::from_event(opp)`.
    /// 3. Per-bundle notional sizing → clamp (never aborts; reduces
    ///    `requested_size` to `min(opp.optimal_amount_in_wei, effective_per_bundle_cap)`).
    /// 4. `InsufficientCanaryCapital` — clamped size > remaining canary.
    /// 5. `DailyLossCapWouldBeExceeded` — `state.daily_realized_loss_wei >= effective_loss_cap`.
    /// 6. `DailyGasCapWouldBeExceeded` — `state.gas_spend_today_wei +
    ///    opp.gas_estimate * config.gas_price_proxy_wei > config.max_gas_spend_per_day_wei`.
    pub fn evaluate(
        &self,
        opp: &OpportunityEvent,
    ) -> Result<RiskCheckedOpportunity, Box<OpportunityAborted>> {
        let aborted = |category| {
            Box::new(OpportunityAborted {
                opportunity: opp.clone(),
                category,
            })
        };
        let state = self.state.read();

        // 1. Concurrency cap.
        if state.concurrent_live_bundles >= self.config.max_concurrent_live_bundles {
            return Err(aborted(AbortCategory::ConcurrencyCapExceeded));
        }

        // 2. Resubmit cap.
        let key = OpportunityKey::from_event(opp);
        let resubmits = state
            .resubmits_per_opportunity
            .get(&key)
            .copied()
            .unwrap_or(0);
        if resubmits >= self.config.max_resubmits_per_opportunity {
            return Err(aborted(AbortCategory::ResubmitCapExceeded));
        }

        // 3. Per-bundle notional clamp. Effective cap = min(absolute,
        //    relative_if_strategy_capital_set). Per spec, absolute
        //    always applies; relative kicks in only when strategy
        //    capital is set.
        let effective_per_bundle_cap = match self.config.strategy_capital_wei {
            Some(strategy_cap) => {
                let relative = strategy_cap
                    * U256::from(self.config.per_bundle_max_notional_relative_bps)
                    / U256::from(10_000u32);
                self.config.per_bundle_max_notional_wei.min(relative)
            }
            None => self.config.per_bundle_max_notional_wei,
        };
        let size_wei = opp.optimal_amount_in_wei.min(effective_per_bundle_cap);

        // 4. Canary capital check.
        if size_wei > state.canary_remaining_wei {
            return Err(aborted(AbortCategory::InsufficientCanaryCapital));
        }

        // 5. Daily realized loss cap (tripwire on state value).
        let effective_loss_cap = match self.config.strategy_capital_wei {
            Some(strategy_cap) => {
                let relative = strategy_cap
                    * U256::from(self.config.daily_realized_loss_cap_relative_bps)
                    / U256::from(10_000u32);
                self.config.daily_realized_loss_cap_wei.min(relative)
            }
            None => self.config.daily_realized_loss_cap_wei,
        };
        if state.daily_realized_loss_wei >= effective_loss_cap {
            return Err(aborted(AbortCategory::DailyLossCapWouldBeExceeded));
        }

        // 6. Daily gas cap projection: gas_estimate * gas_price_proxy.
        let projected_gas_wei = U256::from(opp.gas_estimate) * self.config.gas_price_proxy_wei;
        let total_gas_after_this_bundle = state.gas_spend_today_wei + projected_gas_wei;
        if total_gas_after_this_bundle > self.config.max_gas_spend_per_day_wei {
            return Err(aborted(AbortCategory::DailyGasCapWouldBeExceeded));
        }

        Ok(RiskCheckedOpportunity {
            opportunity: opp.clone(),
            size_wei,
        })
    }
}
