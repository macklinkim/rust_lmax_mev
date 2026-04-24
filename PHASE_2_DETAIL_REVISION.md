# Phase 2 Detail Revision

작성일: 2026-04-24  
문서 상태: Draft v0.1  
대상 문서: `.superpowers/brainstorm/1875-1777019549/phase-2-detail.html`  
용도: 본 문서는 Phase 2 상세 설계 리뷰 결과를 정리한 별도 보완안이다. Phase 2 구현 시작 전에 replay 경계, fixture recorder 위치, V3 상태 범위, rollback 메커니즘, dedup/replacement 키 설계를 명확히 하기 위한 입력 문서다.

## 1. 개정 목적

현재 Phase 2 설계는 전체 구조가 좋고, thin path 통제도 잘 되어 있다. 특히 아래 강점은 유지해야 한다.

- `RawChainEvent -> NormalizedEvent -> PoolStateSnapshot -> OpportunityEvent` 경계를 명시했다.
- `fixture_reorg`, `fixture_duplicate`, `fixture_replacement`, `fixture_resync`를 구현 전에 고정했다.
- scope lock이 강하고, anti-pattern으로 범위 확장을 잘 막고 있다.
- replay gate와 state correctness gate를 Phase 2 출구에 고정했다.

다만 구현에 바로 들어가면 이후 구조 변경이나 replay/test 체계 재작업이 생길 가능성이 있는 핵심 모호성이 남아 있다. 본 개정안은 그 부분을 선제적으로 닫기 위한 문서다.

## 2. 리뷰 요약

| 우선순위 | 항목 | 요약 |
|---|---|---|
| High | replay fixture 경계 불일치 | 파이프라인은 `EventEnvelope<NormalizedEvent>`인데 fixture는 `NormalizedEvent` 기준으로 적혀 있음 |
| High | fixture recorder 위치 미정 | single-consumer bus 구조에서 recorder를 어디에 삽입할지 문서화되지 않음 |
| Medium | V3 profit estimate 범위 과도 | 현재 상태 모델만으로는 실행 수준의 `estimated_profit_wei`가 과도할 수 있음 |
| Medium | reorg rollback 메커니즘 부재 | 책임은 적혀 있으나 rollback 데이터 구조와 방식이 없음 |
| Medium | dedup_key 설계 모호 | duplicate/replacement 판정을 단일 hash key로 처리하면 의미론이 약함 |

## 3. Mandatory Revisions

### 3.1 Replay 입력 경계를 명시적으로 고정

#### 문제

문서상 Phase 2 파이프라인의 state-engine 입력은 `EventEnvelope<NormalizedEvent>`다.  
하지만 fixture 포맷 설명은 `input_events.bin = sequence of serialized NormalizedEvents`라고 적고 있어, 실제 live path와 replay path의 경계가 다르다.

이 상태에서는 replay가 아래 필드를 제대로 검증하지 못한다.

- `sequence`
- `timestamp_ns`
- `event_version`
- `correlation_id`
- `chain_context`

#### 권장 결정

아래 둘 중 하나를 명시적으로 선택해야 한다.

**권장안 A**

- replay fixture의 canonical 입력은 `EventEnvelope<NormalizedEvent>`다.
- fixture 파일은 envelope 전체를 직렬화해서 저장한다.
- replay는 실제 live path의 post-bus/post-normalizer 이벤트를 그대로 재생한다.

**대안 B**

- replay 입력 경계를 `pre-bus normalized payload`로 낮춘다.
- 대신 replay gate 설명에서 envelope 수준 필드 검증을 제외한다.

프로젝트의 replay 중심 설계를 고려하면 **권장안 A**가 더 적절하다.

#### 권장 문구

`Phase 2 replay fixture의 canonical unit은 EventEnvelope<NormalizedEvent>다. Fixture replay는 live path와 동일한 envelope schema를 사용하며, sequence, chain_context, event_version, correlation_id를 포함한 전체 경계를 검증한다.`

### 3.2 Fixture Recorder의 삽입 위치를 고정

#### 문제

Phase 2는 fixture recorder로 live 데이터를 캡처한다고 하지만, Phase 1 설계상 bus는 `single-consumer queue`다.  
따라서 recorder를 consumer처럼 뒤에 붙일 수 없다.

지금처럼 recorder 위치가 미정이면 구현 시 아래 중 하나로 흔들릴 수 있다.

- bus 이전에서 기록
- bus 이후에서 기록
- journal append 시점에서 기록
- normalizer 내부에서 별도 tee

#### 권장 결정

fixture recorder는 `normalizer -> bus.publish()` 직전 또는 `journal append` 직후 중 하나로 고정해야 한다.

**권장안**

- canonical recorder 삽입 위치는 `normalizer output / pre-bus publish`다.
- recorder는 `EventEnvelope<NormalizedEvent>`를 기록한다.
- journal append는 Phase 2에서도 가능하지만, fixture source of truth는 pre-bus normalized envelope로 본다.

#### 권장 문구

`FixtureRecorder is attached at the normalizer output boundary, immediately before bus.publish(). It records canonical EventEnvelope<NormalizedEvent> units. It is not implemented as a second EventBus consumer.`

### 3.3 OpportunityEvent의 의미를 후보 탐지 수준으로 낮춰서 명확화

#### 문제

현재 `OpportunityEvent`에는 `estimated_profit_wei`가 들어가 있는데, Phase 2 상태 모델은 V3에서 `sqrt_price_x96`, `tick`, `liquidity` 정도만 포함한다.  
이 정도로는 여러 tick을 넘는 실제 swap 영향 계산이나 execution-quality 수준의 수익 계산을 안정적으로 하기는 어렵다.

즉 현재 문서는 아래 둘 사이에서 모호하다.

- 단순 candidate detection
- execution-grade profit estimation

#### 권장 결정

Phase 2의 `OpportunityEvent`는 “실행 확정 전 후보 탐지 이벤트”로 정의하는 편이 맞다.

#### 권장 문구

`Phase 2 OpportunityEvent is a candidate detection artifact, not an execution-grade profitability guarantee. estimated_profit_wei should be interpreted as heuristic candidate score or coarse estimate unless full V3 liquidity traversal is modeled.`

#### 대안

만약 `estimated_profit_wei`를 유지하고 싶다면 아래 중 하나를 추가해야 한다.

- V3 tick bitmap + initialized ticks 일부 추적
- profit field를 `coarse_profit_estimate_wei`로 rename

가장 현실적인 건 rename 또는 의미 축소다.

### 3.4 Reorg rollback 메커니즘을 contract 수준으로 추가

#### 문제

문서에는 normalizer가 reorg를 감지하고 state-engine이 rollback한다고 적혀 있지만, 실제 rollback이 무엇을 기준으로 작동하는지는 적혀 있지 않다.

이 상태면 구현은 쉽게 아래 둘 중 하나로 흐른다.

- 사실상 full resync만 수행
- 부분 rollback을 하려다 상태 정합성이 깨짐

#### 권장 결정

Phase 2 문서에 rollback 메커니즘을 하나 명시적으로 추가한다.

**권장안 A**

- block-indexed undo log
- 각 block 적용 시 state delta를 기록
- reorg 시 common ancestor까지 역적용 후 새 블록 적용

**권장안 B**

- short-horizon snapshot ring
- 최근 N개 블록 state snapshot 유지
- reorg 시 가장 가까운 snapshot으로 되돌린 뒤 재적용

1차 thin path 관점에서는 **권장안 A**가 문서상 더 자연스럽다.

#### 권장 문구

`State reorg handling is implemented via block-indexed undo records. Every applied block produces reversible state deltas sufficient to rollback to the last common ancestor before re-applying replacement blocks. Full resync remains a fallback path, not the primary reorg mechanism.`

#### DoD 추가 권장

- `reorg rollback path exercised without full resync in fixture_reorg`

### 3.5 Dedup / Replacement 키를 typed key로 분리

#### 문제

현재 `dedup_key: u64 (hash-based)`는 duplicate, replacement, late event 등 서로 다른 의미론을 하나의 불투명한 값으로 뭉개 버린다.

이 방식의 문제:

- duplicate는 보통 `tx_hash` 또는 `(block_hash, log_index)`가 핵심
- replacement는 `(sender, nonce)`가 핵심
- reorg 관련 이벤트는 block lineage가 핵심

단일 64-bit hash는 collision 위험보다도, 의미론이 가려지는 점이 더 큰 문제다.

#### 권장 결정

dedup/replacement 판정용 typed key를 나눈다.

#### 권장 예시

- `DuplicateKey`
  - `PendingTxByHash(TxHash)`
  - `LogByBlockAndIndex(BlockHash, u32)`
  - `BlockByHash(BlockHash)`

- `ReplacementKey`
  - `PendingTxBySenderNonce(Address, u64)`

#### 권장 문구

`Normalizer does not rely on a single opaque dedup_key for all event classes. Duplicate suppression and replacement detection use typed keys appropriate to each event kind (e.g. tx_hash for duplicates, sender+nonce for replacements).`

## 4. 권장 텍스트 치환안

### 4.1 Fixture Format 섹션

기존 취지:

- `input_events.bin`에 serialized `NormalizedEvents`

권장 치환:

`Each fixture directory contains input_events.bin as a sequence of serialized EventEnvelope<NormalizedEvent> units, expected_state.json, and expected_opportunities.json. Replay uses the same envelope schema as the live normalized path.`

### 4.2 Fixture Recorder 섹션

권장 치환:

`FixtureRecorder is attached to the normalizer output boundary, before bus.publish(). It records canonical EventEnvelope<NormalizedEvent> units and is not modeled as a second EventBus consumer.`

### 4.3 OpportunityEvent 섹션

권장 치환:

`estimated_profit_wei in Phase 2 is a coarse candidate estimate, not an execution-grade profitability guarantee. Final simulation-validated profitability remains a Phase 3 concern.`

### 4.4 Reorg Handling 섹션

권장 치환:

`State engine performs reorg rollback using block-indexed undo records. Full resync is reserved as a recovery fallback when rollback preconditions fail or state confidence is lost.`

### 4.5 Normalizer Keying 섹션

권장 치환:

`Duplicate suppression and replacement detection use typed event keys rather than a single opaque hash-based dedup key.`

## 5. DoD 개정안

현재 DoD에 아래 항목을 추가하거나 치환하는 것을 권장한다.

### 추가

- `Fixture input format is EventEnvelope<NormalizedEvent>, identical to live normalized boundary`
- `FixtureRecorder captures canonical envelopes at normalizer output`
- `fixture_reorg verifies rollback path without requiring full resync as primary mechanism`
- `OpportunityEvent candidate estimate semantics documented as coarse, not execution-final`
- `Typed duplicate/replacement keys implemented and covered by fixture tests`

### 교체

기존:

- `External mempool feed connected and deduped against local node`

교체:

- `External mempool feed connected and duplicate/replacement logic validated using typed keys against local node data`

## 6. 우선순위

가장 먼저 닫아야 할 것은 아래 순서다.

1. replay fixture canonical boundary
2. fixture recorder insertion point
3. rollback primary mechanism
4. OpportunityEvent 의미 수준
5. typed dedup/replacement keys

## 7. 권장 결론

Phase 2 설계는 전체적으로 좋고, 바로 구현에 내려갈 수 있는 수준에 가깝다. 다만 아래 3가지는 구현 전에 반드시 문서로 먼저 닫는 것이 좋다.

1. replay fixture와 live path의 canonical boundary를 동일하게 맞춘다.
2. fixture recorder를 second consumer가 아니라 normalizer output boundary에 고정한다.
3. reorg rollback의 primary 메커니즘을 문서로 확정한다.

이 세 가지가 정리되면, 나머지 `OpportunityEvent 의미 수준`과 `typed key` 보완까지 포함해 Phase 2는 꽤 안정적인 설계가 된다.
