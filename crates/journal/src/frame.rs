//! Frame encode/decode helpers and file-header constants.
//!
//! Per spec §B.1.1 (constants), §B.1.2 (file header layout), §B.1.3 (per-record
//! layout), §B.1.4 (private helper signatures), §X.9 (IEEE CRC32 via
//! `crc32fast`), and §X.15 (`MAX_FRAME_LEN = 16 MiB`).
//!
//! The helpers in this module are `pub(crate)` because they are used by
//! `FileJournal::open` / `append` / `iter_all` (Tasks 3-7) but are not part
//! of the public API surface.

// Task 2 ships frame constants and helpers ahead of their consumers
// (`FileJournal::open` lands in Task 3; `append` / `iter_all` land in Tasks 4-7).
// Per the approved plan v0.3, the module-level `allow(dead_code)` is the
// "annotate the module, not individual items" approach; the annotation is
// removed in Task 12 once every constant and helper has at least one
// non-test caller.
#![allow(dead_code)]

use std::io::{Read, Write};

use crate::error::JournalError;

// === Constants (spec §B.1.1) ===

/// File-header magic 4-byte identifier `*b"LMEJ"` (LMAX MEV Event Journal).
/// Compared as raw bytes; not interpreted as a `u32` (no endian semantics).
pub(crate) const MAGIC: [u8; 4] = *b"LMEJ";

/// Phase 1 file-format version. Phase 2 may bump this and the open path will
/// reject unknown values via `UnsupportedFileVersion`.
pub(crate) const FILE_FORMAT_VERSION: u8 = 1;

/// File-header reserved bytes. Phase 1 enforces all-zero per spec §4.5;
/// Phase 2 may relax this if reserved bytes get defined semantics.
pub(crate) const RESERVED_HEADER: [u8; 3] = [0, 0, 0];

/// Total file-header length: 4 magic + 1 version + 3 reserved = 8 bytes.
pub(crate) const FILE_HEADER_LEN: usize = 8;

/// Maximum permitted per-record payload length (16 MiB) per spec §X.15.
/// Allocation-DoS protection: corrupted length prefixes are rejected before
/// the read path attempts to allocate `Vec::with_capacity(corrupt_length)`.
pub(crate) const MAX_FRAME_LEN: u64 = 16 * 1024 * 1024;

/// Per-record framing overhead: 4-byte length prefix + 4-byte CRC trailer.
pub(crate) const FRAME_OVERHEAD: usize = 4 + 4;

// === Helpers (spec §B.1.4) ===

/// Writes the 8-byte file header at the writer's current position.
///
/// Used by `FileJournal::open` when creating a new (or empty existing) file.
/// Layout per spec §B.1.2: 4 magic + 1 version + 3 reserved.
pub(crate) fn write_file_header<W: Write>(w: &mut W) -> std::io::Result<()> {
    w.write_all(&MAGIC)?;
    w.write_all(&[FILE_FORMAT_VERSION])?;
    w.write_all(&RESERVED_HEADER)?;
    Ok(())
}

/// Reads and validates the 8-byte file header from `r`.
///
/// Returns `Ok(())` when the magic, version, and reserved bytes all match.
/// On mismatch, returns the appropriate `JournalError` variant per spec
/// §5.3 + §B.4. I/O errors during the read surface as `JournalError::Io`;
/// open-time length pre-checks live in `FileJournal::open` so this helper
/// is only invoked on inputs of length ≥ `FILE_HEADER_LEN` per spec §B.2.1.
pub(crate) fn read_and_validate_file_header<R: Read>(r: &mut R) -> Result<(), JournalError> {
    let mut buf = [0u8; FILE_HEADER_LEN];
    r.read_exact(&mut buf).map_err(JournalError::Io)?;

    let mut magic = [0u8; 4];
    magic.copy_from_slice(&buf[0..4]);
    if magic != MAGIC {
        return Err(JournalError::InvalidFileHeader {
            expected: MAGIC,
            found: magic,
        });
    }

    let version = buf[4];
    if version != FILE_FORMAT_VERSION {
        return Err(JournalError::UnsupportedFileVersion { version });
    }

    let mut reserved = [0u8; 3];
    reserved.copy_from_slice(&buf[5..8]);
    if reserved != RESERVED_HEADER {
        return Err(JournalError::InvalidReservedHeader { found: reserved });
    }

    Ok(())
}

/// Validates that a frame's payload length is within the permitted Phase 1
/// range (`1..=MAX_FRAME_LEN`).
///
/// Used at both append-time (pre-write, per spec §4.4 step 3) and iter-time
/// (post-length-read, per spec §B.2.4 step b). The `offset` parameter is the
/// caller's frame-start byte offset for diagnostics: append callers pass
/// `self.byte_offset`, read callers pass `frame_start` from
/// `JournalIter::next` step b.
///
/// Returns `Err(InvalidFrameLength { offset, length, max: MAX_FRAME_LEN })`
/// for `length == 0` (zero-byte rkyv payload cannot decode to a valid
/// `EventEnvelope`) and `length > MAX_FRAME_LEN` (allocation-DoS protection).
pub(crate) fn validate_frame_length(length: u64, offset: u64) -> Result<(), JournalError> {
    if length == 0 || length > MAX_FRAME_LEN {
        return Err(JournalError::InvalidFrameLength {
            offset,
            length,
            max: MAX_FRAME_LEN,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// F-1: zero-length payload must surface `InvalidFrameLength`.
    /// `validate_frame_length` signature is `(length, offset)` per spec §B.1.4
    /// line 843; the `max` field in the error variant is hardcoded to
    /// `MAX_FRAME_LEN`.
    #[test]
    fn frame_encode_rejects_zero_length_payload() {
        let err = validate_frame_length(0, /* offset = */ 0).unwrap_err();
        match err {
            JournalError::InvalidFrameLength {
                length,
                max,
                offset,
            } => {
                assert_eq!(length, 0);
                assert_eq!(max, MAX_FRAME_LEN);
                assert_eq!(offset, 0);
            }
            other => panic!("expected InvalidFrameLength, got {other:?}"),
        }
    }

    /// F-2: `MAX_FRAME_LEN + 1` must surface `InvalidFrameLength` with the
    /// constant baked into the error context.
    #[test]
    fn frame_encode_rejects_oversized_payload() {
        let err = validate_frame_length(MAX_FRAME_LEN + 1, /* offset = */ 0).unwrap_err();
        assert!(matches!(
            err,
            JournalError::InvalidFrameLength { length, max, .. }
                if length == MAX_FRAME_LEN + 1 && max == MAX_FRAME_LEN
        ));
    }

    /// F-3: lengths `1` and `MAX_FRAME_LEN` (inclusive) must accept.
    #[test]
    fn frame_encode_accepts_boundary_lengths() {
        assert!(validate_frame_length(1, /* offset = */ 0).is_ok());
        assert!(validate_frame_length(MAX_FRAME_LEN, /* offset = */ 0).is_ok());
    }

    /// F-4: file-header round-trip with mutation cases for the four failure
    /// modes (`InvalidFileHeader`, `UnsupportedFileVersion`,
    /// `InvalidReservedHeader`, plus the happy path).
    #[test]
    fn write_then_read_file_header_round_trip() {
        let mut buf = Vec::new();
        write_file_header(&mut buf).unwrap();
        assert_eq!(buf.len(), FILE_HEADER_LEN);
        assert_eq!(&buf[0..4], MAGIC.as_slice());
        assert_eq!(buf[4], FILE_FORMAT_VERSION);
        assert_eq!(&buf[5..8], &RESERVED_HEADER[..]);

        // Happy path round-trip
        let mut cur = Cursor::new(&buf);
        assert!(read_and_validate_file_header(&mut cur).is_ok());

        // Bad magic
        let mut bad_magic = buf.clone();
        bad_magic[0] = b'X';
        let mut cur = Cursor::new(&bad_magic);
        match read_and_validate_file_header(&mut cur) {
            Err(JournalError::InvalidFileHeader { expected, found }) => {
                assert_eq!(expected, MAGIC);
                assert_eq!(found, [b'X', b'M', b'E', b'J']);
            }
            other => panic!("expected InvalidFileHeader, got {other:?}"),
        }

        // Bad version
        let mut bad_ver = buf.clone();
        bad_ver[4] = 2;
        let mut cur = Cursor::new(&bad_ver);
        assert!(matches!(
            read_and_validate_file_header(&mut cur),
            Err(JournalError::UnsupportedFileVersion { version: 2 })
        ));

        // Non-zero reserved
        let mut bad_rsv = buf.clone();
        bad_rsv[5] = 1;
        let mut cur = Cursor::new(&bad_rsv);
        assert!(matches!(
            read_and_validate_file_header(&mut cur),
            Err(JournalError::InvalidReservedHeader { found }) if found == [1, 0, 0]
        ));
    }
}
