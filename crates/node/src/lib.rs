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

use alloy::eips::BlockId;
use alloy::network::Ethereum;
use alloy::providers::{Provider, ProviderBuilder, RootProvider};
use alloy::rpc::types::eth::{Filter, Header, Log, Transaction, TransactionRequest};
use alloy::transports::http::{Client as ReqwestClient, Http};
use alloy::rpc::types::eth::EIP1186AccountProofResponse;
use alloy_primitives::{Address, Bytes, B256, U256};
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
    async fn eth_call_at_block(
        &self,
        req: TransactionRequest,
        block_id: BlockId,
    ) -> Result<Bytes, NodeError>;
    async fn eth_get_transaction_by_hash(
        &self,
        hash: B256,
    ) -> Result<Option<Transaction>, NodeError>;

    // Phase 4 P4-A archive-mode methods. Default-stubbed (additive) so
    // existing P2-A test mocks compile unchanged; AlloyHttp overrides
    // with real alloy::providers::Provider calls. NodeProvider
    // dispatches archive calls to its archive_http handle (when
    // configured) and NEVER falls back to primary (DP-1).
    async fn eth_get_proof(
        &self,
        _address: Address,
        _slots: Vec<B256>,
        _block_id: BlockId,
    ) -> Result<EIP1186AccountProofResponse, NodeError> {
        Err(NodeError::Rpc(
            "archive method `eth_get_proof` not implemented for this transport".to_string(),
        ))
    }

    async fn eth_get_storage_at(
        &self,
        _address: Address,
        _slot: U256,
        _block_id: BlockId,
    ) -> Result<B256, NodeError> {
        Err(NodeError::Rpc(
            "archive method `eth_get_storage_at` not implemented for this transport".to_string(),
        ))
    }

    async fn eth_get_code(
        &self,
        _address: Address,
        _block_id: BlockId,
    ) -> Result<Bytes, NodeError> {
        Err(NodeError::Rpc(
            "archive method `eth_get_code` not implemented for this transport".to_string(),
        ))
    }
}

/// Production HTTP adapter wrapping alloy's HTTP-backed `RootProvider`.
struct AlloyHttp(RootProvider<Http<ReqwestClient>, Ethereum>);

#[async_trait]
impl HttpRpc for AlloyHttp {
    async fn eth_call(&self, req: TransactionRequest) -> Result<Bytes, NodeError> {
        self.0.call(&req).await.map_err(classify)
    }

    async fn eth_call_at_block(
        &self,
        req: TransactionRequest,
        block_id: BlockId,
    ) -> Result<Bytes, NodeError> {
        self.0.call(&req).block(block_id).await.map_err(classify)
    }

    async fn eth_get_transaction_by_hash(
        &self,
        hash: B256,
    ) -> Result<Option<Transaction>, NodeError> {
        self.0.get_transaction_by_hash(hash).await.map_err(classify)
    }

    async fn eth_get_proof(
        &self,
        address: Address,
        slots: Vec<B256>,
        block_id: BlockId,
    ) -> Result<EIP1186AccountProofResponse, NodeError> {
        self.0
            .get_proof(address, slots)
            .block_id(block_id)
            .await
            .map_err(classify)
    }

    async fn eth_get_storage_at(
        &self,
        address: Address,
        slot: U256,
        block_id: BlockId,
    ) -> Result<B256, NodeError> {
        self.0
            .get_storage_at(address, slot)
            .block_id(block_id)
            .await
            .map(B256::from)
            .map_err(classify)
    }

    async fn eth_get_code(
        &self,
        address: Address,
        block_id: BlockId,
    ) -> Result<Bytes, NodeError> {
        self.0
            .get_code_at(address)
            .block_id(block_id)
            .await
            .map_err(classify)
    }
}

/// The public node-provider handle owned by upstream consumers
/// (`crates/ingress`, `crates/state`, `crates/app`). One instance per
/// process; clone via `Arc<NodeProvider>` if multiple consumers need it.
///
/// Phase 4 P4-A additive: `archive_http` is `Some` only when
/// `NodeConfig.archive_rpc` is configured. Archive-mode methods
/// (`eth_get_proof` / `eth_get_storage_at` / `eth_get_code`) dispatch
/// to `archive_http` and return `Err(NodeError::ArchiveNotConfigured)`
/// when it is `None`. They NEVER fall back to `primary_http` (DP-1
/// no-fallback policy: silent fallback to a non-archive node would
/// produce wrong historical answers).
pub struct NodeProvider {
    primary_http: Box<dyn HttpRpc>,
    fallback_http: Option<Box<dyn HttpRpc>>,
    archive_http: Option<Box<dyn HttpRpc>>,
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
        // Phase 4 P4-A: archive endpoint is OPTIONAL. None default per
        // Q8 fail-closed hardening; archive methods Err(ArchiveNotConfigured)
        // when this is None. URL parsing is the only validation here —
        // operational failures (provider unreachable, rate-limited, etc.)
        // surface lazily on the first archive call (alloy lazy connect).
        let archive = match &config.archive_rpc {
            Some(a) => {
                let u = parse_http_url(&a.url, "archive")?;
                Some(Box::new(AlloyHttp(ProviderBuilder::new().on_http(u))) as Box<dyn HttpRpc>)
            }
            None => None,
        };
        Ok(Self {
            primary_http: Box::new(AlloyHttp(primary)),
            fallback_http: fallback,
            archive_http: archive,
            ws_url: config.geth_ws_url.clone(),
        })
    }

    /// `eth_call` against primary with the v0.4 retry-then-fallback
    /// policy: on first `Err(NodeError::Transport(_))`, retry primary
    /// once. Only if the retry ALSO returns `Transport` is the fallback
    /// consulted. Non-`Transport` retry outcomes (`Ok` / `Rpc` /
    /// `Decode`) are authoritative and returned directly. RPC error
    /// responses on the first call are also authoritative — fallback
    /// NOT consulted.
    pub async fn eth_call(&self, req: TransactionRequest) -> Result<Bytes, NodeError> {
        match self.primary_http.eth_call(req.clone()).await {
            Err(NodeError::Transport(_)) => {
                // Single primary retry per v0.4 Risk Decision 3.
                match self.primary_http.eth_call(req.clone()).await {
                    Err(NodeError::Transport(_)) => match &self.fallback_http {
                        Some(fb) => fb.eth_call(req).await,
                        None => Err(NodeError::Transport(
                            "primary transport failed (after retry); no fallback configured"
                                .to_string(),
                        )),
                    },
                    other => other,
                }
            }
            other => other,
        }
    }

    /// `eth_call` pinned at `block_id` with the same retry-then-fallback
    /// policy as [`Self::eth_call`]. Phase 2 P2-B additive API per the
    /// approved P2-B execution note v0.4 (Risk Decision 2): `crates/state`
    /// needs block-hash-pinned reads for State Correctness Gate
    /// determinism, which the prepending-`Pending`-tag `eth_call` could
    /// not deliver. `eth_call(req)` semantics are unchanged.
    pub async fn eth_call_at_block(
        &self,
        req: TransactionRequest,
        block_id: BlockId,
    ) -> Result<Bytes, NodeError> {
        match self
            .primary_http
            .eth_call_at_block(req.clone(), block_id)
            .await
        {
            Err(NodeError::Transport(_)) => {
                match self
                    .primary_http
                    .eth_call_at_block(req.clone(), block_id)
                    .await
                {
                    Err(NodeError::Transport(_)) => match &self.fallback_http {
                        Some(fb) => fb.eth_call_at_block(req, block_id).await,
                        None => Err(NodeError::Transport(
                            "primary transport failed (after retry); no fallback configured"
                                .to_string(),
                        )),
                    },
                    other => other,
                }
            }
            other => other,
        }
    }

    /// `eth_getTransactionByHash` against primary with the v0.4
    /// retry-then-fallback policy (same as [`Self::eth_call`]). A
    /// primary `Ok(None)` (the node definitively does not know this
    /// hash) is **authoritative** — neither retry nor fallback is
    /// consulted.
    pub async fn eth_get_transaction_by_hash(
        &self,
        hash: B256,
    ) -> Result<Option<Transaction>, NodeError> {
        match self.primary_http.eth_get_transaction_by_hash(hash).await {
            Err(NodeError::Transport(_)) => {
                match self.primary_http.eth_get_transaction_by_hash(hash).await {
                    Err(NodeError::Transport(_)) => match &self.fallback_http {
                        Some(fb) => fb.eth_get_transaction_by_hash(hash).await,
                        None => Err(NodeError::Transport(
                            "primary transport failed (after retry); no fallback configured"
                                .to_string(),
                        )),
                    },
                    other => other,
                }
            }
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

    /// Phase 4 P4-A archive method — pinned at `block_id` proof for
    /// `address` over the given storage `slots`. Returns
    /// `Err(NodeError::ArchiveNotConfigured)` if `NodeConfig.archive_rpc`
    /// was not set at `connect()` time. **NEVER falls back to primary
    /// HTTP** (DP-1; silent fallback to a non-archive node would
    /// produce wrong historical answers).
    pub async fn eth_get_proof(
        &self,
        address: Address,
        slots: Vec<B256>,
        block_id: BlockId,
    ) -> Result<EIP1186AccountProofResponse, NodeError> {
        match &self.archive_http {
            Some(h) => h.eth_get_proof(address, slots, block_id).await,
            None => Err(NodeError::ArchiveNotConfigured),
        }
    }

    /// Phase 4 P4-A archive method — pinned at `block_id` storage read
    /// for (`address`, `slot`). Same no-fallback policy as `eth_get_proof`.
    pub async fn eth_get_storage_at(
        &self,
        address: Address,
        slot: U256,
        block_id: BlockId,
    ) -> Result<B256, NodeError> {
        match &self.archive_http {
            Some(h) => h.eth_get_storage_at(address, slot, block_id).await,
            None => Err(NodeError::ArchiveNotConfigured),
        }
    }

    /// Phase 4 P4-A archive method — pinned at `block_id` deployed
    /// bytecode at `address`. Same no-fallback policy (DP-1 covers all
    /// three methods including `eth_getCode` per Codex 21:37:53 explicit
    /// rejection of the special-case fallback).
    pub async fn eth_get_code(
        &self,
        address: Address,
        block_id: BlockId,
    ) -> Result<Bytes, NodeError> {
        match &self.archive_http {
            Some(h) => h.eth_get_code(address, block_id).await,
            None => Err(NodeError::ArchiveNotConfigured),
        }
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

    /// Per-call deterministic outcome closure. Receives the 0-based
    /// call index so tests can express sequences like "first call
    /// Transport, second call Rpc" needed to exercise the v0.4
    /// retry-then-fallback policy.
    struct MockHttp {
        outcome: Box<dyn Fn(usize) -> Result<Bytes, NodeError> + Send + Sync>,
        call_count: Arc<AtomicUsize>,
    }

    impl MockHttp {
        fn new(
            outcome: impl Fn(usize) -> Result<Bytes, NodeError> + Send + Sync + 'static,
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
            let n = self.call_count.fetch_add(1, Ordering::Relaxed);
            (self.outcome)(n)
        }
        async fn eth_call_at_block(
            &self,
            _req: TransactionRequest,
            _block_id: BlockId,
        ) -> Result<Bytes, NodeError> {
            let n = self.call_count.fetch_add(1, Ordering::Relaxed);
            (self.outcome)(n)
        }
        async fn eth_get_transaction_by_hash(
            &self,
            _hash: B256,
        ) -> Result<Option<Transaction>, NodeError> {
            let n = self.call_count.fetch_add(1, Ordering::Relaxed);
            // Re-use eth_call's outcome shape: Ok(empty) → Ok(None);
            // Err(_) → Err(_). Tests for tx-by-hash live in N-2 below.
            match (self.outcome)(n) {
                Ok(_) => Ok(None),
                Err(e) => Err(e),
            }
        }
    }

    fn np_with(primary: MockHttp, fallback: Option<MockHttp>) -> NodeProvider {
        NodeProvider {
            primary_http: Box::new(primary),
            fallback_http: fallback.map(|m| Box::new(m) as Box<dyn HttpRpc>),
            archive_http: None,
            ws_url: "ws://test".to_string(),
        }
    }

    fn np_with_archive(
        primary: MockHttp,
        fallback: Option<MockHttp>,
        archive: Option<Box<dyn HttpRpc>>,
    ) -> NodeProvider {
        NodeProvider {
            primary_http: Box::new(primary),
            fallback_http: fallback.map(|m| Box::new(m) as Box<dyn HttpRpc>),
            archive_http: archive,
            ws_url: "ws://test".to_string(),
        }
    }

    /// N-1 happy: eth_call returns expected bytes from primary; fallback
    /// is not consulted (call_count remains 0). No retry on success.
    #[tokio::test]
    async fn eth_call_returns_primary_result_without_consulting_fallback() {
        let expected = Bytes::from_static(b"\x01\x02\x03");
        let (primary, primary_calls) = MockHttp::new({
            let e = expected.clone();
            move |_| Ok(e.clone())
        });
        let (fallback, fallback_calls) = MockHttp::new(|_| Ok(Bytes::from_static(b"WRONG")));
        let np = np_with(primary, Some(fallback));

        let got = np.eth_call(TransactionRequest::default()).await.unwrap();
        assert_eq!(got, expected);
        assert_eq!(primary_calls.load(Ordering::Relaxed), 1);
        assert_eq!(fallback_calls.load(Ordering::Relaxed), 0);
    }

    /// N-1b (P2-B additive API): `eth_call_at_block` returns expected
    /// bytes from primary; fallback NOT consulted. Same retry-then-
    /// fallback policy as `eth_call` (verified in companion N-2-style
    /// case below by setting outcome to Transport-then-Ok).
    #[tokio::test]
    async fn eth_call_at_block_pins_block_id_and_returns_primary_result() {
        let expected = Bytes::from_static(b"\xCA\xFE");
        let (primary, p_calls) = MockHttp::new({
            let e = expected.clone();
            move |_| Ok(e.clone())
        });
        let (fallback, f_calls) = MockHttp::new(|_| Ok(Bytes::from_static(b"WRONG")));
        let np = np_with(primary, Some(fallback));

        let block_hash = B256::from([0xAB; 32]);
        let got = np
            .eth_call_at_block(
                TransactionRequest::default(),
                BlockId::Hash(block_hash.into()),
            )
            .await
            .unwrap();
        assert_eq!(got, expected);
        assert_eq!(p_calls.load(Ordering::Relaxed), 1);
        assert_eq!(f_calls.load(Ordering::Relaxed), 0);

        // Retry-then-fallback symmetry: Transport then Ok on primary →
        // p_calls == 2, f_calls == 0, returns Ok from retry.
        let (primary2, p2) = MockHttp::new(|n| {
            if n == 0 {
                Err(NodeError::Transport("transient".to_string()))
            } else {
                Ok(Bytes::from_static(b"RETRY-OK"))
            }
        });
        let (fallback2, f2) = MockHttp::new(|_| Ok(Bytes::from_static(b"WRONG")));
        let np2 = np_with(primary2, Some(fallback2));
        let got2 = np2
            .eth_call_at_block(
                TransactionRequest::default(),
                BlockId::Hash(block_hash.into()),
            )
            .await
            .unwrap();
        assert_eq!(&got2[..], b"RETRY-OK");
        assert_eq!(p2.load(Ordering::Relaxed), 2, "primary retried once");
        assert_eq!(f2.load(Ordering::Relaxed), 0, "fallback NOT consulted");
    }

    /// N-2 failure: v0.4 retry-then-fallback policy.
    /// Three cases:
    /// - (a) Primary returns `Transport` on BOTH calls → primary called
    ///   twice, then fallback consulted; final result is fallback's
    ///   `Ok`. p_calls == 2, f_calls == 1.
    /// - (b) Primary returns `Transport` then `Rpc` on retry → primary
    ///   called twice, fallback NOT called (retry's non-Transport
    ///   outcome is authoritative). p_calls == 2, f_calls == 0.
    /// - (c) Primary returns `Rpc` on first call → no retry, fallback
    ///   NOT called. p_calls == 1, f_calls == 0.
    #[tokio::test]
    async fn eth_call_retries_primary_once_then_falls_over_only_on_repeated_transport() {
        // Case (a): Transport on both primary calls → fallback fires.
        {
            let (primary, p_calls) =
                MockHttp::new(|_| Err(NodeError::Transport("connect refused".to_string())));
            let (fallback, f_calls) = MockHttp::new(|_| Ok(Bytes::from_static(b"OK")));
            let np = np_with(primary, Some(fallback));
            let got = np.eth_call(TransactionRequest::default()).await.unwrap();
            assert_eq!(&got[..], b"OK");
            assert_eq!(p_calls.load(Ordering::Relaxed), 2, "primary retried once");
            assert_eq!(f_calls.load(Ordering::Relaxed), 1);
        }
        // Case (b): Transport on call 0, Rpc on call 1 → fallback NOT called.
        {
            let (primary, p_calls) = MockHttp::new(|n| {
                if n == 0 {
                    Err(NodeError::Transport("transient".to_string()))
                } else {
                    Err(NodeError::Rpc("revert".to_string()))
                }
            });
            let (fallback, f_calls) = MockHttp::new(|_| Ok(Bytes::from_static(b"WRONG")));
            let np = np_with(primary, Some(fallback));
            let err = np
                .eth_call(TransactionRequest::default())
                .await
                .expect_err("retry returning Rpc must propagate as Rpc");
            assert!(matches!(err, NodeError::Rpc(_)), "got {err:?}");
            assert_eq!(p_calls.load(Ordering::Relaxed), 2, "primary retried once");
            assert_eq!(f_calls.load(Ordering::Relaxed), 0, "fallback NOT consulted");
        }
        // Case (c): Rpc on first primary call → no retry, no fallback.
        {
            let (primary, p_calls) = MockHttp::new(|_| Err(NodeError::Rpc("revert".to_string())));
            let (fallback, f_calls) = MockHttp::new(|_| Ok(Bytes::from_static(b"WRONG")));
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

    // ----- Phase 4 P4-A archive method mocks + N4A-1..N4A-4 tests -----

    /// Per-call deterministic outcome for archive `eth_get_storage_at`.
    /// Mirrors `MockHttp` shape but specialized to the storage-at return
    /// type. Other archive methods are stubbed to default Err so their
    /// calls aren't accidentally observable.
    struct MockArchiveStorage {
        outcome: Box<dyn Fn(usize) -> Result<B256, NodeError> + Send + Sync>,
        call_count: Arc<AtomicUsize>,
    }
    impl MockArchiveStorage {
        fn new(
            outcome: impl Fn(usize) -> Result<B256, NodeError> + Send + Sync + 'static,
        ) -> (Self, Arc<AtomicUsize>) {
            let cc = Arc::new(AtomicUsize::new(0));
            (
                Self {
                    outcome: Box::new(outcome),
                    call_count: Arc::clone(&cc),
                },
                cc,
            )
        }
    }
    #[async_trait]
    impl HttpRpc for MockArchiveStorage {
        async fn eth_call(&self, _: TransactionRequest) -> Result<Bytes, NodeError> {
            Err(NodeError::Rpc("not used in archive tests".into()))
        }
        async fn eth_call_at_block(
            &self,
            _: TransactionRequest,
            _: BlockId,
        ) -> Result<Bytes, NodeError> {
            Err(NodeError::Rpc("not used in archive tests".into()))
        }
        async fn eth_get_transaction_by_hash(
            &self,
            _: B256,
        ) -> Result<Option<Transaction>, NodeError> {
            Err(NodeError::Rpc("not used in archive tests".into()))
        }
        async fn eth_get_storage_at(
            &self,
            _address: Address,
            _slot: U256,
            _block_id: BlockId,
        ) -> Result<B256, NodeError> {
            let n = self.call_count.fetch_add(1, Ordering::Relaxed);
            (self.outcome)(n)
        }
    }

    /// Per-call deterministic outcome for archive `eth_get_proof` /
    /// `eth_get_code`. Returns generic `Bytes` per call so we can drive
    /// the get_code happy path AND inject `Transport` errors for the
    /// no-fallback assertion (N4A-3).
    struct MockArchiveProofCode {
        proof_outcome: Box<dyn Fn(usize) -> Result<EIP1186AccountProofResponse, NodeError> + Send + Sync>,
        code_outcome: Box<dyn Fn(usize) -> Result<Bytes, NodeError> + Send + Sync>,
        proof_calls: Arc<AtomicUsize>,
        code_calls: Arc<AtomicUsize>,
    }
    impl MockArchiveProofCode {
        fn new(
            proof_outcome: impl Fn(usize) -> Result<EIP1186AccountProofResponse, NodeError>
                + Send
                + Sync
                + 'static,
            code_outcome: impl Fn(usize) -> Result<Bytes, NodeError> + Send + Sync + 'static,
        ) -> (Self, Arc<AtomicUsize>, Arc<AtomicUsize>) {
            let pc = Arc::new(AtomicUsize::new(0));
            let cc = Arc::new(AtomicUsize::new(0));
            (
                Self {
                    proof_outcome: Box::new(proof_outcome),
                    code_outcome: Box::new(code_outcome),
                    proof_calls: Arc::clone(&pc),
                    code_calls: Arc::clone(&cc),
                },
                pc,
                cc,
            )
        }
    }
    #[async_trait]
    impl HttpRpc for MockArchiveProofCode {
        async fn eth_call(&self, _: TransactionRequest) -> Result<Bytes, NodeError> {
            Err(NodeError::Rpc("not used in archive tests".into()))
        }
        async fn eth_call_at_block(
            &self,
            _: TransactionRequest,
            _: BlockId,
        ) -> Result<Bytes, NodeError> {
            Err(NodeError::Rpc("not used in archive tests".into()))
        }
        async fn eth_get_transaction_by_hash(
            &self,
            _: B256,
        ) -> Result<Option<Transaction>, NodeError> {
            Err(NodeError::Rpc("not used in archive tests".into()))
        }
        async fn eth_get_proof(
            &self,
            _: Address,
            _: Vec<B256>,
            _: BlockId,
        ) -> Result<EIP1186AccountProofResponse, NodeError> {
            let n = self.proof_calls.fetch_add(1, Ordering::Relaxed);
            (self.proof_outcome)(n)
        }
        async fn eth_get_code(&self, _: Address, _: BlockId) -> Result<Bytes, NodeError> {
            let n = self.code_calls.fetch_add(1, Ordering::Relaxed);
            (self.code_outcome)(n)
        }
    }

    fn block_id_at(n: u64) -> BlockId {
        BlockId::Number(alloy::eips::BlockNumberOrTag::Number(n))
    }

    /// N4A-1 happy: configured archive returns canned B256;
    /// `NodeProvider::eth_get_storage_at` passes it through.
    #[tokio::test]
    async fn eth_get_storage_at_returns_archive_value() {
        let expected = B256::from([0xAB; 32]);
        let (archive, archive_calls) = MockArchiveStorage::new({
            let e = expected;
            move |_| Ok(e)
        });
        let (primary, _primary_calls) = MockHttp::new(|_| Ok(Bytes::from_static(b"NEVER")));
        let np = np_with_archive(primary, None, Some(Box::new(archive) as Box<dyn HttpRpc>));

        let got = np
            .eth_get_storage_at(Address::ZERO, U256::ZERO, block_id_at(18_000_000))
            .await
            .unwrap();
        assert_eq!(got, expected);
        assert_eq!(archive_calls.load(Ordering::Relaxed), 1);
    }

    /// N4A-2 abort: archive_http is None → ArchiveNotConfigured. NO
    /// fallback to primary HTTP (DP-1).
    #[tokio::test]
    async fn eth_get_storage_at_returns_archive_not_configured_when_unset() {
        let (primary, primary_calls) = MockHttp::new(|_| Ok(Bytes::from_static(b"NEVER")));
        let np = np_with(primary, None);

        let err = np
            .eth_get_storage_at(Address::ZERO, U256::ZERO, block_id_at(18_000_000))
            .await
            .expect_err("archive_http=None must Err(ArchiveNotConfigured)");
        assert!(matches!(err, NodeError::ArchiveNotConfigured));
        assert_eq!(
            primary_calls.load(Ordering::Relaxed),
            0,
            "primary must NOT be consulted (DP-1 no-fallback policy)"
        );
    }

    /// N4A-3 abort: archive returns Transport error → propagates Err
    /// directly. NO fallback to primary (DP-1).
    #[tokio::test]
    async fn eth_get_proof_does_not_fall_back_on_transport_error() {
        let (archive, archive_calls, _) = MockArchiveProofCode::new(
            |_| Err(NodeError::Transport("simulated".into())),
            |_| Ok(Bytes::new()),
        );
        let (primary, primary_calls) = MockHttp::new(|_| Ok(Bytes::from_static(b"NEVER")));
        let np = np_with_archive(primary, None, Some(Box::new(archive) as Box<dyn HttpRpc>));

        let err = np
            .eth_get_proof(Address::ZERO, vec![], block_id_at(18_000_000))
            .await
            .expect_err("must Err(Transport)");
        assert!(
            matches!(err, NodeError::Transport(_)),
            "expected Transport, got {err:?}"
        );
        assert_eq!(archive_calls.load(Ordering::Relaxed), 1);
        assert_eq!(
            primary_calls.load(Ordering::Relaxed),
            0,
            "primary must NOT be consulted on archive Transport error (DP-1)"
        );
    }

    /// N4A-4 boundary: archive returns empty Bytes for eth_get_code →
    /// passed through unchanged (covers the empty-bytecode case for an
    /// EOA address).
    #[tokio::test]
    async fn eth_get_code_round_trips_empty_bytes() {
        let (archive, _, code_calls) = MockArchiveProofCode::new(
            |_| {
                Err(NodeError::Rpc("proof not exercised in this test".into()))
            },
            |_| Ok(Bytes::new()),
        );
        let (primary, _) = MockHttp::new(|_| Ok(Bytes::from_static(b"NEVER")));
        let np = np_with_archive(primary, None, Some(Box::new(archive) as Box<dyn HttpRpc>));

        let got = np
            .eth_get_code(Address::ZERO, block_id_at(18_000_000))
            .await
            .unwrap();
        assert_eq!(got, Bytes::new());
        assert_eq!(code_calls.load(Ordering::Relaxed), 1);
    }
}
