# Task 12: `crates/event-bus` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan version:** 0.2 (revised after user review of v0.1).

**Goal:** Implement the `crates/event-bus` crate per `docs/superpowers/specs/2026-04-29-task-12-event-bus-design.md` (v0.3 Approved). Output: a Lean single-file crate exposing `EventBus<T>` + `EventConsumer<T>` traits, `CrossbeamBoundedBus<T>` + `CrossbeamConsumer<T>` impls, three support types (`PublishAck`, `BusStats`, `BusError`), four metrics emitted via the `metrics` facade, and seven inline TDD-driven unit tests, plus the workspace `Cargo.toml` updates required for it to build.

**Architecture:** Single `lib.rs` (~500–600 LOC) housing every type and the `#[cfg(test)] mod tests`. Publish path is serialized end-to-end by a `parking_lot::Mutex<PublishState>` so that sequence assignment, the optional blocking `send`, and the success-only sequence advance all happen atomically per publisher. `try_send → Full` increments `backpressure_total` exactly once before the blocking `send` fallback. Consumer side is lock-free; `consumed_total` is shared via `Arc<AtomicU64>` so the bus can read the value back through `stats()` without coordinating with the consumer.

**Tech Stack:** Rust 1.80, edition 2021. Runtime deps: `rust-lmax-mev-types` (workspace path), `crossbeam-channel = "0.5"`, `metrics = "0.23"`, `thiserror = "2"`, `parking_lot = "0.12"` (newly added to `[workspace.dependencies]`). No `[dev-dependencies]` — `SmokeTestPayload` reaches tests via the normal `rust-lmax-mev-types` runtime dep.

**Execution shell:** Windows 11 + PowerShell. Same conventions as Task 11's plan:
- GNU CLI shims (`cat.exe`, `mkdir.exe`, ...) exist via Scoop but PowerShell aliases shadow them. Append `.exe` if GNU semantics needed.
- Multi-line strings to native executables (`git commit -m`) use **multiple `-m` arguments**, one per paragraph. Avoids here-string parser edge cases. Single-quoted `-m '...'` arguments cannot contain literal apostrophes — phrasing avoids them.
- Line-continuation is the backtick (`` ` ``) at end of line.

---

## Pre-flight Reading

Before starting, the implementer **must** read:

1. **Spec:** `docs/superpowers/specs/2026-04-29-task-12-event-bus-design.md` (v0.3 Approved). The contract.
2. **Sibling plan (style reference):** `docs/superpowers/plans/2026-04-27-task-11-types-crate-implementation.md`. Mirrors the conventions used here.
3. **Frozen specs:** `docs/specs/event-model.md` (envelope schema), `docs/adr/ADR-005-event-bus-implementation-policy.md` (single-consumer + crossbeam bounded + block-on-full), `docs/adr/ADR-008-observability-ci-baseline.md` (metrics facade).
4. **Phase 1 detail revision:** `PHASE_1_DETAIL_REVISION.md` §3.1 (single-consumer recommendation), §3.4 (sequence assignment recommendation A — refined to `PublishAck` return).
5. **Already-shipped types crate:** `crates/types/src/lib.rs`. Confirm `EventEnvelope::seal(meta, payload, sequence, timestamp_ns)` signature, `PublishMeta` field set, `EventEnvelope` accessor methods (`.sequence()`, `.timestamp_ns()`, `.source()`, etc.), and that `PublishMeta`, `ChainContext`, `EventSource`, `SmokeTestPayload` all derive `Clone`.

The plan below assumes these are read. Spec section references use `§N.M` to point into the spec.

---

## File Structure

| Path | Action | Responsibility |
|---|---|---|
| `Cargo.toml` (workspace root) | **Modify** | (a) re-add `crates/event-bus` to `members`. (b) add `parking_lot = "0.12"` to `[workspace.dependencies]`. |
| `crates/event-bus/Cargo.toml` | **Create** | Manifest matching spec §4.2 byte-for-byte. |
| `crates/event-bus/src/lib.rs` | **Create** | Single source file: crate docstring, support types, traits, concrete impls, `now_ns` + `depth_as_f64` helpers, `#[cfg(test)] mod tests`. |

**No other files are touched.** Tests are inline in `lib.rs`; there is no `tests/` directory for this crate (spec §10 + §4.1).

---

## Why workspace and crate scaffolding land in one commit

`Cargo.toml`'s `[workspace] members` list was trimmed to only `crates/types` in Task 11. Task 12 adds `crates/event-bus` back. Adding a member entry while the corresponding `crates/event-bus/Cargo.toml` does not yet exist puts the workspace into a state where `cargo metadata` (and any `cargo` invocation that loads the workspace) fails with "manifest path … does not exist". A commit in that state would be a broken-checkout commit.

To avoid it, **Task 1 lands the workspace edit, the crate manifest, and the placeholder `lib.rs` in a single commit**. This collapses the original two-task scaffold pattern from Task 11's plan into one. `parking_lot = "0.12"` is added to `[workspace.dependencies]` in the same commit because it is the only new workspace dep this task introduces and it serves the new crate exclusively.

---

## Task 1: Workspace + crate scaffold

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Create: `crates/event-bus/Cargo.toml`
- Create: `crates/event-bus/src/lib.rs`

Goal: a buildable empty crate with the workspace already updated. Single commit.

- [ ] **Step 1.1: Read current workspace manifest**

```powershell
Get-Content Cargo.toml
```

Confirm lines 3–7 list only `crates/types` in `members` (with the comment about Task 12–16 re-adds), and that `[workspace.dependencies]` does not contain `parking_lot`.

- [ ] **Step 1.2: Re-add `crates/event-bus` to `members`**

Replace:

```toml
members = [
    "crates/types",
    # crates/event-bus, crates/journal, crates/config, crates/observability,
    # crates/app are added in Task 12-16 as those crates are created.
]
```

with:

```toml
members = [
    "crates/types",
    "crates/event-bus",
    # crates/journal, crates/config, crates/observability, crates/app
    # are added in Task 13-16 as those crates are created.
]
```

- [ ] **Step 1.3: Add `parking_lot` to `[workspace.dependencies]`**

Insert `parking_lot = "0.12"` into the `[workspace.dependencies]` block. Place it next to `crossbeam-channel` since both are concurrency primitives:

```toml
# channels
crossbeam-channel = "0.5"
parking_lot = "0.12"
```

- [ ] **Step 1.4: Create the directory tree**

```powershell
New-Item -ItemType Directory -Path crates/event-bus/src -Force | Out-Null
```

- [ ] **Step 1.5: Write `crates/event-bus/Cargo.toml`**

Exact content (matches spec §4.2):

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

- [ ] **Step 1.6: Write minimal `crates/event-bus/src/lib.rs`**

Single placeholder line for now:

```rust
// Task 12 — event-bus crate. Contents are added incrementally per the implementation plan.
```

- [ ] **Step 1.7: Verify the workspace metadata loads**

```powershell
cargo metadata --format-version 1 --no-deps | Out-Null
```

Expected: exits 0 with no output. **Must pass before commit.** If it errors with "manifest path … does not exist", recheck that Step 1.5 wrote the crate `Cargo.toml` correctly.

- [ ] **Step 1.8: Verify the crate builds**

```powershell
cargo check -p rust-lmax-mev-event-bus
```

Expected: compiles with no errors and no warnings (an empty lib has no warnings to emit). **Must pass before commit.**

- [ ] **Step 1.9: Single commit covering all three files**

```powershell
git add Cargo.toml crates/event-bus/Cargo.toml crates/event-bus/src/lib.rs
git commit `
    -m 'chore(event-bus): scaffold rust-lmax-mev-event-bus crate and update workspace' `
    -m 'Workspace edits: re-adds crates/event-bus to [workspace] members for Task 12, reversing the Task 11 trim. Other Phase 1 crates (journal, config, observability, app) re-add their member entries in Task 13-16 as those crates are created. Adds parking_lot = "0.12" to [workspace.dependencies] so the new crate can use parking_lot::Mutex for the publish-path lock per spec section 4.3.' `
    -m 'Crate scaffold: empty buildable crate with rust-lmax-mev-types path-dep, crossbeam-channel, metrics, thiserror, and parking_lot runtime deps per spec section 4.2. No dev-dependencies because SmokeTestPayload reaches tests via the rust-lmax-mev-types runtime dep. Subsequent tasks fill the type definitions, trait impls, and TDD tests inline in lib.rs.' `
    -m 'parking_lot is chosen for the simpler API (no poisoning) and lighter handle, not for performance. Replacement of the channel/mutex stack with a lock-free ring buffer remains gated on benchmark proof per ADR-005.' `
    -m 'Workspace edit and crate scaffold land in a single commit because adding the member entry without the crate manifest would put cargo metadata into a failure state. Mirrors the Task 11 first-commit pattern.' `
    -m 'Co-Authored-By: Claude <noreply@anthropic.com>'
```

---

## Task 2: Support types — `PublishAck`, `BusStats`, `BusError`

**Files:**
- Modify: `crates/event-bus/src/lib.rs`

Goal: add the three non-trait public types. All transparent data carriers; no methods on `PublishAck`/`BusStats`. `BusError` uses `thiserror` for `Display` + `Error` impls. `BusStats` and `BusError` carry `#[non_exhaustive]` per spec §5.1 / §5.4.

- [ ] **Step 2.1: Replace `lib.rs` content with imports + the three types**

```rust
// Crate-level docstring is added in Task 11. Traits, concrete impls, and tests
// land in subsequent tasks.

use rust_lmax_mev_types::TypesError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublishAck {
    pub sequence: u64,
    pub timestamp_ns: u64,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusStats {
    pub published_total: u64,
    pub consumed_total: u64,
    pub backpressure_total: u64,
    pub current_depth: usize,
    pub capacity: usize,
}

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

- [ ] **Step 2.2: Verify the crate builds**

```powershell
cargo check -p rust-lmax-mev-event-bus
```

Expected: compiles. Possible warnings on unused variants (`ClockUnavailable`, `SequenceExhausted`, `Closed`) since nothing constructs them yet — these go away in Tasks 4–6. **Do not** silence them with `#[allow(dead_code)]`; same as Task 11's policy on `UnsupportedEventVersion`.

- [ ] **Step 2.3: Commit**

```powershell
git add crates/event-bus/src/lib.rs
git commit `
    -m 'feat(event-bus): add PublishAck, BusStats, BusError support types' `
    -m 'PublishAck is the clone-free identity carrier returned from publish() per spec section 5.1; envelope itself flows only to the consumer side. BusStats is the in-process observability snapshot exposed via stats(); both BusStats and BusError carry #[non_exhaustive] so Phase 2 may add fields/variants additively per spec sections 5.1, 5.4, and 6.2.' `
    -m 'BusError variants cover the five Phase 1 failure modes: capacity validation, clock fault, sequence exhaustion sentinel, envelope-invariant violation (forwarded from TypesError via #[from]), and channel-closed peer-drop.' `
    -m 'Co-Authored-By: Claude <noreply@anthropic.com>'
```

---

## Task 3: Traits + concrete struct skeletons + stub impls

**Files:**
- Modify: `crates/event-bus/src/lib.rs`

Goal: add `EventBus<T>` and `EventConsumer<T>` traits, the `PublishState` private struct, the two concrete struct shells (`CrossbeamBoundedBus<T>` and `CrossbeamConsumer<T>`), and stub `impl` blocks that all compile but `unimplemented!()` at runtime. **No `new()` constructor in this task** — it lands in Task 4 driven by T1.

This task makes the API surface complete enough for the next task to write a test that constructs the bus through `new()` (which doesn't exist yet — that's the red phase of Task 4).

- [ ] **Step 3.1: Append imports + traits + struct + stub impls to `lib.rs`**

After the existing types, add:

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crossbeam_channel::{Receiver, Sender};
use parking_lot::Mutex;

use rust_lmax_mev_types::{EventEnvelope, PublishMeta};

// === Traits ===

pub trait EventBus<T>: Send + Sync
where
    T: Send + 'static,
{
    fn publish(&self, payload: T, meta: PublishMeta) -> Result<PublishAck, BusError>;
    fn len(&self) -> usize;
    fn capacity(&self) -> usize;
    fn stats(&self) -> BusStats;
}

pub trait EventConsumer<T>: Send + Sync
where
    T: Send + 'static,
{
    fn recv(&self) -> Result<EventEnvelope<T>, BusError>;
    fn try_recv(&self) -> Result<Option<EventEnvelope<T>>, BusError>;
    fn len(&self) -> usize;
}

// === Internal state ===

struct PublishState {
    next_sequence: u64,
}

// === Concrete impls ===

pub struct CrossbeamBoundedBus<T> {
    sender: Sender<EventEnvelope<T>>,
    state: Mutex<PublishState>,
    published_total: AtomicU64,
    backpressure_total: AtomicU64,
    consumed_total: Arc<AtomicU64>,
    capacity: usize,
}

pub struct CrossbeamConsumer<T> {
    receiver: Receiver<EventEnvelope<T>>,
    consumed_total: Arc<AtomicU64>,
}

impl<T> EventBus<T> for CrossbeamBoundedBus<T>
where
    T: Send + 'static,
{
    fn publish(&self, _payload: T, _meta: PublishMeta) -> Result<PublishAck, BusError> {
        unimplemented!("publish lands in Task 5")
    }

    fn len(&self) -> usize {
        self.sender.len()
    }

    fn capacity(&self) -> usize {
        self.capacity
    }

    fn stats(&self) -> BusStats {
        BusStats {
            published_total: self.published_total.load(Ordering::Relaxed),
            consumed_total: self.consumed_total.load(Ordering::Relaxed),
            backpressure_total: self.backpressure_total.load(Ordering::Relaxed),
            current_depth: self.sender.len(),
            capacity: self.capacity,
        }
    }
}

impl<T> EventConsumer<T> for CrossbeamConsumer<T>
where
    T: Send + 'static,
{
    fn recv(&self) -> Result<EventEnvelope<T>, BusError> {
        unimplemented!("recv lands in Task 5")
    }

    fn try_recv(&self) -> Result<Option<EventEnvelope<T>>, BusError> {
        unimplemented!("try_recv lands in Task 5")
    }

    fn len(&self) -> usize {
        self.receiver.len()
    }
}
```

Note: `CrossbeamConsumer<T>` does **not** derive `Clone` (spec §5.3, §6.1, DoD D4). `stats()`, `len()`, and `capacity()` are **fully implemented** because they are trivial and Tasks 5+ rely on them. Only `publish`, `recv`, `try_recv` are stubs — they all land in Task 5.

- [ ] **Step 3.2: Verify the crate builds**

```powershell
cargo check -p rust-lmax-mev-event-bus
```

Expected: compiles. `PhantomData` is **not** needed because `Sender<EventEnvelope<T>>` and `Receiver<EventEnvelope<T>>` already use `T` non-phantomly.

If any compile error mentions missing `T: Send + 'static` bounds, double-check that the trait bound `where T: Send + 'static` appears on each `impl<T> EventBus<T> for CrossbeamBoundedBus<T>` / `impl<T> EventConsumer<T> for CrossbeamConsumer<T>` block.

- [ ] **Step 3.3: Commit**

```powershell
git add crates/event-bus/src/lib.rs
git commit `
    -m 'feat(event-bus): add EventBus and EventConsumer traits with stub impls' `
    -m 'Adds the two trait definitions per spec section 5.2 with Send + Sync trait bounds and a static-lifetime requirement on T. Adds the private PublishState struct and the CrossbeamBoundedBus<T> / CrossbeamConsumer<T> concrete shells per spec section 7.1. CrossbeamConsumer deliberately does not derive Clone, enforcing the single-consumer contract per spec section 5.3 and DoD D4.' `
    -m 'len(), capacity(), and stats() are fully implemented because they are trivial and Tasks 5 onward rely on them. publish(), recv(), try_recv() are unimplemented!() stubs - they all land in Task 5 driven by test T2; T1 drives only the new() constructor in Task 4.' `
    -m 'Co-Authored-By: Claude <noreply@anthropic.com>'
```

---

## Task 4: TDD T1 — `new(capacity)` rejects zero capacity

**Files:**
- Modify: `crates/event-bus/src/lib.rs`

Goal: write Test 1 (`new_rejects_zero_capacity`) per spec §10, observe it fail (no `new` exists yet → compile error), implement `CrossbeamBoundedBus::new`, observe it pass.

- [ ] **Step 4.1: Append the test module skeleton + Test 1 to `lib.rs`**

At the end of `lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rust_lmax_mev_types::{ChainContext, EventSource, SmokeTestPayload};

    fn meta() -> PublishMeta {
        PublishMeta {
            source: EventSource::Ingress,
            chain_context: ChainContext {
                chain_id: 1,
                block_number: 18_000_000,
                block_hash: [0xAB; 32],
            },
            event_version: 1,
            correlation_id: 42,
        }
    }

    fn payload(nonce: u64) -> SmokeTestPayload {
        SmokeTestPayload {
            nonce,
            data: [0xCD; 32],
        }
    }

    #[test]
    fn new_rejects_zero_capacity() {
        // Note: Result::expect_err would require the Ok variant
        // (CrossbeamBoundedBus<T>, CrossbeamConsumer<T>) to implement Debug,
        // which is not part of the Phase 1 contract. Use an explicit match
        // instead so the test does not silently demand a Debug derive on the
        // bus/consumer pair.
        let err = match CrossbeamBoundedBus::<SmokeTestPayload>::new(0) {
            Err(err) => err,
            Ok(_) => panic!("capacity 0 must reject"),
        };
        assert!(matches!(err, BusError::InvalidCapacity(0)));
    }
}
```

The `meta()` and `payload(nonce)` helpers are added now even though T1 doesn't use them; later tests share them. `PublishMeta: Clone` is already derived in Task 11 (`crates/types/src/lib.rs:73`).

- [ ] **Step 4.2: Run Test 1, expect FAIL (compile error)**

```powershell
cargo test -p rust-lmax-mev-event-bus new_rejects_zero_capacity
```

Expected: **compile error** — `CrossbeamBoundedBus::new` does not exist. This is the red phase. The error message will mention "no function or associated item named `new` found". This is an unusual TDD red phase (compile error rather than runtime failure) but is the legitimate Rust equivalent of "missing implementation."

- [ ] **Step 4.3: Implement `CrossbeamBoundedBus::new`**

Add this `impl` block above the `impl EventBus<T> for CrossbeamBoundedBus<T>` block (placement is stylistic, not load-bearing):

```rust
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
    pub fn new(capacity: usize) -> Result<(Self, CrossbeamConsumer<T>), BusError> {
        if capacity == 0 {
            return Err(BusError::InvalidCapacity(0));
        }
        let (sender, receiver) = crossbeam_channel::bounded(capacity);
        let consumed_total = Arc::new(AtomicU64::new(0));
        let bus = CrossbeamBoundedBus {
            sender,
            state: Mutex::new(PublishState { next_sequence: 0 }),
            published_total: AtomicU64::new(0),
            backpressure_total: AtomicU64::new(0),
            consumed_total: Arc::clone(&consumed_total),
            capacity,
        };
        let consumer = CrossbeamConsumer {
            receiver,
            consumed_total,
        };
        Ok((bus, consumer))
    }
}
```

- [ ] **Step 4.4: Run Test 1, expect PASS**

```powershell
cargo test -p rust-lmax-mev-event-bus new_rejects_zero_capacity
```

Expected: PASS (1 test).

- [ ] **Step 4.5: Commit**

```powershell
git add crates/event-bus/src/lib.rs
git commit `
    -m 'feat(event-bus): new() rejects capacity == 0 (TDD T1)' `
    -m 'Implements CrossbeamBoundedBus::new(capacity: usize) -> Result<(Self, CrossbeamConsumer<T>), BusError> per spec section 5.3. Rejects capacity == 0 with BusError::InvalidCapacity(0); crossbeams zero-capacity rendezvous semantics are deliberately excluded because they conflict with the Phase 1 metric definitions per spec section 5.3.' `
    -m 'Adds the test module skeleton with shared meta() and payload(n) helpers, plus T1 new_rejects_zero_capacity. The test uses an explicit match block instead of Result::expect_err so it does not silently require a Debug derive on the (Bus, Consumer) Ok variant.' `
    -m 'Co-Authored-By: Claude <noreply@anthropic.com>'
```

---

## Task 5: TDD T2 — publish + recv basic flow with envelope preservation

**Files:**
- Modify: `crates/event-bus/src/lib.rs`

Goal: write Test 2 (`publish_assigns_sequence_nonzero_timestamp_and_preserves_envelope`) per spec §10. Implement `publish()` with the simplest direct blocking `send` (no try_send fallback yet — that's Task 6), `recv()`, `try_recv()`, the `now_ns` helper, and the `depth_as_f64` cast helper. Verify the test passes.

After this task: capacity > 0 → publish/recv work end-to-end for non-full queues, sequence advances 0→1→2, all `PublishMeta` fields and the payload are preserved on the consumer side, and `consumed_total` increments correctly.

- [ ] **Step 5.1: Append T2 to the `tests` module**

Inside `mod tests`:

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
    // No timestamp monotonicity assertion - wall clock may move backward.

    // Drain and verify the third envelope matches its ack and preserves all
    // meta + payload.
    let _e0 = consumer.recv().expect("recv 0");
    let _e1 = consumer.recv().expect("recv 1");
    let e2 = consumer.recv().expect("recv 2");

    assert_eq!(e2.sequence(), ack2.sequence);
    assert_eq!(e2.timestamp_ns(), ack2.timestamp_ns);
    assert_eq!(e2.source(), m.source);
    assert_eq!(e2.event_version(), m.event_version);
    assert_eq!(e2.correlation_id(), m.correlation_id);
    assert_eq!(e2.chain_context(), &m.chain_context);
    assert_eq!(e2.payload(), &p);

    let stats = bus.stats();
    assert_eq!(stats.published_total, 3);
    assert_eq!(stats.consumed_total, 3);
    assert_eq!(stats.backpressure_total, 0);
    assert_eq!(stats.current_depth, 0);
    assert_eq!(stats.capacity, 8);
}
```

- [ ] **Step 5.2: Run Test 2, expect FAIL**

```powershell
cargo test -p rust-lmax-mev-event-bus publish_assigns_sequence_nonzero_timestamp_and_preserves_envelope
```

Expected: panic with `"publish lands in Task 5"` from the `unimplemented!()` stub. This is the red phase.

- [ ] **Step 5.3: Add `now_ns` and `depth_as_f64` helpers (at module scope)**

Insert above the `impl<T> CrossbeamBoundedBus<T>` block (or anywhere at module scope, but grouping these two helpers together near the bus impl reads better):

```rust
use std::time::{SystemTime, UNIX_EPOCH};

fn now_ns() -> Result<u64, BusError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| u64::try_from(d.as_nanos()).ok())
        .filter(|&ns| ns != 0)
        .ok_or(BusError::ClockUnavailable)
}

/// Centralized `usize → f64` cast for the `current_depth` gauge.
///
/// Phase 1 capacities are well below 2^53, so precision loss does not apply
/// (per spec §8.1). Centralizing the cast keeps the `#[allow(...)]` rationale
/// in one place rather than scattered across publish/recv call sites.
#[allow(clippy::cast_precision_loss)]
fn depth_as_f64(depth: usize) -> f64 {
    depth as f64
}
```

The three rejected cases for `now_ns` (pre-epoch, `u128 → u64` overflow, exact-zero) all map to `ClockUnavailable` per spec §7.3.

- [ ] **Step 5.4: Replace the `publish()` stub with a direct blocking-send implementation**

In the `impl<T> EventBus<T> for CrossbeamBoundedBus<T>` block, replace the stub `publish` body with:

```rust
fn publish(&self, payload: T, meta: PublishMeta) -> Result<PublishAck, BusError> {
    let mut state = self.state.lock();

    let timestamp_ns = now_ns()?;

    let sequence = state.next_sequence;
    if sequence == u64::MAX {
        return Err(BusError::SequenceExhausted);
    }

    let envelope = EventEnvelope::seal(meta, payload, sequence, timestamp_ns)?;

    self.sender
        .send(envelope)
        .map_err(|_| BusError::Closed)?;

    state.next_sequence = sequence + 1;
    self.published_total.fetch_add(1, Ordering::Relaxed);
    metrics::counter!("event_bus_published_total").increment(1);
    metrics::gauge!("event_bus_current_depth").set(depth_as_f64(self.sender.len()));

    Ok(PublishAck {
        sequence,
        timestamp_ns,
    })
}
```

The `try_send` + `Full` fallback is **not** added in this task — Task 6 introduces it as a refactor driven by T3. For now, `send` is direct and blocking.

- [ ] **Step 5.5: Replace the `recv` and `try_recv` stubs**

In the `impl<T> EventConsumer<T> for CrossbeamConsumer<T>` block, replace the two stub bodies with:

```rust
fn recv(&self) -> Result<EventEnvelope<T>, BusError> {
    match self.receiver.recv() {
        Ok(env) => {
            self.consumed_total.fetch_add(1, Ordering::Relaxed);
            metrics::counter!("event_bus_consumed_total").increment(1);
            metrics::gauge!("event_bus_current_depth").set(depth_as_f64(self.receiver.len()));
            Ok(env)
        }
        Err(_) => Err(BusError::Closed),
    }
}

fn try_recv(&self) -> Result<Option<EventEnvelope<T>>, BusError> {
    use crossbeam_channel::TryRecvError;
    match self.receiver.try_recv() {
        Ok(env) => {
            self.consumed_total.fetch_add(1, Ordering::Relaxed);
            metrics::counter!("event_bus_consumed_total").increment(1);
            metrics::gauge!("event_bus_current_depth").set(depth_as_f64(self.receiver.len()));
            Ok(Some(env))
        }
        Err(TryRecvError::Empty) => Ok(None),
        Err(TryRecvError::Disconnected) => Err(BusError::Closed),
    }
}
```

- [ ] **Step 5.6: Run all tests, expect PASS**

```powershell
cargo test -p rust-lmax-mev-event-bus
```

Expected: 2 tests pass (`new_rejects_zero_capacity` + `publish_assigns_sequence_nonzero_timestamp_and_preserves_envelope`).

If T2 fails on the envelope getter assertions (`e2.sequence()`, `e2.timestamp_ns()`, etc.), recheck that `EventEnvelope` getter names in `crates/types/src/lib.rs:222-271` match the test (lines `222: sequence`, `226: timestamp_ns`, `230: source`, `234: event_version`, `238: correlation_id`, `242: chain_context`, `246: payload`).

If T2 fails on `consumed_total == 3` but `published_total == 3`, the most likely cause is the `Arc<AtomicU64>` not being properly shared — recheck that `consumer.consumed_total` and `bus.consumed_total` reference the same `Arc` instance (Step 4.3 uses `Arc::clone`).

- [ ] **Step 5.7: Commit**

```powershell
git add crates/event-bus/src/lib.rs
git commit `
    -m 'feat(event-bus): publish + recv basic flow with envelope preservation (TDD T2)' `
    -m 'Implements the publish path per spec section 7.3 with a direct blocking send (no try_send/Full fallback yet - that lands in Task 6 driven by T3). Adds the now_ns() helper that rejects pre-epoch, u128->u64 overflow, and exact-zero timestamps to BusError::ClockUnavailable. Adds the depth_as_f64() helper that centralizes the usize -> f64 cast for the current_depth gauge with a single #[allow(clippy::cast_precision_loss)] keyed to the spec section 8.1 rationale.' `
    -m 'Implements recv() and try_recv() per spec section 7.4. Both increment consumed_total via the Arc<AtomicU64> shared with the bus and emit the metrics-facade counter and current_depth gauge. try_recv()s Ok(None) branch on empty queue performs no side effects.' `
    -m 'Adds T2 publish_assigns_sequence_nonzero_timestamp_and_preserves_envelope which verifies sequence starts at 0 and advances monotonically, timestamps are non-zero (no monotonicity assertion - wall clock may move backward), all PublishMeta fields plus the payload are preserved on the consumer-received envelope, and stats counters track published/consumed correctly.' `
    -m 'Co-Authored-By: Claude <noreply@anthropic.com>'
```

---

## Task 6: TDD T3 — try_send + `Full` → backpressure_total fallback

**Files:**
- Modify: `crates/event-bus/src/lib.rs`

Goal: write Test 3 (`publish_registers_backpressure_when_full_and_completes_after_recv`) per spec §10, observe it fail (current direct `send` doesn't increment `backpressure_total`), refactor `publish()` to the spec §7.3 try_send + Full fallback shape, observe it pass.

The T3 test sketch includes a **deadline cleanup branch**: if the deadline expires (i.e., the test is failing), the test drains a slot and joins the spawned publisher thread before panicking. This avoids leaving a parked publisher thread when the assertion fails — most relevant in the TDD red phase where the publisher is genuinely blocked inside `send()` because the current implementation never increments `backpressure_total`.

- [ ] **Step 6.1: Append T3 to the `tests` module**

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
    //
    // Cleanup-on-failure: if the deadline expires (e.g., during the TDD red
    // phase, where the implementation does not yet increment
    // backpressure_total and the publisher is genuinely blocked inside
    // send()), drain a slot and join the spawned thread before panicking.
    // This ensures the test does not leak a parked thread when it fails.
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if bus.stats().backpressure_total > 0 {
            break;
        }
        if Instant::now() >= deadline {
            // Drain one slot so the publisher's blocking send completes,
            // then join.
            let _ = consumer.recv();
            let _ = handle.join();
            panic!("publisher thread never registered backpressure within 3s");
        }
        std::thread::yield_now();
    }
    assert_eq!(bus.stats().backpressure_total, 1);

    let env0 = consumer.recv().expect("drain 0");
    assert_eq!(env0.sequence(), 0);

    let ack2 = handle
        .join()
        .expect("publisher thread did not panic")
        .expect("publish 2 succeeded after drain");
    assert_eq!(ack2.sequence, 2);

    let env1 = consumer.recv().expect("drain 1");
    let env2 = consumer.recv().expect("drain 2");
    assert_eq!(env1.sequence(), 1);
    assert_eq!(env2.sequence(), 2);

    let stats = bus.stats();
    assert_eq!(stats.published_total, 3);
    assert_eq!(stats.consumed_total, 3);
    assert_eq!(stats.backpressure_total, 1);
    assert_eq!(stats.current_depth, 0);
    assert_eq!(stats.capacity, 2);
}
```

- [ ] **Step 6.2: Run T3, expect FAIL (with clean cleanup)**

```powershell
cargo test -p rust-lmax-mev-event-bus publish_registers_backpressure_when_full_and_completes_after_recv
```

Expected: the deadline-cleanup branch fires, the publisher thread is unblocked via `consumer.recv()`, joined, and the test panics with `"publisher thread never registered backpressure within 3s"`. **Critically, no thread is left parked.** This is the red phase.

- [ ] **Step 6.3: Refactor `publish()` to the spec §7.3 try_send + Full fallback shape**

Replace the `self.sender.send(envelope).map_err(|_| BusError::Closed)?;` line in `publish()` with the spec §7.3 step 6 match:

```rust
match self.sender.try_send(envelope) {
    Ok(()) => { /* fast path */ }
    Err(crossbeam_channel::TrySendError::Full(env)) => {
        self.backpressure_total.fetch_add(1, Ordering::Relaxed);
        metrics::counter!("event_bus_backpressure_total").increment(1);
        self.sender.send(env).map_err(|_| BusError::Closed)?;
    }
    Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
        return Err(BusError::Closed);
    }
}
```

The full `publish()` body now matches spec §7.3 exactly:

```rust
fn publish(&self, payload: T, meta: PublishMeta) -> Result<PublishAck, BusError> {
    let mut state = self.state.lock();

    let timestamp_ns = now_ns()?;

    let sequence = state.next_sequence;
    if sequence == u64::MAX {
        return Err(BusError::SequenceExhausted);
    }

    let envelope = EventEnvelope::seal(meta, payload, sequence, timestamp_ns)?;

    match self.sender.try_send(envelope) {
        Ok(()) => { /* fast path */ }
        Err(crossbeam_channel::TrySendError::Full(env)) => {
            self.backpressure_total.fetch_add(1, Ordering::Relaxed);
            metrics::counter!("event_bus_backpressure_total").increment(1);
            self.sender.send(env).map_err(|_| BusError::Closed)?;
        }
        Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
            return Err(BusError::Closed);
        }
    }

    state.next_sequence = sequence + 1;
    self.published_total.fetch_add(1, Ordering::Relaxed);
    metrics::counter!("event_bus_published_total").increment(1);
    metrics::gauge!("event_bus_current_depth").set(depth_as_f64(self.sender.len()));

    Ok(PublishAck {
        sequence,
        timestamp_ns,
    })
}
```

- [ ] **Step 6.4: Run all tests, expect PASS**

```powershell
cargo test -p rust-lmax-mev-event-bus
```

Expected: 3 tests pass.

If T3 still hangs longer than 3s on the deadline (and the cleanup branch panics), the most likely cause is `stats()` being implemented to acquire the publish-state mutex (deadlock). Re-verify `stats()` from Task 3 reads only the atomics + `sender.len()` and does **not** call `self.state.lock()` (spec §7.2 + DoD D8).

If T2 regresses, the most likely cause is a typo in the refactored `publish()` — recheck that the success-path counter advance and the `Ok(PublishAck)` return are unchanged from Task 5.

- [ ] **Step 6.5: Commit**

```powershell
git add crates/event-bus/src/lib.rs
git commit `
    -m 'feat(event-bus): backpressure_total fallback via try_send + Full (TDD T3)' `
    -m 'Refactors publish() to the spec section 7.3 try_send + Full fallback shape: fast path is a single try_send call; Full triggers a backpressure_total increment (atomic + metrics counter) before falling back to a blocking send that retains the same lock. Disconnected on either try_send or the blocking send maps to BusError::Closed without advancing the sequence counter. backpressure_total is a full-queue encounter count per spec section 8.1, not a measurement of actual block duration.' `
    -m 'Adds T3 publish_registers_backpressure_when_full_and_completes_after_recv. Uses an Instant-based 3s deadline as a hang guard, not a sleep - the loop yields with std::thread::yield_now and exits as soon as backpressure_total observes a non-zero value, which is set inside the publish lock just before the blocking send. The test relies on stats() not acquiring the publish-state mutex per spec section 7.2 and DoD D8; if stats() ever started taking the lock, this test would deadlock. The deadline-failure branch drains the queue and joins the publisher thread before panicking so a failing test never leaks a parked thread (this matters most during the TDD red phase).' `
    -m 'Co-Authored-By: Claude <noreply@anthropic.com>'
```

---

## Task 7: T4 — `publish` after consumer drop returns `Closed`

**Files:**
- Modify: `crates/event-bus/src/lib.rs`

Goal: add T4 (`publish_after_consumer_drop_returns_closed`) per spec §10. The current `publish()` from Task 6 already maps `TrySendError::Disconnected` to `BusError::Closed`, so this test is expected to pass on first run (test-first verification, not red→green TDD).

- [ ] **Step 7.1: Append T4 to the `tests` module**

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

`expect_err` is OK here because `publish()` returns `Result<PublishAck, BusError>`, and `PublishAck` does derive `Debug` (Task 2, Step 2.1). The reasoning that motivated the T1 explicit-match form does not apply.

- [ ] **Step 7.2: Run T4, expect PASS on first try**

```powershell
cargo test -p rust-lmax-mev-event-bus publish_after_consumer_drop_returns_closed
```

Expected: PASS.

If FAIL with a different `BusError` variant, the most likely cause is the `try_send` `Disconnected` arm not being added in Task 6 — recheck Step 6.3.

- [ ] **Step 7.3: Mutation check (optional but recommended)**

Briefly mutate the `Disconnected` arm of `publish()` to `return Err(BusError::ClockUnavailable);` (a clearly wrong variant), run T4, observe failure with `matches!` mismatch, then revert the mutation. This confirms T4 is a real test, not a passive observer.

- [ ] **Step 7.4: Commit**

```powershell
git add crates/event-bus/src/lib.rs
git commit `
    -m 'test(event-bus): publish after consumer drop returns Closed (T4)' `
    -m 'Adds T4 publish_after_consumer_drop_returns_closed which verifies the BusError::Closed mapping for both the try_send and blocking send Disconnected paths in the publish flow, plus the invariant that a failed publish does not advance the sequence counter (inferred from stats.published_total == 0).' `
    -m 'No implementation change needed - the closed-channel mapping was already wired in Task 6 step 6.3 via the TrySendError::Disconnected arm and the blocking send map_err.' `
    -m 'Co-Authored-By: Claude <noreply@anthropic.com>'
```

---

## Task 8: T5 — `try_recv` empty + `recv` after bus drop

**Files:**
- Modify: `crates/event-bus/src/lib.rs`

Goal: add T5 (`try_recv_empty_returns_none_and_recv_after_bus_drop_returns_closed`) per spec §10. The `try_recv` Empty → `Ok(None)` and `recv` Disconnected → `Err(Closed)` mappings already exist from Task 5, so this is also test-first verification.

- [ ] **Step 8.1: Append T5 to the `tests` module**

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

- [ ] **Step 8.2: Run T5, expect PASS on first try**

```powershell
cargo test -p rust-lmax-mev-event-bus try_recv_empty_returns_none_and_recv_after_bus_drop_returns_closed
```

Expected: PASS.

If FAIL on the `consumed_total == 0` assertion after `try_recv == Ok(None)`, the most likely cause is `try_recv` incrementing the counter on the empty branch — recheck Step 5.5 and confirm the increment is **only** in the `Ok(env)` arm.

- [ ] **Step 8.3: Mutation check (optional)**

Mutate `try_recv`'s empty branch to also `consumed_total.fetch_add(1, Ordering::Relaxed);` before returning `Ok(None)`, run T5, observe failure on `consumed_total == 0`, revert. Confirms the test catches the regression.

- [ ] **Step 8.4: Commit**

```powershell
git add crates/event-bus/src/lib.rs
git commit `
    -m 'test(event-bus): try_recv None and recv after bus drop closed mapping (T5)' `
    -m 'Adds T5 try_recv_empty_returns_none_and_recv_after_bus_drop_returns_closed. Verifies (a) the Ok(None) semantics on an empty queue and the no-side-effect rule (consumed_total stays at 0), (b) consumed_total advancement on the Ok(Some(_)) branch, and (c) the BusError::Closed mapping when the bus is dropped while the consumer holds the receiver.' `
    -m 'No implementation change needed - both mappings were wired in Task 5 step 5.5.' `
    -m 'Co-Authored-By: Claude <noreply@anthropic.com>'
```

---

## Task 9: T6 — invalid meta retry safety (sequence not consumed on Envelope error)

**Files:**
- Modify: `crates/event-bus/src/lib.rs`

Goal: add T6 (`publish_rejects_invalid_meta_without_consuming_sequence`) per spec §10. The "success-only sequence advance" invariant from Task 5 already guarantees this — the test verifies it via the public API.

- [ ] **Step 9.1: Append T6 to the `tests` module**

```rust
#[test]
fn publish_rejects_invalid_meta_without_consuming_sequence() {
    let (bus, consumer) = CrossbeamBoundedBus::<SmokeTestPayload>::new(2)
        .expect("capacity 2 valid");

    // Invalid meta: chain_id = 0 violates Phase 1 envelope invariant.
    let mut bad_meta = meta();
    bad_meta.chain_context.chain_id = 0;

    let err = bus
        .publish(payload(0), bad_meta)
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

- [ ] **Step 9.2: Run T6, expect PASS on first try**

```powershell
cargo test -p rust-lmax-mev-event-bus publish_rejects_invalid_meta_without_consuming_sequence
```

Expected: PASS.

If FAIL on `ack.sequence == 0` after the retry (i.e., the retry's ack.sequence is 1), the most likely cause is `next_sequence` being advanced on the Envelope error path — recheck that the `state.next_sequence = sequence + 1;` line in `publish()` (Step 6.3) sits **after** the match block, so it only runs on success.

- [ ] **Step 9.3: Mutation check (optional)**

Move `state.next_sequence = sequence + 1;` to immediately before the match block (so it runs even when `seal()` fails). Run T6, observe failure on the retry assertion. Revert.

- [ ] **Step 9.4: Commit**

```powershell
git add crates/event-bus/src/lib.rs
git commit `
    -m 'test(event-bus): invalid meta retry safety - sequence not consumed (T6)' `
    -m 'Adds T6 publish_rejects_invalid_meta_without_consuming_sequence. Verifies the core retry-safety invariant from spec section 7.3 and the section 9 error matrix: a publish that fails with BusError::Envelope (caller-supplied meta violates a Phase 1 envelope invariant via TypesError) does not advance state.next_sequence, so the next valid publish reuses the would-have-been sequence 0 and lands the envelope on the consumer.' `
    -m 'No implementation change needed - the success-only advance was wired in Task 5 step 5.4 and Task 6 step 6.3 by placing state.next_sequence = sequence + 1 strictly after the match block.' `
    -m 'Co-Authored-By: Claude <noreply@anthropic.com>'
```

---

## Task 10: T7 — sequence exhaustion no-wrap (white-box)

**Files:**
- Modify: `crates/event-bus/src/lib.rs`

Goal: add T7 (`sequence_exhausted_does_not_wrap`) per spec §10. White-box — relies on same-module visibility of the private `state` field on `CrossbeamBoundedBus`. The implementation already includes the `if sequence == u64::MAX → SequenceExhausted` check from Task 5 step 5.4, so this is also test-first verification.

- [ ] **Step 10.1: Append T7 to the `tests` module**

```rust
#[test]
fn sequence_exhausted_does_not_wrap() {
    let (bus, consumer) = CrossbeamBoundedBus::<SmokeTestPayload>::new(2)
        .expect("capacity 2 valid");

    // Force the boundary: next publish would attempt sequence == u64::MAX.
    bus.state.lock().next_sequence = u64::MAX;

    let err = bus
        .publish(payload(0), meta())
        .expect_err("must be SequenceExhausted at u64::MAX");
    assert!(matches!(err, BusError::SequenceExhausted));

    // No advance, no envelope sent, and crucially no progress past step 4 of
    // section 7.3: backpressure_total and current_depth must both be 0 to
    // confirm the publish path returned before reaching seal/try_send.
    assert_eq!(bus.state.lock().next_sequence, u64::MAX);
    assert!(consumer.try_recv().expect("try_recv ok").is_none());
    let stats = bus.stats();
    assert_eq!(stats.published_total, 0);
    assert_eq!(stats.backpressure_total, 0);
    assert_eq!(stats.current_depth, 0);
}
```

The `bus.state.lock()` access works because tests live inside `#[cfg(test)] mod tests` in `lib.rs`, which is a child module of the parent module containing the private `state` field. Same-module visibility lets the test mutate the private field directly. T7 is the only test in the suite that does this; all others use the public API.

- [ ] **Step 10.2: Run T7, expect PASS on first try**

```powershell
cargo test -p rust-lmax-mev-event-bus sequence_exhausted_does_not_wrap
```

Expected: PASS.

If FAIL on `consumer.try_recv == Ok(None)` (i.e., the consumer received an envelope), the most likely cause is the `if sequence == u64::MAX` check being placed **after** `seal()` rather than before. Recheck Step 5.4 / Step 6.3 — the check must be at step 4 of spec §7.3, before `seal` (step 5) and `try_send` (step 6).

- [ ] **Step 10.3: Mutation check (optional)**

Replace the `if sequence == u64::MAX { return Err(BusError::SequenceExhausted); }` line with a no-op (`// `) and observe T7 fails — the publish would now use `sequence = u64::MAX` (and via the success-path advance, wrap to 0 next time, which is the silent-wrap behavior the spec forbids). Revert.

- [ ] **Step 10.4: Run all tests**

```powershell
cargo test -p rust-lmax-mev-event-bus
```

Expected: 7 tests pass.

- [ ] **Step 10.5: Commit**

```powershell
git add crates/event-bus/src/lib.rs
git commit `
    -m 'test(event-bus): sequence exhaustion no-wrap white-box test (T7)' `
    -m 'Adds T7 sequence_exhausted_does_not_wrap. Forces the boundary by setting bus.state.lock().next_sequence = u64::MAX directly, then confirms publish() returns BusError::SequenceExhausted (the u64::MAX exhaustion sentinel value is reserved per spec section 5.4 and is never published). Verifies the publish path returned at step 4 of spec section 7.3 by asserting next_sequence is unchanged, no envelope reached the channel, and stats published_total / backpressure_total / current_depth are all 0.' `
    -m 'White-box test - relies on same-module visibility of the private state field. This is the only test in the suite that crosses encapsulation; all others use the public API.' `
    -m 'Co-Authored-By: Claude <noreply@anthropic.com>'
```

---

## Task 11: Crate-level docstring + per-item docstrings

**Files:**
- Modify: `crates/event-bus/src/lib.rs`

Goal: add the spec §11 D11 docstring set: crate-level docstring plus docstrings on `EventBus`, `EventConsumer`, `CrossbeamBoundedBus::new`, `publish`, `recv`, `try_recv`, `stats`, `PublishAck`, `BusStats`, `BusError`. The crate docstring covers timestamp/ordering semantics, sequence ownership, the `stats()` non-linearizability + no-mutex-acquisition contract, the retry-safety policy (Closed = terminal, ClockUnavailable/Envelope = retryable), and the `#[non_exhaustive]` extension policy.

- [ ] **Step 11.1: Replace the `lib.rs` opening comment with the crate-level docstring**

Replace the placeholder line at the top of `lib.rs` with:

```rust
//! # rust-lmax-mev-event-bus
//!
//! Phase 1 single-consumer bounded event bus for the LMAX-style MEV engine.
//!
//! ## Topology
//!
//! Each domain pipeline stage boundary owns one `(CrossbeamBoundedBus<T>,
//! CrossbeamConsumer<T>)` pair. There is **no global engine-wide queue** —
//! the number of bus instances is decided by the wiring crate (Task 16,
//! `crates/app`) and each instance carries one ordered stream.
//!
//! Multi-consumer cursors, broadcast semantics, and `subscribe(name)`-style
//! APIs are explicitly forbidden in Phase 1 and are deferred to Phase 2 per
//! ADR-005 §"Phase 1 baseline" + `PHASE_1_DETAIL_REVISION` §3.1.
//!
//! ## Sequence and timestamp ownership
//!
//! `EventEnvelope::sequence` and `EventEnvelope::timestamp_ns` are
//! **bus-assigned**. Callers pass `PublishMeta` and a payload to
//! [`EventBus::publish`]; the bus internally calls
//! [`rust_lmax_mev_types::EventEnvelope::seal`] with values it owns and
//! returns a [`PublishAck`] containing the assigned sequence and timestamp.
//! Envelope itself flows only to the consumer side — the publish path is
//! clone-free.
//!
//! `timestamp_ns` is captured at publish attempt time before a potentially
//! blocking send. It may therefore be earlier than the actual enqueue/receive
//! time. Ordering is defined by `sequence`, not timestamp monotonicity.
//!
//! ## Publish-path serialization
//!
//! The publish path is serialized end-to-end by a `parking_lot::Mutex`. The
//! lock is held across the optional blocking `send`, which is intentional:
//! it ensures sequence assignment order matches channel arrival order, and
//! it propagates backpressure to all publishers via the mutex. The mutex
//! does **not** guarantee strict FIFO acquisition fairness.
//!
//! [`EventBus::stats`] **must not** acquire the publish-state mutex. It
//! reads only atomics and the channel `len`/`capacity`. This is a hard
//! correctness requirement: tests (and operational diagnostics) must be
//! able to call `stats()` while another thread holds the lock inside a
//! blocking `send`.
//!
//! ## Sequence-exhaustion policy
//!
//! `u64::MAX` is reserved as the exhaustion sentinel and is **never
//! published**. Phase 1 publishable sequence range is `0..u64::MAX`
//! (half-open); the maximum published value is `u64::MAX - 1`. When
//! `next_sequence` reaches `u64::MAX`, [`EventBus::publish`] returns
//! [`BusError::SequenceExhausted`] terminally — the bus instance must be
//! discarded. Silent wrap is forbidden.
//!
//! ## Retry safety
//!
//! `state.next_sequence` advances **only on publish success**. All failed
//! publish paths leave the counter unchanged, so the next publish attempt
//! reuses the would-have-been sequence. Among the failure modes:
//!
//! - [`BusError::ClockUnavailable`] and [`BusError::Envelope`] are
//!   **retryable** — the underlying problem (system clock or caller-supplied
//!   `PublishMeta`) can be corrected and the same publish re-attempted.
//! - [`BusError::Closed`] is **terminal** — the consumer has dropped and no
//!   further publish on this bus can succeed; the caller should discard the
//!   bus instance. The "does not consume a sequence" rule still applies, so
//!   a Closed-failed publish leaves `next_sequence` unchanged for the
//!   bookkeeping symmetry.
//! - [`BusError::SequenceExhausted`] is also **terminal** — the bus has
//!   reached `u64::MAX` and must be discarded.
//!
//! This is the foundation of the retry-safety invariant verified by T6.
//!
//! ## Phase 2 extension policy
//!
//! [`BusStats`] and [`BusError`] both carry `#[non_exhaustive]` so future
//! Phase 2 work (e.g. backpressure timeout variants, additional metric
//! fields) can land additively. New impls of [`EventBus`] / [`EventConsumer`]
//! (e.g. lock-free ring buffer, cursor-aware multi-consumer) are also
//! additive — gated on benchmark proof per ADR-005.
```

- [ ] **Step 11.2: Add docstrings on `EventBus` and `EventConsumer` traits**

Above each trait definition:

```rust
/// Producer-side handle to a single ordered event stream.
///
/// Domain events block on a full queue; telemetry-channel saturation drops
/// are the responsibility of a separate channel (Task 15) and are not
/// modeled here. See ADR-005 for the policy.
pub trait EventBus<T>: Send + Sync
where
    T: Send + 'static,
{
    /// Publishes `payload` with the caller-supplied `meta`. Returns the
    /// bus-assigned identity. The envelope itself flows only to the
    /// consumer; the publish path is clone-free.
    ///
    /// On success, `state.next_sequence` is advanced by 1. On any error,
    /// the counter is unchanged — see the crate-level "Retry safety" section.
    fn publish(&self, payload: T, meta: PublishMeta) -> Result<PublishAck, BusError>;

    /// Returns the current channel depth (`sender.len()`).
    fn len(&self) -> usize;

    /// Returns the configured channel capacity (set at `new()` time).
    fn capacity(&self) -> usize;

    /// Returns an in-process snapshot of the four metrics + structural
    /// quantities. **Must not acquire the publish-state mutex** — reads
    /// only the atomic counters and the channel `len`/`capacity`.
    fn stats(&self) -> BusStats;
}
```

```rust
/// Consumer-side handle to a single ordered event stream.
///
/// `CrossbeamConsumer<T>` (the only Phase 1 impl) does not implement
/// `Clone`. The single-consumer contract is enforced by construction:
/// `CrossbeamBoundedBus::new` returns exactly one consumer per bus.
///
/// Sharing a consumer across multiple worker threads (which would
/// distribute order-dependent processing) is **outside** the Phase 1
/// contract. Producer-side sharing across threads is supported via
/// `Arc<CrossbeamBoundedBus<T>>`.
pub trait EventConsumer<T>: Send + Sync
where
    T: Send + 'static,
{
    /// Blocks until an envelope is available or the bus is dropped.
    fn recv(&self) -> Result<EventEnvelope<T>, BusError>;

    /// Returns immediately. `Ok(None)` indicates an empty queue and is
    /// **not an error**; `consumed_total` is not advanced on this branch.
    fn try_recv(&self) -> Result<Option<EventEnvelope<T>>, BusError>;

    /// Returns the current channel depth (`receiver.len()`).
    fn len(&self) -> usize;
}
```

- [ ] **Step 11.3: Add docstrings on `PublishAck`, `BusStats`, `BusError`**

Replace the existing minimal `BusStats` doc with the spec §5.1 wording:

```rust
/// Returned from `publish` on success.
///
/// Carries only the bus-assigned identity of the published event; the
/// envelope itself flows exclusively to the consumer side. Keeping the
/// publish path clone-free is the reason `publish` does not return
/// `EventEnvelope<T>`.
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
/// Marked `#[non_exhaustive]` so downstream callers cannot rely on
/// exhaustive construction or matching; Phase 2 may add fields while
/// callers continue to read the values through `stats()` and pattern-match
/// with `..` rest patterns. (Note: `#[non_exhaustive]` on a public struct
/// forbids struct-literal construction outside this crate, which is the
/// desired contract here — `BusStats` is produced only by
/// [`EventBus::stats`].)
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusStats { /* fields */ }
```

`BusError` keeps the existing `#[error("...")]` attributes — they double as docstrings for each variant. Add a top-level doc:

```rust
/// Errors returned by [`EventBus`] / [`EventConsumer`] operations.
///
/// Marked `#[non_exhaustive]` so Phase 2 may add variants (e.g.
/// `BackpressureTimeout`, `MetricsBackendUnavailable`) without breaking
/// downstream `match` consumers — they will route the new variants
/// through the `_ =>` arm.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum BusError { /* variants */ }
```

- [ ] **Step 11.4: Verify `CrossbeamBoundedBus::new` docstring is in place**

The docstring from Step 4.3 already contains the substantive content (capacity == 0 rejection rationale). Re-verify it survives any reformatting from this task — no further change required if it does.

- [ ] **Step 11.5: Verify docs render**

```powershell
cargo doc -p rust-lmax-mev-event-bus --no-deps
```

Expected: builds with no errors. Warnings about broken intra-doc links are acceptable to fix here (e.g. `[`PublishAck`]` resolving correctly), but a full doc pass should already work because every link target lives in this crate.

- [ ] **Step 11.6: Commit**

```powershell
git add crates/event-bus/src/lib.rs
git commit `
    -m 'docs(event-bus): add crate-level and per-item docstrings (DoD D11)' `
    -m 'Crate-level docstring covers topology (each pipeline stage boundary owns one bus pair, no global queue), sequence/timestamp ownership and the clone-free publish-path contract, the parking_lot mutex serialization role, the stats() no-mutex-acquisition hard requirement, the u64::MAX exhaustion sentinel and retry-safety policies (ClockUnavailable and Envelope retryable, Closed and SequenceExhausted terminal), and the #[non_exhaustive] Phase 2 extension policy per spec section 11 DoD D11.' `
    -m 'EventBus and EventConsumer traits get individual docstrings spelling out the publish contract, the stats() lock-free guarantee, the no-Clone single-consumer enforcement, and the try_recv Ok(None) no-side-effect rule. PublishAck, BusStats, and BusError get docstrings matching spec sections 5.1 and 5.4, including the corrected #[non_exhaustive] semantics for BusStats.' `
    -m 'Co-Authored-By: Claude <noreply@anthropic.com>'
```

---

## Task 12: Final QA — fmt, build, test, clippy + DoD walk

**Files:**
- Possibly modify: `crates/event-bus/src/lib.rs` (only if fmt/clippy require)

Goal: verify all spec §11 Definition of Done items D1–D11 are satisfied. Per spec D10, run gates in order **fmt → build → test → clippy** so failures surface from cheapest to most expensive.

- [ ] **Step 12.1: `cargo fmt`**

```powershell
cargo fmt --check
```

If FAIL:

```powershell
cargo fmt
```

then re-run `cargo fmt --check` to confirm clean.

- [ ] **Step 12.2: `cargo build`**

```powershell
cargo build -p rust-lmax-mev-event-bus
```

Expected: no warnings, exits 0.

- [ ] **Step 12.3: `cargo test`**

```powershell
cargo test -p rust-lmax-mev-event-bus
```

Expected: 7 tests pass:
- `new_rejects_zero_capacity` (T1)
- `publish_assigns_sequence_nonzero_timestamp_and_preserves_envelope` (T2)
- `publish_registers_backpressure_when_full_and_completes_after_recv` (T3)
- `publish_after_consumer_drop_returns_closed` (T4)
- `try_recv_empty_returns_none_and_recv_after_bus_drop_returns_closed` (T5)
- `publish_rejects_invalid_meta_without_consuming_sequence` (T6)
- `sequence_exhausted_does_not_wrap` (T7)

- [ ] **Step 12.4: `cargo clippy` with `-D warnings`**

```powershell
cargo clippy -p rust-lmax-mev-event-bus -- -D warnings
```

Expected: no warnings, exits 0. The `clippy::cast_precision_loss` lint is already addressed by the `depth_as_f64` helper introduced in Task 5 step 5.3, so it should not fire on the `as f64` cast inside that helper (the `#[allow]` is local) and there are no other `as f64` cast sites.

Additionally, `cargo clippy -p rust-lmax-mev-event-bus --all-targets -- -D warnings` must pass as a Task 18 CI preview.

If clippy fires any other lint:
- Stylistic ones (e.g. `redundant_field_names`, `needless_return`) — fix by editing.
- Semantic ones — pause and re-read the lint message; do not silence with `#[allow]` without spec-level justification. **Do not** suppress lints workspace-wide here — that decision belongs to Task 18 (CI pipeline).

- [ ] **Step 12.5: Walk the spec §11 DoD checklist**

Open `docs/superpowers/specs/2026-04-29-task-12-event-bus-design.md` and verify each item:

- [ ] D1 — Workspace `Cargo.toml` `members` re-adds `crates/event-bus`; `[workspace.dependencies]` adds `parking_lot = "0.12"`.
- [ ] D2 — `crates/event-bus/Cargo.toml` matches spec §4.2 verbatim.
- [ ] D3 — `EventBus<T>` and `EventConsumer<T>` traits defined with `Send + Sync` and `where T: Send + 'static`, signatures matching spec §5.2.
- [ ] D4 — `CrossbeamBoundedBus<T>` and `CrossbeamConsumer<T>` structs implemented; `CrossbeamConsumer<T>` does not derive or implement `Clone`.
- [ ] D5 — `PublishAck`, `BusStats` (`#[non_exhaustive]`), and `BusError` (`#[non_exhaustive]`, 5 variants) defined per spec §5.1 / §5.4.
- [ ] D6 — Publish path enforces invariants: lock held end-to-end, `state.next_sequence` advanced only on success, `try_send → Full` increments `backpressure_total`, `SequenceExhausted` fires before `seal()` when `next_sequence == u64::MAX`.
- [ ] D7 — `recv` / `try_recv` advance `consumed_total` (atomic + metric) only on `Some`; `Ok(None)` from `try_recv` performs no side effects.
- [ ] D8 — All four metrics emit through `metrics` facade. Three counters keep `AtomicU64`. `current_depth` does **not** keep a separate atomic — read live from channel `len()`. `stats()` does not acquire the publish-state mutex.
- [ ] D9 — Seven inline tests (T1–T7) all PASS.
- [ ] D10 — fmt → build → test → clippy all clean.
- [ ] D11 — Crate-level docstring + per-item docstrings present.

- [ ] **Step 12.6: (Optional) Cleanup commit**

If Steps 12.1–12.4 produced any auto-formatting fixes or clippy fixes:

```powershell
git add crates/event-bus/src/lib.rs
git commit `
    -m 'chore(event-bus): final fmt/clippy cleanup' `
    -m 'Co-Authored-By: Claude <noreply@anthropic.com>'
```

If everything was already clean, no commit is needed.

- [ ] **Step 12.7: Verify Task 12 readiness for Task 13 handoff**

Confirm the API surface that Task 13 (`crates/journal`) and later Task 16 (`crates/app`) will import:

- `EventBus` (trait) — for the producer-side wiring in Task 16.
- `EventConsumer` (trait) — for the consumer-side wiring in Task 16; Task 13 (journal) likely uses this too if journal stages reads downstream of a bus.
- `CrossbeamBoundedBus` (concrete) — for the bootstrap path in Task 16 that calls `new(capacity)`.
- `CrossbeamConsumer` (concrete) — paired with the bus.
- `PublishAck`, `BusStats`, `BusError` — public support types.

Re-read `docs/superpowers/specs/2026-04-29-task-12-event-bus-design.md §3 Out of Scope` to confirm nothing crept in from a deferred task.

No code change here — purely a sanity readback.

---

## Out of scope for this plan

These are explicitly **not** Task 12 work and remain deferred per spec §3:

- 100k events smoke test → **Task 17** (integration smoke tests).
- TOML capacity loading / config wrapper → **Task 14** (`crates/config`).
- Prometheus exporter activation (`metrics-exporter-prometheus` init) → **Task 15** (`crates/observability`).
- Telemetry-channel saturation drop counter → **Task 15** (`crates/observability`).
- Multi-consumer cursor / broadcast / `subscribe(name)` → **Phase 2** (event-log abstraction).
- Lock-free ring buffer replacement → **ADR-005 revisit trigger** (benchmark proof gate).
- Metric labels → **Task 15** (label policy decision).
- Domain event payloads (`BlockObserved`, etc.) → **Phase 2**.
- `alloy-primitives` direct dependency → **Phase 2** domain-event crate.
- Importing `EventBus` / `EventConsumer` from another crate to wire stages → **Task 16** (`crates/app`).
- Re-adding the other workspace members (`crates/journal`, `crates/config`, `crates/observability`, `crates/app`) → **Tasks 13–16** respectively.

If the implementer encounters pressure to expand scope into any of these, defer per spec §3.

---

## Summary

12 tasks, ~13–14 commits.

TDD discipline applied to:
- Task 4 (T1: `new(0)` reject — red is a compile error).
- Task 5 (T2: publish + recv basic flow + envelope preservation — red is `unimplemented!()` panic, green is the full publish path with direct blocking send + `now_ns` + `depth_as_f64` helpers).
- Task 6 (T3: backpressure_total via try_send + Full fallback — red is the 3s deadline trip with cleanup-on-failure (drain + join), green is the spec §7.3 step 6 match).

Test-first verification (no red phase needed because the implementation already covers the contract by Task 6) for:
- Task 7 (T4: closed-channel mapping).
- Task 8 (T5: try_recv None + recv-after-bus-drop).
- Task 9 (T6: invalid-meta retry safety).
- Task 10 (T7: sequence-exhaustion no-wrap, white-box).

Each test-first task includes an optional mutation check that briefly inverts the relevant invariant to confirm the test catches the regression, then reverts. This is cheap insurance against passive tests.

Workspace `Cargo.toml` precondition (member re-add + `parking_lot` workspace dep) and the crate scaffold are bundled in Task 1 as a single chore commit. This avoids the broken-checkout window that would result from committing the workspace edit before the crate manifest exists.

Final state matches spec §11 DoD items D1–D11. The crate is ready to be imported by `crates/journal` (Task 13) or `crates/app` (Task 16) on the next plan.
