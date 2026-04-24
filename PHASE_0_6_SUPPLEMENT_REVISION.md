# Phase 0-6 Supplement Revision

작성일: 2026-04-24  
문서 상태: Draft v0.1  
대상 문서: `PHASE_0_6_SUPPLEMENT.md`  
용도: 본 문서는 `PHASE_0_6_SUPPLEMENT.md`를 각 Phase별 상세 docs/plan 작성 전에 보강하기 위한 별도 개정안이다.

## 1. 개정 목적

현재 `PHASE_0_6_SUPPLEMENT.md`는 방향성과 gate는 충분히 갖추고 있으나, 실제 Phase별 계획 문서를 작성하기 전 기준선으로는 몇 가지 핵심 항목이 부족하다.  
본 개정안의 목적은 다음과 같다.

- Phase 문서 작성 전에 반드시 고정해야 할 기술/운영 기준을 추가한다.
- `Vertical Slice + Replay Hooks` 접근을 실제 실행 계약 수준으로 강화한다.
- AI 에이전트가 임의로 스택과 운영 정책을 바꾸지 못하도록 `Decision Freeze` 목록을 명시한다.
- execution safety, observability, node topology, CI baseline을 보완한다.

## 2. 리뷰 요약

리뷰 결과 반영이 필요한 항목은 총 8개다.

| 구분 | 항목 | 개정 방향 |
|---|---|---|
| Critical | 기술 스택 결정 누락 | `Phase 0 Decision Freeze Matrix` 추가 |
| Critical | Phase 3 gas/simulation 전략 부재 | `Execution Safety Baseline` 추가 |
| Weak | risk budget 수치 없음 | 초기 기준선 수치 추가 |
| Weak | observability 툴링 미지정 | 기본 툴 체인 명시 |
| Weak | Phase 간 의존 관계 미명시 | `Phase Dependency Matrix` 추가 |
| Missing | event bus 구현 전략 부재 | 구현 정책과 backpressure 규칙 추가 |
| Missing | CI/CD 파이프라인 부재 | 최소 CI baseline 명시 |
| Missing | node 인프라 요구사항 부재 | local node / fallback RPC / archive 정책 추가 |

## 3. 개정 원칙

이번 개정은 아래 원칙으로 진행한다.

1. `PHASE_0_6_SUPPLEMENT.md`는 여전히 실행 기준 문서여야 하며, 세부 설계서처럼 비대해지면 안 된다.
2. 기술 선택의 세부 비교는 ADR로 분리하되, `Phase 0`에서 반드시 얼려야 할 선택 목록은 supplement 본문에 있어야 한다.
3. 수치가 필요한 운영 정책은 “나중에 정한다”로 넘기지 않고, 조정 가능한 초기 기준선이라도 둔다.
4. `Phase 3` execution은 여전히 `shadow-only`여야 하며, gas/simulation 정책도 이 원칙을 보강하는 방향으로만 추가한다.

## 4. Supplement에 추가할 신규 섹션

아래 섹션을 `PHASE_0_6_SUPPLEMENT.md`에 신규 추가하는 것을 권장한다.

### 4.1 Phase 0 Decision Freeze Matrix

목적:

- 각 Phase 상세 docs/plan 작성 전에 반드시 확정해야 할 기술 결정을 명시한다.
- AI 에이전트가 crate 선택이나 포맷을 임의로 바꾸지 못하게 한다.

필수 포함 항목:

| 영역 | 반드시 확정할 항목 | 권장 기본안 |
|---|---|---|
| RPC/EVM stack | `alloy` vs `ethers-rs` | 신규 구현은 `alloy` 우선 |
| local simulation | `revm` 채택 여부 | `revm` 채택 권장 |
| embedded KV | `RocksDB` vs 대안 | `RocksDB` 우선 |
| serialization | journal/snapshot 직렬화 포맷 | hot path는 `JSON` 금지, binary 포맷 우선 |
| async runtime | `tokio` 등 | `tokio` 표준화 |
| config format | `toml/yaml/json` | `toml` 우선 |
| error/telemetry | tracing/metrics stack | Rust 표준 관측 스택 고정 |

권장 운영 문구:

`Phase 0에서는 구현 속도보다 기술 선택의 일관성을 우선한다. 위 Decision Freeze Matrix에 포함된 항목은 ADR 승인 전까지 임의로 변경하지 않는다.`

### 4.2 Execution Safety Baseline

목적:

- `Phase 3` execution path에 gas/simulation/mismatch abort 정책을 명시한다.
- “코드는 제출 경로가 있지만 실제 운영은 shadow-only”라는 원칙을 더 구체화한다.

필수 포함 항목:

- local pre-simulation 필수
- relay simulation 또는 relay-side validation 사용
- local sim fail 시 submit 금지
- local sim / relay sim mismatch 시 submit 금지
- expected profit after gas가 최소 기준 미달이면 submit 금지
- gas bidding은 v1에서 단순하고 보수적인 정책을 사용
- dynamic optimizer, adaptive bidding, ML 기반 정책은 v1 범위에서 제외

권장 운영 문구:

`Phase 3 execution은 반드시 local pre-sim을 통과해야 하며, relay simulation 결과와 불일치할 경우 즉시 abort한다. 초기 gas bidding 정책은 보수적 고정 규칙 기반으로 유지하고, 공격적 입찰 최적화는 Phase 5 이후로 미룬다.`

### 4.3 Risk Budget Baseline

목적:

- live/prod 이전 safety gate에 필요한 초기 수치를 제공한다.

초기 기준선 제안:

| 항목 | 초기 기준 |
|---|---|
| shadow mode capital | `0` |
| max concurrent live bundles | `1` |
| per-bundle notional cap | 할당 전략 자본의 `1%` 이하 |
| daily realized loss cap | 할당 전략 자본의 `3%` 이하 |
| max resubmits per opportunity | `2` |
| simulation mismatch tolerance | `0` |
| unexplained divergence tolerance | `0` |
| live default config | 항상 `false` |

권장 운영 문구:

`초기 risk budget은 수익 극대화보다 손실 제한과 운영 단순성을 우선한다. 모든 수치는 조정 가능하되, 변경 시 근거를 기록한다.`

### 4.4 Observability & Tooling Baseline

목적:

- metric, tracing, dashboard, alerting을 각 Phase 문서가 공통으로 참조할 수 있게 한다.

권장 기본안:

| 영역 | 권장 도구 |
|---|---|
| structured logging | `tracing` + JSON logs |
| metrics | `metrics` crate + Prometheus exporter |
| dashboard | `Grafana` |
| alerting | `Alertmanager` |
| trace correlation | `tracing span` + request/event correlation id |

최소 필수 항목:

- event ingress rate
- queue depth
- queue saturation
- state update latency
- opportunity detection latency
- simulation pass/fail
- relay response latency
- replay divergence
- kill switch activation

### 4.5 Phase Dependency Matrix

목적:

- gate와 별도로, 어떤 산출물이 다음 Phase의 전제조건인지 명확히 한다.

권장 표:

| 다음 Phase | 선행 조건 |
|---|---|
| Phase 1 | Phase 0 ADR 승인, thin path 범위 확정 |
| Phase 2 | event envelope, event bus abstraction, journal interface 준비 |
| Phase 3 | Replay Gate 통과, State Correctness Gate 통과 |
| Phase 4 | end-to-end shadow path 완성 |
| Phase 5 | regression suite와 simulation baseline 준비 |
| Phase 6 | Safety Gate 통과, runbook/kill switch/alerts 준비 |

### 4.6 Event Bus Implementation Policy

목적:

- `LMAX 스타일` 원칙을 유지하면서도, 초기에 과도한 저수준 구현으로 일정이 무너지지 않게 한다.

권장 정책:

- `Phase 1`은 `EventBus` abstraction부터 고정한다.
- reference implementation은 bounded 구조를 사용한다.
- custom ring buffer는 benchmark로 필요성이 입증될 때 도입한다.
- ordered domain event는 drop하지 않는다.
- queue saturation 시 metric 증가, degraded mode 진입 또는 upstream throttling을 우선 검토한다.
- observability/telemetry 성격 이벤트는 필요 시 sampling 또는 drop 허용 가능하다.

권장 운영 문구:

`EventBus 구현은 초기부터 abstraction을 분리하되, custom ring buffer 자체를 목표로 두지 않는다. sequence-aware bounded design을 유지하면서, 실제 benchmark로 병목이 입증된 뒤 저수준 최적화를 진행한다.`

### 4.7 CI Baseline

목적:

- 상세 Phase 문서 전에 품질 자동화 기준을 고정한다.

최소 포함 항목:

- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo test`
- synthetic smoke test
- replay fixture smoke test
- `cargo deny` 또는 동등한 dependency/license 점검

운영 원칙:

- `CD`보다 `CI`를 우선한다.
- `Phase 6` 이전에는 자동 배포보다 자동 검증이 더 중요하다.

### 4.8 Node Infrastructure Baseline

목적:

- mempool/state/replay 검증에 필요한 최소 인프라 기준을 고정한다.

필수 포함 항목:

- local primary node 1개 필수
- fallback RPC provider 1개 이상 필수
- websocket + HTTP endpoint 모두 필요
- mempool validation은 local node 기준
- archive access는 thin path에는 선택, deep replay/diagnostics에는 권장

권장 기본안:

- `Phase 0`에서는 `Geth`와 `Reth` 중 1개를 primary로 결정한다.
- 초기 기준으로는 운영 안정성과 호환성을 위해 `Geth` primary를 우선 검토한다.
- 성능 비교 또는 Rust-native integration 필요 시 `Reth`는 Phase 4 이후 shadow node로 검토한다.

## 5. 권장 ADR 패키지

아래 ADR은 각 Phase 상세 문서 전에 먼저 작성하는 것을 권장한다.

| ADR | 주제 |
|---|---|
| `ADR-001` | Vertical Slice + Replay Hooks + Gate 정책 |
| `ADR-002` | Ethereum mainnet + DEX Arbitrage thin path |
| `ADR-003` | mempool / relay / persistence 전략 |
| `ADR-004` | RPC/EVM stack 선택 (`alloy`, `revm`, runtime 포함) |
| `ADR-005` | event bus 구현 정책과 backpressure 규칙 |
| `ADR-006` | execution simulation / gas bidding / abort 정책 |
| `ADR-007` | node topology와 fallback RPC |
| `ADR-008` | observability / CI baseline |

## 6. Phase 문서 작성 전 선행 순서

각 Phase별 docs/plan 작성 전에 아래 순서를 권장한다.

1. `PHASE_0_6_SUPPLEMENT.md`에 본 개정안의 핵심 섹션을 반영한다.
2. `ADR-004 ~ ADR-008` 초안을 작성한다.
3. thin path 범위를 다시 한번 freeze한다.
4. risk budget과 execution safety baseline을 문서상 확정한다.
5. 그 다음에야 `Phase 0`, `Phase 1`, `Phase 2` 상세 계획서를 작성한다.

## 7. 우선순위

가장 먼저 반영할 항목은 아래 순서다.

1. 기술 스택 결정 목록
2. gas/simulation/abort 정책
3. risk budget 수치
4. Phase dependency matrix
5. node infrastructure baseline
6. observability baseline
7. event bus implementation policy
8. CI baseline

## 8. 권장 결론

현재 시점에서는 `Phase 0~6` 상세 계획서를 바로 세분화하기보다, 먼저 supplement를 실행 계약 수준으로 한 차례 더 강화하는 것이 맞다.  
즉, 지금 단계의 우선순위는 `세부 작업 분해`가 아니라 `기준선 고정`이다.

본 개정안의 핵심 메시지는 다음과 같다.

- 지금 필요한 것은 더 많은 Phase 문서가 아니라, 더 명확한 `Freeze Points`다.
- `Phase 3` execution은 기능 완성이 아니라 `shadow-safe execution`을 의미해야 한다.
- `Replayability`, `State Correctness`, `Risk Budget`, `Node Topology`, `Observability`는 상위 문서 수준에서 고정되어야 한다.

이 문서는 `PHASE_0_6_SUPPLEMENT.md` 개정 작업의 입력 문서로 사용한다.
