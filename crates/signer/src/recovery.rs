//! P6B-CD D-CD4 narrow recovery-only carve-out (ADR-001 Amendment 2).
//!
//! This is the ONLY workspace file allowed to import the recovery
//! crate's symbols. The G2f + G2g grep gates (P6B-CD v0.4 plan)
//! enforce that no other `crates/*.rs` file references the wider
//! signing-key surface (the literal banned token names are enumerated
//! in the plan + the gate `rg` patterns; they are intentionally NOT
//! quoted here so this source file itself stays G2g-clean).
//!
//! The recovery library's full feature set technically exposes both
//! signing and verification APIs; the workspace ban on signing-only
//! constructs is enforced by the G2f narrow-surface allow-list + G2g
//! constructor-name ban + the single-file import gate, NOT by the
//! feature set.
//!
//! Two `pub(crate)` functions:
//!
//! - `parse_der_to_rs`: parses an AWS KMS DER ECDSA signature into
//!   normalized low-s `(r, s)` 32-byte buffers.
//! - `recover_y_parity`: trial-recovers the `yParity in {0, 1}` that
//!   produces the expected SEC1-uncompressed public-key bytes.

use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
use k256::PublicKey;

use crate::SignerError;

/// Parse AWS KMS DER-encoded ECDSA signature bytes into a canonical
/// low-s `(r, s)` 32-byte-pair. Applies `Signature::normalize_s()`
/// unconditionally so AWS KMS high-s signatures are reduced to the
/// EIP-2-mandated low-s form before recovery and assembly.
///
/// Errors:
///
/// - `Err(SignerError::InvalidSignatureBytes)` on any DER parse
///   failure or non-32-byte `r` / `s` length.
pub(crate) fn parse_der_to_rs(der: &[u8]) -> Result<([u8; 32], [u8; 32]), SignerError> {
    let sig = Signature::from_der(der).map_err(|_| SignerError::InvalidSignatureBytes)?;
    let sig = sig.normalize_s().unwrap_or(sig);
    let bytes = sig.to_bytes();
    if bytes.len() != 64 {
        return Err(SignerError::InvalidSignatureBytes);
    }
    let mut r = [0u8; 32];
    let mut s = [0u8; 32];
    r.copy_from_slice(&bytes[..32]);
    s.copy_from_slice(&bytes[32..]);
    Ok((r, s))
}

/// Trial-recover the Ethereum `yParity` byte (`0` or `1`) for the
/// signature `(r, s)` over `digest`, expecting the resulting public
/// key to equal `expected_pubkey_sec1_uncompressed_65`.
///
/// Errors:
///
/// - `Err(SignerError::InvalidSignatureBytes)` if `(r, s)` does not
///   form a structurally valid ECDSA signature (e.g., zero scalars).
/// - `Err(SignerError::ClientInit)` if the expected public-key bytes
///   do not parse as a valid SEC1 uncompressed point (indicates a
///   workspace-internal invariant violation since the bytes are set
///   at boot from a successful `GetPublicKey` parse).
/// - `Err(SignerError::SignatureRecoveryFailed)` if neither `yParity`
///   value recovers to the expected public key.
pub(crate) fn recover_y_parity(
    digest: &[u8; 32],
    r: [u8; 32],
    s: [u8; 32],
    expected_pubkey_sec1_uncompressed_65: &[u8; 65],
) -> Result<u8, SignerError> {
    let mut sig_bytes = [0u8; 64];
    sig_bytes[..32].copy_from_slice(&r);
    sig_bytes[32..].copy_from_slice(&s);
    let sig = Signature::from_slice(&sig_bytes).map_err(|_| SignerError::InvalidSignatureBytes)?;
    let expected = PublicKey::from_sec1_bytes(expected_pubkey_sec1_uncompressed_65)
        .map_err(|_| SignerError::ClientInit)?;
    let expected_vk = VerifyingKey::from(&expected);
    for v in [0u8, 1u8] {
        let Ok(rid) = RecoveryId::try_from(v) else {
            continue;
        };
        if let Ok(recovered) = VerifyingKey::recover_from_prehash(digest, &sig, rid) {
            if recovered == expected_vk {
                return Ok(v);
            }
        }
    }
    Err(SignerError::SignatureRecoveryFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand-crafted valid SEC1 uncompressed point on the SECG K1 curve.
    /// The (x, y) below is the SECG-published generator point coordinates;
    /// the generator is public-domain mathematical material with no
    /// secret. Source: SEC2 v2, Section 2.4.1 ("Recommended Parameters
    /// the K1 curve").
    fn generator_sec1_65() -> [u8; 65] {
        let mut out = [0u8; 65];
        out[0] = 0x04;
        // G_x
        out[1..33].copy_from_slice(&[
            0x79, 0xBE, 0x66, 0x7E, 0xF9, 0xDC, 0xBB, 0xAC, 0x55, 0xA0, 0x62, 0x95, 0xCE, 0x87,
            0x0B, 0x07, 0x02, 0x9B, 0xFC, 0xDB, 0x2D, 0xCE, 0x28, 0xD9, 0x59, 0xF2, 0x81, 0x5B,
            0x16, 0xF8, 0x17, 0x98,
        ]);
        // G_y
        out[33..65].copy_from_slice(&[
            0x48, 0x3A, 0xDA, 0x77, 0x26, 0xA3, 0xC4, 0x65, 0x5D, 0xA4, 0xFB, 0xFC, 0x0E, 0x11,
            0x08, 0xA8, 0xFD, 0x17, 0xB4, 0x48, 0xA6, 0x85, 0x54, 0x19, 0x9C, 0x47, 0xD0, 0x8F,
            0xFB, 0x10, 0xD4, 0xB8,
        ]);
        out
    }

    /// D-T-CD3: `parse_der_to_rs` rejects malformed DER with no panic.
    /// Three sub-cases: wrong tag, truncated, valid prefix + garbage trailer.
    #[test]
    fn d_t_cd3_parse_der_to_rs_rejects_malformed() {
        // (a) wrong outer SEQUENCE tag.
        let wrong_tag = [0x31u8, 0x44, 0x02, 0x20, 0x00, 0x01, 0x02, 0x03];
        let r1 = parse_der_to_rs(&wrong_tag);
        assert_eq!(r1, Err(SignerError::InvalidSignatureBytes));

        // (b) truncated mid-SEQUENCE.
        let truncated = [0x30u8, 0x44, 0x02];
        let r2 = parse_der_to_rs(&truncated);
        assert_eq!(r2, Err(SignerError::InvalidSignatureBytes));

        // (c) valid SEQUENCE header but garbage body that fails INTEGER parse.
        let garbage_body = [0x30u8, 0x06, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff];
        let r3 = parse_der_to_rs(&garbage_body);
        assert_eq!(r3, Err(SignerError::InvalidSignatureBytes));
    }

    /// D-T-CD4 (v0.4 source-lock deviation): a true positive recovery
    /// vector requires a precomputed (digest, r, s, pubkey, y_parity)
    /// tuple sourced from a published ECDSA fixture; in this
    /// implementation environment that vector cannot be lifted from
    /// the named alloy-consensus / go-ethereum sources without web
    /// access AND without violating the G2g ban (any in-tree key
    /// derivation would import a signing-key symbol). v0.4 plan D-T-CD4
    /// is implemented in v0.4-impl as a NEGATIVE-recovery test instead:
    /// for an arbitrary (r, s, digest) tuple paired with the curve
    /// generator's SEC1 bytes, recovery returns
    /// `Err(SignatureRecoveryFailed)` because the random tuple does
    /// not correspond to a real signature by the generator's private
    /// key (which is mathematically G itself; no workspace code holds
    /// it). Positive Ok(y_parity) recovery is exercised implicitly by
    /// the production pipeline at runtime once real AWS KMS output is
    /// available; a future batch with off-tree precomputed vectors
    /// can extend this test to assert positive recovery. NO test-key
    /// byte literals or signing-key constructor symbols appear in
    /// this file -- G2f / G2g 0-hit invariants preserved.
    #[test]
    fn d_t_cd4_recover_y_parity_rejects_unrelated_signature() {
        let digest = [0xAAu8; 32];
        let r = [0xBBu8; 32];
        let s = [0x01u8; 32]; // non-zero
        let pubkey = generator_sec1_65();
        let result = recover_y_parity(&digest, r, s, &pubkey);
        // Unrelated (r, s, digest) cannot recover to the generator;
        // recovery returns Err(SignatureRecoveryFailed). It MUST NOT
        // panic and MUST NOT return Ok.
        assert_eq!(result, Err(SignerError::SignatureRecoveryFailed));
    }

    /// D-T-CD-POS: positive recovery vector exercises the Ok-return
    /// arm of `recover_y_parity` using the off-tree precomputed
    /// non-secret material in [`crate::recovery::pos_vector`].
    #[test]
    fn d_t_cd_pos_recover_y_parity_returns_expected_for_known_vector() {
        use super::pos_vector::*;
        let result = recover_y_parity(&POS_DIGEST, POS_R, POS_S, &POS_PUBKEY_SEC1_65);
        assert_eq!(result, Ok(POS_Y_PARITY));
    }

    /// D-T-CD-POS-DER: round-trip through `parse_der_to_rs` then
    /// `recover_y_parity` for the off-tree precomputed DER.
    #[test]
    fn d_t_cd_pos_parse_der_then_recover_round_trips() {
        use super::pos_vector::*;
        let (r, s) = parse_der_to_rs(POS_DER_SIGNATURE).expect("DER parse");
        assert_eq!(r, POS_R);
        assert_eq!(s, POS_S);
        let y = recover_y_parity(&POS_DIGEST, r, s, &POS_PUBKEY_SEC1_65).expect("recover");
        assert_eq!(y, POS_Y_PARITY);
    }

    /// D-T-CD8: parse_der_to_rs normalizes a hand-built high-s DER
    /// signature (s = n - 1) to its low-s counterpart (s = 1). Asserts
    /// the EXACT post-normalization s value (advisory fix: strengthen
    /// from "top bit clear" to "equals 1").
    ///
    /// For the SECG K1 curve, s_low = n - s_high. With s_high = n - 1,
    /// s_low = n - (n - 1) = 1 -> encoded as `[0; 31] || 0x01`.
    #[test]
    fn d_t_cd8_parse_der_to_rs_normalizes_high_s() {
        // SECG K1 curve order n (SEC2 v2 Section 2.4.1):
        //   FFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFE BAAEDCE6 AF48A03B BFD25E8C D0364141
        // s = n - 1 -> last byte 0x40.
        const N_MINUS_1: [u8; 32] = [
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFE, 0xBA, 0xAE, 0xDC, 0xE6, 0xAF, 0x48, 0xA0, 0x3B, 0xBF, 0xD2, 0x5E, 0x8C,
            0xD0, 0x36, 0x41, 0x40,
        ];
        // DER: 30 26  02 01 01  02 21 00 <N-1>
        // r = 1 -> 3 bytes; s INTEGER header 0x02 0x21 0x00 then 32-byte
        // s (leading 0x00 because top bit of FF is set). SEQUENCE body
        // = 3 + 35 = 38 bytes; outer header 0x30 0x26.
        let mut der = Vec::with_capacity(40);
        der.extend_from_slice(&[0x30, 0x26]);
        der.extend_from_slice(&[0x02, 0x01, 0x01]);
        der.extend_from_slice(&[0x02, 0x21, 0x00]);
        der.extend_from_slice(&N_MINUS_1);

        let (r, s) = parse_der_to_rs(&der).expect("hand-built DER must parse");
        let mut expected_r = [0u8; 32];
        expected_r[31] = 1;
        let mut expected_s = [0u8; 32];
        expected_s[31] = 1;
        assert_eq!(r, expected_r, "r preserved through parse");
        assert_eq!(
            s, expected_s,
            "normalize_s must reduce s=n-1 to s=1 exactly"
        );
    }
}

// --------------------------------------------------------------
// Off-tree-precomputed positive recovery vector (P6B-CD R-10).
//
// The vector below was generated by a one-off k256 program OUTSIDE
// this repository. The seed scalar used to derive the public key
// NEVER touched this source file or the git tree. Only NON-SECRET
// material is pasted here: the SEC1-uncompressed public-key bytes,
// the message digest, the (r, s) signature components in low-s
// form, the expected y_parity, the corresponding DER signature
// bytes, and the Ethereum-derived address. This satisfies the v0.4
// plan D-T-CD4 source-lock invariant + the v0.4 R-7 / R-10
// private-key-bytes-absolute-ban while finally exercising the
// positive Ok-return path.
//
// The digest corresponds to keccak256 of the type-2 unsigned-tx
// preimage for the test bundle in `rlp::tests::minimal_tx()`
// (chain_id=1, nonce=0, max_priority=1, max_fee=2, gas_limit=21000,
// to=0x11..11, value=0, data=[], access_list=[]). That preimage's
// canonical 33-byte form is asserted in
// `rlp::tests::encode_eip1559_unsigned_known_vector`.
//
// Reproducibility procedure (off-tree, NOT to be committed):
//   1. Outside this repo, create a tiny k256 cargo project.
//   2. Use a local test scalar to derive the public key (do NOT paste
//      the scalar into the repo).
//   3. Compute the digest as described above.
//   4. sign_prehash(digest) -> (sig, rec).
//   5. If sig.is_high_s() apply normalize_s + flip rec_id ^ 1.
//   6. Paste only (r, s, y_parity, DER, pubkey_sec1_65,
//      derived_address) here.
// --------------------------------------------------------------
#[cfg(test)]
pub(crate) mod pos_vector {
    pub(crate) const POS_PUBKEY_SEC1_65: [u8; 65] = [
        0x04, 0x1B, 0x84, 0xC5, 0x56, 0x7B, 0x12, 0x64, 0x40, 0x99, 0x5D, 0x3E, 0xD5, 0xAA, 0xBA,
        0x05, 0x65, 0xD7, 0x1E, 0x18, 0x34, 0x60, 0x48, 0x19, 0xFF, 0x9C, 0x17, 0xF5, 0xE9, 0xD5,
        0xDD, 0x07, 0x8F, 0x70, 0xBE, 0xAF, 0x8F, 0x58, 0x8B, 0x54, 0x15, 0x07, 0xFE, 0xD6, 0xA6,
        0x42, 0xC5, 0xAB, 0x42, 0xDF, 0xDF, 0x81, 0x20, 0xA7, 0xF6, 0x39, 0xDE, 0x51, 0x22, 0xD4,
        0x7A, 0x69, 0xA8, 0xE8, 0xD1,
    ];
    pub(crate) const POS_DIGEST: [u8; 32] = [
        0x5E, 0xBD, 0x82, 0x9A, 0x5D, 0x55, 0x62, 0xF9, 0xC1, 0x77, 0x41, 0xCF, 0x1E, 0x3A, 0x80,
        0xF6, 0xDC, 0xBF, 0x45, 0xAE, 0x13, 0x36, 0xE3, 0x3A, 0xA5, 0x56, 0x5D, 0x4C, 0xE0, 0xEA,
        0x76, 0xBC,
    ];
    pub(crate) const POS_R: [u8; 32] = [
        0x52, 0xFB, 0xFD, 0x25, 0x2B, 0x77, 0x22, 0x68, 0xCA, 0x1D, 0x88, 0x1A, 0x27, 0x73, 0xFC,
        0xBA, 0x2D, 0x0D, 0x50, 0x85, 0x2A, 0x33, 0xA5, 0x00, 0x7C, 0x32, 0x7D, 0x7E, 0x79, 0x02,
        0xA2, 0xE6,
    ];
    pub(crate) const POS_S: [u8; 32] = [
        0x0D, 0xFD, 0xC3, 0x61, 0x24, 0x80, 0xBA, 0x39, 0x9B, 0x0D, 0x17, 0x3C, 0xA2, 0x82, 0x11,
        0xA8, 0x9F, 0x4F, 0x4B, 0x8C, 0x1D, 0xC5, 0x77, 0x2B, 0xC7, 0x93, 0x0B, 0x24, 0x88, 0xAD,
        0xFD, 0x68,
    ];
    pub(crate) const POS_Y_PARITY: u8 = 0;
    pub(crate) const POS_DERIVED_ADDRESS: [u8; 20] = [
        0x1A, 0x64, 0x2F, 0x0E, 0x3C, 0x3A, 0xF5, 0x45, 0xE7, 0xAC, 0xBD, 0x38, 0xB0, 0x72, 0x51,
        0xB3, 0x99, 0x09, 0x14, 0xF1,
    ];
    pub(crate) const POS_DER_SIGNATURE: &[u8] = &[
        0x30, 0x44, 0x02, 0x20, 0x52, 0xFB, 0xFD, 0x25, 0x2B, 0x77, 0x22, 0x68, 0xCA, 0x1D, 0x88,
        0x1A, 0x27, 0x73, 0xFC, 0xBA, 0x2D, 0x0D, 0x50, 0x85, 0x2A, 0x33, 0xA5, 0x00, 0x7C, 0x32,
        0x7D, 0x7E, 0x79, 0x02, 0xA2, 0xE6, 0x02, 0x20, 0x0D, 0xFD, 0xC3, 0x61, 0x24, 0x80, 0xBA,
        0x39, 0x9B, 0x0D, 0x17, 0x3C, 0xA2, 0x82, 0x11, 0xA8, 0x9F, 0x4F, 0x4B, 0x8C, 0x1D, 0xC5,
        0x77, 0x2B, 0xC7, 0x93, 0x0B, 0x24, 0x88, 0xAD, 0xFD, 0x68,
    ];
}
