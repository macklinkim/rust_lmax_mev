# Phase 3 Batch A — Spec-Compliance Repair (rkyv + serde derives)

**Date:** 2026-05-04
**Status:** Draft v0.1. Pre-impl Codex review requested per Phase 3 overview §"Architectural risks" (P3-A: first edit to a P2-A/B-frozen crate after batch close, even if strictly additive). Architectural pivot: not a free design call but a spec-compliance repair against `docs/specs/event-model.md`.
**Predecessor:** Phase 3 overview at `c755ccb`.

## Scope

Bring P2-A `crates/ingress` and P2-B `crates/state` payload types into compliance with `docs/specs/event-model.md`, which mandates every `EventEnvelope<T>` payload type derive `Clone, Debug, PartialEq, rkyv::{Archive, Serialize, Deserialize}, serde::{Serialize, Deserialize}`. Touched types:

- `crates/ingress`: `IngressEvent`, `MempoolEvent`, `BlockEvent` (all currently `Debug, Clone, PartialEq, Eq` only).
- `crates/state`: `PoolId`, `PoolState`, `StateUpdateEvent` (currently have serde but lack rkyv; `StateUpdateEvent` lacks both serde and rkyv).

`crates/types::EventSource` already has all 8 spec variants (`Ingress`/`Normalizer`/`StateEngine`/`OpportunityEngine`/`RiskEngine`/`Simulator`/`Execution`/`Relay`) — no `crates/types` edit needed.

No API renames, removals, or behavior changes. Strictly additive derives + the field-level rkyv adapter wiring needed to make alloy-bound fields archivable (see §"Hard problem" below).

## Hard problem — alloy types and rkyv 0.8

`MempoolEvent` carries `B256` / `Address` / `U256` / `Bytes` (alloy_primitives + alloy::primitives::Bytes). Workspace `alloy-primitives = "0.8"` is pinned without an rkyv feature, and CLAUDE.md "Important Notes for AI Agents" already warns (verbatim):

> rkyv 0.8 has breaking API changes from 0.7. If derives do not work with alloy-primitives, use `[u8; N]` field types or fall back to bincode-only for Phase 1.

Since spec mandates rkyv, "fall back to bincode" is off the table for these payloads. Three options for closing the gap:

- **DP-A: rkyv-with adapters per field.** Use `#[rkyv(with = ...)]` field annotations + small `ArchiveWith`/`SerializeWith`/`DeserializeWith` impls in a new `crates/ingress::rkyv_compat` (and `crates/state::rkyv_compat`) submodule. Field type unchanged at the API surface; archived form is `[u8; N]` for fixed types and `ArchivedVec<u8>` for `Bytes`. Pros: API stays intact, derive macro carries the wiring; downstream code unchanged. Cons: small adapter glue per type (5 in ingress + 1 `U256` in state).
- **DP-B: Field-shape change** — switch field types to `[u8; 32]` / `[u8; 20]` / `Vec<u8>` directly; reconstruct alloy types at use sites. Cons: API breakage on `MempoolEvent` / `BlockEvent` / `StateUpdateEvent` field access; downstream code (P2-C `crates/replay`, gate tests, P2-D app types) must update. Loses the additive-only freeze carve-out.
- **DP-C: Wire-type companions** — define parallel `WireMempoolEvent { tx_hash: [u8;32], … }` etc., with `From`/`Into` conversion. Journal stores wire form; in-memory bus uses runtime form. Cons: doubles the type surface; conversion overhead at every journal append.

**Default: DP-A** — preserves the additive-only freeze carve-out (no API breakage), minimum code surface, idiomatic rkyv 0.8. Adapter modules are small and crate-local.

Codex Q1 below asks for sign-off on DP-A vs DP-B/C.

## Required derives (per `event-model.md`)

```rust
#[derive(Clone, Debug, PartialEq, Eq,
         rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
         serde::Serialize, serde::Deserialize)]
```

Applies to all 6 payload types listed in §Scope. Existing `Eq` derive stays where present (it's stricter than the spec's `PartialEq` requirement and already there). `Hash` is NOT in the spec's mandatory set; existing `Hash` derives stay where present.

## Workspace + per-crate dependency deltas

`crates/ingress/Cargo.toml`: already has `rkyv = { workspace = true }` and `serde = { workspace = true }`. No dep changes needed.

`crates/state/Cargo.toml`: ADD `rkyv = { workspace = true }` to `[dependencies]`. `serde` already present.

No workspace-level `Cargo.toml` changes (rkyv/serde already pinned).

## Test matrix (lean per `feedback_phase2_doc_volume.md`)

P3-A is a spec-compliance repair; tests verify the derives actually work end-to-end through the existing `FileJournal` + `EventEnvelope::validate` path:

- **A-1 happy** `crates/ingress/tests/rkyv_round_trip.rs` — construct one `MempoolEvent` (with non-zero `B256`/`Address`/`U256`/`Bytes`) + one `BlockEvent`; rkyv-serialize via `rkyv::to_bytes`; rkyv-deserialize via `rkyv::from_bytes`; assert byte-equal to the input.
- **A-2 happy** `crates/state/tests/rkyv_round_trip.rs` — same shape for `PoolState::UniV2` + `PoolState::UniV3` + `StateUpdateEvent`.
- **A-3 boundary** `crates/ingress/tests/journal_compat.rs` — `FileJournal::<MempoolEvent>::open` + `append` + `iter_all` round-trip across one envelope; asserts the `T: rkyv::Archive + Serialize<...>` bound is now satisfied (compile-time + runtime).

Total: 3 new tests. Workspace cumulative: 71 → **74** in CI.

## Commit grouping (3 commits)

1. **`docs: add Phase 3 Batch A spec-compliance execution note`** — this file.
2. **`feat(ingress): add rkyv + serde derives on IngressEvent/MempoolEvent/BlockEvent (spec compliance)`** — derives + `rkyv_compat` adapter module + A-1 + A-3 tests.
3. **`feat(state): add rkyv derives on PoolId/PoolState/StateUpdateEvent (spec compliance)`** — derives + adapter module if `U256` needs one + A-2 test + Cargo.toml rkyv dep.

Targeted `cargo test -p rust-lmax-mev-ingress` and `cargo test -p rust-lmax-mev-state` per code commit; full workspace gates at batch close.

## Forbidden delta (only NEW)

- No API renames / removals / behavior changes on touched types — derives + adapter glue ONLY (per the freeze carve-out in Phase 3 overview).
- No edits to ANY field type at the public API surface (DP-A constraint; rkyv-with adapters do not change field types).
- All standing Phase 1 + Phase 2 + Phase 3 forbids carry over.

## Question for Codex (pre-impl)

1. Is **DP-A** (rkyv-with adapters per alloy-bound field) the right approach vs. DP-B (field-shape change → API breakage) or DP-C (wire-type companions → doubled surface)? DP-A preserves the additive-only freeze carve-out.
2. Given rkyv 0.8's `#[rkyv(with = ...)]` syntax, is a per-crate `rkyv_compat` submodule the right home for the small adapters (one per `B256` / `Address` / `U256` / `Bytes`), or should they live in a new shared `crates/types::rkyv_compat` so they're reused across ingress + state?
3. Does `Hash` need to stay on `EventSource`-style enums where it's currently present? (Spec doesn't mention it; existing derives stay per "additive only" but worth confirming.)
4. Test matrix sufficient (3 tests covering rkyv round-trip for both crates' payloads + a journal_compat sanity)?
5. Anything else needed before the 3-commit ladder runs?

If APPROVED: execute the 3-commit ladder + batch-close re-emit. If REVISION: edit + re-emit. If ADR/spec change required: HALT to user.
