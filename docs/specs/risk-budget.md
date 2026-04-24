# Risk Budget Baseline

## Cap Evaluation Rule

Effective cap = min(absolute cap, relative cap). Absolute caps always apply, even when strategy capital is undefined or unavailable.

## Parameters

| Parameter | Absolute Cap | Relative Cap |
|-----------|-------------|--------------|
| Initial canary capital | 0.5 ETH | - |
| Per-bundle max notional | 0.1 ETH | Strategy capital 1% |
| Daily realized loss cap | 0.05 ETH | Strategy capital 3% |
| Max gas spend / day | 0.03 ETH | - |
| Max concurrent live bundles | 1 | - |
| Max resubmits / opportunity | 2 | - |
| Sim mismatch tolerance | 0 | - |
| Shadow mode capital | 0 | - |
| `live_send` default | false (always) | - |

## Change Policy

All values are adjustable. Any change requires:

- Date of change
- Reason / rationale documented alongside the new value
