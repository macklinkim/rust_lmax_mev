//! Solidity storage-key derivation helpers.
//!
//! Per Solidity storage layout:
//! - Mapping at slot `M` for key `K`: stored at `keccak256(abi.encode(K, M))`
//!   where `K` is left-padded to 32 bytes.
//! - Dynamic array at slot `S`, element `i` of `element_words` 32-byte
//!   words: stored at `keccak256(S) + i * element_words`.
//!
//! P4-B ships these generic helpers; verified Uniswap V2/V3 layout
//! constants live in P4-C (in `UniswapV2Layout` / `UniswapV3Fee005Layout`
//! impls of the `PoolSlotLayout` trait) against recorded mainnet pool
//! fixtures.
//!
//! Codex 2026-05-04 22:48 non-blocking note: signed Solidity mapping
//! keys (e.g., UniV3 `ticks: mapping(int24 => Tick.Info)` and
//! `tickBitmap: mapping(int16 => uint256)`) require correct 32-byte
//! signed sign-extension at the caller BEFORE invoking
//! `mapping_slot_u256`. P4-C tests must include signed-key vectors;
//! P4-B stays generic.

use alloy_primitives::{keccak256, Address, B256, U256};

/// Storage slot for `mapping(_ => _) value` declared at slot
/// `mapping_slot`, indexed by `key`. `key` is the already-32-byte
/// (left-padded for unsigned, sign-extended for signed) ABI encoding of
/// the mapping key.
pub fn mapping_slot_u256(mapping_slot: U256, key: B256) -> U256 {
    let mut buf = [0u8; 64];
    buf[..32].copy_from_slice(key.as_slice());
    buf[32..].copy_from_slice(&mapping_slot.to_be_bytes::<32>());
    U256::from_be_bytes(keccak256(buf).0)
}

/// 32-byte sign-extended ABI encoding of a Solidity signed integer
/// mapping key (e.g. UniV3 `int24 tick` or `int16 wordPos`).
/// `bytes` = the Solidity declared width in bytes (3 for int24, 2 for
/// int16, etc.). Positive values left-pad with `0x00`; negative values
/// left-pad with `0xff` then take the low `bytes` of the i64 BE
/// representation. P4-C uses this for UniswapV3Pool's `ticks` and
/// `tickBitmap` mapping derivations.
pub fn signed_int_key(value: i32, bytes: usize) -> B256 {
    let signed_be = (value as i64).to_be_bytes();
    let mut key = if value < 0 { [0xffu8; 32] } else { [0u8; 32] };
    let take = bytes.min(8);
    key[32 - take..].copy_from_slice(&signed_be[8 - take..]);
    B256::from(key)
}

/// 32-byte left-padded ABI encoding of an EVM `address` (for use as
/// the unsigned key of an ERC-20 `balances`/`allowed`/`blacklisted`
/// mapping).
pub fn address_key(addr: Address) -> B256 {
    let mut key = [0u8; 32];
    key[12..].copy_from_slice(addr.as_slice());
    B256::from(key)
}

/// Storage slot for the `i`-th element of a dynamic array declared at
/// slot `array_slot`, where each element occupies `element_words`
/// 32-byte words. Pass `element_words = 1` for `uint256[]`,
/// `element_words = 2` for a 2-word struct, etc.
pub fn array_element_slot(array_slot: U256, i: U256, element_words: u32) -> U256 {
    let base = U256::from_be_bytes(keccak256(array_slot.to_be_bytes::<32>()).0);
    base + i * U256::from(element_words)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independently-computed expected hashes — produced via the
    /// `tiny-keccak` crate (a separate keccak256 implementation from
    /// `alloy_primitives::keccak256`, which uses the `sha3` crate
    /// internally). Sanity-checked against the well-known Ethereum
    /// constant `keccak256("") == c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470`.
    /// These constants are HARDCODED snapshots: a future regression in
    /// either `mapping_slot_u256`'s buffer assembly OR in
    /// `alloy_primitives::keccak256` would diverge from them and fail
    /// the test.
    ///
    /// `keccak256(abi.encode(uint256(0x42), uint256(1)))` — the storage
    /// slot for `mapping(uint256 => _) m` declared at slot 1, indexed
    /// by key `0x42`. Computed via `tiny-keccak` over the 64-byte ABI
    /// buffer.
    const SK1_EXPECTED_MAPPING_SLOT_HEX: [u8; 32] = [
        0xe9, 0x96, 0x35, 0xfc, 0xcc, 0x85, 0x93, 0xe1, 0x8a, 0x8f, 0x8d, 0x41, 0xf3, 0x81, 0x9f,
        0xbb, 0xb2, 0x3d, 0x11, 0x6b, 0x5e, 0x97, 0x9c, 0xc6, 0x8c, 0x43, 0xa9, 0x8e, 0x9c, 0x10,
        0xe5, 0x2a,
    ];

    /// `keccak256(uint256(2))` — base slot of a dynamic array declared
    /// at storage slot 2. Computed via `tiny-keccak`.
    const SK2_EXPECTED_ARRAY_BASE_HEX: [u8; 32] = [
        0x40, 0x57, 0x87, 0xfa, 0x12, 0xa8, 0x23, 0xe0, 0xf2, 0xb7, 0x63, 0x1c, 0xc4, 0x1b, 0x3b,
        0xa8, 0x82, 0x8b, 0x33, 0x21, 0xca, 0x81, 0x11, 0x11, 0xfa, 0x75, 0xcd, 0x3a, 0xa3, 0xbb,
        0x5a, 0xce,
    ];

    /// S-K-1: `mapping_slot_u256(1, B256(uint256(0x42)))` matches the
    /// independently-computed `tiny-keccak` snapshot.
    #[test]
    fn mapping_slot_u256_matches_independent_keccak_vector() {
        let mapping_slot = U256::from(1u64);
        let key = B256::from(U256::from(0x42u64).to_be_bytes::<32>());
        let got = mapping_slot_u256(mapping_slot, key);
        let expected = U256::from_be_bytes(SK1_EXPECTED_MAPPING_SLOT_HEX);
        assert_eq!(
            got, expected,
            "mapping_slot_u256(1, key=0x42) must match the tiny-keccak-computed slot \
             0xe99635fccc8593e18a8f8d41f3819fbbb23d116b5e979cc68c43a98e9c10e52a"
        );
        // Cross-check: a different mapping_slot must yield a different
        // slot (catches accidental slot-arg-ignored bugs).
        let other = mapping_slot_u256(U256::from(2u64), key);
        assert_ne!(
            got, other,
            "different mapping_slot must yield different storage slot"
        );
    }

    /// S-K-3 (P4-C): `signed_int_key` sign-extends i32 to 32 bytes per
    /// Solidity ABI rules.
    ///
    /// Tests negative i24 (UniV3 MIN_TICK = -887272) + positive i16
    /// (smoke) + boundary -1 to catch off-by-one in the padding fill.
    #[test]
    fn signed_int_key_sign_extends_negative_and_positive_correctly() {
        // MIN_TICK = -887272. In 24-bit two's complement:
        // 2^24 - 887272 = 16777216 - 887272 = 15889944 = 0xF27618.
        // Expected: high 29 bytes 0xff, low 3 bytes f27618.
        let got = signed_int_key(-887_272, 3);
        let mut expected = [0xffu8; 32];
        expected[29..].copy_from_slice(&[0xf2, 0x76, 0x18]);
        assert_eq!(got, B256::from(expected), "MIN_TICK i24 sign-extend");

        // Positive int16 = 42. Expected: high 30 bytes 0x00, low 2
        // bytes 0x002a.
        let got = signed_int_key(42, 2);
        let mut expected = [0u8; 32];
        expected[30..].copy_from_slice(&[0x00, 0x2a]);
        assert_eq!(got, B256::from(expected), "positive i16 left-pad");

        // -1 in i24: low 3 bytes 0xffffff; high 29 bytes 0xff. The full
        // word is therefore all-0xff.
        let got = signed_int_key(-1, 3);
        assert_eq!(got, B256::from([0xffu8; 32]), "-1 i24 = all 0xff");
    }

    /// S-K-4 (P4-C): `address_key` left-pads an EVM address to 32
    /// bytes. Verifies the high 12 bytes are zero and the low 20 are
    /// the address big-endian.
    #[test]
    fn address_key_left_pads_address_to_32_bytes() {
        let addr = alloy_primitives::address!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        let got = address_key(addr);
        let mut expected = [0u8; 32];
        expected[12..].copy_from_slice(addr.as_slice());
        assert_eq!(got, B256::from(expected));
    }

    /// S-K-2: `array_element_slot(2, i, w)` matches the
    /// independently-computed `tiny-keccak` base PLUS the documented
    /// `i * element_words` offset arithmetic.
    #[test]
    fn array_element_slot_matches_independent_keccak_vector() {
        let array_slot = U256::from(2u64);
        let base = U256::from_be_bytes(SK2_EXPECTED_ARRAY_BASE_HEX);
        // i=0, words=1 → base; i=5, words=1 → base+5; i=5, words=2 → base+10.
        assert_eq!(array_element_slot(array_slot, U256::from(0u64), 1), base);
        assert_eq!(
            array_element_slot(array_slot, U256::from(5u64), 1),
            base + U256::from(5u64),
        );
        assert_eq!(
            array_element_slot(array_slot, U256::from(5u64), 2),
            base + U256::from(10u64),
        );
        // Confirm the helper's own `keccak256` call agrees with the
        // tiny-keccak snapshot (i.e., alloy_primitives::keccak256 and
        // tiny-keccak produce the same digest for the same input).
        let alloy_base = U256::from_be_bytes(keccak256(array_slot.to_be_bytes::<32>()).0);
        assert_eq!(
            alloy_base, base,
            "alloy_primitives::keccak256 must agree with tiny-keccak snapshot"
        );
    }
}
