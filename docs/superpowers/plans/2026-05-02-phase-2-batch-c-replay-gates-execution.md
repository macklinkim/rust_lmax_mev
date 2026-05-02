# Phase 2 Batch C — Replay + EXIT Gates

**Date:** 2026-05-02
**Status:** Draft v0.2 (Codex 2026-05-02 21:37:25 +09:00 MEDIUM REVISION REQUIRED — 3 substantive items: (1) drop `rust-lmax-mev-config` from `crates/replay` deps since P2-C has no concrete use; (2) add explicit block-hash pinning negative test G-Pin as a third CI test; (3) clarify the ignored live-smoke is an env-contract stub, NOT the Phase 2 EXIT correctness proof)
**Predecessor:** P2-B closed at `310f6c7`.

## Scope

New crate `crates/replay`. Owns Phase 2 EXIT (ADR-001):
- **G-Replay** — same recorded input + same code → byte-identical
  emitted `Vec<StateUpdateEvent>` across two runs.
- **G-State** — engine output decoded from recorded `eth_call` bytes
  matches hand-computed expected values byte-for-byte (tolerance 0
  per ADR-006).
- Block-hash pinning verified inside the recorded `EthCaller` mock
  per Codex 21:27:24 #3 (mismatch → `NodeError::Rpc("unexpected
  block_id")`). No `crates/app` wiring (P2-D).

## API surface

```rust
#[async_trait::async_trait]
pub trait Replayer<I>: Send + Sync {
    type Output;
    type Error;
    async fn replay(&self, input: I) -> Result<Vec<Self::Output>, Self::Error>;
}

pub struct StateReplayer { engine: Arc<StateEngine> }

impl Replayer<Vec<RecordedBlock>> for StateReplayer {
    type Output = StateUpdateEvent;
    type Error = StateError;
    // For each block: engine.refresh_block(b.number, b.hash).await
}

pub struct RecordedBlock { pub number: u64, pub hash: B256 }
pub struct RecordedEthCaller { /* (selector, pool, block_hash) -> Bytes */ }
impl EthCaller for RecordedEthCaller { /* asserts BlockId::Hash matches a recorded block */ }
```

`RecordedEthCaller` is supplied to `StateEngine::with_caller(...)` per
P2-B. Errors flow through existing `StateError` (`Node(#[from])` /
`Decode` / `Snapshot` / `UnknownPool`) — no new error type.

`JournalReplayer<I = FileJournal>` is deferred to a post-Phase-2 batch;
Phase 2 EXIT only requires `RecordedBlock`-driven replay.

## Risk decisions (NEW to this batch)

1. **Inline `const`-byte fixtures**, not JSON files — mirrors P2-B
   `MockEthCaller`. No `serde_json` dep, no `tests/fixtures/` directory.
2. **Tolerance = 0 for G-State**, `==` on `Vec<StateUpdateEvent>` per
   ADR-006 precedent.
3. **Block-hash pinning verified inside `RecordedEthCaller`** per
   Codex 21:27:24 #3: `BlockId` arg MUST equal `BlockId::Hash(recorded
   block_hash)`; mismatch → `NodeError::Rpc("unexpected block_id ...")`.
4. **Live smoke `#[ignore]`'d, env-contract STUB ONLY** under
   `tests/g_state_live.rs`, reads `MEV_LIVE_NODE_URL` env and asserts
   it parses as a URL. **NOT the Phase 2 EXIT correctness proof** —
   G-Replay + G-State + G-Pin together ARE the proof. Full live-
   comparison loop is Phase 4 hardening.
5. **G-State expected values hand-computed** in test source (literal
   `U256::from(...)`, `i32`, `u128`). Proves end-to-end decode +
   persistence + emit shape against recorded bytes.
6. **Trait kept** `Replayer<I>: Send + Sync` (assoc types + async) for
   future `JournalReplayer<I = FileJournal>` without breakage; Phase 2
   ships only `StateReplayer`.

## Test matrix (lean per `feedback_phase2_doc_volume.md`)

`crates/replay` (3 CI + 1 ignored = 4 declared):
- **G-Replay** `tests/g_replay.rs` — 5-block × 2-pool fixture; build
  fresh fixture/engine/snapshot twice; assert run1 events == run2
  events byte-identical.
- **G-State** `tests/g_state.rs` — same fixture; assert each emitted
  `StateUpdateEvent` matches hand-computed expected (V2 reserves+ts,
  V3 sqrtPriceX96+tick+liquidity); also asserts block-hash pinning
  witness sequence over the recorded blocks (positive coverage).
- **G-Pin** `tests/g_pin.rs` (NEW v0.2 per Codex 21:37:25 #2) — feed
  `RecordedEthCaller` an `eth_call_at_block` invocation with a
  `BlockId::Hash` whose hash is NOT in the recorded set, AND a
  `BlockId::Number(_)` (non-Hash variant); both MUST return
  `Err(NodeError::Rpc("unexpected block_id ..."))`. Surfaces a
  pinning regression at the engine boundary even without driving
  StateEngine.
- **G-State live (ignored)** `tests/g_state_live.rs` — env-contract
  STUB ONLY: reads `MEV_LIVE_NODE_URL` env and asserts that if the
  var is set, it parses as a URL. **Not the Phase 2 EXIT correctness
  proof** (G-Replay + G-State + G-Pin together ARE the proof). Full
  live-comparison loop is Phase 4 hardening.

Total 4 new tests; CI counts the 3 non-ignored. Workspace target 64 →
**67 in CI** (68 declared).

## Workspace + per-crate dependency deltas

Workspace `[workspace.dependencies]`: no new entries.

`crates/replay/Cargo.toml` runtime: `rust-lmax-mev-state` (re-exports
`EthCaller`/`StateEngine`/`PoolState`/`StateUpdateEvent`),
`rust-lmax-mev-node`, `rust-lmax-mev-journal`; workspace deps `alloy`,
`alloy-primitives`, `tokio`, `parking_lot`, `tracing`, `thiserror`;
per-crate `async-trait = "0.1"`. (`rust-lmax-mev-config` removed v0.2
per Codex 21:37:25 #1 — P2-C has no concrete config use; config wiring
+ TOML pool examples are P2-D scope.)

Dev: `tokio` (test-util/macros), `tempfile = "3"`.

## Commit grouping (4 commits)

1. **docs: add Phase 2 Batch C replay + gates execution note** — this file.
2. **chore(workspace): scaffold crates/replay** — members += "crates/
   replay", Cargo.toml + placeholder lib.rs.
3. **feat(replay): Replayer trait + StateReplayer + RecordedEthCaller**.
4. **test(replay): G-Replay + G-State + G-Pin EXIT gate tests + ignored live smoke**
   — `tests/g_replay.rs`, `tests/g_state.rs`, `tests/g_pin.rs`,
   `tests/g_state_live.rs`, shared `tests/common/mod.rs` fixture builder.
5. (optional) **chore(batch-p2-c): final fmt/clippy cleanup**.

Per `feedback_verification_cadence.md`: targeted per-commit; full gates
ONLY at batch close.

## Forbidden delta (only NEW)

- No live-mainnet calls in CI (the live test is `#[ignore]`d).
- No JSON fixture files (inline `const` bytes only per Risk Decision 1).
- No `JournalReplayer` impl yet (deferred per Risk Decision 6).
- All standing Phase 1 + Phase 2 forbids carry over.

## Question for Codex (pre-impl)

v0.2 incorporates Codex 21:37:25 (3 substantive items): config dep
dropped, G-Pin added as separate CI test, live-smoke clarified as
env-contract stub. Open question: anything else needed before the
4-commit ladder runs?

If APPROVED: execute the 4-commit ladder. If REVISION REQUIRED: edit
+ re-emit. If ADR/scope change: HALT to user.
