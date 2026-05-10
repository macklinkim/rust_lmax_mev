//! Phase 4 P4-C2 T-USDC-1: prove that `USDC.transfer(router → pool)`
//! succeeds against the recorded ZeppelinOS-proxy + FiatTokenV2_2-impl
//! fixtures under `StrictMissingDb`.
//!
//! If this test reports `StrictMissingError::MissingStorage { addr,
//! slot }`, the operator must add `slot` to `usdc_proxy_slots` in
//! `crates/simulator/examples/dump_fixture.rs` and re-record the
//! fixture at the same block hash.
//!
//! This is the load-bearing fixture-completeness proof for the rest
//! of P4-C2. SR-1..3 (real-fixture e2e arb replay) build on the same
//! StrictMissingDb seeding pattern.

use alloy_primitives::{Address, U256};
use revm::db::{CacheDB, EmptyDB};
use revm::primitives::{
    AccountInfo, Bytecode, Bytes as RevmBytes, ExecutionResult, TxKind, KECCAK_EMPTY,
    U256 as RevmU256,
};
use revm::{Database, Evm};
use rust_lmax_mev_simulator::fixtures;
use rust_lmax_mev_simulator::mock_router::MOCK_ROUTER_ADDRESS;
use rust_lmax_mev_simulator::strict_db::{StrictMissingDb, StrictMissingError};
use rust_lmax_mev_state_fetcher::storage_key::{address_key, mapping_slot_u256};

/// FiatTokenV2_2 `balances` mapping declaration slot. Same constant the
/// dump_fixture tool used to record `balanceOf[router/pools]`.
const USDC_BALANCES_SLOT: u64 = 9;

/// USDC `transfer(address,uint256)` selector = `0xa9059cbb`.
const SELECTOR_TRANSFER: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];

/// One USDC = 1e6 raw units (USDC has 6 decimals).
const TRANSFER_AMOUNT_USDC_RAW: u64 = 1_000_000;

/// Pre-fund the router with this much USDC so the transfer has a
/// balance to draw from. The amount is a synthetic test value (NOT
/// drawn from the recorded fixture, which has the router at zero
/// balance at the recording block). We override the recorded
/// `balanceOf[router]` slot with this value before executing.
const ROUTER_PREFUNDED_USDC_RAW: u64 = 10_000_000_000; // 10_000 USDC

#[test]
fn usdc_transfer_router_to_pool_succeeds_against_recorded_fixture_under_strict_missing_db() {
    let mut db = StrictMissingDb::new(CacheDB::new(EmptyDB::default()));

    // 1. Insert USDC proxy account with all recorded slots.
    insert_account_with_storage(
        &mut db,
        addr_from_arr(fixtures::USDC_PROXY_ADDRESS),
        fixtures::USDC_PROXY_CODE,
        fixtures::USDC_PROXY_STORAGE,
    );

    // 2. Insert USDC implementation account (code only — delegatecall
    //    semantics route SLOAD/SSTORE through the proxy's storage).
    insert_account_with_storage(
        &mut db,
        addr_from_arr(fixtures::USDC_IMPL_ADDRESS),
        fixtures::USDC_IMPL_CODE,
        fixtures::USDC_IMPL_STORAGE,
    );

    // 3a. Insert block.coinbase (Address::ZERO) as a code-less EOA so
    //     revm's fee-transfer step doesn't trip MissingAccount.
    db.insert_account(
        Address::ZERO,
        AccountInfo {
            balance: U256::ZERO,
            nonce: 0,
            code_hash: KECCAK_EMPTY,
            code: None,
        },
    );

    // 3b. Insert the router as a CODE-LESS EOA. T-USDC-1 only exercises
    //    plain ERC-20 transfer; the V3-callback bytecode is not invoked
    //    here. Using a code-bearing account would trigger revm's
    //    EIP-3607 `RejectCallerWithCode` check on the outer tx caller.
    //    SR-1..3 (V3 swap path) will use the bytecode-bearing router as
    //    `msg.sender` to the pool via an EOA → router → pool sequence.
    let router = MOCK_ROUTER_ADDRESS;
    db.insert_account(
        router,
        AccountInfo {
            balance: RevmU256::from(1_000_000_000_000_000_000u128), // 1 ETH for gas
            nonce: 0,
            code_hash: KECCAK_EMPTY,
            code: None,
        },
    );

    // 4. Override the router's USDC balance slot with the pre-loaded
    //    amount (the recorded value is zero at the recording block).
    let usdc_proxy_addr = addr_from_arr(fixtures::USDC_PROXY_ADDRESS);
    let router_balance_slot =
        mapping_slot_u256(U256::from(USDC_BALANCES_SLOT), address_key(router));
    db.insert_storage(
        usdc_proxy_addr,
        router_balance_slot,
        U256::from(ROUTER_PREFUNDED_USDC_RAW),
    )
    .expect("seed router USDC balance");

    // 5. Pick the V2 pool as transfer recipient (its balanceOf slot is
    //    in the recorded proxy fixture).
    let recipient = addr_from_arr(fixtures::V2_WETH_USDC_ADDRESS);
    let recipient_balance_slot =
        mapping_slot_u256(U256::from(USDC_BALANCES_SLOT), address_key(recipient));

    // Snapshot recipient's pre-balance through the strict DB (this read
    // also acts as a sanity check that the slot is populated).
    let pre_recipient = db
        .storage(usdc_proxy_addr, recipient_balance_slot)
        .expect("recipient USDC balance slot must be populated by recorded fixture");

    // 6. Build transfer(recipient, amount) calldata.
    let mut calldata = Vec::with_capacity(4 + 32 + 32);
    calldata.extend_from_slice(&SELECTOR_TRANSFER);
    // recipient as 32-byte left-padded
    calldata.extend_from_slice(&[0u8; 12]);
    calldata.extend_from_slice(recipient.as_slice());
    // amount as uint256 BE
    let amount = U256::from(TRANSFER_AMOUNT_USDC_RAW);
    calldata.extend_from_slice(&amount.to_be_bytes::<32>());

    // 7. Execute via revm.
    let mut evm = Evm::builder()
        .with_db(db)
        .modify_cfg_env(|c| {
            c.chain_id = 1;
        })
        .modify_block_env(|b| {
            b.basefee = RevmU256::from(30_000_000_000u128);
            b.gas_limit = RevmU256::from(30_000_000u64);
            // Use a recent block number; not asserted on by USDC.
            b.number = RevmU256::from(20_000_000u64);
        })
        .modify_tx_env(|tx| {
            tx.caller = router;
            tx.transact_to = TxKind::Call(usdc_proxy_addr);
            tx.gas_limit = 5_000_000;
            tx.gas_price = RevmU256::from(30_000_000_000u128);
            tx.value = RevmU256::ZERO;
            tx.data = RevmBytes::from(calldata);
            tx.chain_id = Some(1);
            tx.nonce = Some(0);
        })
        .build();

    let exec_result = evm.transact_commit();

    // 8. Surface MissingStorage / MissingAccount details up front so
    //    the operator knows exactly which slot to add to dump_fixture.
    let result = match exec_result {
        Ok(r) => r,
        Err(e) => {
            // Pretty-format StrictMissingError details if present.
            let detail = format!("{e:?}");
            if detail.contains("MissingStorage") || detail.contains("MissingAccount") {
                panic!(
                    "T-USDC-1 failed under StrictMissingDb (fixture incomplete):\n  {detail}\n\n\
                     Action: add the missing slot to `usdc_proxy_slots` in \
                     crates/simulator/examples/dump_fixture.rs and re-record at the \
                     same block hash.",
                );
            }
            panic!("T-USDC-1 unexpected revm error: {detail}");
        }
    };

    match &result {
        ExecutionResult::Success { .. } => {}
        ExecutionResult::Revert { gas_used, output } => panic!(
            "T-USDC-1 USDC.transfer reverted (gas_used={gas_used}, output={output:?}); \
             this likely means the StrictMissingDb seeding is incomplete or the recorded \
             modifier preconditions (paused / blacklisted / etc.) reject the transfer.",
        ),
        ExecutionResult::Halt { reason, gas_used } => {
            panic!("T-USDC-1 USDC.transfer halted (reason={reason:?}, gas_used={gas_used})",)
        }
    }

    // 9. Verify recipient balance increased by the transfer amount.
    let db_after = &mut evm.context.evm.db;
    let post_recipient = db_after
        .storage(usdc_proxy_addr, recipient_balance_slot)
        .expect("post-tx recipient balance read");
    let pre_recipient_u256 = pre_recipient;
    let post_recipient_u256 = post_recipient;
    assert_eq!(
        post_recipient_u256,
        pre_recipient_u256 + U256::from(TRANSFER_AMOUNT_USDC_RAW),
        "recipient USDC balance must increase by the transfer amount"
    );

    let post_router = db_after
        .storage(usdc_proxy_addr, router_balance_slot)
        .expect("post-tx router balance read");
    assert_eq!(
        post_router,
        U256::from(ROUTER_PREFUNDED_USDC_RAW - TRANSFER_AMOUNT_USDC_RAW),
        "router USDC balance must decrease by the transfer amount"
    );
}

fn addr_from_arr(arr: [u8; 20]) -> Address {
    Address::from(arr)
}

fn insert_account_with_storage(
    db: &mut StrictMissingDb,
    address: Address,
    code: &[u8],
    storage: &[([u8; 32], [u8; 32])],
) {
    let bytecode = if code.is_empty() {
        None
    } else {
        Some(Bytecode::new_raw(RevmBytes::copy_from_slice(code)))
    };
    let (code_hash, nonce) = match &bytecode {
        Some(b) => (b.hash_slow(), 1),
        None => (KECCAK_EMPTY, 0),
    };
    db.insert_account(
        address,
        AccountInfo {
            balance: U256::ZERO,
            nonce,
            code_hash,
            code: bytecode,
        },
    );
    for (slot_be, value_be) in storage {
        let slot = U256::from_be_bytes(*slot_be);
        let value = U256::from_be_bytes(*value_be);
        if let Err(e) = db.insert_storage(address, slot, value) {
            panic!("insert recorded storage at {address:?}[{slot}] = {value}: {e:?}");
        }
    }
}

#[allow(dead_code)]
fn _silence_unused_warning(_e: StrictMissingError) {}
