# Thin Path Scope Definition

## Token Pair

WETH/USDC only.

## Protocols

- Uniswap V2 (constant product AMM)
- Uniswap V3 (0.05% fee tier only)

## Strategy

DEX Arbitrage only.

## Chain

Ethereum mainnet.

## Mode

Shadow-only through Phase 3. No live capital. `live_send = false` enforced across all profiles.

## Scope Freeze

This scope is frozen and unchanged until the Phase 4 partial unlock.

## Phase 4 Unlock

The only addition permitted at Phase 4 is Sushiswap WETH/USDC.

The following remain locked through Phase 4 and beyond until explicitly unlocked by a later phase gate:

- Curve
- Balancer
- Backrun strategies
- Liquidation strategies
- New chains (any chain other than Ethereum mainnet)

## Phase 4 Still Locked

Non-WETH/USDC pairs on any protocol remain locked regardless of phase.
