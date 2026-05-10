//! Phase 4 P4-E Flashbots + bloXroute HTTP relay-sim adapters.
//!
//! Per the user-approved P4-E execution note v0.6 (manual Codex
//! APPROVED HIGH 2026-05-10 KST). Each adapter implements
//! `RelaySimulator` (the simulator-only path used by the
//! `comparator_driver`) AND `BundleRelay` (whose `submit_bundle`
//! returns `Err(SubmitDisabled)` unconditionally per DP-E1).
//!
//! Read-only `eth_callBundle` only. NO `eth_sendBundle`, NO funded
//! key, NO signing infrastructure, NO production submission.

pub mod bloxroute;
pub(crate) mod call_bundle;
pub mod flashbots;

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
