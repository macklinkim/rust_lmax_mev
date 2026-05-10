# Phase 5 Batch B — dynamic gas bidding strategy infrastructure

**Date:** 2026-05-10 KST
**Status:** Draft v0.3 (revised after manual Codex REVISION REQUIRED HIGH on v0.2, 2026-05-10 KST). Five cleanup/validation revisions R-B7..R-B11 applied (EIP-1559 fallible ctor + bps validation; BS-3 formula update; Q-B6 wording; DP-B6 helper consistency; process version bump). v0.1 R-B1..R-B6 + Q-B standing answers + BS-8 carried unchanged. Awaiting manual Codex re-review.
**Predecessor:** P5-A closed + pushed at `0c28d5c` (Codex closeout NO ACTION HIGH 2026-05-10 KST). Phase 5 overview at `ac07024`.

## Scope

Land the dynamic gas bidding infrastructure unlocked by ADR-006 §"Gas bidding" Phase 5+ language. Per overview Q-P5-3 + Q-P5-7 + R-P5-3 standing answers:

- Add a `BidStrategy` trait in `crates/execution`.
- Ship at least two impls: `FixedFractionBidStrategy` (preserves existing P3-F default) + `Eip1559BasefeeAwareBidStrategy` (NEW).
- `BundleConstructor::new(cfg)` keeps its existing signature + behavior byte-identical; NEW `BundleConstructor::with_strategy(cfg, Arc<dyn BidStrategy>)` ctor accepts an explicit strategy. The default `new(cfg)` internally constructs `FixedFractionBidStrategy::new(cfg.fixed_bid_fraction_bps)?` so all existing call sites + the `wire_phase4` default path remain unchanged. (R-B1: aligns Scope with DP-B5.)
- Document the canonical metric counter contract (`execution_bid_strategy_total{strategy=…}`) — implementation deferred per Q-P5-3.
- **NO ADR-006 text amendment** per R-P5-3 + Q-P5-7 standing — execution-note records the unlock only.
- **NO submission path change**. **NO signing**. **NO `live_send`**. **NO live network**.

## Decision points

- **DP-B1 (trait shape)**: object-safe, sync (no async I/O — bidding is a pure function of inputs).
  ```rust
  pub trait BidStrategy: Send + Sync + std::fmt::Debug + 'static {
      /// Stable canonical name for the metric label (must be stable
      /// across versions; spec-drift guard test asserts the wording).
      fn name(&self) -> &'static str;
      fn compute_bid(
          &self,
          outcome: &SimulationOutcome,
          ctx: &BidContext,
      ) -> U256;
  }
  ```
- **DP-B2 (REVISED v0.2 per R-B5 — `BidContext` shape + ctor)**: minimal Phase 5 fields; Phase 6+ extends. `#[non_exhaustive]` for forward compat; expose `BidContext::new(...)` ctor + a `for_legacy_outcome(&SimulationOutcome)` helper so external crates can construct without struct-literal access. Derives: `Debug + Clone + PartialEq + Eq` only (no serde — defer to Phase 6 per Q-B4).
  ```rust
  #[non_exhaustive]
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct BidContext {
      pub block_base_fee_wei: U256,
      pub gas_used_estimate: u64,
  }

  impl BidContext {
      pub fn new(block_base_fee_wei: U256, gas_used_estimate: u64) -> Self;

      /// Legacy helper: constructs a `BidContext` from just the
      /// simulation outcome with `block_base_fee_wei = U256::ZERO`.
      /// Used by `BundleConstructor::construct(&outcome)` to preserve
      /// pre-P5-B byte-identical behavior of the FixedFraction
      /// default strategy. Phase 6 wiring of live `block_base_fee_wei`
      /// through the bus replaces this helper at the call site.
      pub fn for_legacy_outcome(outcome: &SimulationOutcome) -> Self;
  }
  ```
  Rationale: `block_base_fee_wei` is the EIP-1559 floor every strategy needs; `gas_used_estimate` lets a strategy bound `bid ≤ (base_fee + tip_floor) * gas`. No other fields needed for the two P5-B impls. The `#[non_exhaustive]` + `BidContext::new` pattern lets Phase 6+ add fields (e.g., `prior_block_tip_distribution`) without breaking external constructors.
- **DP-B3 (REVISED v0.2 per R-B2 + R-B6 — `FixedFractionBidStrategy` semantics)**: byte-identical math to the existing P3-F `BundleConstructor::construct` —
  `bid = profit.saturating_mul(U256::from(bps)) / U256::from(10_000u32)`. Uses `saturating_mul` per Q-B5 / R-B6; div is U256 integer div (no panic). Constructor `pub fn new(bps: u16) -> Result<Self, ExecutionError>` validates `bps ≤ 10_000` (rejects with `ExecutionError::Setup("fixed_bid_fraction_bps must be in 0..=10_000, got N")`); the previous v0.1 `From<u16>` API is dropped (cannot fail-validate). `Default` impl uses `bps = DEFAULT_FIXED_BID_FRACTION_BPS = 9_000` and is therefore infallible.
- **DP-B4 (REVISED v0.3 per R-B3 + R-B6 + R-B7 — `Eip1559BasefeeAwareBidStrategy` semantics)**: new strategy.
  `bid = min( profit.saturating_mul(U256::from(bps)) / U256::from(10_000u32), (base_fee_wei.saturating_add(tip_floor_per_gas_wei)).saturating_mul(U256::from(gas_used_estimate)) )`.
  Caps the bid at the `(base_fee + tip_floor_per_gas) × gas` envelope so an over-eager profit estimate never overspends gas. Fields are **private**: `fixed_bid_fraction_bps: u16`, `tip_floor_per_gas_wei: U256`.
  R-B3: the field is **per-gas** (matches the EIP-1559 priority-fee unit); the cap multiplies by `gas_used_estimate` to derive total.
  R-B7 fallible constructor: `pub fn new(fixed_bid_fraction_bps: u16, tip_floor_per_gas_wei: U256) -> Result<Self, ExecutionError>` validates `bps ≤ 10_000` (rejects with `ExecutionError::Setup("fixed_bid_fraction_bps must be in 0..=10_000, got N")`); the validation mirrors `FixedFractionBidStrategy::new`. `Default` uses `bps = 9_000`, `tip_floor_per_gas_wei = 1 gwei` (1e9 wei) and is therefore infallible.
  All arithmetic uses `saturating_*` per R-B6 (no panic on extreme inputs).
- **DP-B5 (REVISED v0.2 per R-B1 + R-B2 — `BundleConstructor` integration, non-breaking)**: existing `BundleConstructor::new(cfg: BundleConfig) -> Result<Self, ExecutionError>` keeps identical signature + behavior. Adds NEW `BundleConstructor::with_strategy(cfg: BundleConfig, strategy: Arc<dyn BidStrategy>) -> Result<Self, ExecutionError>`. The default `new` body becomes:
  ```rust
  let strategy = Arc::new(FixedFractionBidStrategy::new(cfg.fixed_bid_fraction_bps)?);
  Self::with_strategy(cfg, strategy)
  ```
  All existing tests + `wire_phase4` path stay byte-identical (BS-4 regression-guards this). The previous v0.1 `From<u16>` API is dropped per R-B2 — fallible validation lives only on the constructor.
- **DP-B6 (REVISED v0.3 per R-B10 — BidContext at construct site, Phase 5 sourcing)**: in P5-B, the `BundleConstructor::construct(...)` body internally calls `BidContext::for_legacy_outcome(outcome)` (the `BidContext` API helper from DP-B2 R-B5) when the legacy single-arg `construct(&outcome)` API is called. The helper sets `block_base_fee_wei = U256::ZERO` and `gas_used_estimate = outcome.gas_used` — using the helper instead of struct-literal construction is mandatory because `BidContext` is `#[non_exhaustive]` per R-B5 (external code, including `BundleConstructor::construct`, cannot construct via struct literal across the field-private boundary). NEW `construct_with_context(&outcome, &ctx)` API takes the explicit context. The `wire_phase4` execution_driver continues to call the legacy `construct(&outcome)` path in P5-B — **no broadcast-channel type change** in Phase 5; live `block_base_fee_wei` plumbing through the bus happens at P6 alongside the comparator's full bundle-shape work. This isolates P5-B from a `sim_tx` envelope shape change.
- **DP-B7 (metric contract documented; impl deferred per Q-P5-3)**: canonical metric name `execution_bid_strategy_total` with one label `strategy = <strategy_name>` (e.g., `fixed_fraction`, `eip1559_basefee_aware`). NOT emitted in P5-B; documented in code as a future-call-site contract. P5-E or Phase 6 wires the `metrics::counter!` call.
- **DP-B8 (no ADR-006 edit, R-P5-3 carry-forward)**: P5-B does NOT modify `docs/adr/ADR-006-execution-simulation-gas-bidding-abort-policy.md`. The P5-B execution note records inline that ADR-006 §"Gas bidding" already permits dynamic bidding in Phase 5+; the unlock is reading ADR-006, not rewriting it.
- **DP-B9 (forbids hold)**: no signing / no submission / no `live_send` / no funded key / no `eth_sendBundle` / no live API / no Phase 6 work / no asset-scope widening / no V3 fee-tier widening.

## Test matrix (lean per overview "avoid padding")

| Test | Verifies | Location |
|---|---|---|
| BS-1 trait object-safety | `let _: Box<dyn BidStrategy> = Box::new(FixedFractionBidStrategy::default()); let _: Box<dyn BidStrategy> = Box::new(Eip1559BasefeeAwareBidStrategy::default());` (compile-asserted helper) | `crates/execution/src/lib.rs` cfg(test) |
| BS-2 fixed-fraction parity | `FixedFractionBidStrategy::default().compute_bid(outcome, &ctx)` returns the same value as the legacy P3-F formula on a fixed input vector (regression guard against accidental drift) | `crates/execution/src/lib.rs` cfg(test) |
| BS-3 (REVISED v0.3 per R-B8) eip1559 cap behavior | `Eip1559BasefeeAwareBidStrategy::compute_bid` returns the LESSER of (`profit * bps / 10_000`) and (`(base_fee + tip_floor_per_gas) × gas`) for two paired inputs (one where fixed-fraction wins; one where the EIP-1559 cap binds) | `crates/execution/src/lib.rs` cfg(test) |
| BS-4 default strategy unchanged | `BundleConstructor::new(BundleConfig::defaults()).construct(&outcome)` produces a `BundleCandidate` byte-identical to the P3-F E-1/E-2 expected output (regression — `wire_phase4` path unchanged) | `crates/execution/src/lib.rs` cfg(test) |
| BS-5 (REVISED v0.2 per R-B4) with_strategy non-default + explicit context | `BundleConstructor::with_strategy(cfg, Arc::new(Eip1559BasefeeAwareBidStrategy::default())).construct_with_context(&outcome, &BidContext::new(nonzero_base_fee_wei, gas_used))` returns a `BundleCandidate` with `gas_bid_wei` capped per DP-B4. The legacy `construct(&outcome)` API is NOT used here because `BidContext::for_legacy_outcome` sets `block_base_fee_wei = 0` which doesn't exercise the EIP-1559 cap — the test inputs choose values such that the cap binds (else fails to verify). | `crates/execution/src/lib.rs` cfg(test) |
| BS-6 strategy name stability | spec-drift guard: `FixedFractionBidStrategy::default().name() == "fixed_fraction"`; `Eip1559BasefeeAwareBidStrategy::default().name() == "eip1559_basefee_aware"`. Loosening the wording forces a test-text + metric-doc update | `crates/execution/src/lib.rs` cfg(test) |
| BS-7 (EXTENDED v0.3 per R-B7) bps validation on both strategies | `FixedFractionBidStrategy::new(11_000).is_err()` AND `Eip1559BasefeeAwareBidStrategy::new(11_000, U256::from(1_000_000_000u64)).is_err()` — both reject `bps > 10_000` with the same `ExecutionError::Setup` wording | `crates/execution/src/lib.rs` cfg(test) |
| BS-8 (NEW v0.2 per R-B6) saturating arithmetic / no-panic | Drives both strategies with extreme inputs (`U256::MAX` profit, `u64::MAX` gas, `U256::MAX` base fee, `U256::MAX` tip floor) and asserts `compute_bid` returns SOME `U256` value without panicking. Confirms the `saturating_*` contract from DP-B3 + DP-B4 + Q-B5. | `crates/execution/src/lib.rs` cfg(test) |

**Total**: 8 new tests. Workspace target: 213 → **221 passed + 1 ignored**.

## Hard forbids during P5-B (carried verbatim from overview Phase 5)

- No `eth_sendBundle`.
- No funded key / production signer / private key material.
- No `live_send=true`.
- No actual relay submission (`submit_bundle` stays `Err(SubmitDisabled)`).
- No real paid API in CI.
- No live-network test enabled by default.
- No Phase 6 Production Gate work.
- No new asset pairs / V3 fee tiers / venues.
- No edits to `crates/relay-sim` / `crates/bundle-relay` / `crates/relay-clients` / `crates/types` / `crates/risk` / `crates/opportunity` / `crates/state` / `crates/state-fetcher` / `crates/node` / `crates/ingress` / `crates/event-bus` / `crates/journal` / `crates/observability` / `crates/config` / `crates/simulator` / `crates/app`.
- `crates/execution` body edits: NEW `bid_strategy` module + new `BundleConstructor::with_strategy` ctor + internal refactor of `construct` body to delegate bid math to the strategy. Existing `BundleConstructor::new(BundleConfig)` external behavior unchanged; existing `BundleConfig` fields unchanged.
- NO ADR-006 text amendment (R-P5-3 / Q-P5-7).
- NO `wire_phase4` / `execution_driver` signature change in P5-B (DP-B6 isolation).
- NO `metrics::counter!` call in P5-B (DP-B7 — contract documented; impl deferred).

## Phase 5+ safety grep gate carry-forward

Per overview R-P5-2 (P5-C+ supersedes Phase 4 G2): the forbidden symbol set is `Wallet|PrivateKey|secp256k1|k256|alloy-signer|ethers-signers|sign_transaction|funded` — zero hits in P5-B-touched files. P5-B introduces no new Signer-trait symbols (Signer infrastructure lands in P5-C); the grep is a pure carry-forward.

## Codex Q-B standing answers (v0.3)

All six v0.1 open questions received Codex verdicts; encoded as standing decisions for v0.2.

- **Q-B1 — APPROVED**: separate `BundleConstructor::with_strategy(cfg, strategy)` ctor (matches P5-A `with_cache` precedent; keeps `BundleConfig` Serialize-clean).
- **Q-B2 — APPROVED with R-B3 unit fix**: default `tip_floor_per_gas_wei = 1 gwei per gas` (matches typical mainnet priority fees; keeps the strategy demonstrably non-trivial vs FixedFraction).
- **Q-B3 — APPROVED**: `BidContext.gas_used_estimate = outcome.gas_used` with 1.0× multiplier in P5-B; safety-multiplier tuning is a Phase 6 concern.
- **Q-B4 — APPROVED**: `BidContext` derives `Debug + Clone + PartialEq + Eq` only; serde defer to Phase 6.
- **Q-B5 — APPROVED with R-B6 contract**: `compute_bid` returns `U256` (pure; no I/O); arithmetic uses `saturating_*` to keep panics out (DP-B3/DP-B4 + BS-8 guards).
- **Q-B6 — APPROVED with R-B9 rewrite**: both built-in strategies derive `Default` (`bps = 9_000`; `tip_floor_per_gas_wei = 1 gwei`). Both built-in strategies also expose fallible constructors that validate `bps ≤ 10_000` (`FixedFractionBidStrategy::new(bps)` and `Eip1559BasefeeAwareBidStrategy::new(bps, tip_floor_per_gas_wei)`). `Default` is infallible because it uses known-good constants. (Previous v0.2 wording incorrectly said bps validation lived only on `FixedFractionBidStrategy::new`; corrected here per R-B7 which adds the same validation to the EIP-1559 ctor.)

## Process

Per the standing Phase 5 process from the overview v0.3:

1. Claude has emitted v0.3 + the review pack to `.coordination/claude_outbox.md`. Plan stays UNCOMMITTED on disk pending Codex APPROVED.
2. Claude STOPS and reports "manual Codex review required".
3. User pastes pack to Codex; relays verdict.
4. Claude records verdict in `.coordination/codex_review.md`.
5. **APPROVED** → commit + push P5-B plan as routine doc commit; THEN implement; THEN batch-close gates + commit + push.
6. **REVISION REQUIRED** → revise + re-emit.
7. **Scope/ADR change required** → HALT to user.

No code or `Cargo.toml` edits in this turn. No commit. No push. No tag.
