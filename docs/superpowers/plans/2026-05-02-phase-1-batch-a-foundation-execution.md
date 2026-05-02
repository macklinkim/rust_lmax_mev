# Phase 1 Batch A — Foundation Execution Note

**Date:** 2026-05-02
**Status:** Draft v0.2 (revised after Codex 2026-05-02 15:23:50 +09:00 HIGH-confidence REVISION REQUIRED — fixed O-combined test logic, updated Risk Decision 2, added per-crate dependency deltas)
**Scope:** Tasks 14 (`crates/config`) + 15 (`crates/observability`), grouped as one Foundation batch.
**Predecessor:** Task 13 closed at `task-13-complete` (`9c81e27`).
**Authoritative sources:** ADR-003, ADR-007, ADR-008, CLAUDE.md, the four frozen `docs/specs/` documents. No detailed batch spec is produced; this note IS the planning artifact.

## Scope

Two passive infrastructure crates that `crates/app` (Batch B) wires at startup:

- **`crates/config`** — typed TOML loader with env overlay; required-field validation per ADR-007 (≥ 1 fallback RPC) + ADR-008 (Prometheus listen socket) + ADR-003 (FileJournal + RocksDbSnapshot paths).
- **`crates/observability`** — single `init(&ObservabilityConfig)` that wires `tracing-subscriber` (env-filter + JSON / pretty formatter) + `metrics-exporter-prometheus` (HTTP listener); single-init guarded by `OnceLock`.

Neither crate performs network I/O at construction; neither owns long-running tasks; neither imports `crates/journal` or `crates/event-bus`.

## Source References

- **ADR-007** — Node Topology & Fallback RPC: drives `NodeConfig` shape (Geth WS + HTTP + ≥ 1 fallback HTTP).
- **ADR-008** — Observability & CI Baseline: drives `crates/observability` impl + `ObservabilityConfig` (Prometheus default `0.0.0.0:9090`, `tracing` + `metrics` facade).
- **ADR-003** — Mempool/Relay/Persistence: drives `JournalConfig` shape (FileJournal path + RocksDbSnapshot path; primitives already shipped at `task-13-complete`).
- **CLAUDE.md** — `tempfile = "3"` dev-dep convention; the four `docs/specs/` documents and ADRs are the authoritative source.

## Public API Sketch

### `crates/config`

```rust
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub node: NodeConfig,
    pub observability: ObservabilityConfig,
    pub journal: JournalConfig,
}

pub struct NodeConfig {
    pub geth_ws_url: String,
    pub geth_http_url: String,
    pub fallback_rpc: Vec<FallbackRpcConfig>,   // length >= 1 enforced post-load
}
pub struct FallbackRpcConfig { pub url: String, pub label: String }

pub struct ObservabilityConfig {
    pub prometheus_listen: SocketAddr,          // ADR-008 default 0.0.0.0:9090
    pub log_filter: String,                     // RUST_LOG-style directive
    pub log_format: LogFormat,                  // Json | Pretty
}
pub enum LogFormat { Json, Pretty }

pub struct JournalConfig {
    pub file_journal_path: PathBuf,
    pub rocksdb_snapshot_path: PathBuf,
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError>;
    pub fn from_toml_str(s: &str) -> Result<Self, ConfigError>;
}

#[non_exhaustive]
pub enum ConfigError {
    Io(std::io::Error),                                            // #[from]
    Parse(config::ConfigError),                                    // #[from]
    MissingFallbackRpc,                                            // ADR-007
    EmptyRequiredField { field: &'static str },                    // post-overlay scalar empty
    InvalidSocketAddr { field: &'static str, reason: String },
}
```

Env overlay: prefix `RUST_LMAX_MEV`, separator `__` (config-rs default). Lists not env-overlay-friendly; TOML is the source of truth for `fallback_rpc`.

### `crates/observability`

```rust
pub fn init(config: &rust_lmax_mev_config::ObservabilityConfig)
    -> Result<ObservabilityHandle, ObservabilityError>;

pub struct ObservabilityHandle { /* private: PrometheusHandle */ }

#[non_exhaustive]
pub enum ObservabilityError {
    AlreadyInitialized,                          // OnceLock guard
    TracingInstall(String),                      // try_init failure
    PrometheusInstall(String),                   // bind / install failure
}
```

## Workspace + Per-Crate Dependency Deltas

Workspace `[workspace.dependencies]`:
- ADD `config = "0.14"` (config-rs; new). All other workspace deps used by Batch A (`tracing`, `tracing-subscriber`, `metrics`, `metrics-exporter-prometheus`, `serde`, `thiserror`, `tokio`, `toml`) are ALREADY present and unchanged.

`crates/config/Cargo.toml`:
- Runtime: `rust-lmax-mev-types = { path = "../types" }` (only for shared error/path types if needed; otherwise omitted), `config = { workspace = true }`, `serde = { workspace = true }`, `thiserror = { workspace = true }`.
- Dev: `tempfile = "3"` per CLAUDE.md convention.

`crates/observability/Cargo.toml`:
- Runtime: `rust-lmax-mev-config = { path = "../config" }` (path-dep for the shared `ObservabilityConfig` type; mirrors how `crates/journal` path-deps `crates/types`), `tracing = { workspace = true }`, `tracing-subscriber = { workspace = true }`, `metrics = { workspace = true }`, `metrics-exporter-prometheus = { workspace = true }`, `thiserror = { workspace = true }`.
- Dev: none (single combined `#[test]` per Risk Decision 2 avoids `serial_test`).

No new transitive C/C++ build dependencies (config-rs is pure Rust). No new licenses-of-concern (all crates above are MIT/Apache-2.0).

## Risk Decisions

1. **`config = "0.14"` (config-rs)** added to `[workspace.dependencies]`. Hand-rolling TOML + env overlay on top of `toml = "0.8"` would require ~150 LOC + a wider test surface; config-rs handles nested env overlay (`PREFIX__a__b__c`), default merging, and type coercion. Not an ADR-level decision (ADR-008 covers observability, not config plumbing); flagging here for Codex confirmation in the single batch review.
2. **`crates/observability` test isolation + scope**: tests share the process-wide `tracing` subscriber and the `metrics::set_global_recorder` slot, AND the `OnceLock` guard inside `init()` locks on the first successful call. Once `init()` succeeds, every subsequent `init()` short-circuits to `AlreadyInitialized` BEFORE reaching the Prometheus bind step — so port-conflict behavior cannot be exercised through the public `init()` API after a successful initialization. Implementation collapses the two reachable-via-public-API cases (success + double-init rejection) into a single ordered `#[test]` function (option (b)) so we avoid adding `serial_test` as a dev-dep. Loses per-test naming; gains zero new dev deps. Port-conflict coverage on `init()` is intentionally OUT of scope for Batch A: `metrics-exporter-prometheus`'s own test suite covers bind-failure paths, and Phase 1 single-host deployments handle port conflicts at the operator/deploy layer; if a future revision needs in-tree port-conflict coverage, the implementation can extract a `pub(crate) fn try_install_recorder(addr) -> Result<PrometheusHandle, ObservabilityError>` private helper that's testable without touching the OnceLock guard.
3. **`#[serde(deny_unknown_fields)]`** on every config struct so a TOML typo surfaces as a parse error rather than a silent default. Phase 2 may revisit if forward-compat config friction appears.
4. **`prometheus_listen` default `0.0.0.0:9090`** per ADR-008 verbatim. Operators can narrow to loopback via TOML or env overlay.

No frozen-decision changes, no new ADR, no crate-boundary changes, no persistence-format changes.

## Minimal Test Matrix (Risk-Based Minimal — 6 tests total)

`crates/config` (3 tests, inline `#[cfg(test)] mod tests`):
- **C-1 happy** `from_toml_str_round_trips_minimum_valid_config` — minimum valid TOML with all 3 sections + 1 fallback RPC parses; field values match.
- **C-2 failure** `from_toml_str_rejects_empty_fallback_rpc_list` — `fallback_rpc = []` → `Err(MissingFallbackRpc)`. Asserts the ADR-007 invariant.
- **C-3 boundary** `load_overlays_env_var_over_toml_value` — tempdir TOML + `RUST_LMAX_MEV__OBSERVABILITY__LOG_FILTER=trace` env → resulting `log_filter == "trace"`. Asserts env-overlay precedence; uses a uniquely-prefixed env var to avoid cross-test pollution.

`crates/observability` (1 combined `#[test]` exercising 2 cases in order, per Risk Decision 2):
- **O-combined** `init_succeeds_then_rejects_double_init` — first call with port 0 returns `Ok(handle)`; the same handle is held for the rest of the test scope so the recorder stays installed; a second `init(...)` call returns `Err(AlreadyInitialized)`. The single test runs in one binary so the OnceLock + global-recorder state is deterministic across the two cases. Port-conflict behavior on `init()` is OUT of scope for Batch A per Risk Decision 2.

Cumulative test count after Batch A: 41 (current) + 3 (config) + 1 (observability) = **45 workspace tests**.

Wide perf / integration coverage (100k bus throughput, journal round-trip, snapshot smoke) deferred to Task 17 per the test policy.

## Verification Commands (Run at Batch Close, Not Per-Commit)

```powershell
cargo fmt --check
cargo build --workspace
cargo test --workspace                                # expect 45 passed
cargo clippy --workspace --all-targets -- -D warnings
cargo doc -p rust-lmax-mev-config --no-deps
cargo doc -p rust-lmax-mev-observability --no-deps
```

All gates must exit 0 before the batch is considered closed and the outbox compact summary is posted for Codex's batch-close review.

## Commit Grouping (Lean — 4 commits target)

1. **`docs: add Batch A foundation execution note`** — this file. Single docs commit BEFORE any code so the planning record is in place per the project's Task 11/12/13 convention.
2. **`chore(workspace): scaffold crates/config + crates/observability`** — single chore commit covering: workspace `Cargo.toml` (`members` += both crates; `[workspace.dependencies]` += `config = "0.14"`), both crates' `Cargo.toml`, both placeholder `lib.rs` files. Scaffolds compile but contain no behavior. Mirrors the Task 13 Gate 3 scaffold pattern.
3. **`feat(config): Config + sub-configs + ConfigError + 3 tests (C-1..C-3)`** — single feature commit; full crate-config behavior + lean tests.
4. **`feat(observability): init + ObservabilityHandle + ObservabilityError + 1 combined test`** — single feature commit; full observability behavior + 1 test.

Optional 5th commit `chore(batch-A): final fmt/clippy cleanup` only if the verification gate surfaces formatting drift at batch close.

All commits use `git commit -F <file>` form per the project PowerShell precedent. No tag creation in this batch; `phase-1-complete` belongs to Task 19.

## Forbidden (Reaffirmed)

- No `git push`, no `git tag` without explicit user approval (Batch A produces no tag of its own).
- No staging of `CLAUDE.md`, `AGENTS.md`, `.claude/`.
- No edits to `crates/types/**` (frozen at `e2911cf`), `crates/event-bus/**` (frozen at `bb2e020`), `crates/journal/**` (frozen at `9c81e27` = `task-13-complete`).
- No alternative configuration backend (Figment, etc.). No alternative metrics exporter (StatsD, OTLP). No alternative log subscriber (slog, log facade).
- No multi `-m` git commits in PowerShell.
- No detailed `docs/superpowers/specs/` document — this execution note is the sole planning artifact for Batch A per the new policy.
