# Task 11: `crates/types` — Phase 1 Type Primitives Design

**작성일:** 2026-04-27
**문서 상태:** v0.3 Approved (reviewer 3회 통과, 사용자 최종 검토 대기)
**대상 작업:** Phase 1 / Task 11 / `crates/types` 크레이트 신설
**선행 입력:**
- `docs/specs/event-model.md` (frozen)
- `docs/adr/ADR-004-rpc-evm-stack-selection.md` (frozen)
- `docs/adr/ADR-005-event-bus-implementation-policy.md` (frozen)
- `PHASE_1_DETAIL_REVISION.md`
- `CLAUDE.md`
**후속 작업:** Task 12 (`crates/event-bus`)는 본 크레이트를 첫 번째 소비자로 사용

---

## 1. 컨텍스트

Phase 0(8 ADR + 4 frozen spec)이 완료되었고 Phase 1의 Task 10(워크스페이스 스캐폴드)도 커밋된 상태이다. 본 문서는 Phase 1 두 번째 작업인 `crates/types`를 정의한다.

`crates/types`는 LMAX 스타일 이벤트 버스 위에서 모든 도메인 메시지를 감싸는 `EventEnvelope<T>`와 그에 부속되는 메타데이터 타입을 보유하는 가장 얇은 토대 크레이트다. Phase 2 이후의 도메인 이벤트(`BlockObserved`, `MempoolTxObserved`, `OpportunityDetected` 등)는 본 크레이트의 envelope generic 위에 올라간다.

### 1.1 본 크레이트의 책임 (Phase 1)

- 이벤트 envelope의 **데이터 형상**과 **생성 계약**을 정의
- replay/round-trip을 위한 직렬화 derive 부착
- `EventBus`/`Journal` 등 후속 크레이트에 공통으로 쓰이는 좁은 에러 타입 노출

### 1.2 본 크레이트가 다루지 않는 것

- 실제 도메인 이벤트 페이로드 (Phase 2)
- 직렬화기 호출 (consumer 크레이트가 호출)
- bus/journal/snapshot 동작 (Task 12, 13)
- 추적/메트릭 emit (Task 15)

---

## 2. 스코프 결정 (브레인스토밍 Q1–Q5)

| # | 결정 | 근거 |
|---|---|---|
| Q1 | Phase 1 한정 stub은 `SmokeTestPayload` 1개. 도메인 이벤트는 Phase 2에서 정의. | YAGNI. 100k bus smoke / journal round-trip / snapshot smoke는 단일 generic payload로 충족. Phase 2에서 thin path 실 데이터를 보고 이벤트 모양 결정. |
| Q2 | rkyv + serde derive 둘 다 부착. rkyv 필수 = `EventEnvelope`/`EventSource`/`ChainContext`/`SmokeTestPayload`. `PublishMeta`/`JournalPosition`/`TypesError`는 rkyv 제외. derive 실패 시 `[u8; N]` primitive fallback → ADR/spec surface (silent bincode downgrade 금지). | ADR-004이 envelope에 rkyv를 강제. spec이 `[u8; 32]`를 이미 사용해 alloy 호환 리스크 회피됨. transient 입력과 반환값은 rkyv 불필요. |
| Q3 | per-crate thiserror. types는 `TypesError`만 최소 정의 (`InvalidEnvelope`, `UnsupportedEventVersion` 2 variants). 다른 크레이트는 `BusError`/`JournalError`/`ConfigError` 등을 자기 책임으로 정의하고 필요 시 `#[from] TypesError`로 래핑. `anyhow`는 app 경계에서만 사용. | 관용적 Rust 패턴. 응집도 보존. PHASE_1_DETAIL_REVISION의 `BusError` 명명과 정합. |
| Q4 | `JournalPosition`은 `crates/types`에 위치. | journal/replay 양쪽이 참조하는 공유 타입. types에 두면 순환 결합 발생 안 함. |
| Q5 | `crates/types`는 `alloy-primitives`를 직접 의존하지 않음. `BlockHash = [u8; 32]` 로컬 alias만 허용. alloy-primitives는 워크스페이스 dep으로 남기되 Phase 2 이후 도메인 이벤트 크레이트가 명시적으로 가져감. | spec이 `[u8; 32]`로 frozen. Phase 1 어느 타입도 alloy 형이 자연스러운 필드 없음. CLAUDE.md가 우려한 rkyv 0.8 + alloy-primitives derive 호환 리스크를 Phase 1으로 끌어오지 않음. |

---

## 3. 크레이트 구조 및 공개 API 표면 (접근 1: Lean)

### 3.1 파일 레이아웃

```
crates/types/
├── Cargo.toml
└── src/
    └── lib.rs        # 단일 파일, 약 250–350 LOC
```

단일 `lib.rs` 파일 구조. 7개 작은 타입은 모듈 분리할 응집 경계 부족. Phase 2에서 도메인 이벤트가 등장할 때 자연스러운 split 시점 도래.

### 3.2 공개 export (lib.rs 루트에서 직접 노출)

| 항목 | 종류 | 가시성 정책 |
|---|---|---|
| `EventEnvelope<T>` | struct | **필드 private + getter** (bus-assigned invariant 캡슐화) |
| `EventSource` | enum (Copy) | variants pub |
| `ChainContext` | struct | 필드 `pub` (transparent data carrier) |
| `PublishMeta` | struct | 필드 `pub` (caller-provided 입력 묶음) |
| `JournalPosition` | struct | 필드 `pub` (return value) |
| `SmokeTestPayload` | struct | 필드 `pub` (test-only payload) |
| `TypesError` | enum (thiserror) | variants pub |
| `BlockHash` | `type BlockHash = [u8; 32];` | pub alias |

`prelude` 모듈, `Default` derive, builder 패턴은 Phase 1에서 도입하지 않음. 7개 export는 `use rust_lmax_mev_types::*;` 한 줄로 충분히 가져올 수 있음.

### 3.3 캡슐화 비대칭의 근거

`EventEnvelope`의 `sequence` / `timestamp_ns`는 bus가 단독 할당하는 invariant이므로 외부에서 위조 불가하도록 private + 단일 `seal()` entry로 막아야 한다. 그 외 데이터 타입은 caller-side carrier로 bus-assigned invariant가 없어 over-encapsulation이 비용만 발생시키므로 `pub` 필드를 유지한다.

### 3.4 Getter 시그니처 규약

```rust
impl<T> EventEnvelope<T> {
    pub fn seal(
        meta: PublishMeta,
        payload: T,
        sequence: u64,
        timestamp_ns: u64,
    ) -> Result<Self, TypesError>;

    // Post-decode invariant 재검증 (deserialize 경계용)
    pub fn validate(&self) -> Result<(), TypesError>;

    // Copy 필드: by value
    pub fn sequence(&self) -> u64;
    pub fn timestamp_ns(&self) -> u64;
    pub fn source(&self) -> EventSource;
    pub fn event_version(&self) -> u16;
    pub fn correlation_id(&self) -> u64;

    // Owned 필드: by reference
    pub fn chain_context(&self) -> &ChainContext;
    pub fn payload(&self) -> &T;

    // Consume helper — 메타데이터 의도적 폐기
    pub fn into_payload(self) -> T;
}
```

setter는 제공하지 않음. envelope는 `seal()` 이후 불변.

**`seal()`은 단일 _validated_ 생성 경로**다. `serde::Deserialize` 및 `rkyv::Deserialize`는 `seal()`을 우회하여 필드를 직접 재구성하므로, decode 경계에서는 별도의 `validate()` 호출로 invariant를 다시 강제해야 한다 (§5.3 참조).

---

## 4. 타입 정의

### 4.1 `EventEnvelope<T>`

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct EventEnvelope<T> {
    sequence: u64,
    timestamp_ns: u64,
    source: EventSource,
    chain_context: ChainContext,
    event_version: u16,
    correlation_id: u64,
    payload: T,
}
```

- 필드 순서는 frozen spec과 1:1.
- `T`에 명시적 trait bound 없음. derive가 `impl<T: Clone> Clone`, `impl<T: PartialEq> PartialEq`, `impl<T: Eq> Eq`, `impl<T: rkyv::Archive> rkyv::Archive` 등 조건부 impl을 자동 생성. T가 derive 요구사항을 만족하지 않으면 컴파일 시 자연스럽게 실패.
- `Hash`는 derive하지 않음. envelope 자체를 HashMap key로 쓸 use case 없음.
- **Frozen spec과의 관계:** `event-model.md`의 derive 요구사항은 `Clone, Debug, PartialEq, rkyv::*, serde::*`이다. 본 설계는 그 위에 `Eq`를 의도적으로 추가하는 superset이다. 추가 근거는 PHASE_1_DETAIL_REVISION §3.2의 semantic equality 요구사항을 한 줄 비교(`assert_eq!`)로 검증하기 위함이다. `Eq`는 `PartialEq`의 marker trait이라 ABI 영향이 없으며 frozen spec과 충돌하지 않는다. 동일한 superset 정책이 §4.3 `ChainContext`, §4.4 `PublishMeta`, §4.6 `SmokeTestPayload`에도 적용된다.

### 4.2 `EventSource`

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum EventSource {
    Ingress,
    Normalizer,
    StateEngine,
    OpportunityEngine,
    RiskEngine,
    Simulator,
    Execution,
    Relay,
}
```

frozen spec의 8개 variant 그대로. `Copy + Eq + Hash` 부착으로 metric 라벨/라우팅 키로 사용 가능 (비용 0).

### 4.3 `ChainContext`

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ChainContext {
    pub chain_id: u64,
    pub block_number: u64,
    pub block_hash: BlockHash,
}
```

`block_hash`에 `BlockHash = [u8; 32]` 사용. spec과 정확히 일치. `Hash` derive는 부착하지 않음 (현재 사용처 없음).

### 4.4 `PublishMeta`

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PublishMeta {
    pub source: EventSource,
    pub chain_context: ChainContext,
    pub event_version: u16,
    pub correlation_id: u64,
}
```

bus를 거치지 않는 transient 입력이므로 rkyv derive 제외. 디버그/로그 직렬화를 위해 serde만 부착.

### 4.5 `JournalPosition`

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct JournalPosition {
    pub sequence: u64,
    pub byte_offset: u64,
}
```

journal append 반환값. plain 16바이트 데이터로 `Copy`. rkyv 제외.

### 4.6 `SmokeTestPayload`

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct SmokeTestPayload {
    pub nonce: u64,
    pub data: [u8; 32],
}
```

Phase 1 한정 synthetic payload. `nonce`로 publish 순서 검증, `data`로 multi-field archive layout 검증 (rkyv padding 동작 자연 트리거). 총 40바이트로 100k events에 대해 메모리 영향 미미.

### 4.7 `TypesError`

```rust
#[derive(Debug, thiserror::Error)]
pub enum TypesError {
    #[error("invalid envelope: field={field}, reason={reason}")]
    InvalidEnvelope {
        field: &'static str,
        reason: &'static str,
    },
    #[error("unsupported event_version: found={found}, max_supported={max_supported}")]
    UnsupportedEventVersion {
        found: u16,
        max_supported: u16,
    },
}
```

`&'static str` 사용 — Phase 1에서 모든 invariant 위반 사유는 컴파일 시점에 알려져 있음. heap 할당 회피, 가벼운 에러 유지. `UnsupportedEventVersion`은 Phase 1에 emit 지점 없음 (Task 13 decoder 또는 Phase 2 replay에서 사용 예정). pub enum의 공개 variant이므로 `#[allow(dead_code)]` 부착 불필요.

### 4.8 `BlockHash` type alias

```rust
pub type BlockHash = [u8; 32];
```

`ChainContext.block_hash`가 사용. alloy-primitives 비의존 정책 (Q5)의 결과.

---

## 5. 생성 및 검증 계약

### 5.1 `EventEnvelope::seal()` 단일 생성 경로

```rust
impl<T> EventEnvelope<T> {
    /// Seals an envelope with bus-assigned `sequence` and `timestamp_ns`.
    ///
    /// **Intended caller: EventBus implementations only.** Downstream
    /// consumers receive sealed envelopes and access fields via getters.
    ///
    /// Validates Phase 1 invariants:
    /// - `timestamp_ns != 0`
    /// - `meta.event_version != 0`  (event_version = 0 is reserved/uninitialized
    ///   per Phase 1 policy; not enforced by the frozen spec)
    /// - `meta.chain_context.chain_id != 0`
    ///
    /// `sequence`, `block_number`, `correlation_id` are accepted as-is.
    pub fn seal(
        meta: PublishMeta,
        payload: T,
        sequence: u64,
        timestamp_ns: u64,
    ) -> Result<Self, TypesError> { /* ... */ }
}
```

### 5.2 seal() invariant 3개

| 검사 | 실패 시 에러 |
|---|---|
| `timestamp_ns != 0` | `InvalidEnvelope { field: "timestamp_ns", reason: "must be non-zero" }` |
| `meta.event_version != 0` | `InvalidEnvelope { field: "event_version", reason: "must be non-zero" }` |
| `meta.chain_context.chain_id != 0` | `InvalidEnvelope { field: "chain_context.chain_id", reason: "must be non-zero" }` |

**제외한 후보:**
- `sequence != 0` — bus 구현체가 0부터 시작할 수도 있음. **호출자 측 가정:** Task 12 이후 `EventBus` 구현체는 `sequence`의 시작값을 자유롭게 선택할 수 있다 (0 또는 1). 소비자는 `sequence`를 단조 증가하는 식별자로만 취급하고 특정 시작값에 의존하지 않는다.
- `block_number != 0` — genesis 블록도 합법.
- `correlation_id != 0` — 0이 "no parent / root span" sentinel일 수 있음.

### 5.3 `validate()` — deserialize 경계 invariant 재검증

`seal()`은 envelope **생성** 경로의 invariant를 강제하지만 **deserialize** 경로는 우회한다. `serde::Deserialize`와 `rkyv::Deserialize`는 envelope의 필드를 직접 재구성하므로, 손상되거나 악의적으로 조작된 journal frame / wire frame이 `timestamp_ns = 0`, `event_version = 0`, `chain_context.chain_id = 0` 같은 invariant 위반 값을 가진 envelope를 만들 수 있다. `seal()`이 거부했을 값들이 deserialize를 통해 시스템에 유입될 수 있는 것이다.

이 구멍을 막기 위해 `EventEnvelope`는 두 번째 검증 entry로 `validate()`를 노출한다.

```rust
impl<T> EventEnvelope<T> {
    /// Re-validates Phase 1 invariants without reconstructing the envelope.
    ///
    /// Use this at deserialization boundaries (journal decode, replay, wire
    /// decode) to confirm a decoded envelope still satisfies the invariants
    /// that `seal()` enforced at construction time.
    ///
    /// `serde::Deserialize` and `rkyv::Deserialize` reconstruct fields
    /// directly, **bypassing `seal()`**. Without `validate()`, a corrupted or
    /// malicious frame could produce an envelope with `timestamp_ns = 0`,
    /// `event_version = 0`, or `chain_context.chain_id = 0`.
    ///
    /// Checks the same three invariants as `seal()`:
    /// - `timestamp_ns != 0`
    /// - `event_version != 0`
    /// - `chain_context.chain_id != 0`
    ///
    /// Journal, replay, and decoder consumers MUST call `validate()` after
    /// any deserialization, before passing the envelope to downstream
    /// pipeline stages.
    pub fn validate(&self) -> Result<(), TypesError> { /* ... */ }
}
```

**구현 권장 사항 (강제 아님):** `seal()`과 `validate()`가 동일한 3개 검사를 중복 작성하지 않도록 내부 helper(예: `fn check_invariants(timestamp_ns, event_version, chain_id) -> Result<(), TypesError>`)로 추출. 단, helper는 crate-private (`pub` 아님). 본 spec은 helper 시그니처를 강제하지 않으며, 두 메서드가 같은 3개 invariant를 검사하는 것만이 계약이다.

**호출 책임 분배:**

| 경로 | 검증 메서드 |
|---|---|
| `EventBus::publish` 내부에서 envelope 생성 | `seal()` 호출 |
| Journal frame decode (Task 13) | decode 후 `validate()` 호출 의무 |
| Replay loader (Phase 2) | decode 후 `validate()` 호출 의무 |
| Wire/RPC decode (Phase 2+) | decode 후 `validate()` 호출 의무 |

**왜 두 메서드를 모두 두는가:**
- `seal()`만 두고 `validate()`를 빼면, deserialize 후 검증을 강제할 표준 entry가 없다.
- `validate()`만 두고 `seal()`을 빼면, 생성 시 `Result`를 통한 명시적 실패 경로가 사라진다 (생성자가 모든 입력을 받아들인 후 별도 호출에서 검증되어야 함 — caller-side 의무 부담↑).
- 두 메서드를 같은 invariant 셋으로 둘 다 노출하는 것이 가장 적은 우회 표면을 만든다.

**테스트에서의 직접 struct literal 허용:**

`validate()` reject 케이스는 corrupted/deserialized frame 시뮬레이션이 필요하다. `serde::Deserialize` / `rkyv::Deserialize`가 만들어낼 수 있는 invariant-violating envelope를 produce하는 가장 직접적인 방법은 unit test 모듈 내부에서 struct literal로 envelope를 직접 구성하는 것이다 — `mod tests` 가 `super::*`를 통해 같은 모듈의 private 필드에 접근 가능하기 때문에 이는 컴파일러가 허용하는 합법적 경로다.

이 우회는 **test-only 한정**이며, 공개 생성 계약(`seal()`이 유일한 _validated_ 생성 경로라는 본 spec의 §3.4 원칙)을 약화시키지 않는다. 테스트 모듈 외부의 어떤 consumer도 envelope의 private 필드를 struct literal로 채울 수 없다 — 이는 same-module 가시성 규칙이 보장한다. 본 spec은 §7.3 테스트(`validate_rejects_decoded_envelope_violations`)가 이 패턴을 사용함을 명시적으로 허용한다.

### 5.4 `event_version = 0` 정책 명시

`event_version = 0`이 reserved/uninitialized라는 결정은 frozen spec(`docs/specs/event-model.md`)에 직접 박힌 문장이 아니다. Phase 1 정책으로 다음 두 곳에 명시한다:

- `lib.rs`의 crate-level docstring (sequence/timestamp 소유권 정책 옆)
- `seal()` 메서드 docstring (위 5.1 참조)

Phase 2 이후 이 정책이 변경되면 이 두 docstring과 `TypesError::InvalidEnvelope` 반환 케이스를 동시에 갱신.

### 5.5 `into_payload()` — metadata-discard helper

```rust
impl<T> EventEnvelope<T> {
    /// Consumes the envelope and returns only the payload, **intentionally
    /// discarding all metadata** (sequence, timestamp_ns, source,
    /// chain_context, event_version, correlation_id).
    ///
    /// # When to use
    ///
    /// Terminal consumers that have already logged or otherwise persisted the
    /// envelope metadata upstream and only need the typed payload for further
    /// processing.
    ///
    /// # When NOT to use
    ///
    /// **Never** call this at bus, journal, or replay boundaries. Metadata
    /// preservation is a hard correctness requirement at those layers — losing
    /// `sequence` or `correlation_id` breaks ordering and trace linkage.
    ///
    /// If you need both the payload and a metadata field, prefer the getters
    /// (`.payload()`, `.sequence()`, etc.) over consuming the envelope.
    pub fn into_payload(self) -> T {
        self.payload
    }
}
```

### 5.6 `UnsupportedEventVersion`의 raise 지점 (types 외부)

types는 envelope의 version 필드를 표현하고 최소 invariant(`!= 0`)만 확인할 뿐 **버전 범위 강제는 하지 않는다**. `MAX_SUPPORTED_EVENT_VERSION` 같은 상수와 그에 따른 `UnsupportedEventVersion` 반환 책임은 다음 소비자가 자기 도메인에서 소유한다:

- Task 13 (`crates/journal`) decoder
- Phase 2 replay/normalizer
- 기타 envelope를 역직렬화하는 consumer

types는 이 variant의 표현 자리만 미리 제공.

### 5.7 크레이트 레벨 docstring

`lib.rs` 최상단에 다음 내용을 둔다:

```rust
//! # rust-lmax-mev-types
//!
//! Phase 1 type primitives for the LMAX-style MEV engine.
//!
//! ## Sequence and timestamp ownership
//!
//! `EventEnvelope::sequence` and `EventEnvelope::timestamp_ns` are
//! **bus-assigned** invariants. Production code populates them exclusively
//! through the `EventBus::publish(payload, meta)` API; the bus calls
//! `EventEnvelope::seal()` internally with the values it owns.
//!
//! Direct calls to `seal()` outside the bus implementation are reserved for
//! tests and replay/decode infrastructure. ADR-005 and the event-model spec
//! govern the canonical ownership contract.
//!
//! ## Validation at the deserialize boundary
//!
//! `seal()` is the single _validated_ construction path. `serde::Deserialize`
//! and `rkyv::Deserialize` reconstruct envelope fields directly, **bypassing
//! `seal()`**. Journal, replay, and decoder consumers MUST call
//! `EventEnvelope::validate()` immediately after any deserialization to
//! re-enforce the same invariants `seal()` would have rejected at construction.
//!
//! ## Phase 1 version policy
//!
//! `event_version = 0` is treated as reserved/uninitialized in Phase 1 and is
//! rejected by both `seal()` and `validate()`. This is a Phase 1 policy
//! decision, not a constraint from the frozen event-model spec.
```

---

## 6. Derive 매트릭스 및 `Cargo.toml`

### 6.1 통합 derive 매트릭스

| 타입 | Clone | Copy | Debug | PartialEq | Eq | Hash | rkyv* | serde** | thiserror::Error |
|---|---|---|---|---|---|---|---|---|---|
| `EventEnvelope<T>` | ✅ | ❌ | ✅ | ✅ | ✅ (T:Eq 조건부) | ❌ | ✅ | ✅ | ❌ |
| `EventSource` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ |
| `ChainContext` | ✅ | ❌ | ✅ | ✅ | ✅ | ❌ | ✅ | ✅ | ❌ |
| `PublishMeta` | ✅ | ❌ | ✅ | ✅ | ✅ | ❌ | ❌ | ✅ | ❌ |
| `JournalPosition` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ | ❌ |
| `SmokeTestPayload` | ✅ | ❌ | ✅ | ✅ | ✅ | ❌ | ✅ | ✅ | ❌ |
| `TypesError` | ❌ | ❌ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ |

\* rkyv = `rkyv::Archive` + `rkyv::Serialize` + `rkyv::Deserialize` 3개 함께
\** serde = `serde::Serialize` + `serde::Deserialize` 2개 함께
\*** `thiserror::Error`는 `Display + std::error::Error` 자동 부여 → 별도 `Display` derive 불필요

### 6.2 `crates/types/Cargo.toml`

```toml
[package]
name = "rust-lmax-mev-types"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
publish = false
description = "Phase 1 type primitives for the LMAX-style MEV engine"

[dependencies]
rkyv = { workspace = true }
serde = { workspace = true }
thiserror = { workspace = true }

[dev-dependencies]
bincode = { workspace = true }
```

**런타임 dep 3개:**
- `rkyv` — Archive/Serialize/Deserialize derive
- `serde` — Serialize/Deserialize derive (`features = ["derive"]`는 워크스페이스에서 활성)
- `thiserror` — Error derive

**dev dep 1개:**
- `bincode` — serde 어댑터 기반 cold-path serializer. §7.4의 serde round-trip 테스트 전용. 런타임 의존 아님.

### 6.3 제외한 워크스페이스 deps와 그 이유

| Dep | 제외 이유 |
|---|---|
| `tokio`, `crossbeam-channel`, `rocksdb` | bus/journal/snapshot 동작은 Task 12, 13. types는 pure data. |
| `tracing`, `metrics`, `metrics-exporter-prometheus`, `tracing-subscriber` | observability는 Task 15. types는 emit 안 함. |
| `toml` | config 로딩은 Task 14. |
| `proptest` | round-trip property test는 Task 13/17로 위임 (Q1). |
| `alloy-primitives` | Q5 결정. |
| `anyhow` | app 경계에서만 사용 (Q3). |

### 6.4 `[lints]` / 크레이트 레벨 lint

워크스페이스 `Cargo.toml`에 `[workspace.lints]`가 아직 없으므로 본 크레이트도 `[lints]` 블록 없음. `#![deny(missing_docs)]` 같은 강제 docs lint도 부착하지 않음 (Phase 1 단순화). 핵심 contract(`seal`, `into_payload`, sequence 소유권 정책)에만 docstring 집중.

### 6.5 워크스페이스 `Cargo.toml`의 rkyv feature 보정 (Task 11 작업 범위)

현재 워크스페이스 `Cargo.toml`은 `rkyv = { version = "0.8", features = ["validation"] }`로 선언되어 있다. **`validation`은 rkyv 0.8에 존재하지 않는 feature 이름이다.**

rkyv 0.8 docs(<https://docs.rs/crate/rkyv/latest/features>) 기준 정확한 feature 구성:
- **default features:** `bytecheck` + `std`
- `std`가 활성화되면 `alloc`이 자동 enable됨
- safe `rkyv::from_bytes::<T, E>(...)` 호출에는 `bytecheck`가 필요 (default에 이미 포함)

Task 11 구현자는 `cargo check -p rust-lmax-mev-types` 또는 워크스페이스 빌드에서 feature 오류가 나면 **bincode-only downgrade가 아니라 rkyv 설정 정정** 경로를 따라야 한다:

1. 워크스페이스 `Cargo.toml`의 `rkyv = { version = "0.8", features = ["validation"] }`을 다음 target으로 수정한다:
   ```toml
   rkyv = { version = "0.8", features = ["bytecheck"] }
   ```
   - `bytecheck`는 rkyv 0.8 default에 이미 포함되어 있지만 **의도를 명시**하기 위해 features에 부착한다.
   - `std` / `alloc`은 default에 포함되므로 별도 명시 불필요.
   - `default-features = false`는 사용하지 않는다 — std 기반 환경 그대로 유지.
2. 위 feature 구성이 docs.rs/rkyv/0.8 (<https://docs.rs/crate/rkyv/latest/features>)와 일치하는지 구현 시점에 1회 cross-check. 만약 0.8.x 마이너 버전 동안 feature 이름이 변경되었다면 docs를 신뢰하고 spec을 후속 update.
3. `cargo check -p rust-lmax-mev-types`로 빌드 통과 확인.
4. 변경된 rkyv 0.8 feature 이름과 그 근거를 커밋 메시지에 기록.

이 보정은 silent bincode-only downgrade와 구별되며 Q2 에스컬레이션 정책의 적용 대상이 아니다 (단순한 설정 정정이다).

---

## 7. 테스트 계획 (4개 인라인 `#[cfg(test)]`)

### 7.1 공통 fixture

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn valid_envelope() -> EventEnvelope<SmokeTestPayload> {
        let meta = PublishMeta {
            source: EventSource::Ingress,
            chain_context: ChainContext {
                chain_id: 1,
                block_number: 18_000_000,
                block_hash: [0xAB; 32],
            },
            event_version: 1,
            correlation_id: 42,
        };
        let payload = SmokeTestPayload {
            nonce: 7,
            data: [0xCD; 32],
        };
        EventEnvelope::seal(meta, payload, 100, 1_700_000_000_000_000_000)
            .expect("valid envelope should seal")
    }

    // ... tests below
}
```

`valid_envelope()`는 반드시 `EventEnvelope::seal(...).expect(...)`를 통한다. private field 우회 금지 — fixture조차 실제 생성 계약을 통과해 envelope를 구성.

### 7.2 테스트 1 — `seal_enforces_phase_1_invariants`

**목적:** §5.2의 invariant 3개에 대한 `seal()` reject + happy path 검증.

한 함수 안에서 다음을 순차 assertion으로 검증:

1. `seal()` reject: `timestamp_ns=0`, `event_version=0`, `chain_id=0` 각각 → `InvalidEnvelope { field, reason }` (정확한 `field`/`reason` 페어 매칭).
2. `seal()` happy path: 정상 입력으로 envelope 생성 → 모든 getter 반환값이 입력 그대로.
3. happy envelope에 대해 `env.validate()` → `Ok(())` (validate()와 seal()이 동일한 happy 입력을 동일하게 받아들이는지 cross-check).

### 7.3 테스트 2 — `validate_rejects_decoded_envelope_violations`

**목적:** §5.3의 `validate()`가 deserialize 우회로 들어온 invariant-violating envelope를 정확히 거부하는지 검증. unit test 모듈이 `super::*`로 private 필드에 접근 가능한 점을 이용해, struct literal로 invalid envelope를 직접 구성한다 — 이는 corrupted/deserialized frame이 만들어낼 수 있는 envelope shape를 정확히 시뮬레이션한다 (§5.3 test-only 우회 허용 참조).

```rust
#[test]
fn validate_rejects_decoded_envelope_violations() {
    let valid_chain = ChainContext {
        chain_id: 1,
        block_number: 18_000_000,
        block_hash: [0xAB; 32],
    };
    let valid_payload = SmokeTestPayload { nonce: 7, data: [0xCD; 32] };

    // Case 1: timestamp_ns = 0 (corrupted frame)
    let bad_ts = EventEnvelope::<SmokeTestPayload> {
        sequence: 100,
        timestamp_ns: 0,
        source: EventSource::Ingress,
        chain_context: valid_chain.clone(),
        event_version: 1,
        correlation_id: 42,
        payload: valid_payload.clone(),
    };
    assert!(matches!(
        bad_ts.validate(),
        Err(TypesError::InvalidEnvelope { field: "timestamp_ns", .. })
    ));

    // Case 2: event_version = 0
    let bad_ver = EventEnvelope::<SmokeTestPayload> {
        sequence: 100,
        timestamp_ns: 1_700_000_000_000_000_000,
        source: EventSource::Ingress,
        chain_context: valid_chain.clone(),
        event_version: 0,
        correlation_id: 42,
        payload: valid_payload.clone(),
    };
    assert!(matches!(
        bad_ver.validate(),
        Err(TypesError::InvalidEnvelope { field: "event_version", .. })
    ));

    // Case 3: chain_context.chain_id = 0
    let bad_chain = EventEnvelope::<SmokeTestPayload> {
        sequence: 100,
        timestamp_ns: 1_700_000_000_000_000_000,
        source: EventSource::Ingress,
        chain_context: ChainContext { chain_id: 0, ..valid_chain.clone() },
        event_version: 1,
        correlation_id: 42,
        payload: valid_payload,
    };
    assert!(matches!(
        bad_chain.validate(),
        Err(TypesError::InvalidEnvelope { field: "chain_context.chain_id", .. })
    ));
}
```

**테스트 의의:**
- §5.3의 우회 차단 계약을 실제 코드로 못박는다. validate() implementation에 회귀가 발생하면 이 테스트가 즉시 잡는다.
- struct literal 직접 생성은 테스트 모듈 한정 패턴이며, lib.rs 외부의 어떤 consumer도 흉내낼 수 없다 — same-module 가시성으로 보장.
- 이 테스트가 cover하는 영역은 binary frame 조작 기반 corruption test와 별개. binary 손상 시나리오는 여전히 Task 13 (journal)에서 별도로 검증.

### 7.4 테스트 3 — `serde_bincode_round_trip_preserves_envelope`



**목적:** serde derive가 envelope 전체에 대해 직렬화 → 역직렬화 round-trip을 보존하는지 검증. PHASE_1_DETAIL_REVISION 3.2의 "semantic equality" 요구사항을 가장 작은 surface에서 확인. 추가로, decode 후 `validate()` 호출이 happy path를 통과하는지도 확인 (§5.3의 호출 책임을 테스트로 시연).

```rust
#[test]
fn serde_bincode_round_trip_preserves_envelope() {
    let original = valid_envelope();
    let bytes = bincode::serialize(&original).expect("bincode serialize");
    let decoded: EventEnvelope<SmokeTestPayload> =
        bincode::deserialize(&bytes).expect("bincode deserialize");
    assert_eq!(original, decoded);
    // decode 경계의 표준 호출 패턴 시연
    decoded.validate().expect("decoded envelope must pass validate()");
}
```

`PartialEq + Eq` derive로 한 줄 비교로 충분. ADR-004의 cold-path serializer를 실제로 통과시키는 의미적 round-trip이다.

### 7.5 테스트 4 — `rkyv_archive_round_trip_preserves_envelope`

**목적:** rkyv archive → deserialize round-trip이 envelope 전체에 대해 동작하는지 검증. ADR-004 "all event bus message types must derive `rkyv::Archive`/`Serialize`/`Deserialize`" consequence를 컴파일 시점과 런타임 시점 양쪽에서 입증.

**rkyv 0.8 high-level API 정확한 형태:**

```rust
#[test]
fn rkyv_archive_round_trip_preserves_envelope() {
    let original = valid_envelope();

    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original)
        .expect("rkyv serialize");
    let decoded: EventEnvelope<SmokeTestPayload> =
        rkyv::from_bytes::<EventEnvelope<SmokeTestPayload>, rkyv::rancor::Error>(&bytes)
            .expect("rkyv deserialize");

    assert_eq!(original, decoded);
    // decode 경계의 표준 호출 패턴 시연
    decoded.validate().expect("decoded envelope must pass validate()");
}
```

API 시그니처 출처: <https://docs.rs/rkyv/latest/rkyv/fn.to_bytes.html>, <https://docs.rs/rkyv/latest/rkyv/fn.from_bytes.html>. safe `from_bytes`에는 `bytecheck` feature가 필요하며, `alloc`은 default feature `std`에 의해 자동 활성화된다 (6.5의 워크스페이스 보정으로 해결).

### 7.6 fallback 정책

위 테스트 작성 중 다음 상황이 발생할 수 있다:

| 발생 사례 | 대응 |
|---|---|
| rkyv 0.8 derive 매크로 컴파일 실패 (Phase 1 타입에 alloy 형이 없으므로 발생 가능성 낮음) | Q2: `[u8; N]` primitive로 fallback 시도 → 그래도 실패하면 ADR-004/event-model spec update 대상으로 surface (silent bincode-only 금지) |
| rkyv feature 누락 / 잘못된 feature 이름으로 인한 빌드 실패 | 6.5: 워크스페이스 `Cargo.toml`의 rkyv features를 docs.rs에서 확인한 정확한 이름으로 수정 |
| safe `from_bytes` 호출에 추가 feature 필요 | 워크스페이스 `Cargo.toml`의 `features = ["bytecheck"]` 외에 추가 feature가 docs에서 요구되면 명시적으로 부착 |
| `rkyv::rancor::Error` 외 다른 helper가 필요 | dev-dep에서 rkyv feature를 보강 또는 대체 helper 사용 |

각 보정은 silent downgrade가 아니라 명시적 설정 변경. 커밋 메시지에 이유 기록.

---

## 8. 의도적으로 위임한 항목 (Phase 1 본 작업 외 책임)

| 검증 항목 | 위임 대상 |
|---|---|
| 100k bus smoke (event loss 0, ordering 보존) | Task 17 integration smoke tests |
| Journal frame encode/decode byte 안정성 | Task 13 journal crate |
| Snapshot open/save/load/last_sequence smoke | Task 13 journal/snapshot |
| proptest 기반 N-event round-trip fuzzing | Task 13/17 |
| Replay determinism cross-validation | Phase 2 Replay Gate |
| `UnsupportedEventVersion` 실제 raise 경로 검증 | Task 13 decoder 또는 Phase 2 replay |
| 도메인 이벤트(`BlockObserved` 등) 정의 | Phase 2 (thin path 실 데이터 확인 후) |
| `MAX_SUPPORTED_EVENT_VERSION` 상수 | journal/replay/decoder consumer 크레이트 |
| `alloy-primitives` 직접 의존 | Phase 2 도메인 이벤트 크레이트 |
| `[workspace.lints]` 정책 | Task 18 CI 작업 |
| 모든 pub item의 docstring 강제 | Phase 1 외 (`#![deny(missing_docs)]` 미부착) |

`crates/types`의 테스트 책임은 "타입의 contract를 가장 작은 surface에서 증명"으로 한정. 통합·확장 검증은 consumer 크레이트와 smoke 단계가 담당.

---

## 9. 인수 기준 (Definition of Done)

Task 11이 완료되었다고 선언하기 위한 검증 가능 항목:

1. `crates/types/src/lib.rs`에 §4의 7개 타입 + `BlockHash` alias가 정의되어 있다.
2. `crates/types/Cargo.toml`이 §6.2와 정확히 일치한다.
3. `EventEnvelope`의 필드는 모두 private이며 §3.4의 메서드만 노출한다 — getter 7개 + `seal()` + `validate()` + `into_payload()` 총 10개.
4. `EventEnvelope::seal()`이 §5.2의 invariant 3개를 강제하고 위반 시 `TypesError::InvalidEnvelope`을 반환한다.
5. `EventEnvelope::validate()`가 `seal()`과 **동일한** 3개 invariant를 검사하며, 위반 시 같은 `field`/`reason` 페어를 가진 `TypesError::InvalidEnvelope`을 반환한다 (deserialize 경계 보호).
6. §7의 테스트 4개가 모두 통과한다 (`cargo test -p rust-lmax-mev-types`). 구체적으로:
   - 테스트 1: `seal()` reject + happy path + happy envelope의 `validate()` Ok cross-check.
   - 테스트 2: `validate()` reject — unit test 모듈 내부에서 struct literal로 invariant-violating envelope를 직접 구성해 3개 케이스 모두 거부됨을 검증.
   - 테스트 3 (serde) / 테스트 4 (rkyv): round-trip 후 `decoded.validate()`가 `Ok`임을 같이 검증.
7. `cargo build -p rust-lmax-mev-types`가 경고 없이 통과한다.
8. `cargo clippy -p rust-lmax-mev-types -- -D warnings`가 통과한다.
9. `cargo fmt --check`가 통과한다.
10. crate-level docstring(§5.7)과 `seal()` / `validate()` / `into_payload()` docstring(§5.1, §5.3, §5.5)이 부착되어 있다. crate-level docstring은 deserialize 경계의 `validate()` 호출 의무를 명시한다.
11. (필요 시) §6.5에 따른 워크스페이스 `Cargo.toml`의 rkyv feature 정정이 같은 커밋 또는 직전 커밋으로 반영되어 있다.

다음 단계 (Task 12 `crates/event-bus`)가 본 크레이트의 `EventEnvelope`/`EventSource`/`ChainContext`/`PublishMeta`/`TypesError`를 import하여 컴파일 가능해야 한다. Task 13 (`crates/journal`) 이후의 deserialize 소비자는 decode 후 `EventEnvelope::validate()`를 호출하는 표준 패턴을 따른다.

---

## 10. 교차 참조

- `docs/specs/event-model.md` — frozen envelope 스키마, derive 요구사항, PublishMeta 책임 분배.
- `docs/adr/ADR-004-rpc-evm-stack-selection.md` — rkyv hot / bincode cold 직렬화 전략, "all event bus message types must derive rkyv" consequence.
- `docs/adr/ADR-005-event-bus-implementation-policy.md` — single-consumer bounded queue, sequence/timestamp 소유권.
- `PHASE_1_DETAIL_REVISION.md` — §3.2 Replay Contract 의미 분리, §3.4 sequence assignment 위치, `JournalPosition { sequence, byte_offset }` 도입.
- `CLAUDE.md` — rkyv 0.8 호환성 주의, Task 11 위치.
- `Cargo.toml` (workspace 루트) — 워크스페이스 deps 사전 정의 및 §6.5 rkyv feature 정정 대상.
- §11 Open Issues — Task 11 비차단 후속 보정 항목 (ADR-004 bincode API 표기 불일치).

---

## 11. Open Issues (Task 11 비차단, 후속 작업에서 처리)

### 11.1 ADR-004의 bincode API 표기 불일치

`docs/adr/ADR-004-rpc-evm-stack-selection.md` line 48의 Consequences는 다음과 같이 적혀 있다:

> Snapshot types must derive `bincode::Encode` and `bincode::Decode`.

그러나 워크스페이스 `Cargo.toml`은 `bincode = "1.3"`로 고정되어 있고, **bincode 1.x는 자체 `Encode`/`Decode` derive가 없으며 serde 어댑터(`bincode::serialize` / `bincode::deserialize`)를 통해 동작한다**. `Encode`/`Decode` derive는 bincode 2.x에서 도입된 별도 API다.

본 spec(§7.4 serde round-trip 테스트)은 `bincode = 1.3`의 serde 어댑터 형태(`bincode::serialize` / `bincode::deserialize`)를 사용하므로 Task 11 작업 자체는 영향받지 않는다. 다만 ADR-004 Consequences 문장이 현재 워크스페이스 의존성과 일치하지 않으므로 다음 중 하나로 후속 보정해야 한다:

1. ADR-004 Consequences를 "Snapshot types must implement `serde::Serialize` and `serde::Deserialize`, encoded via `bincode` (1.x serde adapter)"로 수정.
2. 또는 워크스페이스 `Cargo.toml`을 `bincode = "2.x"`로 업그레이드하고 ADR-004 문구는 그대로 유지하되 Migration 노트를 추가.

**처리 시점:** Task 13 (`crates/journal`) 시작 전 — journal/snapshot이 실제 bincode를 호출하는 첫 consumer이므로 그 작업 진입 직전에 ADR을 정합 상태로 정리하는 것이 안전하다. Task 11에서는 본 issue를 등록만 하고 spec 자체에는 수정을 가하지 않는다.
