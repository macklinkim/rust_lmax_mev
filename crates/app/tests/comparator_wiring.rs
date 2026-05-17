//! Phase 4 P4-E CW-1..5 tests for the `comparator_driver` per the
//! user-approved execution note v0.6 §D-E3 + DP-E12 + DP-E13.
//!
//! - CW-1 happy path: mock relay returns Ok-equivalent → comparator
//!   emits `MismatchCheckPassed`; no journal entry.
//! - CW-2 mismatch path: mock relay returns Err → abort journal
//!   contains the abort BEFORE any broadcast subscriber observes the
//!   `MismatchAbortRecord` (R-E9 + DP-E8 v0.4 ordering).
//! - CW-3 grep gate: zero `Arc<dyn BundleRelay>` / `submit_bundle(`
//!   call sites in `crates/app` (verified by ripgrep at batch close;
//!   asserted programmatically here against the static source bytes).
//! - CW-4 every relay error variant + every comparator outcome ends
//!   at `MismatchCheckPassed` (Ok only) or a journaled `MismatchAbort`
//!   (every Err variant); never submit (DP-E12).
//! - CW-5 type-system gate: comparator_driver constructor takes
//!   `Arc<dyn RelaySimulator>`, not `Arc<dyn BundleRelay>` (DP-E13
//!   v0.3). Compile-asserted via the public
//!   `_cw_5_compile_check_comparator_driver_takes_dyn_relay_simulator`
//!   helper.

mod common;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use alloy_primitives::{B256, U256};
use rust_lmax_mev_app::KillSwitch;
use rust_lmax_mev_app::{
    _cw_5_compile_check_comparator_driver_takes_dyn_relay_simulator, comparator_driver,
    MismatchAbortRecord, MismatchCheckPassed, MismatchJournalSink,
    SimulationOutcomeWithFingerprint,
};
use rust_lmax_mev_execution::{BundleConfig, BundleConstructor};
use rust_lmax_mev_journal::FileJournal;
use rust_lmax_mev_relay_sim::{
    MismatchAbort, MockRelaySimulator, RelaySimError, RelaySimStatus, RelaySimulationOutcome,
    RelaySimulator,
};
use rust_lmax_mev_simulator::{LocalStateFingerprint, ProfitSource, SimStatus, SimulationOutcome};
use rust_lmax_mev_types::{ChainContext, EventEnvelope, EventSource, PublishMeta};
use tokio::sync::broadcast;

fn sample_outcome_envelope() -> EventEnvelope<SimulationOutcomeWithFingerprint> {
    let outcome = SimulationOutcome {
        opportunity_block_number: 22_000_000,
        gas_used: 100_000,
        status: SimStatus::Success,
        // Construct must succeed → profit > 0.
        simulated_profit_wei: U256::from(50_000u64),
        profit_source: ProfitSource::RevmComputed,
    };
    let fingerprint = LocalStateFingerprint {
        block_hash: B256::from([0xCD; 32]),
        observations: Vec::new(),
    };
    let payload = SimulationOutcomeWithFingerprint {
        outcome,
        fingerprint,
        signed_bundle: None,
    };
    let meta = PublishMeta {
        source: EventSource::Simulator,
        chain_context: ChainContext {
            chain_id: 1,
            block_number: 22_000_000,
            block_hash: [0xCD; 32],
        },
        event_version: 1,
        correlation_id: 99,
    };
    EventEnvelope::seal(meta, payload, 1, 1_700_000_000_000_000_000).expect("envelope seal")
}

fn sample_relay_outcome_matching_local() -> RelaySimulationOutcome {
    // Build a relay outcome that the comparator will accept as a
    // perfect match against the candidate constructed from
    // sample_outcome_envelope() above (Profit/Gas: zero + zero match
    // because the comparator compares local SimulationOutcome's
    // simulated_profit_wei against relay measured_profit_wei; for the
    // mock to produce Ok we need to also match those values).
    // sample local: gas=100_000, profit=50_000 wei.
    RelaySimulationOutcome {
        gas_used: 100_000,
        status: RelaySimStatus::Success,
        measured_profit_wei: U256::from(50_000u64),
        state_observations: Vec::new(),
        inclusion_index: 0,
        coinbase_transfer_wei: U256::ZERO,
    }
}

fn open_journal(tempdir: &std::path::Path, name: &str) -> (FileJournal<MismatchAbort>, PathBuf) {
    let path = tempdir.join(name);
    let j = FileJournal::open(&path).expect("journal open");
    (j, path)
}

/// CW-5 (DP-E13 v0.3) type-system check: the comparator_driver's
/// relay-sim parameter must be `Arc<dyn RelaySimulator>`, not
/// `Arc<dyn BundleRelay>`. The compile-check helper exists in
/// `crates/app::lib.rs` and accepts only the simulator-trait object;
/// constructing one + invoking the helper is the type-system test.
#[test]
fn cw_5_comparator_driver_takes_dyn_relay_simulator() {
    let mock: Arc<dyn RelaySimulator> = Arc::new(MockRelaySimulator::default());
    _cw_5_compile_check_comparator_driver_takes_dyn_relay_simulator(mock);
}

/// CW-3 source-byte grep at P6B-E1 close: the P4-E "0 hits" invariant
/// is RELAXED to "exactly 1 documented call site" per the v0.1 plan
/// lock (J): G3 grows 0 -> 1 (the new `submission_driver` task body's
/// `submit_bundle(&attempt.signed_bundle)` call) and G4 grows 0 -> 1
/// (the `Arc<dyn BundleRelay>` parameter of `submission_driver`).
///
/// The `dyn BundleRelay` substring appears in `lib.rs` only at the
/// `submission_driver` signature, `build_bundle_relay_from_config`,
/// and the `Option<Arc<dyn BundleRelay>>` plumbing into `wire_phase4`.
/// This test counts occurrences and asserts an upper bound to catch
/// future PRs that grow the surface beyond the documented set.
#[test]
fn cw_3_bundle_relay_callsite_count_at_p6b_e1_close() {
    let src = include_str!("../src/lib.rs");
    // P6B-E1: count `.submit_bundle(` METHOD invocations (not the
    // bare `submit_bundle(` token which also appears in doc-comments
    // referencing the symbol by name). Exactly 1 method call
    // expected inside `submission_driver`.
    let submit_method_call_count = src.matches(".submit_bundle(").count();
    assert_eq!(
        submit_method_call_count, 1,
        "CW-3 P6B-E1: exactly 1 .submit_bundle( method call in crates/app/src/lib.rs (submission_driver); got {submit_method_call_count}",
    );
    // The `submission_driver` parameter + the `build_bundle_relay_from_config`
    // return type + the local binding + comments bring this to a small
    // bounded number. The exact count can drift with cosmetic changes;
    // assert an upper bound that catches an order-of-magnitude growth.
    let dyn_bundle_relay_count = src.matches("dyn BundleRelay").count();
    assert!(
        dyn_bundle_relay_count <= 12,
        "CW-3 P6B-E1: `dyn BundleRelay` mentions in crates/app/src/lib.rs grew unexpectedly; got {dyn_bundle_relay_count}",
    );
}

/// CW-1 happy path: mock relay returns matching outcome →
/// comparator emits `MismatchCheckPassed`; journal stays empty.
#[tokio::test(flavor = "multi_thread")]
async fn cw_1_happy_path_emits_mismatch_check_passed() {
    let dir = tempfile::tempdir().unwrap();
    let (journal, journal_path) = open_journal(dir.path(), "cw1.bin");
    let (sim_tx, sim_rx) = broadcast::channel::<EventEnvelope<SimulationOutcomeWithFingerprint>>(8);
    let (cmp_tx, mut cmp_rx) = broadcast::channel::<MismatchCheckPassed>(8);
    let (mismatch_tx, _mismatch_rx) = broadcast::channel::<MismatchAbortRecord>(8);

    let mock = Arc::new(MockRelaySimulator::default());
    mock.program(Ok(sample_relay_outcome_matching_local()));
    let relay_sim: Arc<dyn RelaySimulator> = mock;

    let bundle = Arc::new(BundleConstructor::new(BundleConfig::defaults()).expect("ctor"));

    // P6B-E1 D-E1-6: comparator_driver now also takes a
    // submission_tx Sender. Existing CW tests don't exercise the
    // submission path; create a no-subscriber broadcast and drop the
    // receiver immediately.
    let (sub_tx, _) = tokio::sync::broadcast::channel(8);
    let driver = tokio::spawn(comparator_driver(
        sim_rx,
        cmp_tx.clone(),
        mismatch_tx,
        sub_tx,
        Some(relay_sim),
        bundle,
        journal,
        "cw-1-driver",
        KillSwitch::new(false),
    ));

    sim_tx.send(sample_outcome_envelope()).expect("publish ok");

    let passed = tokio::time::timeout(Duration::from_secs(2), cmp_rx.recv())
        .await
        .expect("comparator must emit within 2s")
        .expect("MismatchCheckPassed expected");
    assert!(passed.candidate.simulated_profit_wei > U256::ZERO);

    drop(sim_tx);
    drop(cmp_tx);
    let _ = driver.await;

    // Journal must be untouched on the happy path.
    let bytes = std::fs::read(&journal_path).unwrap_or_default();
    // FileJournal writes an 8-byte file header on open; nothing else
    // on the happy path.
    assert!(
        bytes.len() <= 8,
        "CW-1: journal must contain no records on happy path; got {} bytes",
        bytes.len()
    );
}

/// CW-2 mismatch path: mock returns Err(Transport) → comparator
/// journals the abort BEFORE the broadcast subscriber observes the
/// `MismatchAbortRecord` (R-E9 + DP-E8 v0.4 synchronous ordering).
/// Test reads the journal file BEFORE the broadcast recv to verify.
#[tokio::test(flavor = "multi_thread")]
async fn cw_2_mismatch_path_journals_before_broadcast() {
    let dir = tempfile::tempdir().unwrap();
    let (journal, journal_path) = open_journal(dir.path(), "cw2.bin");
    let (sim_tx, sim_rx) = broadcast::channel::<EventEnvelope<SimulationOutcomeWithFingerprint>>(8);
    let (cmp_tx, _cmp_rx) = broadcast::channel::<MismatchCheckPassed>(8);
    let (mismatch_tx, mut mismatch_rx) = broadcast::channel::<MismatchAbortRecord>(8);

    let mock = Arc::new(MockRelaySimulator::default());
    mock.program(Err(RelaySimError::Transport("simulated".into())));
    let relay_sim: Arc<dyn RelaySimulator> = mock;

    let bundle = Arc::new(BundleConstructor::new(BundleConfig::defaults()).expect("ctor"));

    let (sub_tx_cw2, _) = tokio::sync::broadcast::channel(8);
    let driver = tokio::spawn(comparator_driver(
        sim_rx,
        cmp_tx,
        mismatch_tx.clone(),
        sub_tx_cw2,
        Some(relay_sim),
        bundle,
        journal,
        "cw-2-driver",
        KillSwitch::new(false),
    ));

    sim_tx.send(sample_outcome_envelope()).expect("publish ok");

    // Wait for the broadcast emission. The driver guarantees journal
    // append+flush completes BEFORE this emission.
    let record = tokio::time::timeout(Duration::from_secs(2), mismatch_rx.recv())
        .await
        .expect("comparator must emit within 2s")
        .expect("MismatchAbortRecord expected");
    // R-E9 + DP-E8 v0.4 ordering: at the moment the broadcast was
    // received, the journal MUST already contain the record.
    let bytes = std::fs::read(&journal_path).expect("journal readable");
    assert!(
        bytes.len() > 8,
        "CW-2: journal must contain the abort record before the broadcast emit; got {} bytes",
        bytes.len()
    );
    // Sanity: the abort category was populated by the comparator.
    let _ = record.abort.category;

    drop(sim_tx);
    drop(mismatch_tx);
    let _ = driver.await;
}

/// CW-4 (DP-E12): drives every relay error variant + the
/// `compare_result` mismatch path. EVERY case must end at either a
/// `MismatchCheckPassed` (only the Ok arm) or a journaled
/// `MismatchAbort` — never submit.
#[tokio::test(flavor = "multi_thread")]
async fn cw_4_every_relay_outcome_terminates_at_passed_or_journal() {
    use rust_lmax_mev_relay_sim::{compare_result, ComparatorInputs, LocalBundleShape};
    let local = SimulationOutcome {
        opportunity_block_number: 22_000_000,
        gas_used: 100_000,
        status: SimStatus::Success,
        simulated_profit_wei: U256::from(50_000u64),
        profit_source: ProfitSource::RevmComputed,
    };
    let shape = LocalBundleShape {
        expected_inclusion_index: 0,
        expected_coinbase_transfer_wei: U256::ZERO,
    };
    let fp = LocalStateFingerprint {
        block_hash: B256::from([0xCD; 32]),
        observations: Vec::new(),
    };
    // Each Err variant must classify as Unknown via compare_result.
    for err in [
        RelaySimError::NotConfigured,
        RelaySimError::Transport("x".into()),
        RelaySimError::UnrecognizedResponse("x".into()),
        RelaySimError::UnsignedBundleUnavailable,
    ] {
        let abort = compare_result(
            ComparatorInputs {
                local: &local,
                local_shape: &shape,
                local_fingerprint: &fp,
            },
            Err(&err),
        )
        .expect_err("every relay-sim Err must classify as MismatchAbort");
        assert_eq!(
            abort.category,
            rust_lmax_mev_types::MismatchCategory::Unknown
        );
        // The wiring's abort path is exercised end-to-end by CW-2.
        // Per DP-E12 there is NO third terminal outcome.
    }
    // The Ok arm is exercised by CW-1.
}

/// Programmable failing journal for CW-2-fail (R-E21). Implements
/// `MismatchJournalSink` and returns `Err(JournalError)` from
/// `append`; the comparator MUST fail-closed (no broadcast) on this
/// path.
struct FailingJournal {
    appends_attempted: Arc<std::sync::Mutex<usize>>,
}

impl MismatchJournalSink for FailingJournal {
    fn append(
        &mut self,
        _env: &EventEnvelope<MismatchAbort>,
    ) -> Result<(), rust_lmax_mev_journal::JournalError> {
        *self.appends_attempted.lock().unwrap() += 1;
        Err(rust_lmax_mev_journal::JournalError::Io(
            std::io::Error::other("simulated journal failure"),
        ))
    }
    fn flush(&mut self) -> Result<(), rust_lmax_mev_journal::JournalError> {
        Ok(())
    }
}

/// CW-2-fail (R-E21): if `journal.append` fails, the comparator MUST
/// NOT emit `MismatchAbortRecord`. Verifies fail-closed broadcast
/// inhibition end-to-end.
#[tokio::test(flavor = "multi_thread")]
async fn cw_2_fail_journal_failure_suppresses_broadcast() {
    let (sim_tx, sim_rx) = broadcast::channel::<EventEnvelope<SimulationOutcomeWithFingerprint>>(8);
    let (cmp_tx, _cmp_rx) = broadcast::channel::<MismatchCheckPassed>(8);
    let (mismatch_tx, mut mismatch_rx) = broadcast::channel::<MismatchAbortRecord>(8);

    // Mock relay returns Err so the comparator enters the abort path.
    let mock = Arc::new(MockRelaySimulator::default());
    mock.program(Err(RelaySimError::Transport("simulated".into())));
    let relay_sim: Arc<dyn RelaySimulator> = mock;

    let bundle = Arc::new(BundleConstructor::new(BundleConfig::defaults()).expect("ctor"));
    let appends_attempted = Arc::new(std::sync::Mutex::new(0usize));
    let failing = FailingJournal {
        appends_attempted: Arc::clone(&appends_attempted),
    };

    let (sub_tx_fail, _) = tokio::sync::broadcast::channel(8);
    let driver = tokio::spawn(comparator_driver(
        sim_rx,
        cmp_tx,
        mismatch_tx.clone(),
        sub_tx_fail,
        Some(relay_sim),
        bundle,
        failing,
        "cw-2-fail-driver",
        KillSwitch::new(false),
    ));

    sim_tx.send(sample_outcome_envelope()).expect("publish ok");

    // Give the driver a generous window to attempt append + (per
    // R-E21) fail-closed by NOT broadcasting. We expect a Timeout
    // (no message) on mismatch_rx within that window.
    let result = tokio::time::timeout(Duration::from_millis(500), mismatch_rx.recv()).await;
    assert!(
        result.is_err(),
        "R-E21: comparator must NOT broadcast MismatchAbortRecord when journal.append fails; got {result:?}"
    );
    // Sanity: the driver did attempt the append (otherwise the test
    // could pass by spuriously not running the abort path at all).
    assert!(
        *appends_attempted.lock().unwrap() >= 1,
        "comparator must have attempted journal append on the abort path"
    );

    drop(sim_tx);
    drop(mismatch_tx);
    let _ = driver.await;
}

/// KS-5 (P5-D DP-D7): comparator_driver per-driver kill-switch guard.
/// Active kill switch suppresses the entire iteration: no comparator
/// broadcast, no mismatch broadcast, no journal append. Off (default)
/// behavior is byte-identical to CW-1 (covered by CW-1 itself).
#[tokio::test(flavor = "multi_thread")]
async fn ks_5_comparator_driver_kill_switch_suppresses_iteration() {
    let dir = tempfile::tempdir().unwrap();
    let (journal, journal_path) = open_journal(dir.path(), "ks5.bin");
    let (sim_tx, sim_rx) = broadcast::channel::<EventEnvelope<SimulationOutcomeWithFingerprint>>(8);
    let (cmp_tx, mut cmp_rx) = broadcast::channel::<MismatchCheckPassed>(8);
    let (mismatch_tx, mut mismatch_rx) = broadcast::channel::<MismatchAbortRecord>(8);

    let mock = Arc::new(MockRelaySimulator::default());
    mock.program(Ok(sample_relay_outcome_matching_local()));
    let relay_sim: Arc<dyn RelaySimulator> = mock;

    let bundle = Arc::new(BundleConstructor::new(BundleConfig::defaults()).expect("ctor"));

    // Kill switch ACTIVE from the start.
    let ks = KillSwitch::new(true);

    let (sub_tx_ks, _) = tokio::sync::broadcast::channel(8);
    let driver = tokio::spawn(comparator_driver(
        sim_rx,
        cmp_tx.clone(),
        mismatch_tx.clone(),
        sub_tx_ks,
        Some(relay_sim),
        bundle,
        journal,
        "ks-5-driver",
        ks,
    ));

    sim_tx.send(sample_outcome_envelope()).expect("publish ok");

    // Neither comparator nor mismatch should fire while kill is active.
    let cmp_result = tokio::time::timeout(Duration::from_millis(500), cmp_rx.recv()).await;
    assert!(
        cmp_result.is_err(),
        "KS-5: comparator must NOT emit MismatchCheckPassed while kill switch active; got {cmp_result:?}"
    );
    let mis_result = tokio::time::timeout(Duration::from_millis(50), mismatch_rx.recv()).await;
    assert!(
        mis_result.is_err(),
        "KS-5: comparator must NOT emit MismatchAbortRecord while kill switch active; got {mis_result:?}"
    );

    // Journal must remain at the file header (no abort records).
    let bytes = std::fs::read(&journal_path).unwrap_or_default();
    assert!(
        bytes.len() <= 8,
        "KS-5: journal must contain no records while kill switch active; got {} bytes",
        bytes.len()
    );

    drop(sim_tx);
    drop(cmp_tx);
    drop(mismatch_tx);
    let _ = driver.await;
}
