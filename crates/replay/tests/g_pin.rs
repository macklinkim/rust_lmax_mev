//! G-Pin — block-hash pinning negative coverage.
//!
//! Feed `RecordedEthCaller` directly with `BlockId::Number(_)` (non-Hash
//! variant) and an unknown `BlockId::Hash(_)`; both must return
//! `NodeError::Rpc("unexpected block_id ...")`.

use alloy::eips::{BlockId, BlockNumberOrTag, RpcBlockHash};
use alloy::rpc::types::eth::TransactionRequest;
use alloy_primitives::{address, Bytes, B256};
use rust_lmax_mev_node::NodeError;
use rust_lmax_mev_state::{EthCaller, SELECTOR_GET_RESERVES};

mod common;

#[tokio::test]
async fn recorded_caller_rejects_non_hash_block_id() {
    let blocks = common::blocks();
    let caller = common::build_caller(&blocks);
    let req = TransactionRequest::default()
        .to(common::POOL_V2)
        .input(Bytes::from(SELECTOR_GET_RESERVES.to_vec()).into());
    let bad = BlockId::Number(BlockNumberOrTag::Latest);
    let err = caller
        .eth_call_at_block(req, bad)
        .await
        .expect_err("non-Hash BlockId must be rejected");
    match err {
        NodeError::Rpc(msg) => {
            assert!(
                msg.contains("non-hash"),
                "expected non-hash message, got: {msg}"
            );
        }
        other => panic!("expected Rpc, got {other:?}"),
    }
}

#[tokio::test]
async fn recorded_caller_rejects_unknown_block_hash() {
    let blocks = common::blocks();
    let caller = common::build_caller(&blocks);
    let req = TransactionRequest::default()
        .to(common::POOL_V2)
        .input(Bytes::from(SELECTOR_GET_RESERVES.to_vec()).into());
    // A hash NOT in the recorded set (build_caller registers [1;32]..[5;32]).
    let unknown_hash = B256::from([0xEE; 32]);
    let err = caller
        .eth_call_at_block(
            req,
            BlockId::Hash(RpcBlockHash::from_hash(unknown_hash, None)),
        )
        .await
        .expect_err("unknown block_hash must be rejected");
    match err {
        NodeError::Rpc(msg) => {
            assert!(
                msg.contains("unexpected block_id hash"),
                "expected unknown-hash message, got: {msg}"
            );
        }
        other => panic!("expected Rpc, got {other:?}"),
    }
}

#[tokio::test]
async fn recorded_caller_missing_fixture_does_not_append_to_witness() {
    // Use a known block hash but a pool address that has no fixture.
    let blocks = common::blocks();
    let caller = common::build_caller(&blocks);
    let recorded_hash = blocks[0].hash;
    let unknown_pool = address!("dddddddddddddddddddddddddddddddddddddddd");
    let req = TransactionRequest::default()
        .to(unknown_pool)
        .input(Bytes::from(SELECTOR_GET_RESERVES.to_vec()).into());
    let baseline_witness_len = caller.witness().len();
    let err = caller
        .eth_call_at_block(
            req,
            BlockId::Hash(RpcBlockHash::from_hash(recorded_hash, None)),
        )
        .await
        .expect_err("missing fixture must error");
    match err {
        NodeError::Rpc(msg) => {
            assert!(
                msg.contains("no fixture"),
                "expected no-fixture message, got: {msg}"
            );
        }
        other => panic!("expected Rpc, got {other:?}"),
    }
    assert_eq!(
        caller.witness().len(),
        baseline_witness_len,
        "missing-fixture lookup MUST NOT append to witness (per RecordedEthCaller docs)"
    );
}
