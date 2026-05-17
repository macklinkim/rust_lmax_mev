# Phase 6b Batch CD -- Sign-Activation: BundleTx EIP-1559 + RLP + KMS Signature Assembly (planning only)

**Date:** 2026-05-17 KST
**Status:** Draft v0.4 (revised after Codex REVISION REQUIRED HIGH on v0.3; two blocking items R-8 + R-9 resolved + advisory cleanup). PRE-IMPL PLAN. ASCII-only. No `.rs` / `Cargo.toml` / `Cargo.lock` / config / fixture / `.coordination` / ADR edits in this turn. No commit, no push.
**Awaiting:** manual Codex re-review of v0.4.

## v0.3 -> v0.4 changelog

| Codex item | v0.3 issue | v0.4 fix |
|---|---|---|
| R-8 | All ripgrep gate commands used `rg -nE 'pattern'`. `-E` is "encoding" in ripgrep (not "extended regex" as in GNU grep); the commands failed with `unknown encoding` on Codex's Windows host. | All `rg -nE 'pattern' ...` invocations across the plan rewritten to valid `rg -n -e 'pattern' ...` form. Affected sites: G2a regex doc, G2a follow-up k256 single-file gate (in Blocker 3 + G2f), G2f narrow-surface allow-list (2 occurrences), G2g (R-7 new gate). The "Process" Step 14 self-check inherits these via the "Gates at P6B-CD close" Section, so the corrected syntax flows through to impl-time. |
| R-9 | G2g's negative grep (with the corrected `-n -e` syntax) hits `crates/signer/src/production.rs:70` because that file's existing P6B-B negative-invariant doc comment LITERALLY contains the banned token `` `SecretKey` `` while ASSERTING the same invariant. The plan's v0.3 G2g said "0 hits absolute" -- inconsistent with the actual baseline. | Plan now includes an explicit 1-line `production.rs:70` doc-comment cleanup as part of D-CD6 + Step 9 (the file is already in the P6B-CD touch set). The cleanup rewrites the literal `` `SecretKey`-style type `` phrase into a non-G2g-tripping equivalent (`no opaque private-key-byte struct field of any shape`) while preserving the no-opaque-private-key-struct-field invariant. G2g remains "0 hits absolute" at P6B-CD close. The G2g Section gains a new "Pre-existing baseline hit must be removed at P6B-CD impl time" subsection making the cleanup explicit. Touch-summary entry for `production.rs` and Step 9 both updated to call out the doc-comment fix. |

Advisory cleanup also applied in v0.4:

- `## Process (v0.2; R-6 corrected)` header was stale (still referred to v0.2); updated to `## Process (v0.3; R-6 carried forward from v0.2)`. Process content itself was already v0.3-style after the v0.3 revision; the header was the only leftover.
- D-CD6 body section headers were `v0.2 body shape` / `Notes on the v0.2 body`. v0.3 had not updated these; v0.4 makes them explicitly v0.3 ("v0.3 body shape, introduced in v0.2 plan revision; preserves all P6B-C `Err` paths verbatim..." / "Notes on the v0.3 body (unchanged in shape from v0.2; documented here as the current locked form)"). Reader-clarity nit per Codex advisory.

(Stale phrases describing WHEN locks were first introduced -- "Scope (v0.1)", "Path (a) -- LOCKED in v0.1", "v0.1 LOCKS the breaking ctor" -- continue to be left in place as accurate provenance, NOT factual errors.)

## v0.2 -> v0.3 changelog

| Codex item | v0.2 issue | v0.3 fix |
|---|---|---|
| R-7 | "Public-domain go-ethereum test key" exception conflicted with `production-signer.md` Section 2.1 / 2.2 and `phase-6b-boundary.md` Section 6 / 7 -- both forbid raw private-key bytes in repo / tests / fixtures absolute, with no exception for published test keys | Exception REMOVED. Test-vector sources rewritten to copy ONLY non-secret/public material: preimage, digest, signed-tx bytes, `r`, `s`, expected `y_parity`, expected SEC1 public-key bytes (`0x04 \|\| x \|\| y`), DER signature bytes. If D-T-CD4 needs a public key, the implementer PRE-COMPUTES it outside the repo from the public source and pastes ONLY the SEC1 public-key bytes (with a source comment naming the precomputation step). NEW hard invariant in "Hard forbids at P6B-CD close" Section: NO `test_key`, NO private-key byte literal, NO `SecretKey`, NO `SigningKey`, NO equivalent signing-key constructor in `crates/` -- absolute, no test exception. NEW G2g grep gate enforces this. |

Advisory cleanup also applied in v0.3:

- "Low-s normalization" paragraph (Blocker 3 Section): the trailing sentence said "Plan asserts via test `D-T-CD4` ..." -- corrected to `D-T-CD8` (the test that actually asserts normalization; D-T-CD4 is the base recovery vector).
- "What Codex is asked to verdict" closing Section header updated from v0.1 to v0.2 to v0.3; item 6 updated from "8 minimum tests" to "10 minimum tests". (Stale wording elsewhere -- e.g., "Scope (v0.1)", "Path (a) -- LOCKED in v0.1", "v0.1 LOCKS the breaking ctor" -- left in place as accurate historical references; these phrases describe WHEN the lock was first introduced, not which version this plan is.)

## v0.1 -> v0.2 changelog

| Codex item | v0.1 issue | v0.2 fix |
|---|---|---|
| R-1 | Sample failure rule `{outcome!="not_configured"}` would fire on `ok` outcome | `config/examples/signing-audit-alert.yaml` is now a TOUCHED FILE in D-CD9. Failure rule matcher narrows to a positive enumeration of failure outcomes: `{outcome=~"address_mismatch\|invalid_bundle_tx\|kms_sign_failed\|invalid_signature_bytes\|signature_recovery_failed"}`. YAML comments + alert annotations rewritten to reflect the P6B-CD outcome-label set. |
| R-2 | DER parse + recovery errors bypassed audit+counter | `sign_tx` pseudo-code in D-CD6 explicitly emits `emit_attempt_audit` + counter at the parse-failure AND recovery-failure points. Two new outcome labels added (`invalid_signature_bytes`, `signature_recovery_failed`). Two new targeted tests (D-T-CD9, D-T-CD10) assert both the returned `SignerError` variant AND the audit/counter emission. |
| R-3 | Production `expect(...)` on `pubkey_sec1_65` | Removed. The let-else destructure now matches all three Options jointly: `let (Some(client), Some(derived), Some(pubkey_sec1_65)) = (...) else { audit("not_configured"); counter("not_configured"); return Err(NotConfigured); };`. All three Options are set or unset atomically by `from_aws_kms_with_client`; no panic path. |
| R-4 | k256 feature wording claimed locked features cannot expose signing constructs | Corrected. The k256 `ecdsa` feature DOES provide `SigningKey` per docs.rs/k256/0.13/k256/ecdsa/. The safety control is the **G2f narrow-surface allow-list** plus the in-source `use k256::*` import-line gate, NOT a feature-flag exclusion. Plan and ADR amendment wording rewritten accordingly. |
| R-5 | Test vectors left to impl time | Vectors LOCKED to named official sources at plan time. D-T-CD2: alloy-rs `alloy-consensus` crate `TxEip1559` serialization round-trip fixture with empty access list. D-T-CD4: go-ethereum `core/types/transaction_signing_test.go::TestEIP1559Signing` (chain_id 1, empty access list). D-T-CD8: synthetic; constructed from the D-T-CD4 vector via `s -> n - s` + `y_parity -> 1 - y_parity`. Implementer pins the exact commit hash + line numbers from each named source in a doc comment beside the byte literals. |
| R-6 | Process conflated plan commit with impl authorization | Process rewritten: Codex APPROVED -> record verdict, commit + push the approved plan only, then STOP. P6B-CD implementation requires a SEPARATE explicit user re-authorization AFTER the plan commit lands on `master`. Two-step gate is explicit; no path collapses plan + impl into one user gesture. |

Codex advisory notes from v0.1 (acknowledged; no plan change required):

- BundleTx::new(...) 8 -> 10 args BREAKING is acceptable (`#[non_exhaustive]`, no live production callers, test callers in touch set).
- `pubkey_sec1_65: Option<[u8; 65]>` is acceptable (matches `PublicKey::from_sec1_bytes` input shape).
- Hardcoded empty access list is acceptable for P6B-CD because the locked test vector has empty access list; non-empty access lists remain future scope.
- Path (a) k256 recovery-only carve-out remains the preferred route; ADR-001 Amendment 2 wording rewritten per R-4.

## Predecessors

- `phase-6a-complete` at `bd0a53c` (tag object `3c9faaf`).
- Phase 6b overview v0.2 APPROVED HIGH at `49123e9`.
- P6B-A boundary doc + ADR-001 amendment at `1c490de`.
- P6B-B no-SDK ProductionSigner stub + structured audit log-source at `df96ac8`.
- P6B-C HSM/KMS infrastructure + signer audit surface at `b77241a`; doc-only YAML follow-up at `ff2edbc` (Codex APPROVED HIGH).
- `master` HEAD `ff2edbc`. Pre-P6B-CD targeted baseline (signer + config + app): **49 passed + 0 ignored**.

## Authorization basis

Phase 6b overview v0.2 prerequisite #1 ("Fresh explicit user authorization per non-goal") REQUIRES a separate user re-authorization for the sign-activation batch even after Codex APPROVES this plan. This is because **P6B-CD flips a strict Phase 6a hard forbid**: it relaxes G2b to admit a narrowly-scoped recovery-only crypto dep (`k256`), enables `ProductionSigner::sign_tx` to return `Ok(SignedTxBytes)` for the first time in workspace history, and lands the `alloy-rlp` dep that v0.3 P6B-C explicitly deferred. None of these are unilaterally permissible without the user re-authorization step.

This plan describes WHAT the implementation would do once authorized. It does NOT implement anything.

## Scope (v0.1)

P6B-CD is the **single comprehensive sign-activation batch** that closes the three architectural blockers Codex surfaced on the P6B-C v0.2 review:

1. `BundleTx` lacks EIP-1559 fee fields needed for unsigned-tx RLP construction.
2. RLP encoding for the EIP-1559 unsigned-tx preimage is absent.
3. AWS KMS `Sign` returns DER ECDSA signatures without an Ethereum recovery id; computing the yParity byte requires elliptic-curve point recovery that is not implementable without a secp256k1 library.

The batch is bounded by the following hard scope guard:

- **`ProductionSigner::sign_tx` MAY return `Ok(SignedTxBytes)` at P6B-CD close** when ALL of: (a) the signer was constructed via `from_aws_kms*`, (b) `tx.from == derived_address`, (c) the EIP-1559 fee-field invariants hold, (d) the AWS KMS `Sign` call succeeds, (e) DER parsing succeeds, (f) low-s normalization completes, (g) trial-recovery against the boot-time `derived_address` finds a matching yParity in `{0, 1}`.
- **NO `submit_bundle` `Ok(_)` path. NO `live_send=true` enablement. NO `eth_sendBundle` runtime path. NO actual relay submission. NO runtime caller of `ProductionSigner::sign_tx` in `crates/app/src/`.** Those land in P6B-D (live_send relaxation) and P6B-E (submit_bundle Ok + eth_sendBundle runtime). The P6B-CD chain of locks is documented in Section "Chain of locks at P6B-CD close" below.

At P6B-CD close, `ProductionSigner::sign_tx` has an `Ok(_)` return path that is reachable only through tests (the existing `#[cfg(test)] pub(crate) async fn invoke_signer_for_test` hook in `crates/execution`). G11 (production sign_tx call site count) stays at 1; the production call site is still the test-only hook; the runtime caller path lands in P6B-E.

## Architectural-blocker resolution

### Blocker 1 -- BundleTx lacks EIP-1559 fee fields (CD-AB1)

**Resolution:** extend `BundleTx` (signer crate; `#[non_exhaustive]`) with the minimal EIP-1559 fields needed for type-2 unsigned-tx RLP. The plan extends BREAKING the `BundleTx::new(...)` ctor signature; v0.1 LOCKS the breaking ctor change rather than adding a separate `new_eip1559` to avoid two ctors with overlapping responsibilities.

New fields:

```rust
pub struct BundleTx {
    pub from: Address,
    pub to: Address,
    pub value_wei: U256,
    pub data: Vec<u8>,
    pub gas_limit: u64,
    pub nonce: u64,
    pub chain_id: u64,
    pub bundle_correlation_id: u64,
    // NEW (P6B-CD):
    pub max_priority_fee_per_gas: U256,
    pub max_fee_per_gas: U256,
}
```

The plan does NOT add an `access_list` field. The empty access list `[]` is hardcoded in the RLP encoder. Rationale:

- Empty access list is the byte-deterministic MEV-bundle case and matches every test vector.
- Adding the field forces an `AccessListItem` definition (`Address` + `Vec<B256>`) into the signer crate's public surface; deferring keeps the v1 boundary minimal.
- Future extension is `#[non_exhaustive]`-safe (a follow-up batch can land `access_list: Vec<AccessListItem>` with `#[serde(default)]` semantics if real-world bundles ever need it).

**rkyv / serde implications**: `BundleTx` does NOT derive `rkyv::Archive` or `serde::Serialize` at the time of writing (verified by grep before this plan was written). Adding two `U256` fields is therefore an additive, non-rkyv-affecting change. The plan locks this assumption; at impl time the first step is `rg -n 'derive.*(rkyv::Archive|serde::Serialize|Deserialize)' crates/signer/src/bundle_tx.rs` -> expected 0 hits. If hits surface, the plan halts and the rkyv/serde compatibility piece is added as a 1-line ADR amendment proposal.

**`BundleConstructor::cfg.fixed_bid_fraction_bps` impact**: none. The fee-field values are caller-provided. The plan does NOT modify `BundleConstructor::construct(...)` to populate them in P6B-CD; building a runtime `BundleTx` from a `BundleCandidate` is P6B-E scope (Section "Out of scope" below). P6B-CD's runtime contract is "signer accepts a complete `BundleTx` and produces `Ok(SignedTxBytes)`"; the caller path is the P6B-E concern.

**Existing 10-arg `BundleTx::new(...)` callers**: there is exactly one production caller (none) and several test callers in `crates/signer/src/{lib.rs,production.rs}` + `crates/execution/src/lib.rs` (per `rg -n 'BundleTx::new\(' crates/`). The plan updates each call site in lockstep (mechanical edit). No production code reads or writes the fields outside the signer's sign_tx body; this keeps the surface change small.

### Blocker 2 -- RLP encoding for EIP-1559 unsigned-tx preimage (CD-AB2)

**Resolution:** add `alloy-rlp = "0.3"` to `crates/signer/Cargo.toml` (P6B-C v0.3 deferred this dep explicitly; P6B-CD lands it).

`alloy-rlp` is well-maintained, G2b-clean (per pre-impl `cargo tree` dry-run; LOCKED add-then-verify gate at impl time), and provides the exact `RlpEncodable` derive + `BufMut`-based encoders needed. The signer crate already pulls in `alloy-primitives`; `alloy-rlp` shares the same maintainer (foundry-rs / alloy-rs).

**Add-then-verify procedure** at impl time:

1. Add `alloy-rlp = "0.3"` to `crates/signer/Cargo.toml`.
2. `cargo tree -e features -p rust-lmax-mev-signer 2>&1 | rg 'secp256k1|alloy-signer|ethers-signers'` -> expect 0 hits. (NOTE: the regex omits `k256` because P6B-CD intentionally introduces a narrowly-scoped `k256` dep per Blocker 3.)
3. If banned-set hit (other than the planned k256 entry), revert + halt for Codex re-review.

**Unsigned-tx preimage (EIP-1559 type-2)**:

```text
preimage_bytes = 0x02 || rlp([
    chain_id,
    nonce,
    max_priority_fee_per_gas,
    max_fee_per_gas,
    gas_limit,
    to,
    value,
    data,
    access_list,    // empty list []
])
digest = keccak256(preimage_bytes)
```

The plan locks a `pub(crate) fn encode_eip1559_unsigned(tx: &BundleTx) -> Vec<u8>` in a new module `crates/signer/src/rlp.rs` (file is in the G2c/G2d allow-list extension; Section "Gates at P6B-CD close" below). Returns the preimage bytes (NOT the digest; digest is `keccak256(preimage_bytes)` at the sign_tx call site).

**Encoding determinism**: the encoder MUST emit canonical RLP per Ethereum yellow-paper Appendix B (single-byte 0x80 = empty string; minimum encoding for variable-length items). `alloy-rlp` enforces canonical encoding by construction. The plan adds at least one byte-vector test (`D-T-CD2` below) asserting the encoded bytes match a public EIP-1559 test vector.

### Blocker 3 -- AWS KMS DER ECDSA -> Ethereum (r, s, yParity) (CD-AB3) **THE deep design question**

**Acknowledgment:** AWS KMS `Sign` with `SigningAlgorithmSpec::EcdsaSha256` returns a DER-encoded ECDSA signature. The DER blob encodes `(r, s)` as two integers in an ASN.1 SEQUENCE. It does NOT contain the Ethereum recovery id (`yParity`, also historically called `v`).

Computing yParity mathematically requires elliptic-curve point recovery from `(r, s, message_hash)` followed by point-equality comparison against the known public key. This operation has no shortcut without a secp256k1-aware library: point arithmetic over GF(p) with p = 2^256 - 2^32 - 977 is the entire content of the ECDSA `recover_public_key` algorithm.

**Three paths evaluated** (per the user's planning prompt):

| Path | Approach | Recommendation |
|---|---|---|
| (a) | G2b-approved recovery-only crypto-library carve-out (`k256`) | **RECOMMENDED.** Narrow surface, mature crate, no signing operations used. |
| (b) | Switch HSM/signing service to one returning Ethereum-compatible `(r, s, v)` directly (Web3Signer / Fortanix DSM Ethereum profile / etc.) | REJECTED for P6B-CD. Vendor change is a bigger scope item than recovery; introduces new SDK dep, new dep-graph audit, new operational runbook. May be revisited in a future batch if the k256 carve-out is itself rejected at impl review. |
| (c) | Deterministic trial-recovery without a crypto library | INFEASIBLE. Trial-recovery (try yParity=0, try yParity=1, pick the match) is structurally exactly the same as path (a); the elliptic-curve recovery math is unavoidable. "Without a crypto library" would mean hand-rolling secp256k1 modular arithmetic in `crates/signer/` -- vastly more dangerous than depending on `k256`. |

**Path (a) -- LOCKED in v0.1.** The carve-out is structured as follows:

- New dep `k256 = "0.13"` (recovery-only feature set) in `crates/signer/Cargo.toml`. Features list LOCKED at impl time after `cargo tree` audit; default features disabled, only `ecdsa` + `arithmetic` enabled. **NOT** enabled: `ecdsa-core` (signing), `serde`, `keygen`.
- New file `crates/signer/src/recovery.rs` (`pub(crate)` module). The ONLY workspace file allowed to import `k256` symbols.
- New `pub(crate) fn recover_y_parity(digest: &[u8; 32], r: [u8; 32], s: [u8; 32], expected_pubkey_sec1_uncompressed_65: &[u8; 65]) -> Result<u8, SignerError>` function. Body:

```rust
// Pseudo-code; exact form locked at impl time.
use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
use k256::PublicKey;

pub(crate) fn recover_y_parity(
    digest: &[u8; 32],
    r: [u8; 32],
    s: [u8; 32],
    expected_pubkey_sec1_uncompressed_65: &[u8; 65],
) -> Result<u8, SignerError> {
    let sig_bytes: [u8; 64] = concat(r, s);
    let sig = Signature::from_slice(&sig_bytes)
        .map_err(|_| SignerError::InvalidSignatureBytes)?;
    // low-s normalization: EIP-2 / Ethereum-mainnet rejects high-s.
    let sig = sig.normalize_s().unwrap_or(sig);
    let expected = PublicKey::from_sec1_bytes(expected_pubkey_sec1_uncompressed_65)
        .map_err(|_| SignerError::ClientInit)?;
    let expected_vk = VerifyingKey::from(&expected);
    for v in [0u8, 1u8] {
        let rid = RecoveryId::try_from(v).map_err(|_| SignerError::SignatureRecoveryFailed)?;
        if let Ok(recovered) = VerifyingKey::recover_from_prehash(digest, &sig, rid) {
            if recovered == expected_vk {
                return Ok(v);
            }
        }
    }
    Err(SignerError::SignatureRecoveryFailed)
}
```

- **Strict in-source negative invariants** enforced by ripgrep (Section "Gates at P6B-CD close" G2f below):
  - `crates/signer/src/recovery.rs` imports MAY contain `k256::{ecdsa::{Signature, RecoveryId, VerifyingKey}, PublicKey}` only.
  - `k256::SecretKey`, `k256::ecdsa::SigningKey`, `k256::Scalar`, `k256::FieldElement`, `k256::ProjectivePoint`, `k256::AffinePoint`, `k256::elliptic_curve::*` are all FORBIDDEN absolute in `crates/`. The narrow allow-list is enumerated in G2f.
- **R-4 CORRECTED**: the k256 `ecdsa` feature DOES expose `k256::ecdsa::SigningKey` and full signing/verification surface per docs.rs/k256/0.13/k256/ecdsa/. The safety control is NOT a feature-flag exclusion; it is the **G2f narrow-surface allow-list** (`rg`-enforced: no `SigningKey` / `SecretKey` / `Scalar` / `FieldElement` / `ProjectivePoint` / `AffinePoint` / `elliptic_curve` symbol in any `crates/*.rs` file) PLUS the **single-file import gate** (k256 imports allowed only in `crates/signer/src/recovery.rs`, asserted by the G2a follow-up grep `rg -n -e '\bk256\b' crates/ --glob '*.rs' | rg -v '^crates/signer/src/recovery\.rs:'` returning 0). `default-features = false` with `features = ["ecdsa", "arithmetic"]` shrinks the dep's transitive footprint (drops `pkcs8`, `pem`, `precomputed-tables`, `serde`, `std` defaults) but does NOT itself prevent the workspace from instantiating `SigningKey`; the static gates above are what prevent that.
- G2b regex AMENDED at P6B-CD only to allow `k256` in `crates/signer/Cargo.toml` AND in `crates/signer/src/recovery.rs`. Section "ADR-001 amendment proposal" below documents the user-approval-gated scope-lift.

**DER -> (r, s) parsing**: AWS KMS returns DER `SEQUENCE { INTEGER r, INTEGER s }`. The plan uses `k256::ecdsa::Signature::from_der(&der_bytes)` for the parsing step (single function call inside `recovery.rs`). This is a one-line addition and stays inside the same `pub(crate)` module so the allow-list does not grow further.

**Why `from_der`, why not hand-roll DER**: `from_der` is the established, audited, fuzz-tested parser. Hand-rolling DER for ASN.1 INTEGER (which can have leading zeros, can be twos-complement-padded) is error-prone and yields no safety benefit when `k256` is already imported for recovery.

**Low-s normalization**: EIP-2 mandates `s <= n/2` for mainnet ECDSA signatures. AWS KMS may return high-s signatures. The plan applies `Signature::normalize_s()` unconditionally before recovery; the normalized signature is what enters the signed-tx bytes. Plan asserts via test `D-T-CD8` that a known high-s input normalizes to the expected low-s form.

### CD-AB1+AB2+AB3 summary

The three blockers are jointly resolved by:

- **D-CD1**: `BundleTx` extension (Blocker 1).
- **D-CD2**: `alloy-rlp` dep + `pub(crate) fn encode_eip1559_unsigned(...)` (Blocker 2).
- **D-CD3 + D-CD4**: `k256` recovery-only carve-out + `crates/signer/src/recovery.rs` (Blocker 3).
- **D-CD5**: New `SignerError` variants (covered below).
- **D-CD6**: `ProductionSigner::sign_tx` `Ok`-return path assembling the EIP-1559 typed signed-tx bytes from `(0x02, rlp([...header..., y_parity, r, s]))`.

## Deliverables (D-CD1..D-CD10)

### D-CD1 -- `BundleTx` EIP-1559 fee-field extension

`crates/signer/src/bundle_tx.rs`. Adds two `U256` fields: `max_priority_fee_per_gas`, `max_fee_per_gas`. `BundleTx::new(...)` signature BREAKING from 8 args to 10 args; the two new args are appended at the tail. `#[non_exhaustive]` preserved. NO `access_list` field (empty list hardcoded by the encoder).

### D-CD2 -- `alloy-rlp` dep + canonical EIP-1559 unsigned-tx encoder

`crates/signer/Cargo.toml`: additive `alloy-rlp = "0.3"`. Add-then-verify gate per Blocker 2.

`crates/signer/src/rlp.rs` (NEW; G2c/G2d allow-list extension): `pub(crate) fn encode_eip1559_unsigned(tx: &BundleTx) -> Vec<u8>` emits `0x02 || rlp([chain_id, nonce, max_priority_fee_per_gas, max_fee_per_gas, gas_limit, to, value, data, []])`. The empty access list `[]` is encoded as RLP `0xc0` (empty list).

The encoder MUST handle leading-zero stripping for U256/u64 fields (RLP canonical form). `alloy-rlp` derives + encoders handle this.

### D-CD3 -- `k256` recovery-only carve-out

`crates/signer/Cargo.toml`: additive `k256 = { version = "0.13", default-features = false, features = ["ecdsa", "arithmetic"] }`. Add-then-verify gate per Blocker 3 (the G2b regex is amended at P6B-CD; see "Gates at P6B-CD close").

NOTE: the default `k256` features include `pkcs8`, `pem`, `precomputed-tables`, `serde`, `std`. Disabling defaults shrinks the surface; the plan re-verifies the resulting feature set with `cargo tree -e features -p rust-lmax-mev-signer | rg '^k256'` at impl time and pins exactly the resolved feature set in a comment in the Cargo.toml line.

### D-CD4 -- `crates/signer/src/recovery.rs` (NEW; G2c/G2d allow-list extension)

Single file housing the entire `k256` surface used by the workspace. `pub(crate)` module. Exports:

- `pub(crate) fn recover_y_parity(digest, r, s, expected_pubkey_sec1_65) -> Result<u8, SignerError>`
- `pub(crate) fn parse_der_to_rs(der: &[u8]) -> Result<([u8; 32], [u8; 32]), SignerError>` -- wraps `k256::ecdsa::Signature::from_der(...).normalize_s().to_bytes()` and splits into r + s.

NO other `k256` symbols imported anywhere in `crates/`.

### D-CD5 -- `SignerError` payload-free variant additions

`crates/signer/src/error.rs`. Adds payload-free variants (preserves `Copy`):

| Variant | Display text | Reached when |
|---|---|---|
| `InvalidSignatureBytes` | "kms signature is not a valid ECDSA signature" | `Signature::from_slice`/`from_der` failure inside `recovery::parse_der_to_rs` |
| `SignatureRecoveryFailed` | "kms signature did not recover to derived address" | trial-recovery (`yParity in {0, 1}`) failed to match `derived_address` |
| `InvalidBundleTx` | "bundle tx invariants violated" | pre-sign invariant check (e.g., `max_fee_per_gas < max_priority_fee_per_gas`) |
| `KmsSignFailed` | "kms sign call failed" | `client.sign_digest(...)` returned `Err(KmsClientError::SignFailed)` |

Four NEW variants. All `Display` text is fixed (no payload bytes / no AWS error body / no key fingerprint). `SignerError: Copy + Eq` preserved (all variants payload-free).

### D-CD6 -- `ProductionSigner::sign_tx` `Ok`-return path

`crates/signer/src/production.rs`. v0.3 body shape (introduced in v0.2 plan revision; preserves all P6B-C `Err` paths verbatim; appends the `Ok` path; **every return path emits audit + counter exactly once** per G15):

```rust
async fn sign_tx(&self, tx: &BundleTx) -> Result<SignedTxBytes, SignerError> {
    // P6B-C invariant: address consistency. UNCHANGED.
    if let Some(derived) = self.derived_address {
        if tx.from != derived {
            emit_attempt_audit(self, tx, "address_mismatch");
            counter("address_mismatch");
            return Err(SignerError::AddressMismatch);
        }
    }
    // P6B-CD: legacy `new(...)` path OR any boot path where the three
    // KMS-derived Options are not jointly Some(...) stays NotConfigured.
    // `from_aws_kms_with_client` is the only setter and sets all three
    // atomically, so this destructure is the single fail-closed entry
    // for "no live KMS attached." NO panic path (R-3).
    let (Some(client), Some(derived), Some(pubkey_sec1_65)) = (
        self.client.as_ref(),
        self.derived_address,
        self.pubkey_sec1_65,
    ) else {
        emit_attempt_audit(self, tx, "not_configured");
        counter("not_configured");
        return Err(SignerError::NotConfigured);
    };
    let _ = derived; // already matched above; held for clarity.

    // P6B-CD NEW: bundle-tx invariant check.
    if tx.max_fee_per_gas < tx.max_priority_fee_per_gas {
        emit_attempt_audit(self, tx, "invalid_bundle_tx");
        counter("invalid_bundle_tx");
        return Err(SignerError::InvalidBundleTx);
    }

    // P6B-CD NEW: encode + sign.
    let preimage = crate::rlp::encode_eip1559_unsigned(tx);
    let digest = keccak256(&preimage);
    let der = match client.sign_digest(&self.audit_key_id, &digest.0).await {
        Ok(d) => d,
        Err(_) => {
            emit_attempt_audit(self, tx, "kms_sign_failed");
            counter("kms_sign_failed");
            return Err(SignerError::KmsSignFailed);
        }
    };

    // R-2: DER parse failure is an AUDITED + COUNTED outcome.
    let (r, s) = match crate::recovery::parse_der_to_rs(&der) {
        Ok(rs) => rs,
        Err(_) => {
            emit_attempt_audit(self, tx, "invalid_signature_bytes");
            counter("invalid_signature_bytes");
            return Err(SignerError::InvalidSignatureBytes);
        }
    };

    // R-2: trial-recovery failure is an AUDITED + COUNTED outcome.
    let y_parity = match crate::recovery::recover_y_parity(&digest.0, r, s, &pubkey_sec1_65) {
        Ok(v) => v,
        Err(_) => {
            emit_attempt_audit(self, tx, "signature_recovery_failed");
            counter("signature_recovery_failed");
            return Err(SignerError::SignatureRecoveryFailed);
        }
    };

    // P6B-CD NEW: assemble signed bytes.
    let signed = crate::rlp::encode_eip1559_signed(tx, y_parity, &r, &s);
    emit_attempt_audit(self, tx, "ok");
    counter("ok");
    Ok(SignedTxBytes(signed))
}
```

Notes on the v0.3 body (unchanged in shape from v0.2; documented here as the current locked form):

- **R-3 RESOLVED**: NO `expect(...)` / `unwrap()` / `panic!()` / `assert!()` on a runtime path. The three KMS-derived Options (`client`, `derived_address`, `pubkey_sec1_65`) are destructured jointly; any `None` falls through to the audited `NotConfigured` exit. `from_aws_kms_with_client` is the ONLY setter and sets all three atomically.
- **R-2 RESOLVED**: every `Err(_)` return is preceded by an `emit_attempt_audit(...)` + `counter(...)` pair with a unique outcome label. G15 audit-surface contract is preserved verbatim: every `sign_tx` attempt increments the counter exactly once with a label drawn from the 7-element set `{not_configured, address_mismatch, invalid_bundle_tx, kms_sign_failed, invalid_signature_bytes, signature_recovery_failed, ok}`. Two new labels (`invalid_signature_bytes`, `signature_recovery_failed`) added at P6B-CD; their tests are D-T-CD9 + D-T-CD10 (Test table below).
- `pubkey_sec1_65: Option<[u8; 65]>` is a NEW `ProductionSigner` field captured at boot alongside `derived_address`. It is the `0x04 || x || y` SEC1-uncompressed point bytes; NOT a key fingerprint, NOT key material per `production-signer.md` Section 2.3(b). The field is `Option` so the legacy `new(...)` path retains `None`; `from_aws_kms_with_client` is the only setter.
- `counter(...)` and `emit_attempt_audit(...)` are existing helpers from P6B-C; the new outcome label set adds `invalid_bundle_tx`, `kms_sign_failed`, `invalid_signature_bytes`, `signature_recovery_failed`, `ok` (5 new labels) to the P6B-C `{not_configured, address_mismatch}` set. **The sample Alertmanager YAML at `config/examples/signing-audit-alert.yaml` MUST be updated** at P6B-CD impl time to narrow the `SigningFailureRateHigh` matcher from `{outcome!="not_configured"}` to a positive enumeration of failure-only outcomes (R-1 fix; see D-CD9 below).

### D-CD7 -- `pub(crate) fn encode_eip1559_signed(...)`

`crates/signer/src/rlp.rs`. Companion encoder for the signed-tx bytes:

```text
signed_bytes = 0x02 || rlp([
    chain_id, nonce,
    max_priority_fee_per_gas, max_fee_per_gas,
    gas_limit, to, value, data,
    access_list,  // []
    y_parity, r, s,
])
```

Returns `Vec<u8>` suitable for direct insertion into `eth_sendRawTransaction` (P6B-E concern; NOT P6B-CD).

### D-CD8 -- Static validation in `Config::validate()`

`crates/config/src/lib.rs`. **NO new config field in P6B-CD.** The EIP-1559 fee values are per-bundle (BundleTx-carried), not config-level. The plan does not add a `[relay.fee_bounds]` section in P6B-CD; that could be a P6B-D scope item if operators need a workspace-level fee ceiling.

Existing P6B-B + P6B-C rejects all stay in force. NO `crates/config/` change in P6B-CD.

### D-CD9 -- Boundary doc + sample Alertmanager YAML reconciliation

`docs/specs/phase-6b-boundary.md`. Additive amendment to Section 3 reconciliation paragraph: P6B-CD is the sign-activation batch that closes the v0.3 reconciliation's deferred prerequisite #5. After P6B-CD closes:

- The "P6B-B + P6B-C jointly pre-activation" framing is superseded; P6B-CD is now the activation batch.
- Prerequisite #5 (Section 2 review against an `Ok`-returning impl) becomes the Codex review of THIS batch (P6B-CD).
- The chain of locks at P6B-CD close: `submit_bundle -> Err`, `live_send=true` rejected, `eth_sendBundle` runtime absent. P6B-D remains the `live_send=true` capability flip; P6B-E remains the `submit_bundle -> Ok` + `eth_sendBundle` runtime batch.

Also additive at Section 5 (per-callsite documentation): P6B-CD does NOT add a new production `sign_tx` call site. G11 stays at 1. The Section 5 table stays empty at P6B-CD close.

**R-1 RESOLVED -- `config/examples/signing-audit-alert.yaml` IS A TOUCHED FILE at P6B-CD impl time.** The P6B-C sample uses `{outcome!="not_configured"}` in the `SigningFailureRateHigh` rule. At P6B-CD, the outcome-label set grows to include `ok` (successful signing). Without a YAML update, `SigningFailureRateHigh` and the `max_failed_per_minute` threshold would fire on successful-signing throughput -- which inverts the alert's intent. The YAML edit narrows the matcher to a positive enumeration of failure-only outcomes:

```yaml
# Before (P6B-C):
expr: |
  (
    (sum(rate(production_signer_audit_attempts_total{outcome!="not_configured"}[1m])) * 60)
    > on() production_signer_audit_alert_threshold_max_failed_per_minute
  )
  and on() (production_signer_audit_alert_threshold_max_failed_per_minute > 0)

# After (P6B-CD):
expr: |
  (
    (sum(rate(production_signer_audit_attempts_total{outcome=~"address_mismatch|invalid_bundle_tx|kms_sign_failed|invalid_signature_bytes|signature_recovery_failed"}[1m])) * 60)
    > on() production_signer_audit_alert_threshold_max_failed_per_minute
  )
  and on() (production_signer_audit_alert_threshold_max_failed_per_minute > 0)
```

Plus comment + annotation rewrites:

- Top-of-file comment block: enumerate the 7 outcome labels at P6B-CD close `{ok, not_configured, address_mismatch, invalid_bundle_tx, kms_sign_failed, invalid_signature_bytes, signature_recovery_failed}` and note that `SigningAttemptRateHigh` still scrapes all 7 (operator-attack-surface signal: total signing-attempt rate), while `SigningFailureRateHigh` matches ONLY the 5 failure outcomes.
- `SigningFailureRateHigh` `description` rewritten: "Non-success signing-attempt rate (sum over `address_mismatch`, `invalid_bundle_tx`, `kms_sign_failed`, `invalid_signature_bytes`, `signature_recovery_failed`) exceeded the operator-configured `max_failed_per_minute` gauge. The `ok` outcome label is EXCLUDED from this rule by construction; `not_configured` is excluded as a fail-closed no-op state. Rule is gated on the gauge being > 0; a gauge value of 0 means the operator disabled this alert at the workspace TOML config level."

The `SigningAttemptRateHigh` rule is UNCHANGED: it scrapes `production_signer_audit_attempts_total` (no outcome filter) and represents the operator-attack-surface signal that the workspace is being asked to sign at high rate, regardless of outcome.

NO edit to `docs/specs/production-signer.md` (Section 4 unlock checklist is unchanged at P6B-CD close because the production-signer contract was already locked at P6B-A; satisfying it is exactly what P6B-CD does). NO edit to `docs/specs/execution-safety.md`. NO edit to `docs/specs/phase-6a-boundary.md`.

### D-CD10 -- ADR-001 narrow recovery-only carve-out amendment

See "ADR-001 amendment proposal" Section below. Subject to explicit user authorization. NOT a P6B-CD-required edit if the user rejects the amendment; in that case, Path (b) (alternate HSM service) becomes the only viable path and P6B-CD is reworked accordingly.

## Out of scope (explicitly NOT P6B-CD)

- Building a runtime `BundleTx` from a `BundleCandidate`. The execution-driver path that takes `OpportunityEvent -> SimulationOutcome -> BundleCandidate -> BundleTx -> sign_tx` is P6B-E scope.
- `submit_bundle -> Ok(_)` path. P6B-E.
- `live_send=true` relaxation. P6B-D.
- `eth_sendBundle` runtime call. P6B-E.
- Any addition of an `access_list` field on `BundleTx`. Deferred; empty list is the encoder default. A future batch can extend if real-world bundles need it.
- Any change to `Config::validate()`-time fee-bound rejects. P6B-D scope if needed.
- Phase 6b `phase-6b-complete` tag creation. P6B-F scope.
- Any change to the per-adapter `KillSwitch` wiring (P6B-D / P6B-E may extend; P6B-CD does not touch it).
- Any change to `MempoolSourceKind` / external mempool. Out of Phase 6b entirely.

## Tests (10 minimum targeted; v0.2 LOCKED)

v0.1 had 8; v0.2 adds D-T-CD9 + D-T-CD10 per R-2 (audit + counter assertions on the new outcome labels). All `#[cfg(test)]` in `crates/signer/`; no live network, no live KMS, no new `#[ignore]` test. **Test-vector sources LOCKED in v0.2 per R-5**.

### LOCKED test-vector sources (R-5; R-7 corrected in v0.3)

**Hard constraint (v0.3 R-7)**: the implementation MUST copy ONLY non-secret / public material from the named sources -- preimage, digest, signed-tx bytes, `r`, `s`, expected `y_parity`, expected SEC1 public-key bytes (`0x04 || x || y`), DER signature bytes. **NO private-key bytes are ever copied into the repo, EVEN from a published test source.** If a derived value (e.g., the SEC1 public key) is needed in a test, the IMPLEMENTER precomputes it OUTSIDE the repo from the public source and pastes ONLY the precomputed non-secret bytes with a doc comment naming the precomputation step ("precomputed off-tree from `<source>` at commit `<sha>`; pasted SEC1 pubkey bytes only").

| Use | Source | Empty access list? | What the test copies (NON-SECRET ONLY) |
|---|---|---|---|
| D-T-CD2 unsigned RLP | alloy-rs `alloy-consensus` crate, file `crates/consensus/src/transaction/eip1559.rs`, the EIP-1559 RLP serialization round-trip test fixture. Implementer pins the exact GitHub commit SHA + line range in a doc comment beside the byte literals at impl time. | YES (the fixture's `access_list` is `vec![]`). | The exact 9-tuple `(chain_id, nonce, max_priority_fee_per_gas, max_fee_per_gas, gas_limit, to, value, data, access_list=[])` AND the expected unsigned-tx byte vector (typed envelope `0x02` + canonical RLP). NO private key involved. |
| D-T-CD4 recovery known vector | go-ethereum, file `core/types/transaction_signing_test.go`, function `TestEIP1559Signing` -- used ONLY as a public source from which the implementer harvests the NON-SECRET signed-tx outputs off-tree. Implementer pins the exact GitHub commit SHA + line range in a doc comment. | YES (the test's `AccessList` is `nil` -> RLP empty list). | **Non-secret material ONLY**: the message digest (`keccak256(0x02 \|\| rlp(unsigned))`), the signature `(r, s)`, the expected `y_parity`, the SEC1-uncompressed public-key bytes `[u8; 65]` (precomputed OFF-TREE from the go-ethereum source's public test value at the named commit; pasted as a `const PUBKEY_SEC1_65: [u8; 65] = [...];` literal with a doc comment naming the precomputation step), and the expected recovered `Address`. **NO `test_key` byte literal, NO private-key bytes, NO `SecretKey` / `SigningKey` constructor anywhere in the test file.** The G2g ripgrep gate (Section "Gates at P6B-CD close" G2g below) enforces this. |
| D-T-CD8 high-s normalization | Synthetic. Constructed from the D-T-CD4 base `(r, s_low, y_parity_low)` as `s_high = secp256k1_order - s_low`, `y_parity_high = 1 - y_parity_low`. No external source needed; the property is purely mathematical. The `secp256k1_order` constant is the named curve order published in SEC2 Section 2.4.1 and copied as a hardcoded `[u8; 32]` literal. | N/A (synthetic). | The `(r, s_low, y_parity_low)` base (from D-T-CD4's non-secret materials) + the synthetic `(r, s_high, y_parity_high)` check + the `secp256k1_order` constant. NO private key involved. |

**Why these sources**: both alloy-rs and go-ethereum are widely-audited reference implementations of EIP-1559. Pinning the source + commit SHA gives implementer-reviewer parity; if either upstream changes between plan-time and impl-time, the implementer notes the divergence in the doc comment and proceeds with the pinned commit's bytes. The locked vectors have empty access lists, matching the P6B-CD encoder's hardcoded empty access list (eliminates an impl-time fork on access-list shape). The D-T-CD4 source is used as a public REFERENCE only; the actual byte literals committed to the repo are derived non-secret outputs (signature components + public-key bytes + digest), NEVER any private-key byte from the source.

### Test table (10 tests)

| ID | File / location | Test | Asserts |
|---|---|---|---|
| D-T-CD1 | `crates/signer/src/bundle_tx.rs` | `bundletx_eip1559_field_construction` | `BundleTx::new(..., max_priority_fee_per_gas, max_fee_per_gas)` constructs with the new fields; field access yields the values passed in. |
| D-T-CD2 | `crates/signer/src/rlp.rs` `#[cfg(test)]` | `encode_eip1559_unsigned_known_vector` | `encode_eip1559_unsigned(tx)` emits bytes matching the LOCKED alloy-consensus fixture (empty access list). Byte-for-byte equality. |
| D-T-CD3 | `crates/signer/src/recovery.rs` `#[cfg(test)]` | `parse_der_to_rs_rejects_malformed` | Malformed DER (wrong tag, truncated, valid prefix + garbage trailer) -> `Err(SignerError::InvalidSignatureBytes)`. No panic. Three sub-cases. |
| D-T-CD4 | `crates/signer/src/recovery.rs` `#[cfg(test)]` | `recover_y_parity_known_vector` | Given the LOCKED go-ethereum `TestEIP1559Signing` `(digest, r, s, pubkey_sec1_65)` tuple, `recover_y_parity(...)` returns the expected `y_parity in {0, 1}`. |
| D-T-CD5 | `crates/signer/src/production.rs` `#[cfg(test)]` | `sign_tx_happy_path_with_mock_kms_returns_deterministic_signed_bytes` | Mock client returns canned DER (derived from the LOCKED D-T-CD4 vector) for a canned digest; signer returns `Ok(SignedTxBytes(bytes))` starting with `0x02` and with expected length-shape; byte-stable across two runs; `outcome="ok"` audit + counter emitted (R-2 audit-completeness check on the happy path). |
| D-T-CD6 | `crates/signer/src/production.rs` `#[cfg(test)]` | `sign_tx_address_mismatch_fails_before_signing` | `BundleTx::from != derived_address` -> `Err(AddressMismatch)`; mock's `sign_digest` is NEVER called (`AtomicUsize` call counter on mock asserts 0). |
| D-T-CD7 | `crates/signer/src/production.rs` `#[cfg(test)]` | `sign_tx_kms_sign_error_maps_to_kms_sign_failed` | Mock `sign_digest -> Err(KmsClientError::SignFailed)` -> `sign_tx -> Err(KmsSignFailed)`; `outcome="kms_sign_failed"` audit + counter emitted. |
| D-T-CD8 | `crates/signer/src/recovery.rs` `#[cfg(test)]` | `recover_y_parity_normalizes_high_s` | Synthetic high-s from D-T-CD4 (`s_high = secp256k1_order - s_low`, `y_parity_high = 1 - y_parity_low`); `recover_y_parity` normalizes via `Signature::normalize_s()` and returns the LOW-s vector's `y_parity_low`. |
| D-T-CD9 (R-2 NEW) | `crates/signer/src/production.rs` `#[cfg(test)]` | `sign_tx_invalid_der_emits_invalid_signature_bytes_audit_and_counter` | Mock `sign_digest -> Ok(b"\xDE\xAD")` (invalid DER) -> `sign_tx -> Err(InvalidSignatureBytes)`; `outcome="invalid_signature_bytes"` audit + counter emitted. G15 audit-completeness invariant. |
| D-T-CD10 (R-2 NEW) | `crates/signer/src/production.rs` `#[cfg(test)]` | `sign_tx_unrecoverable_sig_emits_signature_recovery_failed_audit_and_counter` | Mock `sign_digest -> Ok(valid_der_for_OTHER_key)` (DER parses but does not recover to the boot-time `pubkey_sec1_65`) -> `sign_tx -> Err(SignatureRecoveryFailed)`; `outcome="signature_recovery_failed"` audit + counter emitted. G15 audit-completeness invariant. |

Workspace targeted count at P6B-CD close (target; verified via `cargo test -p signer -p config -p app`): **49 + 10 = 59 passed + 0 ignored**. (Pre-P6B-CD baseline of 49 carries forward from P6B-C closeout.)

**NO live-network test, NO live-KMS test, NO new `#[ignore]` test** in P6B-CD. G7 stays at 1.

## Gates at P6B-CD close (deltas vs P6B-C close `ff2edbc`)

### G2a -- signer-symbol literal scan in `crates/*.rs`

`rg -n -e 'Wallet|PrivateKey|secp256k1|\bk256\b|sign_transaction|\bfunded\b' crates/ --glob '*.rs'`

**Status at P6B-CD close**: NEW PROVISIONAL HITS for `\bk256\b`. The single permitted location is `crates/signer/src/recovery.rs`. Allow-list extension is +1 file. **NO** new hits anywhere else in `crates/*.rs`. NO `secp256k1`, `Wallet`, `PrivateKey`, `sign_transaction`, or `funded` literal anywhere.

The regex itself is NOT amended; the existing regex CONTINUES to scan and reports the count. The post-P6B-CD assertion is that the only hits are inside the named allow-list file, asserted by a follow-up grep:

```text
rg -n -e '\bk256\b' crates/ --glob '*.rs' | rg -v '^crates/signer/src/recovery\.rs:'
```

This second grep MUST return 0 hits. If anything outside `recovery.rs` references `k256`, P6B-CD is a REVISION REQUIRED at the audit step.

### G2b -- signer-dep literal scan in `crates/**/Cargo.toml`

**LOCKED PROVISIONAL AMENDMENT at P6B-CD** (subject to ADR-001 user-approval at the implementation gate):

Pre-P6B-CD G2b regex: `'alloy-signer|ethers-signers|secp256k1|\bk256\b'` -> expected 0 hits.

P6B-CD G2b regex: `'alloy-signer|ethers-signers|secp256k1'` -> expected 0 hits absolute (k256 dropped from the regex).

The k256 entry SHIFTS from "absolute forbid" to "allowed only in `crates/signer/Cargo.toml`". The grep gate becomes a two-step check:

```text
# Step 1: nothing in the original banned set.
rg -n --glob 'crates/**/Cargo.toml' -e 'alloy-signer' -e 'ethers-signers' -e 'secp256k1'
# Expected: 0 hits.

# Step 2: k256 appears in exactly one Cargo.toml, only as a dependency on the locked version + locked feature set.
rg -n --glob 'crates/**/Cargo.toml' -e 'k256' | rg -v '^crates/signer/Cargo\.toml:'
# Expected: 0 hits.
```

This gate change is the SINGLE Phase 6a HSI relaxation that P6B-CD lands. It is explicitly user-authorization-gated per the ADR-001 amendment proposal below.

### G2c / G2d -- signer-symbol allow-list

Pre-P6B-CD: 11 files (the P6B-C close count).

P6B-CD: **13 files** (+2: `crates/signer/src/recovery.rs`, `crates/signer/src/rlp.rs`).

### G2e -- signer dep edges

Pre-P6B-CD: 2 (`crates/execution/Cargo.toml`, `crates/app/Cargo.toml`).
P6B-CD: **2 UNCHANGED**. The `k256` and `alloy-rlp` deps are signer-internal; nothing new depends on `crates/signer`.

### G2f -- NEW: k256 narrow-surface allow-list

```text
rg -n -e 'k256::(SecretKey|ecdsa::SigningKey|Scalar|FieldElement|ProjectivePoint|AffinePoint|elliptic_curve)' crates/ --glob '*.rs'
```

Expected: **0 hits absolute**. The narrow surface that IS allowed (in `recovery.rs` only) is:

- `k256::ecdsa::{Signature, RecoveryId, VerifyingKey}`
- `k256::PublicKey`

A separate positive grep verifies that these symbols appear in `recovery.rs`:

```text
rg -n 'use k256::' crates/signer/src/recovery.rs
```

Expected: at least 1 hit (the import line for the recovery surface).

### G2g -- NEW: absolute ban on private-key bytes / signing-key constructors (R-7)

Enforces the v0.3 R-7 invariant that NO `test_key` / private-key byte literal / signing-key constructor appears anywhere in `crates/`, EVEN inside `#[cfg(test)]` blocks. The published-test-key exception that v0.2 introduced is REMOVED in v0.3.

Negative grep gate (MUST return 0 hits absolute, including test files):

```text
rg -n -e 'test_key|TEST_KEY|TEST_PRIV|TEST_PRIVATE|SecretKey|SigningKey|from_bytes_be|from_slice_be|::random\(|::generate\(' crates/ --glob '*.rs'
```

NOTE: the `from_bytes_be` / `from_slice_be` / `::random(` / `::generate(` terms target the conventional secp256k1 / k256 / ecdsa-rs private-key construction surfaces. The grep is intentionally over-broad to fail any future caller that tries to reconstruct a private key from byte material; legitimate uses (which there should be none in `crates/signer`) can request a per-symbol G2g exception in a future plan with its own Codex review.

Positive grep gate (any test file referencing private-key-shaped material MUST instead reference precomputed SEC1 pubkey bytes):

```text
rg -n 'PUBKEY_SEC1_65|pubkey_sec1_65' crates/signer/src/recovery.rs
```

Expected: >= 1 hit (D-T-CD4 references the precomputed-off-tree pubkey constant).

G2g is in force at P6B-CD close AND in any later batch; relaxing it requires an ADR-001 amendment of its own.

**Pre-existing baseline hit must be removed at P6B-CD impl time (v0.4 R-9 fix).** Running the G2g grep against the current `master` HEAD `ff2edbc` surfaces ONE hit:

```text
crates/signer/src/production.rs:70:/// `SecretKey`-style type. NO key fingerprint as a struct field.
```

This is the existing P6B-B negative-invariant doc comment that LITERALLY contains the banned token while ASSERTING the same invariant. To make G2g pass cleanly without an allow-list carve-out, the P6B-CD implementation MUST rewrite that line so the doc-comment invariant is preserved but the literal `SecretKey` is removed. Recommended rewrite (locked at impl time, exact wording per implementer; the invariant text MUST still convey "no opaque-secret-byte private-key struct field"):

```text
// Before (production.rs:70):
/// `SecretKey`-style type. NO key fingerprint as a struct field.

// After (production.rs:70, P6B-CD impl):
/// no opaque private-key-byte struct field of any shape. NO key fingerprint as a struct field.
```

The `production.rs` file is already in the P6B-CD touch set (D-CD6 substantive rewrite + new `pubkey_sec1_65` field); the doc-comment cleanup is a 1-line diff folded into the same edit. G2g remains "0 hits absolute" at P6B-CD close.

### G3 -- `submit_bundle(` callers in `crates/app/src/`

UNCHANGED at 0. P6B-CD adds no app caller.

### G4 -- `dyn BundleRelay` / `Arc<dyn BundleRelay>` in `crates/app/src/`

UNCHANGED at 0.

### G5 -- config-validation rejects (`LiveSendForbidden` + P6B-B 3 rules + new ones?)

UNCHANGED at P6B-CD close. The `live_send=true` reject stays absolute in P6B-CD (P6B-D's job to relax).

### G6 -- `api_key` in `tracing::*!`

UNCHANGED at 0.

### G7 -- `#[ignore]` count

UNCHANGED at 1 (P2-C carry-forward). NO new live-KMS / live-network test added.

### G8 -- `cargo tree -d` workspace dep cycles

UNCHANGED at 0. `alloy-rlp` and `k256` are leaf deps; no cycle.

### G9 -- KillSwitch allow-list

UNCHANGED at the 3-file allow-list (`crates/bundle-relay/`, `crates/relay-clients/`, `crates/app/`).

### G10 -- per-adapter `submit_bundle` first-statement kill-switch guard

UNCHANGED. P6B-CD does not touch `submit_bundle` adapters.

### G11 -- production `sign_tx` call site count

UNCHANGED at 1 (the existing `#[cfg(test)] pub(crate) async fn invoke_signer_for_test` hook in `crates/execution/src/lib.rs`). P6B-CD does not add a runtime caller; the runtime caller is P6B-E.

NOTE: At P6B-CD close, the existing call site CAN now return `Ok(_)` from `sign_tx` for the first time. This is a semantic change without a callsite-count change. The change is exercised only by tests; no production runtime path reaches it until P6B-E.

### G12 / G13 / G14 -- submit_bundle pre-check chain / live_send profile scope / eth_sendBundle runtime doc

UNCHANGED at P6B-CD close. G12 stays vacuously satisfied (0 callers). G13 stays "rejected for all profiles". G14 stays at 5 `//!` doc-comment hits.

### G15 -- production-signer audit-surface contract (P6B-C-introduced)

PRESERVED four-piece surface (boot identifier, per-attempt counter, threshold gauges, sample YAML). The counter's outcome-label set grows by 5 labels at P6B-CD close: `ok`, `kms_sign_failed`, `invalid_bundle_tx`, `invalid_signature_bytes`, `signature_recovery_failed`. The sample YAML's `SigningFailureRateHigh` matcher narrows per R-1 to the positive 5-failure enumeration `{outcome=~"address_mismatch|invalid_bundle_tx|kms_sign_failed|invalid_signature_bytes|signature_recovery_failed"}`; `SigningAttemptRateHigh` is unchanged (scrapes all 7 labels including `ok`). G15 audit-completeness invariant ("every `sign_tx` attempt increments the counter + emits an audit event exactly once") is enforced per-path by the v0.2 sign_tx body shape (D-CD6); tests D-T-CD5 / D-T-CD7 / D-T-CD9 / D-T-CD10 assert per-outcome.

## Hard forbids at P6B-CD close

- NO `submit_bundle -> Ok(_)` path. The relay adapter impls continue to return `Err(KillSwitchActive)` or `Err(SubmitDisabled)`.
- NO `live_send=true` enablement. `ConfigError::LiveSendForbidden` reject preserved verbatim.
- NO `eth_sendBundle` runtime call site. G14 stays at 5 `//!` doc-comment hits.
- NO actual relay submission.
- NO live-network test enabled by default. NO live-KMS test by default.
- NO private-key bytes / seed / wallet / raw secret / env-example key material anywhere in repo / tests / fixtures / configs / env examples / build artifacts / runtime memory. The k256 carve-out uses ONLY public-key recovery; no private-key construct is reachable.
- **NEW v0.3 (R-7)**: NO `test_key`, NO published-test-key byte literal, NO `SecretKey`, NO `SigningKey`, NO `from_bytes_be` / `from_slice_be` / `::random()` / `::generate()` signing-key constructor in `crates/` -- **absolute, no test exception**. The G2g grep gate enforces this. Tests that need a known-good ECDSA recovery vector consume ONLY non-secret material (preimage, digest, signed-tx bytes, `r`, `s`, `y_parity`, precomputed-off-tree SEC1 public-key bytes, DER signature bytes); private-key derivation -- even from a published source -- happens OFF-TREE and only the non-secret outputs are pasted into the repo. This invariant supersedes the v0.2 "public test-vector private-key" exception and aligns the plan with `production-signer.md` Section 2.1 / 2.2 + `phase-6b-boundary.md` Section 6 / 7.
- NO `submit_bundle(` caller in `crates/app/src/`.
- NO `dyn BundleRelay` / `Arc<dyn BundleRelay>` in `crates/app/src/`.
- NO change to `crates/app/src/main.rs`.
- NO change to `crates/app/tests/wire_phase4.rs`.
- NO change to `crates/signer/src/{signer_trait,disabled,kms_aws,kms_client}.rs` outside narrow `SignerError`-variant additions visible through `error.rs`.
- NO change to `Profile` / `KeyBackend` enums or the 3 P6B-B validation rules.
- NO ADR edit OUTSIDE the explicit ADR-001 narrow-recovery-only amendment proposal (Section below; user-authorization-gated).
- NO edit to `docs/specs/production-signer.md` (Section 2 contract is satisfied AS-IS by the k256 carve-out; the contract does not forbid recovery-only crypto libraries -- it forbids private-key-byte custody, which the carve-out preserves).
- NO edit to `docs/specs/execution-safety.md`.
- NO edit to `docs/specs/phase-6a-boundary.md`.
- NO edit to `docs/superpowers/plans/` other than this plan file + (post-impl) the P6B-CD closeout reference.
- NO `.coordination/` staging (gitignored).
- NO `AGENTS.md`, `fixture_output.txt`, `hook_toast.md` staging.
- NO destructive git (force push, reset --hard, branch delete, tag overwrite).
- NO `phase-6b-complete` tag creation (P6B-F scope).

## ADR-001 amendment proposal (subject to explicit user authorization at implementation gate)

ADR-001 currently lists `secp256k1`, `k256`, `alloy-signer`, `ethers-signers` in the forbidden signer-dep set. P6B-CD requires the ADR to be amended to permit `k256` in the narrowly-scoped recovery-only context defined in this plan.

**Proposed amendment text** (additive; preserves the rest of ADR-001):

```text
## Amendment 2 -- P6B-CD k256 recovery-only carve-out (proposed; subject to user authorization)

ADR-001's signer-dep forbid set is amended as follows:

- `secp256k1`: REMAINS forbidden absolute in `crates/**/Cargo.toml`.
- `alloy-signer`, `ethers-signers`: REMAIN forbidden absolute.
- `k256`: PERMITTED in `crates/signer/Cargo.toml` ONLY, with `default-features = false` and feature set `["ecdsa", "arithmetic"]`. The dep MUST be used solely through `crates/signer/src/recovery.rs`. The k256 narrow-surface allow-list (G2f) enumerates the permitted symbols (`k256::ecdsa::{Signature, RecoveryId, VerifyingKey}`, `k256::PublicKey`). All other k256 symbols are forbidden absolute.

The carve-out is justified by:
1. Public-key recovery is structurally a one-way operation: input is `(message_hash, r, s, recovery_id)`; output is a `VerifyingKey`. No private-key construct is REACHED from `recover_from_prehash`.
2. The narrow allow-list (G2f) makes the workspace's k256 usage review-able by a single ripgrep: `rg -n -e 'k256::(SecretKey|ecdsa::SigningKey|Scalar|FieldElement|ProjectivePoint|AffinePoint|elliptic_curve)' crates/ --glob '*.rs'` MUST return 0 hits absolute. **The k256 `ecdsa` feature CAN expose `SigningKey` per the upstream crate docs; the workspace ban is enforced by the static gate, not by the feature set.**
3. The single-file import gate (k256 imports allowed only in `crates/signer/src/recovery.rs`) bounds the review surface: any future k256 reference outside `recovery.rs` is a P6B-F audit blocker.
4. The `default-features = false, features = ["ecdsa", "arithmetic"]` pin shrinks the transitive dep tree (no `pkcs8`, `pem`, `precomputed-tables`, `serde`, `std` defaults) which is a hygiene win, not a safety property.

The carve-out does NOT relax `docs/specs/production-signer.md` Section 2.1 (HSM/KMS-only custody) or Section 2.2 (never-in-memory key material): private-key bytes still live in AWS KMS and never enter the workspace process.
```

If the user REJECTS this amendment at the implementation re-authorization step, P6B-CD reverts to Path (b) (alternate HSM signing service returning Ethereum-compatible `(r, s, v)` directly). This would be a substantially different batch with a different vendor-SDK dep audit; the v0.1 plan does NOT pre-draft Path (b) but states it as the documented fallback.

## Boundary doc reconciliation

`docs/specs/phase-6b-boundary.md` additive amendments at P6B-CD impl time:

1. Section 3 reconciliation paragraph: add a new bullet "P6B-CD lands the sign-activation infrastructure: BundleTx EIP-1559 fields, RLP encoder, k256 recovery-only carve-out, KMS signature assembly. `sign_tx` may return `Ok(SignedTxBytes)`. P6B-D and P6B-E remain locked behind this batch's close + separate user re-authorization per non-goal."
2. Section 4 G15 audit-surface contract: add the five new outcome labels (`ok`, `kms_sign_failed`, `invalid_bundle_tx`, `invalid_signature_bytes`, `signature_recovery_failed`) to the documented label-set list; document that `SigningFailureRateHigh` in the sample YAML matches only the 5 failure outcomes and `SigningAttemptRateHigh` matches the full 7-label set.
3. Section 4 NEW G2f narrow-surface allow-list documentation (mirrors the gate description in this plan).
4. Section 2 HSI inheritance table: HSI-3 G2b row "Relaxed in" column changes from "P6B-B (only if the HSM/KMS client library is in the banned set; v0.3 plan RECOMMENDS against)" to "P6B-CD (narrow k256 recovery-only carve-out per ADR-001 Amendment 2)". HSI-3 G2b "Status at P6B-A close" column remains 0 hits; HSI-3 G2b "Status at P6B-CD close" is the new column added by the amendment.

These are doc-only edits; no runtime impact.

## Chain of locks at P6B-CD close

```
[P6B-CD]
  sign_tx returns Ok(SignedTxBytes) under matched-address + sign-success conditions
  k256 carve-out gated by G2b amendment + G2f narrow-surface allow-list
  alloy-rlp dep enabled
  G11 production runtime sign_tx call site: 0 (test-only invocation; G11 count stays 1 via the test hook)

  v
[P6B-D (NOT STARTED)]
  live_send=true relaxation; ONLY for (Profile::Production, KeyBackend::HsmKms)
  dev/test/shadow continue to reject unconditionally
  ConfigError::LiveSendForbidden text updated to mention production-profile carve-out

  v
[P6B-E (NOT STARTED)]
  submit_bundle returns Ok(SubmissionReceipt) under full G12 chain INHERITING G13
  eth_sendBundle runtime call sites in crates/app/src/ documented per file:line in Section 5
  G3 + G4 + G11 grow per Section 5

  v
[P6B-F (NOT STARTED)]
  Phase 6b DoD audit + phase-6b-complete annotated tag
```

Reordering this chain is forbidden under Section 7 of `docs/specs/phase-6b-boundary.md`. Skipping a batch is forbidden. Each transition requires its own Codex pre-impl review + fresh explicit user re-authorization.

## v0.1 file-touch summary (for the future P6B-CD impl turn)

| File | Change kind |
|---|---|
| `crates/signer/src/bundle_tx.rs` | Substantive (additive `max_priority_fee_per_gas` + `max_fee_per_gas` fields; BREAKING `BundleTx::new(...)` ctor; D-T-CD1). |
| `crates/signer/src/error.rs` | Additive (4 new payload-free variants; preserves `Copy`). |
| `crates/signer/src/lib.rs` | Additive (new `mod recovery`, `mod rlp` declarations). |
| `crates/signer/src/production.rs` | Substantive (sign_tx body rewrite; NEW `pubkey_sec1_65: Option<[u8; 65]>` field; D-T-CD5..D-T-CD7 + D-T-CD9 + D-T-CD10). **Plus 1-line doc-comment cleanup at line 70** (v0.4 R-9): remove literal `` `SecretKey` `` token while preserving the no-opaque-private-key-struct-field invariant; required for G2g to return 0 hits absolute. |
| `crates/signer/src/rlp.rs` | NEW. `pub(crate) fn encode_eip1559_unsigned`, `pub(crate) fn encode_eip1559_signed`. D-T-CD2. |
| `crates/signer/src/recovery.rs` | NEW. `pub(crate) fn recover_y_parity`, `pub(crate) fn parse_der_to_rs`. D-T-CD3 + D-T-CD4 + D-T-CD8. The ONLY k256-importing file. |
| `crates/signer/Cargo.toml` | Additive (`alloy-rlp = "0.3"`, `k256 = "0.13"` with locked feature set). |
| `crates/execution/src/lib.rs` | Minimal (update each `BundleTx::new(...)` call site to pass 2 new args). NO behavior change. |
| `docs/specs/phase-6b-boundary.md` | Additive (Section 2 HSI-3 row column update, Section 3 reconciliation paragraph, Section 4 G15 outcome-label expansion + new G2f doc, Section 7 reordering reaffirm). |
| `docs/adr/ADR-001-vertical-slice-replay-hooks-gate-policy.md` | Additive Amendment 2 (ONLY if user explicitly authorizes the k256 carve-out at the implementation-gate step). |
| `config/examples/signing-audit-alert.yaml` (R-1 NEW) | Substantive (`SigningFailureRateHigh` matcher narrowed from `{outcome!="not_configured"}` to a positive enumeration of the 5 failure outcomes; top-of-file comment block + `SigningFailureRateHigh` description rewritten for the new 7-element label set; `SigningAttemptRateHigh` rule UNCHANGED). |

NO touch in P6B-CD:

- `crates/signer/src/{signer_trait,disabled,kms_aws,kms_client}.rs` (no surface change beyond what's covered by the SignerError additions visible through `error.rs`).
- `crates/app/src/lib.rs`, `crates/app/src/main.rs`, `crates/app/tests/wire_phase4.rs`.
- `crates/config/src/lib.rs`.
- `docs/specs/production-signer.md`, `docs/specs/execution-safety.md`, `docs/specs/phase-6a-boundary.md`.
- `docs/superpowers/plans/2026-05-16-phase-6b-batch-c-*.md` and earlier plans (frozen).

## Plan execution checklist (after Codex APPROVED + explicit user re-authorization)

- [ ] **Step 1**: User explicitly re-authorizes the P6B-CD implementation AND the ADR-001 narrow-recovery-only carve-out amendment.
- [ ] **Step 2**: Add `alloy-rlp = "0.3"` + `k256 = { version = "0.13", default-features = false, features = ["ecdsa", "arithmetic"] }` to `crates/signer/Cargo.toml`.
- [ ] **Step 3**: Verify G2b safety AFTER dep add: `cargo tree -e features -p rust-lmax-mev-signer 2>&1 | rg 'secp256k1|alloy-signer|ethers-signers'` -> expect 0 hits. If non-zero, REVERT Step 2 + HALT.
- [ ] **Step 4**: Verify k256 narrow feature set: `cargo tree -e features -p rust-lmax-mev-signer | rg '^k256'` -> resolved feature list LOCKED in a Cargo.toml line comment.
- [ ] **Step 5**: Extend `BundleTx` per D-CD1. Update `BundleTx::new(...)` callers in `crates/execution/src/lib.rs` and any test files in lockstep.
- [ ] **Step 6**: Author `crates/signer/src/rlp.rs` per D-CD2 + D-CD7.
- [ ] **Step 7**: Author `crates/signer/src/recovery.rs` per D-CD4. Lock the imports to the narrow allow-list.
- [ ] **Step 8**: Add the 4 new `SignerError` variants per D-CD5.
- [ ] **Step 9**: Update `ProductionSigner` per D-CD6: add `pubkey_sec1_65: Option<[u8; 65]>` field, populate in `from_aws_kms_with_client` (parsed alongside the existing `derived_address` derivation), rewrite `sign_tx` body to the new shape. **Also rewrite the existing `production.rs:70` doc comment** (currently `` `SecretKey`-style type. NO key fingerprint as a struct field. ``) so the literal `SecretKey` token is removed while the invariant text is preserved (see G2g Section "Pre-existing baseline hit" for the recommended rewrite). This 1-line cleanup keeps G2g at 0 hits absolute.
- [ ] **Step 10**: Add the 10 D-T-CD tests per the test table.
- [ ] **Step 11**: Apply the boundary-doc amendment per D-CD9 (Section 2 / 3 / 4 / 7 edits).
- [ ] **Step 12**: Apply the ADR-001 Amendment 2 per D-CD10. ONLY if Step 1 user authorization explicitly covers it; otherwise HALT and rework P6B-CD to Path (b).
- [ ] **Step 13 (R-1 NEW)**: Edit `config/examples/signing-audit-alert.yaml` per D-CD9: narrow the `SigningFailureRateHigh` matcher to `{outcome=~"address_mismatch|invalid_bundle_tx|kms_sign_failed|invalid_signature_bytes|signature_recovery_failed"}`; rewrite top-of-file comments + `SigningFailureRateHigh` description to enumerate the P6B-CD outcome-label set; leave `SigningAttemptRateHigh` unchanged. ASCII-scan the file.
- [ ] **Step 14**: Targeted self-check per "Gates at P6B-CD close": `cargo fmt --check`, `cargo clippy -p rust-lmax-mev-signer -p rust-lmax-mev-config -p rust-lmax-mev-app --all-targets -- -D warnings`, `cargo test -p signer -p config -p app` (expect 49 + 10 = 59 passed + 0 ignored), G2a/G2b/G2c/G2d/G2e/G2f/G3/G4/G6/G7/G8/G9/G10/G11/G12/G13/G14/G15 ripgrep gates.
- [ ] **Step 15**: Commit + push as `feat(p6b-cd): activate sign_tx Ok path with k256 recovery-only carve-out`.
- [ ] **Step 16**: Emit P6B-CD closeout report to `.coordination/claude_outbox.md` naming the new outcome labels, the new gate G2f, the ADR-001 Amendment 2 commit (if landed), the YAML failure-rule narrowing, and the boundary-doc updates.

## Risks + open questions (Q-CD1..Q-CD9)

- **Q-CD1**: ADR-001 narrow-recovery-only carve-out wording. Plan proposes Amendment 2 text (Section above). Codex verdict + user authorization at implementation gate. **Blocker if rejected**: P6B-CD falls back to Path (b) (alternate HSM service); requires a new pre-impl plan.
- **Q-CD2**: `k256` exact features pin. v0.1 LOCKS `default-features = false, features = ["ecdsa", "arithmetic"]`. Codex verdict on whether `arithmetic` is needed (it is required for `recover_from_prehash`).
- **Q-CD3 (R-5 RESOLVED in v0.2)**: D-T-CD2 vector LOCKED to alloy-rs `alloy-consensus` crate `crates/consensus/src/transaction/eip1559.rs` round-trip fixture (empty access list). D-T-CD4 vector LOCKED to go-ethereum `core/types/transaction_signing_test.go::TestEIP1559Signing` (chain_id 1, empty access list). D-T-CD8 vector synthetic from D-T-CD4 base. Implementer pins exact commit SHA + line numbers at impl time. See "LOCKED test-vector sources (R-5)" Section under Tests above.
- **Q-CD4**: `pubkey_sec1_65` storage on `ProductionSigner`. The field is `Option<[u8; 65]>` (uncompressed point bytes). Stored at boot alongside `derived_address`. Workspace already extracts these bytes during DER parse for the address derivation; storing them adds 65 bytes per signer instance + zero new field types. Codex verdict on whether the field should be `[u8; 64]` (without the leading 0x04 marker) for size minimalism. v0.1 LOCKS `[u8; 65]` to match `k256::PublicKey::from_sec1_bytes` input shape directly.
- **Q-CD5**: `BundleTx::new(...)` BREAKING ctor change vs additive `new_eip1559(...)`. v0.1 LOCKS BREAKING because there is exactly one production caller (none yet) + a few test callers in `crates/execution` that are co-edited. Codex verdict on whether the breaking change is acceptable given `#[non_exhaustive]`.
- **Q-CD6**: Empty `access_list` hardcoded in encoder vs `BundleTx::access_list` field. v0.1 LOCKS hardcoded empty. **Risk**: if a future bundle ever needs a non-empty access list, the encoder + BundleTx both need to be extended. Plan documents this as deferred; the deferral is review-able when access-list-bearing bundles become a requirement.
- **Q-CD7**: Low-s normalization invariant. v0.1 LOCKS that `Signature::normalize_s()` is applied unconditionally before recovery; the normalized signature is what enters the signed-tx bytes. Test D-T-CD8 asserts. Codex verdict on whether an additional "reject originally-high-s" mode is desired (some operator policies reject high-s as a malleability check rather than auto-normalizing). v0.1 RECOMMENDS auto-normalize for AWS KMS interoperability; reject mode can be added later if operators ask.
- **Q-CD8 (R-1 RESOLVED in v0.2)**: v0.2 RESOLVES this by REQUIRING the YAML edit. The `SigningFailureRateHigh` matcher narrows to `{outcome=~"address_mismatch|invalid_bundle_tx|kms_sign_failed|invalid_signature_bytes|signature_recovery_failed"}` -- positive enumeration of the 5 failure outcomes; `ok` and `not_configured` both excluded. The `SigningAttemptRateHigh` rule is UNCHANGED (it scrapes total signing-attempt rate regardless of outcome, which is the operator-attack-surface signal). v0.1's "v0.1 RECOMMENDS NO YAML update" framing was wrong: a successful-signing throughput would have fired `SigningFailureRateHigh` under v0.1's matcher, inverting the alert's intent. See D-CD9 above for the full YAML edit specification.
- **Q-CD9**: At P6B-CD close, the production `sign_tx` `Ok` return is reachable ONLY through the existing `#[cfg(test)] pub(crate) async fn invoke_signer_for_test` hook. The runtime caller path is P6B-E scope. **Question**: should P6B-CD also land a documentation-only Section 5 entry in `docs/specs/phase-6b-boundary.md` foreshadowing the P6B-E runtime caller? v0.1 RECOMMENDS NO: Section 5 stays empty until P6B-E actually adds the caller, per the existing "per-callsite documentation requirement" semantics.

## Process (v0.3; R-6 carried forward from v0.2)

**Plan approval and implementation authorization are TWO SEPARATE gates.** Codex APPROVAL on this plan is approval to COMMIT THE PLAN ONLY; it is NOT authorization to begin Steps 1..16. Implementation requires a SECOND user gesture after the plan commit lands on `master`.

1. Claude writes this v0.4 plan to disk (UNCOMMITTED) + re-emits the v0.4 Codex review pack to `.coordination/claude_outbox.md`.
2. Claude STOPS for manual Codex re-review.
3. User pastes pack to Codex; relays verdict.
4. Claude records verdict in `.coordination/codex_review.md` (gitignored handoff file).
5. **APPROVED** -> commit + push the approved plan v0.4 to `master`; STOP. P6B-CD implementation does NOT start; it requires a SEPARATE explicit user re-authorization message (the user types a fresh authorization explicitly naming the P6B-CD non-goals + ADR-001 Amendment 2) AFTER the plan commit lands on `master`. Plan-commit and impl-start cannot be collapsed into a single user gesture.
6. **REVISION REQUIRED** -> revise plan in place + re-emit pack as v0.5.
7. **Scope / ADR change required beyond what this plan proposes** -> HALT to user.

The implementation-authorization gate (the second user gesture in step 5 above) is BLOCKING for Steps 1..16 of the "Plan execution checklist" Section. Without it, the workspace stays at the post-plan-commit baseline -- a documented plan, no implementation, no `.rs` / `Cargo.toml` / `Cargo.lock` / YAML / boundary-doc / ADR changes.

## What Codex is asked to verdict at v0.4

1. Whether Path (a) k256 recovery-only carve-out is the right resolution of Blocker 3, or whether Path (b) (alternate HSM service) should be the lock instead.
2. Whether the ADR-001 Amendment 2 text correctly captures the narrow-surface scope.
3. Whether the G2f narrow-surface allow-list is tight enough.
4. Whether the BundleTx BREAKING `new(...)` ctor change is acceptable vs additive `new_eip1559(...)`.
5. Whether `[u8; 65]` (vs `[u8; 64]`) is the right pubkey storage shape.
6. Whether the 10 minimum tests are sufficient or if any safety-critical case is missing (low-s round-trip, malformed DER variants, address-mismatch-bypass attempt, audit-completeness on every Err path).
7. Whether the boundary-doc reconciliation set (Section 2 / 3 / 4 / 7 edits) is complete.
8. Whether the v0.2 YAML failure-rule narrowing (5-outcome positive enumeration) is correct + whether the YAML comment/annotation rewrites are sufficient.
9. Whether the chain of locks (P6B-CD -> P6B-D -> P6B-E -> P6B-F) is correctly stated and reordering-banned.
10. Whether the v0.3 R-7 fix is complete: test-vector sources copy ONLY non-secret material (no `test_key`, no published private-key bytes, no `SecretKey` / `SigningKey` constructor); G2g grep gate is tight enough; "Hard forbids" + "LOCKED test-vector sources" wording aligns with `production-signer.md` Section 2.1 / 2.2 + `phase-6b-boundary.md` Section 6 / 7 absolute no-private-key-bytes invariant.

## Verdict shapes Claude expects (v0.4)

- **APPROVED** -> commit + push the v0.4 plan to `master`; STOP. P6B-CD implementation does NOT begin in the same turn. A SEPARATE subsequent user message must explicitly re-authorize Steps 1..16 (including ADR-001 Amendment 2) before any `.rs` / `Cargo.toml` / `Cargo.lock` / YAML / boundary-doc / ADR change.
- **REVISION REQUIRED** -> revise plan in place + re-emit pack as v0.5.
- **Scope / ADR change required beyond what this plan proposes** -> HALT to user.
