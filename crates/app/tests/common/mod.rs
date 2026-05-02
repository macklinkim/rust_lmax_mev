//! Shared helpers for the `rust-lmax-mev-app` integration tests.
//!
//! Each integration test file (`run_happy.rs`, `run_invalid_path.rs`,
//! `run_double_init.rs`) is compiled into its own test binary, which
//! gives each test process-level isolation of the global tracing
//! subscriber and Prometheus recorder. The helpers below build a
//! tempdir-backed `Config` so a test can wire the engine without
//! requiring a checked-in TOML or any host filesystem state.

// Each integration-test file is compiled as its own crate, so items only
// used by a subset of those files surface as dead-code warnings in the
// crates that don't use them. `#[allow(dead_code)]` is the conventional
// fix for shared `tests/common/mod.rs` helpers.
#![allow(dead_code)]

use std::net::SocketAddr;
use std::path::PathBuf;

use rust_lmax_mev_config::{
    BusConfig, Config, FallbackRpcConfig, JournalConfig, LogFormat, NodeConfig, ObservabilityConfig,
};

/// Builds a `Config` whose journal + snapshot paths live under
/// `tempdir`, with bus capacity 8 and Prometheus listening on
/// `127.0.0.1:0` (OS picks a free port — important so parallel cargo
/// runs across crates do not collide).
pub fn make_config(tempdir: &std::path::Path) -> Config {
    Config {
        node: NodeConfig {
            geth_ws_url: "ws://localhost:8546".to_string(),
            geth_http_url: "http://localhost:8545".to_string(),
            fallback_rpc: vec![FallbackRpcConfig {
                url: "http://localhost:8545".to_string(),
                label: "local".to_string(),
            }],
        },
        observability: ObservabilityConfig {
            prometheus_listen: SocketAddr::from(([127, 0, 0, 1], 0)),
            log_filter: "info".to_string(),
            log_format: LogFormat::Pretty,
        },
        journal: JournalConfig {
            file_journal_path: tempdir.join("journal.log"),
            rocksdb_snapshot_path: tempdir.join("snapshot"),
        },
        bus: BusConfig { capacity: 8 },
    }
}

/// Returns `(file_journal_path, rocksdb_snapshot_path)` for a tempdir.
pub fn paths(tempdir: &std::path::Path) -> (PathBuf, PathBuf) {
    (tempdir.join("journal.log"), tempdir.join("snapshot"))
}
