# Phase 1 Batch C — Smoke Tests + CI Execution Note

**Date:** 2026-05-02
**Status:** Draft v0.3 (revised after Codex 2026-05-02 16:43:55 +09:00 HIGH-confidence REVISION REQUIRED — B-1 cleanup-on-timeout path is now non-deadlocking by running the same full drain as the success path before panicking, instead of attempting a single-slot drain that cannot unblock 99,936 pending publishes)
**Scope:** Tasks 17 (integration smoke tests) + 18 (CI workflow + `deny.toml` refresh).
**Predecessor:** Batch B closed at `2b82272` (Codex APPROVED 2026-05-02 16:30:10 +09:00 MEDIUM).
**Authoritative sources:** ADR-005 (bus capacity + backpressure), ADR-008 (CI baseline — 7 checks defined verbatim), CLAUDE.md (test policy: "Backpressure test must be fully implemented (not stub)"), the four frozen `docs/specs/` documents. No detailed batch spec is produced; this note IS the planning artifact.

## Scope

Two passive infrastructure deliverables that complete the Phase 1 quality bar:

- **Task 17** — three integration smoke tests in a dedicated `crates/smoke-tests` crate so the frozen crates (`types` `e2911cf`, `event-bus` `bb2e020`, `journal` `9c81e27`) stay frozen at source level. The smoke tests cover ADR-008 checks 5 (bus 100k + backpressure), 6 (journal round-trip), and 7 (snapshot smoke).
- **Task 18** — `.github/workflows/ci.yml` with the 7 ADR-008-mandated checks, and a `deny.toml` refresh against the cargo-deny 0.14+ schema (`vulnerability` / `unmaintained` / `unlicensed` keys were renamed to `version` / `severity` / `unlicensed` in newer schemas).

No real producer pipeline (Phase 3), no Grafana dashboards (Phase 5), no multi-OS matrix (Phase 4). CI runs on `ubuntu-latest` only.

## Source References

- **ADR-005** — Single-consumer crossbeam bounded; bus smoke must verify backpressure (publish blocks → unblock when consumer drains).
- **ADR-008** — Defines the 7 CI checks verbatim: fmt, clippy `-D warnings`, test, `cargo deny check`, bus smoke (100k events), journal round-trip, snapshot smoke. "All 7 must pass before merge." Also: "checks 5–7 are initially stubs that always pass" — but CLAUDE.md overrides for the bus check ("Backpressure test must be fully implemented (not stub)"); journal + snapshot checks are exercised end-to-end too since both crates are now implemented.
- **CLAUDE.md** — `tempfile = "3"` dev-dep convention; PowerShell `git commit -F` form; `task-X-complete` tag = user only.
- **Existing `deny.toml`** — committed at Task 10 scaffold (`5691b66`); may need a schema refresh for cargo-deny 0.14+.

## Public API Sketch

`crates/smoke-tests` is a test-only crate (no `pub` API; only integration tests under `tests/`):

- `tests/bus_smoke.rs` — 100k events through `CrossbeamBoundedBus<SmokeTestPayload>` with backpressure verified.
- `tests/journal_round_trip.rs` — N events written via `FileJournal::append`, reread via `iter_all`, bit-exact equality assertion.
- `tests/snapshot_smoke.rs` — `RocksDbSnapshot::save` / `load` round-trip on a synthetic key + bincoded payload.

`crates/smoke-tests/src/lib.rs` is a near-empty crate-level docstring file (cargo requires a lib or bin target).

`.github/workflows/ci.yml` job graph (single workflow, parallel where possible):

| # | Job | Command |
|---|---|---|
| 1 | fmt | `cargo fmt --check` |
| 2 | clippy | `cargo clippy --workspace --all-targets -- -D warnings` |
| 3 | test | `cargo test --workspace` |
| 4 | deny | `cargo install cargo-deny --version 0.16 --locked` then `cargo deny check` |
| 5+6+7 | smoke | included in (3) since smoke tests are `#[test]` functions in `crates/smoke-tests/tests/` |

Jobs 1-4 run on `ubuntu-latest` with `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2` for build caching, and `apt-get install -y clang libclang-dev` for `librocksdb-sys`'s `bindgen` precondition.

## Workspace + Per-Crate Dependency Deltas

Workspace `[workspace.dependencies]`:
- No new entries. All test deps come from already-present workspace entries (`rkyv`, `bincode`, `crossbeam-channel`, `tokio`, `metrics`).

Workspace `members` adds `"crates/smoke-tests"` after `"crates/app"`.

`crates/smoke-tests/Cargo.toml`:
- Runtime: empty (lib has no items; only tests). Optionally `tracing = { workspace = true }` if smoke tests need to log diagnostics; otherwise omitted.
- Dev: `rust-lmax-mev-types = { path = "../types" }`, `rust-lmax-mev-event-bus = { path = "../event-bus" }`, `rust-lmax-mev-journal = { path = "../journal" }`, `tempfile = "3"`, `bincode = { workspace = true }`, `serde = { workspace = true }`.

No new transitive C/C++ build dependencies (rocksdb is already pulled by `crates/journal`).

`.github/workflows/ci.yml`: new file. `deny.toml`: in-place edit (schema refresh).

## Risk Decisions

1. **Smoke tests live in dedicated `crates/smoke-tests`, NOT in `tests/` directories of frozen `event-bus` / `journal` crates.** Adding `tests/` files to a frozen crate IS a source-tree edit even if no `src/` files change, and the `task-13-complete` tag freezes that subtree. Dedicated crate keeps the frozen tags meaningful and isolates smoke tests in one location.
2. **CI on `ubuntu-latest` only for Phase 1.** Multi-OS matrix (windows-latest, macos-latest) is Phase 4 hardening per ADR-001's vertical-slice ordering. Adding it now would triple CI runtime + multiply cache-invalidation surface for no Phase 1 thin-path benefit.
3. **`cargo-deny` install pinned to an explicit version via `cargo install cargo-deny --version 0.16 --locked`.** The same `--version 0.16` is used in CI and locally so the gate behaves identically in both environments. `--locked` only forces use of cargo-deny's own bundled `Cargo.lock` (it does NOT pin to *this* project's resolved version — Codex 2026-05-02 16:37:11 corrected an earlier draft on this point). Drift risk: when cargo-deny ships a 0.17, the explicit `--version 0.16` keeps both CI and local runs deterministic until a follow-on commit bumps both call sites in lockstep. Direct install (rather than `EmbarkStudios/cargo-deny-action@v2`) eliminates a third-party action dependency and works identically; caching via `Swatinem/rust-cache@v2` covers the install cost on warm runs.
4. **Bus smoke = 100k events with DETERMINISTIC backpressure observation AND non-deadlocking cleanup** (per ADR-008 verbatim, per CLAUDE.md "not stub" override, per Codex 2026-05-02 16:37:11 + 16:43:55 substantive items). Mirrors the existing event-bus T2 test pattern (`crates/event-bus/src/lib.rs` lines ~480-541 — `publish_registers_backpressure_when_full_and_completes_after_recv`) with cleanup hardened for the 100k scale:
   1. Open `CrossbeamBoundedBus<SmokeTestPayload>` with capacity 64.
   2. Spawn a producer thread (`std::thread::spawn`) that publishes 100_000 envelopes sequentially; the first 64 succeed instantly, #65 blocks at the crossbeam send.
   3. The main thread polls `bus.stats().backpressure_total > 0` in a `loop { ... yield_now() ... }` with a 5-second deadline. Set a `timed_out: bool` on deadline expiry but DO NOT panic yet — fall through to step 4.
   4. Whether step 3 observed backpressure or timed out, spawn the consumer thread that drains via `EventConsumer::recv` until the channel closes. This unblocks the producer regardless of how many slots are full at the moment of timeout (single-slot drains are insufficient at 100k scale per Codex 2026-05-02 16:43:55).
   5. Join the producer thread (it will complete after publishing all 100_000 because the consumer is now draining); drop the main thread's bus reference so the channel can close after the producer's last publish; join the consumer thread (it exits when `recv` returns `Err(Closed)`).
   6. AFTER both joins succeed (so no thread leaks on the failure path), inspect `timed_out`:
      - If `timed_out == true`: panic with a clear message ("backpressure_total never reached >= 1 within 5s deadline") — the test fails cleanly without hanging.
      - If `timed_out == false`: assert `published_total == 100_000`, `consumed_total == 100_000`, `backpressure_total >= 1`, `current_depth == 0`. The `>= 1` lower bound (rather than `== 1`) accommodates additional backpressure events that may occur if the consumer falls behind the producer briefly during the 100k drain — those are also ADR-005-correct backpressure events.

   This design eliminates the v0.2 hang risk: the cleanup path no longer attempts a single-slot drain (which leaves 99,936 publishes still blocked); instead it runs the same full drain as the success path so the producer can always complete to 100k regardless of how many slots are full at the moment of the deadline. The consumer drain is itself non-blocking-on-empty-then-closed (`recv` returns `Err` once the producer drops its last sender), so neither join can hang.
5. **Journal round-trip = 1024 events** (much smaller than 100k). The journal writes 4-byte length + payload + 4-byte CRC per record; 100k events would balloon the test to multi-MB I/O for no extra coverage. 1024 events exercise the BufWriter flush boundary (default 8KB buffer) several times.
6. **Snapshot smoke = single save/load round-trip** on a 256-byte synthetic payload. The journal crate already has 5 dedicated `RocksDbSnapshot` tests (S-1..S-7); the smoke test exists for the CI-level "is RocksDB still working?" gate, not to replicate the unit coverage.
7. **`deny.toml` schema refresh defers to impl-time discovery.** If `cargo deny check` fails on the existing schema during the implementation pass, the `deny.toml` is updated in the same commit as `feat(smoke-tests)` or in a follow-on `chore(deny)` commit. License allowlist stays MIT / Apache-2.0 / BSD-2 / BSD-3 / ISC / Unicode-3.0 / Zlib unless transitive crates introduce new ones.

No frozen-decision changes, no new ADR, no crate-boundary changes, no persistence-format changes.

## Minimal Test Matrix (Risk-Based — 3 tests total)

`crates/smoke-tests` (3 integration tests, one binary each):
- **B-1 happy** `bus_smoke.rs::bus_handles_100k_events_with_backpressure` — capacity 64; producer thread publishes 100_000 SmokeTestPayloads; main thread polls `backpressure_total > 0` with 5-second deadline (sets `timed_out` flag, does NOT panic mid-flight); ALWAYS proceeds to spawn the consumer + drain so the producer can complete regardless of timeout state; joins both threads first; THEN panics on `timed_out` or asserts on success. Final assertions: `published_total == 100_000`, `consumed_total == 100_000`, `backpressure_total >= 1`, `current_depth == 0`. ADR-008 check 5. See Risk Decision 4 for the full non-deadlocking 6-step pattern.
- **B-2 happy** `journal_round_trip.rs::file_journal_appends_and_iters_back_1024_events` — open `FileJournal<SmokeTestPayload>` in tempdir, append 1024 envelopes (sequence 0..1024, mixed nonces), `flush`, drop, reopen via `FileJournal::open` at the same path, collect `iter_all` to Vec, assert length = 1024 and per-element field equality (sequence + payload). ADR-008 check 6.
- **B-3 happy** `snapshot_smoke.rs::rocksdb_snapshot_save_load_round_trip` — open `RocksDbSnapshot` in tempdir, save a synthetic struct (256-byte `Vec<u8>` payload) under key `b"smoke-key"`, load it back, assert byte equality. ADR-008 check 7.

Failure / boundary tests deferred: the unit suites in `crates/event-bus` (7 tests) and `crates/journal` (30 tests) already cover error paths exhaustively. Smoke tests verify integration-level happy paths only, per the lean policy.

Cumulative test count after Batch C: 49 (current) + 3 (smoke) = **52 workspace tests**.

## Verification Commands (Run at Batch Close, Not Per-Commit)

```powershell
cargo fmt --check
cargo build --workspace
cargo test --workspace                                # expect 52 passed
cargo clippy --workspace --all-targets -- -D warnings
cargo doc -p rust-lmax-mev-smoke-tests --no-deps

# cargo-deny gate, pinned to the same version as CI:
cargo install cargo-deny --version 0.16 --locked     # idempotent if already installed at 0.16
cargo deny check                                     # advisories + licenses + bans per deny.toml
# CI YAML lint: GitHub Actions schema is validated server-side on push;
# locally we rely on `actionlint` if installed (optional, not gating).
```

`cargo deny check` IS a Batch C close gate (per Codex 2026-05-02 16:37:11 substantive item 3). The execution note pins cargo-deny to `0.16` in both CI and local close gates so behavior is identical in both environments. If the local install fails (network, permission), the close report explicitly states the failure and the batch is NOT closed locally — verification falls back to CI evidence.

All PowerShell gates above must exit 0 before the batch is considered closed.

## Commit Grouping (Lean — 4 commits target)

1. **`docs: add Batch C tests + CI execution note`** — this file. Single docs commit BEFORE any code, mirroring Batch A `adad010` and Batch B `9ce85df`.
2. **`chore(workspace): scaffold crates/smoke-tests`** — workspace `Cargo.toml` (`members` += `"crates/smoke-tests"`), `crates/smoke-tests/Cargo.toml`, placeholder `src/lib.rs`. Compiles, no tests yet.
3. **`feat(smoke-tests): bus + journal + snapshot smoke (B-1..B-3)`** — three `tests/*.rs` files; full ADR-008 check 5/6/7 implementations.
4. **`feat(ci): add GitHub Actions workflow + refresh deny.toml`** — `.github/workflows/ci.yml` with the 4 jobs (fmt / clippy / test / deny); `deny.toml` schema-refreshed if needed for cargo-deny 0.14+.

Optional 5th commit `chore(batch-C): final fmt/clippy cleanup` only if the verification gate surfaces formatting drift.

All commits use `git commit -F <file>` form per the project PowerShell precedent. No tag creation in this batch; `phase-1-complete` belongs to Task 19 (Batch D).

## Forbidden (Reaffirmed)

- No `git push`, no `git tag` without explicit user approval (Batch C produces no tag of its own; `phase-1-complete` is Batch D / Task 19).
- No staging of `CLAUDE.md`, `AGENTS.md`, `.claude/`.
- No edits to `crates/types/**` (frozen at `e2911cf`), `crates/event-bus/**` (frozen at `bb2e020`), `crates/journal/**` (frozen at `9c81e27` = `task-13-complete`), `crates/observability/**` (frozen by Batch A close), `crates/config/**` (frozen by Batch B close), `crates/app/**` (frozen by Batch B close at `2b82272`).
- No multi-OS CI matrix in Phase 1 (Phase 4 hardening).
- No third-party `cargo-deny` GitHub Action (use direct `cargo install` per Risk Decision 3).
- No bus smoke that elides backpressure (CLAUDE.md "not stub" override of ADR-008's "initially stubs" sentence).
- No multi `-m` git commits in PowerShell.
- No detailed `docs/superpowers/specs/` document — this execution note is the sole planning artifact for Batch C per the policy.
