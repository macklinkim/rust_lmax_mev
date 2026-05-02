//! G-Replay — Phase 2 EXIT gate (per ADR-001).
//!
//! Same recorded input + same code → byte-identical
//! `Vec<StateUpdateEvent>` across two runs.

use std::sync::Arc;

use rust_lmax_mev_replay::{Replayer, StateReplayer};

mod common;

#[tokio::test]
async fn same_input_yields_byte_identical_events_across_two_runs() {
    let pools = common::pools();
    let blocks = common::blocks();

    // Run 1
    let caller1 = Arc::new(common::build_caller(&blocks));
    let (engine1, _snap1, _dir1) =
        common::make_engine_with_caller(Arc::clone(&caller1), pools.clone());
    let replayer1 = StateReplayer::new(engine1);
    let events1 = replayer1.replay(blocks.clone()).await.expect("run1 ok");

    // Run 2 — fresh fixture/engine/snapshot
    let caller2 = Arc::new(common::build_caller(&blocks));
    let (engine2, _snap2, _dir2) =
        common::make_engine_with_caller(Arc::clone(&caller2), pools.clone());
    let replayer2 = StateReplayer::new(engine2);
    let events2 = replayer2.replay(blocks.clone()).await.expect("run2 ok");

    assert_eq!(
        events1, events2,
        "Replay Gate violated: same input produced different output"
    );
    assert_eq!(events1.len(), blocks.len() * pools.len());
}
