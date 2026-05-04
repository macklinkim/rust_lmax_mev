# Phase 4 Batch A — ADR-007 Archive Node Integration

**Date:** 2026-05-04
**Status:** Draft v0.2 (revised after Codex 2026-05-04 21:37:53 +09:00 REVISION REQUIRED MEDIUM; procedural — full content not visible to watcher — but Codex pre-supplied provisional Q1..Q5 answers, encoded below as Codex 21:37:53 v0.2 verdicts. Substantive plan unchanged from v0.1; v0.2 records the standing answers + clarifies the eth_getCode no-fallback per Codex's explicit verdict.)
**Predecessor:** Phase 4 overview at `8fcc83d` (Codex content review NO ACTION HIGH 2026-05-04 21:30:56).

## Scope

Land ADR-007 §"Archive access" ("required" at Phase 4):

1. Extend `crates/config::NodeConfig` (additive): `archive_rpc: Option<FallbackRpcConfig>`. Default `None` (fail-closed per Q8 hardening).
2. Extend `crates/node::NodeProvider` (additive — frozen-crate carve-out as in P3-A spec-compliance pattern, narrowly scoped to Phase 4 archive support): hold `archive_http: Option<Box<dyn HttpRpc>>` constructed at `connect()` time when `config.archive_rpc.is_some()`.
3. Add three archive-mode methods on `NodeProvider`: `eth_get_proof`, `eth_get_storage_at`, `eth_get_code`. All return `NodeError::ArchiveNotConfigured` if `archive_http.is_none()`. No fallback to primary HTTP for these calls — archive RPC reads at historical block heights are precisely what local-Geth-full-sync cannot serve, so falling back would silently produce wrong answers.
4. Extend `crates/node::HttpRpc` trait (additive): three new methods with default-impl returning `Err(NodeError::Rpc("archive method not implemented for this transport"))` so the existing `AlloyHttp` impl + any test mocks can opt in incrementally.
5. Wire `AlloyHttp` impls for the three methods using the `alloy::providers::Provider` proof/storage/code APIs.

Sample TOML refresh in `config/{base,dev,test}/default.toml` adds an optional `[node.archive_rpc]` block (commented-out by default — operators uncomment + supply API key when using an archive provider).

## Decision points (defaults; Codex pre-impl review confirms)

- **DP-1 (no-fallback policy)**: archive calls do NOT fall back to `primary_http` on `NodeError::Transport` (the standard P2-A fallback policy). If archive is unconfigured OR returns transport error, return `Err(NodeError::ArchiveNotConfigured | Transport)` immediately. Rationale: silently using a non-archive node for a historical-state read produces incorrect answers (it returns either the latest state or a "no data" error depending on the node's serving policy).
- **DP-2 (no in-process caching in P4-A)**: each call hits the archive endpoint directly. Caching strategy (per-block storage cache, LRU on `(block, address, slot)`) is P4-B state-fetcher concern, not P4-A. P4-A keeps the call surface clean.
- **DP-3 (cost-control discussion only — no quota enforcement in code)**: archive RPC providers (Alchemy/Infura archive tier) charge per call. P4-A's deliverable is the call surface; per-call rate limiting / request batching / quota tracking is operations-side (config + monitoring) not code-side. Document this in §"Cost control" below.
- **DP-4 (HttpRpc trait extension is additive)**: default impls return `Err(NodeError::Rpc("archive method not implemented for this transport"))` so existing test mocks in `crates/node/src/lib.rs` (P2-A) compile unchanged. Real `AlloyHttp` impl overrides with the proper `alloy::providers::Provider` calls.
- **DP-5 (config schema)**: `NodeConfig.archive_rpc: Option<FallbackRpcConfig>` reuses the existing `FallbackRpcConfig { url, label }` struct shape. Validate URL parse same as primary/fallback; no new validate variant needed unless the URL is empty (use existing `EmptyRequiredField`).

## Cost control (DP-3 elaboration; DOC ONLY — no code in P4-A)

Archive RPC calls (`eth_getProof`, `eth_getStorageAt`, `eth_getCode`) are the most expensive call class on hosted providers (Alchemy/Infura archive tier). The Phase 3 baseline made one `eth_call_at_block` per pool per block; P4 will additionally make ~10-50 storage reads per pool per block via the state-fetcher. At 5-10 pools per block × 10-50 storage slots × 1-2 blocks/sec = 100-1000 archive calls/sec sustained — enough to exceed most provider free tiers within minutes.

Operations-side mitigations (NOT in P4-A code, documented for the operator + Phase 5+ work):
- Use a dedicated archive provider (Alchemy/Infura archive plan) with sufficient quota.
- Add per-call request batching (`alloy::providers::batch::BatchRequest`) in P4-B state-fetcher to amortize HTTP overhead.
- Run a local archive-mode Geth (~2 TB+ disk) for production deployments (per ADR-007 §"Archive access" final paragraph).
- Add a per-second rate limiter at the operations layer (Phase 5 or Phase 6 production hardening).

## New types & API surface

```rust
// crates/config/src/lib.rs (additive on NodeConfig)
pub struct NodeConfig {
    pub geth_ws_url: String,
    pub geth_http_url: String,
    pub fallback_rpc: Vec<FallbackRpcConfig>,
    pub archive_rpc: Option<FallbackRpcConfig>,   // NEW — None default
}

// crates/node/src/lib.rs (additive)
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum NodeError {
    // ... existing variants ...
    /// Phase 4 P4-A: archive RPC call attempted but `archive_rpc` is
    /// not configured in NodeConfig.
    #[error("archive RPC not configured (set node.archive_rpc in config)")]
    ArchiveNotConfigured,
}

impl NodeProvider {
    // ... existing methods unchanged ...

    /// Phase 4 P4-A. Returns Err(ArchiveNotConfigured) if archive_rpc is not set.
    pub async fn eth_get_proof(
        &self,
        address: Address,
        slots: Vec<B256>,
        block_id: BlockId,
    ) -> Result<EIP1186AccountProofResponse, NodeError>;

    pub async fn eth_get_storage_at(
        &self,
        address: Address,
        slot: U256,
        block_id: BlockId,
    ) -> Result<B256, NodeError>;

    pub async fn eth_get_code(
        &self,
        address: Address,
        block_id: BlockId,
    ) -> Result<Bytes, NodeError>;
}

// HttpRpc trait gains three default-stub methods so existing mocks compile.
```

## Test matrix (lean per `feedback_phase2_doc_volume.md`)

`crates/node` (4 new tests; 9 total in node):
- **N4A-1 happy** `eth_get_storage_at_returns_archive_value` — mock `HttpRpc` impl returns a known `B256`; assert pass-through.
- **N4A-2 abort** `eth_get_storage_at_returns_archive_not_configured_when_unset` — `NodeProvider::connect` with `archive_rpc: None`; calling `eth_get_storage_at` returns `Err(NodeError::ArchiveNotConfigured)`. NO fallback to primary.
- **N4A-3 abort** `eth_get_proof_does_not_fall_back_on_transport_error` — mock archive returns `NodeError::Transport`; `NodeProvider::eth_get_proof` returns the `Err(Transport)` directly without trying primary HTTP (DP-1 policy).
- **N4A-4 boundary** `eth_get_code_round_trips_empty_bytes` — mock returns `Bytes::new()`; assert returned `Bytes` matches.

`crates/config` (1 new test; 5 total in config):
- **C4A-1** `archive_rpc_optional_in_minimum_toml` — `Config::from_toml_str` on the existing `valid_minimum_toml()` (no `archive_rpc`) parses; `cfg.node.archive_rpc.is_none()`.

Total 5 new tests; workspace cumulative: 107 → **112** in CI (+1 ignored unchanged).

## Workspace + per-crate dependency deltas

`crates/config/Cargo.toml`: no changes (uses existing `serde` + `alloy-primitives`).

`crates/node/Cargo.toml`: no changes (uses existing `alloy` + `alloy-primitives`).

`config/{base,dev,test}/default.toml`: add commented-out `# [node.archive_rpc]` block (operator uncomments + supplies endpoint when using archive).

## Commit grouping (4-5 commits)

1. `docs: add Phase 4 Batch A archive node execution note` — this file.
2. `feat(config): add NodeConfig.archive_rpc Option (additive)` — config struct + sample TOML refresh + C4A-1 test.
3. `feat(node): add archive HTTP methods to NodeProvider (eth_get_proof/getStorageAt/getCode)` — `HttpRpc` trait extension + `AlloyHttp` impl + `NodeProvider` methods + N4A-1..N4A-4 tests.
4. (optional) `chore(batch-p4-a): pick up fmt + Cargo.lock drift at batch close` — only if needed.

Targeted `cargo test -p rust-lmax-mev-config` and `cargo test -p rust-lmax-mev-node` per code commit; full workspace gates ONLY at batch close + tail-summary append.

## Forbidden delta (only NEW)

- No fallback from archive to primary HTTP (DP-1).
- No in-process caching in P4-A (DP-2; deferred to P4-B state-fetcher).
- No quota enforcement in code (DP-3; operations-side concern).
- Archive endpoint defaults `None` (fail-closed per Q8 hardening invariant).
- All Phase 4 forbidden additions carry over (no `eth_sendBundle`, no funded key, no `live_send=true`, no `.claude/`/`AGENTS.md` staging, etc.).

## Codex 21:37:53 v0.2 verdicts (encoded; standing answers unless revised)

- **Q1 (DP-1 no-fallback)**: keep no archive→primary fallback for ALL three methods including `eth_getCode`. Codex explicitly rejected the special-case fallback for `eth_getCode`. Rationale (mine): even though the deployed bytecode at a given address is immutable post-deployment, the archive call's failure mode (provider down, rate-limit, etc.) is the same as for state reads, and the no-fallback policy is one consistent policy easier to reason about than two.
- **Q2 (DP-2 no caching in P4-A)**: confirmed; LRU is P4-B state-fetcher concern.
- **Q3 (DP-4 HttpRpc trait default-stubs)**: acceptable if "clearly temporary/additive and tested" per Codex. The default-stubs are temporary in the sense that they exist only so P2-A test mocks don't need updating; they're additive (no API removal); they're tested by N4A-2 (which exercises the `ArchiveNotConfigured` path that necessarily traverses the default-stub when archive_http is None). v0.2 keeps DP-4 as written.
- **Q4 (test matrix)**: "directionally sufficient" per Codex; v0.2 keeps the 5-test matrix as written.
- **Q5 (Q8 invariants for batch close)**: Codex confirms — batch-close evidence pack MUST include explicit verification of: (a) `archive_rpc` defaults `None` (fail-closed) — verified by C4A-1 reading the minimum TOML + asserting `is_none()`; (b) env-gated `#[ignore]` network tests for any test that would touch a live archive RPC (mirrors `crates/replay/tests/g_state_live.rs` pattern); (c) secret redaction on archive URL logs — `tracing` emits the URL only at `debug` level with explicit `?` operator on a redacted shape, never the raw URL with API key at `info`/`error`.

## Question for Codex (v0.2 — non-scope items only)

v0.2 encodes Codex 21:37:53 verdicts on Q1..Q5 inline. Open: confirmation-at-approval only.

If APPROVED: execute the 3-4 commit ladder + batch-close evidence pack with auto_check.md tail-summary refresh. If REVISION: revise + re-emit. If ADR/scope/freeze change required: HALT to user.
