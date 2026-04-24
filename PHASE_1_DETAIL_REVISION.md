# Phase 1 Detail Revision

작성일: 2026-04-24  
문서 상태: Draft v0.1  
대상 문서: `.superpowers/brainstorm/1875-1777019549/phase-1-detail.html`  
용도: 본 문서는 Phase 1 상세 설계 리뷰 결과를 정리한 별도 보완안이다. Phase 2로 넘어가기 전에 Phase 1 설계 문서의 구조적 충돌과 모호한 계약을 정리하는 것을 목표로 한다.

## 1. 개정 목적

현재 Phase 1 설계는 방향이 좋고, 범위 통제도 잘 되어 있다. 특히 아래 강점은 유지해야 한다.

- `CREATE / DEFER` 구분이 명확하다.
- `CI`와 `Observability`를 완료 조건으로 끌어올렸다.
- empty crate 생성 금지 원칙이 분명하다.
- `Grafana`, 실데이터, ingress stub 등을 뒤로 미뤄 일정이 퍼지는 것을 막고 있다.

다만 그대로 구현에 들어가면 추후 재작업 가능성이 높은 구조적 이슈가 몇 가지 있다. 본 개정안은 그 이슈를 수정하기 위한 입력 문서다.

## 2. 리뷰 요약

이번 리뷰에서 확인된 핵심 보완 포인트는 아래 5개다.

| 우선순위 | 항목 | 요약 |
|---|---|---|
| High | EventBus 의미론 충돌 | `crossbeam bounded`와 `independent cursor` 요구사항이 양립하지 않음 |
| High | Replay Contract 과도 명세 | 역직렬화 후 객체에 대해 `byte-identical` 요구는 부정확함 |
| Medium | SnapshotStore 검증 부재 | trait은 있으나 CI/DoD에서 거의 검증하지 않음 |
| Medium | Sequence assignment 지점 모호 | `publish()` 입력과 출력 계약이 모순적임 |
| Low | CI merge 문구 애매 | `main branch pass`보다 `PR/default branch checks` 기준이 적절 |

## 3. Mandatory Revisions

### 3.1 EventBus 의미론을 정직하게 수정

#### 문제

현재 설계는 아래 두 가지를 동시에 요구한다.

- `crossbeam::channel::bounded` 기반 Phase 1 구현
- `subscribe(name)` + `independent cursor` + `consumer_lag per consumer`

하지만 `crossbeam bounded channel`은 본질적으로 work-queue 성격이다. 여러 consumer가 붙으면 각 이벤트가 모든 consumer에게 전달되는 것이 아니라 분배된다. 따라서 현재 문서의 bus trait과 concrete impl은 구조적으로 충돌한다.

#### 권장 결정

아래 둘 중 하나를 고정해야 한다.

**권장안 A**

- Phase 1의 `EventBus`를 `single logical consumer queue`로 축소한다.
- `independent cursor` 요구사항은 `Phase 2` 또는 `ADR-005` 개정 시점으로 미룬다.
- `crossbeam bounded`는 유지한다.

**대안 B**

- `independent cursor` 요구사항을 유지한다.
- 대신 `crossbeam bounded`를 버리고 `bounded broadcast log` 또는 `cursor-aware bus`로 Phase 1 구현을 변경한다.

1인 개발 + 일정 제약을 고려하면 **권장안 A**가 더 적절하다.

#### 권장 문구

`Phase 1 EventBus는 single logical consumer bounded queue로 정의한다. crossbeam bounded 구현은 backpressure와 ordering 검증에 집중하며, multi-consumer independent cursor semantics는 Phase 2 이후 별도 event-log abstraction으로 확장한다.`

#### 문서 반영 항목

- `subscribe(name)`를 `consumer()` 또는 `take_consumer()` 형태로 단순화
- `consumer_lag per consumer`를 `current_depth`, `published_total`, `consumed_total`, `saturation_events` 중심으로 조정
- anti-pattern에는 `Phase 1에서 pseudo-disruptor를 억지로 만들지 않는다`를 추가

### 3.2 Replay Contract를 현실적인 계약으로 수정

#### 문제

현재 문서는 아래를 요구한다.

`deserialized envelope must be byte-identical to the original`

이 표현은 Rust 값 객체 기준으로는 부정확하다. 메모리 padding, 내부 레이아웃, 직렬화 방식 차이 때문에 역직렬화 후 객체가 “바이트 단위로 동일”하다는 요구는 구현자를 잘못된 방향으로 몰 수 있다.

#### 권장 결정

Replay Contract는 아래 두 가지로 분리한다.

1. `serialized frame bytes` 안정성
2. `deserialized value equality` 안정성

#### 권장 문구

`Phase 1 Replay Contract는 두 가지를 보장한다. 첫째, 동일한 EventEnvelope를 동일한 serializer 설정으로 기록했을 때 journal frame bytes가 재현 가능해야 한다. 둘째, journal에서 읽어 역직렬화한 EventEnvelope는 원본과 semantic equality를 만족해야 한다.`

#### 구현 기준

- `EventEnvelope`에 `PartialEq` 또는 동등한 비교 가능 계약 부여
- property test는 아래로 분리
  - `serialize -> deserialize -> equal`
  - `frame encode -> frame decode -> same payload + same sequence + same metadata`

### 3.3 SnapshotStore smoke test를 CI/DoD에 포함

#### 문제

Phase 1에서 `SnapshotStore`와 `RocksDbSnapshot`을 생성 대상으로 두고 있지만, 현재 DoD와 CI 항목에는 이에 대한 직접적인 검증이 없다.

이 상태면 stub이 존재만 하고, 실제로 열기/쓰기/읽기가 가능한지 확인하지 못한 채 Phase 2로 넘어갈 수 있다.

#### 권장 결정

최소 수준의 smoke test를 Phase 1 완료 조건에 추가한다.

#### 권장 문구

`RocksDbSnapshot must pass a smoke test covering open -> save -> load -> last_sequence on a temporary database path.`

#### CI 추가 항목

현재 6단계에 아래를 추가해 `7단계`로 만든다.

- `snapshot store smoke test (open/save/load/last_sequence)`

#### DoD 추가 항목

- `RocksDbSnapshot smoke test passes on temporary path`

### 3.4 Sequence assignment 지점을 명확히 고정

#### 문제

현재 문서는 `EventEnvelope`에 `sequence`가 이미 들어 있고, 동시에 `publish()`가 sequence를 할당한다고 적고 있다.

즉 아래가 동시에 존재한다.

- 입력: `EventEnvelope<E>`
- 설명: `bus-assigned sequence`
- 반환: `Result<u64, BusError>`

이 계약은 caller가 빈 sequence를 넣어야 하는지, bus가 envelope를 복사해 sequence를 덮어쓰는지 불명확하다.

#### 권장 결정

canonical sequence assignment는 bus 내부에서 일어나야 한다. 따라서 API도 그에 맞춰 정리한다.

#### 권장안

**권장안 A**

`fn publish(&self, payload: E, meta: PublishMeta) -> Result<EventEnvelope<E>, BusError>`

**권장안 B**

`fn publish(&self, envelope: UnsequencedEnvelope<E>) -> Result<EventEnvelope<E>, BusError>`

가장 단순한 것은 **권장안 A**다.

#### 추가 권장 사항

Journal append 결과도 `byte offset`만 반환하지 말고 아래처럼 위치 정보를 함께 반환하는 편이 좋다.

`JournalPosition { sequence, byte_offset }`

이렇게 하면 replay, diagnostics, snapshot resume 기준점이 더 명확해진다.

### 3.5 CI 기준 문구를 merge 흐름에 맞게 수정

#### 문제

현재 문구는 `main branch passes` 쪽에 가까운데, 실무적으로는 PR gate와 default branch gate 기준이 더 적절하다.

#### 권장 문구

`CI pipeline runs all required checks on pull requests and default branch, and all checks must be green before merge.`

## 4. 권장 텍스트 치환안

아래는 문서에 바로 반영 가능한 짧은 치환안이다.

### 4.1 EventBus 섹션

기존 취지:

- crossbeam bounded
- subscribe
- independent cursor

권장 치환:

`Phase 1 EventBus는 single logical consumer bounded queue로 정의한다. publish는 canonical sequence를 내부에서 할당하고, consumer는 하나의 domain consumer handle을 제공한다. multi-consumer independent cursor semantics는 Phase 2 이후 event-log abstraction으로 확장한다.`

### 4.2 Replay Contract 섹션

권장 치환:

`Phase 1 Replay Contract는 serialize/decode 안정성과 semantic equality를 보장한다. 동일한 serializer 설정으로 기록된 frame은 decode 후 원본 EventEnvelope와 동일한 의미론적 값을 반환해야 한다.`

### 4.3 CI 섹션

권장 치환:

`CI consists of 7 required checks: fmt, clippy, test, deny, 100k bus smoke, journal round-trip, snapshot store smoke. All checks must pass on pull requests and default branch before merge.`

## 5. DoD 개정안

현재 DoD에 아래 항목을 추가 또는 교체하는 것을 권장한다.

### 추가

- `Snapshot store smoke test passes (open/save/load/last_sequence)`
- `Sequence assignment ownership is documented and enforced by API`
- `Replay contract is verified by semantic equality and stable frame decode behavior`

### 교체

기존:

- `Journal round-trip: write N events, read back, byte-identical to originals`

교체:

- `Journal round-trip: write N events, decode them back, and confirm semantic equality with original envelopes plus sequence preservation`

## 6. 선택지 정리

Phase 1 설계를 고칠 때 가장 중요한 결정은 아래다.

### 선택 1. EventBus

- `권장`: Phase 1은 single-consumer bounded queue
- `비권장`: crossbeam을 유지하면서 multi-cursor bus처럼 문서화

### 선택 2. Replay Contract

- `권장`: semantic equality + stable frame decode
- `비권장`: deserialized object byte identity

### 선택 3. Snapshot 검증

- `권장`: CI 7단계에 snapshot smoke test 추가
- `비권장`: stub만 두고 검증 없이 Phase 2 진행

## 7. 권장 결론

Phase 1 설계는 전체적으로 좋고, 방향도 맞다. 다만 다음 세 가지는 수정 후 진행하는 것이 안전하다.

1. EventBus 의미론을 Phase 1 수준에 맞게 축소하거나 구현 방식을 바꾼다.
2. Replay Contract를 현실적인 검증 기준으로 고친다.
3. SnapshotStore smoke test를 CI/DoD에 넣는다.

이 세 가지가 반영되면 Phase 1은 구현 단계로 내려도 충분히 단단한 상태가 된다.
