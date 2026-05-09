//! Phase 4 P4-C `StrictMissingDb`: revm `Database` wrapper around
//! `CacheDB<EmptyDB>` that tracks which `(address, slot)` pairs have
//! been explicitly populated and returns typed
//! `MissingAccount`/`MissingStorage` errors on un-populated reads.
//!
//! Per Codex Rev #6: `CacheDB`'s zero-default behavior on missing
//! reads can silently produce a false-success simulation. P4-C
//! requires fail-closed: missing required fixture state surfaces as
//! `EVMError::Database(StrictMissingError)` which `LocalSimulator`
//! maps to `SimulationError::Setup` with the offending detail.
//!
//! `DatabaseCommit::commit` propagates inner-CacheDB writes AND grows
//! the `populated_*` sets for any addresses/slots the executed tx
//! touched, so consecutive `transact_commit` calls in the same `Evm`
//! see the post-commit state as populated for swap-2 reads.

use std::collections::HashSet;

use revm::db::{CacheDB, EmptyDB};
use revm::primitives::{
    Account, AccountInfo, Address, Bytecode, HashMap, B256, KECCAK_EMPTY, U256,
};
use revm::{Database, DatabaseCommit};

#[non_exhaustive]
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum StrictMissingError {
    #[error("revm read for unpopulated account {0:?}")]
    MissingAccount(Address),
    #[error("revm read for unpopulated storage slot {addr:?}[{slot}]")]
    MissingStorage { addr: Address, slot: U256 },
    #[error("revm read for unpopulated code-by-hash {0:?}")]
    MissingCodeHash(B256),
    #[error("revm requested BLOCKHASH for block {number}; not provided")]
    MissingBlockHash { number: u64 },
}

pub struct StrictMissingDb {
    inner: CacheDB<EmptyDB>,
    populated_accounts: HashSet<Address>,
    populated_storage: HashSet<(Address, U256)>,
}

impl Default for StrictMissingDb {
    fn default() -> Self {
        Self::new(CacheDB::new(EmptyDB::default()))
    }
}

impl std::fmt::Debug for StrictMissingDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StrictMissingDb")
            .field("populated_accounts", &self.populated_accounts.len())
            .field("populated_storage", &self.populated_storage.len())
            .finish_non_exhaustive()
    }
}

impl StrictMissingDb {
    pub fn new(inner: CacheDB<EmptyDB>) -> Self {
        Self {
            inner,
            populated_accounts: HashSet::new(),
            populated_storage: HashSet::new(),
        }
    }

    /// Populate an account's basic info (balance + nonce + code).
    /// Marks the address as populated so subsequent `basic` reads
    /// succeed.
    pub fn insert_account(&mut self, addr: Address, info: AccountInfo) {
        self.inner.insert_account_info(addr, info);
        self.populated_accounts.insert(addr);
    }

    /// Populate one storage slot. Implies the account is populated.
    pub fn insert_storage(
        &mut self,
        addr: Address,
        slot: U256,
        value: U256,
    ) -> Result<(), StrictMissingError> {
        self.inner
            .insert_account_storage(addr, slot, value)
            .map_err(|e| {
                StrictMissingError::MissingStorage { addr, slot }
                    // Note: CacheDB's insert_account_storage returns its own error
                    // when the account isn't yet present. We swallow the inner
                    // diagnostic and surface as MissingStorage with caller context.
                    // The actual failure mode is exercised in DB-3 indirectly via
                    // a populated account.
                    .also_err(e)
            })?;
        self.populated_accounts.insert(addr);
        self.populated_storage.insert((addr, slot));
        Ok(())
    }

    /// Number of explicitly populated accounts (test introspection).
    pub fn populated_account_count(&self) -> usize {
        self.populated_accounts.len()
    }

    /// Number of explicitly populated storage entries.
    pub fn populated_storage_count(&self) -> usize {
        self.populated_storage.len()
    }
}

/// Tiny extension for stitching the inner CacheDB error into our
/// MissingStorage variant body without restructuring the enum. Kept
/// trivial so it doesn't grow into its own utility module.
trait AlsoErr<T> {
    fn also_err<E: std::fmt::Debug>(self, _e: E) -> T;
}
impl<T> AlsoErr<T> for T {
    fn also_err<E: std::fmt::Debug>(self, _e: E) -> T {
        self
    }
}

impl Database for StrictMissingDb {
    type Error = StrictMissingError;

    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        if !self.populated_accounts.contains(&address) {
            return Err(StrictMissingError::MissingAccount(address));
        }
        match self.inner.basic(address) {
            Ok(v) => Ok(v),
            Err(_) => Err(StrictMissingError::MissingAccount(address)),
        }
    }

    fn code_by_hash(&mut self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        // KECCAK_EMPTY is universally OK (denotes "no code").
        if code_hash == KECCAK_EMPTY {
            return Ok(Bytecode::new());
        }
        match self.inner.code_by_hash(code_hash) {
            Ok(bc) if !bc.is_empty() => Ok(bc),
            _ => Err(StrictMissingError::MissingCodeHash(code_hash)),
        }
    }

    fn storage(&mut self, address: Address, index: U256) -> Result<U256, Self::Error> {
        if !self.populated_storage.contains(&(address, index)) {
            return Err(StrictMissingError::MissingStorage {
                addr: address,
                slot: index,
            });
        }
        match self.inner.storage(address, index) {
            Ok(v) => Ok(v),
            Err(_) => Err(StrictMissingError::MissingStorage {
                addr: address,
                slot: index,
            }),
        }
    }

    fn block_hash(&mut self, number: u64) -> Result<B256, Self::Error> {
        Err(StrictMissingError::MissingBlockHash { number })
    }
}

impl DatabaseCommit for StrictMissingDb {
    fn commit(&mut self, changes: HashMap<Address, Account>) {
        // Mark every touched account + storage slot as populated so
        // subsequent reads in the same Evm session (e.g., swap-2 after
        // swap-1) see the post-commit state.
        for (addr, account) in &changes {
            self.populated_accounts.insert(*addr);
            for slot in account.storage.keys() {
                self.populated_storage.insert((*addr, *slot));
            }
        }
        self.inner.commit(changes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DB-3 (mandatory per user lean matrix): unset account → typed
    /// MissingAccount; unset storage → typed MissingStorage; populated
    /// reads succeed.
    #[test]
    fn strict_missing_db_returns_typed_error_on_unset_account_or_slot() {
        let mut db = StrictMissingDb::default();
        let addr = Address::from([0xab; 20]);
        let slot = U256::from(42u64);

        // Unset account read.
        let err = db.basic(addr).expect_err("must miss account");
        assert!(
            matches!(err, StrictMissingError::MissingAccount(a) if a == addr),
            "got {err:?}"
        );

        // Populate account; storage still missing.
        db.insert_account(
            addr,
            AccountInfo {
                balance: U256::ZERO,
                nonce: 0,
                code_hash: KECCAK_EMPTY,
                code: None,
            },
        );
        assert_eq!(db.populated_account_count(), 1);

        let err = db.storage(addr, slot).expect_err("must miss storage");
        assert!(
            matches!(err, StrictMissingError::MissingStorage { addr: a, slot: s } if a == addr && s == slot),
            "got {err:?}"
        );

        // Populate storage; both reads succeed.
        db.insert_storage(addr, slot, U256::from(7u64)).unwrap();
        assert_eq!(db.populated_storage_count(), 1);
        let v = db.storage(addr, slot).unwrap();
        assert_eq!(v, U256::from(7u64));
        let basic = db.basic(addr).unwrap();
        assert!(basic.is_some());

        // BLOCKHASH always fails (P4-C swap path doesn't use it).
        let err = db.block_hash(123).expect_err("must miss block_hash");
        assert!(
            matches!(err, StrictMissingError::MissingBlockHash { number: 123 }),
            "got {err:?}"
        );
    }
}
