# Task 13: `crates/journal` Design Spec — Batch A (Architecture / Scope / API surface / Resolved Decisions)

**Version:** 0.7 (v0.6 user-approved + 2026-05-02 rocksdb staged-deferral amendment per Codex resolution path B)
**Date:** 2026-05-02
**Status:** Batch A v0.4 + B v0.5 + C v0.6 user-approved 2026-05-01 (committed at `6f0368c`). v0.7 amendment 2026-05-02 documents rocksdb staged-deferral in §4.2 Notes and §C.1.2 after Gate 3 scaffold attempt found `libclang` transitive build dep on host (`bindgen` requirement of `rocksdb` crate). RocksDB remains the Phase 1 snapshot backend; only the dep listing is staged to Gate 5 implementation. Codex APPROVED resolution path B 2026-05-02 (HIGH confidence). §C.5 gate 3 (workspace + scaffold) retry blocked on host LLVM/libclang installation.
**Implements:** Task 13 of Phase 1 plan (`CLAUDE.md` Phase 1 task checklist).
**Depends on:** Task 11 (`crates/types`) — DONE; Task 12 (`crates/event-bus`) — DONE (final HEAD `bb2e020`).
**References:** ADR-003 (mempool/relay/persistence), ADR-004 (RPC/EVM stack), ADR-008 (observability + CI baseline), Task 11 spec §11.1 (deferred ADR-004 bincode wording reconciliation), Task 12 spec (`#[non_exhaustive]` policy + augmented D10 `--all-targets` clippy gate).

---

## 1. Goal

The crate `crates/journal` introduces:

- **`FileJournal<T>`** — append-only binary journal of `EventEnvelope<T>` records, persisted to disk per ADR-003.
- **`RocksDbSnapshot`** — embedded KV store for state snapshots; supports `save<V>` / `load<V>` / `last_sequence` / `set_last_sequence` smoke API per ADR-003.
- **`JournalError`** — unified error enum with `#[non_exhaustive]`, covering I/O, rkyv, bincode, RocksDB, CRC, frame validity, and types-layer validation failures.
- **Frame format** — file-level magic + version header (8 bytes, raw bytes) followed by per-record `[u32 length (LE)][rkyv payload][u32 CRC32 (LE)]`.
- **Metrics** — append/read/bytes-written/corrupt counters for the journal, save/load counters for the snapshot, all via the `metrics` facade with in-process `AtomicU64` mirrors exposed through `JournalStats` / `SnapshotStats`.
- **Tests** — round-trip, ordering, truncated-frame rejection, checksum-mismatch rejection, invariant-violating decoded envelope rejection (verifies the `validate()` boundary via byte-patch synthesis — see §5.4), invalid file header rejection, snapshot save/load, snapshot `last_sequence` smoke, reserved-key rejection. Test names finalized in Batch B.

The crate is the first Phase 1 consumer of two contracts deliberately deferred from earlier tasks:

1. **`EventEnvelope::validate()` at the deserialize boundary** (Task 11 crate-level docs `crates/types/src/lib.rs:16-23`). `serde::Deserialize` and `rkyv::Deserialize` reconstruct envelope fields directly, bypassing `seal()`. Without `validate()` after rkyv-decode, a corrupted-but-CRC-passing frame could yield an envelope with `timestamp_ns = 0`, `event_version = 0`, or `chain_context.chain_id = 0` and propagate into downstream stages.
2. **The bincode 1.3 serde adapter API** (Task 11 spec §11.1). ADR-004 Consequences names `bincode::Encode` / `bincode::Decode` (bincode 2.x), but the workspace pins `bincode = "1.3"` and `crates/types`'s round-trip test uses the serde adapter (`bincode::serialize` / `bincode::deserialize`). Task 11 deferred reconciliation to Task 13 entry; resolved in §X.4 (separate small ADR-004 docs commit).

The crate is **NOT** a downstream consumer of `EventBus<T>` / `EventConsumer<T>` (Task 12) in Phase 1. Wiring journal stages downstream of the bus is Task 16 (`crates/app`) work; Task 13 ships standalone primitives. See §4.6 for the relationship between Task 13's blocking primitives and the ADR-003 "never block event processing" rule.

---

## 2. Scope

### In scope (what Task 13 produces)

| # | Deliverable | Notes |
|---|---|---|
| 1 | `crates/journal` crate created at workspace path | Split modules from start: `lib.rs` + `error.rs` + `frame.rs` + `journal.rs` + `snapshot.rs`. See §4.1. |
| 2 | `FileJournal<T>` struct | `open` + `append` + `flush` + `iter_all` + `stats`. See §5.1. |
| 3 | `RocksDbSnapshot` struct | `open` + `save<V>` + `load<V>` + `last_sequence` + `set_last_sequence` + `stats`. See §5.2. |
| 4 | Frame format | File header `[u8;4] magic = *b"LMEJ"` + `[u8] version = 1` + `[u8;3] reserved = [0;3]`; per-record `[u32 length (LE)][rkyv payload][u32 CRC32 (LE)]`. See §4.5. |
| 5 | `JournalError` enum (`#[non_exhaustive]`) | Unifies I/O, rkyv, bincode, RocksDB, CRC, frame validity, types-layer validation. See §5.3. |
| 6 | `EventEnvelope::validate()` invocation at every decode site | Mandatory per Task 11 contract; verified by a dedicated byte-patch test. See §5.4. |
| 7 | Metrics emission via the `metrics` facade with in-process `AtomicU64` mirrors | Six counters total (4 journal + 2 snapshot). No labels (Phase 1 policy carried from Task 12). See §5.6 for emission semantics. |
| 8 | Inline tests | Round-trip + ordering + truncated/checksum/invariant rejection + header rejection + snapshot smoke + reserved-key rejection. Full plan in Batch B. |
| 9 | Workspace `Cargo.toml` updates | Re-add `crates/journal` member. Add `crc32fast = "1"` to `[workspace.dependencies]`. Bincode stays at 1.3; ADR-004 wording reconciled in a separate small docs commit. |
| 10 | Separate small ADR-004 docs commit | Updates ADR-004 Consequences from `bincode::Encode`/`Decode` (2.x form) to `serde::Serialize`/`serde::Deserialize` + `bincode` 1.x serde adapter wording. See §X.4. |

### Adjacent decisions baked in (not subject to revision in Batch A)

- **Append-only, never mutate.** `FileJournal::append` writes are strictly forward-only on the file. No record mutation, no in-place edits. Per ADR-003.
- **Per-record CRC32.** Frame integrity is checked on every read per ADR-003. Mismatched checksum → `JournalError::ChecksumMismatch` and the iterator becomes fused (§X.8).
- **Decode boundary calls `validate()`.** Every path that produces an `EventEnvelope<T>` from bytes (rkyv-deserialize) MUST call `EventEnvelope::validate()` before returning. A failed validate produces `JournalError::Types(TypesError::InvalidEnvelope { .. })`.
- **rkyv for journal frames** per ADR-003 + ADR-004 hot-path serialization decision.
- **bincode 1.3 (serde adapter) for snapshot values** per ADR-004 cold-path decision; ADR-004 wording reconciliation lands as a separate small docs commit (§X.4).
- **Phase 1 single-thread access.** `FileJournal` and `RocksDbSnapshot` are NOT designed for concurrent multi-writer use in Phase 1. The ownership pattern is one writer per journal / snapshot. `Send`/`Sync` bounds are not promised. Phase 2 concurrency wrapping is out of scope.
- **Phase 1 single-segment file.** No segment rotation, no compaction, no retention. One file per `FileJournal` instance.
- **No labels on metrics in Phase 1**, carried from Task 12 + ADR-008 deferred Task 15 label policy. If multiple `FileJournal` / `RocksDbSnapshot` instances are wired into the running app before Task 15 is complete, label policy becomes mandatory before merge.
- **No public traits in Phase 1.** Concrete `FileJournal<T>` and `RocksDbSnapshot` types only. Phase 2 may extract traits when a second impl is needed (e.g., S3-backed journal). See §5.5.
- **No fsync / crash-durability guarantee from `flush()`.** `flush()` flushes buffered bytes to the underlying file handle/OS only; it does NOT call `sync_all` / `sync_data`. Crash durability is Phase 4 reliability work. See §X.10 and §5.1.

---

## 3. Out of Scope

| Item | Lives in | Why deferred |
|---|---|---|
| Real Ethereum domain event payloads (`MempoolTx`, `BlockObserved`, etc.) | Phase 2 domain-event crate | Phase 1 ships only `SmokeTestPayload` per Task 11 spec §3. |
| Live mempool / RPC ingestion | Tasks beyond Phase 1 | The journal is the persistence layer, not the source. Per ADR-003 hybrid mempool design. |
| App-level wiring (journal stage downstream of `EventBus`) | Task 16 (`crates/app`) | Task 13 ships standalone primitives. See §4.6 for the relationship to ADR-003's "never block event processing" rule. |
| Async / non-blocking journal writer | Task 16 (`crates/app`) | `FileJournal::append` is sync-blocking I/O. Avoiding event-bus blocking is solved by running the journal on a dedicated consumer thread/stage in Task 16, not by adding async to Task 13. See §4.6. |
| Crash-durable fsync (`File::sync_all` / `sync_data`) on `append` or `flush` | Phase 4 reliability work | `flush()` is buffer→OS only; durability across crashes is out of Phase 1 scope. See §X.10. |
| `read_at(JournalPosition)` random-access reads | Phase 2 replay engine | Phase 1 ships `iter_all()` only. `append` returns `JournalPosition` for audit and future Phase 2 `read_at` preparation. See §X.3. |
| Continue-past-corruption iterator semantics | Phase 2 recovery feature | Phase 1 iterator yields exactly one `Err` for the first corrupt frame, then is fused. See §X.8. |
| `TypesError::UnsupportedEventVersion` raise path in journal | Phase 2 replay engine | Phase 1 events all use `event_version = 1`; Task 11's `validate()` does not currently raise this variant. See §X.14. |
| Replay engine (bus rehydration from journal + snapshot) | Phase 2 | Phase 1 verifies round-trip equality only, not replay semantics. |
| Prometheus exporter initialization | Task 15 (`crates/observability`) | Per ADR-008. The journal only emits via the `metrics` facade. |
| Grafana dashboards | Phase 5 | Per ADR-008 Consequences. |
| Archive-node / backtesting historical state access | Phase 4–5 | Out of Phase 1 vertical slice. |
| Production RocksDB tuning (block cache, compaction settings) | Phase 4 performance work | Per ADR-003 Consequences. |
| Multi-file segment rotation, compaction, retention policy | Phase 2 | Phase 1 single-file journal is sufficient for smoke. |
| S3 / cloud backup | Phase 4–5 | Out of scope. |
| Encryption / compression at rest | Phase 4–5 | Out of scope. |
| `EventBus<T>` / `EventConsumer<T>` integration | Task 16 (`crates/app`) | Task 13 ships standalone primitives. |
| Public traits (`Journal`, `Snapshot`) for replay-backend pluggability | Phase 2 | Phase 1 has one impl each; concrete types avoid speculative abstraction. |
| `proptest`-driven property tests for journal | Task 17 (integration smoke tests) | Per CLAUDE.md task list; Task 17 owns the smoke workload. See §X.13. |

If implementation pressure tries to expand into any of these, defer per this section.

---

## 4. Architecture & Crate Layout

### 4.1 File layout

**Resolved:** Split modules from start.

```
crates/journal/
├── Cargo.toml
└── src/
    ├── lib.rs      # public re-exports + crate-level docstring + module declarations
    ├── error.rs    # JournalError
    ├── frame.rs    # frame encode/decode + CRC32 helper + file header constants
    ├── journal.rs  # FileJournal<T> + JournalStats
    └── snapshot.rs # RocksDbSnapshot + SnapshotStats
```

**Test placement rule:** tests that touch a module's private helpers (e.g., raw byte-patch tests in `frame.rs`, white-box state inspection in `journal.rs`) MUST live inside that module's `#[cfg(test)] mod tests` block. Cross-module integration tests MAY live in `lib.rs`'s `#[cfg(test)] mod tests` block or in any module that is the natural owner of the assertion. There is no shared `tests.rs` sibling — each module owns its own test surface.

Rationale:
- Estimated LOC over the single-file 600 threshold based on Task 12 surface comparison; Task 13 surface (FileJournal + RocksDbSnapshot + JournalError + frame encode/decode + metrics + tests) is plausibly 700–1,000 LOC.
- Frame encoding + CRC + length validation is conceptually distinct from append/read I/O; separating clarifies invariants per module.
- Module split now is cheaper than refactoring later when Task 17 smoke tests + Task 18 CI add code that imports from this crate.

### 4.2 Crate dependencies (`crates/journal/Cargo.toml`)

```toml
[package]
name = "rust-lmax-mev-journal"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
publish = false
description = "Phase 1 append-only journal + RocksDB snapshot for the LMAX-style MEV engine"

[dependencies]
rust-lmax-mev-types = { path = "../types" }
rkyv = { workspace = true }
bincode = { workspace = true }      # 1.3 serde adapter (resolved §X.4 = (a))
rocksdb = { workspace = true }
metrics = { workspace = true }
thiserror = { workspace = true }
serde = { workspace = true }        # required for snapshot generic save<V>/load<V> bounds under bincode 1.3
crc32fast = { workspace = true }    # NEW workspace dep — resolved §X.1

[dev-dependencies]
tempfile = "3"                      # filesystem-backed test fixtures
```

Notes:
- **Staged dependency note (amended 2026-05-02): the `rocksdb = { workspace = true }` line is staged-deferred — Gate 3 scaffold ships `crates/journal/Cargo.toml` WITHOUT this line. `rocksdb` is added during Gate 5 implementation at the time the `RocksDbSnapshot` impl lands.** Reason: the `rocksdb` crate transitively requires `libclang` for `bindgen` FFI generation (host-side build prerequisite). Gate 3 scaffold ships without `rocksdb` so the workspace builds on hosts where `libclang` is not yet configured. **RocksDB remains the Phase 1 snapshot backend per ADR-003 + §5.2 + §B.2.5–§B.2.7; no alternate backend is selected; only the dep listing is deferred.**
- `crc32fast` is new to the workspace and must be added to `[workspace.dependencies]` in this task's first commit.
- `tempfile` follows the CLAUDE.md "Config crate needs `tempfile = '3'` in dev-dependencies" precedent. NOT a workspace dep — local dev-dep only.
- `serde` is needed for the snapshot generic API's `<V: serde::Serialize + serde::de::DeserializeOwned>` bounds (under bincode 1.3 serde adapter).
- The actual `rkyv` 0.8 trait bounds on `FileJournal<T>::T` are finalized in Batch B once the rkyv 0.8 API form is verified against `crates/types`'s envelope derives. Implementation memo for Batch B: `FileJournal<T>` will need a `PhantomData<T>` private field because `T` does not appear non-phantomly in the struct (bytes go through the file, not through any `T`-typed in-memory channel).

### 4.3 Workspace edits

Single chore commit (mirrors Task 11 + Task 12 first-commit pattern):

1. **`[workspace] members`:** re-add `"crates/journal"` (currently `["crates/types", "crates/event-bus"]`).
2. **`[workspace.dependencies]`:** add `crc32fast = "1"`.
3. **No bincode bump.** `bincode = "1.3"` stays.
4. **Separate small docs commit (independently committable, single file):** ADR-004 Consequences wording fix per §X.4.

The workspace edit + crate scaffold MUST land in a single commit to avoid the broken-checkout window (cargo metadata fails if a member entry references a non-existent crate manifest). Task 11 commit `4ac6f3c` and Task 12 commit `a92898e` established this pattern.

### 4.4 High-level data flow

#### Append path (write)

```
1. caller invokes FileJournal::<T>::append(&envelope)
2. envelope rkyv-serialized to a Vec<u8> (payload)
3. validate payload.len() (BEFORE any file write):
     - payload.len() == 0           → no write, no counter increment;
                                       return Err(InvalidFrameLength { offset: frame_start, length: 0, max })
     - payload.len() > MAX_FRAME_LEN → no write, no counter increment;
                                       return Err(InvalidFrameLength { offset: frame_start, length: payload.len() as u64, max })
   Note: the frame format encodes length as a u32 (§4.5). MAX_FRAME_LEN = 16 MiB
   is well within u32 range, so the u32 cast at write time is always safe by
   construction once this validation passes.
4. CRC32 of payload computed via crc32fast (IEEE polynomial; §X.9)
5. frame written to file: [u32 length=payload.len() as u32 (LE)][payload][u32 crc32 (LE)]
6. metrics (success path only — increment AFTER step 5 returns Ok):
     - event_journal_appended_total += 1
     - event_journal_bytes_written_total += frame_size  (record bytes only; file-header bytes excluded — §5.6)
7. JournalPosition { sequence: envelope.sequence(), byte_offset: frame_start } returned.
   The byte_offset is the offset of the FIRST byte of the frame (start of the length field) for audit and future
   Phase 2 read_at preparation.
```

**Partial-write failure semantics.** `append` is NOT transactional. If an I/O error occurs during step 5 (e.g., disk full mid-write), trailing bytes for the in-progress record may be left on the file even though `append` returns `Err`. Existing complete records before the failure remain intact (the journal is append-only and never mutates committed bytes). On a subsequent `iter_all`, the trailing partial record will surface as `JournalError::TruncatedFrame` or `ChecksumMismatch` (depending on which step the partial write reached) — whichever it is, it terminates the iterator via the fused-on-first-Err rule (§X.8).

This is the Phase 1 policy: **partial-write recovery / truncation repair is Phase 2 or Phase 4 reliability work**, not Task 13. Tests injecting mid-write I/O failure are likewise out of scope for Task 13 (see Batch B test list).

#### Read / replay path (`iter_all`)

```
1. caller invokes FileJournal::<T>::iter_all()
2. iterator opens a fresh read cursor at byte offset 8 (post-file-header; §4.5)
3. for each frame:
     a. read [u32 length (LE)]
     b. validate length:
        - length == 0           → metrics: corrupt_frames_total += 1; yield Err(InvalidFrameLength); fuse
        - length > MAX_FRAME_LEN → metrics: corrupt_frames_total += 1; yield Err(InvalidFrameLength); fuse
                                    (MAX_FRAME_LEN = 16 MiB, §X.15; rejection happens before allocation)
     c. read [length bytes] payload  (TruncatedFrame on EOF → metrics + yield + fuse)
     d. read [u32 crc32 (LE)]        (TruncatedFrame on EOF → metrics + yield + fuse)
     e. verify crc32fast(payload) == crc32
        ↳ mismatch → metrics: corrupt_frames_total += 1; yield Err(ChecksumMismatch); fuse
     f. rkyv-deserialize payload into EventEnvelope<T>
        ↳ rkyv error → metrics: corrupt_frames_total += 1; yield Err(Rkyv); fuse
     g. envelope.validate() called (mandatory per §5.4)
        ↳ failure → metrics: corrupt_frames_total += 1; yield Err(Types(InvalidEnvelope { .. })); fuse
     h. metrics: event_journal_read_total += 1
     i. yield Ok(envelope)
4. on clean EOF: iterator returns None
```

The iterator yields **exactly one `Err`** for the first corrupt or invariant-violating frame, then becomes **fused** — all subsequent `next()` calls return `None` regardless of remaining file content (§X.8). Continue-past-corruption recovery is Phase 2 work.

The `validate()` call at step 3.g is the **mandatory boundary** per Task 11 crate-level docs `crates/types/src/lib.rs:16-23`.

#### Snapshot save / load

```
save<V: Serialize>:
1. caller invokes RocksDbSnapshot::save(key, &value)
2. reject if key starts with reserved prefix (§5.2 reserved keys) → ReservedKey
3. value bincode-serialized (1.3 serde adapter)
4. RocksDB put(key, encoded_bytes)
5. metrics: event_snapshot_saved_total += 1   (success only)

load<V: DeserializeOwned>:
1. caller invokes RocksDbSnapshot::load(key)
2. reject if key starts with reserved prefix → ReservedKey
3. RocksDB get(key) → Option<Vec<u8>>
4. if None: return Ok(None)         (NO counter increment; §5.6)
5. if Some(bytes): bincode-deserialize into V
     - decode error → return Err(BincodeDeserialize(...))   (NO counter increment)
     - decode ok    → metrics: event_snapshot_loaded_total += 1; return Ok(Some(V))
```

`last_sequence()` reads the reserved key `b"\0rust_lmax_mev:snapshot:last_sequence"` holding a bincode-encoded `u64`. Initial state (no `set_last_sequence` ever called): returns `Err(JournalError::LastSequenceUnavailable)` (§X.12). `0` is NOT used as a sentinel because `0` is a valid sequence value (Task 11 sequences start at 0).

`set_last_sequence(seq)` writes the same reserved key. It uses internal RocksDB access, bypassing the user-facing `save`'s reserved-prefix rejection.

Reserved-key collision policy: caller's `save` and `load` reject any key starting with the reserved prefix `b"\0rust_lmax_mev:snapshot:"` with `JournalError::ReservedKey(...)`. The leading null byte makes accidental collision with human-readable user keys effectively impossible.

### 4.5 Frame layout

**File header (8 bytes total, written once at file creation):**

```
offset  size  field
0       4     [u8; 4] magic = *b"LMEJ"          (LMAX MEV Event Journal; raw bytes, not u32)
4       1     [u8]    file_format_version = 1
5       3     [u8; 3] reserved = [0, 0, 0]
```

The magic is a 4-byte ASCII identifier. It is NOT interpreted as a `u32` integer (no big-endian / little-endian semantics). On open, the implementation reads exactly 4 bytes and compares against the literal `*b"LMEJ"`. `file_format_version = 1` is the Phase 1 value; Phase 2 may bump this to introduce a new frame layout, with the open path raising `UnsupportedFileVersion` for unknown values.

**Per-record (8 bytes overhead per record):**

```
offset(rel)  size  field
0            4     [u32 length (LE)]            length of the payload bytes
4            length  payload                    rkyv-encoded EventEnvelope<T>
4 + length   4     [u32 CRC32 (LE)]             IEEE CRC32 of the payload bytes
```

Per-record fields are **little-endian**. The endianness choice matches Rust's most common convention and is consistent with x86-64/aarch64 host byte order.

The first record begins at file offset 8 (immediately after the file header).

**Bounds:**
- `length` MUST be in `1..=MAX_FRAME_LEN` (`MAX_FRAME_LEN = 16 MiB`, §X.15).
- `length == 0` is rejected with `InvalidFrameLength` because a zero-byte rkyv payload cannot decode a valid `EventEnvelope`. Same rule applies on both the read path and the append path (§4.4).
- `length > MAX_FRAME_LEN` is rejected with `InvalidFrameLength` BEFORE the implementation attempts to allocate or read the payload; this prevents a corrupted length prefix from triggering an allocation DoS. Same rule on append (caller cannot publish an oversized envelope without `append` returning `InvalidFrameLength`).
- `MAX_FRAME_LEN` (16 MiB) fits in `u32`, so the per-record length field's `u32` width is non-narrowing for any value that passes validation.

**File header reserved bytes:** the `[u8; 3] reserved` field MUST be all-zero (`[0, 0, 0]`). On open, a non-zero reserved-bytes pattern is rejected with `JournalError::InvalidReservedHeader { found: [u8; 3] }` (§5.3). This preserves the option for Phase 2 to assign meaning to one or more reserved bytes; Phase 1 readers refuse to interpret a file whose reserved bytes claim a Phase 2 capability the Phase 1 reader does not understand. Phase 2 may relax this rejection if/when reserved bytes get defined semantics.

### 4.6 Relationship to ADR-003 "journal writer must never block event processing"

ADR-003 Consequences states: "The journal writer must be a dedicated component on the event bus hot path; it must never block event processing."

Task 13's `FileJournal::append` is a synchronous, blocking primitive. Filesystem I/O and `crc32fast` computation block the calling thread. **Task 13 does NOT add async or non-blocking semantics to the primitive.**

The "never block event processing" rule is satisfied at app-wiring time (Task 16, `crates/app`):
- The journal writer runs as a **dedicated consumer thread/stage** downstream of an `EventBus<T>` (Task 12).
- The bus's bounded queue absorbs publish-side bursts; the journal consumer drains its own pace and applies backpressure to upstream stages via the bus's natural `try_send → Full` path.
- The blocking `append` is only blocking on the journal consumer's own thread, NOT on bus producer threads or pipeline-stage threads.

**Task 13's blocking primitive alone does not satisfy the ADR-003 non-blocking pipeline property.** That property is satisfied only when Task 16 wires the journal as a dedicated stage/thread with appropriate backpressure and monitoring. Reading "Task 13 implements `FileJournal::append` as sync I/O, therefore ADR-003 is satisfied" is a category error: ADR-003's "never block event processing" is a pipeline-topology property, not an API-surface property. Task 13 ships only the primitive.

Async / non-blocking journal writers are out of scope for Phase 1 (see §3 Out of Scope).

---

## 5. Public API Surface

This section sketches the public types and their method signatures. Internal field layouts and concrete trait bounds are finalized in Batch B.

### 5.1 `FileJournal<T>`

```rust
pub struct FileJournal<T> {
    /* private; finalized in Batch B. Will include PhantomData<T> because T
       does not appear non-phantomly elsewhere in the struct. */
}

impl<T> FileJournal<T>
where
    T: /* rkyv 0.8 archive/serialize/deserialize bounds — Batch B */,
{
    /// Opens (or creates) a journal file at `path`. Open semantics:
    ///
    /// - Path absent on disk          → create file and write the 8-byte file header.
    /// - Existing file with `len == 0` → treat as a fresh empty journal; write the file header.
    /// - Existing file with `1 <= len < 8` → reject with `TruncatedFileHeader { found }`.
    /// - Existing file with `len >= 8`:
    ///     - Wrong magic               → `InvalidFileHeader`.
    ///     - Unsupported version       → `UnsupportedFileVersion`.
    ///     - Reserved bytes != [0;3]   → `InvalidReservedHeader`.
    ///     - All header fields valid   → open succeeds; first record is at file offset 8.
    ///
    /// Open-time header errors are NOT counted by `event_journal_corrupt_frames_total`
    /// (that counter is iter-time only — see §5.6).
    pub fn open(path: impl AsRef<Path>) -> Result<Self, JournalError>;

    /// Appends an envelope as a single rkyv-encoded frame with CRC32 trailer.
    ///
    /// The journal does NOT enforce sequence monotonicity at the file layer;
    /// the bus owns that invariant. The journal records what it is given.
    ///
    /// Frame length validation occurs BEFORE the file write. A zero-length
    /// rkyv payload or a payload exceeding `MAX_FRAME_LEN` (16 MiB, §X.15) is
    /// rejected with `JournalError::InvalidFrameLength` and no bytes are
    /// written, no counters increment, and no `JournalPosition` is returned.
    /// See §4.4 append path.
    ///
    /// `append` is NOT transactional. If an I/O error interrupts the frame
    /// write mid-record, trailing partial bytes may remain on the file even
    /// though `append` returns `Err`. Existing complete records before the
    /// failure are intact (append-only semantics, never mutated). A later
    /// `iter_all` will surface the trailing partial record as
    /// `TruncatedFrame` or `ChecksumMismatch`. Crash/partial-write recovery
    /// is Phase 2 / Phase 4 reliability work — see §4.4.
    ///
    /// Returns the position of the newly written frame: `JournalPosition {
    /// sequence: envelope.sequence(), byte_offset: frame_start }`. The
    /// `byte_offset` is the offset of the FIRST byte of the frame (start of
    /// the length field), recorded for audit and future Phase 2 `read_at`
    /// random-access reads. Phase 1 callers can ignore it; the value is
    /// retained for forward compatibility.
    pub fn append(&mut self, envelope: &EventEnvelope<T>) -> Result<JournalPosition, JournalError>;

    /// Flushes buffered bytes to the underlying file handle/OS. Does NOT
    /// call `sync_all` / `sync_data` — crash durability is Phase 4
    /// reliability work and is out of Phase 1 scope.
    ///
    /// Tests must call `flush()` before `iter_all()` if using the same
    /// process after `append` to ensure the OS sees the buffered writes.
    pub fn flush(&mut self) -> Result<(), JournalError>;

    /// Returns an iterator over all events in the journal, validating each
    /// envelope before yielding (per Task 11 boundary policy in §5.4). The
    /// iterator yields `Result<EventEnvelope<T>, JournalError>`. On the
    /// first corrupt or invariant-violating frame, the iterator yields
    /// exactly one `Err(...)` and becomes **fused**: all subsequent
    /// `next()` calls return `None` regardless of remaining file content.
    pub fn iter_all(&self) -> impl Iterator<Item = Result<EventEnvelope<T>, JournalError>> + '_;

    /// In-process metrics snapshot mirror.
    pub fn stats(&self) -> JournalStats;
}
```

`JournalStats` shape (mirrors Task 12's `BusStats` pattern):

```rust
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JournalStats {
    pub appended_total:        u64,
    pub read_total:            u64,
    pub bytes_written_total:   u64,
    pub corrupt_frames_total:  u64,
}
```

### 5.2 `RocksDbSnapshot`

```rust
pub struct RocksDbSnapshot { /* private */ }

impl RocksDbSnapshot {
    /// Opens (or creates) a RocksDB database at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, JournalError>;

    /// Saves a value under `key` using bincode encoding.
    /// Rejects keys starting with the reserved prefix `b"\0rust_lmax_mev:snapshot:"`
    /// with `JournalError::ReservedKey`.
    pub fn save<V: serde::Serialize>(&self, key: &[u8], value: &V)
        -> Result<(), JournalError>;

    /// Loads a value at `key`. Returns `Ok(None)` if absent.
    /// Rejects keys starting with the reserved prefix with
    /// `JournalError::ReservedKey`.
    pub fn load<V: serde::de::DeserializeOwned>(&self, key: &[u8])
        -> Result<Option<V>, JournalError>;

    /// Returns the last recorded sequence number.
    /// Returns `Err(JournalError::LastSequenceUnavailable)` if
    /// `set_last_sequence` has not yet been called. `0` is NOT used as a
    /// sentinel because `0` is a valid sequence value.
    pub fn last_sequence(&self) -> Result<u64, JournalError>;

    /// Records the last sequence; called after a snapshot is taken.
    /// Uses internal RocksDB access; bypasses the user-facing reserved-
    /// prefix rejection.
    pub fn set_last_sequence(&self, sequence: u64) -> Result<(), JournalError>;

    /// In-process metrics snapshot mirror.
    pub fn stats(&self) -> SnapshotStats;
}
```

`SnapshotStats` shape:

```rust
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotStats {
    pub saved_total:  u64,
    pub loaded_total: u64,
}
```

**Reserved keys:**
- Reserved prefix: `b"\0rust_lmax_mev:snapshot:"` — any user-supplied key starting with this prefix is rejected on `save` / `load`.
- `last_sequence` key (internal): `b"\0rust_lmax_mev:snapshot:last_sequence"` — bincode-encoded `u64`.

The leading null byte makes accidental collision with human-readable user keys effectively impossible. Future Phase 2 additions (e.g., `b"\0rust_lmax_mev:snapshot:checkpoint:..."`) can extend under the same reserved prefix without breaking existing user keys.

### 5.3 `JournalError`

```rust
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("journal io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("envelope invariant violation: {0}")]
    Types(#[from] TypesError),

    #[error("rocksdb error: {0}")]
    RocksDb(#[from] rocksdb::Error),

    #[error("rkyv error: {0}")]
    Rkyv(#[from] rkyv::rancor::Error),

    // bincode 1.3 serialize/deserialize errors share Box<bincode::ErrorKind>;
    // we cannot put #[from] on both variants (would create conflicting From impls).
    // Implementation uses map_err(JournalError::BincodeSerialize) /
    // map_err(JournalError::BincodeDeserialize) at the call sites. Variant shape:
    //   BincodeSerialize(Box<bincode::ErrorKind>),
    //   BincodeDeserialize(Box<bincode::ErrorKind>),
    // Final form (including any type alias) confirmed in Batch B.
    #[error("bincode serialize error: {0}")]
    BincodeSerialize(/* Box<bincode::ErrorKind> — Batch B; no #[from], use map_err */),

    #[error("bincode deserialize error: {0}")]
    BincodeDeserialize(/* Box<bincode::ErrorKind> — Batch B; no #[from], use map_err */),

    #[error("checksum mismatch at offset {offset}: expected={expected:#x}, found={found:#x}")]
    ChecksumMismatch { offset: u64, expected: u32, found: u32 },

    #[error("invalid frame length at offset {offset}: length={length} (max permitted={max})")]
    InvalidFrameLength { offset: u64, length: u64, max: u64 },

    #[error("truncated frame at offset {offset}: needed {needed} bytes, got {got}")]
    TruncatedFrame { offset: u64, needed: usize, got: usize },

    #[error("invalid journal file header: expected magic={expected:?}, found={found:?}")]
    InvalidFileHeader { expected: [u8; 4], found: [u8; 4] },

    #[error("truncated journal file header: file is {found} bytes, header requires 8")]
    TruncatedFileHeader { found: usize },

    #[error("unsupported journal file format version: {version}")]
    UnsupportedFileVersion { version: u8 },

    #[error("invalid reserved header bytes: found={found:?} (expected [0, 0, 0])")]
    InvalidReservedHeader { found: [u8; 3] },

    #[error("snapshot last_sequence not initialized")]
    LastSequenceUnavailable,

    #[error("snapshot key reserved: {0:?}")]
    ReservedKey(Vec<u8>),
}
```

Notes:
- `InvalidFileHeader` carries `[u8; 4]` for both expected and found — the magic is raw bytes, not an integer (§4.5).
- `InvalidReservedHeader` is distinct from `InvalidFileHeader`: magic mismatches indicate "not a journal file at all", reserved-bytes mismatches indicate "is a journal file, but the reserved field is non-conformant" (Phase 2 forward-compat). Splitting the two variants lets callers and operators distinguish "wrong file" from "wrong reserved-bytes pattern". See §4.5.
- BincodeSerialize/BincodeDeserialize do NOT use `#[from]`. Both wrap `Box<bincode::ErrorKind>`, so a blanket `From<Box<bincode::ErrorKind>> for JournalError` would be ambiguous (which variant?). Call sites use `.map_err(JournalError::BincodeSerialize)` / `.map_err(JournalError::BincodeDeserialize)` to disambiguate. Final variant shape (and any local type alias) is confirmed in Batch B.
- `InvalidFrameLength.max` will be 16 MiB (`16 * 1024 * 1024`) per §X.15.
- `ReservedKey(Vec<u8>)` — the rejected key is captured for diagnostics (rather than just a `&'static str` literal) because the user-supplied key is dynamic.
- `TruncatedFileHeader` is distinct from `TruncatedFrame` because the header truncation is an open-time failure, not a per-record iteration failure.

### 5.4 Validation boundary policy

Per `crates/types/src/lib.rs:16-23` (Task 11 crate-level docs):

> `serde::Deserialize` and `rkyv::Deserialize` reconstruct envelope fields directly, **bypassing `seal()`**. Journal, replay, and decoder consumers MUST call `EventEnvelope::validate()` immediately after any deserialization to re-enforce the same invariants `seal()` would have rejected at construction.

`FileJournal::iter_all()` is the principal Task 13 enforcement site. Every yielded `Ok(EventEnvelope<T>)` has been `validate()`-checked. Any failure yields `Err(JournalError::Types(TypesError::InvalidEnvelope { .. }))` (§5.3).

#### Test strategy: byte-patch synthesis (resolved here, finalized in Batch B)

A dedicated test (`invariant_violating_decoded_envelope_is_rejected_via_validate`, name-finalized in Batch B) verifies the `validate()` boundary. The test cannot construct an invalid `EventEnvelope` directly because envelope fields are private in `crates/types` — the only public path is `EventEnvelope::seal()`, which itself validates and rejects bad inputs.

**Strategy:** byte-patch a serialized valid envelope to flip an invariant, recompute the CRC, and confirm `iter_all` rejects via `validate()`.

```text
1. Construct a valid EventEnvelope<SmokeTestPayload> via EventEnvelope::seal()
   with timestamp_ns set to a unique sentinel value (e.g. 0xDEADBEEFDEADBEEF).
2. rkyv-serialize the envelope to bytes (`Vec<u8>`).
3. Compute the sentinel byte pattern via `sentinel.to_le_bytes()` (8 bytes).
   The test relies on this matching the rkyv archived primitive layout for
   the timestamp_ns field. Batch B confirms the actual rkyv 0.8 archived
   bytes match this pattern; if a future rkyv update changes the archived
   layout (e.g., big-endian primitives, or per-field framing), the test's
   match-count check (step 4) will trip immediately.
4. Search the rkyv bytes for the sentinel byte pattern.
   - If the pattern is found exactly once: proceed.
   - If the pattern is found 0 times or 2+ times: panic the test with a
     descriptive message ("rkyv layout changed; sentinel pattern matches=N,
     expected=1"). This panic is the test's "rkyv layout drift" alarm. A
     drift-induced failure is a signal to revisit the test strategy in
     Batch B — it is NOT a license to add a test-support helper to the
     `crates/types` crate without separate explicit user approval.
5. Patch the matched 8 bytes to all-zero (representing timestamp_ns = 0
   in the decoded envelope).
6. Compute CRC32 of the patched payload bytes.
7. Open a fresh FileJournal at a tempfile path (Batch B uses tempfile dev-dep).
8. Bypass FileJournal::append by writing directly to the underlying file:
     - file header (8 bytes magic + version + reserved)
     - [u32 length (LE) = patched_payload.len() as u32]
     - patched payload bytes
     - [u32 crc32 (LE) = computed CRC]
   This produces a frame whose CRC verifies but whose decoded envelope
   has timestamp_ns = 0.
9. Reopen the FileJournal via FileJournal::<SmokeTestPayload>::open(path).
10. Bind one iterator instance: `let mut iter = journal.iter_all();`.
11. First `iter.next()` MUST yield Some(Err(...)):
      - Outer Some, inner Err
      - Err variant is JournalError::Types(TypesError::InvalidEnvelope {
          field: "timestamp_ns", ..
        })
12. Confirm event_journal_corrupt_frames_total incremented by 1.
13. Second `iter.next()` (SAME iterator instance from step 10) MUST yield
    None (the iterator is fused after the first Err — §X.8).

Note on iterator identity: the fused-on-first-Err invariant is a property
of a single iterator INSTANCE. Calling `journal.iter_all()` again creates
a NEW iterator that re-reads the file from offset 8 and will yield the
same Err on its first next() call (same corrupt frame, same Err). Re-
calling iter_all() and seeing the Err again is NOT a fused-rule violation;
it's two independent iterator lifetimes over the same on-disk content.
```

The "bypass `append` and write the frame manually" step requires either:
- Module-internal write helpers exposed `pub(crate)` to the test, or
- A test-only helper inside `frame.rs`'s `mod tests` that re-uses the same encoding code.

Final placement decision in Batch B; the strategy itself is Batch-A-resolved.

**Fallback option (NOT used unless byte-patch proves infeasible in Batch B):** add a `pub(crate)` test-support helper to `crates/types` (e.g., `EventEnvelope::test_only_construct_unchecked(...)` behind a `cfg(test)` or feature flag). This requires modifying Task 11 (frozen) and is therefore **forbidden without separate explicit user approval**. If Batch B implementation discovers byte-patch is infeasible, escalate to user before adding any types-crate helper.

`TypesError::UnsupportedEventVersion` is **out of scope for Phase 1 journal validate** — see §X.14.

### 5.5 Public traits

**Resolved:** Phase 1 ships **no public traits**. `FileJournal<T>` and `RocksDbSnapshot` are concrete types.

Reasons:
- Phase 1 has exactly one journal impl and one snapshot impl. A trait with one impl is speculative abstraction.
- Phase 2 may introduce a `Journal` trait for replay-backend pluggability (e.g., in-memory replay for tests, S3-backed journal for archive). Adding traits then is additive; adding them now bakes assumptions into the API surface before the use cases exist.

### 5.6 Metric counter semantics

The six counters are emitted via the `metrics` facade with in-process `AtomicU64` mirrors readable through `JournalStats` / `SnapshotStats`. Emission rules:

| Counter | Increments when | Does NOT increment when |
|---|---|---|
| `event_journal_appended_total` | `FileJournal::append` succeeds (frame fully written and `JournalPosition` returned). | `append` fails at any step (rkyv error, I/O error, etc.). |
| `event_journal_read_total` | `iter_all` is about to yield `Ok(envelope)` — i.e., CRC verified, rkyv decoded, validate() passed. | `iter_all` yields `Err(...)`. |
| `event_journal_bytes_written_total` | `FileJournal::append` succeeds; the increment is the **record frame size** (`4 + length + 4`) only. | File header bytes (initial 8 bytes) are NOT counted. Failed appends are NOT counted. |
| `event_journal_corrupt_frames_total` | Any iter-time frame-level corruption: CRC mismatch, truncation (frame body or trailer), `InvalidFrameLength` (zero or > max), rkyv decode error, `validate()` failure. The increment happens at the failure detection site, BEFORE the `Err` is yielded. | Clean EOF (iterator returns `None`). Successful reads. **Open-time header errors** (`InvalidFileHeader`, `TruncatedFileHeader`, `UnsupportedFileVersion`, `InvalidReservedHeader`) are NOT counted by this counter — they occur before a `FileJournal` instance exists, so there is no in-process atomic to mirror, and the `metrics` facade is also not emitted on `open` failure. (If Phase 2 / Task 15 introduces a global open-failure counter, it lives outside this crate's `JournalStats`.) |
| `event_snapshot_saved_total` | `RocksDbSnapshot::save` succeeds (bincode encode + RocksDB put both succeed). | Reserved-key rejection. Bincode encode error. RocksDB put error. |
| `event_snapshot_loaded_total` | `RocksDbSnapshot::load` returns `Ok(Some(V))` — the bytes were present AND bincode decode succeeded. | `Ok(None)` (key absent) — no decode work was done. Bincode decode error on a present-but-corrupt entry. RocksDB get error. Reserved-key rejection. |

The atomic mirrors are bus-internal `AtomicU64` per counter, accessed with `Ordering::Relaxed` (matching Task 12's pattern for monotonic observability counters).

`current_depth`-style gauge is NOT emitted by this crate; the journal does not have a meaningful in-memory depth analog (the file is the depth, and `Sender::len()` has no equivalent).

### 5.7 Anti-patterns (forbidden in Phase 1)

- ❌ Writing or reading frames without per-record CRC32 verification.
- ❌ Skipping `EventEnvelope::validate()` after rkyv-decode.
- ❌ Mutating an existing journal record (any in-place edit, including reusing a byte range for a different envelope).
- ❌ Silently deserializing a corrupted frame as a "best-effort" envelope. Decode failures, CRC mismatches, and invariant violations MUST surface as `Err`.
- ❌ Continuing past a corrupt frame in `iter_all`. The iterator yields exactly one `Err` then becomes fused (§X.8).
- ❌ Using JSON anywhere in the journal hot path (per ADR-004 hot-path JSON ban).
- ❌ Allocating a `Vec` of `length` bytes before validating `length <= MAX_FRAME_LEN`.
- ❌ Allowing `append` to write any frame bytes before the payload-length validation in §4.4 step 3 has passed. Failed validation must leave the file untouched and counters untouched.
- ❌ Treating non-zero file-header reserved bytes as acceptable in Phase 1 (§4.5 reserved-bytes rule).
- ❌ Putting `#[from]` on both `BincodeSerialize` and `BincodeDeserialize`. Both wrap `Box<bincode::ErrorKind>`; a blanket `From` would be ambiguous. Use `.map_err(...)` at call sites (§5.3).
- ❌ Using `0` as a sentinel for "last_sequence not yet set" (§X.12).
- ❌ Allowing `save<V>` or `load<V>` to read or write any key starting with the reserved prefix (§5.2).
- ❌ Treating `flush()` as a durability barrier. `flush` is buffer→OS only (§X.10).
- ❌ Logging payload contents at `INFO` level (`TRACE` for inner-loop diagnostics per ADR-008).
- ❌ Modifying `crates/types` to add a test-support helper for invalid-envelope synthesis without explicit user approval (§5.4).

(Renumbered from §6 to §5.7 in v0.2 to keep the §5 API surface block self-contained.)

---

## X. Resolved Decisions (v0.3)

All Batch A decisions are now resolved. Each entry records the user-approved choice, the rationale, and any Batch B implementation memo.

### §X.1 — Checksum crate

**Decision:** `crc32fast = "1"`.

**Rationale:** ADR-003 says "CRC32" without qualifier; the unqualified ecosystem default is IEEE, and `crc32fast` is the canonical pure-Rust implementation (used by Cargo, gzip, png, zip).

**Batch B memo:** add `crc32fast = "1"` to `[workspace.dependencies]` and reference via `{ workspace = true }` in `crates/journal/Cargo.toml`.

### §X.2 — File-level magic + version header

**Decision:** Option B — file-level header. Magic is **raw bytes** `[u8; 4] = *b"LMEJ"` (LMAX MEV Event Journal). Version is `[u8] = 1`. Reserved is `[u8; 3] = [0; 3]`. Total file header: 8 bytes. Per-record fields are little-endian.

**Rationale:** Catches "wrong file format" early at `open()`; reserves a 1-byte version field for Phase 2 evolution at zero per-record cost. Raw-byte magic is unambiguous (no endianness debate) and aligns with how the implementation will compare bytes literally rather than converting to a `u32`.

**Batch B memo:** the `frame.rs` module exports `pub(crate) const MAGIC: [u8; 4] = *b"LMEJ";` and `pub(crate) const FILE_FORMAT_VERSION: u8 = 1;`. `open()` reads exactly 8 bytes into a fixed-size buffer; literal comparison with `MAGIC` produces `InvalidFileHeader` on mismatch.

### §X.3 — Read API shape (Phase 1)

**Decision:** `iter_all()` only. `read_at(JournalPosition)` is deferred to Phase 2 as an additive API.

**Rationale:** Phase 1 smoke (CI check #6 per ADR-008) needs round-trip; replay engine is Phase 2 work and will own the position-bookkeeping policy. `append` returns `JournalPosition` so audit logs and Phase 2 `read_at` have the per-record byte offset already recorded.

**Batch B memo:** keep `JournalPosition.byte_offset` populated even though Phase 1 `iter_all` does not consume it. The position is a Phase 2 affordance baked into the Phase 1 API to avoid a breaking change later.

### §X.4 — Bincode 1.3 vs 2.x

**Decision:** Keep `bincode = "1.3"`. Reconcile ADR-004 wording in a separate small docs commit.

**ADR-004 fix wording (single-line edit):** ADR-004 Consequences currently reads "Snapshot types must derive `bincode::Encode` and `bincode::Decode`." The fix replaces it with "Snapshot types must implement `serde::Serialize` and `serde::Deserialize`, encoded via the `bincode` 1.x serde adapter (`bincode::serialize` / `bincode::deserialize`)."

The ADR-004 fix lands as an independently committable single-file docs commit (suggested commit subject: `docs(adr): fix ADR-004 bincode wording for 1.x serde adapter`). It is NOT bundled with the Task 13 crate scaffold; it lands separately so the docs commit history remains clean.

**Rationale:** bincode 2.x migration would touch Task 11 (frozen) test code via the serde-vs-Encode/Decode API split. The serde adapter is a real, stable, idiomatic encoding path; the original ADR-004 wording was over-specification.

**Batch B memo:** snapshot `save<V>` / `load<V>` use `bincode::serialize` / `bincode::deserialize` (the 1.3 serde adapter API). `JournalError::BincodeSerialize` and `BincodeDeserialize` wrap `Box<bincode::ErrorKind>` (the 1.3 error type).

### §X.5 — Journal metrics in-process atomic mirror

**Decision:** Mirror counters as `AtomicU64`; expose via `JournalStats` / `SnapshotStats` snapshot structs.

**Counter naming (resolved to namespace-safe form):**
- `event_journal_appended_total`
- `event_journal_read_total`
- `event_journal_bytes_written_total`
- `event_journal_corrupt_frames_total`
- `event_snapshot_saved_total` (renamed from `snapshot_save_total`)
- `event_snapshot_loaded_total` (renamed from `snapshot_load_total`)

**Rationale:** `event_*` prefix matches the Task 12 convention (`event_bus_*`) and avoids the over-generic `snapshot_save_total` namespace clash with future cross-cutting `snapshot_*` metrics.

**Batch B memo:** counter atomics are private to each impl (`FileJournal` owns 4, `RocksDbSnapshot` owns 2). Stats structs use `#[non_exhaustive]` per Task 12 policy. No labels on any counter (Phase 1 policy).

### §X.6 — `FileJournal<T>` generic structure

**Decision:** Generic struct `FileJournal<T>`. One journal = one type. Type-safe at compile time.

**Batch B memo:** the struct will need a `PhantomData<T>` private field because `T` does not appear non-phantomly in the struct (the file holds bytes, and there is no in-memory `T`-typed channel). Without `PhantomData<T>`, the compiler will reject the unused-type-parameter.

### §X.7 — Phase 1 snapshot value shape

**Decision:** Generic `<V: serde::Serialize + serde::de::DeserializeOwned>` API. Bincode encoding lives once inside the snapshot layer. Phase 1 smoke uses `SmokeTestPayload` (Task 11) as `V`.

**Rationale:** Cleanest user-facing surface; no new domain types required.

### §X.8 — Iterator yield semantics on bad records

**Decision:** Iterator yields **exactly one `Err`** for the first corrupt or invariant-violating frame, then becomes **fused** — all subsequent `next()` calls return `None`.

**Rationale:** Phase 1 journal is a sequential ordered log; a bad record means the file is corrupt and continuing past it is a recovery problem (Phase 2). Fusing after the first error gives callers a clean signal: collect the one error, stop iterating, decide what to do at a higher layer.

**Batch B memo:** the iterator type carries a private `fused: bool` flag. Set on first `Err`. Tested via `corrupt_then_next_returns_none_on_same_iterator_instance` in Batch B (renamed from v0.2's `corrupt_then_next_returns_none_fused` to make the iterator-instance scoping explicit). Re-calling `journal.iter_all()` produces a fresh iterator whose first `next()` will re-yield the same Err — that is not a fused-rule violation; the rule applies per-instance.

### §X.9 — CRC32 polynomial

**Decision:** IEEE CRC32 (default `crc32fast` polynomial).

**Rationale:** Matches the unqualified "CRC32" wording in ADR-003. CRC32C migration is a Phase 2 performance optimization gated on benchmark proof.

### §X.10 — Append fsync policy

**Decision:** `append()` does NOT fsync. `flush()` flushes buffered bytes to the underlying file handle/OS only — it does NOT call `sync_all` / `sync_data`. Crash-durable fsync is Phase 4 reliability work.

**Wording rule:** documentation MUST NOT describe `flush()` as "flushes to disk" or imply durability. The accurate wording is "flushes buffered bytes to the underlying file handle/OS." Tests must call `flush()` before `iter_all()` in the same process to ensure the OS sees the buffered writes; this is a process-local visibility guarantee, not a durability guarantee.

**Rationale:** Phase 1 smoke is in-process round-trip; durability across crashes is out of scope. `BufWriter` semantics are well understood; `sync_all` / `sync_data` are explicit follow-ons that callers can add when reliability work begins.

**Batch B memo:** `flush()` calls `BufWriter::flush()` then propagates the result. No `sync_all` / `sync_data` call anywhere in Phase 1 journal code.

### §X.11 — File layout: split modules from start

**Decision:** Split modules from start.

```
crates/journal/src/
├── lib.rs      # public re-exports + crate-level docstring + module declarations
├── error.rs    # JournalError
├── frame.rs    # frame encode/decode + CRC32 helper + file header constants
├── journal.rs  # FileJournal<T> + JournalStats
└── snapshot.rs # RocksDbSnapshot + SnapshotStats
```

**Test placement rule:** tests that touch a module's private helpers (e.g., raw byte-patch tests in `frame.rs`, white-box state inspection in `journal.rs`) MUST live inside that module's `#[cfg(test)] mod tests` block. There is no shared `tests.rs` sibling.

**Rationale:** (See §4.1.) Estimated LOC over the single-file 600 threshold; module split clarifies invariants per module.

### §X.12 — `last_sequence()` before any `set_last_sequence`

**Decision:** `Err(JournalError::LastSequenceUnavailable)`. `0` is NOT used as a sentinel because `0` is a valid sequence value (Task 11 sequences start at 0).

**Rationale:** Explicit error variant is unambiguous; sentinel collision with valid data is forbidden.

### §X.13 — `proptest`-driven property tests

**Decision:** `proptest` deferred to Task 17 (smoke tests crate). Task 13 ships unit + targeted failure-injection tests only.

**Rationale:** Task 17 explicitly owns the smoke-test workload (per CLAUDE.md task list "Task 17: Integration smoke tests (100k bus, journal round-trip, snapshot)"). Task 13 ships the primitives + targeted invariant tests; Task 17 layers proptest on top.

### §X.14 — `TypesError::UnsupportedEventVersion` raise path

**Decision:** Phase 1 journal does NOT raise `TypesError::UnsupportedEventVersion`. The variant is reserved for the Phase 2 replay engine when cross-version skew between writer and reader becomes a real concern.

**Phase 1 observability:** Task 11's `EventEnvelope::validate()` (`crates/types/src/lib.rs:214-220`) checks only `timestamp_ns != 0`, `event_version != 0`, and `chain_context.chain_id != 0` — all three failure modes route to `TypesError::InvalidEnvelope`. The `UnsupportedEventVersion` variant exists in the enum but is not raised by any code path Task 11 ships, and Task 13 inherits this.

**Phase 1 events all use `event_version = 1`** (the Phase 1 default; Task 11 spec §3). Cross-version journal entries do not occur in Phase 1 traffic.

**Phase 2 plan:** the Phase 2 replay engine will add a version-skew check — likely a wrapper around `iter_all` that compares `envelope.event_version()` against a reader-supplied `max_supported` and raises `UnsupportedEventVersion` for newer-than-supported entries. This wrapper is not Task 13's responsibility.

**Resolution of v0.1 §3 incorrect cross-reference:** v0.1 §3 (Out of scope) referenced "Q11-related discussion" for `UnsupportedEventVersion`, which was wrong (Q11 is file layout). The correct cross-reference is this §X.14, applied in v0.2's §3 row.

### §X.15 — Maximum frame length

**Decision:** `MAX_FRAME_LEN = 16 * 1024 * 1024` (16 MiB) for Phase 1.

`length == 0` is rejected with `InvalidFrameLength` because a zero-length rkyv payload cannot decode a valid `EventEnvelope`.
`length > MAX_FRAME_LEN` is rejected with `InvalidFrameLength` BEFORE the implementation attempts to allocate or read the payload (allocation-DoS protection).

**Rationale:**
- 16 MiB is comfortably above any plausible Phase 1 envelope size (`SmokeTestPayload` is 40 bytes; even with future complex domain payloads, individual envelopes far exceeding 16 MiB indicate a design error elsewhere).
- The cap is a safety net against corrupted length prefixes triggering `Vec::with_capacity(corrupt_length)` on the read path.

**Batch B memo:** export `pub(crate) const MAX_FRAME_LEN: u64 = 16 * 1024 * 1024;` from `frame.rs`. Length validation in `iter_all` happens immediately after reading the length prefix, before any payload allocation.

---

## References

- **ADR-003** — Mempool/Relay/Persistence. Append-only journal, RocksDB snapshot, rkyv frame format with 4-byte length prefix and CRC32 per record. "Journal writer must never block event processing" rule reconciled in §4.6.
- **ADR-004** — RPC/EVM stack. rkyv hot-path / bincode cold-path serialization. ADR-004 Consequences wording fix lands in a separate small docs commit per §X.4.
- **ADR-008** — Observability + CI baseline. CI checks #6 (journal round-trip) and #7 (snapshot smoke) target this crate's Task 17 smoke harness.
- **`docs/superpowers/specs/2026-04-27-task-11-types-crate-design.md`** — Task 11 §11.1 deferred ADR-004 bincode wording reconciliation to Task 13 entry; the per-event-version policy (`event_version != 0`) governs Phase 1 envelope decode validity.
- **`docs/superpowers/specs/2026-04-29-task-12-event-bus-design.md`** — Task 12 `#[non_exhaustive]` policy (carried into `JournalError`/`JournalStats`/`SnapshotStats`); augmented D10 `--all-targets` clippy preview gate (carried into Task 13 DoD per Batch C).
- **`crates/types/src/lib.rs:16-23`** — Crate-level docstring mandating `validate()` at deserialize boundaries; binding contract for §5.4.
- **`crates/types/src/lib.rs:82-85`** — `JournalPosition { sequence: u64, byte_offset: u64 }` shape (already shipped in Task 11).
- **`crates/types/src/lib.rs:214-220`** — `EventEnvelope::validate()` body; raises only `InvalidEnvelope`, never `UnsupportedEventVersion` (§X.14).
- **`CLAUDE.md`** — Phase 1 task list; `tempfile = "3"` dev-dep convention; `proptest = "1.4"` workspace dep availability.

---

## Batch B — Implementation Detail (drafted 2026-05-01)

This section concretizes the implementation-level details deferred from Batch A. Decisions in §1–§X are NOT revisited; Batch B is purely the implementation specification consistent with those decisions. Code blocks below are pseudo-Rust signatures and bodies illustrating the contract; the actual implementation may rearrange variable names and intermediate locals as long as externally observable behavior matches.

### B.1 — Frame internals

#### B.1.1 Constants (in `frame.rs`)

```rust
pub(crate) const MAGIC: [u8; 4] = *b"LMEJ";              // raw bytes; not interpreted as u32
pub(crate) const FILE_FORMAT_VERSION: u8 = 1;
pub(crate) const RESERVED_HEADER: [u8; 3] = [0, 0, 0];
pub(crate) const FILE_HEADER_LEN: usize = 8;             // 4 magic + 1 version + 3 reserved
pub(crate) const MAX_FRAME_LEN: u64 = 16 * 1024 * 1024;  // 16 MiB; per §X.15
pub(crate) const FRAME_OVERHEAD: usize = 4 + 4;          // length prefix + CRC trailer
```

#### B.1.2 File header byte layout

| offset | size | field                       | encoding              |
|-------:|-----:|-----------------------------|-----------------------|
|   0    |  4   | magic = `*b"LMEJ"`          | raw bytes (no endian) |
|   4    |  1   | file_format_version = 1     | u8                    |
|   5    |  3   | reserved = `[0, 0, 0]`      | raw bytes             |

Total: 8 bytes. Written once at file creation; verified once per `open()` against an existing non-empty file.

#### B.1.3 Per-record byte layout

| offset(rel)   | size   | field                          | encoding              |
|--------------:|-------:|--------------------------------|-----------------------|
| 0             | 4      | length = `payload.len() as u32`| u32 little-endian     |
| 4             | length | rkyv-encoded `EventEnvelope<T>`| rkyv 0.8              |
| 4 + length    | 4      | crc32 = `crc32fast::hash(payload)` | u32 little-endian |

Total per record: `4 + length + 4` bytes. The first record begins at file offset `FILE_HEADER_LEN` (= 8).

`length` invariant: `1 <= length <= MAX_FRAME_LEN`. Enforced on BOTH the write (append) and read (iter_all) paths per §X.15.

#### B.1.4 Private helper signatures (`frame.rs`)

```rust
/// Writes the 8-byte file header at the writer's current position.
/// Used by FileJournal::open when creating a new (or empty existing) file.
pub(crate) fn write_file_header<W: std::io::Write>(w: &mut W) -> std::io::Result<()>;

/// Reads and validates the 8-byte file header from `r`. Returns `Ok(())` on
/// match; otherwise returns the appropriate JournalError variant.
pub(crate) fn read_and_validate_file_header<R: std::io::Read>(
    r: &mut R,
) -> Result<(), JournalError>;

/// Validates that a payload length is within the permitted Phase 1 range.
/// Used at both append-time (pre-write) and iter-time (post-length-read).
/// `offset` is the byte offset of the frame's length-field start (for diagnostics).
pub(crate) fn validate_frame_length(length: u64, offset: u64) -> Result<(), JournalError>;
```

Note: there is intentionally no `encode_frame(payload) -> Vec<u8>` helper. `FileJournal::append` writes the three pieces (`length_le`, `payload`, `crc_le`) directly through the buffered writer to avoid a second allocation.

The unit test `frame_encode_rejects_zero_length_payload` (per §5.4 / Batch A) targets `validate_frame_length(0, _)` directly, since the public `FileJournal::append` cannot exercise the zero-length branch (rkyv produces non-zero bytes for any valid envelope).

### B.2 — Method data flow

#### B.2.1 `FileJournal::open`

```rust
pub fn open(path: impl AsRef<Path>) -> Result<Self, JournalError> {
    let path = path.as_ref().to_path_buf();
    let mut file = std::fs::OpenOptions::new()
        .read(true).write(true).create(true)
        .open(&path)?;

    let len = file.metadata()?.len();

    if len == 0 {
        // Path absent OR existing-but-empty: write fresh header.
        write_file_header(&mut file)?;
        // file cursor is now at offset FILE_HEADER_LEN (8)
    } else if len < FILE_HEADER_LEN as u64 {
        return Err(JournalError::TruncatedFileHeader { found: len as usize });
    } else {
        file.seek(SeekFrom::Start(0))?;
        read_and_validate_file_header(&mut file)?;
        // On success, the writer must be repositioned at end-of-file for append.
        file.seek(SeekFrom::End(0))?;
    }

    let next_byte_offset = file.metadata()?.len();
    let writer = std::io::BufWriter::new(file);

    Ok(FileJournal {
        path,
        writer,
        next_byte_offset,
        appended_total: AtomicU64::new(0),
        read_total: Arc::new(AtomicU64::new(0)),
        bytes_written_total: AtomicU64::new(0),
        corrupt_frames_total: Arc::new(AtomicU64::new(0)),
        _phantom: PhantomData,
    })
}
```

Failure modes:
- `Io(_)` from `OpenOptions::open`, `metadata`, `seek`, or `write_file_header`.
- `TruncatedFileHeader { found }` when `1 <= len < FILE_HEADER_LEN`.
- `InvalidFileHeader { expected, found }` from `read_and_validate_file_header` when magic mismatches.
- `UnsupportedFileVersion { version }` when `version != FILE_FORMAT_VERSION`.
- `InvalidReservedHeader { found }` when reserved != `[0, 0, 0]`.

None of the above failures increment `event_journal_corrupt_frames_total` (open-time errors are NOT counted; see §5.6).

#### B.2.2 `FileJournal::append`

```rust
pub fn append(
    &mut self,
    envelope: &EventEnvelope<T>,
) -> Result<JournalPosition, JournalError> {
    // Step 1: rkyv-serialize the envelope. May fail with Rkyv(_).
    let payload_bytes = rkyv::to_bytes::<rkyv::rancor::Error>(envelope)?;
    let payload: &[u8] = payload_bytes.as_ref();   // exact rkyv 0.8 form may vary; see B.6

    // Step 2: validate length BEFORE any file write. May fail with InvalidFrameLength.
    let len_u64 = payload.len() as u64;
    validate_frame_length(len_u64, self.next_byte_offset)?;
    let length_u32 = payload.len() as u32; // safe: payload.len() <= MAX_FRAME_LEN < u32::MAX

    let frame_start = self.next_byte_offset;

    // Step 3: compute CRC.
    let crc = crc32fast::hash(payload);

    // Step 4: write [length_le][payload][crc_le] to the buffered writer.
    // No fsync; no sync_all. Caller must call flush() before reading.
    self.writer.write_all(&length_u32.to_le_bytes())?;
    self.writer.write_all(payload)?;
    self.writer.write_all(&crc.to_le_bytes())?;

    // Step 5: update tracked offset (success path only).
    let frame_size = (FRAME_OVERHEAD + payload.len()) as u64;
    self.next_byte_offset = frame_start + frame_size;

    // Step 6: increment counters (success path only).
    self.appended_total.fetch_add(1, Ordering::Relaxed);
    self.bytes_written_total.fetch_add(frame_size, Ordering::Relaxed);
    metrics::counter!("event_journal_appended_total").increment(1);
    metrics::counter!("event_journal_bytes_written_total").increment(frame_size);

    // Step 7: return position.
    Ok(JournalPosition {
        sequence: envelope.sequence(),
        byte_offset: frame_start,
    })
}
```

Failure-mode notes:
- `Rkyv(_)` from step 1 — counters NOT incremented, file untouched.
- `InvalidFrameLength { offset, length, max }` from step 2 — counters NOT incremented, file untouched.
- `Io(_)` from any `write_all` in step 4 — partial bytes may have entered the BufWriter (and possibly flushed under buffer pressure to the OS); counters NOT incremented, `self.next_byte_offset` NOT advanced. The trailing partial bytes will surface on a subsequent `iter_all` as `TruncatedFrame` or `ChecksumMismatch` per the §4.4 partial-write policy.

The `next_byte_offset` advancement (step 5) sits AFTER all writes — same retry-safety pattern as Task 12 `EventBus::publish` step 7 ("success-only sequence advance"). On any error in steps 1–4, `next_byte_offset` is preserved so the next `append` reports a consistent `frame_start` for diagnostics.

#### B.2.3 `FileJournal::flush`

```rust
pub fn flush(&mut self) -> Result<(), JournalError> {
    self.writer.flush()?;          // BufWriter -> File handle (= OS)
    Ok(())                         // NO sync_all / sync_data; per §X.10 wording rule
}
```

Process-local visibility only. Crash durability is Phase 4 reliability work.

#### B.2.4 `FileJournal::iter_all` + `JournalIter::next`

`iter_all` returns a fresh iterator that opens its OWN read handle on the underlying file path. Each call produces an independent iterator that re-reads from offset `FILE_HEADER_LEN`. Per the §X.8 fused-on-first-Err contract and the Batch B memo near line 800 of this spec.

```rust
pub fn iter_all(&self) -> impl Iterator<Item = Result<EventEnvelope<T>, JournalError>> + '_ {
    JournalIter::<T>::new(
        self.path.clone(),
        Arc::clone(&self.read_total),
        Arc::clone(&self.corrupt_frames_total),
    )
}

pub(crate) struct JournalIter<T> {
    file: Option<std::io::BufReader<std::fs::File>>,  // None if open() failed in new()
    offset: u64,
    fused: bool,
    read_total: Arc<AtomicU64>,
    corrupt_frames_total: Arc<AtomicU64>,
    _phantom: PhantomData<T>,
}

impl<T> JournalIter<T> {
    fn new(
        path: PathBuf,
        read_total: Arc<AtomicU64>,
        corrupt_frames_total: Arc<AtomicU64>,
    ) -> Self {
        let file = std::fs::File::open(&path).ok().map(|f| {
            let mut reader = std::io::BufReader::new(f);
            // Skip the file header. open() guaranteed it is valid (we own the path).
            let _ = reader.seek(SeekFrom::Start(FILE_HEADER_LEN as u64));
            reader
        });
        Self {
            file,
            offset: FILE_HEADER_LEN as u64,
            fused: false,
            read_total,
            corrupt_frames_total,
            _phantom: PhantomData,
        }
    }

    /// Helper: increment corrupt counter (atomic + metric facade) and fuse.
    fn fail(&mut self, err: JournalError) -> Option<Result<EventEnvelope<T>, JournalError>> {
        self.corrupt_frames_total.fetch_add(1, Ordering::Relaxed);
        metrics::counter!("event_journal_corrupt_frames_total").increment(1);
        self.fused = true;
        Some(Err(err))
    }
}

impl<T> Iterator for JournalIter<T>
where
    T: /* rkyv 0.8 deserialize bounds — see B.6 */,
{
    type Item = Result<EventEnvelope<T>, JournalError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.fused { return None; }
        let file = self.file.as_mut()?; // open failure → empty iterator

        let frame_start = self.offset;

        // Step a: read 4-byte length prefix.
        //
        // Critical distinction at the record boundary:
        //   - 0 bytes available BEFORE we read anything → clean EOF; return None.
        //   - 1, 2, or 3 bytes available, then EOF → the file is truncated
        //     inside the length prefix. Yield exactly one
        //     JournalError::TruncatedFrame { offset, needed: 4, got: <0..4> },
        //     increment event_journal_corrupt_frames_total via fail(), and fuse.
        //
        // We cannot use `read_exact` because it returns UnexpectedEof for both
        // cases without telling us how many bytes were read. Instead we loop
        // with `read` until we have 4 bytes or hit EOF, tracking the byte count.
        let mut len_buf = [0u8; 4];
        let mut len_filled = 0usize;
        while len_filled < 4 {
            match file.read(&mut len_buf[len_filled..]) {
                Ok(0) => {
                    // EOF reached.
                    if len_filled == 0 {
                        // Clean EOF at the record boundary.
                        return None;
                    }
                    // Partial length-prefix EOF: 1, 2, or 3 bytes seen, then EOF.
                    // This is a truncated frame and counts as corruption.
                    return self.fail(JournalError::TruncatedFrame {
                        offset: frame_start,
                        needed: 4,
                        got: len_filled,
                    });
                }
                Ok(n) => len_filled += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return self.fail(JournalError::Io(e)),
            }
        }
        let length = u32::from_le_bytes(len_buf) as u64;

        // Step b: validate length.
        if let Err(e) = validate_frame_length(length, frame_start) {
            return self.fail(e);
        }

        // Step c: read the payload.
        let mut payload = vec![0u8; length as usize];
        match file.read_exact(&mut payload) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return self.fail(JournalError::TruncatedFrame {
                    offset: frame_start,
                    needed: length as usize,
                    got: 0, // exact got computed by impl from BufReader internals
                });
            }
            Err(e) => return self.fail(JournalError::Io(e)),
        }

        // Step d: read the 4-byte CRC trailer.
        let mut crc_buf = [0u8; 4];
        match file.read_exact(&mut crc_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return self.fail(JournalError::TruncatedFrame {
                    offset: frame_start,
                    needed: 4,
                    got: 0,
                });
            }
            Err(e) => return self.fail(JournalError::Io(e)),
        }
        let expected_crc = u32::from_le_bytes(crc_buf);

        // Step e: verify CRC.
        let actual_crc = crc32fast::hash(&payload);
        if actual_crc != expected_crc {
            return self.fail(JournalError::ChecksumMismatch {
                offset: frame_start,
                expected: expected_crc,
                found: actual_crc,
            });
        }

        // Step f: rkyv-decode.
        let envelope = match rkyv::from_bytes::<EventEnvelope<T>, rkyv::rancor::Error>(&payload) {
            Ok(e) => e,
            Err(e) => return self.fail(JournalError::Rkyv(e)),
        };

        // Step g: validate decoded envelope (Task 11 boundary; mandatory).
        if let Err(e) = envelope.validate() {
            return self.fail(JournalError::Types(e));
        }

        // Step h: advance offset, increment success counters, yield.
        self.offset = frame_start + 4 + length + 4;
        self.read_total.fetch_add(1, Ordering::Relaxed);
        metrics::counter!("event_journal_read_total").increment(1);
        Some(Ok(envelope))
    }
}
```

Notes:
- The fused-on-first-Err invariant (§X.8) is enforced by the `if self.fused { return None }` guard at the top of `next()` plus the `self.fused = true` assignment inside `fail()`.
- Re-calling `journal.iter_all()` produces a NEW `JournalIter` with `fused = false` and `offset = FILE_HEADER_LEN`. This re-yields the same `Err` for the same on-disk corruption — that is intentional and not a fused-rule violation (per §5.4 step-13 note).
- The shared `Arc<AtomicU64>` for `read_total` and `corrupt_frames_total` mirrors Task 12's `consumed_total` pattern: the parent `FileJournal` and its independent iterators all observe the same counter values via `stats()`.

#### B.2.5 `RocksDbSnapshot::save`

```rust
pub(crate) const RESERVED_KEY_PREFIX: &[u8] = b"\0rust_lmax_mev:snapshot:";

pub fn save<V: serde::Serialize>(
    &self,
    key: &[u8],
    value: &V,
) -> Result<(), JournalError> {
    if key.starts_with(RESERVED_KEY_PREFIX) {
        return Err(JournalError::ReservedKey(key.to_vec()));
    }
    let bytes = bincode::serialize(value)
        .map_err(JournalError::BincodeSerialize)?;   // no #[from] on bincode variants
    self.db.put(key, &bytes)?;                       // rocksdb::Error -> JournalError::RocksDb
    self.saved_total.fetch_add(1, Ordering::Relaxed);
    metrics::counter!("event_snapshot_saved_total").increment(1);
    Ok(())
}
```

#### B.2.6 `RocksDbSnapshot::load`

```rust
pub fn load<V: serde::de::DeserializeOwned>(
    &self,
    key: &[u8],
) -> Result<Option<V>, JournalError> {
    if key.starts_with(RESERVED_KEY_PREFIX) {
        return Err(JournalError::ReservedKey(key.to_vec()));
    }
    let opt_bytes = self.db.get(key)?;
    let Some(bytes) = opt_bytes else { return Ok(None); };
    let value: V = bincode::deserialize(&bytes)
        .map_err(JournalError::BincodeDeserialize)?;
    self.loaded_total.fetch_add(1, Ordering::Relaxed);
    metrics::counter!("event_snapshot_loaded_total").increment(1);
    Ok(Some(value))
}
```

Note: `Ok(None)` does NOT increment `loaded_total`. Only successful Some-decode does. Per §5.6 / B.3.

#### B.2.7 `RocksDbSnapshot::last_sequence` + `set_last_sequence`

```rust
pub(crate) const LAST_SEQUENCE_KEY: &[u8] = b"\0rust_lmax_mev:snapshot:last_sequence";

pub fn last_sequence(&self) -> Result<u64, JournalError> {
    let bytes = self.db.get(LAST_SEQUENCE_KEY)?
        .ok_or(JournalError::LastSequenceUnavailable)?;
    let seq: u64 = bincode::deserialize(&bytes)
        .map_err(JournalError::BincodeDeserialize)?;
    Ok(seq)
}

/// Bypasses the user-facing reserved-prefix rejection because LAST_SEQUENCE_KEY
/// is the canonical Phase 1 reserved key by design.
pub fn set_last_sequence(&self, sequence: u64) -> Result<(), JournalError> {
    let bytes = bincode::serialize(&sequence)
        .map_err(JournalError::BincodeSerialize)?;
    self.db.put(LAST_SEQUENCE_KEY, &bytes)?;
    Ok(())
}
```

`set_last_sequence` does NOT increment `event_snapshot_saved_total` — the "saved" counter tracks user-data writes, while `last_sequence` is bookkeeping. (Codex/user can flag if they want it counted; current design treats them as orthogonal.)

### B.3 — Metrics emission map (call-site granular)

| Counter | Site | Increment | Trigger |
|---|---|---:|---|
| `event_journal_appended_total` | `journal.rs::FileJournal::append` step 6 | 1 | Steps 1–4 succeed (rkyv encode + length validate + 3× write_all). |
| `event_journal_bytes_written_total` | `journal.rs::FileJournal::append` step 6 | `frame_size = 4 + length + 4` | Same condition as above. **File-header bytes (8) are NOT counted.** |
| `event_journal_read_total` | `journal.rs::JournalIter::next` step h | 1 | All of: length read, length validate, payload read, CRC read, CRC verify, rkyv decode, `validate()` succeed. |
| `event_journal_corrupt_frames_total` | `journal.rs::JournalIter::fail()` helper, called from steps a/b/c/d/e/f/g | 1 per occurrence | Any iter-time corruption: `Io` mid-frame, length out of `1..=MAX_FRAME_LEN`, truncated payload, truncated CRC, CRC mismatch, rkyv decode failure, `validate()` failure. |
| `event_snapshot_saved_total` | `snapshot.rs::RocksDbSnapshot::save` post-`db.put` | 1 | Reserved-key check + bincode encode + RocksDB put all succeed. |
| `event_snapshot_loaded_total` | `snapshot.rs::RocksDbSnapshot::load` post-bincode-decode | 1 | RocksDB returned `Some` AND bincode decode succeeded. `Ok(None)` does NOT increment. Reserved-key rejection does NOT increment. RocksDB error does NOT increment. Bincode decode error on present-but-corrupt entry does NOT increment. |

Open-time `FileJournal::open` errors and snapshot key-validation errors do NOT increment any counter (no instance to count on yet, or pure-validation rejection). Per §5.6.

### B.4 — Error matrix (all 15 `JournalError` variants)

| # | Variant | Emit site | Trigger | Counter incremented? | Caller action |
|--:|---|---|---|---|---|
| 1 | `Io(io::Error)` | various — `open` (mid-validation), `append` (writes), `iter_all` (mid-frame reads), `flush`, snapshot ops | underlying `std::io::Error` from filesystem | iter-time only → `corrupt_frames_total` | Inspect `io::Error` cause; retry or escalate to operator. |
| 2 | `Types(TypesError)` | `JournalIter::next` step g (post-decode validate) | `EventEnvelope::validate()` rejects (timestamp_ns=0, event_version=0, or chain_id=0) | iter-time → `corrupt_frames_total` | Reject record; data corruption or wrong-Phase file. |
| 3 | `RocksDb(rocksdb::Error)` | snapshot ops (`open`, `save`, `load`, `last_sequence`, `set_last_sequence`) | underlying `rocksdb::Error` | NO | Inspect RocksDB; check filesystem permissions / disk space. |
| 4 | `Rkyv(rkyv::rancor::Error)` | `append` step 1 (encode) AND `JournalIter::next` step f (decode) | rkyv 0.8 serialization or deserialization failure | append: NO; iter: `corrupt_frames_total` | Append: check `T` rkyv impl. Iter: data corruption. |
| 5 | `BincodeSerialize(Box<bincode::ErrorKind>)` | `RocksDbSnapshot::save` (encode V) AND `set_last_sequence` (encode u64) | bincode 1.3 serialize failure | NO | Programming bug in `V`'s `Serialize` impl. |
| 6 | `BincodeDeserialize(Box<bincode::ErrorKind>)` | `RocksDbSnapshot::load` (decode V) AND `last_sequence` (decode u64) | bincode 1.3 deserialize failure on stored bytes | NO | Snapshot data corruption or schema mismatch with prior writer. |
| 7 | `ChecksumMismatch { offset, expected, found }` | `JournalIter::next` step e | `crc32fast::hash(payload)` differs from stored CRC | iter-time → `corrupt_frames_total` | Reject record; data corruption. Iterator fuses. |
| 8 | `InvalidFrameLength { offset, length, max }` | `FileJournal::append` step 2 (`validate_frame_length` pre-write) AND `JournalIter::next` step b (post-length-read) | `length == 0` OR `length > MAX_FRAME_LEN` (16 MiB) | append: NO (counters not yet incremented); iter: `corrupt_frames_total` | Append: caller-side bug or oversize payload — fix the caller. Iter: data corruption or wrong-Phase file. Iterator fuses. |
| 9 | `TruncatedFrame { offset, needed, got }` | `JournalIter::next` step a (length-prefix partial EOF: 1–3 bytes seen, then EOF), step c (payload read), AND step d (CRC read) | EOF reached mid-frame at any of: length prefix (1–3 bytes in), payload body, or CRC trailer | iter-time → `corrupt_frames_total` | File truncated mid-record (crash during append, partial transfer, etc.). Iterator fuses. **0-byte EOF at record boundary** (step a, `len_filled == 0`) is clean EOF and yields `None` — NOT this variant; counter NOT incremented. |
| 10 | `InvalidFileHeader { expected: [u8;4], found: [u8;4] }` | `FileJournal::open` (during `read_and_validate_file_header`) | magic 4-byte block != `*b"LMEJ"` | NO (open-time) | Wrong file format. |
| 11 | `TruncatedFileHeader { found }` | `FileJournal::open` (post-len check) | existing file len in `1..FILE_HEADER_LEN` | NO (open-time) | File truncated; cannot recover via Phase 1. |
| 12 | `UnsupportedFileVersion { version }` | `FileJournal::open` (during `read_and_validate_file_header`) | `version != FILE_FORMAT_VERSION` (currently `1`) | NO (open-time) | Phase 2+ or future-format file. |
| 13 | `InvalidReservedHeader { found: [u8;3] }` | `FileJournal::open` (during `read_and_validate_file_header`) | reserved bytes != `[0, 0, 0]` | NO (open-time) | Reserved-bytes pattern claims Phase 2 capability; refuse. |
| 14 | `LastSequenceUnavailable` | `RocksDbSnapshot::last_sequence` | reserved key absent (no prior `set_last_sequence` call) | NO | Snapshot not initialized; caller decides default policy. |
| 15 | `ReservedKey(Vec<u8>)` | `RocksDbSnapshot::save` AND `load` (pre-RocksDB call) | user-supplied key starts with `RESERVED_KEY_PREFIX` | NO | Caller bug; rename the key. |

Note: Batch A's "Deferred to Batch B" list mentioned "all 14 variants" — that figure was off-by-one. Actual count is 15 (the matrix above is exhaustive).

### B.5 — Named test plan

All tests live inline in their respective module's `#[cfg(test)] mod tests` block per §X.11. Tests touching private helpers (e.g., direct byte-patch into the file, white-box state inspection of `JournalIter::fused`) MUST live in the SAME module as their target. Cross-module tests using only public API may live in any module.

#### B.5.1 `frame.rs` tests (private helper coverage)

| # | Test | Discipline | Covers |
|---|---|---|---|
| F-1 | `frame_encode_rejects_zero_length_payload` | TDD red→green | `validate_frame_length(0, _)` returns `InvalidFrameLength { length: 0 }`. |
| F-2 | `frame_encode_rejects_oversized_payload` | test-first verification | `validate_frame_length(MAX_FRAME_LEN + 1, _)` returns `InvalidFrameLength`. |
| F-3 | `frame_encode_accepts_boundary_lengths` | test-first verification | `validate_frame_length(1, _)` and `validate_frame_length(MAX_FRAME_LEN, _)` both return `Ok(())`. |
| F-4 | `write_then_read_file_header_round_trip` | TDD red→green | Write header to `Vec<u8>`, read back, confirm `Ok(())`. Plus mutation cases: bad magic → `InvalidFileHeader`; bad version → `UnsupportedFileVersion`; non-zero reserved → `InvalidReservedHeader`. |

#### B.5.2 `journal.rs::FileJournal` tests

Tempfile-backed (`tempfile = "3"` dev-dep). Each test gets a fresh `tempfile::tempdir()` for isolation.

| # | Test | Discipline | Covers |
|---|---|---|---|
| J-1 | `open_creates_journal_with_valid_header` | TDD red→green | Path absent → `open()` creates file, writes 8-byte header, file size == 8 after close. |
| J-2 | `open_empty_existing_file_writes_header` | test-first verification | Pre-create 0-byte file → `open()` writes header. |
| J-3 | `open_existing_journal_with_valid_header_succeeds` | test-first verification | Open + append + flush + drop → re-open succeeds; iter_all yields the previously-appended record. |
| J-4 | `open_truncated_file_returns_truncated_file_header` | test-first verification | Pre-create file with 1, 4, 7 bytes (parameterized) → `open()` returns `TruncatedFileHeader { found }`. |
| J-5 | `open_wrong_magic_returns_invalid_file_header` | test-first verification | Pre-create file with `b"XXXX\x01\x00\x00\x00"` → `InvalidFileHeader`. |
| J-6 | `open_unsupported_version_returns_unsupported_file_version` | test-first verification | Pre-create with valid magic but `version = 2` → `UnsupportedFileVersion { version: 2 }`. |
| J-7 | `open_nonzero_reserved_header_bytes_returns_invalid_reserved_header` | test-first verification | Pre-create with valid magic + version but `reserved = [0, 1, 0]` → `InvalidReservedHeader { found }`. |
| J-8 | `append_then_read_round_trip_preserves_envelope` | TDD red→green | open → append(env) → flush → iter_all().next() yields `Ok(decoded)` with same fields. |
| J-9 | `append_multiple_preserves_order_and_positions` | test-first verification | Append 3 envelopes; iter_all yields them in order; `JournalPosition.byte_offset` matches cumulative file size at each step. |
| J-10 | `append_rejects_oversized_payload_before_write` | TDD red→green | Inject an oversize payload (synthetic envelope with custom `T`); `append` returns `InvalidFrameLength`; `bytes_written_total == 0` after the call. |
| J-11 | `truncated_frame_is_rejected_and_iterator_is_fused` | synthetic | Append 2, flush, truncate file to `8 + 4 + 1` bytes. iter_all yields `Err(TruncatedFrame)` then `None` from the SAME iterator. |
| J-12 | `checksum_mismatch_is_rejected_and_iterator_is_fused` | synthetic | Append 1, flush, flip a CRC byte. iter_all yields `Err(ChecksumMismatch)` then `None`. |
| J-13 | `zero_length_frame_is_rejected` | synthetic (read-path) | Manually write file_header + `[u32 0_le]` + `[u32 0_le]`. iter_all yields `Err(InvalidFrameLength)` (length=0). |
| J-14 | `oversized_length_frame_is_rejected_before_allocation` | synthetic (read-path) | Manually write file_header + `[u32 (MAX_FRAME_LEN+1)_le]`. iter_all yields `Err(InvalidFrameLength)` immediately, before any payload allocation. |
| J-15 | `invariant_violating_decoded_envelope_is_rejected_via_validate` | synthetic byte-patch (per §5.4) | 11-step procedure: serialize valid envelope with sentinel timestamp, find sentinel pattern (must match exactly once), patch to zero, recompute CRC, write directly into file, reopen, iter_all yields `Err(Types(InvalidEnvelope { field: "timestamp_ns", .. }))`. |
| J-16 | `corrupt_then_next_returns_none_on_same_iterator_instance` | derived from J-11 or J-12 | Same iterator's second `next()` after first `Err(...)` returns `None`. Re-calling `journal.iter_all()` produces a new iterator that re-yields the same Err — that is NOT a violation (intentional re-read). |
| J-17 | `flush_makes_appends_visible_to_iter_all` | test-first verification | Append + iter_all WITHOUT flush may not see the record. Append + flush + iter_all sees it. **Process-local visibility, NOT durability** — test asserts only the read-back, never `sync_all`. |
| J-18 | `partial_length_prefix_eof_is_truncated_frame_then_fused` | synthetic | Manually write a file containing only the 8-byte file header followed by 1, 2, or 3 bytes of would-be length prefix (parameterized). On `iter_all().next()`, the SAME iterator must yield exactly one `Err(TruncatedFrame { offset: 8, needed: 4, got: <1..4> })`, and a subsequent `iter.next()` on the same iterator must return `None`. `event_journal_corrupt_frames_total` increments by exactly 1. Distinguishes step-a partial-EOF corruption from step-a 0-byte clean EOF (which yields `None` directly without incrementing the counter — covered implicitly by J-1's "open + immediate iter_all on header-only file yields None" baseline). |

#### B.5.3 `snapshot.rs::RocksDbSnapshot` tests

Tempfile-backed RocksDB instance per test (each test gets a fresh `tempdir()`).

| # | Test | Discipline | Covers |
|---|---|---|---|
| S-1 | `snapshot_save_load_round_trip` | TDD red→green | `save(b"k", &SmokeTestPayload{..})` then `load::<SmokeTestPayload>(b"k")` returns `Ok(Some(equal))`. |
| S-2 | `snapshot_load_absent_key_returns_none` | test-first verification | `load(b"missing")` returns `Ok(None)`; `loaded_total` not incremented (verify via stats). |
| S-3 | `snapshot_last_sequence_round_trip` | TDD red→green | `set_last_sequence(42)` then `last_sequence()` returns `Ok(42)`. |
| S-4 | `snapshot_last_sequence_before_set_returns_unavailable` | test-first verification | Open fresh snapshot, immediately `last_sequence()` → `Err(LastSequenceUnavailable)`. |
| S-5 | `snapshot_save_under_reserved_prefix_is_rejected` | test-first verification | `save(b"\0rust_lmax_mev:snapshot:foo", &v)` returns `Err(ReservedKey(...))`. RocksDB is NOT touched. |
| S-6 | `snapshot_load_under_reserved_prefix_is_rejected` | test-first verification | `load::<V>(b"\0rust_lmax_mev:snapshot:foo")` returns `Err(ReservedKey(...))`. |
| S-7 | `snapshot_save_load_increment_counters_correctly` | test-first verification | After save + save + load(absent) + load(present): stats shows saved_total=2, loaded_total=1. |

#### B.5.4 Cross-module / integration tests

| # | Test | Discipline | Covers |
|---|---|---|---|
| I-1 | `journal_and_snapshot_can_coexist_in_separate_directories` | smoke | Open a `FileJournal` and a `RocksDbSnapshot` against different paths in the same `tempdir`; both work independently. |

#### B.5.5 Test pattern conventions

- **Tempfile usage:** all FS-touching tests use `tempfile::tempdir()` and pass the resulting `PathBuf` to `FileJournal::open` / `RocksDbSnapshot::open`. The directory auto-cleans on drop.
- **Byte-patch utilities:** synthetic frames (J-13, J-14, J-15) write directly into the file via `std::fs::OpenOptions` outside the `FileJournal` API. The test code reproduces the file-header bytes (`*b"LMEJ"` + `[0x01, 0, 0, 0]`) and then appends synthetic frame bytes manually. For J-15, the byte-patch procedure follows §5.4 step-by-step.
- **Same-iterator fused testing:** J-16 binds `let mut iter = journal.iter_all();` once and asserts `iter.next()` twice. It does NOT re-call `journal.iter_all()` between assertions.
- **Counter readback:** tests that verify counter values use `journal.stats()` / `snapshot.stats()` (per §5.1 / §5.2). The `metrics` facade is NOT readable from tests in Phase 1 (no exporter wired); the in-process atomic mirrors are the only readable surface.
- **No proptest in Task 13:** per §X.13. `proptest`-driven journal property tests are Task 17 work.
- **TDD discipline:** "TDD red→green" tests require writing the test first, observing failure, then implementing. "test-first verification" tests are written after the implementation lands and lock in the contract; they are expected to pass on first run.

### B.6 — rkyv 0.8 trait bounds for `FileJournal<T>`

The exact trait bounds for `T` on `FileJournal<T>` are constrained by the rkyv 0.8 API used by `append` (`rkyv::to_bytes::<rkyv::rancor::Error>(&envelope)`) and `iter_all` (`rkyv::from_bytes::<EventEnvelope<T>, rkyv::rancor::Error>(&payload)`).

For `EventEnvelope<T>` (which Task 11 derives `rkyv::Archive`, `rkyv::Serialize`, `rkyv::Deserialize`), the implementation will need approximately:

- `T: rkyv::Archive`
- `T: for<'a> rkyv::Serialize<rkyv::ser::Strategy<rkyv::ser::sharing::Share, rkyv::rancor::Error>>` (or the rkyv 0.8 equivalent active at implementation time)
- `T::Archived: rkyv::Deserialize<T, rkyv::rancor::Error>` (or equivalent)
- `T: 'static` (no borrowed lifetimes inside the payload — Phase 1 payloads are owned)

The exact bound expressions depend on the rkyv 0.8.x patch version. Implementer copies the bound from the actual `rkyv::to_bytes::<rkyv::rancor::Error>` signature in the workspace-pinned `rkyv` version and pins them on `FileJournal<T>`.

If those bounds prove unwieldy in practice, the implementation MAY introduce a marker trait `JournalPayload` with a blanket impl that bundles the rkyv bounds:

```rust
pub trait JournalPayload: rkyv::Archive
    + for<'a> rkyv::Serialize</* serializer strategy */>
    + 'static
where
    Self::Archived: rkyv::Deserialize<Self, rkyv::rancor::Error>,
{}

impl<T> JournalPayload for T where T: /* same bounds */ {}
```

Then `FileJournal<T: JournalPayload>` keeps the public surface clean. This is an implementer-discretion refactor; the public API exposed to callers (`append(&EventEnvelope<T>)`) stays the same.

### B.7 — Out-of-scope reaffirmations

These items are explicitly NOT in Batch B and not in the Phase 1 implementation:

- `read_at(JournalPosition)` — Phase 2 (per §X.3).
- Continue-past-corruption iter recovery — Phase 2 (per §X.8).
- `append_partial_write_failure_leaves_trailing_corrupt_frame` test — synthesized mid-write I/O failure is Phase 2 / Phase 4 (per Batch A out-of-scope test list).
- `proptest` — Task 17 (per §X.13).
- `UnsupportedEventVersion` raise path — Phase 2 replay engine (per §X.14).
- Any change to Task 11 (`crates/types`) — frozen.
- Any change to Task 12 (`crates/event-bus`) — frozen at HEAD `bb2e020`.
- Async / non-blocking journal writer — Task 16 wiring concern (per §4.6).
- `sync_all` / `sync_data` durability — Phase 4 reliability (per §X.10).

---

## Batch C — Operational Detail (drafted 2026-05-01)

This section concretizes the operational artifacts deferred from Batch B: exact `Cargo.toml` deltas, the workspace+scaffold commit body, the separate ADR-004 docs commit body, the Definition-of-Done checklist (D1–D20), and the recommended commit ordering for the Task 13 implementation phase. Decisions in §1–§X and §B.1–§B.7 are NOT revisited; Batch C is purely the operational specification consistent with those decisions.

### C.1 — Concrete `Cargo.toml` deltas

#### C.1.1 Workspace root `Cargo.toml`

Two additive edits:

```toml
# 1. [workspace] members: add "crates/journal" entry. Current state has
#    crates/types and crates/event-bus only.
[workspace]
resolver = "2"
members = [
    "crates/types",
    "crates/event-bus",
    "crates/journal",   # <-- ADDED for Task 13
]

# 2. [workspace.dependencies]: add crc32fast = "1". Other entries unchanged.
[workspace.dependencies]
# ... existing entries (rkyv, bincode, rocksdb, metrics, thiserror, parking_lot,
#     crossbeam-channel, serde, ...)
crc32fast = "1"   # <-- ADDED for Task 13 frame CRC, per §X.1
```

No other workspace dep changes. `bincode` stays at `1.3` per §X.4. The conditional bincode 2.x bump option from Batch A v0.1 §X.4 is rejected.

#### C.1.2 `crates/journal/Cargo.toml` (new file)

```toml
[package]
name = "rust-lmax-mev-journal"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
publish = false
description = "Phase 1 append-only journal + RocksDB snapshot for the LMAX-style MEV engine"

[dependencies]
rust-lmax-mev-types = { path = "../types" }
rkyv = { workspace = true }
bincode = { workspace = true }
rocksdb = { workspace = true }
metrics = { workspace = true }
thiserror = { workspace = true }
serde = { workspace = true }
crc32fast = { workspace = true }

[dev-dependencies]
tempfile = "3"
```

Identical to the §4.2 specification.

**Staged inclusion (amended 2026-05-02): the `rocksdb = { workspace = true }` line is omitted from the Gate 3 scaffold commit.** Gate 3 ships `crates/journal/Cargo.toml` with 7 runtime deps (rust-lmax-mev-types path-dep + rkyv + bincode + metrics + thiserror + serde + crc32fast) plus 1 dev-dep (tempfile). `rocksdb` is added during Gate 5 implementation at the time `RocksDbSnapshot` impl lands; see §4.2 Notes for the libclang rationale. RocksDbSnapshot remains the Phase 1 snapshot backend per ADR-003 + §5.2 + §B.2.5–§B.2.7.

### C.2 — Workspace-edit chore commit body

Single chore commit covering the workspace edit + crate scaffold (mirrors Task 11 first commit `4ac6f3c` and Task 12 first commit `a92898e`). PowerShell-clean multi `-m` form, no embedded apostrophes inside `-m '...'`:

```powershell
git add Cargo.toml crates/journal/Cargo.toml crates/journal/src/lib.rs `
        crates/journal/src/error.rs crates/journal/src/frame.rs `
        crates/journal/src/journal.rs crates/journal/src/snapshot.rs
git commit `
    -m 'chore(journal): scaffold journal crate and update workspace' `
    -m 'Workspace edits: re-adds crates/journal to [workspace] members for Task 13 implementation. Other Phase 1 crates (config, observability, app) re-add their member entries in Tasks 14-16 as those crates are created. Adds crc32fast = "1" to [workspace.dependencies] so the new crate can compute per-record CRC32 checksums per spec section 4.5 and ADR-003.' `
    -m 'Crate scaffold: empty buildable crate with rust-lmax-mev-types path-dep, rkyv, bincode, rocksdb, metrics, thiserror, serde, and crc32fast runtime deps per spec section 4.2. tempfile is the only dev-dep (filesystem-backed test fixtures per CLAUDE.md). The lib.rs re-exports plus four sibling modules (error.rs, frame.rs, journal.rs, snapshot.rs) follow the split-modules-from-start decision in spec section X.11.' `
    -m 'crc32fast is the canonical pure-Rust IEEE CRC32 implementation per spec section X.1. bincode stays at 1.3 (serde adapter form) per spec section X.4; the ADR-004 wording fix lands as a separate small docs commit.' `
    -m 'Workspace edit and crate scaffold land in a single commit because adding the member entry without the crate manifest would put cargo metadata into a failure state. Mirrors the Task 11 first-commit pattern at 4ac6f3c and the Task 12 first-commit pattern at a92898e.' `
    -m 'Co-Authored-By: Claude <noreply@anthropic.com>'
```

Subject `chore(journal): scaffold journal crate and update workspace` is 60 characters. Stage list covers the workspace `Cargo.toml` plus the crate manifest plus the five split-module source files (placeholder content; subsequent commits fill them).

### C.3 — ADR-004 docs commit body

Separate single-file commit, NOT bundled with the workspace+scaffold commit:

```powershell
git add docs/adr/ADR-004-rpc-evm-stack-selection.md
git commit `
    -m 'docs(adr): fix ADR-004 bincode wording for 1.x serde adapter' `
    -m 'ADR-004 Consequences originally read "Snapshot types must derive bincode::Encode and bincode::Decode" but the workspace pins bincode = "1.3" which has no Encode/Decode derives. The serde adapter (bincode::serialize / bincode::deserialize) is the actual encoding path used by crates/types tests and crates/journal snapshot save/load.' `
    -m 'Replaces the Encode/Decode wording with: "Snapshot types must implement serde::Serialize and serde::Deserialize, encoded via the bincode 1.x serde adapter (bincode::serialize / bincode::deserialize)." Single-line change inside ADR-004; no other ADR or spec edits in this commit. Mirrors the §X.4 ADR-004 fix wording in this spec.' `
    -m 'Source: Task 11 spec section 11.1 deferred this reconciliation to Task 13 entry. Task 13 spec section X.4 (Resolved Decision) confirmed option (a) (keep bincode 1.3 and fix ADR wording) over option (b) bincode 2.x upgrade.' `
    -m 'Co-Authored-By: Claude <noreply@anthropic.com>'
```

Single tracked-file change. Subject 56 characters. Lands BEFORE the workspace+scaffold commit so the ADR is consistent with the snapshot encoding before any journal code is written.

### C.4 — Definition of Done (D1–D20)

The implementer / reviewer must confirm each item before declaring Task 13 implementation done. Items D1–D20 cover the spec's Batch A + B + C contract end-to-end.

| # | Item | Verification command / check |
|--:|---|---|
| D1 | Workspace `Cargo.toml` has `"crates/journal"` in `[workspace] members` AND `crc32fast = "1"` in `[workspace.dependencies]`. | `grep -n "crates/journal\|crc32fast" Cargo.toml` → 2 hits. |
| D2 | `crates/journal/Cargo.toml` exists with the exact §C.1.2 / §4.2 content. | byte-compare against §C.1.2 listing. |
| D3 | `crates/journal/src/` contains `lib.rs`, `error.rs`, `frame.rs`, `journal.rs`, `snapshot.rs` per §X.11. | `ls crates/journal/src/` → 5 files. |
| D4 | `JournalError` enum has 15 variants per §B.4 with `#[non_exhaustive]` and `thiserror::Error` derive. NO `#[from]` on either bincode variant (§5.3). | grep on `error.rs` for variant count + `#[from]` audit. |
| D5 | `frame.rs` exports `pub(crate)` constants `MAGIC`, `FILE_FORMAT_VERSION`, `RESERVED_HEADER`, `FILE_HEADER_LEN`, `MAX_FRAME_LEN`, `FRAME_OVERHEAD` per §B.1.1. | `grep -n "pub(crate) const" crates/journal/src/frame.rs` → 6 hits with the listed names. |
| D6 | `frame.rs` exposes `pub(crate)` helpers `write_file_header`, `read_and_validate_file_header`, `validate_frame_length` per §B.1.4. | grep on `frame.rs`. |
| D7 | `FileJournal<T>` is a generic struct with a `PhantomData<T>` private field; impls `open` / `append` / `flush` / `iter_all` / `stats` per §5.1 + §B.2. `JournalIter<T>` carries a private `fused: bool` field. | grep + manual review. |
| D8 | `RocksDbSnapshot` exposes `open` / `save<V>` / `load<V>` / `last_sequence` / `set_last_sequence` / `stats` per §5.2 + §B.2. Reserved-key prefix constant `RESERVED_KEY_PREFIX = b"\0rust_lmax_mev:snapshot:"` and `LAST_SEQUENCE_KEY = b"\0rust_lmax_mev:snapshot:last_sequence"` present. | grep. |
| D9 | All 6 metrics emitted via `metrics::counter!` at the §B.3 sites; counters mirrored as `AtomicU64` in `JournalStats` / `SnapshotStats`; `current_depth`-style gauge is NOT emitted by this crate. | grep + stats-struct field audit. |
| D10 | All 30 named tests present and passing: 4 frame-module (F-1..F-4), 18 FileJournal (J-1..J-18), 7 RocksDbSnapshot (S-1..S-7), 1 cross-module (I-1). | `cargo test -p rust-lmax-mev-journal -- --list` lists 30 test functions; `cargo test -p rust-lmax-mev-journal` reports `30 passed; 0 failed`. |
| D11 | `cargo fmt --check` clean. | exit 0. |
| D12 | `cargo build -p rust-lmax-mev-journal` clean (no warnings). | exit 0. |
| D13 | `cargo test -p rust-lmax-mev-journal` clean. | exit 0; 30 passed. |
| D14 | `cargo test --workspace` clean (event-bus 7 + types 4 + journal 30 = 41 tests). | exit 0; 41 passed. |
| D15 | `cargo clippy -p rust-lmax-mev-journal -- -D warnings` clean. | exit 0. |
| D16 | `cargo clippy --workspace --all-targets -- -D warnings` clean (Task 18 CI preview, carried from Task 12 D10 augmentation). | exit 0. |
| D17 | `cargo doc -p rust-lmax-mev-journal --no-deps` clean. | exit 0. No broken intra-doc links. |
| D18 | ADR-004 docs commit landed (single file, body per §C.3) BEFORE the workspace+scaffold commit. | Combined-path log proves ordering: `git log --oneline -- docs/adr/ADR-004-rpc-evm-stack-selection.md crates/journal/` shows the ADR-004 fix commit appearing earlier (lower in chronological-newest-first output) than any `crates/journal/` commit. Alternatively, `git log --reverse --oneline -- docs/adr/ADR-004-rpc-evm-stack-selection.md crates/journal/` should list the ADR-004 fix as the FIRST entry. |
| D19 | `AGENTS.md` not included in any commit AND not staged at observed commit boundaries; `.claude/` not included in any commit AND not staged at observed commit boundaries; Task 11 (`crates/types/**`) untouched after `e2911cf`; Task 12 (`crates/event-bus/**`) untouched after `bb2e020`. | `git log --all -- AGENTS.md .claude/` returns empty (never committed). `git status --short --branch` checked at each commit boundary during implementation shows AGENTS.md and `.claude/` as `??` (untracked) and never `A `/` M ` (staged). `git log --oneline -- crates/types/ crates/event-bus/` shows no commits after `e2911cf` / `bb2e020` respectively (path-limited log; avoids the unnecessary `--follow` semantic). |
| D20 | Task 13 spec untouched after Batch C draft (this document) — no in-progress edits during implementation. | `git diff` on the spec yields empty during implementation phase. |

### C.5 — Commit ordering and policy

Recommended order for the Task 13 implementation phase, after Batch B + C user approval:

1. **Spec commit** (this document, untracked → tracked):
   - Subject: `docs: add Task 13 (crates/journal) design spec`
   - Body: brief reference to Batch A v0.4 + B v0.5 + C v0.6 user approval; reference to ADR-003 + ADR-004 + Task 11 spec §11.1 deferred reconciliation; reference to Task 12 spec D10 augmented gate.
   - Single docs commit; the spec is final at v0.6 (Batch A v0.4 + Batch B v0.5 + Batch C v0.6).
2. **ADR-004 wording fix** (separate, single-file): commit body per §C.3.
3. **Workspace + crate scaffold** (combined per §C.2): commit body per §C.2.
4. **Implementation commits** following the Task 12 pattern (one TDD red-green pair per behavior, or test-first verification + impl in a single commit per behavior). Tentative ordering — finalized during implementation:
   - `feat(journal): JournalError enum + non_exhaustive + thiserror impls` (covers D4)
   - `feat(journal): frame.rs constants + write_file_header (TDD F-4)` (covers parts of D5, D6)
   - `feat(journal): validate_frame_length (TDD F-1, F-2, F-3)` (D6)
   - `feat(journal): read_and_validate_file_header` (D6)
   - `feat(journal): FileJournal::open create/empty path (TDD J-1, J-2)` (parts of D7)
   - `feat(journal): FileJournal::open existing-header path + negative tests (J-3, J-4..J-7)` (D7)
   - `feat(journal): FileJournal::append round-trip (TDD J-8) + helpers` (D7)
   - `feat(journal): FileJournal::append length validation (TDD J-10)` (D7)
   - `feat(journal): FileJournal::iter_all + JournalIter::next happy path (J-9)` (D7, D9 read_total)
   - `feat(journal): JournalIter corruption detection (J-11, J-12, J-13, J-14, J-18)` (D9 corrupt_frames_total)
   - `feat(journal): JournalIter validate boundary (J-15, J-16)` (D9, validate boundary §5.4)
   - `feat(journal): FileJournal::flush + flush-makes-visible (J-17)` (D7)
   - `feat(journal): RocksDbSnapshot::save + load + reserved-key (S-1, S-2, S-5, S-6)` (D8)
   - `feat(journal): RocksDbSnapshot::last_sequence + set_last_sequence (S-3, S-4)` (D8)
   - `feat(journal): journal+snapshot coexistence test (I-1)` (cross-module integration)
   - `chore(journal): final fmt/clippy cleanup` if needed (parallel to Task 12's `bb2e020`)
5. **No commit lands until the user explicitly approves each gate.** Push happens only on explicit user instruction; the `task-13-complete` and `phase-1-complete` tags require explicit approval (the latter is reserved for Task 19 per CLAUDE.md).

### C.6 — Out-of-scope reaffirmations (Batch C)

- No `phase-1-complete` tag from Task 13 work — that tag is reserved for Task 19 per CLAUDE.md.
- No `task-13-complete` tag without explicit user approval. Task 12 didn't get a tag at completion either; tag policy is consistent.
- No bincode 2.x migration (per §X.4 option (a) decision).
- No alternative CRC polynomial (per §X.9 IEEE decision).
- No async / non-blocking journal writer (per §3 + §4.6 + §B.7).
- No `read_at(JournalPosition)` Phase 2 work bleeding into Phase 1 implementation.
- No `proptest`-driven tests in Task 13 (per §X.13 → Task 17).
- No `crates/types` (Task 11) edits — frozen.
- No `crates/event-bus` (Task 12) edits — frozen at HEAD `bb2e020`.

---

## Status of this draft (Batch A v0.4 + Batch B v0.5 + Batch C v0.6, all user-approved 2026-05-01)

### Batch A v0.4 — user-approved 2026-05-01

The Batch A content (§1–§X) was user-approved on 2026-05-01 after two rounds of user feedback (v0.1 → v0.2 → v0.3) and a Codex cleanup review (v0.3 → v0.4). The §1–§X content is frozen for the purposes of Batch B and Batch C work; revisiting requires explicit user direction.

What v0.4 fixed on top of v0.3:

- §1–§3: Goal, scope (in-scope + adjacent decisions), out-of-scope. UnsupportedEventVersion cross-reference fixed (now points to §X.14). v0.3 fixes the v0.2 line-60 typo "No `cargo build` or run-time durability" → "No fsync / crash-durability". v0.4 normalizes the version-header self-identification so the line-3 `Version: 0.4 (Batch A v0.4 cleanup applied per Codex 2026-05-01 review)` no longer carries stale `v0.3` self-references.
- §4: Architecture & layout — file layout split-modules resolved; data flow includes counter increment sites and bad-frame fusing semantics; frame layout uses raw-byte magic `*b"LMEJ"` with little-endian record fields. §4.4 (v0.3) adds explicit append-path payload length validation and partial-write failure semantics. §4.5 (v0.3) adds the file-header reserved-bytes rule. §4.6 (v0.3) adds the "Task 13 blocking primitive alone does not satisfy ADR-003 non-blocking pipeline property" sentence.
- §5: Public API surface — `FileJournal::open` doc (v0.3) enumerates the 6 open-time path/length cases. `FileJournal::append` doc (v0.3) documents the pre-write length validation and the non-transactional partial-write semantics. §5.3 `JournalError` adds `InvalidReservedHeader { found: [u8; 3] }`; bincode variants documented as no-`#[from]` with `.map_err(...)` call-site disambiguation. §5.4 (v0.3) makes the byte-patch test step 13 explicit about iterator-instance scoping and adds an endianness-safety note. §5.6 (v0.3) clarifies that open-time header errors are NOT counted by `event_journal_corrupt_frames_total`. §5.7 anti-patterns updated.
- §X: 15 Resolved Decisions. v0.3 keeps all 15 unchanged but tightens cross-reference text in §X.5 and §X.8 memos. v0.4 keeps decisions unchanged.

### Batch B v0.5 — Codex-approved + user-approved 2026-05-01

Batch B (§B.1–§B.7) is appended to the spec without revisiting any §1–§X decision. It concretizes:

- **§B.1 Frame internals**: `MAGIC` / `FILE_FORMAT_VERSION` / `RESERVED_HEADER` / `FILE_HEADER_LEN` / `MAX_FRAME_LEN` / `FRAME_OVERHEAD` constants in `frame.rs`; file-header byte layout (8 bytes); per-record byte layout (4 + length + 4); private helper signatures `write_file_header`, `read_and_validate_file_header`, `validate_frame_length`. The Batch A "Deferred to Batch B" frame-spec line items are all covered.
- **§B.2 Method data flow**: pseudo-Rust bodies for `FileJournal::open`, `append`, `flush`, `iter_all` + `JournalIter::next`, `RocksDbSnapshot::save` / `load` / `last_sequence` / `set_last_sequence`. The Batch A "Deferred to Batch B" per-method data-flow line items are covered. The §B.2 bodies follow the `iter_all` Batch B memo (each call opens a fresh read handle at offset 8) and the partial-write semantics from §4.4.
- **§B.3 Metrics emission map**: call-site granular table with site / increment / trigger for all six counters. Batch A "Deferred to Batch B" metrics-map item covered.
- **§B.4 Error matrix**: 15 variants × (emit site, trigger, counter incremented?, caller action). Batch A "Deferred to Batch B" error-matrix item covered. Note: Batch A's "all 14 variants" wording was off-by-one; actual count is 15.
- **§B.5 Named test plan**: 4 frame-module tests (F-1..F-4), 18 FileJournal tests (J-1..J-18), 7 RocksDbSnapshot tests (S-1..S-7), 1 cross-module integration test (I-1). Each test has a discipline tag (TDD red→green / test-first verification / synthetic). Test pattern conventions in §B.5.5. Batch A "Deferred to Batch B" test-plan item covered.
- **§B.6 rkyv 0.8 trait bounds**: implementer-discretion guidance for `FileJournal<T>` bounds, with optional `JournalPayload` marker-trait refactor. Batch A "Deferred to Batch B" rkyv-bounds item covered.
- **§B.7 Out-of-scope reaffirmations**: cross-references the Phase-1 exclusions back to their §X / §4 / §5 decision sites.

### Open questions Batch B carried into approval (still open at implementation time)

- The implementation MAY introduce a `JournalPayload` marker trait (§B.6) to bundle the rkyv 0.8 bounds, but the spec leaves this as implementer discretion. Codex did not flag a stricter position; user approval did not constrain.
- §B.2.7 documents that `set_last_sequence` does NOT increment `event_snapshot_saved_total` (treating it as bookkeeping rather than user data). Codex did not flag; user approval did not constrain.
- §B.5.2 J-10 (`append_rejects_oversized_payload_before_write`) requires a synthetic `T` whose rkyv serialization exceeds `MAX_FRAME_LEN` (16 MiB) — non-trivial to construct in a unit test. Implementer may need a feature-gated test-only payload type, OR the test may stub the rkyv encoder. Decision deferred to implementation.
- §B.5.2 J-15 byte-patch strategy depends on the rkyv 0.8 archived layout for `EventEnvelope<SmokeTestPayload>` having `timestamp_ns` as a contiguous little-endian `u64`. If a future rkyv update changes this, J-15's match-count check will trip and the implementation must either adjust the sentinel-search OR escalate to user (no `crates/types` test-helper change without explicit approval per §5.4).

### Batch C v0.6 — Codex-approved + user-approved 2026-05-01

Batch C (§C.1–§C.6) is appended to the spec without revisiting any §1–§X / §B.1–§B.7 decision. It concretizes:

- **§C.1 Concrete `Cargo.toml` deltas**: workspace root edits (`members` += `crates/journal`, `[workspace.dependencies]` += `crc32fast = "1"`) + new `crates/journal/Cargo.toml` matching §4.2.
- **§C.2 Workspace-edit chore commit body**: PowerShell-clean multi `-m` form for the scaffold commit; subject `chore(journal): scaffold journal crate and update workspace` (60 chars). Mirrors Task 11 + Task 12 first-commit precedents.
- **§C.3 ADR-004 docs commit body**: separate single-file commit landing BEFORE the workspace+scaffold commit. Subject `docs(adr): fix ADR-004 bincode wording for 1.x serde adapter` (56 chars). Replaces the bincode 2.x Encode/Decode wording with bincode 1.x serde adapter wording per §X.4 option (a).
- **§C.4 DoD D1–D20**: 20-item Definition-of-Done covering workspace edits, crate manifest, source layout, all 15 `JournalError` variants, frame constants + helpers, `FileJournal<T>` impl shape, `RocksDbSnapshot` impl shape, all 6 metrics, all 30 named tests passing, `cargo fmt` / `build` / `test` / `clippy -p` / `clippy --workspace --all-targets` / `doc` gates, ADR-004 commit ordering, AGENTS.md / `.claude/` / Task 11 / Task 12 untouched-ness, spec-stability during implementation.
- **§C.5 Commit ordering**: spec commit → ADR-004 wording fix → workspace+scaffold → 15 implementation commits (TDD discipline per behavior) → optional final fmt/clippy cleanup. No commits land without explicit user approval per gate.
- **§C.6 Out-of-scope reaffirmations**: no `phase-1-complete` / `task-13-complete` tags without explicit user approval; no bincode 2.x; no CRC32C; no async writer; no `read_at` in Phase 1; no proptest in Task 13; no Task 11 / Task 12 edits.

### Open questions Batch C carries forward to Codex / user review

- **D10 test count**: 4 (frame) + 18 (FileJournal) + 7 (Snapshot) + 1 (cross) = 30. Carried into D14 as event-bus 7 + types 4 + journal 30 = 41 workspace tests. If Codex spots additional deferred tests, the count adjusts.
- **§C.5 implementation commit ordering** is tentative; the implementer may merge or split commits as needed during the Task 12-style subagent-driven-development pass.

**This draft is NOT committed to git.** Per session policy, the file is written to disk for review; commit decision pending user approval after Batch C sign-off. The recommended commit lands the full v0.6 spec as a single docs commit per §C.5 step 1 once user approves.
