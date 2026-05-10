//! Phase 4 P4-E shared `eth_callBundle` JSON-RPC helper.
//!
//! Used by both `FlashbotsRelay` and `BloxrouteRelay`. POSTs a
//! standard JSON-RPC `eth_callBundle` request to the relay's HTTP
//! endpoint and parses the response into a `RelaySimulationOutcome`.
//!
//! Per DP-E11: this module never logs URLs, headers, or request
//! payloads. Errors are constructed without the URL or any header
//! values to prevent secret leakage.

use rust_lmax_mev_relay_sim::{
    RelaySimError, RelaySimRequest, RelaySimStatus, RelaySimulationOutcome,
};
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Serialize)]
struct EthCallBundleRequest<'a> {
    jsonrpc: &'a str,
    id: u64,
    method: &'a str,
    params: [EthCallBundleParams<'a>; 1],
}

#[derive(Debug, Serialize)]
struct EthCallBundleParams<'a> {
    txs: Vec<String>,
    #[serde(rename = "blockNumber")]
    block_number: String,
    #[serde(rename = "stateBlockNumber")]
    state_block_number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp: Option<u64>,
    // Phantom lifetime tie so this struct shares the lifetime with
    // the &'a str fields above without forcing each field to declare
    // 'a separately.
    #[serde(skip)]
    _marker: std::marker::PhantomData<&'a ()>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[serde(default)]
    result: Option<EthCallBundleResult>,
    #[serde(default)]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    #[serde(default)]
    code: i64,
    /// R-E22 SECRET REDACTION: kept on the wire-shape struct so
    /// JSON-RPC responses parse correctly, but DELIBERATELY NOT
    /// surfaced into any downstream `RelaySimError` / `MismatchAbort`
    /// / journal payload — a relay or proxy can echo URL tokens,
    /// auth headers, or API keys into this field.
    #[serde(default, rename = "message")]
    _message_redacted: String,
}

#[derive(Debug, Deserialize)]
struct EthCallBundleResult {
    #[serde(rename = "totalGasUsed", default)]
    total_gas_used: u64,
    #[serde(rename = "coinbaseDiff", default)]
    coinbase_diff: String,
    #[serde(rename = "ethSentToCoinbase", default)]
    eth_sent_to_coinbase: String,
    #[serde(rename = "stateBlockNumber", default)]
    _state_block_number: u64,
    #[serde(default)]
    results: Vec<EthCallBundleTxResult>,
}

#[derive(Debug, Deserialize)]
struct EthCallBundleTxResult {
    #[serde(default)]
    revert: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

/// Issues `eth_callBundle` against the given endpoint and parses
/// the response. Caller must have already verified `req.txs` is
/// non-empty (the per-adapter R-E2 fail-closed check); this helper
/// does NOT re-check.
///
/// Errors carry NO URL, header, or request-payload data — only
/// reqwest-supplied error categories ("transport"), or response-shape
/// surface ("unrecognized payload"). DP-E11 verified by RC-COMMON-2.
pub(crate) async fn call_eth_call_bundle(
    http: &reqwest::Client,
    endpoint: &Url,
    req: &RelaySimRequest,
) -> Result<RelaySimulationOutcome, RelaySimError> {
    let block_hex = format!("0x{:x}", req.state_block_number);
    let txs: Vec<String> = req
        .txs
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
    let body = EthCallBundleRequest {
        jsonrpc: "2.0",
        id: 1,
        method: "eth_callBundle",
        params: [EthCallBundleParams {
            txs,
            block_number: block_hex.clone(),
            state_block_number: block_hex,
            timestamp: None,
            _marker: std::marker::PhantomData,
        }],
    };
    let resp = http
        .post(endpoint.clone())
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            // SECRET REDACTION (DP-E11): include only the categorical
            // reqwest error label, NOT the URL or any payload bytes.
            let kind = if e.is_timeout() {
                "timeout"
            } else if e.is_connect() {
                "connect"
            } else if e.is_request() {
                "request"
            } else {
                "other"
            };
            RelaySimError::Transport(format!("relay HTTP error: {kind}"))
        })?;
    let status = resp.status();
    if !status.is_success() {
        // Status code is safe to surface (no secrets); body is NOT
        // included.
        return Err(RelaySimError::Transport(format!(
            "relay HTTP status: {}",
            status.as_u16()
        )));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|_| RelaySimError::Transport("relay response body read failed".to_string()))?;
    let parsed: JsonRpcResponse = serde_json::from_slice(&bytes).map_err(|_| {
        RelaySimError::UnrecognizedResponse("relay response is not valid JSON-RPC".to_string())
    })?;
    if let Some(err) = parsed.error {
        // R-E22 SECRET REDACTION: do NOT include `err.message` —
        // a relay or proxy can echo URL tokens / auth headers / API
        // keys into the JSON-RPC error message, after which it would
        // leak into MismatchAbort.detail and the mismatch journal.
        // Keep ONLY the categorical numeric code (relay-controlled
        // but not free-form text).
        return Err(RelaySimError::Transport(format!(
            "relay JSON-RPC error code {}",
            err.code
        )));
    }
    let result = parsed.result.ok_or_else(|| {
        RelaySimError::UnrecognizedResponse("relay response missing 'result' field".to_string())
    })?;

    // Status: any per-tx revert/error → Reverted with a sanitized
    // reason; otherwise Success.
    //
    // R-E22 SECRET REDACTION: relay-supplied free-form `error`
    // strings are dropped entirely (replaced with a fixed-format
    // tag). The `revert` field is sanitized to `0x[0-9a-f]*` only;
    // any non-hex byte from a malicious or buggy relay is stripped
    // before the value reaches `MismatchAbort.detail` or the journal.
    let status_field = result
        .results
        .iter()
        .find_map(|r| {
            r.revert
                .as_ref()
                .map(|s| RelaySimStatus::Reverted {
                    reason_hex: sanitize_hex(s),
                })
                .or_else(|| {
                    r.error.as_ref().map(|_| RelaySimStatus::Reverted {
                        reason_hex: "relay-tx-error-redacted".to_string(),
                    })
                })
        })
        .unwrap_or(RelaySimStatus::Success);

    let coinbase_transfer = parse_decimal_or_hex_u256(&result.eth_sent_to_coinbase);
    let measured_profit = parse_decimal_or_hex_u256(&result.coinbase_diff);

    Ok(RelaySimulationOutcome {
        gas_used: result.total_gas_used,
        status: status_field,
        measured_profit_wei: measured_profit,
        // Flashbots/bloXroute eth_callBundle does NOT return per-slot
        // observations; the comparator's StateDependency check
        // intersects with the local fingerprint and (per DP-D10
        // boundary established in P4-D) treats relay-omitted slots
        // as "no info". Empty here is correct.
        state_observations: Vec::new(),
        // P4-E sends one bundle per request → inclusion_index = 0.
        inclusion_index: 0,
        coinbase_transfer_wei: coinbase_transfer,
    })
}

/// R-E22 SECRET REDACTION: strip any non-hex byte from a
/// relay-supplied "revert" string. Returns `"0x"` followed by ONLY
/// the hex digits the relay sent. A malicious or buggy relay that
/// stuffs `SECRETTOKEN` into the revert field would have those
/// characters silently dropped here.
fn sanitize_hex(s: &str) -> String {
    let mut out = String::with_capacity(2 + s.len());
    out.push_str("0x");
    let trimmed = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    for c in trimmed.chars() {
        if c.is_ascii_hexdigit() {
            out.push(c.to_ascii_lowercase());
        }
    }
    out
}

/// Best-effort U256 parse: accepts decimal-string, "0x"-hex, or empty
/// (→ ZERO). Used for the relay's coinbaseDiff / ethSentToCoinbase
/// fields, which different relays serialize differently.
fn parse_decimal_or_hex_u256(s: &str) -> alloy_primitives::U256 {
    use alloy_primitives::U256;
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return U256::ZERO;
    }
    if let Some(stripped) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        return U256::from_str_radix(stripped, 16).unwrap_or(U256::ZERO);
    }
    U256::from_str_radix(trimmed, 10).unwrap_or(U256::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::U256;

    #[test]
    fn parse_handles_empty_decimal_hex() {
        assert_eq!(parse_decimal_or_hex_u256(""), U256::ZERO);
        assert_eq!(parse_decimal_or_hex_u256("12345"), U256::from(12_345u64));
        assert_eq!(parse_decimal_or_hex_u256("0xff"), U256::from(255u64));
        assert_eq!(parse_decimal_or_hex_u256("garbage"), U256::ZERO);
    }

    /// R-E22: sanitize_hex strips non-hex bytes (catches a malicious
    /// or buggy relay echoing secrets into the revert field).
    #[test]
    fn sanitize_hex_strips_non_hex_bytes() {
        assert_eq!(sanitize_hex(""), "0x");
        assert_eq!(sanitize_hex("0xCAFEBABE"), "0xcafebabe");
        // SECRETTOKEN contains chars that are also hex digits ('E','A','D')
        // — those will pass through, but the alphabetic non-hex bytes
        // ('S','R','T','K','N','O') are dropped.
        let dirty = "0xCAFE_SECRETTOKEN_DEAD";
        let out = sanitize_hex(dirty);
        // Must NOT contain the full secret as a substring.
        assert!(!out.contains("SECRETTOKEN"), "got {out}");
        assert!(!out.contains("S"));
        assert!(!out.contains("K"));
        assert!(!out.contains("N"));
        assert!(out.starts_with("0x"));
    }
}
