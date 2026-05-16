# Phase 6 Batch E -- Production signer design review (doc-only)

**Date:** 2026-05-16 KST
**Status:** Draft v0.3 (revised after manual Codex REVISION REQUIRED HIGH on v0.2 for mojibake, 2026-05-16 KST). One v0.2 -> v0.3 fix:
(0) **ASCII-only normalization.** v0.2 used UTF-8 characters (`Section ` for U+00A7 SECTION SIGN, `->` for U+2192 RIGHTWARDS ARROW, `<=` for U+2264 LESS-THAN-OR-EQUAL-TO, `--` for U+2014 EM DASH) which Codex's reading pipeline rendered as mojibake (`??`, `??50`, `??20`, Korean syllables under a CP949 codepage). v0.3 normalizes the entire plan file to **ASCII-only** with the equivalents above, plus `=>` for any U+21D2 and `<->` for any U+2194. Mirror normalization applied in this outbox. Substantive content v0.2 -> v0.3 unchanged: lean-matrix replacement Step 5 (Codex v0.1 item 1), Section 2.2 necessary-but-not-sufficient framing (Codex v0.1 item 2), Section 2.3 positive auditability requirement (Codex v0.1 item 3), Section 2.5 host-compromise residual + Phase 6b control point (Codex v0.1 item 4), Q-E1 length cap `<=150` lines. Length-cap value is unambiguous as `<=150` throughout.

**v0.1 -> v0.2 changelog retained verbatim below for traceability:** Four v0.1 -> v0.2 fixes:
(1) **Step 5 lightweight gate set reduced.** v0.1 still ran `cargo test --workspace` at P6-E close. P6-E is **strictly doc-only** -- `docs/specs/` is outside the `cargo` build graph, so running the workspace test suite at P6-E close provides no new safety signal and conflates P6-E gating with P6-F audit-time gating. v0.2 reduces Step 5 to the **minimum checks** appropriate for a doc-only batch: `git status` + `git diff` review, `wc -l docs/specs/production-signer.md` (<=150 cap check per v0.2 Q-E1), targeted `rg` text checks against the new doc for forbidden Rust-identifier code-fences. The workspace test baseline is **inherited from P6-D close** (`d88693b` = 239 passed + 1 ignored) without re-running. Full `cargo fmt --check` / `cargo clippy` / `cargo test --workspace` / `cargo deny check` / G1..G11 ripgrep gates land at **P6-F** as the formal phase-6a-complete audit step.
(2) **Section 2.2 reworded.** v0.1 Section 2.2 claimed `BundleTx -> Result<SignedTxBytes, SignerError>` "enforces at the type level" the never-in-memory invariant. Codex correctly flagged: the trait shape only keeps key material out of the **caller-facing API**; it does not enforce HSM/KMS-mediated custody inside the eventual impl. v0.2 Section 2.2 reframes accurately: the trait shape is a **necessary but not sufficient** condition; the actual enforcement is (a) HSM/KMS custody from Section 2.1 + (b) Phase 6b implementation review against the Section 2 contract before the impl can replace `DisabledSigner`.
(3) **Section 2.3 reworded.** v0.1 Section 2.3 said "every `sign_tx` invocation in Phase 6b production code MUST be deducible from existing structured-event journals ... without introducing a new 'key request' journal". Codex correctly flagged: this over-forbids -- a non-secret signing audit event (an event that records the existing opportunity/bundle linkage, contains no key material) is a legitimate Phase 6b design choice. v0.2 Section 2.3 reframes as a **positive requirement**: any signing-related audit event/journal MUST (a) link the signing operation to the existing opportunity/bundle chain (i.e., be correlatable to the upstream `OpportunityEvent` / `MismatchAbort` IDs), and (b) contain no key material / no key fingerprint / no derivable secret. The exact event shape is locked in Phase 6b; this spec only locks the auditability + secret-redaction invariants.
(4) **Section 2.5 threat model expanded.** v0.1 Section 2.5 named raw key extraction as the threat REJECTED by Section 2.1+Section 2.2. Codex correctly flagged: HSM/KMS prevents *raw key extraction* but does **NOT** by itself prevent a compromised host from **requesting malicious signatures** (the host still controls what `BundleTx` is sent to the HSM/KMS for signing; the HSM/KMS will sign whatever it receives if request authorization checks pass). v0.2 Section 2.5 names this explicitly as a **residual risk** + a **Phase 6b control point**: Phase 6b must add request-authorization / per-bundle pre-sign attestation / signing-rate limits / pre-sign mismatch-comparator gating as the host-compromise mitigation. The threat is named; the controls live in Phase 6b.
Awaiting manual Codex re-review.
**Predecessors:**

- Phase 6 overview v0.3 at `c08db38` (pushed). P6-E batch row: "Production signer design review, disabled by default. Document the Phase 6b unlock contract (HSM/KMS-only, never-in-memory key material, audit log shape, key rotation, lifecycle, threat model). NO impl. NO `secp256k1`/`k256`/`alloy-signer`/`ethers-signers`/`Wallet`/`PrivateKey` symbols added. `DisabledSigner` remains the only impl. Document MUST land at `docs/specs/production-signer.md` and be referenced from `docs/specs/execution-safety.md`."
- P6-A pre-impl plan + boundary spec -- FULLY CLOSED.
- P6-B / P6-C / P6-D -- FULLY CLOSED.
- HEAD `d88693b`. Workspace baseline **239 passed + 1 ignored**.

## Scope

P6-E is a **docs-only batch** authoring a new spec document `docs/specs/production-signer.md` that captures the Phase 6b unlock contract for production signing. It is **design review only**; no Rust code, no Cargo dep, no feature flag, no ADR amendment, no signer impl, no key material. The document explicitly forward-references the Phase 6b Production Gate as the only path to enabling any of the work it describes. `DisabledSigner` remains the **only** `Signer` impl in the workspace at P6-E close.

**Phase 6a invariants explicitly preserved:**

- NO production signer impl (G2a/G2b stay at 0; G2c/G2d 3-file allow-list stays as-is; G2e stays at 2).
- NO funded key / NO private key material / NO key derivation / NO `secp256k1`/`k256`/`alloy-signer`/`ethers-signers`/`Wallet`/`PrivateKey`/`sign_transaction`/`funded` symbol added anywhere in `crates/`.
- NO `eth_sendBundle` runtime path; G1 stays doc-only (the new spec doc lives under `docs/specs/`, which is OUTSIDE the G1 `crates/` path scope; `eth_sendBundle` appearing in the new doc's prose is permitted just as it is in `execution-safety.md` and `phase-6a-boundary.md`).
- NO actual relay submission. G3 + G4 stay at 0 in `crates/app/src/`.
- NO `live_send = true` capability.
- NO live relay tests; NO env-gated test paths.
- NO `#[ignore]` test additions.
- NO new Cargo dep / dev-dep / feature flag.
- NO `.rs` change anywhere in the workspace.
- NO `docs/adr/` text amendment.

## What P6-E does NOT do (overview-locked non-goals)

P6-E does NOT propose, design, sketch, or stub:

- Any concrete `Signer` impl that performs real cryptographic operations.
- Any HSM/KMS client integration in Rust.
- Any Cargo dep on a signing library, KMS SDK, HSM driver, or key-management library.
- Any `secp256k1`/`k256`/`alloy-signer`/`ethers-signers`/`Wallet`/`PrivateKey`/`sign_transaction`/`funded` identifier added anywhere in `crates/`.
- Any funded-key fixture, env-example, config example, or test that constructs a real signing key.
- Any change to the `Signer` trait surface (the trait shape was locked at P5-C and approved through P6-B; future signature change is a Phase 6b decision).
- Any change to `DisabledSigner` (continues to be the only impl).
- Any change to `SignerError::SignerDisabled` (continues to surface the `"Phase 6b Production Gate"` Display literal as the only forward-link).
- Any change to runtime `Signer`-routing behavior in `crates/execution::BundleConstructor` or `crates/app::wire_phase4` (Phase 6b unlock).
- Any "shadow" production-signer mode, "test-key" production-signer mode, or any other backdoor that softens the `DisabledSigner`-only invariant.

If any of these creep into the spec text during impl, **HALT** and re-emit the plan with a flagged scope expansion for explicit user approval.

## Why P6-E exists

The Phase 6 overview explicitly names a production-signer **design document** (not impl) as a Phase 6a deliverable. The motivation is:

1. **Locking the Phase 6b unlock contract on paper before Phase 6b implementation begins.** Without a written contract, Phase 6b reviewers (Codex + user) have no fixed reference for what a "compliant" production signer looks like.
2. **Pinning the safety expectations into a Phase 6a deliverable.** The HSM/KMS-only requirement, never-in-memory key material requirement, audit-log shape, key-rotation lifecycle, and threat model are decisions that benefit from review **now**, while there is still no production-signer code to argue over.
3. **Naming the cross-reference from `docs/specs/execution-safety.md`** so the parent safety policy points at the unlock contract; today `execution-safety.md` Section "Funded Key / Prod Signer Ban" forward-references "Phase 6b Production Gate" by name only, with no doc to point at.

## Deliverables

### D-E1 -- New file: `docs/specs/production-signer.md`

A single new spec document with the following section structure (content drafted **at impl time only**, after Codex pre-impl approval of this plan; the structure itself is what Codex is asked to verdict in this plan):

1. **Section 1 Status + Scope** -- header noting "doc-only design contract; no impl in Phase 6a; Phase 6b Production Gate is the only path to landing any signer that returns `Ok(...)`"; explicit reference to `docs/specs/phase-6a-boundary.md` Section 6 + `execution-safety.md` Section "Funded Key / Prod Signer Ban".
2. **Section 2 The Phase 6b unlock contract (5 hard requirements)** -- five numbered requirements that any production `Signer` impl MUST satisfy before it can replace `DisabledSigner` in Phase 6b:
   - **Section 2.1 HSM/KMS-only key custody.** Private keys live in an HSM/KMS-managed key store; the workspace never receives, allocates, or persists raw private-key bytes. Signing operations are performed by remote calls to the HSM/KMS; the workspace only handles signed-tx bytes.
   - **Section 2.2 Never-in-memory key material.** No code path in the workspace may load private-key bytes into Rust process memory at any time (including during ctor, tests, fixtures, env-overlay, debug prints, panic messages, journal serialization, tracing logs, or any other reachable surface). The `Signer::sign_tx` request-response shape locked at P5-C (`BundleTx -> Result<SignedTxBytes, SignerError>`) is a **necessary but not sufficient** condition: the trait shape keeps key material out of the **caller-facing API**, but it does NOT by itself enforce HSM/KMS-mediated custody inside the eventual impl. The actual enforcement of the never-in-memory invariant comes from (a) HSM/KMS custody per Section 2.1 (the keys physically reside in the HSM/KMS, and signing operations are performed remotely so raw key bytes never traverse the workspace process), and (b) the Phase 6b implementation-review step (the production `Signer` impl is reviewed against this Section 2 contract before it can replace `DisabledSigner`). The trait shape locks the API surface; the Section 2.1 custody + the Section 4 Phase 6b unlock checklist together lock the runtime invariant.
   - **Section 2.3 Signing-related audit linkage (positive requirement; exact event shape Phase 6b-locked).** Any audit event or audit journal that Phase 6b introduces for signing operations MUST satisfy two invariants: (a) **opportunity/bundle correlation** -- the event MUST be correlatable to the upstream `OpportunityEvent` / `MismatchAbort` (or successor) IDs already in the structured-event journal chain, so a single bundle's history is recoverable end-to-end without joining across unrelated event streams; (b) **no key material** -- the event MUST NOT contain any private-key bytes, key derivative (e.g., HKDF output), key fingerprint, derivable secret, or HSM/KMS-internal handle whose disclosure would let an attacker request signatures. Phase 6b is free to design the exact event payload, journal target, retention policy, and field set; this spec locks only the auditability + secret-redaction invariants the design must satisfy.
   - **Section 2.4 Key rotation + lifecycle.** Lists the operator-side rotation procedure (rotate at HSM/KMS; restart workspace; the active key's audit-safe identifier -- see Section 2.3-(b) for what that identifier may NOT be -- surfaces in startup log; old identifier never re-appears in production) and the workspace-side surface (boot-time tracing line naming the active audit-safe identifier only; no raw key bytes; no key fingerprint that would itself qualify as a secret under Section 2.3-(b)).
   - **Section 2.5 Threat model -- assumed adversaries + REJECTED threats + RESIDUAL risks + Phase 6b control points.**
     - **Assumed adversaries**: RCE on the host running the workspace, RCE on the HSM/KMS client library / transport, compromised CI, log exfiltration, journal exfiltration.
     - **Threats REJECTED by Section 2.1 + Section 2.2 (raw-key isolation)**: raw key exfiltration via host memory dump, key extraction from journal bytes, key derivation from any workspace artifact (config, env, fixture, build artifact, log output). HSM/KMS-only custody + the never-in-memory invariant together close these.
     - **RESIDUAL risk -- host-compromise-requested malicious signing.** A compromised host (the Phase 6b workspace process) still controls **what `BundleTx` is sent to the HSM/KMS for signing**. HSM/KMS prevents raw key extraction but the HSM/KMS will sign whatever it receives if request authorization checks pass -- so a sufficiently-privileged compromised host can request signatures on bundles the operator never authorized. This risk is **NOT closed** by Section 2.1+Section 2.2 alone; it is named here as a **Phase 6b control point**. Phase 6b MUST add at minimum one of: per-bundle pre-sign attestation (operator-side cryptographic approval before HSM/KMS will sign), request-authorization rate limits at the HSM/KMS, pre-sign mismatch-comparator gating (sign only if the just-completed local + relay sims passed and the bundle matches the simulated artifact), and an operator-visible audit log of every signing attempt (linked to Section 2.3). The specific control mix is Phase 6b-impl-time; this spec locks the requirement that **at least one** non-trivial host-compromise control MUST land before any production signer replaces `DisabledSigner`.
     - **Accepted residual risks (operational)**: signing-throughput dependency on HSM/KMS availability; cost; HSM/KMS vendor lock-in. Out of scope for cryptographic-design controls.
3. **Section 3 What this document is NOT** -- explicit non-goals matching this plan's Section "What P6-E does NOT do" verbatim, so the spec doc self-references its own scope-locking.
4. **Section 4 Phase 6b unlock checklist** -- bulleted list of the user-approval and Codex-review steps that Phase 6b MUST satisfy before any of Section 2 can begin (overview-locked; matches the Phase 6 overview's "Phase 6b user-approval basis" non-goals list).
5. **Section 5 Cross-references** -- pointers to `docs/specs/execution-safety.md`, `docs/specs/phase-6a-boundary.md`, `docs/adr/ADR-001.md`, `crates/signer/src/lib.rs` (the `Signer` trait + `DisabledSigner` + `SignerError::SignerDisabled` Display literal as the only forward-link).

**Hard invariants on the document text:**

- No `Wallet` / `PrivateKey` / `secp256k1` / `k256` / `alloy-signer` / `ethers-signers` / `sign_transaction` / `funded` Rust identifier example in any code-fence (the doc may discuss the concepts in prose, but no Rust code block introducing the symbol). G2a is `crates/`-scoped so prose mentions are out of gate path, but the spec stylistically MUST NOT model production signer code as Rust snippets -- only prose + interface contracts.
- No vendor-specific code snippet (AWS KMS / GCP Cloud HSM / YubiHSM SDK examples). Vendor names may appear in prose **as examples of the HSM/KMS class**, never as a recommendation or code dependency. (Per Q-E5 below: prefer vendor-neutral wording with a single optional bracketed "[e.g., AWS KMS, GCP Cloud HSM, YubiHSM]"-style example list per Section 2.1.)
- No example RPC payload that includes a key. No example signed-tx that uses a known test-vector private key (e.g., the Ethereum tutorial private keys). Signed-tx examples, if any, MUST use the synthetic placeholder pattern from P6-C v0.3 D-C1 (`FIXTURE_PLACEHOLDER_BYTES`).
- No example signed-tx with a recognizable Ethereum address derived from a known test key. The doc names addresses ONLY as `0x...placeholder...` shapes if needed.
- Length: target **<=150 lines** (v0.2 relaxed from v0.1's <=120; see Q-E1) including blank lines + the cross-reference list. Phase 6a design docs (P6-A boundary spec at 131 lines) set the precedent; the v0.2-expanded Section 2.5 four-sub-bullet threat model needs the small buffer. If Section 2.5 grows past 150, split it into Section 2.5.1 (assumed adversaries + REJECTED threats) + Section 2.5.2 (host-compromise residual risk + Phase 6b control points) inside the 150-line budget.

### D-E2 -- Cross-reference edit: `docs/specs/execution-safety.md` Section "Funded Key / Prod Signer Ban"

Append a single sentence after line 36 pointing to the new doc:

```text
The Phase 6b unlock contract -- five hard requirements a production signer impl must satisfy -- is documented at `docs/specs/production-signer.md`.
```

Single-line additive edit; no other change to `execution-safety.md`. No reordering. No section rename.

### D-E3 -- NO `.rs` / Cargo / ADR / runtime change; NO unnecessary cross-references

- No `.rs` change anywhere in `crates/`.
- No `Cargo.toml` change anywhere.
- No ADR text amendment.
- No new `docs/specs/` file beyond `production-signer.md`.
- No other edit to `execution-safety.md` beyond the single-line cross-reference in D-E2.
- **No edit to `docs/specs/phase-6a-boundary.md`**. The overview's P6-E row mandates a cross-reference ONLY from `execution-safety.md`; a `phase-6a-boundary.md` Section 7 cross-link is **not** required by the overview, so v0.1 explicitly excludes it. The new doc itself cross-links back to `phase-6a-boundary.md` (one-way reference), which is sufficient for navigation. See Q-E6.

## Tests

**N/A -- P6-E is doc-only.** No new Rust tests; no test-file edit; no `cargo test --workspace` count change. Workspace test total at P6-E close: **239 passed + 1 ignored** (unchanged from P6-D close).

## Reused (no duplication)

- Section 3 PRECEDENCE rule from `docs/specs/phase-6a-boundary.md` -- referenced from Section 1 + Section 3 of the new doc, not duplicated.
- `Signer` trait shape from `crates/signer/src/lib.rs` (P5-C) -- referenced from Section 5, never quoted as a Rust snippet.
- `SignerError::SignerDisabled` Display literal `"Phase 6b Production Gate"` -- referenced from Section 5 as the canonical forward-link symbol.
- `BundleTx` / `SignedTxBytes` request-response shape (P5-C DP-C3 / DP-C10) -- referenced from Section 2.2 as the type-level evidence for the never-in-memory invariant.
- Existing P5-D `KillSwitch` PRECEDENCE rule + per-adapter G10 enforcement (P6-D) -- referenced from Section 2.3 as the audit-trail backbone the signing request rides on.

## Gates at P6-E close (deltas vs P6-D close baseline at `d88693b`)

| Gate | Result | Notes |
|---|---|---|
| G1 (`eth_sendBundle` runtime in `crates/`) | **unchanged** at 5 doc-comment hits | P6-E adds no `crates/` change; the new spec doc lives under `docs/specs/` and is OUTSIDE the G1 path scope; prose use of `eth_sendBundle` in the spec doc is permitted (precedent: `execution-safety.md` + `phase-6a-boundary.md` both use the term in prose). |
| G2a (signer-symbol set in `crates/`) | **unchanged at 0** | the new spec doc is under `docs/specs/`, OUT of the G2a path scope. |
| G2b (Cargo signer-dep symbols) | unchanged (0) | no `Cargo.toml` change. |
| G2c / G2d (3-file allow-list in `crates/`) | unchanged | no `crates/` change. |
| G2e (signer dep edges) | unchanged (2) | no `Cargo.toml` change. |
| G3 (`submit_bundle(` callers in `crates/app/src/`) | **unchanged at 0** | no `crates/app/` change. |
| G4 (`dyn BundleRelay` / `Arc<dyn BundleRelay>` in `crates/app/src/`) | **unchanged at 0** | no `crates/app/` change. |
| G5 (`live_send` mutation) | unchanged | no config / runtime change. |
| G6 (secret logging in `crates/`) | unchanged | no `crates/` change. |
| G7 (`#[ignore]` count) | unchanged at 1 | no test change. |
| G8 (workspace dep cycles) | unchanged | no Cargo change. |
| G9 (`KillSwitch` reach in `crates/`) | unchanged | no `crates/` change. |
| G10 (per-adapter `submit_bundle` first-statement KS check) | unchanged (enforced at P6-D close) | no `crates/relay-clients/src/` change. |
| G11 (production `sign_tx` call site) | unchanged at 1 | no `crates/execution/src/` change. |

Workspace tests at P6-E close: **239 passed + 1 ignored** -- **inherited from P6-D close (`d88693b`); NOT re-verified at P6-E** per Codex v0.1 item 1 (doc-only batch; full cargo verification is the P6-F audit step).

## Forbidden in P6-E

- Any `.rs` change anywhere in `crates/`.
- Any `Cargo.toml` change anywhere.
- Any ADR text amendment.
- Any new `docs/specs/` file beyond `production-signer.md`.
- Any edit to `docs/specs/execution-safety.md` beyond the single-line D-E2 cross-reference append.
- Any edit to `docs/specs/phase-6a-boundary.md` (the overview's P6-E row does NOT require a back-cross-link; v0.1 excludes it).
- Any Rust code block in `production-signer.md` that introduces a `Wallet` / `PrivateKey` / `secp256k1` / `k256` / `alloy-signer` / `ethers-signers` / `sign_transaction` / `funded` identifier.
- Any vendor SDK code snippet (AWS KMS / GCP Cloud HSM / YubiHSM / etc.) -- vendor names appear ONLY in prose Section 2.1 as an example class, never as a code dependency.
- Any example RPC payload containing a private key.
- Any example signed-tx using a recognizable test-vector private key.
- Any new `Signer` trait method, new `SignerError` variant, new `Signer` impl proposal that the spec doc claims is "production-ready" or "Phase 6a-ready". All references to a production impl are explicitly framed as "Phase 6b unlock; not in scope here".
- Any "shadow signer" / "test-key signer" / "feature-flagged signer" carve-out that would soften the `DisabledSigner`-only invariant in Phase 6a.
- Any change to the `SignerError::SignerDisabled` `Display` literal.
- Any `live_send = true` reference (the doc may mention `live_send = false` as the parent ban; flipping is exclusively a Phase 6b decision).
- Any `eth_sendBundle` claim in `crates/` (prose mention in `docs/specs/` is permitted; G1 stays clean).
- Any `.claude/` / `AGENTS.md` / `fixture_output.txt` / `hook_toast.md` staging.
- Any destructive git or force-push.
- Any asset / V3-fee-tier / venue widening.

## Plan execution checklist (doc-only)

- [ ] **Step 1: Confirm predecessor state.** `git log --oneline -5` shows `d88693b` HEAD; workspace 239 passed + 1 ignored; G1..G11 all green at P6-D close.
- [ ] **Step 2: Author `docs/specs/production-signer.md`** per Section D-E1 (5 sections, <=150 lines per v0.2 Q-E1, no Rust code blocks introducing forbidden symbols, no vendor SDK snippets, no example private keys).
- [ ] **Step 3: Append single-line cross-reference to `docs/specs/execution-safety.md`** per Section D-E2.
- [ ] **Step 4: Self-check the spec doc.** Run `rg -n -e 'Wallet' -e 'PrivateKey' -e 'secp256k1' -e '\bk256\b' -e 'sign_transaction' -e 'funded' docs/specs/production-signer.md` -- confirm zero hits inside code-fences (prose hits OK). Run `wc -l docs/specs/production-signer.md` -- confirm <=150 lines (v0.2 cap per Q-E1).
- [ ] **Step 5: Minimum-checks-only gate set (v0.2 per Codex item 1 -- doc-only, no cargo verification).** `git status` + `git diff` review of the new doc + the single `execution-safety.md` cross-reference line; `wc -l docs/specs/production-signer.md` confirms the Section "v0.2 length cap" target (Q-E1); targeted ripgrep checks on the new doc per Step 4 confirm no forbidden-symbol code-fences. **Workspace test baseline is INHERITED from P6-D close** (`d88693b` = 239 passed + 1 ignored) without re-running. **NO `cargo test --workspace` / `cargo fmt --check` / `cargo clippy` / `cargo deny check` / `cargo tree -d` runs at P6-E close.** G1..G11 ripgrep gates are naturally unchanged because `crates/` is untouched -- re-running them at P6-E provides no new signal. Full cargo + G1..G11 verification is the formal **P6-F audit step** (phase-6a-complete tag prerequisite). Rationale: doc-only batches inherit baselines from the most recent code-change batch; conflating P6-E gating with P6-F audit-time gating wastes context + cycles without changing the safety story. Per project memory `feedback_phase2_doc_volume` + the v0.2 Codex item 1.
- [ ] **Step 6: Commit + push.** Single routine `docs(p6-e)` commit. Suggested message: `docs(p6-e): production-signer.md design contract (Phase 6b unlock) + execution-safety cross-reference`.
- [ ] **Step 7: Update `.coordination/claude_outbox.md`** with the P6-E closeout report; emit P6-F pre-impl plan draft.

## Risks + open questions

- **Q-E1 -- Spec doc length cap: <=150 lines (v0.2 relaxed from v0.1's <=120).** v0.2 expanded Section 2.5 substantially per Codex item 4 (host-compromise-requested malicious signing residual risk + Phase 6b control points are now four sub-bullets, not a single sentence). The <=120 cap is no longer feasible without compressing Section 2.5 to the point of losing the Codex-mandated detail. v0.2 recommends **<=150 lines** (still <= P6-A boundary spec's 131-line precedent + a small buffer for the four-sub-bullet Section 2.5). If Section 2.5 grows past 150, split into Section 2.5.1 (assumed adversaries + REJECTED threats) + Section 2.5.2 (host-compromise residual risk + Phase 6b control points) inside the 150-line budget rather than expanding further. Codex verdict on the relaxed cap?
- **Q-E2 -- Should Section 2 list specific HSM/KMS vendors as examples, or stay fully vendor-neutral?** v0.1 recommends **bracketed example list** in Section 2.1 only (single line: "[e.g., AWS KMS, GCP Cloud HSM, YubiHSM]"). Vendor naming there is illustrative -- the doc itself takes no dependency, and Section 2.1 emphasizes the contract (private keys in HSM/KMS-managed store) rather than the vendor. Vendor names MUST NOT appear in Section 2.2..Section 2.5 or any code-fence. Codex verdict on vendor naming?
- **Q-E3 -- Should `production-signer.md` introduce a `ProductionSigner` placeholder type name** (as a prose-only forward-reference, never a Rust identifier added to `crates/`)? v0.1 recommends **NO**. Naming a placeholder type in prose risks future readers searching for it as if it were a real symbol. The doc should refer to the future impl only as "the production `Signer` impl" or "the Phase 6b `Signer` impl"; no new identifier introduced. Codex verdict?
- **Q-E4 -- Should `production-signer.md` propose an explicit list of "deferred-to-Phase-6b" TODOs**, or stay forward-looking-design-ONLY? v0.1 recommends **forward-looking design ONLY**. Numbered TODOs invite "let's land #1 in Phase 6a as a small win" creep; the doc must be unambiguously Phase 6b-gated. The Phase 6 overview already enumerates the Phase 6b deliverables; the spec doc just refers to that list rather than restating it. Codex verdict?
- **Q-E5 -- Threat model breadth.** v0.2 Section 2.5 now covers: assumed adversaries (RCE on host, RCE on HSM/KMS client library, compromised CI, log/journal exfiltration), REJECTED threats (raw key extraction; closed by Section 2.1+Section 2.2), the **host-compromise-requested malicious signing RESIDUAL risk** (per Codex v0.1 item 4; named as a Phase 6b control point requiring at least one of: per-bundle pre-sign attestation, request-authorization rate limits, pre-sign mismatch-comparator gating, operator-visible signing audit log), and accepted operational residuals (availability, cost, vendor lock-in). Broader threat-modeling (supply-chain attack on Cargo deps, kernel-level memory inspection, side-channel attacks against the HSM/KMS itself, vendor compromise) is real but Phase 6b-implementation-time concern, not Phase 6a-design-time concern. The v0.2 <=150-line cap (Q-E1) accommodates the v0.2 Section 2.5 expansion without further broadening. Codex verdict on this scope?
- **Q-E6 -- `phase-6a-boundary.md` Section 7 back-cross-link.** v0.1 **excludes** any edit to `phase-6a-boundary.md`. The Phase 6 overview's P6-E row mandates a cross-reference ONLY from `execution-safety.md`; a `phase-6a-boundary.md` Section 7 entry is convenient but **not required by the overview**, and the user clarification "Plan the execution-safety.md cross-link only if required by the already-approved overview" rules out adding it. The new `production-signer.md` Section 5 cross-references `phase-6a-boundary.md` (one-way) which is sufficient navigation. Codex verdict?

## Process

Per the 2026-05-04 routine-closeout policy + the overview Section Process:

1. Claude writes this pre-impl plan to disk (UNCOMMITTED) + emits the review pack to `.coordination/claude_outbox.md`.
2. Claude STOPS and reports "manual Codex pre-impl review required". **No `.rs` / `Cargo.toml` / ADR / `docs/specs/` edits in this turn (no `production-signer.md` content lands at planning time).**
3. User pastes pack to Codex; relays verdict.
4. Claude records verdict in `.coordination/codex_review.md`.
5. **APPROVED** -> commit + push this plan as a routine doc commit; THEN execute per Section "Plan execution checklist" (write `production-signer.md`, append cross-references, run lightweight gates); THEN commit + push the spec-doc impl; THEN draft P6-F pre-impl plan (audit + tag).
6. **REVISION REQUIRED** -> revise plan in place + re-emit pack.
7. **Scope / ADR change required** -> HALT to user.
