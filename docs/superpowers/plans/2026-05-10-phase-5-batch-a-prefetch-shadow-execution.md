# Phase 5 Batch A — production `LocalSimulator::prefetch_for` shadow-mode wiring

**Date:** 2026-05-10 KST
**Status:** Draft v0.3 (revised after manual Codex REVISION REQUIRED HIGH on v0.2, 2026-05-10 KST). Six stale-residue cleanups R-A7..R-A12 applied (Scope/Open-questions/forbids/Process re-aligned with v0.2's DP-A8/A9/A10/A11 design decisions). v0.1 R-A1..R-A6 + v0.2 Q-A standing answers + CFG-A2 carried unchanged. Awaiting manual Codex re-review.
**Predecessor:** Phase 5 overview v0.3 APPROVED HIGH at `ac07024`. Phase 4 closed at `phase-4-complete` (`e5f13ea`).

## Scope

Land the production `LocalSimulator::prefetch_for` integration the P4-G DoD audit explicitly deferred. Phase 5 Safety Gate constraints carry: no signing / no submission / no `live_send=true` / no `eth_sendBundle`.

Deliverables (all additive; Safety Gate scope only):

1. **Interior-mutability redesign of `LocalSimulator`** — replace the current `&mut self` `load_fixture` / `prefetch_for` API surface with `&self` interior-mutability so the existing `Arc<LocalSimulator>` shared from `wire_phase4` can drive prefetches per inbound `RiskCheckedOpportunity`. Per Q-P5-2 standing answer: default `parking_lot::Mutex<Option<FixtureSet>>` (revisit only if profiling demands).
2. **`simulator_driver` prefetch dispatch** — when both `config.node.archive_rpc.is_some()` AND `config.simulator.prefetch_enabled = true` (NEW config field; default `false` per Q-P5-6), the driver calls `simulator.prefetch_for(&fetcher, opp).await` BEFORE `simulate_with_fingerprint(...)`. Otherwise the existing P4-E behavior (no prefetch → `Setup` warn → drop event) is preserved exactly.
3. **Per-block fixture cache** — keyed by `(block_hash, source_pool, sink_pool)`. Bounded LRU (default capacity 64). On cache hit, `prefetch_for` short-circuits to the cached fixture set without an archive RPC call. `freshness_window_blocks` is a **retention/eviction** parameter (per DP-A11): because the cache key includes `block_hash`, a different block produces a different key by definition — the cache CANNOT serve stale state under any value. Default `1` evicts immediately on a new block; `≥ 2` retains prior blocks' entries in the LRU for in-flight events on those blocks. Cache lives inside `LocalSimulator` behind the same `Mutex` as the active fixture slot.
4. **Fail-closed semantics** — every archive failure mode (`ArchiveNotConfigured`, `Transport`, `UnrecognizedResponse`, JSON-RPC error, timeout, rate-limit-as-Transport) → driver logs at WARN + drops the event. NEVER substitutes a stale fixture; NEVER panics; NEVER blocks the producer chain.
5. **NEW `crates/config` `SimulatorConfig` section** — `prefetch_enabled: bool` (default `false`), `prefetch_cache_capacity: usize` (default 64; wire-shape `usize` per DP-A8 v0.2 — runtime wraps in `NonZeroUsize` after `Config::validate` rejects `0`), `freshness_window_blocks: u64` (default 1). All `#[serde(default)]` so existing TOML configs predating P5-A parse unchanged.
6. **NEW `crates/app::wire_phase4` `Arc<dyn StateFetcher>` construction** — when `prefetch_enabled = true`, build an `ArchiveStateFetcher` from `Arc::clone(&provider)` + the existing P4-B `StateFetcherConfig::defaults()` (the actual ctor API per DP-A10; mirrors the `SimConfig::defaults()` pattern). Hold it in `AppHandle4` so the driver task can call its `fetch_pool` / `fetch_account`. When disabled, no fetcher is constructed (no live RPC surface activated). Per DP-A9, the `prefetch_enabled = true` + `archive_rpc = None` combination is rejected at `Config::validate` time, so this branch is reached only when an archive endpoint is configured.

## Decision points

- **DP-A1 (interior mutability)**: `parking_lot::Mutex<Option<FixtureSet>>` for the active-fixture slot AND for the LRU cache (one shared mutex covers both — single critical section per prefetch call). Per Q-P5-2 standing answer; revisit at impl time only if profiling shows hold-time regression. The mutex is held only across `cache.get` + `cache.put` + assignment — bounded constant time; no .await across the lock.
- **DP-A2 (prefetch dispatch order)**: driver pseudocode is `if prefetch_enabled { prefetch_for().await? }; simulate_with_fingerprint()`. The prefetch failure goes to a `tracing::warn!` + continue (matches existing simulate-failure handling) — does NOT abort the driver task and does NOT cause `Lagged` on the broadcast.
- **DP-A3 (cache-hit semantics, REVISED v0.2 per R-A1; minor wording fix v0.3 closeout)**: cache lookup keyed by `(block_hash, source_pool, sink_pool)`. Three cases handled under (per-call) mutex acquisitions:
  1. **Cache hit + active slot already matches the same key** → no-op. Active fixture stays as-is; no clone, no copy. `prefetch_for` returns `Ok(())` immediately.
  2. **Cache hit + active slot does NOT match (different key OR `None`)** → clone the cached `FixtureSet` into the active slot. The clone is performed inside the mutex critical section (bounded constant time; `FixtureSet` carries owned `Vec`/`Bytes` so clone is a controlled allocation). After clone, return `Ok(())`.
  3. **Cache miss** → fetch via `StateFetcher` (NOT under the mutex; archive RPC is async I/O), then under a fresh mutex acquisition: insert into LRU cache + clone-load into active slot.

  This eliminates the v0.1 "never load cache → active" / "still call load_fixture" contradiction.
- **DP-A4 (REVISED v0.3 per R-A9 — retention-only semantics)**: `freshness_window_blocks` is purely a **retention/eviction** parameter; cf. DP-A11 for the full rationale. Default `1` evicts cache entries immediately when a new block arrives (so the LRU only ever holds the active block's entries). `≥ 2` keeps the prior N blocks' entries in the LRU so concurrent in-flight events on a previous block can still hit the cache before eviction. Under NO value can the cache serve stale state — the `block_hash` component of the cache key guarantees that a different block is a different key by construction. The previous v0.1/v0.2 wording suggesting "trades freshness for archive-RPC cost" is dropped; only retention/eviction is being tuned.
- **DP-A5 (live RPC surface activation)**: archive RPC client construction happens INSIDE `wire_phase4` only when `prefetch_enabled = true`. When disabled (default), no `ArchiveStateFetcher` is instantiated and no live RPC connection is opened. Operator opt-in is required to incur live archive cost (Q-P5-6 standing answer).
- **DP-A6 (no submission path change)**: P5-A does NOT touch `submit_bundle`, the comparator chain, the relay-sim adapters, or the `live_send` config. The kill switch (P5-D) and signer (P5-C) work is independent.
- **DP-A7 (no live tests in CI)**: per Phase 5 forbids "no live-network test enabled by default". P5-A tests use:
  - in-memory mocks for `StateFetcher` (existing trait pattern from P4-B tests) for cache-hit + freshness + fail-closed-on-error tests,
  - the existing recorded fixtures from P4-C2 for the load-fixture-after-cached-prefetch parity test.
  - NO `#[ignore]`'d live archive tests added in P5-A.
- **DP-A8 (REVISED v0.2 per R-A5) — config-validation + capacity type**: `prefetch_cache_capacity` is typed `usize` on the wire-shape struct (so deserialize accepts `0`, `1`, `2`, ...) and `Config::validate` rejects `0` with a new `ConfigError::InvalidCacheCapacity { value: 0 }` variant. The runtime cache then constructs `NonZeroUsize::new(value).expect("validated > 0")` after `validate()` has run. This eliminates the v0.1 contradiction (could not have both `NonZeroUsize` field type AND a `value: 0` error variant — `NonZeroUsize` Deserialize would reject 0 before `validate` ran).
- **DP-A9 (NEW v0.2 per R-A4) — `prefetch_enabled` requires `archive_rpc`**: `Config::validate` rejects the combination `prefetch_enabled = true` + `archive_rpc = None` with a new `ConfigError::PrefetchRequiresArchiveRpc` variant (payload-free). NO runtime fail-closed fallback — operators get a loud config-load error rather than silent inertness. CFG-A2 covers.
- **DP-A10 (NEW v0.2 per R-A3) — workspace dependency adjustments**: this batch adds direct dependencies that are presently transitive-only:
  - `crates/simulator/Cargo.toml`: add `parking_lot = { workspace = true }` + `lru = { workspace = true }` (both already in workspace deps from earlier batches; promote to direct deps here).
  - `crates/app/Cargo.toml`: add `rust-lmax-mev-state-fetcher = { path = "../state-fetcher" }` (currently transitive via simulator's dev-dep only).
  - `cargo deny` sources/licenses unaffected (no new external crates).
- **DP-A11 (NEW v0.2 per R-A6) — freshness-window semantics**: `freshness_window_blocks` is a **retention/eviction** parameter, NOT a stale-fixture-reuse parameter. Because the cache is keyed by `block_hash` (and a different block produces a different cache key by definition), the cache CANNOT serve stale state under any value. `freshness_window_blocks` controls only HOW LONG an entry stays in the LRU after it stops being the active block:
  - `1` (default) → evict immediately when a new block arrives (no cross-block retention).
  - `≥ 2` → keep the prior N blocks' entries in the LRU so concurrent in-flight events on the previous block can still hit cache before eviction. Eviction never serves stale data; it only saves repeat-fetches for the same `block_hash`.
  - The previous v0.1 wording "trades freshness for archive cost" suggested stale-substitution behavior; the redefined wording above is correct: there is NO stale substitution under any value.

## API surface (proposed)

```rust
// crates/simulator/src/lib.rs (interior-mutability redesign)

pub struct LocalSimulator {
    cfg: SimConfig,
    state: parking_lot::Mutex<SimulatorState>,
}

struct SimulatorState {
    active: Option<FixtureSet>,
    cache: lru::LruCache<FixtureKey, FixtureSet>,
    cache_capacity: std::num::NonZeroUsize,
    freshness_window_blocks: u64,
    last_block_seen: u64,
}

#[derive(Hash, PartialEq, Eq, Clone)]
struct FixtureKey {
    block_hash: B256,
    source_pool: Address,
    sink_pool: Address,
}

impl LocalSimulator {
    pub fn new(cfg: SimConfig) -> Result<Self, SimulationError>;
    pub fn with_cache(
        cfg: SimConfig,
        cache_capacity: std::num::NonZeroUsize,
        freshness_window_blocks: u64,
    ) -> Result<Self, SimulationError>;

    // CHANGED: &self (was &mut self in P4-C2). Test path unchanged in semantics.
    pub fn load_fixture(&self, source: FetchedPoolState, ...) -> Result<(), SimulationError>;

    // CHANGED: &self (was &mut self). Production async path; same external behavior.
    pub async fn prefetch_for(
        &self,
        fetcher: &Arc<dyn StateFetcher>,
        opp: &OpportunityEvent,
        weth_address: Address,
        usdc_proxy_address: Address,
    ) -> Result<(), SimulationError>;

    // R-A2 v0.2 fix: cannot expose `fn fixtures(&self) -> Option<&FixtureSet>`
    // behind a Mutex (would require holding the lock across the borrow).
    // Replace with these snapshot-shaped methods. simulate paths CLONE the
    // active fixture out of the lock, drop the lock, then run revm; the
    // mutex is held only across the clone.
    pub fn fixtures_loaded(&self) -> bool;            // cheap; no clone
    pub fn fixtures_snapshot(&self) -> Option<FixtureSet>; // clone-out

    pub fn simulate(&self, risk_checked: &RiskCheckedOpportunity) -> Result<SimulationOutcome, SimulationError>;
    pub fn simulate_with_fingerprint(&self, risk_checked: &RiskCheckedOpportunity) -> Result<(SimulationOutcome, LocalStateFingerprint), SimulationError>;
}
```

```rust
// crates/config/src/lib.rs (additive)

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, default)]
pub struct SimulatorConfig {
    pub prefetch_enabled: bool,           // default false (Q-P5-6)
    // R-A5 v0.2 fix: typed `usize` on the wire so `0` deserializes
    // and `Config::validate` rejects with InvalidCacheCapacity.
    // Runtime cache wraps in NonZeroUsize after validate has run.
    pub prefetch_cache_capacity: usize,   // default 64; validated > 0
    pub freshness_window_blocks: u64,     // default 1 (retention only; per DP-A11)
}

impl Default for SimulatorConfig { ... }

pub struct Config {
    // existing fields ...
    #[serde(default)]
    pub simulator: SimulatorConfig,
}
```

```rust
// crates/app/src/lib.rs (additive in wire_phase4)

let simulator = Arc::new(LocalSimulator::with_cache(...)?);

// R-A4 v0.2: prefetch_enabled=true + archive_rpc=None is rejected at
// config-validation time (CFG-A2) — by the time we reach this point,
// either both are set or prefetch_enabled is false.
let prefetch_fetcher: Option<Arc<dyn StateFetcher>> =
    if config.simulator.prefetch_enabled {
        // R-A3 v0.2: real ctor is StateFetcherConfig::defaults() (the
        // existing defaults() helper, mirroring SimConfig::defaults()).
        Some(Arc::new(ArchiveStateFetcher::new(
            Arc::clone(&provider),
            StateFetcherConfig::defaults(),
        )))
    } else {
        None
    };

// simulator_driver receives Option<Arc<dyn StateFetcher>> (None disables prefetch).
```

## Test matrix (lean)

| Bucket | Tests | Location |
|---|---|---|
| PA-1 disabled-by-default invariant | `simulator_driver` with `prefetch_enabled = false` produces the same observable behavior as P4-G (no fetcher constructed, no archive RPC call attempted, simulate returns Setup, no event emitted) | `crates/app/tests/prefetch_wiring.rs` |
| PA-2 cache hit short-circuits | Mock `StateFetcher` counts calls; calling prefetch twice with same `(block_hash, pools)` results in N fetches the first time + 0 fetches the second time | `crates/simulator/tests/prefetch_cache.rs` |
| PA-3 freshness eviction | After `freshness_window_blocks` block boundary, prior cache entry is evicted; mock fetcher counts re-fetches | `crates/simulator/tests/prefetch_cache.rs` |
| PA-4 fail-closed on archive error | Mock fetcher returns `FetchError::ArchiveNotConfigured`; driver logs WARN + drops event; no event emitted on `sim_tx`; subsequent events are not blocked | `crates/app/tests/prefetch_wiring.rs` |
| PA-5 simulate parity post-prefetch | After prefetch loads a fixture set matching the SR-1 recorded fixtures, `simulate_with_fingerprint` returns a byte-identical outcome to the P4-C2 `load_fixture` test (FP-1 carries forward) | `crates/simulator/tests/prefetch_cache.rs` |
| CFG-A1 default + cache-capacity validation | `prefetch_enabled` default false; `prefetch_cache_capacity = 0` rejected with `InvalidCacheCapacity { value: 0 }` | `crates/config/src/lib.rs` cfg(test) |
| CFG-A2 (NEW v0.2 per R-A4) prefetch requires archive_rpc | `prefetch_enabled = true` + `archive_rpc = None` rejected with `ConfigError::PrefetchRequiresArchiveRpc` (payload-free); `prefetch_enabled = true` + `archive_rpc = Some(_)` parses cleanly | `crates/config/src/lib.rs` cfg(test) |

**Total**: 7 new tests. Workspace target: 206 → **213 passed + 1 ignored**.

## Phase 5 forbids carried into P5-A

- No `eth_sendBundle`. No funded key. No production signer. No `live_send=true`. No actual relay submission.
- No real paid API in CI (PA tests use mock `StateFetcher`).
- No live-network test enabled by default (no `#[ignore]`'d live archive tests added).
- No new asset pairs / V3 fee tiers / venues.
- No edits to `crates/relay-sim` / `crates/bundle-relay` / `crates/relay-clients` / `crates/types` / `crates/risk` / `crates/opportunity` / `crates/execution` / `crates/state` / `crates/state-fetcher` / `crates/node` / `crates/ingress` / `crates/event-bus` / `crates/journal` / `crates/observability`.
- `crates/simulator` body edits limited to the interior-mutability redesign + `with_cache` ctor + cache fields. Existing `simulate(...)` / `simulate_with_fingerprint(...)` external behavior unchanged on the no-prefetch path.
- `crates/config` edit limited to the additive `SimulatorConfig` + two new `ConfigError` variants: `InvalidCacheCapacity { value: 0 }` (per DP-A8) and `PrefetchRequiresArchiveRpc` (per DP-A9 / R-A4; payload-free).
- `crates/app::wire_phase4` body edit limited to the optional `Arc<dyn StateFetcher>` construction + threading into the renamed `simulator_driver` parameter list.
- No Phase 6 work.
- No `.claude/` / `AGENTS.md` / `fixture_output.txt` staging.

## Phase 5 safety grep gates carry-forward (per overview §"P5-C grep-gate redefinition" — already in effect from P5-A)

The Phase 4 G2 grep is superseded for Phase 5+ (per overview R-P5-2). P5-A close runs the redefined gate:
- Forbidden: `Wallet|PrivateKey|secp256k1|k256|alloy-signer|ethers-signers|sign_transaction|funded` — zero hits in `crates/`.
- The other Phase 4 grep gates (G1 `eth_sendBundle`, G3 `submit_bundle(` in `crates/app/src/`, G4 `Arc<dyn BundleRelay>` in `crates/app/src/`, G5 `live_send` outside config, G6 R-E20 doc-residue, G7 live-network tests, G8 cycle gate) carry forward unchanged.
- NEW P5-A grep: `prefetch_enabled.*=.*true` outside `crates/config` test fixtures + the `wire_phase4` plumbing must produce zero accidental defaults — sanity check at batch close.

## Codex Q-A standing answers (v0.3)

All five v0.1 open questions received Codex verdicts in the v0.1 review; encoded as standing decisions for v0.3.

- **Q-A1 — APPROVED**: separate `with_cache` constructor (existing `LocalSimulator::new(SimConfig::defaults())` callers unchanged).
- **Q-A2 — APPROVED**: cache key = `(block_hash, source_pool, sink_pool)` only (NOT `optimal_amount_in_wei`); probe-size variance affects swap calldata, not the fixture.
- **Q-A3 — APPROVED with redefinition (R-A10)**: `freshness_window_blocks = 1` default — semantics is **retention only**. Default `1` evicts on each new block; `≥ 2` retains prior blocks' entries in the LRU. NO stale-substitution under any value (DP-A11).
- **Q-A4 — REVISED to validation reject (R-A4 + R-A10)**: `prefetch_enabled = true` + `archive_rpc = None` is rejected at `Config::validate` time with `ConfigError::PrefetchRequiresArchiveRpc`. NO runtime fail-closed fallback — operators get a loud config-load error.
- **Q-A5 — APPROVED**: PA-5 reuses the P4-C2 SR-1 inline fixture data (single source of truth; no duplication).

## Process

Per the 2026-05-04 routine-closeout policy + the user-confirmed Phase 5 process from the overview v0.3:

1. Claude has emitted this v0.3 + the review pack to `.coordination/claude_outbox.md`. Plan stays UNCOMMITTED on disk pending Codex APPROVED (per overview standing process).
2. Claude STOPS and reports "manual Codex review required".
3. User pastes pack to Codex; relays verdict.
4. Claude records verdict in `.coordination/codex_review.md`.
5. **APPROVED** → commit + push this plan as routine doc commit; THEN implement P5-A in batches per the test matrix; THEN batch-close gates + commit + push.
6. **REVISION REQUIRED** → revise + re-emit.
7. **Scope/ADR change required** → HALT to user.

No code or `Cargo.toml` edits in this turn. No commit. No push. No tag.
