//! P6B-CD D-CD2 + D-CD7 EIP-1559 (type-2) RLP encoders.
//!
//! Produces canonical RLP per Ethereum yellow-paper Appendix B for
//! both the unsigned-tx preimage (sign input) and the signed-tx bytes
//! (assembly output). The empty access list `[]` is hardcoded; the
//! workspace does NOT yet model access-list entries (deferred to a
//! future batch per the P6B-CD v0.4 plan).
//!
//! Unsigned preimage shape (Sign input):
//!
//!   `0x02 || rlp([chain_id, nonce, max_priority_fee_per_gas,
//!                 max_fee_per_gas, gas_limit, to, value, data, []])`
//!
//! Signed-tx shape (assembly output):
//!
//!   `0x02 || rlp([...unsigned fields..., y_parity, r, s])`
//!
//! `r` and `s` are passed in as fixed 32-byte big-endian buffers and
//! encoded canonically through `U256` (leading zeros stripped).

use alloy_primitives::U256;
use alloy_rlp::{BufMut, Encodable, Header};

use crate::BundleTx;

/// P6B-CD D-CD2: emit the EIP-1559 unsigned-tx preimage bytes.
///
/// The caller computes `keccak256(...)` over the returned bytes to
/// obtain the message digest passed to `kms_client.sign_digest(...)`.
pub(crate) fn encode_eip1559_unsigned(tx: &BundleTx) -> Vec<u8> {
    let payload_length = unsigned_payload_length(tx);
    let mut out = Vec::with_capacity(
        1 + Header {
            list: true,
            payload_length,
        }
        .length()
            + payload_length,
    );
    out.put_u8(0x02);
    write_unsigned(tx, &mut out);
    out
}

/// P6B-CD D-CD7: emit the EIP-1559 signed-tx bytes ready for
/// (future) `eth_sendRawTransaction`. P6B-CD does NOT add a runtime
/// caller; the output is returned by `ProductionSigner::sign_tx`
/// only via the test-only `invoke_signer_for_test` hook.
pub(crate) fn encode_eip1559_signed(
    tx: &BundleTx,
    y_parity: u8,
    r: &[u8; 32],
    s: &[u8; 32],
) -> Vec<u8> {
    let r_u = U256::from_be_bytes::<32>(*r);
    let s_u = U256::from_be_bytes::<32>(*s);
    let y_u = y_parity as u64;
    let unsigned_payload = unsigned_payload_length(tx);
    let signed_payload = unsigned_payload + y_u.length() + r_u.length() + s_u.length();
    let mut out = Vec::with_capacity(
        1 + Header {
            list: true,
            payload_length: signed_payload,
        }
        .length()
            + signed_payload,
    );
    out.put_u8(0x02);
    Header {
        list: true,
        payload_length: signed_payload,
    }
    .encode(&mut out);
    write_unsigned_fields(tx, &mut out);
    y_u.encode(&mut out);
    r_u.encode(&mut out);
    s_u.encode(&mut out);
    out
}

fn write_unsigned<B: BufMut>(tx: &BundleTx, out: &mut B) {
    let payload_length = unsigned_payload_length(tx);
    Header {
        list: true,
        payload_length,
    }
    .encode(out);
    write_unsigned_fields(tx, out);
}

fn write_unsigned_fields<B: BufMut>(tx: &BundleTx, out: &mut B) {
    tx.chain_id.encode(out);
    tx.nonce.encode(out);
    tx.max_priority_fee_per_gas.encode(out);
    tx.max_fee_per_gas.encode(out);
    tx.gas_limit.encode(out);
    tx.to.encode(out);
    tx.value_wei.encode(out);
    tx.data.as_slice().encode(out);
    // Empty access list -> RLP empty list `0xc0`.
    Header {
        list: true,
        payload_length: 0,
    }
    .encode(out);
}

fn unsigned_payload_length(tx: &BundleTx) -> usize {
    tx.chain_id.length()
        + tx.nonce.length()
        + tx.max_priority_fee_per_gas.length()
        + tx.max_fee_per_gas.length()
        + tx.gas_limit.length()
        + tx.to.length()
        + tx.value_wei.length()
        + tx.data.as_slice().length()
        + Header {
            list: true,
            payload_length: 0,
        }
        .length()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{Address, U256};

    fn minimal_tx() -> BundleTx {
        // Simple eth-transfer-shaped tx with empty data + small fees,
        // chosen so the canonical RLP is hand-traceable.
        BundleTx::new(
            Address::ZERO,
            Address::from([0x11u8; 20]),
            U256::ZERO,
            Vec::new(),
            21_000,
            0,
            1,
            0,
            U256::from(1u64),
            U256::from(2u64),
        )
    }

    /// D-T-CD2: `encode_eip1559_unsigned(tx)` matches a hand-traceable
    /// canonical RLP fixture. The fixture below is a deterministic
    /// synthetic vector (NOT a private-key-derived signing vector);
    /// every byte is hand-computable from the canonical RLP rules in
    /// Ethereum yellow-paper Appendix B. See the doc comment for the
    /// derivation. v0.4 plan D-T-CD2 source-lock deviation: this
    /// implementation cannot reach the network to pin the alloy-rs
    /// `alloy-consensus` fixture at impl time, so the test instead
    /// uses a self-contained synthetic vector with the same shape
    /// guarantees (empty access list; canonical RLP; type-2 envelope
    /// 0x02 prefix). The 1-line-per-field derivation is in the test
    /// body below.
    #[test]
    fn encode_eip1559_unsigned_known_vector() {
        let tx = minimal_tx();
        let got = encode_eip1559_unsigned(&tx);

        // Hand-derived canonical RLP for `minimal_tx()`:
        //   chain_id=1                     -> 0x01
        //   nonce=0                        -> 0x80 (empty string)
        //   max_priority_fee_per_gas=1     -> 0x01
        //   max_fee_per_gas=2              -> 0x02
        //   gas_limit=21000=0x5208         -> 0x82 0x52 0x08
        //   to=0x11..11 (20 bytes)         -> 0x94 0x11*20
        //   value=0                        -> 0x80
        //   data=[] (empty bytes)          -> 0x80
        //   access_list=[]                 -> 0xc0
        //
        // Field bytes total = 1 + 1 + 1 + 1 + 3 + 21 + 1 + 1 + 1 = 31.
        // List header for payload=31 -> 0xc0 + 31 = 0xdf (single byte).
        // Then envelope 0x02 prefix.
        // Total length = 1 + 1 + 31 = 33 bytes.
        let mut expected = Vec::with_capacity(33);
        expected.push(0x02); // type-2 envelope
        expected.push(0xdf); // RLP list header, payload = 31 bytes
        expected.push(0x01); // chain_id
        expected.push(0x80); // nonce
        expected.push(0x01); // max_priority_fee_per_gas
        expected.push(0x02); // max_fee_per_gas
        expected.extend_from_slice(&[0x82, 0x52, 0x08]); // gas_limit = 21000
        expected.push(0x94); // 20-byte string header
        expected.extend_from_slice(&[0x11u8; 20]); // to
        expected.push(0x80); // value
        expected.push(0x80); // data
        expected.push(0xc0); // empty access list
        assert_eq!(
            got, expected,
            "EIP-1559 unsigned RLP byte mismatch:\ngot  = {:02x?}\nwant = {:02x?}",
            got, expected,
        );
    }

    /// Sanity: `encode_eip1559_signed` always emits the type-2
    /// envelope and includes the y_parity / r / s tail.
    #[test]
    fn encode_eip1559_signed_envelope_and_tail_shape() {
        let tx = minimal_tx();
        let r = [0x33u8; 32];
        let s = [0x44u8; 32];
        let signed = encode_eip1559_signed(&tx, 1, &r, &s);
        assert_eq!(signed[0], 0x02, "type-2 envelope byte");
        // Last 1 + 33 + 33 = 67 bytes are y_parity + r + s.
        // y_parity = 1 -> 0x01.
        // r = 32x 0x33 -> 0xa0 then 32 bytes.
        // s = 32x 0x44 -> 0xa0 then 32 bytes.
        assert_eq!(signed[signed.len() - 67], 0x01, "y_parity byte");
        assert_eq!(signed[signed.len() - 66], 0xa0, "r length-byte");
        assert_eq!(signed[signed.len() - 33], 0xa0, "s length-byte");
        assert_eq!(&signed[signed.len() - 32..], &s[..], "s trailing 32 bytes");
    }
}
