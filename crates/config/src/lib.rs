//! Phase 1 typed TOML configuration loader for the LMAX-style MEV engine.
//!
//! Per Batch A execution note (`docs/superpowers/plans/2026-05-02-phase-1-
//! batch-a-foundation-execution.md`). Authoritative sources: ADR-007
//! (Node Topology + Fallback RPC), ADR-008 (Observability + CI Baseline),
//! ADR-003 (Mempool/Relay/Persistence; FileJournal + RocksDbSnapshot
//! paths).
//!
//! - [`Config::load`] parses a TOML file at `path`, applies env-overlay
//!   under prefix `RUST_LMAX_MEV` with `__` separator (config-rs default),
//!   then validates ADR-007's "≥ 1 fallback RPC" invariant.
//! - [`Config::from_toml_str`] is the in-memory test-friendly variant
//!   (no env overlay, no filesystem).
//!
//! All sub-config structs use `#[serde(deny_unknown_fields)]` per
//! execution-note Risk Decision 3 so a TOML typo surfaces as a parse
//! error rather than a silent default.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use serde::Deserialize;

const ENV_PREFIX: &str = "RUST_LMAX_MEV";
const ENV_SEPARATOR: &str = "__";

/// Top-level engine configuration.
///
/// Layered loading: a single TOML file, then environment-variable overlay
/// using prefix `RUST_LMAX_MEV` and separator `__`. Required-field
/// validation runs after the merge.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub node: NodeConfig,
    pub observability: ObservabilityConfig,
    pub journal: JournalConfig,
}

/// Node topology section per ADR-007. Primary Geth WS + HTTP plus at least
/// one fallback HTTP provider URL.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NodeConfig {
    pub geth_ws_url: String,
    pub geth_http_url: String,
    /// Fallback HTTP RPC providers per ADR-007. **Length must be ≥ 1**;
    /// `Config::validate` (called from `load` and `from_toml_str`) rejects
    /// an empty list with `ConfigError::MissingFallbackRpc`.
    pub fallback_rpc: Vec<FallbackRpcConfig>,
}

/// One fallback HTTP RPC provider entry per ADR-007.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FallbackRpcConfig {
    pub url: String,
    pub label: String,
}

/// Observability section per ADR-008.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ObservabilityConfig {
    /// Prometheus exporter listen socket. ADR-008 default `0.0.0.0:9090`;
    /// operators can narrow to loopback via TOML or env overlay.
    pub prometheus_listen: SocketAddr,
    /// `RUST_LOG`-style filter directive consumed by
    /// `tracing_subscriber::EnvFilter`.
    pub log_filter: String,
    /// `Json` for production, `Pretty` for dev/test.
    pub log_format: LogFormat,
}

/// Log output format selector.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LogFormat {
    Json,
    Pretty,
}

/// Journal section per ADR-003. Both paths are filesystem-bound; the
/// crate does NOT validate existence at load time (that's the job of
/// `crates/journal::FileJournal::open` and `RocksDbSnapshot::open`,
/// which create or validate on first use).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct JournalConfig {
    pub file_journal_path: PathBuf,
    pub rocksdb_snapshot_path: PathBuf,
}

/// All errors produced by [`Config::load`] and [`Config::from_toml_str`].
///
/// `#[non_exhaustive]` so Phase 2 may add variants additively without
/// breaking downstream pattern matches.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Filesystem I/O error opening or reading the TOML file.
    #[error("config I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// config-rs deserialization failure (TOML syntax, type mismatch,
    /// unknown key under `#[serde(deny_unknown_fields)]`, env-overlay
    /// type coercion).
    #[error("config parse error: {0}")]
    Parse(#[from] config::ConfigError),

    /// ADR-007 invariant: at least one fallback HTTP RPC provider must
    /// be configured.
    #[error("node.fallback_rpc must contain at least one entry per ADR-007")]
    MissingFallbackRpc,

    /// A required scalar field is empty / whitespace-only after the
    /// env-overlay merge. Surfaces typos that bypass `deny_unknown_fields`
    /// because the field is recognized but blank.
    #[error("required field is empty or whitespace-only: {field}")]
    EmptyRequiredField { field: &'static str },

    /// `prometheus_listen` could not be parsed as a `SocketAddr`. config-rs
    /// surfaces this as a `Parse` variant in practice; this variant is
    /// reserved for callers that want a more specific match in the future.
    #[error("invalid socket address for {field}: {reason}")]
    InvalidSocketAddr { field: &'static str, reason: String },
}

impl Config {
    /// Loads `Config` from a TOML file at `path`, then overlays
    /// environment variables prefixed `RUST_LMAX_MEV` using `__` as the
    /// nested-key separator (config-rs default). The merged Config is
    /// then validated per [`Config::validate`].
    ///
    /// Lists (e.g., `node.fallback_rpc`) are NOT env-overlay-friendly:
    /// the TOML file is the source of truth for list contents.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path_ref = path.as_ref();
        let builder = config::Config::builder()
            .add_source(config::File::from(path_ref))
            .add_source(config::Environment::with_prefix(ENV_PREFIX).separator(ENV_SEPARATOR));
        let raw = builder.build()?;
        let cfg: Config = raw.try_deserialize()?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Pure-in-memory variant for unit tests. Parses a TOML string with
    /// no env overlay and no filesystem touch, then validates.
    pub fn from_toml_str(s: &str) -> Result<Self, ConfigError> {
        let raw = config::Config::builder()
            .add_source(config::File::from_str(s, config::FileFormat::Toml))
            .build()?;
        let cfg: Config = raw.try_deserialize()?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Validates ADR-007's "≥ 1 fallback RPC" invariant and the empty-
    /// scalar guard. Called automatically from `load` and `from_toml_str`;
    /// exposed publicly for callers that construct `Config` programmatically.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.node.fallback_rpc.is_empty() {
            return Err(ConfigError::MissingFallbackRpc);
        }
        if self.node.geth_ws_url.trim().is_empty() {
            return Err(ConfigError::EmptyRequiredField {
                field: "node.geth_ws_url",
            });
        }
        if self.node.geth_http_url.trim().is_empty() {
            return Err(ConfigError::EmptyRequiredField {
                field: "node.geth_http_url",
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Canonical minimum-valid TOML used by C-1 and C-3.
    fn valid_minimum_toml() -> &'static str {
        r#"
[node]
geth_ws_url = "ws://localhost:8546"
geth_http_url = "http://localhost:8545"

[[node.fallback_rpc]]
url = "https://eth-mainnet.g.alchemy.com/v2/demo"
label = "alchemy"

[observability]
prometheus_listen = "0.0.0.0:9090"
log_filter = "info"
log_format = "json"

[journal]
file_journal_path = "/var/lib/lmax/journal.log"
rocksdb_snapshot_path = "/var/lib/lmax/snapshot"
"#
    }

    /// C-1 (happy): minimum valid TOML with all 3 sections + 1 fallback RPC
    /// parses into `Config` with the expected field values.
    #[test]
    fn from_toml_str_round_trips_minimum_valid_config() {
        let cfg =
            Config::from_toml_str(valid_minimum_toml()).expect("minimum valid TOML must parse");
        assert_eq!(cfg.node.geth_ws_url, "ws://localhost:8546");
        assert_eq!(cfg.node.geth_http_url, "http://localhost:8545");
        assert_eq!(cfg.node.fallback_rpc.len(), 1);
        assert_eq!(cfg.node.fallback_rpc[0].label, "alchemy");
        assert_eq!(
            cfg.observability.prometheus_listen,
            "0.0.0.0:9090".parse::<SocketAddr>().unwrap()
        );
        assert_eq!(cfg.observability.log_filter, "info");
        assert_eq!(cfg.observability.log_format, LogFormat::Json);
        assert_eq!(
            cfg.journal.file_journal_path,
            PathBuf::from("/var/lib/lmax/journal.log")
        );
        assert_eq!(
            cfg.journal.rocksdb_snapshot_path,
            PathBuf::from("/var/lib/lmax/snapshot")
        );
    }

    /// C-2 (failure): `fallback_rpc = []` → `Err(ConfigError::
    /// MissingFallbackRpc)`. Asserts the ADR-007 invariant.
    #[test]
    fn from_toml_str_rejects_empty_fallback_rpc_list() {
        let toml = r#"
[node]
geth_ws_url = "ws://localhost:8546"
geth_http_url = "http://localhost:8545"
fallback_rpc = []

[observability]
prometheus_listen = "0.0.0.0:9090"
log_filter = "info"
log_format = "json"

[journal]
file_journal_path = "/tmp/journal.log"
rocksdb_snapshot_path = "/tmp/snapshot"
"#;
        let err = Config::from_toml_str(toml).unwrap_err();
        assert!(
            matches!(err, ConfigError::MissingFallbackRpc),
            "expected MissingFallbackRpc, got {err:?}"
        );
    }

    /// C-3 (boundary): `Config::load(tempdir TOML)` with the env override
    /// `RUST_LMAX_MEV__OBSERVABILITY__LOG_FILTER=trace` set yields
    /// `cfg.observability.log_filter == "trace"` (overrides the TOML
    /// default `"info"`). Asserts the env-overlay precedence contract.
    ///
    /// Cleans up the env var on test exit per Codex implementation hint
    /// (see codex_review.md 2026-05-02 15:30:24) so the var does not
    /// pollute later tests in the same process.
    #[test]
    fn load_overlays_env_var_over_toml_value() {
        const ENV_VAR: &str = "RUST_LMAX_MEV__OBSERVABILITY__LOG_FILTER";

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("engine.toml");
        std::fs::write(&path, valid_minimum_toml()).unwrap();

        // Drop guard ensures we always remove the env var, even on panic.
        struct EnvGuard(&'static str);
        impl Drop for EnvGuard {
            fn drop(&mut self) {
                // SAFETY: single-threaded test; this is the only writer of
                // the variable in this process.
                unsafe { std::env::remove_var(self.0) }
            }
        }
        // SAFETY: see EnvGuard.
        unsafe { std::env::set_var(ENV_VAR, "trace") }
        let _guard = EnvGuard(ENV_VAR);

        let cfg = Config::load(&path).expect("load must succeed with env overlay");
        assert_eq!(
            cfg.observability.log_filter, "trace",
            "env overlay must override TOML's `info`"
        );
        // Other fields unaffected by the overlay.
        assert_eq!(cfg.node.fallback_rpc.len(), 1);
        assert_eq!(cfg.observability.log_format, LogFormat::Json);
    }
}
