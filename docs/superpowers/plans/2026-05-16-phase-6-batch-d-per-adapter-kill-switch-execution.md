# Phase 6 Batch D — Per-adapter kill-switch wiring (G10 enforcement)

**Date:** 2026-05-16 KST
**Status:** Draft v0.3 (revised 2026-05-16 KST after user **OVERRIDE** of Codex-APPROVED v0.2 — committed at `98ab10c`). User direction: **reduce P6-D test matrix from 8 to 4** before any implementation lands. Three v0.2 → v0.3 changes:
(A) **Test matrix reduced to 4** (Flashbots precedence + bloXroute precedence + one shared-state on Flashbots + one inactive-baseline on bloXroute). All four together cover: the §3 PRECEDENCE rule on BOTH adapters, the shared-`Arc<AtomicBool>` clone-and-flip semantics across the ctor seam (one adapter), and the inactive-baseline regression (the other adapter, on the existing `submit_bundle` path). The G10 manual-inspection enforcement at Step 8 remains the authoritative "FIRST non-trivia statement / no pre-guard work" guard.
(B) **D-T-D7 + D-T-D8 (no-HTTP-I/O) DROPPED.** Rationale: at HEAD `93803b2` (pre-P6-D) `submit_bundle` performs no HTTP I/O at all (every impl returns `Err(SubmitDisabled)` directly), so a wiremock zero-request assertion adds runtime cost without distinguishing "guard ran first" from "guard never ran but no I/O happened anyway". The "no pre-guard work" property is enforced statically by G10 manual inspection — that is sufficient for Phase 6a. If a Phase 6b PR adds real HTTP-issuing code to `submit_bundle`, that PR is the right place to add the no-HTTP-I/O regression guard, scoped to the new behavior.
(C) **Duplicate-adapter coverage dropped.** v0.2 had both Flashbots AND bloXroute shared-state tests (D-T-D3 + D-T-D4) AND both inactive-baseline tests (D-T-D5 + D-T-D6). v0.3 keeps one shared-state (Flashbots) + one inactive-baseline (bloXroute) — asymmetric across adapters. Both adapters use the identical `if self.kill_switch.is_active() { return Err(KillSwitchActive); }` guard pattern (single canonical idiom proven once at the AST level via G10), so the cross-adapter duplication adds no new safety signal in Phase 6a. The PRECEDENCE proof (D-T-D1 + D-T-D2) stays on BOTH adapters because that is the property the boundary spec actually names per-adapter.
Q-D5 reframed to RECOMMEND the lean 4-test matrix. Q-D1..Q-D4 unchanged from v0.2 (Codex APPROVED). Expected workspace test total **243 → 239 passed + 1 ignored** (235 baseline + 4 new). Awaiting manual Codex re-review.

**Predecessor history retained verbatim below**: v0.1 → v0.2 had **four** Codex REVISION REQUIRED HIGH fixes:
(1) **Ctor blast-radius corrected.** v0.1 claimed "zero `crates/app/` impact" but `crates/app/src/lib.rs` constructs both adapters at lines 1523 + 1531 inside `build_relay_sim_from_config`. v0.2 keeps Q-D1 = BREAKING `::new(cfg, kill_switch)` AND **includes the 2 `crates/app/src/lib.rs` call-site updates in scope**. The kill switch is constructed at `crates/app/src/lib.rs:890` AFTER `build_relay_sim_from_config` is called at `:844`; v0.2 reorders so the kill switch is constructed BEFORE the relay-sim build, and `build_relay_sim_from_config(&config.relay, kill_switch.clone())` threads it in. G3 stays 0 (no new `submit_bundle(` caller) and G4 stays 0 (return type at `crates/app/src/lib.rs:1521` stays `Arc<dyn RelaySimulator>` — the upcast prevents `dyn BundleRelay` from appearing in app). The change is reviewed-delta inside `crates/app/src/lib.rs` `build_relay_sim_from_config` ONLY; no other `crates/app/` code is touched.
(2) **Ctor call-site inventory + count fixed.** v0.1 stated "19 inside `crates/relay-clients/`" but the correct decomposition at HEAD `93803b2` is: **17 inside `crates/relay-clients/`** (5 `flashbots.rs` test mod + 5 `bloxroute.rs` test mod + 2 `tests/{flashbots,bloxroute}.rs` `relay_pointing_at` helpers + 2 `tests/submit_disabled.rs` + 3 `tests/redaction.rs`) **+ 2 in `crates/app/src/lib.rs`** = 19 total. v0.2 corrects §D-D2 explicitly.
(3) **D-D6 contradiction fixed.** v0.1 §D-D6 said "no `crates/relay-clients/tests/flashbots.rs` change. No `crates/relay-clients/tests/bloxroute.rs` change beyond the `relay_pointing_at` helper" — but those `relay_pointing_at` helpers DO need a mechanical signature change for the breaking ctor. v0.2 rewrites §D-D6 to: "test files listed in §"Tests summary" receive mechanical ctor-call signature updates ONLY (helper signature changes / `KillSwitch::new(false)` arg additions); no test assertion changes." The negative invariant `crates/app/src/lib.rs` adds the `build_relay_sim_from_config` reorder + kill_switch threading and nothing else.
(4) **D-T-D7 / D-T-D8 rationale reworded.** v0.1 said the wiremock zero-request tests "prevent a future refactor from sneaking a `tracing::*` ... call between the guard and the return". That's wrong: wiremock only proves **no HTTP I/O is performed under an active kill switch**. The "FIRST non-trivia statement / no pre-guard work" guarantee (including blocking `tracing::*` insertions before the guard) is enforced by the **G10 manual inspection** at Step 8, not by the runtime tests. v0.2 reframes D-T-D7 / D-T-D8 as no-HTTP-I/O regression guards (complementary to G10), not as anti-`tracing` guards.
Awaiting manual Codex re-review.
**Predecessors:**

- Phase 6 overview v0.3 at `c08db38` (pushed). P6-D batch row: per-adapter kill-switch wiring + submission-boundary hard guards.
- P6-A pre-impl plan at `4c4c0dd`; boundary spec at `64ffaee` / `19e263a` / `a7367b7` — FULLY CLOSED. §2.3 + §3 + §G9 + §G10 of `docs/specs/phase-6a-boundary.md` already commit to the exact P6-D shape (cited inline below).
- P6-B pre-impl plan at `9a6ebd2`; impl at `b27d01a`; closeout v3 at `a7367b7` — FULLY CLOSED.
- P6-C pre-impl plan v0.3 at `07a0256`; impl at `93803b2` — FULLY CLOSED.
- HEAD `93803b2`. Workspace baseline **235 passed + 1 ignored**.

## Scope

P6-D wires the existing P5-D `KillSwitch` newtype into each relay adapter constructor in `crates/relay-clients/` and adds the **first non-trivia statement** kill-switch guard to every `BundleRelay::submit_bundle` impl. After P6-D, G10 promotes from "documented; enforces P6-D" → **enforced**: every `submit_bundle` body's first statement is `if self.kill_switch.is_active() { return Err(BundleRelayError::KillSwitchActive); }`, BEFORE the `Err(SubmitDisabled)` short-circuit. Implements the boundary-spec §3 PRECEDENCE rule (`Err(KillSwitchActive)` FIRST) at the per-adapter level. G9's allow-list extends to also include `crates/relay-clients/`.

**Phase 6a invariants explicitly preserved:**

- NO production signer impl; NO `Signer` / `DisabledSigner` / `SignerError` / `SignerDisabled` symbol references in `crates/relay-clients/` (G2c 3-file allow-list unchanged — relay-clients is NOT added).
- NO `eth_sendBundle` runtime path; G1 stays doc-only.
- NO actual relay submission. `submit_bundle` continues to terminate with `Err(SubmitDisabled)` when the kill switch is inactive (existing behavior preserved by the second statement after the new guard). When the kill switch is active, it terminates with `Err(BundleRelayError::KillSwitchActive)`. Neither path performs network I/O.
- NO `submit_bundle` caller introduced in `crates/app/src/`. G3 stays 0; G4 stays 0.
- NO `live_send = true` capability.
- NO live relay tests; all new tests are unit-level on the adapter ctor + the new error path.
- NO `secp256k1` / `k256` / `alloy-signer` / `ethers-signers` / `Wallet` / `PrivateKey` / `sign_transaction` / `funded` symbol additions (G2a stays 0).
- NO `#[ignore]` test additions (G7 stays at P2-C baseline = 1).
- NO new Cargo dep additions; `rust-lmax-mev-bundle-relay = { path = "../bundle-relay" }` already exists in `crates/relay-clients/Cargo.toml:11`, exporting `KillSwitch` via `pub use kill_switch::KillSwitch;` at `crates/bundle-relay/src/lib.rs:19`.
- NO ADR text amendment.
- NO `docs/specs/` text amendment (boundary spec §G9 already says "After P6-D: extended allow-list ALSO includes `crates/relay-clients/`" and §G10 already says "Documented at P6-A; enforces at P6-D close" — both are written as state machines that flip on P6-D close without re-edit).

## Why P6-D is small

The hard work landed at Phase 5 P5-D and is fully spec-documented at P6-A:

- `KillSwitch` newtype (`Arc<AtomicBool>`-backed, `Clone` derives shared state) at `crates/bundle-relay/src/kill_switch.rs:16-37` — re-exported from `crates/bundle-relay/src/lib.rs:19`.
- `BundleRelayError::KillSwitchActive` variant at `crates/bundle-relay/src/lib.rs:97-98`, with `Display` text containing literals `"kill switch active"` AND `"Phase 5 P5-D"` (the KS-3 spec-drift guard).
- `BundleRelay::submit_bundle` trait doc at `crates/bundle-relay/src/lib.rs:117-123` already states the PRECEDENCE rule that P6-D enforces in the impl.
- §3 PRECEDENCE of `docs/specs/phase-6a-boundary.md` already says: "Phase 5 P5-D ships the comparator-driver guard at runtime; Phase 6a P6-D extends to per-adapter `submit_bundle` first-statement."
- §G10 of the boundary spec already lists the exact grep gate: `rg -n --type rust -B 1 -A 3 'fn submit_bundle' crates/relay-clients/src/` + manual inspection that the FIRST non-trivia statement is `if self.kill_switch.is_active() { return Err(BundleRelayError::KillSwitchActive); }`.
- §2.3 of the boundary spec already locks the ctor shape: "Adapters take `KillSwitch` (not `Arc<KillSwitch>`) directly in their ctors per overview Q-P6-F resolution; `KillSwitch` already owns the `Arc<AtomicBool>` internally."

What's missing — and what P6-D ships — is the **adapter-side wiring** and the **two-line guard** in each `submit_bundle` body, plus the per-adapter tests proving the PRECEDENCE rule and the shared-`Arc<AtomicBool>` semantics across `KillSwitch::clone()`.

## Deliverables

### D-D1 — Add `KillSwitch` field to `FlashbotsRelay` + `BloxrouteRelay` struct

- `crates/relay-clients/src/flashbots.rs`: add `kill_switch: KillSwitch` field to `pub struct FlashbotsRelay`. Update the hand-written `Debug` impl to NOT emit the kill-switch state (DP-E11 secret-redaction parity — `is_active()` MAY surface in tracing later, but the `Debug` impl stays elided; field-name presence is acceptable; only the dynamic state is omitted).
- `crates/relay-clients/src/bloxroute.rs`: mirror change to `pub struct BloxrouteRelay`.
- Import: `use rust_lmax_mev_bundle_relay::{BundleRelay, BundleRelayError, KillSwitch, SignedBundle, SubmissionReceipt};` — the `KillSwitch` symbol is added to the existing import list at each file's top.

### D-D2 — Ctor signature change to take `KillSwitch` directly

- `FlashbotsRelay::new(cfg: FlashbotsConfig)` → `FlashbotsRelay::new(cfg: FlashbotsConfig, kill_switch: KillSwitch)`. Returns `Result<Self, RelaySimError>` unchanged.
- `BloxrouteRelay::new(cfg: BloxrouteConfig)` → `BloxrouteRelay::new(cfg: BloxrouteConfig, kill_switch: KillSwitch)`. Same return type.
- Rationale: boundary-spec §2.3 line 54 explicitly says "Adapters take `KillSwitch` ... directly in their ctors". No `Arc<KillSwitch>` — `KillSwitch` is already internally `Arc<AtomicBool>` (DP-D2). No `Option<KillSwitch>` — fail-closed; every adapter MUST be wired to a kill switch.
- Blast radius: **19 ctor call sites total** = **17 inside `crates/relay-clients/`** + **2 inside `crates/app/src/lib.rs`** (mechanical update). Inventory at HEAD `93803b2` per `git grep "FlashbotsRelay::new\|BloxrouteRelay::new" crates/ --include='*.rs'`:
  - `crates/relay-clients/src/flashbots.rs` `#[cfg(test)]` mod — 5 sites (`flashbots_new_default_succeeds`, `flashbots_new_empty_endpoint_rejected`, `flashbots_new_garbage_endpoint_rejected`, `flashbots_debug_elides_url_and_secret`, `rc_f_4_submit_bundle_always_disabled`).
  - `crates/relay-clients/src/bloxroute.rs` `#[cfg(test)]` mod — 5 sites (`bloxroute_new_default_succeeds`, `bloxroute_new_empty_endpoint_rejected`, `bloxroute_debug_elides_url_and_secret`, `bloxroute_missing_api_key_is_not_configured`, `rc_b_4_submit_bundle_always_disabled`).
  - `crates/relay-clients/tests/flashbots.rs` — 1 site (`relay_pointing_at` helper).
  - `crates/relay-clients/tests/bloxroute.rs` — 1 site (`relay_pointing_at` helper).
  - `crates/relay-clients/tests/submit_disabled.rs` — 2 sites (`submit_disabled_1_flashbots`, `submit_disabled_2_bloxroute`).
  - `crates/relay-clients/tests/redaction.rs` — 3 sites (lines 62, 196, 264 per `git grep`).
  - `crates/app/src/lib.rs` — **2 sites**: line 1523 (`FlashbotsRelay::new(FlashbotsConfig { ... })` inside `build_relay_sim_from_config` `RelayKind::Flashbots` arm) and line 1531 (`BloxrouteRelay::new(BloxrouteConfig { ... })` inside the `RelayKind::Bloxroute` arm).
- The 17 sites inside `crates/relay-clients/` pass `KillSwitch::new(false)` (inactive baseline) — preserves existing semantics (`Err(SubmitDisabled)` continues to flow as the second statement). The **4 NEW tests** (D-T-D1..D-T-D4 below, v0.3 lean matrix) use explicit `KillSwitch` instances they construct + flip (D-T-D3 is the only test that calls `set_active(true)`; D-T-D1/D-T-D2 construct with `KillSwitch::new(true)` directly; D-T-D4 constructs with `KillSwitch::new(false)`). The 2 sites inside `crates/app/src/lib.rs` receive the in-scope `kill_switch.clone()` parameter described in §D-D2a immediately below.

### D-D2a — `crates/app/src/lib.rs` `build_relay_sim_from_config` threading (in scope per Codex v0.1 verdict item 1)

- `build_relay_sim_from_config(cfg: &RelayConfig)` → `build_relay_sim_from_config(cfg: &RelayConfig, kill_switch: KillSwitch)`. Returns `Result<Option<Arc<dyn RelaySimulator>>, AppError>` **UNCHANGED**.
- Inside the function, both arms pass `kill_switch.clone()` (or move on the second arm if the match is restructured; current shape uses a single arm at a time, so move is fine) into the adapter ctor: `FlashbotsRelay::new(FlashbotsConfig { ... }, kill_switch)` and `BloxrouteRelay::new(BloxrouteConfig { ... }, kill_switch)`.
- Caller-site reorder: at `crates/app/src/lib.rs:844` the call `build_relay_sim_from_config(&config.relay)?` is invoked BEFORE the kill-switch is constructed at `:890`. v0.2 reorders: construct `let kill_switch = KillSwitch::new(config.relay.execution_disabled);` BEFORE the `build_relay_sim_from_config` call, then invoke `build_relay_sim_from_config(&config.relay, kill_switch.clone())?`. The downstream uses of `kill_switch` at the original `:890` line and later (`AppHandle4::kill_switch()` field, `comparator_driver` per-driver guard) consume the same instance — they all share `Arc<AtomicBool>` via the `KillSwitch::clone()` semantics. No other `crates/app/` code is touched.
- **G3 + G4 preservation analysis (the key Phase 6a invariant Codex v0.1 verdict item 1 required Claude to demonstrate):**
  - G3 (`submit_bundle(` callers in `crates/app/src/`): **stays 0**. The new code in `build_relay_sim_from_config` constructs adapters and returns `Arc<dyn RelaySimulator>`; it does NOT invoke `submit_bundle`. No new `submit_bundle(` call site is added in `crates/app/src/lib.rs`.
  - G4 (`dyn BundleRelay` / `Arc<dyn BundleRelay>` in `crates/app/src/`): **stays 0**. The return type at `crates/app/src/lib.rs:1521` (`let arc: Arc<dyn RelaySimulator>`) is unchanged — the adapter is upcast to `Arc<dyn RelaySimulator>` only (DP-E13 upcast-prevention). The function signature return is `Result<Option<Arc<dyn RelaySimulator>>, AppError>` — unchanged. No `Arc<dyn BundleRelay>` / `dyn BundleRelay` field, parameter, return type, or local binding is introduced anywhere in `crates/app/src/`.
- **Why this preserves the "kill switch only reaches the adapter via the operator-flippable seam" property:** the `KillSwitch` instance held by `AppHandle4` and the `KillSwitch` field inside each adapter are clones of the same underlying `Arc<AtomicBool>`. An operator flipping the switch via `AppHandle4::set_execution_disabled(true)` is observed by `kill_switch.is_active()` inside the adapter's `submit_bundle` first statement on the very next call. **D-T-D3 alone** (v0.3 lean matrix) proves this at the adapter level with a synthetic `KillSwitch::clone()` on the Flashbots side; the property generalizes to bloXroute via the identical guard idiom (verified by G10) + the KS-1/KS-2 baseline `KillSwitch::clone()` semantics tests in `crates/bundle-relay/src/lib.rs` `#[cfg(test)]`. The production wiring at `build_relay_sim_from_config` uses exactly the same `Clone` mechanism.

### D-D3 — `submit_bundle` first-statement kill-switch guard in both adapters

- `crates/relay-clients/src/flashbots.rs` `impl BundleRelay for FlashbotsRelay::submit_bundle`:

  ```text
  async fn submit_bundle(
      &self,
      _bundle: &SignedBundle,
  ) -> Result<SubmissionReceipt, BundleRelayError> {
      // P6-D §3 PRECEDENCE: kill-switch FIRST, before SubmitDisabled.
      if self.kill_switch.is_active() {
          return Err(BundleRelayError::KillSwitchActive);
      }
      Err(BundleRelayError::SubmitDisabled)
  }
  ```

- `crates/relay-clients/src/bloxroute.rs` `impl BundleRelay for BloxrouteRelay::submit_bundle` — identical two-line guard at the top of the body.
- The guard is the FIRST non-trivia statement: zero local-binding lines, zero pattern-matches, zero `let`s precede it. G10 manual-inspection passes.

### D-D4 — `pub use rust_lmax_mev_bundle_relay::KillSwitch` is NOT added at the `crates/relay-clients` root

Per the boundary-spec §G9 allow-list extension ("After P6-D: extended allow-list ALSO includes `crates/relay-clients/`"), the `KillSwitch` symbol may appear in `crates/relay-clients/src/` files. But the `crates/relay-clients/src/lib.rs` does NOT re-export `KillSwitch` — callers import it from `rust_lmax_mev_bundle_relay::KillSwitch` directly (single canonical re-export path; matches the existing `BundleRelay` + `BundleRelayError` + `SignedBundle` + `SubmissionReceipt` import pattern at `crates/relay-clients/src/flashbots.rs:25`). This avoids creating a second re-export path that could fork the type identity.

### D-D5 — Tests (4 new — v0.3 lean matrix per user override)

| ID | File | Test function | Kind | New / modified |
|---|---|---|---|---|
| D-T-D1 | `crates/relay-clients/tests/submit_disabled.rs` | `submit_disabled_3_flashbots_kill_switch_active_takes_precedence` | `#[tokio::test]` | **new** |
| D-T-D2 | `crates/relay-clients/tests/submit_disabled.rs` | `submit_disabled_4_bloxroute_kill_switch_active_takes_precedence` | `#[tokio::test]` | **new** |
| D-T-D3 | `crates/relay-clients/tests/submit_disabled.rs` | `submit_disabled_5_flashbots_shared_kill_switch_flip_visible` | `#[tokio::test]` | **new** |
| D-T-D4 | `crates/relay-clients/src/bloxroute.rs` `#[cfg(test)]` | `bloxroute_kill_switch_inactive_baseline_returns_submit_disabled` | `#[tokio::test]` | **new** |

Test details:

- **D-T-D1 / D-T-D2 (PRECEDENCE proof — BOTH adapters)** — construct adapter with `KillSwitch::new(true)`, call `submit_bundle(&dummy_bundle())`, assert `Err(BundleRelayError::KillSwitchActive)`. Directly proves §3 PRECEDENCE: `KillSwitchActive` FIRST, ahead of `SubmitDisabled`. Per-adapter because the boundary-spec §3 names the rule per-adapter; each adapter holds its own `KillSwitch` field and runs its own guard.
- **D-T-D3 (shared-state proof — Flashbots only)** — construct `let ks = KillSwitch::new(false); let ks_clone = ks.clone();`, pass `ks_clone` to `FlashbotsRelay::new(...)`. **Phase 1**: `submit_bundle` returns `SubmitDisabled` (inactive-baseline path for Flashbots). **Phase 2**: `ks.set_active(true)` on the original outside-the-adapter handle. **Phase 3**: next `submit_bundle` call returns `KillSwitchActive` (shared-state proof). Single asymmetric test on Flashbots; the property generalizes to bloXroute because (a) both adapters use the identical guard pattern (verified by G10 manual inspection) and (b) `KillSwitch::clone()` semantics are owned by `crates/bundle-relay/src/kill_switch.rs` and proven generally by KS-1/KS-2 in `crates/bundle-relay/src/lib.rs` `#[cfg(test)]` (P5-D baseline).
- **D-T-D4 (inactive-baseline regression — bloXroute only)** — `let ks = KillSwitch::new(false);` + `BloxrouteRelay::new(BloxrouteConfig::default(), ks)`, then `submit_bundle(&dummy)` asserts `Err(BundleRelayError::SubmitDisabled)`. Regression check that the new guard does NOT short-circuit when the switch is inactive (proves the second statement of the guarded `submit_bundle` body is still reachable on the asymmetric adapter — Flashbots inactive-baseline is covered by D-T-D3 Phase 1). Co-located with the existing `bloxroute_new_*` unit tests in the `#[cfg(test)]` mod. The Flashbots side's existing `rc_f_4_submit_bundle_always_disabled` test (carry-forward) is updated mechanically with `KillSwitch::new(false)` and continues to enforce the same inactive-baseline regression on Flashbots — no new test needed for that path.

Coverage matrix:

| Property | Flashbots | bloXroute | Where proved |
|---|---|---|---|
| PRECEDENCE (KS=active → `KillSwitchActive`) | YES | YES | D-T-D1 + D-T-D2 |
| Shared-state (`KillSwitch::clone()` across ctor seam, flip-visible) | YES | (generalizes) | D-T-D3 + KS-1/KS-2 baseline |
| Inactive-baseline (KS=inactive → `SubmitDisabled`) | YES (via D-T-D3 Phase 1 + existing `rc_f_4_*`) | YES (D-T-D4) | D-T-D3 + D-T-D4 + carry-forward `rc_f_4_*` / `rc_b_4_*` / `submit_disabled_1/2` |
| FIRST non-trivia statement (no pre-guard work) | YES (G10 inspection) | YES (G10 inspection) | Step 8 manual inspection |
| Bytes-on-the-wire under active KS | (not enforced runtime in Phase 6a) | (not enforced runtime in Phase 6a) | At HEAD `submit_bundle` performs no HTTP at all; reserved for the Phase 6+ PR that adds actual submission code |

No `#[ignore]` additions; no live-network tests; no env-gated paths.

Expected workspace test total at P6-D close: **235 + 4 = 239 passed + 1 ignored**.

### D-D6 — NO new Cargo dep / feature / directory edit; touched-file inventory exhaustive

- No new `[dependencies]` / `[dev-dependencies]` — `rust-lmax-mev-bundle-relay` already in `crates/relay-clients/Cargo.toml:11`. No new `[features]` block.
- No edits to `docs/specs/` or `docs/adr/`. The boundary spec §G9 + §G10 prose already pre-encodes the state-machine flip "on P6-D close" without text edit.
- **Touched-file inventory (exhaustive) — all changes outside this list are forbidden:**

  | File | Change kind | Why touched |
  |---|---|---|
  | `crates/relay-clients/src/flashbots.rs` | substantive | new `kill_switch: KillSwitch` field; ctor sig; first-statement guard; 5 existing `#[cfg(test)]` ctor-call updates (mechanical). No new in-src test (Flashbots inactive-baseline covered by D-T-D3 Phase 1 + carry-forward `rc_f_4_*`). |
  | `crates/relay-clients/src/bloxroute.rs` | substantive | new `kill_switch: KillSwitch` field; ctor sig; first-statement guard; **D-T-D4** (`bloxroute_kill_switch_inactive_baseline_returns_submit_disabled`); 5 existing `#[cfg(test)]` ctor-call updates (mechanical). |
  | `crates/relay-clients/tests/submit_disabled.rs` | substantive | **3 new tests** (D-T-D1 + D-T-D2 + D-T-D3); 2 existing tests get mechanical ctor-call updates (assertion preserved). |
  | `crates/relay-clients/tests/flashbots.rs` | **mechanical only** | the `relay_pointing_at` helper signature is updated (`fn relay_pointing_at(uri: &str, kill_switch: KillSwitch) -> FlashbotsRelay`) and existing test bodies pass `KillSwitch::new(false)`. NO assertion change. NO new test. |
  | `crates/relay-clients/tests/bloxroute.rs` | **mechanical only** | mirror — `relay_pointing_at` helper signature updated; existing test bodies pass `KillSwitch::new(false)`. NO assertion change. NO new test. |
  | `crates/relay-clients/tests/redaction.rs` | **mechanical only** | 3 ctor-call sites updated to pass `KillSwitch::new(false)`. NO assertion change. NO new test. |
  | `crates/app/src/lib.rs` | **scoped reorder + 2 ctor-call updates** per §D-D2a | (a) reorder `let kill_switch = KillSwitch::new(config.relay.execution_disabled);` BEFORE the `build_relay_sim_from_config` call at line ~844 (currently at line 890); (b) update `build_relay_sim_from_config` signature to take `kill_switch: KillSwitch`; (c) update the call site to pass `kill_switch.clone()`; (d) update the 2 inner ctor calls at lines 1523 + 1531 to thread `kill_switch` into the adapter ctor. NO other `crates/app/` change. G3 + G4 preserved (see §D-D2a analysis). |

- **Forbidden outside the table above:**
  - No other `.rs` change in `crates/app/src/` beyond the four scoped edits in `build_relay_sim_from_config` + the kill_switch reorder.
  - No `.rs` change in `crates/execution/`, `crates/signer/`, `crates/bundle-relay/`, `crates/relay-sim/`, `crates/state-fetcher/`, `crates/opportunity/`, `crates/risk/`, `crates/simulator/`, `crates/relay-clients/src/call_bundle.rs`, `crates/relay-clients/src/lib.rs`.
  - No new test file added. v0.3 lean matrix: **3 new integration tests** land in the existing `crates/relay-clients/tests/submit_disabled.rs` (D-T-D1 + D-T-D2 + D-T-D3), and **1 new unit test** lands in the existing `#[cfg(test)]` mod of `crates/relay-clients/src/bloxroute.rs` (D-T-D4). Total: 3 integration + 1 unit = 4 new.

## Tests summary

| ID | Crate / file | Test function | Kind | New / modified |
|---|---|---|---|---|
| D-T-D1 | `crates/relay-clients/tests/submit_disabled.rs` | `submit_disabled_3_flashbots_kill_switch_active_takes_precedence` | `#[tokio::test]` integration | **new** |
| D-T-D2 | `crates/relay-clients/tests/submit_disabled.rs` | `submit_disabled_4_bloxroute_kill_switch_active_takes_precedence` | `#[tokio::test]` integration | **new** |
| D-T-D3 | `crates/relay-clients/tests/submit_disabled.rs` | `submit_disabled_5_flashbots_shared_kill_switch_flip_visible` | `#[tokio::test]` integration | **new** |
| D-T-D4 | `crates/relay-clients/src/bloxroute.rs` (test mod) | `bloxroute_kill_switch_inactive_baseline_returns_submit_disabled` | `#[tokio::test]` unit | **new** |

Existing tests touched only mechanically (ctor signature update; no assertion change):

- `crates/relay-clients/tests/flashbots.rs` `relay_pointing_at` helper.
- `crates/relay-clients/tests/bloxroute.rs` `relay_pointing_at` helper.
- `crates/relay-clients/tests/submit_disabled.rs` `submit_disabled_1_flashbots` + `submit_disabled_2_bloxroute` (existing — call sites updated; assertions unchanged).
- `crates/relay-clients/tests/redaction.rs` (3 mechanical ctor-call edits).
- `crates/relay-clients/src/flashbots.rs` `#[cfg(test)]` 5 existing unit tests (ctor calls updated; assertions unchanged) — `rc_f_4_submit_bundle_always_disabled` continues to enforce the Flashbots inactive-baseline regression (no separate Flashbots D-T-D needed).
- `crates/relay-clients/src/bloxroute.rs` `#[cfg(test)]` 5 existing unit tests (ctor calls updated; assertions unchanged) — `rc_b_4_submit_bundle_always_disabled` continues to enforce the bloXroute inactive-baseline regression on the existing test path, while the new D-T-D4 lands in the same `#[cfg(test)]` mod with an explicit "inactive_baseline" name for spec-drift readability.

Expected workspace test total at P6-D close: **235 + 4 = 239 passed + 1 ignored**.

## Reused (no duplication)

- `KillSwitch` newtype + `Arc<AtomicBool>` semantics — `crates/bundle-relay/src/kill_switch.rs`. P5-D, unchanged.
- `BundleRelayError::KillSwitchActive` + Display literals `"kill switch active"` / `"Phase 5 P5-D"` (KS-3 spec-drift guard) — `crates/bundle-relay/src/lib.rs:97-98`. P5-D, unchanged.
- `BundleRelay::submit_bundle` PRECEDENCE doc — `crates/bundle-relay/src/lib.rs:117-123`. P5-D, unchanged.
- §3 PRECEDENCE + §G9 + §G10 of `docs/specs/phase-6a-boundary.md`. P6-A, unchanged.
- KS-1 + KS-2 baseline `KillSwitch::clone()` semantics tests in `crates/bundle-relay/src/lib.rs` `#[cfg(test)]` (P5-D) — generalize the shared-state property that D-T-D3 proves at the adapter boundary, so the bloXroute side does not need a redundant runtime test.

## Gates at P6-D close (deltas vs P6-C close baseline at `93803b2`)

| Gate | Delta | Notes |
|---|---|---|
| G1 | unchanged | 5 doc-comment `eth_sendBundle` hits; no runtime ref added. |
| G2a (signer-symbol set, POST-D-B0 form) | **unchanged at 0** | no new signer symbols. |
| G2b | unchanged (0) | no signer-dep symbols in Cargo.toml. |
| G2c | **unchanged 3-file allow-list** | `crates/relay-clients/` is NOT added to G2c — no `Signer` / `DisabledSigner` / `SignerError` / `SignerDisabled` symbol use in the new code. |
| G2d | unchanged | zero hits outside the 3-file allow-list. |
| G2e | unchanged (2) | `rust-lmax-mev-signer` dep edge set unchanged at 2 (`crates/execution` + `crates/app`); `crates/relay-clients/Cargo.toml` does NOT gain a signer dep. |
| G3 | **unchanged at 0** | no new `submit_bundle(` callers in `crates/app/src/`. |
| G4 | **unchanged at 0** | no new `dyn BundleRelay` / `Arc<dyn BundleRelay>` in `crates/app/src/`. |
| G5 | unchanged | no `live_send` mutation. |
| G6 | unchanged | no new `api_key` log emission; no new tracing of secrets. |
| G7 | unchanged | no new `#[ignore]` tests. |
| G8 | unchanged | no new workspace dep cycles. |
| G9 | **allow-list EXTENDED** | per boundary-spec §G9 prose ("After P6-D: extended allow-list ALSO includes `crates/relay-clients/`"). New hits live only in the following **categories** (no count claim — exact `KillSwitch`-symbol hit count is closeout-owned and verified at P6-D close, not in this plan): (a) one `KillSwitch` field on each adapter struct; (b) the `kill_switch: KillSwitch` ctor parameter on each adapter ctor; (c) the `self.kill_switch.is_active()` guard inside each `submit_bundle` body; (d) the `use ...::KillSwitch` import at the top of each adapter `.rs`; (e) the new D-T-D1..D-T-D4 test bodies (v0.3 lean matrix); (f) the mechanical `KillSwitch::new(false)` arg added to each carry-forward ctor call site enumerated in the §D-D2 inventory. All hits must land under the post-P6-D allow-list (`crates/bundle-relay/`, `crates/app/`, `crates/relay-clients/`); zero hits outside the allow-list. |
| G10 | **DOCUMENTED → ENFORCED** | each `impl BundleRelay for ... { fn submit_bundle }` body's FIRST non-trivia statement matches `if self.kill_switch.is_active() { return Err(BundleRelayError::KillSwitchActive); }`. Manual inspection per the spec §G10 grep. |
| G11 | unchanged (1 site) | single `sign_tx` production call site at `crates/execution/src/lib.rs:238`; P6-D does not touch `crates/execution/`. |

Workspace tests at P6-D close: **239 passed + 1 ignored** (v0.3 lean matrix).

## Forbidden in P6-D

- Any `.rs` change in `crates/execution/`, `crates/signer/`, `crates/relay-sim/`, `crates/bundle-relay/`, `crates/relay-clients/src/call_bundle.rs`, `crates/relay-clients/src/lib.rs`. The kill-switch wiring is confined to the two adapter implementation files (`crates/relay-clients/src/flashbots.rs` + `crates/relay-clients/src/bloxroute.rs`) plus the test files listed in §"Tests summary".
- Any `.rs` change in `crates/app/src/` beyond the four scoped edits inside `build_relay_sim_from_config` + the `let kill_switch = ...` reorder enumerated in §D-D2a. The §D-D6 touched-file table is exhaustive; any further `crates/app/` edit is forbidden.
- Any `submit_bundle` caller in `crates/app/src/` (G3 stays 0). The new tests live in `crates/relay-clients/tests/` or `crates/relay-clients/src/*#[cfg(test)]`, not in `crates/app/`.
- Any `Arc<dyn BundleRelay>` or `dyn BundleRelay` construction in `crates/app/src/` (G4 stays 0).
- Any `Arc<KillSwitch>` field/parameter/local. The boundary-spec §2.3 explicitly forbids the double-Arc; `KillSwitch` already owns the `Arc<AtomicBool>` internally.
- Any `Option<KillSwitch>` field/parameter — fail-closed; every adapter MUST receive an explicit `KillSwitch`. (Construction with `KillSwitch::new(false)` is acceptable; absence is not.)
- Any `Default` impl for `KillSwitch` (carry from P5-D DP-D2: explicit ctor only).
- Any signer-symbol use (`Signer`, `DisabledSigner`, `SignerError`, `SignerDisabled`) in P6-D code or tests (G2c 3-file allow-list unchanged — relay-clients not added).
- Any `secp256k1` / `k256` / `alloy-signer` / `ethers-signers` / `Wallet` / `PrivateKey` / `sign_transaction` / `funded` symbol additions (G2a stays 0).
- Any private key material, signed-tx bytes derived from key material, or hex-encoded signed-tx fixture. (P6-D does not exercise the txs path; `submit_bundle` short-circuits BEFORE looking at the bundle's `signed_txs` field.)
- Any new `BundleRelayError` variant (P6-D uses the existing `KillSwitchActive` variant verbatim).
- Any change to the `BundleRelayError::KillSwitchActive` `Display` text (KS-3 spec-drift guard).
- Any change to the `BundleRelay::submit_bundle` trait signature.
- Any new Cargo dep / dev-dep / feature flag.
- Any `live_send = true` capability.
- Any `eth_sendBundle` reference anywhere.
- Any live-network test code, env-gated or otherwise.
- Any new `#[ignore]` test.
- Any ADR amendment.
- Any edit to `docs/specs/execution-safety.md` or `docs/specs/phase-6a-boundary.md`. (The boundary spec already pre-encodes the P6-D close flip in §G9 + §G10 prose.)
- Any `.claude/` / `AGENTS.md` / `fixture_output.txt` / `hook_toast.md` staging.
- Any destructive git or force-push.
- Any asset / V3-fee-tier / venue widening.

## Plan execution checklist (TDD-style)

- [ ] **Step 1: Confirm predecessor state.** `git log --oneline -5` shows `93803b2` HEAD; workspace 235 passed + 1 ignored; G1..G11 all green at P6-C close.
- [ ] **Step 2: Red — write D-T-D1 first.** Add `submit_disabled_3_flashbots_kill_switch_active_takes_precedence` to `crates/relay-clients/tests/submit_disabled.rs`. Test will fail to compile because `FlashbotsRelay::new(cfg, kill_switch)` takes only one arg at HEAD `93803b2`. Confirm red.
- [ ] **Step 3: Green — D-T-D1.** (a) Add `KillSwitch` field + import to `FlashbotsRelay` struct. (b) Update `FlashbotsRelay::new` signature to take `kill_switch: KillSwitch`. (c) Add the two-line guard at the top of `FlashbotsRelay::submit_bundle`. (d) Mechanically update every existing `FlashbotsRelay::new(cfg)` callsite inside `crates/relay-clients/` to `FlashbotsRelay::new(cfg, KillSwitch::new(false))` (callsites enumerated in §D-D2). (e) `cargo test -p rust-lmax-mev-relay-clients --test submit_disabled` — D-T-D1 green; existing tests still green.
- [ ] **Step 3a: `crates/app/src/lib.rs` threading (§D-D2a).** (a) Update `build_relay_sim_from_config` signature to take `kill_switch: KillSwitch`. (b) Move `let kill_switch = KillSwitch::new(config.relay.execution_disabled);` from line ~890 to BEFORE the `build_relay_sim_from_config` call at line ~844. (c) Update the call site at `:844` to pass `kill_switch.clone()`. (d) Inside `build_relay_sim_from_config`, pass `kill_switch` into `FlashbotsRelay::new` (line 1523) and `BloxrouteRelay::new` (line 1531) — `kill_switch.clone()` on the first arm if both arms need an instance after the move, otherwise move once. (e) Confirm `cargo build -p rust-lmax-mev-app` compiles; confirm `cargo test -p rust-lmax-mev-app` still green (existing app-side tests preserved). G3 + G4 verification at Step 10.
- [ ] **Step 4: Red → green — D-T-D2.** Mirror for `BloxrouteRelay` (struct field + ctor sig + guard + callsite updates + D-T-D2 in `submit_disabled.rs`).
- [ ] **Step 5: Red → green — D-T-D3 (shared-state, Flashbots only).** Add the clone + flip + observe test on `FlashbotsRelay`.
- [ ] **Step 6: Red → green — D-T-D4 (inactive-baseline, bloXroute only).** Add the test inside `crates/relay-clients/src/bloxroute.rs` `#[cfg(test)]` mod.
- [ ] **Step 7 (v0.3): Deleted.** v0.2 D-T-D5/D-T-D6 (per-adapter inactive-baseline duplicates) consolidated — Flashbots inactive-baseline is covered by D-T-D3 Phase 1 + carry-forward `rc_f_4_*`; bloXroute inactive-baseline is covered by D-T-D4 + carry-forward `rc_b_4_*`. v0.2 D-T-D7/D-T-D8 (no-HTTP-I/O) DROPPED per user override + §Status item (B). G10 is the static authority for "no pre-guard work".
- [ ] **Step 8: G10 manual inspection.** Run `rg -n --type rust -B 1 -A 5 'fn submit_bundle' crates/relay-clients/src/`; verify both adapter `submit_bundle` bodies' first non-trivia statement is `if self.kill_switch.is_active() { return Err(BundleRelayError::KillSwitchActive); }`. No comments / `let` bindings / `tracing::*` calls precede.
- [ ] **Step 9: G9 inspection.** Run `rg -n --type rust 'KillSwitch' crates/`; verify new hits land only under `crates/bundle-relay/`, `crates/app/`, AND `crates/relay-clients/` (the post-P6-D extended allow-list per boundary-spec §G9). No hits under `crates/execution/`, `crates/signer/`, `crates/relay-sim/`, `crates/state-fetcher/`, etc.
- [ ] **Step 10: Full gate set.** Workspace `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (expect **239 passed + 1 ignored**), `cargo deny check`, `cargo tree -d`, and all G1..G11 ripgrep gates from §"Gates at P6-D close".
- [ ] **Step 11: Commit + push.** Single routine `feat(p6-d)` commit. Suggested message: `feat(p6-d): per-adapter kill-switch wiring (G10 enforcement) on Flashbots + bloXroute adapters`.
- [ ] **Step 12: Update `.coordination/claude_outbox.md`** with the P6-D closeout report; emit P6-E pre-impl plan draft.

## Risks + open questions

- **Q-D1 — Ctor signature: breaking `::new(cfg, kill_switch)` vs additive `with_kill_switch(cfg, ks)` keeping `::new(cfg)` as default-inactive shim.** v0.2 **recommends BREAKING** (`::new(cfg, kill_switch)`): matches the boundary-spec §2.3 wording literally ("Adapters take `KillSwitch` ... directly in their ctors") and is the only option that actually wires the operator-flippable `AppHandle4::kill_switch()` to production adapters — additive `with_kill_switch` keeps `crates/app/src/lib.rs::build_relay_sim_from_config` on the default-inactive `::new(cfg)` path, disconnecting production adapters from the operator surface and defeating the purpose of P6-D. Blast radius is **19 ctor call sites total** = 17 inside `crates/relay-clients/` + 2 inside `crates/app/src/lib.rs`; the latter is covered in scope per §D-D2a with G3 + G4 preservation explicitly analyzed. Codex verdict?
- **Q-D2 — Should existing `submit_disabled_1_flashbots` / `submit_disabled_2_bloxroute` be split into two assertions (KS-inactive → `SubmitDisabled` AND KS-active → `KillSwitchActive`)?** v0.1 → v0.3 LOCKED: **NO** — keep `submit_disabled_1/2` semantics narrow (the original P4-E invariant they encode is "regardless of input, ctor-default returns `SubmitDisabled`"). v0.3 adds `submit_disabled_3..5` as the new PRECEDENCE + shared-state assertions (3 new tests in that file); the bloXroute inactive-baseline goes to the `crates/relay-clients/src/bloxroute.rs` `#[cfg(test)]` mod as D-T-D4 instead of a fourth `submit_disabled_*` test, keeping the integration file focused on cross-adapter PRECEDENCE + shared-state. No no-IO assertions in v0.3 (D-T-D7/D-T-D8 dropped per §Q-D5). This preserves the original tests' historical meaning and prevents test-name semantic drift. Codex verdict?
- **Q-D3 — Should a `crates/app/`-side integration test be added that constructs adapters via `AppHandle4::kill_switch()` and proves the operator can flip the switch from outside?** v0.1 → v0.3 LOCKED: **NO**. Such a test would require either (a) a `submit_bundle` caller in `crates/app/src/` (breaks G3) or (b) an `Arc<dyn BundleRelay>` field in `AppHandle4` (breaks G4). Both are explicitly forbidden in Phase 6a. The shared-state property is proven by **D-T-D3 alone** at the adapter level (Flashbots clone + flip + observe), generalized to bloXroute via the identical guard idiom + KS-1/KS-2 baseline `KillSwitch::clone()` semantics. The `AppHandle4` ↔ adapter wiring lands in Phase 6b when production submission unlocks. Codex verdict?
- **Q-D4 — Should the `Debug` impl emit `kill_switch: <active|inactive>`?** v0.1 recommends **NO**. The `Debug` impl is the DP-E11 secret-redaction surface and should remain conservative. Operators read `kill_switch.is_active()` directly via the `AppHandle4` surface, not via adapter `Debug`. Adding the state to `Debug` would invite tracing of it, which has no current consumer. Codex verdict?
- **Q-D5 — Test matrix size: 8 (v0.2) vs lean 4 (v0.3) vs other shape?** **v0.3 RECOMMENDS LEAN 4** per user override:
  1. `submit_disabled_3_flashbots_kill_switch_active_takes_precedence` — Flashbots PRECEDENCE proof.
  2. `submit_disabled_4_bloxroute_kill_switch_active_takes_precedence` — bloXroute PRECEDENCE proof.
  3. `submit_disabled_5_flashbots_shared_kill_switch_flip_visible` — Flashbots shared-state (also covers Flashbots inactive-baseline Phase 1).
  4. `bloxroute_kill_switch_inactive_baseline_returns_submit_disabled` — bloXroute inactive-baseline (in `#[cfg(test)]` mod).

  **Dropped** vs v0.2:
  - D-T-D7 + D-T-D8 (no-HTTP-I/O wiremock zero-request) — at HEAD `submit_bundle` performs no HTTP I/O at all, so these tests add runtime cost without distinguishing "guard ran first" from "guard never ran". G10 manual inspection at Step 8 is the authoritative static enforcement of "FIRST non-trivia statement / no pre-guard work". The no-HTTP-I/O regression guard belongs to the future Phase 6+ PR that introduces real HTTP-issuing code to `submit_bundle`, scoped to that PR's new behavior.
  - D-T-D4 v0.2 (bloXroute shared-state) — redundant with D-T-D3 Flashbots shared-state given (a) identical guard idiom across both adapters (G10) and (b) `KillSwitch::clone()` semantics already proven by KS-1/KS-2 in `crates/bundle-relay/src/lib.rs` `#[cfg(test)]`.
  - D-T-D5 v0.2 (Flashbots inactive-baseline standalone) — Phase 1 of D-T-D3 already covers it; plus carry-forward `rc_f_4_*` continues to enforce on the existing path.

  **G10 manual inspection at Step 8 remains the main enforcement** for the "first non-trivia statement / no pre-guard work" rule. Workspace test total at P6-D close: **243 → 239 passed + 1 ignored**. Codex verdict?

## Process

Per the 2026-05-04 routine-closeout policy + the overview §Process:

1. Claude writes this pre-impl plan to disk (UNCOMMITTED) + emits the review pack to `.coordination/claude_outbox.md`.
2. Claude STOPS and reports "manual Codex pre-impl review required". **No `.rs` / `Cargo.toml` / ADR / `docs/specs/` edits in this turn.**
3. User pastes pack to Codex; relays verdict.
4. Claude records verdict in `.coordination/codex_review.md`.
5. **APPROVED** → commit + push this plan as a routine doc commit; THEN execute per §"Plan execution checklist"; THEN commit + push the impl; THEN draft P6-E pre-impl plan.
6. **REVISION REQUIRED** → revise plan in place + re-emit pack.
7. **Scope / ADR change required** → HALT to user.
