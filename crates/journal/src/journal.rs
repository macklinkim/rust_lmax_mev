//! `FileJournal<T>` — append-only file-backed log primitive.
//!
//! Gate 3 scaffold placeholder. The `FileJournal<T>` struct (with
//! `PhantomData<T>`), its `open` / `append` / `flush` / `iter_all` / `stats`
//! impls, the `JournalIter<T>` (with `fused: bool`), and `JournalStats` land
//! during Gate 5 implementation per spec sections 5.1 and B.2.1-B.2.4. The
//! mandatory `EventEnvelope::validate()` boundary on the read path is
//! enforced per spec section 5.4.
