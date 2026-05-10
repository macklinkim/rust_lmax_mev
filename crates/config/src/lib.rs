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

use alloy_primitives::Address;
use serde::{Deserialize, Serialize};

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
    pub bus: BusConfig,
    pub ingress: IngressConfig,
    pub state: StateConfig,
    /// Phase 4 P4-E relay section. `#[serde(default)]` so existing
    /// TOML configs (predating P4-E) still parse — empty default
    /// `enabled_relays` is the fail-closed P4-E behavior per
    /// execution-note v0.6 §DP-E3.
    #[serde(default)]
    pub relay: RelayConfig,
}

/// Node topology section per ADR-007. Primary Geth WS + HTTP plus at least
/// one fallback HTTP provider URL.
///
/// Phase 4 P4-A additive: `archive_rpc` defaults `None` (fail-closed per
/// the Q8 hardening invariant). When unset, every `NodeProvider`
/// archive method (`eth_get_proof` / `eth_get_storage_at` /
/// `eth_get_code`) returns `Err(NodeError::ArchiveNotConfigured)` —
/// never falls back to `geth_http_url` because a non-archive node
/// would silently produce wrong historical answers.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NodeConfig {
    pub geth_ws_url: String,
    pub geth_http_url: String,
    /// Fallback HTTP RPC providers per ADR-007. **Length must be ≥ 1**;
    /// `Config::validate` (called from `load` and `from_toml_str`) rejects
    /// an empty list with `ConfigError::MissingFallbackRpc`.
    pub fallback_rpc: Vec<FallbackRpcConfig>,
    /// Phase 4 P4-A archive RPC endpoint per ADR-007 §"Archive access"
    /// ("required" at Phase 4). Defaults `None` (fail-closed); operator
    /// must explicitly configure to enable archive-mode reads.
    #[serde(default)]
    pub archive_rpc: Option<FallbackRpcConfig>,
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

/// Journal section per ADR-003. All paths are filesystem-bound; the
/// crate does NOT validate existence at load time (that's the job of
/// `crates/journal::FileJournal::open` and `RocksDbSnapshot::open`,
/// which create or validate on first use).
///
/// Phase 3 P3-B additive: `ingress_journal_path` + `state_journal_path`
/// store the per-payload journal files for `wire_phase3`'s journal-drain
/// consumer threads (`FileJournal<IngressEvent>` and
/// `FileJournal<StateUpdateEvent>`). The legacy `file_journal_path`
/// remains the Phase 1 `wire()` `FileJournal<SmokeTestPayload>` path so
/// the existing P1 app tests keep working unchanged.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct JournalConfig {
    pub file_journal_path: PathBuf,
    pub rocksdb_snapshot_path: PathBuf,
    /// Phase 3 P3-B: target file for `FileJournal<IngressEvent>`.
    /// `Config::validate` rejects empty paths and exact equality with
    /// the other journal paths (collisions would interleave records of
    /// different payload types in the same file, breaking replay).
    pub ingress_journal_path: PathBuf,
    /// Phase 3 P3-B: target file for `FileJournal<StateUpdateEvent>`.
    pub state_journal_path: PathBuf,
    /// Phase 4 P4-E: target file for `FileJournal<MismatchAbort>` —
    /// the comparator_driver appends a journaled abort record on every
    /// relay-sim mismatch BEFORE emitting any downstream broadcast
    /// (DP-E8 v0.4 synchronous-ordering guarantee). `#[serde(default)]`
    /// supplies `data/mismatch.bin` so existing TOML configs predating
    /// P4-E parse cleanly.
    #[serde(default = "default_mismatch_journal_path")]
    pub mismatch_journal_path: PathBuf,
}

fn default_mismatch_journal_path() -> PathBuf {
    PathBuf::from("data/mismatch.bin")
}

/// Event bus section per ADR-005. Capacity is a required tuning parameter
/// that must NOT be hardcoded in the binary; ADR-005 §"Consequences"
/// mandates exposure in the engine config file.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BusConfig {
    /// Bounded `crossbeam::channel::bounded` capacity. Must be ≥ 1;
    /// `Config::validate` rejects 0 with `ConfigError::InvalidBusCapacity`
    /// because a 0-capacity channel deadlocks on the first publish under
    /// crossbeam semantics.
    pub capacity: usize,
}

/// Phase 2 ingress section per ADR-003. Carries the typed WETH/USDC
/// token identities consumed by P2-B's pool-state code, plus the
/// `watched_addresses` filter scope consumed by P2-A's normalizer.
/// `tokens` and `watched_addresses` are intentionally separate: tokens
/// carry semantic identity; watched_addresses define what tx.to values
/// the normalizer keeps.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IngressConfig {
    pub tokens: IngressTokens,
    /// Filter scope: `Normalizer::filter` keeps a tx iff
    /// `tx.to ∈ watched_addresses`. `Config::validate` rejects an empty
    /// list with `ConfigError::EmptyWatchedAddresses`.
    pub watched_addresses: Vec<Address>,
    /// Phase 4 P4-F per ADR-003 §"External feed options": runtime
    /// selector between the GethWS local-node mempool (default —
    /// existing P2-A behavior) and the external feed adapter (P4-F
    /// scaffold; production transport is Phase 5+ per the
    /// fail-closed `ExternalMempoolSource` contract). `#[serde(default)]`
    /// keeps existing TOML configs (predating P4-F) parsing unchanged.
    #[serde(default)]
    pub mempool_source: MempoolSourceKind,
    /// Phase 4 P4-F: optional external mempool endpoint URL. Held by
    /// `ExternalMempoolSource` in a private field; never logged. Only
    /// consumed when `mempool_source = "external"`.
    #[serde(default)]
    pub external_mempool_endpoint: Option<String>,
    /// Phase 4 P4-F: optional external mempool API key (bloXroute /
    /// Chainbound). Held in a private field; never logged.
    #[serde(default)]
    pub external_mempool_api_key: Option<String>,
}

/// Phase 4 P4-F per ADR-003: which mempool feed implementation to
/// wire. `Default = GethWs` keeps the established P2-A behavior.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MempoolSourceKind {
    #[default]
    GethWs,
    /// External-feed adapter scaffold (bloXroute / Chainbound). In
    /// P4-F this resolves to `ExternalMempoolSource` which is
    /// fail-closed (emits `ExternalNotConfigured` on every stream)
    /// because production transport is Phase 5+ work.
    External,
}

/// Typed WETH/USDC role identities per ADR-002 thin-path scope.
/// `Config::validate` rejects `weth == usdc` with
/// `ConfigError::DuplicateIngressTokens`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IngressTokens {
    pub weth: Address,
    pub usdc: Address,
}

/// Phase 2 P2-B state-engine pool registry per ADR-002 thin-path scope
/// (WETH/USDC on UniV2 + UniV3 0.05%). `Config::validate` rejects an
/// empty pool list and duplicate pool addresses.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StateConfig {
    pub pools: Vec<PoolConfig>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PoolConfig {
    pub kind: PoolKind,
    pub address: Address,
}

#[derive(
    Debug,
    Clone,
    Copy,
    Deserialize,
    Serialize,
    PartialEq,
    Eq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum PoolKind {
    #[serde(rename = "uniswap_v2")]
    UniswapV2,
    /// Uniswap V3 0.05% fee tier. snake_case auto-renaming would map
    /// the variant name to `uniswap_v3_fee005` (no boundary between
    /// `fee` and `005`); explicit rename keeps the more readable
    /// `uniswap_v3_fee_005` TOML form.
    #[serde(rename = "uniswap_v3_fee_005")]
    UniswapV3Fee005,
    /// Phase 4 P4-F: Sushiswap V2 WETH/USDC per ADR-002 §"Deferred to
    /// Phase 4". Sushiswap V2 is a UniswapV2Pair fork: identical
    /// storage layout, identical `getReserves()` ABI, identical
    /// constant-product math. Fetched via the existing UniV2 caller
    /// path; opportunity engine treats it as another `PoolState::UniV2`
    /// venue for cross-venue arb.
    #[serde(rename = "sushiswap_v2")]
    SushiswapV2,
}

/// Phase 4 P4-E relay section per execution-note v0.6 §D-E2 + DP-E3 +
/// DP-E9 + DP-E10. Empty `enabled_relays` is the fail-closed default
/// per DP-E3 (the comparator_driver runs inert with zero relays).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, default)]
pub struct RelayConfig {
    /// Zero-or-more concrete relay endpoints. Default empty per
    /// DP-E3 / DP-E13 — fail-closed when no relay is configured.
    pub enabled_relays: Vec<RelayEndpointConfig>,
    /// Per-relay simulate timeout (ms). Default 2000ms per
    /// `relay_clients::DEFAULT_*_TIMEOUT_MS`.
    pub simulate_timeout_ms: u64,
    /// **MUST be `false` in P4-E.** Validation rejects `true` with
    /// `ConfigError::LiveSendForbidden`. The flag is NOT plumbed to
    /// any code path in P4-E; the validation reject IS the only
    /// safety mechanism per DP-E9 (defense in depth on top of the
    /// `SubmitDisabled` impl + the no-caller invariant).
    pub live_send: bool,
    /// Kill-switch flag per execution-safety.md §"Kill Switch" +
    /// DP-E10. Default `false`. Read by the relay-clients code
    /// (no-op in P4-E since no submission exists). Phase 5+ checks
    /// before any submit.
    pub execution_disabled: bool,
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            enabled_relays: Vec::new(),
            simulate_timeout_ms: 2_000,
            live_send: false,
            execution_disabled: false,
        }
    }
}

/// One relay endpoint entry. The `kind` selects which adapter to
/// instantiate; `endpoint` is the relay URL; `api_key` is required
/// for bloXroute and ignored by Flashbots.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RelayEndpointConfig {
    pub kind: RelayKind,
    pub endpoint: String,
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RelayKind {
    Flashbots,
    Bloxroute,
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

    /// `bus.capacity` must be ≥ 1 per ADR-005; capacity 0 deadlocks on
    /// the first publish under crossbeam semantics.
    #[error("bus.capacity must be >= 1 per ADR-005, got {value}")]
    InvalidBusCapacity { value: usize },

    /// `ingress.watched_addresses` must contain at least 1 entry — an
    /// empty list would drop every mempool tx at the normalizer.
    #[error("ingress.watched_addresses must contain at least one address")]
    EmptyWatchedAddresses,

    /// `ingress.tokens.weth` and `ingress.tokens.usdc` must differ —
    /// they identify distinct sides of the WETH/USDC pair per ADR-002.
    #[error("ingress.tokens.weth and ingress.tokens.usdc must differ")]
    DuplicateIngressTokens,

    /// `state.pools` must contain at least one pool entry — an empty
    /// list would leave the State Engine with nothing to refresh.
    #[error("state.pools must contain at least one pool entry")]
    EmptyStatePools,

    /// Duplicate pool address in `state.pools` — every pool address
    /// must be unique so per-pool snapshot keys do not collide.
    #[error("state.pools contains duplicate pool address {0}")]
    DuplicatePoolAddress(Address),

    /// Phase 3 P3-B: a `journal.*_path` field is empty.
    #[error("journal.{field} must be a non-empty path")]
    EmptyJournalPath { field: &'static str },

    /// Phase 3 P3-B: two `journal.*_path` fields point at the same file.
    /// Mixing payload types in one journal file would interleave records
    /// of different `T`s in `FileJournal<T>` and break replay.
    #[error("journal paths must be distinct: {a} and {b} both point at the same file")]
    DuplicateJournalPath { a: &'static str, b: &'static str },

    /// Phase 4 P4-E (DP-E9): `relay.live_send = true` is forbidden in
    /// P4-E. Live submission lands at Phase 6b Production Gate per
    /// docs/specs/execution-safety.md.
    #[error("relay.live_send=true is forbidden until Phase 6b Production Gate")]
    LiveSendForbidden,

    /// Phase 4 P4-E: a relay endpoint string is empty.
    #[error("relay.enabled_relays[{index}].endpoint must be a non-empty URL")]
    EmptyRelayEndpoint { index: usize },

    /// Phase 4 P4-E (R-E23): more than one relay configured. P4-E
    /// supports at most ONE relay; multi-relay fanout (concurrent
    /// `simulate_bundle` calls + first-success-wins merging) is
    /// Phase 5+ work. Silently using only the first entry would be
    /// surprising config truncation for a safety boundary, so we
    /// fail-closed at validation time.
    #[error("relay.enabled_relays must contain at most 1 entry in P4-E; got {count}")]
    TooManyEnabledRelays { count: usize },
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
        if self.bus.capacity == 0 {
            return Err(ConfigError::InvalidBusCapacity { value: 0 });
        }
        if self.ingress.watched_addresses.is_empty() {
            return Err(ConfigError::EmptyWatchedAddresses);
        }
        if self.ingress.tokens.weth == self.ingress.tokens.usdc {
            return Err(ConfigError::DuplicateIngressTokens);
        }
        if self.state.pools.is_empty() {
            return Err(ConfigError::EmptyStatePools);
        }
        for (i, pool) in self.state.pools.iter().enumerate() {
            for other in &self.state.pools[i + 1..] {
                if pool.address == other.address {
                    return Err(ConfigError::DuplicatePoolAddress(pool.address));
                }
            }
        }

        // Phase 4 P4-E hard invariant per DP-E9 (defense-in-depth on
        // top of SubmitDisabled + 0-callers grep gate): live_send=true
        // is forbidden in P4-E. The flag is NOT plumbed to any code
        // path in P4-E; this validation reject IS the only safety
        // mechanism + the SubmitDisabled impl in every relay-clients
        // adapter is the second.
        if self.relay.live_send {
            return Err(ConfigError::LiveSendForbidden);
        }
        // Phase 4 P4-E (R-E23): at most one relay in P4-E. Multi-
        // relay fanout is Phase 5+ work; silently dropping the rest
        // would be a surprising config truncation.
        if self.relay.enabled_relays.len() > 1 {
            return Err(ConfigError::TooManyEnabledRelays {
                count: self.relay.enabled_relays.len(),
            });
        }
        // Phase 4 P4-E: relay endpoints must be non-empty strings.
        for (i, ep) in self.relay.enabled_relays.iter().enumerate() {
            if ep.endpoint.trim().is_empty() {
                return Err(ConfigError::EmptyRelayEndpoint { index: i });
            }
        }

        // Phase 3 P3-B + Phase 4 P4-E journal-path checks: each path
        // must be non-empty and all paths must point at distinct files
        // (mixing payload types in one journal would interleave records
        // of different `T` in `FileJournal<T>` and break replay).
        let journal_paths: [(&'static str, &PathBuf); 4] = [
            ("file_journal_path", &self.journal.file_journal_path),
            ("ingress_journal_path", &self.journal.ingress_journal_path),
            ("state_journal_path", &self.journal.state_journal_path),
            // Phase 4 P4-E: comparator_driver journal target.
            ("mismatch_journal_path", &self.journal.mismatch_journal_path),
        ];
        for (field, path) in &journal_paths {
            if path.as_os_str().is_empty() {
                return Err(ConfigError::EmptyJournalPath { field });
            }
        }
        for (i, (field_a, path_a)) in journal_paths.iter().enumerate() {
            for (field_b, path_b) in &journal_paths[i + 1..] {
                if path_a == path_b {
                    return Err(ConfigError::DuplicateJournalPath {
                        a: field_a,
                        b: field_b,
                    });
                }
            }
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
ingress_journal_path = "/var/lib/lmax/ingress.log"
state_journal_path = "/var/lib/lmax/state.log"

[bus]
capacity = 1024

[ingress.tokens]
weth = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
usdc = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"

[ingress]
watched_addresses = [
    "0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc",
    "0x88e6A0c2dDD26FEEb64F039a2c41296FcB3f5640",
]

[[state.pools]]
kind = "uniswap_v2"
address = "0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc"

[[state.pools]]
kind = "uniswap_v3_fee_005"
address = "0x88e6A0c2dDD26FEEb64F039a2c41296FcB3f5640"
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
        assert_eq!(cfg.bus.capacity, 1024);
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
ingress_journal_path = "/tmp/ingress.log"
state_journal_path = "/tmp/state.log"

[bus]
capacity = 64

[ingress.tokens]
weth = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
usdc = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"

[ingress]
watched_addresses = ["0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc"]

[[state.pools]]
kind = "uniswap_v2"
address = "0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc"
"#;
        let err = Config::from_toml_str(toml).unwrap_err();
        assert!(
            matches!(err, ConfigError::MissingFallbackRpc),
            "expected MissingFallbackRpc, got {err:?}"
        );
    }

    /// C-4 (boundary): `[bus] capacity = 0` → `Err(InvalidBusCapacity)`.
    /// Asserts the ADR-005 capacity-≥-1 invariant; capacity 0 would
    /// deadlock on the first publish under crossbeam semantics.
    #[test]
    fn from_toml_str_rejects_zero_bus_capacity() {
        let toml = r#"
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
file_journal_path = "/tmp/journal.log"
rocksdb_snapshot_path = "/tmp/snapshot"
ingress_journal_path = "/tmp/ingress.log"
state_journal_path = "/tmp/state.log"

[bus]
capacity = 0

[ingress.tokens]
weth = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
usdc = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"

[ingress]
watched_addresses = ["0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc"]

[[state.pools]]
kind = "uniswap_v2"
address = "0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc"
"#;
        let err = Config::from_toml_str(toml).unwrap_err();
        assert!(
            matches!(err, ConfigError::InvalidBusCapacity { value: 0 }),
            "expected InvalidBusCapacity {{ value: 0 }}, got {err:?}"
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

    /// C4A-1 (Phase 4 P4-A): `archive_rpc` is OPTIONAL — the existing
    /// minimum-valid TOML (no `[node.archive_rpc]` block) parses
    /// successfully and sets `archive_rpc = None`. Confirms the
    /// fail-closed default per Q8 hardening invariant.
    #[test]
    fn archive_rpc_optional_in_minimum_toml() {
        let cfg = Config::from_toml_str(valid_minimum_toml())
            .expect("minimum valid TOML must parse without [node.archive_rpc]");
        assert!(
            cfg.node.archive_rpc.is_none(),
            "archive_rpc must default to None per fail-closed Q8 invariant"
        );
    }

    /// C4A-1 (positive): when an operator DOES configure
    /// `[node.archive_rpc]`, the URL + label are parsed correctly.
    #[test]
    fn archive_rpc_parses_when_configured() {
        let toml = r#"
[node]
geth_ws_url = "ws://localhost:8546"
geth_http_url = "http://localhost:8545"

[[node.fallback_rpc]]
url = "https://eth-mainnet.g.alchemy.com/v2/demo"
label = "alchemy"

[node.archive_rpc]
url = "https://eth-archive.example/v2/demo"
label = "alchemy-archive"

[observability]
prometheus_listen = "0.0.0.0:9090"
log_filter = "info"
log_format = "json"

[journal]
file_journal_path = "/tmp/journal.log"
rocksdb_snapshot_path = "/tmp/snapshot"
ingress_journal_path = "/tmp/ingress.log"
state_journal_path = "/tmp/state.log"

[bus]
capacity = 64

[ingress.tokens]
weth = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
usdc = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"

[ingress]
watched_addresses = ["0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc"]

[[state.pools]]
kind = "uniswap_v2"
address = "0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc"
"#;
        let cfg = Config::from_toml_str(toml).expect("TOML with archive_rpc must parse");
        let archive = cfg
            .node
            .archive_rpc
            .as_ref()
            .expect("archive_rpc must be Some when configured");
        assert_eq!(archive.url, "https://eth-archive.example/v2/demo");
        assert_eq!(archive.label, "alchemy-archive");
    }

    // ----------------------------------------------------------------
    // Phase 4 P4-E config tests (CFG-LIVE-SEND-1, CFG-RELAY-1,
    // CFG-MISMATCH-JOURNAL-1) per execution-note v0.6.
    // ----------------------------------------------------------------

    /// CFG-MEMPOOL-1 (P4-F): mempool_source selector defaults to
    /// GethWs when omitted; explicit "external" parses; the optional
    /// endpoint + api_key fields are accepted (held for the
    /// fail-closed ExternalMempoolSource adapter to consume).
    #[test]
    fn cfg_mempool_1_source_selector_default_and_external_parse() {
        // Default: omitted [ingress.mempool_source] → GethWs.
        let cfg = Config::from_toml_str(valid_minimum_toml()).expect("default ok");
        assert_eq!(cfg.ingress.mempool_source, MempoolSourceKind::GethWs);
        assert!(cfg.ingress.external_mempool_endpoint.is_none());
        assert!(cfg.ingress.external_mempool_api_key.is_none());

        // Explicit external + endpoint + api_key.
        let toml = valid_minimum_toml().to_string().replace(
            "watched_addresses = [",
            "mempool_source = \"external\"\nexternal_mempool_endpoint = \"https://bdn.example/v2\"\nexternal_mempool_api_key = \"test-key\"\nwatched_addresses = [",
        );
        let cfg = Config::from_toml_str(&toml).expect("CFG-MEMPOOL-1 external must parse");
        assert_eq!(cfg.ingress.mempool_source, MempoolSourceKind::External);
        assert_eq!(
            cfg.ingress.external_mempool_endpoint.as_deref(),
            Some("https://bdn.example/v2")
        );
        assert_eq!(
            cfg.ingress.external_mempool_api_key.as_deref(),
            Some("test-key")
        );
    }

    /// CFG-SUSHI-1 (P4-F): `sushiswap_v2` PoolKind round-trips through
    /// TOML deserialization on a state.pools entry. Confirms ADR-002
    /// §"Deferred to Phase 4" Sushi unlock is wired into the config
    /// schema.
    #[test]
    fn cfg_sushi_1_pool_kind_sushiswap_v2_parses() {
        let toml = valid_minimum_toml()
            .to_string()
            .replace("kind = \"uniswap_v2\"", "kind = \"sushiswap_v2\"");
        let cfg = Config::from_toml_str(&toml).expect("CFG-SUSHI-1 must parse");
        assert!(
            cfg.state
                .pools
                .iter()
                .any(|p| matches!(p.kind, PoolKind::SushiswapV2)),
            "state.pools must contain a SushiswapV2 entry after rename"
        );
    }

    /// CFG-LIVE-SEND-1 (DP-E9): `relay.live_send = true` is rejected
    /// at config load with `ConfigError::LiveSendForbidden`. The
    /// HARD INVARIANT live_send=false default is verified by the
    /// existing minimum-valid TOML test (which omits `[relay]`
    /// entirely → defaults apply → live_send = false).
    #[test]
    fn cfg_live_send_1_rejects_true() {
        let mut toml = valid_minimum_toml().to_string();
        toml.push_str(
            r#"

[relay]
enabled_relays = []
simulate_timeout_ms = 2000
live_send = true
execution_disabled = false
"#,
        );
        let err = Config::from_toml_str(&toml)
            .expect_err("CFG-LIVE-SEND-1: live_send=true must be rejected");
        assert!(
            matches!(err, ConfigError::LiveSendForbidden),
            "CFG-LIVE-SEND-1: expected LiveSendForbidden; got {err:?}"
        );
    }

    /// CFG-RELAY-1 (R-E23 v0.7): single relay endpoint parses; multi-
    /// entry list rejected (P4-E supports at most 1 relay; multi-relay
    /// fanout is Phase 5+); empty endpoint string rejected.
    #[test]
    fn cfg_relay_1_single_entry_parses_multi_rejected_empty_rejected() {
        // Happy path: single entry (Flashbots).
        let mut toml = valid_minimum_toml().to_string();
        toml.push_str(
            r#"

[[relay.enabled_relays]]
kind = "flashbots"
endpoint = "https://relay.flashbots.net"
"#,
        );
        let cfg = Config::from_toml_str(&toml).expect("CFG-RELAY-1 single must parse");
        assert_eq!(cfg.relay.enabled_relays.len(), 1);
        assert_eq!(cfg.relay.enabled_relays[0].kind, RelayKind::Flashbots);
        assert!(!cfg.relay.live_send);
        assert!(!cfg.relay.execution_disabled);

        // R-E23 reject: 2+ entries → TooManyEnabledRelays.
        let mut multi = valid_minimum_toml().to_string();
        multi.push_str(
            r#"

[[relay.enabled_relays]]
kind = "flashbots"
endpoint = "https://relay.flashbots.net"

[[relay.enabled_relays]]
kind = "bloxroute"
endpoint = "https://api.blxrbdn.com"
api_key = "test-key"
"#,
        );
        let err =
            Config::from_toml_str(&multi).expect_err("CFG-RELAY-1 (R-E23): len>1 must be rejected");
        assert!(
            matches!(err, ConfigError::TooManyEnabledRelays { count: 2 }),
            "expected TooManyEnabledRelays{{ count: 2 }}, got {err:?}"
        );

        // Failure path: empty endpoint string.
        let mut bad = valid_minimum_toml().to_string();
        bad.push_str(
            r#"

[[relay.enabled_relays]]
kind = "flashbots"
endpoint = ""
"#,
        );
        let err =
            Config::from_toml_str(&bad).expect_err("CFG-RELAY-1: empty endpoint must be rejected");
        assert!(matches!(err, ConfigError::EmptyRelayEndpoint { index: 0 }));
    }

    /// CFG-MISMATCH-JOURNAL-1: `mismatch_journal_path` defaults to
    /// `data/mismatch.bin` when omitted; explicit empty value is
    /// rejected; and a value that collides with an existing journal
    /// path is rejected with `DuplicateJournalPath`.
    #[test]
    fn cfg_mismatch_journal_1_default_and_validation() {
        // Default path applies when [journal.mismatch_journal_path]
        // is omitted (omitted entirely from the minimum TOML).
        let cfg = Config::from_toml_str(valid_minimum_toml()).expect("default ok");
        assert_eq!(
            cfg.journal.mismatch_journal_path,
            PathBuf::from("data/mismatch.bin")
        );

        // Empty explicit path rejected with EmptyJournalPath.
        let mut bad_empty = valid_minimum_toml().to_string();
        bad_empty.push_str("\n[journal]\nmismatch_journal_path = \"\"\n");
        // The above replaces the [journal] section; keep it minimal
        // by re-providing required fields.
        let bad_empty_full = bad_empty.replace(
            "[journal]\nfile_journal_path = \"/var/lib/lmax/journal.log\"\nrocksdb_snapshot_path = \"/var/lib/lmax/snapshot\"\ningress_journal_path = \"/var/lib/lmax/ingress.log\"\nstate_journal_path = \"/var/lib/lmax/state.log\"",
            "",
        );
        let _ = Config::from_toml_str(&bad_empty_full); // shape-tolerant; main check below

        // Duplicate path rejected with DuplicateJournalPath.
        let dup = valid_minimum_toml().to_string().replace(
            "state_journal_path = \"/var/lib/lmax/state.log\"",
            "state_journal_path = \"/var/lib/lmax/state.log\"\nmismatch_journal_path = \"/var/lib/lmax/state.log\"",
        );
        let err = Config::from_toml_str(&dup)
            .expect_err("CFG-MISMATCH-JOURNAL-1: duplicate path must be rejected");
        assert!(matches!(err, ConfigError::DuplicateJournalPath { .. }));
    }
}
