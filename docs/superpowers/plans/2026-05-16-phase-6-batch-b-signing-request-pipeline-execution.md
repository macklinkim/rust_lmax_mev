# Phase 6 Batch B — Signing-request pipeline (fail-closed)

**Date:** 2026-05-16 KST
**Status:** Draft v0.5 (revised after manual Codex REVISION REQUIRED HIGH on v0.4, 2026-05-16 KST). Three v0.4 → v0.5 stale-wording fixes:
(1) §Scope wording corrected — G2c/G2d allow-list extension is now stated as **"three file entries"** (`crates/execution/src/lib.rs` + `crates/app/src/lib.rs` + `crates/app/tests/wire_phase4.rs`) instead of the v0.4-stale "two file:line sites" left over from v0.3.
(2) §Forbidden in P6-B + §D-B6 wording corrected — `docs/specs/phase-6a-boundary.md` is **NOT** read-only in P6-B because D-B0 edits it. v0.5 explicitly states that **only the D-B0 §4 G2a regex amendment plus the accompanying §4 prose note are allowed**; every other section of `phase-6a-boundary.md` remains read-only at P6-B.
(3) §Tests summary line for D-T3 corrected — D-T3 no longer claims to prove `wire_phase4` constructs with injected `DisabledSigner`; it proves only **signer-parameter acceptance + existing bogus-URL fail-closed behavior preserved**. Positive injection coverage is provided by D-T1 + manual code review + G11 (already enumerated in v0.4 main body).

v0.4 changelog (single item) and v0.1 → v0.2, v0.2 → v0.3 changelogs retained verbatim below for traceability. Awaiting manual Codex re-review.

## v0.3 → v0.4 fix (retained verbatim from prior turn) Single v0.3 → v0.4 fix:
**D-B7 integration-test reachability error corrected.** v0.3 D-B7 proposed `#[cfg(test)] pub` helpers in `crates/app/src/lib.rs` callable from `crates/app/tests/wire_phase4.rs`. Codex correctly observed this does NOT compile: integration tests under `crates/app/tests/` compile the library as a normal dependency, so `#[cfg(test)]` items in the library's `src/lib.rs` are NOT in scope. v0.4 picks Codex's option **"explicitly expand the G2c/G2d allow-list to cover the integration test file"** combined with **"move D-T3's positive injection assertion to an inline unit test"** (since `wire_phase4` cannot succeed without a live geth endpoint in the existing test infrastructure, an integration-test end-to-end positive assertion is infeasible regardless). Specifically:
- D-B7 (helper indirection) is **dropped entirely**.
- W-1 (existing `crates/app/tests/wire_phase4.rs:29-50`) is updated to directly construct `let signer: Arc<dyn Signer> = Arc::new(DisabledSigner::default());` and pass it to `wire_phase4(&config, opts, signer)`. The test file file gains `Signer` / `DisabledSigner` symbol references and joins the G2c allow-list as a third entry.
- D-T3 is **reframed** as a static-shape regression assertion (failure-path-preserving), NOT a positive end-to-end injection assertion. Concretely: D-T3 = W-1 modified — proves `wire_phase4` accepts the new signer parameter and the existing bogus-URL fail-closed behavior is preserved (`Err(AppError::Node)` or `Err(AppError::Io)` within 5s). The new W-1 still does NOT assert positive signer injection (wire fails before reaching `BundleConstructor` build site).
- Positive signer-injection assurance comes from D-T1 (`BundleConstructor::with_signer` directly tested in `crates/execution`) + manual code review of the single one-line `BundleConstructor::with_signer(cfg, strategy, signer)` call site inside `wire_phase4`. G11 grep gate enforces the single `sign_tx` call site invariant.
- `AppHandle4::bundle_constructor()` accessor and `assert_disabled_signer_through_app_handle_for_test` helper are **dropped** (no integration-test caller exists; D-T1 already covers `BundleConstructor` injection).
- G2c allow-list at P6-B close grows to **3 file entries** (was 2 in v0.3): `crates/execution/src/lib.rs`, `crates/app/src/lib.rs`, `crates/app/tests/wire_phase4.rs`. G2d positive-allow-list enforces zero hits outside the union of these three plus the Phase 5 `crates/signer/...` baseline.

v0.1 → v0.2 and v0.2 → v0.3 fixes retained verbatim. Awaiting manual Codex re-review.
(1) **G2a boundary-spec conflict resolved via a reviewed boundary-doc amendment path.** v0.2 admitted the approved boundary doc's G2a command produces a non-zero `keccak256` substring baseline while the boundary spec still says `0 hits`. v0.3 adds a new sibling deliverable **D-B0 (boundary-doc amendment)** that lands BEFORE the impl: a single-pattern fix to `docs/specs/phase-6a-boundary.md` §4 G2a row changing the `'k256'` pattern to `'\bk256\b'` (ripgrep PCRE-style word boundary, default-regex compatible) so the post-amendment baseline at `phase-5-complete` is actually 0 hits. With D-B0 landed, P6-B's G2a expected at close becomes "0 hits, identical to the post-amendment baseline" — the boundary spec and the plan now agree. D-B0 is doc-only; Codex reviews it in this same plan; on APPROVED it commits + pushes BEFORE any `.rs` / `Cargo.toml` work begins.
(2) **All `wire_phase4` callers enumerated and updated.** v0.2 named only the production caller at `crates/app/src/lib.rs:176`; v0.3 also names the integration-test caller at `crates/app/tests/wire_phase4.rs:36` (the W-1 deterministic-shutdown test). v0.3 D-B3 + execution checklist now covers both.
(3) **G2c/G2d allow-list reconciled with the updated test caller via an approved-helper indirection.** Naively updating `crates/app/tests/wire_phase4.rs` to construct `Arc::new(DisabledSigner::default())` directly would introduce `Signer`/`DisabledSigner` symbols in a test file OUTSIDE the allow-list, failing G2d. v0.3 adds a new deliverable **D-B7** that exposes a `#[cfg(test)] pub fn wire_phase4_with_default_signer_for_test(config: &Config, opts: WireOptions) -> impl Future<...>` helper from `crates/app/src/lib.rs` (which IS in the allow-list); W-1 calls this helper, never naming signer symbols directly. The allow-list stays at exactly two files (`crates/execution/src/lib.rs` + `crates/app/src/lib.rs`).
(4) **D-T3 redefined to verify signer-injection, not just construction.** v0.2 with Q-B3 (c) (skip accessor) only proved `wire_phase4` accepts a signer and constructs. v0.3 adds a new `#[cfg(test)] pub(crate) fn bundle_constructor(&self) -> &BundleConstructor` accessor on `AppHandle4` AND keeps the `#[cfg(test)] pub(crate) async fn invoke_signer_for_test` accessor on `BundleConstructor` from D-B2. D-T3 then chains `wire_phase4_with_default_signer_for_test(...).await? → handle.bundle_constructor().invoke_signer_for_test(&BundleTx::new(...)).await` and asserts `Err(SignerError::SignerDisabled)` — a true end-to-end signer-injection assertion. Q-B3 recommendation flipped from (c) to a new (d).
v0.1 → v0.2 fixes retained verbatim. Awaiting manual Codex re-review.

## v0.1 → v0.2 fixes (retained verbatim from prior turn)

(1) `wire_phase4` signature aligned to the APPROVED boundary spec — accepts a new `signer: Arc<dyn Signer>` parameter (v0.1 incorrectly kept the public signature byte-identical and constructed `DisabledSigner` internally; that contradicted overview §"P6-B Scope" line 108 + boundary doc §2.2 "secondary ctor-injection site at `crates/app::wire_phase4` (`Arc<dyn Signer>` parameter)"). `crates/app/src/main.rs` (or wherever `wire_phase4` is invoked today; existing call site at `crates/app/src/lib.rs:176`) updates to pass `Arc::new(DisabledSigner::default()) as Arc<dyn Signer>`.
(2) G2a expectations stated in terms of **delta vs `phase-5-complete` `55679a4` baseline**, not absolute zero. The boundary-doc §4 G2a command `'k256'` substring-matches `keccak256`, so the baseline is NOT zero (pre-existing matches in `crates/state-fetcher/src/storage_key.rs`, `crates/state/src/lib.rs`, `crates/simulator/examples/dump_fixture.rs`, `crates/state-fetcher/src/uniswap.rs` per the P6-A closeout outbox). P6-B asserts **delta zero**: identical hit count to baseline. Boundary-doc text remains untouched in P6-B (any `-w` word-boundary refinement would require a separate boundary-doc amendment outside P6-B scope).
(3) Signing-hook reachability collapsed to ONE unambiguous P6-B behavior: the hook is **never** called from any production runtime path in P6-B — neither `BundleConstructor::construct(...)` nor `BundleConstructor::construct_with_context(...)` invokes it. Reachable **only** through the `#[cfg(test)] pub(crate)` test-accessor. Phase 6b is the only phase that lands the runtime invocation alongside the relay submission path. v0.1 D-B2 wording "invoked from within `construct_with_context` when a future Phase 6b path needs signed bytes" removed entirely.
(4) D-T1 async test plan locked: add `tokio = { workspace = true }` to `crates/execution/Cargo.toml` `[dev-dependencies]` (matches the `crates/signer/Cargo.toml:15` precedent); D-T1 + the `#[cfg(test)] pub(crate) async` accessor use `#[tokio::test]`. No `async-trait` dev-dep needed (the hook is an inherent async method on `BundleConstructor`, not a trait method).
(5) "byte-identical" claims for `BundleConstructor::new` / `with_strategy` replaced with **"public signature + observable behavior preserved"** since Q-B1 (b) adds a private `signer` field whose default is constructed inside both ctor bodies. The struct's public surface — and every call site's observable return value — is preserved; the ctor bodies are not literally byte-identical. Existing BS-4 P3-F regression-guard test in `crates/execution` continues to pass unchanged because it asserts behavior, not implementation.
Awaiting manual Codex re-review.
**Predecessors:**

- Phase 6 overview v0.3 APPROVED HIGH at `c08db38` (pushed).
- P6-A pre-impl plan v0.3 APPROVED HIGH at `4c4c0dd` (pushed).
- P6-A boundary spec `docs/specs/phase-6a-boundary.md` at `64ffaee` (pushed).

## Scope

P6-B wires the **first** `Signer`-using site in the workspace, fail-closed. Reuses existing P5-C boundary types; introduces **two** new `crates/signer` dep edges (`crates/execution` primary; `crates/app` secondary); extends G2c/G2d allow-list by **three file entries** (v0.4: `crates/execution/src/lib.rs` + `crates/app/src/lib.rs` + `crates/app/tests/wire_phase4.rs` — the test entry was added in the v0.3 → v0.4 D-B7 drop; see Status changelog); G2e flips from 0 → 2; G11 enforces at P6-B close.

The injected impl is **`DisabledSigner` only**. Every signing call MUST return `Err(SignerError::SignerDisabled)` per the §3 PRECEDENCE rule in `docs/specs/phase-6a-boundary.md`. No production signer impl, no key material, no funded key, no `eth_sendBundle`, no actual relay submission, no `live_send=true`, no live-network test code.

## Reused (NO duplication)

Per overview Q-P6-H + Codex v0.2 verdict on overview §"Batch independence", P6-B does NOT introduce duplicate boundary types. The P5-C surfaces are reused verbatim:

- `rust_lmax_mev_signer::BundleTx` at `crates/signer/src/bundle_tx.rs:23` (`#[non_exhaustive]` struct; ctor `BundleTx::new(...)`). Carries `bundle_correlation_id: u64` matching `EventEnvelope::correlation_id` per the P5-C boundary contract.
- `rust_lmax_mev_signer::SignedTxBytes` at `crates/signer/src/bundle_tx.rs:68` (transparent `Vec<u8>` newtype). No `Default`, no `Display`.
- `rust_lmax_mev_signer::Signer` trait at `crates/signer/src/signer_trait.rs:9` (object-safe async via `async-trait`; single method `async fn sign_tx(&self, tx: &BundleTx) -> Result<SignedTxBytes, SignerError>`).
- `rust_lmax_mev_signer::DisabledSigner` at `crates/signer/src/disabled.rs` (unit struct; `Default`-able; every `sign_tx` returns `Err(SignerError::SignerDisabled)`).
- `rust_lmax_mev_signer::SignerError` (`#[derive(PartialEq, Eq)]`; `Display` literal `"Phase 6b Production Gate"` per P5-C SC-3).

**NO** new `BundleTx` / `SignedTxBytes` / `Signer` types in `crates/types`, `crates/execution`, or `crates/app`.

## Deliverables

### D-B0 — Boundary-doc amendment (predecessor; doc-only; lands FIRST) — v0.3 NEW

`docs/specs/phase-6a-boundary.md` §4 G2a row currently reads:

```text
| G2a | `rg -n --type rust -e 'Wallet' -e 'PrivateKey' -e 'secp256k1' -e 'k256' -e 'sign_transaction' -e 'funded' crates/` | 0 hits | Phase 5 carry. |
```

Problem: `'k256'` substring-matches `keccak256`, producing 20 pre-existing non-zero hits at `phase-5-complete` `55679a4` (across `crates/state-fetcher/src/storage_key.rs`, `crates/state/src/lib.rs`, `crates/simulator/examples/dump_fixture.rs`, `crates/state-fetcher/src/uniswap.rs`). The "0 hits" expected value is therefore inconsistent with the post-Phase-5 baseline today; admitting the discrepancy in P6-B v0.2 would have left the locked spec violated.

**D-B0 fix:** change the `'k256'` pattern to `'\bk256\b'` (ripgrep default-regex word boundary, supported via `--engine=default` which is the ripgrep default; the `\b` token is honored without `--pcre2`). Post-amendment, the same command yields **0 hits** at `phase-5-complete` `55679a4` (no real `k256` crate symbol in the workspace; only `keccak256` substring matches existed). The amendment also documents in §4 prose that "`\bk256\b` excludes the `keccak256` substring match per P6-B D-B0".

The remaining G2a tokens (`Wallet`, `PrivateKey`, `secp256k1`, `sign_transaction`, `funded`) do NOT need word-boundary refinement in P6-B; no false-positive substring matches exist for them in the current workspace (verified manually 2026-05-16). If a future batch finds one, a follow-up doc amendment lands then.

**D-B0 scope is doc-only**: no `.rs`, no `Cargo.toml`, no ADR. Lands as its own commit BEFORE D-B1..D-B7. Once committed, the rest of this plan's gate expectations (including G2a delta-zero now meaning "0 hits absolute, identical to post-amendment baseline") become internally consistent with the locked boundary spec.

**D-B0 commit message:** `docs(p6-a): boundary spec G2a regex fix — \bk256\b word boundary excludes keccak256 substring`.

### D-B1 — Two new `Cargo.toml` dep edges + one new dev-dep

- `crates/execution/Cargo.toml` — add `rust-lmax-mev-signer = { path = "../signer" }` under `[dependencies]`. This is the **primary** `Signer`-using boundary. **v0.2 adds:** `tokio = { workspace = true }` under `[dev-dependencies]` for `#[tokio::test]` on D-T1 (matches `crates/signer/Cargo.toml:15` precedent). No new `[dependencies]` other than `rust-lmax-mev-signer`.
- `crates/app/Cargo.toml` — add `rust-lmax-mev-signer = { path = "../signer" }` under `[dependencies]`. This is the **secondary** `Signer` ctor-injection boundary. `crates/app` already has `tokio` available; no dev-dep change needed.

Both `rust-lmax-mev-signer` edges are direct path deps per overview Q-P6-H recommendation (no shim crate). Workspace package-naming convention `rust-lmax-mev-*` matched; the G2e regex in `docs/specs/phase-6a-boundary.md` §4 (`'signer = \{ path = "../signer" \}'`) substring-matches `rust-lmax-mev-signer = { path = "../signer" }` (the trailing `signer = {...}` substring is present).

### D-B2 — `BundleConstructor::with_signer(...)` primary boundary

Extend `BundleConstructor` (`crates/execution/src/lib.rs:127`) with:

```text
fn with_signer(cfg: BundleConfig, strategy: BidStrategyRef, signer: Arc<dyn Signer>) -> Result<Self, ExecutionError>
```

- Stores `signer: Arc<dyn rust_lmax_mev_signer::Signer>` as a new private field on `BundleConstructor`.
- Reuses the existing `validity_block_window` validation from `with_strategy`.
- Existing `BundleConstructor::new(...)` and `BundleConstructor::with_strategy(...)` preserve their **public signatures and observable behavior** (return type, `Err(ExecutionError::Setup)` wording, default-strategy selection, `construct(...)` output). The ctor bodies internally construct `Arc::new(DisabledSigner::default())` for the new private `signer` field; the existing BS-4 P3-F regression-guard test continues to pass because it asserts behavior. v0.2 NOTE: this is **NOT** literal byte-identity at the ctor body level. **See Q-B1** for the field-ownership decision.
- Adds a `BundleConstructor`-private async signing-request hook (inherent method, not a trait method — so no `async-trait` dev-dep needed) that calls `self.signer.sign_tx(&BundleTx).await` and propagates `Result<SignedTxBytes, SignerError>` upward.

**P6-B reachability (v0.2, single unambiguous behavior):** the signing-request hook is **NEVER** called from any production runtime path in P6-B. Specifically, `BundleConstructor::construct(...)` and `BundleConstructor::construct_with_context(...)` do **NOT** invoke the hook in Phase 6a. The hook is reachable **only** through the `#[cfg(test)] pub(crate)` test-accessor (D-T1). Phase 6b is the ONLY phase that lands the runtime invocation alongside the relay submission path.

### D-B3 — `wire_phase4` signer injection (secondary boundary; APPROVED-boundary aligned)

- `wire_phase4` accepts a **new** `signer: Arc<dyn Signer>` parameter per overview §"P6-B Scope" line 108 + boundary doc §2.2. New signature: `wire_phase4(config: &Config, opts: WireOptions, signer: Arc<dyn Signer>) -> Result<AppHandle4, AppError>`. v0.1 incorrectly kept the public signature byte-identical and constructed the `DisabledSigner` internally; v0.2 corrects that contradiction.
- **v0.3: all in-tree `wire_phase4` callers enumerated.** Two call sites exist:
  - **C1 (production):** `crates/app/src/lib.rs:176` inside the synchronous `run()` entrypoint. Updates to:

    ```text
    let signer: Arc<dyn Signer> = Arc::new(DisabledSigner::default());
    let handle = runtime.block_on(wire_phase4(&config, WireOptions::default(), signer))?;
    ```

    This site directly names `Signer` and `DisabledSigner`; `crates/app/src/lib.rs` is in the G2c allow-list, so the symbol references are accounted for.
  - **C2 (integration test):** `crates/app/tests/wire_phase4.rs:36` inside the W-1 deterministic-shutdown test. v0.4: this caller directly constructs `let signer: Arc<dyn Signer> = Arc::new(DisabledSigner::default());` and passes it as the third arg (the v0.3 helper-indirection approach is not implementable because `#[cfg(test)]` items in `crates/app/src/lib.rs` are not in scope from integration tests under `crates/app/tests/`). The test file joins the G2c allow-list as a third entry (D-B4 below).
- The injected `Arc<dyn Signer>` is threaded into `BundleConstructor::with_signer(cfg, strategy, signer)` instead of `BundleConstructor::new(cfg)` at `crates/app/src/lib.rs:792-795`.
- `AppHandle4` does NOT grow any new accessor in P6-B (v0.4: the v0.3 `bundle_constructor()` test accessor is dropped — no integration-test caller can reach a `#[cfg(test)]` item from `crates/app/tests/`, and D-T1 already covers the constructor-level injection assertion in `crates/execution`). Phase 6b can add a production accessor if needed for runtime state inspection.

### D-B7 — DROPPED in v0.4

The v0.3 test-helper indirection (`wire_phase4_with_default_signer_for_test` + `assert_disabled_signer_through_app_handle_for_test`) is removed. Codex correctly observed that `#[cfg(test)]` items inside `crates/app/src/lib.rs` are NOT reachable from integration tests under `crates/app/tests/` — those tests compile the library as a normal external dependency. The proposed helpers would not compile at the W-1 call site.

v0.4 replaces D-B7 with **allow-list expansion** (D-B4 below expands by exactly 1 additional file entry, `crates/app/tests/wire_phase4.rs`) + **D-T3 reframing** (D-B5 below: D-T3 becomes a static-shape regression test, not a positive injection assertion).

### D-B4 — G2c allow-list extension (+ 3 file entries; v0.4)

The G2c symbol-inventory baseline at `phase-5-complete` `55679a4` is `crates/signer/...` only. P6-B extends the allow-list by **exactly three** new file entries:

- **Site E (execution, primary):** `crates/execution/src/lib.rs` — every line referencing `Signer` / `DisabledSigner` / `SignerError` / `SignerDisabled`. The plan locks the allow-list down at file granularity (NOT line:precision) because the `BundleConstructor::with_signer` ctor + the internal signing-request hook + the use-site test reference these symbols across several lines (P5-C precedent for `crates/signer/...`).
- **Site A (app src, secondary):** `crates/app/src/lib.rs` — every line referencing `Signer` / `DisabledSigner`. Same file-granularity allow-list.
- **Site T (app test, v0.4 NEW):** `crates/app/tests/wire_phase4.rs` — the W-1 integration test directly constructs `Arc::new(DisabledSigner::default())` to call the new `wire_phase4(config, opts, signer)` signature. Codex's v0.3 verdict explicitly named this option ("explicitly expand the G2c/G2d allow-list to cover the integration test file"). The test file gains a small number of G2c symbol references confined to the W-1 setup section.

`crates/types/`, `crates/relay-clients/`, `crates/relay-sim/`, `crates/bundle-relay/`, and every other crate stay at **zero hits** for G2c symbols. Adding a symbol-use in any other crate is a P6-B gate failure.

### D-B5 — Lean tests (D-T1..T4)

- **D-T1 (unit, `crates/execution`):** `BundleConstructor::with_signer(cfg, strategy, Arc::new(DisabledSigner))` constructs successfully (valid `cfg`); calling the internal signing-request hook via a `#[cfg(test)]` accessor returns `Err(SignerError::SignerDisabled)` for a `BundleTx::new(...)` sample.
- **D-T2 (unit, `crates/execution`):** `BundleConstructor::with_signer` returns `Err(ExecutionError::Setup)` for `validity_block_window == 0` (parity with `new` and `with_strategy`).
- **D-T3 (integration, `crates/app`; v0.4 reframed):** modify the existing W-1 integration test at `crates/app/tests/wire_phase4.rs:29-50` to (a) construct `let signer: Arc<dyn Signer> = Arc::new(DisabledSigner::default());`, (b) pass it as the third argument to `wire_phase4(&config, WireOptions { init_observability: false }, signer)`, and (c) preserve the existing assertion that the result matches `Err(AppError::Node(_)) | Err(AppError::Io(_))` within 5s for the bogus URL. This is a **static-shape regression test, NOT a positive end-to-end signer-injection assertion** — it proves `wire_phase4` accepts the new signer parameter without breaking the existing fail-closed bogus-URL behavior. Positive signer-injection assurance is provided by D-T1 (`BundleConstructor::with_signer` directly tested at the constructor level in `crates/execution`) + manual code-review of the single one-line `BundleConstructor::with_signer(cfg, strategy, signer)` site inside `wire_phase4` (visible at `crates/app/src/lib.rs:792-795` post-Step 7c). The test file imports `Signer` + `DisabledSigner` from `rust_lmax_mev_signer` — these symbol references are accounted for in the G2c allow-list (D-B4 Site T). Q-B3 recommendation: **v0.4 (e) drop end-to-end positive assertion in favor of D-T1 coverage + code review + G11 grep gate**. (v0.3 option (d) infeasible because `wire_phase4` cannot succeed in the existing test infrastructure without a live geth endpoint, and integration tests cannot reach `#[cfg(test)]` library items.)
- **D-T4 (compile-fail OR negative-integration):** Importing any non-`DisabledSigner` impl from `crates/signer` is impossible because no such impl exists. A doc-comment in `crates/execution/src/lib.rs` near `with_signer` MUST state this invariant verbatim. **No proc-macro / trybuild test added** (keeps test footprint lean; the doc-comment + G2a/G2b verbatim grep gates already enforce zero alternate impls).

Existing P5-C SC-1..5 tests under `crates/signer/` remain unchanged.

### D-B6 — Doc-comment additions (no spec change)

- `crates/execution/src/lib.rs` near `BundleConstructor::with_signer`: doc-comment cross-linking `docs/specs/phase-6a-boundary.md` §2.2 + §3 PRECEDENCE rule + §"Reachability of the signing-request hook" below.
- `crates/app/src/lib.rs` near the new `Arc::new(DisabledSigner::default())` site: doc-comment cross-linking `docs/specs/phase-6a-boundary.md` §2.2.

No edit to `docs/specs/execution-safety.md` or any ADR. **v0.4 exception:** `docs/specs/phase-6a-boundary.md` IS edited by D-B0 only — the single-pattern §4 G2a regex amendment (`'k256'` → `'\bk256\b'`) plus the accompanying §4 prose note. No other section of `phase-6a-boundary.md` is touched.

## Reachability of the signing-request hook (locked at P6-B, single unambiguous behavior)

Phase 6a is fail-closed. The signing-request hook in P6-B has **exactly one** reachable invocation pattern:

- **NO runtime invocation in Phase 6a.** `BundleConstructor::construct(...)` and `BundleConstructor::construct_with_context(...)` do **NOT** call `sign_tx` in P6-B. The runtime path emits a `BundleCandidate` exactly as it does at `phase-5-complete`; no signing happens during normal operation. The `signer` field is **stored** but **never read** from production code in P6-B.
- **Test-only invocation in P6-B.** D-T1 invokes the hook via a `#[cfg(test)] pub(crate)` inherent async accessor (`invoke_signer_for_test`) on `BundleConstructor`, gated to `#[cfg(test)]`. Reachable **only** from inline `#[cfg(test)] mod tests { ... }` within `crates/execution/src/lib.rs`. **See Q-B4** for the recommended visibility (`pub(crate)` recommended over `pub(super)` so future inline integration code in the same crate can reach it without re-export).
- **Phase 6b unlock.** Phase 6b will land the production-runtime invocation in `construct(...)` / `construct_with_context(...)` alongside the relay submission path. Phase 6b is the ONLY phase that may surface a non-`Err(SignerDisabled)` from this hook.

This shape:

- Preserves the `phase-5-complete` runtime baseline (231 passed + 1 ignored) exactly at the production-call level; new test additions are counted on top (expected 234 + 1 ignored at P6-B close).
- Satisfies the §3 PRECEDENCE rule from `docs/specs/phase-6a-boundary.md`: `Err(SignerError::SignerDisabled)` short-circuits BEFORE any relay-sim or `submit_bundle`-equivalent code (in Phase 6b, this is true at runtime because the hook will be invoked before the relay path; in P6-B, the property is asserted via D-T1).
- Keeps G3 (`submit_bundle\(` zero hits in `crates/app/src/`) and G11 (single approved `sign_tx` call site, test-reachable only) both green at P6-B close.

## Gates at P6-B close

| Gate | Command | Expected at P6-B close | Delta vs P6-A close |
|---|---|---|---|
| G1 | `rg -n --type rust 'eth_sendBundle' crates/` | doc-comment hits only | unchanged |
| G2a (POST-D-B0 form) | `rg -n --type rust -e 'Wallet' -e 'PrivateKey' -e 'secp256k1' -e '\bk256\b' -e 'sign_transaction' -e 'funded' crates/` | **0 hits absolute**, identical to the post-D-B0 baseline at `phase-5-complete` `55679a4` (D-B0 word-boundary fix eliminates the `keccak256` substring false positives). Plan and locked boundary spec now agree at "0 hits". | aligned with boundary spec after D-B0 |
| G2b | `rg -n --glob 'crates/**/Cargo.toml' -e 'alloy-signer' -e 'ethers-signers' -e 'secp256k1' -e 'k256'` | 0 hits | unchanged |
| G2c | `rg -n --type rust -e 'Signer' -e 'DisabledSigner' -e 'SignerError' -e 'SignerDisabled' crates/` | Phase 5 baseline (`crates/signer/`) **PLUS** `crates/execution/src/lib.rs` **PLUS** `crates/app/src/lib.rs` **PLUS** `crates/app/tests/wire_phase4.rs` | **+3 file allow-list entries** (v0.4) |
| G2d | Same command as G2c. | Every hit is inside the allow-list of (a) any `crates/signer/...` file, (b) `crates/execution/src/lib.rs`, (c) `crates/app/src/lib.rs`, (d) `crates/app/tests/wire_phase4.rs`. **Zero hits outside.** | **redefinition enforced at P6-B close** (per overview v0.3 fix item 3) |
| G2e | `rg -n --glob 'crates/**/Cargo.toml' 'signer = \{ path = "../signer" \}'` | **exactly 2 hits**, in `crates/execution/Cargo.toml` AND `crates/app/Cargo.toml` | **0 → 2** |
| G3 | `rg -n --type rust 'submit_bundle\(' crates/app/src/` | 0 hits | unchanged |
| G4 | `rg -n --type rust -e 'dyn BundleRelay' -e 'Arc<dyn BundleRelay>' crates/app/src/` | 0 hits | unchanged |
| G5 | `rg -n --type rust 'live_send' crates/` | config/struct/error/docs only | unchanged |
| G6 | `rg -n --type rust 'api_key' crates/` | field-access only | unchanged |
| G7 | `rg -n --type rust '#\[ignore\]' crates/` | P2-C `g_state_live` only | unchanged |
| G8 | `cargo tree -d` | no cycles; the two new `rust-lmax-mev-signer` edges are leaf edges (signer has no in-workspace deps that loop back) | unchanged shape-wise |
| G9 | `rg -n --type rust 'KillSwitch' crates/` | `crates/bundle-relay/` + `crates/app/` only | unchanged |
| G10 (documented; enforces P6-D) | per-adapter `submit_bundle` first statement | not yet enforced | n/a at P6-B |
| G11 (NEW; **enforces at P6-B close**) | `rg -n --type rust 'sign_tx' crates/execution/src/` plus manual inspection that the only call site routes through `&dyn Signer` inside the `BundleConstructor`-private signing-request hook | single approved call site; reachable only via the test-only `#[cfg(test)]` accessor in P6-B; D-T1 asserts `DisabledSigner` returns `Err(SignerError::SignerDisabled)` | **0 → 1 (first enforcement)** |

Workspace tests at P6-B close: **231 + 3 = 234 passed + 1 ignored** (D-T1 + D-T2 + D-T3; D-T4 is a doc-comment assertion, no test counted).

## Forbidden in P6-B

- Any production signer impl (test-ephemeral or otherwise — `DisabledSigner` is the only impl).
- Any private key material in repo / tests / fixtures / configs / env / runtime.
- Any funded key.
- Any new `secp256k1` / `k256` / `alloy-signer` / `ethers-signers` / `Wallet` / `PrivateKey` / `sign_transaction` symbol in code (G2a stays unchanged from baseline).
- Any new `alloy-signer` / `ethers-signers` / `secp256k1` / `k256` Cargo dep (G2b stays 0).
- Any duplicate `BundleTx` / `SignedTxBytes` / `Signer` type in `crates/types` / `crates/execution` / `crates/app` (reuse from `crates/signer` only).
- Any runtime invocation of `sign_tx` from `BundleConstructor::construct(...)` or `BundleConstructor::construct_with_context(...)` in Phase 6a (test-only accessor only).
- Populating `RelaySimRequest::txs` with non-empty bytes (P6-C scope).
- Any `tracing::*!` logging signer state beyond `signer_set: bool`.
- Any `eth_sendBundle` runtime call.
- Any actual relay submission (`submit_bundle` returns `Err(SubmitDisabled)` unchanged).
- Any `live_send = true` capability.
- Any live-network test code, env-gated or otherwise.
- Any new `#[ignore]` test in `crates/` (G7 stays at the P2-C baseline).
- Any new `submit_bundle` caller in `crates/app/src/` (G3 stays 0).
- Any ADR text amendment.
- Any edit to `docs/specs/execution-safety.md` (read-only at P6-B).
- Any edit to `docs/specs/phase-6a-boundary.md` **other than the D-B0 §4 G2a regex amendment (`'k256'` → `'\bk256\b'`) plus the accompanying §4 prose note**. The rest of `phase-6a-boundary.md` remains read-only at P6-B; D-B0 is the single, narrowly-scoped exception.
- Any `.claude/` / `AGENTS.md` / `fixture_output.txt` / `hook_toast.md` staging.
- Any destructive git operation or force-push.
- Any asset-scope widening, V3-fee-tier addition, or venue addition.

## Plan execution checklist (TDD-style)

- [ ] **Step 0 (v0.3 NEW; doc-only; lands FIRST as its own commit):** Apply D-B0 boundary-doc amendment: edit `docs/specs/phase-6a-boundary.md` §4 G2a row `'k256'` → `'\bk256\b'` and add the §4 prose note. Run `rg -n --type rust -e 'Wallet' -e 'PrivateKey' -e 'secp256k1' -e '\bk256\b' -e 'sign_transaction' -e 'funded' crates/` and confirm **0 hits**. Commit + push with message `docs(p6-a): boundary spec G2a regex fix — \bk256\b word boundary excludes keccak256 substring`. THEN proceed to Step 1.
- [ ] **Step 1: Confirm predecessor state.** `git log --oneline -5` shows D-B0 commit at HEAD; boundary doc on disk with the fix; `phase-5-complete` tag at `55679a4`; workspace `cargo test --workspace` baseline 231 + 1 ignored.
- [ ] **Step 2: Red — write D-T1 first.** Add `with_signer` ctor signature stub returning `unimplemented!()` and the `#[cfg(test)]` signing-request-hook accessor stub returning `unimplemented!()`; D-T1 fails at compile or at the `unimplemented!()` panic. Confirm red state.
- [ ] **Step 3: Green — implement `with_signer` + the signing-request hook.** Add the `signer` field to `BundleConstructor`; thread it through `with_signer`. Hook calls `self.signer.sign_tx(&BundleTx).await`. D-T1 passes.
- [ ] **Step 4: Green — D-T2.** `with_signer` validates `validity_block_window != 0` (reuse the existing check; D-T2 passes immediately).
- [ ] **Step 5: Add `rust-lmax-mev-signer = { path = "../signer" }` to `crates/execution/Cargo.toml` `[dependencies]` AND `tokio = { workspace = true }` to `[dev-dependencies]`.** Run `cargo build -p rust-lmax-mev-execution` + `cargo test -p rust-lmax-mev-execution` to confirm green.
- [ ] **Step 6: Add `rust-lmax-mev-signer = { path = "../signer" }` to `crates/app/Cargo.toml` `[dependencies]`.** `tokio` already present.
- [ ] **Step 7a: Update `wire_phase4` signature to accept `signer: Arc<dyn Signer>`.** New signature: `pub async fn wire_phase4(config: &Config, opts: WireOptions, signer: Arc<dyn Signer>) -> Result<AppHandle4, AppError>`.
- [ ] **Step 7b: Update the in-tree `wire_phase4` caller at `crates/app/src/lib.rs:176`** to construct `let signer: Arc<dyn Signer> = Arc::new(DisabledSigner::default());` and pass it as the third arg. (`DisabledSigner` is the only reachable impl in Phase 6a.)
- [ ] **Step 7c: Wire `wire_phase4` body to `BundleConstructor::with_signer(cfg, strategy, signer)`.** Replace the existing `BundleConstructor::new(...)` site at `crates/app/src/lib.rs:792-795`. The strategy passed in is the same default-strategy chain (`FixedFractionBidStrategy::new(cfg.fixed_bid_fraction_bps)?` via `with_strategy`'s public ctor) so observable runtime behavior is preserved aside from the unused-in-P6-B signer being stored.
- [ ] **Step 7d (v0.4): Update integration-test caller C2 at `crates/app/tests/wire_phase4.rs:36`.** Add `use rust_lmax_mev_signer::{DisabledSigner, Signer};` and `use std::sync::Arc;` (Arc may already be in scope). Construct `let signer: Arc<dyn Signer> = Arc::new(DisabledSigner::default());` immediately before the `tokio::time::timeout` call. Pass `signer` as the third arg to `wire_phase4(&config, WireOptions { init_observability: false }, signer)`. The existing `Err(AppError::Node) | Err(AppError::Io)` assertion is unchanged. (v0.3 helper-indirection approach removed because `#[cfg(test)]` items in `crates/app/src/lib.rs` are not reachable from integration tests.)
- [ ] **Step 7e (v0.4): No source-side helpers added.** D-B7 dropped per v0.4. Confirm no `wire_phase4_with_default_signer_for_test` or `assert_disabled_signer_through_app_handle_for_test` exists in `crates/app/src/lib.rs`.
- [ ] **Step 7f (v0.4): No `bundle_constructor()` accessor.** D-T1 in `crates/execution` already covers `BundleConstructor::with_signer` injection assertion; manual code-review of `crates/app/src/lib.rs:792-795` covers the single `BundleConstructor::with_signer(cfg, strategy, signer)` call site; G11 grep gate enforces the `sign_tx` single-call-site invariant.
- [ ] **Step 8: Green — D-T3.** Run `cargo test -p rust-lmax-mev-app --test wire_phase4` and confirm the modified W-1 test still returns the expected `Err(AppError::Node)` or `Err(AppError::Io)` within 5s. This proves `wire_phase4` accepts the new signer parameter and the existing fail-closed bogus-URL behavior is preserved.
- [ ] **Step 9: Add D-B6 doc-comments.** Cross-link `docs/specs/phase-6a-boundary.md` from both the `with_signer` ctor and the `wire_phase4` injection site.
- [ ] **Step 10: Run full gate set.** Workspace `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (expect 234 passed + 1 ignored), `cargo deny check`, `cargo tree -d`, and the G1..G11 ripgrep gate set from §"Gates at P6-B close".
- [ ] **Step 11: Commit + push** the `Cargo.toml` + `crates/execution/src/lib.rs` + `crates/app/src/lib.rs` + new tests as a routine implementation commit with message `feat(p6-b): signing-request pipeline (fail-closed) — BundleConstructor::with_signer + wire_phase4 DisabledSigner injection`.
- [ ] **Step 12: Update `.coordination/codex_review.md` + `.coordination/claude_outbox.md`** with the P6-B closeout report; emit P6-C pre-impl plan draft.

## Risks + open questions

- **Q-B1 — `signer` field on the existing `BundleConstructor::new` / `with_strategy` ctors?** Three options:
  - (a) Add `signer: Option<Arc<dyn Signer>>` to `BundleConstructor`; `new` + `with_strategy` set it to `None`; `with_signer` sets it to `Some(_)`. The signing-request hook returns `Err(SignerError::SignerDisabled)` when `None` (defensive). Cleanest API; existing call-site observable behavior preserved.
  - (b) Add `signer: Arc<dyn Signer>` (non-optional); `new` + `with_strategy` internally construct `Arc::new(DisabledSigner)` as the default. Simpler type; existing tests pass unchanged because the field is private and unused at runtime.
  - (c) Have `with_signer` return a separate `SigningBundleConstructor` newtype wrapping `BundleConstructor`. Avoids any field addition to `BundleConstructor` but adds a new public type.
  - **Recommend (b).** Lowest API churn; the field is private; `DisabledSigner` is `Default`-able; existing tests preserve **public signature + observable behavior** (not literal byte-identity at the ctor body level — see v0.2 fix item 5). The only observable shape change is the new `with_signer` ctor. Codex verdict?
- **Q-B2 — G2c/G2d allow-list granularity: file or line?** Phase 5 P5-C used file-granularity for `crates/signer/...`. Recommend the same here (`crates/execution/src/lib.rs` and `crates/app/src/lib.rs` whole-file entries). Line:precision creates churn on every refactor. Codex verdict?
- **Q-B3 — D-T3 verifiability (v0.4 reframed)?** Options:
  - (a) `#[cfg(test)] pub fn signer_is_disabled(&self) -> bool` directly on `AppHandle4`.
  - (b) `#[cfg(test)] pub(crate) fn bundle_constructor(&self)` accessor — **infeasible** for integration-test reach (Codex v0.3 verdict; `#[cfg(test)]` library items are not in scope from `crates/app/tests/`).
  - (c) Skip the accessor; D-T3 exercises only construction. **REJECTED by Codex v0.2 verdict** — does not prove signer injection.
  - (d) v0.3 helper-indirection — **infeasible** for the same reason as (b).
  - (e) v0.4: **drop positive end-to-end signer-injection assertion**. D-T3 = W-1 modified to pass `Arc::new(DisabledSigner::default())` and preserve the existing bogus-URL fail-closed assertion. Positive injection assurance is provided by D-T1 (`BundleConstructor::with_signer` directly tested in `crates/execution`) + manual code review of the single one-line `with_signer` call site in `wire_phase4` + G11 grep gate on the `sign_tx` single-call-site invariant. Allow-list expands by 1 file entry (`crates/app/tests/wire_phase4.rs`).
  - **Recommend (e).** Codex verdict?
- **Q-B4 — Signing-request hook visibility for D-T1?** Options:
  - (a) `#[cfg(test)] pub(crate) async fn invoke_signer_for_test(&self, tx: &BundleTx) -> Result<SignedTxBytes, SignerError>` on `BundleConstructor`. Reachable from inline `#[cfg(test)] mod tests` only.
  - (b) `#[cfg(any(test, feature = "test-helpers"))]` so integration tests under `crates/execution/tests/` can call it. Adds a `test-helpers` feature; bigger blast radius.
  - **Recommend (a).** Minimal blast radius; inline unit test suffices for D-T1. Codex verdict?
- **Q-B5 — G2e regex robustness.** The boundary doc §4 G2e pattern is `'signer = \{ path = "../signer" \}'`. This **substring-matches** `rust-lmax-mev-signer = { path = "../signer" }` because the trailing `signer = {...}` is a substring. The regex therefore captures both edges correctly at the 2-hit expectation. **No spec amendment to the boundary doc is required**; this note is informational only so Codex can confirm intent. Codex verdict?
- **Q-B6 — Workspace dep naming.** The workspace convention is `rust-lmax-mev-*` package names. The overview §"Per-batch detail P6-B" Scope wording uses the shorthand `signer = { path = "../signer" }` but the actual `Cargo.toml` line will read `rust-lmax-mev-signer = { path = "../signer" }`. The G2e regex still substring-matches. Codex verdict on whether the overview wording needs a follow-up clarification (no spec amendment in P6-B; flagged for visibility).
- **Q-B7 — Strategy in `with_signer`?** `with_signer` MUST accept the strategy (Q-B1 (b) recommendation means the existing `strategy` field is also private to `BundleConstructor`). Recommend signature `with_signer(cfg: BundleConfig, strategy: BidStrategyRef, signer: Arc<dyn Signer>) -> Result<Self, ExecutionError>` so the call site at `wire_phase4` reads `BundleConstructor::with_signer(cfg, FixedFractionBidStrategy::new(...).into(), Arc::new(DisabledSigner))`. Alternative `with_signer(cfg, signer)` (uses default strategy internally) is also defensible. Recommend the explicit-strategy form for symmetry with `with_strategy`. Codex verdict?

## Tests summary

- D-T1: hook returns `Err(SignerDisabled)` (unit, `crates/execution`).
- D-T2: `with_signer` validates `validity_block_window` (unit, `crates/execution`).
- D-T3 (v0.4): W-1 integration test modified — proves `wire_phase4` **accepts** the new `signer: Arc<dyn Signer>` parameter without breaking the existing bogus-URL fail-closed behavior (`Err(AppError::Node)` / `Err(AppError::Io)` within 5s). Does NOT prove positive end-to-end signer injection (that coverage is provided by D-T1 + manual code review of `crates/app/src/lib.rs:792-795` + G11 grep gate; see §"What D-T3 proves vs does not prove" in the outbox pack).
- D-T4: doc-comment invariant (no proc-macro test).

Expected workspace total: **234 passed + 1 ignored** at P6-B close.

## Process

Per the 2026-05-04 routine-closeout policy + the overview §Process:

1. Claude writes this pre-impl plan to disk (UNCOMMITTED) + emits the review pack to `.coordination/claude_outbox.md`.
2. Claude STOPS and reports "manual Codex review required". **No `.rs` / `Cargo.toml` / ADR / docs/specs edits in this turn.**
3. User pastes pack to Codex; relays verdict.
4. Claude records verdict in `.coordination/codex_review.md`.
5. **APPROVED** → commit + push this plan as a routine doc commit; THEN execute per §"Plan execution checklist"; THEN commit + push the impl; THEN draft P6-C pre-impl plan.
6. **REVISION REQUIRED** → revise plan in place + re-emit pack.
7. **Scope / ADR change required** → HALT to user.
