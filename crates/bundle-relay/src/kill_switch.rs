//! P5-D DP-D2 process-wide kill switch.
//!
//! Transparent newtype over `Arc<AtomicBool>`. Every `KillSwitch::clone()`
//! shares the same underlying `AtomicBool`, so a flip from any clone is
//! visible from every other clone (Arc semantics).
//!
//! No `Default` derive (DP-D2): explicit ctor only, prevents accidental
//! "off by default" assumptions in tests / wiring.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Process-wide atomic kill switch for execution. `is_active() == true`
/// means execution is disabled (kill switch active); guarded code paths
/// MUST short-circuit before any submission-equivalent work.
#[derive(Debug, Clone)]
pub struct KillSwitch(Arc<AtomicBool>);

impl KillSwitch {
    /// Constructs a new `KillSwitch` with the given initial state.
    /// `true` means execution is disabled (kill switch active).
    pub fn new(initial_disabled: bool) -> Self {
        Self(Arc::new(AtomicBool::new(initial_disabled)))
    }

    /// Returns `true` if execution is currently disabled.
    pub fn is_active(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    /// Sets the execution-disabled state. `true` activates the kill
    /// switch (every guarded site short-circuits); `false` deactivates.
    /// Lock-free atomic store; runtime-agnostic.
    pub fn set_active(&self, disabled: bool) {
        self.0.store(disabled, Ordering::Release);
    }
}
