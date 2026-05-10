# Phase 5 Batch D — kill switch wiring

**Date:** 2026-05-10 KST
**Status:** Draft v0.1. Awaiting manual Codex review.
**Predecessor:** P5-C closed + pushed at `db6a7b8` (Codex APPROVED HIGH 2026-05-10 KST). Phase 5 overview at `ac07024`. P5-A closed at `0c28d5c`. P5-B closed at `84283d9`. P5-C plan at `0bd21f9`.

## Scope

Wire the kill switch per `docs/specs/execution-safety.md` §"Kill Switch" + Phase 5 overview §"P5-D — kill switch wiring" + Q-P5-5 standing answer (BOTH per-driver AND per-adapter). Per overview hard-forbids:

- Promote `relay.execution_disabled: bool` from per-config-read static value to a process-wide `KillSwitch` (transparent `Arc<AtomicBool>` newtype) constructed once by `wire_phase4` from the config initial value.
- NEW `KillSwitch` type in `crates/bundle-relay` (lowest-existing crate that owns the relay-submission concept). Methods: `new(initial: bool)`, `is_active() -> bool`, `set_active(disabled: bool)`. Derives `Debug + Clone` (Arc clone — both clones see the same atomic). NO `Default::default() == off` shortcut accepted; default is `KillSwitch::new(false)` only via explicit ctor (R-equivalent: ctor-only construction prevents accidental "off by default" assumptions in tests).
- NEW `BundleRelayError::KillSwitchActive` variant (`#[non_exhaustive]` already). `Display` text MUST contain literal `"kill switch active"` + literal `"Phase 5 P5-D"` (KS-3 BR-3-style spec-drift guard).
- `BundleRelay::submit_bundle` trait doc updated to spec the precedence: when a future wiring (Phase 6) threads in a kill switch, the `submit_bundle` impl MUST return `Err(KillSwitchActive)` BEFORE `Err(SubmitDisabled)` if kill switch is on. No adapter-level field added in Phase 5 (Phase 6 owns that wiring); existing Flashbots/bloXroute `submit_bundle` impls stay byte-identical (still `SubmitDisabled` unconditionally — no kill_switch field on them).
- `AppHandle4` exposes the kill switch:
  - NEW `kill_switch: KillSwitch` field.
  - NEW `pub fn kill_switch(&self) -> &KillSwitch` accessor.
  - NEW `pub fn set_execution_disabled(&self, disabled: bool)` operator-toggle method (delegates to `KillSwitch::set_active`).
  - `wire_phase4` constructs `KillSwitch::new(config.relay.execution_disabled)` once and stashes a clone in `AppHandle4`.
- `comparator_driver` (per-driver guard — Q-P5-5):
  - NEW parameter: `kill_switch: KillSwitch` threaded by `wire_phase4`.
  - At the top of the recv-loop body: if `kill_switch.is_active()`, log at WARN + skip the iteration (no comparator sim call, no comparator_tx broadcast, no journal append, no mismatch_tx broadcast). Per overview "(a) the `comparator_driver` would-be submission point (still SubmitDisabled at trait level — the kill switch is the second layer)" — Phase 5 has no actual submit, so the kill switch check is at the iteration entry as the would-be-submission gate.
  - When `kill_switch` is NOT active (default), comparator_driver behavior is byte-identical to P5-C.
- NO submission path is enabled. NO `live_send` is permitted. NO actual relay submission code is added or wired. NO `eth_sendBundle`. NO live network. NO Phase 6 work. NO ADR amendment.

## Decision points

- **DP-D1 (kill switch home crate)**: `KillSwitch` lives in `crates/bundle-relay` — the lowest-existing crate that owns the relay-submission concept and is already a dep of `crates/app` (for comparator_driver) and `crates/relay-clients` (for adapters). Avoids creating a new crate just for one type. (Q-D1 open: Codex may prefer a separate `crates/kill-switch` crate for symmetry with P5-C; plan default is `crates/bundle-relay`.)

- **DP-D2 (`KillSwitch` shape)**: transparent newtype over `Arc<AtomicBool>` with explicit ctor only.
  ```rust
  use std::sync::Arc;
  use std::sync::atomic::{AtomicBool, Ordering};

  #[derive(Debug, Clone)]
  pub struct KillSwitch(Arc<AtomicBool>);

  impl KillSwitch {
      pub fn new(initial_disabled: bool) -> Self {
          Self(Arc::new(AtomicBool::new(initial_disabled)))
      }
      pub fn is_active(&self) -> bool {
          // "active" == "execution disabled" == AtomicBool true
          self.0.load(Ordering::Acquire)
      }
      pub fn set_active(&self, disabled: bool) {
          self.0.store(disabled, Ordering::Release);
      }
  }
  ```
  No `Default` derive (avoids accidental "off by default" assumptions). `Arc::clone` semantics: every `KillSwitch::clone()` shares the underlying `AtomicBool`, so a flip from any clone is visible from every other clone. KS-2 asserts this.

- **DP-D3 (`BundleRelayError::KillSwitchActive` variant)**: added as a sibling to `SubmitDisabled`. `Display` text pinned to contain literal `"kill switch active"` + `"Phase 5 P5-D"` (KS-3 BR-3-style spec-drift guard, mirroring the existing `SubmitDisabled` "Phase 5 Safety Gate" guard).
  ```rust
  #[non_exhaustive]
  #[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
  pub enum BundleRelayError {
      #[error("submit_bundle disabled in this build (Phase 5 Safety Gate required)")]
      SubmitDisabled,
      // NEW (P5-D):
      #[error("kill switch active — Phase 5 P5-D execution disabled")]
      KillSwitchActive,
  }
  ```
  (`PartialEq + Eq` carry over from existing P4-E derives — both variants are payload-free.)

- **DP-D4 (`BundleRelay::submit_bundle` doc-only spec amendment)**: doc comment on `submit_bundle` updated to record the precedence rule for Phase 6 wiring: "when a kill switch is threaded into the implementation (Phase 6), the impl MUST check `KillSwitch::is_active()` and return `Err(BundleRelayError::KillSwitchActive)` BEFORE returning `Err(SubmitDisabled)`. Phase 5 P5-D adds the type but does NOT add adapter-level kill-switch fields; existing Flashbots/bloXroute impls remain byte-identical." No code change to existing adapters; existing `submit_bundle` impls + `Err(SubmitDisabled)` invariant preserved.

- **DP-D5 (`AppHandle4` kill switch surface)**: lean — single field + two methods:
  - field `kill_switch: KillSwitch` (Arc-shared).
  - `pub fn kill_switch(&self) -> &KillSwitch` accessor.
  - `pub fn set_execution_disabled(&self, disabled: bool) { self.kill_switch.set_active(disabled) }`.
  No `is_execution_disabled` helper (callers use `handle.kill_switch().is_active()` to keep one source of truth).

- **DP-D6 (`wire_phase4` construction + threading)**: in `wire_phase4`:
  ```rust
  let kill_switch = KillSwitch::new(config.relay.execution_disabled);
  // ... existing setup ...
  let comparator_driver_task = tokio::spawn(comparator_driver(
      // ... existing args ...
      kill_switch.clone(),  // NEW per-driver guard
  ));
  // ... AppHandle4 { ..., kill_switch, comparator_driver_task, ... } ...
  ```
  Single ctor call; cloned into the comparator_driver task; cloned into `AppHandle4`. The Arc inside is shared.

- **DP-D7 (`comparator_driver` per-driver guard)**: NEW parameter at the END of the existing parameter list (additive; preserves call-site shape modulo one new arg):
  ```rust
  pub async fn comparator_driver<J: MismatchJournalSink>(
      // ... existing params ...
      kill_switch: KillSwitch,  // NEW
  ) {
      while let Ok(env) = sim_rx.recv().await {
          if kill_switch.is_active() {
              tracing::warn!(
                  target: "kill_switch",
                  "comparator_driver: kill switch active, skipping iteration"
              );
              continue;
          }
          // ... existing comparator body unchanged ...
      }
  }
  ```
  `tracing::warn!` only logs `kill_switch=active`, NOT any `env` field (no event-content leakage). When kill switch is OFF (default), the iteration body is byte-identical to P5-C.

- **DP-D8 (no per-adapter wiring in P5-D)**: per overview Q-P5-5 "BOTH per-driver AND per-adapter", the per-adapter integration is **deferred to Phase 6** because adapter constructors do not currently take a kill switch and adding it would require a config + ctor surface change in `crates/relay-clients` that touches no Phase 5 functional gate. P5-D ships the **trait-doc spec** (DP-D4) + the **error variant** (DP-D3) so Phase 6 can wire adapters without a new pre-impl gate. Codex Q-D2 may push for adapter wiring in P5-D; plan default keeps adapters untouched to preserve P5-A/B/C-D blast-radius minimization.

- **DP-D9 (config validation NOT changed)**: `RelayConfig.execution_disabled` already exists and parses correctly (P4-E DP-E10). P5-D does not alter the field, the default, or the validation. No new config-validation reject is introduced. (`live_send=true` rejection remains.)

- **DP-D10 (no `tokio` runtime impact)**: `KillSwitch::set_active` is sync, lock-free, async-runtime-agnostic (`AtomicBool::store(Ordering::Release)`). No new tokio task. No new channel.

## Non-goals (Phase 5 hard forbids reaffirmed + P5-D-specific)

- No production signer / no funded key / no key material.
- No actual `eth_sendBundle` / no real relay submission / no `live_send=true` flip.
- No adapter-level kill switch field (deferred to Phase 6 per DP-D8).
- No semantic change to existing `submit_bundle` impls in `crates/relay-clients` (Flashbots/bloXroute stay `Err(SubmitDisabled)` unconditionally).
- No semantic change to `comparator_driver` when kill switch is OFF (default) — KS-equivalent regression guard via existing CW-1/CW-2 tests.
- No new live network surface; no paid live API; no live-network test enabled by default.
- No ADR text amendment.
- No widening of asset / venue / V3-fee-tier scope.
- No new crate in P5-D (DP-D1 places the type in existing `crates/bundle-relay`).
- No `Default` derive on `KillSwitch` (DP-D2 — explicit ctor only).
- No new public method on `BundleRelay` trait beyond the doc amendment (DP-D4).
- No `.claude/` / `AGENTS.md` / `fixture_output.txt` staging. No destructive git. No force-push.

## Test matrix (lean — 5 required tests)

| ID | Crate | What it asserts | Why (mapping) |
|---|---|---|---|
| **KS-1** | `bundle-relay` | `KillSwitch::new(false).is_active() == false`; `KillSwitch::new(true).is_active() == true`. | DP-D2 ctor + initial state. |
| **KS-2** | `bundle-relay` | `set_active(true)` flips; `set_active(false)` restores. Across `Clone`: a flip via clone1 is observable via clone2 (Arc semantics). | DP-D2 toggle + Arc shared-state. |
| **KS-3** | `bundle-relay` | `format!("{}", BundleRelayError::KillSwitchActive)` contains literals `"kill switch active"` AND `"Phase 5 P5-D"`. | DP-D3 spec-drift guard. |
| **KS-4** | `app` | `wire_phase4(config, opts)` produces a handle whose `kill_switch().is_active() == config.relay.execution_disabled`. After `handle.set_execution_disabled(true)`, `handle.kill_switch().is_active() == true`. | DP-D5 + DP-D6 ctor + toggle. |
| **KS-5** | `app` | `comparator_driver` with kill_switch active suppresses comparator_tx + mismatch_tx + journal append for an inbound sim event; with kill_switch off (default), behavior is byte-identical to P5-C (one comparator broadcast or one mismatch broadcast per event). | DP-D7 per-driver guard + DP-D6 wiring. |

Total NEW tests: **5**. Workspace target: **226 → 231 passed + 1 ignored**.

## Implementation steps

1. `crates/bundle-relay/src/lib.rs`: add `pub mod kill_switch;` (or inline) — define `KillSwitch` per DP-D2.
2. `crates/bundle-relay/src/lib.rs`: add `BundleRelayError::KillSwitchActive` variant per DP-D3.
3. `crates/bundle-relay/src/lib.rs`: update `BundleRelay::submit_bundle` doc per DP-D4 (doc only; no signature change).
4. `crates/bundle-relay/src/lib.rs` `#[cfg(test)]`: add KS-1, KS-2, KS-3.
5. `crates/app/src/lib.rs`: add `KillSwitch` field to `AppHandle4`; add `kill_switch()` accessor + `set_execution_disabled()`; update `wire_phase4` per DP-D6; update `comparator_driver` signature + body per DP-D7.
6. `crates/app/tests/wire_phase4_*.rs` (existing test file): add KS-4 + KS-5.
7. Run **batch-close gates**:
   - `cargo fmt --check`
   - `cargo build --workspace --all-targets`
   - `cargo test --workspace` (expect **231 passed + 1 ignored**)
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo deny check`
   - `cargo tree -p rust-lmax-mev-app` (cycle gate)
   - **DP-C7 four-grep gate** (carry-forward from P5-C): G2a + G2b + G2d each zero hits; G2c inventory under `crates/signer/` only.
   - **NEW G9 gate**: `rg -n -w 'KillSwitch' crates/ --type rust` — every hit MUST be in `crates/bundle-relay/` or `crates/app/`. (Hard gate.)
8. **No `wire_phase3` / `wire_phase2` / `wire` change** — P5-D only touches `wire_phase4`.

## Risks + mitigations

| Risk | Mitigation |
|---|---|
| Threading a NEW arg into `comparator_driver` ripples into existing test files. | KS-4 + KS-5 use the same test file pattern as existing CW tests. Existing `wire_phase4`-based tests get the new field via additive `AppHandle4` — they consume `handle.exec_subscribe()` etc. without touching the new field. |
| `KillSwitch::is_active() == true` accidentally suppresses comparator's observability path even when no submission is wired. | Documented as intentional in DP-D7 — kill switch is the "would-be submission point gate". When operator flips kill switch on, ALL would-be submission sites stop. Comparator's mismatch detection is part of that chain (would-feed-submission). |
| `Ordering::Release` / `Ordering::Acquire` mismatch causes stale reads. | DP-D2 uses `store=Release` / `load=Acquire` pair — standard for "publish a flag, observe the latest". |
| KS-5 brittle if comparator_tx broadcast receives nothing legitimately (e.g., test races). | KS-5 uses the same `recv_timeout(2s)` pattern as existing CW-1/CW-2 tests. Both branches (kill on / kill off) tested in the same test or split into KS-5a/KS-5b. |

## Codex Q-D standing questions (open — please answer in the v0.1 review)

- **Q-D1**: `KillSwitch` home crate — `crates/bundle-relay` (plan default; minimizes new crates) vs new `crates/kill-switch` (Phase 5 batch-symmetry with P5-C)?
- **Q-D2**: Per-adapter kill switch wiring in P5-D vs deferred to Phase 6 (plan default: deferred; rationale in DP-D8)?
- **Q-D3**: `comparator_driver` kill-switch behavior when active — silently skip the iteration with WARN log (plan default; matches "would-be submission point gate" framing) vs journal a "comparator suppressed by kill switch" record?
- **Q-D4**: `KillSwitch` method names — `is_active()` / `set_active()` (plan default; "active" == "execution disabled") vs `is_execution_disabled()` / `disable()` / `enable()` vs `load()` / `store()` (raw)?
- **Q-D5**: `KillSwitchActive` Display phrase — `"Phase 5 P5-D"` is the spec-drift guard; alternative (e.g., `"Phase 5 Safety Gate kill switch"`) acceptable?
- **Q-D6**: NEW G9 grep gate (`KillSwitch` symbol confined to `crates/bundle-relay/` + `crates/app/`) — required at P5-D close, or inventory-only?

## Process

Per the 2026-05-04 routine-closeout policy + 2026-05-04 22:20 KST manual-Codex-review-mode + the user's autonomous-Phase-5-execution authorization (P5-D plan stays UNCOMMITTED on disk until Codex APPROVED):

1. Claude writes this plan to disk (UNCOMMITTED) + emits the review pack to `.coordination/claude_outbox.md`.
2. Claude STOPS and reports "manual Codex review required for P5-D v0.1".
3. User pastes pack to Codex; relays verdict.
4. Claude records verdict in `.coordination/codex_review.md`.
5. **APPROVED** → commit + push this plan as a routine doc commit; THEN implement P5-D per the test matrix; THEN batch-close gates; THEN commit + push; THEN proceed to P5-E final-wiring + DoD audit + tag automatically per the autonomous-execution authorization.
6. **REVISION REQUIRED** → revise this plan in place + re-emit pack.
7. **Scope/ADR change required** → HALT to user (any item from Phase 5 hard-forbids list, any ADR text change, any signer/submission/`live_send` capability addition, any actual submission wiring).

No code, no `Cargo.toml` edits, no commit, no push, no tag in this turn.
