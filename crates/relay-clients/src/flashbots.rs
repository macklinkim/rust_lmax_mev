//! Phase 4 P4-E Flashbots `eth_callBundle` HTTP adapter.
//!
//! Per the user-approved P4-E execution note v0.6 §D-E2.
//! Read-only `eth_callBundle` only. NO `eth_sendBundle`, NO funded
//! key, NO signing, NO production submission.
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
use rust_lmax_mev_bundle_relay::{BundleRelay, BundleRelayError, SignedBundle, SubmissionReceipt};
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
    /// Construct a Flashbots adapter from the given config.
    /// Returns `Err(RelaySimError::NotConfigured)` if the endpoint is
    /// not a parseable URL or the reqwest client cannot be built.
    pub fn new(cfg: FlashbotsConfig) -> Result<Self, RelaySimError> {
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

    /// HARD INVARIANT (DP-E1): always `Err(SubmitDisabled)`.
    async fn submit_bundle(
        &self,
        _bundle: &SignedBundle,
    ) -> Result<SubmissionReceipt, BundleRelayError> {
        Err(BundleRelayError::SubmitDisabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Construct succeeds with the default endpoint.
    #[test]
    fn flashbots_new_default_succeeds() {
        let r = FlashbotsRelay::new(FlashbotsConfig::default()).expect("default config ok");
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
            FlashbotsRelay::new(cfg),
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
            FlashbotsRelay::new(cfg),
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
        let r = FlashbotsRelay::new(cfg).expect("ctor ok");
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

    /// RC-F-4: submit_bundle always returns Err(SubmitDisabled).
    #[tokio::test]
    async fn rc_f_4_submit_bundle_always_disabled() {
        let r = FlashbotsRelay::new(FlashbotsConfig::default()).expect("ctor ok");
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
            Err(BundleRelayError::SubmitDisabled) => {}
            other => panic!("submit_bundle must return SubmitDisabled; got {other:?}"),
        }
    }
}

