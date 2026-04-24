# ADR-002: Ethereum Mainnet + DEX Arbitrage Thin Path

**Date:** 2026-04-24
**Status:** Accepted

## Context

The engine must target a specific chain, asset pair, venue set, and strategy type for the thin-path phases (P1–P3). Choosing too broad a scope risks incomplete integration; choosing too narrow a scope risks failing to validate the core strategy loop. Additionally, live-capital risk during early phases must be eliminated.

Options considered:
- Chain: Ethereum mainnet vs. L2s (Arbitrum, Base, etc.)
- Asset pair: WETH/USDC vs. broader basket
- Venues: Uniswap V2 only vs. V2+V3 vs. broader DEX set
- Strategy: DEX arbitrage vs. liquidations vs. sandwich
- Execution mode: live vs. shadow-only

## Decision

- **Chain:** Ethereum mainnet only.
- **Asset pair:** WETH/USDC only through Phase 3.
- **Venues:** Uniswap V2 + Uniswap V3 (0.05% fee tier only) through Phase 3.
- **Strategy:** DEX arbitrage only through Phase 3.
- **Execution mode:** Shadow-only (observe, simulate, log — no bundle submission) through the entirety of Phase 3.
- **Deferred to Phase 4:** Sushiswap (WETH/USDC only), additional V3 fee tiers, additional asset pairs.
- **Scope freeze:** All items not listed above are frozen until the Phase 4 partial unlock.

## Rationale

- Ethereum mainnet has the deepest Uniswap V2/V3 liquidity and the most available MEV infrastructure (Flashbots, bloXroute), making it the highest-signal environment for strategy validation.
- WETH/USDC is the highest-volume pair on both Uniswap V2 and V3, maximizing arbitrage opportunity density during the thin-path test window.
- Limiting to one fee tier (0.05%) reduces state complexity; the 0.3% and 1% tiers can be added in P4 once the pipeline is validated.
- DEX arbitrage has the most straightforward profit calculation (price delta minus gas), making it the best strategy for end-to-end pipeline validation.
- Shadow-only through P3 eliminates live capital risk while the pipeline is unproven.
- An explicit scope freeze prevents scope creep from delaying gate passage.

## Revisit Trigger

The thin path scope proves insufficient for strategy validation by Phase 3 exit (e.g., WETH/USDC arbitrage opportunities are too infrequent on the captured event stream to produce a statistically meaningful backtest result).

## Consequences

- The mempool listener, bundle builder, and simulation components must handle Uniswap V2 and V3 (0.05%) from P1; no other venue code is written until P4.
- No bundle submission logic is wired to a relay until P4 at the earliest.
- Sushiswap integration code must not be merged to main until the P4 unlock.
- Phase 3 exit review will check that the shadow log contains sufficient arbitrage signal to proceed.
