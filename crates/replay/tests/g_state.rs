//! G-State — Phase 2 EXIT gate (per ADR-001).
//!
//! Engine-emitted `StateUpdateEvent`s match hand-computed expected
//! values byte-for-byte (tolerance 0 per ADR-006). Also asserts the
//! block-hash pinning witness sequence over the recorded blocks
//! (positive coverage).

use std::sync::Arc;

use rust_lmax_mev_replay::{Replayer, StateReplayer};
use rust_lmax_mev_state::{SELECTOR_GET_RESERVES, SELECTOR_LIQUIDITY, SELECTOR_SLOT0};

mod common;

#[tokio::test]
async fn engine_emits_expected_state_for_recorded_fixture() {
    let pools = common::pools();
    let blocks = common::blocks();
    let caller = Arc::new(common::build_caller(&blocks));
    let (engine, _snap, _dir) = common::make_engine_with_caller(Arc::clone(&caller), pools.clone());
    let replayer = StateReplayer::new(engine);

    let got = replayer.replay(blocks.clone()).await.expect("replay ok");
    let expected = common::expected_events(&blocks, &pools);
    assert_eq!(
        got, expected,
        "State Correctness Gate violated: engine output does not match hand-computed expected"
    );

    // Block-hash pinning witness: for each block, V2 getReserves on POOL_V2,
    // then V3 slot0 on POOL_V3, then V3 liquidity on POOL_V3.
    let witness = caller.witness();
    assert_eq!(
        witness.len(),
        blocks.len() * 3,
        "expected 3 calls per block"
    );
    for (idx, b) in blocks.iter().enumerate() {
        let base = idx * 3;
        assert_eq!(
            witness[base],
            (b.hash, SELECTOR_GET_RESERVES, common::POOL_V2),
            "block {idx} call 0 should be V2 getReserves"
        );
        assert_eq!(
            witness[base + 1],
            (b.hash, SELECTOR_SLOT0, common::POOL_V3),
            "block {idx} call 1 should be V3 slot0"
        );
        assert_eq!(
            witness[base + 2],
            (b.hash, SELECTOR_LIQUIDITY, common::POOL_V3),
            "block {idx} call 2 should be V3 liquidity"
        );
    }
}
