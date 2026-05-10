//! P4-E RC-F-1..5 integration tests for `FlashbotsRelay` against
//! `wiremock::MockServer`. No live network calls.

use alloy_primitives::{B256, U256};
use rust_lmax_mev_relay_clients::{FlashbotsConfig, FlashbotsRelay};
use rust_lmax_mev_relay_sim::{RelaySimError, RelaySimRequest, RelaySimStatus, RelaySimulator};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn relay_pointing_at(uri: &str) -> FlashbotsRelay {
    FlashbotsRelay::new(FlashbotsConfig {
        endpoint: uri.to_string(),
        timeout_ms: 2_000,
    })
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
