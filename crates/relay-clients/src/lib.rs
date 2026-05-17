//! Phase 4 P4-E Flashbots + bloXroute HTTP relay-sim adapters.
//!
//! Per the user-approved P4-E execution note v0.6 (manual Codex
//! APPROVED HIGH 2026-05-10 KST). Each adapter implements
//! `RelaySimulator` (the simulator-only path used by the
//! `comparator_driver`) AND `BundleRelay` (whose `submit_bundle`
//! returns `Err(SubmitDisabled)` unconditionally per DP-E1).
//!
//! Read-only `eth_callBundle` on the `RelaySimulator` path. NO
//! production key material, NO signing infrastructure. The
//! `BundleRelay::submit_bundle` body in `flashbots.rs` is extended
//! at P6B-E1 to perform `eth_sendBundle` POST under a
//! localhost-only gate (per `is_localhost_url(...)` + ALL of
//! `live_send=true`, `Production` profile, `HsmKms` key backend,
//! kill-switch inactive). The `bloxroute.rs` `submit_bundle` body
//! remains `Err(SubmitDisabled)` per P6B-D close + the v0.1 plan
//! lock (I) (single adapter at E1; Bloxroute parity is P6B-E2).
//! No real external relay URL anywhere in this crate.

pub mod bloxroute;
pub(crate) mod call_bundle;
pub mod flashbots;
pub(crate) mod send_bundle;

pub use bloxroute::{BloxrouteConfig, BloxrouteRelay};
pub use flashbots::{FlashbotsConfig, FlashbotsRelay};

#[cfg(test)]
mod tests {
    /// Smoke check that the crate compiles and links into the
    /// workspace. Real adapter tests live alongside each module
    /// and in `tests/`.
    #[test]
    fn crate_compiles() {
        assert_eq!(2 + 2, 4);
    }
}
