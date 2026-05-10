# Phase 5 Batch E — final wiring + Phase 5 DoD audit + `phase-5-complete` tag

**Date:** 2026-05-10 KST
**Status:** Final batch close. Per Phase 5 overview §"Provisional batch breakdown" line for P5-E ("NO (audit/tag-only per Phase 4 P4-G precedent)"), no pre-impl Codex review required.
**Predecessor:** P5-D closed + pushed at `e3e11bb` (full gates + four-grep + G9 leak gate + 231 passed + 1 ignored).

## Scope

P5-E is the Phase 5 closeout. Per the Phase 5 overview table:

> P5-E | Final wiring + Phase 5 DoD audit + `phase-5-complete` tag. Audit P5-A..D completion. Run all carry-forward Phase 4 safety grep gates + new Phase 5 invariants. DoD audit doc. Tag draft. | NO

P5-A..D shipped all functional Phase 5 work in their respective batches:

- **P5-A** wired production `LocalSimulator::prefetch_for` in shadow mode (archive-RPC-gated, fail-closed when archive unconfigured/stale; per-block fixture cache; interior-mutability redesign; `simulator_driver` integration).
- **P5-B** added `BidStrategy` trait + `FixedFractionBidStrategy` + `Eip1559BasefeeAwareBidStrategy` + `BundleConstructor::with_strategy(...)` explicit-strategy ctor.
- **P5-C** introduced the boundary-only `crates/signer` workspace member with `Signer` trait + `DisabledSigner` fail-closed impl + `SignerError::SignerDisabled` whose `Display` pins `"Phase 6b Production Gate"`. Promoted Phase 4 G2 zero-hit `Signer` grep to four-gate G2a/G2b/G2c/G2d redefined gate per R-P5-2.
- **P5-D** wired the kill switch: `KillSwitch` (`Arc<AtomicBool>` newtype) in `crates/bundle-relay`, `BundleRelayError::KillSwitchActive` variant, `AppHandle4::{kill_switch(), set_execution_disabled(bool)}` surface, `comparator_driver` per-driver guard. Per-adapter wiring deferred to Phase 6 per Q-P5-5 / DP-D8.

P5-E's responsibilities reduce to:

1. Audit the remaining wiring boundary.
2. Document the **Phase 6 Production Gate deferred items** that Phase 5 does NOT ship (signer impl, real signing, `eth_sendBundle`, actual relay submission, `live_send=true`, per-adapter kill-switch wiring).
3. Run all Phase 4 carry-forward safety grep gates + new Phase 5 invariants (G2a..G2d redefined per R-P5-2; G9 added per P5-D Q-D6).
4. Document the Phase 5 DoD per A..E.
5. Create + push the `phase-5-complete` annotated tag.

## Final wiring decision

### Per-adapter kill switch threading → DEFERRED to Phase 6

Per overview Q-P5-5 standing answer ("kill switch placement: BOTH per-driver AND per-adapter") + P5-D §DP-D8, the per-driver guard landed in P5-D (`comparator_driver` ↑) and the **per-adapter trait-doc spec** is in place (`BundleRelay::submit_bundle` PRECEDENCE rule: when wired, MUST return `Err(KillSwitchActive)` BEFORE `Err(SubmitDisabled)`). The actual adapter-level wiring (Flashbots / bloXroute constructors taking `KillSwitch`, `submit_bundle` impls calling `kill_switch.is_active()` first) is Phase 6 work because:

1. Phase 5 has zero `submit_bundle` call sites — the trait surface is present but no caller exists in `crates/app/src/` (G3 zero-hit gate). Threading a kill switch into adapter constructors changes the constructor surface (breaking change to relay-clients) without exercising any code path in Phase 5.
2. Phase 6 adapter wiring happens alongside the production signer integration (which gives `submit_bundle` a real call site) — cleaner to extend the constructor surface once.
3. The trait-doc PRECEDENCE rule makes the Phase 6 wiring contract explicit: any future `submit_bundle` impl MUST consult the kill switch before any other return.

**Fail-closed boundary verified**: `wire_phase4` at `crates/app/src/lib.rs:865` constructs `KillSwitch::new(config.relay.execution_disabled)` once and threads it into `comparator_driver`. `AppHandle4::set_execution_disabled(bool)` toggles the underlying `AtomicBool`; the `comparator_driver` recv-loop top-of-iteration check at `crates/app/src/lib.rs:1342` short-circuits with `tracing::warn!` + `continue`, suppressing relay sim, comparator broadcast, journal append, and mismatch broadcast in one go.

### No new wiring constructor

Phase 5 honors the P4-E §DP-E2 choice to extend `wire_phase4` IN PLACE rather than introduce `wire_phase5_pre_safety` or `wire_phase5_safety`. P5-A added the `prefetch_for` call site, P5-B + P5-D extended `BundleConstructor` + `comparator_driver` signatures, all reachable through the same `wire_phase4` surface.

### Production signer integration → DEFERRED to Phase 6b Production Gate

`crates/signer` ships `Signer` + `DisabledSigner` + `SignerError::SignerDisabled`. There is **NO** production signer impl, **NO** key material, **NO** key derivation, **NO** funded key, **NO** `secp256k1`/`k256`/`alloy-signer`/`ethers-signers`/`Wallet`/`PrivateKey` symbol anywhere in the workspace (verified by G2a/G2b/G2d zero-hit gates). `crates/app` does NOT depend on `crates/signer`; the signer is reachable only by direct unit tests in `crates/signer/`. Phase 6b Production Gate is the only path to enabling real signing, real submission, `eth_sendBundle`, and `live_send=true` per `docs/specs/execution-safety.md`.

## Phase 5 DoD audit

### Batch-level closure

| Batch | HEAD | Status | Tests delta | Notes |
|---|---|---|---|---|
| Phase 5 overview | `ac07024` | DOC pushed | — | v0.3 APPROVED HIGH |
| P5-A `prefetch_for` shadow-mode wiring | `0c28d5c` | CLOSED + pushed | 206 → 213 (+7) | Archive-RPC-gated; fail-closed when archive unconfigured |
| P5-B dynamic gas bidding strategy | `84283d9` | CLOSED + pushed | 213 → 221 (+8) | `BidStrategy` trait + 2 impls; `BundleConstructor::with_strategy` |
| P5-C signer boundary + fail-closed stub | `db6a7b8` | CLOSED + pushed | 221 → 226 (+5) | NEW `crates/signer`; G2a..G2d redefined gate |
| P5-D kill switch wiring | `e3e11bb` | CLOSED + pushed | 226 → 231 (+5) | `KillSwitch` + `comparator_driver` per-driver guard; G9 gate |
| **P5-E final wiring + DoD audit + tag** | (this commit) | THIS BATCH | unchanged | Audit + tag only |

Total Phase 5 test delta: 206 → 231 (+25). Workspace currently **231 passed + 1 ignored** (the ignored test is the pre-existing P2-C `g_state_live` env-contract live-smoke stub; carry-forward).

### Hard safety invariants verified

- **HSI-1**: NO production signer impl. `crates/signer` ships only `DisabledSigner` whose `sign_tx` returns `Err(SignerError::SignerDisabled)` unconditionally. SC-1 / SC-2 / SC-4 verify; SC-3 pins the `"Phase 6b Production Gate"` Display literal.
- **HSI-2**: NO funded key / NO key material in repo / tests / fixtures / configs / env examples. G2a (zero hits on `Wallet|PrivateKey|secp256k1|k256|sign_transaction|funded`) verified at this commit.
- **HSI-3**: NO `eth_sendBundle` runtime call path. G1 hits all in doc/comment lines explicitly asserting "NO `eth_sendBundle`". No code reference.
- **HSI-4**: NO `live_send=true` capability. `crates/config::ConfigError::LiveSendForbidden` rejects `live_send=true` at config load time; CFG-LIVE-SEND-1 test verifies. G5 hits all in config validation, struct definition, error variant, and doc/comment lines.
- **HSI-5**: NO actual relay submission. G3 (`submit_bundle(` call site in `crates/app/src/`) zero hits; G4 (`dyn BundleRelay` / `Arc<dyn BundleRelay>` in `crates/app/src/`) zero hits. The four-layer submission lock from P4-E remains: (a) `submit_bundle` returns `Err(SubmitDisabled)` unconditionally in every adapter; (b) `crates/app` holds adapters as `Arc<dyn RelaySimulator>` only (DP-E13 type-system upcast prevention); (c) no caller exists; (d) P5-D `KillSwitch` adds the runtime kill switch as a fifth layer.
- **HSI-6**: NO new live-network surface in P5. P5-A's archive-RPC integration is gated on `config.simulator.prefetch_enabled` (default `false`) AND `config.node.archive_rpc.is_some()` AND fail-closed on archive errors.
- **HSI-7**: NO paid live API in CI. G7 confirms one pre-existing `#[ignore]`'d live-smoke test (`crates/replay/tests/g_state_live.rs`, P2-C carry-forward); not enabled by default; no live API key in CI.
- **HSI-8**: NO ADR text amendment. ADR-006 §"Gas bidding" Phase 5+ unlock recorded in P5-B execution note only per Q-P5-7 / R-P5-3 standing.
- **HSI-9**: NO asset / venue / V3-fee-tier widening. WETH/USDC + UniV2 + UniV3 0.05% + Sushi V2 scope held throughout Phase 5.
- **HSI-10**: NO destructive git / NO force-push / NO `.claude/` or `AGENTS.md` or `fixture_output.txt` staging. Verified across all P5 commits.
- **HSI-11**: NO Phase 6 Production Gate work. Per-adapter kill switch wiring + production signer + `live_send=true` capability + actual `eth_sendBundle` deferred.

### Grep gate audit (this commit)

| Gate | Description | Result |
|---|---|---|
| G1 | `eth_sendBundle` runtime call path | 5 hits, all `//!` doc comments asserting "NO `eth_sendBundle`" — no runtime call |
| G2a | Forbidden `.rs` symbols (`Wallet\|PrivateKey\|secp256k1\|k256\|sign_transaction\|funded`) | **0 hits** |
| G2b | Forbidden `Cargo.toml` deps (`alloy-signer\|ethers-signers\|secp256k1\|k256`) | **0 hits** |
| G2c | Allowed-Signer-symbol inventory (`Signer\|DisabledSigner\|SignerError\|SignerDisabled`) | All 5 hits under `crates/signer/` |
| G2d | Allowed-Signer-symbol leak outside `crates/signer/` | **0 hits** |
| G3 | `submit_bundle(` call site in `crates/app/src/` | **0 hits** |
| G4 | `dyn BundleRelay` / `Arc<dyn BundleRelay>` in `crates/app/src/` | **0 hits** |
| G5 | `live_send` runtime enabling | All hits in config validation / struct definition / error variant / docs — no runtime enabling |
| G6 | R-E20 secret-redaction residue | `api_key` only in field-access positions; never logged |
| G7 | Live-network tests / new `#[ignore]`'d network adapter tests | Pre-existing P2-C `g_state_live` ignored test only; no new in P5 |
| G8 | Crate cycle gate (`cargo tree -d`) | Duplicate-version edges only; **no cycles** |
| G9 | `KillSwitch` leak outside `crates/{bundle-relay, app}/` | **0 hits** |

### Test gate audit (this commit)

- `cargo fmt --check` — clean
- `cargo build --workspace --all-targets` — clean
- `cargo test --workspace` — **231 passed + 1 ignored**
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo deny check` — advisories ok, bans ok, licenses ok, sources ok
- `cargo tree -d` — no cycles

## Process

1. Commit this DoD audit doc as the P5-E close.
2. Push to origin.
3. Create annotated tag `phase-5-complete` at the P5-E commit.
4. Push the tag.
5. Report close per the user-supplied closeout template.

Per the 2026-05-04 routine-closeout policy + the user-supplied P5-E directive ("If all gates and safety invariants pass, commit + push the P5-E closeout doc. Create and push annotated tag `phase-5-complete`."), tag creation + push proceeds without further user re-confirmation.

## Honest scope-leak guard

P5-E **does NOT** introduce:

- Phase 6 Production Gate work.
- Funded key / production signer / production submission.
- `live_send=true` capability.
- Per-adapter kill switch wiring (deferred to Phase 6 per DP-D8).
- Real signing infrastructure.
- `eth_sendBundle` call path.
- New asset pairs (WETH/USDC only).
- New V3 fee tiers (0.05% only).
- Any code path that could reach `submit_bundle`.

P5-E **DOES** add only:

- This DoD audit doc.
- The `phase-5-complete` annotated tag.

Phase 6 work — including production signer integration, real `eth_callBundle` with signed transactions, per-adapter kill switch wiring, and (Phase 6b only) live submission with `eth_sendBundle` + `live_send=true` — is NOT started in P5-E and remains gated by the Phase 6 Pre-Production Gate (6a) and Phase 6b Production Gate per `docs/specs/execution-safety.md`.
