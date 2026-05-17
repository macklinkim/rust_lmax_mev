//! P6B-E1 D-E1-4 `eth_sendBundle` JSON-RPC helper.
//!
//! Used by `FlashbotsRelay::submit_bundle` (P6B-E1 D-E1-3) when ALL of
//! the localhost-only gates have passed:
//!
//! 1. `config.relay.live_send == true` (validated at boot per
//!    `ConfigError::LiveSendRequiresLocalhostEndpoint`).
//! 2. `KillSwitch::is_active() == false` (PRECEDENCE per
//!    `BundleRelayError::KillSwitchActive`).
//! 3. Endpoint host in `{"127.0.0.1", "localhost", "::1"}`
//!    (adapter-runtime defense-in-depth per
//!    `BundleRelayError::SubmitDisabledNonLocalhost`).
//!
//! This helper performs the HTTP POST + response parse only; gating is
//! the caller's responsibility. POSTs to the relay's HTTP endpoint
//! using the standard Flashbots-shaped `eth_sendBundle` envelope:
//!
//! ```text
//! {
//!   "jsonrpc": "2.0",
//!   "id": 1,
//!   "method": "eth_sendBundle",
//!   "params": [{
//!     "txs": ["0x..."],
//!     "blockNumber": "0x..."
//!   }]
//! }
//! ```
//!
//! The response is parsed into a `SubmissionReceipt`. Per the workspace
//! DP-E11 secret-redaction convention, no URL, header, or request
//! payload is ever rendered into any error variant.

use rust_lmax_mev_bundle_relay::{BundleRelayError, SignedBundle, SubmissionReceipt};
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Serialize)]
struct EthSendBundleRequest<'a> {
    jsonrpc: &'a str,
    id: u64,
    method: &'a str,
    params: [EthSendBundleParams; 1],
}

#[derive(Debug, Serialize)]
struct EthSendBundleParams {
    txs: Vec<String>,
    #[serde(rename = "blockNumber")]
    block_number: String,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[serde(default)]
    result: Option<EthSendBundleResult>,
    #[serde(default)]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    #[serde(default)]
    _code: i64,
    /// SECRET REDACTION (R-E22 carry-forward): kept on the wire-shape
    /// struct so JSON-RPC responses parse correctly, but DELIBERATELY
    /// NOT surfaced into any downstream `BundleRelayError` / journal
    /// payload -- a relay or proxy can echo URL tokens, auth headers,
    /// or API keys into this field.
    #[serde(default, rename = "message")]
    _message_redacted: String,
}

#[derive(Debug, Deserialize)]
struct EthSendBundleResult {
    /// Bundle hash returned by the relay. For the P6B-E1 local-wiremock
    /// dress rehearsal the mock can return any string; the
    /// workspace's bundle-byte equality check (G12 Step 6) is
    /// simplified to "non-empty signed_bytes + len >= 64" in P6B-E1
    /// per v0.1 lock (D). True keccak-against-relay-echo is P6B-E2
    /// scope when production relay responses are observable.
    #[serde(rename = "bundleHash", default)]
    bundle_hash: String,
}

/// P6B-E1 D-E1-4: issue `eth_sendBundle` against the given endpoint
/// and parse the response into a `SubmissionReceipt`.
///
/// Preconditions (caller responsibility):
/// - `endpoint` host already verified to be localhost.
/// - `bundle.signed_txs` non-empty.
/// - Kill switch already verified inactive.
///
/// Errors carry NO URL, header, or request-payload data per DP-E11.
/// The relay's `bundleHash` field becomes the receipt's
/// `bundle_hash` value verbatim; the receipt's `submitted_at_unix_ns`
/// is captured at the start of the POST.
pub(crate) async fn submit_eth_send_bundle(
    http: &reqwest::Client,
    endpoint: &Url,
    relay_name: &str,
    bundle: &SignedBundle,
) -> Result<SubmissionReceipt, BundleRelayError> {
    let block_hex = format!("0x{:x}", bundle.validity_block_min);
    let txs: Vec<String> = bundle
        .signed_txs
        .iter()
        .map(|raw| {
            let mut s = String::with_capacity(2 + raw.len() * 2);
            s.push_str("0x");
            for b in raw {
                s.push_str(&format!("{b:02x}"));
            }
            s
        })
        .collect();
    let body = EthSendBundleRequest {
        jsonrpc: "2.0",
        id: 1,
        method: "eth_sendBundle",
        params: [EthSendBundleParams {
            txs,
            block_number: block_hex,
        }],
    };

    let submitted_at_unix_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);

    let resp = http
        .post(endpoint.clone())
        .json(&body)
        .send()
        .await
        .map_err(|_| BundleRelayError::SubmitHttpFailed)?;
    if !resp.status().is_success() {
        return Err(BundleRelayError::SubmitHttpFailed);
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|_| BundleRelayError::SubmitHttpFailed)?;
    let parsed: JsonRpcResponse =
        serde_json::from_slice(&bytes).map_err(|_| BundleRelayError::SubmitHttpFailed)?;
    if parsed.error.is_some() {
        return Err(BundleRelayError::SubmitHttpFailed);
    }
    let result = parsed.result.ok_or(BundleRelayError::SubmitHttpFailed)?;

    Ok(SubmissionReceipt {
        relay_name: relay_name.to_string(),
        bundle_hash: result.bundle_hash,
        submitted_at_unix_ns,
        // P6B-E2 D-E2-3: `submission_driver` populates this on the
        // bundle-byte equality check site; adapter leaves it empty.
        local_bundle_hash: String::new(),
    })
}

/// P6B-E1 D-E1-3 helper: checks whether the parsed endpoint URL's
/// host is in the localhost-permitted set
/// (`127.0.0.1`, `localhost`, `::1`). Defense-in-depth with the
/// config-validate-time `ConfigError::LiveSendRequiresLocalhostEndpoint`.
pub(crate) fn is_localhost_url(endpoint: &Url) -> bool {
    matches!(
        endpoint.host_str(),
        Some("127.0.0.1") | Some("localhost") | Some("::1") | Some("[::1]")
    )
}
