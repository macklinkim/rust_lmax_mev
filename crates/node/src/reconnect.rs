//! `ReconnectingStream` — wraps a stream factory with transparent
//! reconnect-on-close per ADR-007 + P2-A v0.4 Risk Decision 2.
//!
//! Implementation strategy: spawn a background tokio task that drives
//! the reconnect loop, pumping items from the current inner stream into
//! a `futures::channel::mpsc::unbounded` channel. The consumer holds the
//! `UnboundedReceiver` (which implements `Stream`) and is unaware of
//! reconnect events — the channel hides the seam.
//!
//! On inner-stream end: factory called again after `backoff(attempt)`.
//! On `factory()` returning `Err(NodeError::Closed)`: the background
//! task exits and the channel closes, terminating the outer stream.
//! On `factory()` returning any other error: forwarded to the consumer
//! and a retry is scheduled.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use futures::channel::mpsc;
use futures::{Stream, StreamExt};

use crate::error::NodeError;

type InnerStream<T> = Pin<Box<dyn Stream<Item = Result<T, NodeError>> + Send + 'static>>;
type FactoryFut<T> =
    Pin<Box<dyn Future<Output = Result<InnerStream<T>, NodeError>> + Send + 'static>>;

pub struct ReconnectingStream<T, F> {
    factory: F,
    backoff: Box<dyn Fn(u32) -> Duration + Send + Sync + 'static>,
    _marker: std::marker::PhantomData<T>,
}

impl<T, F> ReconnectingStream<T, F>
where
    T: Send + 'static,
    F: FnMut() -> FactoryFut<T> + Send + 'static,
{
    /// Default backoff: `crate::backoff_delay` (1s → 60s cap).
    pub fn new(factory: F) -> Self {
        Self {
            factory,
            backoff: Box::new(crate::backoff_delay),
            _marker: std::marker::PhantomData,
        }
    }

    /// Test-only override of the backoff function so tests can use
    /// zero-duration delays for fast deterministic execution.
    #[cfg(test)]
    pub fn new_with_backoff(
        factory: F,
        backoff: impl Fn(u32) -> Duration + Send + Sync + 'static,
    ) -> Self {
        Self {
            factory,
            backoff: Box::new(backoff),
            _marker: std::marker::PhantomData,
        }
    }

    /// Spawns the background reconnect task and returns the consumer-
    /// facing boxed stream. The returned stream terminates when the
    /// factory returns `Err(NodeError::Closed)`.
    pub fn into_stream(self) -> Pin<Box<dyn Stream<Item = Result<T, NodeError>> + Send + 'static>> {
        let (tx, rx) = mpsc::unbounded::<Result<T, NodeError>>();
        let ReconnectingStream {
            mut factory,
            backoff,
            ..
        } = self;
        tokio::spawn(async move {
            let mut attempt: u32 = 0;
            loop {
                match factory().await {
                    Err(NodeError::Closed) => break,
                    Err(other) => {
                        if tx.unbounded_send(Err(other)).is_err() {
                            return; // consumer dropped
                        }
                        tokio::time::sleep((backoff)(attempt)).await;
                        attempt = attempt.saturating_add(1);
                        continue;
                    }
                    Ok(mut inner) => {
                        attempt = 0;
                        while let Some(item) = inner.next().await {
                            if tx.unbounded_send(item).is_err() {
                                return; // consumer dropped
                            }
                        }
                        tracing::info!(
                            "node subscription stream ended; reconnecting after backoff"
                        );
                        tokio::time::sleep((backoff)(attempt)).await;
                        attempt = attempt.saturating_add(1);
                    }
                }
            }
        });
        Box::pin(rx)
    }
}
