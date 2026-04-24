# Phase 0-6 Supplement

작성일: 2026-04-24  
문서 상태: Draft v0.1  
관련 기준 문서: `PROJECT_BASE.md`  
용도: 본 문서는 `PROJECT_BASE.md`의 실행 보완 문서다. 특히 `Phase 0~6` 진행 방식, thin path 범위, gate, 정량 완료 기준, shadow/prod 안전 원칙을 명시한다.

## 1. 문서 목적

이 문서는 아래 목적을 가진다.

- `PROJECT_BASE.md`의 상위 방향을 유지하면서 실행 순서를 더 구체화한다.
- 1인 개발 + AI 에이전트 환경에서 발생하기 쉬운 범위 확장과 무의미한 리팩토링을 줄인다.
- Vertical Slice 접근을 유지하되, replay 가능성과 상태 정확성 검증을 초기부터 강제한다.
- 각 Phase 종료 조건을 정량 기준으로 명시해 “다음 단계로 넘어가도 되는가”를 판단할 수 있게 한다.
- live capital 사용 이전에 필수 safety gate를 문서로 고정한다.

## 2. 현재 기준 확정 사항

현재 기준으로 아래 사항은 실행 기준선으로 간주한다.

| 항목 | 결정 |
|---|---|
| 대상 체인 | `Ethereum mainnet` |
| 2차 확장 체인 | `BSC`, `Base`, `Arbitrum` |
| 1차 전략 | `DEX Arbitrage` |
| 전략 우선순위 | 2순위 `Backrun`, 3순위 `Liquidation` |
| mempool source | 하이브리드: 자체 노드 + 외부 feed |
| bundle target | 멀티 릴레이: `Flashbots + bloXroute` 중심, adapter 확장 |
| persistence | embedded KV + append-only binary journal |
| 1차 DEX 범위 | `Uniswap V2/V3 + Sushiswap` |
| 실행 접근 | `Approach A: Vertical Slice + Replay Hooks` |
| 개발 체계 | 1인 개발 + AI 에이전트 |

## 3. 실행 접근 원칙

### 3.1 기본 접근

본 프로젝트는 `Vertical Slice 우선`으로 진행한다.

- `Phase 0~3`은 하나의 얇은 end-to-end 경로를 실제로 관통시키는 데 집중한다.
- `Phase 4~6`은 이미 동작하는 경로를 넓히고, 안전하게 만들고, 운영 가능 상태로 끌어올리는 데 집중한다.
- replay/simulation은 나중에 붙이는 기능이 아니라, `Phase 1`부터 구조적으로 심는다.

### 3.2 왜 이 접근을 택하는가

- 1인 개발에서는 레이어를 완성도 높게 따로 쌓는 방식보다 end-to-end 병목을 빨리 드러내는 방식이 유리하다.
- AI 에이전트는 얇고 명확한 경로가 있을 때 더 안정적으로 병렬 작업과 문서 정렬이 가능하다.
- 단, Vertical Slice의 약점인 “나중에 상태 정확성/리플레이/안전성 검증이 밀리는 문제”는 본 보완 문서의 gate로 통제한다.

## 4. Thin Path Baseline

초기 vertical slice의 범위는 아래로 고정한다.

### 4.1 체인 및 전략

- 체인: `Ethereum mainnet`
- 전략: `DEX Arbitrage`
- 실행 목표: mempool 또는 block-triggered 가격 불일치를 감지하고, bundle candidate를 생성하며, shadow 제출까지 관통

### 4.2 초기 토큰 및 풀 범위

- 초기 token pair universe는 `WETH/USDC`로 제한한다.
- 초기 pool shape는 아래 둘로 제한한다.
  - `Uniswap V2-style constant product` pool 1개 계열
  - `Uniswap V3 concentrated liquidity` pool 1개 fee tier
- `Uniswap V3`의 초기 fee tier는 `0.05%`를 우선 기본안으로 둔다.
- `Sushiswap`은 문서상 1차 범위에 포함되지만, 실제 thin path 관통은 `Phase 4`에서 확장하는 것을 기본으로 한다.
- `Curve/Balancer`는 본 thin path에 포함하지 않는다.

### 4.3 mempool 및 relay 범위

- 자체 노드 1개를 canonical validation 기준점으로 둔다.
- 외부 mempool feed는 1개만 우선 연결한다.
  - 구현 시점 선택지는 `bloXroute` 또는 `Fiber`
- relay target adapter는 `Flashbots`, `bloXroute`를 우선 지원 대상으로 둔다.
- `Phase 3`까지는 실제 live capital 제출이 아닌 `shadow-only`를 기본 원칙으로 한다.

### 4.4 persistence 범위

- event journal은 append-only binary log를 사용한다.
- snapshot/state metadata는 embedded KV에 저장한다.
- analytics용 외부 DB는 본 thin path 범위에 넣지 않는다.

## 5. Cross-Phase Guardrails

아래 항목은 모든 Phase에 공통 적용한다.

### 5.1 Shadow-Only Until Safety Gate

- `Phase 3`의 execution path는 “실제 번들 제출 코드를 가진다”와 “실제 자본으로 운영한다”를 동일하게 취급하지 않는다.
- `Phase 3`에서는 `live_send=false`가 기본값이어야 한다.
- funded key, prod signer, 실제 자본 사용은 `Phase 6`의 safety gate를 통과하기 전까지 금지한다.

### 5.2 Replayability First

- 모든 핵심 이벤트는 replay 가능한 envelope를 가진다.
- 이벤트에는 최소한 `timestamp`, `source`, `sequence`, `chain_context`, `event_version`을 포함한다.
- “지금은 빨리 만들고 replay는 나중에 붙인다”는 허용하지 않는다.

### 5.3 State Correctness Before Execution

- 상태 추적 검증 없이 execution path를 열지 않는다.
- reorg, duplicate, replacement, late event, resync 경로가 최소 fixture 수준에서 검증되어야 한다.

### 5.4 Quantified Definition of Done

- 각 Phase에는 수치 또는 pass/fail 성격의 명확한 종료 기준이 있어야 한다.
- 모호한 표현 대신 `테스트 pass`, `determinism 100%`, `event loss 0`, `p99 latency budget` 같은 기준을 사용한다.

### 5.5 Scope Freeze

- thin path가 끝나기 전까지 pair universe, protocol universe, strategy 종류를 임의로 넓히지 않는다.
- 새로운 확장은 `Phase 4` 이후에만 기본적으로 허용한다.

## 6. Mandatory Gates

### 6.1 Replay Gate

적용 시점: `Phase 2` 종료 시점

필수 조건:

- recorded fixture를 이용해 동일 입력에 대해 동일 opportunity 결과를 재현해야 한다.
- replay run과 live-normalized event run 사이의 핵심 필드가 일치해야 한다.
- serialization/deserialization round-trip test가 fixture 세트에서 100% 통과해야 한다.

### 6.2 State Correctness Gate

적용 시점: `Phase 2`와 `Phase 3` 사이

필수 조건:

- pending tx replacement 처리 테스트 pass
- duplicate dedupe 테스트 pass
- chain reorg 처리 테스트 pass
- block/state resync 테스트 pass
- tracked reserve/state가 reference fixture 기준으로 divergence 0이어야 한다.

### 6.3 Safety Gate

적용 시점: `Phase 5`와 `Phase 6` 사이

필수 조건:

- kill switch가 수동/자동 모두 동작해야 한다.
- daily loss cap, per-bundle cap, signer isolation 정책이 설정되어 있어야 한다.
- shadow mode 기준 경보, 로그, 대시보드가 준비되어 있어야 한다.
- relay 실패, RPC 장애, simulation mismatch에 대한 runbook이 있어야 한다.

### 6.4 Production Gate

적용 시점: `Phase 6` limited production 진입 직전

필수 조건:

- 최소 7일 shadow run 완료
- unexplained divergence 0건
- manual rollback drill 완료
- canary key 및 소규모 notional cap 적용 완료

## 7. Phase-by-Phase Supplement

## Phase 0. Discovery & Decision

### 목표

- 얇은 경로를 실제로 고정한다.
- 이후 구현 중 다시 같은 결정을 반복하지 않도록 ADR과 실행 규칙을 정리한다.

### 필수 산출물

- `ADR-001`: Vertical Slice 접근과 gate 채택
- `ADR-002`: Ethereum mainnet + DEX Arbitrage 범위
- `ADR-003`: mempool / relay / persistence 전략
- thin path scope 문서
- risk budget 초안

### 종료 기준

- `WETH/USDC` thin path 범위 확정
- 초기 DEX 경로와 fee tier 확정
- mempool source 1차 구성 확정
- relay adapter 목표 확정
- `Phase 1~3`에서 live capital 사용 금지 원칙 문서화

## Phase 1. Foundation + Event Bus

### 목표

- workspace, 타입, event ring, config, tracing의 최소 skeleton을 만든다.
- replay-ready envelope를 day 1부터 내장한다.

### 구현 범위

- Cargo workspace
- `types`, `event_bus`, `config`, `observability`, `app` 최소 crate
- event envelope, event versioning
- append-only journal writer interface
- embedded KV abstraction 초안

### 종료 기준

- `cargo build`, `cargo test`, `cargo clippy` pass
- synthetic event publish/consume 테스트 pass
- event ordering 보존 테스트 pass
- event loss 0으로 100k synthetic events 처리
- journal write/read smoke test pass

## Phase 2. Ingress + State + Opportunity

### 목표

- 하나의 thin path에서 데이터가 실제로 들어와 상태를 만들고 opportunity를 산출하게 만든다.
- replay gate와 state correctness gate를 이 단계 안에서 준비한다.

### 구현 범위

- local node ingestion
- external mempool feed adapter 1종
- block + pending tx normalize
- `WETH/USDC` 대상 pool state tracking
- Uniswap V2-style + Uniswap V3 1개 fee tier 기회 탐지
- fixture recorder / replay loader

### 종료 기준

- live ingress와 fixture replay가 동일 event schema 사용
- fixture replay determinism 100%
- duplicate/replacement/reorg/resync 테스트 pass
- reserve/state divergence 0
- first opportunity candidate가 live 또는 replay에서 안정적으로 발생

## Phase 3. Execution + Replay End-to-End

### 목표

- opportunity에서 risk, bundle build, relay adapter, feedback까지 관통시킨다.
- 단, 이 단계는 `shadow-only`다.

### 구현 범위

- risk engine 최소 정책
- bundle candidate builder
- relay adapter (`Flashbots`, `bloXroute`) skeleton
- submission result feedback events
- replay-to-bundle simulation path
- kill switch stub

### 필수 운영 원칙

- 기본 설정은 `live_send=false`
- prod signer 사용 금지
- funded key 사용 금지
- notional cap은 0 또는 비활성 상태의 shadow configuration으로 유지

### 종료 기준

- opportunity -> risk -> bundle -> relay adapter -> feedback 흐름이 end-to-end로 동작
- curated fixture 기준 bundle simulation pass rate가 문서상 정의된 기준 이상
- execution path의 모든 결과가 journaling/replay 가능
- live submission이 기본 설정에서 절대 활성화되지 않음

## Phase 4. Strategy Hardening + Simulation

### 목표

- vertical slice를 넓히고, 전략의 신뢰성과 회귀 검증 체계를 강화한다.

### 구현 범위

- `Sushiswap` adapter 추가
- cross-DEX arbitrage path 확장
- backtest harness
- profit attribution / 실패 원인 분류
- strategy regression suite

### 종료 기준

- Uniswap/Sushiswap 조합에서 cross-DEX path 검증 완료
- replay dataset에 대해 deterministic regression pass
- simulation 결과와 opportunity 판단 차이를 보고서로 설명 가능
- false positive 사례가 분류되고 재현 가능

## Phase 5. Performance + Observability

### 목표

- 병목을 줄이고 운영 가능 수준의 메트릭, 경보, 장애 대응 준비를 완료한다.

### 구현 범위

- stage별 latency 계측
- queue depth, drop, retry, simulation mismatch 지표
- dashboards + alerts
- chaos/failure test
- kill switch 실장

### 종료 기준

- 주요 stage별 p50/p95/p99 latency 측정 가능
- event drop 0 또는 문서상 허용 한도 이하
- queue backlog alert 동작 확인
- relay/RPC 장애 시 degraded mode 또는 fail-safe 동작 확인
- kill switch 수동/자동 테스트 pass

## Phase 6. Shadow Mode + Limited Production

### 목표

- shadow run으로 실제 환경 적합성을 검증하고, 제한된 범위에서만 실거래를 연다.

### 구현 범위

- shadow mode 운영
- limited prod canary
- runbook / rollback / alarm routing
- signer isolation
- capital guardrails

### 종료 기준

- 최소 7일 shadow mode 운영
- unexplained divergence 0건
- limited prod는 canary key만 사용
- per-bundle cap, daily loss cap, manual approval policy 적용
- rollback drill 및 postmortem template 준비 완료

## 8. Quantitative DoD Baseline

아래 수치는 초기 기준선이며, 실제 구현 중 조정할 수 있다. 단, 변경 시 근거를 기록한다.

| 항목 | 초기 기준 |
|---|---|
| replay determinism | fixture 세트 기준 `100%` |
| state divergence | 기준 fixture 대비 `0` |
| synthetic event loss | `0` |
| shadow unexplained divergence | `0` |
| shadow minimum run | `7일` |
| limited prod signer | `canary key only` |
| live default config | 항상 `false` |

## 9. AI 에이전트 작업 규칙

이 문서를 전달받은 AI 에이전트는 아래를 따른다.

1. thin path 범위를 임의로 넓히지 않는다.
2. `Phase 2` 이전에 execution 최적화에 과도하게 시간을 쓰지 않는다.
3. `Phase 3`에서 live capital path를 열지 않는다.
4. replay gate와 state correctness gate를 우회하지 않는다.
5. 정량 기준 없이 “충분히 됐다”고 판단하지 않는다.
6. 성능 최적화 전에는 측정값을 먼저 남긴다.
7. 범위 확대 제안은 반드시 다음 Phase로 미루거나 ADR로 올린다.

## 10. 문서 운영 규칙

- 이 문서는 `PROJECT_BASE.md`의 실행 보완 문서다.
- 상위 방향 충돌 시 `PROJECT_BASE.md`를 우선하되, 실행 gate와 safety 규칙은 본 문서를 따른다.
- 숫자 기준 변경 시 날짜와 사유를 함께 남긴다.
- 실제 구현에서 더 좋은 baseline이 확인되면 문서를 업데이트한다.
