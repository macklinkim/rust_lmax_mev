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
    BusConfig, Config, FallbackRpcConfig, IngressConfig, IngressTokens, JournalConfig, LogFormat,
    NodeConfig, ObservabilityConfig, PoolConfig, PoolKind, StateConfig,
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
            archive_rpc: None,
        },
        observability: ObservabilityConfig {
            prometheus_listen: SocketAddr::from(([127, 0, 0, 1], 0)),
            log_filter: "info".to_string(),
            log_format: LogFormat::Pretty,
        },
        journal: JournalConfig {
            file_journal_path: tempdir.join("journal.log"),
            rocksdb_snapshot_path: tempdir.join("snapshot"),
            ingress_journal_path: tempdir.join("ingress.log"),
            state_journal_path: tempdir.join("state.log"),
        },
        bus: BusConfig { capacity: 8 },
        ingress: IngressConfig {
            tokens: IngressTokens {
                weth: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
                    .parse()
                    .unwrap(),
                usdc: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
                    .parse()
                    .unwrap(),
            },
            watched_addresses: vec!["0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc"
                .parse()
                .unwrap()],
        },
        state: StateConfig {
            pools: vec![PoolConfig {
                kind: PoolKind::UniswapV2,
                address: "0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc"
                    .parse()
                    .unwrap(),
            }],
        },
    }
}

/// Returns `(file_journal_path, rocksdb_snapshot_path)` for a tempdir.
pub fn paths(tempdir: &std::path::Path) -> (PathBuf, PathBuf) {
    (tempdir.join("journal.log"), tempdir.join("snapshot"))
}
