# Rust LMAX MEV Project Base

작성일: 2026-04-24  
문서 상태: Draft v0.1  
용도: 본 문서는 Rust 기반 LMAX 스타일 MEV 프로젝트의 기준 문서다. 이후 ChatGPT 계열 모델, Claude 계열 에이전트, 개발자 협업 시 공통 베이스 문서로 사용한다.

## 1. 문서 목적

이 문서는 아래 목적을 가진다.

- 프로젝트의 목표, 범위, 기술 방향을 빠르게 공유한다.
- Rust + LMAX 스타일 아키텍처 원칙을 고정한다.
- 초기 구현 순서와 마일스톤을 정리한다.
- 다른 AI 에이전트가 작업할 때 지켜야 할 규칙을 제공한다.
- 향후 ADR, 스펙 문서, 이슈 분해의 상위 기준점으로 사용한다.

## 2. 프로젝트 개요

### 2.1 프로젝트 목표

본 프로젝트의 1차 목표는 Rust를 이용해 저지연 이벤트 기반의 MEV 탐지 및 실행 엔진을 구축하는 것이다.  
핵심 방향은 LMAX Disruptor 철학을 참고한 단일 작성자 중심의 상태 갱신, 고정 크기 버퍼 기반 이벤트 전달, 스테이지 분리, 예측 가능한 지연시간 관리다.

### 2.2 현재 기준 가정 및 확정 사항

현재 기준으로 아래 항목은 프로젝트의 기본 가정이자 초기 확정 사항이다.

- 대상 생태계는 우선 EVM 계열로 가정한다.
- 1차 타깃 네트워크는 `Ethereum mainnet`으로 우선 확정한다.
- `BSC`, `Base`, `Arbitrum`은 2차 확장 후보로 유지한다.
- 다른 EVM 체인은 Ethereum mainnet과 동일한 실행 모델을 공유하지 않으므로, sequencer, PBS, private orderflow 차이를 반영한 체인별 adapter 전략을 전제로 검토한다.
- 1차 릴리즈는 `searcher` 관점의 시스템에 집중한다.
- 첫 번째 구현 전략은 `DEX Arbitrage`로 확정한다.
- `Backrun`은 2순위, `Liquidation`은 3순위 후보로 유지한다.
- `sandwich`와 같이 정책/윤리/규제 민감도가 높은 전략은 별도 의사결정 없이는 기본 범위에서 제외한다.
- 초기 단계에서는 온체인 실행과 오프라인 리플레이/시뮬레이션을 모두 지원해야 한다.

위 항목 중 변경이 발생하면 본 문서를 우선 업데이트한다.

### 2.3 최종 지향점

장기적으로는 다음을 만족하는 실행 플랫폼을 지향한다.

- 실시간 시장 데이터와 mempool 이벤트를 통합 수집
- 결정론적 상태 갱신
- 전략 모듈의 독립적 개발 및 실험 가능
- 백테스트, 리플레이, 섀도우 모드, 실거래 전환이 같은 이벤트 모델 위에서 동작
- 저지연 처리와 운영 안정성을 동시에 확보

## 3. 범위 정의

### 3.1 In Scope

- Rust workspace 기반 모노레포 구성
- 이벤트 버스 및 LMAX 스타일 파이프라인 설계
- 시장 데이터 수집기
- mempool / block / log / state ingestion
- 상태 엔진
- 전략 실행 엔진
- 리스크 가드
- 번들 생성 및 제출 레이어
- 시뮬레이터 / 리플레이 엔진
- 관측성, 로깅, 메트릭, 알림
- 테스트/벤치/리플레이 기반 성능 검증

### 3.2 Out of Scope for v1

- 범용 멀티체인 풀스택 지원
- 검증자/빌더 자체 구현
- 대규모 GUI 우선 개발
- 초저수준 커널/FPGA 최적화
- 규제/법률 검토 자동화
- 전략 포트폴리오를 무제한 확장하는 범용 플랫폼화

## 4. 핵심 설계 원칙

### 4.1 LMAX 스타일 원칙

- 가능한 한 `single writer per state shard` 원칙을 따른다.
- hot path에서는 lock contention을 최소화한다.
- hot path에는 가능한 한 동적 메모리 할당을 넣지 않는다.
- 이벤트는 가능한 한 사전에 정의된 구조체와 고정된 흐름으로 전달한다.
- 느린 I/O와 계산 집약 로직은 파이프라인 단계로 분리한다.
- backpressure를 무시하지 않고, 큐 적체를 주요 장애 신호로 본다.
- 처리 순서가 중요한 도메인 이벤트는 순서를 보존한다.

### 4.2 Rust 구현 원칙

- hot path는 필요 시 async보다 전용 스레드 + lock-free / bounded 구조를 우선 검토한다.
- async는 네트워크 I/O, 외부 RPC, 파일 I/O 등 경계 영역에 우선 적용한다.
- `unsafe`는 금지하지 않지만, 벤치마크와 명확한 정당화 없이는 도입하지 않는다.
- 에러 타입은 전파 가능하고 관측 가능해야 한다.
- 구조체/이벤트/상태 모델은 직렬화 가능성과 replay 가능성을 염두에 두고 설계한다.
- 성능 최적화는 측정 후 진행한다. 추측 기반 최적화는 피한다.

### 4.3 운영 원칙

- 실거래보다 먼저 `replay -> simulation -> shadow mode -> limited prod` 순서를 따른다.
- 결정 경로는 사후 분석이 가능해야 한다.
- 수익성보다 먼저 안정성, 재현성, 리스크 통제를 확보한다.
- 체인 reorg, RPC 일시 장애, relay 실패를 정상 상황으로 간주하고 설계한다.

## 5. 시스템 상위 아키텍처

### 5.1 논리 파이프라인

권장 파이프라인은 아래와 같다.

`Ingress -> Normalize -> Event Ring -> State Update -> Opportunity Detection -> Strategy Eval -> Risk Guard -> Bundle Build -> Submission -> Feedback -> Persistence/Replay`

### 5.2 주요 컴포넌트

1. `ingress`
   외부 데이터 수집 계층. websocket, rpc, block stream, mempool source, relay/builder 응답 수신.

2. `normalizer`
   체인/프로토콜별 입력을 내부 표준 이벤트 형식으로 변환.

3. `event_bus`
   고정 크기 ring buffer 또는 이에 준하는 bounded event pipeline.

4. `state_engine`
   블록, 트랜잭션, 풀 상태, 토큰 상태, 전략별 파생 상태를 갱신.

5. `opportunity_engine`
   이벤트 기반으로 후보 기회를 탐지하고 우선순위를 계산.

6. `strategy_engine`
   전략별 계산 수행. 입력 상태와 정책에 따라 실행 후보 생성.

7. `risk_engine`
   손실 한도, 가스 한도, 재진입 금지 조건, 중복 제출 방지, 시장 충격 조건 등 검사.

8. `execution_engine`
   번들/트랜잭션 생성, 서명, relay 제출, 결과 수집 담당.

9. `simulator`
   과거 이벤트 replay, 시뮬레이션, 전략 회귀 검증 담당.

10. `observability`
    메트릭, 로그, tracing, 알림, 운영 대시보드 제공.

## 6. 제안 리포 구조

초기 모노레포 구조는 아래를 기본안으로 한다.

```text
/
|- Cargo.toml
|- rust-toolchain.toml
|- rustfmt.toml
|- clippy.toml
|- .gitignore
|- PROJECT_BASE.md
|- docs/
|  |- adr/
|  |- specs/
|  `- runbooks/
|- config/
|  |- base/
|  |- dev/
|  |- test/
|  `- prod/
|- scripts/
|- benches/
|- fixtures/
`- crates/
   |- common/
   |- config/
   |- types/
   |- event_bus/
   |- ingress/
   |- normalizer/
   |- state_engine/
   |- opportunity_engine/
   |- strategy_api/
   |- strategies/
   |- risk_engine/
   |- execution_engine/
   |- relay_client/
   |- simulator/
   |- persistence/
   |- observability/
   `- app/
```

### 6.1 crate 역할 초안

- `common`: 공통 유틸리티, 공통 에러, 시간/ID/헬퍼
- `config`: 설정 로딩, 환경별 병합, 검증
- `types`: 도메인 타입, 공용 이벤트/명세
- `event_bus`: ring buffer, sequence, barrier, consumer abstraction
- `ingress`: 외부 데이터 수집
- `normalizer`: 원시 입력을 내부 표준 이벤트로 변환
- `state_engine`: 상태 저장소 및 상태 갱신 로직
- `opportunity_engine`: 후보 기회 탐지
- `strategy_api`: 전략 인터페이스
- `strategies`: 실제 전략 구현
- `risk_engine`: 리스크 정책
- `execution_engine`: tx/bundle 생성과 실행 흐름
- `relay_client`: relay/builder 연동
- `simulator`: replay, backtest, regression harness
- `persistence`: snapshot, journal, replay source 저장
- `observability`: metrics, tracing, health, profiling
- `app`: 실행 바이너리 진입점

## 7. 데이터 및 이벤트 모델 원칙

### 7.1 이벤트 분류

핵심 이벤트는 최소 아래 카테고리로 나눈다.

- `BlockEvent`
- `PendingTxEvent`
- `LogEvent`
- `PoolStateEvent`
- `PriceUpdateEvent`
- `OpportunityEvent`
- `RiskDecisionEvent`
- `ExecutionRequestEvent`
- `ExecutionResultEvent`
- `ReplayControlEvent`

### 7.2 이벤트 설계 규칙

- 이벤트 타입은 버전 관리가 가능해야 한다.
- 이벤트에는 가능한 한 `timestamp`, `source`, `sequence`, `chain_context`를 포함한다.
- 외부 입력 원본과 내부 정규화 결과를 모두 추적 가능해야 한다.
- replay 가능한 형태를 우선한다.
- 이벤트 스키마 변경 시 하위 호환 여부를 문서화한다.

## 8. 개발 단계 및 마일스톤

아래 일정은 초안이며, 실제 기간은 팀 규모와 대상 체인에 따라 조정한다.

| 단계 | 예상 기간 | 목표 | 핵심 산출물 | 완료 기준 |
|---|---:|---|---|---|
| Phase 0 | 1~2주 | 요구사항/전략/체인 확정 | ADR 3~5건, 범위 문서, 위험 목록 | `Ethereum mainnet` / `DEX Arbitrage` / relay 후보 확정 |
| Phase 1 | 2주 | Rust workspace 및 기반 프레임 구축 | Cargo workspace, 공통 타입, config, logging, CI | 기본 빌드/테스트/포맷 파이프라인 동작 |
| Phase 2 | 2~4주 | event_bus 및 state skeleton 구현 | ring buffer 초안, 표준 이벤트, state engine skeleton | synthetic event load 통과, replay 골격 동작 |
| Phase 3 | 3~5주 | ingress/normalizer/state 동작 연결 | mempool/block/log 수집, 표준 이벤트화 | 지정 소스에서 안정적으로 데이터 수집 |
| Phase 4 | 3~5주 | strategy/risk/simulator 연결 | 전략 API, 샘플 전략, replay/simulation | 과거 데이터 기반 회귀 검증 가능 |
| Phase 5 | 2~4주 | execution/relay 통합 | bundle builder, signer, relay client | 테스트 환경 또는 제한된 실환경 제출 성공 |
| Phase 6 | 2~4주 | 관측성/안정화/섀도우 운영 | metrics, tracing, alarms, dashboards, runbook | shadow mode에서 오류율/지연시간 기준 충족 |
| Phase 7 | 별도 승인 | 제한적 프로덕션 전환 | 운영 정책, 위험 통제, 롤백 절차 | 사전 승인된 범위 내 실운영 가능 |

## 9. 단계별 작업 상세

### Phase 0. Discovery / Decision

- `Ethereum mainnet` 기준 세부 범위 확정
- 우선 지원 프로토콜 선정
- `DEX Arbitrage` 세부 범위와 2차 전략 우선순위 정의
- relay / builder / node provider 후보 선정
- 리스크 경계와 운영 정책 정의
- 법률/정책 민감 전략 제외 기준 명시

필수 산출물:

- `docs/adr/ADR-001-architecture.md`
- `docs/adr/ADR-002-target-chain.md`
- `docs/adr/ADR-003-strategy-scope.md`
- `docs/specs/event-model.md`

### Phase 1. Foundation

- Rust workspace 초기화
- lint, fmt, clippy, test, bench 명령 세트 정리
- 환경별 config 구조 설계
- tracing / metrics / error handling 표준화
- 공통 타입과 이벤트 envelope 정의

필수 완료 조건:

- 신규 crate 추가 규칙 확정
- 기본 CI 파이프라인 동작
- local dev bootstrap 문서 작성

### Phase 2. Event Bus / State Skeleton

- bounded event ring 설계
- sequence cursor와 consumer abstraction 설계
- state shard 전략 정의
- snapshot / journal 인터페이스 초안
- synthetic load benchmark 작성

필수 완료 조건:

- 단일 producer/복수 consumer 시나리오 벤치 확보
- backpressure 발생 시 동작 원칙 문서화
- 상태 갱신 순서가 재현 가능함을 검증

### Phase 3. Ingestion / Normalization

- block subscription 연결
- mempool source 연결
- log/event subscription 연결
- DEX/pool 상태 변환 로직 작성
- 내부 표준 이벤트로 정규화

필수 완료 조건:

- 끊김/재연결 처리 존재
- duplicate / late event 처리 규칙 존재
- 최소 1개 프로토콜 데이터가 end-to-end로 흐름

### Phase 4. Strategy / Simulation

- 전략 인터페이스와 샘플 전략 구현
- replay 데이터 로더 작성
- 과거 이벤트 기반 시뮬레이션 엔진 작성
- 수익 계산 및 실패 분석 리포트 작성

필수 완료 조건:

- replay 결과가 결정론적으로 재현됨
- 전략 실패 원인을 추적 가능함
- 샘플 전략 1개 이상이 전 구간 테스트 가능

### Phase 5. Execution / Relay

- signer/key 관리 정책 수립
- tx/bundle builder 구현
- relay/builder 클라이언트 구현
- 제출 결과와 revert/timeout 분석 파이프라인 구현

필수 완료 조건:

- dry-run 또는 test environment 제출 가능
- 제출 실패 분류 가능
- 중복 제출 방지 장치 존재

### Phase 6. Hardening / Shadow Mode

- p50/p95/p99 지연시간 계측
- queue depth / drop / retry 지표 추가
- 알림 임계치 정의
- runbook / 장애 대응 절차 작성
- shadow mode 운영

필수 완료 조건:

- 장애 재현 절차 존재
- 운영 중 핵심 지표 관측 가능
- shadow mode에서 의사결정 흐름 확인 가능

## 10. 성능 및 품질 목표

수치는 대상 체인과 전략에 따라 조정하되, 초기 기준은 아래를 제안한다.

- bounded queue 사용률과 적체 상황을 실시간 관측 가능해야 한다.
- hot path 내부 stage는 불필요한 heap allocation을 피한다.
- synthetic load 기준으로 event processing latency를 p99까지 추적한다.
- replay 결과는 동일 입력에 대해 동일 결과를 내야 한다.
- state inconsistency가 발생하면 즉시 감지 가능해야 한다.
- 외부 I/O 실패가 내부 상태를 오염시키지 않아야 한다.

## 11. 테스트 전략

### 11.1 테스트 종류

- unit test: 순수 로직 검증
- integration test: 모듈 간 연결 검증
- replay test: 과거 이벤트 재현
- property test: 상태 전이 불변식 검증
- benchmark: latency/throughput 측정
- chaos/failure test: reconnect, timeout, partial failure 검증

### 11.2 필수 검증 포인트

- 이벤트 순서 보존
- duplicate event 처리
- chain reorg 대응
- relay timeout 및 retry
- signer 오류
- state snapshot 복구
- 전략 결과의 결정론성

## 12. 관측성 및 운영

### 12.1 최소 메트릭

- ingest rate
- normalize latency
- ring buffer utilization
- queue backlog
- state update latency
- strategy eval latency
- risk reject count
- execution success/fail count
- relay response latency
- replay divergence count

### 12.2 로그 원칙

- 결정 경로가 추적 가능해야 한다.
- 외부 원본 식별자와 내부 correlation id를 연결한다.
- 에러는 재시도 가능 여부와 함께 기록한다.
- 전략 판단 로직은 과도한 로그 대신 샘플링/구조화된 이벤트로 남긴다.

## 13. 보안 및 리스크

핵심 리스크는 아래와 같다.

- 잘못된 상태 해석으로 인한 손실
- chain reorg 미반영
- relay 실패 또는 제출 지연
- 중복 실행
- 잘못된 키 관리
- 성능 병목으로 인한 기회 상실
- replay와 실환경 간 동작 불일치
- 규제/정책 민감 전략 포함 위험

대응 방향:

- 실거래 이전에 replay와 shadow mode를 충분히 운영
- signer와 secret은 최소 권한 원칙 적용
- kill switch와 일일 손실 한도 도입
- 전략별 enable/disable feature flag 제공
- 수동 롤백 절차 문서화

## 14. 다른 AI 에이전트를 위한 작업 규칙

이 문서를 전달받은 AI 에이전트는 아래를 따른다.

1. 본 문서를 현재 프로젝트의 상위 기준 문서로 간주한다.
2. 아키텍처를 바꾸는 제안은 반드시 ADR 또는 문서 변경과 함께 제출한다.
3. hot path에 blocking I/O를 추가하지 않는다.
4. event schema를 바꿀 때는 버전 또는 마이그레이션 전략을 함께 제시한다.
5. 상태 갱신 순서를 깨는 병렬화는 임의로 도입하지 않는다.
6. 성능 개선 주장에는 최소한 benchmark 또는 측정 근거를 첨부한다.
7. replay 가능성을 해치는 설계를 피한다.
8. 신규 전략 추가 시 risk guard와 simulator 경로를 함께 고려한다.
9. 코드만 제출하지 말고 테스트와 짧은 설계 메모를 같이 남긴다.
10. 불확실한 부분은 임의로 확정하지 말고 `Open Questions`에 추가한다.

## 15. Open Questions

아래 항목은 빠르게 확정이 필요하다.

- `Ethereum mainnet` 이후 2차 확장 체인 우선순위는 무엇인가
- mempool source는 무엇을 사용할 것인가
- `Ethereum mainnet`에서 1차 arbitrage 대상 DEX / pool universe는 무엇인가
- bundle relay 대상은 어떤 조합으로 갈 것인가
- persistence는 무엇을 사용할 것인가
- state snapshot 주기는 어떻게 가져갈 것인가
- 전략별 자본 한도와 실패 허용치는 얼마인가
- 정책상 제외할 전략 범위는 어디까지인가

## 16. 초기 액션 아이템

프로젝트 시작 직후 우선순위는 아래와 같다.

1. Rust workspace 초기화
2. `docs/adr` 및 `docs/specs` 디렉터리 생성
3. event envelope 초안 작성
4. `event_bus`, `types`, `state_engine`, `app` 최소 crate 생성
5. synthetic benchmark harness 추가
6. replay를 위한 fixture 포맷 정의
7. `Ethereum mainnet` 기준 1차 DEX / pool / relay 범위 확정

## 17. Definition of Done

각 작업은 아래 조건을 만족해야 완료로 본다.

- 코드가 빌드된다.
- 관련 테스트가 존재한다.
- 관측 가능한 로그 또는 메트릭이 추가된다.
- 문서 또는 주석으로 의도가 남아 있다.
- 성능 민감 변경은 측정 근거가 있다.
- 운영 리스크가 있으면 명시되어 있다.

## 18. 문서 운영 규칙

- 이 문서는 프로젝트 시작 기준점이며, 큰 방향 전환 시 우선 업데이트한다.
- 상세 구현은 ADR, spec, task 문서로 분리한다.
- `Phase 0~6` 실행 보완, thin path 범위, gate, 정량 완료 기준은 `PHASE_0_6_SUPPLEMENT.md`를 함께 따른다.
- 초안 단계의 수치와 기간은 변경 가능하나, 변경 사유는 기록한다.
- 문서 변경 시 버전과 날짜를 함께 갱신한다.

---

## 부록 A. 추천 초기 ADR 목록

- ADR-001: 전체 아키텍처와 bounded event pipeline 채택 여부
- ADR-002: 대상 체인 선정
- ADR-003: 초기 전략 범위와 제외 전략
- ADR-004: event schema versioning 규칙
- ADR-005: state storage / snapshot 정책
- ADR-006: execution 및 relay 인터페이스 설계

## 부록 B. 첫 스프린트 제안

첫 스프린트에서는 아래 결과물을 목표로 한다.

- 빈 Rust workspace가 아닌, 최소 4개 crate가 연결된 실행 가능한 skeleton
- 샘플 이벤트 1종을 ring buffer에 publish/consume 하는 예제
- tracing/metrics 기본 탑재
- replay 가능한 fixture 포맷 초안
- 이후 에이전트가 바로 이어서 작업할 수 있는 ADR 템플릿

이 문서는 현재 프로젝트의 `base document`이며, 이후 세부 설계와 구현은 이 기준을 따라 확장한다.
