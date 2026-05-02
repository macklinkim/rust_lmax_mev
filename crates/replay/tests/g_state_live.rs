//! G-State live — env-contract STUB ONLY (NOT the Phase 2 EXIT proof).
//!
//! `#[ignore]`'d in CI. Reads `MEV_LIVE_NODE_URL` from env at runtime.
//! Phase 2 ships only the env-contract assertion (URL parses); the
//! actual live-comparison loop against a real Geth node is Phase 4
//! hardening per the P2-C execution note v0.2 Risk Decision 4.

#[test]
#[ignore]
fn engine_matches_live_node_at_block_hash() {
    let url = std::env::var("MEV_LIVE_NODE_URL")
        .expect("MEV_LIVE_NODE_URL must be set for the live smoke");
    assert!(
        url.starts_with("http://") || url.starts_with("https://"),
        "MEV_LIVE_NODE_URL scheme must be http:// or https://; got: {url}"
    );
    // Phase 4 hardening will replace this stub with a real
    // alloy-based comparison against StateEngine output.
}
