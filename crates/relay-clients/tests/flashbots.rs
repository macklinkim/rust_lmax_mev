//! P4-E RC-F-1..5 integration tests for `FlashbotsRelay` against
//! `wiremock::MockServer`. No live network calls.

use alloy_primitives::{Address, B256, U256};
use rust_lmax_mev_bundle_relay::{BundleRelay, KillSwitch, SignedBundle};
use rust_lmax_mev_relay_clients::{FlashbotsConfig, FlashbotsRelay};
use rust_lmax_mev_relay_sim::{RelaySimError, RelaySimRequest, RelaySimStatus, RelaySimulator};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn relay_pointing_at(uri: &str) -> FlashbotsRelay {
    FlashbotsRelay::new(
        FlashbotsConfig {
            endpoint: uri.to_string(),
            timeout_ms: 2_000,
        },
        KillSwitch::new(false),
    )
    .expect("ctor ok")
}

fn req_with_one_tx() -> RelaySimRequest {
    RelaySimRequest {
        block_hash: B256::from([0u8; 32]),
        state_block_number: 22_000_000,
        txs: vec![vec![0xDE, 0xAD, 0xBE, 0xEF]],
    }
}

/// RC-F-1: happy-path eth_callBundle response is parsed into a
/// non-default RelaySimulationOutcome (gas_used + measured_profit).
#[tokio::test]
async fn rc_f_1_happy_path_response_parsed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 1,
            "jsonrpc": "2.0",
            "result": {
                "totalGasUsed": 123_456u64,
                "coinbaseDiff": "777",
                "ethSentToCoinbase": "888",
                "stateBlockNumber": 22_000_000u64,
                "results": [{}]
            }
        })))
        .mount(&server)
        .await;
    let relay = relay_pointing_at(&server.uri());
    let outcome = relay
        .simulate_bundle(req_with_one_tx())
        .await
        .expect("happy path must succeed");
    assert_eq!(outcome.gas_used, 123_456);
    assert_eq!(outcome.measured_profit_wei, U256::from(777u64));
    assert_eq!(outcome.coinbase_transfer_wei, U256::from(888u64));
    assert_eq!(outcome.status, RelaySimStatus::Success);
    assert!(outcome.state_observations.is_empty());
}

/// RC-F-2: HTTP 500 from server → RelaySimError::Transport.
#[tokio::test]
async fn rc_f_2_transport_error_on_500() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let relay = relay_pointing_at(&server.uri());
    match relay.simulate_bundle(req_with_one_tx()).await {
        Err(RelaySimError::Transport(s)) => {
            // Must NOT contain the URL (DP-E11).
            assert!(!s.contains(server.uri().as_str()));
            assert!(s.contains("500"));
        }
        other => panic!("expected Transport(500), got {other:?}"),
    }
}

/// RC-F-3: malformed JSON response → UnrecognizedResponse.
#[tokio::test]
async fn rc_f_3_unrecognized_response_on_garbage() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
        .mount(&server)
        .await;
    let relay = relay_pointing_at(&server.uri());
    match relay.simulate_bundle(req_with_one_tx()).await {
        Err(RelaySimError::UnrecognizedResponse(_)) => {}
        other => panic!("expected UnrecognizedResponse, got {other:?}"),
    }
}

/// RC-F-5 (R-E2): empty txs → UnsignedBundleUnavailable WITHOUT
/// ANY HTTP I/O. wiremock server is set to PANIC on any request;
/// asserts that no request is made.
#[tokio::test]
async fn rc_f_5_empty_txs_short_circuits_before_network_io() {
    let server = MockServer::start().await;
    // No mock mounted: wiremock returns 404 for any received request,
    // and we additionally verify zero requests at the end.
    let relay = relay_pointing_at(&server.uri());
    let req = RelaySimRequest {
        block_hash: B256::from([0u8; 32]),
        state_block_number: 22_000_000,
        txs: Vec::new(), // empty
    };
    match relay.simulate_bundle(req).await {
        Err(RelaySimError::UnsignedBundleUnavailable) => {}
        other => panic!("expected UnsignedBundleUnavailable short-circuit, got {other:?}"),
    }
    // R-E2 invariant: ZERO requests received by the mock.
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty(),
        "RC-F-5: empty-txs path must NOT issue any HTTP request"
    );
}

// P6-C D-C1: synthetic wire-format placeholder bytes — NOT a valid
// RLP-encoded signed transaction; NO key material implied; the only
// purpose of these bytes is to exercise the hex-encoding path inside
// `crates/relay-clients/src/call_bundle.rs`.
const FIXTURE_PLACEHOLDER_BYTES: &[u8] = &[0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xF0, 0x0D];
const FIXTURE_PLACEHOLDER_HEX: &str = "0xdeadbeefcafef00d";

/// RC-F-6 (P6-C D-T-C1): asserts the JSON-RPC body the FlashbotsRelay
/// POSTs to the relay matches the expected `eth_callBundle` envelope
/// exactly when fed synthetic placeholder bytes. Strategy: permissive
/// happy-path mock (no body matcher) + `received_requests()` + exact
/// `serde_json::Value` equality. Exact equality enforces both presence
/// of every expected key AND absence of any extra key — any future
/// addition of `coinbase` / `gas_price` / `tx_index` in the adapter
/// will fail this test until the expected envelope is updated.
#[tokio::test]
async fn rc_f_6_call_bundle_body_shape_matches_with_placeholder_bytes() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 1,
            "jsonrpc": "2.0",
            "result": {
                "totalGasUsed": 1u64,
                "coinbaseDiff": "0",
                "ethSentToCoinbase": "0",
                "stateBlockNumber": 22_000_000u64,
                "results": [{}]
            }
        })))
        .mount(&server)
        .await;
    let relay = relay_pointing_at(&server.uri());
    let req = RelaySimRequest {
        block_hash: B256::from([0u8; 32]),
        state_block_number: 22_000_000u64, // 0x14fb180
        txs: vec![FIXTURE_PLACEHOLDER_BYTES.to_vec()],
    };
    relay
        .simulate_bundle(req)
        .await
        .expect("happy path must succeed");
    let received = server
        .received_requests()
        .await
        .expect("wiremock recorded requests");
    assert_eq!(received.len(), 1, "exactly one POST expected");
    let parsed: serde_json::Value =
        serde_json::from_slice(&received[0].body).expect("body is valid JSON");
    let expected = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_callBundle",
        "params": [{
            "txs": [FIXTURE_PLACEHOLDER_HEX],
            "blockNumber": "0x14fb180",
            "stateBlockNumber": "0x14fb180"
        }]
    });
    assert_eq!(
        parsed, expected,
        "request body must match expected JSON-RPC envelope exactly"
    );
}

/// D-T-E1-5: P6B-E1 localhost-only happy-path. Wiremock listens at
/// `127.0.0.1:<random>`; the adapter's localhost runtime check passes;
/// `submit_bundle` POSTs an `eth_sendBundle` JSON-RPC envelope and
/// returns `Ok(SubmissionReceipt)`. The request body shape is verified
/// by exact `serde_json::Value` equality (mirrors the P6B-C RC-F-6
/// `eth_callBundle` pattern).
#[tokio::test]
async fn d_t_e1_5_submit_bundle_ok_on_local_wiremock() {
    let server = MockServer::start().await;
    // The mock returns a canned bundleHash. P6B-E1 SIMPLIFICATION:
    // the workspace's bundle-byte equality check (G12 step 6) is
    // "non-empty + len >= 64"; the mock can return any string.
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 1,
            "jsonrpc": "2.0",
            "result": {
                "bundleHash": "0xabcdef0123456789",
            }
        })))
        .mount(&server)
        .await;
    let relay = relay_pointing_at(&server.uri());
    let bundle = SignedBundle {
        block_hash: [0u8; 32],
        state_block_number: 22_000_000,
        signed_txs: vec![vec![0xAAu8; 80]],
        coinbase_recipient: Address::ZERO,
        coinbase_transfer_wei: U256::ZERO,
        validity_block_min: 22_000_001,
        validity_block_max: 22_000_005,
    };
    let receipt = relay
        .submit_bundle(&bundle)
        .await
        .expect("D-T-E1-5: localhost submit_bundle must return Ok");
    assert_eq!(receipt.relay_name, "flashbots");
    assert_eq!(receipt.bundle_hash, "0xabcdef0123456789");
    assert!(receipt.submitted_at_unix_ns > 0);

    let received = server.received_requests().await.expect("wiremock recorded");
    assert_eq!(received.len(), 1, "exactly one POST expected");
    let parsed: serde_json::Value =
        serde_json::from_slice(&received[0].body).expect("body is valid JSON");
    let expected = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_sendBundle",
        "params": [{
            "txs": [format!("0x{}", "aa".repeat(80))],
            "blockNumber": "0x14fb181"
        }]
    });
    assert_eq!(parsed, expected, "eth_sendBundle envelope must match");
}
