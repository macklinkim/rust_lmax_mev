//! Phase 4 P4-D heuristic-vs-revm reconciliation diagnostic.
//!
//! Compares two profit estimates carried through the pipeline:
//! - `heuristic_wei` — upstream `OpportunityEvent::expected_profit_wei`
//!   (P3-C linear approximation; intentionally cheap).
//! - `revm_wei` — `SimulationOutcome::simulated_profit_wei` (P4-C2
//!   real-revm `RevmComputed` measurement).
//!
//! These will diverge in normal operation. P4-D ships this as a
//! **diagnostic only** — the reconciler does NOT trigger an abort.
//! Callers map the returned `ReconciliationLabel` to a counter
//! increment (the `simulator_reconciliation_total{outcome}` metric is
//! a documented future contract for the P4-E or P4-G call site, NOT
//! shipped in P4-D). Threshold-based aborting is Phase 5+ work — it
//! depends on real-arb agreement-band data that only emerges from
//! shadow-running with the comparator wired.
//!
//! This module is **pure**: no I/O, no metrics emission, no `unsafe`,
//! no `tracing`. The function is `#[must_use]`; the call site decides
//! what to do with the report + label.

use crate::SimStatus;
use alloy_primitives::U256;

/// Detailed diagnostic record returned alongside the canonical label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconciliationReport {
    /// `(revm - heuristic) * 10_000 / heuristic`, signed, saturating.
    /// `None` iff `heuristic == 0` (avoids div-by-zero; the caller
    /// can read this case from `sign_flip` and the raw inputs).
    pub ratio_bps: Option<i64>,
    /// True iff exactly one of `heuristic_wei` and `revm_wei` is zero.
    /// Useful pipeline-health signal (heuristic said yes but revm
    /// said no, or vice versa).
    pub sign_flip: bool,
    /// True iff `status` is non-Success OR (`revm == 0` while
    /// `heuristic > 0`). Captures "we expected profit, the real
    /// simulation said otherwise" in a single field.
    pub revm_unprofitable_after_heuristic_pass: bool,
}

/// Canonical label callers use to drive the
/// `simulator_reconciliation_total{outcome}` counter (when wired).
/// Deterministic — no implicit threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconciliationLabel {
    /// `status == Success` AND `revm_wei == heuristic_wei`.
    Equal,
    /// `status == Success` AND values differ (any direction, any
    /// magnitude). Caller decides whether to alarm based on its own
    /// (deferred to Phase 5+) threshold policy.
    Divergent,
    /// `status != Success` OR (`revm == 0` while `heuristic > 0`).
    /// The revm pipeline did not confirm a profitable outcome that
    /// the upstream heuristic predicted.
    RevmUnprofitable,
}

/// Pure reconciliation function. Returns both a structured report and
/// the canonical label. NO metrics emission; NO I/O; NO tracing.
///
/// Caller maps `label` to a counter increment at the call site (which
/// lives downstream of `LocalSimulator::simulate` and lands in P4-E
/// or P4-G alongside the relay-sim wiring per the P4-D execution
/// note v0.4 §DP-D11).
#[must_use]
pub fn reconcile(
    heuristic_wei: U256,
    revm_wei: U256,
    status: &SimStatus,
) -> (ReconciliationReport, ReconciliationLabel) {
    let success = matches!(status, SimStatus::Success);
    let heuristic_zero = heuristic_wei.is_zero();
    let revm_zero = revm_wei.is_zero();

    let sign_flip = heuristic_zero ^ revm_zero;

    let revm_unprofitable_after_heuristic_pass = !success || (revm_zero && !heuristic_zero);

    let ratio_bps = if heuristic_zero {
        None
    } else {
        // (revm - heuristic) * 10_000 / heuristic, saturating to i64.
        // Magnitude bounded by (max(U256) / heuristic) * 10_000;
        // for any non-trivial heuristic this fits in i64 — but use
        // saturating arithmetic at every step to be safe.
        let h_u128: u128 = u256_to_u128_sat(heuristic_wei);
        let r_u128: u128 = u256_to_u128_sat(revm_wei);
        // Compute |delta| as u128 then re-sign as i128 then sat to i64.
        let (abs_delta, positive) = if r_u128 >= h_u128 {
            (r_u128 - h_u128, true)
        } else {
            (h_u128 - r_u128, false)
        };
        let scaled = abs_delta.saturating_mul(10_000);
        let bps_unsigned = scaled / h_u128.max(1);
        let signed: i128 = if positive {
            bps_unsigned.min(i128::MAX as u128) as i128
        } else {
            -(bps_unsigned.min(i128::MAX as u128) as i128)
        };
        Some(signed.clamp(i64::MIN as i128, i64::MAX as i128) as i64)
    };

    let report = ReconciliationReport {
        ratio_bps,
        sign_flip,
        revm_unprofitable_after_heuristic_pass,
    };

    let label = if !success || (revm_zero && !heuristic_zero) {
        ReconciliationLabel::RevmUnprofitable
    } else if heuristic_wei == revm_wei {
        ReconciliationLabel::Equal
    } else {
        ReconciliationLabel::Divergent
    };

    (report, label)
}

/// Saturating U256 → u128 conversion. Values exceeding u128::MAX
/// saturate. The caller's downstream `* 10_000` step also saturates.
fn u256_to_u128_sat(v: U256) -> u128 {
    let bytes: [u8; 32] = v.to_be_bytes();
    // High 128 bits non-zero → saturate.
    if bytes[..16].iter().any(|&b| b != 0) {
        return u128::MAX;
    }
    let mut low = [0u8; 16];
    low.copy_from_slice(&bytes[16..]);
    u128::from_be_bytes(low)
}
