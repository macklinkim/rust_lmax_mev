//! P4-E RC-SUBMIT-DISABLED-1/2 (R-E10): explicit-wrapper concrete-
//! adapter verification that every `BundleRelay::submit_bundle` impl
//! returns `Err(SubmitDisabled)` regardless of input. This is the
//! type-system-independent check that does NOT rely on trait-object
//! upcast (which is unstable on Rust 1.80 stable per DP-E13 v0.3).

use alloy_primitives::{Address, U256};
use rust_lmax_mev_bundle_relay::{BundleRelay, BundleRelayError, SignedBundle};
use rust_lmax_mev_relay_clients::{
    BloxrouteConfig, BloxrouteRelay, FlashbotsConfig, FlashbotsRelay,
};

fn dummy_bundle() -> SignedBundle {
    SignedBundle {
        block_hash: [0u8; 32],
        state_block_number: 0,
        signed_txs: vec![vec![0xAB]],
        coinbase_recipient: Address::ZERO,
        coinbase_transfer_wei: U256::ZERO,
        validity_block_min: 0,
        validity_block_max: 0,
    }
}

#[tokio::test]
async fn submit_disabled_1_flashbots() {
    let r = FlashbotsRelay::new(FlashbotsConfig::default()).expect("ctor ok");
    match r.submit_bundle(&dummy_bundle()).await {
        Err(BundleRelayError::SubmitDisabled) => {}
        other => panic!("Flashbots submit_bundle must return SubmitDisabled; got {other:?}"),
    }
}

#[tokio::test]
async fn submit_disabled_2_bloxroute() {
    let r = BloxrouteRelay::new(BloxrouteConfig::default()).expect("ctor ok");
    match r.submit_bundle(&dummy_bundle()).await {
        Err(BundleRelayError::SubmitDisabled) => {}
        other => panic!("bloXroute submit_bundle must return SubmitDisabled; got {other:?}"),
    }
}
