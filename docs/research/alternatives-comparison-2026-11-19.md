# 대안 기술 비교: Tauri 유지 · Electron · TS 로컬 서버+PWA · Go

- 작성일: 2026-11-19
- 기준 저장소: `122yjs/skills-manage` (`origin/main`)
- 관련 이슈: #49(기술 방향 지도), #53(본 조사), #50(CI 시간), #51(앱 무게), #52(코드 이전 범위), #54(휴대폰 접속 구조·보안)
- 표기 규칙: 각 근거는 **[사실(확인함)]**, **[경험담]**, **[추정]**으로 구분해 적는다.

---

## 한눈에 보기 (쉬운 설명)

지금 앱은 화면은 TypeScript(React), 뒤쪽 파일·DB 작업은 Rust(Tauri v2), 온디바이스 번역은 Swift로 나뉘어 있다. 이 글은 "Rust를 버리고 다른 언어로 갈아타야 할까, 아니면 웹 대시보드를 키워 휴대폰에서도 쓰게 만들까"를 고르기 위한 비교표다.

핵심 결론은 네 줄로 요약된다.

1. **가장 적게 고치고 앱을 가볍게·빠르게 쓰고 싶다면 `1안(Rust/Tauri 유지 + 개선)`**: 이미 만든 Rust 코드 약 52,200줄과 명령 123개를 버리지 않아도 된다. 현재 설정 파일에는 실행 파일 크기를 줄이는 Release 옵션(`strip`, `lto` 등)이 빠져 있고, 화면만 바꾸는 PR은 이미 Mac CI를 건너뛰게 개선되었다 [1][2]. 단, 휴대폰 관리 기능은 따로 추가해야 한다.
2. **휴대폰 관리를 붙이되 기존 Rust 코드를 살리고 싶다면 `5안(Rust 핵심 유지 + 로컬 HTTP 통로 추가)`**: 지금 있는 Rust 로직을 웹 API 서버(예: `axum`)로도 열어 데스크톱과 휴대폰이 같은 기능을 쓰게 만드는 중간 다리다 [3]. 단, Rust 코드를 계속 유지보수해야 하는 부담은 남는다.
3. **Rust가 너무 어렵고 앞으로 혼자(또는 AI와) TypeScript 하나로 오래 고치고 싶다면 `3안(TypeScript 로컬 서버 + PWA)`**: 화면과 서버를 같은 언어(TS)로 통합해 CI가 Linux에서 1~2분 내로 끝나고 휴대폰 웹 관리와도 가장 잘 맞는다 [4][5]. 대신 기존 Rust 명령 123개를 다시 짜는 큰 이전 작업(#52 참조)과 키체인·Swift 번역을 다루는 별도 처리를 감수해야 한다.
4. **`2안(Electron)`과 `4안(Go)`은 이 프로젝트 조건과 엇갈리는 면이 있다**: Electron은 115MB 이상으로 지금(85MB)보다 무거워지고 [6], Go(Wails 또는 서버)는 TS+Go 2개 언어 구조를 다시 짜야 해서 언어 단일화 이점이 약하다.

---

## 현재 출발점 (`122yjs/skills-manage` 실측)

비교 기준이 되는 현재 저장소 상태를 1차 소스 코드와 설정에서 확인한 결과는 다음과 같다.

| 항목 | 현재 상태 (2026-11-19 기준 확인) | 비고 | 출처 |
|---|---|---|---|
| 프런트엔드(화면) | React 18 + TypeScript, 비테스트 코드 31,619줄(테스트 포함 55,208줄) | UI·상태 관리 자산 | **[사실(확인함)]** `package.json`, `src/` |
| 백엔드(뒤쪽 로직) | Rust(Tauri v2), 52,233줄, `#[tauri::command]` 123개 | `sqlx` SQLite + `keyring` v4 + `serde_yaml` 0.9 | **[사실(확인함)]** `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs` |
| macOS 번역 브리지 | Swift 349줄 (`swift-rs`로 정적 링크, `Translation.framework`) | macOS 15+ 온디바이스 번역, 하위 버전 weak link | **[사실(확인함)]** `src-tauri/build.rs`, `src-tauri/native/` |
| 기존 웹 대시보드 | Vite 개발 서버(`127.0.0.1:24200`)에서 `node:sqlite`로 DB 읽기 전용 조회(`GET /api/dashboard`) | `web-dashboard/snapshot.ts` | **[사실(확인함)]** `vite.config.ts`, `web-dashboard/snapshot.ts` |
| 보안·데이터 경계 | Capability(`core:default`, `dialog:allow-open`, `shell:allow-open`), 비밀값은 OS 키체인(ADR-0003) | `release-desktop.yml`은 현재 `--no-sign`(미서명), 자동 업데이터 미구현 | **[사실(확인함)]** `default.json`, `secrets.rs`, `release-desktop.yml` |
| CI·무게 실측 | Backend(Rust, `macos-15-intel`) 캐시 4분 / 미스 16~19분, Frontend(ubuntu) 1.5~2분, 앱 85MB(실행 파일 88.6MB 중 `__LINKEDIT` 47.9MB) | 세부 원인은 #50, #51 참조 | **[사실(확인함)]** 티켓 #53 출발점, `backend.yml`, `ci.yml` |

---

## 쉬운 비교표

각 항목은 `좋음` / `보통` / `나쁨`으로 평가했다. 코드 이전 범위(#52)와 휴대폰 접속 구조·보안(#54)은 별도 티켓 범위이므로 방향만 적는다.

### 1) 핵심 대안 5종 비교

| 비교 항목 | ① Rust/Tauri 유지 + 개선 | ② Electron (TypeScript) | ③ TS 로컬 서버 + PWA (Node/Bun) | ④ Go (Wails v3 또는 Go 서버+웹) | ⑤ Rust 핵심 유지 + 로컬 HTTP 통로 |
|---|---|---|---|---|---|
| **CI 시간** | **보통~나쁨** — Rust 콜드 빌드 16~19분·캐시 4분 + Mac 서버 필요. 단 경로 필터로 UI만 수정 시 건너뜀 [1][2] | **좋음~보통** — TS/JS 빌드는 빠르나(1~2분대), Mac 패키징/네이티브 addon 재빌드 시 Mac 러너(분당 $0.062, Linux의 약 10.3배) 필요 [4][7] | **좋음** — ubuntu 표준 러너(4코어)에서 1~2분 내 테스트·빌드 가능. Mac 러너 의존이 가장 적음 [4][8] | **좋음** — Go 컴파일은 Rust보다 수배 빠르고 크로스 컴파일 쉬움. 단 Wails Mac 번들은 Mac 러너 필요 [9][10] | **보통~나쁨** — Rust 크레이트(`axum` 등)가 추가되어 ①과 비슷하거나 컴파일 대상이 약간 늘어남 [11] |
| **앱 무게 (크기·메모리)** | **좋음** — 현재 85MB이나 Release 프로필(`strip=true`, `lto=true`) 미적용 상태라 줄일 여지 큼(상세 #51 참조) [12][13] | **나쁨** — Chromium+Node 내장으로 `darwin-arm64.zip`만 115MB, 설치 시 150MB+. 창마다 렌더러 프로세스 추가 [6][14] | **좋음~보통** — 서버만 배포 시 아주 작음. 단일 실행 파일로 묶으면 Bun(zip 38MB, 실행 파일 약 50~90MB 추정)·Node SEA 크기 발생 [6][15] | **좋음** — Wails v3 release 빌드 약 15MB(debug 약 50MB). 순수 Go HTTP 서버는 더 가벼움 [9] | **좋음** — Tauri 기반 유지, `axum` 추가분은 수 MB 수준으로 작음(**추정**) |
| **보안 위험·키체인** | **좋음** — Capability 최소 권한 모델 + Rust `keyring` v4 직접 사용. 단 로컬 웹 없음 [16][17] | **보통** — `contextIsolation`/`sandbox` 등 20가지 체크리스트 준수 필요. `keytar`는 2022년 보관됨, `safeStorage`는 macOS 코드 서명 권장 [18][19] | **보통** — 로컬 HTTP의 CSRF·DNS 리바인딩(도메인 주소 바꿔치기) 방어 필수(#54 참조). 키체인: `@napi-rs/keyring` 또는 `Bun.secrets`(실험적) [20][21] | **좋음~보통** — Go 1.25 `CrossOriginProtection` 표준 내장(1.25.1 패치 권장). `go-keyring`은 macOS `/usr/bin/security` CLI 의존 [22][23] | **보통** — Tauri IPC 보안에 로컬 HTTP 방어(Host/Origin 검증, 토큰 등, #54 참조) 추가 필요. 키체인은 기존 `keyring` v4 유지 [17][24] |
| **휴대폰 관리 적합성** | **나쁨** — IPC(창 내부 통신) 전용이라 외부 HTTP 접속 불가. 웹 대시보드가 읽기 전용으로 분리돼 있음 [25] | **보통~나쁨** — 기본은 데스크톱 창 전용. 휴대폰 관리를 하려면 내부에 HTTP 서버를 별도로 띄워야 함 [14] | **좋음** — HTTP API + 웹 화면이 본체. Tailscale Serve(`https://*.ts.net`)와 조합하면 iOS PWA 조건(HTTPS) 충족(#54 참조) [26][27] | **좋음(서버) / 보통(Wails)** — Go 표준 `net/http`로 API 서빙이 자연스러움. Wails는 기본 IPC지만 Go 서버 병행 가능 [9] | **좋음** — Tauri는 데스크톱용으로 두고, 같은 Rust 로직을 `axum` HTTP/WS로 웹·PWA에 동시 노출(GitButler `but-server` 패턴) [3][28] |
| **초보자 1인 유지보수** | **나쁨** — TS+Rust+Swift 3개 언어. Rust 소유권·컴파일 오류를 초보자가 고치기 어려움(**경험담** 다수) [29] | **좋음** — TS 단일 언어로 통일 가능. 단 Swift 온디바이스 번역은 별도 CLI 분리 필요 | **좋음** — TS 단일 언어 + Vite로 빠른 수정-확인 루프. 단 백그라운드 데몬·LaunchAgent 관리 필요 | **보통** — Rust보다 문법이 쉽고 AI도 잘 쓰지만, 결국 TS+Go 2개 언어를 유지해야 함 [30] | **나쁨~보통** — 기존 TS+Rust+Swift 3언어에 HTTP 계층(CORS·라우팅·상태 동기화)까지 더해짐 |
| **생태계 (DB·YAML·감시·배포)** | **보통** — `sqlx`/`notify` 탄탄하나 `serde_yaml`이 2024-03 아카이브됨. 자동 업데이트 미구현 [13][31][32] | **좋음** — 성숙도 최고. 단 `better-sqlite3` 같은 네이티브 모듈은 Electron ABI 재빌드 주의 필요 [33][34] | **좋음** — `node:sqlite`(v26 기준 RC)·`chokidar` v6·`octokit` 풍부. 단 Node 22 기준 `node:sqlite`는 실험적 단계였음 [5][35][36] | **보통** — `go-github`·`fsnotify`·`go.yaml.in/yaml/v3` 탄탄. 단 Pure Go SQLite는 대량 쓰기 느리고 CGo는 크로스 컴파일 불편 [37][38][39] | **보통** — ①의 생태계 + `axum` 0.8(`tokio`·`tower-http` 호환) 추가. `serde_yaml` 교체 과제 동일 [11][31] |
| **이전 비용 (개요)** | **매우 적음** — 현 코드 100% 유지(#52 참조) | **큼** — Rust 52,233줄/123개 명령을 TS로 재작성(#52 참조) | **큼** — Rust 52,233줄/123개 명령을 TS로 재작성(#52 참조) | **매우 큼** — Rust 52,233줄을 Go로 재작성(#52 참조) | **중간** — Rust 핵심을 공용 crate로 분리하고 HTTP 어댑터 추가(#52 참조) |
| **[확장 고려] 헤드리스 서버·다중 OS 실행** | **나쁨** — Tauri GUI는 GTK/WebKitGTK 등 화면 의존성이 있어 CLI/데몬으로 따로 떼지 않으면 화면 없는 Linux 서버 실행 불가 [63] | **나쁨** — Chromium 창(Main/Renderer) 구조라 화면 없는 서버에서 직접 실행 불가(백엔드만 Node CLI 분리 필요) [46] | **좋음** — GUI 없는 순수 서버 프로세스라 화면 없는 Linux 서버나 다른 OS에서도 그대로 실행(단 Mac Swift 번역 제외) [25][43] | **좋음(서버) / 보통(Wails)** — 순수 Go HTTP 서버는 단일 바이너리로 헤드리스 서버 실행이 매우 쉬움. Wails GUI는 화면 필요 [9] | **좋음~보통** — Rust 핵심을 헤드리스 HTTP 서버 바이너리(`but-server` 패턴)로 함께 빌드하면 화면 없는 서버에서도 동일 로직 실행 가능 [3] |
| **[확장 고려] 여러 기기 한 화면 관리** | **나쁨** — 로컬 Tauri IPC 전용이라 다른 기기를 함께 보려면 원격 통신 계층을 새로 설계해야 함 [16] | **보통~나쁨** — 웹 UI에서 여러 원격 API를 묶는 것은 가능하나 각 기기에 별도 Node 에이전트 구축 필요 [14] | **좋음** — 각 기기에 같은 경량 HTTP 서버를 띄우고 사설망을 통해 한 웹 UI에서 여러 기기 엔드포인트를 조회·관리하기 쉬움(#54 참조) | **좋음(서버) / 보통(Wails)** — 기기마다 단일 Go HTTP 에이전트를 띄워 한 웹 UI로 모으기 적합(Wails는 서버 분리 필요) | **좋음~보통** — 기기별 OS용 Rust HTTP 서버 바이너리를 배포하면 한 웹 UI/Tauri UI에서 다중 기기 API 호출로 확장 가능 [3] |

### 2) 근거가 있어 함께 검토한 추가 대안 3종

| 대안 | 핵심 특징과 근거 | 이 프로젝트에서의 평가 | 출처 |
|---|---|---|---|
| **⑥ Electrobun** | Bun + Zig/Obj-C/C++ 기반, 시스템 WebView 기본(CEF 선택), Zstandard 자체 압축 번들·초경량 BSDIFF 패치. GitHub 스타 13k+(v2.0.6 확인) | **보통~비추** — TS(Bun) 단일 언어와 가벼움은 매력적이나, 공식 저장소에 "이슈/PR 리뷰를 기대하지 말라"고 명시돼 있고 생태계가 신생이며 휴대폰 HTTP 서버는 별도 필요 | **[사실(확인함)]** [40] |
| **⑦ Neutralinojs** | 시스템 WebView + 로컬 WebSocket + 일회성 토큰(OTT) 인증으로 극소 크기 달성 | **나쁨(부적합)** — 범용 JS 백엔드 런타임이 없고, Windows 공식 프리빌드가 x64에 치우쳐 있으며, 123개 SQLite·파일·키체인 명령을 구현하기에 생태계가 부족 | **[사실(확인함)]** [41][42] |
| **⑧ Swift 네이티브 (SwiftUI)** | Apple Silicon macOS 전용, `Translation.framework`·Keychain 직접 호출, OS 기본 크기·성능 | **나쁨(목표 불일치)** — 기존 React/TS UI 31,619줄을 전부 버리고 SwiftUI로 새로 써야 하며, 웹 대시보드/휴대폰 PWA와 UI 코드를 공유할 수 없음 | **[사실(확인함)]** `src/` 줄 수, [43] |

---

## 항목별 상세 비교와 근거

### 1. CI 시간 (GitHub Actions 기준)

- **[사실(확인함)] 러너 사양과 비용 구조**: 2026-11-19 기준 GitHub Actions 공식 문서에 따르면 공개 저장소는 표준 러너가 무료다 [7]. 비공개로 전환하거나 과금 단가를 비교하면 Linux 2코어 x64가 분당 `$0.006`, Windows가 `$0.010`, macOS(3코어 M1 또는 4코어 Intel)가 `$0.062`로 **macOS 러너는 Linux보다 약 10.3배 비싸다** [7]. 공개 저장소 표준 리눅스 러너(`ubuntu-latest`)는 4코어 16GB RAM을 제공하는 반면 `macos-15-intel`은 4코어 14GB RAM, `macos-15`(M1)는 3코어 7GB RAM이다 [8].
- **[사실(확인함)] 현재 저장소 현황**: `.github/workflows/backend.yml`은 `src-tauri/**`가 바뀔 때만 `macos-15-intel`에서 `cargo fmt --check`, `cargo test`, `cargo clippy`를 돌린다 [2]. `ci.yml`의 Frontend 작업(`ubuntu-latest`)은 `pnpm install`, `build`, `test`, `typecheck`, `lint`를 합쳐 1분 30초~2분 걸린다 [4].
- **[사실(확인함)] Rust CI가 느린 이유와 한계**: GitHub Actions의 캐시 한도는 저장소당 총 10GB이며 매 실행마다 압축 업로드·다운로드 네트워크 시간이 든다 [44]. Rust는 의존성(크레이트)을 매크로·제네릭 단위로 컴파일하므로 캐시가 깨지면 16~19분처럼 콜드 컴파일이 길어진다.
- **[경험담] Rust vs Go vs TS 빌드 체감**:
  - Go 개발자 사례에서 콜드 빌드는 Go가 Rust보다 수배 빠르다는 보고가 흔하다(블로그 벤치마크 예: 10만 줄 기준 Go 1.24 약 4.2초 vs Rust 1.85 약 11.7초, 단 CI 환경은 다름) [45]. Go는 1개 언어의 모듈 캐시만 쓰며 크로스 컴파일이 매우 빠르다 [30].
  - TS 로컬 서버 + PWA는 컴파일 단계가 `tsc` 타입 검사와 번들링뿐이라 Linux 러너만으로 1~2분 내 CI가 끝난다 [4].
  - Electron은 TS 코드 검사 자체는 빠르지만, `better-sqlite3` 같은 C++ 네이티브 모듈을 쓰거나 macOS `.app`/`.dmg` 릴리즈 빌드를 만들 때는 결국 macOS 러너가 필요하다 [33].

### 2. 앱 무게 (설치 크기와 메모리)

- **[사실(확인함)] Electron**: 2026-11-17 릴리즈된 Electron `v45.3.4`의 `electron-v45.3.4-darwin-arm64.zip` 원본 자산 크기만 **115MB**다(Windows x64 zip은 150MB) [6]. 압축을 풀고 Node 모듈을 넣은 "Hello World" 앱도 디스크 **150MB 이상**을 차지한다 [14]. 또한 Electron은 Chromium 다중 프로세스 모델을 그대로 쓰므로 메인 프로세스 1개 + 창마다 별도 렌더러 프로세스가 떠서 기본 메모리 점유가 크다 [46].
- **[사실(확인함)] Tauri v2**: 시스템 WebView(macOS는 WKWebView)를 재사용해 공식 문서상 아주 작은 바이너리를 만든다 [12]. 현재 `skills-manage` 앱은 85MB(실행 파일 88.6MB 중 `__LINKEDIT` 47.9MB)로 측정됐는데, `src-tauri/Cargo.toml`을 확인한 결과 Tauri 공식 문서가 크기 절감용으로 권장하는 **`[profile.release]` 설정(`strip = true`, `lto = true`, `codegen-units = 1`, `opt-level = "s"`)과 `removeUnusedCommands`가 현재 전혀 적용되어 있지 않다** [12][13]. (상세 분석은 #51 참조)
- **[사실(확인함) / 추정] Bun `--compile`과 Node SEA**:
  - Bun은 `v1.4.4` 기준 `bun-darwin-aarch64.zip` 자체가 **38MB**이며 [6], `--compile` 문서 자체가 "Bun 바이너리는 아직 너무 크고 더 줄여야 한다"고 명시한다 [47]. 제3자 문서 미러 기준 단일 실행 파일 크기는 플랫폼별로 약 **50~90MB**로 추정된다 [15].
  - Node.js SEA(Single Executable Applications)는 v26 문서 기준 아직 **Stability: 1.1 (Active development)** 단계이며 macOS에서 `codesign` 제거 후 blob 주입 및 재서명이 필요하다 [48]. Node 런타임 자체를 복사하므로 수십 MB 이상(정확한 공식 수치 미기재)이 된다.
  - 반면 로컬 서버를 별도 단일 실행 파일로 묶지 않고 `pnpm`/시스템 Node(이미 개발자 PC에 있음)로 실행하면 앱 패키지 무게 자체가 수 MB 이하로 줄어든다.
- **[사실(확인함)] Wails v3 (Go)**: Wails v3(2026-11-01 `v3.1.0` 안정 릴리즈 [6]) 공식 빌드 시스템 문서는 디버그 심볼을 제거한(`-ldflags="-s -w"`) 프로덕션 단일 바이너리가 macOS/Windows/Linux에서 **약 15MB**(디버그 빌드는 약 50MB)라고 명시한다 [9].

### 3. 보안 위험과 키체인 접근

- **[사실(확인함)] Tauri v2 (①안·⑤안)**: `src-tauri/capabilities/default.json`처럼 창(label) 단위로 허용할 명령·플러그인을 선언하는 Capability 모델(최소 권한 원칙)을 쓴다 [16][49]. 다만 Rust 백엔드 코드 내부에 범위 검증 버그가 있거나 Capability를 너무 넓게 열면 Rust 코드 자체가 막아주지는 못한다 [16]. 현재 앱의 `default.json`은 `core:default`, `dialog:allow-open`, `shell:allow-open`만 열려 있다 [49].
- **[사실(확인함)] Electron (②안)**: Chromium+Node가 함께 있으므로 공식 보안 체크리스트 20가지(원격 콘텐츠에 `nodeIntegration: false`, `contextIsolation: true`, `sandbox: true` 유지, `will-navigate`로 이동 차단, `setWindowOpenHandler` 제한, `shell.openExternal` 검증, 모든 IPC 송신자 출처 검증, `@electron/fuses`로 `runAsNode` 끄기 등)를 직접 지켜야 한다 [18]. Electron 20 이후 샌드박스가 기본 켜짐이지만, `contextIsolation`을 끄거나 `nodeIntegration`을 켜면 샌드박스도 함께 꺼진다 [18].
- **[사실(확인함)] 로컬 HTTP 서버 위험 (③안·④안·⑤안)**: 브라우저가 `127.0.0.1`이나 사설망 서버를 호출할 수 있으므로 두 가지를 반드시 막아야 한다(세부 구조는 #54 참조).
  - **CSRF(사이트 간 요청 위조)**: 다른 악성 웹페이지를 열었을 때 사용자 브라우저가 몰래 로컬 서버의 삭제/설치 API를 부르지 못하게 해야 한다. Go 1.25는 표준 라이브러리 `net/http`에 브라우저 `Sec-Fetch-Site` 헤더를 검사하는 **`http.CrossOriginProtection`** 을 내장했다 [22]. (단 Go 1.25.0의 bypass 패턴 버그 CVE-2025-47910이 있어 **Go 1.25.1 이상**이 필요하며 [50], `Sec-Fetch-Site` 없이 `Origin`만 있는 경우 등 예외에 주의해야 한다.) Node/TS(`hono`/`express`/내장 `http`)나 Rust(`axum`)는 미들웨어로 `Origin`/`Sec-Fetch-Site` 검증 및 인증 토큰을 직접 붙인다.
  - **DNS 리바인딩(DNS Rebinding)**: 외부 도메인이 DNS 응답을 잠깐 `127.0.0.1`로 바꿔 동일 출처 정책을 우회하는 문제다. 방어 동작으로는 **HTTP `Host` 헤더 화이트리스트 검증**(예: `localhost`, `127.0.0.1`, `[::1]`, 허가된 Tailscale 호스트명만 허용하고 나머지는 403 거부)이 필수다 [20]. (실제로 MCP TypeScript SDK도 초기 기본값 미적용으로 CVE-2025-66414 High 경고를 받고 `hostHeaderValidation()`을 기본 탑재했다 [20].)
- **[사실(확인함)] OS 키체인 접근 (ADR-0003 준수 여부)**:
  - **Rust (`keyring` v4)**: 현재 `secrets.rs`에서 사용 중이며 최신 v4.2.0(2026-08-29)이 macOS 네이티브 `apple-native-keyring-store`를 직접 호출한다 [17][51].
  - **Node (`keytar` vs `@napi-rs/keyring`)**: 과거 표준이던 `atom/node-keytar`는 **2022년 12월 15일 공식 보관(archived)** 되어 보안 패치가 없다 [52]. 대안인 **`@napi-rs/keyring`** 은 Rust `keyring-rs`를 N-API 프리빌드 바이너리로 감싼 패키지여서 C++ 재빌드 부담이 적다(단 macOS에서 Touch ID 인증 연동은 제한됨이 보고됨) [53].
  - **Electron (`safeStorage`)**: Electron 내장 API로 macOS Keychain에서 암호화 키를 가져와 문자열을 로컬에 암호화 저장할 수 있다 [19]. 단 **임의의 자격 증명 관리자(Key/Value 직접 읽기·쓰기)가 아니라 로컬 저장 데이터 암호화 용도**이며, 공식 문서상 **macOS에서는 앱에 코드 서명을 해야 업데이트 때마다 키체인 허용 팝업이 반복되지 않는다** [19].
  - **Bun (`Bun.secrets`)**: macOS Keychain Services·Windows 자격 증명 관리자·Linux libsecret을 네이티브로 지원하나, 2026-11-19 기준 공식 문서에 **Experimental(실험적)** 로 표시되어 API 변경 가능성이 있다 [21].
  - **Go (`zalando/go-keyring`)**: v0.2.8(2026-03)이 널리 쓰이나 macOS 구현(`keyring_darwin.go`)은 Apple C API 직접 호출이 아니라 **`/usr/bin/security` 바이너리 실행에 의존**한다 [23]. (대규모 반복 호출이나 앱 샌드박스 환경에서는 주의가 필요하다.)

### 4. 휴대폰 관리 적합성

- **[사실(확인함)] 아키텍처 적합성**:
  - **③ TS 로컬 서버 + PWA**, **④ Go 서버(또는 Wails+HTTP)**, **⑤ Rust 핵심 + HTTP 통로**는 모두 HTTP JSON API(및 WebSocket/SSE)와 정적 웹 화면을 직접 내어주므로 데스크톱과 휴대폰 브라우저가 같은 서버를 바라보는 구조가 매우 자연스럽다.
  - 반면 **① Tauri 단독**, **② Electron 단독**은 창 내부 IPC 전용이라 휴대폰에서 접속하려면 어차피 안에 HTTP 서버를 하나 더 띄워야 한다.
- **[사실(확인함)] iOS Safari / 홈 화면 PWA 조건**:
  - **HTTPS 필수**: 서비스 워커(Service Worker)와 많은 최신 웹 API는 보안 컨텍스트(HTTPS)가 필요하다. `localhost`는 예외지만 휴대폰에서 맥으로 접속하는 사설 IP나 호스트명에서는 일반 HTTP로 PWA 설치/서비스 워커가 온전히 동작하지 않는다. Tailscale의 **`tailscale serve`** 를 쓰면 Tailnet 내부 전용으로 `https://<기기명>.<tailnet>.ts.net` 주소와 Let's Encrypt TLS 인증서를 자동 중계할 수 있다 [26][27]. (단 Tailscale 관리자 콘솔에서 MagicDNS 및 HTTPS Certificates 활성화가 필요하며, 기기 이름이 공개 인증서 투명성 원장(CT log)에 기록되는 점은 주의해야 한다 [27].)
  - **iOS PWA 제약**: Web Push 등 일부 기능은 Safari에서 바로 쓸 수 없고 "홈 화면에 추가"(`manifest.json`의 `display: "standalone"`)한 웹 앱에서만 동작한다(iOS 16.4+ Web Push, iOS 18.4+ Declarative Web Push) [54]. 또한 Apple WebKit ITP(지능형 추적 방지) 정책에 따라 **일정 기간 방문하지 않은 웹 앱의 서비스 워커 등록과 사이트 저장소가 삭제될 수 있다** [54].
  - 세부 인증 방식, 포트 바인딩, Paseo 연동 구조는 **#54 참조**.

### 5. 초보자 1인 유지보수와 AI 에이전트 협업

- **[사실(확인함)] 저장소 언어 수**:
  - 현재: **3개 언어**(TypeScript 31,619줄 + Rust 52,233줄 + Swift 349줄).
  - ①·⑤안: **3개 언어 유지**. AI 코딩 에이전트가 한 기능을 고칠 때 TS 화면 ↔ Rust Tauri 명령 ↔ SQLite 스키마까지 두세 언어를 가로질러 수정해야 하고, Rust 컴파일 오류(소유권·라이프타임·async Send 경계)를 초보 개발자가 직접 읽고 판단하기 어렵다.
  - ④안(Go): **2~3개 언어**(TS + Go + 선택적 Swift). Go는 문법이 단순하고 컴파일이 빨라 AI 수정-확인 루프가 짧다.
  - ②·③안(TypeScript): **1개 언어**(TS 풀스택, 단 macOS 온디바이스 번역 유지 시 Swift 349줄을 작은 별도 CLI 실행 파일로 떼어내 호출하거나 AI 번역만 남겨야 함). 프런트엔드와 백엔드가 타입을 그대로 공유할 수 있어 초보자와 AI 모두 문맥 전환 비용이 가장 적다.
- **[경험담] 언어·DX 피로도**: Reddit `r/tauri`·`r/webdev` 등 여러 1인 개발자가 "AI를 써도 프로젝트가 커질수록 Rust borrow checker가 개발 속도를 깎는다", "1인 개발에서 절반은 Rust, 절반은 TS로 짜는 것이 피곤하다"고 보고했다 [29][55]. 반대로 "컴파일러가 타입/메모리 실수를 엄격히 잡아주어 대규모 로직 안전성이 높다"는 긍정 평가도 공존한다.
- **코드 이전 비용**: 기존 Rust 명령 123개와 비즈니스 로직을 얼마나 옮겨야 하는지는 **#52 참조**.

### 6. 생태계 (SQLite · YAML · GitHub · 파일 감시 · 배포/서명)

| 세부 요소 | Rust (①·⑤) | TypeScript / Node · Bun (②·③) | Go (④) |
|---|---|---|---|
| **SQLite** | `sqlx` 0.8 (현재 사용 중, 비동기/마이그레이션 안정) [13] | **`node:sqlite`**: 2026-11-19 기준 Node v26.12.1 문서에서 **`Stability: 1.2 - Release candidate`**(`DatabaseSync`는 deprecated 별칭이며 `Database`가 정식명) [5]. 현재 프로젝트 기준인 **Node v22.13**에서는 `--experimental-sqlite` 플래그만 빠진 **Experimental** 단계였다 [5]. 성능 벤치마크(Node v25.3.0)에서는 `better-sqlite3`가 `node:sqlite`보다 대부분 1.11~1.67배 빠르다 [56].<br>**`better-sqlite3`**: 최고 성능이나 C++ 네이티브 애드온이라 Electron ABI 재빌드 이슈(`NODE_MODULE_VERSION` 불일치) 잦음 [33][34].<br>**`bun:sqlite`**: Bun 내장 동기 API. | **`mattn/go-sqlite3`**: 빠르지만 **CGo(C 컴파일러) 필수**라 크로스 컴파일·CI가 불편.<br>**`modernc.org/sqlite`**: Pure Go라 크로스 컴파일이 쉬우나 인덱스 SELECT/대량 쓰기 벤치마크에서 CGo 대비 느림 [38].<br>**`ncruces/go-sqlite3`**: WASM 기반, WAL 지원 [38]. |
| **YAML 머리말** | 현재 `Cargo.toml`의 **`serde_yaml` 0.9는 2024-03-25 아카이브(유지보수 중단)** 상태다 [13][31]. 유지 시 후속 포크(`serde_yml` 등)로 교체 필요. | **`gray-matter`**(이미 `package.json`에 있음) [57] 또는 `yaml` 패키지. `gray-matter`는 2023년 이후 정체 상태이나 단순 frontmatter 파싱·문자열화에 널리 쓰임 [58]. | 기존 `gopkg.in/yaml.v3`는 2025-04-01 아카이브되었으나, YAML 팀이 공식 후속 모듈 **`go.yaml.in/yaml/v3`**(및 v4)로 인계받아 유지보수 중 [39][59]. |
| **GitHub API** | `reqwest` 직접 호출(현재 구현됨) 또는 `octocrab` | **`octokit`** 공식 SDK(TypeScript 타입 완벽 지원, Node 18+ 내장 `fetch` 사용) [60] | **`google/go-github`** (2026-11 기준 v93까지 활발히 유지보수됨) [37] |
| **파일 감시** | `notify`(FSEvents / ReadDirectoryChangesW / inotify, 현재 프로젝트는 수동 스캔) [32] | **`chokidar` v6**(2026-10 릴리즈, Node 22.22+에서 macOS/Windows 네이티브 `fs.watch(recursive)` 지원) [36] | **`fsnotify`**(Go 1.23+, macOS/Windows/Linux 지원) [61] |
| **패키징 · 서명 · 자동 업데이트** | Tauri `tauri-plugin-updater`는 **Apple 서명/공증(notarization)과 별개로 전용 minisign 서명 키**가 필수다 [62]. 현재 저장소는 `--no-sign` 빌드이며 업데이터 미구현 [63]. | Electron(`electron-builder`/`electron-updater`): **macOS 자동 업데이트와 `safeStorage` 정상 동작을 위해 Apple Developer ID 코드 서명·공증이 사실상 필수**다 [19][64].<br>TS 로컬 서버(③안): 데스크톱 앱 번들 대신 CLI/LaunchAgent/Homebrew로 배포 가능해 Gatekeeper `.app` 공증 부담을 피해 갈 수 있음(**추정**). | Wails v3: `wails3 build` 단일 빌드 + v3부터 내장 업데이터 지원 [9][65]. macOS `.app` 배포 시 Apple 서명/공증 요구는 동일. |

### 7. 실제 사용자 경험담 (GitHub · Reddit · Hacker News · 개발자 회고)

아래 내용은 공개 포스트·이슈·스레드에서 수집한 **[경험담]** 이다(일부는 각 팀의 맥락에 따른 판단이며 우리 앱의 사실과 구분해야 한다).

1. **OpenCode Desktop — Tauri에서 Electron으로 이전 (2026-04-19 회고)** **[경험담]**:
   - 원래 `opencode serve`(TypeScript/Bun 서버) + 웹 UI 구조에 가벼운 껍데기로 Tauri를 씌워 CLI를 사이드카로 띄웠다 [66].
   - 그러나 ① macOS/Linux의 WebKit 렌더링 스타일 불일치·성능 저하, ② 별도 CLI 프로세스 실행의 시작 지연 및 Windows 간헐 실패 때문에 결국 Electron으로 옮겼다 [66].
   - 저자가 남긴 핵심 교훈: *"핵심 로직이 이미 TypeScript라면 Tauri는 웹뷰 껍데기일 뿐 성능을 높여주지 못한다. 반대로 핵심 로직이 Rust라면 Tauri가 잘 맞는다."* [66]
2. **GitButler — Rust 핵심을 HTTP 서버(`but-server`)로 분리** **[사실(확인함) + 경험담]**:
   - GitButler 저장소 구조(`crates/but-server/Cargo.toml`)를 확인한 결과, 기존 Tauri 데스크톱 앱(`gitbutler-tauri`) 외에 **`but-api`와 `axum` 0.8 + `tower-http` 기반의 `but-server` crate**를 별도로 두어 웹 클라이언트(`apps/web`)와 로직을 공유하는 **5안 구조**를 실제 사용 중이다 [3][28].
   - Hacker News 스레드(`#46082291`) 댓글에서는 GitButler가 Linux 패키징 문제와 WebView 편차 때문에 Tauri를 떠나 Rust 백엔드 웹 서버 + Electron 조합으로 옮기려 한다는 경험담도 공유되었다 [67].
3. **DoltHub — Electron에서 Tauri 검토 후 보류 (2025-11-13 회고)** **[경험담]**:
   - 같은 앱을 Tauri로 복제해 보니 크기가 크게 줄고 JS API 연결도 만족스러웠다 [14]. 그러나 Windows Store용 `.appx`/`.msix` 미지원과 macOS universal 바이너리 이중 코드 서명 문제로 당장 이전을 보류했다 [14]. (참고로 `skills-manage`는 Apple Silicon 단독 배포라 macOS universal 문제가 없다.)
4. **WaveTerm — Wails 심층 테스트 후 Electron 유지 (GitHub Discussion #1523)** **[경험담]**:
   - Rust를 배우지 않으려고 Go 기반 Wails를 깊이 테스트했으나, 다수 IPC 호출 시 응답이 500ms 단위로 뭉쳐 돌아오는 등 IPC 지연과 렌더링 프레임 드랍이 심해 Electron에 남았다 [68].
   - 반대로 Wails v3 발표 스레드(`r/golang`) 등에서는 단순한 CRUD/유틸리티 앱에서 "Go 문법이 쉽고 크로스 컴파일이 빠르며 10~15MB대 작은 바이너리가 매력적"이라는 호평도 많다 [65][69].
5. **Tauri vs Electron 및 Rust CI 경험 (Reddit / HN)** **[경험담]**:
   - Linux WebKitGTK 렌더링 느림·AppImage 라이브러리 호환 문제가 Tauri 최대 불만으로 자주 꼽힌다 [67][70]. 단 `skills-manage`의 주 배포 대상은 Apple 칩 macOS이므로 Linux WebKitGTK 위험은 상대적으로 작다.
   - Electron은 크롬 엔진을 포함해 플랫폼 간 화면 일관성이 높고 생태계 문서가 풍부하지만, 설치 크기와 메모리 부담이 크다 [67][71].
   - Rust CI는 `Swatinem/rust-cache@v2`나 `sccache`로 완화할 수 있지만 GitHub Actions 10GB 캐시 한도와 가상 머신 단일 코어 성능 때문에 여전히 다른 언어보다 느리다는 호소가 많다 [44][72].

### 8. [확장 고려] 다중 기기·헤드리스 서버 확장성

사용자가 Mac(거의 상시 가동) 외에 항상 켜진 원격 클라우드 서버와 향후 추가될 보조 PC(미니 PC)를 함께 쓰게 될 가능성이 있다. 다만 **여러 기기를 실제로 통합 관리할지는 아직 결정되지 않았으므로**, 이 항목은 핵심 평가 기준이 아닌 **확장 고려 사항**으로만 정리한다(세부 접속·보안 설계는 #54 참조).

- **[사실(확인함)] 화면 없는 Linux 서버(헤드리스) 및 다중 OS 실행**:
  - **Tauri(①)·Electron(②) 데스크톱 앱 자체**는 GUI 라이브러리(Linux 기준 GTK3/WebKitGTK [63] 또는 Chromium [46])를 전제로 해 화면 없는 서버에서 그대로 돌릴 수 없다. 서버에서 핵심 로직을 쓰려면 UI 없는 CLI/서버 바이너리를 따로 분리해야 한다.
  - **TS 로컬 서버(③)**, **Go HTTP 서버(④의 서버형)**, **Rust HTTP 서버 분리(⑤)** 는 모두 GUI 없이 실행되는 서버 프로세스이므로 화면 없는 Linux 서버나 다른 운영체제(보조 PC 등)에서도 핵심 스킬 스캔·설치 로직을 돌릴 수 있다.
  - 단, 헤드리스 Linux에서는 OS 키체인 데몬이 없는 경우가 많아 ADR-0003 설계상 비밀값이 디스크에 남지 않고 프로세스 메모리에만 유지되므로(재시작 시 재입력 필요) [24], 원격 서버에서의 비밀값 입력·유지 방식은 별도로 고려해야 한다. 또한 macOS `Translation.framework`(Swift 349줄) [43]는 macOS 전용이므로 Linux·Windows 기기에서는 AI API 번역만 동작한다.
- **[추정] 여러 기기의 스킬을 한 화면에서 관리**:
  - HTTP API를 기본 통신 통로로 쓰는 **③ TS 서버+PWA**, **④ Go 서버**, **⑤ Rust+HTTP** 구조는 각 기기(Mac, 원격 서버, 보조 PC)에 같은 서버 에이전트를 띄워 두고 하나의 웹 화면에서 기기별 API를 조회·조작하는 구조로 넓히기가 자연스럽다.
  - 그중에서도 **③ TS 서버**는 런타임만 있으면 별도 크로스 컴파일 없이 여러 OS에서 같은 코드를 실행할 수 있고, **④ Go 서버**는 OS별 단일 바이너리 크로스 컴파일이 빠르며 [30], **⑤ Rust+HTTP**는 OS별 바이너리 빌드 비용은 들지만 기존 52,233줄 검증된 로직을 그대로 각 기기에 배포할 수 있다 [3].

---

## 대안별 "이런 경우에 고른다"

1. **① Rust/Tauri 유지 + 개선**
   - **이런 경우에 고른다**: 휴대폰 관리가 당장 필수는 아니고, 이미 완성된 Rust 52,233줄·보안 경계(#18)·OS 키체인·Swift 온디바이스 번역을 버리지 않은 채 **앱 크기(Release 프로필 최적화)와 CI 부담(경로 필터·캐시 점검)만 가장 적은 공수로 줄이고 싶을 때**.
2. **⑤ Rust 핵심 유지 + 로컬 HTTP 통로 추가**
   - **이런 경우에 고른다**: **휴대폰에서도 스킬을 관리(추가·제거·토글)하고 싶지만**, 52,233줄과 123개 명령을 다른 언어로 다시 짤 시간·위험을 감당하기 싫을 때. Tauri 앱을 유지하면서 Rust 코어를 `axum` 로컬 HTTP API로도 열어 점진적으로 웹/PWA를 키우는 현실적 절충안이다.
3. **③ TypeScript(Node 또는 Bun) 로컬 서버 + 웹 화면(PWA)**
   - **이런 경우에 고른다**: **초보자 1인 + AI 코딩 에이전트로 장기 유지보수**하는 것을 최우선으로 두고, **데스크톱·웹·휴대폰(Tailscale + PWA)을 단일 언어(TS)로 통합**해 빠른 개발·CI 속도(Linux 1~2분)를 얻고 싶을 때. 단 #52에서 확인되는 초기 재작성 비용과 Swift 번역 분리(또는 AI 번역 대체)를 감수할 수 있어야 한다.
4. **② Electron (TypeScript)**
   - **이런 경우에 고른다**: 85MB보다 커져도(150MB+) 상관없고 휴대폰 관리도 주 목적이 아니며, 오직 **TS 단일 언어 + macOS/Windows/Linux 어디서나 똑같은 Chromium 화면 + 검증된 데스크톱 앱 배포**가 가장 중요할 때. (현재 목표인 "가벼움 + 휴대폰 관리"와는 방향이 잘 맞지 않는다.)
5. **④ Go (Wails v3 또는 Go 서버 + 웹)**
   - **이런 경우에 고른다**: 개발자가 Go 언어를 이미 잘 알고 선호하거나, 15MB 이하 단일 바이너리 데스크톱 앱과 Go 내장 HTTP 서버·빠른 크로스 컴파일을 동시에 쓰고 싶을 때. (현재 TS+React 31,619줄에 1인 초보자라면 Go까지 새로 익혀 52,000줄을 옮길 실익이 크지 않다.)

---

## 출처 목록

1. `[저장소 소스]` `.github/workflows/backend.yml` (`src-tauri/**` 경로 필터 및 `macos-15-intel` CI 설정)
2. `[저장소 소스]` `src-tauri/Cargo.toml` (`[profile.release]` 미설정 상태, `serde_yaml = "0.9"`, `keyring = "4"`, `swift-rs` 등 확인)
3. `[1차 소스]` GitButler `crates/but-server/Cargo.toml` (`axum` 0.8.9 + `tower-http` + `but-api` 분리 구조): https://github.com/gitbutlerapp/gitbutler/blob/master/crates/but-server/Cargo.toml
4. `[저장소 소스]` `.github/workflows/ci.yml` (`ubuntu-latest` Frontend CI 단계)
5. `[공식 문서]` Node.js v26.12.1 Documentation — SQLite (`Stability: 1.2 - Release candidate`, v22.13 unflagged, v25.7 RC, `DatabaseSync` DEP0210): https://nodejs.org/api/sqlite.html
6. `[1차 릴리즈 API]` GitHub Releases API 확인(2026-11-19 조회) — Electron `v45.3.4` (`electron-v45.3.4-darwin-arm64.zip` 115MB), Bun `bun-v1.4.4` (`bun-darwin-aarch64.zip` 38MB), Wails `v3.1.0`: https://github.com/electron/electron/releases / https://github.com/oven-sh/bun/releases / https://github.com/wailsapp/wails/releases
7. `[공식 문서]` GitHub Docs — About billing for GitHub Actions (Linux `$0.006/min` vs macOS `$0.062/min`, 공개 저장소 무료): https://docs.github.com/en/billing/managing-billing-for-github-actions/about-billing-for-github-actions
8. `[공식 문서]` GitHub Docs — Standard GitHub-hosted runners (공개 저장소 Ubuntu 4코어/16GB vs macOS M1 3코어/7GB, Intel 4코어/14GB): https://docs.github.com/en/actions/reference/runners/github-hosted-runners
9. `[공식 문서]` Wails v3 Build System (~15MB release, ~50MB debug, `-ldflags="-s -w"`, `embed.FS`): https://v3.wails.io/concepts/build-system
10. `[공식 문서]` Wails GitHub README (`v2` 및 `v3.0.0` Stable 상태): https://github.com/wailsapp/wails
11. `[공식 문서]` docs.rs — `axum` 0.8.9 (`tokio` 및 `tower-http` 생태계): https://docs.rs/crate/axum/latest
12. `[공식 문서]` Tauri v2 Docs — App Size (`Cargo.toml` release 프로필 및 `removeUnusedCommands`): https://v2.tauri.app/concept/size/
13. `[저장소 소스]` `src-tauri/src/lib.rs` (123개 `#[tauri::command]` 등록), `package.json`, `vite.config.ts`, `web-dashboard/snapshot.ts`
14. `[1차 개발자 회고]` DoltHub Blog — *Electron vs. Tauri* (2025-11-13, Eric Richardson): https://www.dolthub.com/blog/2025-11-13-electron-vs-tauri/
15. `[2차 문서 미러]` Bun Single-file executable 크기 언급(50~90MB 추정, 공식 확인 불가로 추정 표시): https://www.mintlify.com/zhcndoc/bun/bundler/executables
16. `[공식 문서]` Tauri v2 Docs — Security & Capabilities: https://v2.tauri.app/security/ 및 https://v2.tauri.app/security/capabilities/
17. `[저장소 소스]` `src-tauri/src/secrets.rs` (OS 키체인 저장소 `keyring` v4 및 메모리 폴백, ADR-0003)
18. `[공식 문서]` Electron Docs — Security Checklist (`contextIsolation`, `sandbox`, `nodeIntegration`, IPC 검증 등): https://electronjs.org/docs/latest/tutorial/security
19. `[공식 문서]` Electron Docs — `safeStorage` API (macOS 코드 서명 권장, 로컬 암호화 용도): https://www.electronjs.org/docs/latest/api/safe-storage
20. `[1차 보안 권고]` MCP TypeScript SDK Security Advisory GHSA-w48q-cv73-mx4w (CVE-2025-66414, localhost DNS rebinding 및 `Host` 헤더 검증): https://github.com/modelcontextprotocol/typescript-sdk/security/advisories/GHSA-w48q-cv73-mx4w
21. `[공식 문서]` Bun Docs — Secrets (`Bun.secrets`, Experimental, macOS Keychain): https://bun.com/docs/runtime/secrets
22. `[공식 문서]` Go 1.25 Release Notes (`net/http` `CrossOriginProtection`): https://go.dev/doc/go1.25 및 Go 취약점 보고 GO-2025-3955 (CVE-2025-47910, 1.25.1 수정): https://pkg.go.dev/vuln/GO-2025-3955
23. `[1차 소스]` `zalando/go-keyring` GitHub (macOS `/usr/bin/security` 의존): https://github.com/zalando/go-keyring
24. `[저장소 문서]` `docs/adr/0003-secrets-in-os-keychain.md`
25. `[저장소 소스]` `vite.config.ts` (`mode === "web"`, `127.0.0.1:24200`, `GET /api/dashboard`) 및 `web-dashboard/snapshot.ts`
26. `[공식 문서]` Tailscale KB — Tailscale Serve (`tailscale serve` 자동 HTTPS 프록시): https://tailscale.com/kb/1312/serve
27. `[공식 문서]` Tailscale KB — Enabling HTTPS (MagicDNS, CT 로그 공개 주의, 인증서 갱신): https://tailscale.com/kb/1153/enabling-https/
28. `[1차 소스]` GitButler `apps/desktop/package.json` (`@tauri-apps/api` 사용 확인): https://github.com/gitbutlerapp/gitbutler/blob/master/apps/desktop/package.json
29. `[경험담]` Reddit `r/tauri` — *How do you feel about Rust in the Tauri context?* (2026-01-18): https://www.reddit.com/r/tauri/comments/1qg6khc/how_do_you_feel_about_rust_in_the_tauri_context/
30. `[경험담]` Reddit `r/webdev` — *Angular + Wails (Golang)* (2026-01-11, Go 크로스 컴파일/단순성): https://www.reddit.com/r/webdev/comments/1q9zo8q/angular_wails_golang_crossplatform_desktop_app/
31. `[1차 소스]` `dtolnay/serde-yaml` GitHub (2024-03-25 아카이브 확인): https://github.com/dtolnay/serde-yaml
32. `[1차 소스]` `notify-rs/notify` GitHub (크로스 플랫폼 파일 감시 백엔드): https://github.com/notify-rs/notify
33. `[공식 문서]` Electron Docs — Using Native Node Modules (`@electron/rebuild`, `NODE_MODULE_VERSION`): https://www.electronjs.org/docs/latest/tutorial/using-native-node-modules
34. `[1차 이슈/릴리즈]` `WiseLibs/better-sqlite3` issues (Electron ABI 재빌드 관련): https://www.github.com/WiseLibs/better-sqlite3/issues
35. `[1차 이슈]` `WiseLibs/better-sqlite3` #1234 — *future of better-sqlite3 vs node:sqlite*: https://github.com/WiseLibs/better-sqlite3/issues/1234
36. `[1차 소스]` `paulmillr/chokidar` GitHub (v6, 2026-10, `fs.watch(recursive)` 지원): https://github.com/paulmillr/chokidar
37. `[1차 소스]` `google/go-github` GitHub (v93.0.0): https://github.com/google/go-github
38. `[벤치마크/문서]` `modernc.org/sqlite` vs `mattn/go-sqlite3` 벤치마크 요약 및 `ncruces/go-sqlite3` 설명: https://socket.dev/go/package/modernc.org/sqlite/benchmark / https://github.com/ncruces/go-sqlite3
39. `[1차 소스]` `go-yaml/yaml` GitHub (2025-04-01 아카이브) 및 YAML 팀 공식 후속 `go.yaml.in/yaml/v3`: https://github.com/go-yaml/yaml / https://pkg.go.dev/go.yaml.in/yaml/v3
40. `[1차 소스]` `blackboardsh/electrobun` GitHub (`v2.0.6`, 13k+ stars, 유지보수 안내): https://github.com/blackboardsh/electrobun
41. `[공식 문서]` Neutralinojs Docs — Distribution Overview: https://neutralino.js.org/docs/distribution/overview/
42. `[공식 문서]` Neutralinojs Docs — Security (WebSocket + OTT 모델): https://neutralino.js.org/docs/contributing/security/
43. `[저장소 소스]` `src-tauri/native/SkillsManageTranslation/Package.swift` 및 `Sources/` (macOS `Translation.framework` Swift 브리지 349줄)
44. `[기술 가이드]` Depot Blog / RunsOn — Rust CI 빌드 및 GitHub Actions 10GB 캐시 한도: https://depot.dev/blog/guide-to-faster-rust-builds-in-ci / https://runs-on.com/docs/guides/languages/rust/
45. `[블로그 벤치마크]` Go 1.24 vs Rust 1.85 컴파일 속도 비교 (비독립 측정): https://dev.to/johalputt/rust-185-vs-go-124-compilation-speed-and-memory-usage-benchmarks-for-2026-cli-tool-development-4hoo
46. `[공식 문서]` Electron Docs — Process Model: https://www.electronjs.org/docs/latest/tutorial/process-model
47. `[공식 문서]` Bun GitHub Docs — Single-file executable (`compiler/executables.mdx`): https://raw.githubusercontent.com/oven-sh/bun/main/docs/compiler/executables.mdx
48. `[공식 문서]` Node.js Docs — Single executable applications (Stability 1.1, macOS `codesign` 필요): https://nodejs.org/api/single-executable-applications.html
49. `[저장소 소스]` `src-tauri/capabilities/default.json`
50. `[참고 해설]` Alex Edwards — *A modern approach to preventing CSRF in Go* (`CrossOriginProtection` 예외 주의): https://alexedwards.net/blog/preventing-csrf-in-go
51. `[공식 문서]` docs.rs — `keyring` v4.2.0 (2026-08-29): https://docs.rs/crate/keyring/latest
52. `[1차 소스]` `atom/node-keytar` GitHub (2022-12-15 아카이브 확인): https://github.com/atom/node-keytar
53. `[패키지/PR]` `@napi-rs/keyring` 및 Apify PR #7 / JSR `@optique/keyring` (Touch ID 미지원 메모): https://github.com/apify/mcpc/pull/7 / https://jsr.io/@optique/keyring
54. `[공식 블로그]` WebKit Blog — *Meet Declarative Web Push* (iOS/iPadOS Home Screen web apps, ITP에 의한 Service Worker 삭제 설명): https://webkit.org/?p=16535
55. `[경험담]` Reddit `r/webdev` — *Electronjs or Tauri or .NET MAUI* (2026-03-06, 1인 개발자 Rust+TS 피로도): https://www.reddit.com/r/webdev/comments/1rmkt6l/electronjs_or_tauri_or_net_maui_what_would_you/
56. `[벤치마크]` SQLite Driver Benchmark on Node.js v25.3.0 (`better-sqlite3` vs `node:sqlite`): https://sqg.dev/blog/sqlite-driver-benchmark
57. `[저장소 소스]` `package.json` (`gray-matter`, `engines.node >=22.13.0`)
58. `[1차 소스]` `jonschlinkert/gray-matter` GitHub: https://github.com/jonschlinkert/gray-matter
59. `[1차 소스]` `pkg.go.dev/go.yaml.in/yaml/v3` (YAML 팀 공식 유지보수 전환 안내): https://pkg.go.dev/go.yaml.in/yaml/v3
60. `[1차 소스]` `octokit/octokit.js` GitHub: https://github.com/octokit/octokit.js
61. `[1차 소스]` `fsnotify/fsnotify` GitHub: https://github.com/fsnotify/fsnotify
62. `[공식 문서]` Tauri v2 Docs — Updater Plugin (Signing 및 Rust 1.90+ 요구): https://v2.tauri.app/plugin/updater/
63. `[저장소 소스]` `.github/workflows/release-desktop.yml` (`--no-sign` 빌드 확인)
64. `[공식 문서]` Electron Docs — Code Signing (macOS 서명/공증, `safeStorage`/`autoUpdater` 연동): https://www.electronjs.org/docs/latest/tutorial/code-signing
65. `[1차 발표/경험담]` Reddit `r/golang` — *Wails v3.0.0 released* (2026-10-26, 업데이터·모바일 실험 지원 등): https://www.reddit.com/r/golang/comments/1xfva18/wails_v300_released_go_applications_for_desktop/
66. `[1차 개발자 회고]` Brendonovich — *Moving OpenCode Desktop to Electron* (2026-04-19): https://dev.to/brendonovich/moving-opencode-desktop-to-electron-4hip
67. `[경험담]` Hacker News #46082291 (Tauri Linux 패키징 문제, GitButler 사례 언급 등): https://news.ycombinator.com/item?id=46082291
68. `[1차 논의]` WaveTerm Discussion #1523 — *Wails instead of Electron* (2024-12): https://github.com/wavetermdev/waveterm/discussions/1523
69. `[경험담]` Hacker News #46030414 (Go 데스크톱 앱, Wails vs Electron vs Tauri): https://news.ycombinator.com/item?id=46030414
70. `[경험담]` Reddit `r/tauri` — *How is Tauri for building production desktop apps* (2026-03-29): https://www.reddit.com/r/tauri/comments/1s6xqem/how_is_tauri_for_building_production_desktop_apps/
71. `[경험담]` Reddit `r/electronjs` — *I ran the same React UI on Electron, Tauri, and Deno Desktop* (2026-09-06): https://www.reddit.com/r/electronjs/comments/1w95jn0/i_ran_the_same_react_ui_on_electron_tauri_and/
72. `[경험담]` Reddit `r/rust` / `r/programming` — Rust CI 캐싱 (`rust-cache`, `sccache`) 경험: https://www.reddit.com/r/rust/comments/1j56sag/rust_builds_with_sccache_and_github_actions/ / https://www.reddit.com/r/programming/comments/18bgjg9/incremental_rust_builds_in_ci/
