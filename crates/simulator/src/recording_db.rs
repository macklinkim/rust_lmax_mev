//! Phase 4 P4-D `RecordingDb<DB>` wrapper for capturing the
//! storage-read set during one `LocalSimulator::simulate_with_fingerprint`
//! call.
//!
//! Per the P4-D execution note v0.4 §R8 + §DP-D9' + §DP-D15:
//! - Wraps any `revm::Database` and records every successful
//!   `Database::storage(account, slot) -> Ok(value)` call as a
//!   `StateObservation`.
//! - Failed reads (`Err(_)` from the inner DB) are NOT recorded —
//!   they propagate as `SimulationError` via the existing path.
//! - `basic`, `code_by_hash`, `block_hash` are pass-through; only
//!   storage reads matter for ADR-006 `StateDependency` detection.
//! - Implements `DatabaseCommit` via straight inner-delegation so
//!   the existing `evm.transact_commit()` call sites compile when
//!   the inner CacheDB is wrapped (R12 fix).
//! - `commit()` is NOT recorded; the comparator only needs READS.
//!
//! `RecordingDb` lives entirely on the call's stack frame —
//! `LocalSimulator` does NOT gain interior state.

use crate::observation::StateObservation;
use std::cell::RefCell;

use alloy_primitives::{Address, B256, U256};
use revm::primitives::{Account, AccountInfo, Bytecode, HashMap};
use revm::{Database, DatabaseCommit};

/// Database wrapper that records every successful `Database::storage`
/// call. Use `into_observations()` after the simulation pass to
/// extract the captured read-set.
pub struct RecordingDb<DB> {
    inner: DB,
    reads: RefCell<Vec<StateObservation>>,
}

impl<DB> RecordingDb<DB> {
    /// Wrap an existing DB. The recording buffer starts empty.
    pub fn new(inner: DB) -> Self {
        Self {
            inner,
            reads: RefCell::new(Vec::new()),
        }
    }

    /// Borrow the recorded reads without consuming the wrapper.
    /// Useful for assertions during a session that must continue
    /// after the borrow.
    pub fn observations(&self) -> Vec<StateObservation> {
        self.reads.borrow().clone()
    }

    /// Consume the wrapper and return the recorded reads.
    pub fn into_observations(self) -> Vec<StateObservation> {
        self.reads.into_inner()
    }

    /// Borrow the wrapped inner DB.
    pub fn inner(&self) -> &DB {
        &self.inner
    }

    /// Mutably borrow the wrapped inner DB. Useful for callers that
    /// need to seed inner state before recording (e.g., tests).
    pub fn inner_mut(&mut self) -> &mut DB {
        &mut self.inner
    }
}

impl<DB: Database> Database for RecordingDb<DB> {
    type Error = DB::Error;

    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        self.inner.basic(address)
    }

    fn code_by_hash(&mut self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        self.inner.code_by_hash(code_hash)
    }

    fn storage(&mut self, address: Address, index: U256) -> Result<U256, Self::Error> {
        let value_u = self.inner.storage(address, index)?;
        let value_b = B256::from(value_u.to_be_bytes());
        self.reads.borrow_mut().push(StateObservation {
            account: address,
            slot: index,
            value: value_b,
        });
        Ok(value_u)
    }

    fn block_hash(&mut self, number: u64) -> Result<B256, Self::Error> {
        self.inner.block_hash(number)
    }
}

// R12 fix: DatabaseCommit delegation so transact_commit() works when
// the inner DB is wrapped. commit() itself is NOT recorded —
// comparator only needs storage *reads* for StateDependency detection.
impl<DB: DatabaseCommit> DatabaseCommit for RecordingDb<DB> {
    fn commit(&mut self, changes: HashMap<Address, Account>) {
        self.inner.commit(changes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strict_db::{StrictMissingDb, StrictMissingError};
    use revm::db::{CacheDB, EmptyDB};
    use revm::primitives::{AccountInfo, KECCAK_EMPTY};

    fn cache_db_with_one_slot(addr: Address, slot: U256, value: U256) -> CacheDB<EmptyDB> {
        let mut db = CacheDB::new(EmptyDB::default());
        db.insert_account_info(
            addr,
            AccountInfo {
                balance: U256::ZERO,
                nonce: 0,
                code_hash: KECCAK_EMPTY,
                code: None,
            },
        );
        db.insert_account_storage(addr, slot, value)
            .expect("seed storage");
        db
    }

    /// RDB-1: `storage` reads are recorded; `basic` / `code_by_hash` /
    /// `block_hash` are NOT recorded (the read-set the comparator
    /// needs only includes storage observations per ADR-006
    /// StateDependency).
    #[test]
    fn rdb_1_only_storage_reads_recorded() {
        let addr = Address::from([0x11u8; 20]);
        let slot = U256::from(0x42u64);
        let value = U256::from(0xCAFEu64);
        let db = cache_db_with_one_slot(addr, slot, value);
        let mut rec = RecordingDb::new(db);

        // basic + code_by_hash + block_hash are pass-through.
        let _ = rec.basic(addr).expect("basic ok");
        let _ = rec.code_by_hash(KECCAK_EMPTY).expect("code ok");
        let _ = rec.block_hash(0).expect("block_hash ok");
        assert!(
            rec.observations().is_empty(),
            "non-storage calls must not be recorded"
        );

        // storage IS recorded.
        let v = rec.storage(addr, slot).expect("storage ok");
        assert_eq!(v, value);
        let obs = rec.observations();
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].account, addr);
        assert_eq!(obs[0].slot, slot);
        assert_eq!(obs[0].value, B256::from(value.to_be_bytes()));
    }

    /// RDB-2: failed `storage` reads are NOT recorded (the error
    /// propagates as the inner DB's error type; nothing is added to
    /// the recorded set so the comparator does not see phantom slots).
    #[test]
    fn rdb_2_failed_read_not_recorded() {
        let inner = StrictMissingDb::new(CacheDB::new(EmptyDB::default()));
        let mut rec = RecordingDb::new(inner);
        let addr = Address::from([0x22u8; 20]);
        let slot = U256::from(0x99u64);
        let err = rec
            .storage(addr, slot)
            .expect_err("StrictMissingDb must reject unpopulated storage");
        assert!(matches!(err, StrictMissingError::MissingStorage { .. }));
        assert!(rec.observations().is_empty());
    }

    /// RDB-3: multiple reads are recorded in call order.
    #[test]
    fn rdb_3_multiple_reads_in_order() {
        let addr_a = Address::from([0x10u8; 20]);
        let addr_b = Address::from([0x20u8; 20]);
        let mut db = cache_db_with_one_slot(addr_a, U256::from(1u64), U256::from(0x1111u64));
        // Add a second slot on addr_a + first slot on addr_b.
        db.insert_account_info(
            addr_b,
            AccountInfo {
                balance: U256::ZERO,
                nonce: 0,
                code_hash: KECCAK_EMPTY,
                code: None,
            },
        );
        db.insert_account_storage(addr_a, U256::from(2u64), U256::from(0x2222u64))
            .unwrap();
        db.insert_account_storage(addr_b, U256::from(7u64), U256::from(0x7777u64))
            .unwrap();

        let mut rec = RecordingDb::new(db);
        let _ = rec.storage(addr_a, U256::from(1u64)).unwrap();
        let _ = rec.storage(addr_b, U256::from(7u64)).unwrap();
        let _ = rec.storage(addr_a, U256::from(2u64)).unwrap();

        let obs = rec.observations();
        assert_eq!(obs.len(), 3);
        assert_eq!(obs[0].account, addr_a);
        assert_eq!(obs[0].slot, U256::from(1u64));
        assert_eq!(obs[1].account, addr_b);
        assert_eq!(obs[1].slot, U256::from(7u64));
        assert_eq!(obs[2].account, addr_a);
        assert_eq!(obs[2].slot, U256::from(2u64));
    }

    /// RDB-4 (R12): `commit()` delegates to the inner DB —
    /// read-after-commit returns the committed value.
    #[test]
    fn rdb_4_commit_delegates_to_inner() {
        let addr = Address::from([0x33u8; 20]);
        let slot = U256::from(0x55u64);
        let initial = U256::from(0xA0A0u64);
        let updated = U256::from(0xB1B1u64);

        let db = cache_db_with_one_slot(addr, slot, initial);
        let mut rec = RecordingDb::new(db);

        // Pre-commit read returns the initial value.
        assert_eq!(rec.storage(addr, slot).unwrap(), initial);

        // Construct a minimal commit changeset that updates slot.
        let mut account = Account {
            info: AccountInfo {
                balance: U256::ZERO,
                nonce: 1,
                code_hash: KECCAK_EMPTY,
                code: None,
            },
            storage: HashMap::default(),
            status: revm::primitives::AccountStatus::Touched,
        };
        account.storage.insert(
            slot,
            revm::primitives::EvmStorageSlot::new_changed(initial, updated),
        );
        let mut changes: HashMap<Address, Account> = HashMap::default();
        changes.insert(addr, account);
        rec.commit(changes);

        // Post-commit read returns the updated value (proves commit
        // reached the inner DB). The recording wrapper records this
        // new read; the prior pre-commit value is also still in the
        // observation list.
        let pre_obs_count = rec.observations().len();
        assert_eq!(rec.storage(addr, slot).unwrap(), updated);
        assert_eq!(rec.observations().len(), pre_obs_count + 1);
    }
}
