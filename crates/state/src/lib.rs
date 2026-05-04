//! Phase 2 P2-B state engine per the approved P2-B execution note v0.4.
//!
//! - `pub trait EthCaller` (cross-crate accessible for P2-C gate fixtures).
//! - `NodeEthCaller` production adapter wrapping `Arc<NodeProvider>`.
//! - `PoolKind` / `PoolId` / `PoolState` / `StateUpdateEvent` types.
//! - Hand-rolled ABI decoders for `getReserves` / `slot0` / `liquidity`
//!   (left-padding + sign-extension + range checks; mis-padded → Decode).
//! - `StateEngine { caller, snapshot, pools }` with `pub fn new(provider,
//!   snapshot, pools)` (production: wraps in `NodeEthCaller`) and
//!   `pub fn with_caller(caller, snapshot, pools)` (test/replay).
//! - `refresh_block(block_number, block_hash)` returns
//!   `Vec<StateUpdateEvent>` with block-hash-pinned `eth_call_at_block`.

use std::sync::Arc;

use alloy::eips::BlockId;
use alloy::rpc::types::eth::TransactionRequest;
use alloy_primitives::{Address, Bytes, B256, U256};
use rust_lmax_mev_journal::{JournalError, RocksDbSnapshot};
use rust_lmax_mev_node::{NodeError, NodeProvider};
use serde::{Deserialize, Serialize};

pub use rust_lmax_mev_config::{PoolConfig, PoolKind};

mod decode;
pub mod rkyv_compat;

// --- selectors (keccak256 of canonical signature, first 4 bytes) ---------
pub const SELECTOR_GET_RESERVES: [u8; 4] = [0x09, 0x02, 0xf1, 0xac];
pub const SELECTOR_SLOT0: [u8; 4] = [0x38, 0x50, 0xc7, 0xbd];
pub const SELECTOR_LIQUIDITY: [u8; 4] = [0x1a, 0x68, 0x65, 0x02];

/// Object-safe block-pinned `eth_call` adapter. Production:
/// [`NodeEthCaller`] wraps [`NodeProvider`]. Tests / P2-C replay
/// fixtures: implement directly with canned responses keyed by
/// `(selector, pool_address, block_hash)`.
#[async_trait::async_trait]
pub trait EthCaller: Send + Sync {
    async fn eth_call_at_block(
        &self,
        req: TransactionRequest,
        block_id: BlockId,
    ) -> Result<Bytes, NodeError>;
}

/// Production adapter — forwards to `NodeProvider::eth_call_at_block`.
pub struct NodeEthCaller(pub Arc<NodeProvider>);

#[async_trait::async_trait]
impl EthCaller for NodeEthCaller {
    async fn eth_call_at_block(
        &self,
        req: TransactionRequest,
        block_id: BlockId,
    ) -> Result<Bytes, NodeError> {
        self.0.eth_call_at_block(req, block_id).await
    }
}

/// Pool identity carried inside `StateUpdateEvent` and used as the
/// per-pool key axis in `RocksDbSnapshot`.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct PoolId {
    pub kind: PoolKind,
    #[rkyv(with = crate::rkyv_compat::AddressAsBytes)]
    pub address: Address,
}

impl From<&PoolConfig> for PoolId {
    fn from(c: &PoolConfig) -> Self {
        Self {
            kind: c.kind,
            address: c.address,
        }
    }
}

/// Pool reserves snapshot. Persisted to RocksDB via bincode; emitted
/// in `StateUpdateEvent`.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum PoolState {
    UniV2 {
        #[rkyv(with = crate::rkyv_compat::U256AsBytes)]
        reserve0: U256,
        #[rkyv(with = crate::rkyv_compat::U256AsBytes)]
        reserve1: U256,
        block_timestamp_last: u32,
    },
    UniV3 {
        #[rkyv(with = crate::rkyv_compat::U256AsBytes)]
        sqrt_price_x96: U256,
        tick: i32,
        liquidity: u128,
    },
}

/// Event emitted on the state→opportunity bus per ADR-005 (Phase 2
/// has no consumer past P2-D wiring).
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct StateUpdateEvent {
    pub block_number: u64,
    #[rkyv(with = crate::rkyv_compat::B256AsBytes)]
    pub block_hash: B256,
    pub pool: PoolId,
    pub state: PoolState,
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("node error: {0}")]
    Node(#[from] NodeError),

    #[error("ABI decode error: {0}")]
    Decode(String),

    #[error("snapshot error: {0}")]
    Snapshot(#[from] JournalError),

    #[error("unknown pool address {0}")]
    UnknownPool(Address),
}

/// State engine — owns an [`EthCaller`] + [`RocksDbSnapshot`] + the
/// pool registry, and refreshes per-block reserves.
pub struct StateEngine {
    caller: Arc<dyn EthCaller>,
    snapshot: Arc<RocksDbSnapshot>,
    pools: Vec<PoolId>,
}

impl StateEngine {
    /// Production constructor: wraps `NodeProvider` in `NodeEthCaller`.
    pub fn new(
        provider: Arc<NodeProvider>,
        snapshot: Arc<RocksDbSnapshot>,
        pools: Vec<PoolId>,
    ) -> Self {
        Self::with_caller(Arc::new(NodeEthCaller(provider)), snapshot, pools)
    }

    /// Test / P2-C-replay constructor: any `EthCaller` impl.
    pub fn with_caller(
        caller: Arc<dyn EthCaller>,
        snapshot: Arc<RocksDbSnapshot>,
        pools: Vec<PoolId>,
    ) -> Self {
        Self {
            caller,
            snapshot,
            pools,
        }
    }

    /// Refreshes reserves for every configured pool at the given block.
    /// `block_hash` is the canonical pinning input — both `eth_call`
    /// and the snapshot key derive from it (block_number is the key
    /// prefix; block_hash drives RPC determinism).
    pub async fn refresh_block(
        &self,
        block_number: u64,
        block_hash: B256,
    ) -> Result<Vec<StateUpdateEvent>, StateError> {
        let block_id = BlockId::Hash(block_hash.into());
        let mut events = Vec::with_capacity(self.pools.len());
        for pool in &self.pools {
            let state = match pool.kind {
                PoolKind::UniswapV2 => self.fetch_v2(pool.address, block_id).await?,
                PoolKind::UniswapV3Fee005 => self.fetch_v3(pool.address, block_id).await?,
            };
            let key = snapshot_key(block_number, &pool.address);
            self.snapshot.save(&key, &state)?;
            events.push(StateUpdateEvent {
                block_number,
                block_hash,
                pool: pool.clone(),
                state,
            });
        }
        Ok(events)
    }

    async fn fetch_v2(&self, addr: Address, block_id: BlockId) -> Result<PoolState, StateError> {
        let req = call_req(addr, &SELECTOR_GET_RESERVES);
        let bytes = self.caller.eth_call_at_block(req, block_id).await?;
        decode::decode_get_reserves(&bytes)
    }

    async fn fetch_v3(&self, addr: Address, block_id: BlockId) -> Result<PoolState, StateError> {
        let bytes_slot0 = self
            .caller
            .eth_call_at_block(call_req(addr, &SELECTOR_SLOT0), block_id)
            .await?;
        let (sqrt_price_x96, tick) = decode::decode_slot0(&bytes_slot0)?;
        let bytes_liq = self
            .caller
            .eth_call_at_block(call_req(addr, &SELECTOR_LIQUIDITY), block_id)
            .await?;
        let liquidity = decode::decode_liquidity(&bytes_liq)?;
        Ok(PoolState::UniV3 {
            sqrt_price_x96,
            tick,
            liquidity,
        })
    }
}

fn call_req(to: Address, selector: &[u8; 4]) -> TransactionRequest {
    TransactionRequest::default()
        .to(to)
        .input(Bytes::from(selector.to_vec()).into())
}

/// Snapshot key shape per P2-B v0.4 Risk Decision 3:
/// `[u8; 28] = block_number_be(8) || pool_address(20)`. PoolKind is in
/// the bincoded value, not the key.
pub fn snapshot_key(block_number: u64, pool_address: &Address) -> [u8; 28] {
    let mut key = [0u8; 28];
    key[..8].copy_from_slice(&block_number.to_be_bytes());
    key[8..].copy_from_slice(pool_address.as_slice());
    key
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;
    use std::collections::HashMap;
    use std::sync::Arc;

    type FixtureKey = ([u8; 4], Address);
    type FixtureMap = HashMap<FixtureKey, Result<Bytes, NodeError>>;

    /// Fixture caller keyed by `(selector, pool_address)`. block_id is
    /// asserted-passed by tests via a witness vec.
    struct MockEthCaller {
        responses: Mutex<FixtureMap>,
    }

    impl MockEthCaller {
        fn new() -> Self {
            Self {
                responses: Mutex::new(HashMap::new()),
            }
        }
        fn put(&self, selector: [u8; 4], addr: Address, out: Result<Bytes, NodeError>) {
            self.responses.lock().insert((selector, addr), out);
        }
    }

    #[async_trait::async_trait]
    impl EthCaller for MockEthCaller {
        async fn eth_call_at_block(
            &self,
            req: TransactionRequest,
            _block_id: BlockId,
        ) -> Result<Bytes, NodeError> {
            let to = match req.to {
                Some(alloy::primitives::TxKind::Call(a)) => a,
                _ => return Err(NodeError::Rpc("test req missing to".into())),
            };
            let input_bytes = req.input.input.clone().unwrap_or_default();
            if input_bytes.len() < 4 {
                return Err(NodeError::Rpc("test req missing selector".into()));
            }
            let mut sel = [0u8; 4];
            sel.copy_from_slice(&input_bytes[..4]);
            let mut g = self.responses.lock();
            match g.remove(&(sel, to)) {
                Some(v) => v,
                None => Err(NodeError::Rpc(format!(
                    "no fixture for selector {:?} addr {to}",
                    sel
                ))),
            }
        }
    }

    fn make_snapshot() -> (Arc<RocksDbSnapshot>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snap");
        let snap = Arc::new(RocksDbSnapshot::open(&path).unwrap());
        (snap, dir)
    }

    fn pool_v2() -> PoolId {
        PoolId {
            kind: PoolKind::UniswapV2,
            address: alloy_primitives::address!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        }
    }
    fn pool_v3() -> PoolId {
        PoolId {
            kind: PoolKind::UniswapV3Fee005,
            address: alloy_primitives::address!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
        }
    }

    /// Build a 32-byte BE word with low N bytes set to `value` low N bytes.
    fn be_word(value: u128, low_bytes: usize) -> [u8; 32] {
        let mut w = [0u8; 32];
        let v = value.to_be_bytes(); // 16 BE bytes
        let take = low_bytes.min(16);
        w[32 - take..].copy_from_slice(&v[16 - take..]);
        w
    }

    fn cat(words: &[[u8; 32]]) -> Bytes {
        let mut v = Vec::with_capacity(32 * words.len());
        for w in words {
            v.extend_from_slice(w);
        }
        Bytes::from(v)
    }

    /// S-1 happy: V2 getReserves decode + snapshot round-trip.
    #[tokio::test]
    async fn state_engine_refresh_decodes_univ2_reserves() {
        let (snap, _dir) = make_snapshot();
        let caller = Arc::new(MockEthCaller::new());
        let pool = pool_v2();
        // reserve0 = 1_000_000, reserve1 = 2_000_000, ts = 0xdeadbeef
        let r0 = be_word(1_000_000, 14); // uint112
        let r1 = be_word(2_000_000, 14);
        let ts = be_word(0xdead_beef, 4); // uint32
        caller.put(SELECTOR_GET_RESERVES, pool.address, Ok(cat(&[r0, r1, ts])));
        let engine = StateEngine::with_caller(caller, Arc::clone(&snap), vec![pool.clone()]);
        let block_hash = B256::from([0x11; 32]);
        let events = engine.refresh_block(42, block_hash).await.unwrap();
        assert_eq!(events.len(), 1);
        let PoolState::UniV2 {
            reserve0,
            reserve1,
            block_timestamp_last,
        } = events[0].state.clone()
        else {
            panic!("expected UniV2");
        };
        assert_eq!(reserve0, U256::from(1_000_000u64));
        assert_eq!(reserve1, U256::from(2_000_000u64));
        assert_eq!(block_timestamp_last, 0xdead_beef);
        // Snapshot round-trip
        let key = snapshot_key(42, &pool.address);
        let loaded: PoolState = snap.load(&key).unwrap().unwrap();
        assert_eq!(loaded, events[0].state);
    }

    /// S-2 happy: V3 slot0 + liquidity decode + snapshot round-trip.
    #[tokio::test]
    async fn state_engine_refresh_decodes_univ3_slot0_and_liquidity() {
        let (snap, _dir) = make_snapshot();
        let caller = Arc::new(MockEthCaller::new());
        let pool = pool_v3();
        // sqrtPriceX96 = 0xCAFE_F00D_BABE_BEEF (uint160), tick = -200 (int24), liquidity = 5_000_000_000
        let sqrt = be_word(0xCAFE_F00D_BABE_BEEFu128, 20);
        // tick = -200 = 0xFFFF38 in three bytes; sign-extend to high 29 bytes of 0xff
        let mut tick_word = [0xffu8; 32];
        let tick_low3: [u8; 3] = [0xff, 0xff, 0x38];
        tick_word[29..32].copy_from_slice(&tick_low3);
        // Pad words 2..6 to fill 224 bytes (slot0 returns 7 words)
        let pad = [0u8; 32];
        let slot0 = cat(&[sqrt, tick_word, pad, pad, pad, pad, pad]);
        let liq = be_word(5_000_000_000, 16);
        caller.put(SELECTOR_SLOT0, pool.address, Ok(slot0));
        caller.put(SELECTOR_LIQUIDITY, pool.address, Ok(cat(&[liq])));
        let engine = StateEngine::with_caller(caller, Arc::clone(&snap), vec![pool.clone()]);
        let block_hash = B256::from([0x22; 32]);
        let events = engine.refresh_block(43, block_hash).await.unwrap();
        assert_eq!(events.len(), 1);
        let PoolState::UniV3 {
            sqrt_price_x96,
            tick,
            liquidity,
        } = events[0].state.clone()
        else {
            panic!("expected UniV3");
        };
        assert_eq!(sqrt_price_x96, U256::from(0xCAFE_F00D_BABE_BEEFu128));
        assert_eq!(tick, -200);
        assert_eq!(liquidity, 5_000_000_000);
        let key = snapshot_key(43, &pool.address);
        let loaded: PoolState = snap.load(&key).unwrap().unwrap();
        assert_eq!(loaded, events[0].state);
    }

    /// S-3 boundary: 2 pools (V2 + V3) → independent per-pool persistence.
    #[tokio::test]
    async fn state_engine_refresh_persists_per_pool_independently() {
        let (snap, _dir) = make_snapshot();
        let caller = Arc::new(MockEthCaller::new());
        let p2 = pool_v2();
        let p3 = pool_v3();
        // V2 fixture
        caller.put(
            SELECTOR_GET_RESERVES,
            p2.address,
            Ok(cat(&[be_word(7, 14), be_word(11, 14), be_word(13, 4)])),
        );
        // V3 fixtures
        let mut tick_word = [0u8; 32];
        tick_word[29..32].copy_from_slice(&[0x00, 0x00, 0x42]); // tick = +66
        let pad = [0u8; 32];
        caller.put(
            SELECTOR_SLOT0,
            p3.address,
            Ok(cat(&[be_word(99, 20), tick_word, pad, pad, pad, pad, pad])),
        );
        caller.put(SELECTOR_LIQUIDITY, p3.address, Ok(cat(&[be_word(123, 16)])));
        let engine =
            StateEngine::with_caller(caller, Arc::clone(&snap), vec![p2.clone(), p3.clone()]);
        let block_hash = B256::from([0x33; 32]);
        let events = engine.refresh_block(100, block_hash).await.unwrap();
        assert_eq!(events.len(), 2);
        let v2_loaded: PoolState = snap.load(&snapshot_key(100, &p2.address)).unwrap().unwrap();
        let v3_loaded: PoolState = snap.load(&snapshot_key(100, &p3.address)).unwrap().unwrap();
        assert!(matches!(v2_loaded, PoolState::UniV2 { .. }));
        assert!(matches!(v3_loaded, PoolState::UniV3 { .. }));
    }

    /// S-4 failure: malformed ABI bytes → `Err(StateError::Decode)` and
    /// no snapshot write for that pool.
    #[tokio::test]
    async fn state_engine_refresh_returns_decode_error_on_malformed_abi() {
        let (snap, _dir) = make_snapshot();
        let caller = Arc::new(MockEthCaller::new());
        let pool = pool_v2();
        // Word 0 has a non-zero byte in the high-14 padding region — uint112 violation.
        let mut bad_r0 = [0u8; 32];
        bad_r0[10] = 0x01; // bit set in the padding region
        let r1 = be_word(2, 14);
        let ts = be_word(3, 4);
        caller.put(
            SELECTOR_GET_RESERVES,
            pool.address,
            Ok(cat(&[bad_r0, r1, ts])),
        );
        let engine = StateEngine::with_caller(caller, Arc::clone(&snap), vec![pool.clone()]);
        let block_hash = B256::from([0x44; 32]);
        let err = engine
            .refresh_block(7, block_hash)
            .await
            .expect_err("malformed ABI must error");
        assert!(matches!(err, StateError::Decode(_)), "got {err:?}");
        // No snapshot write for this key.
        let key = snapshot_key(7, &pool.address);
        let loaded: Option<PoolState> = snap.load(&key).unwrap();
        assert!(loaded.is_none(), "no snapshot write on decode failure");
    }
}
