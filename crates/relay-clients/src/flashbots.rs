//! Phase 4 P4-E Flashbots `eth_callBundle` HTTP adapter.
//!
//! Per the user-approved P4-E execution note v0.6 §D-E2.
//! Read-only `eth_callBundle` only. NO `eth_sendBundle`, NO production
//! key material, NO signing, NO production submission.
//!
//! `simulate_bundle` performs a fail-closed pre-check (R-E2): if
//! `req.txs.is_empty()` the adapter returns `Err(UnsignedBundleUnavailable)`
//! BEFORE any URL is opened and BEFORE any HTTP I/O. Empty-txs
//! cannot leak into the URL or HTTP-level logging because no HTTP
//! call is constructed.
//!
//! `submit_bundle` always returns `Err(SubmitDisabled)` per DP-E1.
//! Phase 5 Safety Gate is the only path to enabling submission.
//!
//! Secret redaction (DP-E11): the URL is held in a private field and
//! NEVER appears in `tracing::*` calls, error strings, Debug output,
//! or any other user-facing surface. The `Debug` impl is hand-written
//! and elides the URL.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rust_lmax_mev_bundle_relay::{
    BundleRelay, BundleRelayError, KillSwitch, SignedBundle, SubmissionReceipt,
};
use rust_lmax_mev_relay_sim::{
    RelaySimError, RelaySimRequest, RelaySimulationOutcome, RelaySimulator,
};
use serde::{Deserialize, Serialize};
use url::Url;

/// Default Flashbots relay endpoint for `eth_callBundle`.
pub const DEFAULT_FLASHBOTS_ENDPOINT: &str = "https://relay.flashbots.net";

/// Default per-request timeout (ms). Comparator latency budget per
/// `BundleCandidate` is single-digit hundreds of ms; 2000ms is a
/// generous upper bound that never blocks the producer chain.
pub const DEFAULT_FLASHBOTS_TIMEOUT_MS: u64 = 2_000;

/// Flashbots adapter configuration. Operator-supplied via the engine
/// config in `crates/config`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlashbotsConfig {
    pub endpoint: String,
    pub timeout_ms: u64,
}

impl Default for FlashbotsConfig {
    fn default() -> Self {
        Self {
            endpoint: DEFAULT_FLASHBOTS_ENDPOINT.to_string(),
            timeout_ms: DEFAULT_FLASHBOTS_TIMEOUT_MS,
        }
    }
}

/// Flashbots `eth_callBundle` HTTP adapter.
///
/// Per DP-E11, the URL is kept in a private field and never emitted.
/// The `Debug` impl is hand-written and elides URL + transport state.
pub struct FlashbotsRelay {
    name: Arc<str>,
    endpoint: Url,
    http: reqwest::Client,
    timeout: Duration,
    /// P6-D D-D1: process-wide kill switch. Internally `Arc<AtomicBool>`
    /// so this clone shares state with the operator-flippable
    /// `AppHandle4::kill_switch()`. `submit_bundle` checks
    /// `is_active()` as its FIRST non-trivia statement (G10).
    kill_switch: KillSwitch,
}

impl std::fmt::Debug for FlashbotsRelay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // SECRET REDACTION (DP-E11): elide endpoint URL + http client.
        f.debug_struct("FlashbotsRelay")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl FlashbotsRelay {
    /// Construct a Flashbots adapter from the given config + kill switch.
    /// Returns `Err(RelaySimError::NotConfigured)` if the endpoint is
    /// not a parseable URL or the reqwest client cannot be built.
    ///
    /// P6-D D-D2: `kill_switch` is taken by value (boundary-spec §2.3 —
    /// `KillSwitch` already owns its `Arc<AtomicBool>` internally; no
    /// `Arc<KillSwitch>` / `Option<KillSwitch>`). Pass `kill_switch.clone()`
    /// from a shared instance to keep the adapter wired to the operator
    /// surface via `AppHandle4::kill_switch()`.
    pub fn new(cfg: FlashbotsConfig, kill_switch: KillSwitch) -> Result<Self, RelaySimError> {
        if cfg.endpoint.trim().is_empty() {
            return Err(RelaySimError::NotConfigured);
        }
        let endpoint = Url::parse(cfg.endpoint.trim()).map_err(|_| RelaySimError::NotConfigured)?;
        let timeout = Duration::from_millis(cfg.timeout_ms);
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|_| RelaySimError::NotConfigured)?;
        Ok(Self {
            name: Arc::from("flashbots"),
            endpoint,
            http,
            timeout,
            kill_switch,
        })
    }

    /// Direct accessor for the (private) timeout, used by integration
    /// tests that programmatically verify the construction.
    #[doc(hidden)]
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

#[async_trait]
impl RelaySimulator for FlashbotsRelay {
    async fn simulate_bundle(
        &self,
        req: RelaySimRequest,
    ) -> Result<RelaySimulationOutcome, RelaySimError> {
        // R-E2 fail-closed pre-check: NO network I/O, NO URL open,
        // NO HTTP-level logging when there are no signed txs to send.
        if req.txs.is_empty() {
            return Err(RelaySimError::UnsignedBundleUnavailable);
        }
        crate::call_bundle::call_eth_call_bundle(&self.http, &self.endpoint, &req).await
    }
}

#[async_trait]
impl BundleRelay for FlashbotsRelay {
    fn name(&self) -> &str {
        &self.name
    }

    /// P6B-E1 D-E1-3 Ok-path flip (single-adapter scope per v0.1 lock (I)).
    ///
    /// PRECEDENCE (G10 + boundary doc Section 4 G12):
    /// 1. Kill switch active -> `Err(KillSwitchActive)` (P5-D + P6-D §3).
    /// 2. Endpoint host NOT in `{"127.0.0.1", "localhost", "::1"}` ->
    ///    `Err(SubmitDisabledNonLocalhost)` (defense-in-depth with the
    ///    config-validate-time `ConfigError::LiveSendRequiresLocalhostEndpoint`).
    /// 3. HTTP POST `eth_sendBundle` to the localhost endpoint -> on
    ///    success returns `Ok(SubmissionReceipt)`; on transport /
    ///    parse failure returns `Err(SubmitHttpFailed)`.
    ///
    /// The G12 7-step pre-check chain (kill-switch + signer Ok +
    /// local-sim Ok + relay-sim Ok + comparator Match + bundle-byte
    /// equality + G13 inheritance) is the CALLER's responsibility
    /// (`submission_driver` in `crates/app`); this adapter performs
    /// only the localhost + kill-switch gate before the HTTP I/O.
    async fn submit_bundle(
        &self,
        bundle: &SignedBundle,
    ) -> Result<SubmissionReceipt, BundleRelayError> {
        // Step (1): kill switch precedence per G10.
        if self.kill_switch.is_active() {
            return Err(BundleRelayError::KillSwitchActive);
        }
        // Step (2): localhost-only defense-in-depth (R-D7 style
        // mechanical check). If the endpoint somehow bypasses the
        // config validate gate, fail-closed here before any HTTP I/O.
        if !crate::send_bundle::is_localhost_url(&self.endpoint) {
            return Err(BundleRelayError::SubmitDisabledNonLocalhost);
        }
        // Step (3): localhost HTTP POST `eth_sendBundle`. Body shape
        // matches the Flashbots-flavored JSON-RPC envelope; the local
        // wiremock relay responds with a `bundleHash` field.
        crate::send_bundle::submit_eth_send_bundle(
            &self.http,
            &self.endpoint,
            self.name.as_ref(),
            bundle,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Construct succeeds with the default endpoint.
    #[test]
    fn flashbots_new_default_succeeds() {
        let r = FlashbotsRelay::new(FlashbotsConfig::default(), KillSwitch::new(false))
            .expect("default config ok");
        assert_eq!(r.name(), "flashbots");
        assert_eq!(r.timeout().as_millis() as u64, DEFAULT_FLASHBOTS_TIMEOUT_MS);
    }

    /// Empty endpoint → NotConfigured.
    #[test]
    fn flashbots_new_empty_endpoint_rejected() {
        let cfg = FlashbotsConfig {
            endpoint: "   ".to_string(),
            timeout_ms: 1_000,
        };
        assert!(matches!(
            FlashbotsRelay::new(cfg, KillSwitch::new(false)),
            Err(RelaySimError::NotConfigured)
        ));
    }

    /// Garbage endpoint → NotConfigured.
    #[test]
    fn flashbots_new_garbage_endpoint_rejected() {
        let cfg = FlashbotsConfig {
            endpoint: "not a url at all".to_string(),
            timeout_ms: 1_000,
        };
        assert!(matches!(
            FlashbotsRelay::new(cfg, KillSwitch::new(false)),
            Err(RelaySimError::NotConfigured)
        ));
    }

    /// Debug elides the URL (DP-E11).
    #[test]
    fn flashbots_debug_elides_url_and_secret() {
        let cfg = FlashbotsConfig {
            endpoint: "http://example.com/?token=SECRETTOKEN".to_string(),
            timeout_ms: 1_000,
        };
        let r = FlashbotsRelay::new(cfg, KillSwitch::new(false)).expect("ctor ok");
        let dbg = format!("{r:?}");
        assert!(
            !dbg.contains("SECRETTOKEN"),
            "FlashbotsRelay Debug must elide URL secrets; got {dbg:?}"
        );
        assert!(
            !dbg.contains("example.com"),
            "FlashbotsRelay Debug must elide endpoint host; got {dbg:?}"
        );
    }

    /// RC-F-4 P6B-E1 D-T-E1-4: submit_bundle with the default
    /// (non-localhost) endpoint + kill switch INACTIVE returns
    /// `Err(SubmitDisabledNonLocalhost)`. The pre-P6B-E1 invariant
    /// was `Err(SubmitDisabled)`; P6B-E1's Ok-path flip narrows the
    /// adapter to `Ok(_)` for localhost only and routes every other
    /// endpoint to the defense-in-depth `SubmitDisabledNonLocalhost`.
    /// Config-validate-time `LiveSendRequiresLocalhostEndpoint`
    /// covers the boot path; this test covers the adapter-runtime
    /// path (R-D7 style mechanical check) so a non-localhost
    /// endpoint that somehow bypasses config validation still
    /// fails-closed BEFORE any HTTP I/O.
    #[tokio::test]
    async fn rc_f_4_submit_bundle_non_localhost_rejects_at_runtime() {
        let r = FlashbotsRelay::new(FlashbotsConfig::default(), KillSwitch::new(false))
            .expect("ctor ok");
        let dummy = SignedBundle {
            block_hash: [0u8; 32],
            state_block_number: 0,
            signed_txs: vec![vec![0xAB]],
            coinbase_recipient: alloy_primitives::Address::ZERO,
            coinbase_transfer_wei: alloy_primitives::U256::ZERO,
            validity_block_min: 0,
            validity_block_max: 0,
        };
        match r.submit_bundle(&dummy).await {
            Err(BundleRelayError::SubmitDisabledNonLocalhost) => {}
            other => panic!(
                "submit_bundle must return SubmitDisabledNonLocalhost for non-localhost endpoint; got {other:?}"
            ),
        }
    }
}
