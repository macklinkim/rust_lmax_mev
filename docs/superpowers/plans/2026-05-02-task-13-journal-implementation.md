# Task 13: `crates/journal` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan version:** 0.3 (revised after Codex 2026-05-02 11:25:42 +09:00 HIGH-confidence REVISION REQUIRED).

Changes from v0.2 (all five Codex 11:25:42 items addressed):

1. **`FileJournal<T>` struct now includes `path: PathBuf`** — without it, `iter_all(&self)` cannot open a fresh `BufReader<File>` (spec line 855-856's `open()` body does `path.as_ref().to_path_buf()`, confirming the path must be stored). Task 3 Step 3.1 struct definition adds `path: PathBuf` between the writer and the byte_offset.
2. **`validate_frame_length` signature aligned with spec §B.1.4 (line 843)** — spec is `validate_frame_length(length: u64, offset: u64)` (NOT `(length, max)`); `max` is the fixed `MAX_FRAME_LEN` constant in the error context, not a parameter. Task 2 Step 2.1 helper signature + Step 2.2/2.3 F-test bodies updated to pass `offset: 0` (or the test-supplied frame_start) and assert `max == MAX_FRAME_LEN` against the error variant's field.
3. **Task 10 Step 10.5 test count fixed: 28 → 29** — Task 9 cumulative is 27, Task 10 adds 2, so Task 10 final = 29 (matches the matrix table).
4. **Task 11 Step 11.1 binding fixed: `let journal` → `let mut journal`** — the I-1 sample code calls `journal.append(&env)` and `journal.flush()`, both `&mut self` per spec §5.1; `let mut` binding is required.
5. **Minor cleanups:**
   - Footer line "This plan v0.1 is drafted as a single docs file." → "This plan v0.3 is drafted as a single docs file." (was stale at v0.2; bumped to v0.3 in this revision).
   - Task 3 Step 3.2 "durable enough to survive a subsequent `iter_all` open" → "visible to a subsequent `open` / `iter_all`" to avoid implying crash durability (which is Phase 2/4 work per spec §X.10).

Changes from v0.1 (all five Codex 10:32:09 items addressed; preserved through v0.2 → v0.3):

1. **`JournalError::RocksDb` variant deferred from Task 1 to Task 9** — Task 1 ships 14 of 15 variants; the `RocksDb(rocksdb::Error)` variant is added in Task 9 alongside the rocksdb dep activation, so Task 1 can compile without the rocksdb crate in `crates/journal/Cargo.toml`. Resolves the v0.7 staged-deferral / Task 1 compile sequencing conflict.
2. **`FileJournal` API signatures aligned with spec §5.1** — `append(&mut self)`, `flush(&mut self)`, `iter_all(&self)`, `stats(&self)`, `open(path) -> Result<Self, _>`. Plan v0.1's `append(&self)` / `flush(&self)` was a scope deviation not authorized by the spec; v0.2 follows the spec verbatim.
3. **Storage model aligned: `BufWriter<File>` in struct, no `Mutex`** — flush semantics now consistent with the buffered-write API. Spec line 978's `JournalIter` already uses `BufReader<File>` for the read side; the write side mirrors with `BufWriter<File>`. Tests use `let mut journal = FileJournal::open(...)?` (mut binding for `&mut self` calls). v0.3 adds the `path: PathBuf` field needed for `iter_all` to open fresh read handles.
4. **J-3 (re-open + iter_all yields prior record) moved from Task 3 to Task 4** — J-3 depends on `append` + `iter_all`, both of which land in Task 4. Task 3 ships only J-1, J-2, J-4–J-7 (6 open-time tests, all using either `open` alone or pre-created synthetic files via `std::fs::write`).
5. **Task 9 verification prose cleaned up** — single authoritative running count line instead of the stricken-through "Wait..." paragraph. Final matrix unchanged: 30 journal + 7 event-bus + 4 types = 41 workspace tests (D14).

**Goal:** Implement the `crates/journal` crate per `docs/superpowers/specs/2026-05-01-task-13-journal-design.md` (v0.7, Batch A v0.4 + B v0.5 + C v0.6 + v0.7 amendment, all user-approved). Output: a four-module split-from-start crate exposing `FileJournal<T>` (append-only file-backed log) + `RocksDbSnapshot` (keyed save/load over RocksDB), the unified 15-variant `JournalError` enum, six metrics counters (4 journal + 2 snapshot), and 30 named tests (4 frame-module + 18 FileJournal + 7 RocksDbSnapshot + 1 cross-module).

**Architecture:** Five split modules under `crates/journal/src/` per spec §X.11:

| File | Responsibility |
|---|---|
| `lib.rs` | Crate-level docstring, `pub mod` declarations, public re-exports |
| `error.rs` | `JournalError` enum (15 variants, `#[non_exhaustive]`, `thiserror::Error`; no `#[from]` on either bincode variant) |
| `frame.rs` | `pub(crate)` constants (`MAGIC`, `FILE_FORMAT_VERSION`, `RESERVED_HEADER`, `FILE_HEADER_LEN`, `MAX_FRAME_LEN`, `FRAME_OVERHEAD`); `pub(crate)` helpers (`write_file_header`, `read_and_validate_file_header`, `validate_frame_length`); 4 inline F-1..F-4 tests |
| `journal.rs` | `FileJournal<T>` (open/append/flush/iter_all/stats); `JournalIter<T>` (fused-on-first-Err); `JournalStats` (atomic counter mirrors); 18 inline J-1..J-18 tests |
| `snapshot.rs` | `RocksDbSnapshot` (open/save/load/last_sequence/set_last_sequence/stats); `SnapshotStats` (atomic counter mirrors); reserved-key prefix constants; 7 inline S-1..S-7 tests; 1 cross-module I-1 test |

The split-from-start structure is mandated by §X.11 because per-file LOC was estimated to exceed the single-file 600 threshold. Tests touching private helpers MUST live inside that module's `#[cfg(test)] mod tests` block (no shared `tests.rs` sibling).

**Tech Stack:** Rust 1.80, edition 2021. Runtime deps already shipped at Gate 3 (`8f471b7`): `rust-lmax-mev-types` (path) + `rkyv = 0.8` + `bincode = 1.3` + `metrics = 0.23` + `thiserror = 2` + `serde = 1` + `crc32fast = 1`. Dev-dep already shipped: `tempfile = "3"`. **One additional runtime dep activation lands during this plan: `rocksdb = { workspace = true }` is added to `crates/journal/Cargo.toml` at the start of Task 9 per spec v0.7 amendment + commit `aa5c7c4`.** The workspace dep entry `rocksdb = "0.22"` already exists in `[workspace.dependencies]` from Task 10 scaffold; only the per-crate include is staged-deferred.

**Execution shell:** Windows 11 + PowerShell. CLAUDE.md conventions:

- GNU CLI shims (`cat.exe`, `mkdir.exe`, ...) exist via Scoop but PowerShell aliases shadow them; append `.exe` if GNU semantics needed.
- **Commit form: `git commit -F <file>` (file form), NOT multi `-m`.** Past sessions found the multi `-m` form trips on slash patterns inside arguments (e.g., `serialize / deserialize`). The Gate 3 commit (`8f471b7`) established the file-form pattern; this plan follows it. Commit-message file naming convention: `.git/COMMIT_TASK13_TN.txt` where `N` is the task number; the file is removed post-commit.
- Line-continuation: backtick (`` ` ``) at end of line.

**LLVM/libclang precondition (active for Task 9 onward):** the `rocksdb` crate transitively requires `libclang` for `bindgen` FFI generation. Host setup verified at the start of the Gate 3 retry session (clang 22.1.4 at `C:\Program Files\LLVM\bin`, `LIBCLANG_PATH` set). If the implementer enters Task 9 from a fresh session, re-verify with `clang --version`, `echo $env:LIBCLANG_PATH`, `where.exe libclang.dll` before adding the rocksdb dep.

---

## Pre-flight Reading

Before starting, the implementer **must** read:

1. **Spec:** `docs/superpowers/specs/2026-05-01-task-13-journal-design.md` (v0.7). The contract. Specifically internalize:
   - §3 Out-of-Scope (no fsync, no async writer, no read_at, no proptest, no bincode 2.x).
   - §4.4 (data flow + partial-write semantics + counter increment sites).
   - §4.5 (frame layout: 8-byte file header + per-record `[u32 len LE][payload][u32 crc LE]`).
   - §4.6 (ADR-003 reconciliation — Task 13 ships blocking primitive only; Task 16 wires non-blocking).
   - §5.1–§5.7 (public API surface + 15 `JournalError` variants + counter sites + anti-patterns).
   - §B.1 (frame internals: constants + helpers).
   - §B.2 (per-method data flow pseudo-Rust).
   - §B.3 (metrics emission map).
   - §B.4 (15-variant error matrix; **no `#[from]` on either bincode variant**).
   - §B.5 (named test plan, 4+18+7+1 = 30 tests; discipline tags).
   - §B.6 (rkyv 0.8 trait bounds + optional `JournalPayload` marker trait).
   - §C.1–§C.6 (concrete Cargo.toml deltas, commit bodies, DoD D1–D20).
   - §X.4 (bincode 1.x serde adapter; no `#[from]` on bincode variants).
   - §X.9 (IEEE CRC32 via `crc32fast`).
   - §X.11 (split-modules-from-start).
   - §X.13 (no proptest in Task 13).
   - §X.14 (`UnsupportedEventVersion` not raised in Phase 1).
   - §X.15 (`MAX_FRAME_LEN = 16 MiB`).

2. **Sibling plan (style reference):** `docs/superpowers/plans/2026-04-29-task-12-event-bus-implementation.md` (Task 12 plan v0.2). Mirrors task structure, TDD discipline, step-by-step verification.

3. **Frozen specs:**
   - `docs/adr/ADR-003-mempool-relay-persistence.md` (append-only journal + RocksDB snapshot + per-record CRC32).
   - `docs/adr/ADR-004-rpc-evm-stack-selection.md` (rkyv hot-path / bincode cold-path; ADR-004 wording fix landed at `f9e42fe`).
   - `docs/adr/ADR-008-observability-ci-baseline.md` (metrics facade for counter emission).

4. **Already-shipped types crate:** `crates/types/src/lib.rs`. Confirm:
   - `EventEnvelope::seal(meta, payload, sequence, timestamp_ns)` signature.
   - `EventEnvelope::validate()` body and the three rejected fields (`timestamp_ns == 0`, `event_version == 0`, `chain_id == 0`).
   - `JournalPosition { sequence: u64, byte_offset: u64 }` shape.
   - `TypesError::InvalidEnvelope { field, .. }` field name.
   - rkyv 0.8 derives on `EventEnvelope<T>` and the smoke-test payload.

5. **Already-shipped event-bus crate:** `crates/event-bus/src/lib.rs`. Confirm metric naming pattern (`event_journal_*` will mirror `event_bus_*`).

6. **Gate 3 scaffold commit:** `git show 8f471b7` — confirm the 8 files staged: `Cargo.toml`, `Cargo.lock`, `crates/journal/Cargo.toml`, `crates/journal/src/{lib.rs,error.rs,frame.rs,journal.rs,snapshot.rs}`. The 5 source files contain only crate-level docstring + module declarations + placeholder doc comments referencing spec sections. Tasks 1–11 below progressively fill them.

The plan below assumes the above are read. Spec section references use `§N.M` to point into the spec.

---

## File Structure

| Path | Action | Responsibility |
|---|---|---|
| `crates/journal/src/error.rs` | **Modify (Task 1)** | `JournalError` enum with 15 variants. |
| `crates/journal/src/frame.rs` | **Modify (Task 2)** | Constants + helpers + 4 inline tests. |
| `crates/journal/src/journal.rs` | **Modify (Tasks 3–8)** | `FileJournal<T>` + `JournalIter<T>` + `JournalStats` + 18 inline tests. |
| `crates/journal/Cargo.toml` | **Modify (Task 9)** | Activate `rocksdb = { workspace = true }` per v0.7 amendment. |
| `crates/journal/src/snapshot.rs` | **Modify (Tasks 9–11)** | `RocksDbSnapshot` + `SnapshotStats` + 7 inline + 1 cross-module test. |
| `crates/journal/src/lib.rs` | **Modify (Tasks 1, 12)** | Add public re-exports as types land; final cleanup. |
| `Cargo.lock` | Auto-modified (Task 9) | rocksdb compilation populates lockfile. |

**No other files are touched.** No `tests/` directory is created (tests are inline per `§X.11`).

---

## Why no Task 0 / scaffold task

Gate 3 (`8f471b7 chore(journal): scaffold journal crate and update workspace`) already shipped:

- Workspace edits: `crates/journal` member added; `crc32fast = "1"` added to `[workspace.dependencies]`.
- `crates/journal/Cargo.toml`: 7 runtime deps (rust-lmax-mev-types path-dep + rkyv + bincode + metrics + thiserror + serde + crc32fast) + 1 dev-dep (`tempfile = "3"`). The `rocksdb` line is staged-deferred per v0.7 amendment (commented-out hint in the manifest documents Task 9 activation).
- 5 placeholder source files in `crates/journal/src/` per §X.11 split-modules layout.

The scaffold passes all 5 verification gates: `cargo metadata` / `cargo build -p rust-lmax-mev-journal` / `cargo fmt --check` / `cargo test --workspace` (11 passed) / `cargo clippy --workspace --all-targets -- -D warnings`. Plan Task 1 enters directly into the `JournalError` content; no scaffolding remains.

---

## Task ordering rationale

The 12 tasks below preserve spec §C.5 step 4's recommended commit ordering, with these bundling decisions:

- **Frame module bundled (Task 2):** §C.5 step 4 lists three frame-related commits (`constants + write_file_header`, `validate_frame_length`, `read_and_validate_file_header`). Frame is small and the helpers compose cleanly; bundling keeps `frame.rs` to a single coherent commit covering all 4 F-tests (F-1..F-4).
- **Open path bundled (Task 3):** §C.5 step 4 lists two open commits (`create/empty path` + `existing-header path + negatives`). The negative cases (J-4..J-7) and the happy paths (J-1..J-3) all flow through `FileJournal::open` and are tightly coupled to the frame helpers; bundling keeps the 7 open-time tests in one commit.
- **rocksdb dep activation co-lands with first snapshot impl (Task 9):** §C.5 step 4 lists `RocksDbSnapshot::save + load + reserved-key (S-1, S-2, S-5, S-6)` as one commit. Because adding `rocksdb = { workspace = true }` and shipping a code path that uses it must land together (manifest-without-impl would generate a clippy `unused-crate-dependencies` warning under `-D warnings`), Task 9's first step is the manifest activation followed immediately by the impl.
- **Final cleanup task (Task 12):** §C.5 step 4 ends with "optional final fmt/clippy cleanup". Task 12 holds the DoD D1–D20 audit AND any fmt/clippy follow-up. If no follow-up is needed, Task 12 ships only the audit checklist (no commit).

Tasks 1–11 each produce **exactly one** commit. Task 12 produces 0 or 1 commits depending on cleanup need.

---

## Task 1: `JournalError` enum (14 of 15 variants; `RocksDb` deferred to Task 9)

**Discipline tag:** test-first verification (no TDD red→green; the enum compiles either way and tests follow in subsequent tasks).

**Files:**
- Modify: `crates/journal/src/error.rs`
- Modify: `crates/journal/src/lib.rs` (add `pub use error::JournalError;`)

**DoD coverage:** D4 partial (14 of 15 variants); `RocksDb` variant added in Task 9 alongside the rocksdb dep activation per spec v0.7 amendment + commit `aa5c7c4`.

- [ ] **Step 1.1:** Replace `crates/journal/src/error.rs` placeholder content with the enum per spec §B.4 + §5.3, **14 of 15 variants**: `Io`, `Types`, `Rkyv`, `BincodeSerialize`, `BincodeDeserialize`, `ChecksumMismatch`, `InvalidFrameLength`, `TruncatedFrame`, `InvalidFileHeader`, `TruncatedFileHeader`, `UnsupportedFileVersion`, `InvalidReservedHeader`, `LastSequenceUnavailable`, `ReservedKey`. The 15th variant `RocksDb(rocksdb::Error)` is intentionally NOT added in this task because the rocksdb dep is staged-deferred to Task 9 per spec v0.7 amendment + commit `aa5c7c4`; adding `rocksdb::Error` here would require pulling the rocksdb crate into `crates/journal/Cargo.toml` before Task 9, breaking the staged-deferral. Add a `// RocksDb variant added in Task 9 per spec v0.7 amendment` comment placeholder where the variant will land.

  Apply `#[non_exhaustive]` and `derive(Debug, thiserror::Error)`. **Critical: do NOT apply `#[from]` to `BincodeSerialize` or `BincodeDeserialize`** per §X.4 (callers use `.map_err(JournalError::BincodeSerialize)` / `.map_err(JournalError::BincodeDeserialize)` to disambiguate at the call site since both variants share `Box<bincode::ErrorKind>`). The other carrier variants (`Io`, `Types`, `Rkyv`) MAY use `#[from]`. The structured variants carry their context fields and use `#[error("...")]` Display strings.

- [ ] **Step 1.2:** Update `crates/journal/src/lib.rs` to add `pub use error::JournalError;` so callers can import `JournalError` directly via `rust_lmax_mev_journal::JournalError`.

- [ ] **Step 1.3 — verify:**
  ```powershell
  cargo build -p rust-lmax-mev-journal
  cargo clippy -p rust-lmax-mev-journal -- -D warnings
  cargo fmt --check
  ```
  Expected: all exit 0. Possible warnings on unused variants (most won't construct yet) — these go away as Tasks 2–11 fill the construction sites. **Do not** silence them with `#[allow(dead_code)]`; same as Task 11/12's policy on `UnsupportedEventVersion` / `Closed`. If clippy `-D warnings` rejects them now, audit each variant — for now most are fine because the enum has no constructors anyway, but if needed annotate the enum (NOT individual variants) with `#[allow(dead_code)]` and remove the annotation in Task 12.

- [ ] **Step 1.4 — commit (file-form):**

  Write `.git/COMMIT_TASK13_T1.txt`:
  ```
  feat(journal): JournalError enum with 14 of 15 variants and thiserror derives

  Adds the unified error type for FileJournal and RocksDbSnapshot per spec
  sections 5.3 and B.4. The enum is #[non_exhaustive] so Phase 2 may add
  variants additively.

  Ships 14 of 15 variants in this commit; the 15th variant
  RocksDb(rocksdb::Error) is intentionally deferred to Task 9 alongside
  the rocksdb dep activation per spec v0.7 amendment + commit aa5c7c4
  staged-deferral. Adding RocksDb here would require pulling the rocksdb
  crate into crates/journal/Cargo.toml before Task 9, breaking the
  staged-deferral.

  No #[from] is applied to BincodeSerialize or BincodeDeserialize because
  both wrap Box<bincode::ErrorKind> and a blanket #[from] would create
  ambiguity at call sites that need to distinguish the encode vs decode
  failure direction. Callers use .map_err(JournalError::BincodeSerialize)
  or .map_err(JournalError::BincodeDeserialize) explicitly per spec section
  X.4.

  The other carrier variants (Io, Types, Rkyv) use #[from] for callsite
  ergonomics. Structured variants (ChecksumMismatch, InvalidFrameLength,
  TruncatedFrame, InvalidFileHeader, TruncatedFileHeader,
  UnsupportedFileVersion, InvalidReservedHeader, ReservedKey) carry the
  context fields documented in spec section B.4. LastSequenceUnavailable
  is a unit variant.

  lib.rs adds pub use error::JournalError; so callers import the type
  via rust_lmax_mev_journal::JournalError.

  Co-Authored-By: Claude <noreply@anthropic.com>
  ```

  ```powershell
  git add crates/journal/src/error.rs crates/journal/src/lib.rs
  git commit -F .git/COMMIT_TASK13_T1.txt
  Remove-Item .git/COMMIT_TASK13_T1.txt
  ```

---

## Task 2: `frame.rs` — constants, helpers, F-1..F-4 tests

**Discipline tag:** F-1 (TDD red→green) + F-4 (TDD red→green) + F-2/F-3 (test-first verification).

**Files:**
- Modify: `crates/journal/src/frame.rs`

**DoD coverage:** D5, D6, partial D10 (4 tests added).

- [ ] **Step 2.1:** Replace `frame.rs` placeholder with the full module per spec §B.1.1 + §B.1.4:
  - `pub(crate) const MAGIC: [u8; 4] = *b"LMEJ";`
  - `pub(crate) const FILE_FORMAT_VERSION: u8 = 1;`
  - `pub(crate) const RESERVED_HEADER: [u8; 3] = [0, 0, 0];`
  - `pub(crate) const FILE_HEADER_LEN: usize = 8;`
  - `pub(crate) const MAX_FRAME_LEN: u64 = 16 * 1024 * 1024;`
  - `pub(crate) const FRAME_OVERHEAD: usize = 4 + 4;`
  - `pub(crate) fn write_file_header<W: std::io::Write>(w: &mut W) -> std::io::Result<()>` — writes 4 magic + 1 version + 3 reserved (8 bytes total) per §B.1.2.
  - `pub(crate) fn read_and_validate_file_header<R: std::io::Read>(r: &mut R) -> Result<(), JournalError>` — reads 8 bytes; if any read errors with EOF → `Err(JournalError::TruncatedFileHeader { found })` (caller passes already-known-prefix); compares magic against `MAGIC`, returns `InvalidFileHeader { expected, found }` on mismatch; checks version against `FILE_FORMAT_VERSION`, returns `UnsupportedFileVersion { version }` on mismatch; checks reserved against `RESERVED_HEADER`, returns `InvalidReservedHeader { found }` on mismatch.
  - `pub(crate) fn validate_frame_length(length: u64, offset: u64) -> Result<(), JournalError>` — returns `Err(InvalidFrameLength { offset, length, max: MAX_FRAME_LEN })` if `length == 0` or `length > MAX_FRAME_LEN`; otherwise `Ok(())`. **Signature locked to `(length, offset)` per spec §B.1.4 line 843** — `max` is the fixed `MAX_FRAME_LEN` constant baked into the error context, NOT a parameter. The `offset` parameter is the caller's frame-start byte offset for diagnostics: append path passes `self.byte_offset`, read path passes `frame_start` from `JournalIter::next` step b.

- [ ] **Step 2.2 (TDD red for F-1):** Add inline `#[cfg(test)] mod tests` block. First test:
  ```rust
  #[test]
  fn frame_encode_rejects_zero_length_payload() {
      // F-1: zero-length payload must surface InvalidFrameLength.
      // validate_frame_length signature is (length, offset) per spec §B.1.4 line 843;
      // `max` field in the error variant is hardcoded to MAX_FRAME_LEN.
      let err = validate_frame_length(0, /* offset = */ 0).unwrap_err();
      match err {
          JournalError::InvalidFrameLength { length, max, offset, .. } => {
              assert_eq!(length, 0);
              assert_eq!(max, MAX_FRAME_LEN);
              assert_eq!(offset, 0);
          }
          other => panic!("expected InvalidFrameLength, got {other:?}"),
      }
  }
  ```
  Run `cargo test -p rust-lmax-mev-journal frame_encode_rejects_zero_length_payload`. Expected: FAIL (compile error if `validate_frame_length` doesn't exist yet, OR runtime fail). Then implement `validate_frame_length` per §B.1.4 (Step 2.1's helper). Re-run. Expected: PASS.

- [ ] **Step 2.3 (test-first for F-2, F-3):** Add tests covering oversized + boundary lengths:
  ```rust
  #[test]
  fn frame_encode_rejects_oversized_payload() {
      // (length, offset) signature; max is the constant in the error context.
      let err = validate_frame_length(MAX_FRAME_LEN + 1, /* offset = */ 0).unwrap_err();
      assert!(matches!(
          err,
          JournalError::InvalidFrameLength { length, max, .. }
              if length == MAX_FRAME_LEN + 1 && max == MAX_FRAME_LEN
      ));
  }

  #[test]
  fn frame_encode_accepts_boundary_lengths() {
      assert!(validate_frame_length(1, /* offset = */ 0).is_ok());
      assert!(validate_frame_length(MAX_FRAME_LEN, /* offset = */ 0).is_ok());
  }
  ```
  Both should pass on first run because Step 2.1 already implements the helper.

- [ ] **Step 2.4 (TDD red→green for F-4):** Add the file-header round-trip test:
  ```rust
  #[test]
  fn write_then_read_file_header_round_trip() {
      let mut buf = Vec::new();
      write_file_header(&mut buf).unwrap();
      assert_eq!(buf.len(), FILE_HEADER_LEN);

      // Happy path
      let mut cur = std::io::Cursor::new(&buf);
      assert!(read_and_validate_file_header(&mut cur).is_ok());

      // Bad magic
      let mut bad_magic = buf.clone();
      bad_magic[0] = b'X';
      let mut cur = std::io::Cursor::new(&bad_magic);
      assert!(matches!(
          read_and_validate_file_header(&mut cur),
          Err(JournalError::InvalidFileHeader { .. })
      ));

      // Bad version
      let mut bad_ver = buf.clone();
      bad_ver[4] = 2;
      let mut cur = std::io::Cursor::new(&bad_ver);
      assert!(matches!(
          read_and_validate_file_header(&mut cur),
          Err(JournalError::UnsupportedFileVersion { version: 2 })
      ));

      // Non-zero reserved
      let mut bad_rsv = buf.clone();
      bad_rsv[5] = 1;
      let mut cur = std::io::Cursor::new(&bad_rsv);
      assert!(matches!(
          read_and_validate_file_header(&mut cur),
          Err(JournalError::InvalidReservedHeader { found }) if found == [1, 0, 0]
      ));
  }
  ```
  Run, then implement `write_file_header` + `read_and_validate_file_header` per Step 2.1 spec to make it pass.

- [ ] **Step 2.5 — verify:**
  ```powershell
  cargo test -p rust-lmax-mev-journal
  cargo clippy -p rust-lmax-mev-journal --all-targets -- -D warnings
  cargo fmt --check
  ```
  Expected: 4 frame tests pass (F-1, F-2, F-3, F-4). All gates exit 0.

- [ ] **Step 2.6 — commit (file-form):**

  Write `.git/COMMIT_TASK13_T2.txt`:
  ```
  feat(journal): frame.rs constants + helpers + F-1..F-4 tests

  Adds the frame module per spec sections B.1.1, B.1.2, B.1.3, B.1.4, X.9
  and X.15.

  Constants: MAGIC = *b"LMEJ", FILE_FORMAT_VERSION = 1, RESERVED_HEADER =
  [0, 0, 0], FILE_HEADER_LEN = 8, MAX_FRAME_LEN = 16 MiB, FRAME_OVERHEAD =
  8 (length prefix + CRC trailer). MAX_FRAME_LEN per spec section X.15
  prevents allocation-DoS from corrupted length prefixes.

  Helpers: write_file_header writes the 8-byte file header; the 8-byte
  layout is 4 magic + 1 version + 3 reserved per spec section B.1.2.
  read_and_validate_file_header reads the 8 bytes and surfaces the four
  failure modes (InvalidFileHeader, TruncatedFileHeader,
  UnsupportedFileVersion, InvalidReservedHeader) per spec section 5.3.
  validate_frame_length rejects length 0 (zero-byte rkyv payload cannot
  decode to EventEnvelope) and length > MAX_FRAME_LEN (allocation-DoS
  protection); both rejections happen BEFORE allocation per spec section
  4.5.

  Tests F-1..F-4 cover the four invariants and the file-header round-trip
  with mutation cases per spec section B.5.1. CRC32 helper emission lands
  alongside append in Task 4 because the helper signature depends on the
  payload-side bytes flowing through.

  Co-Authored-By: Claude <noreply@anthropic.com>
  ```

  ```powershell
  git add crates/journal/src/frame.rs
  git commit -F .git/COMMIT_TASK13_T2.txt
  Remove-Item .git/COMMIT_TASK13_T2.txt
  ```

---

## Task 3: `FileJournal::open` + J-1..J-7 tests

**Discipline tag:** J-1 (TDD red→green) + J-2..J-7 (test-first verification).

**Files:**
- Modify: `crates/journal/src/journal.rs`
- Modify: `crates/journal/src/lib.rs` (add `pub use journal::{FileJournal, JournalStats};` once `JournalStats` shape is settled)

**DoD coverage:** D7 partial (open path), partial D9 (`JournalStats` struct exists with all 4 atomic fields).

- [ ] **Step 3.1:** Add the `FileJournal<T>` struct + `JournalStats` to `journal.rs` per spec §5.1 + §B.2.1:
  - `pub struct FileJournal<T> { writer: BufWriter<File>, path: PathBuf, byte_offset: u64, appended_total: AtomicU64, bytes_written_total: AtomicU64, read_total: AtomicU64, corrupt_frames_total: AtomicU64, _marker: PhantomData<T> }` — `BufWriter<File>` mirrors the `BufReader<File>` used on the read side per spec line 978; supports the buffer→OS flush semantics in spec §B.2.3 + §X.10. **`path: PathBuf` is required** because `iter_all(&self)` opens a fresh `BufReader<File>` from the path each call (spec line 855-856 confirms `open()` body does `path.as_ref().to_path_buf()`); without storing the path, `iter_all` cannot create the read handle. **No `Mutex`** because `append` / `flush` use `&mut self` per spec §5.1, which already serializes access. `byte_offset` is plain `u64` (not `AtomicU64`) because `&mut self` serializes; the four counter atomics stay `AtomicU64` because `stats(&self)` reads them through `&self`.
  - `#[non_exhaustive] pub struct JournalStats { pub appended_total: u64, pub bytes_written_total: u64, pub read_total: u64, pub corrupt_frames_total: u64 }` per spec §5.1 + §B.3.
  - Trait bound on `T`: copy from §B.6. Implementer chooses inline bounds OR optional `JournalPayload` marker trait.
  - `PhantomData<T>` is required because `T` does not appear non-phantomly in the struct (bytes flow through the file, not through any T-typed channel).

- [ ] **Step 3.2:** Implement `FileJournal::open(path: impl AsRef<Path>) -> Result<Self, JournalError>` per spec §5.1 + §B.2.1's 6-case decision tree. First step: `let path = path.as_ref().to_path_buf();` (per spec line 855-856) — store this in the struct's `path` field so `iter_all` can open fresh read handles.
  - Path absent → create file (write+read mode), call `write_file_header`, wrap in `BufWriter::new(file)`, set `byte_offset = FILE_HEADER_LEN as u64`.
  - Existing 0-byte file → write file header (then `BufWriter::flush()` so the header bytes are visible to a subsequent `open` / `iter_all`), set `byte_offset = FILE_HEADER_LEN`. Note: this is process-local visibility (BufWriter buffer → kernel page cache), NOT crash durability — `sync_all` / `sync_data` is Phase 2/4 work per spec §X.10.
  - Existing 1..FILE_HEADER_LEN bytes → return `Err(TruncatedFileHeader { found: <vec_of_partial> })`.
  - Existing >= FILE_HEADER_LEN bytes → call `read_and_validate_file_header`; on error return that variant; on success seek to file end and set `byte_offset = file_len`.
  - Open errors are surfaced as `JournalError::Io(...)`; spec §5.6 says open-time errors do NOT increment `corrupt_frames_total`.

- [ ] **Step 3.3 (TDD red→green for J-1):** Add inline `#[cfg(test)] mod tests` block. First test:
  ```rust
  #[test]
  fn open_creates_journal_with_valid_header() {
      let dir = tempfile::tempdir().unwrap();
      let path = dir.path().join("journal.log");
      let mut journal = FileJournal::<SmokeTestPayload>::open(&path).unwrap();
      journal.flush().unwrap();  // Task 8 makes this real; for J-1 a stub Ok(()) is fine
      drop(journal);
      let bytes = std::fs::read(&path).unwrap();
      assert_eq!(bytes.len(), FILE_HEADER_LEN);
      assert_eq!(&bytes[0..4], MAGIC.as_slice());
      assert_eq!(bytes[4], FILE_FORMAT_VERSION);
      assert_eq!(&bytes[5..8], &RESERVED_HEADER[..]);
  }
  ```
  Run, fail, implement Steps 3.1+3.2, run again, pass. Note: `flush` may not be implemented yet at this task boundary; a trivial `pub fn flush(&mut self) -> Result<(), JournalError> { self.writer.flush().map_err(JournalError::Io) }` works for J-1 — Task 8 / J-17 formalizes the contract test.

- [ ] **Step 3.4 (test-first for J-2, J-4, J-5, J-6, J-7):** Add 5 open-path tests per spec §B.5.2. **J-3 (re-open + iter_all yields prior record) is deferred to Task 4** because it depends on `append` + `iter_all` which both land in Task 4. The tests in this step:
  - J-2 (`open_empty_existing_file_writes_header`): pre-create 0-byte file, `open` writes header.
  - J-4 (`open_truncated_file_returns_truncated_file_header`): pre-create 1/4/7-byte files, `open` returns `TruncatedFileHeader { found }`.
  - J-5 (`open_wrong_magic_returns_invalid_file_header`): pre-create file `b"XXXX\x01\x00\x00\x00"`, `open` returns `InvalidFileHeader`.
  - J-6 (`open_unsupported_version_returns_unsupported_file_version`): pre-create valid magic + `version = 2`, `open` returns `UnsupportedFileVersion { version: 2 }`.
  - J-7 (`open_nonzero_reserved_header_bytes_returns_invalid_reserved_header`): pre-create valid magic + version + `reserved = [0, 1, 0]`, `open` returns `InvalidReservedHeader { found: [0, 1, 0] }`.

  Each test uses `tempfile::tempdir()` + `std::fs::write` for synthetic setup; no append / iter dependency.

- [ ] **Step 3.5 — verify:**
  ```powershell
  cargo test -p rust-lmax-mev-journal
  cargo clippy -p rust-lmax-mev-journal --all-targets -- -D warnings
  cargo fmt --check
  ```
  Expected: 4 (frame) + 6 (open: J-1, J-2, J-4, J-5, J-6, J-7) = 10 journal-crate tests pass. **J-3 lands in Task 4.**

- [ ] **Step 3.6 — commit (file-form):**

  Write `.git/COMMIT_TASK13_T3.txt` with subject `feat(journal): FileJournal::open + 6 open-path tests (J-1, J-2, J-4..J-7)` and a 4-paragraph body describing: (1) the 6-case open decision tree per spec section B.2.1 with `BufWriter<File>` for the write-side mirroring the `BufReader<File>` read-side per spec line 978, (2) PhantomData<T> + rkyv 0.8 trait bounds rationale per spec section B.6, (3) JournalStats with 4 atomic counter fields per spec sections 5.1 and B.3 (atomics for `&self` stats() reads; struct uses `&mut self` for append/flush per spec §5.1 so no Mutex), (4) test coverage J-1 (happy path) + J-2 (empty existing) + 4 file-header failure modes (J-4/J-5/J-6/J-7); J-3 (re-open round-trip) deferred to Task 4 because it depends on append + iter_all.

  ```powershell
  git add crates/journal/src/journal.rs crates/journal/src/lib.rs
  git commit -F .git/COMMIT_TASK13_T3.txt
  Remove-Item .git/COMMIT_TASK13_T3.txt
  ```

---

## Task 4: `FileJournal::append` round-trip + iter_all happy path (J-3, J-8, J-9)

**Discipline tag:** J-8 (TDD red→green) + J-3 / J-9 (test-first verification).

**Files:**
- Modify: `crates/journal/src/journal.rs`

**DoD coverage:** D7 partial (append + iter_all happy path), D9 partial (`appended_total`, `bytes_written_total`, `read_total` increment sites).

- [ ] **Step 4.1:** Implement `FileJournal::append(&mut self, envelope: &EventEnvelope<T>) -> Result<JournalPosition, JournalError>` per spec §5.1 + §B.2.2 (append-side validation, partial-write semantics, success-only counter increment; counters via `metrics::counter!` AND atomic mirrors). Signature MUST be `&mut self` per spec §5.1; `&self` is a scope deviation not authorized. Writes go through `self.writer: BufWriter<File>` from Task 3.

- [ ] **Step 4.2:** Implement `FileJournal::iter_all(&self) -> impl Iterator<Item = Result<EventEnvelope<T>, JournalError>> + '_` per spec §5.1 + §B.2.4 — opens a fresh `BufReader<File>` at offset `FILE_HEADER_LEN` (matches spec line 978's `JournalIter` field `file: Option<BufReader<File>>`). Implement `JournalIter<T>` with `fused: bool` private field. `JournalIter::next` implements steps a–i per §B.2.4, INCLUDING the step-a 0-byte vs 1–3 byte EOF distinction (J-18). For Task 4, only the happy path needs to fully work; corruption paths can return placeholder errors that get exercised in Task 6. **Important read-side visibility note:** because `iter_all` opens a fresh OS file handle, in-flight `BufWriter` bytes that have not been flushed are NOT visible to the read handle. Tests that interleave append + iter_all MUST call `flush` between them — that is exactly what J-17 (Task 8) formalizes; J-3, J-8, J-9 use it implicitly.

- [ ] **Step 4.3 (TDD red→green for J-8):** Add inline test:
  ```rust
  #[test]
  fn append_then_read_round_trip_preserves_envelope() {
      let dir = tempfile::tempdir().unwrap();
      let path = dir.path().join("journal.log");
      let mut journal = FileJournal::<SmokeTestPayload>::open(&path).unwrap();
      let env = make_test_envelope();  // helper using EventEnvelope::seal()
      let position = journal.append(&env).unwrap();
      journal.flush().unwrap();
      assert_eq!(position.sequence, env.sequence());
      let mut iter = journal.iter_all();
      let decoded = iter.next().unwrap().unwrap();
      assert_eq!(decoded, env);  // EventEnvelope: Eq via Task 11 derives
      assert!(iter.next().is_none());
  }
  ```
  Note: `journal` MUST be `let mut` because `append` and `flush` take `&mut self` per spec §5.1.

- [ ] **Step 4.4 (test-first for J-9):** Add multi-record test:
  ```rust
  #[test]
  fn append_multiple_preserves_order_and_positions() {
      // 3 envelopes; iter_all yields them in order;
      // JournalPosition.byte_offset matches cumulative file size
      // before each append, accounting for FILE_HEADER_LEN start.
      // Uses `let mut journal` because append takes &mut self.
  }
  ```

- [ ] **Step 4.5 (test-first for J-3):** Add re-open + iter_all test (moved from Task 3 v0.1):
  ```rust
  #[test]
  fn open_existing_journal_with_valid_header_succeeds() {
      let dir = tempfile::tempdir().unwrap();
      let path = dir.path().join("journal.log");
      let env = make_test_envelope();
      {
          let mut journal = FileJournal::<SmokeTestPayload>::open(&path).unwrap();
          journal.append(&env).unwrap();
          journal.flush().unwrap();
      }  // BufWriter dropped here; bytes durable in OS page cache.
      // Re-open and confirm previously-appended record is iterable.
      let journal = FileJournal::<SmokeTestPayload>::open(&path).unwrap();
      let mut iter = journal.iter_all();
      let decoded = iter.next().unwrap().unwrap();
      assert_eq!(decoded, env);
  }
  ```
  This test was deferred from Task 3 v0.1 because it depends on `append` + `iter_all` which only land in this task.

- [ ] **Step 4.6 — verify:** `cargo test -p rust-lmax-mev-journal` → 4 (frame) + 6 (Task 3 open) + 3 (J-3, J-8, J-9 here) = 13 journal tests pass; `cargo clippy --all-targets -D warnings` clean; `cargo fmt --check` clean.

- [ ] **Step 4.7 — commit (file-form):**

  Write `.git/COMMIT_TASK13_T4.txt` with subject `feat(journal): FileJournal::append + iter_all happy path (J-3, J-8, J-9)` and body covering: append uses `&mut self` per spec §5.1 (no scope deviation), append-side invariant validation per spec §4.4, success-only counter increment per spec §B.3, partial-write non-transactional semantics per spec §4.4, iter_all opens fresh `BufReader<File>` at FILE_HEADER_LEN per spec §B.2.4 + spec line 978, JournalIter fused-bool per spec §X.8, J-3 deferred from Task 3 because it depends on append + iter_all.

  ```powershell
  git add crates/journal/src/journal.rs
  git commit -F .git/COMMIT_TASK13_T4.txt
  Remove-Item .git/COMMIT_TASK13_T4.txt
  ```

---

## Task 5: `FileJournal::append` length validation (J-10)

**Discipline tag:** J-10 (TDD red→green). Synthetic oversize payload required.

**Files:**
- Modify: `crates/journal/src/journal.rs`

**DoD coverage:** D7 partial (append-path length-validation gate).

- [ ] **Step 5.1 (TDD red for J-10):** Add inline test:
  ```rust
  #[test]
  fn append_rejects_oversized_payload_before_write() {
      // Use `let mut journal` because append takes &mut self per spec §5.1.
      // Synthetic envelope whose rkyv-encoded payload exceeds MAX_FRAME_LEN.
      // Implementer choice: feature-gated test-only payload type with a
      // large fixed-size byte array, OR a test-utility that bypasses
      // EventEnvelope::seal to construct an envelope with oversized inner
      // payload. Spec section B.5.2 J-10 leaves the construction strategy
      // to the implementer.
      // Assertions: append returns Err(InvalidFrameLength); after the call,
      // journal.stats() shows appended_total == 0 and bytes_written_total == 0.
  }
  ```
  Implement append-side `validate_frame_length` invocation BEFORE the file write so neither counter increments and no bytes hit the file (per §4.4 step 3 + §5.6 success-only increment rule).

- [ ] **Step 5.2 — verify:** test passes; clippy + fmt clean.

- [ ] **Step 5.3 — commit (file-form):**

  Write `.git/COMMIT_TASK13_T5.txt` with subject `feat(journal): FileJournal::append length validation (J-10)` and body covering: pre-write zero/oversize rejection per spec section 4.4 step 3, no-counter-increment + no-bytes-written success-only semantic per spec section 5.6, test construction strategy chosen.

  ```powershell
  git add crates/journal/src/journal.rs
  git commit -F .git/COMMIT_TASK13_T5.txt
  Remove-Item .git/COMMIT_TASK13_T5.txt
  ```

---

## Task 6: `JournalIter` corruption detection (J-11, J-12, J-13, J-14, J-18)

**Discipline tag:** J-11..J-14, J-18 (synthetic — direct file byte writes).

**Files:**
- Modify: `crates/journal/src/journal.rs`

**DoD coverage:** D9 (`corrupt_frames_total` increment sites), D7 partial (full `JournalIter::next` corruption paths).

- [ ] **Step 6.1:** Flesh out `JournalIter::next` corruption paths per §B.2.4 steps a-h:
  - Step b (length validation): `length == 0` → `InvalidFrameLength`; `length > MAX_FRAME_LEN` → `InvalidFrameLength` (rejected before allocation).
  - Step a (truncated length prefix): `len_filled in 1..4` → `TruncatedFrame { offset, needed: 4, got: len_filled }`.
  - Step a (clean EOF): `len_filled == 0` → `None` (NOT TruncatedFrame; counter NOT incremented).
  - Step c (truncated payload): partial read of payload bytes → `TruncatedFrame`.
  - Step d (truncated CRC): partial read of CRC bytes → `TruncatedFrame`.
  - Step e (CRC mismatch): `crc32fast::hash(payload) != crc_read` → `ChecksumMismatch`.
  - Step f (rkyv decode error): → `Rkyv(...)`.
  - On any Err: increment `corrupt_frames_total` (atomic + metrics), set `self.fused = true`, return Err. Subsequent `next()` returns `None`.

- [ ] **Step 6.2:** Add the 5 synthetic tests per spec §B.5.2 J-11, J-12, J-13, J-14, J-18. Each test writes the file header bytes manually + synthetic frame bytes via `std::fs::OpenOptions`, then opens a `FileJournal` and calls `iter_all().next()`. Pattern conventions per §B.5.5.

- [ ] **Step 6.3 — verify:** `cargo test -p rust-lmax-mev-journal` → 4+7+2+1+5 = 19 journal tests pass; clippy + fmt clean.

- [ ] **Step 6.4 — commit (file-form):** subject `feat(journal): JournalIter corruption detection (J-11..J-14, J-18)`. Body covers fused-on-first-Err per spec section X.8, step-a 0-byte clean-EOF vs 1-3 byte partial-EOF distinction per spec section B.2.4, allocation-DoS protection per spec section X.15.

---

## Task 7: `JournalIter` validate boundary (J-15, J-16)

**Discipline tag:** J-15 (synthetic byte-patch per §5.4) + J-16 (derived).

**Files:**
- Modify: `crates/journal/src/journal.rs`

**DoD coverage:** D7 (validate-boundary), D9 partial.

- [ ] **Step 7.1:** Confirm `JournalIter::next` step g calls `envelope.validate()` per spec §5.4. On `Err(InvalidEnvelope { .. })` → wrap in `JournalError::Types(...)`, increment `corrupt_frames_total`, fuse, yield Err.

- [ ] **Step 7.2 (J-15):** Implement the 11-step byte-patch test per spec §5.4 + §B.5.2 J-15. Procedure: serialize a valid envelope with sentinel `timestamp_ns` (e.g., `0x0123456789ABCDEF`); search for the sentinel little-endian byte pattern in the rkyv-encoded payload; if not exactly-once, panic with diagnostic; patch to `0x00..00`; recompute CRC; write file_header + length + patched-payload + recomputed-CRC directly via `std::fs::OpenOptions`; reopen `FileJournal::<SmokeTestPayload>`; assert `iter_all().next() == Err(JournalError::Types(TypesError::InvalidEnvelope { field, .. }))` with `field == "timestamp_ns"`.

- [ ] **Step 7.3 (J-16):** Add same-iterator fused test:
  ```rust
  #[test]
  fn corrupt_then_next_returns_none_on_same_iterator_instance() {
      // Build a corrupt file (truncated tail, e.g.).
      let mut iter = journal.iter_all();
      let first = iter.next();  // Err
      assert!(first.unwrap().is_err());
      let second = iter.next();  // None on SAME iterator
      assert!(second.is_none());
      // Re-call iter_all on a fresh iterator returns Err again — that's fine.
  }
  ```

- [ ] **Step 7.4 — verify:** `cargo test -p rust-lmax-mev-journal` → 4+7+2+1+5+2 = 21 journal tests pass; clippy + fmt clean.

- [ ] **Step 7.5 — commit (file-form):** subject `feat(journal): JournalIter validate boundary + same-iterator fused (J-15, J-16)`. Body covers mandatory validate() at deserialize boundary per spec section 5.4 + crates/types/src/lib.rs:16-23, byte-patch sentinel-search exactly-once invariant per J-15 step list, fused-per-iterator-instance semantics per spec section X.8.

---

## Task 8: `FileJournal::flush` + flush-makes-visible (J-17)

**Discipline tag:** J-17 (test-first verification).

**Files:**
- Modify: `crates/journal/src/journal.rs`

**DoD coverage:** D7 (`flush` method).

- [ ] **Step 8.1:** Implement `FileJournal::flush(&mut self) -> Result<(), JournalError>` per spec §5.1 + §B.2.3. Internal: call `self.writer.flush().map_err(JournalError::Io)` — `self.writer` is the `BufWriter<File>` from Task 3, so this drains the userspace buffer into the kernel (buffer→OS). NOT `sync_all` / `sync_data`. Spec §B.2.3 + §X.10 explicitly say flush is buffer→OS only; durability is Phase 2/4 work. No counter increment on flush. Signature MUST be `&mut self` per spec §5.1.

- [ ] **Step 8.2 (test-first for J-17):** Add inline test:
  ```rust
  #[test]
  fn flush_makes_appends_visible_to_iter_all() {
      // Append without flush: iter_all may not see the record (depends on
      // BufWriter behavior — assertion is "iter_all sees N or N-1 records").
      // Append + flush: iter_all sees N records reliably.
      // Test asserts the flush-then-iter_all sees all records;
      // does NOT assert sync_all (process-local visibility, not durability).
  }
  ```

- [ ] **Step 8.3 — verify:** `cargo test -p rust-lmax-mev-journal` → 22 journal tests pass; clippy + fmt clean.

- [ ] **Step 8.4 — commit (file-form):** subject `feat(journal): FileJournal::flush + flush-makes-visible (J-17)`. Body covers flush is buffer→OS not durability per spec sections B.2.3 and X.10, no counter increment per spec section B.3, deferred fsync per Phase 2.

---

## Task 9: rocksdb dep activation + `JournalError::RocksDb` variant + `RocksDbSnapshot::save/load/reserved-key` (S-1, S-2, S-5, S-6, S-7)

**Discipline tag:** S-1 (TDD red→green) + S-2/S-5/S-6/S-7 (test-first verification). **This task activates the v0.7 amendment-deferred rocksdb dep AND adds the 15th `JournalError` variant (`RocksDb`) deferred from Task 1.**

**Files:**
- Modify: `crates/journal/Cargo.toml` (**add `rocksdb = { workspace = true }` per spec v0.7 amendment + commit `aa5c7c4` staged-deferral**)
- Modify: `crates/journal/src/error.rs` (add `RocksDb(rocksdb::Error)` variant deferred from Task 1; uses `#[from]` per spec §B.4)
- Modify: `crates/journal/src/snapshot.rs`
- Modify: `crates/journal/src/lib.rs` (add `pub use snapshot::{RocksDbSnapshot, SnapshotStats};`)

**DoD coverage:** D4 (final variant — `RocksDb` brings count to 15 of 15), D8 partial (save + load + reserved-key paths), D9 partial (snapshot counters).

**Pre-flight check (libclang):** before this task starts, re-verify host has libclang configured for rocksdb's bindgen FFI generation:
```powershell
clang --version
echo $env:LIBCLANG_PATH
where.exe libclang.dll
```
All three must succeed. If any fails, halt and surface to user via outbox.

- [ ] **Step 9.1 (rocksdb dep activation per v0.7):** Edit `crates/journal/Cargo.toml`:
  - Remove the comment block documenting the staged deferral (the `# rocksdb = ...` and `# staged-deferred per spec v0.7 amendment ...` lines).
  - Replace with the active dep line: `rocksdb = { workspace = true }`.
  - Final dep order in `[dependencies]`: `rust-lmax-mev-types` → `rkyv` → `bincode` → `rocksdb` (newly added here) → `metrics` → `thiserror` → `serde` → `crc32fast`.

- [ ] **Step 9.1b (`JournalError::RocksDb` variant — deferred from Task 1):** Edit `crates/journal/src/error.rs`:
  - Remove the Task 1 placeholder comment marking the variant location.
  - Add `#[error("RocksDB error: {0}")] RocksDb(#[from] rocksdb::Error),` per spec §B.4 (uses `#[from]` for callsite ergonomics; spec §B.4 row 3 documents the `#[from]` choice for the carrier variant).
  - Verify the enum now has 15 variants total per spec §B.4.

- [ ] **Step 9.2:** Verify the dep activation builds cleanly (this is the first cargo build that compiles rocksdb + bindgen; expect 5–10 min on first run, downloading and compiling C++ rocksdb + transitive deps):
  ```powershell
  cargo build -p rust-lmax-mev-journal
  ```
  Expected: exits 0. If libclang error surfaces, halt and surface to user.

- [ ] **Step 9.3:** Implement `RocksDbSnapshot` struct + `SnapshotStats` per spec §5.2 + §B.2.5–§B.2.7:
  - `pub struct RocksDbSnapshot { db: Arc<rocksdb::DB>, saved_total: AtomicU64, loaded_total: AtomicU64 }`.
  - `#[non_exhaustive] pub struct SnapshotStats { pub saved_total: u64, pub loaded_total: u64 }`.
  - `pub(crate) const RESERVED_KEY_PREFIX: &[u8] = b"\0rust_lmax_mev:snapshot:";` and `pub(crate) const LAST_SEQUENCE_KEY: &[u8] = b"\0rust_lmax_mev:snapshot:last_sequence";`.
  - `RocksDbSnapshot::open(path: impl AsRef<Path>) -> Result<Self, JournalError>` opens or creates the RocksDB instance per §B.2.5.
  - `RocksDbSnapshot::save<V>(&self, key: &[u8], value: &V) -> Result<(), JournalError>` per §B.2.5: reject reserved-prefix keys before any RocksDB call; bincode-serialize via 1.3 serde adapter; put. `V: serde::Serialize`.
  - `RocksDbSnapshot::load<V>(&self, key: &[u8]) -> Result<Option<V>, JournalError>` per §B.2.6: reject reserved-prefix; get; if None return Ok(None) (no counter); if Some, bincode-deserialize; on success increment `loaded_total`. `V: serde::de::DeserializeOwned`.
  - `pub fn stats(&self) -> SnapshotStats`.

- [ ] **Step 9.4 (TDD red→green for S-1):** Add `#[cfg(test)] mod tests`:
  ```rust
  #[test]
  fn snapshot_save_load_round_trip() {
      let dir = tempfile::tempdir().unwrap();
      let snap = RocksDbSnapshot::open(dir.path().join("rocks")).unwrap();
      let val = SmokeTestPayload::default();
      snap.save(b"key1", &val).unwrap();
      let loaded: Option<SmokeTestPayload> = snap.load(b"key1").unwrap();
      assert_eq!(loaded, Some(val));
  }
  ```

- [ ] **Step 9.5 (test-first S-2, S-5, S-6, S-7):** Add 4 more snapshot tests per spec §B.5.3.

- [ ] **Step 9.6 — verify:**
  ```powershell
  cargo build -p rust-lmax-mev-journal
  cargo test -p rust-lmax-mev-journal
  cargo clippy -p rust-lmax-mev-journal --all-targets -- -D warnings
  cargo fmt --check
  ```
  Expected: **27 journal-crate tests pass** = 4 (frame F-1..F-4) + 6 (Task 3 open: J-1, J-2, J-4..J-7) + 3 (Task 4: J-3, J-8, J-9) + 1 (Task 5: J-10) + 5 (Task 6: J-11..J-14, J-18) + 2 (Task 7: J-15, J-16) + 1 (Task 8: J-17) + 5 (this task: S-1, S-2, S-5, S-6, S-7). Tasks 10 and 11 add 2 + 1 = 3 more for the final total of 30 (D10).

- [ ] **Step 9.7 — commit (file-form):**

  Write `.git/COMMIT_TASK13_T9.txt` with subject `feat(journal): rocksdb dep + JournalError::RocksDb + RocksDbSnapshot save/load (S-1, S-2, S-5, S-6, S-7)` and body covering: **v0.7 amendment activation point — adds rocksdb = { workspace = true } to crates/journal/Cargo.toml per spec v0.7 amendment + commit aa5c7c4 staged-deferral; libclang precondition verified at task entry**; **15th JournalError variant (RocksDb(rocksdb::Error)) deferred from Task 1 lands here alongside the dep activation per spec §B.4**; RocksDbSnapshot uses bincode 1.x serde adapter per spec §X.4; reserved-key prefix b"\\0rust_lmax_mev:snapshot:" rejection happens BEFORE any RocksDB call per spec §5.2 + §B.2.5; load(absent) returns Ok(None) with NO counter increment per spec §5.6 + §B.2.6; S-7 verifies aggregate counter behavior.

  ```powershell
  git add crates/journal/Cargo.toml crates/journal/src/error.rs crates/journal/src/snapshot.rs crates/journal/src/lib.rs Cargo.lock
  git commit -F .git/COMMIT_TASK13_T9.txt
  Remove-Item .git/COMMIT_TASK13_T9.txt
  ```

---

## Task 10: `RocksDbSnapshot::last_sequence` + `set_last_sequence` (S-3, S-4)

**Discipline tag:** S-3 (TDD red→green) + S-4 (test-first verification).

**Files:**
- Modify: `crates/journal/src/snapshot.rs`

**DoD coverage:** D8 (full).

- [ ] **Step 10.1:** Implement `RocksDbSnapshot::set_last_sequence(&self, seq: u64) -> Result<(), JournalError>` per §B.2.7: bincode-serialize the u64; write to `LAST_SEQUENCE_KEY` directly via internal RocksDB access (BYPASSES the user-facing reserved-prefix rejection because the reserved key IS the target). **Does NOT increment `saved_total`** per §B.2.7 (treated as bookkeeping, not user data).

- [ ] **Step 10.2:** Implement `RocksDbSnapshot::last_sequence(&self) -> Result<u64, JournalError>`: get `LAST_SEQUENCE_KEY`; if None → `Err(LastSequenceUnavailable)` (0 is NOT a sentinel because 0 is a valid sequence per Task 11); if Some → bincode-deserialize u64; on success return `Ok(u64)` (no counter increment).

- [ ] **Step 10.3 (TDD red→green S-3):** Add inline test:
  ```rust
  #[test]
  fn snapshot_last_sequence_round_trip() {
      let dir = tempfile::tempdir().unwrap();
      let snap = RocksDbSnapshot::open(dir.path().join("rocks")).unwrap();
      snap.set_last_sequence(42).unwrap();
      assert_eq!(snap.last_sequence().unwrap(), 42);
  }
  ```

- [ ] **Step 10.4 (test-first S-4):** Add absent-key test:
  ```rust
  #[test]
  fn snapshot_last_sequence_before_set_returns_unavailable() {
      let dir = tempfile::tempdir().unwrap();
      let snap = RocksDbSnapshot::open(dir.path().join("rocks")).unwrap();
      assert!(matches!(
          snap.last_sequence(),
          Err(JournalError::LastSequenceUnavailable)
      ));
  }
  ```

- [ ] **Step 10.5 — verify:** `cargo test -p rust-lmax-mev-journal` → 29 journal-crate tests pass (Task 9 cumulative 27 + Task 10 adds 2 = 29; matches the verification matrix).

- [ ] **Step 10.6 — commit (file-form):** subject `feat(journal): RocksDbSnapshot last_sequence + set_last_sequence (S-3, S-4)`. Body covers LAST_SEQUENCE_KEY reserved-prefix bypass (internal access) per spec section B.2.7, no saved_total increment for set_last_sequence (bookkeeping not user data) per spec section B.2.7, 0-is-not-sentinel rationale per spec section X.12, bincode-encoded u64 on the wire.

---

## Task 11: Cross-module integration test (I-1)

**Discipline tag:** I-1 (smoke).

**Files:**
- Modify: `crates/journal/src/snapshot.rs` (add the I-1 test in the snapshot module's tests block per spec §B.5.4 — using `RocksDbSnapshot` and `FileJournal` together; the snapshot module is the natural home).

**DoD coverage:** D10 (final test count = 30).

- [ ] **Step 11.1:** Add the I-1 test:
  ```rust
  #[test]
  fn journal_and_snapshot_can_coexist_in_separate_directories() {
      let dir = tempfile::tempdir().unwrap();
      let journal_path = dir.path().join("journal.log");
      let snapshot_path = dir.path().join("rocks");

      let mut journal = FileJournal::<SmokeTestPayload>::open(&journal_path).unwrap();
      let snapshot = RocksDbSnapshot::open(&snapshot_path).unwrap();

      let env = make_test_envelope();
      journal.append(&env).unwrap();  // &mut self per spec §5.1
      journal.flush().unwrap();       // &mut self per spec §5.1
      snapshot.save(b"checkpoint", &env.payload().clone()).unwrap();
      snapshot.set_last_sequence(env.sequence()).unwrap();

      let read_back: Option<SmokeTestPayload> = snapshot.load(b"checkpoint").unwrap();
      assert!(read_back.is_some());
      assert_eq!(snapshot.last_sequence().unwrap(), env.sequence());

      let mut iter = journal.iter_all();
      let decoded = iter.next().unwrap().unwrap();
      assert_eq!(decoded, env);
  }
  ```

- [ ] **Step 11.2 — verify:** `cargo test -p rust-lmax-mev-journal -- --list` lists 30 test functions; `cargo test -p rust-lmax-mev-journal` reports `30 passed; 0 failed`. `cargo test --workspace` → event-bus 7 + types 4 + journal 30 = 41 tests passing (D14). Clippy + fmt clean.

- [ ] **Step 11.3 — commit (file-form):** subject `feat(journal): cross-module integration test (I-1)`. Body covers smoke-level integration (journal + snapshot in separate paths within shared tempdir), exercises full save→last_sequence→load→iter_all path, no shared state between the two primitives (Phase 1 is split-stack snapshot-vs-journal per spec section 4.4), final test count 30 satisfies DoD D10.

---

## Task 12: DoD audit + final fmt/clippy/doc cleanup

**Discipline tag:** cleanup (no new tests; verification only).

**Files:**
- Audit only; modify only if cleanup is needed.

**DoD coverage:** D1, D2, D3, D11, D12, D13, D14, D15, D16, D17, D18, D19, D20.

- [ ] **Step 12.1 — DoD audit (D1–D20):** verify each DoD item per spec §C.4 table. Examples:
  - D1: `Select-String "crates/journal|crc32fast" Cargo.toml | Measure-Object` → ≥ 2 hits.
  - D2: byte-compare `crates/journal/Cargo.toml` against §C.1.2 listing (note: rocksdb line will now be present after Task 9 activation).
  - D3: `Get-ChildItem crates/journal/src/` → 5 files (`lib.rs`, `error.rs`, `frame.rs`, `journal.rs`, `snapshot.rs`).
  - D4: `Select-String -Pattern '^\\s+#\\[from\\]' -Path crates/journal/src/error.rs` → no hits on bincode variants.
  - D5: `Select-String "pub\\(crate\\) const" crates/journal/src/frame.rs` → 6 hits with named constants.
  - D6: grep for the 3 helper signatures.
  - D10: `cargo test -p rust-lmax-mev-journal -- --list | Select-String "^test " | Measure-Object` → 30.
  - D17: `cargo doc -p rust-lmax-mev-journal --no-deps`. Expected: clean, no broken intra-doc links.
  - D18: `git log --oneline -- docs/adr/ADR-004-rpc-evm-stack-selection.md crates/journal/` shows ADR-004 fix (`f9e42fe`) earlier than the journal commits (Gate 3 onward).
  - D19: `git log --all -- AGENTS.md .claude/` returns empty; `git log --oneline -- crates/types/ crates/event-bus/` shows no commits after `e2911cf` / `bb2e020`.
  - D20: `git diff -- docs/superpowers/specs/2026-05-01-task-13-journal-design.md` empty.

- [ ] **Step 12.2 — full workspace verification:**
  ```powershell
  cargo metadata --format-version 1 --no-deps > $null; Write-Host "METADATA=$LASTEXITCODE"
  cargo build --workspace; Write-Host "BUILD=$LASTEXITCODE"
  cargo fmt --check; Write-Host "FMT=$LASTEXITCODE"
  cargo test --workspace; Write-Host "TEST=$LASTEXITCODE"
  cargo clippy --workspace --all-targets -- -D warnings; Write-Host "CLIPPY=$LASTEXITCODE"
  cargo doc -p rust-lmax-mev-journal --no-deps; Write-Host "DOC=$LASTEXITCODE"
  ```
  All must exit 0. test count = 7 + 4 + 30 = 41.

- [ ] **Step 12.3 — optional cleanup commit (file-form):** if any fmt / clippy / dead-code annotations need cleanup that wasn't possible at earlier task boundaries (e.g., `#[allow(dead_code)]` on `JournalError` from Task 1 is no longer needed because all variants are now constructed), make those edits and ship a single commit.

  Subject: `chore(journal): final fmt/clippy cleanup`. Body covers: removed Task 1 `#[allow(dead_code)]` if any; final clippy/fmt pass; DoD D11–D17 all green.

  ```powershell
  git add crates/journal/src/...
  git commit -F .git/COMMIT_TASK13_T12.txt
  Remove-Item .git/COMMIT_TASK13_T12.txt
  ```

  If no cleanup is needed, **skip the commit**. Task 12 may ship 0 commits.

- [ ] **Step 12.4 — outbox + task_state.md update:** post Gate 5 completion summary to `.coordination/claude_outbox.md` requesting Codex review of the full Task 13 implementation pass; update `.coordination/task_state.md` gate progress table to mark Gate 5 ✅ COMPLETE. **Do NOT push, do NOT tag** — `task-13-complete` tag requires explicit user approval per CLAUDE.md.

---

## Verification matrix

After Task 12, the implementer runs the full DoD verification matrix per spec §C.4 and confirms all 20 items green. Per gate-by-gate plan:

| Gate | Tasks bundled | Tests added | Cumulative journal tests |
|---|---|---:|---:|
| Task 1 done | error.rs (14 of 15 variants; RocksDb deferred to Task 9) | 0 | 0 |
| Task 2 done | + frame.rs (constants + 3 helpers + F-1..F-4) | 4 | 4 |
| Task 3 done | + open path (J-1, J-2, J-4, J-5, J-6, J-7) | 6 | 10 |
| Task 4 done | + append + iter_all happy (J-3, J-8, J-9) | 3 | 13 |
| Task 5 done | + append length validation (J-10) | 1 | 14 |
| Task 6 done | + iter corruption (J-11..J-14, J-18) | 5 | 19 |
| Task 7 done | + iter validate (J-15, J-16) | 2 | 21 |
| Task 8 done | + flush (J-17) | 1 | 22 |
| Task 9 done | + rocksdb dep + RocksDb variant + save/load (S-1, S-2, S-5, S-6, S-7) | 5 | 27 |
| Task 10 done | + last_sequence (S-3, S-4) | 2 | 29 |
| Task 11 done | + cross-module (I-1) | 1 | 30 |
| Task 12 done | (DoD audit + optional cleanup) | 0 | 30 |

Implementer should verify the actual count after each task with `cargo test -p rust-lmax-mev-journal -- --list | Select-String "^test " | Measure-Object`.

Final Task 13 = 30 journal tests + 7 event-bus + 4 types = **41 workspace tests** (D14). 0 ignored, 0 failed.

---

## Out-of-scope reaffirmations (Task 13 implementation phase)

Same as spec §C.6 + §B.7:

- **No `phase-1-complete` tag** — reserved for Task 19.
- **No `task-13-complete` tag** without explicit user approval.
- **No bincode 2.x migration** per spec §X.4.
- **No alternative CRC polynomial** (IEEE only) per spec §X.9.
- **No async / non-blocking journal writer** per spec §3 + §4.6 + §B.7.
- **No `read_at(JournalPosition)`** (Phase 2 work) bleeding into implementation.
- **No `proptest`-driven tests** in Task 13 — Task 17 owns proptest workload per spec §X.13.
- **No edits to `crates/types/**`** (frozen at `e2911cf`).
- **No edits to `crates/event-bus/**`** (frozen at `bb2e020`).
- **No staging of `CLAUDE.md`, `AGENTS.md`, or `.claude/`** — `CLAUDE.md` is unstaged-deferred per Q2 (a); `AGENTS.md` and `.claude/` are forbidden to stage.
- **No `git push`, no `git tag` without explicit user approval**.
- **No alternative snapshot backend (sled/redb/fjall)** — RocksDB only per spec §C.6 + ADR-003.
- **Multi `-m` git commits forbidden in PowerShell** — `git commit -F <file>` form only.

---

## Approval and resume

This plan v0.3 is drafted as a single docs file. Per spec §C.5 + the user-delegated approval policy, this plan needs Codex APPROVED via `.coordination/codex_review.md` before Gate 5 (Task 1 implementation) begins.

After Codex APPROVED of this plan: Claude commits the plan as a single docs commit (`docs: add Task 13 (crates/journal) implementation plan`), then enters Task 1 using `superpowers:subagent-driven-development` to execute task-by-task with per-task review gates.
