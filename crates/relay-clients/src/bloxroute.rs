//! Phase 4 P4-E bloXroute `eth_callBundle` HTTP adapter.
//!
//! Per the user-approved P4-E execution note v0.6 §D-E2.
//! Read-only `eth_callBundle` only. NO `eth_sendBundle`, NO production
//! key material, NO signing, NO production submission.
//!
//! bloXroute requires an `Authorization` API key for production. In
//! P4-E the adapter accepts an `Option<String>` API key from config;
//! if `None`, `simulate_bundle` returns `Err(NotConfigured)` (R-E7).
//! API key is held only in a private adapter field and NEVER logged
//! (DP-E11; RC-COMMON-2 verifies redaction across Debug + error
//! strings + MismatchAbort.detail + tracing log + journal bytes).

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rust_lmax_mev_bundle_relay::{BundleRelay, BundleRelayError, SignedBundle, SubmissionReceipt};
use rust_lmax_mev_relay_sim::{
    RelaySimError, RelaySimRequest, RelaySimulationOutcome, RelaySimulator,
};
use serde::{Deserialize, Serialize};
use url::Url;

/// Default bloXroute relay endpoint.
pub const DEFAULT_BLOXROUTE_ENDPOINT: &str = "https://api.blxrbdn.com";

/// Default per-request timeout (ms). Same rationale as Flashbots.
pub const DEFAULT_BLOXROUTE_TIMEOUT_MS: u64 = 2_000;

/// bloXroute adapter configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BloxrouteConfig {
    pub endpoint: String,
    pub timeout_ms: u64,
    /// Optional API key. `None` → adapter fails closed with
    /// `RelaySimError::NotConfigured` on every `simulate_bundle`
    /// call (R-E7 / DP-E9 invariant: missing required config does
    /// NOT cause a network call).
    pub api_key: Option<String>,
}

impl Default for BloxrouteConfig {
    fn default() -> Self {
        Self {
            endpoint: DEFAULT_BLOXROUTE_ENDPOINT.to_string(),
            timeout_ms: DEFAULT_BLOXROUTE_TIMEOUT_MS,
            api_key: None,
        }
    }
}

/// bloXroute `eth_callBundle` HTTP adapter.
pub struct BloxrouteRelay {
    name: Arc<str>,
    endpoint: Url,
    /// Held inside a builder-configured reqwest client (with
    /// Authorization default header) when api_key is Some; None
    /// otherwise.
    http: Option<reqwest::Client>,
    timeout: Duration,
}

impl std::fmt::Debug for BloxrouteRelay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // SECRET REDACTION (DP-E11): elide endpoint URL + http +
        // api-key configuration state.
        f.debug_struct("BloxrouteRelay")
            .field("name", &self.name)
            .field("api_key_set", &self.http.is_some())
            .finish_non_exhaustive()
    }
}

impl BloxrouteRelay {
    /// Construct a bloXroute adapter from the given config.
    /// Returns `Err(RelaySimError::NotConfigured)` if the endpoint
    /// is not parseable. If `api_key` is `None`, the adapter is
    /// constructed but every `simulate_bundle` call returns
    /// `Err(NotConfigured)` (fail-closed).
    pub fn new(cfg: BloxrouteConfig) -> Result<Self, RelaySimError> {
        if cfg.endpoint.trim().is_empty() {
            return Err(RelaySimError::NotConfigured);
        }
        let endpoint = Url::parse(cfg.endpoint.trim()).map_err(|_| RelaySimError::NotConfigured)?;
        let timeout = Duration::from_millis(cfg.timeout_ms);
        let http = match cfg.api_key {
            Some(key) if !key.is_empty() => {
                let mut headers = reqwest::header::HeaderMap::new();
                let value = reqwest::header::HeaderValue::from_str(&key)
                    .map_err(|_| RelaySimError::NotConfigured)?;
                headers.insert(reqwest::header::AUTHORIZATION, value);
                let client = reqwest::Client::builder()
                    .timeout(timeout)
                    .default_headers(headers)
                    .build()
                    .map_err(|_| RelaySimError::NotConfigured)?;
                Some(client)
            }
            _ => None,
        };
        Ok(Self {
            name: Arc::from("bloxroute"),
            endpoint,
            http,
            timeout,
        })
    }

    #[doc(hidden)]
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

#[async_trait]
impl RelaySimulator for BloxrouteRelay {
    async fn simulate_bundle(
        &self,
        req: RelaySimRequest,
    ) -> Result<RelaySimulationOutcome, RelaySimError> {
        // R-E2 fail-closed pre-check: NO network I/O when there are
        // no signed txs to send.
        if req.txs.is_empty() {
            return Err(RelaySimError::UnsignedBundleUnavailable);
        }
        // R-E7 fail-closed: missing API key surfaces NotConfigured;
        // NO network call attempted.
        let http = self.http.as_ref().ok_or(RelaySimError::NotConfigured)?;
        crate::call_bundle::call_eth_call_bundle(http, &self.endpoint, &req).await
    }
}

#[async_trait]
impl BundleRelay for BloxrouteRelay {
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

    #[test]
    fn bloxroute_new_default_succeeds() {
        let r = BloxrouteRelay::new(BloxrouteConfig::default()).expect("default config ok");
        assert_eq!(r.name(), "bloxroute");
        assert_eq!(r.timeout().as_millis() as u64, DEFAULT_BLOXROUTE_TIMEOUT_MS);
    }

    #[test]
    fn bloxroute_new_empty_endpoint_rejected() {
        let cfg = BloxrouteConfig {
            endpoint: "  ".to_string(),
            ..Default::default()
        };
        assert!(matches!(
            BloxrouteRelay::new(cfg),
            Err(RelaySimError::NotConfigured)
        ));
    }

    #[test]
    fn bloxroute_debug_elides_url_and_secret() {
        let cfg = BloxrouteConfig {
            endpoint: "http://example.com/?token=SECRETTOKEN".to_string(),
            timeout_ms: 1_000,
            api_key: Some("SECRETKEY".to_string()),
        };
        let r = BloxrouteRelay::new(cfg).expect("ctor ok");
        let dbg = format!("{r:?}");
        assert!(!dbg.contains("SECRETTOKEN"));
        assert!(!dbg.contains("SECRETKEY"));
        assert!(!dbg.contains("example.com"));
    }

    /// R-E7: missing API key → simulate_bundle returns
    /// NotConfigured WITHOUT issuing a network call.
    #[tokio::test]
    async fn bloxroute_missing_api_key_is_not_configured() {
        let cfg = BloxrouteConfig {
            api_key: None,
            ..Default::default()
        };
        let r = BloxrouteRelay::new(cfg).expect("ctor ok");
        let req = RelaySimRequest {
            block_hash: alloy_primitives::B256::from([0u8; 32]),
            state_block_number: 0,
            txs: vec![vec![0xAB]],
        };
        match r.simulate_bundle(req).await {
            Err(RelaySimError::NotConfigured) => {}
            other => panic!("expected NotConfigured, got {other:?}"),
        }
    }

    /// RC-B-4: submit_bundle always returns Err(SubmitDisabled).
    #[tokio::test]
    async fn rc_b_4_submit_bundle_always_disabled() {
        let r = BloxrouteRelay::new(BloxrouteConfig::default()).expect("ctor ok");
        let dummy = SignedBundle {
            block_hash: [0u8; 32],
            state_block_number: 0,
            signed_txs: vec![vec![0x01]],
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
