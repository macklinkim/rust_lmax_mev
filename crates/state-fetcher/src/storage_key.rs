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

use alloy_primitives::{keccak256, B256, U256};

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
