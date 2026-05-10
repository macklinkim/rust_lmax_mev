//! Phase 4 P4-D relay sim comparator infrastructure.
//!
//! Per the user-approved P4-D execution note v0.4 (manual Codex
//! APPROVED HIGH 2026-05-10 KST). Ships ADR-006 §"Abort policy" types +
//! a zero-tolerance comparator + a `RelaySimulator` async trait + an
//! in-memory `MockRelaySimulator` for tests. **The actual
//! `eth_callBundle` HTTP client and the relay-sim → comparator wiring
//! into the producer chain land in P4-E.** P4-D's comparator is
//! exercised against the mock only.
//!
//! Forbids reaffirmed: no `eth_sendBundle`, no funded key, no signing,
//! no `live_send=true`, no relay submission, no live network tests.

pub mod rkyv_compat;

// Stub — implementation lands in the next commit per the P4-D plan
// (D-3.c types + D-3.d MockRelaySimulator). This commit ships only
// the workspace scaffold + crate metadata so the workspace compiles.

#[cfg(test)]
mod tests {
    /// Smoke check that the crate compiles and links into the
    /// workspace. Real CMP tests land alongside the comparator impl.
    #[test]
    fn crate_compiles() {
        assert_eq!(2 + 2, 4);
    }
}
