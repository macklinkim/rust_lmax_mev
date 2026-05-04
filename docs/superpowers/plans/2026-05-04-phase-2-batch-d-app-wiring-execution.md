# Phase 2 Batch D — App Wiring + Final DoD Audit + `phase-2-complete` Tag Draft

**Date:** 2026-05-04
**Status:** Draft v0.3 (post-impl amendment: Risks gains an `IngressEvent` rkyv-gap entry; §Wiring delta drops the journal-draining consumer thread that v0.2 promised. Discovered during commit-2 implementation.) — v0.2 (revised after user 2026-05-04 KST self-check directive: split `wire` / `wire_phase2` to dodge the runtime-lifetime trap; added §Risks naming runtime-lifetime + config-TOML-schema mismatch.)
**Predecessor:** P2-C closed at `239ea86` (Codex APPROVED MEDIUM 2026-05-02 22:06:29 +09:00).
**Authoritative sources:** ADR-001 (Phase 2 EXIT gates), ADR-003 / 005 / 007, frozen `docs/specs/`, Phase 2 overview note.

## Scope

Extend `crates/app` with Phase 2 producer-side construction, audit
Phase 2 against ADR-001 EXIT, draft the `phase-2-complete` tag message.
Tag creation + master push (17+ unpushed commits) stay user-gated. No
new crates; no frozen-Phase-1 src edits; no P2-A/P2-B/P2-C src edits.
Only non-app file touched: `config/{base,dev,test}/default.toml`.

## Wiring delta (`crates/app`)

Phase 1 sync `pub fn wire(config, opts) -> AppHandle` is **unchanged**
— the 3 P1 app tests + `SmokeTestPayload` consumer keep working as-is.
Add a NEW async constructor:

```rust
pub async fn wire_phase2(
    config: &Config,
    opts: WireOptions,
) -> Result<AppHandle2, AppError>;
```

Inside `wire_phase2`:

- `observability::init` (gated by `WireOptions.init_observability` as P1).
- `NodeProvider::connect(&config.node).await`.
- `RocksDbSnapshot::open(&config.journal.rocksdb_snapshot_path)`.
- `pools = config.state.pools.iter().map(PoolId::from).collect()`.
- `StateEngine::new(Arc::new(provider_handle), Arc::new(snapshot), pools)`.
- `CrossbeamBoundedBus::<IngressEvent>::new(config.bus.capacity)?`.

v0.3 amendment: NO `FileJournal<IngressEvent>::open` and NO
journal-drain consumer thread are constructed (see §Risks
"`IngressEvent` is not `rkyv::Archive`").

`AppHandle2` holds `bus`, held `_consumer`, `provider`, `engine`. No
producer-side spawn (DP-2 default — Phase 3 owns the pipeline). No
consumer thread either.

`pub fn run(config_path)` builds a `multi_thread` tokio runtime,
`block_on`s `wire_phase2`, `block_on`s `ctrl_c`, then `handle.shutdown()`,
then drops the runtime. Runtime lives the full process lifetime so
`NodeProvider`'s WS handle is never orphaned.

`AppError` (already `#[non_exhaustive]`) gains `Node(NodeError)` and
`State(StateError)` with `#[from]`. `crates/app/Cargo.toml` adds
path-deps `rust-lmax-mev-node`, `rust-lmax-mev-ingress`,
`rust-lmax-mev-state`. No workspace-dep changes.

## Risks (P2-D specific)

- **Runtime lifetime.** A NodeProvider built inside an async fn is
  bound to that runtime; if a sync wrapper internally `block_on`s a
  constructor and then drops the runtime, the WS handle becomes
  unusable on next call. **Mitigation:** ONLY construct NodeProvider
  via async `wire_phase2`; NEVER offer a sync wrapper that creates +
  destroys a runtime. `run()` builds the runtime once at process
  start and keeps it alive through `handle.shutdown()`. Tests use
  `#[tokio::test(flavor = "multi_thread")]`.
- **Config TOML schema mismatch.** `config/{base,dev,test}/default.toml`
  predate Phase 1 Batch B's `Config` and would be rejected by
  `Config::load` (missing `[node]` / `[ingress]` / `[state]`). They are
  not loaded by any test (P1+P2 tests use inline TOML), so CI does not
  catch this — but `cargo run --bin rust-lmax-mev-app config/dev/
  default.toml` fails immediately. **Mitigation:** DP-3 (commit 4)
  rewrites all three to a valid Phase 2 schema with placeholder Geth
  URLs + real Uniswap V2 USDC/WETH + V3 0.05% pool addresses.
- **`IngressEvent` is not `rkyv::Archive`** (v0.3, post-impl). A
  `FileJournal<IngressEvent>::append` would not type-check: the
  journal's `append` requires `T: rkyv::Archive + Serialize<...>` and
  `IngressEvent` (in P2-A-frozen `crates/ingress`) carries no such
  derives. **Mitigation:** P2-D's `wire_phase2` builds NEITHER a
  `FileJournal<IngressEvent>` NOR a journal-drain consumer thread —
  symmetric with DP-2 (no producer spawn). The bus producer + held
  consumer handle live inside `AppHandle2` so Phase 3 can swap in both
  ends without changing the `wire_phase2` surface. Phase 3 will need to
  add rkyv derives to `IngressEvent` (one-shot additive edit to the
  P2-A-frozen crate) or split the bus payload type.

## Test matrix (lean)

`crates/app` (2 new, 5 total):
- **D-1 failure** `wire_phase2_returns_error_for_bogus_geth_url`
  (`#[tokio::test(flavor = "multi_thread")]`) — `geth_ws_url =
  "ws://127.0.0.1:1"` → `Err(AppError::Node(_) | AppError::Io(_))`
  within `tokio::time::timeout(Duration::from_secs(5), ...)`.
- **D-2 boundary** `app_error_from_impls_compile` — compile-time
  `let _: AppError = NodeError::Closed.into();` and `let _: AppError =
  StateError::UnknownPool(Address::ZERO).into();`.

Workspace cumulative: 69 → **71** in CI (+1 ignored unchanged).

## Commit grouping (5 commits — v0.3)

1. `docs: add Phase 2 Batch D app wiring + DoD audit + phase-2-complete tag draft` (v0.2 of this note).
2. `feat(app): add wire_phase2 + AppError Node/State variants + runtime in run`.
3. `test(app): D-1 + D-2 P2-D wire_phase2 tests`.
4. `chore(config): refresh sample TOMLs to current Phase 2 schema`.
5. `docs: amend Batch D note for IngressEvent rkyv-gap discovery (v0.3)` — this amendment.

Targeted `cargo test -p rust-lmax-mev-app` per code commit; full
workspace gates (target 71 tests + fmt + clippy + doc + cargo deny)
ONLY at batch close, then post for Codex batch-close review.

## Phase 2 DoD audit (against overview + ADR-001 EXIT)

| Item | Status | Evidence |
|---|---|---|
| P2-A node + ingress | ✅ | `d9e7d48..9487cce`; APPROVED 2026-05-02 20:38:59 |
| P2-B state engine | ✅ | `9311d8d..310f6c7`; APPROVED 2026-05-02 21:27:24 |
| P2-C replay + EXIT gates | ✅ | `8f297ed..239ea86`; APPROVED 2026-05-02 22:06:29 |
| Replay Gate (ADR-001 EXIT #1) | ✅ | `crates/replay/tests/g_replay.rs` byte-identical |
| State Correctness Gate (ADR-001 EXIT #2) | ✅ | `crates/replay/tests/g_state.rs` + `g_pin.rs` (3 cases) |
| `crates/app` producer wiring | 🟡 | this batch |
| Frozen P1 crates not edited | ✅ | grep `git log` since `phase-1-complete` shows zero src changes |
| `crates/config` additive only | ✅ | new structs only; existing structs unchanged |
| No live-mainnet in CI | ✅ | `g_state_live.rs` `#[ignore]`d; mocks elsewhere |
| No revm / BundleRelay / archive / external mempool | ✅ | grep empty |
| AGENTS.md / .claude/ never staged | ✅ | `git ls-files | grep` empty |
| No push / tag without user approval | ✅ | `master...origin/master [ahead 17]`; no Phase 2 tag |

## Draft `phase-2-complete` tag message

Place in `.coordination/.phase_2_complete_msg.txt`, apply via
`git tag -a -F` (PowerShell `-m` is unreliable for multi-line).

```text
phase-2-complete: Phase 2 vertical slice + EXIT gates shipped

Phase 2 ships mempool ingestion + per-block state engine + replay
hooks per ADR-001 vertical-slice ordering, with both ADR-001 EXIT
gates passing in CI.

New crates (Phase 2):
  - rust-lmax-mev-node      (Geth WS+HTTP + fallback per ADR-007)
  - rust-lmax-mev-ingress   (MempoolSource trait + GethWsMempool)
  - rust-lmax-mev-state     (UniV2 + UniV3 0.05% reserves snapshot)
  - rust-lmax-mev-replay    (Replayer trait + StateReplayer)

Touched (additive only): crates/config, crates/app (producer wiring).
Frozen since phase-1-complete: crates/{types,event-bus,journal,
observability,smoke-tests}.

EXIT gates passing in CI:
  - Replay Gate            crates/replay/tests/g_replay.rs
  - State Correctness Gate crates/replay/tests/g_state.rs + g_pin.rs

Test count: 71 workspace tests in CI (52 P1 + 6 P2-A + 6 P2-B + 5
P2-C + 2 P2-D, plus 1 ignored live-smoke env-contract stub).

Phase 3 (6-stage pipeline + revm simulation) is the next phase per
ADR-001.
```

Tag target: SHA of the Batch D batch-close HEAD. User may override.

## Forbidden delta (only NEW)

- No edits to ANY P2-A / P2-B / P2-C crate src after their batch-close commits.
- No new ADRs / no spec edits.
- All standing Phase 1 + Phase 2 forbids carry over (no push / no tag / no `CLAUDE.md` / no `AGENTS.md` / no `.claude/` staging without explicit user approval).
