# Phase 4 Batch C — C1 / C2 Split Amendment

**Date:** 2026-05-09 KST
**Status:** Amendment to `2026-05-04-phase-4-batch-c-real-revm-execution.md` v0.3 (manual Codex APPROVED MEDIUM 2026-05-09 KST). Splits the originally-monolithic P4-C into two batches per user direction 2026-05-09 KST ("Option D-lite"). The v0.3 plan content stands; this amendment narrows P4-C1's scope to what shipped + defers the `ProfitSource::HeuristicPassthrough → RevmComputed` flip to a new P4-C2 batch.

## User direction (verbatim, 2026-05-09 KST)

> Option D-lite로 가십시오. 정확히는 **B의 stub commit은 하지 말고**, commits 1-3을 "P4-C infrastructure / P4-C1"로 정직하게 닫는 쪽입니다.
>
> - `LocalSimulator::simulate`를 placeholder fixture로 `RevmComputed`처럼 보이게 만드는 stub commit은 금지한다.
> - `ProfitSource::RevmComputed` flip은 실제 mainnet fixture + mock-router callback + USDC proxy transfer가 검증될 때까지 유지 보류한다.
> - 현재 commits 1-3은 가치 있는 구현이므로 버리지 말고, 짧은 docs amendment commit 하나로 P4-C를 `P4-C1 simulator/state-fetcher infrastructure`로 분리 기록하라.
> - 새 후속 batch는 `P4-C2 real fixture replay + ProfitSource flip`으로 잡는다. P4-D/E/F/G로 넘어가지 말라.
> - 추가 테스트를 억지로 만들지 말라. `#[ignore]` stub tests도 만들지 말라.
> - docs amendment에는 두 blocker를 명확히 남겨라: compiled mock-router bytecode 또는 solc path, archive RPC로 녹화한 V2/V3/WETH/USDC proxy+impl fixtures.
> - push는 commits 1-3 + amendment까지만. 태그는 만들지 말라.

## P4-C1 (this batch — closes here)

**Scope retroactively narrowed to**: simulator + state-fetcher INFRASTRUCTURE that the real-revm path will consume. No `ProfitSource` flip. No real-fixture e2e. No mock router. No T-USDC-1.

Commits shipped at HEAD `81f5995`:

1. **`9c5aee7` docs**: P4-C v0.3 execution note (the original full plan). Stays committed as the historical record of the intended end-state for the combined C1+C2 work.
2. **`65a4768` feat(state-fetcher)**: `UniswapV2Layout` + `UniswapV3Fee005Layout` + `compress_tick` (Solidity floor) + `signed_int_key` + `address_key` + `fetch_account` API additive trait extension + `FetchedAccount` shape. 6 new tests (TM-1, L-V2-1, L-V3-1, L-V3-2, S-K-3, S-K-4, F-A-1).
3. **`81f5995` feat(simulator)**: `StrictMissingDb` (Database+DatabaseCommit wrapper, populated_* HashSets, typed `MissingAccount`/`MissingStorage`) + `swap_calldata` (V2/V3 ABI builders + `uniswap_v2_get_amount_out` + UniV3 boundary helpers) + `cache_db_builder::build_prepared` (two-pool shared CacheDB). 4 new tests (DB-3 mandatory, CD-1 V2 calldata+formula, CD-2 V3 calldata, DB-1 build_prepared).

Workspace test count at P4-C1 close: **133 passed + 1 ignored** (123 P4-B baseline + 6 state-fetcher + 4 simulator = 10 new from P4-C1).

P4-C1 explicitly does NOT include:
- `ProfitSource::RevmComputed` flip (`LocalSimulator::simulate` body unchanged from P3-E; still emits `HeuristicPassthrough`).
- Mock router bytecode (callback receiver for V3).
- Recorded mainnet fixtures (V2 + V3 + WETH9 + USDC proxy + USDC impl).
- T-USDC-1 (USDC proxy transfer under StrictMissingDb).
- SR-1..4 (real-fixture e2e arb replay).
- `prefetch_for` async constructor on `LocalSimulator`.

Per user direction, no `#[ignore]` stub tests are added. The P4-C1 boundary is honest: framework code shipped, e2e proof deferred to P4-C2.

## P4-C2 (new follow-up batch — held until both blockers resolved)

**Scope**: complete the originally-planned P4-C work — real-revm pipeline + `ProfitSource::HeuristicPassthrough → RevmComputed` flip + recorded fixtures + T-USDC-1 + SR e2e tests + `prefetch_for`. Pre-impl Codex review NOT required (already approved as part of v0.3); P4-C2 reuses the v0.3 plan content for the deferred items.

### Blocker B1 — Mock-router bytecode

The V3 callback `uniswapV3SwapCallback(int256 amount0Delta, int256 amount1Delta, bytes data)` requires a callback contract that:
1. Decodes `data` as `abi.encode(address)` to recover the input token.
2. Picks whichever `amountNDelta` is positive (= what the router owes the pool).
3. Builds `IERC20(token).transfer(msg.sender, uint256(amountNDelta))` calldata.
4. CALLs the token; STOPs.

Two paths to obtain the bytecode:

**Path 1 — solc-compiled (recommended)**:

Solidity source (paste into Remix or `forge` / `solc`):
```solidity
// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;
interface IERC20 { function transfer(address, uint256) external returns (bool); }
contract MockV3CallbackRouter {
    function uniswapV3SwapCallback(int256 amount0Delta, int256 amount1Delta, bytes calldata data) external {
        address token = abi.decode(data, (address));
        uint256 amount = amount0Delta > 0 ? uint256(amount0Delta) : uint256(amount1Delta);
        require(IERC20(token).transfer(msg.sender, amount), "transfer failed");
    }
}
```

Compile with `solc --bin-runtime --optimize --optimize-runs=200` (or equivalent in Remix's compile output panel). Paste the runtime bytecode hex into `crates/simulator/src/mock_router.rs` as a `&'static [u8]` constant. Approximate size: 200-400 bytes.

**Path 2 — hand-assembled EVM**:

~80-120 byte hand-written sequence. Fits, but is fiddly (stack juggling, MSTORE for transfer calldata, CALL gas/value/in/out arg ordering). Not recommended unless solc unavailable.

### Blocker B2 — Recorded mainnet fixtures

Five fixtures, all at the same recently-finalized mainnet block hash:

| Fixture | Address | Source |
|---|---|---|
| `FIXTURE_V2_WETH_USDC_BLOCK_X` | `0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc` | `UniswapV2Layout` slots {0,6,7,8} via `eth_getStorageAt` |
| `FIXTURE_V3_WETH_USDC_005_BLOCK_X` | `0x88e6A0c2dDD26FEEb64F039a2c41296FcB3f5640` | `UniswapV3Fee005Layout` 3-phase resolver |
| `FIXTURE_WETH9_BLOCK_X` | `0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2` | code + balanceOf slots (slot 3 mapping; keys = router + V2 pool + V3 pool) |
| `FIXTURE_USDC_PROXY_BLOCK_X` | `0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48` | proxy code + EIP-1967 impl slot + admin slot + paused + blacklisted[router/pools] + balanceOf[router/pools] + totalSupply + impl-read modifier slots (EXACT slot list determined empirically by `RecordingDb<EmptyDB>` probe in `dump_fixture.rs` — DO NOT assume EIP-1967 layout per Codex 2026-05-09 v0.3 verdict) |
| `FIXTURE_USDC_IMPL_BLOCK_X` | (parsed from proxy's EIP-1967 slot at recording time) | FiatTokenV2_2 implementation code only (delegatecall semantics → no own-storage reads) |

Recording requires:
- An archive-mode RPC endpoint (Alchemy archive plan, Infura archive, QuickNode archive, or local archive Geth ~2 TB+ disk).
- `ARCHIVE_RPC_URL` env var set.
- Run `cargo run --example dump_fixture` (which P4-C2 ships) — emits Rust literal text → paste into `crates/simulator/src/fixtures.rs`.

Cost: a single recording run touches ~20-50 archive `eth_getStorageAt` + 5 `eth_getCode` calls. Within free-tier limits of most archive providers.

### P4-C2 ledger of work

When both blockers are unblocked, P4-C2 commits (in order):

1. `feat(simulator): add MockV3CallbackRouter compiled bytecode constant` — paste the solc-compiled runtime hex into `crates/simulator/src/mock_router.rs` + 1 unit test that the bytecode dispatches the callback selector.
2. `feat(simulator): add dump_fixture example tool + RecordingDb probe wrapper` — operator-only `examples/dump_fixture.rs` + a `RecordingDb<EmptyDB>` test wrapper that captures the actual slot read-set during a probe transfer.
3. `feat(simulator): add recorded V2/V3/WETH9/USDC fixtures` — inline `const`-byte data pasted from the operator's `dump_fixture` run.
4. `feat(simulator): T-USDC-1 USDC proxy transfer under StrictMissingDb` — direct test against the recorded USDC proxy + impl fixtures.
5. `feat(simulator): real-revm LocalSimulator::simulate + ProfitSource::RevmComputed flip + prefetch_for + SR e2e tests` — the actual flip; `LocalSimulator` rewritten; `wire_phase4` left untouched per DP-C14 (no-fixture path returns typed `Setup` error).
6. (optional) `chore(batch-p4-c2): pick up fmt + Cargo.lock drift at batch close`.

Per user 2026-05-09: no extra padding tests, no `#[ignore]` stubs. Lean matrix: T-USDC-1 + SR happy + SR no-fixture-Setup-error + (existing P4-C1 unit tests cover everything else).

### P4-C2 forbids (carry from P4-C v0.3)

- No edits to `crates/state` / `crates/types` / `crates/risk` / `crates/opportunity` / `crates/execution` / `crates/app`.
- No revm version bump.
- No `BundleRelay` / Sushiswap / second mempool feed.
- No new ADR.
- No live network tests in CI.
- No `live_send=true`, no funded key, no `eth_sendBundle`, no relay submission.
- No destructive git, no force-push.
- No `.claude/` / `AGENTS.md` staging.

## Phase 4 progress with this amendment

| Batch | Status | Tag |
|---|---|---|
| P4-A archive node | CLOSED `e2b6704` | — |
| P4-B state-fetcher | CLOSED `856a859` | — |
| **P4-C1 simulator/state-fetcher infrastructure** | **CLOSED at this commit** | — (no tag per user direction) |
| **P4-C2 real fixture replay + ProfitSource flip** | **HELD** (B1 + B2 blockers) | — |
| P4-D `MismatchCategory` + relay sim comparator | NOT STARTED | — |
| P4-E `BundleRelay` trait + Flashbots/bloXroute adapters | NOT STARTED | — |
| P4-F Sushiswap WETH/USDC + external mempool feed | NOT STARTED | — |
| P4-G final wiring + DoD audit + `phase-4-complete` tag | NOT STARTED | — |

Per user direction 2026-05-09: do NOT proceed to P4-D until P4-C2 closes. Phase 4's ADR-001 line 43 revisit-trigger conditions remain incomplete (the `RevmComputed` flip is the load-bearing piece) until P4-C2 ships.
