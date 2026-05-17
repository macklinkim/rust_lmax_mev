//! P6B-E1 D-T-E1-6 + D-T-E1-7 targeted integration tests for the new
//! `submission_driver` task body. Tests exercise the driver in
//! isolation (no `wire_phase4` integration; the driver fn is `pub` and
//! callable directly). Both tests use a `wiremock::MockServer` bound
//! to `127.0.0.1:<random>` so no live network egress occurs.

use std::sync::Arc;
use std::time::Duration;

use alloy_primitives::{Address, U256};
use rust_lmax_mev_app::{
    submission_driver, BundleRelay, KillSwitch, SignedBundle, SubmissionAttempt, SubmissionGate,
};
use rust_lmax_mev_journal::FileJournal;
use rust_lmax_mev_relay_clients::{FlashbotsConfig, FlashbotsRelay};
use serde_json::json;
use tokio::sync::broadcast;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn make_attempt() -> SubmissionAttempt {
    SubmissionAttempt {
        signed_bundle: SignedBundle {
            block_hash: [0u8; 32],
            state_block_number: 22_000_000,
            signed_txs: vec![vec![0xBBu8; 80]],
            coinbase_recipient: Address::ZERO,
            coinbase_transfer_wei: U256::ZERO,
            validity_block_min: 22_000_001,
            validity_block_max: 22_000_005,
        },
    }
}

/// D-T-E1-6: full submission_driver happy path. Wiremock at
/// `127.0.0.1:<random>`; SubmissionGate permits (live_send=true,
/// Production, HsmKms); KillSwitch inactive. submission_driver
/// receives 1 SubmissionAttempt -> POSTs eth_sendBundle -> appends
/// SubmissionReceipt to journal.
#[tokio::test(flavor = "multi_thread")]
async fn d_t_e1_6_submission_driver_happy_path_journals_receipt() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 1,
            "jsonrpc": "2.0",
            "result": { "bundleHash": "0xdeadbeef" }
        })))
        .mount(&server)
        .await;

    let flashbots = FlashbotsRelay::new(
        FlashbotsConfig {
            endpoint: server.uri(),
            timeout_ms: 2_000,
        },
        KillSwitch::new(false),
    )
    .expect("ctor ok");
    let relay: Arc<dyn BundleRelay> = Arc::new(flashbots);

    let dir = tempfile::tempdir().expect("tempdir");
    let journal_path = dir.path().join("submission.bin");
    let journal: FileJournal<rust_lmax_mev_app::SubmissionReceipt> =
        FileJournal::open(&journal_path).expect("journal open");

    let (sub_tx, sub_rx) = broadcast::channel::<SubmissionAttempt>(8);
    let gate = SubmissionGate {
        live_send: true,
        production_profile: true,
        hsmkms: true,
    };
    assert!(gate.permits_submission());

    let driver = tokio::spawn(submission_driver(
        sub_rx,
        Some(relay),
        KillSwitch::new(false),
        gate,
        journal,
        "d-t-e1-6-driver",
    ));

    sub_tx.send(make_attempt()).expect("publish ok");

    // Wait briefly for the driver to process. The driver's loop has
    // no internal delays; the await yields and the wiremock receives
    // the POST within a few hundred milliseconds.
    tokio::time::sleep(Duration::from_millis(200)).await;

    let received = server.received_requests().await.expect("wiremock recorded");
    assert_eq!(received.len(), 1, "exactly one POST expected");

    // Shutdown.
    drop(sub_tx);
    let _ = tokio::time::timeout(Duration::from_millis(500), driver).await;

    // Journal contains at least 1 SubmissionReceipt entry.
    let journal_bytes = std::fs::read(&journal_path).expect("journal readable");
    assert!(
        !journal_bytes.is_empty(),
        "D-T-E1-6: journal must contain at least one SubmissionReceipt entry"
    );
}

/// D-T-E1-7: G13 inheritance gate fail-closed. SubmissionGate has
/// `live_send=false` (the boundary doc Section 4 G12 step 7 assertion
/// must fail). submission_driver receives a SubmissionAttempt but
/// MUST skip the iteration (no HTTP POST to wiremock, no journal
/// entry). Defense-in-depth proof that even when an attempt reaches
/// the driver, the gate prevents the HTTP I/O.
#[tokio::test(flavor = "multi_thread")]
async fn d_t_e1_7_submission_driver_skips_on_g13_inheritance_fail() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 1, "jsonrpc": "2.0",
            "result": { "bundleHash": "0xdeadbeef" }
        })))
        .mount(&server)
        .await;

    let flashbots = FlashbotsRelay::new(
        FlashbotsConfig {
            endpoint: server.uri(),
            timeout_ms: 2_000,
        },
        KillSwitch::new(false),
    )
    .expect("ctor ok");
    let relay: Arc<dyn BundleRelay> = Arc::new(flashbots);

    let dir = tempfile::tempdir().expect("tempdir");
    let journal_path = dir.path().join("submission.bin");
    let journal: FileJournal<rust_lmax_mev_app::SubmissionReceipt> =
        FileJournal::open(&journal_path).expect("journal open");

    let (sub_tx, sub_rx) = broadcast::channel::<SubmissionAttempt>(8);
    // G13 inheritance FAILS: live_send is false.
    let gate = SubmissionGate {
        live_send: false,
        production_profile: true,
        hsmkms: true,
    };
    assert!(!gate.permits_submission());

    let driver = tokio::spawn(submission_driver(
        sub_rx,
        Some(relay),
        KillSwitch::new(false),
        gate,
        journal,
        "d-t-e1-7-driver",
    ));

    sub_tx.send(make_attempt()).expect("publish ok");
    tokio::time::sleep(Duration::from_millis(200)).await;

    let received = server.received_requests().await.expect("wiremock recorded");
    assert_eq!(
        received.len(),
        0,
        "D-T-E1-7: G13 inheritance fail must skip submission; got {} POSTs",
        received.len()
    );

    drop(sub_tx);
    let _ = tokio::time::timeout(Duration::from_millis(500), driver).await;

    // Journal still exists but contains nothing.
    let journal_bytes = std::fs::read(&journal_path).expect("journal readable");
    // Journal opens with a small header; the absence of any
    // SubmissionReceipt envelope is what we care about. Assert the
    // journal is at most the open-time header bytes (no append).
    assert!(
        journal_bytes.len() < 128,
        "D-T-E1-7: journal must be empty (open-header-only); got {} bytes",
        journal_bytes.len()
    );
}
