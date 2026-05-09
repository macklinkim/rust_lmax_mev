# Phase 4 Batch C — Real revm + `ProfitSource::RevmComputed` Flip

**Date:** 2026-05-04 (v0.2 revised 2026-05-09 KST; v0.3 revised 2026-05-09 KST later)
**Status:** Draft v0.3 (revised after MANUAL Codex 2026-05-09 KST second REVISION REQUIRED HIGH; addresses the USDC-proxy execution blocker + DP-C10 count fix + explicit V2/V3 calldata amount derivation. v0.2 confirmed acceptable on Q-C2/Q-C5/Q-C8/Q-C9/Q-C10 — no further open questions). v0.2 already addressed the original 8 required revisions: V3 callback topology, two-pool CacheDB, token-flow consistency, auxiliary-state fetcher API, UniV3 tick math + Tick.Info completeness, strict-missing DB wrapper, `SimulationOutcome` shape preservation, production-wiring honesty.
**Predecessor:** P4-B state-fetcher CLOSED at `856a859` (manual Codex APPROVED MEDIUM 2026-05-04 23:02).

## v0.2 → v0.3 changelog (per Codex 2026-05-09 second verdict)

| Codex item | v0.3 change |
|---|---|
| **BLOCKER USDC proxy execution** | NEW DP-C9a: explicit USDC FiatTokenProxy + FiatTokenV2_2 implementation modeling. Fixture loads (a) proxy code at `0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48`; (b) EIP-1967 implementation slot `0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc` value (= the implementation address); (c) implementation account code as a separate `FetchedAccount` at the implementation address; (d) the proxy storage slots the implementation reads during `transfer`: `paused` (slot 0 packed), `blacklisted[router]` / `blacklisted[V2_pool]` / `blacklisted[V3_pool]` (mapping at slot 5 keyed by address), `balanceOf[router]` / `balanceOf[V2_pool]` / `balanceOf[V3_pool]` (mapping at slot 9), and the rescuer/owner slots (1, 51) read by the modifier chain. NEW test T-USDC-1 directly proves `USDC.transfer(router → pool)` succeeds against the recorded fixture under `StrictMissingDb` with NO `MissingStorage` error. |
| Non-blocking DP-C10 count fix | DP-C10 heading updated to "18 new tests" matching the listed items. v0.3 adds T-USDC-1 → "19 new tests; workspace 123 → 142". |
| Non-blocking V2/V3 calldata amount derivation | DP-C5a expanded with explicit derivation. V2: `amount_out = uniswap_v2_get_amount_out(amount_in, reserve_in, reserve_out)` using the canonical `(amount_in * 997 * reserve_out) / (reserve_in * 1000 + amount_in * 997)` formula computed before building swap calldata; (`amount0Out`, `amount1Out`) selection driven by which token is being received. V3: exact-input positive `amount_specified = i256::try_from(amount_in).unwrap()`; `sqrt_price_limit_x96 = MIN_SQRT_RATIO + 1` for `zero_for_one = true`, `MAX_SQRT_RATIO - 1` for `zero_for_one = false` (constants `MIN_SQRT_RATIO = 4295128739`, `MAX_SQRT_RATIO = 1461446703485210103287273052203988822378723970342` — the canonical UniswapV3 boundary values). |

## v0.1 → v0.2 changelog (per Codex 2026-05-09 first verdict)

| Codex revision | v0.2 change |
|---|---|
| #1 V3 callback path | DP-C3 sets `tx.caller = MOCK_ROUTER_ADDRESS`. Router is `msg.sender` to pool; V3 callback returns to router bytecode. Profit measured on router's WETH balance delta. |
| #2 Two-pool CacheDB | DP-C4 `RevmDbBuilder::build` now consumes BOTH `source_pool_state` AND `sink_pool_state` and inserts both into one shared `CacheDB`. |
| #3 Token flow consistency | DP-C6 explicit token-flow specification: router starts with `OPTIMAL_AMOUNT_IN_WEI` WETH; swap-1 sells WETH at sink (expensive); swap-2 buys WETH at source (cheap); router ends with WETH+Δ; profit Δ is WETH delta on router's balanceOf slot. |
| #4 Auxiliary fetching | DP-C5 adds `StateFetcher::fetch_account` (additive trait method) + `FetchedAccount` shape to `crates/state-fetcher`. Tokens are fetched as separate `FetchedAccount`s; `RevmDbBuilder` merges them. `FetchedPoolState.auxiliary` field stays unused in P4-C (kept for forward-compat per P4-B). |
| #5 UniV3 tick math + Tick.Info | DP-C1 `compress_tick(tick, spacing)` uses Uniswap floor semantics (Rust trunc-toward-zero + adjustment for negative non-aligned). `UniswapV3Fee005Layout` Phase-3 emits 4 sequential slots per active tick (the full packed `Tick.Info`). SR-1..SR-3 explicitly assert the post-swap tick == pre-swap tick (no crossing) for the chosen probe size. |
| #6 No silent missing-slot success | NEW DP-C13 `StrictMissingDb<CacheDB<EmptyDB>>` wrapper implementing revm `Database` returns typed `MissingAccount` / `MissingStorage` errors on unset keys; revm bubbles these as `EVMError::Database`; simulator maps them to `SimulationError::Setup("missing fixture state for ...")`. NO silent zero reads. |
| #7 SimulationOutcome breakage | `MismatchDiagnostic` is **DEFERRED** to P4-D in full. P4-C's `SimulationOutcome` shape is byte-identical to P3-E except `profit_source` flips to `RevmComputed`. Zero downstream churn; `crates/execution` tests stay byte-identical. |
| #8 Production wiring honesty | NEW DP-C14: P4-C is fixture/test-only at the simulator level. `wire_phase4` is NOT touched. The runtime `LocalSimulator` requires `prefetch_for(&fetcher, opportunity)` before `simulate`; without it, `simulate` returns `Err(SimulationError::Setup("no fixture for pool {addr}@{block_hash}"))` — a hard, visible failure (NOT silent wrong answer). The runtime driver chain in `wire_phase4` is shadow-only per ADR-002 and currently consumes no `SimulationOutcome` for live action; production wiring of `prefetch_for` lands in P4-G `wire_phase5_pre_safety`. |

## Scope

Land the real-revm path that resolves the user-approved P3-E ADR-006 deferral. After P4-C close:

- The P3-E `[0x00]`-STOP test bytecode + `ProfitSource::HeuristicPassthrough` shim is gone from the active code path.
- `LocalSimulator::simulate` runs **real Uniswap V2 + V3 0.05% swap calldata** (two sequential swaps in one revm session) against **real on-chain bytecode + storage** loaded via `ArchiveStateFetcher` (P4-B).
- `simulated_profit_wei` is the revm-measured **WETH-balance delta** of the mock-router actor between pre- and post-execution snapshots.
- `profit_source = ProfitSource::RevmComputed` on every outcome; `HeuristicPassthrough` retained as `#[deprecated]` variant for rkyv compat.
- Determinism: same recorded `(opportunity, fixture-pair)` → byte-identical `SimulationOutcome` (extends P3-E S-2 + P4-B S-F-2 chain).

P4-C does NOT touch:

- Frozen P1 / P2 / P3 crates beyond `crates/simulator` (rewritten internal pipeline) and `crates/state-fetcher` (additive: `fetch_account` trait method + `FetchedAccount` shape + `signed_int_key` helper + `uniswap.rs` layouts module).
- `crates/types` — `MismatchCategory` enum is P4-D scope.
- `crates/risk` / `crates/opportunity` / `crates/execution` / `crates/app` — public API on `LocalSimulator::simulate` is byte-identical to P3-E.
- `BundleRelay` / signing / submission (P4-E).
- `MismatchDiagnostic` field on `SimulationOutcome` (deferred to P4-D per Rev #7).
- `wire_phase4` production wiring (deferred to P4-G per Rev #8).
- Sushiswap / second mempool feed (P4-F).

## Decision points (defaults; Codex pre-impl review confirms)

### DP-C1 — Verified Uniswap storage layouts (per Rev #5)

P4-C ships two `PoolSlotLayout` impls in **`crates/state-fetcher/src/uniswap.rs`** (NEW module; additive):

**`pub struct UniswapV2Layout`** — V2 pair (`UniswapV2Pair`) per canonical Solidity:
- slot 0: `factory` (address; recorded for provenance, not strictly needed for swap exec).
- slot 6: `token0` (address).
- slot 7: `token1` (address).
- slot 8: packed `(uint112 reserve0, uint112 reserve1, uint32 blockTimestampLast)`.
- **Verified against fixture**: at recorded mainnet block `B_V2_FIXTURE_HASH` we dump WETH/USDC pair `0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc` slots {0,6,7,8} via Alchemy `eth_getStorageAt` and inline the 32-byte values + an assertion that decoded `(reserve0, reserve1)` matches the `getReserves()` ABI return at the same block (red-on-mismatch).

**`pub struct UniswapV3Fee005Layout`** — V3 0.05% pool per canonical `UniswapV3Pool` Solidity:
- slot 0: packed `slot0` = `(uint160 sqrtPriceX96, int24 tick, uint16 observationIndex, uint16 observationCardinality, uint16 observationCardinalityNext, uint8 feeProtocol, bool unlocked)`.
- slot 1: `feeGrowthGlobal0X128` (uint256).
- slot 2: `feeGrowthGlobal1X128` (uint256).
- slot 3: packed `protocolFees` `(uint128, uint128)`.
- slot 4: `liquidity` (uint128).
- slot 5: `ticks: mapping(int24 => Tick.Info)` declaration slot.
- slot 6: `tickBitmap: mapping(int16 => uint256)` declaration slot.
- slot 7: `positions: mapping(bytes32 => Position.Info)` (NOT loaded — positions are mint/collect concern, not swap).
- slot 8: `observations: array[Oracle.Observation]` (NOT loaded — see DP-C12 caveat below; relies on observation-skip).

**Three-phase resolution for V3** (per P4-B DP-3 derived-slot loop, capped at `DERIVED_SLOTS_MAX_DEPTH = 3`):

- **Phase 1** `base_slots` returns `[0, 1, 2, 3, 4]`.
- **Phase 2** `derived_slots(after Phase 1)`:
  1. Parse `tick: i32` from slot 0 (bits 160..184, sign-extended from i24).
  2. Compute `compressed = compress_tick(tick, 10)` (Uniswap floor semantics — see DP-C1a below).
  3. Compute `wordPos = (compressed >> 8) as i16` for active word + `wordPos ± 1` for crossing margin (3 words per Phase 4 overview Q-B2).
  4. Return `[mapping_slot_u256(6, signed_int_key(wp as i32, 2)) for wp in [wordPos-1, wordPos, wordPos+1]]`.
- **Phase 3** `derived_slots(after Phase 1+2)`:
  1. Parse each fetched bitmap word (3 words = up to 768 set bits, but real WETH/USDC 0.05% pools have ~5-30 active ticks per active 256-bit window).
  2. For every set bit, compute `tick_index = (wp * 256 + bit_pos) * TICK_SPACING_005` (TICK_SPACING_005 = 10).
  3. For each `tick_index`, return the **4 sequential `Tick.Info` struct slots**: `[mapping_slot_u256(5, signed_int_key(tick_index, 3)) + i for i in 0..=3]`.
  - `Tick.Info` is 4 packed 32-byte words: `(uint128 liquidityGross + int128 liquidityNet)` || `feeGrowthOutside0X128` || `feeGrowthOutside1X128` || `(int56 tickCumulativeOutside + uint160 secondsPerLiquidityOutsideX128 + uint32 secondsOutside + bool initialized)`.
  - Returning 4 sequential slots per active tick is what Rev #5 mandates.

The Phase-2 → Phase-3 chain consumes 2 of the 3 allowed depth steps. The 3rd is reserved as a future safety margin.

### DP-C1a — Uniswap tick-compression floor semantics (per Rev #5)

Solidity (`TickBitmap.position`):
```solidity
int24 compressed = tick / tickSpacing;
if (tick < 0 && tick % tickSpacing != 0) compressed--;
int16 wordPos = int16(compressed >> 8);
uint8 bitPos = uint8(uint24(compressed % 256));
```

Rust mirror in `crates/state-fetcher/src/uniswap.rs`:
```rust
pub fn compress_tick(tick: i32, spacing: i32) -> i32 {
    let mut compressed = tick / spacing; // Rust trunc-toward-zero
    if tick < 0 && tick % spacing != 0 {
        compressed -= 1;
    }
    compressed
}
```

This is critical for negative ticks (mainnet WETH/USDC 0.05% spends much of its life at negative-tick territory: USDC has 6 decimals, WETH has 18; price `USDC/WETH ~ 1e-3` in raw units → `tick ~ -200000`). Rust `-1 / 10 = 0` (trunc), but `-11 / 10 = -1` (trunc), and the Solidity floor would be `-2`. The adjustment matters for every non-aligned negative tick.

Test **TM-1** `compress_tick_uniswap_floor_semantics` exercises:
- Positive aligned: `compress_tick(20, 10) == 2`.
- Positive non-aligned: `compress_tick(25, 10) == 2`.
- Negative aligned: `compress_tick(-20, 10) == -2`.
- Negative non-aligned: `compress_tick(-25, 10) == -3` (Solidity floor; NOT Rust trunc -2).
- Mainnet sample: `compress_tick(-200001, 10) == -20001` (verified: -200001/10 = -20000 trunc, -200001 % 10 = -1 ≠ 0, so -20001).

### DP-C2 — Signed mapping-key encoding (Codex 22:48 follow-up; v0.1 unchanged)

`crates/state-fetcher/src/storage_key.rs` gains `signed_int_key(value: i32, bytes: usize) -> B256`. Sign-extends i32 to 32 bytes per ABI rules (high padding = `0xff` for negative, `0x00` for positive; low `bytes` = signed BE encoding).

Tests S-K-3..S-K-5 cover MIN_TICK i24, positive i16, and `mapping_slot_u256` cross-check via a tiny-keccak-derived expected hash.

### DP-C3 — V3 callback execution topology (per Rev #1)

**Tx caller is the mock router**, not an EOA:

- `tx.caller = MOCK_ROUTER_ADDRESS = [0x33; 20]`.
- `tx.transact_to = TxKind::Call(pool_address)`.
- `msg.sender` inside the pool's `swap` function = `MOCK_ROUTER_ADDRESS`.
- UniV3 `IUniswapV3SwapCallback(msg.sender).uniswapV3SwapCallback(...)` therefore calls back into the router's bytecode.
- The mock router's bytecode (hand-assembled in `crates/simulator/src/mock_router.rs`) implements only the V3 callback selector `0xfa461e33`: it decodes `(int256 amount0Delta, int256 amount1Delta, bytes data)`, picks whichever delta is positive (the token the router owes), looks up `data` for the token address, and `transfer`s that amount from its own balance to `msg.sender` (the pool).
- For V2 swaps: the router uses the optimistic-payment pattern — it `transfer`s the input token directly to the pool BEFORE calling `swap(amount0Out, amount1Out, to, data="")`, mimicking the real periphery router. V2 has no callback so the router's bytecode does nothing on the V2 path.
- An "EOA" account is not part of the v0.2 design; the router IS the actor. Eliminates the `tx.caller = EOA → router` indirection per Rev #1's first option.
- Router is pre-funded at `RevmDbBuilder` time with:
  - Native ETH balance = `cfg.gas_limit_per_sim * cfg.base_fee_wei` (covers tx gas).
  - WETH balance = `OPTIMAL_AMOUNT_IN_WEI` (the probe size from `crates/opportunity::OPTIMAL_AMOUNT_IN_WEI = 1e16` = 0.01 ETH).
  - USDC balance = 0 (router buys USDC in swap-1 from sink, spends in swap-2 to source).

### DP-C4 — Two-pool shared `CacheDB` + `RevmDbBuilder` (per Rev #2 + #3)

New module `crates/simulator/src/cache_db_builder.rs`:

```rust
pub struct AuxiliaryAccounts {
    pub mock_router_address: RevmAddress,
    pub mock_router_bytecode: Bytes,
    pub mock_router_eth_balance_wei: U256,
    pub mock_router_weth_balance_wei: U256, // pre-funded WETH for swap-1
    pub weth_account: FetchedAccount,        // WETH9 code + balanceOf slots
    pub usdc_account: FetchedAccount,        // USDC code + balanceOf slots
}

pub struct PreparedSimulation {
    pub db: StrictMissingDb,                 // wraps CacheDB<EmptyDB>
    pub pre_router_weth_wei: U256,           // measured pre-execution
}

pub fn build_prepared(
    source_pool: &FetchedPoolState,
    sink_pool: &FetchedPoolState,
    aux: &AuxiliaryAccounts,
) -> Result<PreparedSimulation, SimulationError>;
```

Steps:

1. Fresh `CacheDB::new(EmptyDB::default())`.
2. Insert `source_pool` code + storage (every slot from `pool_storage`).
3. Insert `sink_pool` code + storage. Both pools must be at the same `block_hash` (asserted; `Err(SimulationError::Setup("source/sink block_hash mismatch"))` otherwise).
4. Insert `weth_account` and `usdc_account` code + storage.
5. Insert mock-router account info (code = router bytecode; balance = ETH for gas).
6. Insert mock-router's WETH `balanceOf[router]` slot via `mapping_slot_u256(WETH_BALANCES_SLOT=3, router_address_as_b256)` = `mock_router_weth_balance_wei`.
7. Insert mock-router's USDC `balanceOf[router]` = 0 (initial).
8. Compute `pre_router_weth_wei` by reading the same slot back through the strict DB (round-trip sanity).
9. Wrap `CacheDB` in `StrictMissingDb` (DP-C13).
10. Return `PreparedSimulation`.

**Determinism**: pure function; same inputs → byte-identical `CacheDB::AccountState` map (verified via `BTreeMap`-sorted snapshot in DB-2 test).

### DP-C5 — `state-fetcher` API extension: `fetch_account` (per Rev #4)

P4-C **extends** the `state-fetcher` public API additively:

```rust
// crates/state-fetcher/src/lib.rs (additive)

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedAccount {
    pub address: Address,
    pub block_hash: B256,
    pub code: Bytes,
    pub storage: Vec<(U256, B256)>,  // sorted by slot for determinism
}

#[async_trait]
pub trait StateFetcher: Send + Sync {
    // ... existing fetch_pool unchanged ...

    /// Fetch a non-pool contract account (e.g. ERC-20 token, callback
    /// receiver). `slots` is the explicit storage-slot list; the layout
    /// abstraction is bypassed because the caller (P4-C `RevmDbBuilder`)
    /// computes token slots directly via `mapping_slot_u256(BALANCES_SLOT, ...)`.
    async fn fetch_account(
        &self,
        address: Address,
        slots: &[U256],
        block_hash: B256,
    ) -> Result<FetchedAccount, FetchError>;
}
```

`ArchiveStateFetcher` impl shares the same caching layer (code + storage caches keyed by `(Address, BlockHash[, U256])`) and metrics counters as `fetch_pool`. The `metrics::counter!("state_fetcher_archive_calls_total", "method" => "get_storage_at")` already exists; `fetch_account` reuses it.

The `FetchedPoolState.auxiliary` field stays in the type (P4-B forward-compat) but P4-C never populates it; tokens are passed as separate `FetchedAccount`s into `RevmDbBuilder`.

### DP-C5a — Real swap calldata generation (v0.1 unchanged)

New `crates/simulator/src/swap_calldata.rs`:

```rust
pub const SELECTOR_V2_SWAP: [u8; 4] = [0x02, 0x2c, 0x0d, 0x9f];
pub const SELECTOR_V3_SWAP: [u8; 4] = [0x12, 0x8a, 0xcb, 0x08];
pub const SELECTOR_V3_CALLBACK: [u8; 4] = [0xfa, 0x46, 0x1e, 0x33]; // for mock router

pub fn univ2_swap_calldata(
    amount0_out: U256,
    amount1_out: U256,
    to: Address,
    data: &[u8],
) -> Bytes;

pub fn univ3_swap_calldata(
    recipient: Address,
    zero_for_one: bool,
    amount_specified: I256,
    sqrt_price_limit_x96: U256,
    data: &[u8],
) -> Bytes;
```

Hand-rolled minimal ABI encoding (selector + N×32-byte head + dynamic `bytes` tail). `data` carries the input-token address as a 32-byte ABI-encoded value so the V3 callback knows which token to repay.

**Explicit amount derivation (per Codex v0.3 non-blocking #2)**:

- **V2 amount-out**: use the canonical UniV2 fee-adjusted formula
  ```rust
  pub fn uniswap_v2_get_amount_out(
      amount_in: U256,
      reserve_in: U256,
      reserve_out: U256,
  ) -> U256 {
      let amount_in_with_fee = amount_in.checked_mul(U256::from(997u64)).unwrap();
      let numerator = amount_in_with_fee.checked_mul(reserve_out).unwrap();
      let denominator = reserve_in.checked_mul(U256::from(1000u64)).unwrap()
          .checked_add(amount_in_with_fee).unwrap();
      numerator / denominator
  }
  ```
  computed **before** building swap calldata. Inputs come from the fetched V2 pool's slot 8 packed reserves (decoded the same way `crates/state` already decodes `getReserves`). The `(amount0Out, amount1Out)` pair is set with the receiving side = `amount_out` and the other side = `0`. Direction is determined by which of `pool.token0`/`pool.token1` is the routed-input vs routed-output. WETH/USDC pair: `token0 = USDC`, `token1 = WETH` (per `crates/opportunity` lib doc); selling WETH at sink V2 = `amount_in (WETH) → amount0Out (USDC)`, so calldata = `swap(amount0Out=amount_out, amount1Out=0, router_addr, data="")`.

- **V3 amount-spec + price limit**: use exact-input positive `amount_specified = I256::try_from(amount_in).unwrap()` (positive sign indicates exact-input semantics per UniswapV3Pool source). `sqrt_price_limit_x96` boundaries:
  ```rust
  pub const MIN_SQRT_RATIO_PLUS_ONE: u128 = 4295128740;     // MIN_SQRT_RATIO + 1
  pub const MAX_SQRT_RATIO_MINUS_ONE: U256 = U256::from_be_slice(&[
      // 1461446703485210103287273052203988822378723970341 in 32-byte BE
      // (MAX_SQRT_RATIO - 1)
  ]);
  ```
  - `zero_for_one = true`  (selling token0): `sqrt_price_limit_x96 = MIN_SQRT_RATIO_PLUS_ONE`.
  - `zero_for_one = false` (selling token1): `sqrt_price_limit_x96 = MAX_SQRT_RATIO_MINUS_ONE`.
  These bounds let the swap consume any reachable liquidity; the no-tick-crossing assertion in SR-1/SR-2 then proves the chosen probe size stays inside the active tick window. WETH/USDC 0.05%: `token0 = USDC`, `token1 = WETH`; selling WETH = `zero_for_one = false` → `MAX_SQRT_RATIO_MINUS_ONE`; buying WETH = `zero_for_one = true` → `MIN_SQRT_RATIO_PLUS_ONE`.

The `amount_in` for swap-1 is the heuristic probe `OPTIMAL_AMOUNT_IN_WEI = 1e16` (in WETH wei). The `amount_in` for swap-2 is the exact USDC output of swap-1 (read back from the router's USDC `balanceOf` slot via `StrictMissingDb` between the two `transact_commit` calls).

### DP-C6 — Token flow + profit measurement (per Rev #3)

Per `crates/opportunity::OpportunityEngine::check`:
- `source_pool` = pool with HIGHER Q64 price = WETH cheaper there = the BUY-WETH side.
- `sink_pool` = pool with LOWER Q64 price = WETH expensive there = the SELL-WETH side.

P4-C atomic 2-hop arb in revm, starting + ending in WETH:

| Step | Actor | Action | Token flow |
|---|---|---|---|
| Pre | router | starts with `OPTIMAL_AMOUNT_IN_WEI` WETH (1e16 = 0.01 ETH) | router WETH = 1e16; USDC = 0 |
| 1 | router | `transfer` 1e16 WETH to `sink_pool` (V2 optimistic-pay) OR via V3 callback | router WETH = 0; sink_pool WETH += 1e16 |
| 2 | router | call `sink_pool.swap(...)` to receive USDC out | router USDC = X (sink-pool's `getAmountOut(1e16)` minus 0.3% V2 fee, or V3-equivalent) |
| 3 | router | `transfer` X USDC to `source_pool` (V2) OR via V3 callback | router USDC = 0; source_pool USDC += X |
| 4 | router | call `source_pool.swap(...)` to receive WETH out | router WETH = Y (source-pool's amountOut for X USDC, 0.05% V3 fee) |
| Post | router | ends with Y WETH | profit = Y − 1e16 (saturating-sub; Y < 1e16 → 0) |

`simulated_profit_wei = pre_router_weth_wei.saturating_sub(post_router_weth_wei).inverted_into_gain_or_zero()` — concretely: `if post >= pre { post - pre } else { U256::ZERO }`.

Note: per V2 / V3 mechanics, swap execution is a single `pool.swap(...)` call. The pre-pool funding (steps 1, 3) is what makes the swap succeed:

- V2 path: router does `IERC20(input).transfer(pool, amount_in)` then `pool.swap(amount0Out, amount1Out, router, "")`. Two top-level revm transactions per swap (transfer + swap), executed back-to-back in the same `Evm` (state persists between calls).
- V3 path: router does `pool.swap(router, zero_for_one, amount_specified, sqrt_price_limit, abi.encode(input_token))`. Inside the swap, V3 calls back into the router's bytecode at selector `0xfa461e33`; the router decodes `data`, reads which delta is positive, and `transfer`s that token from its own balance to `msg.sender = pool`. Single top-level revm tx per V3 swap.

For each of source/sink, P4-C dispatches the appropriate sequence based on `pool.kind`. The full pre→swap-1→swap-2→post path is 2-4 top-level revm `transact_commit` calls executed sequentially in the same `Evm` (DB persists; no rebuild between).

### DP-C7 — `MismatchDiagnostic` is DEFERRED to P4-D (per Rev #7)

v0.1 proposed adding `mismatch_diagnostic: Option<MismatchDiagnostic>` to `SimulationOutcome`. v0.2 **drops this entirely**. Rationale per Rev #7: `crates/execution` tests construct `SimulationOutcome` literals; adding a field is a freeze break.

P4-C's `SimulationOutcome` shape stays byte-identical to P3-E:
```rust
pub struct SimulationOutcome {
    pub opportunity_block_number: u64,
    pub gas_used: u64,
    pub status: SimStatus,
    pub simulated_profit_wei: U256,
    pub profit_source: ProfitSource,
}
```

The only difference is `profit_source = ProfitSource::RevmComputed` instead of `HeuristicPassthrough`. The `HeuristicPassthrough` variant stays in the enum with `#[deprecated]` (rkyv archive forward-compat).

Heuristic-vs-revm reconciliation lands in P4-D alongside the `MismatchCategory` enum (`crates/types` carve-out per overview §"Codex 21:24:10 v0.2 verdicts" Q2). P4-D coordinates the `crates/execution`-test edits required to add the diagnostic field; P4-C does not.

### DP-C8 — Failure semantics (revised v0.2)

| Failure | `status` | `simulated_profit_wei` | `profit_source` |
|---|---|---|---|
| Both swaps succeed, post ≥ pre | `Success` | `post − pre` | `RevmComputed` |
| Both swaps succeed, post < pre | `Success` | `0` (saturating) | `RevmComputed` |
| Either swap reverts | `Reverted { reason_hex }` | `0` | `RevmComputed` |
| Either swap halts (out-of-gas) | `OutOfGas` | `0` | `RevmComputed` |
| Either swap halts (other) | `HaltedOther { reason }` | `0` | `RevmComputed` |
| `StrictMissingDb` raises `MissingAccount`/`MissingStorage` | `Err(SimulationError::Setup("missing fixture state for {addr} slot {slot}"))` | n/a | n/a |
| Source/sink block_hash mismatch | `Err(SimulationError::Setup("source/sink block_hash mismatch"))` | n/a | n/a |
| `prefetch_for` not called and `simulate` invoked | `Err(SimulationError::Setup("no fixture for pool {addr}@{block_hash}"))` | n/a | n/a |
| Tick crossing detected (post-swap tick ≠ pre-swap tick on V3) | `Err(SimulationError::Setup("tick crossing not supported in P4-C; pre={pre} post={post}"))` (test-only assertion; safety guard) | n/a | n/a |

Per Rev #6: missing-slot is NEVER a silent zero. `StrictMissingDb` (DP-C13) makes this explicit.

### DP-C9 — Recorded-fixture strategy (v0.1 unchanged)

**Inline `const`-byte fixtures** per P2-B/P2-C precedent. NO JSON files, NO `serde_json` dep, NO live RPC in CI.

Fixtures land in `crates/simulator/src/fixtures.rs`:

- **`FIXTURE_V2_WETH_USDC_BLOCK_X`** — UniV2 pair `0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc` at recorded mainnet block hash (TBD, recently-finalized at impl time).
- **`FIXTURE_V3_WETH_USDC_005_BLOCK_X`** — UniV3 0.05% pool `0x88e6A0c2dDD26FEEb64F039a2c41296FcB3f5640` at the same block hash.
- **`FIXTURE_WETH9_BLOCK_X`** — WETH9 `0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2` code + the `balanceOf[ROUTER]` and `balanceOf[V2_POOL]` and `balanceOf[V3_POOL]` slots at the same block hash.
- **`FIXTURE_USDC_PROXY_BLOCK_X`** — USDC FiatTokenProxy `0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48` code + the proxy storage slots needed for `transfer` execution (concrete list in DP-C9a below) at the same block hash.
- **`FIXTURE_USDC_IMPL_BLOCK_X`** — FiatTokenV2_2 implementation account (address parsed from the proxy's EIP-1967 implementation slot at recording time) code at the same block hash. Implementation has no relevant own-storage because `delegatecall` semantics route all SLOAD/SSTORE through the proxy's storage context; only the implementation's runtime bytecode is needed.

**Fixture recording tool**: `crates/simulator/examples/dump_fixture.rs` (operator-only; CI never runs it; `ARCHIVE_RPC_URL` env required + absent → exits with clear error).

The tool runs `ArchiveStateFetcher::fetch_pool` for both pools + `fetch_account` for the proxy + `fetch_account` for the implementation address (parsed from the proxy fixture data), prints the storage as Rust literal text, and the implementer pastes into `fixtures.rs`.

### DP-C9a — USDC proxy + implementation modeling (per Codex v0.3 BLOCKER)

USDC at `0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48` is a **FiatTokenProxy** that `delegatecall`s into a **FiatTokenV2_2** implementation. Calling `USDC.transfer(pool, amount)` from the mock router executes:

1. Proxy fallback receives the calldata.
2. Proxy `SLOAD`s the EIP-1967 implementation slot `_IMPLEMENTATION_SLOT = bytes32(uint256(keccak256("eip1967.proxy.implementation")) - 1) = 0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc`.
3. Proxy `delegatecall`s the implementation code with the original calldata.
4. Implementation runs `transfer(...)` against the proxy's storage context: reads modifier state (`paused`, `blacklisted[caller]`, `blacklisted[recipient]`), reads/writes `balances[caller]` + `balances[recipient]`, emits `Transfer` event.

A naive load-just-the-proxy-bytecode approach FAILS in revm because:
- Step 2 requires the implementation slot to be populated; `StrictMissingDb` would raise `MissingStorage`.
- Step 3 requires `code_by_hash` of the implementation address to succeed; without an explicit `insert_account_info` for the implementation address, revm's `EXTCODECOPY` returns empty and `delegatecall` no-ops.
- Step 4 reads modifier-chain slots (paused / blacklisted) that, if unset, raise `MissingStorage`.

**P4-C explicit modeling (concrete fixture data required)**:

The `FIXTURE_USDC_PROXY_BLOCK_X` storage list MUST include:

| Slot | Source | Purpose |
|---|---|---|
| `0x36...bbc` (EIP-1967 impl) | recorded archive read | implementation address (32-byte ABI-encoded) |
| `0x36...103` (EIP-1967 admin) | recorded archive read | proxy admin address (read by FiatTokenProxy fallback's admin-bypass branch) |
| slot 0 (packed) | recorded archive read | `_initialized`, `_initializing`, etc. (modifier preconditions) |
| slot 1 | recorded archive read | `_owner` |
| slot 6 | recorded archive read | `paused` (false expected in a healthy block) |
| `mapping_slot_u256(5, signed_int_key(blacklister, 20))` | derived | unused for transfer; skip |
| `mapping_slot_u256(8, address_key(MOCK_ROUTER_ADDRESS))` | derived | `blacklisted[router]` (false expected) |
| `mapping_slot_u256(8, address_key(SOURCE_POOL_ADDRESS))` | derived | `blacklisted[source_pool]` |
| `mapping_slot_u256(8, address_key(SINK_POOL_ADDRESS))` | derived | `blacklisted[sink_pool]` |
| `mapping_slot_u256(9, address_key(MOCK_ROUTER_ADDRESS))` | router-funded value | `balanceOf[router]` (initial 0 for v0.3 because router buys USDC in swap-1, not pre-funded) |
| `mapping_slot_u256(9, address_key(SOURCE_POOL_ADDRESS))` | recorded archive read | `balanceOf[source_pool]` (real on-chain reserve at the recorded block) |
| `mapping_slot_u256(9, address_key(SINK_POOL_ADDRESS))` | recorded archive read | `balanceOf[sink_pool]` |
| slot 11 (packed `_totalSupply` etc.) | recorded archive read | totalSupply read by some impl branches |

The exact slot indices for `paused`, `blacklisted`, `balances`, `_owner`, etc. depend on the FiatTokenV2_2 inheritance chain (ContextUpgradeable → AbstractFiatTokenV2 → AbstractFiatTokenV1 → Ownable → Pausable → Blacklistable → Rescuable → FiatTokenV1 → FiatTokenV2 → FiatTokenV2_2). The recording tool MUST verify them by:

1. Recording the proxy storage slots {0..15} (head padding).
2. Recording the four mapping declaration slots (5 blacklister, 8 blacklisted, 9 balances, 10 allowed) for the four addresses (router + source_pool + sink_pool, plus the reserve list).
3. Running a probe `transfer` simulation against a vanilla `EmptyDB`-backed revm WITHOUT `StrictMissingDb` and recording every `(addr, slot)` pair revm reads via a `RecordingDb` instrumentation wrapper. Whatever the probe reads is exactly what the fixture must populate.

The recording tool `crates/simulator/examples/dump_fixture.rs` includes a `RecordingDb<EmptyDB>` mode that produces the precise slot list (no manual guessing of FiatTokenV2_2 inheritance offsets). The recorded slot list is committed inline in `fixtures.rs`.

**`FIXTURE_USDC_IMPL_BLOCK_X`** is a separate `FetchedAccount` carrying just the implementation address + its runtime bytecode + an EMPTY storage list (delegatecall semantics → impl has no own-storage reads).

`RevmDbBuilder::build_prepared` inserts BOTH proxy and impl as separate accounts at their respective addresses (proxy at `0xA0b8...`, impl at the address parsed from the EIP-1967 slot value).

**Test T-USDC-1 (NEW v0.3)** `usdc_transfer_router_to_pool_succeeds_against_recorded_fixture_under_strict_missing_db`:
- Build `StrictMissingDb` populated only from `FIXTURE_USDC_PROXY_BLOCK_X` + `FIXTURE_USDC_IMPL_BLOCK_X` + minimal mock-router account (with USDC balance preloaded for this test).
- Construct `transfer(SOURCE_POOL_ADDRESS, 1_000_000)` calldata (1.0 USDC at 6 decimals).
- `Evm.transact_commit` from `MOCK_ROUTER_ADDRESS` to USDC proxy.
- Assert: tx succeeds (no `EVMError::Database(StrictMissingError::*)`), `Transfer` event emitted with the right amount, `balanceOf[SOURCE_POOL_ADDRESS]` increased by 1_000_000, `balanceOf[MOCK_ROUTER_ADDRESS]` decreased by 1_000_000.
- The test serves as the proof that the recorded slot list is complete; if FiatTokenV2_2 reads a slot the fixture missed, `StrictMissingDb` raises and the test fails red.

**WETH9** at `0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2` is NOT a proxy — vanilla bytecode-deployed contract. Single `FetchedAccount` `FIXTURE_WETH9_BLOCK_X` with code + `balanceOf[router/V2_pool/V3_pool]` slots (mapping at slot 3) is sufficient. No implementation modeling needed.

### DP-C10 — Test matrix (revised v0.3: 19 new tests; workspace 123 → 142)

`crates/state-fetcher` (6 new):
- **S-K-3** `signed_int_key_negative_int24_matches_solidity_abi`.
- **S-K-4** `signed_int_key_positive_int16_matches_solidity_abi`.
- **S-K-5** `tickbitmap_mapping_slot_via_signed_key_matches_tiny_keccak_vector`.
- **L-V2-1** `uniswap_v2_layout_base_slots_returns_factory_token0_token1_reserves`.
- **L-V3-1** `uniswap_v3_005_layout_base_slots_returns_slot0_through_liquidity`.
- **L-V3-2** `uniswap_v3_005_layout_phase2_resolves_active_tickbitmap_window` + Phase-3 emits 4 sequential slots per active tick.
- **TM-1** `compress_tick_uniswap_floor_semantics_positive_aligned_negative_aligned_negative_unaligned_mainnet_sample`.
- **F-A-1** `archive_state_fetcher_fetch_account_returns_code_and_storage` — verifies the new `fetch_account` API + cache participation.

(Counted as 8; "S-K-3..5 + L-V2-1 + L-V3-1 + L-V3-2 + TM-1 + F-A-1" = 8 in `crates/state-fetcher`.)

`crates/simulator` (11 new):
- **CD-1** `univ2_swap_calldata_matches_solidity_abi_encoding`.
- **CD-2** `univ3_swap_calldata_matches_solidity_abi_encoding`.
- **CD-3** `uniswap_v2_get_amount_out_matches_canonical_fee_adjusted_formula`.
- **MR-1** `mock_router_bytecode_repays_input_token_in_v3_callback`.
- **DB-1** `build_prepared_inserts_both_pool_codes_and_storage_in_one_cachedb` (per Rev #2).
- **DB-2** `build_prepared_byte_identical_across_two_calls`.
- **DB-3** `strict_missing_db_returns_typed_error_on_unset_account_or_slot` (per Rev #6).
- **T-USDC-1** `usdc_transfer_router_to_pool_succeeds_against_recorded_fixture_under_strict_missing_db` (per v0.3 BLOCKER; proves DP-C9a fixture completeness).
- **SR-1** `local_simulator_revm_executes_v2_to_v3_arb_against_recorded_fixture` — assert `Success`, `profit_source = RevmComputed`, `gas_used > 50_000`, post-tick == pre-tick (no crossing).
- **SR-2** `local_simulator_revm_executes_v3_to_v2_arb_against_recorded_fixture` — symmetric direction.
- **SR-3** `local_simulator_revm_outcome_byte_identical_across_two_calls` (determinism).
- **SR-4** `local_simulator_setup_error_when_no_fixtures_loaded` (per Rev #8).

(Counted: 11 in `crates/simulator`. Total v0.3 = 8 fetcher + 11 simulator = **19 new tests**; workspace 123 → 142.)

NO new `app`/`risk`/`opportunity`/`execution` tests.

### DP-C11 — Workspace + per-crate dependency deltas (v0.2)

`crates/state-fetcher/Cargo.toml`: add `hex = { version = "0.4", default-features = false, features = ["alloc"] }` (test-vector parsing). Existing tiny-keccak dev-dep stays.

`crates/simulator/Cargo.toml`: add `rust-lmax-mev-state-fetcher = { workspace = true }`. revm pin (workspace 14) unchanged.

Workspace `Cargo.toml`: add `rust-lmax-mev-state-fetcher` to `[workspace.dependencies]` if not already present (verify at scaffold).

No other edits.

### DP-C12 — Commit grouping (5-6 commits; v0.2 unchanged in count)

1. `docs: add Phase 4 Batch C real-revm execution note` — this v0.2 file.
2. `feat(state-fetcher): UniswapV2 + UniswapV3Fee005 layouts + signed_int_key + compress_tick + fetch_account API + S-K-3..S-K-5 + L-V2-1 + L-V3-1 + L-V3-2 + TM-1 + F-A-1 tests`.
3. `feat(simulator): cache_db_builder + StrictMissingDb + swap_calldata + uniswap_v2_get_amount_out + mock_router + DB-1 + DB-2 + DB-3 + CD-1 + CD-2 + CD-3 + MR-1 tests`.
4. `feat(simulator): real-revm LocalSimulator pipeline + ProfitSource::RevmComputed flip + USDC proxy + impl fixtures + WETH9/V2/V3 fixtures + T-USDC-1 + SR-1..SR-4 tests`.
5. `chore(simulator): mark ProfitSource::HeuristicPassthrough deprecated (variant retained for rkyv compat)`.
6. (optional) `chore(batch-p4-c): pick up fmt + Cargo.lock drift at batch close`.

Targeted `cargo test -p` per code commit; full workspace gates ONLY at batch close + `auto_check.md` tail-summary refresh.

### DP-C13 — `StrictMissingDb` wrapper (per Rev #6)

```rust
// crates/simulator/src/strict_db.rs

use revm::db::{CacheDB, EmptyDB};
use revm::primitives::{AccountInfo, Address, B256, Bytecode, U256};
use revm::Database;

#[derive(Debug, thiserror::Error)]
pub enum StrictMissingError {
    #[error("revm read for unpopulated account {0:?}")]
    MissingAccount(Address),
    #[error("revm read for unpopulated storage slot {addr:?}[{slot}]")]
    MissingStorage { addr: Address, slot: U256 },
    #[error("revm read for unpopulated block_hash[{number}]")]
    MissingBlockHash { number: u64 },
    #[error("inner CacheDB infallible boundary violation: {0}")]
    InnerInfallible(String),
}

pub struct StrictMissingDb {
    inner: CacheDB<EmptyDB>,
}

impl StrictMissingDb {
    pub fn new(inner: CacheDB<EmptyDB>) -> Self { ... }

    /// Mutating insert helpers mirror CacheDB's surface.
    pub fn insert_account_info(&mut self, addr: Address, info: AccountInfo) { ... }
    pub fn insert_account_storage(&mut self, addr: Address, slot: U256, value: U256) -> Result<(), StrictMissingError> { ... }
    pub fn insert_contract(&mut self, code: Bytecode) { ... }
}

impl Database for StrictMissingDb {
    type Error = StrictMissingError;

    fn basic(&mut self, addr: Address) -> Result<Option<AccountInfo>, Self::Error> {
        // Look in inner.accounts; if not present and not just default-empty,
        // return MissingAccount. Distinguishing "explicitly empty" from
        // "never set" requires tracking populated keys separately.
        ...
    }
    fn storage(&mut self, addr: Address, slot: U256) -> Result<U256, Self::Error> {
        // Look in inner.accounts[addr].storage; if slot not present,
        // return MissingStorage. NEVER default to zero.
        ...
    }
    fn code_by_hash(&mut self, hash: B256) -> Result<Bytecode, Self::Error> {
        // Look in inner.contracts; if not present, MissingAccount via the
        // address that produced this hash (tracked separately).
        ...
    }
    fn block_hash(&mut self, number: u64) -> Result<B256, Self::Error> {
        // P4-C does not depend on BLOCKHASH opcode for swap exec; if revm
        // requests it, return MissingBlockHash and let exec revert.
        ...
    }
}
```

To distinguish "explicitly populated as empty/zero" from "never populated", `StrictMissingDb` keeps two `HashSet`s alongside the inner `CacheDB`: `populated_accounts: HashSet<Address>` and `populated_storage: HashSet<(Address, U256)>`. `insert_account_info` and `insert_account_storage` add to the respective set; `Database::basic` and `Database::storage` check the set BEFORE returning the inner value (which CacheDB would default to zero).

DB-3 test:
```rust
let inner = CacheDB::new(EmptyDB::default());
let mut db = StrictMissingDb::new(inner);
let addr = Address::from([0xab; 20]);
let slot = U256::from(42);
// Read without insert → MissingAccount.
let err = db.basic(addr).expect_err("must be missing");
assert!(matches!(err, StrictMissingError::MissingAccount(_)));
// Insert account but no storage slot → reading slot → MissingStorage.
db.insert_account_info(addr, AccountInfo { balance: U256::ZERO, nonce: 0, code_hash: KECCAK_EMPTY, code: None });
let err = db.storage(addr, slot).expect_err("must be missing storage");
assert!(matches!(err, StrictMissingError::MissingStorage { .. }));
// Insert slot → reads succeed.
db.insert_account_storage(addr, slot, U256::from(7)).unwrap();
assert_eq!(db.storage(addr, slot).unwrap(), U256::from(7));
```

The simulator maps the resulting `EVMError::Database(StrictMissingError::...)` to `SimulationError::Setup` with the formatted detail.

### DP-C14 — Production-wiring honesty (per Rev #8)

P4-C is **fixture/test-only** at the simulator level. The `wire_phase4` driver chain in `crates/app` is NOT touched.

`LocalSimulator` ships two ways to load fixtures:

```rust
impl LocalSimulator {
    /// P3-E-style sync constructor. Caller must subsequently call
    /// `load_fixture` (test) OR `prefetch_for` (production) before
    /// `simulate`, else `simulate` returns SimulationError::Setup.
    pub fn new(cfg: SimConfig) -> Result<Self, SimulationError>;

    /// Test path. Loads pre-recorded inline `FetchedPoolState` +
    /// `FetchedAccount` fixtures into the engine's keyed map.
    pub fn load_fixture(
        &mut self,
        source_pool: FetchedPoolState,
        sink_pool: FetchedPoolState,
        weth: FetchedAccount,
        usdc: FetchedAccount,
    );

    /// Production path. Calls `fetcher.fetch_pool` + `fetcher.fetch_account`
    /// for the opportunity's source/sink + WETH9 + USDC at the
    /// opportunity's block_hash. Caller drives this from the runtime
    /// loop. P4-C ships the method but does NOT wire it into the
    /// runtime — that is P4-G `wire_phase5_pre_safety` scope.
    pub async fn prefetch_for(
        &mut self,
        fetcher: &dyn StateFetcher,
        opportunity: &OpportunityEvent,
    ) -> Result<(), FetchError>;

    /// Existing P3-E surface. Returns Setup error if no fixture was
    /// loaded for `risk_checked.opportunity.{source_pool, sink_pool}`
    /// at `risk_checked.opportunity.block_hash`. Public API byte-identical
    /// to P3-E except this Setup-error case.
    pub fn simulate(&self, risk_checked: &RiskCheckedOpportunity) -> Result<SimulationOutcome, SimulationError>;
}
```

The runtime path through `wire_phase4` continues to construct a `LocalSimulator::new(cfg)` with no fixtures loaded; `simulate` therefore returns `SimulationError::Setup`. This is a HARD FAILURE (typed error, `?`-bubbled into the simulator-driver consumer task in `wire_phase4`, which logs + drops the candidate). It is NOT a silent wrong answer.

`wire_phase4` is shadow-only per ADR-002 (Phase 3 closed with the explicit "no live submission" gate). No live action depends on `SimulationOutcome` until P4-G's `wire_phase5_pre_safety` integrates the relay-sim comparator + `prefetch_for` call into the driver loop. P4-C accepting this as the documented gap is the explicit choice.

SR-4 test asserts the no-fixture path returns `Err(SimulationError::Setup(_))` instead of any silent success.

## Q8 hardening verified at batch close

- **(a) No funded key / signing infra**: `crates/simulator` + new `state-fetcher/uniswap.rs` carry zero `Signer|Wallet|PrivateKey|secp256k1` matches.
- **(b) No `submit_bundle` wiring**: P4-C does not touch `BundleRelay`.
- **(c) No `live_send` toggle**: P4-C does not edit config schema.
- **(d) NO live tests in CI**: `dump_fixture.rs` is `examples/`, env-var-required, never run by CI. Recorded-fixture tests are pure CPU.
- **(e) Secret redaction**: zero `tracing::*` macros in `crates/simulator/src/` + new modules. The `dump_fixture` example reads `ARCHIVE_RPC_URL` from env and never logs it raw.
- **(f) Fail-closed**: missing fixture → typed `Setup` error; missing slot → `StrictMissingDb` typed `MissingStorage`/`MissingAccount`; deprecated `HeuristicPassthrough` retained but never emitted by P4-C code; `wire_phase4` no-fixture path is a typed error not silent zero (DP-C14).

## Forbidden delta (only NEW for P4-C; v0.2 reaffirmed)

- No edits to `crates/state` (P2-B-frozen). Layouts live in `crates/state-fetcher/src/uniswap.rs` (additive new module).
- No edits to `crates/types` (`MismatchCategory` is P4-D's job).
- No edits to `crates/risk` / `crates/opportunity` / `crates/execution` (frozen). `SimulationOutcome` shape stays byte-identical to P3-E (per Rev #7); execution-tests stay byte-identical.
- No edits to `crates/app` (`wire_phase4` not touched per Rev #8 / DP-C14).
- `crates/state-fetcher` API extended additively only (`fetch_account` + `FetchedAccount` + `signed_int_key` + `uniswap.rs` module). No removal of v0.2 P4-B surface.
- No revm version bump (workspace pin = 14 stays).
- No `BundleRelay` work (P4-E).
- No Sushiswap / second mempool feed (P4-F).
- No new ADR. No scope creep into `MismatchCategory` enum or relay-sim comparator (P4-D).
- All Phase 4 forbids carry: no `eth_sendBundle`, no funded key, no `live_send=true`, no `.claude/`/`AGENTS.md` staging, no destructive git, no force-push.

## Open questions for Codex (v0.3 — none open)

v0.2 questions Q-C2 / Q-C5 / Q-C8 / Q-C9 / Q-C10 all confirmed acceptable in Codex's second verdict ("mock-router approach is OK for this fixture/test-only P4-C boundary; recent finalized block is fine; additive `fetch_account` is acceptable without ADR; `wire_phase4` hard Setup gap is acceptable since P4-G owns production wiring; 18 tests is a good lean matrix"). v0.3's added T-USDC-1 brings the count to 19 (still inside the lean envelope per Codex).

v0.3 has no scope-level open questions. The USDC-proxy modeling (DP-C9a) is a fixture-completeness concern verified by T-USDC-1 at impl time, not a scope decision.

If APPROVED: execute the 5-6 commit ladder + batch-close evidence pack with `auto_check.md` tail-summary refresh + record verdict. If REVISION: revise → v0.4 + re-emit. If ADR/scope/freeze change required: HALT to user.
