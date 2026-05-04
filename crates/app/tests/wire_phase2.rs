//! Phase 2 P2-D wiring tests for `wire_phase2` per the approved Batch D
//! execution note (`docs/superpowers/plans/2026-05-04-phase-2-batch-d-
//! app-wiring-execution.md`).
//!
//! - **D-1 failure** asserts `wire_phase2` returns
//!   `Err(AppError::Node(_))` (or `AppError::Io(_)`) within a bounded
//!   timeout when `geth_http_url` is unparseable.
//! - **D-2 boundary** is a compile-time assertion that the
//!   `From<NodeError>` and `From<StateError>` plumbing on `AppError`
//!   exists.
//!
//! Both tests avoid live-node dependencies — `NodeProvider::connect`
//! only runs URL parse, so a malformed URL surfaces synchronously
//! without dialing any network.

mod common;

use std::time::Duration;

use alloy_primitives::Address;
use rust_lmax_mev_app::{wire_phase2, AppError, WireOptions};
use rust_lmax_mev_node::NodeError;
use rust_lmax_mev_state::StateError;

/// D-1 failure: bogus `geth_http_url` (no scheme) → `wire_phase2` returns
/// `Err(AppError::Node(_))` within `Duration::from_secs(5)`. Bounds the
/// future via `tokio::time::timeout` so a hang on the connect path
/// surfaces as a test failure, not an infinite test run.
#[tokio::test(flavor = "multi_thread")]
async fn wire_phase2_returns_error_for_bogus_geth_url() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = common::make_config(dir.path());
    // `parse_http_url` rejects URLs without a scheme — surfaces as
    // NodeError::Transport, which `wire_phase2` propagates via
    // `AppError::Node(#[from] NodeError)`.
    config.node.geth_http_url = "not-a-url".to_string();

    let result = tokio::time::timeout(
        Duration::from_secs(5),
        wire_phase2(
            &config,
            WireOptions {
                init_observability: false,
            },
        ),
    )
    .await
    .expect("wire_phase2 must complete within 5s for an unparseable URL");

    match result {
        Err(AppError::Node(_)) | Err(AppError::Io(_)) => {}
        other => panic!("expected AppError::Node or AppError::Io, got {other:?}"),
    }
}

/// D-2 boundary: compile-time assertion that `AppError` plumbs
/// `NodeError` and `StateError` via `#[from]`. The `let _: AppError =
/// X.into();` lines fail to compile if the `From` impls are missing.
#[test]
fn app_error_from_impls_compile() {
    let _: AppError = NodeError::Closed.into();
    let _: AppError = StateError::UnknownPool(Address::ZERO).into();
}
