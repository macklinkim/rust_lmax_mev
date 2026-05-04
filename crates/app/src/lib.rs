//! Phase 1 binary entrypoint library for the LMAX-style MEV engine.
//!
//! Per Batch B execution note (`docs/superpowers/plans/2026-05-02-phase-1-
//! batch-b-app-execution.md`).
//!
//! Wires foundation crates into a runnable Phase 1 process:
//! `Config` → `observability::init` → `FileJournal` + `RocksDbSnapshot`
//! → `CrossbeamBoundedBus` → consumer thread (blocking
//! `EventConsumer::recv` per ADR-005) → wait for `ctrl_c` → drop bus →
//! join consumer thread → flush journal.
//!
//! [`run`] is the production entrypoint; [`wire`] is the test-friendly
//! variant that returns an [`AppHandle`] so integration tests can drive
//! events and shutdown without `tokio::signal::ctrl_c`.

use std::path::Path;
use std::sync::Arc;
use std::thread::JoinHandle;

use rust_lmax_mev_config::Config;
use rust_lmax_mev_event_bus::{CrossbeamBoundedBus, CrossbeamConsumer, EventConsumer};
use rust_lmax_mev_ingress::IngressEvent;
use rust_lmax_mev_journal::{FileJournal, JournalError, RocksDbSnapshot};
use rust_lmax_mev_node::{NodeError, NodeProvider};
use rust_lmax_mev_state::{PoolId, StateEngine, StateError};
use rust_lmax_mev_types::SmokeTestPayload;

/// All errors produced by [`run`] / [`wire`].
///
/// `#[non_exhaustive]` so Phase 2/3 may add variants additively.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// Config load / validation failure.
    #[error("config error: {0}")]
    Config(#[from] rust_lmax_mev_config::ConfigError),

    /// `observability::init` failure (already-init or install failure).
    #[error("observability error: {0}")]
    Observability(#[from] rust_lmax_mev_observability::ObservabilityError),

    /// FileJournal / RocksDbSnapshot open / append / flush failure.
    #[error("journal error: {0}")]
    Journal(#[from] JournalError),

    /// Event-bus construction or runtime failure.
    #[error("bus error: {0}")]
    Bus(#[from] rust_lmax_mev_event_bus::BusError),

    /// Filesystem or runtime I/O failure (e.g., tokio runtime build,
    /// ctrl_c register).
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Consumer thread panicked or otherwise failed to join cleanly.
    #[error("consumer thread join failed: {0}")]
    ConsumerJoin(String),

    /// Phase 2 P2-D: `NodeProvider::connect` failure (URL parse / WS
    /// connect / RPC). Surfaces as `AppError::Node` so callers can
    /// distinguish node-side problems from filesystem/journal/bus.
    #[error("node error: {0}")]
    Node(#[from] NodeError),

    /// Phase 2 P2-D: state-engine failure (snapshot persistence,
    /// ABI decode, unknown-pool). Constructed via `#[from]` from
    /// `rust_lmax_mev_state::StateError`.
    #[error("state error: {0}")]
    State(#[from] StateError),
}

/// Optional knobs for [`wire`]. The production [`run`] always passes
/// `WireOptions::default()`; integration tests pass `init_observability:
/// false` to skip the global tracing / Prometheus install (each test
/// binary is its own process; tests that explicitly verify observability
/// behavior pass `true`).
#[derive(Debug, Clone, Copy)]
pub struct WireOptions {
    pub init_observability: bool,
}

impl Default for WireOptions {
    fn default() -> Self {
        Self {
            init_observability: true,
        }
    }
}

/// Owns the bus producer + consumer-thread join handle. Test code
/// publishes through [`AppHandle::bus`] and shuts down via
/// [`AppHandle::shutdown`].
pub struct AppHandle {
    bus: CrossbeamBoundedBus<SmokeTestPayload>,
    consumer_thread: JoinHandle<()>,
}

// Manual `Debug` so test diagnostics (`Result::expect_err`) compile;
// the bus and join handle have no useful Debug surface to expose.
impl std::fmt::Debug for AppHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppHandle").finish_non_exhaustive()
    }
}

impl AppHandle {
    /// Borrow the bus producer for publishing events.
    pub fn bus(&self) -> &CrossbeamBoundedBus<SmokeTestPayload> {
        &self.bus
    }

    /// Drops the bus (closing the channel), then joins the consumer
    /// thread. The consumer's final `flush()` runs inside the thread
    /// just before it exits.
    pub fn shutdown(self) -> Result<(), AppError> {
        let Self {
            bus,
            consumer_thread,
        } = self;
        drop(bus);
        consumer_thread
            .join()
            .map_err(|_| AppError::ConsumerJoin("consumer thread panicked".to_string()))?;
        Ok(())
    }
}

/// Wires the engine without blocking on a shutdown signal. Returns an
/// [`AppHandle`] the caller drives until ready to shut down.
///
/// The opened [`RocksDbSnapshot`] is dropped at the end of this function:
/// Phase 1 has no producer-side snapshot writes, and the snapshot file
/// can be reopened by the next process. Phase 3 will retain it.
pub fn wire(config: &Config, opts: WireOptions) -> Result<AppHandle, AppError> {
    if opts.init_observability {
        // Held only for the duration of `wire`; the underlying recorder
        // and tracing subscriber are process-global and stay installed.
        let _obs = rust_lmax_mev_observability::init(&config.observability)?;
    }

    let journal: FileJournal<SmokeTestPayload> =
        FileJournal::open(&config.journal.file_journal_path)?;
    let _snapshot = RocksDbSnapshot::open(&config.journal.rocksdb_snapshot_path)?;

    let (bus, consumer) = CrossbeamBoundedBus::<SmokeTestPayload>::new(config.bus.capacity)?;

    let consumer_thread = std::thread::Builder::new()
        .name("rust-lmax-mev-consumer".to_string())
        .spawn(move || consume_loop(consumer, journal))?;

    Ok(AppHandle {
        bus,
        consumer_thread,
    })
}

/// Production entrypoint: load config, wire the Phase 2 producer-side
/// stack, await `ctrl_c`, shut down.
///
/// Phase 2 P2-D wiring per the approved Batch D execution note:
/// constructs the runtime FIRST and keeps it alive for the full
/// process lifetime so `NodeProvider`'s alloy WS handle is never
/// orphaned (runtime-lifetime risk mitigation).
pub fn run(config_path: impl AsRef<Path>) -> Result<(), AppError> {
    let config = Config::load(config_path)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let handle = runtime.block_on(wire_phase2(&config, WireOptions::default()))?;
    runtime.block_on(async { tokio::signal::ctrl_c().await })?;
    handle.shutdown()?;
    drop(runtime);
    Ok(())
}

/// Phase 2 P2-D producer-side handle. Holds the `NodeProvider` and
/// `StateEngine` so they survive until `shutdown` returns; holds the
/// ingress→state bus producer for Phase 3 to drive. No consumer
/// thread is spawned: `IngressEvent` does not yet impl `rkyv::Archive`
/// (would require an additive edit to the P2-A-frozen `crates/ingress`),
/// so a `FileJournal<IngressEvent>`-draining consumer is deferred to
/// Phase 3 along with the producer-task spawn that publishes events.
pub struct AppHandle2 {
    bus: CrossbeamBoundedBus<IngressEvent>,
    _consumer: CrossbeamConsumer<IngressEvent>,
    provider: Arc<NodeProvider>,
    engine: Arc<StateEngine>,
}

impl std::fmt::Debug for AppHandle2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppHandle2").finish_non_exhaustive()
    }
}

impl AppHandle2 {
    /// Borrow the ingress→state bus producer (for Phase 3 wiring).
    pub fn bus(&self) -> &CrossbeamBoundedBus<IngressEvent> {
        &self.bus
    }

    /// Borrow the held `NodeProvider`.
    pub fn provider(&self) -> &Arc<NodeProvider> {
        &self.provider
    }

    /// Borrow the held `StateEngine`.
    pub fn engine(&self) -> &Arc<StateEngine> {
        &self.engine
    }

    /// Drops the bus, consumer handle, engine, and provider. There is
    /// no consumer thread to join (see `AppHandle2` doc).
    pub fn shutdown(self) -> Result<(), AppError> {
        let Self {
            bus,
            _consumer,
            provider,
            engine,
        } = self;
        drop(bus);
        drop(_consumer);
        drop(engine);
        drop(provider);
        Ok(())
    }
}

/// Phase 2 P2-D async constructor. Builds:
/// - `observability::init` (gated by `WireOptions.init_observability`).
/// - `NodeProvider::connect(&config.node).await` — URL parse only;
///   actual HTTP/WS dialing is lazy (alloy default).
/// - `RocksDbSnapshot::open(&config.journal.rocksdb_snapshot_path)`.
/// - `StateEngine::new(provider, snapshot, pools_from_config)`.
/// - `CrossbeamBoundedBus::<IngressEvent>::new(config.bus.capacity)`.
///
/// Does NOT spawn a producer task (Phase 3 owns the 6-stage pipeline)
/// and does NOT spawn a journal-draining consumer thread (`IngressEvent`
/// is not `rkyv::Archive` today). The bus producer + held consumer
/// handle are returned in `AppHandle2` so Phase 3 can swap in both ends
/// without the wire surface changing.
pub async fn wire_phase2(
    config: &Config,
    opts: WireOptions,
) -> Result<AppHandle2, AppError> {
    if opts.init_observability {
        let _obs = rust_lmax_mev_observability::init(&config.observability)?;
    }

    let provider = Arc::new(NodeProvider::connect(&config.node).await?);
    let snapshot = Arc::new(RocksDbSnapshot::open(
        &config.journal.rocksdb_snapshot_path,
    )?);
    let pools: Vec<PoolId> = config.state.pools.iter().map(PoolId::from).collect();
    let engine = Arc::new(StateEngine::new(
        Arc::clone(&provider),
        snapshot,
        pools,
    ));

    let (bus, consumer) = CrossbeamBoundedBus::<IngressEvent>::new(config.bus.capacity)?;

    Ok(AppHandle2 {
        bus,
        _consumer: consumer,
        provider,
        engine,
    })
}

/// Drains the bus into the journal until the bus closes. Best-effort
/// error logging per ADR-001 thin-path policy: append failures are
/// logged and the loop continues so a single bad envelope does not
/// stop the consumer.
fn consume_loop(
    consumer: CrossbeamConsumer<SmokeTestPayload>,
    mut journal: FileJournal<SmokeTestPayload>,
) {
    while let Ok(envelope) = consumer.recv() {
        if let Err(e) = journal.append(&envelope) {
            tracing::error!(error = %e, "journal append failed");
        }
    }
    if let Err(e) = journal.flush() {
        tracing::error!(error = %e, "journal flush at shutdown failed");
    }
}
