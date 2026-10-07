# CLAUDE.md

이 저장소에서 작업하는 AI 코딩 도구를 위한 프로젝트 지침이다.

## 작업 원칙

- 설명, 새 주석, 커밋 메시지는 한국어로 작성한다. 주요 개념은 쉽게 풀어 설명한다.
- 수정 전에 관련 코드, 설정, 도구와 Git 작업 상태를 확인한다. 다른 작업의 변경은 보존한다.
- 요청한 범위에서 최소한으로 수정하고, 관련 검증 결과와 실행하지 못한 검증을 알린다.
- 요청 없이 커밋·푸시·PR을 만들지 않는다. 비밀 정보와 빌드 산출물은 커밋하지 않는다.
- 이 문서의 설명과 코드가 다르면 실제 구현을 확인하고 문서도 함께 갱신한다. 테스트 개수나 과거 실패 목록을 고정된 사실로 취급하지 않는다.

## 개발 명령

Node.js 요구 버전은 `package.json`의 `engines`를 따른다. 현재 최소 버전은 22.13.0이다.

### 프런트엔드: React + TypeScript

```bash
pnpm install              # 의존성 설치
pnpm dev                  # Vite 개발 서버: 24200 포트, 프런트엔드 화면 확인용
pnpm web:dev              # 로컬 DB를 읽는 웹 대시보드 모드
pnpm build                # TypeScript 검사와 Vite 빌드
pnpm test                 # 전체 Vitest 테스트
pnpm exec vitest run src/test/skillStore.test.ts  # 특정 테스트 파일 실행
pnpm test:watch           # 변경을 감지해 테스트 재실행
pnpm typecheck            # TypeScript 타입 검사
pnpm lint                 # ESLint 검사
```

### Rust 백엔드: Tauri v2

```bash
cd src-tauri && cargo check                 # Rust 컴파일 가능 여부 확인
cd src-tauri && cargo test                  # 전체 Rust 테스트
cd src-tauri && cargo test db::             # DB 모듈 테스트
cd src-tauri && cargo clippy -- -D warnings  # 경고도 오류로 처리하는 정적 검사
```

### 앱 실행과 빌드

```bash
pnpm tauri dev                         # 실제 데스크톱 앱 개발 모드
pnpm tauri build --debug --bundles app # 로컬 실행용 앱 묶음 생성
```

## 빠른 Tauri 검증 지침

- 빌드·수동 테스트·컴파일 확인 요청에는 `.agents/skills/tauri-fast-verify/SKILL.md`를 따른다.
- 기본 수동 테스트는 `pnpm tauri dev`로 실행한다. 브라우저 화면 확인만으로 실제 Tauri 명령의 동작까지 검증했다고 판단하지 않는다.
- 실행 가능한 앱 묶음만 필요하면 `pnpm tauri build --debug --bundles app`을 사용한다.
- release 빌드와 DMG 생성은 사용자가 명시적으로 요청한 경우에만 실행한다.
- Tauri 빌드는 내부에서 프런트엔드 빌드를 실행하므로 같은 빌드를 바로 앞에서 중복 실행하지 않는다.
- 실패한 테스트를 확인 없이 기존 문제로 분류하지 않는다. 이번 변경과의 관련성을 확인해 보고한다.
- 문서만 수정한 경우에는 내용·경로·명령과 `git diff --check`를 확인한다. 앱 빌드는 생략할 수 있다.

## 구조 개요

여러 AI 도구의 스킬을 관리하는 데스크톱 앱이며, 세 계층으로 구성된다.

```text
React 화면 (src/) ──Tauri IPC──▶ Rust 백엔드 (src-tauri/src/) ──SQLx──▶ SQLite
```

- **프런트엔드**: React 18, TypeScript, Tailwind CSS 4, shadcn/ui를 사용한다. Zustand는 화면에서 공유하는 상태를 관리하고 React Router v7은 페이지 이동을 담당한다.
- **백엔드**: Rust 함수에 `#[tauri::command]`를 붙여 화면에서 호출할 수 있게 한다. `invoke()`는 이 명령을 호출하는 통로이며, 이를 IPC라고 한다.
- **데이터베이스**: `~/.skillsmanage/db.sqlite`의 SQLite를 사용한다. WAL 모드로 읽기·쓰기의 동시 처리를 지원하며, SQLx로 접근한다. 테이블 구조와 자동 이전 처리는 `src-tauri/src/db.rs`에 있다.
- **HTTP 통신**: `reqwest`로 GitHub와 AI 제공업체의 API를 호출한다.

### 핵심 개념

- **스킬(Skill)**: 이름·설명 등의 YAML 머리말을 포함하는 `SKILL.md`와 관련 파일을 관리하는 단위다.
- **중앙 보관함**: 배포할 원본을 보관한다. 기본 경로는 `~/.skillsmanage/skills/`이며 사용자가 변경할 수 있다. 실제 경로는 `db::get_central_skills_dir`로 조회하고 하드코딩하지 않는다.
- **공용 설치 위치**: `~/.agents/skills/`는 여러 도구가 함께 읽는 설치 위치다. 중앙 보관함과 구분한다.
- **플랫폼 설치**: 중앙 원본을 심볼릭 링크(원본을 가리키는 연결) 또는 복사로 설치한다. Codex의 개별 설치 경로 예시는 `~/.codex/skills/`이며, 실제 대상은 플랫폼 설정과 공용 경로 지원 여부를 확인한다.
- **자동 중앙화**: 다른 플랫폼에만 있는 스킬을 배포할 때 `linker.rs`의 `ensure_centralized`가 중앙으로 복사하고 `canonical_path`와 `is_central` 등을 갱신한다. 새로운 설치 방식에서도 이 처리를 재사용한다.
- **컬렉션(Collection)**: 사용자가 스킬을 묶어 관리하는 기능이다. 일괄 설치와 JSON 가져오기·내보내기를 지원한다.
- **프로젝트 탐색(Discover)**: 프로젝트 안의 스킬을 재귀적으로 찾는다. `is_already_central`은 불러올 때 파일 시스템을 기준으로 다시 계산한다.
- **마켓플레이스(Marketplace)**: GitHub 저장소의 스킬을 탐색하고 설치한다.

### 페이지 경로

실제 경로 정의는 `src/App.tsx`를 기준으로 한다.

| 경로 | 역할 |
|------|------|
| `/central` | 중앙 스킬 보관함, 기본 진입 화면 |
| `/universal` | 공용 설치 관리 |
| `/platform/:agentId` | 플랫폼별 스킬 보기 |
| `/skill/:skillId` | 스킬 문서, 정보, 설치 상태, 컬렉션 보기 |
| `/collections` | 컬렉션 관리 |
| `/discover`, `/discover/:projectPath` | 프로젝트 스킬 탐색 |
| `/obsidian/:vaultId` | Obsidian 보관함의 스킬 보기 |
| `/marketplace` | 마켓플레이스 탐색과 설치 |
| `/settings` | 앱 설정 |

### 주요 백엔드 명령 모듈

명령 모듈은 `src-tauri/src/commands/`에 있으며 전체 목록은 `mod.rs`를 확인한다.

| 파일 | 역할 |
|------|------|
| `scanner.rs` | 폴더 스캔과 SKILL.md 머리말 해석 |
| `agents.rs` | 내장·사용자 정의 플랫폼 조회, 추가, 수정, 삭제 |
| `linker.rs` | 스킬 설치, 연결, 복사, 설치 해제 |
| `skills.rs` | 스킬 조회와 Markdown 읽기 |
| `collections.rs` | 컬렉션 관리와 일괄 설치, 가져오기·내보내기 |
| `discover.rs` | 프로젝트 스킬 탐색과 가져오기 |
| `settings.rs` | 스캔 경로와 앱 설정 저장 |
| `marketplace.rs`, `github_import.rs` | GitHub 소스 동기화, 스킬 가져오기와 설치 |
| `storage.rs` | 중앙 보관함 경로 관리와 이전 |
| `platform_skill_control.rs` | 플랫폼별 스킬 사용 제어 |
| `skill_origin.rs` | 스킬 원본 출처 조회 |
| `recovery.rs`, `shared_delete.rs` | 복구와 공용 설치 삭제 처리 |
| `skill_translation.rs`, `on_device_translation.rs`, `repository_descriptions.rs` | 스킬·저장소 설명 번역과 기기 내 번역 |

### 프런트엔드 정적 데이터

| 파일 | 내용 |
|------|------|
| `src/data/officialSources.ts` | 공식 제공자 정보, 추천 스킬, 태그 |
| `src/data/aiProviders.ts` | AI 제공업체별 모델·접속 주소 설정 |

## 공통 UI와 코드 규칙

- **스킬 카드**: `src/components/skill/UnifiedSkillCard.tsx`를 재사용한다. 페이지마다 같은 역할의 카드를 새로 만들지 않는다. 화면 상황에 따라 props(컴포넌트에 전달하는 값)로 조정한다.
- **설치 창**: `src/components/central/InstallDialog.tsx`는 이미 설치되었거나 사용 가능한 플랫폼을 초기 선택에 반영한다. 공용 설치와 개별 설치의 선택 관계를 유지하고, Obsidian처럼 출처만 제공하는 항목을 설치 대상으로 넣지 않는다. 일괄 설치에는 `src/components/collection/CollectionInstallDialog.tsx`를 사용한다.
- **플랫폼 설치 전환**: `centralSkillsStore.togglePlatformLink` 등 기존 저장소 동작을 재사용한다. 공용 설치와 읽기 전용 출처를 단순한 개별 심볼릭 링크로 취급하지 않는다.
- **경로 별칭**: `@/`는 `src/`를 가리킨다. 설정은 `vite.config.ts`와 TypeScript 설정을 확인한다.
- **상태 관리**: 기능별 Zustand 저장소를 `src/stores/`에 둔다. 백엔드 명령 호출은 저장소에서 처리하고 컴포넌트에 직접 `invoke()`를 추가하지 않는다.
- **테마**: 기존 테마와 강조색 체계를 따른다. `data-theme`, `data-accent` 속성과 관련 스타일을 확인하고 색상을 임의로 고정하지 않는다.
- **다국어**: 한국어·영어·중국어를 지원한다. 화면에 보이는 문구는 `src/i18n/`의 번역 체계를 사용하고 세 언어를 함께 갱신한다. 프로젝트 지침을 한국어로 바꾸는 작업과 앱의 지원 언어 제거는 별개다.
- **프런트엔드 테스트**: Vitest, jsdom, React Testing Library를 사용한다. 공통 준비 코드는 `src/test/setup.ts`에 있다. Tauri 호출은 기존 테스트의 가짜 구현(mock) 방식을 따른다.
- **미사용 변수**: ESLint 설정은 `_`로 시작하는 인자와 변수의 미사용을 허용한다.
- **Rust DB 접근**: DB가 필요한 명령은 `State<AppState>`로 상태를 받는다. 빌드 시 DB 연결이 필요한 `sqlx::query_as!` 대신 기존의 `sqlx::query()`와 `Row::get()` 방식 등을 따른다.
- **마켓플레이스**: 기존 저장소 탐색·머리말 해석과 `marketplace_skills` 캐시를 재사용한다. 동기화와 검색에는 `sync_registry`, `search_marketplace_skills` 등 기존 명령을 우선 확인한다.
- **AI 설명**: 제공업체·키·모델·주소는 설정에서 읽는다. Anthropic/OpenAI 응답 형식과 `thinking` 블록 처리 등 기존 호환 동작을 유지한다.
- **설치·삭제 안전성**: 설치는 중앙 원본 확보 절차를 재사용한다. 삭제는 설치 대상, 공용 위치, 중앙 원본을 구분하고 기존 복구·출처 기록 처리를 확인한다.
