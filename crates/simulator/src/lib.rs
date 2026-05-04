//! Phase 3 P3-E LOCAL revm pre-sim shim per the user-approved P3-E
//! execution note v0.2 (DP-S1: ADR-006 strict "revm against the
//! current state snapshot" deferred to Phase 4 alongside ADR-007
//! archive node integration).
//!
//! `LocalSimulator` constructs a deterministic revm `Evm` around an
//! in-tree test contract bytecode (one `STOP` opcode), pre-funded EOA,
//! and an in-memory `CacheDB`. `simulate(&risk_checked)` executes one
//! transaction and returns a `SimulationOutcome` whose:
//!
//! - `gas_used` IS the real revm-reported gas for the test transaction
//!   (this number is legitimately measured in P3-E).
//! - `simulated_profit_wei` IS heuristic-passthrough from the upstream
//!   `RiskCheckedOpportunity` / `OpportunityEvent::expected_profit_wei`,
//!   stamped with `ProfitSource::HeuristicPassthrough`. Phase 4 swaps
//!   in real Uniswap bytecode + state-fetcher and flips `ProfitSource`
//!   to `RevmComputed` while keeping the API surface unchanged.
//!
//! No relay sim, no submission, no live mainnet, no funded key. Phase 4
//! adds those alongside `BundleRelay` per ADR-002 + ADR-006.

pub mod rkyv_compat;

use alloy_primitives::U256;
use revm::db::{CacheDB, EmptyDB};
use revm::primitives::{
    AccountInfo, Address as RevmAddress, Bytecode, Bytes as RevmBytes, ExecutionResult, HaltReason,
    TxKind, U256 as RevmU256,
};
use revm::Evm;
use rust_lmax_mev_risk::RiskCheckedOpportunity;
use serde::{Deserialize, Serialize};

/// Fixed deterministic addresses for the in-tree test bytecode + EOA.
/// Identical across every `LocalSimulator::new` call so S-2 determinism
/// holds without any per-instance randomness.
const TEST_CONTRACT_ADDRESS_BYTES: [u8; 20] = [0x42; 20];
const EOA_ADDRESS_BYTES: [u8; 20] = [0x11; 20];

/// Test contract bytecode: a single `STOP` opcode (0x00). Costs the
/// EVM intrinsic gas for the transaction call + a tiny constant for
/// the `STOP`. Picked because it is the minimum non-trivial bytecode
/// that exercises the full revm pipeline (deployment account → call
/// dispatch → opcode interpretation → halt).
const TEST_CONTRACT_BYTECODE: &[u8] = &[0x00];

/// Default gas limit per simulation: 30_000_000 (mainnet block-gas-limit
/// scale; conservative upper bound for any single-bundle local sim).
pub const DEFAULT_GAS_LIMIT_PER_SIM: u64 = 30_000_000;

/// Default base fee: 30 gwei (matches the gas-price proxy used by
/// `crates/risk` so the heuristic chain is internally consistent).
pub const DEFAULT_BASE_FEE_WEI_U128: u128 = 30_000_000_000;

/// Default EOA initial balance: 100 ETH (large enough for any clamped
/// per-bundle notional + gas; small enough to be obviously a test
/// fixture).
pub const DEFAULT_EOA_INITIAL_BALANCE_WEI_U128: u128 = 100_000_000_000_000_000_000;

/// Deterministic LOCAL simulation configuration. All values are
/// constants for the thin-path P3-E shim; Phase 4 replaces this with
/// per-block real chain-state config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimConfig {
    pub chain_id: u64,
    pub gas_limit_per_sim: u64,
    pub base_fee_wei: U256,
    pub eoa_initial_balance_wei: U256,
}

impl SimConfig {
    /// One-line spec-defaults constructor (chain_id=1 per ADR-002).
    pub fn defaults() -> Self {
        Self {
            chain_id: 1,
            gas_limit_per_sim: DEFAULT_GAS_LIMIT_PER_SIM,
            base_fee_wei: U256::from(DEFAULT_BASE_FEE_WEI_U128),
            eoa_initial_balance_wei: U256::from(DEFAULT_EOA_INITIAL_BALANCE_WEI_U128),
        }
    }
}

/// Provenance of `SimulationOutcome.simulated_profit_wei`. Phase 3 P3-E
/// always emits `HeuristicPassthrough` per the user-approved DP-S1
/// scope deferral; Phase 4 swaps in real bytecode + state and starts
/// emitting `RevmComputed`.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum ProfitSource {
    /// P3-E DP-S1: revm pipeline ran deterministically; the profit
    /// value is the upstream P3-C heuristic, not a revm-computed delta.
    HeuristicPassthrough,
    /// P4+: `simulated_profit_wei` is the actual revm-computed
    /// post-vs-pre balance delta against real chain state.
    RevmComputed,
}

/// Discrete simulation outcome status. `LocalSimulator::simulate`
/// MUST normalize revm's `HaltReason::OutOfGas(_)` to `OutOfGas` —
/// downstream consumers depend on this exact mapping (see S-3).
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum SimStatus {
    Success,
    Reverted { reason_hex: String },
    OutOfGas,
    HaltedOther { reason: String },
}

/// Output of one local pre-sim. Per `event-model.md` derives the
/// spec-mandated `Clone, Debug, PartialEq, Eq, rkyv::*, serde::*`.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct SimulationOutcome {
    pub opportunity_block_number: u64,
    pub gas_used: u64,
    pub status: SimStatus,
    #[rkyv(with = crate::rkyv_compat::U256AsBytes)]
    pub simulated_profit_wei: U256,
    pub profit_source: ProfitSource,
}

/// Setup / execution failures. `#[non_exhaustive]` so Phase 4 can add
/// variants without breaking downstream `match`.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum SimulationError {
    #[error("invalid SimConfig: {0}")]
    Setup(String),
    #[error("revm execution failed: {0}")]
    Execution(String),
}

/// Deterministic LOCAL revm pre-sim engine. P3-E DP-S1 shim per the
/// user-approved ADR-006 deferral. Stateless beyond the `SimConfig` +
/// pre-deployed addresses captured at construction time; `simulate()`
/// rebuilds a fresh `CacheDB` per call so determinism survives
/// repeated invocations.
pub struct LocalSimulator {
    cfg: SimConfig,
    contract_address: RevmAddress,
    eoa_address: RevmAddress,
    bytecode: Bytecode,
}

impl std::fmt::Debug for LocalSimulator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalSimulator")
            .field("cfg", &self.cfg)
            .finish_non_exhaustive()
    }
}

impl LocalSimulator {
    /// Validates `SimConfig`, captures the deterministic test
    /// addresses + bytecode, and returns a ready-to-simulate engine.
    /// Returns `Err(SimulationError::Setup)` for the obvious bad-config
    /// cases (S-5).
    pub fn new(cfg: SimConfig) -> Result<Self, SimulationError> {
        if cfg.chain_id == 0 {
            return Err(SimulationError::Setup(
                "chain_id must be non-zero".to_string(),
            ));
        }
        if cfg.gas_limit_per_sim == 0 {
            return Err(SimulationError::Setup(
                "gas_limit_per_sim must be non-zero".to_string(),
            ));
        }
        if cfg.eoa_initial_balance_wei.is_zero() {
            return Err(SimulationError::Setup(
                "eoa_initial_balance_wei must be non-zero".to_string(),
            ));
        }
        let contract_address = RevmAddress::from(TEST_CONTRACT_ADDRESS_BYTES);
        let eoa_address = RevmAddress::from(EOA_ADDRESS_BYTES);
        let bytecode = Bytecode::new_raw(RevmBytes::from_static(TEST_CONTRACT_BYTECODE));
        Ok(Self {
            cfg,
            contract_address,
            eoa_address,
            bytecode,
        })
    }

    pub fn cfg(&self) -> &SimConfig {
        &self.cfg
    }

    /// Runs the deterministic LOCAL revm shim for the given checked
    /// opportunity. Same input → byte-identical `SimulationOutcome`
    /// (S-2 determinism test). `simulated_profit_wei` is
    /// heuristic-passthrough from the upstream
    /// `risk_checked.opportunity.expected_profit_wei` per DP-S1; revm
    /// validates only the bytecode pipeline + the gas number.
    pub fn simulate(
        &self,
        risk_checked: &RiskCheckedOpportunity,
    ) -> Result<SimulationOutcome, SimulationError> {
        // Build a fresh CacheDB per call so prior runs do not leak
        // state into this one (determinism).
        let mut db = CacheDB::new(EmptyDB::default());
        db.insert_account_info(
            self.contract_address,
            AccountInfo {
                balance: RevmU256::ZERO,
                nonce: 0,
                code_hash: self.bytecode.hash_slow(),
                code: Some(self.bytecode.clone()),
            },
        );
        let eoa_balance_revm =
            RevmU256::from_be_bytes(self.cfg.eoa_initial_balance_wei.to_be_bytes::<32>());
        db.insert_account_info(
            self.eoa_address,
            AccountInfo {
                balance: eoa_balance_revm,
                nonce: 0,
                code_hash: revm::primitives::KECCAK_EMPTY,
                code: None,
            },
        );

        let chain_id = self.cfg.chain_id;
        let gas_limit = self.cfg.gas_limit_per_sim;
        let base_fee_revm = RevmU256::from_be_bytes(self.cfg.base_fee_wei.to_be_bytes::<32>());
        let caller = self.eoa_address;
        let target = self.contract_address;

        let mut evm = Evm::builder()
            .with_db(db)
            .modify_cfg_env(|c| {
                c.chain_id = chain_id;
            })
            .modify_block_env(|b| {
                b.basefee = base_fee_revm;
                b.gas_limit = RevmU256::from(gas_limit);
                b.number = RevmU256::from(risk_checked.opportunity.block_number);
            })
            .modify_tx_env(|tx| {
                tx.caller = caller;
                tx.transact_to = TxKind::Call(target);
                tx.gas_limit = gas_limit;
                tx.gas_price = base_fee_revm;
                tx.value = RevmU256::ZERO;
                tx.data = RevmBytes::new();
                tx.chain_id = Some(chain_id);
                tx.nonce = Some(0);
            })
            .build();

        let exec_result = evm
            .transact_commit()
            .map_err(|e| SimulationError::Execution(format!("{e:?}")))?;

        let (gas_used, status) = match exec_result {
            ExecutionResult::Success { gas_used, .. } => (gas_used, SimStatus::Success),
            ExecutionResult::Revert { gas_used, output } => (
                gas_used,
                SimStatus::Reverted {
                    reason_hex: hex_lower(&output),
                },
            ),
            ExecutionResult::Halt { reason, gas_used } => (gas_used, classify_halt(reason)),
        };

        Ok(SimulationOutcome {
            opportunity_block_number: risk_checked.opportunity.block_number,
            gas_used,
            status,
            simulated_profit_wei: risk_checked.opportunity.expected_profit_wei,
            profit_source: ProfitSource::HeuristicPassthrough,
        })
    }
}

/// Maps revm's `HaltReason` to the engine's discrete `SimStatus`.
/// Per the v0.2 plan + Codex 18:03:59 S-3 tightening: every variant
/// of `HaltReason::OutOfGas(_)` MUST normalize to `SimStatus::OutOfGas`.
/// Everything else falls into `HaltedOther` with the reason debug string.
fn classify_halt(reason: HaltReason) -> SimStatus {
    // revm 14 OutOfGasError variants: Basic, InvalidOperand, Memory,
    // MemoryLimit, Precompile. ALL of them normalize to
    // SimStatus::OutOfGas per Codex 18:03:59 S-3 tightening.
    match reason {
        HaltReason::OutOfGas(_) => SimStatus::OutOfGas,
        other => SimStatus::HaltedOther {
            reason: format!("{other:?}"),
        },
    }
}

fn hex_lower(bytes: &RevmBytes) -> String {
    let mut s = String::with_capacity(2 + bytes.len() * 2);
    s.push_str("0x");
    for b in bytes.iter() {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
