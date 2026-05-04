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

#![allow(dead_code)] // Phase 3 P3-C scaffold — body lands in commit 3.

pub mod rkyv_compat;

use alloy_primitives::{Address, B256, U256};
use rust_lmax_mev_config::IngressTokens;
use rust_lmax_mev_state::{PoolId, PoolState};
use rust_lmax_mev_types::ChainContext;
use serde::{Deserialize, Serialize};

/// Phase 3 P3-C two-hop arb gas floor (rough thin-path estimate).
/// ADR-006 covers refinement in Phase 5; for now, a conservative
/// constant suffices since `crates/simulator` (P3-E) verifies via
/// `revm` and `crates/risk` (P3-D) re-checks the size.
pub const GAS_ESTIMATE_TWO_HOP_ARB: u64 = 350_000;

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
    /// after the gas floor; `None` otherwise. Body lands in commit 3.
    pub fn check(
        &self,
        _chain_context: &ChainContext,
        _pool_a: &PoolId,
        _state_a: &PoolState,
        _pool_b: &PoolId,
        _state_b: &PoolState,
    ) -> Option<OpportunityEvent> {
        None
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
    /// The two PoolIds reference the same pool — caller bug.
    #[error("source and sink pools must differ; both are {0}")]
    SamePool(Address),

    /// Caller passed two pools at different block heights — undefined
    /// behavior for cross-venue comparison.
    #[error("pool snapshots are at different blocks: {a} vs {b}")]
    BlockMismatch { a: u64, b: u64 },
}
