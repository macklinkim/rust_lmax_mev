# Phase 3 Detail Checklist

Date: 2026-04-24  
Status: Draft v0.1  
Purpose: Execution-enablement checklist for Phase 3. This phase must remain `shadow-only` and must not expand scope beyond the approved thin path.

## 1. Phase Identity

- [ ] Phase 3 is explicitly defined as `execution enablement`, not `production trading`.
- [ ] Default mode is `shadow-only`.
- [ ] `live_send=false` is documented as a mandatory guardrail, not a convenience flag.
- [ ] Scope remains unchanged from earlier phases:
  - [ ] `Ethereum mainnet`
  - [ ] `WETH/USDC`
  - [ ] `Uniswap V2 + Uniswap V3 0.05%`
  - [ ] `DEX Arbitrage only`
- [ ] Out of scope is written down explicitly:
  - [ ] `Backrun`
  - [ ] `Liquidation`
  - [ ] `Sushiswap`
  - [ ] `Curve`
  - [ ] `Balancer`
  - [ ] any production capital deployment

## 2. Boundary Types and Ownership

- [ ] `OpportunityEvent` remains a detection artifact from Phase 2.
- [ ] `ExecutionCandidate` is introduced as a separate type.
- [ ] `ExecutionCandidate` includes at minimum:
  - [ ] `candidate_id`
  - [ ] `arb_path`
  - [ ] `input_amount`
  - [ ] `expected_output`
  - [ ] `expected_profit_after_gas`
  - [ ] `gas_estimate`
  - [ ] `target_block`
  - [ ] `relay_targets`
  - [ ] `candidate_age_ms`
  - [ ] `abort_reason`
- [ ] `RiskDecision` or equivalent type exists and separates `allow` from `reject(reason)`.
- [ ] `BundleArtifact` or equivalent type exists for post-simulation bundle output.
- [ ] Ownership is closed:
  - [ ] opportunity engine creates `OpportunityEvent`
  - [ ] qualification/risk layer creates `ExecutionCandidate`
  - [ ] simulation layer creates sim results
  - [ ] bundle builder creates bundle artifact
  - [ ] relay adapter performs shadow submission only

## 3. Pipeline Definition

- [ ] The Phase 3 path is frozen as:
  - [ ] `Opportunity -> Risk -> Local Sim -> Relay Sim -> Bundle Build -> Shadow Submit`
- [ ] Each stage documents:
  - [ ] input type
  - [ ] output type
  - [ ] reject/abort conditions
  - [ ] observability fields
- [ ] Candidate invalidation rules are explicit for:
  - [ ] pending tx replacement
  - [ ] block reorg
  - [ ] target block expiry
  - [ ] stale pool state

## 4. Simulation Policy

- [ ] `local pre-sim` is mandatory.
- [ ] `relay sim` is mandatory.
- [ ] `simulation mismatch => abort` is written as a strict rule.
- [ ] Mismatch categories are defined:
  - [ ] profitability mismatch
  - [ ] gas mismatch
  - [ ] revert mismatch
  - [ ] state dependency mismatch
  - [ ] bundle outcome mismatch
- [ ] `unknown revert reason => abort` is explicit.
- [ ] No candidate may reach shadow submit unless both local sim and relay sim pass.

## 5. Freshness and Expiry Rules

- [ ] Maximum acceptable state staleness is defined in `ms` or `blocks`.
- [ ] Maximum candidate age is defined.
- [ ] Maximum target block distance is defined.
- [ ] A candidate is dropped when:
  - [ ] target block has passed
  - [ ] source opportunity is invalidated
  - [ ] pool state changed beyond allowed freshness window
  - [ ] replacement tx or reorg breaks assumptions

## 6. Risk Engine Rules

- [ ] Risk engine is framed as a hard gate, not an optimizer.
- [ ] The rules include at minimum:
  - [ ] allowed pair = `WETH/USDC` only
  - [ ] allowed protocol = `Uniswap V2/V3` only
  - [ ] max input size
  - [ ] min profit after gas threshold
  - [ ] stale state rejection
  - [ ] sim mismatch rejection
  - [ ] missing pool state rejection
  - [ ] unknown revert rejection
- [ ] Risk Budget Baseline is referenced directly:
  - [ ] `shadow capital = 0`
  - [ ] `sim mismatch tolerance = 0`
  - [ ] `live send disabled`

## 7. Gas and Bundle Policy

- [ ] Phase 3 uses a conservative gas policy only.
- [ ] The document defines:
  - [ ] `max_fee_per_gas`
  - [ ] `max_priority_fee_per_gas`
  - [ ] `profit_after_gas_threshold`
- [ ] `gas estimate uncertainty => abort` is explicit.
- [ ] Bundle semantics are fixed:
  - [ ] single strategy bundle or multi-tx bundle policy
  - [ ] number of target blocks
  - [ ] replacement/cancel behavior
  - [ ] relay fanout behavior
- [ ] Direct builder integration is deferred.

## 8. Relay Strategy

- [ ] Initial shadow relay targets are fixed to:
  - [ ] `Flashbots`
  - [ ] `bloXroute`
- [ ] Relay adapter output is normalized into internal result types.
- [ ] The document captures relay responses for:
  - [ ] accepted
  - [ ] rejected
  - [ ] simulation failed
  - [ ] timeout
- [ ] No real submission is allowed in Phase 3.

## 9. Replay and Validation

- [ ] Phase 3 reuses Phase 2 replay assumptions where applicable.
- [ ] Determinism goal is documented:
  - [ ] same candidate input -> same sim decision
- [ ] Replay or fixture cases include:
  - [ ] sim pass
  - [ ] sim fail
  - [ ] sim mismatch
  - [ ] stale candidate
  - [ ] target block expired
- [ ] Safety Gate for Phase 3 exit is defined before implementation starts.

## 10. Observability

- [ ] Metrics are defined for the execution path:
  - [ ] `candidate_generated_total`
  - [ ] `candidate_rejected_total{reason}`
  - [ ] `simulation_pass_total`
  - [ ] `simulation_fail_total`
  - [ ] `sim_mismatch_total`
  - [ ] `bundle_built_total`
  - [ ] `relay_submission_shadow_total`
  - [ ] `target_block_missed_total`
- [ ] Latency metrics are defined:
  - [ ] `candidate_age_ms`
  - [ ] `local_sim_latency_ns`
  - [ ] `relay_sim_latency_ns`
  - [ ] `bundle_build_latency_ns`
- [ ] Structured logs/traces include:
  - [ ] `correlation_id`
  - [ ] `candidate_id`
  - [ ] `target_block`
  - [ ] `relay_target`
  - [ ] `abort_reason`
- [ ] Global kill switch / execution disable flag is documented.

## 11. Definition of Done

- [ ] A full `shadow-only` path works end to end.
- [ ] `OpportunityEvent -> ExecutionCandidate -> sim -> bundle -> shadow submit` is demonstrable.
- [ ] Both local sim and relay sim must pass before shadow submit.
- [ ] `sim mismatch tolerance = 0` is enforced.
- [ ] `live send` remains disabled.
- [ ] Rejection reasons are observable and categorized.
- [ ] Thin path scope remains unchanged.
- [ ] No real capital is used.

## 12. Anti-Patterns

- [ ] "Let's send one real bundle just to test" is explicitly forbidden.
- [ ] "Let's add Backrun while we are here" is explicitly forbidden.
- [ ] "Let's add Sushiswap in the same phase" is explicitly forbidden.
- [ ] "Let's optimize gas bidding before the shadow path is stable" is explicitly forbidden.
- [ ] "Let's allow sim mismatch and only log it" is explicitly forbidden.
- [ ] "Let's postpone the risk engine until later" is explicitly forbidden.

## 13. Core Principle

`Phase 3 is not a production trading phase. It is the phase where candidate qualification, deterministic simulation, strict abort policy, and shadow-only relay flow are made reliable enough to support later limited production work.`
