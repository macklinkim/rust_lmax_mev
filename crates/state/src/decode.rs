//! Hand-rolled ABI decoders per P2-B v0.4 Risk Decision 5.
//!
//! All return values are 32-byte big-endian words; unsigned ints
//! left-pad zeros, signed ints sign-extend. Decoders validate padding/
//! sign-extension and return `StateError::Decode(_)` on violation
//! rather than panic, per the chosen error-vs-panic posture.

use alloy_primitives::U256;

use crate::{PoolState, StateError};

/// V2 `getReserves()` returns 3×32 = 96 bytes:
///   word 0 = uint112 reserve0 (high 14 bytes MUST be zero)
///   word 1 = uint112 reserve1 (high 14 bytes MUST be zero)
///   word 2 = uint32 blockTimestampLast (high 28 bytes MUST be zero)
pub fn decode_get_reserves(bytes: &[u8]) -> Result<PoolState, StateError> {
    if bytes.len() < 96 {
        return Err(StateError::Decode(format!(
            "getReserves: expected ≥96 bytes, got {}",
            bytes.len()
        )));
    }
    validate_uint_padding(bytes, 0, 14, "getReserves.reserve0")?;
    let reserve0 = U256::from_be_slice(&bytes[0..32]);
    validate_uint_padding(bytes, 32, 14, "getReserves.reserve1")?;
    let reserve1 = U256::from_be_slice(&bytes[32..64]);
    validate_uint_padding(bytes, 64, 28, "getReserves.blockTimestampLast")?;
    let block_timestamp_last = u32::from_be_bytes(bytes[92..96].try_into().unwrap());
    Ok(PoolState::UniV2 {
        reserve0,
        reserve1,
        block_timestamp_last,
    })
}

/// V3 `slot0()` returns 7×32 = 224 bytes; only words 0 and 1 are
/// persisted (the rest — observationIndex, observationCardinality,
/// observationCardinalityNext, feeProtocol, unlocked — are intentionally
/// read past + ignored).
///   word 0 = uint160 sqrtPriceX96 (high 12 bytes MUST be zero)
///   word 1 = int24 tick (signed, sign-extended): low 3 bytes = magnitude;
///            high 29 bytes MUST be all 0x00 for positive or all 0xff
///            for negative two's-complement; sign-extended to i32.
pub fn decode_slot0(bytes: &[u8]) -> Result<(U256, i32), StateError> {
    if bytes.len() < 64 {
        return Err(StateError::Decode(format!(
            "slot0: expected ≥64 bytes, got {}",
            bytes.len()
        )));
    }
    validate_uint_padding(bytes, 0, 12, "slot0.sqrtPriceX96")?;
    let sqrt_price_x96 = U256::from_be_slice(&bytes[0..32]);

    // int24 tick at word 1
    let tick_word = &bytes[32..64];
    let low3: [u8; 3] = tick_word[29..32].try_into().unwrap();
    let is_negative = low3[0] & 0x80 != 0;
    let expected_pad: u8 = if is_negative { 0xff } else { 0x00 };
    if !tick_word[..29].iter().all(|&b| b == expected_pad) {
        return Err(StateError::Decode(format!(
            "slot0.tick: malformed sign extension; low3 byte 0 = {:#04x} (sign bit {}), high 29 bytes must be all {:#04x}",
            low3[0],
            if is_negative { "set" } else { "clear" },
            expected_pad
        )));
    }
    // Sign-extend i24 → i32: prepend the matching 0xff (negative) or
    // 0x00 (positive) byte.
    let tick = i32::from_be_bytes([expected_pad, low3[0], low3[1], low3[2]]);
    Ok((sqrt_price_x96, tick))
}

/// V3 `liquidity()` returns 32 bytes: uint128 left-padded — high 16
/// bytes MUST be zero, low 16 bytes BE = u128 value.
pub fn decode_liquidity(bytes: &[u8]) -> Result<u128, StateError> {
    if bytes.len() < 32 {
        return Err(StateError::Decode(format!(
            "liquidity: expected ≥32 bytes, got {}",
            bytes.len()
        )));
    }
    validate_uint_padding(bytes, 0, 16, "liquidity")?;
    Ok(u128::from_be_bytes(bytes[16..32].try_into().unwrap()))
}

fn validate_uint_padding(
    bytes: &[u8],
    word_offset: usize,
    high_zero_count: usize,
    label: &str,
) -> Result<(), StateError> {
    if bytes[word_offset..word_offset + high_zero_count]
        .iter()
        .any(|&b| b != 0)
    {
        return Err(StateError::Decode(format!(
            "{label}: high {high_zero_count} bytes must be zero (uint padding violation)"
        )));
    }
    Ok(())
}
