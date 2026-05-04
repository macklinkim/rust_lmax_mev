//! Phase 3 P3-C arbitrage opportunity engine per the approved Batch C
//! execution note (`docs/superpowers/plans/2026-05-04-phase-3-batch-c-
//! opportunity-execution.md`).
//!
//! Pure-function math crate: given the latest `PoolState` for the
//! WETH/USDC pair on Uniswap V2 + Uniswap V3 0.05% at the same
//! `(block_number, block_hash)`, compute the cross-venue arbitrage and
//! emit an [`OpportunityEvent`] iff a positive-EV opportunity exists
//! after the conservative gas floor.
//!
//! No I/O, no spawn, no bus wiring. P3-D wires this engine into the
//! pipeline; P3-B's deferred topology question (multi-consumer fanout
//! on `ingress_bus` + StateEngine driver consumer) is also a P3-D
//! concern, NOT a P3-C concern.
//!
//! # Math (thin-path, P3-C)
//!
//! Both V2 and V3 pools in the WETH/USDC pair use canonical
//! ascending-address token ordering, which on Ethereum places
//! `token0 = USDC` (`0xA0b8...`) and `token1 = WETH` (`0xC02a...`).
//! Both pools therefore expose the same dimensionless ratio
//! `token1/token0` = WETH-per-USDC in raw units. Higher ratio = more
//! WETH per USDC = WETH cheaper on that pool.
//!
//! The engine normalizes each pool's `token1/token0` ratio to a
//! Q64 fixed-point integer (`value * 2^64`) so it can compare V2 and
//! V3 prices in pure U256 arithmetic without a floating-point cast:
//!
//! - **V2**: `price_q64 = (reserve1 << 64) / reserve0`.
//! - **V3**: `price_q64 = (sqrt_price_x96)^2 >> 128`. Derivation:
//!   `sqrt_price_x96 / 2^96 = sqrt(token1/token0)`, so squaring and
//!   shifting from Q192 down to Q64 yields the comparable form.
//!
//! Direction: pool with the *higher* Q64 price has the *cheaper* WETH
//! → that pool is the `source_pool` (buy side); the other is the
//! `sink_pool`. The diff must exceed `cheap_price >> 12` (~24 bps) to
//! clear the gas-floor noise threshold; below that the engine returns
//! `None` and `crates/risk` (P3-D) never sees the candidate.
//!
//! Optimal amount-in is approximated as a fixed `0.01 ETH` thin-path
//! probe (`OPTIMAL_AMOUNT_IN_WEI`). Exact CPMM closed-form sizing is
//! P5 work per ADR-006; downstream `crates/simulator` (P3-E) verifies
//! the predicted profit with `revm` and rejects via the
//! `ADR-006::Profitability` mismatch category if the heuristic was
//! wrong. Expected profit is the linear approximation
//! `amount_in * price_diff_q64 / cheap_price_q64`.

pub mod rkyv_compat;

use alloy_primitives::{Address, B256, U256};
use rust_lmax_mev_config::IngressTokens;
use rust_lmax_mev_state::{PoolId, PoolState};
use rust_lmax_mev_types::ChainContext;
use serde::{Deserialize, Serialize};

/// Phase 3 P3-C two-hop arb gas estimate (rough thin-path constant).
/// ADR-006 covers refinement in Phase 5; for now, a conservative value
/// suffices since `crates/simulator` (P3-E) verifies via `revm` and
/// `crates/risk` (P3-D) re-checks the size.
pub const GAS_ESTIMATE_TWO_HOP_ARB: u64 = 350_000;

/// Heuristic optimal arb size used as a thin-path probe (0.01 ETH in
/// WETH wei). True closed-form optimum is P5 work; the downstream revm
/// sim validates and the risk engine caps regardless.
pub const OPTIMAL_AMOUNT_IN_WEI: u128 = 10_000_000_000_000_000; // 1e16

/// Q64 shift used for the normalized `token1/token0` price representation.
const PRICE_Q64_SHIFT: u32 = 64;

/// Stateless pure-function engine. Construct once per process from the
/// `IngressTokens` config; `check(...)` is `&self` and allocation-free
/// beyond the returned event struct.
pub struct OpportunityEngine {
    weth: Address,
    usdc: Address,
}

impl OpportunityEngine {
    pub fn new(tokens: &IngressTokens) -> Self {
        Self {
            weth: tokens.weth,
            usdc: tokens.usdc,
        }
    }

    pub fn weth(&self) -> Address {
        self.weth
    }
    pub fn usdc(&self) -> Address {
        self.usdc
    }

    /// Pure: returns `Some(event)` iff cross-venue arb has positive EV
    /// after the gas-floor noise threshold; `None` otherwise.
    ///
    /// `None` is returned for any of:
    /// - both pools share the same `address` (caller passed the same pool twice)
    /// - either pool has zero / insufficient liquidity (would divide-by-zero or overflow)
    /// - prices are exactly equal
    /// - price divergence is below the gas-floor threshold (`cheap_price >> 12`)
    /// - linear-approximation profit rounds to zero
    ///
    /// All `None` paths are silent and safe — no panic, no `Result`-ish
    /// errors. Caller bugs (same-pool, mismatched-block) are silent
    /// `None` rather than typed errors per ADR-001 thin-path policy.
    pub fn check(
        &self,
        chain_context: &ChainContext,
        pool_a: &PoolId,
        state_a: &PoolState,
        pool_b: &PoolId,
        state_b: &PoolState,
    ) -> Option<OpportunityEvent> {
        // Caller-bug guards: same pool address, identical pool kinds
        // are still allowed but same address is meaningless.
        if pool_a.address == pool_b.address {
            return None;
        }

        let price_a = pool_price_q64(state_a)?;
        let price_b = pool_price_q64(state_b)?;

        // Higher Q64 price = more WETH per USDC = WETH cheaper there
        // → that pool is the `source_pool` (buy side).
        let (cheap_pool, cheap_price, expensive_pool, expensive_price) = match price_a.cmp(&price_b)
        {
            std::cmp::Ordering::Greater => (pool_a, price_a, pool_b, price_b),
            std::cmp::Ordering::Less => (pool_b, price_b, pool_a, price_a),
            std::cmp::Ordering::Equal => return None,
        };

        let price_diff = cheap_price - expensive_price;

        // Gas-floor noise threshold: at least ~24 bps relative spread
        // (cheap_price / 4096) before the candidate clears the engine.
        let min_diff = cheap_price >> 12;
        if price_diff < min_diff {
            return None;
        }

        // Heuristic amount-in (thin-path probe; P5 refines).
        let amount_in_wei = U256::from(OPTIMAL_AMOUNT_IN_WEI);

        // Linear approximation: profit ≈ amount_in * (price_diff / cheap_price).
        // Both numerator and denominator are Q64; the ratio cancels the
        // shift, leaving the approximate profit in WETH wei. Returns
        // None if the divisor is zero or the result would round to 0.
        let profit_wei = amount_in_wei
            .checked_mul(price_diff)?
            .checked_div(cheap_price)
            .filter(|p| !p.is_zero())?;

        Some(OpportunityEvent {
            block_number: chain_context.block_number,
            block_hash: B256::from(chain_context.block_hash),
            source_pool: cheap_pool.clone(),
            sink_pool: expensive_pool.clone(),
            optimal_amount_in_wei: amount_in_wei,
            expected_profit_wei: profit_wei,
            gas_estimate: GAS_ESTIMATE_TWO_HOP_ARB,
        })
    }
}

/// Computes the Q64 fixed-point representation of `token1/token0` in
/// raw token units for any supported `PoolState`.
///
/// Returns `None` if liquidity is insufficient (zero reserves on V2,
/// zero `sqrt_price_x96` on V3) or if the V3 squaring overflows U256
/// (only possible for unrealistically large `sqrt_price_x96` values
/// well outside the WETH/USDC range).
fn pool_price_q64(state: &PoolState) -> Option<U256> {
    match state {
        PoolState::UniV2 {
            reserve0, reserve1, ..
        } => {
            if reserve0.is_zero() {
                return None;
            }
            // (reserve1 << 64) / reserve0
            let shifted = reserve1.checked_shl(PRICE_Q64_SHIFT as usize)?;
            Some(shifted / *reserve0)
        }
        PoolState::UniV3 { sqrt_price_x96, .. } => {
            if sqrt_price_x96.is_zero() {
                return None;
            }
            // sqrt^2 >> (192 - 64) = sqrt^2 >> 128
            let sq = sqrt_price_x96.checked_mul(*sqrt_price_x96)?;
            Some(sq >> 128)
        }
    }
}

/// Phase 3 P3-C cross-venue arbitrage candidate. Per
/// `docs/specs/event-model.md` the payload type derives the spec-
/// mandated `Clone, Debug, PartialEq, Eq, rkyv::{Archive, Serialize,
/// Deserialize}, serde::{Serialize, Deserialize}`. `B256` and `U256`
/// fields use the per-crate `rkyv_compat` adapters (same pattern as
/// `crates/ingress` and `crates/state` from P3-A).
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
pub struct OpportunityEvent {
    pub block_number: u64,
    #[rkyv(with = crate::rkyv_compat::B256AsBytes)]
    pub block_hash: B256,
    pub source_pool: PoolId,
    pub sink_pool: PoolId,
    #[rkyv(with = crate::rkyv_compat::U256AsBytes)]
    pub optimal_amount_in_wei: U256,
    #[rkyv(with = crate::rkyv_compat::U256AsBytes)]
    pub expected_profit_wei: U256,
    pub gas_estimate: u64,
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum OpportunityError {
    /// The two PoolIds reference the same pool — caller bug. Reserved
    /// for future Result-returning APIs; current `check()` returns None.
    #[error("source and sink pools must differ; both are {0}")]
    SamePool(Address),

    /// Caller passed two pools at different block heights — undefined
    /// behavior for cross-venue comparison. Reserved for future API.
    #[error("pool snapshots are at different blocks: {a} vs {b}")]
    BlockMismatch { a: u64, b: u64 },
}
