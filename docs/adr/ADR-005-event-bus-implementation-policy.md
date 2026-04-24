# ADR-005: Event Bus Implementation Policy

**Date:** 2026-04-24
**Status:** Accepted

## Context

The engine is structured around an LMAX-style event bus: a single ordered stream of domain events processed by a chain of handlers (mempool ingestion, simulation, strategy, bundle construction, journaling, telemetry). The bus implementation affects latency, backpressure behavior, and correctness guarantees.

Key questions:
- Single consumer vs. multi-consumer cursor model?
- Bounded vs. unbounded queue?
- What happens when the queue is full?
- Which Rust channel primitive to start with?
- When (if ever) to build a custom ring buffer?

## Decision

### Phase 1 baseline
- The event bus is **single logical consumer** per domain pipeline stage.
- Implementation: `crossbeam::channel::bounded` with a capacity configured at startup.
- **Domain events** (block arrivals, mempool transactions, simulation results, bundle outcomes) **block on full** — backpressure propagates to the producer. Events are never silently dropped.
- **Telemetry events** (metrics, trace spans) may be dropped when the telemetry channel is saturated. A saturation counter (atomic u64) is incremented on every drop and exported as a Prometheus metric.

### Phase 2+
- Multi-consumer cursor semantics (multiple independent readers at different offsets, like LMAX Disruptor) are **deferred to Phase 2** and only added if Phase 2 benchmarks show single-consumer throughput is insufficient.

### Custom ring buffer
- A custom lock-free ring buffer (Disruptor-style) is only built **after benchmark proof** that `crossbeam::channel::bounded` is the measured bottleneck (p99 latency on the event bus exceeds the per-phase latency budget with no other obvious cause).

## Rationale

- Starting with `crossbeam::channel::bounded` gives correct backpressure semantics, is well-tested, and eliminates the risk of subtle ring buffer bugs during the thin-path phases.
- Blocking on full for domain events is the safest correctness posture: no event is lost, and the system slows down visibly rather than silently discarding data.
- Allowing telemetry drops with a counter avoids the observability pipeline becoming a DoS vector on the domain pipeline during load spikes.
- Deferring multi-consumer cursors avoids premature complexity; a single logical consumer is sufficient for the thin-path strategy pipeline.
- The custom ring buffer gate ("benchmark proof first") prevents engineers from gold-plating the bus before the system is even profiled.

## Revisit Trigger

`crossbeam::channel::bounded` p99 latency on the event bus exceeds the per-phase latency budget as measured by the Phase 2 bus smoke benchmark (100k events, defined in ADR-008 CI checks).

## Consequences

- Phase 1 bus implementation is `crossbeam::channel::bounded`; no other channel type is used for domain events.
- A `TelemetrySaturation` counter must be wired to Prometheus from Phase 1.
- The bus capacity parameter must be exposed in the engine config file (not hardcoded) to allow tuning without recompilation.
- Multi-consumer cursor work (if triggered) must not break existing single-consumer pipeline stages; it must be an additive change.
- Any future custom ring buffer must pass the same bus smoke benchmark before replacing `crossbeam`.
