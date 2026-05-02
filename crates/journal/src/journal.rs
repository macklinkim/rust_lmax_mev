//! `FileJournal<T>` — append-only file-backed log primitive.
//!
//! Per spec §5.1 (public API surface), §5.6 (open-time vs iter-time counter
//! semantics), §B.2.1 (open decision tree), §B.2.2 (append data flow),
//! §B.2.4 (iter_all + JournalIter::next steps), §B.3 (counter mirrors),
//! §B.6 (rkyv 0.8 trait bounds), §X.8 (fused-on-first-Err), §X.9 (IEEE CRC32),
//! §X.10 (flush is buffer→OS only; no durability).
//!
//! Task 4 lands `FileJournal::append`, `FileJournal::iter_all`, and
//! `JournalIter<'_, T>` (with `Iterator` impl). Per spec §B.6 the rkyv 0.8
//! codec bounds are inlined on the relevant `impl` blocks (rather than
//! bundled in a `JournalPayload` marker trait) — see the rkyv-bounds note
//! below. Corruption-path coverage tests are added in Task 6; the J-15 /
//! J-16 validate-boundary + same-iterator-fused tests are added in Task 7;
//! the J-17 flush-makes-visible contract test is added in Task 8.

// Counter atomics, byte_offset, and path are populated by `open` / `append`
// in this task but `corrupt_frames_total` is not yet incremented anywhere
// outside JournalIter (no corruption tests until Task 6); FRAME_OVERHEAD is
// re-exported from frame.rs for the append+iter math. The module-level
// annotation matches the plan v0.3 dead-code policy and is removed in Task
// 12 once every field has at least one non-test reader.
#![allow(dead_code)]

use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rkyv::api::high::{HighDeserializer, HighSerializer, HighValidator};
use rkyv::bytecheck::CheckBytes;
use rkyv::rancor::Error as RancorError;
use rkyv::ser::allocator::ArenaHandle;
use rkyv::util::AlignedVec;
use rust_lmax_mev_types::{EventEnvelope, JournalPosition};

use crate::error::JournalError;
use crate::frame::{
    read_and_validate_file_header, validate_frame_length, write_file_header, FILE_HEADER_LEN,
    FRAME_OVERHEAD,
};

// Note on rkyv 0.8 trait bounds:
// Spec §B.6 leaves the `JournalPayload` marker-trait refactor as
// implementer discretion. Task 4 inlines the bounds on the relevant
// `impl` blocks (FileJournal::append/iter_all and Iterator for
// JournalIter) because the trait-alias form requires the user-site to
// re-satisfy the where-clause bounds (the trait's where bounds are not
// projected through `T: JournalPayload` reliably across rkyv 0.8's
// `Archived` associated type), so the inline form is both shorter and
// more honest about what each method needs.

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
/// `stats(&self)` reads them through `&self` and `JournalIter` borrows the
/// `corrupt_frames_total` / `read_total` references for in-iter increments.
///
/// `PhantomData<T>` is required because `T` does not appear non-phantomly in
/// the struct (bytes flow through the file, not through any T-typed channel).
/// rkyv 0.8 trait bounds on `T` for `append` / `iter_all` are inlined on
/// the relevant `impl` blocks below per spec §B.6.
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
    /// Opens (or creates) a journal file at `path`. See spec §5.1 + §B.2.1.
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
            // see the precise failure mode.
            file.seek(SeekFrom::Start(0)).map_err(JournalError::Io)?;
            read_and_validate_file_header(&mut file)?;
            len
        };

        // Position the writer cursor at byte_offset for subsequent appends.
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
    pub fn stats(&self) -> JournalStats {
        JournalStats {
            appended_total: self.appended_total.load(Ordering::Relaxed),
            bytes_written_total: self.bytes_written_total.load(Ordering::Relaxed),
            read_total: self.read_total.load(Ordering::Relaxed),
            corrupt_frames_total: self.corrupt_frames_total.load(Ordering::Relaxed),
        }
    }

    /// Drains the BufWriter buffer into the kernel page cache (buffer→OS).
    ///
    /// Per spec §B.2.3 + §X.10 this is process-local visibility ONLY, NOT
    /// crash durability — `sync_all` / `sync_data` is Phase 2/4 work. The
    /// J-17 contract test in Task 8 asserts `flush + iter_all` reads back
    /// the appended record reliably.
    pub fn flush(&mut self) -> Result<(), JournalError> {
        self.writer.flush().map_err(JournalError::Io)
    }
}

impl<T> FileJournal<T>
where
    T: rkyv::Archive + 'static,
    T: for<'a> rkyv::Serialize<HighSerializer<AlignedVec, ArenaHandle<'a>, RancorError>>,
    <T as rkyv::Archive>::Archived: rkyv::Deserialize<T, HighDeserializer<RancorError>>
        + for<'a> CheckBytes<HighValidator<'a, RancorError>>,
{
    /// Appends an envelope as `[u32 length LE][payload][u32 CRC32 LE]` after
    /// the current write head. Returns the `JournalPosition` whose
    /// `byte_offset` is the offset of the FIRST byte of the frame (start of
    /// the length prefix).
    ///
    /// Per spec §B.2.2:
    /// 1. rkyv-serialize the envelope into an `AlignedVec` payload buffer.
    /// 2. `validate_frame_length(payload.len() as u64, frame_start)` —
    ///    pre-write rejection of zero / oversize per spec §4.4 step 3 +
    ///    §X.15. No bytes hit the file, no counters increment.
    /// 3. CRC32 = `crc32fast::hash(payload)` (IEEE polynomial per §X.9).
    /// 4. Write `[length LE u32][payload][crc LE u32]` through `self.writer`.
    /// 5. Bump `self.byte_offset` by `FRAME_OVERHEAD + payload.len()`.
    /// 6. Increment `event_journal_appended_total += 1` and
    ///    `event_journal_bytes_written_total += frame_size` (success-only,
    ///    AFTER the writer accepts all three pieces, per spec §5.6).
    /// 7. Return `JournalPosition { sequence: envelope.sequence(),
    ///    byte_offset: frame_start }`.
    ///
    /// **Partial-write semantics (spec §4.4):** `append` is NOT
    /// transactional. If an I/O error occurs during step 4 (e.g., disk full
    /// mid-write), trailing bytes for the in-progress record may be left in
    /// the writer's buffer / kernel page cache even though `append` returns
    /// `Err`. Recovery / truncation repair is Phase 2/4 reliability work.
    pub fn append(&mut self, envelope: &EventEnvelope<T>) -> Result<JournalPosition, JournalError> {
        let frame_start = self.byte_offset;

        // Step 1: rkyv-serialize.
        let payload = rkyv::to_bytes::<RancorError>(envelope).map_err(JournalError::Rkyv)?;

        // Step 2: pre-write length validation. Failure surfaces
        // `InvalidFrameLength` BEFORE the file write so neither counter
        // increments and no bytes hit the writer.
        validate_frame_length(payload.len() as u64, frame_start)?;

        // Step 3: CRC32.
        let crc = crc32fast::hash(payload.as_slice());

        // Step 4: write the three frame pieces. `payload.len()` is bounded
        // by `MAX_FRAME_LEN = 16 MiB` (validated in step 2), so the `as u32`
        // cast is non-narrowing.
        let length_bytes = (payload.len() as u32).to_le_bytes();
        let crc_bytes = crc.to_le_bytes();
        self.writer
            .write_all(&length_bytes)
            .map_err(JournalError::Io)?;
        self.writer
            .write_all(payload.as_slice())
            .map_err(JournalError::Io)?;
        self.writer
            .write_all(&crc_bytes)
            .map_err(JournalError::Io)?;

        // Step 5: advance the write head.
        let frame_size = (FRAME_OVERHEAD as u64) + (payload.len() as u64);
        self.byte_offset = frame_start + frame_size;

        // Step 6: success-only counter increments. Mirrors via both the
        // `metrics` facade (Prometheus export per ADR-008) and the in-process
        // atomics for `stats(&self)` consumers.
        self.appended_total.fetch_add(1, Ordering::Relaxed);
        self.bytes_written_total
            .fetch_add(frame_size, Ordering::Relaxed);
        metrics::counter!("event_journal_appended_total").increment(1);
        metrics::counter!("event_journal_bytes_written_total").increment(frame_size);

        // Step 7: return position.
        Ok(JournalPosition {
            sequence: envelope.sequence(),
            byte_offset: frame_start,
        })
    }

    /// Returns an iterator that walks the journal from the first record
    /// (`FILE_HEADER_LEN` byte offset) forward. Opens a FRESH OS file handle
    /// per call (spec §B.2.4 + line 978's `BufReader<File>`) so that the
    /// returned iterator does not coordinate with the owned writer; in-flight
    /// `BufWriter` bytes that have NOT been flushed are NOT visible to the
    /// read handle (this is the J-17 contract that Task 8 formalizes).
    ///
    /// On `File::open` / `seek` failure the returned iterator yields the
    /// `JournalError::Io(...)` once (incrementing `corrupt_frames_total` is
    /// the in-iter convention) and then fuses to `None` per spec §X.8.
    pub fn iter_all(&self) -> JournalIter<'_, T> {
        let opened = File::open(&self.path).and_then(|file| {
            let mut reader = BufReader::new(file);
            reader
                .seek(SeekFrom::Start(FILE_HEADER_LEN as u64))
                .map(|_| reader)
        });
        match opened {
            Ok(reader) => JournalIter {
                reader: Some(reader),
                byte_offset: FILE_HEADER_LEN as u64,
                fused: false,
                pending_err: None,
                read_total: &self.read_total,
                corrupt_frames_total: &self.corrupt_frames_total,
                _marker: PhantomData,
            },
            Err(e) => JournalIter {
                reader: None,
                byte_offset: 0,
                fused: false,
                pending_err: Some(JournalError::Io(e)),
                read_total: &self.read_total,
                corrupt_frames_total: &self.corrupt_frames_total,
                _marker: PhantomData,
            },
        }
    }
}

/// Forward iterator over journal records.
///
/// Created by `FileJournal::iter_all`. Per spec §X.8 the iterator is
/// **fused on first Err**: subsequent `next()` calls after the first error
/// return `None` regardless of remaining file content. Re-calling
/// `journal.iter_all()` produces a fresh iterator that re-yields the same
/// error on its first `next()`; that is intentional re-read, not a
/// violation.
///
/// Holds borrowed references to the parent's `read_total` and
/// `corrupt_frames_total` atomics so iter-time counter increments are
/// visible to subsequent `journal.stats(&self)` reads without locking.
pub struct JournalIter<'a, T> {
    reader: Option<BufReader<File>>,
    byte_offset: u64,
    fused: bool,
    pending_err: Option<JournalError>,
    read_total: &'a AtomicU64,
    corrupt_frames_total: &'a AtomicU64,
    _marker: PhantomData<T>,
}

impl<'a, T> JournalIter<'a, T> {
    fn fuse_with_corrupt(
        &mut self,
        err: JournalError,
    ) -> Option<Result<EventEnvelope<T>, JournalError>> {
        self.fused = true;
        self.corrupt_frames_total.fetch_add(1, Ordering::Relaxed);
        metrics::counter!("event_journal_corrupt_frames_total").increment(1);
        Some(Err(err))
    }
}

impl<'a, T> Iterator for JournalIter<'a, T>
where
    T: rkyv::Archive + 'static,
    <T as rkyv::Archive>::Archived: rkyv::Deserialize<T, HighDeserializer<RancorError>>
        + for<'b> CheckBytes<HighValidator<'b, RancorError>>,
{
    type Item = Result<EventEnvelope<T>, JournalError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.fused {
            return None;
        }
        if let Some(err) = self.pending_err.take() {
            return self.fuse_with_corrupt(err);
        }
        // SAFETY: `pending_err` was None ⇒ iter_all opened the file.
        let reader = self.reader.as_mut()?;

        let frame_start = self.byte_offset;

        // Step a: read the 4-byte length prefix, distinguishing 0-byte clean
        // EOF from 1..3-byte partial-EOF per spec §B.2.4.
        let mut len_buf = [0u8; 4];
        let mut len_filled = 0usize;
        while len_filled < 4 {
            match reader.read(&mut len_buf[len_filled..]) {
                Ok(0) => break,
                Ok(n) => len_filled += n,
                Err(e) => return self.fuse_with_corrupt(JournalError::Io(e)),
            }
        }
        if len_filled == 0 {
            // Clean EOF at record boundary — yield None WITHOUT incrementing
            // `corrupt_frames_total` per spec §B.4 row 9 footnote.
            return None;
        }
        if len_filled != 4 {
            return self.fuse_with_corrupt(JournalError::TruncatedFrame {
                offset: frame_start,
                needed: 4,
                got: len_filled as u32,
            });
        }
        let length = u32::from_le_bytes(len_buf) as u64;

        // Step b: length validation BEFORE allocation per spec §X.15.
        if let Err(e) = validate_frame_length(length, frame_start) {
            return self.fuse_with_corrupt(e);
        }

        // Step c: read the payload bytes.
        let mut payload = vec![0u8; length as usize];
        if let Err(e) = reader.read_exact(&mut payload) {
            // For Task 4 happy path we surface raw Io; Task 6 distinguishes
            // mid-payload EOF as TruncatedFrame.
            return self.fuse_with_corrupt(JournalError::Io(e));
        }

        // Step d: read the 4-byte CRC trailer.
        let mut crc_buf = [0u8; 4];
        if let Err(e) = reader.read_exact(&mut crc_buf) {
            return self.fuse_with_corrupt(JournalError::Io(e));
        }
        let crc_read = u32::from_le_bytes(crc_buf);

        // Step e: verify CRC.
        let crc_computed = crc32fast::hash(&payload);
        if crc_computed != crc_read {
            return self.fuse_with_corrupt(JournalError::ChecksumMismatch {
                offset: frame_start,
                expected: crc_computed,
                found: crc_read,
            });
        }

        // Step f: rkyv-deserialize.
        let envelope = match rkyv::from_bytes::<EventEnvelope<T>, RancorError>(&payload) {
            Ok(env) => env,
            Err(e) => return self.fuse_with_corrupt(JournalError::Rkyv(e)),
        };

        // Step g: mandatory `validate()` boundary per spec §5.4.
        if let Err(e) = envelope.validate() {
            return self.fuse_with_corrupt(JournalError::Types(e));
        }

        // Steps h-i: success — advance offset, bump counters, yield Ok.
        let frame_size = (FRAME_OVERHEAD as u64) + length;
        self.byte_offset = frame_start + frame_size;
        self.read_total.fetch_add(1, Ordering::Relaxed);
        metrics::counter!("event_journal_read_total").increment(1);
        Some(Ok(envelope))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{FILE_FORMAT_VERSION, MAGIC, MAX_FRAME_LEN, RESERVED_HEADER};
    use rust_lmax_mev_types::{ChainContext, EventSource, PublishMeta, SmokeTestPayload};

    /// Test-only payload whose rkyv-serialized size can be tuned past
    /// `MAX_FRAME_LEN` (16 MiB) for the J-10 oversize-rejection test.
    /// Implementer-discretion construction strategy per spec §B.5.2 J-10
    /// (option (a): feature-equivalent test-only payload type with a large
    /// dynamic byte buffer).
    #[derive(Clone, Debug, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
    struct OversizePayload {
        data: Vec<u8>,
    }

    fn valid_meta() -> PublishMeta {
        PublishMeta {
            source: EventSource::Ingress,
            chain_context: ChainContext {
                chain_id: 1,
                block_number: 18_000_000,
                block_hash: [0xAB; 32],
            },
            event_version: 1,
            correlation_id: 42,
        }
    }

    fn make_test_envelope(sequence: u64) -> EventEnvelope<SmokeTestPayload> {
        let payload = SmokeTestPayload {
            nonce: sequence + 7,
            data: [0xCD; 32],
        };
        EventEnvelope::seal(valid_meta(), payload, sequence, 1_700_000_000_000_000_000)
            .expect("valid envelope must seal")
    }

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

    /// J-3 (test-first; Task 4): inner block opens, appends one envelope,
    /// flushes, drops the BufWriter; outer block re-opens via
    /// `FileJournal::open` and asserts `iter_all` yields the previously-
    /// appended record. Deferred from Task 3 because it depends on
    /// `append` + `iter_all` which only land in this task.
    #[test]
    fn open_existing_journal_with_valid_header_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.log");
        let env = make_test_envelope(100);
        {
            let mut journal = FileJournal::<SmokeTestPayload>::open(&path).unwrap();
            journal.append(&env).unwrap();
            journal.flush().unwrap();
        } // BufWriter dropped here; bytes durable in OS page cache.

        let journal = FileJournal::<SmokeTestPayload>::open(&path).unwrap();
        let mut iter = journal.iter_all();
        let decoded = iter
            .next()
            .expect("re-open iter must yield record")
            .unwrap();
        assert_eq!(decoded, env);
        assert!(
            iter.next().is_none(),
            "single-record journal exhausts after one yield"
        );
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
        bytes.push(2);
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

    /// J-8 (TDD red→green; Task 4): open → append → flush → iter_all().next()
    /// yields `Ok(decoded == env)`, and the next next() yields None.
    #[test]
    fn append_then_read_round_trip_preserves_envelope() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.log");
        let mut journal = FileJournal::<SmokeTestPayload>::open(&path).unwrap();
        let env = make_test_envelope(100);

        let position = journal.append(&env).unwrap();
        journal.flush().unwrap();

        assert_eq!(position.sequence, env.sequence());
        assert_eq!(
            position.byte_offset, FILE_HEADER_LEN as u64,
            "first frame starts at FILE_HEADER_LEN"
        );

        let mut iter = journal.iter_all();
        let decoded = iter.next().expect("happy path must yield").unwrap();
        assert_eq!(decoded, env);
        assert!(iter.next().is_none(), "single-record journal exhausts");

        // stats() reflects success-only increments per spec §5.6.
        let stats = journal.stats();
        assert_eq!(stats.appended_total, 1);
        assert_eq!(stats.read_total, 1);
        assert_eq!(stats.corrupt_frames_total, 0);
        assert!(stats.bytes_written_total > 0);
    }

    /// J-9 (test-first; Task 4): three envelopes round-trip in order;
    /// `JournalPosition.byte_offset` matches the cumulative file position
    /// before each append, accounting for `FILE_HEADER_LEN` start +
    /// per-record `FRAME_OVERHEAD + payload.len()`.
    #[test]
    fn append_multiple_preserves_order_and_positions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.log");
        let mut journal = FileJournal::<SmokeTestPayload>::open(&path).unwrap();

        let envs = [
            make_test_envelope(100),
            make_test_envelope(101),
            make_test_envelope(102),
        ];

        let mut expected_offsets = Vec::with_capacity(envs.len());
        let mut cursor = FILE_HEADER_LEN as u64;
        for env in &envs {
            let pos = journal.append(env).unwrap();
            assert_eq!(pos.sequence, env.sequence());
            assert_eq!(
                pos.byte_offset, cursor,
                "frame_start must match the pre-append byte_offset"
            );
            expected_offsets.push(cursor);

            // Each frame is FRAME_OVERHEAD (8) + rkyv payload size; bump
            // cursor by the on-disk frame size. We measure it from the
            // bytes_written_total delta to avoid hard-coding the rkyv
            // payload size for SmokeTestPayload.
            let stats_after = journal.stats();
            cursor = (FILE_HEADER_LEN as u64) + stats_after.bytes_written_total;
        }
        journal.flush().unwrap();

        let decoded: Vec<EventEnvelope<SmokeTestPayload>> = journal
            .iter_all()
            .collect::<Result<Vec<_>, _>>()
            .expect("iter_all must succeed for happy-path frames");
        assert_eq!(decoded.len(), envs.len());
        for (got, want) in decoded.iter().zip(envs.iter()) {
            assert_eq!(got, want);
        }

        let final_stats = journal.stats();
        assert_eq!(final_stats.appended_total, envs.len() as u64);
        assert_eq!(final_stats.read_total, envs.len() as u64);
        assert_eq!(final_stats.corrupt_frames_total, 0);
    }

    /// J-10 (TDD red→green; Task 5): synthetic oversize payload triggers
    /// pre-write rejection per spec §4.4 step 3 + §X.15. After the failed
    /// `append` call, no counters increment, no bytes hit the file, and the
    /// file size on disk remains exactly `FILE_HEADER_LEN` (success-only
    /// counter semantic per spec §5.6; pre-write rejection per spec §B.4
    /// row 8 — append-side `InvalidFrameLength` does NOT increment
    /// `corrupt_frames_total`, only iter-side does).
    ///
    /// Payload construction strategy (spec §B.5.2 J-10 leaves to
    /// implementer): `OversizePayload { data: Vec<u8> }` test-only type
    /// declared above; populated with `MAX_FRAME_LEN + 1` bytes so the
    /// rkyv-encoded payload exceeds the cap (the rkyv encoding adds a small
    /// fixed header per Vec<u8>, so the raw byte count alone is enough to
    /// trip the limit).
    #[test]
    fn append_rejects_oversized_payload_before_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.log");
        let mut journal = FileJournal::<OversizePayload>::open(&path).unwrap();

        let oversize = OversizePayload {
            data: vec![0xFFu8; (MAX_FRAME_LEN + 1) as usize],
        };
        let env = EventEnvelope::seal(
            valid_meta(),
            oversize,
            /* sequence */ 100,
            /* timestamp_ns */ 1_700_000_000_000_000_000,
        )
        .expect("seal must succeed; payload size is not validated by seal");

        let err = journal.append(&env).unwrap_err();
        match err {
            JournalError::InvalidFrameLength { length, max, .. } => {
                assert!(
                    length > MAX_FRAME_LEN,
                    "rkyv payload {length} must exceed MAX_FRAME_LEN {max}"
                );
                assert_eq!(max, MAX_FRAME_LEN);
            }
            other => panic!("expected InvalidFrameLength, got {other:?}"),
        }

        // Success-only counters: append-side rejection does NOT bump any
        // counter per spec §5.6 + §B.4 row 8.
        let stats = journal.stats();
        assert_eq!(stats.appended_total, 0);
        assert_eq!(stats.bytes_written_total, 0);
        assert_eq!(stats.corrupt_frames_total, 0);

        // No bytes written to the file: validate_frame_length runs BEFORE
        // any of the three write_all() calls per spec §B.2.2 step 2.
        journal.flush().unwrap();
        drop(journal);
        let file_bytes = std::fs::read(&path).unwrap();
        assert_eq!(
            file_bytes.len(),
            FILE_HEADER_LEN,
            "file must contain only the 8-byte header; no partial frame bytes"
        );
    }
}
