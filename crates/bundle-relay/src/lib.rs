//! Phase 4 P4-E `BundleRelay` trait + payload types + `SubmitDisabled`
//! invariant. Per the user-approved P4-E execution note v0.6 (manual
//! Codex APPROVED HIGH 2026-05-10 KST).
//!
//! **HARD INVARIANT (DP-E1)**: every concrete-adapter
//! `BundleRelay::submit_bundle` impl in P4-E returns
//! `Err(BundleRelayError::SubmitDisabled)` UNCONDITIONALLY.
//!
//! **NO `crates/app` call site** invokes `submit_bundle` in P4-E
//! (CW-3 grep gate at batch close); the producer/comparator wiring
//! holds only `Arc<dyn RelaySimulator>` and never constructs
//! `Arc<dyn BundleRelay>` (DP-E6 v0.5 + DP-E13 v0.3).
//!
//! Phase 5 Safety Gate is the only path to enabling real submission.

pub mod kill_switch;
pub mod rkyv_compat;

pub use kill_switch::KillSwitch;

use async_trait::async_trait;
use rust_lmax_mev_relay_sim::RelaySimulator;
use serde::{Deserialize, Serialize};

use alloy_primitives::{Address, U256};

/// Carrier of one signed bundle ready for submission. P4-E ships
/// the type; the only writers in P4-E are tests + the deliberately
/// fail-closed adapter `submit_bundle` impls. Real signers + production
/// key material land in Phase 5 Safety Gate.
///
/// Per-field rkyv adapters (R-E12): `Address` + `U256` cannot derive
/// rkyv natively. `block_hash` is `[u8; 32]` directly (rkyv-native).
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    Serialize,
    Deserialize,
)]
pub struct SignedBundle {
    pub block_hash: [u8; 32],
    pub state_block_number: u64,
    pub signed_txs: Vec<Vec<u8>>,
    #[rkyv(with = crate::rkyv_compat::AddressAsBytes)]
    pub coinbase_recipient: Address,
    #[rkyv(with = crate::rkyv_compat::U256AsBytes)]
    pub coinbase_transfer_wei: U256,
    pub validity_block_min: u64,
    pub validity_block_max: u64,
}

/// Receipt from a submission. P4-E ships the type; no writer exists
/// (every `submit_bundle` returns `Err(SubmitDisabled)`).
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    Serialize,
    Deserialize,
)]
pub struct SubmissionReceipt {
    pub relay_name: String,
    pub bundle_hash: String,
    pub submitted_at_unix_ns: u64,
    /// P6B-E2 D-E2-3: locally-computed `keccak256(concat(signed_txs))`
    /// rendered as lowercase `0x<64hex>`. Empty string on the Ok-path
    /// when `bundle_hash == local_bundle_hash` (the verified-match
    /// case can elide the duplicate). Populated on every mismatch
    /// record so the audit reader can compare side-by-side.
    pub local_bundle_hash: String,
}

/// `BundleRelay` operation errors. `#[non_exhaustive]` so future
/// submit-path codes land additively.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BundleRelayError {
    /// HARD INVARIANT (P4-E §DP-E1): every adapter `submit_bundle`
    /// returns this variant. There is no caller in P4-E that invokes
    /// `submit_bundle` (CW-3 grep gate). Phase 5 Safety Gate is the
    /// only path to enabling real submission. The Display message
    /// MUST contain the substring "Phase 5 Safety Gate" so a future
    /// PR that loosens the invariant is forced to also update test
    /// text + spec docs (BR-3 spec-drift guard).
    #[error("submit_bundle disabled in this build (Phase 5 Safety Gate required)")]
    SubmitDisabled,

    /// P5-D DP-D3: Kill switch active -- process-wide execution disabled.
    /// The `Display` text contains literal `"kill switch active"` AND
    /// literal `"Phase 5 P5-D"` for the KS-3 BR-3-style spec-drift guard.
    /// PRECEDENCE: when a `submit_bundle` impl is wired to a kill switch
    /// (Phase 6), it MUST return this variant BEFORE returning
    /// `SubmitDisabled` if the kill switch is active.
    #[error("kill switch active -- Phase 5 P5-D execution disabled")]
    KillSwitchActive,

    /// P6B-E1 D-E1-2: adapter `submit_bundle` was called with a
    /// configured endpoint whose URL host is NOT in
    /// `{"127.0.0.1", "localhost", "::1"}`. Defense-in-depth with
    /// `ConfigError::LiveSendRequiresLocalhostEndpoint`: even if a
    /// non-localhost endpoint somehow bypasses the config validate
    /// gate, the adapter fails-closed at runtime before any HTTP I/O.
    /// PRECEDENCE: fires AFTER the kill-switch guard so an active
    /// kill switch still reports `KillSwitchActive`. P6B-E2 is the
    /// only path to unlocking non-localhost endpoints.
    #[error(
        "submit_bundle rejected: non-localhost relay endpoint not permitted until Phase 6b-E2"
    )]
    SubmitDisabledNonLocalhost,

    /// P6B-E1 D-E1-2: the localhost HTTP POST to `eth_sendBundle`
    /// failed at the transport or response-parse layer. Wraps
    /// non-success status codes, body-read errors, and JSON-RPC
    /// error envelopes. Payload-free per the workspace error
    /// convention; details land in the journaled audit event (not
    /// landed in P6B-E1; P6B-E2 may add).
    #[error("submit_bundle HTTP transport failed")]
    SubmitHttpFailed,

    /// P6B-E2 D-E2-2: `submission_driver`'s G12 step-6 keccak compare
    /// detected that the relay-returned `bundleHash` does not equal
    /// `keccak256(concat(signed_txs))`. Synthesized by the driver (not
    /// returned by the adapter) after an Ok-shaped HTTP response is
    /// parsed; the mismatch record is appended to the submission journal
    /// with `local_bundle_hash` populated before the next iteration.
    /// Payload-free per the workspace error convention.
    ///
    /// **P6B-F NOTE-2 (audit-note follow-up)**: this variant is
    /// intentionally reserved -- it is referenced only by the
    /// `submission_driver` WARN log path and is NOT returned by any
    /// production code at P6B-F close (the keccak compare is a soft
    /// audit gate; the journaled mismatch record is the durable signal).
    /// A downstream consumer that wants a hard-fail policy on
    /// bundle-hash divergence can promote this variant to a return
    /// value in a future batch without ABI churn (Q-E2-3 lock).
    #[error("bundle hash mismatch: relay-returned hash != local keccak")]
    BundleHashMismatch,
}

/// P6B-E1 D-E1-5: in-process broadcast envelope carrying everything
/// `submission_driver` needs to run the G12 7-step chain INHERITING
/// G13. `comparator_driver` builds this and broadcasts on
/// `submission_tx` ONLY when its comparator match passes AND the
/// upstream `signed_bytes` is `Some(_)`. The struct is in-process
/// only (no rkyv / serde derives -- never journaled or sent over
/// the wire).
#[derive(Debug, Clone)]
pub struct SubmissionAttempt {
    /// Signed-bundle bytes ready for `eth_sendBundle`. The presence of
    /// this field is the G12 Step 2 ("Signer Ok") structural witness.
    /// `submission_driver` rejects iterations where the carrying
    /// envelope omits this (defense-in-depth at the consumer side).
    pub signed_bundle: SignedBundle,
}

/// Object-safe async trait for relay endpoints that expose both the
/// simulation and submission surfaces. P4-E adapters implement both
/// `RelaySimulator` and `BundleRelay`, but P4-E app/comparator wiring
/// stores them only as `Arc<dyn RelaySimulator>`; no `dyn BundleRelay`
/// object and no trait-object upcast is constructed in `crates/app`.
/// The `dyn BundleRelay` shape exists for concrete-adapter
/// submit-disabled tests and Phase 5+ submission consumers.
#[async_trait]
pub trait BundleRelay: RelaySimulator + Send + Sync + 'static {
    fn name(&self) -> &str;

    /// HARD INVARIANT (P4-E §DP-E1): every impl in P4-E returns
    /// `Err(BundleRelayError::SubmitDisabled)`. No call site exists
    /// in P4-E that invokes this method. Phase 5 Safety Gate is the
    /// only path to enabling real submission.
    ///
    /// PRECEDENCE (P5-D DP-D4): when a kill switch is threaded into
    /// the implementation (Phase 6), the impl MUST check
    /// `KillSwitch::is_active()` and return
    /// `Err(BundleRelayError::KillSwitchActive)` BEFORE returning
    /// `Err(SubmitDisabled)` if the kill switch is active. P5-D ships
    /// the type + error variant; per-adapter wiring is deferred to
    /// Phase 6 per overview Q-P5-5.
    async fn submit_bundle(
        &self,
        bundle: &SignedBundle,
    ) -> Result<SubmissionReceipt, BundleRelayError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_bundle() -> SignedBundle {
        SignedBundle {
            block_hash: [0xAB; 32],
            state_block_number: 22_000_000,
            signed_txs: vec![vec![0x01, 0x02, 0x03], vec![0xAA, 0xBB]],
            coinbase_recipient: Address::from([0xCD; 20]),
            coinbase_transfer_wei: U256::from(1_000_000u64),
            validity_block_min: 22_000_000,
            validity_block_max: 22_000_005,
        }
    }

    fn sample_receipt() -> SubmissionReceipt {
        SubmissionReceipt {
            relay_name: "flashbots".into(),
            bundle_hash: "0xdeadbeef".into(),
            submitted_at_unix_ns: 1_700_000_000_000_000_000,
            local_bundle_hash: String::new(),
        }
    }

    /// BR-1 (transitive via concrete-adapter tests in P4-E): trait
    /// is object-safe. Compile-asserted by the placeholder dyn
    /// reference below + the adapter trait-object construction in
    /// `crates/relay-clients/tests/submit_disabled.rs`.
    #[allow(dead_code)]
    fn br_1_object_safety_compile_check(_relay: &dyn BundleRelay) {}

    /// BR-2: SignedBundle + SubmissionReceipt rkyv + serde round-trip.
    #[test]
    fn br_2_signed_bundle_and_receipt_round_trip() {
        let original = sample_bundle();
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("rkyv serialize");
        let decoded: SignedBundle = rkyv::from_bytes::<SignedBundle, rkyv::rancor::Error>(&bytes)
            .expect("rkyv deserialize");
        assert_eq!(original, decoded);
        let bin = bincode::serialize(&original).expect("bincode serialize");
        let from_bin: SignedBundle = bincode::deserialize(&bin).expect("bincode deserialize");
        assert_eq!(original, from_bin);

        let receipt = sample_receipt();
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&receipt).expect("rkyv serialize");
        let decoded: SubmissionReceipt =
            rkyv::from_bytes::<SubmissionReceipt, rkyv::rancor::Error>(&bytes)
                .expect("rkyv deserialize");
        assert_eq!(receipt, decoded);
        let bin = bincode::serialize(&receipt).expect("bincode serialize");
        let from_bin: SubmissionReceipt = bincode::deserialize(&bin).expect("bincode deserialize");
        assert_eq!(receipt, from_bin);
    }

    /// BR-3 (spec-drift guard): SubmitDisabled Display message
    /// contains "Phase 5 Safety Gate" so loosening the invariant
    /// forces a test-text + spec-doc update.
    #[test]
    fn br_3_submit_disabled_display_contains_phase_5_safety_gate() {
        let err = BundleRelayError::SubmitDisabled;
        let display = format!("{err}");
        assert!(
            display.contains("Phase 5 Safety Gate"),
            "BR-3: SubmitDisabled Display must contain 'Phase 5 Safety Gate'; got {display:?}"
        );
    }

    /// KS-1 (P5-D DP-D2): `KillSwitch::new` initial state matches the
    /// `initial_disabled` arg.
    #[test]
    fn ks_1_kill_switch_initial_state() {
        assert!(!KillSwitch::new(false).is_active());
        assert!(KillSwitch::new(true).is_active());
    }

    /// KS-2 (P5-D DP-D2): `set_active` flips; the underlying
    /// `Arc<AtomicBool>` is shared across `Clone` so a flip via one
    /// clone is visible from every other clone.
    #[test]
    fn ks_2_kill_switch_toggle_and_shared_state() {
        let ks = KillSwitch::new(false);
        let ks_clone = ks.clone();

        assert!(!ks.is_active());
        assert!(!ks_clone.is_active());

        ks.set_active(true);
        assert!(ks.is_active());
        assert!(ks_clone.is_active());

        ks_clone.set_active(false);
        assert!(!ks.is_active());
        assert!(!ks_clone.is_active());
    }

    /// KS-3 (P5-D DP-D3 BR-3-style spec-drift guard): `Display` of
    /// `KillSwitchActive` MUST contain literal `"kill switch active"`
    /// AND literal `"Phase 5 P5-D"`.
    #[test]
    fn ks_3_kill_switch_active_display_literals() {
        let err = BundleRelayError::KillSwitchActive;
        let display = format!("{err}");
        assert!(
            display.contains("kill switch active"),
            "KS-3: KillSwitchActive Display must contain 'kill switch active'; got {display:?}"
        );
        assert!(
            display.contains("Phase 5 P5-D"),
            "KS-3: KillSwitchActive Display must contain 'Phase 5 P5-D'; got {display:?}"
        );
    }
}
