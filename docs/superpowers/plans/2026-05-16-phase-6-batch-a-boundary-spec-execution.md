# Phase 6 Batch A — Phase 6a safety boundary refinement (doc-only)

**Date:** 2026-05-16 KST
**Status:** Draft v0.3 (revised after manual Codex REVISION REQUIRED HIGH on v0.2 — single item: stale "G2c stays 0" wording in §"Forbidden in P6-A" was inconsistent with the v0.2 G2c restoration to signer-symbol inventory. Fixed to: "G2a / G2b / G2e stay 0; G2c remains unchanged from the Phase 5 baseline inventory under `crates/signer/`; G2d allow-list remains the Phase 5 baseline set; G3 / G4 stay 0." v0.1 → v0.2 fixes retained verbatim.) v0.2 changelog below (revised after manual Codex REVISION REQUIRED HIGH on v0.1, 2026-05-16 KST). Four fixes: (1) G2c restored to **Signer-symbol inventory** allow-list per P5-E DoD definition (the v0.1 incorrectly redefined G2c as a `Cargo.toml` dep-edge gate); separate **G2e** added for the new `signer = { path = "../signer" }` dep-edge check. (2) All ripgrep patterns rewritten to use **unescaped `|` regex alternation** or **`-e` forms**; no `\|` BRE-style escapes locked into the boundary doc. (3) Every gate row now carries an **exact `rg` command with explicit path scope** (`crates/` for code-side; `crates/**/Cargo.toml` for Cargo-side; the boundary doc lives under `docs/specs/` and therefore does not affect any crate-scoped gate). (4) **Q-A1 resolved**: NO `execution-safety.md` edit in P6-A. The reverse-link can be added in a later batch when the boundary spec is settled; P6-A keeps the single-deliverable scope clean. Awaiting manual Codex re-review.
**Predecessor:** Phase 6 overview v0.3 APPROVED HIGH committed at `c08db38` (pushed to `origin/master`).

## Scope

P6-A is a **pure doc batch**. Single deliverable: NEW `docs/specs/phase-6a-boundary.md`. NO `.rs` edit, NO `Cargo.toml` edit, NO ADR amendment, NO dependency change.

The boundary doc captures the explicit contract between the three Phase 6a safety devices (`submit_bundle`, `KillSwitch`, `Signer`) and locks the grep-gate set that P6-B..F must satisfy. Subsequent batches MUST reference this doc.

## Why P6-A first

- All subsequent batches (P6-B..E) depend on the boundary contract being explicit and reviewed.
- P6-A produces zero code change, so it is the lowest-risk first step.
- The boundary doc gives Codex a single reviewable artifact to verdict the safety shape **before** any `.rs` edit lands in P6-B/C/D.
- Q-P6-A standing recommendation: P6-A first (overview §"Recommended first implementation batch").

## Deliverable

NEW `docs/specs/phase-6a-boundary.md` with the following sections (proposed; Codex may revise during pre-impl review):

### §1 Purpose + scope

- States that the doc captures the Phase 6a fail-closed contract.
- Cross-links `docs/specs/execution-safety.md`.
- Explicitly out-of-scope: Phase 6b Production Gate (separate doc; not authored in 6a).

### §2 Three safety boundaries

- **§2.1 Submission boundary** — `submit_bundle` trait method on every `BundleRelay` impl. Phase 5 invariant carries forward: returns `Err(SubmitDisabled)` unconditionally (every adapter); zero call sites in `crates/app/src/` (G3); adapters held as `Arc<dyn RelaySimulator>` only in `crates/app` (G4 / DP-E13 upcast prevention).
- **§2.2 Signer boundary** — `crates/signer::Signer` trait. Phase 6a wires the FIRST `Signer`-using site at `crates/execution::BundleConstructor::with_signer` (primary boundary) and the secondary ctor-injection site at `crates/app::wire_phase4` (`Arc<dyn Signer>` parameter). Only impl reachable is `crates/signer::DisabledSigner`; every `sign_tx` call returns `Err(SignerError::SignerDisabled)`.
- **§2.3 Kill-switch boundary** — `crates/bundle-relay::KillSwitch` (`pub struct KillSwitch(Arc<AtomicBool>)` with `#[derive(Clone)]`). Per-driver guard at `comparator_driver` (P5-D, already live). Per-adapter guards added in P6-D (Flashbots + bloXroute `submit_bundle` first statement).

### §3 `Result::Err` PRECEDENCE rule

Lock the ordering for every code path that could reach a submission-equivalent operation:

1. **`Err(KillSwitchActive)` FIRST** — if the kill switch is active, return this error before any other branch evaluates. Operator can flip the switch at runtime via `AppHandle4::set_execution_disabled(true)`; the guard must short-circuit on the NEXT iteration / call without waiting for any in-flight RPC.
2. **`Err(SignerError::SignerDisabled)` SECOND** — if the signer is `DisabledSigner` (Phase 6a default), signing fails before any relay-sim or submit work runs. In Phase 6a this happens on every signing call.
3. **`Err(SubmitDisabled)` LAST** — every adapter's `submit_bundle` returns this if neither of the above short-circuited. Phase 6a never reaches this branch via runtime path because (a) `crates/app` has no `submit_bundle` callers and (b) the signer always errors out earlier.

Phase 6b unlock: §3 explicitly notes that Phase 6b is the only context where any of these `Err` returns may become `Ok`, and that Phase 6b will need its own boundary update.

### §4 Grep-gate set (G1..G11; G2c restored to symbol inventory; G2e new for dep edge)

Carry forward G1..G9 verbatim from the P5-E DoD audit at `55679a4`. Add **G2e** (new — `signer` dep edge), **G10**, **G11**. Restore G2c to **Signer-symbol inventory** (P5-E DoD definition; v0.1 of this plan incorrectly redefined it).

Every command below uses **unescaped regex alternation `|`** (ripgrep default regex) or explicit `-e <pattern>` repetitions. **No BRE-style `\|` escapes** are locked into the spec. Every command **explicitly scopes** the path so the new `docs/specs/phase-6a-boundary.md` (which lives under `docs/specs/` and naturally references safety terms in its prose) cannot affect any crate-scoped gate.

| Gate | Command | Expected | Reason |
|---|---|---|---|
| G1 | `rg -n --type rust 'eth_sendBundle' crates/` | Only `//!` / `///` doc-comment lines asserting NO `eth_sendBundle`; zero non-doc hits. Boundary doc under `docs/specs/` is OUT OF SCOPE for this gate. | Phase 5 carry. |
| G2a | `rg -n --type rust -e 'Wallet' -e 'PrivateKey' -e 'secp256k1' -e 'k256' -e 'sign_transaction' -e 'funded' crates/` | 0 hits | Phase 5 carry. |
| G2b | `rg -n --glob 'crates/**/Cargo.toml' -e 'alloy-signer' -e 'ethers-signers' -e 'secp256k1' -e 'k256'` | 0 hits | Phase 5 carry. Forbidden dep set. |
| G2c (restored to P5-E definition) | `rg -n --type rust -e 'Signer' -e 'DisabledSigner' -e 'SignerError' -e 'SignerDisabled' crates/` | **Inventory** of allowed Signer-symbol sites. At Phase 5 baseline `55679a4`: all hits under `crates/signer/`. After P6-B: hits under `crates/signer/` PLUS approved P6-B file:line pairs in `crates/execution/` and `crates/app/`. P6-A close: unchanged from Phase 5 baseline (no `.rs` change). | Phase 5 carry. Per overview Q-P6-F: allow-list expanded by two sites at P6-B. |
| G2d (redefined as positive allow-list per Codex v0.2 item 3) | Same command as G2c. | Every hit MUST appear in the explicit allow-list of approved file:line pairs (Phase 5: only `crates/signer/...`; post-P6-B: also approved sites in `crates/execution/` + `crates/app/`). **Zero hits outside the allow-list.** Removing or relocating an approved site without updating the allow-list is a gate failure. P6-A close: unchanged from Phase 5 baseline. | Positive allow-list gate. |
| G2e (new) | `rg -n --glob 'crates/**/Cargo.toml' 'signer = \{ path = "../signer" \}'` | After P6-B: exactly 2 hits, in `crates/execution/Cargo.toml` AND `crates/app/Cargo.toml`. P6-A close: 0 hits (no `Cargo.toml` change at P6-A). | New dep-edge gate per Codex v0.1 item 1 + overview Q-P6-F + Q-P6-H. |
| G3 | `rg -n --type rust 'submit_bundle\(' crates/app/src/` | 0 hits | Phase 5 carry. No caller in app. |
| G4 | `rg -n --type rust -e 'dyn BundleRelay' -e 'Arc<dyn BundleRelay>' crates/app/src/` | 0 hits | Phase 5 carry. |
| G5 | `rg -n --type rust 'live_send' crates/` | All hits in config validation / struct definition / error variant / doc comments — no runtime enabling site. | Phase 5 carry. |
| G6 | `rg -n --type rust 'api_key' crates/` | Only field-access positions in adapter ctors / error rendering with redaction; never inside a `tracing::*!` log emission. | Phase 5 R-E20 carry. |
| G7 | `rg -n --type rust '#\[ignore\]' crates/` | Pre-existing P2-C `g_state_live` only (`crates/replay/tests/g_state_live.rs`); no new in 6a. | Phase 5 carry. |
| G8 | `cargo tree -d` | No cycles (duplicate-version edges allowed). | Phase 5 carry. |
| G9 | `rg -n --type rust 'KillSwitch' crates/` | At Phase 5 baseline + P6-A close: hits only under `crates/bundle-relay/` and `crates/app/`. After P6-D: extended allow-list ALSO includes `crates/relay-clients/` (Flashbots + bloXroute adapter use). The boundary doc enumerates the per-batch allow-list. | Phase 5 carry, extended in P6-D. |
| G10 (new; **enforces at P6-D close**, documented at P6-A) | `rg -n --type rust -B 1 -A 3 'fn submit_bundle' crates/relay-clients/src/` plus manual inspection that the FIRST non-trivia statement of each `impl BundleRelay for ... { fn submit_bundle }` body contains the literal `kill_switch.is_active()`. | Every adapter's `submit_bundle` body's first statement: `if self.kill_switch.is_active() { return Err(BundleRelayError::KillSwitchActive); }`. | Per-adapter kill-switch PRECEDENCE per §3 + overview Q-P6-F. |
| G11 (new; **enforces at P6-B close**, documented at P6-A) | `rg -n --type rust 'sign_tx' crates/execution/src/` plus manual inspection that the only call site routes through `&dyn Signer` and is reached only inside the `BundleConstructor::with_signer` signing-request hook; integration test asserts `DisabledSigner` returns `Err(SignerError::SignerDisabled)` before any downstream work. | Single approved call site; no bypass. | Signer-routing fail-closed at `BundleConstructor::with_signer` boundary per overview Q-P6-F. |

The above commands are written verbatim into `docs/specs/phase-6a-boundary.md` §4 so the P6-F DoD audit can copy-paste each row. **Path scope is explicit on every gate** to keep the boundary doc (under `docs/specs/`) and the planning notes (under `docs/superpowers/`) out of the gate result set; if a future audit broadens scope, the boundary doc itself must be added to that audit's expected-hits accounting.

### §5 Phase 6a hard forbids

Verbatim from overview §"Hard forbids during all of Phase 6a":

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

### §6 Phase 6b boundary (kept explicit)

Verbatim from overview §"Phase 6b Production Gate (kept entirely separate)". Phase 6b owns: production signer impl, funded key wiring, `live_send=true` flip, `eth_sendBundle` runtime path, actual relay submission. None touchable in Phase 6a. Phase 6b requires its own overview doc + fresh explicit user authorization + separate Codex review.

### §7 Cross-references

- `docs/adr/ADR-001.md` (mempool ingestion + Phase 6 gate)
- `docs/specs/execution-safety.md` (parent safety policy — `submit_bundle` ban, `live_send` default, funded-key ban, gas-bidding policy, kill switch)
- `docs/superpowers/plans/2026-05-16-phase-6-overview-execution.md` (Phase 6 overview v0.3 APPROVED)

**Q-A1 RESOLVED v0.2**: NO `execution-safety.md` edit in P6-A. The reverse-link can be added in a later batch (most naturally P6-E when the production-signer design doc cross-references both) when the boundary spec is settled. P6-A's single deliverable is the new `phase-6a-boundary.md` file ONLY; `execution-safety.md` is read-only at P6-A.

## Forbidden in P6-A

- Any `.rs` file edit.
- Any `Cargo.toml` edit.
- Any ADR amendment (code-shape grep gates unchanged at P6-A close because no code changed: G2a / G2b / G2e stay 0; G2c remains unchanged from the Phase 5 baseline inventory under `crates/signer/`; G2d allow-list remains the Phase 5 baseline set; G3 / G4 stay 0).
- Any new dependency.
- Any test addition (doc-only batch).
- Any edit to `docs/specs/execution-safety.md`, including a reverse-link cross-reference (Q-A1 resolved v0.2 to NO edit). `execution-safety.md` is read-only at P6-A.

## Tests

None. Doc-only batch. Mechanical CI gates still run on the doc commit:

- `cargo fmt --check` — unchanged.
- `cargo build --workspace --all-targets` — unchanged.
- `cargo test --workspace` — unchanged (**231 passed + 1 ignored** baseline preserved).
- `cargo clippy --workspace --all-targets -- -D warnings` — unchanged.
- `cargo deny check` — unchanged.
- `cargo tree -d` — unchanged (no cycles).

The DoD evidence at P6-A close is the doc itself + an unchanged `cargo test --workspace` summary line.

## Gates at P6-A close

P6-A changes only files under `docs/`; **every G1..G9 + G2e command in §4 is path-scoped to `crates/` or `crates/**/Cargo.toml`**, so the new `docs/specs/phase-6a-boundary.md` (which prose-references `eth_sendBundle`, `live_send`, signer terms, etc.) is **outside the scan path** for every crate-scoped gate and cannot affect their hit counts.

- G1..G9: re-run with the exact commands in §4. Expected: identical hit counts to the `55679a4` baseline. The boundary doc under `docs/specs/` is OUT OF SCOPE on every path-restricted command.
- G2e (new): 0 hits at P6-A close (no `Cargo.toml` change). Begins enforcing at P6-B with the expected 2-site hit count.
- G10 and G11: **documented** in the new boundary doc but **not yet enforced** at P6-A close — their target code (P6-D per-adapter kill-switch threading; P6-B `BundleConstructor::with_signer` signing-request hook) doesn't exist yet. They begin enforcing at their respective batch closes; P6-F is the first checkpoint where ALL gates run together.
- `cargo test --workspace`: re-run; expected **231 passed + 1 ignored** (P5-E baseline carried forward).

If any G1..G9 hit count differs from the `55679a4` baseline after P6-A doc commit, treat as evidence the path-scope is wrong and HALT — do not amend the gate to absorb the doc.

## Plan execution checklist (TDD-style, but doc-only)

- [ ] **Step 1: Confirm the overview doc is committed + pushed at `c08db38`.** Verify with `git log --oneline -3`.
- [ ] **Step 2: Re-read `docs/specs/execution-safety.md` + P5-E DoD audit doc (`docs/superpowers/plans/2026-05-10-phase-5-batch-e-final-wiring-execution.md`)** to make sure the carry-forward gate set is captured verbatim.
- [ ] **Step 3: Write `docs/specs/phase-6a-boundary.md`** per the §1..§7 outline above. Inline the exact ripgrep patterns + expected hit counts for G1..G11.
- [ ] **Step 4: Verify the doc renders correctly** (no broken markdown headings; cross-link paths exist on disk).
- [ ] **Step 5: Re-run the carry-forward gates locally to confirm the doc commit doesn't change any value.** Expected: `cargo test --workspace` ⇒ 231 passed + 1 ignored; G1..G9 values unchanged from `55679a4`.
- [ ] **Step 6: Commit + push as a routine doc commit** with message `docs(p6-a): Phase 6a safety boundary spec (docs/specs/phase-6a-boundary.md)`.
- [ ] **Step 7: Update `.coordination/codex_review.md` + `.coordination/claude_outbox.md` with the P6-A closeout report** (gates unchanged; doc on disk + committed + pushed).

## Open questions

- **Q-A1 — RESOLVED v0.2**: NO `execution-safety.md` edit in P6-A. Reverse-link deferred to a later batch when the boundary spec is settled (most naturally P6-E). Closed.
- **Q-A2 — Exact filename: `phase-6a-boundary.md` vs `phase-6-safety-boundary.md` vs `phase-6a-safety-boundary.md`?** Recommend `phase-6a-boundary.md` (matches overview §P6-A wording). Codex verdict?
- **Q-A3 — Does the boundary doc itself need a versioning header (v0.1, v0.2, …) like the overview, or is it a stable spec that doesn't version?** Recommend stable spec, no version header (matches `execution-safety.md` style). Codex verdict?
- **Q-A4 — Should §4 inline the exact ripgrep commands** (e.g., `rg --no-heading -tn rust 'pattern' crates/`) or stay at the pattern + expected-count level? Recommend full commands inline so the P6-F DoD audit copy-pastes verbatim. Codex verdict?
- **Q-A5 — Should §3 PRECEDENCE rule have a stricter wording about atomicity** (e.g., the kill-switch read uses `Ordering::Acquire`, the set uses `Ordering::Release`, which matches `crates/bundle-relay/src/kill_switch.rs`)? Recommend YES — document the ordering choice in §3 so future readers know it is intentional. Codex verdict?

## Process

Per the 2026-05-04 routine-closeout policy + the overview §Process:

1. Claude writes this pre-impl plan to disk (UNCOMMITTED) + emits the review pack to `.coordination/claude_outbox.md`.
2. Claude STOPS and reports "manual Codex review required".
3. User pastes pack to Codex; relays verdict.
4. Claude records verdict in `.coordination/codex_review.md`.
5. **APPROVED** → commit + push this plan as a routine doc commit; THEN execute the plan (write the boundary doc) per §"Plan execution checklist"; THEN commit + push the boundary doc; THEN draft P6-B pre-impl plan.
6. **REVISION REQUIRED** → revise plan in place + re-emit pack.
7. **Scope / ADR change required** → HALT to user.

No code / `Cargo.toml` / ADR edits in this turn. No commit of this plan yet. Plan stays uncommitted until Codex APPROVED.
