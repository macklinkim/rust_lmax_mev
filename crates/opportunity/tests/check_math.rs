//! Phase 3 P3-C tests for `OpportunityEngine::check` per the approved
//! Batch C execution note.
//!
//! All fixtures are deterministic small-integer values chosen so the
//! Q64 normalization arithmetic is exact and direction is unambiguous.
//! No live network, no fixture files.
//!
//! Test ladder:
//! - O-1 happy V2 cheap → V2→V3 source/sink ordering + positive profit.
//! - O-2 happy V3 cheap → V3→V2 ordering.
//! - O-3 boundary equal price → None.
//! - O-4 boundary sub-gas-floor delta → None.
//! - O-5 rkyv envelope round-trip on `OpportunityEvent`.
//! - O-6 boundary zero V2 reserves / zero V3 sqrt → None (no panic).
//! - O-7 determinism: identical inputs produce identical events.

use alloy_primitives::{Address, B256, U256};
use rust_lmax_mev_config::{IngressTokens, PoolKind};
use rust_lmax_mev_opportunity::{
    OpportunityEngine, OpportunityEvent, GAS_ESTIMATE_TWO_HOP_ARB, OPTIMAL_AMOUNT_IN_WEI,
};
use rust_lmax_mev_state::{PoolId, PoolState};
use rust_lmax_mev_types::{ChainContext, EventEnvelope, EventSource, PublishMeta};

// --- Test helpers ---------------------------------------------------------

fn weth() -> Address {
    "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
        .parse()
        .unwrap()
}
fn usdc() -> Address {
    "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
        .parse()
        .unwrap()
}
fn engine() -> OpportunityEngine {
    OpportunityEngine::new(&IngressTokens {
        weth: weth(),
        usdc: usdc(),
    })
}

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

fn ctx() -> ChainContext {
    ChainContext {
        chain_id: 1,
        block_number: 18_000_000,
        block_hash: [0xAB; 32],
    }
}

/// V2 reserves chosen so Q64 token1/token0 = `(reserve1 << 64) / reserve0`
/// equals exactly `ratio_q64`.
fn univ2_with_q64(ratio_q64: U256) -> PoolState {
    // pick reserve0 = 2^32 (well below U256 max even after << 64) and
    // derive reserve1 from the ratio: reserve1 = ratio_q64 * reserve0 >> 64.
    let reserve0 = U256::from(1u64 << 32);
    let reserve1 = (ratio_q64 * reserve0) >> 64;
    PoolState::UniV2 {
        reserve0,
        reserve1,
        block_timestamp_last: 1_700_000_000,
    }
}

/// V3 sqrt_price chosen so Q64 token1/token0 = `sqrt^2 >> 128` equals
/// the requested `ratio_q64`. We pick sqrt_price = sqrt(ratio_q64) << 32
/// so squared gives `ratio_q64 << 64` and the >>128 brings us back to
/// `ratio_q64 >> 64` ... actually: easier to pick sqrt directly.
///
/// For Q64 ratio R, we want sqrt^2 >> 128 = R, i.e., sqrt^2 = R << 128.
/// So sqrt = sqrt(R << 128) = sqrt(R) << 64.
///
/// To keep the math exact in tests, parameterize `r_root` so that
/// ratio_q64 = `r_root^2`; then sqrt = `r_root << 64`. Saves a sqrt.
fn univ3_with_root_shift64(r_root: u128) -> PoolState {
    let sqrt_price_x96 = U256::from(r_root) << 64;
    PoolState::UniV3 {
        sqrt_price_x96,
        tick: 0,
        liquidity: 1_000_000_000_000_000_000,
    }
}

/// Compute the exact Q64 ratio that `univ3_with_root_shift64(r_root)`
/// will report through `pool_price_q64`.
fn univ3_ratio_q64(r_root: u128) -> U256 {
    U256::from(r_root) * U256::from(r_root)
}

// --- Tests ---------------------------------------------------------------

/// O-1 happy V2 cheap: V2 has higher Q64 ratio than V3 → V2 has more
/// WETH per USDC → WETH cheaper on V2 → buy on V2, sell on V3.
/// `source_pool = V2`, `sink_pool = V3`, profit > 0.
#[test]
fn v2_cheap_v3_expensive_emits_v2_to_v3_opportunity() {
    let v3_root: u128 = 1 << 32; // arbitrary; r_root = 2^32
    let v3_ratio = univ3_ratio_q64(v3_root); // = 2^64
    let v2_ratio = v3_ratio + (v3_ratio >> 6); // V2 ratio ~1.56% higher (well above 24bps gas floor)

    let event = engine()
        .check(
            &ctx(),
            &pool_v2(),
            &univ2_with_q64(v2_ratio),
            &pool_v3(),
            &univ3_with_root_shift64(v3_root),
        )
        .expect("V2 cheap should emit Some(opportunity)");

    assert_eq!(event.source_pool, pool_v2(), "buy side must be V2");
    assert_eq!(event.sink_pool, pool_v3(), "sell side must be V3");
    assert_eq!(event.gas_estimate, GAS_ESTIMATE_TWO_HOP_ARB);
    assert_eq!(
        event.optimal_amount_in_wei,
        U256::from(OPTIMAL_AMOUNT_IN_WEI)
    );
    assert!(
        event.expected_profit_wei > U256::ZERO,
        "linear profit approximation must be positive for a clearly profitable spread"
    );
    assert_eq!(event.block_number, ctx().block_number);
    assert_eq!(event.block_hash, B256::from(ctx().block_hash));
}

/// O-2 happy V3 cheap (symmetric): V3 has higher Q64 ratio than V2 →
/// `source_pool = V3`, `sink_pool = V2`.
#[test]
fn v3_cheap_v2_expensive_emits_v3_to_v2_opportunity() {
    let v3_root: u128 = 1 << 32;
    let v3_ratio = univ3_ratio_q64(v3_root); // = 2^64
    let v2_ratio = v3_ratio - (v3_ratio >> 6); // V2 ratio lower → V3 cheaper

    let event = engine()
        .check(
            &ctx(),
            &pool_v2(),
            &univ2_with_q64(v2_ratio),
            &pool_v3(),
            &univ3_with_root_shift64(v3_root),
        )
        .expect("V3 cheap should emit Some(opportunity)");

    assert_eq!(event.source_pool, pool_v3(), "buy side must be V3");
    assert_eq!(event.sink_pool, pool_v2(), "sell side must be V2");
    assert!(event.expected_profit_wei > U256::ZERO);
}

/// O-3 boundary equal Q64 prices → None.
#[test]
fn equal_prices_returns_none() {
    let v3_root: u128 = 1 << 32;
    let v3_ratio = univ3_ratio_q64(v3_root);

    assert!(engine()
        .check(
            &ctx(),
            &pool_v2(),
            &univ2_with_q64(v3_ratio),
            &pool_v3(),
            &univ3_with_root_shift64(v3_root),
        )
        .is_none());
}

/// O-4 boundary sub-gas-floor delta → None. The threshold is
/// `cheap_price >> 12`; any delta strictly below should be filtered.
#[test]
fn sub_gas_floor_delta_returns_none() {
    let v3_root: u128 = 1 << 32;
    let v3_ratio = univ3_ratio_q64(v3_root);
    // Add ~1/8192 = ~12 bps, well below the 1/4096 ≈ 24 bps threshold.
    let v2_ratio = v3_ratio + (v3_ratio >> 13);

    assert!(engine()
        .check(
            &ctx(),
            &pool_v2(),
            &univ2_with_q64(v2_ratio),
            &pool_v3(),
            &univ3_with_root_shift64(v3_root),
        )
        .is_none());
}

/// O-5 rkyv envelope round-trip on `OpportunityEvent`. Mirrors the
/// P3-A ingress/state tests; proves the spec-compliance derives plus
/// per-crate `rkyv_compat` adapters work end-to-end.
#[test]
fn opportunity_event_envelope_round_trips() {
    let v3_root: u128 = 1 << 32;
    let v3_ratio = univ3_ratio_q64(v3_root);
    let v2_ratio = v3_ratio + (v3_ratio >> 6);
    let event = engine()
        .check(
            &ctx(),
            &pool_v2(),
            &univ2_with_q64(v2_ratio),
            &pool_v3(),
            &univ3_with_root_shift64(v3_root),
        )
        .unwrap();

    let meta = PublishMeta {
        source: EventSource::OpportunityEngine,
        chain_context: ctx(),
        event_version: 1,
        correlation_id: 7,
    };
    let envelope = EventEnvelope::seal(meta, event.clone(), 99, 1_700_000_000_000_000_000).unwrap();
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&envelope).expect("rkyv serialize");
    let round_tripped: EventEnvelope<OpportunityEvent> =
        rkyv::from_bytes::<EventEnvelope<OpportunityEvent>, rkyv::rancor::Error>(&bytes)
            .expect("rkyv deserialize");
    assert_eq!(envelope, round_tripped);
    assert_eq!(round_tripped.payload(), &event);
}

/// O-6 boundary zero / insufficient liquidity → None (no panic).
#[test]
fn zero_liquidity_pools_return_none_without_panic() {
    let zero_v2 = PoolState::UniV2 {
        reserve0: U256::ZERO,
        reserve1: U256::from(1_000_000_000u64),
        block_timestamp_last: 1,
    };
    let zero_v3 = PoolState::UniV3 {
        sqrt_price_x96: U256::ZERO,
        tick: 0,
        liquidity: 0,
    };
    let healthy_v3 = univ3_with_root_shift64(1 << 32);

    // Zero V2 reserve0 → V2 price is None → engine returns None.
    assert!(engine()
        .check(&ctx(), &pool_v2(), &zero_v2, &pool_v3(), &healthy_v3)
        .is_none());

    // Zero V3 sqrt → V3 price is None → engine returns None.
    let healthy_v2 = univ2_with_q64(univ3_ratio_q64(1 << 32));
    assert!(engine()
        .check(&ctx(), &pool_v2(), &healthy_v2, &pool_v3(), &zero_v3)
        .is_none());
}

/// O-7 determinism: identical inputs produce byte-identical events.
#[test]
fn identical_inputs_produce_identical_events() {
    let v3_root: u128 = 1 << 32;
    let v3_ratio = univ3_ratio_q64(v3_root);
    let v2_ratio = v3_ratio + (v3_ratio >> 6);
    let v2 = univ2_with_q64(v2_ratio);
    let v3 = univ3_with_root_shift64(v3_root);
    let e1 = engine()
        .check(&ctx(), &pool_v2(), &v2, &pool_v3(), &v3)
        .unwrap();
    let e2 = engine()
        .check(&ctx(), &pool_v2(), &v2, &pool_v3(), &v3)
        .unwrap();
    assert_eq!(e1, e2);
}
