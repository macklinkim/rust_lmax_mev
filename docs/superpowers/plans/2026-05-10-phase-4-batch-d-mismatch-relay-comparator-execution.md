# Phase 4 Batch D — `MismatchCategory` + heuristic-vs-revm reconciliation + relay sim comparator

**Date:** 2026-05-10 KST
**Status:** Draft v0.4 (revised after manual Codex REVISION REQUIRED HIGH on v0.3). Four concrete revisions R12..R15 addressed in place; R7/R9/R10/R11 carried unchanged from v0.3 (already accepted). Q-D answers stand. Awaiting manual Codex re-review.
**Predecessor:** P4-C2 closed + pushed at `74fcec8` (manual Codex APPROVED HIGH 2026-05-10 KST).

## Phase 4 progress at P4-D start

| Batch | Status | HEAD |
|---|---|---|
| P4-A archive node | CLOSED | `e2b6704` |
| P4-B state-fetcher | CLOSED | `856a859` |
| P4-C1 simulator/state-fetcher infra | CLOSED | `7efbb8d` |
| P4-C2 real-revm + `ProfitSource::RevmComputed` flip + SR-1 Success | CLOSED | `74fcec8` |
| **P4-D MismatchCategory + heuristic-vs-revm reconciliation + relay sim comparator infra** | **THIS BATCH** | — |
| P4-E `BundleRelay` trait + Flashbots/bloXroute adapters + actual `eth_callBundle` HTTP client + relay-sim wiring | NOT STARTED | — |
| P4-F Sushiswap WETH/USDC + external mempool feed | NOT STARTED | — |
| P4-G final wiring + DoD audit + `phase-4-complete` tag | NOT STARTED | — |

## Honest scope claim (per Codex v0.1+v0.2 verdicts)

P4-D lands **comparator + type infrastructure** for ADR-006 §"Simulation pipeline" / §"Abort policy". It does NOT complete ADR-006's relay-sim mandate — the actual `eth_callBundle` HTTP client and pipeline wiring land in P4-E. The closeout note will say so explicitly.

## Scope (revised v0.3)

Three deliverables, all additive. No edits to `crates/app::wire_phase4`. `crates/simulator` body edits limited to a new `RecordingDb` wrapper + a new public `simulate_with_fingerprint()` variant; existing `simulate()` signature/body unchanged.

### Deliverable D-1 — `MismatchCategory` enum in `crates/types`

(Unchanged from v0.2.) `#[non_exhaustive]`, full rkyv + serde derives, `Hash`. P3-A carve-out precedent. 3 tests (MC-1..3).

```rust
// crates/types/src/lib.rs (additive)
#[non_exhaustive]
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
    serde::Serialize, serde::Deserialize,
)]
pub enum MismatchCategory {
    Profitability, Gas, Revert, StateDependency, BundleOutcome, Unknown,
}
```

### Deliverable D-2 — Heuristic-vs-revm reconciler (NEW `crates/simulator/src/reconcile.rs`; PURE per R11)

```rust
// crates/simulator/src/reconcile.rs (NEW)
pub struct ReconciliationReport {
    pub ratio_bps: Option<i64>,
    pub sign_flip: bool,
    pub revm_unprofitable_after_heuristic_pass: bool,
}

pub enum ReconciliationLabel { Equal, Divergent, RevmUnprofitable }

/// PURE function. NO metrics emission. NO I/O. Caller maps `label`
/// to a counter increment at the call site (which lives downstream
/// of the simulator and lands in P4-E or P4-G).
pub fn reconcile(
    heuristic_wei: U256,
    revm_wei: U256,
    status: &SimStatus,
) -> (ReconciliationReport, ReconciliationLabel);
```

R11 fix: `reconcile()` returns only the report + label. No `metrics::counter!` calls; no `metrics` crate dependency added in P4-D. The metric counter `simulator_reconciliation_total{outcome=...}` is a documented future contract for the P4-E (or P4-G) call site, NOT shipped in P4-D.

Tests (`crates/simulator/tests/reconcile.rs`): REC-1..6 (unchanged shape from v0.2; assertions now match the pure-function signature — assert returned `(report, label)`).

### Deliverable D-3 — Relay sim comparator (NEW crate `crates/relay-sim`)

#### D-3.a — `crates/simulator` additions (per R7 — fingerprint owned here, not in relay-sim)

R7 fix: `StateObservation` + `LocalStateFingerprint` move to `crates/simulator` (owner of the read-set). `crates/relay-sim` consumes them via the existing path-dep. No crate cycle.

```rust
// crates/simulator/src/lib.rs (additive)

pub use crate::observation::{StateObservation, LocalStateFingerprint};
pub mod observation;
pub mod recording_db;
pub mod reconcile;
```

```rust
// crates/simulator/src/observation.rs (NEW)
use alloy_primitives::{Address, B256, U256};

/// One observed storage read during local simulation. Carries the
/// (account, slot, value) triple revm reported when reading that slot.
#[derive(
    Debug, Clone, PartialEq, Eq,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
    serde::Serialize, serde::Deserialize,
)]
pub struct StateObservation {
    #[rkyv(with = crate::rkyv_compat::AddressAsBytes)]
    pub account: Address,
    #[rkyv(with = crate::rkyv_compat::U256AsBytes)]
    pub slot: U256,
    #[rkyv(with = crate::rkyv_compat::B256AsBytes)]
    pub value: B256,
}

/// Sparse read-set captured during ONE `simulate_with_fingerprint` call.
/// Comparator intersects with relay-side observations to detect
/// `StateDependency` mismatches.
#[derive(
    Debug, Clone, PartialEq, Eq,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
    serde::Serialize, serde::Deserialize,
)]
pub struct LocalStateFingerprint {
    #[rkyv(with = crate::rkyv_compat::B256AsBytes)]
    pub block_hash: B256,
    pub observations: Vec<StateObservation>,
}
```

`crates/simulator/src/rkyv_compat.rs` extends with `AddressAsBytes` (`Address` ↔ `[u8; 20]`) and `B256AsBytes` (`B256` ↔ `[u8; 32]`) adapters — same shape as the existing `U256AsBytes`. R9 fix.

#### D-3.b — `RecordingDb<DB>` wrapper + `simulate_with_fingerprint` (per R8)

R8 fix. `StrictMissingDb` is *not* the read-set; it is the preloaded fixture set. The new wrapper records actual revm-side reads:

```rust
// crates/simulator/src/recording_db.rs (NEW)
use revm::Database;
use std::cell::RefCell;
use crate::observation::StateObservation;

/// Database wrapper that records every successful `Database::storage`
/// call (account, slot, value). `Database::basic` and `code_by_hash`
/// are pass-through; only storage reads are recorded (storage is the
/// only field needed to detect StateDependency mismatches per ADR-006).
///
/// Failed reads (`Err(...)` from the inner DB) are NOT recorded —
/// they propagate up as `SimulationError` via the existing path.
///
/// **R12 fix**: implements both `Database` AND `DatabaseCommit`.
/// `commit()` delegates to the inner DB. The existing simulator path
/// uses `evm.transact_commit()` (pre-swap WETH funding + the swap
/// itself), which requires `DatabaseCommit`; without this impl the
/// `simulate_with_fingerprint(...)` body would not compile when the
/// inner CacheDB is wrapped.
pub struct RecordingDb<DB> {
    inner: DB,
    reads: RefCell<Vec<StateObservation>>,
}

impl<DB> RecordingDb<DB> {
    pub fn new(inner: DB) -> Self { /* ... */ }
    pub fn into_observations(self) -> Vec<StateObservation> {
        self.reads.into_inner()
    }
    pub fn inner(&self) -> &DB { &self.inner }
    pub fn inner_mut(&mut self) -> &mut DB { &mut self.inner }
}

impl<DB: Database> Database for RecordingDb<DB> {
    type Error = DB::Error;
    fn basic(&mut self, addr: Address) -> Result<Option<AccountInfo>, Self::Error> {
        self.inner.basic(addr)
    }
    fn code_by_hash(&mut self, hash: B256) -> Result<Bytecode, Self::Error> {
        self.inner.code_by_hash(hash)
    }
    fn storage(&mut self, addr: Address, slot: U256) -> Result<U256, Self::Error> {
        let v = self.inner.storage(addr, slot)?;
        self.reads.borrow_mut().push(StateObservation {
            account: addr,
            slot,
            value: B256::from(v.to_be_bytes()),
        });
        Ok(v)
    }
    fn block_hash(&mut self, n: u64) -> Result<B256, Self::Error> {
        self.inner.block_hash(n)
    }
}

// R12 fix: DatabaseCommit delegation. transact_commit() needs this.
impl<DB: DatabaseCommit> DatabaseCommit for RecordingDb<DB> {
    fn commit(&mut self, changes: HashMap<Address, Account>) {
        self.inner.commit(changes);
    }
}
```

```rust
// crates/simulator/src/lib.rs (additive — does NOT touch existing simulate())
impl LocalSimulator {
    /// Same execution path as `simulate`, but returns the recorded
    /// `LocalStateFingerprint` of slot reads alongside the outcome.
    /// **R13 fix**: parameter is `&RiskCheckedOpportunity`, mirroring
    /// the existing `simulate(...)` signature so the call site does
    /// not need to bypass the risk-checked surface. `simulate` itself
    /// is unchanged (no interior state mutation; this variant builds
    /// its own RecordingDb wrapper inline).
    ///
    /// FP-1 parity invariant (test): for any `RiskCheckedOpportunity`
    /// + fixture pair, `simulate_with_fingerprint(&rc).0 ==
    /// simulate(&rc)` — byte-identical `SimulationOutcome`.
    pub fn simulate_with_fingerprint(
        &self,
        risk_checked: &RiskCheckedOpportunity,
    ) -> Result<(SimulationOutcome, LocalStateFingerprint), SimulationError>;
}
```

R8 contract: `simulate_with_fingerprint` is a peer to `simulate`, NOT a wrapper that mutates state on `&self`. The `RecordingDb` lives entirely inside the call's stack frame. `LocalSimulator` itself does not gain interior state. CMP-style tests + the new FP-1 cover this.

DP-D9 (v0.2) is replaced by DP-D9' (v0.3) below.

#### D-3.c — `crates/relay-sim` types and comparator (per R10 + R9)

```rust
// crates/relay-sim/src/lib.rs (NEW crate)
use alloy_primitives::{B256, U256, Bytes};
use rust_lmax_mev_simulator::{SimStatus, SimulationOutcome, LocalStateFingerprint, StateObservation};
use rust_lmax_mev_types::MismatchCategory;

pub mod rkyv_compat;  // Adapter for U256/B256/Bytes per R9.

/// R10 fix: concrete shape. P4-D's MockRelaySimulator is the only
/// writer; the HTTP impl in P4-E will populate from `eth_callBundle`
/// request shape.
#[derive(
    Debug, Clone, PartialEq, Eq,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
    serde::Serialize, serde::Deserialize,
)]
pub struct RelaySimRequest {
    #[rkyv(with = rkyv_compat::B256AsBytes)]
    pub block_hash: B256,
    pub state_block_number: u64,
    /// Raw signed transaction bytes. P4-D never populates this from
    /// real signing infra (no funded key); test mock leaves it empty
    /// or sets test-vector bytes.
    #[rkyv(with = rkyv_compat::VecBytesAsVecVecU8)]
    pub txs: Vec<Bytes>,
}

/// R10 fix: reuse `SimStatus` from `crates/simulator` directly
/// (re-exported below for ergonomics). NO `RelaySimStatus` mirror —
/// avoids double-maintenance and keeps the comparator's Revert
/// detection a single enum-equality check.
pub use rust_lmax_mev_simulator::SimStatus as RelaySimStatus;

#[derive(
    Debug, Clone, PartialEq, Eq,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
    serde::Serialize, serde::Deserialize,
)]
pub struct RelaySimulationOutcome {
    pub gas_used: u64,
    pub status: SimStatus,                          // reused per R10
    #[rkyv(with = rkyv_compat::U256AsBytes)]
    pub measured_profit_wei: U256,
    pub state_observations: Vec<StateObservation>,  // reused from simulator per R7
    pub inclusion_index: u32,
    #[rkyv(with = rkyv_compat::U256AsBytes)]
    pub coinbase_transfer_wei: U256,
}

#[derive(
    Debug, Clone, PartialEq, Eq,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
    serde::Serialize, serde::Deserialize,
)]
pub struct LocalBundleShape {
    pub expected_inclusion_index: u32,
    #[rkyv(with = rkyv_compat::U256AsBytes)]
    pub expected_coinbase_transfer_wei: U256,
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]  // R15: Clone needed by MockRelaySimulator
pub enum RelaySimError {
    #[error("relay sim not configured")] NotConfigured,
    #[error("relay sim transport failure: {0}")] Transport(String),
    #[error("relay sim returned unrecognized payload: {0}")] UnrecognizedResponse(String),
}

#[async_trait::async_trait]
pub trait RelaySimulator: Send + Sync + 'static {
    async fn simulate_bundle(
        &self,
        req: RelaySimRequest,
    ) -> Result<RelaySimulationOutcome, RelaySimError>;
}

pub struct ComparatorInputs<'a> {
    pub local: &'a SimulationOutcome,
    pub local_shape: &'a LocalBundleShape,
    pub local_fingerprint: &'a LocalStateFingerprint,
}

/// Pure-data comparator: detects Profitability / Gas / Revert /
/// StateDependency / BundleOutcome. ZERO TOLERANCE per ADR-006.
pub fn compare(
    inputs: ComparatorInputs<'_>,
    relay: &RelaySimulationOutcome,
) -> Result<(), MismatchAbort>;

/// R3 wrapper (carried from v0.2). Classifies relay errors as
/// `MismatchCategory::Unknown`; otherwise delegates to `compare`.
pub fn compare_result(
    inputs: ComparatorInputs<'_>,
    relay_result: Result<&RelaySimulationOutcome, &RelaySimError>,
) -> Result<(), MismatchAbort>;

#[derive(
    Debug, Clone, PartialEq, Eq,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
    serde::Serialize, serde::Deserialize,
)]
pub struct MismatchAbort {
    pub category: MismatchCategory,
    pub detail: String,
    pub local: SimulationOutcome,
    pub local_shape: LocalBundleShape,
    pub local_fingerprint: LocalStateFingerprint,
    pub relay: Option<RelaySimulationOutcome>,
}
```

R9 fix: `crates/relay-sim/src/rkyv_compat.rs` defines:
- `U256AsBytes` (mirrors `crates/simulator`'s — re-implemented because rkyv-with adapters are tied to the crate that defines the deriving struct).
- `B256AsBytes`.
- `VecBytesAsVecVecU8` for `Vec<alloy_primitives::Bytes>` ↔ `Vec<Vec<u8>>`. (`alloy_primitives::Bytes` is a thin newtype around `bytes::Bytes`; the adapter converts each element via `Vec::from(bytes.as_ref())` on serialize and `Bytes::from(vec)` on deserialize.)

NB: there is no `Address` adapter in relay-sim because relay-sim does not hold any `Address` field directly — `StateObservation`'s `account: Address` is derived in `crates/simulator` with that crate's `AddressAsBytes` adapter; rkyv processes the nested struct via its own derive machinery, reusing the simulator-side adapter.

#### D-3.d — `MockRelaySimulator`

```rust
pub struct MockRelaySimulator(std::sync::Mutex<Result<RelaySimulationOutcome, RelaySimError>>);
impl Default for MockRelaySimulator {
    fn default() -> Self { Self(Mutex::new(Err(RelaySimError::NotConfigured))) }
}
impl MockRelaySimulator {
    pub fn program(&self, r: Result<RelaySimulationOutcome, RelaySimError>);
}
#[async_trait::async_trait]
impl RelaySimulator for MockRelaySimulator { /* returns the programmed value (clone) */ }
```

## Decision points (revised v0.3)

- **DP-D1**: `MismatchCategory` in `crates/types`, `#[non_exhaustive]`, `Hash`. Carve-out narrow.
- **DP-D2**: Reconciler is diagnostic only. Labels `Equal | Divergent | RevmUnprofitable`. R6 fix.
- **DP-D3**: Relay sim HTTP client deferred to P4-E (carried).
- **DP-D4**: `crates/relay-sim` peer crate; path-deps on `crates/types` and `crates/simulator` only. NO reverse dependency from `crates/simulator` → `crates/relay-sim`. R7 fix.
- **DP-D5**: `MismatchAbort` single canonical shape `{ category, detail, local, local_shape, local_fingerprint, relay: Option<_> }`. R4 carried.
- **DP-D6**: Comparator runs outside the bus producer task, called downstream of simulator + relay-sim awaits at the call site (call site = P4-E or P4-G). Topology unaffected.
- **DP-D7**: `MismatchCategory` and `crates/risk::AbortCategory` stay separate enums.
- **DP-D8**: NO edits to `crates/app::wire_phase4` in P4-D. R5 carried.
- **DP-D9' (REVISED — R8 fix)**: Read-set capture happens via a new `RecordingDb<DB>` wrapper in `crates/simulator/src/recording_db.rs`, and a new public peer method `LocalSimulator::simulate_with_fingerprint(...) -> Result<(SimulationOutcome, LocalStateFingerprint), SimulationError>`. The wrapper records every successful `Database::storage(addr, slot) -> Ok(value)` call as a `StateObservation`. `LocalSimulator` does NOT gain interior state — the `RecordingDb` lives entirely on the call's stack frame. Existing `simulate(...)` is unchanged. The v0.2 description (populated slots = read-set) was wrong; v0.3 corrects this.
- **DP-D10**: Comparator only flags slots present in BOTH `local_fingerprint.observations` and `relay.state_observations`. Relay-omitted slots are "no info", NOT a mismatch. CMP-5b guards this boundary.
- **DP-D11 (NEW — R11 fix)**: `reconcile()` is a PURE function; no `metrics::counter!` calls; no `metrics` crate dep. Caller maps `ReconciliationLabel` → counter increment at the (deferred-to-P4-E/G) call site. The metric counter name `simulator_reconciliation_total{outcome=...}` is a documented future contract, not shipped in P4-D.
- **DP-D12 (NEW — R10 fix)**: `RelaySimStatus` is a re-export of `SimStatus` from `crates/simulator` (single enum, single maintenance point). `RelaySimRequest` carries `block_hash` (B256), `state_block_number` (u64), and `txs` (`Vec<Bytes>` — populated only by the test mock in P4-D; the HTTP impl in P4-E is the first real producer).
- **DP-D13 (NEW — R9 fix)**: New rkyv adapters needed:
  - In `crates/simulator/src/rkyv_compat.rs`: ADD `AddressAsBytes` (`Address` ↔ `[u8; 20]`) + `B256AsBytes` (`B256` ↔ `[u8; 32]`). `U256AsBytes` already exists (from P3-E).
  - In NEW `crates/relay-sim/src/rkyv_compat.rs`: `U256AsBytes`, `B256AsBytes`, `VecBytesAsVecVecU8`.
  - All adapters follow the existing P3-E `U256AsBytes` shape (impl `ArchiveWith` + `SerializeWith` + `DeserializeWith` over the carrier `[u8; N]` or `Vec<Vec<u8>>` representation).
- **DP-D14 (NEW)**: `crates/simulator` body edits in P4-D are limited to: (a) `pub mod observation;` + `pub mod recording_db;` + `pub mod reconcile;` declarations in `lib.rs`; (b) the new `simulate_with_fingerprint` method on `LocalSimulator`; (c) two new adapter impls in `rkyv_compat.rs`. No edit to existing `simulate(...)` body. No edit to `StrictMissingDb`. No edit to `FixtureSet`.
- **DP-D15 (NEW — R12 fix)**: `RecordingDb<DB>` impls BOTH `revm::Database` AND `revm::DatabaseCommit`. `commit()` is a straight delegation to the inner DB. The existing simulator path uses `evm.transact_commit()` twice (pre-swap WETH funding + the swap itself); without `DatabaseCommit` the wrapper would not compile when used as the `evm` DB. `commit()` is NOT recorded — the comparator only needs storage *reads* to detect StateDependency mismatches. RDB-4 covers `commit()` delegation (read-after-commit returns the committed value).
- **DP-D16 (NEW — R13 fix)**: `simulate_with_fingerprint(&self, risk_checked: &RiskCheckedOpportunity) -> Result<(SimulationOutcome, LocalStateFingerprint), SimulationError>` mirrors `simulate(&self, risk_checked: &RiskCheckedOpportunity) -> Result<SimulationOutcome, SimulationError>` exactly in input shape. Rationale: the call site (P4-E driver) consumes `RiskCheckedOpportunity` from the upstream pipeline; bypassing the risk-checked surface would force an unwrap and lose the budget-gate guarantees. FP-1 parity asserts both methods produce the same `SimulationOutcome` on the same fixture + risk-checked input.
- **DP-D17 (NEW — R14 fix)**: Comparator precedence is **deterministic and documented**. When multiple categories could match, `compare(...)` returns the FIRST matching category in this priority order:
  1. **`Revert`** — if `local.status == Success` XOR `relay.status == Success`. (Liveness mismatch overshadows numeric mismatches; if one side reverts the numeric comparisons are not meaningful.)
  2. **`BundleOutcome`** — if `local_shape.expected_inclusion_index != relay.inclusion_index` OR `local_shape.expected_coinbase_transfer_wei != relay.coinbase_transfer_wei`. (Bundle-shape mismatch indicates the relay built a different bundle, so per-tx numeric comparisons are about a different transaction set.)
  3. **`StateDependency`** — if any slot in the intersection of `local_fingerprint.observations` × `relay.state_observations` disagrees on `value`. (State drift is the upstream cause of any downstream profit/gas drift, so flag the cause rather than the symptom.)
  4. **`Profitability`** — if `local.simulated_profit_wei != relay.measured_profit_wei`.
  5. **`Gas`** — if `local.gas_used != relay.gas_used`.
  6. (No match in the above five → `Ok(())` from `compare`; only the wrapper `compare_result` can yield `Unknown`, exclusively from a relay-side error.)
  Rationale: Revert > BundleOutcome > StateDependency > Profitability > Gas reflects "diagnose the cause, not the symptom" + "structural mismatches block numeric comparisons". CMP-10 (NEW) asserts this precedence by constructing inputs where ALL FIVE categories would match individually and asserting `Revert` is returned.

## Test matrix (revised v0.3)

| Bucket | Tests | Location |
|---|---|---|
| MC (D-1) | MC-1, MC-2, MC-3 | `crates/types/src/lib.rs` cfg(test) |
| REC (D-2) | REC-1..6 | `crates/simulator/tests/reconcile.rs` |
| OBS (D-3.a) | OBS-1 (StateObservation rkyv round-trip), OBS-2 (LocalStateFingerprint rkyv round-trip) | `crates/simulator/src/observation.rs` cfg(test) |
| RDB (D-3.b) | RDB-1 (storage call recorded; basic/code/block_hash NOT recorded), RDB-2 (failed read NOT recorded), RDB-3 (multiple reads in order), RDB-4 (R12 commit delegation: read-after-commit returns the committed value) | `crates/simulator/src/recording_db.rs` cfg(test) |
| FP (D-3.b) | FP-1 (R13 parity: same fixture + same `RiskCheckedOpportunity`, `simulate_with_fingerprint(&rc).0 == simulate(&rc)`; non-empty fingerprint; second call to `simulate_with_fingerprint` returns the same `(outcome, fingerprint)` — interior state not mutated) | `crates/simulator/tests/reconcile.rs` (or sibling) |
| CMP (D-3.c+d) | CMP-1, CMP-2, CMP-3, CMP-4a, CMP-4b, CMP-5, CMP-5b, CMP-6a, CMP-6b, CMP-7, CMP-8, CMP-9, CMP-10 (R14 precedence: all-five-match input → returns `Revert` per DP-D17 priority) | `crates/relay-sim/src/lib.rs` cfg(test) |
| **Total** | **MC 3 + REC 6 + OBS 2 + RDB 4 + FP 1 + CMP 13 = 29** | workspace 133 + 29 = **162 passed + 1 ignored** target |

Slight bump from v0.2 (21 → 27): R8 + R9 + R10 add OBS-1/2, RDB-1/2/3, and split CMP-9 (rkyv round-trip on `MismatchAbort`) is preserved; CMP-7 + CMP-8 cover both Unknown reachable paths.

## Q8 hardening invariants (verified at batch close)

- (a) No funded key (grep gate).
- (b) No signing infra (`RelaySimRequest::txs` only written by test mock in P4-D).
- (c) No `submit_bundle` wiring.
- (d) No `live_send` toggle.
- (e) No live tests in CI (mock + pure-function only).
- (f) No `tracing::*` macros in `crates/relay-sim/` or `crates/simulator/src/{observation,recording_db,reconcile}.rs`.
- (g) Fail-closed `MockRelaySimulator::default()` returns `Err(NotConfigured)` (CMP-8).
- (h) `crates/types` carve-out narrow to `MismatchCategory`.
- (i) `crates/simulator` body edits narrow per DP-D14.
- (j) NO `metrics::counter!` calls in `crates/simulator/src/reconcile.rs` (R11 / DP-D11).
- (k) NO crate cycle: `cargo tree -p rust-lmax-mev-relay-sim` shows path-dep on `rust-lmax-mev-types` and `rust-lmax-mev-simulator`; `cargo tree -p rust-lmax-mev-simulator` shows NO `rust-lmax-mev-relay-sim` (R7).

## Forbids (carry from prior + reaffirmed)

- No `eth_sendBundle`. No `eth_callBundle` HTTP wire-up.
- No funded key, no production signer, no signing infra.
- No `live_send = true`, no relay submission.
- No edits to `crates/state` / `crates/risk` / `crates/opportunity` / `crates/execution` / `crates/state-fetcher` / `crates/node` / `crates/ingress` / `crates/event-bus` / `crates/journal` / `crates/observability` / `crates/config` / `crates/app`.
- `crates/types` edit narrow to `MismatchCategory`.
- `crates/simulator` body edit narrow per DP-D14.
- No revm version bump.
- No new ADR.
- No tag.
- No destructive git, no force push, no `.claude/` / `AGENTS.md` staging, no committing `fixture_output.txt`.

## Process

Per the 2026-05-04 routine-closeout policy + the 2026-05-04 22:20 KST manual-Codex-review-mode policy:

1. Claude commits this v0.3 draft + writes the v0.3 review pack to `.coordination/claude_outbox.md`.
2. Claude STOPS and reports "manual Codex review required".
3. User pastes pack to Codex; relays verdict.
4. Claude records verdict in `.coordination/codex_review.md`.
5. APPROVED → impl + tests + batch-close gates + commit + push per routine policy. No tag.
6. REVISION REQUIRED → revise + re-emit.
7. Scope/ADR change → HALT to user.

No code or `Cargo.toml` edits in this turn. The plan doc is on disk uncommitted.

## Closeout claim P4-D will make at batch-close

> **P4-D ships the comparator + type infrastructure required by ADR-006 §"Abort policy".** It does NOT complete ADR-006's relay-sim mandate — the actual `eth_callBundle` HTTP client and the relay-sim → comparator wiring into the producer chain land in P4-E. P4-D's comparator is exercised against an in-memory `MockRelaySimulator` only.
