# Task 12: `crates/event-bus` Design Spec

**Version:** 0.3
**Date:** 2026-04-29
**Status:** Reviewer-approved + user feedback round 1 incorporated — pending final user approval
**Implements:** Task 12 of Phase 1 plan (`CLAUDE.md` Phase 1 task checklist).
**Depends on:** Task 11 (`crates/types`) — DONE as of commit `e2911cf`.
**References:** ADR-005, ADR-008, `docs/specs/event-model.md`, `PHASE_1_DETAIL_REVISION.md` §3.1 / §3.4.

---

## 1. Goal

Define the Phase 1 event-bus contract and ship its first concrete implementation.

The crate `crates/event-bus` introduces:
- **Two traits** — `EventBus<T>` for the producer side, `EventConsumer<T>` for the consumer side.
- **One concrete pair of impls** — `CrossbeamBoundedBus<T>` and `CrossbeamConsumer<T>`, backed by `crossbeam_channel::bounded`.
- **Three small support types** — `PublishAck`, `BusStats`, `BusError`.
- **Four metrics** — `event_bus_published_total`, `event_bus_consumed_total`, `event_bus_backpressure_total`, `event_bus_current_depth` — emitted via the `metrics` facade and also readable in-process via `BusStats`.
- **Seven inline unit tests** covering capacity validation, sequence/timestamp assignment with envelope preservation, backpressure registration, closed-channel error mapping, empty-queue try_recv semantics, retry safety on invalid meta, and sequence-exhaustion no-wrap policy.

The crate provides, at each domain pipeline stage boundary, a single logical consumer bounded queue. It does **not** put a single global queue in front of the whole engine; downstream wiring (Task 16 `crates/app`) decides how many bus instances exist and which stage owns which.

---

## 2. Scope

### In scope (what Task 12 produces)

| # | Deliverable | Notes |
|---|---|---|
| 1 | `crates/event-bus` crate created at workspace path | Single `lib.rs` (~500 LOC; split modules only if it grows past 600). |
| 2 | `EventBus<T>` trait | Producer side: `publish`, `len`, `capacity`, `stats`. |
| 3 | `EventConsumer<T>` trait | Consumer side: `recv`, `try_recv`, `len`. |
| 4 | `CrossbeamBoundedBus<T>` + `CrossbeamConsumer<T>` | Sole Phase 1 impl. |
| 5 | `PublishAck`, `BusStats`, `BusError` | `BusStats` and `BusError` carry `#[non_exhaustive]`. |
| 6 | Four metrics emitted | `metrics::counter!` / `metrics::gauge!` macros, no labels (see §8). |
| 7 | Seven inline unit tests (T1–T7) | See §10. |
| 8 | Workspace `Cargo.toml` updates | Re-add `crates/event-bus` to `members`; add `parking_lot = "0.12"` to `[workspace.dependencies]`. |

### Adjacent decisions baked in

- `EventBus<T>::publish(payload, meta) -> Result<PublishAck, BusError>`. Bus assigns sequence and timestamp internally; envelope itself flows only to the consumer side. Publish path is **clone-free**.
- Phase 1 is strict single logical consumer. `subscribe`, multi-consumer cursors, and broadcast semantics are explicitly forbidden.
- `capacity == 0` is rejected with `BusError::InvalidCapacity(0)`. Crossbeam's zero-capacity rendezvous semantics are deliberately excluded — they are inconsistent with the meanings of `current_depth`, `capacity`, and `backpressure_total` defined in this spec.
- `new(capacity) -> Result<(CrossbeamBoundedBus<T>, CrossbeamConsumer<T>), BusError>` returns the paired (bus, consumer) handles. Exactly one consumer handle per bus.
- `SmokeTestPayload` (from `rust-lmax-mev-types`) is used as the test payload type and is reached via the normal `[dependencies]` path; no `[dev-dependencies]` are required.

---

## 3. Out of Scope

| Item | Lives in | Why deferred |
|---|---|---|
| 100 000-event smoke test (CI check #5) | Task 17 (integration smoke tests) | Per ADR-008 the smoke binary is a separate harness that imports the bus crate. |
| Loading capacity from TOML | Task 14 (`crates/config`) | The bus accepts a `usize` constructor argument; config translation lands later. |
| Initializing the Prometheus exporter | Task 15 (`crates/observability`) | The `metrics` facade must be wired by the app; the bus only emits via macros. |
| Telemetry-channel saturation drop counter | Task 15 (`crates/observability`) | A separate channel from the domain bus per ADR-005. |
| Multi-consumer cursor / broadcast / `subscribe(name)` | Phase 2 event-log abstraction | Per ADR-005 + `PHASE_1_DETAIL_REVISION` §3.1 (recommendation A). |
| Lock-free ring buffer replacement | ADR-005 revisit trigger | Only after benchmark proof per ADR-005 §"Custom ring buffer". |
| Metric labels | Task 15 label-policy decision | See §8. |
| Domain event payloads (`BlockObserved`, etc.) | Phase 2 | Per Task 11 spec §8 + Phase 1 plan. |
| Direct `alloy-primitives` integration | Phase 2 domain-event crate | Per CLAUDE.md compat note. |
| Importing `EventBus` / `EventConsumer` from another crate to wire stages | Task 16 (`crates/app`) | Task 12 only ships the API and one impl. |

If scope pressure appears, reject it by pointing to this spec's Out-of-scope section.

---

## 4. Architecture & Crate Layout

### 4.1 File layout

```
crates/event-bus/
├── Cargo.toml
└── src/
    └── lib.rs        # ~500 LOC; split modules only if it grows past 600.
```

Single `lib.rs` houses both traits, both impls, the three support types, the error enum, the crate-level docstring, and `#[cfg(test)] mod tests`. Mirrors Task 11 (`crates/types`).

### 4.2 Crate dependencies

`crates/event-bus/Cargo.toml`:

```toml
[package]
name = "rust-lmax-mev-event-bus"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
publish = false
description = "Phase 1 single-consumer bounded event bus for the LMAX-style MEV engine"

[dependencies]
rust-lmax-mev-types = { path = "../types" }
crossbeam-channel = { workspace = true }
metrics = { workspace = true }
thiserror = { workspace = true }
parking_lot = { workspace = true }
```

No `[dev-dependencies]`. `SmokeTestPayload` is reached via the normal `rust-lmax-mev-types` runtime dep.

### 4.3 Workspace edits (one chore commit, mirrors Task 11's "Task 1")

1. `[workspace] members`: re-add `"crates/event-bus"` (currently trimmed to `"crates/types"` from Task 11).
2. `[workspace.dependencies]`: add `parking_lot = "0.12"`.

`parking_lot::Mutex` is chosen for the publish-path lock because of its **simpler API (no poisoning) and lighter handle**; performance is a side effect, not the rationale. Replacement of the `crossbeam-channel`-backed bus or the surrounding mutex with a lock-free ring buffer is gated on benchmark proof per ADR-005.

---

## 5. Public API Surface

### 5.1 Acknowledgements & stats

```rust
/// Returned from `publish` on success.
///
/// Carries only the bus-assigned identity of the published event; the envelope
/// itself flows exclusively to the consumer side. This is intentional —
/// keeping the publish path clone-free is the reason `publish` does not
/// return `EventEnvelope<T>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublishAck {
    pub sequence: u64,
    pub timestamp_ns: u64,
}

/// In-process readable bus snapshot.
///
/// Comprised of three counters (`published_total`, `consumed_total`,
/// `backpressure_total`), one gauge (`current_depth`), and one structural
/// quantity (`capacity`).
///
/// Each field is read independently from its underlying atomic or channel,
/// so the returned value is an **observability sample, not a linearizable
/// transaction snapshot** — concurrent publish / recv may interleave between
/// the per-field reads.
///
/// Marked `#[non_exhaustive]` so downstream callers cannot rely on exhaustive
/// construction or matching; Phase 2 may add fields while callers continue to
/// read the values through `stats()` and pattern-match with `..` rest patterns.
/// (Note: `#[non_exhaustive]` on a public struct forbids struct-literal
/// construction outside this crate, which is the desired contract here —
/// `BusStats` is produced only by `EventBus::stats()`.)
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusStats {
    pub published_total:    u64,
    pub consumed_total:     u64,
    pub backpressure_total: u64,
    pub current_depth:      usize,
    pub capacity:           usize,
}
```

### 5.2 Traits

```rust
pub trait EventBus<T>: Send + Sync
where
    T: Send + 'static,
{
    fn publish(&self, payload: T, meta: PublishMeta)
        -> Result<PublishAck, BusError>;
    fn len(&self)      -> usize;
    fn is_empty(&self) -> bool { self.len() == 0 }   // default method
    fn capacity(&self) -> usize;
    fn stats(&self)    -> BusStats;
}

pub trait EventConsumer<T>: Send + Sync
where
    T: Send + 'static,
{
    fn recv(&self)     -> Result<EventEnvelope<T>, BusError>;
    fn try_recv(&self) -> Result<Option<EventEnvelope<T>>, BusError>;
    fn len(&self)      -> usize;
    fn is_empty(&self) -> bool { self.len() == 0 }   // default method
}
```

The `Send + Sync` bound permits cross-thread movement of the handles. It does **not** force single-threaded processing at compile time. The single-logical-consumer contract is enforced by:
- The constructor returns exactly one `CrossbeamConsumer<T>`.
- `CrossbeamConsumer<T>` does **not** derive `Clone`. Multiple consumer handles cannot be created from one bus.

Sharing a single consumer across multiple worker threads (which would distribute order-dependent processing) is outside the Phase 1 contract. Producer-side sharing across threads is supported and recommended via `Arc<CrossbeamBoundedBus<T>>`.

### 5.3 Concrete impls

```rust
pub struct CrossbeamBoundedBus<T> { /* internals — see §7 */ }
pub struct CrossbeamConsumer<T>   { /* internals — see §7 */ }

impl<T> CrossbeamBoundedBus<T>
where
    T: Send + 'static,
{
    /// Constructs a paired `(bus, consumer)`.
    ///
    /// `capacity == 0` is rejected with `BusError::InvalidCapacity(0)`.
    /// Crossbeam's zero-capacity rendezvous semantics are deliberately
    /// excluded — they conflict with the Phase 1 metric definitions of
    /// `current_depth`, `capacity`, and `backpressure_total`.
    pub fn new(capacity: usize)
        -> Result<(Self, CrossbeamConsumer<T>), BusError>;
}

impl<T> EventBus<T>      for CrossbeamBoundedBus<T> where T: Send + 'static { /* ... */ }
impl<T> EventConsumer<T> for CrossbeamConsumer<T>   where T: Send + 'static { /* ... */ }
```

`CrossbeamConsumer<T>` deliberately does **not** derive or implement `Clone`.

### 5.4 Error type

```rust
/// Errors returned by `EventBus` / `EventConsumer` operations.
///
/// Marked `#[non_exhaustive]` so Phase 2 may add variants (e.g.
/// `BackpressureTimeout`, `MetricsBackendUnavailable`) without breaking
/// downstream `match` consumers — they will route the new variants through
/// the `_ =>` arm.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum BusError {
    #[error("invalid capacity: {0} (must be > 0)")]
    InvalidCapacity(usize),

    #[error("system clock unavailable, pre-epoch, or timestamp out of range")]
    ClockUnavailable,

    #[error("sequence counter exhausted (u64::MAX reached)")]
    SequenceExhausted,

    #[error("envelope construction rejected: {0}")]
    Envelope(#[from] TypesError),

    #[error("channel closed: peer dropped")]
    Closed,
}
```

`SequenceExhausted` semantics: `u64::MAX` is reserved as the exhaustion sentinel and is **never published**. Phase 1 publishable sequence range is `0..u64::MAX` (half-open); the maximum published value is `u64::MAX - 1`. See §7 for the enforcement check.

---

## 6. Anti-patterns & Phase 2 Extension Vectors

### 6.1 Anti-patterns (forbidden in Phase 1)

- ❌ Cloned receivers or multiple consumer handles used to imitate broadcast. The crate does not provide `Clone` on `CrossbeamConsumer`.
- ❌ A `subscribe(name: &str)` API or any other multi-consumer producer surface.
- ❌ A pseudo-Disruptor implementation (cursors, sequence barriers, ring-buffer slots) wired into Phase 1.
- ❌ A `publish` signature that returns `EventEnvelope<T>` and forces a clone on the producer side.
- ❌ Representing an empty queue from `try_recv` as `Err(_)` rather than `Ok(None)`.
- ❌ Allowing the bus to publish `u64::MAX` as a sequence value (it is reserved as the exhaustion sentinel).
- ❌ Silently wrapping the sequence counter at `u64` overflow.

### 6.2 Phase 2 extension vectors (additive, no breaking change required)

- New `EventBus<T>` impls (e.g., `LockFreeRingBus<T>`).
- New `EventConsumer<T>` impls (e.g., a cursor-aware multi-consumer type) — additive per ADR-005 §Consequences.
- Additional `BusStats` fields — protected by `#[non_exhaustive]`.
- Additional `BusError` variants (e.g., `BackpressureTimeout`, `MetricsBackendUnavailable`) — protected by `#[non_exhaustive]`.

---

## 7. Internal State & Publish/Recv Data Flow

### 7.1 Internal state

```rust
struct PublishState {
    next_sequence: u64,   // starts at 0; advanced by +1 only on publish success
}

pub struct CrossbeamBoundedBus<T> {
    sender:             crossbeam_channel::Sender<EventEnvelope<T>>,
    state:              parking_lot::Mutex<PublishState>,
    published_total:    AtomicU64,                  // bus-owned counter
    backpressure_total: AtomicU64,                  // bus-owned counter
    consumed_total:     Arc<AtomicU64>,             // shared with CrossbeamConsumer
    capacity:           usize,                      // cached at construction
}

pub struct CrossbeamConsumer<T> {
    receiver:       crossbeam_channel::Receiver<EventEnvelope<T>>,
    consumed_total: Arc<AtomicU64>,                 // same Arc as above
}
```

`current_depth` is **not** stored as a separate atomic. It is derived live from `sender.len()` (or `receiver.len()`, equivalent for crossbeam-channel) at every read site.

### 7.2 Mutex role

The `parking_lot::Mutex<PublishState>` serializes the entire publish path — including the potentially blocking `send` — for two reasons:

1. **Sequence ordering matches channel ordering.** Without the lock, an `AtomicU64::fetch_add` for the sequence followed by a separate `send` could allow publisher A to acquire `sequence = N` while publisher B's `sequence = N+1` reaches the channel first. In the Phase 1 LMAX-style single ordered stream, sequence and channel order must agree.
2. **Backpressure propagates to other publishers.** When the lock holder is parked inside a blocking `send`, other publishers wait serially in the mutex. This propagates backpressure to all publishers without the bus needing additional synchronization. This is propagation, not strict fairness — `parking_lot::Mutex` and the OS scheduler do not guarantee strict FIFO acquisition.

`stats()` **must not** acquire the publish-state mutex. It reads only the atomics (`published_total`, `consumed_total`, `backpressure_total`) and the channel `len`/`capacity`. This is a hard correctness requirement — T3 (§10) creates a scenario where a publisher thread is blocked inside the publish path with the lock held, and the test must call `stats()` from the main thread without deadlocking.

### 7.3 Publish path (lock held end-to-end)

```text
1. let mut state = self.state.lock();
2. let timestamp_ns = now_ns()?;                         // ClockUnavailable on failure
3. let sequence = state.next_sequence;
4. if sequence == u64::MAX { return Err(SequenceExhausted); }
5. let envelope = EventEnvelope::seal(meta, payload, sequence, timestamp_ns)?;
                                                          // Envelope(TypesError) on seal failure
6. match self.sender.try_send(envelope) {
       Ok(())                       => { /* fast path */ }
       Err(Full(env))               => {
           self.backpressure_total.fetch_add(1, Ordering::Relaxed);
           metrics::counter!("event_bus_backpressure_total").increment(1);
           self.sender.send(env).map_err(|_| BusError::Closed)?;
                                                          // blocking send; lock still held.
                                                          // §7.2's "stats() must not acquire the
                                                          // publish-state mutex" rule exists
                                                          // precisely so a main thread can call
                                                          // stats() while another publisher is
                                                          // parked here — see T3 in §10.
       }
       Err(Disconnected(_))         => return Err(BusError::Closed);
   }
7. state.next_sequence = sequence + 1;                    // success path only
8. self.published_total.fetch_add(1, Ordering::Relaxed);
   metrics::counter!("event_bus_published_total").increment(1);
9. metrics::gauge!("event_bus_current_depth").set(self.sender.len() as f64);
10. drop(state);
11. Ok(PublishAck { sequence, timestamp_ns })
```

**Hard invariant.** If any of steps 2–6 returns `Err`, `state.next_sequence` is **not advanced**. The same sequence number is reserved for the next publish attempt. This guarantees retry safety for transient errors (`ClockUnavailable`, `Envelope(_)`).

**Exception.** `SequenceExhausted` is terminal — `next_sequence` is already at `u64::MAX` and no further publishes are accepted. The bus instance must be discarded. (`Closed` is also effectively terminal, since the receiver is gone.)

`now_ns()` implementation:

```rust
fn now_ns() -> Result<u64, BusError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| u64::try_from(d.as_nanos()).ok())
        .filter(|&ns| ns != 0)
        .ok_or(BusError::ClockUnavailable)
}
```

The three rejected cases — pre-epoch, `u128 → u64` overflow, exact-zero — all map to `ClockUnavailable`. Exact-zero is included because the `EventEnvelope::seal` invariant (`timestamp_ns != 0`) would otherwise reject the envelope at the next step, which would be confusing as a "ClockUnavailable" -> "Envelope" cascade.

**Timestamp semantics (binding contract).**

> `timestamp_ns` is captured before a potentially blocking send. It may therefore be earlier than the actual enqueue/receive time. Ordering is defined by `sequence`, not timestamp monotonicity.

### 7.4 Recv path (no lock)

```text
recv()      → self.receiver.recv()
              Ok(env)            → consumed_total++ ;
                                   metrics::counter!("event_bus_consumed_total").increment(1) ;
                                   metrics::gauge!("event_bus_current_depth").set(self.receiver.len() as f64) ;
                                   return Ok(env)
              Err(RecvError)     → return Err(BusError::Closed)

try_recv()  → self.receiver.try_recv()
              Ok(env)            → consumed_total++ ;
                                   metrics::counter!("event_bus_consumed_total").increment(1) ;
                                   metrics::gauge!("event_bus_current_depth").set(self.receiver.len() as f64) ;
                                   return Ok(Some(env))
              Err(Empty)         → return Ok(None)         // ← consumed_total NOT advanced
              Err(Disconnected)  → return Err(BusError::Closed)
```

`try_recv` distinguishes "nothing right now" (`Ok(None)`, not an error) from "channel will never produce again" (`Err(Closed)`). The `Ok(None)` branch performs no metric or counter side effects.

### 7.5 Sequence assignment ownership

Sequence assignment is solely the bus's responsibility. Callers never set or guess sequences; they pass `PublishMeta` and receive the bus-assigned identity in the returned `PublishAck`. This matches `PHASE_1_DETAIL_REVISION` §3.4 recommendation A and the Task 11 contract on `EventEnvelope::seal`.

---

## 8. Metrics

### 8.1 Four metrics, emission map

| Metric name | Type | Where updated | In-process source |
|---|---|---|---|
| `event_bus_published_total` | counter | publish success, inside lock (step 8) | `published_total: AtomicU64` (bus-owned) |
| `event_bus_consumed_total` | counter | `recv()` success / `try_recv() == Ok(Some(_))` | `consumed_total: Arc<AtomicU64>` (shared by bus + consumer) |
| `event_bus_backpressure_total` | counter | publish hits `try_send → Full` (step 6, before blocking `send`) | `backpressure_total: AtomicU64` (bus-owned) |
| `event_bus_current_depth` | gauge | publish success + `recv` / `try_recv` Some | derived live from `sender.len()` / `receiver.len()`; **no separate atomic** |

`event_bus_backpressure_total` is a **full-queue encounter count** — incremented exactly when `try_send` returns `Full`. It does **not** measure how long the subsequent `send` blocks; if the consumer drains immediately, the blocking `send` may complete with no observable wait.

`event_bus_current_depth` is set, not incremented or decremented (`metrics::gauge!("...").set(x as f64)`). The bus does not maintain a separate atomic for depth; `stats().current_depth` is read via `sender.len()` and is therefore an observability sample, consistent with the `BusStats` contract in §5.1.

The `usize as f64` cast loses precision only above 2^53 ≈ 9 × 10¹⁵. Phase 1 capacity is bounded by config and never approaches this limit, so the cast is safe. A `clippy::cast_precision_loss` lint here may be silenced with a local `#[allow(...)]` plus an explanatory comment, or with the workspace lint config in Task 18 — implementer's choice at plan time.

### 8.2 Label policy

Phase 1 Task 12 emits no labels because the label policy is deferred to Task 15 (`crates/observability`). If more than one event-bus instance is wired into the running app before Task 15 is complete, metric labels become **mandatory before merge** — without per-instance labels, Prometheus time series from multiple buses collide.

### 8.3 In-process counters vs `metrics` facade

The three counters (`published_total`, `consumed_total`, `backpressure_total`) are emitted through the `metrics` facade (consumed by the Prometheus exporter once Task 15 is wired) **and** mirrored in in-process `AtomicU64` counters exposed through `BusStats`. The two paths stay synchronized because the same publish/recv code updates both within the same critical section (publish) or before returning (recv).

`current_depth` is emitted as a gauge via `metrics::gauge!` and read in-process directly from `sender.len()` / `receiver.len()`; it has **no separate `AtomicU64` mirror**. `stats().current_depth` is the same channel-len observability sample.

The `metrics` facade does not let callers read back current counter values — only the exporter consumes them. The in-process atomics exist to bridge that read-back gap for tests and runtime introspection.

---

## 9. Error Matrix

| Variant | Emit site | Trigger condition | `state.next_sequence` after error | Recommended caller action |
|---|---|---|---|---|
| `InvalidCapacity(usize)` | `CrossbeamBoundedBus::new` | `capacity == 0` | n/a (bus never built) | Retry with `capacity > 0`. |
| `ClockUnavailable` | publish step 2 (`now_ns()`) | `SystemTime::now() < UNIX_EPOCH`, nanos exceed `u64`, or nanos is exactly 0 | unchanged | Inspect host clock; retry. Same sequence is reserved for the next attempt. |
| `SequenceExhausted` | publish step 4 | `state.next_sequence == u64::MAX` | unchanged (already `u64::MAX`) | **Terminal**: discard the bus instance and create a new one. |
| `Envelope(TypesError)` | publish step 5 (`EventEnvelope::seal`) | Caller-supplied `PublishMeta` violates a Phase 1 envelope invariant (e.g. `chain_id == 0`, `event_version == 0`) | unchanged | Fix `PublishMeta`; retry with same payload. Same sequence is reserved. |
| `Closed` (publish-side) | publish step 6 (`try_send` Disconnected, or blocking `send` Disconnected) | Consumer dropped | unchanged (no advance) | **Bus is dead**: discard the instance. Do not retry — there is no receiver. |
| `Closed` (recv-side) | `recv` / `try_recv` (`RecvError` / `TryRecvError::Disconnected`) | Bus dropped | n/a | Discard the consumer; the producer side is gone. |

The "success path is the only path that advances `next_sequence`" rule, combined with `#[non_exhaustive]` on `BusError`, ensures retry safety for all transient errors and additive room for new variants in Phase 2.

---

## 10. Test Plan — 7 inline tests

All tests live in `crates/event-bus/src/lib.rs` inside `#[cfg(test)] mod tests`. The payload type is `rust_lmax_mev_types::SmokeTestPayload`, reached via the runtime dep. There are no `[dev-dependencies]`.

Helpers (defined once at the top of the test module):

```rust
fn meta() -> PublishMeta { /* ChainId 1, block_number 18_000_000, event_version 1, correlation_id 42 */ }
fn payload(nonce: u64) -> SmokeTestPayload { SmokeTestPayload { nonce, data: [0xCD; 32] } }
```

The tests rely on `PublishMeta: Clone`, which is already derived in Task 11 (`crates/types/src/lib.rs:73`). No additional derive is required.

### T1 `new_rejects_zero_capacity`

Verifies the rendezvous-semantics rejection.

```rust
#[test]
fn new_rejects_zero_capacity() {
    // Note: `Result::expect_err` would require the Ok variant
    // `(CrossbeamBoundedBus<T>, CrossbeamConsumer<T>)` to implement `Debug`,
    // which is not part of the Phase 1 contract. Use an explicit `match`
    // instead so the test does not silently demand a Debug derive on the
    // bus/consumer pair.
    let err = match CrossbeamBoundedBus::<SmokeTestPayload>::new(0) {
        Err(err) => err,
        Ok(_)    => panic!("capacity 0 must reject"),
    };
    assert!(matches!(err, BusError::InvalidCapacity(0)));
}
```

### T2 `publish_assigns_sequence_nonzero_timestamp_and_preserves_envelope`

Verifies sequence starts at 0 and advances monotonically, timestamp invariant holds, and **all `PublishMeta` fields plus the payload are preserved on the consumer-received envelope**, while ack identity matches the received envelope's identity.

```rust
#[test]
fn publish_assigns_sequence_nonzero_timestamp_and_preserves_envelope() {
    let (bus, consumer) = CrossbeamBoundedBus::<SmokeTestPayload>::new(8)
        .expect("capacity 8 valid");

    let m = meta();
    let p = payload(7);

    let ack0 = bus.publish(payload(0), m.clone()).expect("publish 0");
    let ack1 = bus.publish(payload(1), m.clone()).expect("publish 1");
    let ack2 = bus.publish(p.clone(), m.clone()).expect("publish 2");

    assert_eq!([ack0.sequence, ack1.sequence, ack2.sequence], [0, 1, 2]);
    assert!(ack0.timestamp_ns != 0);
    assert!(ack1.timestamp_ns != 0);
    assert!(ack2.timestamp_ns != 0);
    // No timestamp monotonicity assertion — wall clock may move backward.

    // Drain and verify the third envelope matches its ack and preserves all meta + payload.
    let _e0 = consumer.recv().expect("recv 0");
    let _e1 = consumer.recv().expect("recv 1");
    let e2 = consumer.recv().expect("recv 2");

    assert_eq!(e2.sequence(),       ack2.sequence);
    assert_eq!(e2.timestamp_ns(),   ack2.timestamp_ns);
    assert_eq!(e2.source(),         m.source);
    assert_eq!(e2.event_version(),  m.event_version);
    assert_eq!(e2.correlation_id(), m.correlation_id);
    assert_eq!(e2.chain_context(),  &m.chain_context);
    assert_eq!(e2.payload(),        &p);

    let stats = bus.stats();
    assert_eq!(stats.published_total,    3);
    assert_eq!(stats.consumed_total,     3);
    assert_eq!(stats.backpressure_total, 0);
    assert_eq!(stats.current_depth,      0);
    assert_eq!(stats.capacity,           8);
}
```

### T3 `publish_registers_backpressure_when_full_and_completes_after_recv`

Verifies that a publish that hits a full queue increments `backpressure_total` and eventually completes once the consumer drains a slot. The `Instant`-based deadline (3 seconds) is a hang guard, **not a sleep** — it never elapses on a healthy CI machine and exists only so a future bug cannot hang the test indefinitely.

`stats()` is called from the main thread while a publisher thread holds the publish-state mutex; this requires the mandate from §7.2 that `stats()` must not acquire the publish-state mutex.

```rust
#[test]
fn publish_registers_backpressure_when_full_and_completes_after_recv() {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    let (bus, consumer) = CrossbeamBoundedBus::<SmokeTestPayload>::new(2)
        .expect("capacity 2 valid");
    let bus = Arc::new(bus);

    let ack0 = bus.publish(payload(0), meta()).expect("publish 0");
    let ack1 = bus.publish(payload(1), meta()).expect("publish 1");
    assert_eq!([ack0.sequence, ack1.sequence], [0, 1]);
    assert_eq!(bus.len(), 2);
    assert_eq!(bus.stats().backpressure_total, 0);

    let bus_t = Arc::clone(&bus);
    let handle = std::thread::spawn(move || bus_t.publish(payload(2), meta()));

    // backpressure_total advances inside the publish lock just before the
    // blocking send, so a non-zero value implies the publisher thread has
    // entered the full-queue branch. yield_now() avoids burning CPU; the
    // 3s deadline is a bug guard only.
    let deadline = Instant::now() + Duration::from_secs(3);
    while bus.stats().backpressure_total == 0 {
        assert!(
            Instant::now() < deadline,
            "publisher thread never registered backpressure within 3s",
        );
        std::thread::yield_now();
    }
    assert_eq!(bus.stats().backpressure_total, 1);

    let env0 = consumer.recv().expect("drain 0");
    assert_eq!(env0.sequence(), 0);

    let ack2 = handle.join().expect("publisher thread did not panic")
        .expect("publish 2 succeeded after drain");
    assert_eq!(ack2.sequence, 2);

    let env1 = consumer.recv().expect("drain 1");
    let env2 = consumer.recv().expect("drain 2");
    assert_eq!(env1.sequence(), 1);
    assert_eq!(env2.sequence(), 2);

    let stats = bus.stats();
    assert_eq!(stats.published_total,    3);
    assert_eq!(stats.consumed_total,     3);
    assert_eq!(stats.backpressure_total, 1);
    assert_eq!(stats.current_depth,      0);
    assert_eq!(stats.capacity,           2);
}
```

### T4 `publish_after_consumer_drop_returns_closed`

Verifies that dropping the consumer yields `BusError::Closed` on subsequent publish, and that the failed publish does not consume a sequence (inferred from `published_total == 0`).

```rust
#[test]
fn publish_after_consumer_drop_returns_closed() {
    let (bus, consumer) = CrossbeamBoundedBus::<SmokeTestPayload>::new(4)
        .expect("capacity 4 valid");
    drop(consumer);

    let err = bus.publish(payload(0), meta()).expect_err("publish must fail");
    assert!(matches!(err, BusError::Closed));

    // Failed publish does not advance the sequence counter.
    assert_eq!(bus.stats().published_total, 0);
}
```

### T5 `try_recv_empty_returns_none_and_recv_after_bus_drop_returns_closed`

Verifies the `Ok(None)` semantics on empty queues, `consumed_total` accounting on `Some` vs `None`, and the `Closed` mapping when the bus is dropped.

```rust
#[test]
fn try_recv_empty_returns_none_and_recv_after_bus_drop_returns_closed() {
    let (bus, consumer) = CrossbeamBoundedBus::<SmokeTestPayload>::new(2)
        .expect("capacity 2 valid");

    // (a) try_recv on empty queue: Ok(None), consumed_total stays at 0.
    assert!(consumer.try_recv().expect("try_recv ok").is_none());
    assert_eq!(consumer.len(), 0);
    assert_eq!(bus.stats().consumed_total, 0);

    // (b) After one publish, try_recv returns Ok(Some(_)) and consumed_total advances.
    let ack = bus.publish(payload(0), meta()).expect("publish");
    let env = consumer.try_recv().expect("try_recv ok").expect("event present");
    assert_eq!(env.sequence(), ack.sequence);
    assert_eq!(bus.stats().consumed_total, 1);

    // (c) After dropping the bus, recv returns BusError::Closed.
    drop(bus);
    let err = consumer.recv().expect_err("recv must be closed");
    assert!(matches!(err, BusError::Closed));
}
```

### T6 `publish_rejects_invalid_meta_without_consuming_sequence`

Verifies the core retry-safety invariant: a failed publish (Envelope(TypesError)) does **not** advance `next_sequence`, so the next valid publish reuses the would-have-been sequence.

```rust
#[test]
fn publish_rejects_invalid_meta_without_consuming_sequence() {
    let (bus, consumer) = CrossbeamBoundedBus::<SmokeTestPayload>::new(2)
        .expect("capacity 2 valid");

    // Invalid meta: chain_id = 0 violates Phase 1 envelope invariant.
    let mut bad_meta = meta();
    bad_meta.chain_context.chain_id = 0;

    let err = bus.publish(payload(0), bad_meta)
        .expect_err("invalid meta must reject");
    assert!(matches!(err, BusError::Envelope(_)));
    assert_eq!(bus.stats().published_total, 0);

    // Valid retry must reuse sequence = 0.
    let ack = bus.publish(payload(0), meta()).expect("valid publish");
    assert_eq!(ack.sequence, 0);

    let env = consumer.recv().expect("recv");
    assert_eq!(env.sequence(), 0);
    assert_eq!(bus.stats().published_total, 1);
}
```

### T7 `sequence_exhausted_does_not_wrap` (white-box)

Verifies the no-wrap policy at the `u64::MAX` boundary. This test relies on the same-module visibility of the **private** `state` field on `CrossbeamBoundedBus<T>` — the field has no `pub` or `pub(crate)` modifier and is invisible outside `lib.rs`. Tests live inside `#[cfg(test)] mod tests` in `lib.rs`, which can read and mutate the parent module's private fields. This is the only test in the suite that crosses encapsulation; all others use the public API.

```rust
#[test]
fn sequence_exhausted_does_not_wrap() {
    let (bus, consumer) = CrossbeamBoundedBus::<SmokeTestPayload>::new(2)
        .expect("capacity 2 valid");

    // Force the boundary: next publish would attempt sequence == u64::MAX.
    bus.state.lock().next_sequence = u64::MAX;

    let err = bus.publish(payload(0), meta())
        .expect_err("must be SequenceExhausted at u64::MAX");
    assert!(matches!(err, BusError::SequenceExhausted));

    // No advance, no envelope sent, and crucially no progress past step 4 of
    // §7.3: backpressure_total and current_depth must both be 0 to confirm
    // the publish path returned before reaching seal/try_send.
    assert_eq!(bus.state.lock().next_sequence, u64::MAX);
    assert!(consumer.try_recv().expect("try_recv ok").is_none());
    let stats = bus.stats();
    assert_eq!(stats.published_total,    0);
    assert_eq!(stats.backpressure_total, 0);
    assert_eq!(stats.current_depth,      0);
}
```

---

## 11. Definition of Done

- [ ] **D1** Workspace `Cargo.toml`: `[workspace] members` re-adds `crates/event-bus`; `[workspace.dependencies]` adds `parking_lot = "0.12"`.
- [ ] **D2** `crates/event-bus/Cargo.toml` matches §4.2 verbatim.
- [ ] **D3** `EventBus<T>` and `EventConsumer<T>` traits defined with `Send + Sync` and `where T: Send + 'static`, signatures matching §5.2 (including the `is_empty()` default method).
- [ ] **D4** `CrossbeamBoundedBus<T>` and `CrossbeamConsumer<T>` structs implemented. **`CrossbeamConsumer<T>` does not derive or implement `Clone`.**
- [ ] **D5** `PublishAck`, `BusStats` (`#[non_exhaustive]`), and `BusError` (`#[non_exhaustive]`, 5 variants) defined per §5.1 / §5.4.
- [ ] **D6** Publish path enforces invariants: `parking_lot::Mutex` held end-to-end across the `try_send` → optional blocking `send` flow; `state.next_sequence` advanced **only** on success; `try_send → Full` increments `backpressure_total` (both atomic and metric counter) before the blocking `send`; `SequenceExhausted` fires before `seal` when `next_sequence == u64::MAX`.
- [ ] **D7** `recv` / `try_recv` advance `consumed_total` (atomic + metric counter) only on the `Some` path; `Ok(None)` from `try_recv` performs no side effects.
- [ ] **D8** All four metrics emit through the `metrics` facade. The three counters (`published_total`, `consumed_total`, `backpressure_total`) keep `AtomicU64` in-process counters. `current_depth` does **not** keep a separate atomic — it is read live from `sender.len()` / `receiver.len()` and `set` on the `metrics` gauge. `BusStats::current_depth` is the same channel-len observability sample. **`stats()` must not acquire the publish-state mutex; it reads only the atomics and the channel `len`/`capacity`.** This is a hard correctness requirement — T3 calls `stats()` from the main thread while a publisher thread holds the lock.
- [ ] **D9** Seven inline tests (T1–T7) all PASS.
- [ ] **D10** Run in this order — `cargo fmt --check`, `cargo build -p rust-lmax-mev-event-bus`, `cargo test -p rust-lmax-mev-event-bus`, `cargo clippy -p rust-lmax-mev-event-bus -- -D warnings`. All clean. (Order surfaces formatting / compile / logic / lint failures from cheapest to most expensive.) Additionally, `cargo clippy -p rust-lmax-mev-event-bus --all-targets -- -D warnings` must pass as a Task 18 CI preview.
- [ ] **D11** Crate-level docstring + per-item docstrings on `EventBus`, `EventConsumer`, `CrossbeamBoundedBus::new`, `publish`, `recv`, `try_recv`, `stats`, `is_empty`, `PublishAck`, `BusStats`, and `BusError`. The crate docstring covers: timestamp/ordering semantics, sequence ownership, the `stats()` non-linearizability + no-mutex-acquisition contract, and the `#[non_exhaustive]` extension policy.

---

## 12. References

- **ADR-005** — Event Bus Implementation Policy. Phase 1 = single logical consumer + `crossbeam::channel::bounded` + domain-event block-on-full + telemetry-event drop counter. Multi-consumer cursors deferred to Phase 2; lock-free ring buffer requires benchmark proof.
- **ADR-008** — Observability & CI Baseline. `tracing` + `metrics` facade + Prometheus exporter; CI 7-check baseline including bus smoke (Task 17).
- **`docs/specs/event-model.md`** — `EventEnvelope<T>` schema, the bus-assigned `sequence` / `timestamp_ns` fields, the `PublishMeta` shape passed by the caller.
- **`PHASE_1_DETAIL_REVISION.md`** — §3.1 "EventBus 의미론" (single logical consumer recommendation, anti-patterns), §3.4 "Sequence assignment" (recommendation A: `publish(payload, meta) -> Result<EventEnvelope, _>` — adopted in spirit but refined to return `PublishAck` to keep the publish path clone-free).
- **`crates/types/src/lib.rs`** — `EventEnvelope::seal(meta, payload, sequence, timestamp_ns)` invariant contract (Task 11).
- **`CLAUDE.md`** — Phase 1 task checklist + AI-agent notes on `consumed_total` Arc sharing and metrics macro registration.
