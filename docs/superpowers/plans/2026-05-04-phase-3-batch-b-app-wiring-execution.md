# Phase 3 Batch B — `wire_phase3` Producer + Journal-Drain Wiring

**Date:** 2026-05-04
**Status:** Draft v0.2 (revised after Codex 2026-05-04 16:00:13 +09:00 REVISION REQUIRED HIGH: shutdown ordering made safe via async `shutdown` that awaits the aborted producer task before dropping bus handles; B-2 test strengthened from compile-time shape to a deterministic consumer-shutdown timeout test; DP-1/DP-3/DP-5 retained with the refinements Codex called out). Pre-impl Codex review requested per Phase 3 overview §"Architectural risks".
**Predecessor:** P3-A closed at `ae2fc59` (Codex APPROVED HIGH 2026-05-04 15:53:55).

## Scope

Build `wire_phase3` in `crates/app` so the binary actually moves bytes end-to-end: Geth WS mempool → ingress→state bus → `FileJournal<IngressEvent>`. Add a parallel state→opportunity bus + its journal-drain consumer with NO producer yet (`StateEngine` driver = P3-C/D). Phase 1 sync `wire()` and Phase 2 `wire_phase2` stay byte-identical.

P2-D's deferred work that this batch closes:
- Spawn `GethWsMempool::stream()` producer task.
- Spawn a `FileJournal<IngressEvent>` journal-drain consumer thread (now possible since P3-A added rkyv derives).

## New types & API surface

```rust
pub struct AppHandle3 {
    ingress_bus: CrossbeamBoundedBus<IngressEvent>,
    state_bus:   CrossbeamBoundedBus<StateUpdateEvent>,
    provider:    Arc<NodeProvider>,
    engine:      Arc<StateEngine>,
    producer_task:    tokio::task::JoinHandle<()>,
    ingress_consumer: std::thread::JoinHandle<()>,
    state_consumer:   std::thread::JoinHandle<()>,
}

pub async fn wire_phase3(config: &Config, opts: WireOptions) -> Result<AppHandle3, AppError>;
```

`wire_phase3` does everything `wire_phase2` does PLUS:
- Open `FileJournal::<IngressEvent>::open(&config.journal.file_journal_path)` and `FileJournal::<StateUpdateEvent>::open(...)` against TWO separate files (paths come from a new additive `JournalConfig` field, see DP-3 below).
- Spawn `ingress_consumer = std::thread::spawn(|| consume_loop(ingress_consumer_handle, ingress_journal))`.
- Spawn `state_consumer = std::thread::spawn(|| consume_loop(state_consumer_handle, state_journal))`.
- Spawn `producer_task = tokio::spawn(producer_loop(provider.clone(), ingress_bus.handle()))`.

`AppHandle3::shutdown(self)` is **async** (v0.2 revision per Codex 16:00:13) so the aborted producer task can be awaited to completion before any bus handle drops. Required ordering:

1. `producer_task.abort()` — request cancellation.
2. `let _ = producer_task.await;` — wait until the task actually exits and drops its `ingress_bus` producer handle. Without this, dropping the main `ingress_bus` handle before the task observes cancellation can leave the consumer thread blocked on `recv()` (the channel still has a live producer — the task — even though we dropped the wire-side reference).
3. `drop(ingress_bus)` — now the only producer handle is gone, channel closes.
4. `ingress_consumer.join()` — consumer thread's `recv()` returns `Err(Closed)`, loop exits, journal flushes.
5. `drop(state_bus)` → `state_consumer.join()` (P3-B journal stays empty; thread still spawned for shape-symmetry).
6. `drop(engine); drop(provider);` — final teardown.

Signature: `pub async fn shutdown(self) -> Result<(), AppError>`. `run()` calls `runtime.block_on(handle.shutdown())` after `ctrl_c`.

`run()` updates: `runtime.block_on(wire_phase3(...))` instead of `wire_phase2`.

## Decision points (defaults; Codex pre-impl review confirms or revises)

- **DP-1 (multi-consumer on `ingress_bus`)** — Phase 3 P3-B has only ONE consumer of `ingress_bus` (the journal-drain). The eventual state-engine driver (a consumer that reads `IngressEvent::Block` events and calls `StateEngine::refresh_block`, producing `StateUpdateEvent`) is deferred to P3-C/D. Per ADR-005 §"Consequences" multi-consumer is allowed Phase 2+; P3-B keeps it single-consumer to ship the journal wiring first. **Per Codex 16:00:13 (v0.2)**: P3-C MUST explicitly revisit the topology before adding the state-engine ingress consumer — adding a second consumer to `crossbeam::channel::bounded` requires either a `tokio::sync::broadcast` rebroadcast layer or a tee primitive; P3-C's overview-question list will surface this.
- **DP-2 (producer error handling)** — `producer_loop` is best-effort per ADR-001 thin-path: on `IngressError`, log via `tracing::warn!` and continue. Stream exhaustion (`None`) ends the task cleanly. Hard panic propagates (caught by tokio).
- **DP-3 (config schema)** — Add `JournalConfig.ingress_journal_path: PathBuf` and `JournalConfig.state_journal_path: PathBuf` (additive — ADR-001 freeze policy precedent: `BusConfig`). Existing `file_journal_path` is repurposed as the ingress journal default OR kept reserved for the SmokeTest path (used by Phase 1 `wire()`); details in §Workspace deltas. Sample TOMLs in `config/{base,dev,test}/default.toml` updated.
- **DP-4 (test matrix without live node)** — Same constraint as P2-D: no public `NodeProvider` mock; `wire_phase3` happy-path against a real WS endpoint isn't testable in CI. Test only the failure path (bogus URL → `Err(AppError::Node)` within `tokio::time::timeout(5s)`) and a compile-time `AppHandle3` shape assertion. Producer/consumer happy-path verification is dev-host smoke + future Phase 3 fixture work.
- **DP-5 (`StateUpdateEvent` payload archival)** — `state_consumer` opens `FileJournal<StateUpdateEvent>` (relies on P3-A derives) but in P3-B the journal stays empty because nothing publishes to `state_bus`. Constructed empty so P3-C/D can attach the producer without changing the wire surface.

## Test matrix (lean)

`crates/app` (3 new tests, 8 total — strengthened in v0.2 per Codex 16:00:13):
- **B-1 failure** `wire_phase3_returns_error_for_bogus_geth_url` (`#[tokio::test(flavor = "multi_thread")]`) — `geth_http_url = "not-a-url"` → `Err(AppError::Node | AppError::Io)` within `tokio::time::timeout(5s)`.
- **B-2 deterministic shutdown** `journal_drain_consumer_joins_after_bus_drop` (`#[test]`, no tokio runtime) — directly exercises the new `consume_loop<T>` shape used by `wire_phase3` without needing a NodeProvider:
  1. `tempfile::tempdir` for the journal path.
  2. `let (bus, consumer) = CrossbeamBoundedBus::<IngressEvent>::new(8).unwrap();`
  3. `let journal: FileJournal<IngressEvent> = FileJournal::open(&path).unwrap();`
  4. `let join = std::thread::spawn(|| consume_loop(consumer, journal));`
  5. Optionally publish 1-2 envelopes to verify the loop appends them (using a `meta()` helper + the existing `EventEnvelope::seal` API — the `EventBus::publish` shape is already exercised in `crates/event-bus` tests).
  6. `drop(bus);`
  7. Wrap `join.join()` with a manual timeout via `std::sync::mpsc::channel` polling on `Instant::now() < deadline (2s)`, OR use a `parking_lot::Mutex` + `Condvar`. Assert thread joins within 2s.
  This proves the consumer's `recv()` loop reliably exits when the bus closes — the load-bearing shutdown property `wire_phase3` depends on.
- **B-3 boundary** `app_handle3_shutdown_signature` — compile-time assertion that `AppHandle3::shutdown` returns a future (so the async-shutdown contract from §"New types & API surface" survives refactors): `fn _assert_async_shutdown(h: AppHandle3) -> impl std::future::Future<Output = Result<(), AppError>> { h.shutdown() }`.

DP-4 v0.2 verdict: B-2 covers the load-bearing shutdown semantic without needing a NodeProvider mock; no `fake_provider` module is added (avoids scope creep into a P2-A-frozen surface).

Workspace cumulative: 77 → **80** in CI (+1 ignored unchanged).

## Workspace + per-crate dependency deltas

`crates/app/Cargo.toml`: ADD `rust-lmax-mev-ingress = { path = "../ingress" }` (already present), `rust-lmax-mev-state = { path = "../state" }` (already present). No new path-deps. No workspace deps changes.

`crates/config/src/lib.rs` additive (DP-3):
- `JournalConfig` gains `ingress_journal_path: PathBuf` and `state_journal_path: PathBuf`. Existing `file_journal_path` stays as the Phase 1 SmokeTest path (still used by `wire()`).
- `Config::validate` adds checks (paths non-empty; uniqueness vs each other and vs `file_journal_path`).

`config/{base,dev,test}/default.toml`: add `ingress_journal_path` + `state_journal_path` entries to `[journal]`.

`crates/app/src/lib.rs`: add `wire_phase3` + `AppHandle3` + `producer_loop`. `consume_loop` parameterized over `T` so the same impl drains both `IngressEvent` and `StateUpdateEvent` journals. The generic `consume_loop<T>` is `pub` (renamed from the existing private Phase 1 one if needed) so B-2 can drive it directly from the integration test crate without any NodeProvider mock. The Phase 1 `wire()` codepath continues to use whichever shape it already does (no change to Phase 1 behavior).

## Commit grouping (5 commits)

1. `docs: add Phase 3 Batch B app wiring execution note` — this file.
2. `feat(config): add ingress_journal_path + state_journal_path to JournalConfig (additive)` — config struct + validate + sample TOML refresh.
3. `feat(app): add wire_phase3 + AppHandle3 + producer/journal-drain spawns` — `crates/app/src/lib.rs` + `Cargo.toml` (no new path-deps; both ingress + state already present from P2-D).
4. `test(app): B-1 + B-2 wire_phase3 tests` — `crates/app/tests/wire_phase3.rs`.
5. (optional) `chore(batch-p3-b): pick up fmt + Cargo.lock drift at batch close` — only if needed.

Targeted `cargo test -p rust-lmax-mev-app` per code commit; full workspace gates ONLY at batch close. Then run `.coordination/scripts/run_checks.ps1` + append a fresh "Codex Tail Summary" to `.coordination/auto_check.md` (P3-A lessons learned) before re-emitting outbox for Codex batch-close review.

## Forbidden delta (only NEW)

- No `StateEngine` driver task in P3-B (deferred to P3-C/D).
- No multi-consumer on `ingress_bus` in P3-B (DP-1; deferred to P3-C/D).
- No `RelaySimulator` / submission / live mainnet in CI (carried over).
- No edits to ANY frozen Phase 1 / P2-A / P2-B / P2-C / P3-A crate src; only `crates/config` additive struct extension is allowed.

## Question for Codex (pre-impl, v0.2)

v0.2 incorporates Codex 16:00:13 (REVISION REQUIRED HIGH):
- §"New types & API surface" now specifies `AppHandle3::shutdown` is async + `producer_task.abort(); let _ = producer_task.await;` runs BEFORE any bus drop/consumer join.
- §"Test matrix" replaces compile-time-only B-2 with a deterministic no-network B-2 that spawns `consume_loop` on a temp journal + bus, drops the bus, asserts join within 2s; B-3 retains a thin compile-time async-shutdown signature assertion.
- DP-1 explicitly notes P3-C MUST revisit topology before adding the state-engine ingress consumer.
- DP-3 confirms additive `JournalConfig` paths + sample TOML/validation updates.
- DP-5 confirms dual typed journal files (no sum-type journal).
- `consume_loop<T>` becomes `pub` so the integration test can drive it without a NodeProvider mock.

Open question: anything else needed before the 4-5 commit ladder runs?

If APPROVED: execute the ladder + batch-close evidence pack with auto_check.md tail-summary refresh (P3-A lessons applied). If REVISION: revise + re-emit. If ADR/scope/freeze change required: HALT to user.
