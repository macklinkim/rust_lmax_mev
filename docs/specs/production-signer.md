# Production Signer Design Contract (Phase 6b Unlock)

## Section 1 -- Status + Scope

**Doc-only design contract; no impl in Phase 6a.** This document captures the five hard requirements that any production `Signer` impl MUST satisfy before it can replace `DisabledSigner` in Phase 6b. **The Phase 6b Production Gate is the only path to landing any `Signer` impl that returns `Ok(...)`.** No code in this spec; no Cargo dep; no fixture. The Phase 6a workspace continues to reach only `crates/signer::DisabledSigner`, whose `sign_tx` returns `Err(SignerError::SignerDisabled)` with the `Display` literal `"Phase 6b Production Gate"` as the canonical forward-link.

Parent context:

- `docs/specs/phase-6a-boundary.md` Section 6 (Phase 6b boundary kept explicit).
- `docs/specs/execution-safety.md` Section "Funded Key / Prod Signer Ban".

## Section 2 -- The Phase 6b unlock contract (five hard requirements)

Any production `Signer` impl proposed in Phase 6b MUST satisfy every requirement Section 2.1..Section 2.5 below. None of these requirements may be relaxed in Phase 6a; relaxation is a Phase 6b decision gated on user authorization and a separate Codex review.

### Section 2.1 -- HSM/KMS-only key custody

Private keys live in an HSM/KMS-managed key store. The workspace never receives, allocates, or persists raw private-key bytes. Signing operations are performed by remote calls to the HSM/KMS; the workspace only handles signed-tx bytes returned by the HSM/KMS as opaque payloads.

Vendor naming is illustrative only. Examples of the HSM/KMS class: [e.g., AWS KMS, GCP Cloud HSM, YubiHSM]. No vendor name in this section implies a recommendation, a code dependency, or a fixed deployment target; the Phase 6b implementation may select any vendor that meets the contract.

### Section 2.2 -- Never-in-memory key material (necessary but not sufficient framing)

No code path in the workspace may load private-key bytes into Rust process memory at any time, including ctor, tests, fixtures, env-overlay, debug prints, panic messages, journal serialization, tracing logs, or any other reachable surface.

The `Signer::sign_tx` request-response shape locked at Phase 5 P5-C, `BundleTx -> Result<SignedTxBytes, SignerError>`, is a **necessary but not sufficient** condition for this invariant. The trait shape keeps key material out of the caller-facing API, but it does NOT by itself enforce HSM/KMS-mediated custody inside the eventual impl: a non-compliant impl could still buffer key bytes internally. The actual enforcement of never-in-memory comes from two sources jointly:

1. **HSM/KMS custody** per Section 2.1. Raw key bytes physically reside in the HSM/KMS; signing operations are performed remotely; key bytes never traverse the workspace process.
2. **Phase 6b implementation-review step**. Section 4 below names the review gates the production `Signer` impl MUST pass against this Section 2 contract before it can replace `DisabledSigner`. The trait shape locks the API surface; Section 2.1 custody + Section 4 review jointly lock the runtime invariant.

### Section 2.3 -- Signing-related audit linkage (positive requirement; event shape Phase 6b-locked)

Any audit event or audit journal that Phase 6b introduces for signing operations MUST satisfy two invariants:

1. **Opportunity/bundle correlation.** The event MUST be correlatable to the upstream `OpportunityEvent` / `MismatchAbort` (or successor) IDs already in the structured-event journal chain. A single bundle's history MUST be recoverable end-to-end from the existing event streams without joining across unrelated streams.
2. **No key material in the event.** The event MUST NOT contain any private-key bytes, key derivative (e.g., HKDF output, MAC over the key, symmetric-encryption output keyed by the private key), key fingerprint, derivable secret, or HSM/KMS-internal handle whose disclosure would let an attacker request signatures.

Phase 6b is free to design the exact event payload, journal target, retention policy, and field set. This spec locks only the auditability + secret-redaction invariants the design MUST satisfy. A non-secret signing audit event/journal is a legitimate Phase 6b design choice; this spec explicitly does NOT forbid it.

### Section 2.4 -- Key rotation + lifecycle

Operator-side rotation procedure:

1. Rotate at the HSM/KMS (vendor-specific; out of scope here).
2. Restart the workspace process.
3. On boot, the workspace's active key audit-safe identifier surfaces in the startup tracing line.
4. The old identifier MUST NOT re-appear in production tracing after the restart.

Workspace-side surface:

- A single boot-time tracing line names the active audit-safe identifier only.
- No raw key bytes are emitted at any point.
- No identifier that itself qualifies as a secret under Section 2.3-(b) (e.g., a key fingerprint that an attacker could use to request signatures) may appear in the tracing line.

### Section 2.5 -- Threat model

**Assumed adversaries:**

- RCE on the host running the workspace process.
- RCE on the HSM/KMS client library or transport layer.
- Compromised CI (build-time supply-chain access).
- Log exfiltration (tracing output captured by an external party).
- Journal exfiltration (structured-event bytes captured by an external party).

**Threats REJECTED by Section 2.1 + Section 2.2 (raw-key isolation):**

- Raw key exfiltration via host memory dump.
- Key extraction from journal bytes (rkyv-archived or bincode-serialized payloads).
- Key derivation from any workspace artifact: config file, env variable, fixture file, build artifact, log output.

HSM/KMS-only custody plus the never-in-memory invariant together close these.

**RESIDUAL risk -- host-compromise-requested malicious signing:**

A compromised host (the Phase 6b workspace process) still controls **what `BundleTx` is sent to the HSM/KMS for signing**. HSM/KMS prevents raw key extraction, but the HSM/KMS will sign whatever it receives as long as request authorization checks pass. A sufficiently-privileged compromised host can therefore request signatures on bundles the operator never authorized. This risk is **NOT closed** by Section 2.1 + Section 2.2 alone; it is named here as a **Phase 6b control point**. Phase 6b MUST add at minimum one of the following non-trivial host-compromise controls before any production signer replaces `DisabledSigner`:

- **Per-bundle pre-sign attestation.** Operator-side cryptographic approval of each bundle before the HSM/KMS will sign.
- **Request-authorization rate limits at the HSM/KMS.** Caps on signatures per minute / per block / per operator session enforced by the HSM/KMS, not by the workspace.
- **Pre-sign mismatch-comparator gating.** The signer impl signs only if the just-completed local-simulator + relay-simulator comparison passed AND the bundle matches the simulated artifact byte-for-byte.
- **Operator-visible signing audit log.** Every signing attempt linked to the opportunity/bundle chain per Section 2.3, surfaced in an operator dashboard with a configurable alert threshold.

The specific control mix is Phase 6b-impl-time. This spec locks only the requirement that at least one non-trivial host-compromise control MUST land.

**Accepted residual risks (operational, out of scope for cryptographic-design controls):**

- Signing-throughput dependency on HSM/KMS availability.
- Cost.
- HSM/KMS vendor lock-in.

## Section 3 -- What this document is NOT

- NOT a `Signer` impl design or sketch.
- NOT an HSM/KMS client integration design.
- NOT a Cargo-dep proposal for any signing library, KMS SDK, or HSM driver.
- NOT a vendor recommendation; vendor names in Section 2.1 are illustrative only.
- NOT a "shadow signer" / "test-key signer" / "feature-flagged signer" carve-out that would soften the `DisabledSigner`-only invariant in Phase 6a.
- NOT a change to the `Signer` trait surface, `DisabledSigner` impl, or `SignerError::SignerDisabled` `Display` literal.
- NOT a change to runtime `Signer`-routing behavior in `crates/execution::BundleConstructor` or `crates/app::wire_phase4`.
- NOT a list of Phase 6b TODOs to be incrementally landed in Phase 6a; every requirement here is Phase 6b-gated as a single contract.

## Section 4 -- Phase 6b unlock checklist

Phase 6b MUST satisfy ALL of the following before any production `Signer` impl can replace `DisabledSigner`:

1. Fresh explicit user authorization for Phase 6b Production Gate work (the Phase 6 overview's "Phase 6b user-approval basis" non-goals must be lifted by direct user authorization).
2. A Phase 6b overview document under `docs/superpowers/plans/` mirroring the Phase 6a overview pattern, listing batches and gates.
3. A separate Codex review of the production `Signer` impl against every requirement in Section 2.1 through Section 2.5, with explicit verdicts that each requirement is satisfied.
4. At least one non-trivial host-compromise control per Section 2.5 RESIDUAL risk landed and reviewed.
5. A Phase 6b boundary document under `docs/specs/` (separate from `phase-6a-boundary.md`) capturing the runtime contract for `live_send`, `eth_sendBundle`, and funded-key wiring.
6. Updated ADR-001 amendment authorized by the user that explicitly lifts the funded-key / prod-signer ban for the scoped Phase 6b context.

Until every checklist item is satisfied, the Phase 6a workspace continues to reach only `DisabledSigner`.

## Section 5 -- Cross-references

- `docs/specs/execution-safety.md` -- parent safety policy (`submit_bundle` ban, `live_send` default, funded-key ban, gas-bidding policy, kill switch). The "Funded Key / Prod Signer Ban" section forward-references this document.
- `docs/specs/phase-6a-boundary.md` -- Phase 6a fail-closed safety contract between `submit_bundle`, `Signer`, and `KillSwitch`; Section 3 PRECEDENCE rules; Section 6 Phase 6b boundary kept explicit.
- `docs/adr/ADR-001.md` -- mempool ingestion + Phase 6 gate context.
- `crates/signer/src/lib.rs` -- the `Signer` trait + `DisabledSigner` impl + `SignerError::SignerDisabled` variant whose `Display` literal `"Phase 6b Production Gate"` is the only forward-link symbol in the workspace.
