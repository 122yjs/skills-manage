# 앱은 실제로 얼마나 무겁나: 85MB의 정체와 메모리·시작 시간

> 조사 대상: `/Applications/skills-manage.app` (v0.12.3, 2026-09-29 생성 빌드) 및 저장소 `main` (`f1c1ba1`, 2026-11-19 기준)
>
> 관련 이슈: [#51](https://github.com/122yjs/skills-manage/issues/51), 지도 이슈 [#49](https://github.com/122yjs/skills-manage/issues/49), 연관 현상 [#46](https://github.com/122yjs/skills-manage/issues/46)

---

## 한눈에 보기 (쉬운 설명)

- **85MB의 정체는 "안 지운 이름표(심볼)와 연결 정보"다 [사실]**: 설치된 실행 파일(88.64MB)의 **절반 이상(54.1%, 47.91MB)**은 실제 실행 코드가 아니라 함수·변수 이름표와 연결 색인(`__LINKEDIT`)이다. `src-tauri/Cargo.toml`에 릴리스 최적화 설정(`[profile.release]`)이 전혀 없어, 배포 빌드에서도 이 이름표가 그대로 남아 있었다.
- **릴리스 설정만 제대로 켜도 설치 용량이 절반 이하로 줄어들 여지가 크다 [사실+추정]**: 실행 파일 사본에서 이름표만 지우는 명령(`strip`)을 돌려도 **88.64MB → 52.70MB(-40.5%)**로 즉시 줄어든다 [사실]. 여기에 Tauri 공식 권장 설정(`lto`, `codegen-units = 1`, `opt-level = "s"`, `panic = "abort"`)을 더하면 약 **23~37MB** 수준까지 줄어들 것으로 보인다 [추정].
- **내장된 React 화면 파일은 1%도 안 된다 [사실]**: 현재 `main` 기준으로 화면 파일을 빌드한 `dist/`는 약 1.92MB(1.83 MiB)이고, Tauri가 이를 Brotli(브로틀리, 높은 압축률의 압축 방식)로 압축해 넣으면 약 **0.59MB(573 KiB)**에 불과하다. 85MB가 큰 이유는 화면 파일 때문이 아니다.
- **실행 메모리(131~142MB)는 Tauri 앱으로서 보통 수준이며, 대부분 웹 화면 엔진 몫이다 [사실]**: 빈 가짜 홈 폴더로 3번 실행해 잰 결과, 실제 물리 메모리(`footprint` 기준)는 **131~142MB**였다. 이 중 Rust 백엔드 본체는 **23~25MB(약 18%)**뿐이고, 나머지 **108~117MB(약 82%)**는 macOS 기본 웹 화면 엔진(WebKit)이 쓴다. Electron(크롬 브라우저를 통째로 묶는 방식) 빈 앱의 macOS 메모리(약 370MB)보다는 적지만, 웹 화면을 띄우는 구조상 100MB 이상은 자연스럽게 든다.
- **"무겁다"는 체감의 진짜 원인은 용량보다 시작 직후의 디스크 스캔과 긴 CI 대기일 가능성이 높다 [사실+추정]**: 빈 폴더 기준 창이 뜨는 속도는 **0.27~1.10초**로 빠르다 [사실]. 하지만 코드상 앱을 켜자마자 플랫폼 스캔 뒤 프로젝트 폴더 전체 스캔(최대 10개 폴더, 깊이 8단계)을 자동으로 돌리고 진행률도 루트 하나가 끝날 때만 알린다 [사실]. 사용자가 느끼는 답답함("멈춘 것 같다")은 실행 속도 자체가 아니라 이 스캔 UX(#46)와 백엔드 CI 대기(15~19.5분)에서 왔을 가능성이 크다 [추정].

---

## 1. 설치 용량과 실행 파일 분해 (85MB의 정체)

### 1.1 앱 묶음 구조 [사실(확인함)]

`/Applications/skills-manage.app`을 확인한 결과, 앱 묶음 전체 크기(`du -sh`)는 **85M**이며 사실상 거의 전부가 Apple 칩(`arm64`)용 단일 실행 파일이다.

- 실행 명령 및 확인 근거:
  - `du -sh /Applications/skills-manage.app` → `85M`
  - `du -sh /Applications/skills-manage.app/Contents/*` → `MacOS`: `85M`, `Resources`: `132K`, `Info.plist`: `4.0K`, `_CodeSignature`: `4.0K`
  - `stat -f "%z %N" /Applications/skills-manage.app/Contents/MacOS/skills-manage` → **`88,644,608` 바이트** (84.54 MiB)
  - `file ...` → `Mach-O 64-bit executable arm64`
  - `codesign -dvv /Applications/skills-manage.app` → `Signature=adhoc` (개발자 인증서 서명 없이 `--no-sign`으로 빌드된 기본 임시 서명; `.github/workflows/release-desktop.yml:118`과 일치)
  - 참고(다운로드 파일과 차이): GitHub `v0.12.3` 릴리스(`gh release view v0.12.3`)에서 macOS용 압축본 `skills-manage_0.12.3_macos_arm64.dmg`는 **`10,746,430` B**(약 10.25 MiB), `.zip`은 **`10,649,997` B**(약 10.16 MiB)다. 내려받을 때는 ~10MB인데 설치하면 85MB로 커지는 이유는, 아래에서 보듯 실행 파일 절반을 차지하는 심볼 이름 문자열이 압축에서 약 88% 가까이 줄어들기 때문이다.

### 1.2 실행 파일 내부 영역(`otool -l`, `size -m`, `nm`) [사실(확인함)]

`otool -l`과 `size -m`으로 실행 파일 `88,644,608` 바이트의 구성을 1바이트 오차 없이 분해하면 다음과 같다.

| 세그먼트(영역) | 파일 크기 (바이트) | 비중 | 주요 세부 항목과 설명 |
|---|---:|---:|---|
| **`__TEXT`** | `39,616,512` (37.78 MiB) | 44.69% | 실제 프로그램 코드와 읽기 전용 데이터 |
| ↳ `__text` | `28,060,664` (26.76 MiB) | 31.66% | CPU가 실행하는 기계어 코드 본체 |
| ↳ `__eh_frame` | `6,196,768` (5.91 MiB) | 6.99% | 예외/패닉 시 호출 스택을 거슬러 오르는(unwinding) 표 |
| ↳ `__const` | `2,493,104` (2.38 MiB) | 2.81% | 고정 상수 데이터, 내장 화면 파일, 압축 사전 등 |
| ↳ `__gcc_except_tab` | `1,359,944` (1.30 MiB) | 1.53% | 예외 처리 분기 정보 테이블 |
| ↳ `__unwind_info` | `1,224,176` (1.17 MiB) | 1.38% | macOS 컴팩트 스택 풀기 색인 |
| ↳ `__cstring` 등 나머지 | `~281,856` (0.27 MiB) | 0.32% | 문자열 리터럴(`247,608` B), 스텁, Swift 메타데이터 등 |
| **`__DATA_CONST`** | `1,064,960` (1.02 MiB) | 1.20% | 실행 중 읽기 전용으로 잠기는 상수 포인터 등 (`__const` `1,048,168` B) |
| **`__DATA`** | `49,152` (0.05 MiB) | 0.06% | 초기값이 있는 전역 변수 (`size -m`은 0으로 채워지는 영역 포함 `65,536` B) |
| **`__LINKEDIT`** | `47,913,984` (45.69 MiB) | **54.05%** | 링커·로더용 부가 정보 (`size -m` 가상 크기는 `47,923,200` B) |
| **합계** | **`88,644,608`** | **100.00%** | `39,616,512 + 1,064,960 + 49,152 + 47,913,984` |

`__LINKEDIT` (`47,913,984` B)만 따로 `otool -l`과 `nm`으로 쪼개 보면 85MB의 주범이 뚜렷하다.

- **`LC_SYMTAB` 문자열 테이블 (`strsize`)**: **`25,113,864` B** (약 23.95 MiB) — Rust의 제네릭 함수 이름(예: `__RINv...`)이 길게 풀려 저장된 텍스트 목록이다.
- **`LC_SYMTAB` 심볼 테이블 (`nsyms: 678,508`개 × 16 B)**: **`10,856,128` B** (약 10.35 MiB) — `nm -a` 총 줄 수(`678,508`)와 정확히 일치한다. `LC_DYSYMTAB` 기준 지역 심볼(`nlocalsym`) `574,988`개, 외부 공개 심볼(`nextdefsym`) `102,847`개, 외부 참조 심볼(`nundefsym`) `673`개다(`nm`: `160,060`줄, `nm -g`: `103,520`줄, `nm -u`: `673`줄).
- **`LC_DYLD_INFO_ONLY`**: **`11,055,472` B** (약 10.54 MiB) — 이 중 거의 전부인 **`11,017,784` B**(약 10.51 MiB)가 **Export Trie(`export_size`, 외부로 공개하는 심볼 색인)**다. (`rebase` `16,976` B, `bind` `6,528` B, `weak_bind` `24` B, `lazy_bind` `14,160` B). 일반 실행 파일인데도 외부 심볼 `102,847`개가 Export Trie에 기록되어 `dyld_info -exports`가 `102,850`줄을 출력한다.
- **`LC_CODE_SIGNATURE`**: `705,312` B (adhoc 코드 서명), **`LC_FUNCTION_STARTS`**: `178,920` B (함수 시작 주소 목록), **`LC_DYSYMTAB` 간접 심볼(`1,069`개 + 정렬)**: `4,280` B.
- 참고: `otool -l`의 `LC_BUILD_VERSION`이 `minos 11.0`이므로(`src-tauri/tauri.conf.json:36`), macOS 12+용 `LC_DYLD_CHAINED_FIXUPS`·`LC_DYLD_EXPORTS_TRIE` 대신 `LC_DYLD_INFO_ONLY`가 쓰였다.

### 1.3 임시 사본 `strip` 실측 [사실(확인함)]

원본(`/Applications/skills-manage.app`)은 건드리지 않고, 실행 파일만 임시 폴더에 복사해 macOS `strip` 도구의 각 옵션을 실행해 크기를 쟀다(임시 사본은 측정 직후 삭제함).

| 실험 대상 | 파일 크기 (바이트) | 미변환 대비 감소량 | 비고 |
|---|---:|---:|---|
| 원본 사본 (`orig`) | `88,644,608` (84.54 MiB) | — | `__LINKEDIT` `47,913,984` B |
| `strip -S` (디버그 정보만 제거) | `80,074,432` (76.37 MiB) | `-8,570,176` B (-9.7%) | 이미 `debug = false`라 DWARF가 없어 감소 폭 작음 |
| `strip -x` (비전역·지역 심볼 제거) | `75,858,320` (72.34 MiB) | `-12,786,288` B (-14.4%) | 전역 심볼 `102,847`개와 긴 이름이 남아 절반 미만 감소 |
| **`strip`** (기본, `strip -u -r` 동일) | **`52,702,544`** (50.26 MiB) | **`-35,942,064` B (-40.5%)** | `__LINKEDIT` → `11,976,704` B로 축소 (`nsyms` 678,508 → 675, `strsize` 25.11 MB → 17,128 B) |

**중요한 관찰 [사실(확인함)]**: `strip`을 실행하면 `dyld_info -exports`가 `102,850`줄에서 `__mh_execute_header` **단 1개(4줄)**로 줄어든다. 즉, 실제로 앱 실행에 필요한 외부 공개 심볼은 없었다. 그런데도 `otool -l`의 `export_size`(`11,017,784` B)는 1바이트도 줄어들지 않고 `52.70MB` 안에 그대로 남았다. macOS의 사후 `strip` 도구는 Export Trie 안의 항목을 비워 무효화(prune)할 뿐 영역 자체의 길이를 줄이지 못하기 때문이다([dotnet/macios#18332](https://github.com/dotnet/macios/issues/18332)에서 보고된 macOS `cctools strip`의 알려진 한계와 일치).

---

## 2. 내장 화면 파일 (`dist/`) 크기와 포함 방식

### 2.1 Tauri가 `dist/`를 넣는 방식 [사실(확인함)]

1차 소스(`~/.cargo/registry/src/.../`에 내려받은 실제 의존성 크레이트 원문 및 저장소 설정)로 확인한 동작은 다음과 같다.

1. `src-tauri/Cargo.toml:18`은 `tauri = { version = "2", features = [] }`로 기본 피처(`default-features = true`)를 그대로 사용한다.
2. `tauri` v2.10.3의 `Cargo.toml:82-88`은 `default` 피처에 **`compression`**을 기본 포함한다(`tauri-macros/compression`, `tauri-utils/compression`).
3. 빌드 시 `tauri-codegen` v2.5.5(`src/embedded_assets.rs:289-303, 374-434`)가 `src-tauri/tauri.conf.json:10`의 `frontendDist: "../dist"` 폴더를 순회해 모든 파일을 **Brotli(릴리스 `quality = 9`, 디버그 `quality = 2`)**로 압축하고, `phf::Map<&'static str, &'static [u8]>`(완전 해시 맵)과 `include_bytes!` 매크로를 통해 Rust 실행 파일 안에 정적 바이트 배열로 박아 넣는다.
4. 앱 실행 중 웹 화면이 파일을 요청하면 `tauri-utils` v2.8.3(`src/assets.rs:169-182`)의 `EmbeddedAssets::get`이 `brotli::BrotliDecompress`로 실시간 압축 해제해 WebView에 전달한다.
5. 설치된 v0.12.3 바이너리의 문자열(`strings`)에서도 `brotli_decompressor::BrotliDecompress` 심볼, Brotli 정적 사전(약 122KB), `/assets/index-*.js`, `/assets/index-*.css` 경로가 직접 확인된다.

### 2.2 현재 `main` 기준 `dist/` 크기 실측 [사실(확인함)]

worktree에서 `pnpm install --frozen-lockfile && pnpm build`를 1회 실행해 Vite 프로덕션 빌드 결과(`dist/`)를 재고, `tauri-codegen`과 같은 Brotli `quality = 9` 압축 크기를 Node.js `zlib.brotliCompressSync`로 계산했다(측정 후 `dist/`와 `node_modules/`는 삭제 완료).

| 구분 | 원본 크기 (바이트) | Brotli-9 압축 후 (바이트) | 비중 (원본 기준) | 주요 내용 |
|---|---:|---:|---:|---|
| 주 JS 번들 (`index-*.js` + 소형 청크 1개) | `1,627,118` | `394,439` | 84.87% | React 18, `@lobehub/icons`, `react-markdown`, `gray-matter`, `i18next`, Zustand 등 |
| CSS (`index-*.css`) | `111,888` | `17,685` | 5.84% | Tailwind CSS 4 스타일 |
| 웹 폰트 (`.woff2` 8개) | `142,616` | `142,615` | 7.44% | Geist Variable 3개, JetBrains Mono Variable 5개 (이미 WOFF2 압축됨) |
| 이미지·아이콘 (`.png` 6개, `.svg` 2개) | `35,116` | `31,968` | 1.83% | 플랫폼 아이콘 및 로고 |
| HTML (`index.html`) | `460` | `205` | 0.02% | 진입 HTML |
| **합계 (`du -sh dist` = 1.9M)** | **`1,917,198`** (1.83 MiB) | **`587,072`** (573.3 KiB) | **100%** | **전체 88.64MB 실행 파일의 약 0.66%** |

> **버전 차이 주의 [사실(확인함)]**: 설치된 `/Applications/skills-manage.app`은 **2026-09-29 빌드(v0.12.3, 커밋 `b8b3918` 전후)**이고, 위 `dist/` 실측은 **2026-11-19 `main` (`f1c1ba1`)** 기준이다. 두 시점 사이에 `src/`, `src-tauri/`, `package.json`을 포함해 **82개 파일(`+8,288 / -1,285`줄)**이 변경되었으므로(`git diff --stat b8b3918..HEAD`), 9월 v0.12.3 바이너리에 들어 있는 `dist` 크기는 위 수치와 다를 수 있다. 다만 9월 바이너리 문자열에서도 JS 2개 + CSS 1개 구조가 확인되어 전체 규모(약 1MB 미만)에는 큰 차이가 없을 것으로 보인다 [추정].

---

## 3. 릴리스 설정(`Cargo.toml`) 효과 추정

### 3.1 현재 상태: 기본 Cargo 프로파일 그대로 빌드됨 [사실(확인함)]

`src-tauri/Cargo.toml`에는 `[profile.release]` 블록이 없다. Cargo 공식 문서([Profiles Reference](https://doc.rust-lang.org/cargo/reference/profiles.html))에 따르면 이 경우 다음 기본값이 적용된다.

- `opt-level = 3` (속도 최우선 최적화: 반복문 펼치기·적극적 인라이닝으로 코드 크기 증가)
- `debug = false`
- `strip = "none"` (심볼과 문자열 테이블을 전혀 지우지 않음 → **35.94MB 이상 낭비의 직접 원인**)
- `lto = false` (각 코드 생성 단위 내부만 지역 최적화, 크레이트 간 미사용 코드 제거·심볼 내부화 미실행 → **Export Trie 11.02MB와 중복 제네릭 코드 팽창의 원인**)
- `panic = 'unwind'` (패닉 시 스택을 거슬러 풀기 위한 코드·테이블 포함 → **`__eh_frame` 6.20MB + `__gcc_except_tab` 1.36MB + `__unwind_info` 1.22MB = 8.78MB**)
- `codegen-units = 16` (컴파일 병렬성을 위해 코드를 16조각으로 쪼개어 컴파일 → 크레이트 내 최적화·중복 제거 효율 저하)

### 3.2 Tauri 공식 권장 설정 적용 시 크기 추정 [사실+추정]

Tauri v2 공식 문서([App Size](https://v2.tauri.app/concept/size/))는 안정 도구 모음 기준으로 다음 설정을 권장한다.

```toml
[profile.release]
codegen-units = 1
lto = true
opt-level = "s"
panic = "abort"
strip = true
```

여기에 `tauri.conf.json`의 `"build": { "removeUnusedCommands": true }` (`tauri@2.4` 이상 지원, 본 앱은 `2.10.3`)를 더하면 권한(ACL)에 없는 미사용 Tauri 명령도 제거할 수 있다. 각 설정이 현재 바이너리의 어느 부분을 줄이는지 실측값과 합쳐 추정하면 다음과 같다.

| 설정 | 공식 문서 근거 | 겨냥하는 영역 (v0.12.3 실측값) | 예상 효과 | 근거 구분 |
|---|---|---|---|---|
| `strip = true` | Cargo/Tauri 공식: 심볼 테이블 제거 | `LC_SYMTAB` + `strtab` (`35.97` MB) | **`-35.94` MB** (`88.64 → 52.70` MB) | **사실(사본 실측)** |
| `lto = true` + `codegen-units = 1` | 크레이트 전체 합쳐 죽은 코드 제거·심볼 내부화 | `export_size` (`11.02` MB) + `__text` (`28.06` MB) | Export Trie 대폭 축소 및 중복 제네릭 코드 제거 (Collabora 사례는 LTO 단독 `-67.6%`, 본 앱은 C 정적 라이브러리 등 포함) | **추정** (공식 원리 + 외부 사례) |
| `opt-level = "s"` | 크기 우선 최적화 (vs `"3"` 속도 우선) | `__text` (`28.06` MB) | 인라이닝/루프 언롤링 축소로 코드 수 %~10%대 추가 감소 | **추정** |
| `panic = "abort"` | 패닉 시 스택 풀기 생략 | `__eh_frame` + `__gcc_except_tab` + `__unwind_info` (`8.78` MB) | 스택 풀기 테이블·코드 상당 부분 제거 (단, macOS arm64에서 C/Swift 등 일부 프레임이 남을 수 있음) | **추정** |
| **합산 예상 크기** | — | 남은 `52.70` MB에서 30~55% 추가 감소 가정 | **실행 파일 약 23 ~ 37 MB** (`.app` 약 24~38 MB, `.dmg`/`.zip` 수 MB대) | **추정 (실제 릴리스 빌드 검증 필요)** |

> 왜 Tauri 홍보 수치("수 MB")까지 작아지지 않을까? Tauri 공식 문서와 [min-sized-rust](https://github.com/johnthagen/min-sized-rust)의 초소형 수치는 "기능이 거의 없는 빈 앱" 기준이다. `skills-manage`는 SQLite C 엔진, TLS 암호화(`rustls`+`ring`), HTTP 클라이언트, 비동기 런타임, 파일 아카이브, 파서 여러 종을 정적 링크하므로 최적화를 모두 켜도 **20~30MB대**가 현실적인 목표 범위로 추정된다 [추정].

---

## 4. 의존성 기여 분석 (`Cargo.lock`, `cargo tree`, `nm`)

### 4.1 전체 의존성 규모 [사실(확인함)]

- `src-tauri/Cargo.toml`: 직접 의존성 23개 + macOS 전용 `swift-rs` 1개 + 빌드 의존성 2개(`tauri-build`, `swift-rs`).
- `src-tauri/Cargo.lock`: 총 **687개** 패키지(이 중 버전이 2개 이상 공존하는 크레이트 66종).
- **중요**: `Cargo.lock`에는 Windows(`windows-sys` 등), Linux(`webkit2gtk`), 모바일(iOS/Android용 `reqwest v0.13.2`, `tauri-2.10.3/Cargo.toml:314`) 의존성까지 모두 기록된다. `cargo tree --edges normal`로 **실제 macOS 호스트 런타임 빌드에 들어가는 고유 크레이트**만 세면 **341개**다 [사실(확인함)].

### 4.2 용량·빌드 시간에 기여하는 주요 후보 [사실+추정]

실제 바이트 크기 비중을 재려면 `cargo bloat --release` 빌드가 필요하므로, 여기서는 `cargo tree`의 전이 의존성 수, `otool -L`의 동적 링크 여부, 그리고 설치된 바이너리(`nm`)의 심볼 수로 무거운 후보를 좁혔다.

| 직접 의존성 | 전이 크레이트 수 (`cargo tree`) | 바이너리 내 관련 심볼 수 (`nm`) | 커지는 이유 (사실/추정) |
|---|---:|---:|---|
| **`tauri` v2.10.3** + 플러그인 4종(`dialog`, `shell`, `sql`, `fs`) | 193 (+플러그인) | `tauri`: `25,150` | WRY/TAO, Brotli 압축해제(`brotli-decompressor`), ACL 명령 매크로. v2는 ACL 구조상 미사용 명령 제거(`removeUnusedCommands`)를 따로 켜지 않으면 코드가 크다([tauri#12820](https://github.com/tauri-apps/tauri/issues/12820)) [사실]. |
| **`reqwest` v0.12.28** (`rustls-tls`) | 117 | `rustls`: `6,359`, `ring_`: `292` | macOS 시스템 TLS 대신 `rustls` + `ring`(C/어셈블리 암호화) + `webpki-roots`(루트 인증서 번들) + `hyper`/`h2`를 정적 포함 [사실]. |
| **`sqlx` v0.8.6** (`sqlite`, `runtime-tokio`) | 77 | `sqlx`: `10,047`, `sqlite3`: `1,884` | `libsqlite3-sys v0.30.1`이 `bundled` 피처로 켜져 있어(`otool -L`에 `libsqlite3.dylib` 없음), **SQLite C 엔진 전체가 바이너리에 정적 내장**됨 [사실]. |
| **`tokio` v1.51.1** | (공유) | `tokio`: `11,793` | 비동기 런타임·태스크 스케줄러·프로세스·타이머 코드 [사실]. |
| **파서·도구 모음** (`serde`, `serde_yaml`, `toml`, `toml_edit`, `json5`, `regex`, `chrono`, `tar`, `flate2`, `keyring`) | 각 0~11 | — | 여러 설정 포맷 파서가 중복 파싱 코드를 생성(`toml` 0.9와 `toml_edit` 0.25 등), `regex`는 유니코드 테이블 포함 [사실+추정]. |
| **`swift-rs` v1.0.7** (`SkillsManageTranslation`) | 3 | `SkillsManageTranslation`: `223` | `otool -L` 확인 결과 Swift 런타임과 `Translation`/`SwiftUI`/`NaturalLanguage`는 모두 OS 동적 라이브러리로 링크됨. 자체 Swift 소스도 2개 파일(~11KB)뿐이라 바이너리 용량 비중은 작을 것으로 보임 [추정]. |

---

## 5. 메모리와 시작 시간 실측 (가짜 HOME) [사실(확인함)]

실제 사용자 데이터(`~/.skillsmanage`)와 키체인을 건드리지 않기 위해, 실행 전 기존 `skills-manage` 프로세스가 없음을 확인하고, 매 회차 새로운 임시 폴더(`$FAKE`)에 `HOME="$FAKE" CFFIXED_USER_HOME="$FAKE"`를 지정해 `/Applications/skills-manage.app/Contents/MacOS/skills-manage`를 직접 3회 실행했다(기존 다른 앱의 WebKit 보조 프로세스 3개 PID와 분리해 새로 뜬 PID만 추적, 각 회차 종료 후 띄운 PID와 `$FAKE` 폴더 즉시 삭제 확인).

### 5.1 시작 시간 실측 (빈 가짜 HOME 기준)

`CGWindowListCopyWindowInfo`로 창 생성(`layer == 0` 전체 목록 등장) 및 화면 표시(`OnScreenOnly` 등장) 시점과, WebKit 렌더링 프로세스(`com.apple.WebKit.WebContent`) 스폰 시점을 20ms 간격으로 측정했다.

| 회차 | 창 객체 생성 (`t_allwin`) | WebContent 프로세스 등장 | 화면에 창 표시 (`t_onscreen`) | 비고 |
|---|---:|---:|---:|---|
| **Run 1** (첫 기동) | `0.631초` | `0.918초` | **`1.101초`** | macOS 동적 로더·파일 캐시 적재 포함 |
| **Run 2** (캐시 후) | `0.167초` | `0.193초` | **`0.287초`** | OS 캐시 적용 상태 |
| **Run 3** (캐시 후) | `0.132초` | `0.186초` | **`0.273초`** | OS 캐시 적용 상태 |

> **해석 한계 [사실+추정]**: 위 시간은 "스킬과 프로젝트 폴더가 비어 있는 가짜 HOME에서 네이티브 창이 뜨는 시각"이다. 첫 화면 안의 React 렌더링과 초기 스캔이 완전히 끝나는 시점은 이보다 수십~수백 ms 이상 더 걸리며, 실제 사용자 환경(수백 개 스킬·넓은 프로젝트 폴더)에서는 시작 직후 자동 실행되는 디스크 스캔 때문에 체감 준비 시간이 훨씬 길어질 수 있다 [추정].

### 5.2 안정화 후 메모리 실측 (실행 약 12초 후, CPU 0.0~0.2% 유휴 상태)

macOS에서 프로세스가 실제로 점유한 물리 메모리 지표인 `footprint`(Phys Footprint)와 `ps`의 RSS(상주 메모리, 공유/클린 페이지 포함)를 함께 쟀다.

| 프로세스 | Run 1 `footprint` (RSS) | Run 2 `footprint` (RSS) | Run 3 `footprint` (RSS) | 역할 및 비율 |
|---|---:|---:|---:|---|
| **`com.apple.WebKit.WebContent`** | **92 MB** (34.80 MiB) | **85 MB** (83.80 MiB) | **86 MB** (18.31 MiB) | 웹 화면(React/JS/DOM) 실행 (약 **64~65%**) |
| **`skills-manage`** (메인) | **25 MB** (82.45 MiB) | **23 MB** (84.67 MiB) | **23 MB** (73.03 MiB) | Rust/Tauri 백엔드 + SQLite (약 **17~18%**) |
| **`com.apple.WebKit.GPU`** | **18 MB** (26.84 MiB) | **17 MB** (31.67 MiB) | **17 MB** (20.59 MiB) | WebKit 그래픽 가속 (약 **13%**) |
| **`com.apple.WebKit.Networking`** | **6.2 MB** (9.61 MiB) | **6.3 MB** (12.34 MiB) | **6.3 MB** (8.33 MiB) | WebKit 네트워크/자산 로더 (약 **4.5%**) |
| **총합 (`Summary Footprint`)** | **142 MB** | **131 MB** | **133 MB** | **안정적인 실제 물리 점유: 131~142 MB** |
| 총합 (`ps` RSS 합) | `153.70 MiB` | `212.48 MiB` | `120.27 MiB` | OS 페이지 회수·압축 상태에 따라 크게 변동 |

**핵심 포인트 [사실(확인함)]**:
1. 메인 Rust 프로세스의 `ps` RSS가 73~85 MiB로 크게 보이는 이유 중 하나는, `footprint`의 Clean 영역이 약 32~34MB(실행 파일의 읽기 전용 코드/링크 정보 매핑)에 달하기 때문이다. 실제 수정·할당해 점유한 메모리(`footprint`)는 **23~25 MB**다.
2. 전체 메모리 **131~142 MB 중 82% 이상(108~117 MB)**은 macOS WebKit이 쓰므로, Rust 코드를 다듬는 것만으로는 앱 실행 메모리를 극적으로 줄이기 어렵다.

---

## 6. 일반적인 Tauri · Electron 앱과의 비교 기준

Tauri는 Chromium 브라우저를 통째로 넣지 않고 OS 기본 WebView(macOS는 WKWebView, Windows는 WebView2, Linux는 WebKitGTK)를 쓰지만([Tauri Architecture](https://v2.tauri.app/concept/architecture/)), Electron은 Chromium 다중 프로세스(Main Node.js + 창별 Renderer 등)를 앱에 함께 묶는다([Electron Process Model](https://www.electronjs.org/docs/latest/tutorial/process-model)).

| 비교 대상 | 설치/번들 크기 (macOS arm64) | 런타임 메모리 | 출처 및 성격 |
|---|---|---|---|
| **skills-manage v0.12.3 (현재)** | `.app` **85 MB** (`.dmg` 10.7 MB) | **131~142 MB** (`footprint`) | 본 조사 실측 [사실] |
| **skills-manage (릴리스 최적화 적용 시)** | `.app` **약 24~38 MB** | ~131~142 MB (WebKit 비중 유지) | 본 조사 [추정] |
| **Tauri 빈 앱 (Elanis 벤치마크, Release)** | **≈ 5 MB** | **≈ 96 MB** (가용 메모리 변화 ≈ 76 MB) | [Elanis 비교 저장소](https://github.com/Elanis/web-to-desktop-framework-comparison) [2차 벤치마크] |
| **Tauri v1 → v2 실앱 사례 (`#12820`)** | 6.2 MiB → **9.8 MiB** (Linux, 최적화 켜짐) | — | [tauri#12820](https://github.com/tauri-apps/tauri/issues/12820) [실측 보고] |
| **Electron v45.3.4 공식 런타임 압축본** | **`.zip`만 120.98 MB** (115.4 MiB, 압축 해제 전) | — | [Electron GitHub Release v45.3.4](https://github.com/electron/electron/releases/tag/v45.3.4) [1차 공식] |
| **Electron 빈 앱 (Elanis 벤치마크, Release)** | **≈ 268 MB** | **≈ 370 MB** (가용 메모리 변화 ≈ 111 MB) | [Elanis 비교 저장소](https://github.com/Elanis/web-to-desktop-framework-comparison) [2차 벤치마크] |
| **현장 경험담: React UI 비교 (macOS M4 Max, dev)** | — | Electron **239 MB** physical / 667 MB RSS<br>Tauri **297 MB** physical / 569 MB RSS | [Reddit r/electronjs](https://www.reddit.com/r/electronjs/comments/1w95jn0/i_ran_the_same_react_ui_on_electron_tauri_and/) [경험담] |
| **현장 경험담: Mac Electron → Tauri 전환** | `.app` 1 GB → **172 MB** (-83%), DMG 232 → **69.5 MB** | (기존 Electron 백그라운드 200MB+ 언급) | [HN Show HN (Desktop Docs)](https://news.ycombinator.com/item?id=44118023) [경험담] |

**해석 [사실+추정]**:
- **용량**: 현재 `skills-manage`의 85MB는 Tauri 앱 평균(수~십수 MB)보다 훨씬 크지만, Electron 기본 앱(압축만 121MB, 압축 해제 268MB+)과 비교하면 여전히 3분의 1 이하 수준이다. `[profile.release]` 설정을 넣으면 일반적인 중형 Tauri 앱 수준(20~30MB대)으로 내려간다.
- **메모리**: macOS에서는 Tauri(WebKit) 131~142MB가 Electron 빈 앱(370MB)보다 적지만, 실제 복잡한 UI가 올라가면 Electron과 차이가 좁혀지거나 역전될 수 있다는 현장 보고도 있다(Windows에서는 Tauri도 Chromium 기반 WebView2를 써서 Elanis 벤치마크상 Electron ≈279MB vs Tauri ≈312MB로 차이가 거의 없다). 또한 Hacker News 논의([item?id=44120552](https://news.ycombinator.com/item?id=44120552))에서도 "디스크 용량보다 런타임 메모리·OS별 WebView 렌더링 차이·느린 Rust 빌드/CI 시간"을 실제 병목으로 꼽는 개발자 경험이 반복된다.

---

## 7. "무거움" 후보 종합 정리 (사실 vs 추정)

사용자가 체감하는 "무거움"의 다섯 가지 후보를 근거에 따라 나누면 다음과 같다.

| 후보 항목 | 관측된 수치 · 코드 근거 | 판정 (사실 / 추정) | 왜 무겁게 느껴지나 (요약) |
|---|---|---|---|
| **1. 설치 용량 (85MB `.app`)** | 실행 파일 `88.64MB` 중 `__LINKEDIT` `47.91MB`(심볼+문자열 `35.97MB`, Export Trie `11.02MB`). `Cargo.toml`에 `[profile.release]` 없음. `dist/`는 ~0.59MB(0.7% 미만). | **사실(확인함)** | 릴리스 빌드에서 심볼 제거·LTO를 켜지 않아 개발용에 가까운 이름표가 그대로 포함됨. 다운로드 `.dmg`(10.7MB)보다 설치 후 8배 커짐. |
| **2. 시작 시간 (창 뜨는 속도)** | 빈 가짜 HOME 3회 실측: 첫 기동 `1.101초`, 캐시 후 `0.273~0.287초`. | **실행 속도는 빠름 [사실]**<br>실데이터 체감 지연은 **추정** | 앱 자체 기동은 빠르다. 다만 실환경에서는 창이 뜬 직후 아래 4번 스캔과 GitHub 원본 확인(`check_linked_skill_origins`)이 이어서 돌며 초기 부담을 준다. |
| **3. 실행 메모리 (131~142MB)** | `footprint` 합계 `131~142MB` (WebKit 보조 3개 `108~117MB` + Rust 메인 `23~25MB`). | **사실(확인함)** | Rust 백엔드 문제가 아니라 macOS WebKit 엔진의 기본 체급. 같은 웹 UI 기반 Electron 대비 무겁지 않음. |
| **4. 초기 스캔 & "스캔 중..." 표시 (#46)** | `AppShell.tsx:51-64`가 시작마다 `rescanDiscoverFromDisk()` 자동 실행. `discover.rs:188-210, 473, 1182`가 `~/projects`·`~/Documents`·`/Applications` 등 최대 10개 루트를 깊이 8까지 동기 순회하고 **루트 1개가 끝나야 진행률 emit**. `TopBar.tsx:73-81`은 "스캔 중..."만 표시. | 코드 구조는 **사실(확인함)**<br>#46의 4분 이상 지연 원인은 **추정** | 릴리스/대용량 환경에서 디스크 전체 탐색이 수 분 걸리는 동안 상단 표시가 바뀌지 않아 앱이 멈추고 무거운 것처럼 보이게 만듦. |
| **5. CI 대기 시간 (앱 무게와 별개)** | 최근 GitHub Actions 실측(`gh run list`): `Backend` (`macos-15-intel`) **15m 02s ~ 19m 26s**, `Release Desktop` **~16m 27s ~ 17m 10s**, `CI` (Frontend+Graphify) 성공 시 **1m 32s ~ 4m 07s**. | **사실(확인함)** | 실행되는 앱의 무게가 아니라 **개발·검증 속도**의 무거움. Rust 백엔드를 Intel Mac 가상머신에서 컴파일·테스트하는 비용이 크다. |

---

## 8. 확인하지 못한 것과 추가 측정 제안

### 확인하지 못한 것
- `[profile.release]` 설정을 실제로 넣은 상태의 최종 바이너리 크기 (이번 조사는 릴리스 빌드 금지 원칙에 따라 실측하지 않고 `strip` 실측 + 공식 문서로 추정함).
- 크레이트별 정확한 `.text` 바이트 순위 (`cargo bloat --release` 필요).
- 실제 사용자 데이터(수백 개 스킬과 큰 프로젝트 폴더)가 들어 있는 상태에서의 `scan_all_skills`, `start_project_scan`, WebContent 메모리 증가량.
- 9월 29일 빌드(v0.12.3) 당시의 정확한 `dist/` 바이트 크기 (현재 `main` 대비 82개 파일이 바뀌기 전 시점).

### 추가 측정 제안 (필요 시 후속 티켓에서 실행)
1. **`[profile.release]` 실험 측정**: 임시 브랜치에서 Tauri 권장 5줄(`codegen-units = 1`, `lto = true`, `opt-level = "s"`, `panic = "abort"`, `strip = true`)을 넣고 `pnpm tauri build --debug` 또는 릴리스 빌드를 1회 돌려 `size -m`과 `otool -l`로 `export_size`와 `__eh_frame` 감소량을 실측한다.
2. **`cargo bloat --release --crates` 측정**: `ring`, `libsqlite3-sys`, `sqlx`, `tauri`, `reqwest` 중 실제로 코드(`__text`)를 가장 많이 차지하는 상위 20개 크레이트를 바이트 단위로 확정한다.
3. **실데이터 부하 측정(#46 연계)**: `start_project_scan`에 루트별 소요 시간 로그를 찍어 어느 루트(`/Applications`, `~/Documents`, `iCloud` 등)가 수 분을 소모하는지 확인한다.

---

## 출처 목록

- 설치 앱 바이너리 및 실행 측정: `/Applications/skills-manage.app/Contents/MacOS/skills-manage` (`otool -l`, `size -m`, `nm`, `codesign -dvv`, `dyld_info -exports`, 임시 사본 `strip`, 가짜 HOME 3회 실행 + `footprint` / `ps`)
- 저장소 소스:
  - `src-tauri/Cargo.toml:1-54`
  - `src-tauri/Cargo.lock`
  - `src-tauri/tauri.conf.json:1-87`
  - `src-tauri/build.rs:1-23`
  - `src-tauri/capabilities/default.json:1-12`
  - `src/components/layout/AppShell.tsx:46-88`
  - `src/components/layout/TopBar.tsx:72-81`
  - `src/stores/discoverStore.ts:107-150, 366-453`
  - `src-tauri/src/commands/discover.rs:188-226, 473-479, 962-1057, 1122-1192`
  - `src-tauri/src/commands/skill_origin.rs:1747-1786`
  - `.github/workflows/release-desktop.yml:84-143`, `.github/workflows/backend.yml:1-57`, `.github/workflows/ci.yml:1-50`
- Tauri / Cargo 크레이트 소스 및 공식 문서:
  - Tauri v2 공식 App Size 안내: https://v2.tauri.app/concept/size/
  - Tauri v2 공식 Architecture: https://v2.tauri.app/concept/architecture/
  - Cargo 공식 Profiles Reference: https://doc.rust-lang.org/cargo/reference/profiles.html
  - 로컬 Cargo 캐시 원문: `tauri 2.10.3/Cargo.toml:82-88, 314-320`, `tauri-codegen 2.5.5/src/embedded_assets.rs:289-303, 374-434`, `tauri-utils 2.8.3/src/assets.rs:169-182`
- 비교 대상 공식 출처 및 커뮤니티 실측:
  - Electron 공식 Process Model: https://www.electronjs.org/docs/latest/tutorial/process-model
  - Electron v45.3.4 공식 릴리스 아티팩트: https://github.com/electron/electron/releases/tag/v45.3.4
  - skills-manage v0.12.3 릴리스 아티팩트: https://github.com/122yjs/skills-manage/releases/tag/v0.12.3
  - Tauri v1→v2 크기 증가 이슈: https://github.com/tauri-apps/tauri/issues/12820
  - macOS `strip` export trie 한계 참고 이슈: https://github.com/dotnet/macios/issues/18332
  - 데스크톱 프레임워크 벤치마크(Elanis): https://github.com/Elanis/web-to-desktop-framework-comparison
  - min-sized-rust: https://github.com/johnthagen/min-sized-rust
  - Reddit 실측 경험담(r/electronjs): https://www.reddit.com/r/electronjs/comments/1w95jn0/i_ran_the_same_react_ui_on_electron_tauri_and/
  - Hacker News 실사용 후기: https://news.ycombinator.com/item?id=44118023 , https://news.ycombinator.com/item?id=44120552
