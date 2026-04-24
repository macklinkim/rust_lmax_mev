# ADR-008: Observability & CI Baseline

**Date:** 2026-04-24
**Status:** Accepted

## Context

The engine needs a defined observability stack (structured logging, metrics, dashboards) and a CI pipeline that enforces code quality and functional correctness on every pull request. Both must be established in Phase 1 so that all subsequent work is built on a measurable, reproducible baseline.

Questions to resolve:
- Which Rust crates provide structured logging and metrics?
- How are metrics exported and visualized?
- What checks must pass before code merges?
- Which checks are smoke tests vs. unit/integration tests?

## Decision

### Observability stack

| Concern | Library / Tool |
|---|---|
| Structured logging | `tracing` crate (`tracing-subscriber` for formatting) |
| Metrics | `metrics` crate (facade) + `metrics-exporter-prometheus` |
| Dashboards | Grafana (provisioned dashboards added in Phase 5) |

All log output uses `tracing` spans and events. No `println!` or `eprintln!` in production code paths. Log levels follow standard conventions: `ERROR` for operational failures, `WARN` for degraded-but-continuing conditions, `INFO` for lifecycle events, `DEBUG` for per-event detail, `TRACE` for inner-loop diagnostics.

Grafana dashboard provisioning is deferred to Phase 5 when production-candidate metrics are stable enough to be worth maintaining long-term.

### CI pipeline

All 7 checks below run on every pull request and on the default branch. **All 7 must pass before merge.**

| # | Check | Command | Purpose |
|---|---|---|---|
| 1 | Format | `cargo fmt --check` | Enforce consistent code style |
| 2 | Lint | `cargo clippy -- -D warnings` | Catch common mistakes and non-idiomatic code |
| 3 | Test | `cargo test` | Unit and integration tests |
| 4 | Dependency audit | `cargo deny check` | Block known-vulnerable or banned dependencies |
| 5 | Bus smoke | Custom binary: 100k events through the event bus end-to-end | Catch regressions in bus throughput and backpressure behavior |
| 6 | Journal round-trip | Custom test: write N events to journal, read back, assert bit-exact equality | Catch regressions in journal serialization and file I/O |
| 7 | Snapshot smoke | Custom test: write a state snapshot to RocksDB, read it back, assert equality | Catch regressions in RocksDB snapshot read/write |

Checks 5, 6, and 7 are implemented as Rust binaries or `#[test]` functions in the relevant crates and invoked by the CI runner via `cargo test` or `cargo run --bin`.

## Rationale

- `tracing` is the de-facto standard for structured, async-aware logging in the Rust ecosystem. It integrates directly with `tokio` and `alloy`.
- The `metrics` facade decouples metric instrumentation from the exporter; switching from Prometheus to another backend requires only a config change.
- Prometheus + Grafana is the industry-standard open-source observability stack. Deferring Grafana to Phase 5 avoids maintaining dashboards against a rapidly changing metric set.
- The 7-check CI baseline catches the most common regression categories: style drift, logic bugs, dependency vulnerabilities, and regressions in the three lowest-level infrastructure components (bus, journal, snapshot).
- Requiring all 7 checks on PRs and the default branch prevents the "it was already broken" defense; regressions are caught at introduction, not at discovery.
- The bus smoke test (100k events) is the Phase 2 benchmark baseline referenced in ADR-005's revisit trigger; running it in CI ensures the baseline is always current.

## Revisit Trigger

Prometheus metric cardinality exceeds 10,000 unique label combinations, causing Prometheus memory usage to impact CI runner stability or scrape latency to exceed 10 seconds.

## Consequences

- Phase 1 must wire `tracing-subscriber` at engine startup; all components use `tracing::instrument` or manual span/event macros.
- The Prometheus exporter must listen on a configurable port (default: 9090) from Phase 1.
- CI configuration (GitHub Actions or equivalent) must be committed in Phase 1 with all 7 checks defined, even if checks 5–7 are initially stubs that always pass.
- Grafana dashboard JSON files will be added to the repository under `infra/grafana/` at Phase 5.
- `cargo deny` configuration (`deny.toml`) must be committed in Phase 1 with at minimum a license allowlist and a ban on known-vulnerable crate versions.
