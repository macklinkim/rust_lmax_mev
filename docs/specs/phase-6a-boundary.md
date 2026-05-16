# Phase 6a Safety Boundary

**Status:** Stable spec (no version header per P6-A Q-A3 recommendation).
**Authored:** 2026-05-16 KST under P6-A (`docs/superpowers/plans/2026-05-16-phase-6-batch-a-boundary-spec-execution.md`, APPROVED HIGH at commit `4c4c0dd`).
**Parent policy:** `docs/specs/execution-safety.md`.

## §1 Purpose + scope

This doc captures the Phase 6a **fail-closed safety contract** between the three safety devices (`submit_bundle`, `Signer`, `KillSwitch`) and locks the grep-gate set that every Phase 6a batch (P6-A..F) MUST satisfy.

- **In scope:** Phase 6a Pre-Production Gate — shadow-run + per-adapter kill-switch wiring + production-signer **design doc** (not impl) + comparator + wiremock relay tests.
- **Out of scope:** Phase 6b Production Gate (funded key / `live_send=true` / `eth_sendBundle` runtime path / real `Signer` impl reachable from `crates/app`). Phase 6b lives in a separate boundary doc, requires fresh user authorization, and is NOT authored under 6a.

Cross-link: `docs/specs/execution-safety.md` is the parent policy; this doc refines its Phase 6a contract. The reverse-link from `execution-safety.md` is deferred to a later Phase 6a batch (Q-A1 resolved).

## §2 Three safety boundaries

### §2.1 Submission boundary — `submit_bundle`

Every `BundleRelay` impl in `crates/relay-clients/` (Flashbots adapter + bloXroute adapter) implements `fn submit_bundle(...) -> Result<_, BundleRelayError>`. Phase 5 invariant carries forward verbatim into Phase 6a:

- **Returns `Err(BundleRelayError::SubmitDisabled)` unconditionally** in every adapter. No conditional branch returns `Ok`.
- **Zero call sites in `crates/app/src/`** (G3). The producer/consumer wiring never invokes `submit_bundle`.
- **Adapters held as `Arc<dyn RelaySimulator>` only** in `crates/app` (G4 / DP-E13 upcast prevention). The `BundleRelay` super-trait is not type-visible to `crates/app`.

Phase 6a does NOT change this. Phase 6b is the only context where `submit_bundle` may return anything other than `Err(SubmitDisabled)`.

### §2.2 Signer boundary — `crates/signer::Signer`

The `Signer` trait + `DisabledSigner` fail-closed impl + `SignerError::SignerDisabled` variant landed at Phase 5 P5-C (boundary-only; isolated leaf crate; `crates/app` does NOT depend on it at the `phase-5-complete` tag).

Phase 6a P6-B wires the **first** `Signer`-using site:

- **Primary boundary:** `crates/execution::BundleConstructor::with_signer(...)` — new ctor that accepts `Arc<dyn Signer>` and routes every signing call through `&dyn Signer::sign_tx(...)`.
- **Secondary boundary:** `crates/app::wire_phase4` — accepts `Arc<dyn Signer>` and injects into `BundleConstructor::with_signer`.

In Phase 6a the **only reachable impl is `crates/signer::DisabledSigner`**, whose `sign_tx` returns `Err(SignerError::SignerDisabled)` (`Display` pins the literal `"Phase 6b Production Gate"`). No production signer impl exists anywhere in the workspace.

`SignerError::SignerDisabled` short-circuits BEFORE any relay-sim or submission-shape work (see §3 PRECEDENCE).

### §2.3 Kill-switch boundary — `crates/bundle-relay::KillSwitch`

`pub struct KillSwitch(Arc<AtomicBool>)` with `#[derive(Clone)]` landed at Phase 5 P5-D. `AppHandle4::kill_switch()` + `AppHandle4::set_execution_disabled(bool)` already exist.

Phase 5 baseline guard sites:

- `comparator_driver` (top-of-iteration `kill_switch.is_active()` short-circuit).

Phase 6a P6-D extends to per-adapter guards:

- `Flashbots::submit_bundle` first non-trivia statement: `if self.kill_switch.is_active() { return Err(BundleRelayError::KillSwitchActive); }`.
- `bloXroute::submit_bundle` first non-trivia statement: same.

Adapters take `KillSwitch` (not `Arc<KillSwitch>`) directly in their ctors per overview Q-P6-F resolution; `KillSwitch` already owns the `Arc<AtomicBool>` internally.

Atomic ordering: `KillSwitch::is_active()` uses `Ordering::Acquire`; `KillSwitch::set(true)` uses `Ordering::Release` (Q-A5 RESOLVED YES — match the existing `crates/bundle-relay/src/kill_switch.rs` implementation; document the choice so future readers know it is intentional, not incidental).

## §3 `Result::Err` PRECEDENCE rule

For every code path that could reach a submission-equivalent operation, errors MUST short-circuit in the following strict order:

1. **`Err(BundleRelayError::KillSwitchActive)` FIRST.** If the kill switch is active, return this error before any other branch evaluates. The operator can flip the switch at runtime via `AppHandle4::set_execution_disabled(true)`; the guard MUST short-circuit on the NEXT iteration / call without waiting for any in-flight RPC. Atomic ordering: `Acquire` read.
2. **`Err(SignerError::SignerDisabled)` SECOND.** If the signer is `DisabledSigner` (Phase 6a default everywhere), signing fails before any relay-sim or submission-shape work runs. In Phase 6a this error fires on EVERY signing call.
3. **`Err(BundleRelayError::SubmitDisabled)` LAST.** Every adapter's `submit_bundle` returns this if neither of the above short-circuited. **Phase 6a never reaches this branch via the runtime path** because (a) `crates/app` has no `submit_bundle` callers (G3 = 0) and (b) the signer always errors out earlier (G11 verifies the routing).

**Phase 6b unlock note:** Phase 6b is the ONLY context where any of these `Err` returns may legitimately become `Ok`. Phase 6b will need its own boundary update + fresh user authorization + separate Codex review. None of `submit_bundle`/`sign_tx`/`KillSwitchActive` change semantics in Phase 6a.

## §4 Grep-gate set (G1..G11)

Path scope is **explicit on every gate**. The boundary doc itself lives under `docs/specs/` and naturally references safety terms (`eth_sendBundle`, `live_send`, signer symbols, etc.) in prose; every crate-scoped gate uses `crates/` or `crates/**/Cargo.toml` so this doc is outside the scan path.

If a future audit broadens scope (e.g., adds `docs/`), the boundary doc's expected-hits MUST be added to that audit's accounting.

All commands use **unescaped `|` regex alternation** or explicit `-e <pattern>` repetitions. **No BRE-style `\|` escapes.**

| Gate | Command | Expected | Reason |
|---|---|---|---|
| G1 | `rg -n --type rust 'eth_sendBundle' crates/` | Only `//!` / `///` doc-comment lines asserting NO `eth_sendBundle`; zero non-doc hits. | Phase 5 carry. |
| G2a | `rg -n --type rust -e 'Wallet' -e 'PrivateKey' -e 'secp256k1' -e '\bk256\b' -e 'sign_transaction' -e 'funded' crates/` | 0 hits | Phase 5 carry. Forbidden signer-symbol set in code. `\bk256\b` excludes the `keccak256` substring match (P6-B D-B0 fix; ripgrep default-regex word boundary). |
| G2b | `rg -n --glob 'crates/**/Cargo.toml' -e 'alloy-signer' -e 'ethers-signers' -e 'secp256k1' -e 'k256'` | 0 hits | Phase 5 carry. Forbidden dep set. |
| G2c | `rg -n --type rust -e 'Signer' -e 'DisabledSigner' -e 'SignerError' -e 'SignerDisabled' crates/` | **Inventory** of allowed Signer-symbol sites. Phase 5 baseline `55679a4`: all hits under `crates/signer/`. After P6-B: `crates/signer/` + approved P6-B file:line pairs in `crates/execution/` + `crates/app/`. | Phase 5 carry. Allow-list expanded by two sites at P6-B (overview Q-P6-F). |
| G2d | Same command as G2c. | Every hit MUST appear in the explicit allow-list (Phase 5 baseline = `crates/signer/...` only; post-P6-B = + approved sites in `crates/execution/` + `crates/app/`). **Zero hits outside the allow-list.** Relocating or adding an unapproved site is a gate failure. | Positive allow-list gate (overview-locked redefinition). |
| G2e | `rg -n --glob 'crates/**/Cargo.toml' 'signer = \{ path = "../signer" \}'` | After P6-B: exactly 2 hits, in `crates/execution/Cargo.toml` AND `crates/app/Cargo.toml`. Phase 5 baseline + P6-A close: 0 hits. | New dep-edge gate. |
| G3 | `rg -n --type rust 'submit_bundle\(' crates/app/src/` | 0 hits | Phase 5 carry. No caller in app. |
| G4 | `rg -n --type rust -e 'dyn BundleRelay' -e 'Arc<dyn BundleRelay>' crates/app/src/` | 0 hits | Phase 5 carry. DP-E13 upcast prevention. |
| G5 | `rg -n --type rust 'live_send' crates/` | All hits in config validation / struct definition / error variant / doc comments — NO runtime enabling site. | Phase 5 carry. |
| G6 | `rg -n --type rust 'api_key' crates/` | Only field-access positions in adapter ctors / error rendering with redaction; NEVER inside a `tracing::*!` log emission. | Phase 5 R-E20 carry. |
| G7 | `rg -n --type rust '#\[ignore\]' crates/` | Pre-existing P2-C `g_state_live` only (`crates/replay/tests/g_state_live.rs`); no new `#[ignore]` in 6a. | Phase 5 carry. |
| G8 | `cargo tree -d` | No cycles. Duplicate-version edges allowed. | Phase 5 carry. |
| G9 | `rg -n --type rust 'KillSwitch' crates/` | Phase 5 baseline + P6-A close: hits only under `crates/bundle-relay/` and `crates/app/`. After P6-D: extended allow-list ALSO includes `crates/relay-clients/`. | Phase 5 carry; extended in P6-D. |
| G10 | `rg -n --type rust -B 1 -A 3 'fn submit_bundle' crates/relay-clients/src/` plus manual inspection. | Every `impl BundleRelay for ... { fn submit_bundle }` body's FIRST non-trivia statement is `if self.kill_switch.is_active() { return Err(BundleRelayError::KillSwitchActive); }`. | NEW. Documented at P6-A; enforces at P6-D close. Per-adapter kill-switch PRECEDENCE per §3. |
| G11 | `rg -n --type rust 'sign_tx' crates/execution/src/` plus manual inspection. | Single approved call site, routes through `&dyn Signer` inside `BundleConstructor::with_signer` signing-request hook; integration test asserts `DisabledSigner` returns `Err(SignerError::SignerDisabled)` before any downstream work. | NEW. Documented at P6-A; enforces at P6-B close. |

The above commands are **verbatim copy-paste targets** for the P6-F DoD audit.

## §5 Phase 6a hard forbids

Verbatim from the Phase 6 overview (`docs/superpowers/plans/2026-05-16-phase-6-overview-execution.md` §"Hard forbids during all of Phase 6a"):

- no production signer impl
- no funded key
- no private key material in repo / tests / fixtures / configs / env examples / runtime
- no `live_send = true`
- no `eth_sendBundle`
- no actual relay submission
- no real paid API dependency enabled in CI by default
- no live-network test enabled by default
- no Phase 6b Production Gate work
- no `.claude/` / `AGENTS.md` / `fixture_output.txt` / `hook_toast.md` staging
- no destructive git, no force-push
- no asset-scope widening; no extra V3 fee tiers; no extra venues
- no ADR text amendment without separate explicit user approval

## §6 Phase 6b boundary (kept explicit)

Verbatim from overview §"Phase 6b Production Gate (kept entirely separate)". Phase 6b owns:

- production signer impl (HSM/KMS-backed),
- funded key wiring,
- `live_send = true` flip (config-validation un-rejection),
- `eth_sendBundle` runtime path,
- actual relay submission via `submit_bundle` returning `Ok(_)`.

**None of these are touchable in Phase 6a.** Phase 6b requires its own overview doc + fresh explicit user authorization + separate Codex review. The current `crates/signer::SignerError::SignerDisabled` `Display` literal `"Phase 6b Production Gate"` is the only forward-link, and is intentional: any caller that surfaces the error to a human reader names the gate that would unlock it.

## §7 Cross-references

- `docs/adr/ADR-001.md` — mempool ingestion + Phase 6 gate.
- `docs/specs/execution-safety.md` — parent safety policy (`submit_bundle` ban, `live_send` default, funded-key ban, gas-bidding policy, kill switch).
- `docs/superpowers/plans/2026-05-16-phase-6-overview-execution.md` — Phase 6 overview v0.3 APPROVED HIGH at `c08db38`.
- `docs/superpowers/plans/2026-05-16-phase-6-batch-a-boundary-spec-execution.md` — this doc's pre-impl plan (v0.3 APPROVED HIGH at `4c4c0dd`).
