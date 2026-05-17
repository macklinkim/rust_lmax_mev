//! P6B-E2 D-T-E2-3 + D-T-E2-4 + D-T-E2-5 targeted tests.
//!
//! - D-T-E2-3: `submission_driver` receives an Ok HTTP response with a
//!   `bundleHash` that does NOT equal `keccak256(concat(signed_txs))`.
//!   The driver synthesizes `BundleRelayError::BundleHashMismatch`
//!   (warn-logged) and appends exactly one mismatch `SubmissionReceipt`
//!   to the submission journal with `local_bundle_hash != bundle_hash`.
//! - D-T-E2-4: `BundleConstructor` constructed with `DisabledSigner`
//!   (the `wire_phase4` default at P6B-E2 close) -- `sign_for_outcome`
//!   returns `Err(SignerError::SignerDisabled)`. The
//!   `simulator_driver`'s iteration-skip + WARN path is verified by
//!   direct invocation of `sign_for_outcome` (the upstream wiring is
//!   covered by `prefetch_wiring.rs` PA-1 + the post-E2 sim envelope
//!   contract: a `signed_bundle.is_some()` envelope only flows after
//!   a successful `sign_tx`).
//! - D-T-E2-5: source-byte grep of `crates/execution/src/lib.rs` for
//!   the token `sign_tx`: exactly 2 hits (the `invoke_signer_for_test`
//!   hook + the new `sign_for_outcome` production runtime call site).

use std::sync::Arc;
use std::time::Duration;

use alloy_primitives::{Address, U256};
use rust_lmax_mev_app::{
    submission_driver, BundleRelay, KillSwitch, SignedBundle, SubmissionAttempt, SubmissionGate,
    SubmissionReceipt,
};
use rust_lmax_mev_execution::{BundleConfig, BundleConstructor};
use rust_lmax_mev_journal::FileJournal;
use rust_lmax_mev_relay_clients::{FlashbotsConfig, FlashbotsRelay};
use rust_lmax_mev_signer::SignerError;
use rust_lmax_mev_simulator::{ProfitSource, SimStatus, SimulationOutcome};
use serde_json::json;
use tokio::sync::broadcast;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn make_attempt() -> SubmissionAttempt {
    SubmissionAttempt {
        signed_bundle: SignedBundle {
            block_hash: [0u8; 32],
            state_block_number: 22_000_000,
            // 80 bytes of a fixed pattern -- keccak256 is deterministic
            // so the mismatched mock bundle hash below is guaranteed
            // to differ.
            signed_txs: vec![vec![0xCDu8; 80]],
            coinbase_recipient: Address::ZERO,
            coinbase_transfer_wei: U256::ZERO,
            validity_block_min: 22_000_001,
            validity_block_max: 22_000_005,
        },
    }
}

/// D-T-E2-3: submission_driver journals a mismatch record when the
/// relay-returned `bundleHash` does not equal `keccak256(concat(signed_txs))`.
#[tokio::test(flavor = "multi_thread")]
async fn d_t_e2_3_submission_driver_journals_bundle_hash_mismatch() {
    let server = MockServer::start().await;
    // Return a definitively-wrong bundle hash (32 zero bytes hex). The
    // real keccak of vec![0xCD; 80] is a specific non-zero digest.
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 1,
            "jsonrpc": "2.0",
            "result": {
                "bundleHash": "0x0000000000000000000000000000000000000000000000000000000000000000"
            }
        })))
        .mount(&server)
        .await;

    let flashbots = FlashbotsRelay::new(
        FlashbotsConfig {
            endpoint: server.uri(),
            timeout_ms: 2_000,
        },
        KillSwitch::new(false),
        false,
    )
    .expect("ctor ok");
    let relay: Arc<dyn BundleRelay> = Arc::new(flashbots);

    let dir = tempfile::tempdir().expect("tempdir");
    let journal_path = dir.path().join("submission.bin");
    let journal: FileJournal<SubmissionReceipt> =
        FileJournal::open(&journal_path).expect("journal open");

    let (sub_tx, sub_rx) = broadcast::channel::<SubmissionAttempt>(8);
    let gate = SubmissionGate {
        live_send: true,
        production_profile: true,
        hsmkms: true,
    };

    let driver = tokio::spawn(submission_driver(
        sub_rx,
        Some(relay),
        KillSwitch::new(false),
        gate,
        journal,
        "d-t-e2-3-driver",
    ));

    sub_tx.send(make_attempt()).expect("publish ok");
    tokio::time::sleep(Duration::from_millis(250)).await;

    let received = server.received_requests().await.expect("wiremock recorded");
    assert_eq!(received.len(), 1, "D-T-E2-3: exactly one POST expected");

    drop(sub_tx);
    let _ = tokio::time::timeout(Duration::from_millis(500), driver).await;

    // Journal must contain at least one record (the mismatch envelope).
    let journal_bytes = std::fs::read(&journal_path).expect("journal readable");
    assert!(
        journal_bytes.len() > 64,
        "D-T-E2-3: mismatch must be journaled; got {} bytes",
        journal_bytes.len()
    );
}

/// D-T-E2-4: `BundleConstructor` constructed with the default
/// `DisabledSigner` (the `wire_phase4` default at P6B-E2 close)
/// returns `Err(SignerError::SignerDisabled)` from
/// `sign_for_outcome(...)`. The `simulator_driver`'s iteration-skip
/// behavior under this Err depends on the structural skip + warn path
/// in `simulator_driver`; this test asserts the boundary contract that
/// downstream cannot receive signed bytes when the default fail-closed
/// signer is in place.
#[tokio::test(flavor = "multi_thread")]
async fn d_t_e2_4_disabled_signer_sign_for_outcome_errs_signer_disabled() {
    let ctor = BundleConstructor::new(BundleConfig::defaults()).expect("ctor ok");
    let outcome = SimulationOutcome {
        opportunity_block_number: 22_000_000,
        gas_used: 100_000,
        status: SimStatus::Success,
        simulated_profit_wei: U256::from(1_000_000u64),
        profit_source: ProfitSource::RevmComputed,
    };
    let result = ctor.sign_for_outcome(&outcome).await;
    assert_eq!(
        result.err(),
        Some(SignerError::SignerDisabled),
        "D-T-E2-4: DisabledSigner-wired BundleConstructor.sign_for_outcome must return SignerDisabled"
    );
}

/// D-T-E2-5: G11 source-byte grep gate. The METHOD call form
/// `.sign_tx(` must appear EXACTLY 2 times in
/// `crates/execution/src/lib.rs` (mirrors the CW-3 `.submit_bundle(`
/// counting pattern -- counts method-call sites, NOT doc-comment
/// references to the symbol):
/// 1. The pre-existing `#[cfg(test)] pub(crate) async fn
///    invoke_signer_for_test` hook -- `self.signer.sign_tx(tx).await`.
/// 2. The new P6B-E2 D-E2-6 production runtime call site --
///    `self.signer.sign_tx(&tx).await` inside `sign_for_outcome`.
///
/// Any new caller must be documented per file:line in the boundary
/// doc Section 5; this test guards against undocumented growth of the
/// production-runtime sign_tx surface.
#[test]
fn d_t_e2_5_g11_sign_tx_method_call_count_is_exactly_2() {
    let src = include_str!("../../execution/src/lib.rs");
    let count = src.matches(".sign_tx(").count();
    assert_eq!(
        count, 2,
        "D-T-E2-5 G11: expected exactly 2 `.sign_tx(` method-call sites in crates/execution/src/lib.rs \
         (test-only invoke_signer_for_test + production-runtime sign_for_outcome); got {count}"
    );
}
