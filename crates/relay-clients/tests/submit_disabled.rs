//! P4-E RC-SUBMIT-DISABLED-1/2 (R-E10): explicit-wrapper concrete-
//! adapter verification that every `BundleRelay::submit_bundle` impl
//! returns `Err(SubmitDisabled)` regardless of input. This is the
//! type-system-independent check that does NOT rely on trait-object
//! upcast (which is unstable on Rust 1.80 stable per DP-E13 v0.3).

use alloy_primitives::{Address, U256};
use rust_lmax_mev_bundle_relay::{BundleRelay, BundleRelayError, KillSwitch, SignedBundle};
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
    let r =
        FlashbotsRelay::new(FlashbotsConfig::default(), KillSwitch::new(false)).expect("ctor ok");
    match r.submit_bundle(&dummy_bundle()).await {
        Err(BundleRelayError::SubmitDisabled) => {}
        other => panic!("Flashbots submit_bundle must return SubmitDisabled; got {other:?}"),
    }
}

#[tokio::test]
async fn submit_disabled_2_bloxroute() {
    let r =
        BloxrouteRelay::new(BloxrouteConfig::default(), KillSwitch::new(false)).expect("ctor ok");
    match r.submit_bundle(&dummy_bundle()).await {
        Err(BundleRelayError::SubmitDisabled) => {}
        other => panic!("bloXroute submit_bundle must return SubmitDisabled; got {other:?}"),
    }
}

/// P6-D D-T-D1: PRECEDENCE proof — Flashbots active KillSwitch returns
/// KillSwitchActive BEFORE SubmitDisabled (boundary-spec §3 PRECEDENCE).
#[tokio::test]
async fn submit_disabled_3_flashbots_kill_switch_active_takes_precedence() {
    let r =
        FlashbotsRelay::new(FlashbotsConfig::default(), KillSwitch::new(true)).expect("ctor ok");
    match r.submit_bundle(&dummy_bundle()).await {
        Err(BundleRelayError::KillSwitchActive) => {}
        other => panic!(
            "Flashbots submit_bundle with active KillSwitch must return \
             KillSwitchActive (NOT SubmitDisabled); got {other:?}"
        ),
    }
}

/// P6-D D-T-D2: mirror of D-T-D1 for BloxrouteRelay.
#[tokio::test]
async fn submit_disabled_4_bloxroute_kill_switch_active_takes_precedence() {
    let r =
        BloxrouteRelay::new(BloxrouteConfig::default(), KillSwitch::new(true)).expect("ctor ok");
    match r.submit_bundle(&dummy_bundle()).await {
        Err(BundleRelayError::KillSwitchActive) => {}
        other => panic!(
            "bloXroute submit_bundle with active KillSwitch must return \
             KillSwitchActive (NOT SubmitDisabled); got {other:?}"
        ),
    }
}

/// P6-D D-T-D3: shared-state proof — KillSwitch::clone() across the
/// adapter boundary; flip from outside the adapter is observed by the
/// adapter on the next submit_bundle call. Proves the Arc<AtomicBool>
/// semantics hold across the ctor seam.
///
/// Phase 1: inactive KS → SubmitDisabled (also covers Flashbots
///   inactive-baseline; no separate Flashbots inactive-baseline D-T-D).
/// Phase 2: operator flips the original handle.
/// Phase 3: adapter observes the flip → KillSwitchActive.
///
/// Asymmetric coverage: Flashbots only. bloXroute generalizes via the
/// identical guard idiom (verified by G10) + KS-1/KS-2 baseline
/// `KillSwitch::clone()` semantics in `crates/bundle-relay/`.
#[tokio::test]
async fn submit_disabled_5_flashbots_shared_kill_switch_flip_visible() {
    let ks = KillSwitch::new(false);
    let ks_clone = ks.clone();
    let r = FlashbotsRelay::new(FlashbotsConfig::default(), ks_clone).expect("ctor ok");

    // Phase 1: inactive baseline.
    match r.submit_bundle(&dummy_bundle()).await {
        Err(BundleRelayError::SubmitDisabled) => {}
        other => panic!("pre-flip: expected SubmitDisabled, got {other:?}"),
    }

    // Phase 2: operator flips the switch from the held instance.
    ks.set_active(true);

    // Phase 3: adapter sees the flip via shared Arc<AtomicBool>.
    match r.submit_bundle(&dummy_bundle()).await {
        Err(BundleRelayError::KillSwitchActive) => {}
        other => panic!(
            "post-flip: expected KillSwitchActive (shared-state proof failed); \
             got {other:?}"
        ),
    }
}
