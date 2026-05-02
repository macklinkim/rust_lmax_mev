//! `JournalError` — unified error type for [`crate::journal::FileJournal`] and
//! [`crate::snapshot::RocksDbSnapshot`].
//!
//! Ships 14 of 15 variants in this commit (Task 1). The 15th variant
//! `RocksDb(rocksdb::Error)` is intentionally deferred to Task 9 alongside
//! the rocksdb dep activation per spec v0.7 amendment + commit `aa5c7c4`
//! staged-deferral; adding it here would require pulling the rocksdb crate
//! into `crates/journal/Cargo.toml` before Task 9, breaking the staged
//! deferral. The placeholder location is marked with a comment in the enum
//! body.
//!
//! No `#[from]` is applied to `BincodeSerialize` or `BincodeDeserialize` per
//! spec §X.4: both wrap `Box<bincode::ErrorKind>` and a blanket `#[from]`
//! would create ambiguity at call sites that need to distinguish the encode
//! vs decode failure direction. Callers use
//! `.map_err(JournalError::BincodeSerialize)` /
//! `.map_err(JournalError::BincodeDeserialize)` explicitly.

use rust_lmax_mev_types::TypesError;

/// Unified error type for the journal and snapshot primitives.
///
/// `#[non_exhaustive]` per spec §5.3 so Phase 2 may add variants additively
/// without breaking downstream pattern matches.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    /// Underlying I/O failure (filesystem read/write/seek/etc.).
    ///
    /// Emitted at open-time (mid-validation), append-time (writes), iter-time
    /// (mid-frame reads), and flush-time. Iter-time emissions are the only
    /// path that increments `event_journal_corrupt_frames_total` per spec
    /// §B.4 row 1.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// `EventEnvelope::validate()` rejected a decoded envelope at
    /// `JournalIter::next` step g per spec §5.4. Increments
    /// `corrupt_frames_total` on the iter path per spec §B.4 row 2.
    #[error("envelope invariant violation: {0}")]
    Types(#[from] TypesError),

    /// rkyv 0.8 serialization or deserialization failure. Emitted by
    /// `FileJournal::append` (encode) and `JournalIter::next` step f (decode)
    /// per spec §B.4 row 4. Iter-side emissions increment
    /// `corrupt_frames_total`; append-side emissions do not.
    #[error("rkyv codec error: {0}")]
    Rkyv(#[from] rkyv::rancor::Error),

    /// bincode 1.x serde-adapter serialization failure. Emitted by
    /// `RocksDbSnapshot::save` (encode V) and `set_last_sequence` (encode
    /// u64) per spec §B.4 row 5.
    ///
    /// Intentionally NOT `#[from]` per spec §X.4: shares
    /// `Box<bincode::ErrorKind>` with `BincodeDeserialize` so a blanket
    /// conversion would lose the encode/decode-direction context.
    #[error("bincode serialize error: {0}")]
    BincodeSerialize(Box<bincode::ErrorKind>),

    /// bincode 1.x serde-adapter deserialization failure. Emitted by
    /// `RocksDbSnapshot::load` (decode V) and `last_sequence` (decode u64)
    /// per spec §B.4 row 6.
    ///
    /// Intentionally NOT `#[from]` per spec §X.4: shares
    /// `Box<bincode::ErrorKind>` with `BincodeSerialize` so a blanket
    /// conversion would lose the encode/decode-direction context.
    #[error("bincode deserialize error: {0}")]
    BincodeDeserialize(Box<bincode::ErrorKind>),

    /// CRC32 mismatch between the stored trailer and the recomputed payload
    /// hash at `JournalIter::next` step e per spec §B.4 row 7.
    /// Iterator fuses on this error.
    #[error("CRC32 mismatch at offset {offset}: expected {expected:#010x}, found {found:#010x}")]
    ChecksumMismatch {
        offset: u64,
        expected: u32,
        found: u32,
    },

    /// Frame length is outside the permitted Phase 1 range
    /// (`1..=MAX_FRAME_LEN`). Emitted by `FileJournal::append` step 2
    /// (pre-write `validate_frame_length`) and `JournalIter::next` step b
    /// (post-length-read) per spec §B.4 row 8. Iter-side emissions increment
    /// `corrupt_frames_total`.
    #[error("invalid frame length at offset {offset}: length={length}, max={max}")]
    InvalidFrameLength { offset: u64, length: u64, max: u64 },

    /// EOF reached mid-frame at one of: length prefix (1-3 bytes seen),
    /// payload body, or CRC trailer. Emitted by `JournalIter::next`
    /// steps a/c/d per spec §B.4 row 9. Iterator fuses. Note: 0-byte EOF at
    /// a record boundary is clean EOF and yields `None` — NOT this variant.
    #[error("truncated frame at offset {offset}: needed {needed}, got {got}")]
    TruncatedFrame { offset: u64, needed: u32, got: u32 },

    /// File-header magic 4-byte block did not match `*b"LMEJ"`. Emitted by
    /// `FileJournal::open` during `read_and_validate_file_header` per spec
    /// §B.4 row 10. Open-time error; no counter increment.
    #[error("invalid file header magic: expected {expected:?}, found {found:?}")]
    InvalidFileHeader { expected: [u8; 4], found: [u8; 4] },

    /// Existing file length is in `1..FILE_HEADER_LEN` (8). Emitted by
    /// `FileJournal::open` per spec §B.4 row 11. Open-time error; no counter
    /// increment.
    #[error("truncated file header: only {} bytes present", found.len())]
    TruncatedFileHeader { found: Vec<u8> },

    /// File-header version byte is not the current `FILE_FORMAT_VERSION` (1).
    /// Emitted by `FileJournal::open` per spec §B.4 row 12. Open-time error;
    /// no counter increment.
    #[error("unsupported file format version: {version}")]
    UnsupportedFileVersion { version: u8 },

    /// File-header reserved bytes are not all-zero `[0, 0, 0]`. Emitted by
    /// `FileJournal::open` per spec §B.4 row 13. Open-time error; no counter
    /// increment. Phase 2 may relax this if reserved bytes get defined
    /// semantics.
    #[error("invalid file header reserved bytes: found {found:?}")]
    InvalidReservedHeader { found: [u8; 3] },

    /// `RocksDbSnapshot::last_sequence` called before any
    /// `set_last_sequence`. Emitted per spec §B.4 row 14. No counter
    /// increment. `0` is NOT used as a sentinel because `0` is a valid
    /// sequence value (Task 11 sequences start at 0, see spec §X.12).
    #[error("snapshot last_sequence not yet set")]
    LastSequenceUnavailable,

    /// User-supplied snapshot key starts with the reserved prefix
    /// `b"\0rust_lmax_mev:snapshot:"`. Emitted by `RocksDbSnapshot::save` and
    /// `load` BEFORE any RocksDB call per spec §B.4 row 15. No counter
    /// increment.
    #[error("reserved snapshot key prefix: {0:?}")]
    ReservedKey(Vec<u8>),
    // RocksDb(rocksdb::Error) variant added in Task 9 alongside the rocksdb
    // dep activation per spec v0.7 amendment + commit aa5c7c4.
}
