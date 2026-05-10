# Phase 5 Batch C — signer boundary + fail-closed stub crate

**Date:** 2026-05-10 KST
**Status:** Draft v0.3 (revised after manual Codex REVISION REQUIRED HIGH on v0.2, 2026-05-10 KST). Two cleanup revisions R-C7..R-C8 applied (unescaped `rg` alternation; `SignerError` derives `Clone + Copy + PartialEq + Eq`). v0.2 R-C1..R-C6 + Q-C1..Q-C4 standing answers carried unchanged. Awaiting manual Codex re-review.
**Predecessor:** P5-B closed + pushed at `84283d9` (Codex closeout NO ACTION HIGH 2026-05-10 KST). Phase 5 overview at `ac07024`. P5-A closed at `0c28d5c`.

## Scope

Land the **signer boundary design + fail-closed stub** unlocked by Phase 5 overview §"P5-C — signer boundary design + fail-closed stub" + Q-P5-4 standing answer. Per overview hard-forbids:

- NEW `crates/signer` skeleton workspace member.
- Define a fail-closed signer surface ONLY:
  - `pub trait Signer: Send + Sync + std::fmt::Debug + 'static`
  - `pub struct DisabledSigner` — single shipped impl.
  - `pub enum SignerError` (`#[non_exhaustive]`) with at least one `SignerDisabled` variant.
- Every signing attempt MUST return `Err(SignerError::SignerDisabled)`. The `Display` text MUST contain the literal phrase `Phase 6b Production Gate` (BR-3-style spec-drift guard test).
- `Signer::sign_tx` consumes the structured `BundleTx` shape from overview Q-P5-4.
- **`bundle_correlation_id: u64`** on `BundleTx` per **R-C1**: `crates/types::EventEnvelope` already carries `correlation_id: u64`; `crates/types` exposes no `CorrelationId` newtype today, and P5-C does NOT add one. The `BundleTx.bundle_correlation_id` field documents that callers MUST pass the same `u64` value as the upstream envelope `correlation_id` so journaled signing events can be cross-linked to the comparator chain.
- Promote the Phase 4 G2 zero-hit `Signer` grep to the Phase 5+ redefined gate per overview R-P5-2 (full grep set in DP-C7).
- **Non-semantic grep-residue cleanup** per **R-C2**: replace pre-existing doc-comment + `Cargo.toml description` mentions of `funded` with `production key material` / `preloaded` / `pre-loaded` so the new forbidden grep can be zero-hit. Strictly text-only edits with zero behavioral impact; no source code semantics change. The two code comments in `crates/simulator` that use "pre-funded"/"pre-funded" to describe pre-seeded ERC20 balance slots are reworded to "pre-loaded" / "pre-seeded" — the underlying state-override behavior is unchanged.
- NO production signer impl. NO funded key. NO key derivation. NO `secp256k1` / `k256` / `alloy-signer` / `ethers-signers` / `Wallet` / `PrivateKey` / `sign_transaction` symbols anywhere in `crates/`.
- NO app wiring that enables real signing. `wire_phase4` / `AppHandle4` / `BundleConstructor` byte-identical to P5-B. (P5-D / Phase 6 own wiring decisions.)
- **NO submission path change**. **NO `live_send`**. **NO live network**. **NO Phase 6 work**.

## Decision points

- **DP-C1 (crate location + name)**: NEW `crates/signer` workspace member, `package.name = "rust-lmax-mev-signer"` matching the established convention. Added as member #20 in `Cargo.toml [workspace] members`. Dependencies: `alloy-primitives` + `async-trait` + `thiserror` only (no `rust-lmax-mev-types` dep needed — `bundle_correlation_id` is `u64` per R-C1).

- **DP-C2 (trait shape — Q-P5-4 binding)**: object-safe `async` trait via `async-trait` (matches the existing async patterns in `crates/state-fetcher`, `crates/relay-sim`). Single method `sign_tx(&self, tx: &BundleTx) -> Result<SignedTxBytes, SignerError>`.
  ```rust
  #[async_trait::async_trait]
  pub trait Signer: Send + Sync + std::fmt::Debug + 'static {
      async fn sign_tx(&self, tx: &BundleTx) -> Result<SignedTxBytes, SignerError>;
  }
  ```

- **DP-C3 (`BundleTx` shape — Q-P5-4 binding + R-C1)**: structured shape with REQUIRED `bundle_correlation_id: u64`. `#[non_exhaustive]` + `BundleTx::new(...)` ctor for forward compat. Phase 5 fields are minimal; Phase 6 extends.
  ```rust
  #[non_exhaustive]
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct BundleTx {
      pub from:                  alloy_primitives::Address,
      pub to:                    alloy_primitives::Address,
      pub value_wei:             alloy_primitives::U256,
      pub data:                  Vec<u8>,
      pub gas_limit:             u64,
      pub nonce:                 u64,
      pub chain_id:              u64,
      /// MUST be the same `u64` as the upstream `EventEnvelope.correlation_id`
      /// in `crates/types` (Phase 1 P3-A) so journaled signing events are
      /// cross-linkable to the comparator chain. P5-C does NOT introduce a
      /// `CorrelationId` newtype; `crates/types` exposes no such type today.
      pub bundle_correlation_id: u64,
  }
  ```

- **DP-C4 (`SignerError` shape — R-C8)**: `#[non_exhaustive]` enum + `thiserror::Error` + `Clone + Copy + PartialEq + Eq` derives so SC-1 / SC-2 can compare via `assert_eq!(..., Err(SignerError::SignerDisabled))` directly without `matches!`. Payload-free fail-closed enum so `Copy` is trivially safe. Phase 5 ships exactly one variant: `SignerDisabled`. Future Phase 6b variants reserved (e.g., `KeyUnavailable`, `HsmUnavailable`, `Cancelled`) — not added in P5-C; if Phase 6b adds a payload-bearing variant, the `Copy` derive may need to drop, and tests can switch to `matches!` at that point.
  ```rust
  #[non_exhaustive]
  #[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
  pub enum SignerError {
      #[error("signer disabled — production signing requires Phase 6b Production Gate")]
      SignerDisabled,
  }
  ```
  The literal string MUST contain `Phase 6b Production Gate`. SC-3 spec-drift guard test pins this.

- **DP-C5 (`DisabledSigner` impl)**: zero-state unit struct. `Default + Debug + Clone + Copy`. `sign_tx` returns `Err(SignerError::SignerDisabled)` unconditionally regardless of input.
  ```rust
  #[derive(Debug, Clone, Copy, Default)]
  pub struct DisabledSigner;

  #[async_trait::async_trait]
  impl Signer for DisabledSigner {
      async fn sign_tx(&self, _tx: &BundleTx) -> Result<SignedTxBytes, SignerError> {
          Err(SignerError::SignerDisabled)
      }
  }
  ```

- **DP-C6 (no app wiring)**: P5-C ships `crates/signer` as an isolated leaf crate. NO `crates/app` dep edge added. NO `AppHandle4` field. NO `wire_phase4` change. NO `BundleConstructor` integration. The signer is reachable only by direct unit tests in `crates/signer/`. Phase 5-D / Phase 6 own all integration decisions.

- **DP-C7 (forbidden-symbol grep gate redefinition — R-P5-2 + R-C3 + R-C4)**: P5-C is the batch that promotes the Phase 4 G2 zero-hit `Signer` grep to the Phase 5+ redefined gate. The P5-C batch close evidence MUST include **four** grep runs + their actual output (Q-C4 answer):

  | # | Gate | Command (`rg` canonical — R-C7 unescaped alternation) | Pass condition |
  |---|---|---|---|
  | G2a | Forbidden symbols in `.rs` | `rg -n -w '(Wallet|PrivateKey|secp256k1|k256|sign_transaction|funded)' crates/ --type rust` | **zero hits** |
  | G2b | Forbidden deps in `Cargo.toml` | `rg -n '(alloy-signer|ethers-signers|secp256k1|k256)' crates/ -g '**/Cargo.toml'` | **zero hits** |
  | G2c | Allowed-Signer-symbol inventory (positive) | `rg -n -w '(Signer|DisabledSigner|SignerError|SignerDisabled)' crates/ --type rust` | every hit MUST be under `crates/signer/`; documents WHERE the symbols live |
  | **G2d** | **Allowed-Signer-symbol leak — HARD gate (R-C3)** | `rg -n -w '(Signer|DisabledSigner|SignerError|SignerDisabled)' crates/ --type rust -g '!crates/signer/**'` | **zero hits** |

  **R-C7 note**: The pipes inside the parens are the rg/regex alternation operator and MUST NOT be backslash-escaped. Backslash-escaping `\|` causes rg to look for a literal `|` byte and false-passes the safety gate. Equivalent `-e` form for ambiguity-avoidance: `rg -n -w -e Wallet -e PrivateKey -e secp256k1 -e k256 -e sign_transaction -e funded crates/ --type rust`, etc.

  - G2c is positive-only inventory (records location of the legitimate symbols inside the signer crate); G2d is the new hard gate that asserts those symbols never leak outside `crates/signer/`. Together they replace the Phase 4 G2 zero-hit `Signer` grep.
  - The `funded` token in G2a catches any future `funded_key` / `funded_signer` / `funded_wallet` mention. Per R-C2, pre-existing doc-comment + Cargo-description residue is cleaned up in the same P5-C commit so G2a starts clean.
  - On Windows where `rg` may not be on PATH, `Select-String` equivalents are recorded in the closeout pack with the same pass conditions.
  - Carry-forward Phase 4 zero-hit gates G3..G8 unchanged.
  - The P5-E DoD audit records this G2 transition.

- **DP-C8 (no async runtime in production deps)**: `crates/signer` does NOT depend on `tokio` directly in `[dependencies]`. Tests use `#[tokio::test]` via `tokio = { workspace = true }` as a dev-dep only. Production code is runtime-agnostic.

- **DP-C9 (no compile-time forbid macro in P5-C)**: the overview §P5-C "Optional: a `crates/signer` module-level `forbid!` macro that asserts at compile time that no key material is present (defense-in-depth)" is **deferred** — a meaningful compile-time assertion would need a custom proc-macro source-walker, which is out of P5-C scope. P5-C uses the runtime grep gate (DP-C7) as the enforcement mechanism.

- **DP-C10 (`SignedTxBytes` shape — R-C6)**: explicit public-boundary derives. The signer never produces real bytes in P5-C, but the type is part of the public boundary that Phase 6 will consume.
  ```rust
  #[repr(transparent)]
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct SignedTxBytes(pub Vec<u8>);
  ```
  No `Default` (zero-byte signed payload is meaningless); no `Display` (avoid accidental log of raw bytes — Phase 6 may add hex helpers if needed).

- **DP-C11 (non-semantic grep-residue cleanup — R-C2)**: P5-C's first source change is a **text-only** sweep of pre-existing `funded` mentions so DP-C7 G2a can pass. Concrete edits (verified via `rg -n -w funded crates/`):

  | File | Line | Original | Replacement | Note |
  |---|---|---|---|---|
  | `crates/bundle-relay/Cargo.toml` | 7 (description) | `…No funded key, no signing.` | `…No production key material, no signing.` | Cargo description text only. |
  | `crates/bundle-relay/src/lib.rs` | 26 (doc) | `Real signers + funded` | `Real signers + production key material` | Doc comment. |
  | `crates/execution/src/lib.rs` | 7 (header doc) | `…NO funded key, NO BundleRelay…` | `…NO production key material, NO BundleRelay…` | Doc comment. |
  | `crates/execution/src/lib.rs` | 89 (doc) | `…funded` | `…production key material` | Doc comment. |
  | `crates/execution/tests/construct.rs` | 5 (header doc) | `…no funded key.` | `…no production key material.` | Test-file doc comment. |
  | `crates/relay-clients/Cargo.toml` | 7 (description) | `…No funded key, no signing.` | `…No production key material, no signing.` | Cargo description text only. |
  | `crates/relay-clients/src/bloxroute.rs` | 4 (header doc) | `…NO funded` | `…NO production key material` | Doc comment. |
  | `crates/relay-clients/src/flashbots.rs` | 4 (header doc) | `…NO funded` | `…NO production key material` | Doc comment. |
  | `crates/relay-clients/src/lib.rs` | 9 (header doc) | `…NO funded` | `…NO production key material` | Doc comment. |
  | `crates/relay-sim/Cargo.toml` | 7 (description) | `…NO funded key.` | `…NO production key material.` | Cargo description text only. |
  | `crates/relay-sim/src/lib.rs` | 15 (header doc) | `…no funded key, no` | `…no production key material, no` | Doc comment. |
  | `crates/risk/tests/evaluate.rs` | 6 (header doc) | `no funded key.` | `no production key material.` | Test-file doc comment. |
  | `crates/simulator/examples/dump_fixture.rs` | 28 (header doc) | `No funded key.` | `No production key material.` | Doc comment. |
  | `crates/simulator/src/cache_db_builder.rs` | 22 (doc) | `Pre-funded WETH balance for the router.` | `Pre-loaded WETH balance for the router.` | Doc comment; underlying state-override unchanged. |
  | `crates/simulator/src/fixtures.rs` | 37 (header doc) | `…no funded key, no signer.` | `…no production key material, no signer.` | Doc comment. |
  | `crates/simulator/src/lib.rs` | 26 (header doc) | `…no funded key.` | `…no production key material.` | Doc comment. |
  | `crates/simulator/src/mock_router.rs` | 71 (header doc) | `Holds NO funded key, signs NO tx…` | `Holds NO production key material, signs NO tx…` | Doc comment. |
  | `crates/simulator/tests/t_usdc_1.rs` | 93 (test comment) | `// …with the pre-funded` | `// …with the pre-loaded` | Test code comment; balance slot override unchanged. |

  All edits are **non-semantic** (doc comments + Cargo descriptions + one test-file inline comment). `cargo test --workspace` and clippy outcomes are byte-identical pre-vs-post sweep. The sweep is a **prerequisite** of the P5-C impl commit, not a separate batch.

## Non-goals (Phase 5 hard forbids reaffirmed)

- No production signer (HSM / KMS / file-backed / in-memory).
- No funded key. No production key material in repo / tests / fixtures / configs / env examples.
- No `secp256k1` / `k256` / `alloy-signer` / `ethers-signers` / `Wallet` / `PrivateKey` / `sign_transaction` symbol introduction anywhere in `crates/`.
- No app-level wiring (`AppHandle4` / `wire_phase4` / `BundleConstructor` byte-identical to P5-B).
- No `live_send` / `eth_sendBundle` / actual relay submission path change.
- No `bundle-relay` / `relay-clients` / `execution` semantic edits (DP-C11 is text-only doc-comment + Cargo description sweep).
- No new live network surface; no paid live API; no live-network test enabled by default.
- No ADR text amendment (R-P5-3 standing carry-forward).
- No widening of asset / venue / V3-fee-tier scope.
- **No `CorrelationId` newtype in `crates/types`** (R-C1) — `bundle_correlation_id` is `u64`.

## Test matrix (lean — overview policy ≤ 5 tests for boundary-only stub; all required per R-C5)

Add inside `crates/signer/src/lib.rs` `#[cfg(test)] mod tests`. All five are REQUIRED (R-C5: SC-5 promoted from optional).

| ID | What it asserts | Why (mapping) |
|---|---|---|
| **SC-1** | `DisabledSigner::default().sign_tx(&BundleTx::new(...)).await == Err(SignerError::SignerDisabled)` for any input. | DP-C5 fail-closed contract. |
| **SC-2** | `DisabledSigner::sign_tx` returns `Err` even when called concurrently from multiple tasks (`tokio::join!(a, b, c)` over the same `Arc<DisabledSigner>`). | DP-C5 unconditional + thread-safety smoke. |
| **SC-3** | The `Display` rendering of `SignerError::SignerDisabled` contains the literal substring `"Phase 6b Production Gate"`. | DP-C4 spec-drift guard (BR-3 style). |
| **SC-4** | Trait object-safety: a `Box<dyn Signer>` can be constructed from `DisabledSigner` and used through the trait object. | DP-C2 dyn-compat + future Phase 6 `Arc<dyn Signer>` wiring. |
| **SC-5** | `BundleTx::new(...)` ctor accepts the documented field set (DP-C3) and the produced value round-trips through `Clone + PartialEq + Eq`; `SignedTxBytes(Vec<u8>)` round-trips through `Clone + PartialEq + Eq` (DP-C10). | DP-C3 `#[non_exhaustive]` ctor accessibility + DP-C10 public-boundary derive guard. R-C5: required. |

Total NEW tests: **5 (all required)**. Workspace target: **221 → 226 passed + 1 ignored**.

## Implementation steps

1. **Cargo workspace edit** (`Cargo.toml`): add `"crates/signer"` as member #20.
2. **Pre-impl non-semantic sweep (DP-C11 / R-C2)**: apply the 18 doc-comment + Cargo-description + test-comment text edits in the table above, in the same commit as the new crate. Verify via `cargo test --workspace` byte-identical pre/post.
3. **NEW `crates/signer/Cargo.toml`**:
   ```toml
   [package]
   name = "rust-lmax-mev-signer"
   version = "0.1.0"
   edition.workspace = true
   rust-version.workspace = true
   publish = false
   description = "Phase 5 fail-closed signer boundary stub. Production signing requires Phase 6b Production Gate."

   [dependencies]
   alloy-primitives = { workspace = true }
   async-trait      = { workspace = true }
   thiserror        = { workspace = true }

   [dev-dependencies]
   tokio = { workspace = true }
   ```
   No `secp256k1` / `k256` / `alloy-signer` / `ethers-signers` / `Wallet` / `PrivateKey` deps. (`alloy-primitives` for `Address` / `U256` only — already used across the workspace.)
4. **NEW `crates/signer/src/lib.rs`** containing exactly:
   - `pub use bundle_tx::{BundleTx, SignedTxBytes};`
   - `pub use error::SignerError;`
   - `pub use signer_trait::Signer;`
   - `pub use disabled::DisabledSigner;`
   - five inline modules (`bundle_tx`, `error`, `signer_trait`, `disabled`, `tests`).
5. Write the four source modules (DP-C2..C5 + DP-C10) and the test module (SC-1..5) in the same commit as steps 1..4.
6. Run **batch-close gates**:
   - `cargo fmt --check`
   - `cargo build --workspace --all-targets`
   - `cargo test --workspace` (expect **226 passed + 1 ignored**)
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo deny check`
   - `cargo tree -p rust-lmax-mev-signer` (cycle gate)
   - **DP-C7 four-grep gate** — run G2a, G2b, G2c, G2d; record actual output in closeout pack (Q-C4 answer):
     - G2a + G2b + G2d MUST each be zero hits.
     - G2c MUST show every hit under `crates/signer/`.
     - If any pass condition fails, HALT and emit revision pack.
7. **No `wire_phase4` edit**. `cargo run -p rust-lmax-mev-app` is NOT exercised — P5-C does not touch `crates/app`.

## Risks + mitigations

| Risk | Mitigation |
|---|---|
| `async-trait` introduces an unwanted dyn-incompat surface. | DP-C2 + SC-4 trait-object test catch this. |
| A future code reviewer accidentally adds a real signer in P5-D. | DP-C7 grep gate (G2a + G2b + G2d) runs at every batch close from P5-C onward. |
| Pre-existing `funded` doc residue trips G2a. | DP-C11 text-only sweep applied in the same commit; cleared before grep gate runs. |
| `Signer` token leaks outside `crates/signer/` via a future helper. | G2d is a hard gate (R-C3), not inventory only — leak fails the batch close. |
| `Pre-funded WETH balance` rewording in `cache_db_builder.rs` accidentally changes state-override semantics. | Edit is doc-comment only; the `set_balance` / cache override code beneath the comment is byte-identical. `cargo test -p rust-lmax-mev-simulator` validates SR-1 / FP-1 / T-USDC-1 unchanged. |
| Codex Q-P5-4 may have intended a richer correlation type than `u64`. | Q-C2 v0.1 verdict locked: keep `u64`, do not introduce `CorrelationId` newtype. R-C1 codifies this. |

## Codex Q-C standing answers (v0.1 verdict, carried v0.2 → v0.3)

- **Q-C1**: 5-test matrix sufficient; proptest unnecessary.
- **Q-C2**: Do NOT add a `CorrelationId` re-export; use `u64` directly. (Drives R-C1.)
- **Q-C3**: No additional REQUIRED `BundleTx` field beyond `bundle_correlation_id: u64`; Q-P5-4 field set + `bundle_correlation_id: u64` is sufficient.
- **Q-C4**: Closeout pack MUST include actual grep evidence (output, not just "all clean") — P5-C is the gate-redefinition batch, so the actual output is the contract.

## Process

Per the 2026-05-04 routine-closeout policy + 2026-05-04 22:20 KST manual-Codex-review-mode + the user's autonomous-Phase-5-execution authorization (P5-C plan stays UNCOMMITTED on disk until Codex APPROVED):

1. Claude writes this plan to disk (UNCOMMITTED) + emits the review pack to `.coordination/claude_outbox.md`.
2. Claude STOPS and reports "manual Codex review required for P5-C v0.3".
3. User pastes pack to Codex; relays verdict.
4. Claude records verdict in `.coordination/codex_review.md`.
5. **APPROVED** → commit + push this plan as a routine doc commit; THEN implement P5-C per the test matrix + DP-C11 sweep; THEN batch-close gates including the four-grep evidence; THEN commit + push; THEN proceed to P5-D pre-impl plan automatically per the autonomous-execution authorization.
6. **REVISION REQUIRED** → revise this plan in place + re-emit pack.
7. **Scope/ADR change required** → HALT to user (any item from Phase 5 hard-forbids list, any ADR text change, any signer/submission/`live_send` capability addition, anything that requires real key material).

No code, no `Cargo.toml` edits, no commit, no push, no tag in this turn.
