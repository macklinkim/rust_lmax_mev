//! P4-E RC-B-1..5 integration tests for `BloxrouteRelay` against
//! `wiremock::MockServer`. No live network calls.

use alloy_primitives::{B256, U256};
use rust_lmax_mev_bundle_relay::KillSwitch;
use rust_lmax_mev_relay_clients::{BloxrouteConfig, BloxrouteRelay};
use rust_lmax_mev_relay_sim::{RelaySimError, RelaySimRequest, RelaySimStatus, RelaySimulator};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn relay_pointing_at(uri: &str) -> BloxrouteRelay {
    BloxrouteRelay::new(
        BloxrouteConfig {
            endpoint: uri.to_string(),
            timeout_ms: 2_000,
            api_key: Some("dummy-test-key".to_string()),
        },
        KillSwitch::new(false),
    )
    .expect("ctor ok")
}

fn req_with_one_tx() -> RelaySimRequest {
    RelaySimRequest {
        block_hash: B256::from([0u8; 32]),
        state_block_number: 22_000_000,
        txs: vec![vec![0xCA, 0xFE]],
    }
}

/// RC-B-1: happy-path eth_callBundle response is parsed.
#[tokio::test]
async fn rc_b_1_happy_path_response_parsed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 1,
            "jsonrpc": "2.0",
            "result": {
                "totalGasUsed": 50_000u64,
                "coinbaseDiff": "0x1f4",  // 500
                "ethSentToCoinbase": "0x12c",  // 300
                "stateBlockNumber": 22_000_000u64,
                "results": [{}, {}]
            }
        })))
        .mount(&server)
        .await;
    let relay = relay_pointing_at(&server.uri());
    let outcome = relay.simulate_bundle(req_with_one_tx()).await.unwrap();
    assert_eq!(outcome.gas_used, 50_000);
    assert_eq!(outcome.measured_profit_wei, U256::from(500u64));
    assert_eq!(outcome.coinbase_transfer_wei, U256::from(300u64));
    assert_eq!(outcome.status, RelaySimStatus::Success);
}

/// RC-B-2: HTTP 500 → Transport.
#[tokio::test]
async fn rc_b_2_transport_error_on_500() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let relay = relay_pointing_at(&server.uri());
    match relay.simulate_bundle(req_with_one_tx()).await {
        Err(RelaySimError::Transport(s)) => {
            assert!(!s.contains(server.uri().as_str()));
        }
        other => panic!("expected Transport, got {other:?}"),
    }
}

/// RC-B-3: malformed JSON → UnrecognizedResponse.
#[tokio::test]
async fn rc_b_3_unrecognized_response_on_garbage() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_string("???"))
        .mount(&server)
        .await;
    let relay = relay_pointing_at(&server.uri());
    match relay.simulate_bundle(req_with_one_tx()).await {
        Err(RelaySimError::UnrecognizedResponse(_)) => {}
        other => panic!("expected UnrecognizedResponse, got {other:?}"),
    }
}

/// RC-B-5 (R-E2): empty txs short-circuits before network I/O.
#[tokio::test]
async fn rc_b_5_empty_txs_short_circuits_before_network_io() {
    let server = MockServer::start().await;
    let relay = relay_pointing_at(&server.uri());
    let req = RelaySimRequest {
        block_hash: B256::from([0u8; 32]),
        state_block_number: 22_000_000,
        txs: Vec::new(),
    };
    match relay.simulate_bundle(req).await {
        Err(RelaySimError::UnsignedBundleUnavailable) => {}
        other => panic!("expected UnsignedBundleUnavailable, got {other:?}"),
    }
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty(),
        "RC-B-5: empty-txs path must NOT issue any HTTP request"
    );
}

// P6-C D-C1: synthetic wire-format placeholder bytes — NOT a valid
// RLP-encoded signed transaction; NO key material implied; the only
// purpose of these bytes is to exercise the hex-encoding path inside
// `crates/relay-clients/src/call_bundle.rs`. Constants duplicated
// verbatim across the two test files (per v0.3 §D-C3 self-contained
// test-file pattern).
const FIXTURE_PLACEHOLDER_BYTES: &[u8] = &[0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xF0, 0x0D];
const FIXTURE_PLACEHOLDER_HEX: &str = "0xdeadbeefcafef00d";

/// RC-B-6 (P6-C D-T-C2): mirror of RC-F-6 for `BloxrouteRelay`.
/// Asserts the JSON-RPC body matches the expected `eth_callBundle`
/// envelope exactly. Guards against future adapter divergence from
/// the shared `crates/relay-clients/src/call_bundle.rs` helper.
#[tokio::test]
async fn rc_b_6_call_bundle_body_shape_matches_with_placeholder_bytes() {
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
