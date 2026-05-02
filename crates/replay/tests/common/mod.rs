//! Shared fixture builder for the P2-C EXIT gate tests.
//!
//! Each integration test file in `crates/replay/tests/` is its own
//! crate, so the `#![allow(dead_code)]` is needed for items only used
//! by a subset of the test files.

#![allow(dead_code)]

use std::sync::Arc;

use alloy_primitives::{address, Address, Bytes, B256, U256};
use rust_lmax_mev_journal::RocksDbSnapshot;
use rust_lmax_mev_replay::{RecordedBlock, RecordedEthCaller};
use rust_lmax_mev_state::{
    PoolId, PoolKind, PoolState, StateEngine, StateUpdateEvent, SELECTOR_GET_RESERVES,
    SELECTOR_LIQUIDITY, SELECTOR_SLOT0,
};

pub const POOL_V2: Address = address!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
pub const POOL_V3: Address = address!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");

pub fn pools() -> Vec<PoolId> {
    vec![
        PoolId {
            kind: PoolKind::UniswapV2,
            address: POOL_V2,
        },
        PoolId {
            kind: PoolKind::UniswapV3Fee005,
            address: POOL_V3,
        },
    ]
}

/// Deterministic 5-block recorded sequence with synthetic block_hashes.
pub fn blocks() -> Vec<RecordedBlock> {
    (1..=5u64)
        .map(|i| RecordedBlock {
            number: 100 + i,
            hash: B256::from([i as u8; 32]),
        })
        .collect()
}

/// Build a 32-byte BE word with low N bytes set to `value` low N bytes.
pub fn be_word(value: u128, low_bytes: usize) -> [u8; 32] {
    let mut w = [0u8; 32];
    let v = value.to_be_bytes();
    let take = low_bytes.min(16);
    w[32 - take..].copy_from_slice(&v[16 - take..]);
    w
}

pub fn cat(words: &[[u8; 32]]) -> Bytes {
    let mut v = Vec::with_capacity(32 * words.len());
    for w in words {
        v.extend_from_slice(w);
    }
    Bytes::from(v)
}

/// 32-byte word for an `int24` tick, sign-extended to high 29 bytes.
pub fn tick_word(tick: i32) -> [u8; 32] {
    let mut w = if tick < 0 { [0xffu8; 32] } else { [0u8; 32] };
    let tick_be = tick.to_be_bytes(); // 4 bytes; we only want the low 3
    w[29..32].copy_from_slice(&tick_be[1..4]);
    w
}

/// V2 `getReserves` synthetic fixture: reserve0 = 1_000_000 + i*100,
/// reserve1 = 2_000_000 + i*100, ts = 0xDEAD0000 + i.
pub fn v2_bytes(i: u64) -> Bytes {
    let r0 = be_word(1_000_000 + (i as u128) * 100, 14);
    let r1 = be_word(2_000_000 + (i as u128) * 100, 14);
    let ts = be_word(0xDEAD_0000u128 + i as u128, 4);
    cat(&[r0, r1, ts])
}

/// V3 `slot0` synthetic fixture: sqrtPriceX96 = 0xCAFEF00D + i*100,
/// tick = -200 + i*10. Words 2..6 are zero padding (slot0 returns 7
/// words; only words 0 and 1 are persisted by `decode_slot0`).
pub fn v3_slot0_bytes(i: u64) -> Bytes {
    let sqrt = be_word(0xCAFE_F00Du128 + (i as u128) * 100, 20);
    let tick_raw = -200i32 + (i as i32) * 10;
    let tick = tick_word(tick_raw);
    let pad = [0u8; 32];
    cat(&[sqrt, tick, pad, pad, pad, pad, pad])
}

/// V3 `liquidity` synthetic fixture: 5_000_000_000 + i*1000.
pub fn v3_liquidity_bytes(i: u64) -> Bytes {
    cat(&[be_word(5_000_000_000u128 + (i as u128) * 1000, 16)])
}

/// Build a fully-populated `RecordedEthCaller` for the 5-block × 2-pool
/// sequence.
pub fn build_caller(blocks: &[RecordedBlock]) -> RecordedEthCaller {
    let caller = RecordedEthCaller::new();
    for b in blocks {
        let i = b.number - 100;
        caller.put(b.hash, SELECTOR_GET_RESERVES, POOL_V2, v2_bytes(i));
        caller.put(b.hash, SELECTOR_SLOT0, POOL_V3, v3_slot0_bytes(i));
        caller.put(b.hash, SELECTOR_LIQUIDITY, POOL_V3, v3_liquidity_bytes(i));
    }
    caller
}

/// Hand-computed expected `Vec<StateUpdateEvent>` for the synthetic
/// fixture, in the order `StateEngine::refresh_block` emits them
/// (block ascending, pool ascending in `pools()` order).
pub fn expected_events(blocks: &[RecordedBlock], pools: &[PoolId]) -> Vec<StateUpdateEvent> {
    let mut events = Vec::with_capacity(blocks.len() * pools.len());
    for b in blocks {
        let i = b.number - 100;
        for pool in pools {
            let state = match pool.kind {
                PoolKind::UniswapV2 => PoolState::UniV2 {
                    reserve0: U256::from(1_000_000u128 + (i as u128) * 100),
                    reserve1: U256::from(2_000_000u128 + (i as u128) * 100),
                    block_timestamp_last: (0xDEAD_0000u32) + i as u32,
                },
                PoolKind::UniswapV3Fee005 => PoolState::UniV3 {
                    sqrt_price_x96: U256::from(0xCAFE_F00Du128 + (i as u128) * 100),
                    tick: -200 + i as i32 * 10,
                    liquidity: 5_000_000_000u128 + (i as u128) * 1000,
                },
            };
            events.push(StateUpdateEvent {
                block_number: b.number,
                block_hash: b.hash,
                pool: pool.clone(),
                state,
            });
        }
    }
    events
}

/// Build a fresh tempdir-backed RocksDbSnapshot.
pub fn make_snapshot() -> (Arc<RocksDbSnapshot>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snap");
    let snap = Arc::new(RocksDbSnapshot::open(&path).unwrap());
    (snap, dir)
}

/// Construct a `StateEngine` driven by a `RecordedEthCaller` for the
/// supplied pools + a fresh tempdir snapshot. Returns the engine
/// (Arc-wrapped for `StateReplayer`), the snapshot, and the held
/// TempDir guard.
pub fn make_engine_with_caller(
    caller: Arc<RecordedEthCaller>,
    pools: Vec<PoolId>,
) -> (Arc<StateEngine>, Arc<RocksDbSnapshot>, tempfile::TempDir) {
    let (snap, dir) = make_snapshot();
    let engine = Arc::new(StateEngine::with_caller(caller, Arc::clone(&snap), pools));
    (engine, snap, dir)
}
