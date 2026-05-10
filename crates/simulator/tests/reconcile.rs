//! Phase 4 P4-D REC-1..6 tests for the heuristic-vs-revm reconciler.
//! Per execution note v0.4 §"Test matrix" and §"Deliverable D-2".

use alloy_primitives::U256;
use rust_lmax_mev_simulator::reconcile::{reconcile, ReconciliationLabel};
use rust_lmax_mev_simulator::SimStatus;

/// REC-1: heuristic 100, revm 110, Success → ratio +1000 bps,
/// no sign flip, not revm-unprofitable, label Divergent.
#[test]
fn rec_1_heuristic_below_revm_success() {
    let (report, label) = reconcile(U256::from(100u64), U256::from(110u64), &SimStatus::Success);
    assert_eq!(report.ratio_bps, Some(1_000));
    assert!(!report.sign_flip);
    assert!(!report.revm_unprofitable_after_heuristic_pass);
    assert_eq!(label, ReconciliationLabel::Divergent);
}

/// REC-2: heuristic 100, revm 0, Success → ratio -10_000 bps,
/// sign flip, revm-unprofitable, label RevmUnprofitable.
#[test]
fn rec_2_revm_zero_after_heuristic_pass() {
    let (report, label) = reconcile(U256::from(100u64), U256::ZERO, &SimStatus::Success);
    assert_eq!(report.ratio_bps, Some(-10_000));
    assert!(report.sign_flip);
    assert!(report.revm_unprofitable_after_heuristic_pass);
    assert_eq!(label, ReconciliationLabel::RevmUnprofitable);
}

/// REC-3: heuristic 0, revm 50, Success → ratio None (div-by-zero
/// guard), sign flip, NOT revm-unprofitable (revm passed even though
/// upstream heuristic predicted nothing), label Divergent.
#[test]
fn rec_3_heuristic_zero_revm_positive_success() {
    let (report, label) = reconcile(U256::ZERO, U256::from(50u64), &SimStatus::Success);
    assert_eq!(report.ratio_bps, None);
    assert!(report.sign_flip);
    assert!(!report.revm_unprofitable_after_heuristic_pass);
    assert_eq!(label, ReconciliationLabel::Divergent);
}

/// REC-4: heuristic 100, revm 100, Reverted → revm-unprofitable
/// regardless of numeric agreement, label RevmUnprofitable.
#[test]
fn rec_4_reverted_overrides_numeric_agreement() {
    let (report, label) = reconcile(
        U256::from(100u64),
        U256::from(100u64),
        &SimStatus::Reverted {
            reason_hex: "0x".into(),
        },
    );
    assert!(report.revm_unprofitable_after_heuristic_pass);
    assert_eq!(label, ReconciliationLabel::RevmUnprofitable);
}

/// REC-5: heuristic 100, revm 200, OutOfGas → revm-unprofitable,
/// label RevmUnprofitable (even though revm > heuristic numerically,
/// non-Success status dominates).
#[test]
fn rec_5_out_of_gas_overrides_positive_delta() {
    let (_report, label) = reconcile(U256::from(100u64), U256::from(200u64), &SimStatus::OutOfGas);
    assert_eq!(label, ReconciliationLabel::RevmUnprofitable);
}

/// REC-6: heuristic 100, revm 100, Success → ratio 0 bps, no sign
/// flip, not revm-unprofitable, label Equal. Covers the Equal branch.
#[test]
fn rec_6_exact_match_success() {
    let (report, label) = reconcile(U256::from(100u64), U256::from(100u64), &SimStatus::Success);
    assert_eq!(report.ratio_bps, Some(0));
    assert!(!report.sign_flip);
    assert!(!report.revm_unprofitable_after_heuristic_pass);
    assert_eq!(label, ReconciliationLabel::Equal);
}
