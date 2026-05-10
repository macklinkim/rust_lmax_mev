//! Shared fixture builders for `crates/simulator/tests/`. Mirrors the
//! pattern in `crates/app/tests/common/mod.rs`. Each integration test
//! file is its own crate; helpers used by a subset surface as
//! dead-code warnings without `#[allow(dead_code)]`.

#![allow(dead_code)]

use alloy_primitives::{Address, Bytes, B256, U256};
use rust_lmax_mev_simulator::fixtures;
use rust_lmax_mev_state::{PoolId, PoolKind};
use rust_lmax_mev_state_fetcher::{FetchedAccount, FetchedPoolState};

pub fn v2_fixture() -> FetchedPoolState {
    FetchedPoolState {
        pool: PoolId {
            kind: PoolKind::UniswapV2,
            address: Address::from(fixtures::V2_WETH_USDC_ADDRESS),
        },
        block_hash: B256::from(fixtures::V2_WETH_USDC_BLOCK_HASH),
        pool_code: Bytes::copy_from_slice(fixtures::V2_WETH_USDC_CODE),
        pool_storage: storage_from_pool(fixtures::V2_WETH_USDC_STORAGE),
        auxiliary: Vec::new(),
    }
}

pub fn v3_fixture() -> FetchedPoolState {
    FetchedPoolState {
        pool: PoolId {
            kind: PoolKind::UniswapV3Fee005,
            address: Address::from(fixtures::V3_WETH_USDC_005_ADDRESS),
        },
        block_hash: B256::from(fixtures::V3_WETH_USDC_005_BLOCK_HASH),
        pool_code: Bytes::copy_from_slice(fixtures::V3_WETH_USDC_005_CODE),
        pool_storage: storage_from_pool(fixtures::V3_WETH_USDC_005_STORAGE),
        auxiliary: Vec::new(),
    }
}

pub fn weth_fixture() -> FetchedAccount {
    FetchedAccount {
        address: Address::from(fixtures::WETH9_ADDRESS),
        block_hash: B256::from(fixtures::WETH9_BLOCK_HASH),
        code: Bytes::copy_from_slice(fixtures::WETH9_CODE),
        storage: storage_from_account(fixtures::WETH9_STORAGE),
    }
}

pub fn usdc_proxy_fixture() -> FetchedAccount {
    FetchedAccount {
        address: Address::from(fixtures::USDC_PROXY_ADDRESS),
        block_hash: B256::from(fixtures::USDC_PROXY_BLOCK_HASH),
        code: Bytes::copy_from_slice(fixtures::USDC_PROXY_CODE),
        storage: storage_from_account(fixtures::USDC_PROXY_STORAGE),
    }
}

pub fn usdc_impl_fixture() -> FetchedAccount {
    FetchedAccount {
        address: Address::from(fixtures::USDC_IMPL_ADDRESS),
        block_hash: B256::from(fixtures::USDC_IMPL_BLOCK_HASH),
        code: Bytes::copy_from_slice(fixtures::USDC_IMPL_CODE),
        storage: storage_from_account(fixtures::USDC_IMPL_STORAGE),
    }
}

pub fn v2_factory_fixture() -> FetchedAccount {
    FetchedAccount {
        address: Address::from(fixtures::V2_FACTORY_ADDRESS),
        block_hash: B256::from(fixtures::V2_FACTORY_BLOCK_HASH),
        code: Bytes::copy_from_slice(fixtures::V2_FACTORY_CODE),
        storage: storage_from_account(fixtures::V2_FACTORY_STORAGE),
    }
}

pub fn storage_from_pool(s: &[([u8; 32], [u8; 32])]) -> Vec<(U256, B256)> {
    s.iter()
        .map(|(k, v)| (U256::from_be_bytes(*k), B256::from(*v)))
        .collect()
}

pub fn storage_from_account(s: &[([u8; 32], [u8; 32])]) -> Vec<(U256, B256)> {
    s.iter()
        .map(|(k, v)| (U256::from_be_bytes(*k), B256::from(*v)))
        .collect()
}
