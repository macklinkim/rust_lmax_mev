# Phase 4 Batch E — `BundleRelay` trait + Flashbots/bloXroute adapters + read-only `eth_callBundle` HTTP client + relay-sim → comparator wiring

**Date:** 2026-05-10 KST
**Status:** Draft v0.6 (revised after manual Codex REVISION REQUIRED HIGH on v0.5, 2026-05-10 KST). One residual contradiction R-E20 addressed in place (Deliverable E-1 `BundleRelay` Rust doc comment replaced with Codex-supplied wording; new residue `rg` gate added at batch close). R-E1..R-E19 + trait-shape conditional acceptance carried unchanged from v0.5. Awaiting manual Codex re-review.
**Predecessor:** P4-D closed + pushed at `4e32ab8` (manual Codex APPROVED HIGH on plan v0.4 + NO ACTION HIGH on closeout 2026-05-10 KST).

## Phase 4 progress at P4-E start

| Batch | Status | HEAD |
|---|---|---|
| P4-A archive node | CLOSED | `e2b6704` |
| P4-B state-fetcher | CLOSED | `856a859` |
| P4-C1 simulator/state-fetcher infra | CLOSED | `7efbb8d` |
| P4-C2 real-revm + ProfitSource flip + SR-1 Success | CLOSED | `74fcec8` |
| P4-D MismatchCategory + reconciler + relay-sim comparator infra | CLOSED | `4e32ab8` |
| **P4-E `BundleRelay` trait + Flashbots/bloXroute adapters + `eth_callBundle` HTTP + relay-sim → comparator wiring** | **THIS BATCH** | — |
| P4-F Sushiswap WETH/USDC + external mempool feed | NOT STARTED | — |
| P4-G final wiring + DoD audit + `phase-4-complete` tag | NOT STARTED | — |

## Honest scope claim

P4-E ships:
1. The `BundleRelay` trait (object-safe; both sim + submit methods declared but submit is FAIL-CLOSED).
2. Two HTTP relay adapters (Flashbots + bloXroute) implementing `RelaySimulator::simulate_bundle` (the trait method `BundleRelay` extends) via read-only `eth_callBundle`. Both adapters' `BundleRelay::submit_bundle` returns a hard `Err(SubmitDisabled)` regardless of caller; no submission code path exists in P4-E.
3. The relay-sim → comparator → `MismatchAbort` wiring inside `wire_phase4`'s producer chain (or a NEW `wire_phase5_pre_safety` constructor — see DP-E2). Aborts are journaled.
4. Config additions for relay endpoints + per-relay timeout, with safe defaults (no live submission ever enabled by config).

P4-E does NOT ship:
- Funded key, signer, wallet, key derivation, or any submittable-tx producer.
- `eth_sendBundle` call site (the trait method exists; no caller invokes it; the adapter impls return hard `Err`).
- `live_send = true` capability (config field stays `false` default; runtime check rejects `true`; submit-path returns `Err` even if the flag slipped through — defense in depth).
- Production signer infra (Phase 5 Safety Gate).
- Sushiswap or external mempool feed (P4-F).
- DoD audit or `phase-4-complete` tag (P4-G).

## Scope (foundation-first ordering)

Three deliverables.

### Deliverable E-1 — `BundleRelay` trait + `RelaySubmitDisabled` error variant (NEW `crates/bundle-relay`)

A new top-level crate `crates/bundle-relay` defines the trait + the error type + a stub `submit_bundle` invariant. The trait method shape supports BOTH simulation (used in P4-E) and submission (declared, fail-closed). Co-locating gives Phase 5 a single boundary to flip.

```rust
// crates/bundle-relay/src/lib.rs
use async_trait::async_trait;
use rust_lmax_mev_relay_sim::{
    RelaySimRequest, RelaySimulationOutcome, RelaySimError, RelaySimulator,
};

/// Carrier of one signed bundle ready for submission. P4-E ships
/// the type; the only writers in P4-E are tests + the deliberately
/// fail-closed adapter `submit_bundle` impls. Real signers + funded
/// keys land in Phase 5 Safety Gate.
///
/// Per-field rkyv adapters (R-E12) — `Address` + `U256` cannot derive
/// rkyv natively. `block_hash` is `[u8; 32]` directly (rkyv-native;
/// matches existing P4-D `MismatchAbort` shape) so no adapter needed.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct SignedBundle {
    pub block_hash: [u8; 32],     // target block hash; rkyv-native
    pub state_block_number: u64,  // sim/submit at this block height
    pub signed_txs: Vec<Vec<u8>>, // raw signed tx bytes (only test fixtures populate in P4-E)
    #[rkyv(with = crate::rkyv_compat::AddressAsBytes)]
    pub coinbase_recipient: alloy_primitives::Address,
    #[rkyv(with = crate::rkyv_compat::U256AsBytes)]
    pub coinbase_transfer_wei: alloy_primitives::U256,
    pub validity_block_min: u64,
    pub validity_block_max: u64,
}

/// P4-E + future submit-path errors. `#[non_exhaustive]`.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BundleRelayError {
    #[error("relay sim transport failure: {0}")]
    Transport(String),
    #[error("relay sim returned unrecognized payload: {0}")]
    UnrecognizedResponse(String),
    /// HARD INVARIANT (P4-E §DP-E1): every adapter `submit_bundle`
    /// returns this. There is no caller in P4-E. Phase 5 Safety
    /// Gate adds the funded-key + signing infra and replaces this
    /// with real submission via an explicit code change (NOT a
    /// config flip).
    #[error("submit_bundle disabled in this build (Phase 5 Safety Gate required)")]
    SubmitDisabled,
    #[error("relay configuration invalid: {0}")]
    Config(String),
}

/// Object-safe async trait for relay endpoints that expose both the
/// simulation and submission surfaces. P4-E adapters implement both
/// `RelaySimulator` and `BundleRelay`, but P4-E app/comparator wiring
/// stores them only as `Arc<dyn RelaySimulator>`; no `dyn BundleRelay`
/// object and no trait-object upcast is constructed in `crates/app`.
/// The `dyn BundleRelay` shape exists for concrete-adapter
/// submit-disabled tests and Phase 5+ submission consumers.
#[async_trait]
pub trait BundleRelay: RelaySimulator + Send + Sync + 'static {
    fn name(&self) -> &str;

    /// HARD INVARIANT (P4-E §DP-E1): every impl in P4-E returns
    /// `Err(BundleRelayError::SubmitDisabled)`. No call site exists
    /// in P4-E that invokes this method. Phase 5 Safety Gate is the
    /// only path to enabling real submission.
    async fn submit_bundle(
        &self,
        bundle: &SignedBundle,
    ) -> Result<SubmissionReceipt, BundleRelayError>;
}

/// Receipt from a submission. P4-E ships the type; no writer exists.
/// All fields are rkyv-native (`String`, `u64`); no per-field adapter
/// needed (R-E12).
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct SubmissionReceipt {
    pub relay_name: String,
    pub bundle_hash: String,
    pub submitted_at_unix_ns: u64,
}
```

`crates/bundle-relay/src/rkyv_compat.rs` (NEW) ships local `AddressAsBytes` + `U256AsBytes` adapters following the existing P3-E / P4-D shape. Adapters cannot be shared across crates because rkyv-with bindings are tied to the deriving crate.

Tests (`crates/bundle-relay/src/lib.rs` cfg(test)):
- **BR-1** trait object-safety: `let _: Box<dyn BundleRelay> = Box::new(impl);` compiles (covered transitively by adapter tests in E-2).
- **BR-2** SignedBundle + SubmissionReceipt rkyv + serde round-trip.
- **BR-3** `SubmitDisabled` Display message contains "Phase 5 Safety Gate" (spec-drift guard).

### Deliverable E-2 — Flashbots + bloXroute HTTP adapters (NEW `crates/relay-clients`)

A NEW peer crate `crates/relay-clients` holds the actual HTTP adapters.

```rust
// crates/relay-clients/src/lib.rs
pub mod flashbots;
pub mod bloxroute;
pub use flashbots::FlashbotsRelay;
pub use bloxroute::BloxrouteRelay;
```

Each adapter:
- Holds an `alloy::providers` HTTP client (existing dep) configured against the relay's RPC endpoint.
- `RelaySimulator::simulate_bundle(&self, req)` issues a JSON-RPC `eth_callBundle` call with `req.txs` (raw tx bytes) + `req.state_block_number` + `req.block_hash`.
- Parses the relay's JSON response → `RelaySimulationOutcome`. Profitability + gas + status come from the response; `state_observations` stays empty in P4-E (Flashbots/bloXroute do not return per-slot observations from `eth_callBundle`; the comparator's `StateDependency` check still works because of CMP-5b's "relay-omitted slots are NOT a mismatch" boundary established in P4-D).
- `BundleRelay::submit_bundle(&self, _)` returns `Err(BundleRelayError::SubmitDisabled)` UNCONDITIONALLY. No HTTP code path; no signing; no key.

Per-adapter:
- **`FlashbotsRelay`**: endpoint default `https://relay.flashbots.net`. `eth_callBundle` per https://docs.flashbots.net/flashbots-auction/advanced/rpc-endpoint. P4-E does NOT implement the Flashbots `X-Flashbots-Signature` header — that requires a key. Read-only `eth_callBundle` accepts unsigned requests at the auth-header level (the BUNDLE itself contains raw signed txs; auth header is for rate-limit identity, not auth). The adapter omits the header in P4-E and documents the consequence: rate-limited as anonymous; production signing-header support is a Phase 5 add. CMP tests use a `MockHttp` (in-test wiremock) that does not require the header.
- **`BloxrouteRelay`**: endpoint default `https://api.blxrbdn.com`. Same shape. bloXroute requires an `Authorization` header with an API key for production; in P4-E the adapter accepts an `Option<String>` API key from config; if `None`, `simulate_bundle` returns `Err(RelaySimError::NotConfigured)` (R-E7 v0.3 fix — reuses the existing P4-D variant rather than introducing a new `Config(String)` variant; the variant is payload-free, eliminating any string-leak surface for the missing-key reason). API key is held only in a private adapter field and NEVER logged — see DP-E11 redaction mechanism + RC-COMMON-2 test.

Common HTTP behavior:
- `reqwest`-based via alloy's existing transport (no new dep).
- Per-request timeout: `relay.simulate_timeout_ms` config default 2000ms.
- Retry policy: NO automatic retries in P4-E (latency budget for the comparator path is single-digit hundreds of ms; retrying squanders the per-block deadline). Failed call → `RelaySimError::Transport`. P4-G or Phase 5 may add bounded retries with metrics.
- Secret redaction: zero `tracing::*` calls in adapter src that take URL or header values as arguments. The only `tracing` allowed is fixed-string error reasons + structured fields with PRE-FILTERED values (e.g., `relay = "flashbots"`, never `endpoint = %url`).

Tests (`crates/relay-clients/src/{flashbots,bloxroute}.rs` cfg(test) + `crates/relay-clients/tests/`):
- **RC-F-1** Flashbots `simulate_bundle` against a `wiremock::MockServer`-served `eth_callBundle` happy-path response → `RelaySimulationOutcome` parsed correctly.
- **RC-F-2** Flashbots transport error (mock server returns 500) → `RelaySimError::Transport(...)`.
- **RC-F-3** Flashbots unrecognized payload (mock returns malformed JSON) → `RelaySimError::UnrecognizedResponse(...)`.
- **RC-F-4** Flashbots `submit_bundle` returns `Err(SubmitDisabled)` regardless of input.
- **RC-B-1..RC-B-4** mirrors for bloXroute.
- **RC-COMMON-1** secret redaction grep gate: `grep -RIn 'tracing::.*url\|tracing::.*api_key' crates/relay-clients/src/` → 0 matches (verified at batch close).
- No `#[ignore]`'d live-network tests (env-gated tests against real Flashbots stay deferred to Phase 5; P4-E proves shape against `wiremock` only).

`wiremock` is added as a dev-dep on `crates/relay-clients` only (not workspace-wide).

### Deliverable E-3 — relay-sim → comparator wiring into the producer chain

Extend `wire_phase4` (or introduce `wire_phase5_pre_safety` per DP-E2) with:
1. Construction of zero-or-more concrete relay adapters from `config.relay.enabled_relays` (R-E13: **EMPTY default**, fail-closed). Each is an `Arc<dyn RelaySimulator>` (per DP-E13 v0.3 — no `Arc<dyn BundleRelay>` is constructed in `crates/app`). With the empty default, the comparator_driver is inert per DP-E3.
2. A NEW `comparator_driver` task subscribed to `exec_tx` (the `BundleCandidate` broadcast). For each candidate it:
   - Looks up the upstream `LocalStateFingerprint` (carried alongside `SimulationOutcome` via a new `SimulationOutcomeWithFingerprint` envelope — see DP-E5).
   - Builds a `RelaySimRequest` (the `txs` field is **EMPTY** in P4-E because no signing exists; the relay sim short-circuits with `Err(RelaySimError::UnsignedBundleUnavailable)` BEFORE any network I/O per DP-E4; the comparator classifies via `compare_result` as `MismatchCategory::Unknown`). R-E2 fail-closed pre-check happens inside each adapter; the test mock can still exercise the happy-path wiring with non-empty fixture `txs`.
   - Calls `RelaySimulator::simulate_bundle(req)` on the held `Arc<dyn RelaySimulator>` (R-E18 v0.4: the driver holds the simulator-trait object only — see DP-E13 — so the call site names the simulator method, not the bundle-relay-trait method).
   - Calls `relay_sim::compare_result(...)`.
   - On `Ok(())` → emits a `MismatchCheckPassed { candidate, relay_outcome }` envelope to a NEW `comparator_tx` broadcast channel.
   - On any `Err` (relay-sim error variant or `MismatchAbort` from `compare_result`) → constructs `EventEnvelope<MismatchAbort>` (R-E16 v0.4: NO drain task — the driver itself owns the `FileJournal<MismatchAbort>`) and synchronously calls `journal.append(&envelope)?` + `journal.flush()?` per DP-E8 v0.4. After both calls return Ok, the driver MAY emit an observability `MismatchAbortRecord` envelope on `mismatch_tx` for monitors; this broadcast is best-effort and is the LAST step on the abort path.
3. **`comparator_tx` is NOT subscribed by any submit path**. The `MismatchCheckPassed` envelope is a TERMINAL event in P4-E — observability and journal only. No code path leads to `BundleRelay::submit_bundle`.

Per-step config:
- `relay.enabled_relays: Vec<RelayClientConfig>` — empty default. If empty, the comparator driver runs with the test mock only (or skips itself; see DP-E3).
- `relay.simulate_timeout_ms: u64` default 2000.

Tests (`crates/app/tests/`):
- **CW-1** `comparator_driver` happy path: construct `wire_phase4` (or new wiring) with a `MockRelaySimulator` programmed for `Ok`; publish a synthetic SimulationOutcome upstream; assert a `MismatchCheckPassed` envelope arrives on `comparator_tx`.
- **CW-2** `comparator_driver` mismatch path: mock programmed for a profit-mismatch outcome; assert a `MismatchAbortRecord` envelope arrives + the journal contains the abort record.
- **CW-3** SubmitDisabled hard guarantee: spawn the wiring; subscribe to nothing that could feed `submit_bundle`; sanity-grep that no production code calls `submit_bundle` (ripgrep test asserting zero non-test references).

## Decision points

- **DP-E1 (CRITICAL — fail-closed submit)**: every `BundleRelay::submit_bundle` impl in P4-E returns `Err(BundleRelayError::SubmitDisabled)` UNCONDITIONALLY. The trait method exists so Phase 5 can replace the body with real submission via an explicit code change (NOT a config flip). NO caller in `crates/app` invokes `submit_bundle` in P4-E. Q8(c) grep gate: `grep -RIn 'submit_bundle' crates/app/` → 0 matches; `grep -RIn 'submit_bundle' crates/relay-clients/src/` → matches only the impl-returning-Err lines. Defense in depth: even if a future PR mistakenly adds a caller, the impl returns `Err` and config validation rejects `live_send=true`.
- **DP-E2 (wire_phase4 vs new constructor)**: extend `wire_phase4` IN PLACE, additively. Rationale: P3-F's `wire_phase4` is the canonical Phase-3-shipped wiring, but it was ALWAYS designed to grow into the Phase 4 final wiring (the name `wire_phase4` was chosen at P3-F to anticipate this). Introducing a new constructor here would force P4-G to plumb yet another. Additive: new tasks spawned alongside the existing six; existing tasks unchanged; existing `AppHandle4` gains 2 new task fields + 1 new broadcast sender; existing public methods unchanged. **DOES count as an edit to an additively-frozen crate; the carve-out is narrowly scoped + documented.** Alternative considered + rejected: a NEW `wire_phase5_pre_safety` would have meant maintaining two wiring functions through P4-F + P4-G; the maintenance cost > the carve-out cost.
- **DP-E3 (empty `enabled_relays`)**: when `config.relay.enabled_relays` is empty (default), the `comparator_driver` task IS still spawned but reads zero relays → it logs "no relays configured; comparator inert" once and the task exits cleanly. This keeps behavior deterministic in dev/test profiles where no relay is desired and avoids partial wiring.
- **DP-E4 (REVISED v0.2 per R-E1+R-E2)** — signed-bundle tension at `eth_callBundle` time:
  - Per docs/specs/execution-safety.md + the standing forbids, P4-E has NO funded key + NO signer. `eth_callBundle` requires raw signed tx bytes in the bundle to actually simulate.
  - **HTTP-client preconditions (R-E2)**: every adapter's `simulate_bundle` performs a fail-closed pre-check: `if req.txs.is_empty() { return Err(RelaySimError::UnsignedBundleUnavailable) }` BEFORE issuing any HTTP request. The HTTP client is structurally INCAPABLE of producing tx bytes itself — it only TRANSMITS bytes given to it. The check happens in code, BEFORE any network I/O, BEFORE any URL is opened, so empty-`txs` cannot leak into URL/headers/logs.
  - **New error variant (R-E1)**: `RelaySimError::UnsignedBundleUnavailable` is added to the P4-D `RelaySimError` enum. This is a **narrowly-scoped P4-E carve-out on the P4-D-frozen `crates/relay-sim` surface** (additive variant; `#[non_exhaustive]` was deliberately set in P4-D to anticipate exactly this kind of growth). The variant carries no payload (no detail string that could leak secrets — see DP-E11 / R-E4). Documented in `crates/relay-sim/src/lib.rs` as the canonical "P4-E policy/request-construction failure" variant; classified by `compare_result` as `MismatchCategory::Unknown` (existing logic — `Unknown` matches any `RelaySimError` variant by definition, so no comparator change needed).
  - **Production behavior is HONEST**: real Flashbots/bloXroute call from `comparator_driver` sees `RelaySimRequest::txs` as empty (no signer exists in P4-E to populate it); adapter's `simulate_bundle` short-circuits with `Err(UnsignedBundleUnavailable)` before any network I/O; comparator classifies via `compare_result(_, Err(&UnsignedBundleUnavailable))` → `MismatchCategory::Unknown` → `MismatchAbort` journaled per DP-E8.
  - **CW tests** use `wiremock` for happy-path (with non-empty `txs` populated by test fixtures) AND for the `UnsignedBundleUnavailable` short-circuit (asserts `wiremock` server received ZERO requests when `txs.is_empty()`).
  - **This is CORRECT per ADR-006**: zero-tolerance abort > silent skip > silent network call with bogus payload.
- **DP-E5 (`SimulationOutcomeWithFingerprint` envelope)**: the comparator needs `LocalStateFingerprint` alongside `SimulationOutcome`. P4-D shipped `LocalSimulator::simulate_with_fingerprint` returning a tuple but the existing `simulator_driver` (in `crates/app::wire_phase4`) calls `simulate(...)` and emits only `SimulationOutcome`. P4-E REPLACES that driver call with `simulate_with_fingerprint(...)` and broadcasts a NEW envelope type `SimulationOutcomeWithFingerprint { outcome: SimulationOutcome, fingerprint: LocalStateFingerprint }`. The existing `sim_tx` broadcast type changes from `EventEnvelope<SimulationOutcome>` → `EventEnvelope<SimulationOutcomeWithFingerprint>`. **This is a TYPE CHANGE on a Phase-3-frozen surface** — narrowly scoped P4-E carve-out documented here. The downstream `execution_driver` reads `.outcome` from the new envelope (one-line edit). FP-1 in P4-D already proved parity, so swapping `simulate` → `simulate_with_fingerprint` is safe.
- **DP-E6 (REVISED v0.5 per R-E19) — object-safety of `BundleRelay`**: `BundleRelay` is kept object-safe (a) so the concrete-adapter `submit_disabled` test in `crates/relay-clients/tests/submit_disabled.rs` can exercise the trait method without import gymnastics, and (b) so Phase 5+ submission code can introduce `Arc<dyn BundleRelay>` without re-shaping the trait. **However, P4-E producer/comparator wiring NEVER holds an `Arc<dyn BundleRelay>` trait object** — `crates/app` constructs each adapter concretely and immediately stores it as `Arc<dyn RelaySimulator>` per DP-E13 v0.3. The object-safe trait shape exists at the type level for downstream evolvability; it is NOT used by any P4-E call site.
  - Trait mechanics (carried from v0.4): `async fn` sugar via `async_trait` (workspace dep); bounds `Send + Sync + 'static` per the existing `RelaySimulator` precedent; `name(&self)` is sync (used for tracing labels).
  - Verification at batch close: `grep -RIn 'Arc<dyn BundleRelay>\|dyn BundleRelay' crates/app/` → 0 matches (CW-4 v0.5 covers).
- **DP-E7 (sim + submit on same trait vs split)**: KEEP on the same trait. Rationale: relays are physical endpoints that BOTH sim and submit; an adapter would always implement both; splitting would create a permanent type-bound proliferation (the wiring would need both `Vec<Arc<dyn RelaySimulator>>` AND `Vec<Arc<dyn BundleRelay>>` and have to dedupe). The fail-closed `SubmitDisabled` invariant + the no-caller invariant + the config-validation guard provide three independent layers; trait split would give zero additional safety on top.
- **DP-E8 (REVISED v0.4 per R-E14 + R-E15 + R-E16 + R-E17)** — `MismatchAbort` journaling schema, concretely:
  - **Record type**: `MismatchAbort` already derives `rkyv::Archive + rkyv::Serialize + rkyv::Deserialize + serde::Serialize + serde::Deserialize` per P4-D §CMP-9 (which proved rkyv round-trip). No new derive needed.
  - **Journal type**: `FileJournal<MismatchAbort>`.
  - **Real `FileJournal` API (R-E14 v0.4 correction)**: verified at `crates/journal/src/journal.rs:215`, the actual signature is:
    ```rust
    pub fn append(&mut self, envelope: &EventEnvelope<T>) -> Result<JournalPosition, JournalError>
    ```
    `FileJournal::append` does NOT take a `PublishMeta` and does NOT construct an envelope internally. The CALLER is responsible for constructing the `EventEnvelope<T>` (via `EventEnvelope::seal(...)` or the local `seal_envelope(...)` helper in `crates/app/src/lib.rs`) and passing it BY REFERENCE. v0.3's wording around "FileJournal<T> internally wraps T in EventEnvelope<T>" / "journal.append(meta, &mismatch_abort)" / "FileJournal::append constructs internally" was wrong; v0.4 corrects it.
  - **Envelope metadata** (the caller constructs the envelope before calling `append`):
    - `EventSource = EventSource::Relay` (existing P1 variant; first concrete use; grep-confirmed at close).
    - `event_version = 1` (Phase 1 invariant; non-zero).
    - `chain_context` (`chain_id` + `block_number` + `block_hash`) — threaded VERBATIM from the upstream `BundleCandidate` envelope so post-mortem replay can re-derive the opportunity context.
    - `correlation_id` — same as the upstream `BundleCandidate` envelope (mismatch records grep-link to the opportunity → risk → sim → bundle chain).
    - `sequence` + `timestamp_ns` — supplied by the comparator_driver's local sequence counter + `SystemTime::now()`-derived `timestamp_ns`, mirroring the existing `seal_envelope(source, payload, &mut seq)` helper precedent in `crates/app/src/lib.rs`. The driver uses an analogous local helper that also threads through the upstream `chain_context` + `correlation_id`.
  - **Journal file**: NEW `journal.mismatch_journal_path` config field, default `data/mismatch.bin`. Validation: must be non-empty (reuses existing `EmptyRequiredField` logic from P4-A `archive_rpc.url`). Empty → `ConfigError::Validation` (CFG-MISMATCH-JOURNAL-1 covers).
  - **Synchronous-append guarantee (R-E9 + R-E17 v0.4 — mechanism PINNED)**: `comparator_driver` itself owns the `FileJournal<MismatchAbort>`. **There is no `mismatch_journal_task`; no async drain; no spawn_blocking; no block_in_place.** On every abort path the async driver task:
    1. Constructs the `EventEnvelope<MismatchAbort>` inline via the local seal helper (sets `EventSource::Relay`, threads upstream `chain_context` + `correlation_id`, supplies `sequence` from the driver's counter + `timestamp_ns` from `SystemTime::now()`).
    2. Calls `journal.append(&envelope)?` synchronously (this is a `&mut self` call that performs blocking disk I/O directly inside the async task; `FileJournal::append` is NOT itself async, but the operation is bounded — single-record append + 4-byte length-prefix + CRC32 — and well within the driver's per-candidate latency budget).
    3. Calls `journal.flush()?` synchronously (Phase 1 journal contract: flush ≠ fsync, but the OS buffer is pushed).
    4. ONLY THEN emits any downstream broadcast (e.g., `mismatch_tx` for observability subscribers).
  - **R-E17 rationale**: `spawn_blocking` would force serialization of every mismatch through a separate task pool + an extra await point, complicating ordering analysis without measurable benefit. `block_in_place` only works on multi-thread runtimes and would couple the design to that runtime flavor. Inline blocking-IO inside the async task is correct here because: (a) the workload is a single bounded-size disk write, (b) the driver does NOT hold any other future awaiting concurrent progress, (c) the same pattern is used by the existing P3-F `journal_drain_loop` (which is also sync-IO inside an async task). If profiling at P4-G shows the inline block adversely affects fairness on the runtime, P4-G can revisit; for P4-E the inline-sync choice is the simple correct default.
  - **Replay/audit guarantee** (R-E9 + R-E14 + R-E17): the `journal.append + flush` pair completes BEFORE any other code observes the mismatch. No drain task; no scheduling step can reorder the journal write past a downstream observer. **CW-2 v0.4** asserts this ordering by reading the journal file BEFORE checking any broadcast subscriber for the abort signal.
- **DP-E11 (NEW v0.2 per R-E4)** — secret-redaction extended scope:
  - URL query tokens, auth headers, and API keys MUST NOT appear in:
    - `tracing::*` macro arguments (whether by `%url`, `?url`, structured field, or string formatting).
    - `RelaySimError` variants' inner strings (`Transport(String)`, `UnrecognizedResponse(String)`). `NotConfigured` and `UnsignedBundleUnavailable` are payload-free so cannot leak by construction.
    - `MismatchAbort.detail` (already populated by `compare`/`compare_result` in P4-D — reaffirmed P4-E does not regress this).
    - Journal payloads (the `MismatchAbort` written to `mismatch.bin` is rkyv-serialized; its fields are already secret-free per P4-D, but P4-E adds a redaction test).
  - **Mechanism**: secrets (URLs + API keys) are loaded from config exactly ONCE at adapter construction time, stored in private `String` fields on `FlashbotsRelay` / `BloxrouteRelay`, and NEVER re-emitted into ANY user-facing string. The HTTP client (alloy/reqwest) opens the URL by passing a parsed `Url` directly to the transport; the URL string never goes through `format!` or `tracing::*`.
  - **Test**: `RC-COMMON-2` (NEW) constructs a `FlashbotsRelay` against `http://example.com/?token=SECRETTOKEN` (URL with embedded credentials) + a programmed bloXroute API key `SECRETKEY`; drives a sequence of: happy-path call, transport-error call, unsigned-bundle call, malformed-response call. Asserts:
    - `format!("{:?}", flashbots_adapter)` does NOT contain `SECRETTOKEN` (Debug impl manually elides URL field).
    - The returned `RelaySimError::Transport(s)` from the failure-path call has `s` not containing `SECRETTOKEN` or `SECRETKEY`.
    - The `MismatchAbort.detail` from the resulting `compare_result` Unknown classification does not contain either secret.
    - `tracing` log output captured via `tracing_subscriber::fmt::TestWriter` does not contain either secret.
    - Journal file bytes (rkyv-encoded `EventEnvelope<MismatchAbort>`) do not contain either secret. (Brute byte-grep on the journal file after the test publishes one mismatch.)
  - Adapter's `Debug` impl is hand-written: `f.debug_struct("FlashbotsRelay").field("name", &self.name).finish_non_exhaustive()` — explicitly omits URL + API-key fields. Same for bloXroute.
- **DP-E12 (NEW v0.2 per R-E5)** — comparator_driver hard wiring invariant:
  - The `comparator_driver` task has EXACTLY TWO terminal outcomes per inbound `BundleCandidate`:
    1. **`compare_result(...) == Ok(())`** → emit `MismatchCheckPassed { candidate, relay_outcome }` envelope on `comparator_tx` broadcast. The downstream subscriber list of `comparator_tx` in P4-E is **EMPTY** (the type exists for Phase 5 to wire submission; in P4-E nothing reads it). The task continues to the next candidate.
    2. **`compare_result(...) == Err(MismatchAbort)`** OR **relay sim returned `Err(_)` (any variant including `UnsignedBundleUnavailable` / `Transport` / `NotConfigured` / `UnrecognizedResponse`)** → wrap into `EventEnvelope<MismatchAbort>` per DP-E8 + write to journal + (optionally) emit to a `mismatch_tx` broadcast for observability subscribers. The task continues to the next candidate.
  - **NO third path exists.** No code in `comparator_driver` (or anywhere downstream of it in P4-E) can call `BundleRelay::submit_bundle`.
  - Verified at batch close by:
    - **CW-3** (kept from v0.1, strengthened): a ripgrep test asserts `grep -RIn 'submit_bundle(' crates/app/` returns 0 matches AND `grep -RIn 'submit_bundle(' crates/bundle-relay/src/` returns ONLY the trait method declaration line (no calls).
    - **CW-4 (REVISED v0.5 per R-E19)** unit test on `comparator_driver`: drives a candidate; mock returns each of `Ok(...)` / `Err(UnsignedBundleUnavailable)` / `Err(Transport(...))` / `Err(UnrecognizedResponse(...))` / `Err(NotConfigured)` / `Err(MismatchAbort{...via compare()})`; asserts every case ends at either `MismatchCheckPassed` (only Ok) or a journaled `MismatchAbort`. The driver does NOT hold a `BundleRelay` trait object at all; it holds only `Arc<dyn RelaySimulator>` (DP-E13 v0.3 + DP-E6 v0.5). Verified by two independent gates: (a) the driver's constructor signature accepts `Arc<dyn RelaySimulator>` (compile-time / CW-5 type-system test), and (b) `grep -RIn 'Arc<dyn BundleRelay>\|dyn BundleRelay' crates/app/` → 0 matches at batch close.
- **DP-E13 (REVISED v0.3 per R-E10)** — `comparator_driver` constructed with `Arc<dyn RelaySimulator>` from the start (NO trait-object upcast):
  - **Why no upcast**: trait-object upcast (`Arc<dyn BundleRelay>` → `Arc<dyn RelaySimulator>`) is unstable on Rust 1.80 stable (the `trait_upcasting` feature stabilized at 1.86). The workspace pins `rust-version = "1.80"`. Using upcast would force a toolchain bump or a fragile workaround — neither is acceptable as a P4-E carve-out.
  - **Mechanism**: `wire_phase4` constructs each adapter via its **concrete type** (`FlashbotsRelay::new(cfg)` → `FlashbotsRelay`, `BloxrouteRelay::new(cfg)` → `BloxrouteRelay`). The concrete type implements BOTH `RelaySimulator` AND `BundleRelay`. `wire_phase4` then constructs the `Arc<dyn RelaySimulator>` for `comparator_driver` directly: `let relay_sim: Arc<dyn RelaySimulator> = Arc::new(FlashbotsRelay::new(...));`. The `dyn BundleRelay` form is NEVER constructed in `crates/app` in P4-E — the trait exists for Phase 5+ submission code to consume; P4-E only consumes the simulator surface.
  - **Layered safety in P4-E** (4 independent layers):
    1. Every `BundleRelay::submit_bundle` impl returns `Err(SubmitDisabled)` unconditionally.
    2. `wire_phase4` never constructs `Arc<dyn BundleRelay>` — the trait object form does not exist in the wiring.
    3. `comparator_driver`'s parameter type is `Arc<dyn RelaySimulator>` (CW-5 verifies the constructor signature). The driver literally cannot name `submit_bundle` because it's not on the trait it holds.
    4. `crates/app` 0-callers grep gate (CW-3) catches any future PR that tries to add a `submit_bundle` call site anywhere in the wiring.
  - **Submit-disabled invariant test (R-E10)**: a separate test (`crates/relay-clients/tests/submit_disabled.rs`) constructs each concrete adapter (`FlashbotsRelay` + `BloxrouteRelay`), calls `submit_bundle` directly on the concrete type with a dummy `SignedBundle`, asserts `Err(SubmitDisabled)`. This is the explicit-wrapper-style verification Codex asked for; does not depend on trait upcast.
- **DP-E9 (live_send config field)**: ADD `relay.live_send: bool` config field, **default `false`**. Config validation REJECTS `true` with a hard `ConfigError::Validation("relay.live_send=true is forbidden until Phase 6b Production Gate")`. `live_send` is NOT plumbed to any code path in P4-E — even reading the field's value would be premature. The validation reject IS the only mechanism. Q8(d) grep gate at batch close: `grep -RIn 'live_send' crates/` → matches only the config struct definition + the validation reject + the test that the validation reject fires + this plan doc. Zero usages in submit code paths.
- **DP-E10 (kill switch — execution-safety.md §"Kill Switch")**: ADD `relay.execution_disabled: bool` config field, default `false`. Reading is permitted in P4-E (safe; pulling a kill switch is a valid runtime concern), but since no submission exists in P4-E the field is a no-op for now. Documented as the runtime-toggle mechanism that Phase 5+ will check before any submit. Default `false` keeps existing behavior.

## Test matrix summary

| Bucket | Tests | Location |
|---|---|---|
| BR (D-E1 trait + types) | BR-1, BR-2, BR-3 | `crates/bundle-relay/src/lib.rs` cfg(test) |
| RS-NEW (R-E1 carve-out variant) | RS-N-1 (`UnsignedBundleUnavailable` Display message stable + classified by `compare_result` as `MismatchCategory::Unknown` with `MismatchAbort.relay = None`). NO rkyv/serde round-trip — `RelaySimError` is NOT a journal payload (R-E11 fix). | `crates/relay-sim/src/lib.rs` cfg(test) (additive) |
| RC-F (D-E2 Flashbots) | RC-F-1..4 + RC-F-5 (R-E2 empty-txs short-circuits BEFORE network I/O — wiremock asserts 0 requests) | `crates/relay-clients/src/flashbots.rs` cfg(test) + `crates/relay-clients/tests/flashbots.rs` |
| RC-B (D-E2 bloXroute) | RC-B-1..4 + RC-B-5 (R-E2 empty-txs short-circuit) | `crates/relay-clients/src/bloxroute.rs` cfg(test) + `crates/relay-clients/tests/bloxroute.rs` |
| RC-COMMON | RC-COMMON-1 (grep gate at close) + RC-COMMON-2 (R-E4 secret redaction across Debug + RelaySimError + MismatchAbort.detail + tracing log + journal bytes) | `crates/relay-clients/tests/redaction.rs` |
| CW (D-E3 + DP-E12 + DP-E13 wiring) | CW-1 (happy), CW-2 (mismatch path journals), CW-3 (grep gate: zero `submit_bundle` callers in `crates/app`), CW-4 (R-E5: every relay error variant + every comparator outcome ends at journaled abort or `MismatchCheckPassed`; never submit), CW-5 (DP-E13: `comparator_driver` holds `Arc<dyn RelaySimulator>`, not `Arc<dyn BundleRelay>` — type-system guarantee) | `crates/app/tests/comparator_wiring.rs` |
| Config | CFG-LIVE-SEND-1 (`live_send=true` rejected) + CFG-RELAY-1 (relay endpoints parse from TOML) + CFG-MISMATCH-JOURNAL-1 (`mismatch_journal_path` empty rejected per DP-E8) | `crates/config/src/lib.rs` cfg(test) |
| **Total** | **3 + 1 + 5 + 5 + 2 + 5 + 3 = 24** | workspace 162 + 24 = **186 passed + 1 ignored** target |

Lean per Phase 2-onward policy. Happy + error variant per HTTP failure mode + the load-bearing guarantees (SubmitDisabled, secret-redaction, live_send-reject).

## Q8 hardening invariants (verified at batch close)

- (a) **No funded key**: `grep -RIn 'Signer\|Wallet\|PrivateKey\|secp256k1\|sign_transaction\|sign_tx' crates/bundle-relay/ crates/relay-clients/ crates/app/src/lib.rs` → 0 matches.
- (b) **No signing infra**: `SignedBundle::signed_txs` is only written by tests + the `MockRelaySimulator`. `grep -RIn 'eth_sendBundle' crates/` → 0 matches.
- (c) **No `submit_bundle` caller**: `grep -RIn 'submit_bundle(' crates/app/ crates/bundle-relay/src/lib.rs (caller side, excluding the trait def + the per-adapter impls in crates/relay-clients/)` → 0 matches.
- (d) **No `live_send=true` capability**: `grep -RIn 'live_send' crates/` matches only the config struct + the validation reject + the test that asserts the reject fires.
- (e) **No live tests in CI**: `grep -RIn '#\[ignore\]\|#\[tokio::test.*ignore' crates/relay-clients/ crates/bundle-relay/` returns ONLY the env-gated stubs (none added in P4-E; live HTTP testing is Phase 5).
- (f) **No `tracing::*` macros that take URL or API-key arguments**: dedicated grep gate `grep -RIn 'tracing::.*url\|tracing::.*api_key\|tracing::.*endpoint=' crates/relay-clients/ crates/bundle-relay/` → 0 matches.
- (g) **Fail-closed defaults**: `relay.live_send` default `false` (rejected if `true`); `relay.execution_disabled` default `false`; `relay.enabled_relays` default `Vec::new()` (empty → comparator inert per DP-E3).
- (h) **No crate cycles**: `cargo tree` shows `bundle-relay → relay-sim → simulator`; `relay-clients → bundle-relay + relay-sim`; `app → relay-clients + bundle-relay + relay-sim`. No reverse edges. Verified `cargo tree -p rust-lmax-mev-relay-sim -i rust-lmax-mev-bundle-relay` errors out.
- (i) **`submit_bundle` invariant test**: BR-3 asserts the `SubmitDisabled` Display message contains "Phase 5 Safety Gate" so a future PR that loosens the invariant is forced to also update test text + spec docs.
- (j) **Comparator-driver type-system guarantee** (DP-E13): `comparator_driver` accepts `Arc<dyn RelaySimulator>` only; `submit_bundle` is not in that trait. CW-5 verifies the constructor signature.
- (k) **Secret-redaction extended scope** (DP-E11): RC-COMMON-2 asserts that secrets do not appear in Debug, error strings, MismatchAbort.detail, tracing log, or journal bytes.
- (l) **R-E2 fail-closed pre-check**: RC-F-5 + RC-B-5 assert that `simulate_bundle` with empty `txs` short-circuits BEFORE any network I/O (wiremock receives 0 requests).
- (m) **R-E20 residue grep gate**: `rg 'interchangeably|submit-only relays|downstream relay-sim consumers can hold' crates/` → 0 matches. Catches any future PR that re-introduces the misleading trait-object-upcast / submit-only-relay framings.

## Forbids (carry from prior + reaffirmed for P4-E)

- No `eth_sendBundle` ANYWHERE.
- No funded key, no production signer, no signing that yields a submittable tx, no key derivation.
- No `live_send = true` (default false; config validation rejects true; no read site exists).
- No relay submission. Every `submit_bundle` impl returns `Err(SubmitDisabled)` unconditionally.
- No live-network tests in CI (`wiremock`-based only; env-gated live tests deferred to Phase 5).
- No edits to `crates/state` / `crates/risk` / `crates/opportunity` / `crates/execution` / `crates/state-fetcher` / `crates/node` / `crates/ingress` / `crates/event-bus` / `crates/journal` / `crates/observability` / `crates/types` / `crates/simulator`.
- `crates/config` edit narrowly scoped to: `RelayConfig` struct (NEW) + `JournalConfig.mismatch_journal_path` (NEW) + the `live_send=true` validation reject. No other schema changes.
- `crates/app::wire_phase4` body edit narrowly scoped per DP-E2: simulator driver call swaps `simulate` → `simulate_with_fingerprint` (DP-E5); 2 new tasks spawned; 1 new broadcast sender; existing tasks unchanged in semantics. Existing wire_phase2/wire_phase3 byte-identical.
- `crates/relay-sim` body edit: NARROWLY SCOPED to (a) adding `RelaySimError::UnsignedBundleUnavailable` variant per R-E1 (P4-D `#[non_exhaustive]` was set deliberately to anticipate this), and (b) RS-N-1 test for the variant. No other edits. The added variant carries no payload (no string field that could leak secrets per DP-E11).
- No new ADR (operates under existing ADR-003 + ADR-006 + execution-safety.md).
- No tag (Phase 4 sub-batches do not tag; only `phase-4-complete` at P4-G).
- No destructive git, no force push, no `.claude/` / `AGENTS.md` staging, no committing `fixture_output.txt`.

## Process

Per the 2026-05-04 routine-closeout policy + 2026-05-04 22:20 KST manual-Codex-review-mode:

1. Claude commits this v0.1 draft + writes the v0.1 review pack to `.coordination/claude_outbox.md`.
2. Claude STOPS and reports "manual Codex review required".
3. User pastes pack to Codex; relays verdict.
4. Claude records verdict in `.coordination/codex_review.md`.
5. APPROVED → impl + tests + batch-close gates + commit + push per routine policy. No tag.
6. REVISION REQUIRED → revise + re-emit.
7. Scope/ADR change required → HALT to user.

No code or `Cargo.toml` edits in this turn. The plan doc is on disk uncommitted.

## Honest closeout claim P4-E will make at batch-close

> **P4-E ships the `BundleRelay` trait + Flashbots/bloXroute HTTP adapters wired for read-only `eth_callBundle` + the relay-sim → comparator → mismatch-journal pipeline.** It does NOT enable live submission: every `submit_bundle` impl returns `Err(SubmitDisabled)` unconditionally; no caller invokes `submit_bundle` in `crates/app` (CW-3 grep gate); `comparator_driver` is constructed with `Arc<dyn RelaySimulator>` from the START (not via trait-object upcast — see DP-E13 v0.3) so `submit_bundle` is not type-reachable from the driver; config validation rejects `live_send=true`. Real `eth_callBundle` against a relay endpoint short-circuits with `Err(UnsignedBundleUnavailable)` BEFORE any network I/O because `RelaySimRequest::txs` is empty (P4-E has no signer to populate it); the comparator classifies these as `MismatchCategory::Unknown` and the abort is journaled SYNCHRONOUSLY by the `comparator_driver` itself before any further bus emission (DP-E9 v0.3 fix). Live signing infrastructure + Flashbots auth header + actual relay submission land at Phase 5 Safety Gate.

## Open questions for Codex

- **Q-E1**: Is `crates/bundle-relay` + `crates/relay-clients` (two new crates) the right split, or should they be one crate `crates/relay`? Two-crate split rationale: `bundle-relay` is the trait surface (small, stable, no HTTP deps); `relay-clients` is the impl surface (alloy + reqwest + wiremock dev). Single-crate counter: simpler dep tree. Draft chooses two-crate split for layered-dep clarity; happy to merge if Codex disagrees.
- **Q-E2**: DP-E2 (extend `wire_phase4` in place) vs introduce `wire_phase5_pre_safety`? Draft chooses extend-in-place to avoid double-maintenance through P4-F + P4-G; this DOES touch a Phase-3-shipped surface additively. If Codex prefers the new constructor for cleaner archeology, we can take the maintenance cost.
- **Q-E3**: DP-E5 changes the `sim_tx` broadcast type from `SimulationOutcome` to `SimulationOutcomeWithFingerprint`. This is a downstream-consumer breaking change inside `crates/app` (one-line edit on the `execution_driver`). Acceptable as an additive-spirit P4-E carve-out, or should the pipeline grow a parallel `fingerprint_tx` instead? Parallel-channel counter: more shutdown ordering complexity; harder to keep fingerprint+outcome in sync per opportunity.
- **Q-E4**: DP-E4 honest claim — every real `eth_callBundle` call in P4-E short-circuits with `Err(UnsignedBundleUnavailable)` before any network I/O because `txs` is empty (no signer in P4-E). Should P4-E even ship the Flashbots/bloXroute adapters at all, or defer to P4-G (when Phase 5's signer is closer)? Draft ships them anyway because the HTTP shape + JSON parsing + secret redaction + timeout machinery is real engineering work that's testable now via `wiremock` and the wiring's structure depends on the trait/adapter shapes. Deferring would force P4-E into "trait + types only" territory.
- **Q-E5**: Should the `MismatchAbort` journal be a SEPARATE file (`mismatch.bin`) or appended to an existing journal? Draft uses a separate file (DP-E8) for clean post-mortem grep + bounded retention. If Codex wants single-file journaling with a tag/discriminator, that's a P4-D-era design call we'd need to revisit.
- **Q-E6**: `wiremock` is the test HTTP server. Acceptable workspace dep (dev-dep on `crates/relay-clients` only) or prefer a hand-rolled mock?
