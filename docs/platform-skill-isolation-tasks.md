# 플랫폼별 스킬 제어 구현 작업

상태: 구현 진행 중. [명세](platform-skill-isolation-spec.md)와 [조사표](research/platform-skill-capabilities-2026-09-21.md)가 계약이다. 이 문서는 실제 설치 변경·커밋·배포 승인이 아니다.

구현 진행: T2 설정 Adapter를 OMP·CodeBuddy·OpenClaw·Factory Droid·Command Code·Hermes·Mistral Vibe·OpenCode에 연결했다. 기존 Claude·Codex Adapter와 같은 저장·충돌 감지·검증·롤백 흐름을 사용한다. JSON/YAML/TOML의 다른 설정과 주석을 보존하고, 광범위한 glob·Mistral allowlist·OpenCode의 모호한 버전 형식처럼 한 스킬만 안전하게 바꿀 수 없는 상태는 자동 변경을 거부한다. 플랫폼 실행 중 세션에서 목록이 빠지는지 확인하는 T6 검증은 남아 있다.

## 선행 관계

| 작업 | 선행 | 완료 결과 |
|---|---|---|
| T1 경로·버전과 제어 능력 | 없음 | 전체 등록 플랫폼의 근거/미확인 상태와 읽기 경로 모델 |
| T2 개별 설정 제어 | T1의 해당 플랫폼 | 한 스킬 off/on, 복원·설정 우선순위·실제 목록 확인 |
| T3 대표 출처 선택 | T1, 해당 T2 | 내용 비교→선택→실제 대표 일치, 범위와 업데이트 보존 |
| T4 설치 전환 계획·복구 | T1, T3 | 원본·다른 플랫폼을 보존하는 전환과 실패 복구 |
| T5 화면 연결 | T2; 대표/전환 UI는 T3/T4 | 플랫폼 토글과 공용 전체 조작 분리 |
| T6 전체 플랫폼 검증 | 각 플랫폼의 T2 또는 T4, T5 | 60개 행별 버전·범위·실행 결과, 미완료 명시 |

T1의 확인된 플랫폼부터 T2~T5를 수직으로 완성한다. T6에서 일부 통과를 전체 완료로 취급하지 않는다. 미확인 플랫폼의 조사 과제는 T1에 계속 남는다.

## T1 — 실제로 읽는 경로와 가능한 제어

관련 파일: src-tauri/src/db.rs, commands/scanner.rs, discover.rs, platform_skill_control.rs, src/types/index.ts.

- 등록 기본 경로·실제 탐색·설치 목적지를 구분한다. 설치 목적지가 공용인 7개 플랫폼부터 점검한다.
- AGY/Antigravity, Kilo, OpenCode, Cline, Firebender, Amp, ForgeCode, OpenClaw 등 조사표의 불일치를 버전별로 해결한다.
- 프로젝트·조상·하위·프로필·플러그인·추가 경로를 필요 범위에서 계산한다. 앱이 발견한 저장소 전체를 세션 범위로 합치지 않는다.
- 제품/버전 미확인이나 remote-only를 지원으로 표시하지 않는다. 미탐지 새 플랫폼의 기본값도 설치하지 않음이다.

완료 검증: 등록 60개 ID가 누락 없이 분류됨; 근거 없는 자동 쓰기 없음; 같은 HOME을 가진 가짜 플랫폼 fixture에서 공유·호환 경로 구분; 현재 설치 경로를 근거 없이 일괄 이주하지 않음.

## T2 — 플랫폼 자체 제외 설정

관련 파일: commands/platform_skill_control.rs, db.rs, 설정 형식별 최소 보조 코드.

완료한 설정 Adapter 묶음: OMP, CodeBuddy, OpenClaw, OpenCode, Factory Droid, Command Code, Mistral Vibe, Hermes 및 기존 Codex/Claude 회귀. Copilot CLI·AGY와 나머지 플랫폼은 실행 버전·설정 근거가 확보되는 순서로 연결한다. 이는 전체 대상 축소가 아니라 구현 순서다.

- 정확한 버전의 읽기/쓰기 키, 이름/경로 범위, 상위 설정, 재로드 조건을 확인하고 T1 결과에 연결한다. AGY는 skills.json의 명시 경로 제외를 자동 발견 전체 제외와 구분하고, CodeArts는 프로젝트 상태 파일과 전역 제어를 구분한다.
- 현재 값→부분 변경→다시 읽기→복원. 미지원 형식·오류·외부 충돌은 성공으로 반환하지 않는다.
- 이름 단위 off와 특정 출처 선택을 구분한다. allowlist가 살아 있는데 deny만 추가하는 오류를 막는다.
- UI 토글만 알려진 Junie/Qwen/Cline/Augment/Bob/Trae CN/CodeArts 등은 공식 저장 구조나 API 확인 후 같은 계약으로 추가한다.
- 설정 전체를 꺼야 하는 도구 게이트는 개별 off adapter로 등록하지 않는다.

완료 검증: 임시 설정 round-trip, 기존 값/주석·다른 스킬 유지, 이름 충돌 범위 안내, 각 플랫폼의 로더/목록 결과. 실제 실행 검증이 없으면 문서 근거 수준으로 유지한다.

## T3 — 대표 출처 선택

관련 파일: commands/skill_duplicates.rs, platform_skill_control.rs, db.rs, src/stores/skillUsageStore.ts.

- 기존 전체 패키지 비교 재사용. 안정적인 묶음과 대표 경로를 저장하고 이름만으로 합치지 않는다.
- 작업 범위가 다른 출처를 전역 하나로 덮지 않는다.
- 실제 runtime winner와 선택이 일치할 때만 대표 적용으로 표시한다.
- 업데이트 divergence, 대표 삭제, 스캔 순서 변화, 새 플랫폼 추가를 처리한다.
- 추가 설치 방지 check_install과 대표 전환이 충돌하지 않도록, 전환 계획에 포함된 대상만 제한적으로 검증하는 내부 경로를 둔다. 일반 설치의 중복 방지는 유지한다.

완료 검증: 동일 원본/같은 사본/다른 내용/비교 불가, assets만 다른 사본, 서로 다른 프로젝트, 이름 단위 adapter의 대표 선택 한계, 업데이트 후 대표 유지.

## T4 — 공용에서 플랫폼별 설치로 전환

관련 파일: commands/usage.rs, linker.rs, platform_skill_control.rs, skill_origin.rs, recovery.rs, db.rs.

- 실제 쓰기 진입점의 잠금 순서를 먼저 정하고 직렬화 누락·중첩 교착을 검증한다.
- 예상 변경·원본/설치 구분·소비자 목록·남는 출처·복원을 포함하는 계획과 만료 검사.
- 복구 기록을 먼저 남기고 단계별 적용. 스테이징은 스킬 탐색 루트 밖.
- 원본 Git 저장소·외부 플러그인·보관함과 겹치는 경로는 이동 금지.
- 링크 대상 허용 정책, 복사 설치 업데이트, 일반 폴더 수용의 provenance 보존.
- 중단 후 시작 시 복구를 감지한다. 외부 변경은 보존하고 충돌을 알린다.
- 전환 완료 시 다른 플랫폼의 기존 사용 상태와 대표 내용 보존.

완료 검증: Cursor/Claude 경로 중첩 거절, OpenClaw 유지/OMP 제외, 단계별 실패·강제 종료 복구, 동시 업데이트/삭제, 대상 충돌·권한 오류, 역전환. 무중단 세션 전환을 보장한다고 표시하지 않음.

## T5 — 플랫폼 기준 토글과 확인 화면

관련 파일: src/pages/PlatformView.tsx, skill 카드/출처 그룹, SharedSkillImpactDialog 및 관련 store/types/i18n.

- 플랫폼 화면 주 토글은 해당 플랫폼, 공용 설치 전체는 별도 메뉴.
- 대표/다른 내용/비교 불가/별도 출처 잔존을 쉽게 구분.
- 전환 확인창은 구체적인 대상과 영향·복원 방법을 표시.
- 저장 성공과 재로드 대기·실행 확인을 구분. 검색어·기존 개별 비활성 상태 유지.

완료 검증: 기존 Vitest 범위인 PlatformView, SkillLocationGroup, skillUsageStore, SharedSkillImpactDialog를 확장하여 사용자 흐름을 검증. “설정은 저장했지만 재조회 실패”를 녹색 완료로 표시하지 않음.

## T6 — 전체 범위의 완료 판정

각 조사표 행에 제품/버전, 읽기 경로, on/off 왕복, 실제 목록 결과, 대표 선택 결과, 재로드 조건, 원본/타 플랫폼 보존, 남은 제한을 기록한다. 재현 fixture와 로그는 민감정보를 제외한다.

명세의 필수 시나리오를 우선 실행하고 변경에 맞는 Rust 단위/통합 테스트와 pnpm test, pnpm typecheck, pnpm lint를 실행한다. 코드 변경이 없는 조사 단계에서 전체 빌드를 돌려 기능 검증처럼 보고하지 않는다. 앱 빌드/교체/배포는 별도 범위다.

## 현재 완료된 것

- 사용자 정책 ADR·용어 정리.
- 기존 구현·테스트 도구 조사와 60개 플랫폼 조사표.
- 이 구현 명세와 선행 관계 작성.
- T2 설정 Adapter: OMP·CodeBuddy·OpenClaw·Factory Droid·Command Code·Hermes·Mistral Vibe·OpenCode와 기존 Claude·Codex 회귀 검증.

환경 조회: Node 22.23.1, pnpm 11.17.0, rustc 1.97.1. package.json은 React/Vite/Vitest, Rust는 Tauri 2이며 TOML 편집에는 toml_edit가 이미 있다. pnpm은 기존 package.json의 pnpm.onlyBuiltDependencies 필드를 무시한다는 경고를 출력했다. 이번 문서 작업 범위에서 설정을 변경하지 않았다.

T3 대표 출처 선택, T4 실제 설치 전환, Copilot·AGY를 포함한 나머지 플랫폼 Adapter와 T6 실행 중 세션 검증은 수행하지 않았다. 커밋·푸시·앱 교체도 수행하지 않았다.
