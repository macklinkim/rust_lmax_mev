//! Phase 4 P4-C `RevmDbBuilder` — translates `FetchedPoolState`s +
//! `FetchedAccount`s + mock-router auxiliary state into a
//! `StrictMissingDb`-wrapped `CacheDB<EmptyDB>` ready for two
//! sequential swap `transact_commit` calls. Per the user-approved
//! v0.3 execution note DP-C4 (two-pool shared CacheDB) + DP-C9a (USDC
//! proxy + impl as separate `FetchedAccount`s).

use alloy_primitives::{Bytes, U256};
use revm::primitives::{AccountInfo, Address as RevmAddress, Bytecode, Bytes as RevmBytes};
use rust_lmax_mev_state_fetcher::{FetchedAccount, FetchedPoolState};

use crate::strict_db::StrictMissingDb;
use crate::SimulationError;

/// Mock-router + EOA + token-balance metadata that `RevmDbBuilder`
/// inserts alongside the fetched pool/account states.
#[derive(Debug, Clone)]
pub struct AuxiliaryAccounts {
    pub mock_router_address: RevmAddress,
    pub mock_router_bytecode: Bytes,
    pub mock_router_eth_balance_wei: U256,
    /// Pre-funded WETH balance for the router. The router uses this
    /// in swap-1 (sells WETH at the expensive sink pool).
    pub mock_router_weth_balance_wei: U256,
}

/// Result of `build_prepared`: the strict DB plus a snapshot of the
/// router's WETH balance pre-execution. The simulator measures the
/// post-execution router WETH delta against this baseline.
#[derive(Debug)]
pub struct PreparedSimulation {
    pub db: StrictMissingDb,
    pub pre_router_weth_wei: U256,
}

/// Insert two pool states + a list of `FetchedAccount`s (typically
/// WETH + USDC proxy + USDC impl) + the mock router into a fresh
/// `StrictMissingDb`. Source/sink must be at the same block_hash;
/// any block_hash inconsistency surfaces as `SimulationError::Setup`.
///
/// `weth_balance_slot_for_router` is the storage slot in the WETH
/// account where the router's WETH balance lives — typically
/// `mapping_slot_u256(WETH_BALANCES_SLOT=3, address_key(router))`.
/// The caller pre-computes this and seeds it into the WETH
/// FetchedAccount's storage with the value
/// `mock_router_weth_balance_wei`.
pub fn build_prepared(
    source_pool: &FetchedPoolState,
    sink_pool: &FetchedPoolState,
    extra_accounts: &[FetchedAccount],
    aux: &AuxiliaryAccounts,
    weth_address: RevmAddress,
    weth_router_balance_slot: U256,
) -> Result<PreparedSimulation, SimulationError> {
    if source_pool.block_hash != sink_pool.block_hash {
        return Err(SimulationError::Setup(format!(
            "source/sink block_hash mismatch: source={:?} sink={:?}",
            source_pool.block_hash, sink_pool.block_hash
        )));
    }
    for acc in extra_accounts {
        if acc.block_hash != source_pool.block_hash {
            return Err(SimulationError::Setup(format!(
                "auxiliary account {:?} block_hash mismatch: account={:?} pools={:?}",
                acc.address, acc.block_hash, source_pool.block_hash
            )));
        }
    }

    let mut db = StrictMissingDb::default();

    insert_pool(&mut db, source_pool)?;
    if sink_pool.pool.address != source_pool.pool.address {
        insert_pool(&mut db, sink_pool)?;
    }
    for acc in extra_accounts {
        insert_account_full(&mut db, acc)?;
    }

    // Mock router account.
    let router_code_bytes = RevmBytes::copy_from_slice(&aux.mock_router_bytecode);
    let router_bytecode = Bytecode::new_raw(router_code_bytes);
    db.insert_account(
        aux.mock_router_address,
        AccountInfo {
            balance: aux.mock_router_eth_balance_wei,
            nonce: 0,
            code_hash: router_bytecode.hash_slow(),
            code: Some(router_bytecode),
        },
    );

    // Snapshot router's WETH balance (must already be populated via
    // extra_accounts — verified by reading through StrictMissingDb,
    // which raises MissingStorage if the caller forgot).
    use revm::Database;
    let pre = db
        .storage(weth_address, weth_router_balance_slot)
        .map_err(|e| {
            SimulationError::Setup(format!(
                "router's WETH balance slot {weth_router_balance_slot} not populated on WETH account {weth_address:?}: {e:?}"
            ))
        })?;

    // Sanity: snapshot must equal the configured pre-funding amount.
    if pre != aux.mock_router_weth_balance_wei {
        return Err(SimulationError::Setup(format!(
            "router pre-WETH snapshot {pre} != configured aux.mock_router_weth_balance_wei {}",
            aux.mock_router_weth_balance_wei,
        )));
    }

    Ok(PreparedSimulation {
        db,
        pre_router_weth_wei: pre,
    })
}

fn insert_pool(db: &mut StrictMissingDb, pool: &FetchedPoolState) -> Result<(), SimulationError> {
    let code_bytes = RevmBytes::copy_from_slice(&pool.pool_code);
    let bytecode = Bytecode::new_raw(code_bytes);
    db.insert_account(
        pool.pool.address,
        AccountInfo {
            balance: U256::ZERO,
            nonce: 1,
            code_hash: bytecode.hash_slow(),
            code: Some(bytecode),
        },
    );
    for (slot, value) in &pool.pool_storage {
        db.insert_storage(pool.pool.address, *slot, U256::from_be_bytes(value.0))
            .map_err(|e| SimulationError::Setup(format!("insert pool storage: {e:?}")))?;
    }
    Ok(())
}

fn insert_account_full(
    db: &mut StrictMissingDb,
    acc: &FetchedAccount,
) -> Result<(), SimulationError> {
    let code_bytes = RevmBytes::copy_from_slice(&acc.code);
    let nonce = if acc.code.is_empty() { 0 } else { 1 };
    let bytecode = Bytecode::new_raw(code_bytes);
    db.insert_account(
        acc.address,
        AccountInfo {
            balance: U256::ZERO,
            nonce,
            code_hash: bytecode.hash_slow(),
            code: Some(bytecode),
        },
    );
    for (slot, value) in &acc.storage {
        db.insert_storage(acc.address, *slot, U256::from_be_bytes(value.0))
            .map_err(|e| SimulationError::Setup(format!("insert account storage: {e:?}")))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::B256;
    use rust_lmax_mev_state::{PoolId, PoolKind};

    fn synthetic_pool(addr: [u8; 20], block: B256, slot_base: u64) -> FetchedPoolState {
        FetchedPoolState {
            pool: PoolId {
                kind: PoolKind::UniswapV2,
                address: alloy_primitives::Address::from(addr),
            },
            block_hash: block,
            pool_code: Bytes::from_static(&[0x60, 0x00, 0x00]),
            pool_storage: vec![
                (U256::from(slot_base), B256::from([0x11; 32])),
                (U256::from(slot_base + 1), B256::from([0x22; 32])),
            ],
            auxiliary: Vec::new(),
        }
    }

    /// DB-1: build_prepared inserts BOTH pools' code+storage into one
    /// shared StrictMissingDb (per Codex Rev #2). Verified by reading
    /// each pool's storage through the strict DB without
    /// MissingStorage errors.
    #[test]
    fn build_prepared_inserts_both_pools_into_one_strict_missing_db() {
        let block = B256::from([0xab; 32]);
        let source = synthetic_pool([0x11; 20], block, 0);
        let sink = synthetic_pool([0x22; 20], block, 100);

        let weth_addr = RevmAddress::from([0xc0; 20]);
        let weth_balance_slot = U256::from(99u64);
        let router_addr = RevmAddress::from([0x33; 20]);
        let router_weth_balance = U256::from(1_000_000u64);

        let weth_account = FetchedAccount {
            address: weth_addr,
            block_hash: block,
            code: Bytes::from_static(&[0x60, 0x01]),
            // Pre-seed the router's WETH balance slot.
            storage: vec![(
                weth_balance_slot,
                B256::from(router_weth_balance.to_be_bytes::<32>()),
            )],
        };

        let aux = AuxiliaryAccounts {
            mock_router_address: router_addr,
            mock_router_bytecode: Bytes::from_static(&[0x00]),
            mock_router_eth_balance_wei: U256::from(1_000_000_000u64),
            mock_router_weth_balance_wei: router_weth_balance,
        };

        let prepared = build_prepared(
            &source,
            &sink,
            &[weth_account],
            &aux,
            weth_addr,
            weth_balance_slot,
        )
        .expect("build_prepared OK");

        assert_eq!(prepared.pre_router_weth_wei, router_weth_balance);
        let mut db = prepared.db;

        // Both pools' storage must be readable through StrictMissingDb.
        use revm::Database;
        for pool in [&source, &sink] {
            for (slot, value) in &pool.pool_storage {
                let got = db
                    .storage(pool.pool.address, *slot)
                    .unwrap_or_else(|e| panic!("missing storage for pool: {e:?}"));
                assert_eq!(got, U256::from_be_bytes(value.0));
            }
        }
        // Mismatched block_hash must error.
        let bad_sink = synthetic_pool([0x22; 20], B256::from([0xcd; 32]), 100);
        let err = build_prepared(&source, &bad_sink, &[], &aux, weth_addr, weth_balance_slot)
            .expect_err("must reject mismatched block_hash");
        assert!(matches!(err, SimulationError::Setup(_)));
    }
}
