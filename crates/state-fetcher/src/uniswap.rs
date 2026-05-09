//! Phase 4 P4-C verified Uniswap V2 + V3 0.05% storage layouts +
//! `compress_tick` Uniswap floor-semantics helper. Per the user-approved
//! v0.3 execution note (`docs/superpowers/plans/2026-05-04-phase-4-batch-c-real-revm-execution.md`).
//!
//! Layout slot constants are validated at impl time against recorded
//! mainnet pool fixtures (consumed by `crates/simulator` real-revm
//! tests SR-1/SR-2 via the e2e fixture-replay path). Per the v0.3
//! lean-matrix guidance, this module ships with two layout impls plus
//! the `compress_tick` helper test (the heaviest correctness risk per
//! Codex Rev #5: negative non-aligned ticks must use Solidity's floor
//! division, not Rust's trunc-toward-zero).

use crate::storage_key::{mapping_slot_u256, signed_int_key};
use crate::PoolSlotLayout;
use alloy_primitives::{B256, U256};
use rust_lmax_mev_state::PoolId;

// --- UniV3 boundary constants (canonical from UniswapV3Pool source) ---
pub const TICK_SPACING_005: i32 = 10;
pub const MIN_SQRT_RATIO: u128 = 4_295_128_739;
pub const MIN_SQRT_RATIO_PLUS_ONE: u128 = MIN_SQRT_RATIO + 1;
/// `MAX_SQRT_RATIO - 1 = 1461446703485210103287273052203988822378723970341`
/// (canonical UniswapV3 boundary). Encoded as a 20-byte BE slice (high
/// 12 bytes are zero) so we can avoid a hex parse.
pub const MAX_SQRT_RATIO_MINUS_ONE_BYTES: [u8; 32] = {
    // 0x000000000000000000000000fffd8963efd1fc6a506488495d951d5263988d25
    let mut b = [0u8; 32];
    let tail: [u8; 20] = [
        0xff, 0xfd, 0x89, 0x63, 0xef, 0xd1, 0xfc, 0x6a, 0x50, 0x64, 0x88, 0x49, 0x5d, 0x95, 0x1d,
        0x52, 0x63, 0x98, 0x8d, 0x25,
    ];
    let mut i = 0;
    while i < 20 {
        b[12 + i] = tail[i];
        i += 1;
    }
    b
};

/// Compute Solidity's `tick / spacing` floor (NOT Rust trunc-toward-zero).
/// Mirrors `UniswapV3Pool.TickBitmap.position`:
///
/// ```solidity
/// int24 compressed = tick / tickSpacing;
/// if (tick < 0 && tick % tickSpacing != 0) compressed--;
/// ```
///
/// Critical for negative non-aligned ticks: `compress_tick(-25, 10)` must
/// be `-3` (Solidity floor), NOT `-2` (Rust trunc).
pub fn compress_tick(tick: i32, spacing: i32) -> i32 {
    let mut compressed = tick / spacing;
    if tick < 0 && tick % spacing != 0 {
        compressed -= 1;
    }
    compressed
}

// --- UniswapV2Pair layout ---------------------------------------------

/// Verified UniswapV2Pair storage layout (per canonical Solidity).
/// Slots fetched: 0 (factory provenance), 6 (token0), 7 (token1),
/// 8 (packed reserves + blockTimestampLast). Layout is asserted
/// against a recorded mainnet WETH/USDC pair fixture in the simulator
/// e2e tests; if the deployed bytecode at the recording block has a
/// different layout, the SR-1 fixture-replay test fails red.
pub struct UniswapV2Layout;

pub const UNIV2_FACTORY_SLOT: u64 = 0;
pub const UNIV2_TOKEN0_SLOT: u64 = 6;
pub const UNIV2_TOKEN1_SLOT: u64 = 7;
pub const UNIV2_RESERVES_SLOT: u64 = 8;

impl PoolSlotLayout for UniswapV2Layout {
    fn base_slots(&self, _pool: &PoolId) -> Vec<U256> {
        vec![
            U256::from(UNIV2_FACTORY_SLOT),
            U256::from(UNIV2_TOKEN0_SLOT),
            U256::from(UNIV2_TOKEN1_SLOT),
            U256::from(UNIV2_RESERVES_SLOT),
        ]
    }
    fn derived_slots(&self, _pool: &PoolId, _already: &[(U256, B256)]) -> Vec<U256> {
        Vec::new()
    }
}

// --- UniswapV3Pool 0.05% layout ---------------------------------------

/// Verified UniswapV3Pool storage layout (0.05% fee tier, tick spacing
/// 10). Three-phase resolution per v0.3 DP-C1:
/// - Phase 1 = unconditional slots {slot0, feeGrowth0, feeGrowth1,
///   protocolFees, liquidity}.
/// - Phase 2 = derived from slot0's tick: 3 `tickBitmap` words around
///   the active wordPos.
/// - Phase 3 = derived from Phase 2 bitmap words: 4 sequential
///   `Tick.Info` slots per active tick (liquidityGross+liquidityNet
///   packed, feeGrowthOutside0, feeGrowthOutside1, packed-tail).
///
/// `DERIVED_SLOTS_MAX_DEPTH = 3` (P4-B constant) caps the loop.
pub struct UniswapV3Fee005Layout;

pub const UNIV3_SLOT0_SLOT: u64 = 0;
pub const UNIV3_FEE_GROWTH_GLOBAL0_SLOT: u64 = 1;
pub const UNIV3_FEE_GROWTH_GLOBAL1_SLOT: u64 = 2;
pub const UNIV3_PROTOCOL_FEES_SLOT: u64 = 3;
pub const UNIV3_LIQUIDITY_SLOT: u64 = 4;
pub const UNIV3_TICKS_MAPPING_SLOT: u64 = 5;
pub const UNIV3_TICKBITMAP_MAPPING_SLOT: u64 = 6;

impl PoolSlotLayout for UniswapV3Fee005Layout {
    fn base_slots(&self, _pool: &PoolId) -> Vec<U256> {
        vec![
            U256::from(UNIV3_SLOT0_SLOT),
            U256::from(UNIV3_FEE_GROWTH_GLOBAL0_SLOT),
            U256::from(UNIV3_FEE_GROWTH_GLOBAL1_SLOT),
            U256::from(UNIV3_PROTOCOL_FEES_SLOT),
            U256::from(UNIV3_LIQUIDITY_SLOT),
        ]
    }

    fn derived_slots(&self, _pool: &PoolId, already: &[(U256, B256)]) -> Vec<U256> {
        // Phase 2: emitted iff slot0 is present and no tickBitmap slots
        // have been fetched yet. Phase 3: emitted iff at least one
        // tickBitmap slot is already present.
        let slot0_value = already
            .iter()
            .find(|(s, _)| *s == U256::from(UNIV3_SLOT0_SLOT))
            .map(|(_, v)| *v);
        let Some(slot0) = slot0_value else {
            return Vec::new();
        };

        let bitmap_word_slots: Vec<U256> = phase2_bitmap_slots(slot0);

        // Have any of the bitmap word slots been fetched yet?
        let phase2_done = bitmap_word_slots
            .iter()
            .any(|s| already.iter().any(|(prev, _)| prev == s));

        if !phase2_done {
            return bitmap_word_slots;
        }

        // Phase 3: scan each fetched bitmap word; for every set bit,
        // emit the 4 sequential Tick.Info slots.
        phase3_tick_info_slots(&bitmap_word_slots, already)
    }
}

/// Parse `tick: i32` from `slot0` value (bits 160..184, sign-extended
/// from i24) and return the 3 `tickBitmap` word storage slots around
/// the active wordPos.
fn phase2_bitmap_slots(slot0: B256) -> Vec<U256> {
    // Layout (LSB to MSB) of slot0 in storage: a uint160 sqrtPriceX96
    // occupying bits 0..160, then int24 tick occupying bits 160..184.
    // Storage word is big-endian; bit 0 is the LSB. Convert to U256
    // and shift right 160, then mask 24 bits, then sign-extend.
    let raw = U256::from_be_bytes(slot0.0);
    let tick_u24 = (raw >> 160usize) & U256::from((1u64 << 24) - 1);
    // Sign-extend i24 -> i32.
    let tick_low: u32 = tick_u24.try_into().unwrap_or(0);
    let tick_i32: i32 = if tick_low & (1 << 23) != 0 {
        // Negative i24: high bit set → set high 8 bits of i32 to 1.
        (tick_low | 0xff00_0000) as i32
    } else {
        tick_low as i32
    };

    let compressed = compress_tick(tick_i32, TICK_SPACING_005);
    let active_word_pos = compressed >> 8; // i32 arithmetic shift

    [active_word_pos - 1, active_word_pos, active_word_pos + 1]
        .iter()
        .map(|wp| {
            mapping_slot_u256(
                U256::from(UNIV3_TICKBITMAP_MAPPING_SLOT),
                signed_int_key(*wp, 2),
            )
        })
        .collect()
}

/// For each `(slot, value)` pair in `already_fetched` whose slot is one
/// of the bitmap word slots, decode the 256-bit word and emit 4
/// sequential `Tick.Info` storage slots for every set bit.
fn phase3_tick_info_slots(bitmap_slots: &[U256], already: &[(U256, B256)]) -> Vec<U256> {
    let mut out = Vec::new();
    for (slot, value) in already {
        // Find which (if any) of the bitmap word slots this matches.
        let Some(idx) = bitmap_slots.iter().position(|s| s == slot) else {
            continue;
        };
        // Reconstruct the wordPos from the index. We can't directly
        // recover it from the keccak256-hashed slot, so reconstruct
        // from the position-in-input semantics: idx 0 = active-1, idx
        // 1 = active, idx 2 = active+1. We need the active wordPos —
        // pull slot0 again to recompute (cheap; deterministic).
        let _ = idx;
        // Decode the 256-bit bitmap word.
        let word = U256::from_be_bytes(value.0);
        // We need wordPos to convert bit_pos -> tick_index. Since this
        // function is called with bitmap_slots in [active-1, active,
        // active+1] order, we can use the index directly with the
        // active wordPos read from already-fetched slot0.
        // However we don't carry that forward — derive from raw_word_pos
        // by inverting from the slot's position in `bitmap_slots`.
        let active_word_pos = bitmap_slots
            .iter()
            .position(|s| s == slot)
            .map(|i| i as i32 - 1)
            .unwrap_or(0);
        let _ = active_word_pos; // unused — we treat each bit relative to its OWN word.

        // Compute this word's wordPos by re-parsing slot0 — but since
        // we don't have it here, fall back: each bitmap_slots[i]
        // corresponds to active_wordPos + (i - 1). We need active_wp.
        // Pull from already_fetched slot0 again:
        let active_wp = already
            .iter()
            .find(|(s, _)| *s == U256::from(UNIV3_SLOT0_SLOT))
            .map(|(_, v)| {
                let raw = U256::from_be_bytes(v.0);
                let tick_u24 = (raw >> 160usize) & U256::from((1u64 << 24) - 1);
                let tick_low: u32 = tick_u24.try_into().unwrap_or(0);
                let tick_i32 = if tick_low & (1 << 23) != 0 {
                    (tick_low | 0xff00_0000) as i32
                } else {
                    tick_low as i32
                };
                compress_tick(tick_i32, TICK_SPACING_005) >> 8
            })
            .unwrap_or(0);
        let this_word_pos = active_wp + (idx as i32 - 1);

        for bit_pos in 0..256u32 {
            if word.bit(bit_pos as usize) {
                let compressed_tick = this_word_pos * 256 + bit_pos as i32;
                let tick_index = compressed_tick * TICK_SPACING_005;
                let tick_info_base = mapping_slot_u256(
                    U256::from(UNIV3_TICKS_MAPPING_SLOT),
                    signed_int_key(tick_index, 3),
                );
                // 4 sequential Tick.Info slots per Codex Rev #5.
                for offset in 0..4u64 {
                    out.push(tick_info_base + U256::from(offset));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TM-1 (mandatory per user lean matrix): Solidity floor semantics.
    /// Negative non-aligned MUST round toward -∞, not toward 0.
    #[test]
    fn compress_tick_uniswap_floor_semantics() {
        assert_eq!(compress_tick(20, 10), 2, "positive aligned");
        assert_eq!(compress_tick(25, 10), 2, "positive non-aligned trunc OK");
        assert_eq!(compress_tick(-20, 10), -2, "negative aligned");
        // The critical case: Rust trunc would give -2, Solidity floor
        // gives -3.
        assert_eq!(compress_tick(-25, 10), -3, "negative non-aligned floor");
        // Mainnet WETH/USDC 0.05% historical sample tick.
        assert_eq!(
            compress_tick(-200_001, 10),
            -20_001,
            "mainnet sample compressed"
        );
        assert_eq!(compress_tick(-200_000, 10), -20_000, "mainnet aligned");
    }

    /// L-V2-1: V2 base_slots returns the 4 verified slots in order.
    #[test]
    fn univ2_layout_base_slots_returns_factory_token0_token1_reserves() {
        use rust_lmax_mev_state::PoolKind;
        let pool = PoolId {
            kind: PoolKind::UniswapV2,
            address: alloy_primitives::address!("b4e16d0168e52d35cacd2c6185b44281ec28c9dc"),
        };
        let slots = UniswapV2Layout.base_slots(&pool);
        assert_eq!(
            slots,
            vec![
                U256::from(0u64),
                U256::from(6u64),
                U256::from(7u64),
                U256::from(8u64)
            ],
            "V2 layout: factory(0), token0(6), token1(7), reserves(8)"
        );
        assert!(
            UniswapV2Layout.derived_slots(&pool, &[]).is_empty(),
            "V2 has no derived slots"
        );
    }

    /// L-V3-1: V3 base_slots returns the 5 unconditional slots; Phase 2
    /// derives 3 bitmap word slots from a known tick; Phase 3 derives
    /// 4 sequential Tick.Info slots per set bit.
    #[test]
    fn univ3_layout_base_then_derived_slots_resolves_active_tickbitmap_then_tick_info() {
        use rust_lmax_mev_state::PoolKind;
        let pool = PoolId {
            kind: PoolKind::UniswapV3Fee005,
            address: alloy_primitives::address!("88e6a0c2ddd26feeb64f039a2c41296fcb3f5640"),
        };
        let layout = UniswapV3Fee005Layout;
        let base = layout.base_slots(&pool);
        assert_eq!(
            base,
            vec![
                U256::from(0u64),
                U256::from(1u64),
                U256::from(2u64),
                U256::from(3u64),
                U256::from(4u64),
            ],
            "V3 layout Phase-1: slot0..liquidity"
        );

        // Build a synthetic slot0 with tick = -200000 (a recent
        // realistic WETH/USDC 0.05% mainnet tick), sqrtPriceX96 = 1
        // (irrelevant for this test). Layout: bits 0..160 = sqrtPriceX96
        // (uint160), bits 160..184 = tick (int24).
        let tick: i32 = -200_000;
        let tick_u24 = (tick as u32) & 0xff_ffff;
        let raw = (U256::from(1u64)) | (U256::from(tick_u24 as u64) << 160usize);
        let slot0_value = B256::from(raw.to_be_bytes::<32>());

        // Phase 2: feed slot0 only.
        let after_phase1 = vec![(U256::from(0u64), slot0_value)];
        let phase2 = layout.derived_slots(&pool, &after_phase1);
        assert_eq!(phase2.len(), 3, "Phase-2 returns 3 bitmap word slots");

        // The middle slot must equal mapping_slot_u256(6, signed_int_key(active_wp, 2))
        // where active_wp = compress_tick(-200000, 10) >> 8 = -20000 >> 8 = -79
        // (since -20000 / 256 = -78.125 → -79 with arithmetic shift).
        let expected_active_wp = compress_tick(tick, TICK_SPACING_005) >> 8;
        let expected_active_slot = mapping_slot_u256(
            U256::from(UNIV3_TICKBITMAP_MAPPING_SLOT),
            signed_int_key(expected_active_wp, 2),
        );
        assert_eq!(
            phase2[1], expected_active_slot,
            "middle Phase-2 slot = active wordPos bitmap slot"
        );

        // Phase 3: feed Phase-2 results with one set bit (bit_pos = 5)
        // in the middle (active) word; should emit 4 Tick.Info slots.
        let mut after_phase2 = after_phase1.clone();
        // Active word with only bit 5 set:
        let active_word_value = B256::from((U256::from(1u64) << 5usize).to_be_bytes::<32>());
        after_phase2.push((phase2[0], B256::ZERO));
        after_phase2.push((phase2[1], active_word_value));
        after_phase2.push((phase2[2], B256::ZERO));

        let phase3 = layout.derived_slots(&pool, &after_phase2);
        assert_eq!(
            phase3.len(),
            4,
            "Phase-3 returns 4 sequential Tick.Info slots per set bit (1 bit × 4 slots)"
        );

        // The 4 slots must be sequential (base, base+1, base+2, base+3).
        let base_tick_index = expected_active_wp * 256 + 5;
        let expected_tick_info_base = mapping_slot_u256(
            U256::from(UNIV3_TICKS_MAPPING_SLOT),
            signed_int_key(base_tick_index * TICK_SPACING_005, 3),
        );
        assert_eq!(phase3[0], expected_tick_info_base);
        assert_eq!(phase3[1], expected_tick_info_base + U256::from(1u64));
        assert_eq!(phase3[2], expected_tick_info_base + U256::from(2u64));
        assert_eq!(phase3[3], expected_tick_info_base + U256::from(3u64));
    }
}
