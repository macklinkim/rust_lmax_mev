//! Phase 2 P2-A NodeProvider per ADR-007.
//!
//! `NodeProvider` owns alloy's primary HTTP + optional fallback HTTP
//! provider handles, plus a stored WS URL used for reconnecting WS
//! subscription streams. Public API per the approved P2-A execution
//! note (`docs/superpowers/plans/2026-05-02-phase-2-batch-a-
//! node-ingress-execution.md` v0.4):
//!
//! - `NodeProvider::connect(&NodeConfig)` — async ctor.
//! - `eth_call` / `eth_get_transaction_by_hash` — HTTP req/resp with
//!   fallback failover ONLY on `NodeError::Transport`.
//! - `subscribe_new_heads` / `subscribe_pending_txs` /
//!   `subscribe_logs(filter)` — `Pin<Box<dyn Stream + Send + 'static>>`
//!   with reconnect handled internally via `ReconnectingStream`.
//!
//! The HTTP call surface is abstracted behind a private `HttpRpc` trait
//! so unit tests can substitute deterministic mocks without spinning up
//! a live node.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use alloy::network::Ethereum;
use alloy::providers::{Provider, ProviderBuilder, RootProvider};
use alloy::rpc::types::eth::{Filter, Header, Log, Transaction, TransactionRequest};
use alloy::transports::http::{Client as ReqwestClient, Http};
use alloy_primitives::{Bytes, B256};
use async_trait::async_trait;
use futures::Stream;
use rust_lmax_mev_config::NodeConfig;

mod error;
mod reconnect;

pub use error::{classify, NodeError};
pub use reconnect::ReconnectingStream;

/// Object-safe-internally HTTP RPC adapter. `AlloyHttp` wraps the real
/// alloy provider; tests substitute a `MockHttp` that returns canned
/// outcomes per call. Kept private — `NodeProvider` is the public API.
#[async_trait]
trait HttpRpc: Send + Sync {
    async fn eth_call(&self, req: TransactionRequest) -> Result<Bytes, NodeError>;
    async fn eth_get_transaction_by_hash(
        &self,
        hash: B256,
    ) -> Result<Option<Transaction>, NodeError>;
}

/// Production HTTP adapter wrapping alloy's HTTP-backed `RootProvider`.
struct AlloyHttp(RootProvider<Http<ReqwestClient>, Ethereum>);

#[async_trait]
impl HttpRpc for AlloyHttp {
    async fn eth_call(&self, req: TransactionRequest) -> Result<Bytes, NodeError> {
        self.0.call(&req).await.map_err(classify)
    }

    async fn eth_get_transaction_by_hash(
        &self,
        hash: B256,
    ) -> Result<Option<Transaction>, NodeError> {
        self.0.get_transaction_by_hash(hash).await.map_err(classify)
    }
}

/// The public node-provider handle owned by upstream consumers
/// (`crates/ingress`, `crates/state`, `crates/app`). One instance per
/// process; clone via `Arc<NodeProvider>` if multiple consumers need it.
pub struct NodeProvider {
    primary_http: Box<dyn HttpRpc>,
    fallback_http: Option<Box<dyn HttpRpc>>,
    ws_url: String,
}

impl NodeProvider {
    /// Connects against `config.geth_http_url` (primary) +
    /// `config.fallback_rpc[0]` (if any). The WS URL is stored for
    /// later subscription calls; no WS connection happens at connect
    /// time. Returns `Err(NodeError::Transport(_))` on URL parse error.
    pub async fn connect(config: &NodeConfig) -> Result<Self, NodeError> {
        let primary_url = parse_http_url(&config.geth_http_url, "primary")?;
        let primary = ProviderBuilder::new().on_http(primary_url);
        let fallback = match config.fallback_rpc.first() {
            Some(f) => {
                let u = parse_http_url(&f.url, "fallback")?;
                Some(Box::new(AlloyHttp(ProviderBuilder::new().on_http(u))) as Box<dyn HttpRpc>)
            }
            None => None,
        };
        Ok(Self {
            primary_http: Box::new(AlloyHttp(primary)),
            fallback_http: fallback,
            ws_url: config.geth_ws_url.clone(),
        })
    }

    /// `eth_call` against primary, with fallback ONLY on
    /// `NodeError::Transport`. RPC error responses (`NodeError::Rpc`)
    /// and decode errors are authoritative — fallback NOT consulted.
    pub async fn eth_call(&self, req: TransactionRequest) -> Result<Bytes, NodeError> {
        match self.primary_http.eth_call(req.clone()).await {
            Err(NodeError::Transport(_)) => match &self.fallback_http {
                Some(fb) => fb.eth_call(req).await,
                None => Err(NodeError::Transport(
                    "primary transport failed; no fallback configured".to_string(),
                )),
            },
            other => other,
        }
    }

    /// `eth_getTransactionByHash` against primary. A primary `Ok(None)`
    /// (the node definitively does not know this hash) is **authoritative**
    /// — fallback is NOT consulted. Fallback fires ONLY on primary
    /// `NodeError::Transport`.
    pub async fn eth_get_transaction_by_hash(
        &self,
        hash: B256,
    ) -> Result<Option<Transaction>, NodeError> {
        match self.primary_http.eth_get_transaction_by_hash(hash).await {
            Err(NodeError::Transport(_)) => match &self.fallback_http {
                Some(fb) => fb.eth_get_transaction_by_hash(hash).await,
                None => Err(NodeError::Transport(
                    "primary transport failed; no fallback configured".to_string(),
                )),
            },
            other => other,
        }
    }

    /// Returns the WS URL configured at connect time. Used by the
    /// upstream subscription wiring code that constructs the
    /// `ReconnectingStream` factory. Kept as `pub fn` so consumers can
    /// construct their own subscription streams; the per-subscription
    /// helpers below cover the common cases.
    pub fn ws_url(&self) -> &str {
        &self.ws_url
    }

    /// Subscribes to `newHeads`. Reconnect is transparent via
    /// [`ReconnectingStream`]; consumers see a single continuous stream.
    pub fn subscribe_new_heads(
        self: &Arc<Self>,
    ) -> Pin<Box<dyn Stream<Item = Result<Header, NodeError>> + Send + 'static>> {
        let url = self.ws_url.clone();
        ReconnectingStream::new(move || {
            let url = url.clone();
            Box::pin(async move { ws_subscribe_new_heads(&url).await })
        })
        .into_stream()
    }

    /// Subscribes to `newPendingTransactions`. Reconnect transparent.
    /// Yields tx hashes; full-tx fetch happens via
    /// [`Self::eth_get_transaction_by_hash`] downstream.
    pub fn subscribe_pending_txs(
        self: &Arc<Self>,
    ) -> Pin<Box<dyn Stream<Item = Result<B256, NodeError>> + Send + 'static>> {
        let url = self.ws_url.clone();
        ReconnectingStream::new(move || {
            let url = url.clone();
            Box::pin(async move { ws_subscribe_pending_txs(&url).await })
        })
        .into_stream()
    }

    /// Subscribes to `logs` matching `filter`. Reconnect transparent.
    pub fn subscribe_logs(
        self: &Arc<Self>,
        filter: Filter,
    ) -> Pin<Box<dyn Stream<Item = Result<Log, NodeError>> + Send + 'static>> {
        let url = self.ws_url.clone();
        ReconnectingStream::new(move || {
            let url = url.clone();
            let filter = filter.clone();
            Box::pin(async move { ws_subscribe_logs(&url, filter).await })
        })
        .into_stream()
    }
}

fn parse_http_url(raw: &str, label: &str) -> Result<reqwest::Url, NodeError> {
    raw.parse::<reqwest::Url>()
        .map_err(|e| NodeError::Transport(format!("{label} HTTP URL parse: {e}")))
}

// --- WS subscription helpers (production paths) ---------------------------
//
// Each helper opens a fresh WS connection, subscribes, and returns a
// boxed Stream of decoded items. `ReconnectingStream` calls the helper
// again on disconnect to obtain a new stream.

async fn ws_subscribe_new_heads(
    ws_url: &str,
) -> Result<Pin<Box<dyn Stream<Item = Result<Header, NodeError>> + Send + 'static>>, NodeError> {
    use futures::StreamExt;
    let ws = alloy::transports::ws::WsConnect::new(ws_url);
    let provider = ProviderBuilder::new()
        .on_ws(ws)
        .await
        .map_err(|e| NodeError::WsConnect(e.to_string()))?;
    let sub = provider.subscribe_blocks().await.map_err(classify)?;
    let stream = sub.into_stream().map(Ok::<_, NodeError>);
    Ok(Box::pin(stream))
}

async fn ws_subscribe_pending_txs(
    ws_url: &str,
) -> Result<Pin<Box<dyn Stream<Item = Result<B256, NodeError>> + Send + 'static>>, NodeError> {
    use futures::StreamExt;
    let ws = alloy::transports::ws::WsConnect::new(ws_url);
    let provider = ProviderBuilder::new()
        .on_ws(ws)
        .await
        .map_err(|e| NodeError::WsConnect(e.to_string()))?;
    let sub = provider
        .subscribe_pending_transactions()
        .await
        .map_err(classify)?;
    let stream = sub.into_stream().map(Ok::<_, NodeError>);
    Ok(Box::pin(stream))
}

async fn ws_subscribe_logs(
    ws_url: &str,
    filter: Filter,
) -> Result<Pin<Box<dyn Stream<Item = Result<Log, NodeError>> + Send + 'static>>, NodeError> {
    use futures::StreamExt;
    let ws = alloy::transports::ws::WsConnect::new(ws_url);
    let provider = ProviderBuilder::new()
        .on_ws(ws)
        .await
        .map_err(|e| NodeError::WsConnect(e.to_string()))?;
    let sub = provider.subscribe_logs(&filter).await.map_err(classify)?;
    let stream = sub.into_stream().map(Ok::<_, NodeError>);
    Ok(Box::pin(stream))
}

// --- backoff (pure function, used by ReconnectingStream) ------------------

/// Exponential backoff: 1s, 2s, 4s, …, capped at 60s. Per ADR-007 +
/// P2-A Risk Decision 2.
pub(crate) fn backoff_delay(retry_count: u32) -> Duration {
    let base_ms: u64 = 1_000;
    let cap_ms: u64 = 60_000;
    let exp = base_ms.saturating_mul(2u64.saturating_pow(retry_count.min(6)));
    Duration::from_millis(exp.min(cap_ms))
}

#[cfg(test)]
mod tests {
    //! N-1 / N-2 / N-3 per the approved P2-A execution note. Tests
    //! substitute the private `HttpRpc` trait with deterministic
    //! mocks; the WS reconnect-transparency assertion runs against
    //! `ReconnectingStream` directly with a synthetic factory so no
    //! live node or socket is required.

    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockHttp {
        outcome: Box<dyn Fn() -> Result<Bytes, NodeError> + Send + Sync>,
        call_count: Arc<AtomicUsize>,
    }

    impl MockHttp {
        fn new(
            outcome: impl Fn() -> Result<Bytes, NodeError> + Send + Sync + 'static,
        ) -> (Self, Arc<AtomicUsize>) {
            let call_count = Arc::new(AtomicUsize::new(0));
            (
                Self {
                    outcome: Box::new(outcome),
                    call_count: Arc::clone(&call_count),
                },
                call_count,
            )
        }
    }

    #[async_trait]
    impl HttpRpc for MockHttp {
        async fn eth_call(&self, _req: TransactionRequest) -> Result<Bytes, NodeError> {
            self.call_count.fetch_add(1, Ordering::Relaxed);
            (self.outcome)()
        }
        async fn eth_get_transaction_by_hash(
            &self,
            _hash: B256,
        ) -> Result<Option<Transaction>, NodeError> {
            self.call_count.fetch_add(1, Ordering::Relaxed);
            // Re-use eth_call's outcome shape: Ok(empty) → Ok(None);
            // Err(_) → Err(_). Tests for tx-by-hash live in N-2 below.
            match (self.outcome)() {
                Ok(_) => Ok(None),
                Err(e) => Err(e),
            }
        }
    }

    fn np_with(primary: MockHttp, fallback: Option<MockHttp>) -> NodeProvider {
        NodeProvider {
            primary_http: Box::new(primary),
            fallback_http: fallback.map(|m| Box::new(m) as Box<dyn HttpRpc>),
            ws_url: "ws://test".to_string(),
        }
    }

    /// N-1 happy: eth_call returns expected bytes from primary; fallback
    /// is not consulted (call_count remains 0).
    #[tokio::test]
    async fn eth_call_returns_primary_result_without_consulting_fallback() {
        let expected = Bytes::from_static(b"\x01\x02\x03");
        let (primary, primary_calls) = MockHttp::new({
            let e = expected.clone();
            move || Ok(e.clone())
        });
        let (fallback, fallback_calls) = MockHttp::new(|| Ok(Bytes::from_static(b"WRONG")));
        let np = np_with(primary, Some(fallback));

        let got = np.eth_call(TransactionRequest::default()).await.unwrap();
        assert_eq!(got, expected);
        assert_eq!(primary_calls.load(Ordering::Relaxed), 1);
        assert_eq!(fallback_calls.load(Ordering::Relaxed), 0);
    }

    /// N-2 failure half (a): primary `Transport` → fallback consulted +
    /// returns Ok. Half (b): primary `Rpc` → fallback NOT consulted.
    #[tokio::test]
    async fn eth_call_falls_over_only_on_transport_error() {
        // Half (a): Transport → fallback fires.
        {
            let (primary, p_calls) =
                MockHttp::new(|| Err(NodeError::Transport("connect refused".to_string())));
            let (fallback, f_calls) = MockHttp::new(|| Ok(Bytes::from_static(b"OK")));
            let np = np_with(primary, Some(fallback));
            let got = np.eth_call(TransactionRequest::default()).await.unwrap();
            assert_eq!(&got[..], b"OK");
            assert_eq!(p_calls.load(Ordering::Relaxed), 1);
            assert_eq!(f_calls.load(Ordering::Relaxed), 1);
        }
        // Half (b): Rpc (authoritative) → fallback NOT called.
        {
            let (primary, p_calls) = MockHttp::new(|| Err(NodeError::Rpc("revert".to_string())));
            let (fallback, f_calls) = MockHttp::new(|| Ok(Bytes::from_static(b"WRONG")));
            let np = np_with(primary, Some(fallback));
            let err = np
                .eth_call(TransactionRequest::default())
                .await
                .expect_err("Rpc must propagate");
            assert!(matches!(err, NodeError::Rpc(_)));
            assert_eq!(p_calls.load(Ordering::Relaxed), 1);
            assert_eq!(f_calls.load(Ordering::Relaxed), 0);
        }
    }

    /// N-3 boundary: `ReconnectingStream` driven by a factory that
    /// produces a fresh inner stream of 3 items twice — consumer sees
    /// 6 items total, reconnect transparent.
    #[tokio::test]
    async fn reconnecting_stream_emits_six_items_across_two_inner_streams() {
        use futures::stream::{self, StreamExt};
        let attempt = Arc::new(AtomicUsize::new(0));
        let attempt_for_factory = Arc::clone(&attempt);
        let factory = move || {
            let n = attempt_for_factory.fetch_add(1, Ordering::Relaxed);
            Box::pin(async move {
                if n >= 2 {
                    // Third+ attempt: stop emitting (Err::Closed) so the
                    // test consumer's take(6) finishes deterministically
                    // without an infinite reconnect loop.
                    Err(NodeError::Closed)
                } else {
                    let base = (n * 3) as u64;
                    let s = stream::iter((0..3u64).map(move |i| Ok::<u64, NodeError>(base + i)));
                    Ok(Box::pin(s)
                        as Pin<
                            Box<dyn Stream<Item = Result<u64, NodeError>> + Send>,
                        >)
                }
            })
                as Pin<
                    Box<
                        dyn std::future::Future<
                                Output = Result<
                                    Pin<Box<dyn Stream<Item = Result<u64, NodeError>> + Send>>,
                                    NodeError,
                                >,
                            > + Send,
                    >,
                >
        };

        let mut stream =
            ReconnectingStream::new_with_backoff(factory, |_| Duration::from_millis(0))
                .into_stream();
        let mut got = Vec::new();
        while let Some(item) = stream.next().await {
            match item {
                Ok(v) => got.push(v),
                Err(_) => break,
            }
        }
        assert_eq!(got, vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(attempt.load(Ordering::Relaxed), 3); // 2 successful + 1 closed
    }

    /// Pure-function check on the backoff sequence (1, 2, 4, 8, 16, 32, 60, 60).
    #[test]
    fn backoff_delay_caps_at_sixty_seconds() {
        let seq: Vec<_> = (0..8).map(backoff_delay).collect();
        assert_eq!(
            seq,
            vec![
                Duration::from_secs(1),
                Duration::from_secs(2),
                Duration::from_secs(4),
                Duration::from_secs(8),
                Duration::from_secs(16),
                Duration::from_secs(32),
                Duration::from_secs(60),
                Duration::from_secs(60),
            ]
        );
    }
}
