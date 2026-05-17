//! P4-E RC-COMMON-2 (DP-E11 + R-E4) secret-redaction test across the
//! 5 surfaces specified in the execution note v0.6: Debug output,
//! `RelaySimError` strings, `MismatchAbort.detail`, tracing log
//! capture, journal-bytes brute byte-grep.
//!
//! Constructs a `BloxrouteRelay` against a URL with embedded
//! `SECRETTOKEN` query param + an API key `SECRETKEY`; drives a
//! `Transport(500)` failure (mock receives the request → categorical
//! error message); asserts NEITHER secret appears in:
//!   1. `format!("{:?}", relay)` — Debug elision
//!   2. `RelaySimError::Transport(s)` Display
//!   3. `MismatchAbort.detail` from `compare_result`
//!   4. tracing log output captured via TestWriter
//!   5. rkyv-encoded `EventEnvelope<MismatchAbort>` bytes (brute grep)

use std::io::Write;
use std::sync::{Arc, Mutex};

use alloy_primitives::{B256, U256};
use rust_lmax_mev_bundle_relay::KillSwitch;
use rust_lmax_mev_relay_clients::{BloxrouteConfig, BloxrouteRelay};
use rust_lmax_mev_relay_sim::{
    compare_result, ComparatorInputs, LocalBundleShape, RelaySimError, RelaySimRequest,
    RelaySimulator,
};
use rust_lmax_mev_simulator::{LocalStateFingerprint, ProfitSource, SimStatus, SimulationOutcome};
use rust_lmax_mev_types::{ChainContext, EventEnvelope, EventSource, PublishMeta};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SECRET_TOKEN: &str = "SECRETTOKEN";
const SECRET_KEY: &str = "SECRETKEY";

#[derive(Clone, Default)]
struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

impl Write for CaptureWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn rc_common_2_secret_redaction_across_five_surfaces() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    // Embed SECRETTOKEN in the endpoint URL itself; SECRETKEY in API key.
    let endpoint_with_secret = format!("{}/?token={}", server.uri(), SECRET_TOKEN);
    let cfg = BloxrouteConfig {
        endpoint: endpoint_with_secret,
        timeout_ms: 1_000,
        api_key: Some(SECRET_KEY.to_string()),
    };
    let relay = BloxrouteRelay::new(cfg, KillSwitch::new(false), false).expect("ctor ok");

    // Surface 1: Debug elision.
    let dbg = format!("{relay:?}");
    assert!(
        !dbg.contains(SECRET_TOKEN) && !dbg.contains(SECRET_KEY),
        "Debug must elide both URL secret and API key; got {dbg:?}"
    );

    // Surface 4 setup: tracing TestWriter to capture any secrets that
    // leak into log output during the call. Set up BEFORE the call.
    let buf = CaptureWriter::default();
    let buf_clone = buf.clone();
    let make_writer = move || buf_clone.clone();
    let subscriber = tracing_subscriber::fmt::Subscriber::builder()
        .with_writer(make_writer)
        .with_max_level(tracing::Level::TRACE)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    // Drive a Transport(500) failure.
    let req = RelaySimRequest {
        block_hash: B256::from([0u8; 32]),
        state_block_number: 22_000_000,
        txs: vec![vec![0xAB]],
    };
    let err = relay
        .simulate_bundle(req)
        .await
        .expect_err("RC-COMMON-2 must observe an Err");

    // Surface 2: RelaySimError Display.
    let err_str = format!("{err}");
    assert!(
        !err_str.contains(SECRET_TOKEN) && !err_str.contains(SECRET_KEY),
        "RelaySimError must not leak secrets; got {err_str:?}"
    );

    // Surface 3: MismatchAbort.detail.
    let local_outcome = SimulationOutcome {
        opportunity_block_number: 22_000_000,
        gas_used: 100_000,
        status: SimStatus::Success,
        simulated_profit_wei: U256::from(7_000u64),
        profit_source: ProfitSource::RevmComputed,
    };
    let local_shape = LocalBundleShape {
        expected_inclusion_index: 0,
        expected_coinbase_transfer_wei: U256::ZERO,
    };
    let local_fp = LocalStateFingerprint {
        block_hash: B256::from([0xCD; 32]),
        observations: Vec::new(),
    };
    let abort = compare_result(
        ComparatorInputs {
            local: &local_outcome,
            local_shape: &local_shape,
            local_fingerprint: &local_fp,
        },
        Err(&err),
    )
    .expect_err("Unknown classification expected");
    assert!(
        !abort.detail.contains(SECRET_TOKEN) && !abort.detail.contains(SECRET_KEY),
        "MismatchAbort.detail must not leak secrets; got {:?}",
        abort.detail
    );

    // Surface 4: drop the subscriber guard + read the buffer.
    drop(_guard);
    let log_bytes = buf.0.lock().unwrap().clone();
    let log_str = String::from_utf8_lossy(&log_bytes);
    assert!(
        !log_str.contains(SECRET_TOKEN) && !log_str.contains(SECRET_KEY),
        "tracing log must not leak secrets; got {log_str:?}"
    );

    // Surface 5: rkyv-encoded MismatchAbort bytes (brute byte-grep).
    let envelope = EventEnvelope::seal(
        PublishMeta {
            source: EventSource::Relay,
            chain_context: ChainContext {
                chain_id: 1,
                block_number: 22_000_000,
                block_hash: [0u8; 32],
            },
            event_version: 1,
            correlation_id: 42,
        },
        *abort,
        1,
        1_700_000_000_000_000_000,
    )
    .expect("envelope seal");
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&envelope).expect("rkyv ok");
    let secret_token_bytes = SECRET_TOKEN.as_bytes();
    let secret_key_bytes = SECRET_KEY.as_bytes();
    let contains_subslice = |hay: &[u8], needle: &[u8]| {
        if needle.is_empty() || hay.len() < needle.len() {
            return false;
        }
        hay.windows(needle.len()).any(|w| w == needle)
    };
    assert!(
        !contains_subslice(&bytes, secret_token_bytes),
        "journal bytes must not contain SECRETTOKEN"
    );
    assert!(
        !contains_subslice(&bytes, secret_key_bytes),
        "journal bytes must not contain SECRETKEY"
    );
}

/// R-E22: a malicious or buggy relay echoes secrets into the JSON-RPC
/// error message. The adapter MUST NOT propagate the relay-controlled
/// `error.message` field verbatim into `RelaySimError::Transport`;
/// only the numeric `error.code` is safe to surface.
#[tokio::test]
async fn rc_common_2_extra_jsonrpc_body_secret_redacted() {
    let server = MockServer::start().await;
    let body = json!({
        "id": 1,
        "jsonrpc": "2.0",
        "error": {
            "code": -32000,
            "message": format!("relay error: token={SECRET_TOKEN} key={SECRET_KEY}")
        }
    });
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;
    let relay = BloxrouteRelay::new(
        BloxrouteConfig {
            endpoint: server.uri(),
            timeout_ms: 1_000,
            api_key: Some(SECRET_KEY.to_string()),
        },
        KillSwitch::new(false),
        false,
    )
    .expect("ctor ok");
    let req = RelaySimRequest {
        block_hash: B256::from([0u8; 32]),
        state_block_number: 22_000_000,
        txs: vec![vec![0xAB]],
    };
    let err = relay
        .simulate_bundle(req)
        .await
        .expect_err("R-E22: JSON-RPC error must surface as Err");
    let display = format!("{err}");
    assert!(
        !display.contains(SECRET_TOKEN),
        "R-E22: RelaySimError must not echo SECRETTOKEN from relay JSON-RPC error.message; got {display:?}"
    );
    assert!(
        !display.contains(SECRET_KEY),
        "R-E22: RelaySimError must not echo SECRETKEY; got {display:?}"
    );
    // Should still be a RelaySimError::Transport categorical message
    // mentioning the JSON-RPC code (numeric, safe).
    match err {
        RelaySimError::Transport(s) => {
            assert!(
                s.contains("-32000"),
                "JSON-RPC code must surface; got {s:?}"
            );
        }
        other => panic!("expected Transport, got {other:?}"),
    }
}

/// R-E24: hex-looking text in `result.results[0].revert` is NOT a
/// safe-redaction boundary — URL tokens / API keys can be hex-like.
/// Adapter MUST use a fixed redacted marker. Verified end-to-end:
/// the secret hex string MUST NOT appear in `RelaySimulationOutcome`
/// Debug/serde, `MismatchAbort` Debug/detail, OR rkyv journal bytes.
#[tokio::test]
async fn rc_common_2_extra_relay_revert_hex_secret_redacted() {
    use rust_lmax_mev_relay_sim::compare;
    use rust_lmax_mev_relay_sim::RelaySimulationOutcome;

    // Hex-only secret — passes any "is_ascii_hexdigit" filter, stand-
    // in for a hex-like API key the malicious relay echoed.
    const HEX_SECRET: &str = "0xdeadbeefcafebabedeadbeefcafebabe";

    let server = MockServer::start().await;
    let body = json!({
        "id": 1,
        "jsonrpc": "2.0",
        "result": {
            "totalGasUsed": 100_000u64,
            "coinbaseDiff": "0",
            "ethSentToCoinbase": "0",
            "stateBlockNumber": 22_000_000u64,
            "results": [{ "revert": HEX_SECRET }]
        }
    });
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;
    let relay = BloxrouteRelay::new(
        BloxrouteConfig {
            endpoint: server.uri(),
            timeout_ms: 1_000,
            api_key: Some("dummy-test-key".to_string()),
        },
        KillSwitch::new(false),
        false,
    )
    .expect("ctor ok");
    let req = RelaySimRequest {
        block_hash: B256::from([0u8; 32]),
        state_block_number: 22_000_000,
        txs: vec![vec![0xAB]],
    };
    let outcome: RelaySimulationOutcome = relay
        .simulate_bundle(req)
        .await
        .expect("R-E24: response with revert must parse to RelaySimulationOutcome");

    // Surface 1: outcome Debug must not contain the hex secret.
    let dbg = format!("{outcome:?}");
    assert!(
        !dbg.contains(HEX_SECRET),
        "R-E24: RelaySimulationOutcome Debug must not preserve relay-supplied revert text; got {dbg:?}"
    );
    // Surface 2: outcome serde JSON form must not contain the hex secret.
    let json_str = serde_json::to_string(&outcome).expect("serde");
    assert!(
        !json_str.contains(HEX_SECRET),
        "R-E24: RelaySimulationOutcome serde must not preserve relay revert text; got {json_str:?}"
    );

    // Surface 3: feed outcome into compare() against a non-matching
    // local sim → MismatchAbort.relay carries the redacted outcome;
    // assert the secret does not appear in any field.
    let local_outcome = SimulationOutcome {
        opportunity_block_number: 22_000_000,
        gas_used: 100_000,
        status: SimStatus::Success,
        simulated_profit_wei: U256::ZERO,
        profit_source: ProfitSource::RevmComputed,
    };
    let local_shape = LocalBundleShape {
        expected_inclusion_index: 0,
        expected_coinbase_transfer_wei: U256::ZERO,
    };
    let local_fp = LocalStateFingerprint {
        block_hash: B256::from([0xCD; 32]),
        observations: Vec::new(),
    };
    // Local Success vs relay Reverted (after redaction) → comparator
    // returns Err(Revert) per DP-D17 precedence.
    let abort = compare(
        ComparatorInputs {
            local: &local_outcome,
            local_shape: &local_shape,
            local_fingerprint: &local_fp,
        },
        &outcome,
    )
    .expect_err("Revert expected");
    let abort_dbg = format!("{abort:?}");
    assert!(
        !abort_dbg.contains(HEX_SECRET),
        "R-E24: MismatchAbort Debug must not surface relay revert text; got {abort_dbg:?}"
    );
    assert!(
        !abort.detail.contains(HEX_SECRET),
        "R-E24: MismatchAbort.detail must not surface relay revert text; got {:?}",
        abort.detail
    );

    // Surface 4: rkyv journal bytes must not contain the hex secret.
    let envelope = EventEnvelope::seal(
        PublishMeta {
            source: EventSource::Relay,
            chain_context: ChainContext {
                chain_id: 1,
                block_number: 22_000_000,
                block_hash: [0u8; 32],
            },
            event_version: 1,
            correlation_id: 42,
        },
        *abort,
        1,
        1_700_000_000_000_000_000,
    )
    .expect("envelope seal");
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&envelope).expect("rkyv ok");
    let needle = HEX_SECRET.as_bytes();
    let contains_subslice = |hay: &[u8], n: &[u8]| {
        if n.is_empty() || hay.len() < n.len() {
            return false;
        }
        hay.windows(n.len()).any(|w| w == n)
    };
    assert!(
        !contains_subslice(&bytes, needle),
        "R-E24: journal bytes must not contain the relay's hex-looking secret"
    );
}
