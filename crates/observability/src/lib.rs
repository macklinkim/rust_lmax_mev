//! Phase 1 observability initializer for the LMAX-style MEV engine.
//!
//! Per Batch A execution note (`docs/superpowers/plans/2026-05-02-phase-1-
//! batch-a-foundation-execution.md`). Authoritative source: ADR-008
//! (Observability + CI Baseline) — `tracing-subscriber` for structured
//! logging + `metrics-exporter-prometheus` for the metrics facade.
//!
//! Single entrypoint: [`init`]. Process-global state is guarded by a
//! `OnceLock` so a second `init()` returns
//! [`ObservabilityError::AlreadyInitialized`] (per execution-note Risk
//! Decision 2 the `init()` API does not retry on port conflict after a
//! successful first call; that's an operator-layer concern in Phase 1).

use std::sync::OnceLock;

use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use rust_lmax_mev_config::{LogFormat, ObservabilityConfig};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter, Registry};

/// Process-wide single-init guard. The first successful [`init`] call
/// stores `()`; every subsequent call short-circuits to
/// `Err(ObservabilityError::AlreadyInitialized)` BEFORE touching the
/// global tracing or metrics registries (which are themselves single-
/// install).
static INIT_GUARD: OnceLock<()> = OnceLock::new();

/// Initializes the global tracing subscriber + Prometheus metrics
/// recorder. Must be called exactly once per process; a second call
/// returns [`ObservabilityError::AlreadyInitialized`].
///
/// Returns an opaque [`ObservabilityHandle`] that owns the
/// `PrometheusHandle`. The handle keeps the recorder alive for the
/// lifetime of the engine; drop it at process shutdown to release the
/// HTTP listener cleanly.
///
/// On `tracing_subscriber::try_init()` failure → `TracingInstall(_)`.
/// On `PrometheusBuilder::install_recorder()` failure → `PrometheusInstall(_)`.
pub fn init(config: &ObservabilityConfig) -> Result<ObservabilityHandle, ObservabilityError> {
    if INIT_GUARD.get().is_some() {
        return Err(ObservabilityError::AlreadyInitialized);
    }

    let env_filter = EnvFilter::try_new(&config.log_filter)
        .map_err(|e| ObservabilityError::TracingInstall(e.to_string()))?;

    match config.log_format {
        LogFormat::Json => Registry::default()
            .with(env_filter)
            .with(fmt::layer().json())
            .try_init()
            .map_err(|e| ObservabilityError::TracingInstall(e.to_string()))?,
        LogFormat::Pretty => Registry::default()
            .with(env_filter)
            .with(fmt::layer().pretty())
            .try_init()
            .map_err(|e| ObservabilityError::TracingInstall(e.to_string()))?,
    }

    let prom_handle = PrometheusBuilder::new()
        .with_http_listener(config.prometheus_listen)
        .install_recorder()
        .map_err(|e| ObservabilityError::PrometheusInstall(e.to_string()))?;

    // Lock the OnceLock only AFTER both installs succeed so that a
    // partial-install failure (e.g., Prometheus bind failure after
    // tracing succeeded) leaves the guard open. This is a Phase 1
    // simplification: in practice tracing's try_init is the irreversible
    // step, so a Prometheus failure after that point is rare and the
    // process should exit anyway.
    let _ = INIT_GUARD.set(());

    Ok(ObservabilityHandle { _prom: prom_handle })
}

/// Opaque handle that owns the Prometheus recorder. Keep this alive for
/// the lifetime of the engine; drop releases the HTTP listener.
pub struct ObservabilityHandle {
    // Prefixed `_` because the field is intentionally never read; its
    // job is to keep the underlying recorder + HTTP listener alive
    // until the handle is dropped.
    _prom: PrometheusHandle,
}

// Manual `Debug` because `PrometheusHandle` from
// `metrics-exporter-prometheus` 0.15 does not implement `Debug`. Required
// for ergonomic test diagnostics (`Result::expect_err` bound).
impl std::fmt::Debug for ObservabilityHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObservabilityHandle")
            .finish_non_exhaustive()
    }
}

/// All errors produced by [`init`].
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum ObservabilityError {
    /// `init()` was called more than once per process.
    #[error("observability::init() called more than once per process")]
    AlreadyInitialized,

    /// `tracing_subscriber::try_init()` failed (typically because another
    /// subscriber was already installed by a different code path).
    #[error("tracing subscriber install failed: {0}")]
    TracingInstall(String),

    /// `PrometheusBuilder::install_recorder()` failed (typically because
    /// the configured listen port is already bound).
    #[error("Prometheus exporter install failed: {0}")]
    PrometheusInstall(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    fn obs_config(port: u16) -> ObservabilityConfig {
        ObservabilityConfig {
            prometheus_listen: SocketAddr::from(([127, 0, 0, 1], port)),
            log_filter: "info".to_string(),
            log_format: LogFormat::Pretty,
        }
    }

    /// O-combined (per execution note Risk Decision 2): exercises the two
    /// reachable public-API cases of `init()` in a single ordered `#[test]`
    /// because (a) the OnceLock guard locks on first success and (b) the
    /// global tracing subscriber + global metrics recorder are process-
    /// wide singletons.
    ///
    /// Case 1 (success): first `init()` with port 0 (let OS pick) returns
    /// `Ok(handle)`. The handle is held until the end of the test scope so
    /// the recorder stays installed across case 2.
    ///
    /// Case 2 (already-initialized): a second `init()` call (any config)
    /// returns `Err(AlreadyInitialized)` because the OnceLock is now
    /// locked.
    ///
    /// Port-conflict coverage on `init()` is OUT of scope for Batch A per
    /// Risk Decision 2 (OnceLock short-circuits before the bind step in
    /// the public API); deferred to a future `try_install_recorder`
    /// private helper if needed.
    #[test]
    fn init_succeeds_then_rejects_double_init() {
        let _handle = init(&obs_config(0)).expect("first init must succeed");

        let err = init(&obs_config(0)).expect_err("second init must return AlreadyInitialized");
        assert!(
            matches!(err, ObservabilityError::AlreadyInitialized),
            "expected AlreadyInitialized, got {err:?}"
        );
    }
}
