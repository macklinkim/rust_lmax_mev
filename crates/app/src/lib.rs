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
use rust_lmax_mev_execution::BundleConstructor;
use rust_lmax_mev_ingress::IngressEvent;
use rust_lmax_mev_journal::{FileJournal, JournalError, RocksDbSnapshot};
use rust_lmax_mev_node::{NodeError, NodeProvider};
use rust_lmax_mev_opportunity::OpportunityEngine;
use rust_lmax_mev_risk::RiskGate;
use rust_lmax_mev_simulator::LocalSimulator;
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

/// Production entrypoint: load config, wire the Phase 3 P3-F producer-
/// side stack, await `ctrl_c`, shut down.
///
/// Phase 3 P3-F wiring per the approved Batch F execution note:
/// `wire_phase4` spawns the GethWS mempool producer task plus the
/// `tokio::sync::broadcast` rebroadcast tee plus the journal-drain,
/// state-driver, opportunity-driver, risk-driver, simulator-driver,
/// and execution-driver chain. `AppHandle4::shutdown` is async and
/// runs on the same `runtime`. The runtime stays alive the full process
/// lifetime so the alloy WS handle is never orphaned.
pub fn run(config_path: impl AsRef<Path>) -> Result<(), AppError> {
    let config = Config::load(config_path)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let handle = runtime.block_on(wire_phase4(&config, WireOptions::default()))?;
    runtime.block_on(async { tokio::signal::ctrl_c().await })?;
    runtime.block_on(handle.shutdown())?;
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
pub async fn wire_phase2(config: &Config, opts: WireOptions) -> Result<AppHandle2, AppError> {
    if opts.init_observability {
        let _obs = rust_lmax_mev_observability::init(&config.observability)?;
    }

    let provider = Arc::new(NodeProvider::connect(&config.node).await?);
    let snapshot = Arc::new(RocksDbSnapshot::open(
        &config.journal.rocksdb_snapshot_path,
    )?);
    let pools: Vec<PoolId> = config.state.pools.iter().map(PoolId::from).collect();
    let engine = Arc::new(StateEngine::new(Arc::clone(&provider), snapshot, pools));

    let (bus, consumer) = CrossbeamBoundedBus::<IngressEvent>::new(config.bus.capacity)?;

    Ok(AppHandle2 {
        bus,
        _consumer: consumer,
        provider,
        engine,
    })
}

/// Phase 3 P3-B handle. Owns the producer task + dual journal-drain
/// consumer threads + held bus producer Arcs. Per the approved P3-B
/// execution note v0.2 (Codex APPROVED HIGH 2026-05-04 16:06:52),
/// `AppHandle3::shutdown` is async and runs `producer_task.abort();
/// let _ = producer_task.await;` BEFORE any bus drop / consumer join,
/// so the aborted task releases its bus producer handle before the
/// consumer thread's `recv()` could otherwise hang waiting on a phantom
/// producer.
pub struct AppHandle3 {
    ingress_bus: Arc<CrossbeamBoundedBus<IngressEvent>>,
    state_bus: Arc<CrossbeamBoundedBus<rust_lmax_mev_state::StateUpdateEvent>>,
    provider: Arc<NodeProvider>,
    engine: Arc<StateEngine>,
    producer_task: tokio::task::JoinHandle<()>,
    ingress_consumer: JoinHandle<()>,
    state_consumer: JoinHandle<()>,
}

impl std::fmt::Debug for AppHandle3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppHandle3").finish_non_exhaustive()
    }
}

impl AppHandle3 {
    pub fn ingress_bus(&self) -> &Arc<CrossbeamBoundedBus<IngressEvent>> {
        &self.ingress_bus
    }
    pub fn state_bus(&self) -> &Arc<CrossbeamBoundedBus<rust_lmax_mev_state::StateUpdateEvent>> {
        &self.state_bus
    }
    pub fn provider(&self) -> &Arc<NodeProvider> {
        &self.provider
    }
    pub fn engine(&self) -> &Arc<StateEngine> {
        &self.engine
    }

    /// Async shutdown with the load-bearing ordering Codex called out:
    /// abort + await producer task → drop ingress bus Arc → join ingress
    /// consumer thread → drop state bus Arc → join state consumer thread
    /// → drop engine → drop provider.
    pub async fn shutdown(self) -> Result<(), AppError> {
        let Self {
            ingress_bus,
            state_bus,
            provider,
            engine,
            producer_task,
            ingress_consumer,
            state_consumer,
        } = self;

        // 1. Cancel producer + 2. await its drop of the bus Arc clone.
        producer_task.abort();
        let _ = producer_task.await;

        // 3. Drop the only remaining ingress bus Arc → channel closes.
        drop(ingress_bus);

        // 4. Consumer's recv() returns Err; thread loop exits.
        ingress_consumer
            .join()
            .map_err(|_| AppError::ConsumerJoin("ingress consumer thread panicked".to_string()))?;

        // 5. State bus had no producer in P3-B; dropping closes it
        //    immediately (consumer wakes from recv with Err).
        drop(state_bus);
        state_consumer
            .join()
            .map_err(|_| AppError::ConsumerJoin("state consumer thread panicked".to_string()))?;

        // 6. Final teardown.
        drop(engine);
        drop(provider);

        Ok(())
    }
}

/// Phase 3 P3-B async constructor. Extends `wire_phase2` by:
/// - Opening `FileJournal::<IngressEvent>::open(...)` and
///   `FileJournal::<StateUpdateEvent>::open(...)` against the two new
///   `JournalConfig` paths.
/// - Spawning two journal-drain consumer threads.
/// - Spawning the `producer_loop` tokio task that pumps the GethWS
///   mempool stream onto the ingress→state bus.
///
/// `BlockEvent` producer + `StateEngine` driver consumer + multi-consumer
/// fanout on `ingress_bus` are all deferred to P3-C/D (per the approved
/// P3-B note + Codex 16:00:13 P3-C-revisit obligation).
pub async fn wire_phase3(config: &Config, opts: WireOptions) -> Result<AppHandle3, AppError> {
    if opts.init_observability {
        let _obs = rust_lmax_mev_observability::init(&config.observability)?;
    }

    let provider = Arc::new(NodeProvider::connect(&config.node).await?);
    let snapshot = Arc::new(RocksDbSnapshot::open(
        &config.journal.rocksdb_snapshot_path,
    )?);
    let pools: Vec<PoolId> = config.state.pools.iter().map(PoolId::from).collect();
    let engine = Arc::new(StateEngine::new(Arc::clone(&provider), snapshot, pools));

    let ingress_journal: FileJournal<IngressEvent> =
        FileJournal::open(&config.journal.ingress_journal_path)?;
    let state_journal: FileJournal<rust_lmax_mev_state::StateUpdateEvent> =
        FileJournal::open(&config.journal.state_journal_path)?;

    let (ingress_bus_inner, ingress_consumer_handle) =
        CrossbeamBoundedBus::<IngressEvent>::new(config.bus.capacity)?;
    let (state_bus_inner, state_consumer_handle) =
        CrossbeamBoundedBus::<rust_lmax_mev_state::StateUpdateEvent>::new(config.bus.capacity)?;
    let ingress_bus = Arc::new(ingress_bus_inner);
    let state_bus = Arc::new(state_bus_inner);

    let ingress_consumer = std::thread::Builder::new()
        .name("rust-lmax-mev-ingress-consumer".to_string())
        .spawn(move || consume_loop(ingress_consumer_handle, ingress_journal))?;

    let state_consumer = std::thread::Builder::new()
        .name("rust-lmax-mev-state-consumer".to_string())
        .spawn(move || consume_loop(state_consumer_handle, state_journal))?;

    let watched = config.ingress.watched_addresses.clone();
    let producer_provider = Arc::clone(&provider);
    let producer_bus = Arc::clone(&ingress_bus);
    let producer_task =
        tokio::spawn(async move { producer_loop(producer_provider, watched, producer_bus).await });

    Ok(AppHandle3 {
        ingress_bus,
        state_bus,
        provider,
        engine,
        producer_task,
        ingress_consumer,
        state_consumer,
    })
}

/// Drains the bus into the journal until the bus closes. Best-effort
/// error logging per ADR-001 thin-path policy: append failures are
/// logged and the loop continues so a single bad envelope does not
/// stop the consumer.
///
/// Phase 3 P3-B (v0.2 per Codex pre-impl APPROVED 2026-05-04 16:06:52):
/// generalized over the payload type `T` so the same impl drains both
/// `FileJournal<IngressEvent>` and `FileJournal<StateUpdateEvent>`
/// from `wire_phase3`. `pub` (narrowly: only this fn, not the broader
/// internals) so the deterministic B-2 integration test can drive it
/// directly without a NodeProvider mock.
pub fn consume_loop<T>(consumer: CrossbeamConsumer<T>, mut journal: FileJournal<T>)
where
    T: rkyv::Archive + Send + 'static,
    T: for<'a> rkyv::Serialize<
        rkyv::api::high::HighSerializer<
            rkyv::util::AlignedVec,
            rkyv::ser::allocator::ArenaHandle<'a>,
            rkyv::rancor::Error,
        >,
    >,
    <T as rkyv::Archive>::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    while let Ok(envelope) = consumer.recv() {
        if let Err(e) = journal.append(&envelope) {
            tracing::error!(error = %e, "journal append failed");
        }
    }
    if let Err(e) = journal.flush() {
        tracing::error!(error = %e, "journal flush at shutdown failed");
    }
}

/// Phase 3 P3-B producer loop: subscribes to the GethWS mempool stream
/// via `GethWsMempool` and republishes each normalized `MempoolEvent`
/// onto the ingress->state bus as `IngressEvent::Mempool(...)`. Per
/// ADR-001 thin-path: best-effort error handling — `IngressError`
/// surfaces as `tracing::warn!` and the loop continues; stream
/// exhaustion ends the task cleanly. `BlockEvent` producer is P3-C.
async fn producer_loop(
    provider: std::sync::Arc<NodeProvider>,
    watched: Vec<alloy_primitives::Address>,
    bus: std::sync::Arc<CrossbeamBoundedBus<IngressEvent>>,
) {
    use futures::StreamExt;
    use rust_lmax_mev_event_bus::EventBus;
    use rust_lmax_mev_ingress::{GethWsMempool, MempoolSource};

    let source = GethWsMempool::new(provider, watched);
    let mut stream = source.stream();
    while let Some(item) = stream.next().await {
        match item {
            Ok(mempool_event) => {
                let event = IngressEvent::Mempool(mempool_event);
                let meta = rust_lmax_mev_types::PublishMeta {
                    source: rust_lmax_mev_types::EventSource::Ingress,
                    chain_context: rust_lmax_mev_types::ChainContext {
                        chain_id: 1,
                        block_number: 0,
                        block_hash: [0u8; 32],
                    },
                    event_version: 1,
                    correlation_id: 0,
                };
                if let Err(e) = bus.publish(event, meta) {
                    tracing::warn!(error = %e, "ingress->state bus publish failed");
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "ingress source error; continuing");
            }
        }
    }
}

// =====================================================================
// Phase 3 P3-F: wire_phase4 + AppHandle4 + topology Option A broadcast
// tee + driver chain. Per the approved P3-F execution note v0.1: ADDITIVE
// over wire_phase3 (existing wire / wire_phase2 / wire_phase3 stay
// byte-identical). `tokio::sync::broadcast` rebroadcast layer with the
// v0.2 fail-closed `RecvError::Lagged` policy from P3-D documented design.
// =====================================================================

use rust_lmax_mev_execution::BundleCandidate;
use rust_lmax_mev_opportunity::OpportunityEvent;
use rust_lmax_mev_risk::RiskCheckedOpportunity;
use rust_lmax_mev_simulator::{LocalStateFingerprint, SimulationOutcome};
use rust_lmax_mev_state::StateUpdateEvent;
use rust_lmax_mev_types::{ChainContext, EventEnvelope, EventSource, PublishMeta};
use tokio::sync::broadcast;

// Phase 4 P4-E imports for the comparator_driver wiring per
// execution-note v0.6 §D-E3.
//
// NOTE: `BundleRelay` trait is INTENTIONALLY NOT imported here.
// `crates/app` constructs each adapter as its concrete type and
// stores it as `Arc<dyn RelaySimulator>` per DP-E13 v0.3 — the
// `BundleRelay` trait object is never named in this file (CW-3 grep
// gate at batch close). The concrete adapters from `crates/relay-clients`
// implement `BundleRelay` for use by the per-adapter
// `submit_disabled` tests in `crates/relay-clients/tests/`, but the
// app wiring stays simulator-side only.
use rust_lmax_mev_relay_clients::{
    BloxrouteConfig, BloxrouteRelay, FlashbotsConfig, FlashbotsRelay,
};
use rust_lmax_mev_relay_sim::{
    compare_result, ComparatorInputs, LocalBundleShape, MismatchAbort, RelaySimRequest,
    RelaySimulator,
};

/// Phase 4 P4-E DP-E5: payload of `sim_tx`. Carries both the
/// `SimulationOutcome` (for the existing execution_driver) and the
/// `LocalStateFingerprint` (for the new comparator_driver per R-E1 +
/// DP-D9'). Wiring-only type; not journaled, not bus-archived; only
/// `Clone + Debug` needed.
#[derive(Debug, Clone)]
pub struct SimulationOutcomeWithFingerprint {
    pub outcome: SimulationOutcome,
    pub fingerprint: LocalStateFingerprint,
}

/// Phase 4 P4-E: terminal "comparator passed" envelope payload. Emitted
/// on `comparator_tx` when local + relay outcomes agree. **No subscriber
/// reads `comparator_tx` in P4-E** — the type exists for Phase 5+
/// submission consumers per the v0.6 honest closeout claim.
#[derive(Debug, Clone)]
pub struct MismatchCheckPassed {
    pub candidate: BundleCandidate,
}

/// Phase 4 P4-E DP-E12: observability payload emitted on `mismatch_tx`
/// AFTER the synchronous journal append+flush completes (per DP-E8
/// v0.4 ordering guarantee). Used by future monitor subscribers.
/// In P4-E nothing reads `mismatch_tx` either — the journal IS the
/// canonical record.
#[derive(Debug, Clone)]
pub struct MismatchAbortRecord {
    pub abort: MismatchAbort,
}

// Re-exports for crates/app/tests/comparator_wiring.rs CW-1..5.
pub use rust_lmax_mev_relay_clients::{
    BloxrouteConfig as RelayBloxrouteConfig, FlashbotsConfig as RelayFlashbotsConfig,
};

/// In-test verification (CW-5 type-system gate per DP-E13 v0.3): the
/// `comparator_driver` parameter type for the relay simulator is
/// `Arc<dyn RelaySimulator>` (NOT a BundleRelay trait object). This
/// function exists ONLY to compile-assert the parameter type; never
/// invoked at runtime.
#[doc(hidden)]
pub fn _cw_5_compile_check_comparator_driver_takes_dyn_relay_simulator(_: Arc<dyn RelaySimulator>) {
}

/// Phase 3 P3-F handle. Owns the producer task + the broadcast
/// rebroadcast senders + every spawned driver task.
pub struct AppHandle4 {
    provider: Arc<NodeProvider>,
    engine: Arc<StateEngine>,
    opportunity: Arc<OpportunityEngine>,
    risk: Arc<RiskGate>,
    simulator: Arc<LocalSimulator>,
    bundle_constructor: Arc<BundleConstructor>,
    ingress_tx: broadcast::Sender<EventEnvelope<IngressEvent>>,
    state_tx: broadcast::Sender<EventEnvelope<StateUpdateEvent>>,
    opp_tx: broadcast::Sender<EventEnvelope<OpportunityEvent>>,
    risk_tx: broadcast::Sender<EventEnvelope<RiskCheckedOpportunity>>,
    sim_tx: broadcast::Sender<EventEnvelope<SimulationOutcomeWithFingerprint>>,
    exec_tx: broadcast::Sender<EventEnvelope<BundleCandidate>>,
    // Phase 4 P4-E (D-E3 + DP-E12): comparator broadcast channels.
    // No subscribers in P4-E (terminal observability events).
    comparator_tx: broadcast::Sender<MismatchCheckPassed>,
    mismatch_tx: broadcast::Sender<MismatchAbortRecord>,
    producer_task: tokio::task::JoinHandle<()>,
    ingress_journal_task: tokio::task::JoinHandle<()>,
    state_driver_task: tokio::task::JoinHandle<()>,
    state_journal_task: tokio::task::JoinHandle<()>,
    opportunity_driver_task: tokio::task::JoinHandle<()>,
    risk_driver_task: tokio::task::JoinHandle<()>,
    simulator_driver_task: tokio::task::JoinHandle<()>,
    execution_driver_task: tokio::task::JoinHandle<()>,
    // Phase 4 P4-E: comparator_driver task. Owns the
    // `FileJournal<MismatchAbort>` directly (no separate drain task
    // per DP-E8 v0.4 R-E16 fix).
    comparator_driver_task: tokio::task::JoinHandle<()>,
}

impl std::fmt::Debug for AppHandle4 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppHandle4").finish_non_exhaustive()
    }
}

impl AppHandle4 {
    pub fn provider(&self) -> &Arc<NodeProvider> {
        &self.provider
    }
    pub fn engine(&self) -> &Arc<StateEngine> {
        &self.engine
    }
    pub fn opportunity(&self) -> &Arc<OpportunityEngine> {
        &self.opportunity
    }
    pub fn risk(&self) -> &Arc<RiskGate> {
        &self.risk
    }
    pub fn simulator(&self) -> &Arc<LocalSimulator> {
        &self.simulator
    }
    pub fn bundle_constructor(&self) -> &Arc<BundleConstructor> {
        &self.bundle_constructor
    }
    pub fn exec_subscribe(&self) -> broadcast::Receiver<EventEnvelope<BundleCandidate>> {
        self.exec_tx.subscribe()
    }

    /// Phase 4 P4-E (DP-E12 observability): subscribe to the comparator's
    /// "mismatch passed" broadcast. P4-E has no production subscriber;
    /// tests use this to verify CW-1 happy-path emission.
    pub fn comparator_subscribe(&self) -> broadcast::Receiver<MismatchCheckPassed> {
        self.comparator_tx.subscribe()
    }

    /// Phase 4 P4-E (DP-E12 observability): subscribe to the comparator's
    /// "mismatch abort" broadcast. P4-E has no production subscriber;
    /// tests use this to verify CW-2 abort-path emission AFTER the
    /// synchronous journal append+flush completes.
    pub fn mismatch_subscribe(&self) -> broadcast::Receiver<MismatchAbortRecord> {
        self.mismatch_tx.subscribe()
    }

    /// Async shutdown. Aborts producer + awaits it (so its
    /// broadcast Sender clones drop), then drops senders in reverse-
    /// pipeline order so each driver's recv() returns Closed, then
    /// awaits every driver task.
    pub async fn shutdown(self) -> Result<(), AppError> {
        let Self {
            provider,
            engine,
            opportunity,
            risk,
            simulator,
            bundle_constructor,
            ingress_tx,
            state_tx,
            opp_tx,
            risk_tx,
            sim_tx,
            exec_tx,
            comparator_tx,
            mismatch_tx,
            producer_task,
            ingress_journal_task,
            state_driver_task,
            state_journal_task,
            opportunity_driver_task,
            risk_driver_task,
            simulator_driver_task,
            execution_driver_task,
            comparator_driver_task,
        } = self;

        producer_task.abort();
        let _ = producer_task.await;

        // Drop comparator broadcasts so any subscribers get Closed,
        // then abort comparator driver. The driver owns the journal
        // directly; its drop runs flush at shutdown.
        drop(comparator_tx);
        drop(mismatch_tx);
        comparator_driver_task.abort();
        let _ = comparator_driver_task.await;

        drop(exec_tx);
        execution_driver_task.abort();
        let _ = execution_driver_task.await;

        drop(sim_tx);
        simulator_driver_task.abort();
        let _ = simulator_driver_task.await;

        drop(risk_tx);
        risk_driver_task.abort();
        let _ = risk_driver_task.await;

        drop(opp_tx);
        opportunity_driver_task.abort();
        let _ = opportunity_driver_task.await;

        drop(state_tx);
        state_driver_task.abort();
        let _ = state_driver_task.await;
        state_journal_task.abort();
        let _ = state_journal_task.await;

        drop(ingress_tx);
        ingress_journal_task.abort();
        let _ = ingress_journal_task.await;

        drop(bundle_constructor);
        drop(simulator);
        drop(risk);
        drop(opportunity);
        drop(engine);
        drop(provider);

        Ok(())
    }
}

/// Phase 3 P3-F async constructor. Wires the full pipeline.
pub async fn wire_phase4(config: &Config, opts: WireOptions) -> Result<AppHandle4, AppError> {
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
        pools.clone(),
    ));
    let opportunity = Arc::new(OpportunityEngine::new(&config.ingress.tokens));
    let risk = Arc::new(RiskGate::new(
        rust_lmax_mev_risk::RiskBudgetConfig::defaults(),
    ));
    let simulator = Arc::new(
        LocalSimulator::new(rust_lmax_mev_simulator::SimConfig::defaults())
            .map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))?,
    );
    let bundle_constructor = Arc::new(
        BundleConstructor::new(rust_lmax_mev_execution::BundleConfig::defaults())
            .map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))?,
    );

    let ingress_journal: FileJournal<IngressEvent> =
        FileJournal::open(&config.journal.ingress_journal_path)?;
    let state_journal: FileJournal<StateUpdateEvent> =
        FileJournal::open(&config.journal.state_journal_path)?;

    let cap = config.bus.capacity.max(8);
    let (ingress_tx, _) = broadcast::channel::<EventEnvelope<IngressEvent>>(cap);
    let (state_tx, _) = broadcast::channel::<EventEnvelope<StateUpdateEvent>>(cap);
    let (opp_tx, _) = broadcast::channel::<EventEnvelope<OpportunityEvent>>(cap);
    let (risk_tx, _) = broadcast::channel::<EventEnvelope<RiskCheckedOpportunity>>(cap);
    // Phase 4 P4-E (DP-E5): sim_tx payload now carries the fingerprint.
    let (sim_tx, _) = broadcast::channel::<EventEnvelope<SimulationOutcomeWithFingerprint>>(cap);
    let (exec_tx, _) = broadcast::channel::<EventEnvelope<BundleCandidate>>(cap);
    // Phase 4 P4-E (D-E3 + DP-E12): comparator broadcasts.
    let (comparator_tx, _) = broadcast::channel::<MismatchCheckPassed>(cap);
    let (mismatch_tx, _) = broadcast::channel::<MismatchAbortRecord>(cap);

    // Phase 4 P4-E: open the comparator's mismatch journal.
    let mismatch_journal: FileJournal<MismatchAbort> =
        FileJournal::open(&config.journal.mismatch_journal_path)?;
    // Build the relay-sim adapter from config (None when
    // `enabled_relays` is empty per DP-E3 → comparator inert).
    let relay_sim = build_relay_sim_from_config(&config.relay)?;

    let ingress_journal_task = tokio::spawn(journal_drain_loop(
        "ingress-journal",
        ingress_tx.subscribe(),
        ingress_journal,
    ));
    let state_driver_task = tokio::spawn(state_engine_driver(
        ingress_tx.subscribe(),
        state_tx.clone(),
        Arc::clone(&engine),
    ));
    let state_journal_task = tokio::spawn(journal_drain_loop(
        "state-journal",
        state_tx.subscribe(),
        state_journal,
    ));
    let opportunity_driver_task = tokio::spawn(opportunity_driver(
        state_tx.subscribe(),
        opp_tx.clone(),
        Arc::clone(&opportunity),
        pools,
    ));
    let risk_driver_task = tokio::spawn(risk_driver(
        opp_tx.subscribe(),
        risk_tx.clone(),
        Arc::clone(&risk),
    ));
    let simulator_driver_task = tokio::spawn(simulator_driver(
        risk_tx.subscribe(),
        sim_tx.clone(),
        Arc::clone(&simulator),
    ));
    let execution_driver_task = tokio::spawn(execution_driver(
        sim_tx.subscribe(),
        exec_tx.clone(),
        Arc::clone(&bundle_constructor),
    ));

    // Phase 4 P4-E: spawn comparator_driver. Subscribes to sim_tx
    // (independently of execution_driver so both see every
    // SimulationOutcomeWithFingerprint envelope). Owns the
    // FileJournal<MismatchAbort> directly per DP-E8 v0.4.
    let comparator_driver_task = tokio::spawn(comparator_driver(
        sim_tx.subscribe(),
        comparator_tx.clone(),
        mismatch_tx.clone(),
        relay_sim,
        Arc::clone(&bundle_constructor),
        mismatch_journal,
        "comparator-driver",
    ));

    let watched = config.ingress.watched_addresses.clone();
    let producer_provider = Arc::clone(&provider);
    let producer_ingress_tx = ingress_tx.clone();
    let producer_task = tokio::spawn(async move {
        producer_loop_phase4(producer_provider, watched, producer_ingress_tx).await
    });

    Ok(AppHandle4 {
        provider,
        engine,
        opportunity,
        risk,
        simulator,
        bundle_constructor,
        ingress_tx,
        state_tx,
        opp_tx,
        risk_tx,
        sim_tx,
        exec_tx,
        comparator_tx,
        mismatch_tx,
        producer_task,
        ingress_journal_task,
        state_driver_task,
        state_journal_task,
        opportunity_driver_task,
        risk_driver_task,
        simulator_driver_task,
        execution_driver_task,
        comparator_driver_task,
    })
}

async fn producer_loop_phase4(
    provider: Arc<NodeProvider>,
    watched: Vec<alloy_primitives::Address>,
    bus: broadcast::Sender<EventEnvelope<IngressEvent>>,
) {
    use futures::StreamExt;
    use rust_lmax_mev_ingress::{GethWsMempool, MempoolSource};

    let source = GethWsMempool::new(provider, watched);
    let mut stream = source.stream();
    let mut seq: u64 = 0;
    while let Some(item) = stream.next().await {
        match item {
            Ok(mempool_event) => {
                let event = IngressEvent::Mempool(mempool_event);
                if let Some(envelope) = seal_envelope(EventSource::Ingress, event, &mut seq) {
                    let _ = bus.send(envelope);
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "ingress source error; continuing");
            }
        }
    }
}

/// Canonical journal-drain consumer of a broadcast channel. Per P3-D
/// v0.2 fail-closed `RecvError::Lagged` policy: any Lagged signal
/// aborts the consumer with a structured error log; supervising
/// runtime tears down on shutdown. Public for the W-2 deterministic
/// shutdown test.
pub async fn journal_drain_loop<T>(
    name: &'static str,
    mut rx: broadcast::Receiver<EventEnvelope<T>>,
    mut journal: FileJournal<T>,
) where
    T: rkyv::Archive + Clone + Send + 'static,
    T: for<'a> rkyv::Serialize<
        rkyv::api::high::HighSerializer<
            rkyv::util::AlignedVec,
            rkyv::ser::allocator::ArenaHandle<'a>,
            rkyv::rancor::Error,
        >,
    >,
    <T as rkyv::Archive>::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    loop {
        match rx.recv().await {
            Ok(envelope) => {
                if let Err(e) = journal.append(&envelope) {
                    tracing::error!(consumer = name, error = %e, "journal append failed");
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::error!(
                    consumer = name,
                    skipped = n,
                    "broadcast lagged; aborting per P3-D v0.2 fail-closed policy"
                );
                break;
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
    if let Err(e) = journal.flush() {
        tracing::error!(consumer = name, error = %e, "journal flush at shutdown failed");
    }
}

async fn state_engine_driver(
    mut rx: broadcast::Receiver<EventEnvelope<IngressEvent>>,
    state_tx: broadcast::Sender<EventEnvelope<StateUpdateEvent>>,
    engine: Arc<StateEngine>,
) {
    let mut seq: u64 = 0;
    loop {
        match rx.recv().await {
            Ok(envelope) => {
                if let IngressEvent::Block(block_evt) = envelope.payload() {
                    let block_number = block_evt.block_number;
                    let block_hash = block_evt.block_hash;
                    match engine.refresh_block(block_number, block_hash).await {
                        Ok(events) => {
                            for ev in events {
                                if let Some(env) =
                                    seal_envelope(EventSource::StateEngine, ev, &mut seq)
                                {
                                    let _ = state_tx.send(env);
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "state engine refresh failed; continuing");
                        }
                    }
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::error!(
                    consumer = "state-driver",
                    skipped = n,
                    "broadcast lagged; aborting"
                );
                break;
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

async fn opportunity_driver(
    mut rx: broadcast::Receiver<EventEnvelope<StateUpdateEvent>>,
    opp_tx: broadcast::Sender<EventEnvelope<OpportunityEvent>>,
    opp_engine: Arc<OpportunityEngine>,
    _pools: Vec<PoolId>,
) {
    use std::collections::HashMap;
    let mut by_block: HashMap<u64, Vec<EventEnvelope<StateUpdateEvent>>> = HashMap::new();
    let mut seq: u64 = 0;
    loop {
        match rx.recv().await {
            Ok(envelope) => {
                let block_number = envelope.payload().block_number;
                by_block
                    .entry(block_number)
                    .or_default()
                    .push(envelope.clone());
                let group = by_block.get(&block_number).expect("just inserted");
                if group.len() >= 2 {
                    for i in 0..group.len() {
                        for j in (i + 1)..group.len() {
                            let a = &group[i];
                            let b = &group[j];
                            let pa = &a.payload().pool;
                            let pb = &b.payload().pool;
                            let cc = a.chain_context();
                            if let Some(opp_event) =
                                opp_engine.check(cc, pa, &a.payload().state, pb, &b.payload().state)
                            {
                                if let Some(env) = seal_envelope(
                                    EventSource::OpportunityEngine,
                                    opp_event,
                                    &mut seq,
                                ) {
                                    let _ = opp_tx.send(env);
                                }
                            }
                        }
                    }
                }
                if by_block.len() > 16 {
                    let high = by_block.keys().copied().max().unwrap_or(block_number);
                    let cutoff = high.saturating_sub(8);
                    by_block.retain(|k, _| *k >= cutoff);
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::error!(
                    consumer = "opportunity-driver",
                    skipped = n,
                    "broadcast lagged; aborting"
                );
                break;
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

async fn risk_driver(
    mut rx: broadcast::Receiver<EventEnvelope<OpportunityEvent>>,
    risk_tx: broadcast::Sender<EventEnvelope<RiskCheckedOpportunity>>,
    risk: Arc<RiskGate>,
) {
    let mut seq: u64 = 0;
    loop {
        match rx.recv().await {
            Ok(envelope) => match risk.evaluate(envelope.payload()) {
                Ok(checked) => {
                    if let Some(env) = seal_envelope(EventSource::RiskEngine, checked, &mut seq) {
                        let _ = risk_tx.send(env);
                    }
                }
                Err(aborted) => {
                    tracing::debug!(category = ?aborted.category, "risk gate aborted opportunity");
                }
            },
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::error!(
                    consumer = "risk-driver",
                    skipped = n,
                    "broadcast lagged; aborting"
                );
                break;
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

/// Phase 4 P4-E (DP-E5): `simulator_driver` calls
/// `simulate_with_fingerprint(...)` (R-E13 / DP-D16 — same input
/// shape as `simulate(...)`) and emits `SimulationOutcomeWithFingerprint`
/// envelopes so downstream consumers (execution_driver +
/// comparator_driver) can both access the local state read-set.
async fn simulator_driver(
    mut rx: broadcast::Receiver<EventEnvelope<RiskCheckedOpportunity>>,
    sim_tx: broadcast::Sender<EventEnvelope<SimulationOutcomeWithFingerprint>>,
    simulator: Arc<LocalSimulator>,
) {
    let mut seq: u64 = 0;
    loop {
        match rx.recv().await {
            Ok(envelope) => match simulator.simulate_with_fingerprint(envelope.payload()) {
                Ok((outcome, fingerprint)) => {
                    let payload = SimulationOutcomeWithFingerprint {
                        outcome,
                        fingerprint,
                    };
                    if let Some(env) = seal_envelope(EventSource::Simulator, payload, &mut seq) {
                        let _ = sim_tx.send(env);
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "local simulator failed; continuing");
                }
            },
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::error!(
                    consumer = "simulator-driver",
                    skipped = n,
                    "broadcast lagged; aborting"
                );
                break;
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

/// Phase 4 P4-E (DP-E5): one-line read of `.outcome` from the new
/// envelope payload. No semantic change.
async fn execution_driver(
    mut rx: broadcast::Receiver<EventEnvelope<SimulationOutcomeWithFingerprint>>,
    exec_tx: broadcast::Sender<EventEnvelope<BundleCandidate>>,
    constructor: Arc<BundleConstructor>,
) {
    let mut seq: u64 = 0;
    loop {
        match rx.recv().await {
            Ok(envelope) => match constructor.construct(&envelope.payload().outcome) {
                Ok(candidate) => {
                    if let Some(env) = seal_envelope(EventSource::Execution, candidate, &mut seq) {
                        let _ = exec_tx.send(env);
                    }
                }
                Err(e) => {
                    tracing::debug!(error = %e, "bundle constructor aborted");
                }
            },
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::error!(
                    consumer = "execution-driver",
                    skipped = n,
                    "broadcast lagged; aborting"
                );
                break;
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

/// Phase 4 P4-E comparator_driver (D-E3 + DP-E12 + DP-E13).
///
/// Subscribes to `sim_tx` (the new `SimulationOutcomeWithFingerprint`
/// broadcast). For each successful local simulation:
/// 1. Runs `BundleConstructor::construct` to obtain the local
///    `BundleCandidate`. If construction aborts (non-Success status,
///    zero profit, etc.) the candidate is skipped — no relay sim
///    needed.
/// 2. Builds a `RelaySimRequest` (`txs` is EMPTY in P4-E since no
///    signing exists; the adapter short-circuits with
///    `Err(UnsignedBundleUnavailable)` BEFORE any HTTP I/O per R-E2).
/// 3. Calls `RelaySimulator::simulate_bundle` on the held
///    `Arc<dyn RelaySimulator>` (DP-E13: NO trait-object upcast;
///    the simulator-trait object was constructed concretely in
///    `wire_phase4`).
/// 4. Calls `relay_sim::compare_result(...)`.
///    - **Ok(())** → emit `MismatchCheckPassed` on `comparator_tx`.
///      No subscriber reads it in P4-E (terminal observability event).
///    - **Err(MismatchAbort)** OR **relay-sim Err(_)** → wrap into
///      `EventEnvelope<MismatchAbort>` per DP-E8 v0.4, call
///      `journal.append(&env)?` + `journal.flush()?` SYNCHRONOUSLY,
///      then emit `MismatchAbortRecord` on `mismatch_tx` (also
///      observability-only in P4-E).
///
/// **NO `submit_bundle` call site exists in this function** (CW-3
/// grep gate); the held trait object is `Arc<dyn RelaySimulator>` so
/// `submit_bundle` is not even type-reachable from here (CW-5
/// type-system guarantee).
/// R-E21: trait abstraction over the comparator's mismatch journal so
/// CW-2-fail can inject a programmable failure mode and verify
/// fail-closed broadcast inhibition.
pub trait MismatchJournalSink: Send + 'static {
    fn append(
        &mut self,
        env: &EventEnvelope<MismatchAbort>,
    ) -> Result<(), rust_lmax_mev_journal::JournalError>;
    fn flush(&mut self) -> Result<(), rust_lmax_mev_journal::JournalError>;
}

impl MismatchJournalSink for rust_lmax_mev_journal::FileJournal<MismatchAbort> {
    fn append(
        &mut self,
        env: &EventEnvelope<MismatchAbort>,
    ) -> Result<(), rust_lmax_mev_journal::JournalError> {
        rust_lmax_mev_journal::FileJournal::append(self, env).map(|_| ())
    }
    fn flush(&mut self) -> Result<(), rust_lmax_mev_journal::JournalError> {
        rust_lmax_mev_journal::FileJournal::flush(self)
    }
}

/// Public for CW-1..5 integration tests in `crates/app/tests/`.
#[allow(clippy::too_many_arguments)]
pub async fn comparator_driver<J: MismatchJournalSink>(
    mut rx: broadcast::Receiver<EventEnvelope<SimulationOutcomeWithFingerprint>>,
    comparator_tx: broadcast::Sender<MismatchCheckPassed>,
    mismatch_tx: broadcast::Sender<MismatchAbortRecord>,
    relay_sim: Option<Arc<dyn RelaySimulator>>,
    constructor: Arc<BundleConstructor>,
    mut journal: J,
    name: &'static str,
) {
    if relay_sim.is_none() {
        tracing::info!(
            consumer = name,
            "no relays configured (relay.enabled_relays empty per DP-E3); comparator inert"
        );
        return;
    }
    let relay_sim = relay_sim.expect("just checked is_some");
    let mut seq: u64 = 0;
    loop {
        match rx.recv().await {
            Ok(envelope) => {
                let cc = envelope.chain_context().clone();
                let correlation_id = envelope.correlation_id();
                let local_outcome = envelope.payload().outcome.clone();
                let local_fingerprint = envelope.payload().fingerprint.clone();

                // Step 1: construct local bundle candidate (skip if Aborted).
                let candidate = match constructor.construct(&local_outcome) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                // Step 2: build local-side comparator inputs.
                let local_shape = LocalBundleShape {
                    expected_inclusion_index: 0,
                    expected_coinbase_transfer_wei: alloy_primitives::U256::ZERO,
                };

                // Step 3: build empty-`txs` relay request (P4-E has
                // no signer; adapter short-circuits with
                // UnsignedBundleUnavailable BEFORE any network I/O).
                let req = RelaySimRequest {
                    block_hash: alloy_primitives::B256::from(cc.block_hash),
                    state_block_number: cc.block_number,
                    txs: Vec::new(),
                };
                let relay_result = relay_sim.simulate_bundle(req).await;

                // Step 4: comparator (compare_result handles BOTH
                // Ok(outcome) and Err(_) → Unknown classification).
                let inputs = ComparatorInputs {
                    local: &local_outcome,
                    local_shape: &local_shape,
                    local_fingerprint: &local_fingerprint,
                };
                let cmp = compare_result(inputs, relay_result.as_ref());

                match cmp {
                    Ok(()) => {
                        // No subscriber reads comparator_tx in P4-E.
                        let _ = comparator_tx.send(MismatchCheckPassed { candidate });
                    }
                    Err(abort_box) => {
                        let abort = *abort_box;
                        // R-E9 + R-E21 + DP-E8 v0.4: SYNCHRONOUS
                        // journal append+flush BEFORE any downstream
                        // emission. R-E21 fail-closed: if seal +
                        // append + flush do NOT all succeed, do NOT
                        // emit the broadcast (an unjournaled abort
                        // must never be observable downstream).
                        let Some(env) = seal_envelope_with_context(
                            EventSource::Relay,
                            abort.clone(),
                            &mut seq,
                            cc,
                            correlation_id,
                        ) else {
                            tracing::error!(
                                consumer = name,
                                "comparator envelope seal failed; suppressing broadcast"
                            );
                            continue;
                        };
                        if let Err(e) = journal.append(&env) {
                            tracing::error!(
                                consumer = name,
                                error = %e,
                                "comparator journal append failed; suppressing broadcast"
                            );
                            continue;
                        }
                        if let Err(e) = journal.flush() {
                            tracing::error!(
                                consumer = name,
                                error = %e,
                                "comparator journal flush failed; suppressing broadcast"
                            );
                            continue;
                        }
                        // Append + flush both Ok → safe to emit the
                        // observability broadcast. No subscriber in
                        // P4-E.
                        let _ = mismatch_tx.send(MismatchAbortRecord { abort });
                    }
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::error!(consumer = name, skipped = n, "broadcast lagged; aborting");
                break;
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
    if let Err(e) = journal.flush() {
        tracing::error!(consumer = name, error = %e, "journal flush at shutdown failed");
    }
}

/// Comparator-side helper that threads an upstream chain_context +
/// correlation_id into the new envelope (per DP-E8 v0.4
/// metadata-preservation requirement).
fn seal_envelope_with_context<T>(
    source: EventSource,
    payload: T,
    seq: &mut u64,
    chain_context: ChainContext,
    correlation_id: u64,
) -> Option<EventEnvelope<T>> {
    let meta = PublishMeta {
        source,
        chain_context,
        event_version: 1,
        correlation_id,
    };
    let now_ns: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1);
    *seq = seq.wrapping_add(1);
    EventEnvelope::seal(meta, payload, *seq, now_ns).ok()
}

/// Build the `Arc<dyn RelaySimulator>` from `config.relay.enabled_relays`.
/// Empty list → `None` (comparator_driver runs inert per DP-E3).
/// More-than-one entry: P4-E uses ONLY THE FIRST entry — multi-relay
/// fanout (concurrent `simulate_bundle` calls + first-success-wins
/// merging) is Phase 5+ work. Documented here so a future PR cannot
/// accidentally relax this without scope review.
fn build_relay_sim_from_config(
    cfg: &rust_lmax_mev_config::RelayConfig,
) -> Result<Option<Arc<dyn RelaySimulator>>, AppError> {
    use rust_lmax_mev_config::RelayKind;
    let Some(first) = cfg.enabled_relays.first() else {
        return Ok(None);
    };
    let timeout = cfg.simulate_timeout_ms;
    let arc: Arc<dyn RelaySimulator> = match first.kind {
        RelayKind::Flashbots => {
            let r = FlashbotsRelay::new(FlashbotsConfig {
                endpoint: first.endpoint.clone(),
                timeout_ms: timeout,
            })
            .map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))?;
            Arc::new(r)
        }
        RelayKind::Bloxroute => {
            let r = BloxrouteRelay::new(BloxrouteConfig {
                endpoint: first.endpoint.clone(),
                timeout_ms: timeout,
                api_key: first.api_key.clone(),
            })
            .map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))?;
            Arc::new(r)
        }
    };
    Ok(Some(arc))
}

fn seal_envelope<T>(source: EventSource, payload: T, seq: &mut u64) -> Option<EventEnvelope<T>> {
    let meta = PublishMeta {
        source,
        chain_context: ChainContext {
            chain_id: 1,
            block_number: 0,
            block_hash: [0u8; 32],
        },
        event_version: 1,
        correlation_id: 0,
    };
    let now_ns: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1);
    *seq = seq.wrapping_add(1);
    EventEnvelope::seal(meta, payload, *seq, now_ns).ok()
}
