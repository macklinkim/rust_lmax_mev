//! Phase 2 P2-C replay + EXIT gates per the approved P2-C execution
//! note v0.2.
//!
//! - [`Replayer<I>`] trait: object-safe via `async_trait` + assoc
//!   types; minimal shape; future `JournalReplayer<I = FileJournal>`
//!   plugs in unchanged.
//! - [`StateReplayer`]: drives a [`StateEngine`] over a `Vec<RecordedBlock>`
//!   sequence and concatenates all emitted events.
//! - [`RecordedBlock`]: minimal block descriptor (`number`, `hash`).
//! - [`RecordedEthCaller`]: deterministic [`EthCaller`] fixture
//!   keyed by `(block_hash, selector, pool_address)`. Asserts the
//!   `BlockId` arg is `BlockId::Hash` matching one of the recorded
//!   blocks; otherwise returns `NodeError::Rpc("unexpected block_id ...")`.
//!   The block-hash + selector + pool of each successful lookup is
//!   appended to a witness vec for test assertions.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use alloy::eips::BlockId;
use alloy::rpc::types::eth::TransactionRequest;
use alloy_primitives::{Address, Bytes, B256};
use parking_lot::Mutex;
use rust_lmax_mev_node::NodeError;
use rust_lmax_mev_state::{EthCaller, StateEngine, StateError, StateUpdateEvent};

/// Object-safe replay driver per the P2-C execution note.
#[async_trait::async_trait]
pub trait Replayer<I>: Send + Sync
where
    I: Send + 'static,
{
    type Output: Send + 'static;
    type Error: Send + 'static;
    async fn replay(&self, input: I) -> Result<Vec<Self::Output>, Self::Error>;
}

/// Minimal block descriptor consumed by [`StateReplayer`]. Paired
/// with a [`RecordedEthCaller`] that holds the corresponding eth_call
/// responses keyed by `(hash, selector, pool)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordedBlock {
    pub number: u64,
    pub hash: B256,
}

/// Drives a [`StateEngine`] over a recorded block sequence.
pub struct StateReplayer {
    engine: Arc<StateEngine>,
}

impl StateReplayer {
    pub fn new(engine: Arc<StateEngine>) -> Self {
        Self { engine }
    }
}

#[async_trait::async_trait]
impl Replayer<Vec<RecordedBlock>> for StateReplayer {
    type Output = StateUpdateEvent;
    type Error = StateError;

    async fn replay(
        &self,
        blocks: Vec<RecordedBlock>,
    ) -> Result<Vec<StateUpdateEvent>, StateError> {
        let mut all = Vec::with_capacity(blocks.len() * 2);
        for b in blocks {
            let mut events = self.engine.refresh_block(b.number, b.hash).await?;
            all.append(&mut events);
        }
        Ok(all)
    }
}

/// Lookup key for [`RecordedEthCaller`]: `(block_hash, selector,
/// pool_address)`. `selector` is the 4-byte function selector at the
/// start of `req.input`.
type FixtureKey = (B256, [u8; 4], Address);

struct RecordedState {
    responses: HashMap<FixtureKey, Bytes>,
    known_block_hashes: HashSet<B256>,
}

/// Deterministic [`EthCaller`] backed by a per-`(block_hash, selector,
/// pool)` response map. Block-hash pinning enforced inline:
/// non-`BlockId::Hash` variants and unknown hashes return
/// `NodeError::Rpc("unexpected block_id ...")`.
pub struct RecordedEthCaller {
    state: Mutex<RecordedState>,
    witness: Mutex<Vec<FixtureKey>>,
}

impl RecordedEthCaller {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(RecordedState {
                responses: HashMap::new(),
                known_block_hashes: HashSet::new(),
            }),
            witness: Mutex::new(Vec::new()),
        }
    }

    /// Register a fixture response. Adds `block_hash` to the known set.
    pub fn put(&self, block_hash: B256, selector: [u8; 4], pool: Address, bytes: Bytes) {
        let mut s = self.state.lock();
        s.known_block_hashes.insert(block_hash);
        s.responses.insert((block_hash, selector, pool), bytes);
    }

    /// Snapshot of the lookup witness in arrival order.
    pub fn witness(&self) -> Vec<FixtureKey> {
        self.witness.lock().clone()
    }
}

impl Default for RecordedEthCaller {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl EthCaller for RecordedEthCaller {
    async fn eth_call_at_block(
        &self,
        req: TransactionRequest,
        block_id: BlockId,
    ) -> Result<Bytes, NodeError> {
        // Pinning gate: only BlockId::Hash variants are allowed.
        let block_hash = match block_id {
            BlockId::Hash(rh) => rh.block_hash,
            BlockId::Number(_) => {
                return Err(NodeError::Rpc(format!(
                    "unexpected block_id (non-hash): {block_id:?}"
                )));
            }
        };
        // Pinning gate: hash must be a recorded one.
        {
            let s = self.state.lock();
            if !s.known_block_hashes.contains(&block_hash) {
                return Err(NodeError::Rpc(format!(
                    "unexpected block_id hash: {block_hash}"
                )));
            }
        }
        // Extract `to` + selector.
        let to = match req.to {
            Some(alloy::primitives::TxKind::Call(a)) => a,
            _ => return Err(NodeError::Rpc("missing to".into())),
        };
        let input = req.input.input.clone().unwrap_or_default();
        if input.len() < 4 {
            return Err(NodeError::Rpc("missing selector".into()));
        }
        let mut sel = [0u8; 4];
        sel.copy_from_slice(&input[..4]);
        let key = (block_hash, sel, to);
        let response = self.state.lock().responses.get(&key).cloned();
        self.witness.lock().push(key);
        response.ok_or_else(|| {
            NodeError::Rpc(format!(
                "no fixture for (block={block_hash}, selector={sel:02x?}, pool={to})"
            ))
        })
    }
}
