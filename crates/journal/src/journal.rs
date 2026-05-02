//! `FileJournal<T>` — append-only file-backed log primitive.
//!
//! Per spec §5.1 (public API surface), §5.6 (open-time vs iter-time counter
//! semantics), §B.2.1 (open decision tree), §B.3 (counter mirrors),
//! §B.6 (rkyv 0.8 trait bounds — finalized in Task 4 when `append` lands),
//! §X.10 (flush is buffer→OS only; no durability).
//!
//! Task 3 lands `FileJournal::open` (6-case decision tree), `JournalStats`
//! (4 atomic mirrors), and a stub `flush` (the contract test J-17 lands in
//! Task 8). `append`, `iter_all`, `JournalIter`, and the rkyv 0.8 `T` bounds
//! arrive in Tasks 4-8.

// Counter atomics, byte_offset, and path are populated by `open` in this task
// but not yet read until `append` / `iter_all` / `stats()`-consumers land in
// Tasks 4-8. The module-level annotation matches the plan v0.3 dead-code
// policy (annotate the module, not individual items) and is removed in Task
// 12 once every field has at least one non-test reader.
#![allow(dead_code)]

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::JournalError;
use crate::frame::{read_and_validate_file_header, write_file_header, FILE_HEADER_LEN};

/// Append-only file-backed journal of `EventEnvelope<T>` records.
///
/// Per spec §5.1: `append` / `flush` take `&mut self` (the file handle is the
/// owned write head); `iter_all` / `stats` take `&self`. `BufWriter<File>`
/// mirrors the read side's `BufReader<File>` (spec line 978's `JournalIter`
/// field), supporting buffer→OS flush semantics per spec §B.2.3 + §X.10.
/// `path: PathBuf` is required so `iter_all(&self)` can open a fresh
/// `BufReader<File>` from the path each call without coordinating with the
/// owned writer (spec line 855-856 confirms `open()` does
/// `path.as_ref().to_path_buf()`).
///
/// `byte_offset` is plain `u64` (not `AtomicU64`) because `&mut self`
/// serializes the writer; the four counter atomics stay `AtomicU64` because
/// `stats(&self)` reads them through `&self`.
///
/// `PhantomData<T>` is required because `T` does not appear non-phantomly in
/// the struct (bytes flow through the file, not through any T-typed channel).
/// Trait bounds on `T` are added when Task 4 wires `append` / `iter_all` to
/// the rkyv 0.8 codec per spec §B.6.
///
/// `#[derive(Debug)]` is included for ergonomic test diagnostics
/// (`Result::unwrap_err`, `match` arm `{:?}` formatting). It synthesizes a
/// `T: Debug` bound on the `Debug` impl only — non-Debug method calls on
/// `FileJournal<T>` remain unrestricted.
#[derive(Debug)]
pub struct FileJournal<T> {
    writer: BufWriter<File>,
    path: PathBuf,
    byte_offset: u64,
    appended_total: AtomicU64,
    bytes_written_total: AtomicU64,
    read_total: AtomicU64,
    corrupt_frames_total: AtomicU64,
    _marker: PhantomData<T>,
}

/// In-process counter snapshot returned from `FileJournal::stats(&self)`.
///
/// Mirrors the `metrics::counter!` emissions documented in spec §B.3:
/// `event_journal_appended_total`, `event_journal_bytes_written_total`,
/// `event_journal_read_total`, `event_journal_corrupt_frames_total`. The
/// gauge surface (`current_depth`-style) is intentionally absent on the
/// journal per CLAUDE.md ("Journal and snapshot emit counters only — no
/// gauges").
///
/// `#[non_exhaustive]` per spec §5.1 so Phase 2 may add fields additively.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JournalStats {
    pub appended_total: u64,
    pub bytes_written_total: u64,
    pub read_total: u64,
    pub corrupt_frames_total: u64,
}

impl<T> FileJournal<T> {
    /// Opens (or creates) a journal file at `path`.
    ///
    /// Per spec §5.1 + §B.2.1's 6-case decision tree:
    ///
    /// - Path absent → create file (read+write), write the 8-byte header,
    ///   `byte_offset = FILE_HEADER_LEN`.
    /// - Existing 0-byte file → write the header, `byte_offset = FILE_HEADER_LEN`.
    ///   Note: this is process-local visibility (BufWriter buffer → kernel
    ///   page cache); `sync_all` / `sync_data` is Phase 2/4 work per spec
    ///   §X.10.
    /// - Existing 1..FILE_HEADER_LEN bytes → `Err(TruncatedFileHeader { found })`.
    /// - Existing >= FILE_HEADER_LEN bytes → `read_and_validate_file_header`;
    ///   on success seek to file end and `byte_offset = file_len`; on error
    ///   surface the appropriate variant from `read_and_validate_file_header`.
    /// - Open errors are surfaced as `JournalError::Io(...)`; spec §5.6 says
    ///   open-time errors do NOT increment `corrupt_frames_total`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, JournalError> {
        let path = path.as_ref().to_path_buf();

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(JournalError::Io)?;

        let len = file.metadata().map_err(JournalError::Io)?.len();

        let byte_offset = if len == 0 {
            // Path absent (just created) OR existing 0-byte file: write the
            // header; collapse the two cases per the §B.2.1 decision tree.
            file.seek(SeekFrom::Start(0)).map_err(JournalError::Io)?;
            write_file_header(&mut file).map_err(JournalError::Io)?;
            FILE_HEADER_LEN as u64
        } else if (len as usize) < FILE_HEADER_LEN {
            // Existing partial header — surface bytes for diagnostics.
            let mut found = vec![0u8; len as usize];
            file.seek(SeekFrom::Start(0)).map_err(JournalError::Io)?;
            file.read_exact(&mut found).map_err(JournalError::Io)?;
            return Err(JournalError::TruncatedFileHeader { found });
        } else {
            // Existing valid-length file: validate the header. Errors from
            // `read_and_validate_file_header` propagate verbatim so callers
            // see the precise failure mode (`InvalidFileHeader`,
            // `UnsupportedFileVersion`, `InvalidReservedHeader`).
            file.seek(SeekFrom::Start(0)).map_err(JournalError::Io)?;
            read_and_validate_file_header(&mut file)?;
            len
        };

        // Position the cursor at byte_offset for subsequent appends. For the
        // newly-written-header cases the file write_file_header() left the
        // cursor at 8 already, but we re-seek for clarity and to guard
        // against future helper changes.
        file.seek(SeekFrom::Start(byte_offset))
            .map_err(JournalError::Io)?;

        let writer = BufWriter::new(file);

        Ok(Self {
            writer,
            path,
            byte_offset,
            appended_total: AtomicU64::new(0),
            bytes_written_total: AtomicU64::new(0),
            read_total: AtomicU64::new(0),
            corrupt_frames_total: AtomicU64::new(0),
            _marker: PhantomData,
        })
    }

    /// Returns an in-process snapshot of the four counter atomics.
    ///
    /// Reads via `Ordering::Relaxed` because the consumer (operator dashboard
    /// or test) tolerates eventual consistency; the writer's `&mut self`
    /// path is the only ordered increment path.
    pub fn stats(&self) -> JournalStats {
        JournalStats {
            appended_total: self.appended_total.load(Ordering::Relaxed),
            bytes_written_total: self.bytes_written_total.load(Ordering::Relaxed),
            read_total: self.read_total.load(Ordering::Relaxed),
            corrupt_frames_total: self.corrupt_frames_total.load(Ordering::Relaxed),
        }
    }

    /// Stub flush; the J-17 contract (process-local visibility — NOT crash
    /// durability per spec §X.10) is formalized in Task 8. This stub exists
    /// so Task 3's J-1 test (which calls `journal.flush()`) compiles and so
    /// later tasks can rely on the buffer→OS drain semantics being already
    /// available on the type.
    pub fn flush(&mut self) -> Result<(), JournalError> {
        self.writer.flush().map_err(JournalError::Io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{FILE_FORMAT_VERSION, MAGIC, RESERVED_HEADER};
    use rust_lmax_mev_types::SmokeTestPayload;

    /// J-1 (TDD red→green): `open` on an absent path creates the file and
    /// writes the 8-byte file header (correct magic / version / reserved).
    #[test]
    fn open_creates_journal_with_valid_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.log");
        let mut journal = FileJournal::<SmokeTestPayload>::open(&path).unwrap();
        journal.flush().unwrap();
        drop(journal);
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len(), FILE_HEADER_LEN);
        assert_eq!(&bytes[0..4], MAGIC.as_slice());
        assert_eq!(bytes[4], FILE_FORMAT_VERSION);
        assert_eq!(&bytes[5..8], &RESERVED_HEADER[..]);
    }

    /// J-2 (test-first): pre-existing 0-byte file gets the header written by
    /// `open` (collapsed into the `len == 0` branch with the absent-path case
    /// per the §B.2.1 decision tree).
    #[test]
    fn open_empty_existing_file_writes_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.log");
        // Create the file first as a 0-byte file, then open through FileJournal.
        std::fs::File::create(&path).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);

        let mut journal = FileJournal::<SmokeTestPayload>::open(&path).unwrap();
        journal.flush().unwrap();
        drop(journal);

        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len(), FILE_HEADER_LEN);
        assert_eq!(&bytes[0..4], MAGIC.as_slice());
        assert_eq!(bytes[4], FILE_FORMAT_VERSION);
        assert_eq!(&bytes[5..8], &RESERVED_HEADER[..]);
    }

    /// J-4 (test-first, parameterized over partial lengths 1, 4, 7): existing
    /// file with `1..FILE_HEADER_LEN` bytes returns `TruncatedFileHeader`
    /// carrying the partial bytes for diagnostics.
    #[test]
    fn open_truncated_file_returns_truncated_file_header() {
        for partial in [1usize, 4, 7] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("journal.log");
            let raw = vec![0xABu8; partial];
            std::fs::write(&path, &raw).unwrap();

            // `FileJournal` does not implement Debug (would force a `T: Debug`
            // bound or a manual impl with no in-tree benefit yet); use
            // `unwrap_err()` so the unmatched arm pattern-matches on
            // `JournalError`, which does derive Debug via `thiserror`.
            let err = FileJournal::<SmokeTestPayload>::open(&path).unwrap_err();
            match err {
                JournalError::TruncatedFileHeader { found } => {
                    assert_eq!(found, raw, "partial = {partial}");
                }
                other => {
                    panic!("partial = {partial}: expected TruncatedFileHeader, got {other:?}")
                }
            }
        }
    }

    /// J-5 (test-first): wrong magic → `InvalidFileHeader { expected, found }`.
    #[test]
    fn open_wrong_magic_returns_invalid_file_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.log");
        // 8 bytes: bad magic "XXXX" + valid version (1) + valid reserved.
        std::fs::write(&path, b"XXXX\x01\x00\x00\x00").unwrap();

        let err = FileJournal::<SmokeTestPayload>::open(&path).unwrap_err();
        match err {
            JournalError::InvalidFileHeader { expected, found } => {
                assert_eq!(expected, MAGIC);
                assert_eq!(found, [b'X', b'X', b'X', b'X']);
            }
            other => panic!("expected InvalidFileHeader, got {other:?}"),
        }
    }

    /// J-6 (test-first): valid magic + `version = 2` →
    /// `UnsupportedFileVersion { version: 2 }`.
    #[test]
    fn open_unsupported_version_returns_unsupported_file_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.log");
        let mut bytes = MAGIC.to_vec();
        bytes.push(2); // version
        bytes.extend_from_slice(&RESERVED_HEADER);
        std::fs::write(&path, &bytes).unwrap();

        let err = FileJournal::<SmokeTestPayload>::open(&path).unwrap_err();
        match err {
            JournalError::UnsupportedFileVersion { version } => {
                assert_eq!(version, 2);
            }
            other => panic!("expected UnsupportedFileVersion, got {other:?}"),
        }
    }

    /// J-7 (test-first): valid magic + valid version + non-zero reserved
    /// (`[0, 1, 0]`) → `InvalidReservedHeader { found: [0, 1, 0] }`.
    #[test]
    fn open_nonzero_reserved_header_bytes_returns_invalid_reserved_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.log");
        let mut bytes = MAGIC.to_vec();
        bytes.push(FILE_FORMAT_VERSION);
        bytes.extend_from_slice(&[0, 1, 0]);
        std::fs::write(&path, &bytes).unwrap();

        let err = FileJournal::<SmokeTestPayload>::open(&path).unwrap_err();
        match err {
            JournalError::InvalidReservedHeader { found } => {
                assert_eq!(found, [0, 1, 0]);
            }
            other => panic!("expected InvalidReservedHeader, got {other:?}"),
        }
    }
}
