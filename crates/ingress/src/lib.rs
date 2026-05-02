//! Phase 2 P2-A ingress per ADR-003 + P2-A v0.4.
//!
//! Provides:
//! - [`MempoolSource`] trait (object-safe via `Pin<Box<dyn Stream + Send + 'static>>`).
//! - [`GethWsMempool`] impl that subscribes Geth WS pending-tx hashes
//!   via [`rust_lmax_mev_node::NodeProvider`], dedups by hash via a
//!   4096-entry LRU, fetches full transactions via
//!   `eth_get_transaction_by_hash`, filters by
//!   `rust_lmax_mev_config::IngressConfig::watched_addresses` using
//!   [`Normalizer::filter`], and yields normalized
//!   [`MempoolEvent`]s.
//! - [`IngressEvent`] sum-type payload (`Mempool` | `Block`) for the
//!   ingress→state bus per ADR-005.
//! - [`MempoolEvent`] / [`BlockEvent`] payload structs.
//! - [`IngressError`] (5-ish variants; `Node(NodeError)` with `#[from]`).

use std::future::Future;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use alloy::consensus::Transaction as ConsensusTransaction;
use alloy::rpc::types::eth::Transaction;
use alloy_primitives::{Address, Bytes, B256, U256};
use futures::channel::mpsc;
use futures::{Stream, StreamExt};
use lru::LruCache;
use parking_lot::Mutex;
use rust_lmax_mev_node::{NodeError, NodeProvider};

const DEDUP_CAPACITY: usize = 4096;

/// Object-safe mempool stream contract. Production impl is
/// [`GethWsMempool`]; future impls (e.g., bloXroute) plug in unchanged.
pub trait MempoolSource: Send + Sync {
    fn stream(
        &self,
    ) -> Pin<Box<dyn Stream<Item = Result<MempoolEvent, IngressError>> + Send + 'static>>;
}

/// Geth WS pending-tx subscription wrapped with dedup + per-hash full-
/// tx fetch + WETH/USDC filter.
pub struct GethWsMempool {
    provider: Arc<NodeProvider>,
    watched: Vec<Address>,
}

impl GethWsMempool {
    pub fn new(provider: Arc<NodeProvider>, watched: Vec<Address>) -> Self {
        Self { provider, watched }
    }
}

impl MempoolSource for GethWsMempool {
    fn stream(
        &self,
    ) -> Pin<Box<dyn Stream<Item = Result<MempoolEvent, IngressError>> + Send + 'static>> {
        let hash_stream = self.provider.subscribe_pending_txs();
        let provider = Arc::clone(&self.provider);
        let fetch: FetchFn = Arc::new(move |hash: B256| {
            let provider = Arc::clone(&provider);
            Box::pin(async move { provider.eth_get_transaction_by_hash(hash).await })
        });
        build_mempool_stream(hash_stream, fetch, self.watched.clone())
    }
}

/// Boxed fetch closure type — used by tests to inject deterministic
/// per-hash lookup outcomes without touching alloy.
pub type FetchFn = Arc<
    dyn Fn(B256) -> Pin<Box<dyn Future<Output = Result<Option<Transaction>, NodeError>> + Send>>
        + Send
        + Sync
        + 'static,
>;

/// Internal pipeline: hash stream → dedup LRU → full-tx fetch → filter
/// → MempoolEvent. Spawns a tokio task that pumps items into a
/// `futures::mpsc::unbounded`; consumer holds the receiver as the
/// returned boxed Stream.
pub(crate) fn build_mempool_stream(
    mut hash_stream: Pin<Box<dyn Stream<Item = Result<B256, NodeError>> + Send + 'static>>,
    fetch: FetchFn,
    watched: Vec<Address>,
) -> Pin<Box<dyn Stream<Item = Result<MempoolEvent, IngressError>> + Send + 'static>> {
    let (tx, rx) = mpsc::unbounded::<Result<MempoolEvent, IngressError>>();
    let cache = Arc::new(Mutex::new(LruCache::<B256, ()>::new(
        NonZeroUsize::new(DEDUP_CAPACITY).expect("DEDUP_CAPACITY > 0"),
    )));
    tokio::spawn(async move {
        while let Some(item) = hash_stream.next().await {
            match item {
                Err(NodeError::Closed) => {
                    let _ = tx.unbounded_send(Err(IngressError::Closed));
                    return;
                }
                Err(e) => {
                    if tx.unbounded_send(Err(IngressError::from(e))).is_err() {
                        return;
                    }
                    continue;
                }
                Ok(hash) => {
                    {
                        let mut c = cache.lock();
                        if c.put(hash, ()).is_some() {
                            continue; // already seen
                        }
                    }
                    match (fetch)(hash).await {
                        Ok(Some(t)) => {
                            if let Some(ev) = Normalizer::filter(&t, &watched) {
                                if tx.unbounded_send(Ok(ev)).is_err() {
                                    return;
                                }
                            }
                        }
                        Ok(None) => {
                            // Tx evicted from local mempool between
                            // hash announce and fetch; silently drop.
                        }
                        Err(NodeError::Closed) => {
                            let _ = tx.unbounded_send(Err(IngressError::Closed));
                            return;
                        }
                        Err(e) => {
                            if tx.unbounded_send(Err(IngressError::from(e))).is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        }
        // Hash stream ended → close channel.
    });
    Box::pin(rx)
}

/// Pure-function normalizer: keep tx iff `tx.to ∈ watched`. Returns a
/// fully-populated [`MempoolEvent`] on keep; `None` on drop. Calldata
/// decoding deferred to P3 per v0.4.
pub struct Normalizer;

impl Normalizer {
    pub fn filter(t: &Transaction, watched: &[Address]) -> Option<MempoolEvent> {
        let to = t.to()?;
        if !watched.iter().any(|w| w == &to) {
            return None;
        }
        let observed_at_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        Some(MempoolEvent {
            tx_hash: *t.inner.tx_hash(),
            from: t.from,
            to: Some(to),
            value: t.value(),
            input: t.input().clone(),
            gas_limit: t.gas_limit(),
            max_fee: t.max_fee_per_gas(),
            observed_at_ns,
        })
    }
}

/// Phase 2 ingress→state payload sum type per ADR-005 (single bus per
/// pipeline-stage boundary).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngressEvent {
    Mempool(MempoolEvent),
    Block(BlockEvent),
}

/// Normalized mempool transaction event. Field shape per P2-A v0.4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MempoolEvent {
    pub tx_hash: B256,
    pub from: Address,
    pub to: Option<Address>,
    pub value: U256,
    pub input: Bytes,
    pub gas_limit: u64,
    pub max_fee: u128,
    pub observed_at_ns: u64,
}

/// New-block header event. Field shape per P2-A v0.4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockEvent {
    pub block_number: u64,
    pub block_hash: B256,
    pub parent_hash: B256,
    pub timestamp_ns: u64,
}

/// Ingress-layer error surface.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum IngressError {
    #[error("node error: {0}")]
    Node(#[from] NodeError),

    #[error("decode error: {0}")]
    Decode(String),

    #[error("ingress closed")]
    Closed,
}

#[cfg(test)]
mod tests {
    //! I-1 happy / I-2 failure / I-3 boundary per the approved P2-A
    //! execution note v0.4. Tests construct a `Transaction` literal
    //! where possible, and use `build_mempool_stream` directly with
    //! injected hash streams + fetch closures so no live network or
    //! alloy WS is needed.

    use super::*;
    use alloy::consensus::{Signed, TxEip1559, TxEnvelope};
    use alloy::rpc::types::eth::Transaction as RpcTransaction;
    use alloy_primitives::{address, b256, bytes, PrimitiveSignature};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn watched_a() -> Address {
        address!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    }
    fn watched_b() -> Address {
        address!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
    }
    fn unrelated() -> Address {
        address!("cccccccccccccccccccccccccccccccccccccccc")
    }

    /// Build a fake EIP-1559 signed Transaction whose recipient is
    /// `to`. Other fields are deterministic placeholders.
    fn fake_tx(to: Address) -> RpcTransaction {
        let inner = TxEip1559 {
            chain_id: 1,
            nonce: 1,
            gas_limit: 21_000,
            max_fee_per_gas: 100,
            max_priority_fee_per_gas: 1,
            to: alloy::primitives::TxKind::Call(to),
            value: U256::from(123u64),
            access_list: Default::default(),
            input: bytes!("aabbccdd"),
        };
        let sig = PrimitiveSignature::test_signature();
        let signed = Signed::new_unchecked(inner, sig, B256::ZERO);
        let envelope = TxEnvelope::Eip1559(signed);
        RpcTransaction {
            inner: envelope,
            block_hash: None,
            block_number: None,
            transaction_index: None,
            from: address!("ffffffffffffffffffffffffffffffffffffffff"),
            effective_gas_price: None,
        }
    }

    /// I-1 happy: filter keeps `tx.to == watched_a`, drops `tx.to == unrelated`.
    #[test]
    fn normalizer_keeps_watched_to_drops_unrelated() {
        let watched = vec![watched_a(), watched_b()];
        let kept = Normalizer::filter(&fake_tx(watched_a()), &watched);
        let dropped = Normalizer::filter(&fake_tx(unrelated()), &watched);
        assert!(kept.is_some(), "watched_a tx must be kept");
        assert_eq!(kept.unwrap().to, Some(watched_a()));
        assert!(dropped.is_none(), "unrelated tx must be dropped");
    }

    /// I-2 failure: an upstream `NodeError::Closed` from the hash
    /// stream surfaces as `IngressError::Closed` to the consumer and
    /// the pipeline stops.
    #[tokio::test]
    async fn build_mempool_stream_propagates_closed() {
        use futures::stream;
        let hash_stream: Pin<Box<dyn Stream<Item = Result<B256, NodeError>> + Send + 'static>> =
            Box::pin(stream::iter(vec![Err(NodeError::Closed)]));
        let fetch: FetchFn = Arc::new(|_h| Box::pin(async move { Ok(Some(fake_tx(watched_a()))) }));
        let mut s = build_mempool_stream(hash_stream, fetch, vec![watched_a()]);
        let item = s.next().await.expect("must yield Closed");
        assert!(matches!(item, Err(IngressError::Closed)), "got {item:?}");
        assert!(s.next().await.is_none(), "stream must end after Closed");
    }

    /// I-3 boundary: 5 duplicate hashes → consumer sees 1 event.
    #[tokio::test]
    async fn build_mempool_stream_dedups_repeated_hashes() {
        use futures::stream;
        let h = b256!("1111111111111111111111111111111111111111111111111111111111111111");
        let hash_stream: Pin<Box<dyn Stream<Item = Result<B256, NodeError>> + Send + 'static>> =
            Box::pin(stream::iter(vec![Ok(h), Ok(h), Ok(h), Ok(h), Ok(h)]));
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_fetch = Arc::clone(&calls);
        let fetch: FetchFn = Arc::new(move |_h| {
            calls_for_fetch.fetch_add(1, Ordering::Relaxed);
            Box::pin(async move { Ok(Some(fake_tx(watched_a()))) })
        });
        let mut s = build_mempool_stream(hash_stream, fetch, vec![watched_a()]);
        let mut count = 0;
        while let Some(item) = s.next().await {
            if item.is_ok() {
                count += 1;
            }
        }
        assert_eq!(count, 1, "exactly one dedup'd event");
        assert_eq!(
            calls.load(Ordering::Relaxed),
            1,
            "fetch called exactly once for the dedup'd hash"
        );
    }
}
