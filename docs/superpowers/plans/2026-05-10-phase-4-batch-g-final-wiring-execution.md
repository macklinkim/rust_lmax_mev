# Phase 4 Batch G — final wiring + Phase 4 DoD audit + `phase-4-complete` tag

**Date:** 2026-05-10 KST
**Status:** Final batch close. Per Phase 4 overview line 100 (P4-G: NO architectural risk), no pre-impl Codex review required.
**Predecessor:** P4-F closed + pushed at `d804d28` (Codex closeout NO ACTION HIGH 2026-05-10 KST).

## Scope

P4-G is the Phase 4 closeout. Per the overview table at line 87:

> P4-G | Final wiring (`wire_phase5_pre_safety`) + DoD audit + `phase-4-complete` tag | Connect relay-sim comparator into the existing P3-F driver chain; abort path exercised on mismatch; tag draft.

P4-E already extended `wire_phase4` IN PLACE (per the v0.6 plan §DP-E2 — extending `wire_phase4` was chosen over a new `wire_phase5_pre_safety` to avoid double-maintenance through P4-F + P4-G). Therefore P4-G's "final wiring" responsibility reduces to:

1. Audit the remaining wiring boundary.
2. Document the **fail-closed `LocalSimulator::prefetch_for` deferral** — production prefetch wiring stays Phase 5 work.
3. Run all Phase 4 safety grep gates + cycle gate.
4. Document the Phase 4 DoD per A..G.
5. Create + push the `phase-4-complete` annotated tag.

## Final wiring decision

### Production `LocalSimulator::prefetch_for` integration → DEFERRED to Phase 5 Safety Gate

**What `prefetch_for` would do**: per-opportunity, async-fetch source/sink pool state + WETH9 + USDC proxy + USDC impl + UniswapV2Factory accounts via the archive RPC, then load them into `LocalSimulator.fixtures` so `simulate_with_fingerprint` can run the real-revm path against fresh-per-block state.

**Why P4-G does NOT wire it**:

1. **Touches live mainnet on every event** — every inbound `RiskCheckedOpportunity` would trigger `~5 archive RPC calls` (state-fetcher per pool + 3 accounts). Archive RPC requires credentials, costs money, and contributes to provider rate limits. Wiring this on by default contradicts the standing forbid "no live paid API dependency in CI / runtime".
2. **Requires interior-mutability redesign of `LocalSimulator`** — current `prefetch_for(&mut self, ...)` consumes `&mut`; `simulator_driver` holds `Arc<LocalSimulator>` (shared with no other writer). Wiring would require `Arc<Mutex<LocalSimulator>>` or per-event clone-and-rebuild, both with non-trivial freshness/lifetime/contention semantics.
3. **Per-block fixture-cache invalidation policy** is unspecified at Phase 4 — Phase 5 owns the cache lifecycle (when to evict; whether to tee-fetch on block boundaries to amortize; per-pool freshness windows).
4. **The user-supplied directive explicitly authorizes this deferral**: "If wiring prefetch_for into the live driver would require credentials/live archive access or broaden runtime behavior unsafely, fail closed and document the exact remaining boundary."

**Fail-closed boundary (verified)**:

`wire_phase4` constructs `LocalSimulator::new(SimConfig::defaults())` which holds `fixtures: None`. For every inbound `RiskCheckedOpportunity` the existing `simulator_driver`:

1. calls `simulator.simulate_with_fingerprint(envelope.payload())`
2. receives `Err(SimulationError::Setup("no fixtures loaded; call load_fixture or prefetch_for first"))`
3. logs at `tracing::warn!` and continues to the next event

Cascading consequence:

- No `SimulationOutcomeWithFingerprint` envelope is ever emitted on `sim_tx`.
- `execution_driver` never runs `BundleConstructor::construct(...)` → no `BundleCandidate` → no `MismatchCheckPassed`.
- `comparator_driver` never invokes `RelaySimulator::simulate_bundle(...)` → no relay HTTP call → no `MismatchAbort` → `mismatch.bin` stays empty.
- `submit_bundle` is never reachable (it was already structurally unreachable via P4-E DP-E13's `Arc<dyn RelaySimulator>` upcast prevention + the four independent layers).

**P4-G code change**: extend the doc comment on `simulator_driver` to record the fail-closed boundary, the cascading consequence, and the explicit Phase 5 deferral. No semantic code change.

The next Phase 5 Safety Gate work (NOT P4-G) lands prefetch_for behind:
- a config-validated archive RPC endpoint (already declared in P4-A as `Option<FallbackRpcConfig>` defaulting to `None`),
- a wired `Arc<dyn StateFetcher>` from `crates/state-fetcher::ArchiveStateFetcher`,
- a redesigned `LocalSimulator` with interior-mutable fixture cache + per-block freshness contract.

### No new wiring constructor

P4-E §DP-E2 chose to extend `wire_phase4` IN PLACE rather than introduce `wire_phase5_pre_safety`. P4-G honors that choice; introducing a new constructor now would breach the "extend in place to avoid double-maintenance" rationale that P4-E committed to.

## Phase 4 DoD audit

### Per-batch completion table

| Batch | Status | HEAD | Deliverable verified |
|---|---|---|---|
| P4-A archive node | CLOSED | `e2b6704` | `NodeProvider` archive HTTP support + `eth_getProof`/`getCode`/`getStorageAt` + `archive_rpc: Option<FallbackRpcConfig>` config (default `None`); `ArchiveNotConfigured` fail-closed |
| P4-B state-fetcher | CLOSED | `856a859` | `ArchiveStateFetcher` + `UniswapV2Layout` + `UniswapV3Fee005Layout` + `fetch_account` API; LRU caches; storage-key derivation tests |
| P4-C1 simulator infra | CLOSED | `7efbb8d` | `StrictMissingDb` + `swap_calldata` + `cache_db_builder::build_prepared` + `signed_int_key`/`compress_tick` helpers |
| P4-C2 real-revm + ProfitSource flip | CLOSED | `74fcec8` | `LocalSimulator::simulate` real-revm path against recorded mainnet fixtures; `ProfitSource::RevmComputed` stamped on every outcome; SR-1 reaches `SimStatus::Success` deterministically |
| P4-D MismatchCategory + comparator infra | CLOSED | `4e32ab8` | `MismatchCategory` enum (carve-out in `crates/types`); `crates/relay-sim` `compare`/`compare_result`/`MismatchAbort` with deterministic precedence (CMP-10); `LocalStateFingerprint` + `RecordingDb` + `simulate_with_fingerprint` parity (FP-1) |
| P4-E BundleRelay + Flashbots/bloXroute + comparator wiring | CLOSED | `73cd41b` (after R-E1..R-E24 closeout) | `BundleRelay` trait + 2 HTTP adapters; `eth_callBundle` read-only; comparator wired into `wire_phase4`; `submit_bundle` always `Err(SubmitDisabled)`; `Arc<dyn RelaySimulator>` upcast prevention; multi-relay rejected; mismatch journal append+flush before broadcast; secret redaction across 5 surfaces; relay revert/error text fully dropped (R-E24) |
| P4-F Sushiswap + external mempool scaffold | CLOSED | `d804d28` | `PoolKind::SushiswapV2` additive; SushiV2 reuses V2 storage layout + V2 swap calldata + V2 caller path; cross-venue arb works through pool-kind-agnostic `pool_price_q64`; `ExternalMempoolSource` fail-closed with `IngressError::ExternalNotConfigured` (payload-free); `MempoolSourceKind` runtime selector |
| **P4-G final wiring + DoD + tag** | **THIS BATCH** | (this commit + tag) | Doc comment extension on `simulator_driver` documenting the fail-closed `prefetch_for` deferral; this DoD audit doc; full safety grep gates; `phase-4-complete` tag |

### Hard safety invariants — verified at HEAD `d804d28` + this commit

| # | Invariant | Verification | Result |
|---|---|---|---|
| 1 | No `eth_sendBundle` | `grep -RIn 'eth_sendBundle' crates/` — only doc-comment mentions of "NO eth_sendBundle" | ✅ clean |
| 2 | No signing infra (`Signer`/`Wallet`/`PrivateKey`/`secp256k1`/`sign_transaction`) | `grep -RIn 'Signer\|Wallet\|PrivateKey\|secp256k1\|sign_transaction' crates/` excluding doc lines | ✅ clean |
| 3 | No funded key | (subsumed by #2) | ✅ clean |
| 4 | No `live_send=true` capability outside config validation | `grep -RIn 'live_send' crates/` excluding `crates/config` schema + doc lines | ✅ clean |
| 5 | No relay submission wiring | (subsumed by #6 + #7 + #8) | ✅ clean |
| 6 | No `submit_bundle(` caller in `crates/app/src/` | `grep -RIn 'submit_bundle(' crates/app/src/` | ✅ 0 hits |
| 7 | No `Arc<dyn BundleRelay>` / `dyn BundleRelay` in `crates/app/src/` | `grep -RIn 'Arc<dyn BundleRelay>\|dyn BundleRelay' crates/app/src/` | ✅ 0 hits |
| 8 | No live-network tests in CI (no `#[ignore]`'d/live tests in adapters or ingress) | `grep -RIn '#\[ignore\]\|#\[tokio::test.*ignore' crates/relay-clients/ crates/bundle-relay/ crates/ingress/` | ✅ 0 hits |
| 9 | No secret leakage (URL/API-key/relay-text in `tracing::*`/`Error::Display`/`MismatchAbort.detail`/journal payload) | RC-COMMON-2 (5-surface) + R-E22 JSON-RPC body case + R-E24 hex-secret revert case + EXT-2 external-mempool secret elision | ✅ all assertions pass |
| 10 | External mempool fail-closed by construction | `ExternalMempoolSource` emits exactly one `IngressError::ExternalNotConfigured` and ends; no HTTP/WS connection opened (EXT-1 + EXT-2) | ✅ verified |
| 11 | Relay-sim mismatch aborts journal BEFORE broadcast (R-E9 ordering) | CW-2 reads journal file BEFORE checking broadcast subscriber | ✅ verified |
| 12 | Journal failure suppresses abort broadcast (R-E21) | CW-2-fail uses `FailingJournal` mock returning `Err(JournalError)` from append; asserts no `MismatchAbortRecord` arrives within 500ms | ✅ verified |
| 13 | Multi-relay config rejected (R-E23) | `Config::validate` rejects `relay.enabled_relays.len() > 1` with `TooManyEnabledRelays { count }` (CFG-RELAY-1 covers single-entry happy + multi-entry rejection) | ✅ verified |
| 14 | Doc-residue grep gate (R-E20) | `grep -RIn 'interchangeably\|submit-only relays\|downstream relay-sim consumers can hold' crates/` | ✅ 0 hits |
| 15 | No crate cycles | `cargo tree -p rust-lmax-mev-relay-clients -i rust-lmax-mev-bundle-relay` shows one-direction edge; reverse query errors with "did not match any packages" | ✅ no cycles |
| 16 | `relay.execution_disabled` kill switch present (read-side, no-op in P4-E since no submission exists) | Field exists in `RelayConfig`; default `false` | ✅ present (no submission to disable in P4-E) |

### Final wiring closeout claim (P4-G)

> **Phase 4 ships the comparator + adapter + multi-venue + state-fetcher infrastructure.** It does NOT enable live submission: every concrete-adapter `submit_bundle` returns `Err(SubmitDisabled)`; no `crates/app` caller invokes `submit_bundle`; `comparator_driver` holds `Arc<dyn RelaySimulator>` only; config validation rejects `live_send=true` AND `enabled_relays.len() > 1`. Real `eth_callBundle` against a relay endpoint short-circuits with `Err(UnsignedBundleUnavailable)` BEFORE any network I/O because `RelaySimRequest::txs` is empty (no signer). Relay-controlled error/revert text is fully redacted before reaching journalable payloads (R-E22 + R-E24). External mempool is fail-closed by construction (no HTTP/WS, no URL log). Production `LocalSimulator::prefetch_for` integration is deferred to Phase 5 Safety Gate; in P4-G the production simulator returns `Err(SimulationError::Setup)` on every event, suppressing the entire downstream chain (no `SimulationOutcomeWithFingerprint` → no `BundleCandidate` → no comparator → no journal entry → no submission attempt). Live signing infrastructure + Flashbots auth header + actual submission + per-block prefetch wiring all land at Phase 5 Safety Gate.

## Final gates run (HEAD `d804d28` + P4-G doc-comment edit)

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo build --workspace --all-targets` | clean |
| `cargo test --workspace` | **206 passed + 1 ignored** (no test count change in P4-G — doc-comment-only edit; lean per "avoid padding" directive) |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo deny check` | advisories ok, bans ok, licenses ok, sources ok |
| Cycle gate | clean (3 reverse-edge queries all error with "did not match any packages") |
| Safety grep gates 1–8 | all 0 hits (or only intentional doc mentions) |

## Tag

Per the standing tag policy (CLAUDE.md): `phase-4-complete` annotated tag created at the P4-G commit (this DoD audit doc + the simulator_driver doc-comment extension), then pushed.

Tag message body summarizes:
- Phase 4 batches A..G all closed
- ADR-001 line 43 revisit-trigger conditions held since P3 (P3-F broadcast tee + P4-D comparator + P4-E HTTP adapters)
- ADR-006 deferral resolved at P4-C2 (`ProfitSource::RevmComputed` flipped)
- Hard safety invariants 1..16 verified
- Production `prefetch_for` + signing + submission deferred to Phase 5 Safety Gate

## Process

1. Commit this DoD audit doc + the `simulator_driver` doc-comment extension as one routine doc/wiring close commit.
2. Push to origin.
3. Create annotated tag `phase-4-complete` at the P4-G commit.
4. Push the tag.
5. Report close per the user-supplied closeout template.

Per the 2026-05-04 routine-closeout policy: tag creation + push proceeds without user re-confirmation when (a) Codex APPROVED equivalent (P4-G has NO architectural risk per overview line 100 + no closeout review pending) AND (b) execution-note-documented target/scope (THIS doc).

## Honest scope-leak guard

P4-G **does NOT** introduce:
- Phase 5 Safety Gate work
- Funded key / signer / production submission
- `live_send=true` capability
- Live archive RPC dependency
- New asset pairs (only WETH/USDC)
- New V3 fee tiers
- Any code path that could reach `submit_bundle`

P4-G **DOES** add only:
- A doc comment on `simulator_driver` (no semantic change)
- This DoD audit doc
- The `phase-4-complete` annotated tag

Phase 5 work — including production `prefetch_for` wiring, signing infrastructure, real `eth_callBundle` with signed transactions, and live submission — is NOT started in P4-G and remains gated by the Phase 5 Safety Gate per ADR-001 + execution-safety.md.
