# Task 11: `crates/types` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the `crates/types` crate per `docs/superpowers/specs/2026-04-27-task-11-types-crate-design.md` (v0.3 Approved). Output: a single-file Lean crate exposing 7 types + `BlockHash` alias with `seal()`/`validate()` invariant enforcement and 4 inline tests, plus the workspace `Cargo.toml` corrections required for it to build.

**Architecture:** Single `lib.rs` (~250–350 LOC) housing all primitives. `EventEnvelope<T>` is the only encapsulated type — private fields, single validated `seal()` constructor, peer `validate()` for re-checking invariants on deserialized envelopes. Other types (`EventSource`, `ChainContext`, `PublishMeta`, `JournalPosition`, `SmokeTestPayload`, `TypesError`) are transparent data carriers. Derive matrix covers `rkyv` (hot-path) + `serde` (cold-path) per ADR-004; `thiserror` for the error enum.

**Tech Stack:** Rust 1.80, edition 2021. Workspace deps used: `rkyv = "0.8"` (features `["bytecheck"]`), `serde = "1"` (`features = ["derive"]`), `thiserror = "2"`. Dev-dep: `bincode = "1.3"` (serde adapter for cold-path round-trip test).

**Execution shell:** Windows 11 + PowerShell. Commands below are PowerShell-native. Notes:
- Many GNU CLI shims (`cat.exe`, `mkdir.exe`, `tee.exe`, `grep.exe`, `rg.exe`, ...) exist via Scoop, but PowerShell aliases (`cat`/`mkdir`/`tee`) shadow them. When GNU semantics are needed, append `.exe`.
- `scoop.ps1` may be blocked by execution policy — use `scoop.cmd list` or `cmd /c scoop list` for inspection if needed.
- Multi-line strings to native executables (e.g., `git commit -m`) use **multiple `-m` arguments** rather than here-strings. Each `-m` argument becomes a separate paragraph in the resulting commit message. This avoids edge-case parsing issues with `@'...'@` here-strings inside non-trivial shell contexts.
- Line-continuation is the backtick (`` ` ``) at end of line.

---

## Pre-flight Reading

Before starting, the implementer **must** read:

1. **Spec:** `docs/superpowers/specs/2026-04-27-task-11-types-crate-design.md` (v0.3, the contract).
2. **Frozen specs:** `docs/specs/event-model.md` (envelope schema), `docs/adr/ADR-004-rpc-evm-stack-selection.md` (rkyv/bincode policy), `docs/adr/ADR-005-event-bus-implementation-policy.md` (sequence/timestamp ownership).
3. **Project rules:** `CLAUDE.md` (rkyv 0.8 + alloy-primitives compat note), `PHASE_1_DETAIL_REVISION.md` §3.2/§3.4 (Replay Contract, sequence assignment).

The plan below assumes these are read. References use `§N.M` to point into the spec.

---

## File Structure

| Path | Action | Responsibility |
|---|---|---|
| `Cargo.toml` (workspace root) | **Modify** | (a) trim `members` to `["crates/types"]` for Task 11 isolation; (b) fix `rkyv` feature `"validation"` → `"bytecheck"` per spec §6.5. Other crates re-add their member entries during Task 12–16. |
| `crates/types/Cargo.toml` | **Create** | Manifest exactly per spec §6.2. |
| `crates/types/src/lib.rs` | **Create** | Single source file containing all types, methods, crate-level docstring, and `#[cfg(test)] mod tests`. |

**No other files are touched.** Tests are inline in `lib.rs`; there is no `tests/` directory for this crate (per spec §3.1).

---

## Why workspace `members` is trimmed

`Cargo.toml` line 3–10 currently lists all six Phase 1 crates as workspace members, but only `crates/types` is being created in Task 11. Cargo rejects a workspace whose `members` list references non-existent paths — `cargo check`/`test`/`clippy` would all fail to load the workspace before running the package build.

Two options:
1. Trim `members` to `["crates/types"]` for Task 11; subsequent tasks (12–16) re-add their respective entries.
2. Create empty stub crates for the other five members.

Option 2 is forbidden by `PHASE_1_DETAIL_REVISION.md` "empty crate 생성 금지" rule. Option 1 is taken. The trim is recorded in the same commit as the rkyv feature fix (Task 1).

---

## Task 1: Workspace `Cargo.toml` — trim members + fix rkyv feature

**Files:**
- Modify: `Cargo.toml` (workspace root, lines 3–10 and line 18)

This is the precondition for any subsequent task's `cargo` invocation to succeed.

- [ ] **Step 1.1: Read current workspace manifest**

```powershell
Get-Content Cargo.toml
```

Confirm lines 3–10 list six members and line 18 contains `rkyv = { version = "0.8", features = ["validation"] }`. (`cat` works as a PowerShell alias too, but `Get-Content` is the canonical cmdlet name.)

- [ ] **Step 1.2: Trim `members` to only `crates/types`**

Replace:

```toml
members = [
    "crates/types",
    "crates/event-bus",
    "crates/journal",
    "crates/config",
    "crates/observability",
    "crates/app",
]
```

with:

```toml
members = [
    "crates/types",
    # crates/event-bus, crates/journal, crates/config, crates/observability,
    # crates/app are added in Task 12-16 as those crates are created.
]
```

- [ ] **Step 1.3: Fix the `rkyv` workspace dependency feature**

Replace:

```toml
rkyv = { version = "0.8", features = ["validation"] }
```

with:

```toml
rkyv = { version = "0.8", features = ["bytecheck"] }
```

Rationale (spec §6.5): rkyv 0.8 has no `validation` feature. Default features are `bytecheck + std`, where `std` activates `alloc`. Explicitly listing `bytecheck` matches the spec target without changing the active feature set (since bytecheck is already on by default), but documents intent. Safe `rkyv::from_bytes` requires `bytecheck`, which is the rkyv 0.8 high-level API path used by the round-trip test (spec §7.5).

- [ ] **Step 1.4: Verify the workspace metadata loads**

```powershell
cargo metadata --format-version 1 --no-deps | Out-Null
```

Expected: exits 0 with no output. (Errors here mean the workspace is still misconfigured — re-check Steps 1.2/1.3.) On PowerShell, `| Out-Null` is the idiomatic equivalent of bash's `> /dev/null`.

- [ ] **Step 1.5: Commit**

```powershell
git add Cargo.toml
git commit `
    -m 'chore: scope workspace to crates/types and fix rkyv feature' `
    -m 'Trim workspace members to crates/types only for Task 11 isolation; remaining Phase 1 crates re-add their member entries in Task 12-16 as those crates are created. Empty stub crates are forbidden per PHASE_1_DETAIL_REVISION.' `
    -m 'Replace rkyv features = ["validation"] (non-existent in 0.8) with features = ["bytecheck"] per spec docs/superpowers/specs/2026-04-27-task-11-types-crate-design.md section 6.5. bytecheck is in rkyv 0.8 default features; this lists it explicitly to document intent and to keep the safe from_bytes high-level API available.' `
    -m 'Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>'
```

---

## Task 2: Scaffold the empty `crates/types` crate

**Files:**
- Create: `crates/types/Cargo.toml`
- Create: `crates/types/src/lib.rs`

Goal: a buildable empty crate. No types yet — just the manifest and an empty `lib.rs`.

- [ ] **Step 2.1: Create the directory tree**

```powershell
New-Item -ItemType Directory -Path crates/types/src -Force | Out-Null
```

PowerShell's `New-Item -ItemType Directory` creates intermediate directories as needed. `-Force` makes the command idempotent (no error if the path already exists). `| Out-Null` suppresses the directory-info object that would otherwise print.

- [ ] **Step 2.2: Write `crates/types/Cargo.toml`**

Exact content (matches spec §6.2):

```toml
[package]
name = "rust-lmax-mev-types"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
publish = false
description = "Phase 1 type primitives for the LMAX-style MEV engine"

[dependencies]
rkyv = { workspace = true }
serde = { workspace = true }
thiserror = { workspace = true }

[dev-dependencies]
bincode = { workspace = true }
```

- [ ] **Step 2.3: Write minimal `crates/types/src/lib.rs`**

Single line for now:

```rust
// Task 11 — types crate. Contents are added incrementally per the implementation plan.
```

- [ ] **Step 2.4: Verify the crate builds**

```powershell
cargo check -p rust-lmax-mev-types
```

Expected: compiles with no errors and no warnings (an empty lib has no warnings to emit).

- [ ] **Step 2.5: Commit**

```powershell
git add crates/types/Cargo.toml crates/types/src/lib.rs
git commit `
    -m 'feat(types): scaffold rust-lmax-mev-types crate' `
    -m 'Empty buildable crate with rkyv, serde, thiserror runtime deps and bincode dev-dep per spec section 6.2. Subsequent tasks fill the type definitions and tests inline in lib.rs.' `
    -m 'Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>'
```

---

## Task 3: Carrier types — `BlockHash`, `EventSource`, `ChainContext`, `PublishMeta`, `JournalPosition`, `SmokeTestPayload`

**Files:**
- Modify: `crates/types/src/lib.rs`

Goal: add the six non-error, non-envelope types (spec §4.2 – §4.6, §4.8). All are transparent data carriers with `pub` fields.

- [ ] **Step 3.1: Replace `lib.rs` content with the carrier types**

```rust
// Crate-level docstring is added in Task 10. EventEnvelope and TypesError
// are added in later tasks.

pub type BlockHash = [u8; 32];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum EventSource {
    Ingress,
    Normalizer,
    StateEngine,
    OpportunityEngine,
    RiskEngine,
    Simulator,
    Execution,
    Relay,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ChainContext {
    pub chain_id: u64,
    pub block_number: u64,
    pub block_hash: BlockHash,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PublishMeta {
    pub source: EventSource,
    pub chain_context: ChainContext,
    pub event_version: u16,
    pub correlation_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct JournalPosition {
    pub sequence: u64,
    pub byte_offset: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct SmokeTestPayload {
    pub nonce: u64,
    pub data: [u8; 32],
}
```

- [ ] **Step 3.2: Verify the crate builds**

```powershell
cargo check -p rust-lmax-mev-types
```

Expected: compiles with no errors. There will be no warnings about unused types because they are all `pub`.

If `rkyv::Archive` derive errors on `EventSource`/`ChainContext`/`SmokeTestPayload`:
- First sanity-check the rkyv 0.8 derive syntax in <https://docs.rs/rkyv/0.8>. The derive form may be `#[derive(rkyv::Archive)]` exactly, or may require the macro path to be brought into scope.
- Apply spec §7.6 fallback policy: the only Phase 1 fields involved are primitives (`u64`, `u16`, `[u8; 32]`, plain enum variants). If derive still fails on these primitives, this is a rkyv installation issue, not an alloy-primitives compat issue — surface to spec/ADR per Q2 escalation. **Do not silently fall back to bincode-only.**

- [ ] **Step 3.3: Commit**

```powershell
git add crates/types/src/lib.rs
git commit `
    -m 'feat(types): add carrier type primitives' `
    -m 'Adds BlockHash alias plus EventSource enum, ChainContext, PublishMeta, JournalPosition, and SmokeTestPayload structs per spec sections 4.2-4.6 and 4.8. All are pub-field transparent carriers with derive matrix per spec section 6.1.' `
    -m 'Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>'
```

---

## Task 4: `TypesError` enum

**Files:**
- Modify: `crates/types/src/lib.rs`

Goal: add the error type (spec §4.7) so subsequent `seal()`/`validate()` can return `Result<_, TypesError>`.

- [ ] **Step 4.1: Append `TypesError` to `lib.rs`**

After the `SmokeTestPayload` definition, add:

```rust
#[derive(Debug, thiserror::Error)]
pub enum TypesError {
    #[error("invalid envelope: field={field}, reason={reason}")]
    InvalidEnvelope {
        field: &'static str,
        reason: &'static str,
    },
    #[error("unsupported event_version: found={found}, max_supported={max_supported}")]
    UnsupportedEventVersion {
        found: u16,
        max_supported: u16,
    },
}
```

- [ ] **Step 4.2: Verify the crate builds**

```powershell
cargo check -p rust-lmax-mev-types
```

Expected: compiles with no errors. `UnsupportedEventVersion` may appear in output as "never constructed" — this is **expected** and **must not** be silenced with `#[allow(dead_code)]` (spec §4.7: pub enum variants are not subject to dead_code lint by default in stable Rust; if a warning surfaces under stricter clippy lints, defer to the final QA task).

- [ ] **Step 4.3: Commit**

```powershell
git add crates/types/src/lib.rs
git commit `
    -m 'feat(types): add TypesError with two variants' `
    -m 'InvalidEnvelope is raised by seal()/validate() at the construction and deserialize boundaries. UnsupportedEventVersion has no Phase 1 emit site - it is reserved for journal/replay decoders in Task 13 and Phase 2. Both variants use static string field/reason payloads to avoid heap allocation in the hot error path.' `
    -m 'Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>'
```

---

## Task 5: `EventEnvelope<T>` struct + accessor stubs

**Files:**
- Modify: `crates/types/src/lib.rs`

Goal: add the encapsulated envelope type with private fields, plus stub `seal()`/`validate()`/getters/`into_payload`. **Stub `seal()` returns `Ok` with no validation; stub `validate()` returns `Ok(())` always.** Real invariant logic is implemented in Task 6 and Task 7 via TDD.

This task makes the crate API surface complete enough to write tests against in subsequent tasks.

- [ ] **Step 5.1: Append `EventEnvelope<T>` definition + impl block to `lib.rs`**

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct EventEnvelope<T> {
    sequence: u64,
    timestamp_ns: u64,
    source: EventSource,
    chain_context: ChainContext,
    event_version: u16,
    correlation_id: u64,
    payload: T,
}

impl<T> EventEnvelope<T> {
    /// Seals an envelope with bus-assigned `sequence` and `timestamp_ns`.
    ///
    /// **STUB — Task 6 replaces this with the real invariant-checking
    /// implementation. Do not consume in production code while this stub
    /// is in place.**
    pub fn seal(
        meta: PublishMeta,
        payload: T,
        sequence: u64,
        timestamp_ns: u64,
    ) -> Result<Self, TypesError> {
        Ok(Self {
            sequence,
            timestamp_ns,
            source: meta.source,
            chain_context: meta.chain_context,
            event_version: meta.event_version,
            correlation_id: meta.correlation_id,
            payload,
        })
    }

    /// Re-validates Phase 1 invariants. **STUB — Task 7 replaces this.**
    pub fn validate(&self) -> Result<(), TypesError> {
        Ok(())
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn timestamp_ns(&self) -> u64 {
        self.timestamp_ns
    }

    pub fn source(&self) -> EventSource {
        self.source
    }

    pub fn event_version(&self) -> u16 {
        self.event_version
    }

    pub fn correlation_id(&self) -> u64 {
        self.correlation_id
    }

    pub fn chain_context(&self) -> &ChainContext {
        &self.chain_context
    }

    pub fn payload(&self) -> &T {
        &self.payload
    }

    pub fn into_payload(self) -> T {
        self.payload
    }
}
```

The docstring on `into_payload` is intentionally minimal here; the **full** docstring described in spec §5.5 (including the "When NOT to use" warning) is added in Task 10 alongside the crate-level docstring.

- [ ] **Step 5.2: Verify the crate builds**

```powershell
cargo check -p rust-lmax-mev-types
```

Expected: compiles with no errors. There may be `dead_code` warnings on getters/seal/validate because nothing calls them yet — these will go away once Task 6 adds tests.

If you see a derive error on `EventEnvelope<T>`'s `rkyv::Archive` derive related to bound forwarding (e.g., "T does not implement rkyv::Archive"), apply spec §7.6: rkyv 0.8 derive may need `#[rkyv(bound(...))]` for generics. Try the simplest invocation first; only add bounds if the error message specifically requests them.

- [ ] **Step 5.3: Commit**

```powershell
git add crates/types/src/lib.rs
git commit `
    -m 'feat(types): add EventEnvelope<T> with stub seal/validate/getters' `
    -m 'EventEnvelope is the only encapsulated type in the crate - all 7 fields are private, exposed only through seal() (construction), validate() (post-decode check), getters, and into_payload (consume).' `
    -m 'This commit adds the struct and stub method bodies so subsequent TDD tasks (Task 6, 7) can write tests against the API. seal() and validate() currently lack the invariant checks - those land in Task 6 and Task 7 via test-driven flows.' `
    -m 'Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>'
```

---

## Task 6: TDD — `seal()` invariant enforcement (Test 1)

**Files:**
- Modify: `crates/types/src/lib.rs`

Goal: write Test 1 (`seal_enforces_phase_1_invariants`) per spec §7.2, observe it fail against the stub, implement the real invariant logic, observe it pass.

This task introduces the test module and the `valid_envelope()` helper. The helper goes through `seal()` per spec §7.1 (no private-field bypass for fixtures).

- [ ] **Step 6.1: Append the test module skeleton + Test 1 to `lib.rs`**

At the end of `lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn valid_envelope() -> EventEnvelope<SmokeTestPayload> {
        let meta = PublishMeta {
            source: EventSource::Ingress,
            chain_context: ChainContext {
                chain_id: 1,
                block_number: 18_000_000,
                block_hash: [0xAB; 32],
            },
            event_version: 1,
            correlation_id: 42,
        };
        let payload = SmokeTestPayload {
            nonce: 7,
            data: [0xCD; 32],
        };
        EventEnvelope::seal(meta, payload, 100, 1_700_000_000_000_000_000)
            .expect("valid envelope should seal")
    }

    #[test]
    fn seal_enforces_phase_1_invariants() {
        let valid_meta = || PublishMeta {
            source: EventSource::Ingress,
            chain_context: ChainContext {
                chain_id: 1,
                block_number: 18_000_000,
                block_hash: [0xAB; 32],
            },
            event_version: 1,
            correlation_id: 42,
        };
        let valid_payload = || SmokeTestPayload {
            nonce: 7,
            data: [0xCD; 32],
        };

        // 1. timestamp_ns = 0 must reject
        let err = EventEnvelope::seal(valid_meta(), valid_payload(), 100, 0)
            .expect_err("timestamp_ns=0 must reject");
        assert!(matches!(
            err,
            TypesError::InvalidEnvelope { field: "timestamp_ns", .. }
        ));

        // 2. event_version = 0 must reject
        let mut bad_meta = valid_meta();
        bad_meta.event_version = 0;
        let err = EventEnvelope::seal(bad_meta, valid_payload(), 100, 1_700_000_000_000_000_000)
            .expect_err("event_version=0 must reject");
        assert!(matches!(
            err,
            TypesError::InvalidEnvelope { field: "event_version", .. }
        ));

        // 3. chain_id = 0 must reject
        let mut bad_meta = valid_meta();
        bad_meta.chain_context.chain_id = 0;
        let err = EventEnvelope::seal(bad_meta, valid_payload(), 100, 1_700_000_000_000_000_000)
            .expect_err("chain_id=0 must reject");
        assert!(matches!(
            err,
            TypesError::InvalidEnvelope { field: "chain_context.chain_id", .. }
        ));

        // 4. happy path - seal succeeds and getters return inputs verbatim
        let env = EventEnvelope::seal(
            valid_meta(),
            valid_payload(),
            100,
            1_700_000_000_000_000_000,
        )
        .expect("valid envelope must seal");
        assert_eq!(env.sequence(), 100);
        assert_eq!(env.timestamp_ns(), 1_700_000_000_000_000_000);
        assert_eq!(env.source(), EventSource::Ingress);
        assert_eq!(env.event_version(), 1);
        assert_eq!(env.correlation_id(), 42);
        assert_eq!(env.chain_context().chain_id, 1);
        assert_eq!(env.payload().nonce, 7);

        // 5. happy envelope must also pass validate() (cross-check that
        //    seal() and validate() accept the same valid inputs)
        env.validate().expect("happy envelope must pass validate()");
    }
}
```

- [ ] **Step 6.2: Run Test 1, expect FAIL**

```powershell
cargo test -p rust-lmax-mev-types seal_enforces_phase_1_invariants
```

Expected: test compiles but **fails on the first reject case** (timestamp_ns=0) because the stub `seal()` returns `Ok` unconditionally. The `.expect_err(...)` call panics. This is the red phase.

- [ ] **Step 6.3: Implement real invariant logic in `seal()`**

Two edits to `lib.rs`. Apply them in order.

**Edit A** — Add a crate-private helper **above** the `impl<T> EventEnvelope<T>` block:

```rust
fn check_envelope_invariants(
    timestamp_ns: u64,
    event_version: u16,
    chain_id: u64,
) -> Result<(), TypesError> {
    if timestamp_ns == 0 {
        return Err(TypesError::InvalidEnvelope {
            field: "timestamp_ns",
            reason: "must be non-zero",
        });
    }
    if event_version == 0 {
        return Err(TypesError::InvalidEnvelope {
            field: "event_version",
            reason: "must be non-zero",
        });
    }
    if chain_id == 0 {
        return Err(TypesError::InvalidEnvelope {
            field: "chain_context.chain_id",
            reason: "must be non-zero",
        });
    }
    Ok(())
}
```

**Edit B** — Replace the **entire** `seal()` method (stub docstring + signature + body, as written in Step 5.1) with this canonical production form. Copy-paste verbatim:

```rust
    /// Seals an envelope with bus-assigned `sequence` and `timestamp_ns`.
    ///
    /// **Intended caller: EventBus implementations only.** Downstream
    /// consumers receive sealed envelopes and access fields via getters.
    ///
    /// Validates Phase 1 invariants:
    /// - `timestamp_ns != 0`
    /// - `meta.event_version != 0` (event_version = 0 is reserved per
    ///   Phase 1 policy; see crate-level docs).
    /// - `meta.chain_context.chain_id != 0`
    ///
    /// `sequence`, `block_number`, `correlation_id` are accepted as-is.
    pub fn seal(
        meta: PublishMeta,
        payload: T,
        sequence: u64,
        timestamp_ns: u64,
    ) -> Result<Self, TypesError> {
        check_envelope_invariants(
            timestamp_ns,
            meta.event_version,
            meta.chain_context.chain_id,
        )?;
        Ok(Self {
            sequence,
            timestamp_ns,
            source: meta.source,
            chain_context: meta.chain_context,
            event_version: meta.event_version,
            correlation_id: meta.correlation_id,
            payload,
        })
    }
```

The 4-space indentation (inside `impl<T> EventEnvelope<T> { ... }`) must be preserved.

- [ ] **Step 6.4: Run Test 1, expect PASS**

```powershell
cargo test -p rust-lmax-mev-types seal_enforces_phase_1_invariants
```

Expected: PASS. (Note: `validate()` cross-check passes only because the stub `validate()` returns `Ok` — that case will be checked again in Task 7. For now it's a placeholder happy-path call.)

- [ ] **Step 6.5: Commit**

```powershell
git add crates/types/src/lib.rs
git commit `
    -m 'feat(types): seal() validates Phase 1 invariants (TDD)' `
    -m 'Implements the three seal() invariant checks (timestamp_ns, event_version, chain_id all non-zero) per spec section 5.2 by way of a crate-private check_envelope_invariants helper. Adds Test 1 seal_enforces_phase_1_invariants that exercises all three reject cases plus the happy path with full getter readback.' `
    -m 'Test 1 also calls env.validate() on the happy envelope as a cross-check; the validate() reject paths land in Task 7.' `
    -m 'Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>'
```

---

## Task 7: TDD — `validate()` invariant enforcement (Test 2)

**Files:**
- Modify: `crates/types/src/lib.rs`

Goal: write Test 2 (`validate_rejects_decoded_envelope_violations`) per spec §7.3 using struct-literal direct construction (allowed test-only per spec §5.3), observe failure against stub `validate()`, refactor `validate()` to share the invariant helper, observe pass.

- [ ] **Step 7.1: Append Test 2 to the `tests` module in `lib.rs`**

```rust
#[test]
fn validate_rejects_decoded_envelope_violations() {
    let valid_chain = ChainContext {
        chain_id: 1,
        block_number: 18_000_000,
        block_hash: [0xAB; 32],
    };
    let valid_payload = SmokeTestPayload {
        nonce: 7,
        data: [0xCD; 32],
    };

    // Case 1: timestamp_ns = 0 (simulates corrupted decoded frame).
    // Direct struct literal is permitted in test module per spec section 5.3.
    let bad_ts = EventEnvelope::<SmokeTestPayload> {
        sequence: 100,
        timestamp_ns: 0,
        source: EventSource::Ingress,
        chain_context: valid_chain.clone(),
        event_version: 1,
        correlation_id: 42,
        payload: valid_payload.clone(),
    };
    assert!(matches!(
        bad_ts.validate(),
        Err(TypesError::InvalidEnvelope { field: "timestamp_ns", .. })
    ));

    // Case 2: event_version = 0
    let bad_ver = EventEnvelope::<SmokeTestPayload> {
        sequence: 100,
        timestamp_ns: 1_700_000_000_000_000_000,
        source: EventSource::Ingress,
        chain_context: valid_chain.clone(),
        event_version: 0,
        correlation_id: 42,
        payload: valid_payload.clone(),
    };
    assert!(matches!(
        bad_ver.validate(),
        Err(TypesError::InvalidEnvelope { field: "event_version", .. })
    ));

    // Case 3: chain_context.chain_id = 0
    let bad_chain = EventEnvelope::<SmokeTestPayload> {
        sequence: 100,
        timestamp_ns: 1_700_000_000_000_000_000,
        source: EventSource::Ingress,
        chain_context: ChainContext {
            chain_id: 0,
            ..valid_chain.clone()
        },
        event_version: 1,
        correlation_id: 42,
        payload: valid_payload,
    };
    assert!(matches!(
        bad_chain.validate(),
        Err(TypesError::InvalidEnvelope { field: "chain_context.chain_id", .. })
    ));
}
```

- [ ] **Step 7.2: Run Test 2, expect FAIL**

```powershell
cargo test -p rust-lmax-mev-types validate_rejects_decoded_envelope_violations
```

Expected: all three `assert!(matches!(... Err(..)))` fail because stub `validate()` returns `Ok(())`. This is the red phase for `validate()`.

- [ ] **Step 7.3: Implement `validate()` using the shared helper**

Replace the stub `validate()` body with:

```rust
/// Re-validates Phase 1 invariants without reconstructing the envelope.
///
/// Use this at deserialization boundaries (journal decode, replay,
/// wire decode) to confirm a decoded envelope still satisfies the
/// invariants `seal()` enforced at construction time. `serde::Deserialize`
/// and `rkyv::Deserialize` reconstruct fields directly, **bypassing
/// `seal()`** — without this method, a corrupted frame could produce
/// an envelope with `timestamp_ns = 0`, `event_version = 0`, or
/// `chain_context.chain_id = 0`.
///
/// Checks the same three invariants as `seal()`:
/// - `timestamp_ns != 0`
/// - `event_version != 0`
/// - `chain_context.chain_id != 0`
///
/// Journal, replay, and decoder consumers MUST call `validate()`
/// after any deserialization, before passing the envelope to
/// downstream pipeline stages.
pub fn validate(&self) -> Result<(), TypesError> {
    check_envelope_invariants(
        self.timestamp_ns,
        self.event_version,
        self.chain_context.chain_id,
    )
}
```

- [ ] **Step 7.4: Run both tests, expect PASS**

```powershell
cargo test -p rust-lmax-mev-types
```

Expected: both `seal_enforces_phase_1_invariants` and `validate_rejects_decoded_envelope_violations` pass. (Test 1's `env.validate()` call is now backed by real logic too — it must still pass on the happy envelope.)

- [ ] **Step 7.5: Commit**

```powershell
git add crates/types/src/lib.rs
git commit `
    -m 'feat(types): validate() re-checks invariants on decoded envelopes (TDD)' `
    -m 'Implements EventEnvelope::validate() by delegating to the check_envelope_invariants helper introduced in Task 6, so seal() and validate() share one source of truth for the three Phase 1 invariants per spec section 5.3.' `
    -m "Adds Test 2 validate_rejects_decoded_envelope_violations that constructs invariant-violating envelopes via direct struct literal in the test module (permitted by same-module visibility, see spec section 5.3 'test-only direct struct literal') and asserts each violation is rejected with the expected field tag." `
    -m 'Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>'
```

---

## Task 8: Test 3 — serde + bincode round-trip

**Files:**
- Modify: `crates/types/src/lib.rs`

Goal: verify the serde derives on `EventEnvelope<SmokeTestPayload>` survive a `bincode` (1.x serde adapter) round-trip with `assert_eq!`-equality, and that `validate()` accepts the decoded envelope. Per spec §7.4, this is **not** a TDD task — derives are already in place; the test should pass on first run.

- [ ] **Step 8.1: Append Test 3 to the `tests` module**

```rust
#[test]
fn serde_bincode_round_trip_preserves_envelope() {
    let original = valid_envelope();
    let bytes = bincode::serialize(&original).expect("bincode serialize");
    let decoded: EventEnvelope<SmokeTestPayload> =
        bincode::deserialize(&bytes).expect("bincode deserialize");
    assert_eq!(original, decoded);
    // Demonstrates the standard decode-boundary call pattern: every
    // deserialize path must call validate() before passing the envelope
    // downstream.
    decoded.validate().expect("decoded envelope must pass validate()");
}
```

- [ ] **Step 8.2: Run Test 3, expect PASS**

```powershell
cargo test -p rust-lmax-mev-types serde_bincode_round_trip_preserves_envelope
```

Expected: PASS. If FAIL, the most likely causes are:
- Missing `serde::Serialize`/`serde::Deserialize` derive on a field type. Confirm `EventSource`, `ChainContext`, `SmokeTestPayload` all have them. (`PublishMeta`'s serde derives are not exercised by this test because `PublishMeta` is not embedded in the envelope.)
- bincode dev-dep missing — recheck Step 2.2.

- [ ] **Step 8.3: Commit**

```powershell
git add crates/types/src/lib.rs
git commit `
    -m 'test(types): bincode round-trip preserves envelope semantic equality' `
    -m 'Adds Test 3 serde_bincode_round_trip_preserves_envelope. Verifies that serde derives + bincode 1.x serde adapter produce a decoded envelope that compares equal to the original via PartialEq, and that validate() accepts the decoded envelope (demonstrating the decode-boundary call pattern from spec section 5.3).' `
    -m "Implements the cold-path serializer side of ADR-004's serialization policy at the smallest verification surface." `
    -m 'Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>'
```

---

## Task 9: Test 4 — rkyv archive round-trip

**Files:**
- Modify: `crates/types/src/lib.rs`

Goal: verify rkyv 0.8 derives on `EventEnvelope<SmokeTestPayload>` survive an archive→deserialize round-trip with `assert_eq!`-equality, and that `validate()` accepts the decoded envelope. Per spec §7.5.

- [ ] **Step 9.1: Append Test 4 to the `tests` module**

```rust
#[test]
fn rkyv_archive_round_trip_preserves_envelope() {
    let original = valid_envelope();

    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original)
        .expect("rkyv serialize");
    let decoded: EventEnvelope<SmokeTestPayload> =
        rkyv::from_bytes::<EventEnvelope<SmokeTestPayload>, rkyv::rancor::Error>(&bytes)
            .expect("rkyv deserialize");

    assert_eq!(original, decoded);
    decoded.validate().expect("decoded envelope must pass validate()");
}
```

- [ ] **Step 9.2: Run Test 4, expect PASS**

```powershell
cargo test -p rust-lmax-mev-types rkyv_archive_round_trip_preserves_envelope
```

Expected: PASS.

- [ ] **Step 9.3: If FAIL — diagnose per spec §7.6 fallback table**

| Failure mode | Action |
|---|---|
| `rkyv::to_bytes` / `rkyv::from_bytes` not found in scope or wrong signature | Cross-check <https://docs.rs/rkyv/latest/rkyv/fn.to_bytes.html> and <https://docs.rs/rkyv/latest/rkyv/fn.from_bytes.html> for rkyv 0.8 high-level API. The exact function paths and turbofish forms are version-pinned. Update the test code (not the crate dependency) to match. |
| `rkyv::rancor::Error` not found | The `rancor` module gates the unified error type. Check rkyv 0.8 docs for the actual error path; if a different name is canonical, use it. |
| Compile error on the `rkyv::Archive` derive for `EventEnvelope<T>` (generic) | Add `#[rkyv(bound(serialize = "...", deserialize = "..."))]` if rkyv 0.8 docs require explicit bound forwarding for generic structs. Reference rkyv 0.8 derive docs. **Do not switch to bincode-only** (spec §7.6 / Q2 escalation). |
| Feature gate error mentioning `bytecheck` or `alloc` | Re-verify Step 1.3 was applied. `safe from_bytes` requires `bytecheck` (default in 0.8). |
| Any other rkyv API drift between 0.8.0 and the actually-resolved 0.8.x patch | Surface to spec/ADR per Q2 escalation. Update the spec §7.5 test snippet to match working API and proceed. **Do not silently substitute serializers.** |

- [ ] **Step 9.4: Run all tests once Test 4 passes**

```powershell
cargo test -p rust-lmax-mev-types
```

Expected: 4 tests pass.

- [ ] **Step 9.5: Commit**

```powershell
git add crates/types/src/lib.rs
git commit `
    -m 'test(types): rkyv archive round-trip preserves envelope semantic equality' `
    -m 'Adds Test 4 rkyv_archive_round_trip_preserves_envelope. Exercises rkyv 0.8 to_bytes/from_bytes high-level API with rkyv::rancor::Error unified error path. Verifies decoded envelope is PartialEq with the original and passes validate().' `
    -m "Closes ADR-004's hot-path serialization consequence at the smallest verification surface for Phase 1 types." `
    -m 'Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>'
```

---

## Task 10: Crate-level docstring + `into_payload` full doc

**Files:**
- Modify: `crates/types/src/lib.rs`

Goal: add the spec §5.7 crate-level docstring and the spec §5.5 full `into_payload` docstring.

- [ ] **Step 10.1: Replace the `lib.rs` opening comment with the crate-level docstring**

Replace the placeholder line at the top of `lib.rs` with:

```rust
//! # rust-lmax-mev-types
//!
//! Phase 1 type primitives for the LMAX-style MEV engine.
//!
//! ## Sequence and timestamp ownership
//!
//! `EventEnvelope::sequence` and `EventEnvelope::timestamp_ns` are
//! **bus-assigned** invariants. Production code populates them exclusively
//! through the `EventBus::publish(payload, meta)` API; the bus calls
//! `EventEnvelope::seal()` internally with the values it owns.
//!
//! Direct calls to `seal()` outside the bus implementation are reserved for
//! tests and replay/decode infrastructure. ADR-005 and the event-model spec
//! govern the canonical ownership contract.
//!
//! ## Validation at the deserialize boundary
//!
//! `seal()` is the single _validated_ construction path. `serde::Deserialize`
//! and `rkyv::Deserialize` reconstruct envelope fields directly, **bypassing
//! `seal()`**. Journal, replay, and decoder consumers MUST call
//! `EventEnvelope::validate()` immediately after any deserialization to
//! re-enforce the same invariants `seal()` would have rejected at construction.
//!
//! ## Phase 1 version policy
//!
//! `event_version = 0` is treated as reserved/uninitialized in Phase 1 and is
//! rejected by both `seal()` and `validate()`. This is a Phase 1 policy
//! decision, not a constraint from the frozen event-model spec.
```

- [ ] **Step 10.2: Replace the placeholder docstring on `into_payload` with the full spec §5.5 version**

```rust
/// Consumes the envelope and returns only the payload, **intentionally
/// discarding all metadata** (sequence, timestamp_ns, source,
/// chain_context, event_version, correlation_id).
///
/// # When to use
///
/// Terminal consumers that have already logged or otherwise persisted the
/// envelope metadata upstream and only need the typed payload for further
/// processing.
///
/// # When NOT to use
///
/// **Never** call this at bus, journal, or replay boundaries. Metadata
/// preservation is a hard correctness requirement at those layers — losing
/// `sequence` or `correlation_id` breaks ordering and trace linkage.
///
/// If you need both the payload and a metadata field, prefer the getters
/// (`.payload()`, `.sequence()`, etc.) over consuming the envelope.
pub fn into_payload(self) -> T {
    self.payload
}
```

- [ ] **Step 10.3: Verify docs render**

```powershell
cargo doc -p rust-lmax-mev-types --no-deps
```

Expected: builds with no errors. (No need to open the HTML — successful build is sufficient evidence the doctests / inline links are well-formed.)

- [ ] **Step 10.4: Commit**

```powershell
git add crates/types/src/lib.rs
git commit `
    -m 'docs(types): add crate-level docstring and into_payload full doc' `
    -m 'Crate-level docstring covers sequence/timestamp ownership, the deserialize-boundary validate() obligation, and the Phase 1 event_version=0 reserved policy per spec section 5.7.' `
    -m 'into_payload docstring per spec section 5.5 with the explicit "When NOT to use" warning that deters use at bus/journal/replay boundaries.' `
    -m 'Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>'
```

---

## Task 11: Final QA — fmt, clippy, full test pass, DoD checklist

**Files:**
- Possibly modify: `crates/types/src/lib.rs` (only if fmt/clippy require)

Goal: verify all spec §9 Definition of Done items are satisfied. This task may produce zero commits if everything is already clean.

- [ ] **Step 11.1: `cargo fmt`**

```powershell
cargo fmt --check
```

If FAIL:

```powershell
cargo fmt
```

then re-run `cargo fmt --check` to confirm clean.

- [ ] **Step 11.2: `cargo clippy` with `-D warnings`**

```powershell
cargo clippy -p rust-lmax-mev-types -- -D warnings
```

Expected: no warnings, exits 0.

If clippy warns about `UnsupportedEventVersion` being unconstructed:
- Spec §4.7 and the reviewer note: this should not warn under default lints because `pub` enum variants are not subject to `dead_code`. If a stricter lint configuration triggers it, **do not** add `#[allow(dead_code)]`.
- Instead, leave the warning visible until Task 13 (journal decoder) emits the variant. If `-D warnings` makes this a blocker, surface to spec for guidance on how to handle the API-reservation pattern.

If any other clippy warnings surface that are clearly stylistic (e.g. needless `clone()` in tests), address them and amend the relevant prior commit OR add a `chore(types): clippy cleanup` commit at the end. Prefer the cleanup commit to keep history honest.

- [ ] **Step 11.3: Full test pass**

```powershell
cargo test -p rust-lmax-mev-types
```

Expected: 4 tests pass:
- `seal_enforces_phase_1_invariants`
- `validate_rejects_decoded_envelope_violations`
- `serde_bincode_round_trip_preserves_envelope`
- `rkyv_archive_round_trip_preserves_envelope`

- [ ] **Step 11.4: Build with no warnings**

```powershell
cargo build -p rust-lmax-mev-types
```

Expected: no warnings, exits 0.

- [ ] **Step 11.5: Walk the spec §9 DoD checklist**

Open `docs/superpowers/specs/2026-04-27-task-11-types-crate-design.md` and verify each item:

- [ ] Item 1 — 7 types + `BlockHash` alias defined.
- [ ] Item 2 — `crates/types/Cargo.toml` matches §6.2 byte-for-byte.
- [ ] Item 3 — `EventEnvelope` private fields, exactly 10 methods (7 getters + `seal` + `validate` + `into_payload`).
- [ ] Item 4 — `seal()` enforces 3 invariants, returns `TypesError::InvalidEnvelope`.
- [ ] Item 5 — `validate()` checks the same 3 invariants, returns same `field`/`reason` pairs.
- [ ] Item 6 — 4 tests pass; round-trip tests call `validate()`.
- [ ] Item 7 — `cargo build -p rust-lmax-mev-types` no warnings.
- [ ] Item 8 — `cargo clippy -p rust-lmax-mev-types -- -D warnings` clean.
- [ ] Item 9 — `cargo fmt --check` clean.
- [ ] Item 10 — crate docstring + `seal`/`validate`/`into_payload` docstrings present.
- [ ] Item 11 — workspace `Cargo.toml` rkyv feature corrected (Task 1 commit).

- [ ] **Step 11.6: (Optional) Cleanup commit**

If Steps 11.1–11.4 produced any auto-formatting or clippy fixes:

```powershell
git add crates/types/src/lib.rs
git commit `
    -m 'chore(types): final fmt/clippy cleanup' `
    -m 'Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>'
```

If everything was already clean, no commit is needed.

- [ ] **Step 11.7: Verify Task 11 readiness for Task 12 handoff**

Confirm by reading: `crates/types/src/lib.rs` exposes the API surface that Task 12 (`crates/event-bus`) will import:
- `EventEnvelope`
- `EventSource`
- `ChainContext`
- `PublishMeta`
- `TypesError`

Task 12 will likely also want:
- `JournalPosition` (only if event-bus exposes a `JournalPosition`-typed offset; otherwise Task 13's first consumer)
- `BlockHash` alias (probably yes for tests)
- `SmokeTestPayload` (for bus smoke test in Task 17 via the `EventBus<SmokeTestPayload>` smoke harness)

No code change here — purely a sanity readback.

---

## Out of scope for this plan

These are explicitly **not** Task 11 work and remain deferred per spec §8:

- Domain event payloads (`BlockObserved`, etc.) → Phase 2
- 100k bus smoke test → Task 17
- Journal frame byte stability tests → Task 13
- Snapshot smoke test → Task 13
- proptest fuzzing → Task 13/17
- `MAX_SUPPORTED_EVENT_VERSION` constant → consumer crates (journal/replay/decoder)
- `alloy-primitives` direct dependency → Phase 2 domain-event crate
- ADR-004 `bincode::Encode/Decode` vs bincode 1.x serde-adapter inconsistency → Task 13 ADR fix (spec §11.1)
- Re-adding the other five workspace members (`event-bus`, `journal`, `config`, `observability`, `app`) → Task 12, 13, 14, 15, 16 respectively

If the implementer encounters pressure to expand scope into any of these, defer per spec §8.

---

## Summary

11 tasks, ~10–11 commits. TDD discipline applied to `seal()` (Task 6) and `validate()` (Task 7); plain test-first for round-trip coverage (Tasks 8, 9); structural-only for type definitions (Tasks 3–5).

Workspace `Cargo.toml` precondition (members trim + rkyv feature fix) is bundled in Task 1 as a single commit because both edits touch the same file and are logically tied to enabling the types crate to build.

Final state matches spec §9 Definition of Done items 1–11. The crate is ready to be imported by `crates/event-bus` (Task 12) on the next plan.
