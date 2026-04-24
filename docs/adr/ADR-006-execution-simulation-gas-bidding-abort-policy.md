# ADR-006: Execution Simulation / Gas Bidding / Abort Policy

**Date:** 2026-04-24
**Status:** Accepted

## Context

Before a MEV bundle is submitted to a relay, the engine must simulate its outcome to verify profitability and avoid on-chain reverts. Several design questions arise:

1. Is local simulation (via `revm`) sufficient, or is relay simulation (`eth_callBundle`) also required?
2. What is the tolerance for simulation mismatches between local and relay results?
3. How are mismatch categories classified?
4. How is gas bid calculated?
5. When are more sophisticated bidding strategies permitted?

## Decision

### Simulation pipeline
Both simulation steps are **mandatory** for every bundle before submission:

1. **Local pre-simulation** (`revm` against the current state snapshot): fast, no network round-trip.
2. **Relay simulation** (`eth_callBundle` against the target relay): authoritative, verifies builder-side state.

### Abort policy
**Any mismatch between local simulation and relay simulation = abort.** Tolerance = zero.

A bundle is aborted (not submitted) if the relay simulation result differs from the local simulation result in any of the following categories:

| Category | Description |
|---|---|
| `Profitability` | Relay net profit deviates from local net profit by any amount |
| `Gas` | Relay gas used deviates from local gas used by any amount |
| `Revert` | Local simulation succeeds but relay simulation reverts (or vice versa) |
| `StateDependency` | Relay state slot values differ from local snapshot values |
| `BundleOutcome` | Bundle inclusion order or coinbase transfer amount differs |
| `Unknown` | Any other relay simulation error or unexpected response |

All aborted bundles are logged to the journal with their mismatch category.

### Gas bidding
**Conservative fixed gas bidding only** through Phase 4:

- Bid = `(estimated_profit * fixed_bid_fraction)` where `fixed_bid_fraction` is a config parameter (default: 0.90).
- No dynamic adjustment, no EIP-1559 base fee model, no competitor bid inference.

Dynamic, adaptive, and ML-based gas bidding strategies are **deferred to Phase 5+**.

## Rationale

- Requiring both local and relay simulation catches state divergence before capital is at risk. Local simulation alone is insufficient because the relay may see different pending state.
- Zero tolerance for simulation mismatches is appropriate during phases where the simulation stack is unproven. A non-zero tolerance would mask bugs in `revm` state accuracy.
- Six explicit mismatch categories provide structured logging for post-mortem analysis and make it easy to identify systematic failure modes (e.g., all mismatches are `StateDependency` → state snapshot staleness issue).
- Fixed gas bidding is predictable and easy to reason about. Dynamic bidding introduces a feedback loop that is difficult to validate during thin-path phases.
- Deferring dynamic bidding to P5 ensures that the simulation and abort infrastructure is solid before adding bidding complexity.

## Revisit Trigger

`revm` simulation accuracy is insufficient for production use by Phase 5 exit (e.g., mismatch rate exceeds 0.1% of bundles on the Phase 3–4 shadow log, and the root cause is a `revm` limitation rather than a state snapshot issue).

## Consequences

- The bundle pipeline must include a `RelaySimulator` component that calls `eth_callBundle` synchronously before submission.
- A `MismatchCategory` enum with the six variants above must be defined in the core types crate.
- All aborted bundles must be journaled with their mismatch category; this is required for Phase 3 gate review.
- The `fixed_bid_fraction` parameter must be in the engine config with a documented valid range (0.0–1.0).
- Dynamic bidding code must not be merged to main before the Phase 5 unlock.
