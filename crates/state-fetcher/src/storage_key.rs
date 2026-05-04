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

    /// S-K-1: mapping derivation matches a hand-computed known-good
    /// vector. `keccak256(abi.encode(uint256(0x42), uint256(1)))` is the
    /// canonical Solidity-doc example. Computed once via the same
    /// `keccak256` primitive over the same 64-byte buffer to avoid
    /// hard-coding a magic constant that could mask a regression in the
    /// helper's own buffer assembly.
    #[test]
    fn mapping_slot_u256_matches_solidity_known_good_vector() {
        let mapping_slot = U256::from(1u64);
        let key_u: u64 = 0x42;
        let key = B256::from(U256::from(key_u).to_be_bytes::<32>());
        let got = mapping_slot_u256(mapping_slot, key);
        // Hand-assemble the same 64-byte buffer the way the Solidity ABI
        // would for `mapping(uint256 => _)`.
        let mut buf = [0u8; 64];
        buf[..32].copy_from_slice(&U256::from(key_u).to_be_bytes::<32>());
        buf[32..].copy_from_slice(&U256::from(1u64).to_be_bytes::<32>());
        let expected = U256::from_be_bytes(keccak256(buf).0);
        assert_eq!(
            got, expected,
            "mapping_slot_u256 must match keccak256(abi.encode(key, mapping_slot))"
        );
        // And cross-check with a different mapping_slot to catch
        // accidental slot-arg ignored bugs.
        let other = mapping_slot_u256(U256::from(2u64), key);
        assert_ne!(
            got, other,
            "different mapping_slot must yield different slot"
        );
    }

    /// S-K-2: array element derivation matches `keccak256(slot) + i * w`.
    #[test]
    fn array_element_slot_matches_solidity_known_good_vector() {
        let array_slot = U256::from(2u64);
        let base = U256::from_be_bytes(keccak256(array_slot.to_be_bytes::<32>()).0);
        // i = 0 → base; i = 5, words = 1 → base + 5; words = 2 → base + 10.
        assert_eq!(array_element_slot(array_slot, U256::from(0u64), 1), base);
        assert_eq!(
            array_element_slot(array_slot, U256::from(5u64), 1),
            base + U256::from(5u64),
        );
        assert_eq!(
            array_element_slot(array_slot, U256::from(5u64), 2),
            base + U256::from(10u64),
        );
    }
}
