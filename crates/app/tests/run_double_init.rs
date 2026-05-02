//! A-3 boundary: `wire()` with `init_observability: true` called twice
//! in the same test (single-binary so the OnceLock guard fires) — the
//! second call must return
//! `Err(AppError::Observability(ObservabilityError::AlreadyInitialized))`.
//!
//! Verifies error propagation through the wiring layer's first step.

mod common;

use rust_lmax_mev_app::{wire, AppError, WireOptions};
use rust_lmax_mev_observability::ObservabilityError;

#[test]
fn run_returns_error_on_double_observability_init() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = common::make_config(dir.path());

    let first = wire(
        &cfg,
        WireOptions {
            init_observability: true,
        },
    )
    .expect("first wire must succeed");

    let err = wire(
        &cfg,
        WireOptions {
            init_observability: true,
        },
    )
    .expect_err("second wire must return AlreadyInitialized");
    assert!(
        matches!(
            err,
            AppError::Observability(ObservabilityError::AlreadyInitialized)
        ),
        "expected Observability(AlreadyInitialized), got {err:?}"
    );

    first
        .shutdown()
        .expect("first handle must shut down cleanly");
}
