# Phase 1 Batch B — App (`crates/app`) Execution Note

**Date:** 2026-05-02
**Status:** Draft v0.1
**Scope:** Task 16 (`crates/app`) only. Per the user-adopted lean-batching policy 2026-05-02 KST, Batch A (foundation) and Batch B (app) are NOT merged — app wiring stays separate so foundation closure is reviewable independently.
**Predecessor:** Batch A closed at `587211f` (Codex APPROVED 2026-05-02 15:48:57 +09:00 MEDIUM).
**Authoritative sources:** ADR-001, ADR-003, ADR-005, ADR-007, ADR-008, CLAUDE.md, the four frozen `docs/specs/` documents. No detailed batch spec is produced; this note IS the planning artifact.

## Scope

A binary crate that wires the foundation crates into a runnable Phase 1 process:

- Parse a config path from the command line.
- Load `Config` (TOML + env overlay).
- Initialize observability (tracing + Prometheus) per `ObservabilityConfig`.
- Open `FileJournal<SmokeTestPayload>` and `RocksDbSnapshot` per `JournalConfig`.
- Create a `CrossbeamBoundedBus<SmokeTestPayload>` with capacity from `BusConfig` (added below).
- Spawn a consumer thread that drains the bus, journals each envelope, and exits cleanly on bus closure.
- Wait for `ctrl_c` (cross-platform), then drop the bus to close the channel, join the consumer, flush the journal.

Phase 1 explicitly does NOT include a real producer pipeline (the 6-stage `Pipeline<T>` is Phase 3 work per CLAUDE.md). The app skeleton is a wiring + lifecycle harness; it ends as soon as `ctrl_c` fires.

## Source References

- **ADR-001** — Vertical slice: Phase 1 is the thin end-to-end shell. The app's job is wiring + replay-hook readiness, not strategy.
- **ADR-003** — `FileJournal` + `RocksDbSnapshot` are paired; both are opened at startup.
- **ADR-005** — Single-consumer `crossbeam::channel::bounded`; capacity must come from config (not hardcoded). Domain events block on full; telemetry events drop with a counter.
- **ADR-007** — Node config (Geth WS + HTTP + ≥ 1 fallback) is loaded but NOT dialed in Phase 1; dialing is Phase 3.
- **ADR-008** — Observability initialized exactly once via `rust_lmax_mev_observability::init`.
- **CLAUDE.md** — Solo dev + AI agents; PowerShell `git commit -F` form; `tempfile = "3"` dev-dep convention; `task-X-complete` tags require explicit user approval.

## Public API Sketch

### Additive change to `crates/config` (small, in-batch)

```rust
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BusConfig {
    pub capacity: usize,        // ADR-005 mandates capacity-from-config
}

pub struct Config {
    pub node: NodeConfig,
    pub observability: ObservabilityConfig,
    pub journal: JournalConfig,
    pub bus: BusConfig,         // NEW
}
```

`Config::validate` gains one extra check: `bus.capacity >= 1` (capacity 0 would deadlock on first publish per crossbeam semantics).

This is the only edit to `crates/config` in Batch B and is purely additive at the struct surface; existing C-1..C-3 tests are extended by one field assertion (no new tests needed inside `crates/config`).

### `crates/app`

```rust
// crates/app/src/lib.rs
pub fn run(config_path: impl AsRef<Path>) -> Result<(), AppError>;

#[non_exhaustive]
pub enum AppError {
    Config(rust_lmax_mev_config::ConfigError),                     // #[from]
    Observability(rust_lmax_mev_observability::ObservabilityError),// #[from]
    Journal(rust_lmax_mev_journal::JournalError),                  // #[from]
    Bus(rust_lmax_mev_event_bus::BusError),                        // #[from]
    Io(std::io::Error),                                            // #[from]
    ConsumerJoin(String),                                          // thread join failure
}

// crates/app/src/main.rs
fn main() -> std::process::ExitCode { /* parse argv, call run(), map errors */ }
```

`run()` is the synchronous wiring entrypoint; `main` is a thin shell so integration tests can invoke `run()` directly without spawning a subprocess.

`ctrl_c` handling uses `tokio::signal::ctrl_c().await` inside a `#[tokio::main(flavor = "current_thread")]` runtime spawned for the lifetime of the wait — multi-thread is unnecessary for Phase 1 (no producer-side async work).

## Workspace + Per-Crate Dependency Deltas

Workspace `[workspace.dependencies]`:
- No new entries. `tokio`, `anyhow`, `tempfile`-equivalent (`tempfile = "3"` is per-crate dev-dep convention) are already present.

Workspace `members` adds `"crates/app"` after `"crates/observability"`; updates the trailing comment to drop the "added in Batch B" reservation.

`crates/config/Cargo.toml`: no dependency change. Source change is the additive `BusConfig` struct + one `validate()` check + one assertion in C-1.

`crates/app/Cargo.toml`:
- Runtime: `rust-lmax-mev-types = { path = "../types" }`, `rust-lmax-mev-event-bus = { path = "../event-bus" }`, `rust-lmax-mev-journal = { path = "../journal" }`, `rust-lmax-mev-config = { path = "../config" }`, `rust-lmax-mev-observability = { path = "../observability" }`, `tokio = { workspace = true }`, `tracing = { workspace = true }`, `thiserror = { workspace = true }`.
- Dev: `tempfile = "3"`.

No new transitive C/C++ build dependencies. `librocksdb-sys` is already pulled by `crates/journal` (Task 13 Gate 5); `crates/app` gets it transitively without a new precondition.

## Risk Decisions

1. **Payload type for Phase 1: `SmokeTestPayload`**. The real `DomainEvent` sum type belongs to the Phase 3 pipeline (ADR-001 vertical slice). Hardcoding `SmokeTestPayload` for the Phase 1 binary keeps the wiring concrete and lets Task 17's smoke test exercise the journal/bus paths end-to-end. Phase 3 will swap the type in a focused commit; the bus and journal are generic over `T` so this is a single-line change at wiring time.
2. **CLI parsing: stdlib `std::env::args()`, no `clap`**. Phase 1 has exactly one positional arg (config path). Adding `clap` for a single arg would pull ~12 transitive crates and a build-time cost for no Phase 1 benefit. Phase 4 hardening can revisit when subcommands appear.
3. **Consumer thread: `std::thread::spawn`, not `tokio::task::spawn_blocking`**. `crossbeam::channel::Receiver::recv` is a blocking syscall; running it on a tokio worker would consume a runtime thread for the entire process lifetime. A bare `std::thread` makes the cost explicit and matches the LMAX single-consumer design. The thread joins on `Receiver::recv -> Err(Disconnected)` which fires when the bus's last `Sender` (held by the main thread) is dropped at shutdown.
4. **Shutdown semantics: drop-the-bus-then-join, no explicit cancel token**. The crossbeam channel already provides clean shutdown via `Disconnected`; introducing a separate `CancellationToken` would duplicate the signal and add two failure modes (token sent but consumer mid-recv, etc.). Drop order in `run()`: drop bus producer → consumer's `recv` returns `Err(Disconnected)` → consumer thread loops break → `flush()` journal → `join()` thread.
5. **`#[tokio::main(flavor = "current_thread")]`**. Phase 1 only awaits one future (`ctrl_c`); current-thread runtime avoids spawning extra OS threads. Phase 3 producer wiring will revisit when async RPC clients land.
6. **`run()` returns on first error during wiring; observability init failure is fatal**. No retry, no fallback log subscriber. Operator-layer concern per ADR-008.

No frozen-decision changes, no new ADR, no crate-boundary changes, no persistence-format changes. The `BusConfig` addition is an additive struct field within an existing config crate — explicitly NOT a boundary change.

## Minimal Test Matrix (Risk-Based — 4 tests total)

`crates/config` (extend C-1 only, no new test):
- C-1 happy is extended to assert `cfg.bus.capacity == <value-from-TOML>` after the new `[bus]` section is added to `valid_minimum_toml()`. C-2 and C-3 untouched.
- One NEW test **C-4 boundary** `from_toml_str_rejects_zero_bus_capacity` — `[bus] capacity = 0` → `Err(ConfigError::EmptyRequiredField { field: "bus.capacity" })` (or a dedicated `InvalidBusCapacity` variant if cleaner; decide at impl time).

`crates/app` (3 tests under `crates/app/tests/`):
- **A-1 happy** `run_wires_journal_and_consumer_then_shuts_down_on_drop` — call `run()`-equivalent (refactored helper that takes a shutdown future) on a tempdir-backed config; publish 3 `SmokeTestPayload` events through the returned bus handle; trigger shutdown; assert `FileJournal::iter_all` reads back 3 envelopes in order. Verifies the full wiring (config → observability skipped via test-only flag → journal → bus → consumer thread → drain → flush → join).
- **A-2 failure** `run_returns_error_on_invalid_config_path` — call `run("/nonexistent/path")`; expect `Err(AppError::Config(ConfigError::Io(_)))`. Asserts `main`'s exit-code path has a typed error to map.
- **A-3 boundary** `run_returns_error_on_double_observability_init` — call the test helper twice in the same test (single-binary so OnceLock fires); second call returns `Err(AppError::Observability(ObservabilityError::AlreadyInitialized))`. Verifies error propagation through the wiring layer.

The integration test refactors `run()` so observability init is gated by a `init_observability: bool` parameter (or splits the wiring into `run_with_options(opts)` so tests can reuse a single subscriber across cases). This is the same pattern Batch A used for `O-combined`.

Cumulative test count after Batch B: 45 (current) + 1 (config C-4) + 3 (app A-1..A-3) = **49 workspace tests**.

Wide perf / integration coverage (100k bus throughput, journal round-trip @ scale, snapshot crash recovery) belongs to Task 17 (Batch C) per the test policy.

## Verification Commands (Run at Batch Close, Not Per-Commit)

```powershell
cargo fmt --check
cargo build --workspace
cargo build -p rust-lmax-mev-app --bin rust-lmax-mev-app
cargo test --workspace                                # expect 49 passed
cargo clippy --workspace --all-targets -- -D warnings
cargo doc -p rust-lmax-mev-app --no-deps
```

All gates must exit 0 before the batch is considered closed and the outbox compact summary is posted for Codex's batch-close review.

## Commit Grouping (Lean — 4 commits target)

1. **`docs: add Batch B app execution note`** — this file. Single docs commit BEFORE any code, mirroring Batch A's `adad010`.
2. **`feat(config): add BusConfig + capacity validation + C-4 test`** — additive Config edit + 1 new test + extend C-1's assertion. Keeps Config's surface change reviewable in isolation.
3. **`chore(workspace): scaffold crates/app`** — workspace `Cargo.toml` (`members` += `"crates/app"`), `crates/app/Cargo.toml`, placeholder `lib.rs` + `main.rs` that compile but no behavior.
4. **`feat(app): run() wiring + AppError + 3 integration tests`** — full app behavior + tests under `crates/app/tests/`.

Optional 5th commit `chore(batch-B): final fmt/clippy cleanup` only if the verification gate surfaces formatting drift at batch close.

All commits use `git commit -F <file>` form per the project PowerShell precedent. No tag creation in this batch; `phase-1-complete` belongs to Task 19.

## Forbidden (Reaffirmed)

- No `git push`, no `git tag` without explicit user approval (Batch B produces no tag of its own).
- No staging of `CLAUDE.md`, `AGENTS.md`, `.claude/`.
- No edits to `crates/types/**` (frozen at `e2911cf`), `crates/event-bus/**` (frozen at `bb2e020`), `crates/journal/**` (frozen at `9c81e27` = `task-13-complete`).
- No edits to `crates/observability/**` in this batch (frozen by Batch A close).
- No alternative async runtime (smol, async-std). No alternative CLI parser (clap, structopt, argh) for Phase 1.
- No real producer pipeline implementation — that is Phase 3 work per ADR-001. The Phase 1 binary stays a wiring harness.
- No `CancellationToken` / `tokio_util::sync::CancellationToken` — drop-then-join is the only shutdown path per Risk Decision 4.
- No multi `-m` git commits in PowerShell.
- No detailed `docs/superpowers/specs/` document — this execution note is the sole planning artifact for Batch B per the policy.
