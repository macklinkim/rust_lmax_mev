# Execution Safety Policy

## Simulation Requirements

### revm Local Pre-Simulation

Mandatory for every candidate bundle before any relay submission. No bundle may proceed to relay without a passing local sim.

### Relay Simulation (eth_callBundle)

Mandatory. The relay simulation result must be compared against the local revm result before submission proceeds.

## Simulation Mismatch Policy

Mismatch between local sim and relay sim = abort. Tolerance = 0.

Mismatch categories:

1. Profitability
2. Gas
3. Revert
4. StateDependency
5. BundleOutcome
6. Unknown

Unknown revert must never be suppressed. Any unknown revert category = abort.

Gas estimate uncertainty = abort.

## live_send Default

`live_send = false` is the default across all configuration profiles: dev, test, and shadow. This must never be flipped to `true` in any profile until the Phase 6b Production Gate is passed.

## Funded Key / Prod Signer Ban

A funded private key or production signer is banned from use in any capacity until the Phase 6b Production Gate is formally passed. Violation is a critical safety failure.

## Gas Bidding Strategy

Conservative fixed gas bidding only, through Phase 4. Dynamic gas strategies and ML-based bidding are deferred to Phase 5 and later.

## Kill Switch

An execution disable flag must be present and operable via:

- Config file toggle
- Runtime toggle (no restart required)

The kill switch must halt all bundle submission immediately when activated.
