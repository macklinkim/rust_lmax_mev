# Phase 4-6 Preconditions & Critical Notes

작성일: 2026-04-24
문서 상태: Draft v0.1
근거 문서: `PHASE_0_6_SUPPLEMENT.md`, `PHASE_0_6_SUPPLEMENT_REVISION.md`, Phase 3 상세 설계
용도: Phase 4~6 상세 설계 전에 반드시 확인해야 할 주의사항, 선행 조건, 금지 사항을 정리한다.

## 1. Phase 4: Strategy Hardening + Simulation

### 1.1 선행 조건

- [ ] Phase 3 Exit Gate 7개 조건 전부 통과
- [ ] end-to-end 6-stage shadow path 완성 확인
- [ ] Phase 3 fixture 5개 전부 재현 가능 상태 유지

### 1.2 Scope Freeze 해제 규칙

- Phase 3까지 적용되던 scope freeze(`SUPPLEMENT §5.5`)가 Phase 4에서 부분 해제된다.
- 해제 범위를 명시적으로 문서화해야 한다.
- 허용 확장: `Sushiswap` adapter 추가, cross-DEX arbitrage path
- 여전히 금지: `Curve`, `Balancer`, `Backrun`, `Liquidation`, 새로운 chain

### 1.3 Pipeline 재사용 원칙

- Phase 3에서 확립한 6-stage pipeline을 그대로 재사용한다.
- 새로운 pipeline을 만들지 않는다.
- Risk Gate의 `allowed_protocol` 확장으로 Sushiswap을 수용한다.
- `PipelineOutcome<T>` 제네릭 + immutable 패턴을 변경하지 않는다.

### 1.4 Regression Suite 요구사항

- Phase 3 fixture 5개(sim_pass, sim_fail, sim_mismatch, stale_candidate, target_expired)를 regression baseline에 포함해야 한다.
- Sushiswap 추가 시 cross-DEX fixture를 별도 추가한다.
- `SUPPLEMENT §Phase 4 종료기준`: "replay dataset에 대해 deterministic regression pass"

### 1.5 Backtest vs Replay 구분

- backtest = historical block data 기반 과거 시뮬레이션
- replay = recorded fixture 기반 deterministic 재현
- 두 경로를 혼동하거나 하나로 합치지 않는다.

### 1.6 False Positive 분류 체계

- false positive 사례를 Phase 3의 `AbortReason`/`RejectReason` 체계와 정렬해야 한다.
- `SUPPLEMENT §Phase 4 종료기준`: "false positive 사례가 분류되고 재현 가능"
- 분류 체계 없이 "충분히 낮다"로 판단하지 않는다.

### 1.7 Phase 4 종료 기준 (SUPPLEMENT 원문)

- Uniswap/Sushiswap 조합에서 cross-DEX path 검증 완료
- replay dataset에 대해 deterministic regression pass
- simulation 결과와 opportunity 판단 차이를 보고서로 설명 가능
- false positive 사례가 분류되고 재현 가능

---

## 2. Phase 5: Performance + Observability

### 2.1 선행 조건

- [ ] Phase 4 종료 기준 전부 통과
- [ ] regression suite + simulation baseline 준비 (`REVISION §4.5`)

### 2.2 측정 우선 원칙

- `SUPPLEMENT §9.6`: "성능 최적화 전에는 측정값을 먼저 남긴다"
- 측정 없이 최적화 코드를 작성하지 않는다.
- benchmark로 병목이 입증된 뒤에만 저수준 최적화를 진행한다.

### 2.3 LMAX Ring Buffer 주의

- `REVISION §4.6`: "custom ring buffer는 benchmark로 필요성이 입증될 때 도입"
- LMAX 패턴의 실제 병목은 ring buffer 자체가 아니라 stage 간 전환 지점일 가능성이 높다.
- bounded channel → custom ring buffer 교체는 측정 근거가 있을 때만 허용한다.

### 2.4 Kill Switch 실장

- Phase 3에서 stub으로 구현한 kill switch를 Phase 5에서 실장한다.
- 수동(config toggle) + 자동(threshold-based) 모두 동작해야 한다.
- `SUPPLEMENT §6.3 Safety Gate` 필수 조건이다.

### 2.5 Stage별 Latency 계측

- 6-stage pipeline이므로 stage별 p50/p95/p99를 나눠서 계측한다.
- `REVISION §4.4`의 observability baseline metric 목록과 정렬한다.
- 최소 계측 대상: local_sim, relay_sim, bundle_build, relay_submit, freshness_check

### 2.6 Chaos/Failure Test 범위

아래 시나리오 각각에 대해 expected behavior를 정의해야 한다:

- [ ] relay 실패 (Flashbots/bloXroute timeout 또는 reject)
- [ ] RPC 장애 (primary node down)
- [ ] node disconnect (websocket drop)
- [ ] reorg during pipeline (candidate가 진행 중인 상태에서 reorg 발생)
- [ ] fallback RPC 전환

### 2.7 Safety Gate 위치 확인

- `SUPPLEMENT §6.3`: Safety Gate는 Phase 5와 Phase 6 사이에 위치한다.
- Phase 5 종료 시 Safety Gate 조건을 전부 충족해야 Phase 6 진입 가능하다.

### 2.8 Phase 5 종료 기준 (SUPPLEMENT 원문)

- 주요 stage별 p50/p95/p99 latency 측정 가능
- event drop 0 또는 문서상 허용 한도 이하
- queue backlog alert 동작 확인
- relay/RPC 장애 시 degraded mode 또는 fail-safe 동작 확인
- kill switch 수동/자동 테스트 pass

---

## 3. Phase 6: Shadow Mode + Limited Production

### 3.1 선행 조건

- [ ] Phase 5 종료 기준 전부 통과
- [ ] Safety Gate(`SUPPLEMENT §6.3`) 통과:
  - [ ] kill switch 수동/자동 모두 동작
  - [ ] daily loss cap, per-bundle cap, signer isolation 정책 설정
  - [ ] shadow mode 기준 경보, 로그, 대시보드 준비
  - [ ] relay 실패, RPC 장애, simulation mismatch runbook 준비
- [ ] runbook/kill switch/alerts 준비 (`REVISION §4.5`)

### 3.2 Shadow Run 요구사항

- 최소 7일 continuous shadow run 필수 (`SUPPLEMENT §6.4`)
- Phase 3의 72시간 shadow run과는 별개 — Phase 6에서 다시 7일 수행
- unexplained divergence = 0건
- 7일 중 전체 6-stage path가 실 데이터로 완주한 사례가 존재해야 한다.

### 3.3 Capital 관련 금지/제한

- `canary key ONLY` — funded key ≠ prod signer (`SUPPLEMENT §6.4`)
- signer isolation 필수: canary signer와 main signer를 물리적/논리적으로 분리
- `live_send=true` 전환은 Production Gate 통과 후에만 (`SUPPLEMENT §5.1`)

### 3.4 Risk Budget 수치 확정

Phase 6 진입 전 아래 수치를 확정해야 한다 (`REVISION §4.3`):

| 항목 | 초기 기준 |
|---|---|
| shadow mode capital | `0` |
| max concurrent live bundles | `1` |
| per-bundle notional cap | 전략 자본의 `1%` 이하 |
| daily realized loss cap | 전략 자본의 `3%` 이하 |
| max resubmits per opportunity | `2` |
| simulation mismatch tolerance | `0` |
| unexplained divergence tolerance | `0` |
| live default config | 항상 `false` |

### 3.5 Runbook 3종 필수

`SUPPLEMENT §6.3`에 의거, 아래 3종의 runbook이 준비되어 있어야 한다:

1. **Relay 실패 runbook**: Flashbots/bloXroute 장애 시 대응 절차
2. **RPC 장애 runbook**: primary node down, fallback 전환, 복구 절차
3. **Simulation mismatch runbook**: mismatch 발생 시 원인 분류, 자동 abort 확인, 수동 점검 절차

### 3.6 Rollback Drill

- manual rollback drill 1회 이상 완료 (`SUPPLEMENT §6.4`)
- drill 시나리오와 결과를 기록한다.
- postmortem template 준비 완료

### 3.7 Production Gate (SUPPLEMENT §6.4)

Phase 6 limited production 진입 직전 충족 필수:

- [ ] 최소 7일 shadow run 완료
- [ ] unexplained divergence 0건
- [ ] manual rollback drill 완료
- [ ] canary key 및 소규모 notional cap 적용 완료

### 3.8 Phase 6 종료 기준 (SUPPLEMENT 원문)

- 최소 7일 shadow mode 운영
- unexplained divergence 0건
- limited prod는 canary key만 사용
- per-bundle cap, daily loss cap, manual approval policy 적용
- rollback drill 및 postmortem template 준비 완료

---

## 4. Cross-Phase 공통 주의사항

### 4.1 LMAX Immutable 패턴 유지

- Phase 4-6에서도 `PipelineOutcome<T>` immutable 패턴을 유지한다.
- Sushiswap 추가, 전략 확장 등으로 타입이 늘어도 패턴 자체를 변경하지 않는다.
- 새 타입 추가 시 기존 제네릭 구조에 맞춘다.

### 4.2 ADR 없이 기술 스택 변경 금지

- `REVISION §4.1 Decision Freeze Matrix`에 포함된 항목은 ADR 승인 전까지 변경 금지.
- 대상: RPC/EVM stack(alloy), local sim(revm), KV(RocksDB), serialization, async runtime(tokio), config(toml), telemetry stack

### 4.3 Scope 확장 시 문서화 필수

- `SUPPLEMENT §5.5`: thin path scope freeze는 Phase 4부터 부분 해제된다.
- 어떤 범위를 열 것인지 Phase 4 상세 설계에 명시적으로 기록한다.
- "Phase 4에서 열린 것" vs "여전히 금지인 것"을 구분한다.

### 4.4 각 Phase에 Exit Gate 정의 필수

- Phase 3에서 Exit Gate를 추가한 것처럼, Phase 4-6에도 각각 정량 Exit Gate를 정의한다.
- "모호한 표현 대신 테스트 pass, determinism 100%, event loss 0, p99 latency budget 같은 기준을 사용한다" (`SUPPLEMENT §5.4`)

### 4.5 Gas Bidding 최적화 시점

- Phase 3~4: conservative fixed rule only
- Phase 5+: 측정 근거가 있을 때만 adaptive/dynamic 검토 가능
- ML 기반 정책: v1 범위에서 제외 (`REVISION §4.2`)

### 4.6 AI 에이전트 작업 규칙 재확인

`SUPPLEMENT §9`에 의거:

1. thin path 범위를 임의로 넓히지 않는다 (Phase 4 해제 범위 내에서만)
2. replay gate와 state correctness gate를 우회하지 않는다
3. 정량 기준 없이 "충분히 됐다"고 판단하지 않는다
4. 성능 최적화 전에는 측정값을 먼저 남긴다
5. 범위 확대 제안은 반드시 다음 Phase로 미루거나 ADR로 올린다
