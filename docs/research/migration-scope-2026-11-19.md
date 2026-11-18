# 다른 언어로 옮기면 얼마나 다시 써야 하나: Rust 명령 123개의 이전 범위 (2026-11-19)

> 조사 기준 커밋: `origin/main` (`f1c1ba1`) · 관련 이슈: [#52](https://github.com/122yjs/skills-manage/issues/52) · 지도 이슈: [#49](https://github.com/122yjs/skills-manage/issues/49)

## 한눈에 보기 (쉬운 설명)

- **결론부터 말하면, 백엔드(뒤쪽 일꾼)를 Rust에서 다른 언어로 통째로 옮기는 일은 재작성 부담이 매우 크다.** 프로덕션(실제 동작) Rust 코드만 **28,153줄**(파일 안 단위 테스트까지 합치면 52,233줄)이고, 화면이 부르는 명령 **123개**와 이를 지키는 자동 검사(테스트) **672개**를 다시 만들고 검증해야 한다. (사실)
- **반면 "Rust 핵심을 그대로 두고 로컬 HTTP(웹 주소) 통로만 덧붙이는 방식(d)"은 다시 쓸 양이 가장 적다.** 123개 명령 중 **108개(약 88%)**가 이미 Tauri(데스크톱 앱 틀)와 분리된 순수 Rust 핵심 함수로 짜여 있어서, HTTP 통로와 보안 인증만 얹으면 대략 **1~2주** 안에 웹·휴대폰 연결의 토대를 열 수 있다. (사실 + 작업 기간은 추정)
- **다른 언어로 옮길 때 가장 아픈 병목 두 가지**는 (1) **#18·ADR-0002·ADR-0003 보안·데이터 보호 경계(20여 개 핵심 함수)**를 한 줄이라도 틀리지 않게 다시 짜고 테스트 672개로 다시 검증하는 일, (2) **macOS 기기 내 무료 번역(Swift `Translation.framework`)**이 Tauri 앱 창의 화면 객체(`NSView`)에 붙어 동작해 일반 웹 서버(Node·Bun·Go)로 옮기면 그대로 유지하기 어렵다는 점이다. (사실)
- **화면(React) 코드는 생각보다 훨씬 덜 묶여 있다.** 전체 `invoke()`(화면이 Rust 명령을 부르는 함수) 192회 중 **158회(82.3%)**가 이미 `src/lib/tauri.ts` 한 곳을 거친다. 직접 Tauri를 부르는 3개 파일(34회)만 정리하고, 실시간 진행 상황을 받는 `listen()` 3개 기능(7회)을 SSE(서버가 화면으로 진행 상황을 흘려보내는 통로)로 바꾸면 화면 31,619줄과 화면 테스트 921개는 거의 그대로 쓸 수 있다. (사실)
- **원본 저장소(`upstream`)와의 관계**: 로컬 기록 기준 `origin/main`은 분기점 이후 이미 **92커밋(243파일, +62,697줄 / -3,450줄)** 앞서 있고, 이번 조사에서 `git fetch upstream`을 시도했을 때 원본 저장소가 `Repository not found`로 응답했다. 원본과 패치를 주고받을 실익은 사실상 사라진 상태다. (사실)

---

## 0. 조사 범위와 기준 (사실 / 추정 구분)

이 문서는 모든 주장을 **사실(확인함)**과 **추정**으로 나눈다.

- **사실(확인함)**: 저장소 소스를 직접 읽고 줄 수·호출 횟수를 센 결과, `git` 명령 실행 결과, 공식 문서 URL 확인 결과.
- **추정**: 대안별 개발 기간 범위(초보 1인 개발자 + AI 에이전트 기준), 향후 설계 변경 시 예상 난이도.
- **측정 조건**: 지침에 따라 무거운 빌드·테스트(`cargo test`, `pnpm tauri build` 등)는 실행하지 않고 정적 소스 분석과 Git·외부 공식 문서 조회만 수행했다.

### 코드베이스 전체 규모 요약 (사실)

| 구분 | 파일 수 | 전체 줄 수 | 프로덕션 줄 수 | 테스트 줄 수 | 테스트 개수 | 근거 |
|---|---:|---:|---:|---:|---:|---|
| Rust 백엔드 (`src-tauri/src/`) | 33 | 52,233 | 28,153 (53.9%) | 24,080 (46.1%) | 656 (`#[test]` 164 + `#[tokio::test]` 492) | `wc -l`, `#[cfg(test)]` 범위 집계 |
| Rust 통합 테스트 (`src-tauri/tests/`) | 4 | 737 | 0 | 737 | 16 (`#[tokio::test]` 16) | `src-tauri/tests/*.rs` |
| macOS Swift 번역 브리지 + `build.rs` | 4 | 389 | 389 | 0 | 0 | `src-tauri/build.rs`, `src-tauri/native/SkillsManageTranslation/` |
| 프런트엔드 화면 (`src/`, 웹 대시보드 포함) | 126 | 31,619 | 31,619 | 0 | 0 | `src/` 내 비-테스트 `.ts`/`.tsx` |
| 프런트엔드 테스트 (`src/test/`) | 66 | 23,589 | 0 | 23,589 | 921 (`it`/`test`) | `src/test/*.test.*` |
| 기존 웹 대시보드 서버 (`web-dashboard/`) | 2 | 984 | 677 | 307 | 3 (`it`/`test`) | `web-dashboard/snapshot.ts`, `snapshot.test.ts` |
| **참고**: 웹 대시보드 화면 (`src/web-dashboard/`) | 12 | 1,776 | 1,776 | 0 | (위 `src/` 31,619줄에 포함) | `src/web-dashboard/` |

- **Rust 의존성 규모 (사실)**: `src-tauri/Cargo.toml`의 직접 의존성(`[dependencies]`)은 **23개**(+ macOS 전용 `swift-rs` 1개), `src-tauri/Cargo.lock` 전체 크레이트(간접 포함 라이브러리)는 **687개**다.
- **기존 웹 대시보드 규모 메모 (사실)**: 배경에서 언급한 "약 1,000줄"은 Node 서버 스냅샷 모듈(`web-dashboard/snapshot.ts` 677줄 + `snapshot.test.ts` 307줄 = **984줄**) 기준이며, 브라우저용 화면(`src/web-dashboard/` 12개 파일 **1,776줄**)까지 합치면 총 **2,760줄**(테스트 제외 2,453줄)이다.

---

## 1. 명령 지도: 123개 명령과 모듈별 줄 수·테스트 수 (사실)

`#[tauri::command]`(화면이 부를 수 있게 Rust 함수에 붙이는 표식)는 `src-tauri/src/` 내 **22개 파일**에 총 **123개**가 있으며, 모두 `src-tauri/src/lib.rs:35-171`의 `tauri::generate_handler![]`에 등록되어 있다.

> 참고: `discover.rs`와 `platform_skill_control.rs`, `secrets.rs`에는 파일 하단 `mod tests` 외에도 파일 중간에 `#[cfg(test)]` 테스트 전용 헬퍼가 들어 있다. 아래 표의 "테스트 블록 줄 수"는 중간 헬퍼를 포함한 모든 `#[cfg(test)]` 구간의 합계다.

### 1-1. 기능 묶음(9개) 요약표

| 기능 묶음 | 포함 파일 | 총 줄 수 | 프로덕션 줄 | 테스트 블록 줄 | 명령 수 | 단위 테스트 수 |
|---|---|---:|---:|---:|---:|---:|
| 1. 스캔·관찰·중복 비교 | `scanner.rs`, `observed_import.rs`, `skill_duplicates.rs` | 4,158 | 1,319 | 2,839 | 3 | 65 |
| 2. 플랫폼·도구 관리 | `agents.rs`, `dev_tools.rs` | 1,429 | 601 | 828 | 9 | 34 |
| 3. 설치·링크·사용 제어·삭제 | `linker.rs`, `usage.rs`, `shared_delete.rs`, `platform_skill_control.rs` | 12,125 | 7,171 | 4,954 | 19 | 130 |
| 4. 보관함·스킬·컬렉션·스킬셋 | `skills.rs`, `collections.rs`, `skill_groups.rs` | 5,738 | 2,377 | 3,361 | 24 | 93 |
| 5. 프로젝트 탐색·Obsidian | `discover.rs` | 5,107 | 1,651 | 3,456 | 11 | 66 |
| 6. GitHub 가져오기·출처·업데이트 | `github_import.rs`, `skill_origin.rs`, `skill_origin/*.rs` | 8,765 | 5,018 | 3,747 | 12 | 77 |
| 7. 마켓플레이스·AI 설명·번역 | `marketplace.rs`, `skill_translation.rs`, `on_device_translation.rs`, `repository_descriptions.rs`, `description_translation.rs`, `text_utils.rs` | 5,245 | 3,604 | 1,641 | 22 | 72 |
| 8. 설정·비밀값·복구·보관함 이전 | `settings.rs`, `recovery.rs`, `storage.rs`, `secrets.rs` | 4,586 | 2,996 | 1,590 | 21 | 53 |
| 9. DB·시작 복구·진입·경로 | `db.rs`, `startup.rs`, `path_utils.rs`, `lib.rs`, `main.rs`, `commands/mod.rs` | 5,080 | 3,416 | 1,664 | 2 | 66 |
| **합계** | **33개 파일** (+ 통합 테스트 4개 737줄, 16건) | **52,233** | **28,153** | **24,080** | **123** | **656 (+16)** |

### 1-2. 파일별 상세표

<details>
<summary>33개 Rust 소스 파일 전체 줄 수·명령·테스트 상세 보기 (클릭해 펼치기)</summary>

| 파일 (`src-tauri/src/` 기준) | 전체 줄 | 프로덕션 줄 | 테스트 줄 | 명령 수 | 단위 테스트 |
|---|---:|---:|---:|---:|---:|
| `commands/scanner.rs` | 3,771 | 1,058 | 2,713 | 1 | 62 |
| `commands/observed_import.rs` | 64 | 64 | 0 | 1 | 0 |
| `commands/skill_duplicates.rs` | 323 | 197 | 126 | 1 | 3 |
| `commands/agents.rs` | 1,212 | 461 | 751 | 7 | 31 |
| `commands/dev_tools.rs` | 217 | 140 | 77 | 2 | 3 |
| `commands/linker.rs` | 3,689 | 1,802 | 1,887 | 5 | 61 |
| `commands/usage.rs` | 3,605 | 1,902 | 1,703 | 8 | 44 |
| `commands/shared_delete.rs` | 314 | 314 | 0 | 2 | 0 |
| `commands/platform_skill_control.rs` | 4,517 | 3,150 | 1,367 | 4 | 25 |
| `commands/skills.rs` | 4,037 | 1,671 | 2,366 | 13 | 53 |
| `commands/collections.rs` | 1,092 | 328 | 764 | 10 | 34 |
| `commands/skill_groups.rs` | 609 | 378 | 231 | 1 | 6 |
| `commands/discover.rs` | 5,107 | 1,651 | 3,456 | 11 | 66 |
| `commands/github_import.rs` | 4,320 | 2,338 | 1,982 | 4 | 46 |
| `commands/skill_origin.rs` | 3,749 | 2,205 | 1,544 | 8 | 25 |
| `commands/skill_origin/discovery.rs` | 485 | 348 | 137 | 0 | 4 |
| `commands/skill_origin/installation_records.rs` | 211 | 127 | 84 | 0 | 2 |
| `commands/marketplace.rs` | 3,999 | 2,721 | 1,278 | 18 | 57 |
| `commands/skill_translation.rs` | 206 | 176 | 30 | 3 | 2 |
| `commands/on_device_translation.rs` | 276 | 253 | 23 | 0 | 1 |
| `commands/repository_descriptions.rs` | 684 | 408 | 276 | 1 | 10 |
| `description_translation.rs` | 56 | 35 | 21 | 0 | 2 |
| `text_utils.rs` | 24 | 14 | 10 | 0 | 1 |
| `commands/settings.rs` | 901 | 291 | 610 | 10 | 28 |
| `commands/recovery.rs` | 1,764 | 1,368 | 396 | 5 | 12 |
| `commands/storage.rs` | 1,721 | 1,187 | 534 | 6 | 11 |
| `secrets.rs` | 200 | 150 | 50 | 0 | 2 |
| `db.rs` | 4,419 | 2,984 | 1,435 | 0 | 54 |
| `startup.rs` | 311 | 149 | 162 | 2 | 4 |
| `path_utils.rs` | 147 | 80 | 67 | 0 | 7 |
| `lib.rs` | 174 | 174 | 0 | 0 | 0 |
| `main.rs` | 6 | 6 | 0 | 0 | 0 |
| `commands/mod.rs` | 23 | 23 | 0 | 0 | 0 |

</details>

---

## 2. 묶음별 결합도 분석 (사실)

### 2-1. 세 기준(Tauri 결합 · OS 고유 기능 · 순수 로직) + 핵심 함수 분리도

| 기능 묶음 (명령 수) | Tauri 고유 타입·기능 결합 | OS 고유 기능 사용 | 순수 로직 비중 | 명령 껍데기 분리 (`*_impl` 등) |
|---|---|---|---|---|
| 1. 스캔·관찰·중복 비교 (3) | `State`만 사용 (3/3), `spawn_blocking` 1회(`skill_duplicates.rs:120`) | 심볼릭 링크 메타/해석(`scanner.rs`, `observed_import.rs`), Unix 실행 비트 비교(`skill_duplicates.rs:66-70`) | 높음: YAML 머리말(`serde_yaml`), SHA-256 비교, SQLx | **3/3 (100%) 분리** (`scan_all_skills_impl`, `import_observed_skill_impl`, `compare_paths`) |
| 2. 플랫폼·도구 관리 (9) | `State`만 사용 (9/9) | OS별 경로·실행 파일 탐색 분기(`agents.rs`), 파일 존재 탐지 | 높음: SQLx CRUD, JSON 설정 | **9/9 (100%) 분리** (`agents.rs` 7, `dev_tools.rs` 2) |
| 3. 설치·링크·사용 제어·삭제 (19) | `State`만 사용 (19/19) | 심볼릭 링크 생성·검증·복원(`linker.rs` 51회, `usage.rs` 27회, `shared_delete.rs` 4회; Unix/Windows 분기), 파일 권한 `0o600`(`platform_skill_control.rs:400`) | 매우 높음: 원자적 폴더 복사, YAML/TOML/JSON5 주석 보존 편집(`toml_edit`, `json5`) | **19/19 (100%) 분리** (`*_impl` 및 `shared_delete::preview`/`delete`) |
| 4. 보관함·스킬·컬렉션·스킬셋 (24) | `State` 사용 (23/24), 무상태 1개(`open_in_file_manager`) | `Command::new` OS 파일 관리자 열기(`open`/`explorer`/`xdg-open`, `skills.rs:1642-1660`), 심볼릭 링크(`skills.rs` 16회) | 높음: 파일 트리 순회, SQLx, JSON 내보내기/가져오기 | **23/24 (95.8%) 분리** (`skills.rs:read_skill_content` 1개만 명령 내 직접 `fs::read_to_string`) |
| 5. 프로젝트 탐색·Obsidian (11) | `State` (9/11), 무상태 (2/11), **`AppHandle`+`Emitter`** 1개(`start_project_scan` → `discover:found`/`progress`/`complete`) | macOS Obsidian 경로 탐색(`Library/Application Support/obsidian/...`), 심볼릭 링크(`discover.rs` 4회) | 높음: 재귀 탐색, FNV-1a 해시, SQLx | **11/11 (100%) 분리** (`start_project_scan`도 `start_project_scan_impl`에 이벤트 콜백을 넘기는 구조) |
| 6. GitHub 가져오기·출처·업데이트 (12) | `State` (11/12), 무상태 (1/12), **`AppHandle`+`Emitter`** 1개(`import_github_repo_skills` → `github-import:progress`) | OS 키체인(`secrets::shared()`), `gh` CLI 프로세스(`Command::new`, `github_import.rs:1386`), Unix 실행 비트(`chmod +x`), 심볼릭 링크 | 높음: `reqwest`, `flate2`+`tar` 해제, SHA-256 매니페스트, SQLx | **8/12 (66.7%) 분리** (`skill_origin.rs` 4개 명령—`unlink_skill_origin`, `check_linked_skill_origins`, `prepare_skill_update`, `apply_skill_update`—은 명령 본문에 직접 SQLx/로직 포함) |
| 7. 마켓플레이스·AI 설명·번역 (22) | `State` (22/22), **`AppHandle`+`Emitter`** 2개(`explain_skill_stream`, `refresh_skill_explanation` → `skill:explanation:*`), **`AppHandle`+`WebviewWindow`(`ns_view`)** 1개(`translate_skill_description_on_device`) | **가장 높음**: macOS `Translation.framework` + `NaturalLanguage` + SwiftUI `NSHostingView` (`TranslationBridge.swift`, `on_device_translation.rs`), OS 키체인(AI API 키) | 높음: `reqwest` 스트리밍, Anthropic/OpenAI 파싱, skills.sh API, SQLx 캐시 | **12/22 (54.5%) 분리** (`marketplace.rs`의 10개 명령이 명령 본문에 직접 `sqlx`/`reqwest` 보유) |
| 8. 설정·비밀값·복구·보관함 이전 (21) | `State` (20/21), 무상태 1개(`get_secret_status`) | **OS 키체인**(`keyring` 크레이트 in `secrets.rs`), 심볼릭 링크(`recovery.rs` 25회, `storage.rs` 21회), 파일 권한(`recovery.rs` 8회) | 높음: SQLite 스냅샷 백업, `PRAGMA secure_delete`, JSON 매니페스트 | **21/21 (100%) 분리** (`settings.rs`, `storage.rs`, `recovery.rs` 모두 내부 핵심 함수 호출) |
| 9. DB·시작 복구·진입·경로 (2) | `tauri::Builder`, `tauri::plugin_*` 4개(`lib.rs:19-22`), **`AppHandle::manage`** (`retry_startup`, `startup.rs:139`) | OS 홈·앱 데이터 폴더(`path_utils.rs`) | 높음: DB 스키마 20개 테이블·마이그레이션(`db.rs`) | **1/2 분리** (`initialize_app`/`initialize_default` 분리, 단 `retry_startup`은 `app.manage` 직접 수행) |

### 2-2. Tauri 결합 핵심 수치 요약 (사실)

1. **명령 시그니처 결합 분포 (123개)**:
   - `State<'_, AppState>`만 받는 명령: **112개** (DB 풀 전달 외 Tauri 의존 없음)
   - 인자 없이 Tauri 타입을 전혀 안 쓰는 명령: **5개** (`discover_scan_roots`, `stop_project_scan`, `get_github_auth_status`, `get_secret_status`, `open_in_file_manager`)
   - `AppHandle`을 받는 명령: **6개**
     - 이벤트 전송(`.emit()`) 목적 4개: `discover::start_project_scan`, `github_import::import_github_repo_skills`, `marketplace::explain_skill_stream`, `marketplace::refresh_skill_explanation`
     - 지연 상태 등록(`app.manage()`) 목적 1개: `startup::retry_startup`
     - **실제 데스크톱 창(`WebviewWindow::ns_view()`) 필수 1개**: `skill_translation::translate_skill_description_on_device` → `on_device_translation::translate_on_device_impl`
2. **명령 껍데기 분리율**: **108 / 123개(87.8%)**가 Tauri `State`만 꺼내 순수 핵심 함수(`*_impl` 등)에 넘기는 얇은 어댑터다. 명령 본문에 직접 `sqlx::`/`reqwest::`/`fs::`가 섞인 곳은 **15개(12.2%)**뿐이다 (`marketplace.rs` 10, `skill_origin.rs` 4, `skills.rs` 1; `retry_startup` 포함 시 16개).
3. **Tauri 플러그인 사용 실태**: `Cargo.toml`과 `lib.rs:19-22`에는 `sql`, `fs`, `dialog`, `shell` 4개가 등록돼 있으나, `capabilities/default.json:6-10`에는 `core:default`, `dialog:allow-open`, `shell:allow-open` 3개만 열려 있고 프런트엔드에서는 실제로 `dialog`(2곳)와 `shell`(1곳)만 호출한다. `tauri-plugin-sql`과 `tauri-plugin-fs`는 현재 미사용 상태다.
4. **macOS Swift 번역의 숨은 창 의존성 (`TranslationBridge.swift` + Apple 공식 문서)**:
   - Apple의 `TranslationSession(installedSource:target:)` 직접 초기화(뷰 없이 번역)는 **macOS 26.4 이상**에서만 쓸 수 있다([Apple 공식 문서](https://developer.apple.com/documentation/translation/translationsession/init(installedsource:target:preferredstrategy:))).
   - 현재 앱은 **macOS 15.0+** 지원을 위해 SwiftUI 뷰 수정자(`.translationTask`)를 `NSHostingView`에 감싸 Tauri 메인 창의 `NSView`(`window.ns_view()`)에 붙이는 방식으로 구현되어 있다(`src-tauri/native/SkillsManageTranslation/Sources/SkillsManageTranslation/TranslationBridge.swift:12-37, 148-159`, `src-tauri/src/commands/on_device_translation.rs:142-169`). 따라서 창이 없는 백엔드 단독 서버(Node/Bun/Go CLI)에서는 이 방식이 동작하지 않는다.

---

## 3. 보안·데이터 보호 경계 목록 (재구현 필수 대상) (사실)

#18(평가 후속 작업)과 ADR-0002·ADR-0003이 세운 모든 안전 장치는 어떤 대안(Electron·TS·Go)으로 옮기더라도 **동일한 수준으로 다시 구현하고 테스트로 재검증**해야 한다.

| 보호 영역 | 규칙 요약 | 코드 위치 (`파일:함수`) |
|---|---|---|
| **파일 읽기·삭제 경로 제한** | 화면이 넘긴 임의 경로를 믿지 않고, 스킬 ID·상대 경로(`Component::Normal`)만 받아 `canonicalize` 후 스킬 폴더/보관함/개인 저장소 내부인지 검증 | `src-tauri/src/commands/skills.rs:read_skill_file_impl` (L1544-1579)<br>`src-tauri/src/commands/skills.rs:list_skill_directory_impl` & `build_skill_directory_nodes` (L520-584, L1601-1621, 심볼릭 링크 순환 차단)<br>`src-tauri/src/commands/skills.rs:validate_central_delete_target` (L1398)<br>`src-tauri/src/commands/repository_descriptions.rs:is_known_skill_file` & `known_skill_descriptions` (L371-395)<br>`src-tauri/src/commands/recovery.rs:saved_entry_path` & `is_path_inside` (L238-258)<br>`src-tauri/src/commands/usage.rs:validate_paused_path` (L181-193) |
| **원자적 복사·쓰기와 링크 탈출 차단** | 대상 옆 임시 스테이징 폴더에 전부 복사해 스냅샷(`tree_snapshot`) 검증 후 `rename`으로 일괄 확정. 스킬 밖을 가리키는 심볼릭 링크는 거부하고 내부 링크만 보존. 설정 파일은 `0o600`·`fsync` 후 교체하고 심볼릭 링크 설정 파일은 거부 | `src-tauri/src/commands/linker.rs:copy_dir_all`, `copy_into_stage`, `copy_inner_link`, `tree_snapshot` (L154-312)<br>`src-tauri/src/commands/linker.rs:create_verified_symlink` (L119-139)<br>`src-tauri/src/commands/linker.rs:ensure_centralized` (L323-353)<br>`src-tauri/src/commands/github_import.rs:write_snapshot_source_atomically` & `is_safe_repo_relative_path` (L1893-1924, L1961-1967)<br>`src-tauri/src/commands/skill_origin.rs:apply_downloaded_update` (L1970-2080, 백업 검증 → 격리/교체)<br>`src-tauri/src/commands/platform_skill_control.rs:write_atomic` (L374-434) |
| **관리 설치 / 관찰 항목 판정 (ADR-0002)** | 보관함 원본과 내용·권한이 같은 설치만 `is_managed=1`로 관리하고, 사용자가 직접 만든 폴더나 내용이 달라진 복사본(관찰 항목)은 제거·연쇄 삭제·전체 삭제·비활성 대상에서 제외. 읽기 실패 폴더는 기록 유지 | `src-tauri/src/db.rs:ensure_installation_managed` (L2383-2403)<br>`src-tauri/src/commands/scanner.rs:scan_all_skills_impl` (L767-904)<br>`src-tauri/src/commands/observed_import.rs:import_observed_skill_impl` (L17-55)<br>`src-tauri/src/commands/skill_duplicates.rs:reclassify_copy_installations` & `fingerprint` (L19-159)<br>`src-tauri/src/commands/shared_delete.rs:preview`, `delete_confirmed`, `delete` (L190-297, SHA-256 `confirmation_token`)<br>`src-tauri/src/commands/skills.rs:delete_central_skill_impl` (L1378-1477) |
| **비밀값 키체인 격리 및 이전 (ADR-0003)** | API 키·GitHub 토큰은 OS 키체인(불가 시 메모리 전용)에만 두고 DB에는 `"keychain"` 표시값만 저장. 일반 설정 명령은 비밀 키를 거부. 시작 시 이전 평문 DB·WAL·기존 백업 파일의 비밀값 칸만 덮어써 지움(`PRAGMA secure_delete = ON`) | `src-tauri/src/secrets.rs:Secrets` (`KeychainStore`, `MemoryStore`, L85-146)<br>`src-tauri/src/commands/settings.rs:get_setting_impl`, `set_setting_impl`, `get_secret_status_impl`, `set_secret_impl`, `overwrite_secret_settings`, `migrate_legacy_secrets_impl`, `clear_legacy_secrets_impl` (L62-205)<br>`src-tauri/src/commands/recovery.rs:blank_secrets_in_database_backups` & `blank_secrets_in_database_file` (L1080-1141)<br>`src-tauri/src/startup.rs:initialize_pool` (L66-104) |
| **토큰 전송 호스트 제한 & GitHub CLI 격리** | GitHub 토큰은 `https://api.github.com`·`raw.githubusercontent.com`에만 붙이고 제3자 미러·다른 호스트엔 미전송. 미러 대체는 기본 꺼짐(`false`). 테스트 빌드는 PC의 실제 `gh` CLI·키체인 차단 | `src-tauri/src/commands/github_import.rs:is_github_token_host` & `github_request` (L17-39)<br>`src-tauri/src/commands/github_import.rs:github_mirror_enabled` (L255-260)<br>`src-tauri/src/commands/github_import.rs:github_cli_path` & `resolve_github_auth` (L1352-1409)<br>`src-tauri/Cargo.toml:45-51` (`test-isolation` feature) |
| **압축 해제·파일 수집 한도 & 웹뷰 잠금** | ZIP/tar.gz 폭탄 방어 한도(다운로드 64 MiB, 파일 20,000개, 해제 256 MiB) 및 경로 순회 차단, UTF-8 글자 경계 자르기, 최소 CSP·권한 | `src-tauri/src/commands/github_import.rs:download_repository_archive`, `snapshot_from_repository_archive`, `relative_archive_path` (L1604-1772)<br>`src-tauri/src/commands/marketplace.rs:safe_install_directory_name` (L534-538)<br>`src-tauri/src/commands/skill_origin.rs:remote_files`, `collect_local_files` (L53-55, L250-364, 5,000파일/100 MiB/단일 20 MiB)<br>`src-tauri/src/text_utils.rs:truncate_at_char_boundary` (L4-13)<br>`src-tauri/tauri.conf.json:23` (CSP), `src-tauri/capabilities/default.json:6-10` |

---

## 4. 화면 쪽 결합도: `invoke()` · `@tauri-apps/*` 전수 조사 (사실)

### 4-1. 파일 분류별 `invoke()` 호출 및 Tauri import 현황 (비-테스트 코드 기준)

| 위치 구분 | `invoke()` 호출 횟수 | 파일 수 | 주요 파일과 특징 |
|---|---:|---:|---|
| `src/stores/` (Zustand 저장소) | **153회** (79.7%) | 17 | 전체 18개 store 중 `themeStore.ts` 1개를 제외한 17개가 사용.<br>이 중 **16개는 `@/lib/tauri` 경유**, **`settingsStore.ts:2`만 `@tauri-apps/api/core` 직접 import**(11회) |
| `src/pages/` (화면 페이지) | **27회** (14.1%) | 3 | **`SettingsView.tsx`** 혼자 **22회** 호출 + **`@tauri-apps/api/core` 직접 import**(`L3`), `MarketplaceView.tsx` 4회, `DiscoverView.tsx` 1회 |
| `src/components/` (컴포넌트) | **8회** (4.2%) | 7 | `SkillGroupBrowser.tsx`(2), **`SkillPickerDialog.tsx:3`**(1회, **`@tauri-apps/api/core` 직접 import**), `AppShell.tsx`(1), `MarketplaceSkillDetailDrawer.tsx`(1), `SkillPreviewDialog.tsx`(1), `SkillDetailView.tsx`(1), `SkillGroupLinks.tsx`(1) |
| `src/hooks/` (커스텀 훅) | **4회** (2.1%) | 1 | `useSkillDescriptionTranslation.ts`(4회, `@/lib/tauri` 경유) |
| **합계** | **192회** (100%) | **28** | 이 중 **158회(25파일)**는 `@/lib/tauri` 경유, **34회(3파일)**만 `@tauri-apps/api/core` 직접 import |

### 4-2. 그 밖의 Tauri 전용 결합 지점

1. **이벤트 수신 (`listen()`)**: 총 **7회**, **3개 기능(4개 파일)**에만 한정된다.
   - AI 설명 스트리밍: `src/lib/explanationStream.ts:59,67,73` (`skill:explanation:chunk`, `complete`, `error` — `skillDetailStore.ts:3`, `marketplaceStore.ts`에서 사용)
   - 프로젝트 탐색 진행률: `src/stores/discoverStore.ts:107,116,142` (`discover:found`, `progress`, `complete`)
   - GitHub 가져오기 진행률: `src/stores/marketplaceStore.ts:147` (`github-import:progress`)
2. **Tauri 플러그인 UI 호출 (3파일)**:
   - `@tauri-apps/plugin-dialog`: `src/components/layout/StartupRecovery.tsx:30`, `src/components/settings/CentralVaultSettings.tsx:44` (폴더 선택 대화창)
   - `@tauri-apps/plugin-shell`: `src/components/skill/GitHubSourceLink.tsx:2` (이미 `if (!isTauriRuntime()) return;`으로 웹 분기 처리됨)
3. **로컬 파일 관리자 열기**: `open_in_file_manager` 호출 4곳 (`DiscoverView.tsx:402`, `SkillDetailView.tsx:1028`, `SkillGroupLinks.tsx:27`, `SkillGroupBrowser.tsx:66,92` + `recoveryStore.ts:95,104`). 원격 브라우저나 휴대폰에서는 동작 대상이 없는 데스크톱 전용 기능이다.

### 4-3. `invoke → HTTP fetch`를 한 곳에서 바꿀 수 있는가? (판단)

- **결론(사실 근거 기반 판단)**: **약간의 사전 정리 후, 사실상 `src/lib/tauri.ts` 한 곳에서 바꿀 수 있는 구조다.**
  - 전체 192회 중 **158회(82.3%)**가 이미 `src/lib/tauri.ts`(`export const invoke = tauriInvoke; export const listen = tauriListen;`)를 import하고 있다.
  - 예외는 `@tauri-apps/api/core`를 직접 import하는 **3개 파일**(`src/pages/SettingsView.tsx:3`, `src/stores/settingsStore.ts:2`, `src/components/collection/SkillPickerDialog.tsx:3`, 합계 34회)뿐이다. 이 3곳을 `@/lib/tauri`로 바꾸면(한 줄 import 수정), 모든 명령 호출이 단일 어댑터를 통과하게 된다.
  - 단, `src/lib/tauri.ts`의 HTTP 어댑터 구현 외에 추가로 (1) `listen()` 3개 기능을 HTTP 환경의 **SSE(`/api/events` 또는 명령별 SSE 엔드포인트)**로 연결하는 코드, (2) `plugin-dialog` 2곳의 웹 대체(텍스트 입력 또는 서버 폴더 탐색기 API), (3) `src/test/` 내 `@tauri-apps/api/core` 직접 mock을 쓰는 store 테스트(14개 파일) 조정 여부가 남는다. (`src/lib/tauri.ts`가 `isTauriRuntime()`일 때는 Tauri로, 아닐 때는 HTTP `fetch`로 분기하면 기존 921개 Vitest 테스트의 대부분을 그대로 유지할 수 있다.)

---

## 5. 대안별 이전 범위 비교

### 5-1. 4개 대안 종합 비교표

| 항목 | (a) Electron (TS) | (b) TS 로컬 서버(Node/Bun) + 웹 화면 | (c) Go (Wails 또는 Go 서버) | (d) Rust 핵심 유지 + 로컬 HTTP 통로 추가 |
|---|---|---|---|---|
| **다시 써야 할 코드 (명령·줄 수)** | **123개 명령 전체, Rust 프로덕션 28,153줄**을 TypeScript로 재작성 + **테스트 672개** 대응분 재작성 | **123개 명령 전체, Rust 프로덕션 28,153줄**을 TypeScript로 재작성 + **테스트 672개** 대응분 재작성 (단 기존 `web-dashboard/snapshot.ts` 677줄 패턴 재활용 가능) | **123개 명령 전체, Rust 프로덕션 28,153줄**을 Go로 재작성 + **테스트 672개** 대응분 재작성 | **Rust 재작성 0줄.** 명령 본문에 로직이 직접 섞인 **15개 명령**만 `*_impl` 분리 리팩터링 + `retry_startup` 상태 분리 (~300~500줄 수정) |
| **그대로 쓸 수 있는 것** | React 화면 **31,619줄**, SQLite 스키마 **20개 테이블**, Vitest 화면 테스트 **921개** | React 화면 **31,619줄**, SQLite 스키마 **20개 테이블**, Vitest 화면 테스트 **921개**, `web-dashboard/` 구조 | React 화면 **31,619줄**, SQLite 스키마 **20개 테이블**, Vitest 화면 테스트 **921개** | **Rust 핵심 28,153줄 + Rust 테스트 672개 + Swift 번역 367줄 + React 31,619줄 + Vitest 921개 + SQLite 스키마 20개 테이블 전부 그대로 유지** |
| **새로 만들어야 할 것** | • Preload `contextBridge` + `ipcMain.handle` 123개 채널 ([공식 문서](https://www.electronjs.org/docs/latest/tutorial/ipc))<br>• `safeStorage` 기반 키체인 어댑터·기존 `keyring` 마이그레이션 ([공식 문서](https://www.electronjs.org/docs/latest/api/safe-storage))<br>• macOS Swift 번역용 N-API 네이티브 애드온(+ 숨김 `NSWindow`)<br>• Electron Builder 패키징·서명<br>• (휴대폰 접속 원하면 내부 HTTP 서버 추가) | • HTTP API(122개 라우트) + SSE(3개 스트림)<br>• 로컬/원격 인증(토큰·세션·CORS/Origin 검증)<br>• OS 키체인용 `@napi-rs/keyring` 네이티브 모듈<br>• macOS 번역용 별도 Swift 보조 바이너리(또는 기능 제외/API 대체)<br>• 백그라운드 데몬·자동 시작(LaunchAgent)·CLI/설치기 | • Wails 바인딩(데스크톱, [공식 문서](https://v3.wails.io/concepts/architecture)) **또는** Go `net/http`+SSE 서버 + 인증(웹·휴대폰)<br>• SQLite 드라이버(`modernc.org/sqlite` 등) & `zalando/go-keyring` 어댑터<br>• macOS Swift 번역용 CGo/Swift 브리지(또는 제외)<br>• Wails 또는 LaunchAgent 패키징 | • Rust HTTP 서버(`axum`) 라우터·SSE 엔드포인트 (약 500~1,000줄)<br>• 원격·로컬 접속용 토큰/Origin 인증 미들웨어<br>• `src/lib/tauri.ts` `fetch` + SSE 분기 & 3개 직접 import 파일 정리<br>• 웹용 폴더 선택 대체 UI (`plugin-dialog` 2곳)<br>• 데스크톱 없는 모드(헤드리스 데몬) 실행 진입점 |
| **보안·데이터 경계(#18, ADR-0002·0003) 위험도** | **높음**: 3장 표의 20+ 경계 함수를 JS/Node FS API(`lstat`/`realpath`/원자적 `rename`)로 전면 재구현·재검증 필요 | **높음**: 3장 표의 20+ 경계 재구현 필요. 특히 `node:sqlite`는 Stability 1.2(Release candidate)이며 동기 API([공식 문서](https://nodejs.org/api/sqlite.html)), OS 키체인도 별도 네이티브 애드온 필요 | **높음**: 3장 표의 20+ 경계 Go `os.Lstat`/`EvalSymlinks`로 전면 재구현 필요. Windows 심볼릭 링크·권한 처리 차이도 다시 검증 필요 | **매우 낮음**: #18·ADR-0002·0003을 지키는 Rust 구현과 672개 테스트가 **손대지 않은 채 그대로 동작** (단, HTTP 인증·CSRF·Origin 경계만 새로 검증) |
| **확장 고려: 화면 없는 Linux 서버(헤드리스)** | **부적합**: X11/Wayland 화면 없이는 가상 화면 필요, `safeStorage`도 평문 암호화(`basic_text`)로 떨어짐 | **가능**: 화면 없이 서버 단독 실행 가능(키체인은 메모리 폴백, Swift 번역·`xdg-open` 끄기) | **Go 서버만 가능 / Wails 불가**(화면 필수): 서버 시 키체인 메모리 폴백, Swift 번역·`xdg-open` 끄기 | **분리 실행 시 가능**: `tauri` GUI 없이 핵심 + `axum` 타깃만 띄우면 그대로 동작(Swift 번역 끄기, 키체인 `MemoryStore` 폴백) |

### 5-2. 대안별 주요 기술 1차 근거 (사실)

- **(a) Electron (TS)**:
  - 렌더러↔메인 통신은 `contextBridge.exposeInMainWorld` + `ipcRenderer.invoke` + `ipcMain.handle` 구조를 사용한다([Electron IPC 공식 문서](https://www.electronjs.org/docs/latest/tutorial/ipc)).
  - 비밀값은 Electron 내장 `safeStorage`(macOS Keychain, Windows DPAPI, Linux `gnome-libsecret`/`kwallet`)를 쓸 수 있으나, **Linux에서 비밀 저장소가 없을 때 `basic_text` 평문 암호화로 내려가는 동작**을 막기 위해 `getSelectedStorageBackend()` 확인과 ADR-0003 메모리 전용 대체 로직을 직접 짜야 한다([Electron safeStorage 공식 문서](https://www.electronjs.org/docs/latest/api/safe-storage)). 또한 macOS에서 코드 서명이 바뀌면 키체인 접근 재승인 팝업이 뜰 수 있다.
  - 기존 Rust `keyring` 서비스(`skills-manage`)에 저장한 키를 `safeStorage`로 읽을 수 없으므로(Electron `safeStorage`는 암호화된 바이트를 앱 설정 파일에 저장하는 방식이고 `keyring`은 평문 비밀번호를 키체인 항목으로 직접 저장), 기존 키체인 값을 읽어 옮기거나 재입력이 필요하다.
- **(b) TS 로컬 서버(Node 또는 Bun) + 웹 화면**:
  - 이미 저장소의 `package.json:9`가 `node >= 22.13.0`을 요구하고 있고, `web-dashboard/snapshot.ts:4`가 Node 내장 `node:sqlite`(`DatabaseSync`)를 사용 중이다. Node v22.13+에서는 `--experimental-sqlite` 플래그 없이 바로 쓸 수 있으나 여전히 **Stability: 1.2 (Release candidate)**이며 모든 DB 호출이 **동기식(blocking)**이다([Node.js SQLite 공식 문서](https://nodejs.org/api/sqlite.html)). Bun의 `bun:sqlite`도 동기식이다([Bun SQLite 공식 문서](https://bun.com/docs/runtime/sqlite.md)).
  - 반면 **OS 키체인**은 Node/Bun 표준 라이브러리에 없어 `keyring-rs`의 N-API 바인딩인 `@napi-rs/keyring` 같은 네이티브 애드온이 필요하며(`keytar`는 사실상 유지보수 중단), **macOS Swift 번역**은 GUI 뷰(`NSHostingView`)가 필요해 순수 CLI 서버 안에서는 바로 돌릴 수 없다.
- **(c) Go (Wails 또는 Go 서버)**:
  - Wails(v2/v3)는 Go 백엔드 메서드를 정적 분석/리플렉션으로 TypeScript 바인딩으로 생성해 OS WebView와 직접 연결한다([Wails v3 아키텍처](https://v3.wails.io/concepts/architecture), [Wails v3 브리지](https://v3.wails.io/concepts/bridge/)). 다만 Wails만으로는 휴대폰/외부 웹 접속이 안 되므로 Go HTTP 서버도 함께 열어야 한다.
  - Go 생태계에서는 CGo 없는 SQLite(`modernc.org/sqlite`, [pkg.go.dev](https://pkg.go.dev/modernc.org/sqlite@v1.60.1))와 OS 키체인(`zalando/go-keyring`, [pkg.go.dev](https://pkg.go.dev/github.com/zalando/go-keyring@v0.2.8))을 쓸 수 있다.
- **(d) Rust 핵심 유지 + 로컬 HTTP 통로 추가 — 심층 확인 (사실)**:
  1. **HTTP로 바로 내보낼 수 있는 명령 (최대 122 / 123개)**:
     - **즉시 가능 (117개)**: `State<'_, AppState>`만 받는 **112개** + 무상태 **5개**. (단, `open_in_file_manager` 1개는 서버가 도는 Mac의 Finder를 여는 명령이므로 휴대폰·원격 화면에서는 숨기거나 로컬 요청에만 허용하는 편이 자연스럽다.)
     - **작은 콜백 어댑터 추가 시 가능 (5개)**:
       - `start_project_scan`, `import_github_repo_skills`, `explain_skill_stream`, `refresh_skill_explanation` **4개**는 이미 진행률/청크 이벤트를 내보내는 지점이 명확해 `AppHandle::emit` 대신 Tokio broadcast/mpsc 채널 → **SSE**(`axum::response::sse::Sse`, [docs.rs 공식 문서](https://docs.rs/axum/latest/axum/response/sse/struct.Sse.html))로 내보낼 수 있다.
       - `retry_startup` **1개**는 `app.manage(AppState)` 대신 공유 상태(`Arc<RwLock<Option<DbPool>>>`)로 바꾸면 HTTP로도 동일하게 복구 가능하다.
     - **Tauri 데스크톱 창에 남거나 대체가 필요한 기능 (1개 + 플러그인 UI)**:
       - `translate_skill_description_on_device` (`on_device_translation.rs` + `TranslationBridge.swift`): `WebviewWindow("main").ns_view()`가 필수다. 데스크톱 앱이 함께 켜져 있을 때만 메인 스레드 창을 통해 실행하고, 창 없는 단독 서버·원격 모드에서는 API 번역(`translate_skill_description_with_api`) 또는 캐시 조회(`get_cached_skill_description_translation`)로 안내하는 분기가 필요하다. (macOS 26.4+만 대상으로 하면 `TranslationSession(installedSource:target:)`으로 창 없이 가능하나 macOS 15~26.3 지원이 깨진다.)
       - 프런트엔드 `@tauri-apps/plugin-dialog` 2곳(`StartupRecovery.tsx`, `CentralVaultSettings.tsx`): 웹/모바일에서는 직접 경로 입력 또는 서버 디렉터리 목록 API로 대체해야 한다.
  2. **`Cargo.lock`에 이미 있는 HTTP 관련 크레이트 (사실)**:
     - `axum` 자체는 `Cargo.lock`에 **없다**(새 직접 의존성 1개 추가 필요).
     - 하지만 `axum`의 핵심 기반 크레이트인 **`http` (1.4.0)**, **`hyper` (1.9.0)**, **`hyper-util`**, **`tower` (0.5.3)**, **`tower-http` (0.6.8)**, **`tokio`**, **`rustls`**는 이미 `reqwest`(0.12/0.13)와 `tauri`(2.10.3)가 `Cargo.lock`에 끌어와 놓은 상태다([docs.rs/axum](https://docs.rs/axum/latest/axum/), [Tauri v2 Calling Rust](https://v2.tauri.app/develop/calling-rust/)). 따라서 `axum`을 추가해도 네트워크·비동기 핵심 스택을 중복으로 늘리지 않고 재사용할 수 있다. (참고로 [`tauri-plugin-axum`](https://docs.rs/crate/tauri-plugin-axum/0.9.0)은 TCP 포트가 아니라 커스텀 프로토콜 `axum://localhost`를 쓰며 스트리밍을 지원하지 않으므로, 휴대폰/웹 접속 목적이라면 별도 포트의 `axum::serve`가 맞다.)

### 5-3. 확장 고려: 화면 없는 Linux 서버(헤드리스)에서 돌릴 수 있나 (사실)

> 아직 결정되지 않은 향후 확장(화면 없는 Linux 서버 등) 가능성을 가늠하기 위한 참고 정리다.

- **OS 고유 기능의 macOS 전용 여부와 Linux 헤드리스 동작**:
  - **Swift 기기 내 번역 (`on_device_translation.rs:25-42`, `build.rs:2`)**: **macOS 전용**(`#[cfg(target_os = "macos")]`). Linux에서는 **미지원(자동 기능 끄기)**이며 API 번역(`translate_skill_description_with_api`)으로 **대체 가능**.
  - **키체인 백엔드 (`secrets.rs:29-30, 84-134`, ADR-0003)**: macOS 전용은 아니나 Linux에서는 데스크톱 D-Bus Secret Service에 의존하므로, 화면 없는 헤드리스 Linux에서는 키체인을 쓸 수 없어 **`MemoryStore`(재시작 시 재입력)로 자동 전환(대체 가능)**.
  - **`gh` CLI 연동 (`github_import.rs:1352-1409`)**: macOS 전용 아님. Linux 서버에 `gh`가 설치·로그인돼 있으면 **그대로 동작**하고, 없으면 `cli_missing`으로 **기능 끄기** 후 토큰 입력으로 대체. (단, `agents.rs:122-130`의 추가 탐색 경로와 `discover.rs`의 macOS Obsidian 경로는 **macOS 전용**이며 Linux에서는 일반 `PATH`와 설정된 스캔 경로를 사용.)
  - **파일 권한·심볼릭 링크 (`linker.rs:85-88`, `platform_skill_control.rs:399-400`, `github_import.rs:1927-1953`, `recovery.rs:337-351`, `skill_duplicates.rs:66-70`)**: 모두 `#[cfg(unix)]` 공용 코드라 **Linux에서도 그대로 동작**한다.
  - **파일 관리자 열기 (`skills.rs:1642-1668`)**: Linux에서는 `xdg-open`을 호출하므로 화면 없는 서버에서는 **기능 끄기**(UI 비노출)가 필요하다.

- **대안별 헤드리스 Linux 실행 가능성 요약**:
  - **(a) Electron (TS)**: **부적합** — 화면(X11/Wayland) 없이는 가상 디스플레이(`xvfb`)가 필요하고, `safeStorage`도 Linux 비밀 저장소가 없으면 평문 암호화(`basic_text`)로 떨어져 별도 차단이 필요하다([Electron safeStorage 문서](https://www.electronjs.org/docs/latest/api/safe-storage)).
  - **(b) TS 로컬 서버(Node/Bun) + 웹 화면**: **바로 가능** — 화면 없이 서버만 띄울 수 있다(단, 키체인은 메모리 폴백, Swift 번역·`xdg-open`은 기능 끄기 필요).
  - **(c) Go (Wails 또는 Go 서버)**: **Go HTTP 서버 모드는 가능, Wails 데스크톱 모드는 불가(화면 필수)** — 서버 모드 시 키체인은 메모리 폴백, Swift 번역·`xdg-open`은 기능 끄기.
  - **(d) Rust 핵심 유지 + 로컬 HTTP 통로 추가**: **분리 실행 시 가능** — 현재 `lib.rs`의 `tauri::Builder`는 GUI 창을 전제로 하므로, `tauri` 없이 핵심 모듈 + `axum` 서버만 띄우는 헤드리스 실행 타깃(CLI/데몬)을 두면 POSIX 심볼릭 링크·권한·SQLite는 그대로 동작하고 Swift 번역은 자동 비활성(`unsupported_platform`), 키체인은 `MemoryStore` 폴백으로 돌아간다.

---

## 6. 작업량 추정 (초보 1인 개발자 + AI 에이전트 기준)

> **주의**: 아래 기간은 코드량(프로덕션 28,153줄), 테스트 수(Rust 672개 + Vitest 921개), 보안 경계(#18·ADR-0002·0003의 20+ 핵심 함수) 및 초보 1인 개발자의 검토·디버깅 속도를 바탕으로 한 **추정**이다.

| 대안 | 예상 기간 (추정) | 주요 근거와 병목 요인 |
|---|---|---|
| **(d) Rust 핵심 유지 + 로컬 HTTP(axum) 추가** | **1 ~ 2주** | • Rust 28,153줄·테스트 672개·보안 경계 20+개 **재작성 0줄**<br>• 명령 15개(`marketplace.rs` 10, `skill_origin.rs` 4, `skills.rs` 1)만 `*_impl` 분리 + axum 라우터/SSE 4종 + 토큰 인증 + `src/lib/tauri.ts` HTTP 어댑터 작업<br>• 검증할 테스트: 기존 672개 + 새 HTTP/인증/SSE 테스트 수십 개 |
| **(b) TS 로컬 서버(Node/Bun) + 웹 화면** | **4 ~ 8주** | • Rust 프로덕션 28,153줄 및 테스트 672개(파일시스템·SQLite·YAML/TOML 주석 보존·tar.gz) 전면 재작성<br>• `toml_edit`·`json5` 주석 보존 편집(`platform_skill_control.rs` 3,150줄)과 #18 보안 경계의 Node FS 정밀 구현·검증에 시간 집중<br>• OS 키체인(`@napi-rs/keyring`)·macOS Swift 번역 분리·데몬/LaunchAgent·인증 신규 개발 필요 |
| **(a) Electron (TS)** | **5 ~ 9주** | • 백엔드 재작성 규모(28,153줄 + 테스트 672개)는 (b)와 동일<br>• 추가로 123개 IPC 채널(`preload` + `ipcMain`), `safeStorage` 키체인 이전·Linux fallback 처리, Swift 번역용 N-API 네이티브 모듈, Electron 패키징/코드서명 구성 필요<br>• 휴대폰 웹 접속까지 원하면 내부 HTTP 서버도 별도 추가해야 함 |
| **(c) Go (Wails 또는 Go 서버)** | **6 ~ 11주** | • 백엔드 28,153줄 + 672개 테스트를 Go로 전면 재작성 + TS/Rust/Go 3개 언어 혼용으로 1인 초보 개발자의 디버깅·검토 비용 상승<br>• `modernc.org/sqlite`, `zalando/go-keyring`, YAML/TOML 주석 보존, CGo Swift 번역(또는 제외) 해결 필요<br>• Wails는 모바일 웹 접속을 못 열므로 Go HTTP 서버 또는 이중 모드 설계 추가 부담 |

---

## 7. 원본 저장소(`upstream`)와의 관계 (사실 + 판단)

1. **현재 분기 상태 (사실)**:
   - `git rev-list --count upstream/main..origin/main` = **92** (`origin/main`이 로컬 `upstream/main`보다 92커밋 앞섬).
   - `git rev-list --count origin/main..upstream/main` = **0** (분기점 `467d042`, 2026-05-02 이후 `upstream/main`에만 있는 커밋 0개).
   - `git diff --shortstat upstream/main origin/main` = **243 files changed, 62,697 insertions(+), 3,450 deletions(-)**.
   - 이번 조사 중 `git fetch upstream`을 실행한 결과 `fatal: repository 'https://github.com/iamzhihuix/skills-manage.git/' not found`가 발생했다. (원본 저장소가 삭제·비공개화 또는 주소 변경되어 현재 GitHub에서 내려받을 수 없음.)
2. **언어 변경 시 영향 (판단)**:
   - **(a)·(b)·(c)처럼 다른 언어로 옮기면**, 앞으로 원본이나 다른 포크에서 Rust 버그 수정이 나와도 `git cherry-pick`이나 `git merge`로 가져올 수 없고, 사람이 변경 의도를 읽고 TS/Go로 다시 짜야 한다.
   - **(d) Rust 핵심 유지**는 Rust 파일 구조를 유지하므로 패치 이식이 이론상 가능하다.
   - 다만 이미 243파일(+62,697 / -3,450줄)이 달라졌고 원본 저장소 자체도 현재 접근할 수 없으므로, **원본 추종 여부를 의사결정의 큰 제약으로 삼지 않아도 무방하다**(사실상 독자 제품으로 발전 중).

---

## 8. 이번 조사에서 확인하지 못한 것과 추가 측정 제안

### 확인하지 못한 것 (미실행·미확인 항목)
- 지침에 따라 `cargo test` / `cargo clippy` / `pnpm test`를 실행하지 않았으므로, 현재 커밋에서 672개 Rust 테스트와 921개 Vitest 테스트의 실제 실행 통과 여부 및 소요 시간은 측정하지 않았다(최신 CI 결과는 별도 티켓에서 다룸).
- `marketplace.rs`의 10개 비분리 명령과 `skill_origin.rs` 4개 명령 내부를 완전히 `*_impl`로 추출할 때 필요한 정확한 리팩터링 패치 크기.
- macOS 26.4의 `TranslationSession(installedSource:target:)`이 GUI 없는 백그라운드 데몬 프로세스(LaunchAgent)에서도 실제로 동작하는지 여부는 Apple 공식 문서만으로는 미확인(실행 검증 필요).

### 추가 측정 제안
1. **(d) 선택 전 1시간 스파이크 측정**: `src-tauri/Cargo.toml`에 `axum = "0.8"`을 임시로 얹어 `cargo check` 시 기존 `Cargo.lock` 687개 크레이트에서 **추가되는 크레이트 개수와 증분 빌드 시간**을 1회 실측한다.
2. **기존 테스트 실행 시간 기준선**: `cargo test`와 `pnpm test`의 로컬 실측 시간을 확인해 (d) vs 재작성 대안의 개발 피드백 속도 차이를 비교한다.
3. **웹 모드 UI 갭 점검**: `src/components/layout/StartupRecovery.tsx`, `src/components/settings/CentralVaultSettings.tsx`, `open_in_file_manager` 4곳을 브라우저 단독 환경에서 띄웠을 때 대체 UI가 필요한 화면 목록을 확정한다.
