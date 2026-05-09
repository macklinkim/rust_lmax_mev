//! Phase 4 P4-C real-Uniswap swap calldata builders + UniV2
//! getAmountOut formula. Per the user-approved v0.3 execution note
//! DP-C5a + DP-C6.
//!
//! - `SELECTOR_V2_SWAP = 0x022c0d9f` — `swap(uint amount0Out, uint amount1Out, address to, bytes data)`.
//! - `SELECTOR_V3_SWAP = 0x128acb08` — `swap(address recipient, bool zeroForOne, int256 amountSpecified, uint160 sqrtPriceLimitX96, bytes data)`.
//! - `SELECTOR_V3_CALLBACK = 0xfa461e33` — `uniswapV3SwapCallback(int256, int256, bytes)`.
//! - V2 amount-out: canonical fee-adjusted `(in*997*reserve_out)/(reserve_in*1000+in*997)`.
//! - V3 boundary helpers: `MIN_SQRT_RATIO+1` / `MAX_SQRT_RATIO-1` for unrestricted-direction swap probes.

use alloy_primitives::{Address, Bytes, I256, U256};
use rust_lmax_mev_state_fetcher::uniswap::{
    MAX_SQRT_RATIO_MINUS_ONE_BYTES, MIN_SQRT_RATIO_PLUS_ONE,
};

pub const SELECTOR_V2_SWAP: [u8; 4] = [0x02, 0x2c, 0x0d, 0x9f];
pub const SELECTOR_V3_SWAP: [u8; 4] = [0x12, 0x8a, 0xcb, 0x08];
pub const SELECTOR_V3_CALLBACK: [u8; 4] = [0xfa, 0x46, 0x1e, 0x33];

/// UniV2 fee-adjusted output amount. Reverts on zero `reserve_in` or
/// overflow.
pub fn uniswap_v2_get_amount_out(amount_in: U256, reserve_in: U256, reserve_out: U256) -> U256 {
    let amount_in_with_fee = amount_in
        .checked_mul(U256::from(997u64))
        .expect("amount_in * 997 overflow");
    let numerator = amount_in_with_fee
        .checked_mul(reserve_out)
        .expect("numerator overflow");
    let denominator = reserve_in
        .checked_mul(U256::from(1000u64))
        .expect("reserve_in * 1000 overflow")
        .checked_add(amount_in_with_fee)
        .expect("denominator overflow");
    numerator / denominator
}

/// `MIN_SQRT_RATIO + 1` as `U256` for `zero_for_one = true` swap-probe
/// price limit.
pub fn min_sqrt_ratio_plus_one() -> U256 {
    U256::from(MIN_SQRT_RATIO_PLUS_ONE)
}

/// `MAX_SQRT_RATIO - 1` as `U256` for `zero_for_one = false` swap-probe
/// price limit.
pub fn max_sqrt_ratio_minus_one() -> U256 {
    U256::from_be_bytes(MAX_SQRT_RATIO_MINUS_ONE_BYTES)
}

fn write_u256_be(buf: &mut Vec<u8>, v: U256) {
    buf.extend_from_slice(&v.to_be_bytes::<32>());
}

fn write_address_padded(buf: &mut Vec<u8>, a: Address) {
    buf.extend_from_slice(&[0u8; 12]);
    buf.extend_from_slice(a.as_slice());
}

/// V2 `swap(amount0Out, amount1Out, to, bytes data)` calldata.
/// `data` is empty by convention (V2 has no callback; mock router
/// pre-funds the pool optimistically before invoking swap).
pub fn univ2_swap_calldata(
    amount0_out: U256,
    amount1_out: U256,
    to: Address,
    data: &[u8],
) -> Bytes {
    let mut buf = Vec::with_capacity(4 + 32 * 5 + data.len());
    buf.extend_from_slice(&SELECTOR_V2_SWAP);
    write_u256_be(&mut buf, amount0_out);
    write_u256_be(&mut buf, amount1_out);
    write_address_padded(&mut buf, to);
    // `data` offset = 4 head words × 32 = 128 bytes.
    write_u256_be(&mut buf, U256::from(0x80u64));
    write_u256_be(&mut buf, U256::from(data.len() as u64));
    buf.extend_from_slice(data);
    // Right-pad data to a 32-byte multiple per ABI rules.
    let pad = (32 - (data.len() % 32)) % 32;
    buf.extend(std::iter::repeat(0u8).take(pad));
    Bytes::from(buf)
}

/// V3 `swap(recipient, zeroForOne, amountSpecified, sqrtPriceLimitX96, data)` calldata.
/// `amount_specified > 0` = exact-input semantics. `data` is the V3
/// callback payload (mock router decodes the input-token address from
/// it).
pub fn univ3_swap_calldata(
    recipient: Address,
    zero_for_one: bool,
    amount_specified: I256,
    sqrt_price_limit_x96: U256,
    data: &[u8],
) -> Bytes {
    let mut buf = Vec::with_capacity(4 + 32 * 6 + data.len());
    buf.extend_from_slice(&SELECTOR_V3_SWAP);
    write_address_padded(&mut buf, recipient);
    // bool: low byte 0/1, rest zero.
    let mut bool_word = [0u8; 32];
    bool_word[31] = u8::from(zero_for_one);
    buf.extend_from_slice(&bool_word);
    // int256 BE; I256 already encodes signed.
    buf.extend_from_slice(&amount_specified.to_be_bytes::<32>());
    write_u256_be(&mut buf, sqrt_price_limit_x96);
    // data offset = 5 head words × 32 = 160 bytes.
    write_u256_be(&mut buf, U256::from(0xa0u64));
    write_u256_be(&mut buf, U256::from(data.len() as u64));
    buf.extend_from_slice(data);
    let pad = if data.is_empty() {
        0
    } else {
        (32 - (data.len() % 32)) % 32
    };
    buf.extend(std::iter::repeat(0u8).take(pad));
    Bytes::from(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CD-1 (combined CD-1 + CD-3 per lean-matrix): V2 calldata starts
    /// with the swap selector and contains the right amount/recipient
    /// fields; uniswap_v2_get_amount_out matches a hand-computed value
    /// for a known reserve pair.
    #[test]
    fn univ2_swap_calldata_and_get_amount_out_match_canonical_formula() {
        // amount_in=10, reserve_in=1000, reserve_out=2000.
        // amount_in_with_fee = 10*997 = 9970
        // numerator = 9970 * 2000 = 19_940_000
        // denominator = 1000*1000 + 9970 = 1_009_970
        // out = 19_940_000 / 1_009_970 = 19 (truncated)
        let out =
            uniswap_v2_get_amount_out(U256::from(10u64), U256::from(1000u64), U256::from(2000u64));
        assert_eq!(out, U256::from(19u64), "V2 fee-adjusted formula");

        let to = Address::from([0x33; 20]);
        let cd = univ2_swap_calldata(U256::ZERO, out, to, &[]);
        assert_eq!(&cd[..4], &SELECTOR_V2_SWAP, "selector");
        // amount0Out at bytes 4..36 = 0
        assert!(cd[4..36].iter().all(|b| *b == 0), "amount0Out zeroed");
        // amount1Out at bytes 36..68 = 19 (low byte)
        assert_eq!(cd[36 + 31], 19, "amount1Out low byte");
        // to at bytes 68..100; address occupies low 20 of that word.
        assert_eq!(&cd[100 - 20..100], to.as_slice(), "to address");
        // data offset at bytes 100..132 = 0x80
        assert_eq!(cd[100 + 31], 0x80, "data offset");
        // data length at bytes 132..164 = 0
        assert_eq!(cd[132 + 31], 0, "data length");
        assert_eq!(cd.len(), 4 + 5 * 32, "no data tail");
    }

    /// CD-2: V3 calldata selector + sqrt_price_limit choices match the
    /// canonical UniV3 boundary constants.
    #[test]
    fn univ3_swap_calldata_uses_boundary_sqrt_price_limits() {
        let recipient = Address::from([0x33; 20]);
        let amount_in = U256::from(1_000_000_000_000_000_000u64); // 1 ETH
        let cd = univ3_swap_calldata(
            recipient,
            true, // zero_for_one
            I256::try_from(amount_in).unwrap(),
            min_sqrt_ratio_plus_one(),
            &[0u8; 32], // 32-byte data = an address
        );
        assert_eq!(&cd[..4], &SELECTOR_V3_SWAP);
        assert_eq!(&cd[4 + 12..4 + 32], recipient.as_slice());
        // zero_for_one byte at calldata offset 4 + 32 + 31 = 67
        assert_eq!(cd[67], 1);
        // sqrt_price_limit at bytes 4 + 32*3 = 100; low 16 bytes for u128.
        let mut sqrt_actual = [0u8; 16];
        sqrt_actual.copy_from_slice(&cd[100 + 16..100 + 32]);
        let got = u128::from_be_bytes(sqrt_actual);
        assert_eq!(got, MIN_SQRT_RATIO_PLUS_ONE, "MIN_SQRT_RATIO+1");

        // For zero_for_one = false → MAX_SQRT_RATIO_MINUS_ONE.
        let cd_false = univ3_swap_calldata(
            recipient,
            false,
            I256::try_from(amount_in).unwrap(),
            max_sqrt_ratio_minus_one(),
            &[],
        );
        assert_eq!(cd_false[67], 0, "zero_for_one=false byte");
        // The full 32-byte sqrt_price_limit word should match.
        assert_eq!(
            &cd_false[100..132],
            &MAX_SQRT_RATIO_MINUS_ONE_BYTES[..],
            "MAX_SQRT_RATIO-1 word",
        );
    }
}
