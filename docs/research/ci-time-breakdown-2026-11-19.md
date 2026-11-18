## 한눈에 보기 (쉬운 설명)

- **서버를 받은 뒤 오래 걸린 것과, 서버를 못 받아 취소된 것은 다른 문제다.** Intel Mac에서 끝까지 돈 두 작업은 서버를 받는 데 **4초·27초**, 시작해서 끝나는 데 **16분 26초·18분 56초**가 걸렸다. 이때 대부분의 시간은 지난 빌드 결과(캐시) 없이 Rust 코드를 검사하고 테스트용 프로그램을 만드는 데 들었다. [실행 기록 1][intel-branch] · [실행 기록 2][intel-main]
- Apple 칩 Mac에서 캐시를 일부 재사용한 작업은 **4분 3초**였다. 세 작업에서 테스트 프로그램을 **실제로 돌린** 시간은 **12~38초**였다. 칩과 캐시 상태가 동시에 달랐기 때문에 Intel 자체가 몇 분 느린지는 아직 모른다. [Apple 칩 실행][arm-warm]
- 반대로 **11건**은 약 15분 동안 Mac 서버를 하나도 받지 못해 검사 자체가 시작되지 않았다. PR(변경 제안) 10건과 `main`에 변경을 올린 `push` 1건이다. 따라서 **PR에서만 생기는 현상은 아니다.** 아래 취소 목록과 GitHub 주석을 확인했다.
- 가장 먼저 할 일은 **이미 채워진 `main`의 Intel 캐시를 다음 정상 실행이 재사용하는지 보는 것**이다. 서버 배정 실패는 캐시로 고칠 수 없으므로 따로 기록한다. 이번에는 CI 설정을 바꾸거나 검사를 다시 돌리지 않았다.

## 1. 시간을 어디에 썼나

**확인 범위.** [#50](https://github.com/122yjs/skills-manage/issues/50)의 세 성공 사례를 비교했다. 대상 코드는 `f1c1ba1` 기준이다. [`.github/workflows/backend.yml:5–19, 27–56`][backend-src]에서 `src-tauri/**` 또는 백엔드 워크플로 파일이 바뀔 때 Mac 검사를 요청한다([#48](https://github.com/122yjs/skills-manage/pull/48)). 이전에는 [`.github/workflows/ci.yml:54–82` (`6347f22`)][old-ci-src]에 Mac 작업이 있었고, 현재 CI의 Frontend(화면 검사)와 Graphify(코드 관계 검사)는 Ubuntu에서 돈다([`.github/workflows/ci.yml:16–19, 51–53`][ci-src]). 배포 작업은 범위 밖이다.

**확인한 시간표.** 형식은 `분:초`. 서버 대기는 GitHub 작업 API의 **작업 생성 → 시작**이고, 실행은 **시작 → 완료**다. 서버가 없던 작업에는 이 계산 대신 **생성 → 취소**를 쓴다. 아래의 단계명과 초 단위 경계는 작업 API, 단계 안쪽의 계산은 원본 로그 줄의 UTC 시각을 썼다.

| 항목 | [43777877544][arm-warm] Apple 칩, PR | [44354642420][intel-branch] Intel, 기능 브랜치 수동 | [44361080164][intel-main] Intel, `main` 수동 |
|---|---:|---:|---:|
| 실행 서버 라벨 | `macos-15` | `macos-15-intel` | `macos-15-intel` |
| 서버 대기 | 0:02 | 0:04 | 0:27 |
| 서버 준비 / 코드 받기(Checkout) | 0:02 / 0:02 | 0:03 / 0:03 | 0:03 / 0:03 |
| Rust 도구 설치(세 작업 모두 `rustc 1.100.0`) | 0:10 | 0:12 | 0:16 |
| 캐시 복원 | 0:19, 부분 일치 | 0:01, 없음 | 0:01, 없음 |
| 형식 검사 | 0:01 | 0:01 | 0:02 |
| `cargo test` 전체: 프로그램 만들기 + 테스트 실행 | **1:49** | **9:21** | **11:00** |
| `cargo clippy` 전체: 실행하지 않고 코드 검사 | **0:51** | **3:50** | **4:45** |
| 캐시 저장 단계: 정리·압축·업로드 포함 | 0:44 | 2:50 | 2:41 |
| 기타(단계 사이, 마무리) | 0:05 | 0:05 | 0:05 |
| **서버를 받은 뒤 실행 합계** | **4:03** | **16:26** | **18:56** |
| **작업 생성부터 완료까지** | **4:05** | **16:30** | **19:23** |

### 테스트와 Clippy 더 쪼개기

**확인한 로그 표식.** 아래는 로그 줄 사이의 경과 시간이다. `Compiling`(빌드 시작)과 `Checking`(코드 검사 시작)은 여러 개가 동시에 진행된다. 따라서 첫 줄부터 마지막 줄까지는 **작업이 시작된 범위**이지, 의존성에 든 시간의 합이 아니다. 특히 앱이 시작된 뒤에도 의존성 표식이 남는다. 앱 표식부터 `Finished`까지는 앱만의 시간이 아니라 남은 의존성, 빌드 스크립트, 파일을 합치는 일(링크) 등이 겹치는 구간이다. 이 표의 행을 더해 전체 시간을 만들면 안 된다. [원본 로그: Apple 칩][arm-warm] · [Intel 기능 브랜치][intel-branch] · [Intel main][intel-main] · [Cargo 시간 기록 문서][timings]

| 로그의 계산 기준 | 43777877544 | 44354642420 | 44361080164 |
|---|---:|---:|---:|
| 테스트 명령 → `Finished test` (받기·컴파일·링크) | 1:37 | 8:45 | 10:15 |
| 테스트 의존성(앱이 가져다 쓰는 부품) `Compiling` 줄 수 / 첫 줄 → 마지막 줄 | 3 / 0:01 미만 | 363 / 4:18 | 363 / 5:43 |
| 테스트 첫 의존성 → `Compiling skills-manage` | 0:01 미만 | 3:52 | 5:26 |
| 테스트 `Compiling skills-manage` → `Finished test` | 1:30 | 4:38 | 4:36 |
| 실제 테스트: 첫 `Running unittests` → 마지막 `test result`(문서 예제 포함) | 0:12 | 0:34 | 0:38 |
| 각 `test result ... finished in`에 찍힌 시간 합 | 10.47초 | 30.64초 | 34.45초 |
| Clippy 의존성 `Compiling` 줄 수 / 첫 줄 → 마지막 줄 | 0 / — | 182 / 2:12 | 182 / 2:50 |
| Clippy 의존성 `Checking` 줄 수 / 첫 줄 → 마지막 줄 | 24 / 0:12 | 118 / 2:17 | 118 / 2:57 |
| Clippy 첫 의존성 → `Compiling skills-manage` | 0:01 미만 | 2:13 | 2:51 |
| Clippy `Compiling skills-manage` → `Finished dev` | 0:50 | 1:33 | 1:50 |

예를 들어 `main` Intel 실행에서 테스트 의존성 첫 표식은 **17:40:34.751**, 앱 표식은 **17:46:00.720**, 마지막 의존성 표식은 **17:46:17.977**, `Finished test`는 **17:50:36.655**다. Clippy도 앱 표식(**17:54:16.960**) 뒤에 의존성 `Checking` 표식이 **17:54:23.120**까지 남아 있다. [44361080164 원본 로그][intel-main]

**앱은 몇 번 컴파일됐나?** 각 로그에서 `Compiling skills-manage`가 `test`에 한 줄, `clippy`에 한 줄씩, **두 줄** 보였다. `Checking skills-manage`는 없었다. 이게 `rustc`(Rust 컴파일러)를 정확히 두 번 호출했다는 뜻은 아니다. [Cargo의 출력 코드 `note_working_on`][cargo-queue]는 같은 패키지의 일반 `Compiling`/`Checking` 표식을 한 번만 찍는다. [Cargo test 문서][cargo-test]에 따르면 라이브러리를 단위 테스트용과 통합 테스트에서 가져다 쓰는 용도로 각각 빌드할 수 있다. 이 앱은 테스트 때 실제 키체인·사용자 `gh`를 쓰지 않도록 자기 자신을 `test-isolation` 옵션으로 개발 의존성에 둔다([`src-tauri/Cargo.toml:45–53`][manifest]; [`src-tauri/src/secrets.rs:136–146`][secrets]; `src-tauri/src/commands/github_import.rs:1352–1364`). 하지만 옵션이 두 가지라는 이유만으로 같은 명령 안에서 두 번 빌드된다고 할 수는 없다([Cargo 기능 선택 규칙][cargo-features]). `staticlib`, `cdylib`, `rlib`도 한 컴파일러 호출에서 여러 파일로 나올 수 있다([Rust 링크 규칙][linkage]). 어느 옵션·대상·라이브러리 형식이 추가 호출이나 링크 비용을 만들었는지는 이 로그에서 미확인이다.

**테스트와 Clippy는 얼마나 공유하나?** 캐시가 없던 두 Intel 작업 모두, 테스트에서 컴파일한 **이름·버전**과 일치하는 의존성 중 Clippy에서 **182개 `Compiling`, 118개 `Checking`**이 다시 찍혔다. 적어도 모든 결과를 공유하지는 않는다. 이 버전의 [Clippy 소스][clippy-src]는 `cargo check`를 호출하며, `RUSTC_WORKSPACE_WRAPPER`(앱 코드에 검사기를 끼우는 설정)는 [별도 산출물 이름을 만든다][cargo-env]. `check`는 일반 Rust 코드의 실행 파일을 만드는 단계까지 가지 않지만 일부 빌드 스크립트와 매크로는 실제로 빌드한다. 비슷한 파일 일부를 공유할 수는 있어도, 로그만으로 정확히 무엇을 재사용했는지는 확인할 수 없다. `test`는 테스트 실행 파일이 필요하므로 하나를 다른 하나로 대신할 수도 없다. [Cargo check 문서][cargo-check] · [두 Intel 로그][intel-branch] [main 로그][intel-main]

### 그래서 Rust 탓인가?

- **확인:** Intel에서 서버가 배정된 뒤, 테스트 명령과 Clippy가 **13:11 / 15:45**로 실행 시간의 **80% / 83%**였다. 그중 테스트 프로그램 자체는 **0:34 / 0:38**만 돌았다. 캐시 저장 단계도 **2:50 / 2:41**을 썼다. 대기는 **0:04 / 0:27**이었다. 오래 걸린 성공 사례의 큰 몫은 테스트 결과를 기다린 시간이 아니라 **캐시 없이 Rust와 의존성을 빌드·검사한 시간**이다. Swift 빌드 같은 앱 준비 작업도 그 안에 들어갈 수 있다. [두 Intel 로그][intel-branch] [main 로그][intel-main] · [`src-tauri/build.rs:1–21`][build-src]
- **확인:** Apple 칩·부분 캐시 작업보다 Intel·캐시 없음 작업의 실행 시간이 **12:23 / 14:53** 길었다. Apple 칩에서는 테스트 의존성 컴파일 표식이 **3개**, Intel에서는 **363개**였으므로 캐시가 일을 많이 덜어준 것은 확인된다. 그러나 세 작업의 칩·서버 이미지·커밋·캐시 상태가 같지 않다. **칩 차이만의 시간이나, 캐시만의 절감 시간으로 이 차이를 쓰면 안 된다.** [세 작업 로그][arm-warm] [기능 브랜치][intel-branch] [main][intel-main]
- **확인:** 서버가 없었던 약 15분의 취소 11건은 테스트·컴파일 **0회**다. 언어를 바꾸거나 Rust 빌드를 빠르게 해도 서버를 배정받기 전에 생긴 실패는 해결되지 않는다. 근거는 아래 작업별 API 주석이다.

## 2. 왜 캐시를 못 썼나

**확인: 같은 이름이어도 저장된 브랜치가 다르면 안 보일 수 있다.** [GitHub의 캐시 접근 규칙][gha-cache]에 따르면 현재 브랜치와 기본 브랜치(`main`)의 캐시를 복원할 수 있고, PR은 대상 브랜치의 캐시도 읽을 수 있다. 하지만 `main`은 그 아래 기능 브랜치의 캐시를 못 읽는다. PR이 만든 캐시는 PR의 임시 합친 코드(`refs/pull/.../merge`)에 저장돼 그 PR의 재실행에는 쓰이지만 다른 PR이나 `main`에는 안 보인다.

| 확인한 실행 | 캐시 로그와 API의 결과 |
|---|---|
| [43777877544][arm-warm] Apple 칩 PR | 시도 키 `v0-rust-backend-Darwin-arm64-60948658-39871cdc`. 당시 `main`의 이전 `…-8ad20e75` 키를 복원(`full match: false`). 성공 후 PR #43의 merge 위치에 `…-39871cdc`를 새로 저장. |
| [44354642420][intel-branch] Intel 기능 브랜치 | `No cache found.` 뒤 `v0-rust-backend-Darwin-x64-98c26fc7-39871cdc`를 기능 브랜치에 11-18 **17:21:38 UTC** 저장(668,240,180바이트). Apple 칩의 캐시와는 CPU 종류가 달랐다. |
| [44361080164][intel-main] Intel `main` | 앞선 Intel 실행과 **완전히 같은 키**를 요청했지만 `No cache found.`. `main`은 기능 브랜치의 사본을 못 읽는다. 이 실행이 **17:58:47 UTC**에 `refs/heads/main`에 같은 키의 별도 사본을 저장했다(668,232,294바이트). |

**확인: 키 구성과 저장 조건.** 세 로그에서 액션 버전은 `d1527c8`. 이 버전의 [키 생성 코드][rust-cache-config]는 기본적으로 작업 이름표(`GITHUB_JOB`: 여기서는 `backend`), OS·CPU 종류, 컴파일러 버전·관련 환경 설정, `Cargo.toml`·`Cargo.lock` 등의 지문(내용을 요약한 짧은 값)을 조합한다. [복원 코드][rust-cache-restore]는 전체 키를 먼저 찾고, 없으면 같은 환경에서 파일 지문을 뺀 앞부분으로도 찾는다. [액션 입력과 후처리][rust-cache-action] · [저장 코드][rust-cache-save]에 따르면 기본값은 성공 후 저장, 정확히 일치하면 저장 생략, 부분 일치나 미일치는 새 키로 저장 시도다. `save-if: false`는 저장만 끈다. 기본 저장 대상은 의존성 결과이며 앱 자체와 증분 컴파일 파일(이전 빌드를 조금만 고치기 위한 파일)은 제외한다([rust-cache README][rust-cache-readme]).

**판단:** 현재 `main`의 Intel 사본을 **다음 `main` 실행과 `main` 대상 PR 모두 읽을 수 있다**. 동일 키·같은 캐시 버전(저장 경로/압축 방식 등)이고 항목이 남아 있으면 정확히 일치한다. 의존성 파일만 달라지면 같은 환경의 앞부분 키로 일부 재사용할 수 있다. OS·CPU 또는 Rust 버전·환경이 달라지거나 캐시가 삭제되면 다시 못 쓸 수 있다. **이 사본을 복원한 새 실행은 아직 확인하지 않았다.** 앞선 두 Intel 실행의 실패는 동일 키의 저장 시각과 브랜치 범위로 설명되며, 한도 때문에 지워졌다는 증거는 없다. [로그][intel-main] · [GitHub 규칙][gha-cache] · [캐시 목록·사용량 API][cache-api]

**확인: 용량.** 기본 한도는 저장소 전체 **10 GB**다. Rust 캐시는 한 덩어리가 약 **637~652 MiB**(약 668~684 MB)라서, 다른 캐시가 없어도 약 **15개**면 기본 한도에 가깝다. 조회 당시 전체는 **9개, 3,973,789,591바이트(약 3.97 GB)**였고 이 중 Rust가 5개다. 한도에 닿으면 오래 안 쓴 항목부터 밀려나며, 7일 넘게 접근하지 않은 항목도 정리된다. 여러 PR에서 같은 큰 캐시를 새로 쓰면 `main` 캐시도 밀려날 수 있지만, 이번 조회에서는 용량이 꽉 차 있지 않았다. [GitHub 한도 문서][gha-cache] · [캐시 목록·사용량 API][cache-api]

## 3. Rust를 바꾸지 않고 줄이는 방법

아래 분 단위 효과는 **추정 또는 계산 예시**다. 같은 작업에서 서로 겹치는 효과를 더하면 안 된다. 특히 서버를 배정받는 문제와 배정 후의 시간을 따로 봐야 한다.

| 방법 | 기대 효과(분) | 비용·주의점과 근거 |
|---|---|---|
| **먼저 `main` Intel 캐시 재사용 확인** | **캐시가 없던 Intel 기준 약 8~11분 절감 가능성**. 가정: 테스트와 Clippy에서 첫 의존성 표식 → 앱 표식의 합(6.1~8.3분)이 대부분 사라지고, 정확히 일치해 저장(2.7~2.8분)을 건너뛰며, 복원에 약 0.3분을 쓴다면 그렇다. | Apple 칩의 19초 복원과 Intel의 로그 구간을 사용한 계산 예시이지 Intel 실측은 아니다. 의존성 다운로드·앱 빌드 등은 남을 수 있다. 이미 `main` 사본이 있으므로 새로 채우는 실행을 지금 만들 필요는 없다. [시간표][intel-main] · [GitHub 캐시 규칙][gha-cache] |
| **`save-if`로 `main`만 저장 / `shared-key`는 보류** | 저장할 예정이던 PR에서 **약 0.7~2.8분**을 아낄 여지가 있다(관측한 저장 단계). 정확히 일치해 저장을 이미 건너뛰면 **0분**. `shared-key` 단독은 현재 **0분 예상**. | `main` 사본이 늙지 않도록 먼저 읽고 쓰는 흐름을 확인한다. `save-if`로 PR 저장을 끄면 PR만의 의존성 변경을 재실행할 때 다시 빌드할 수 있다. `shared-key`는 같은 OS·환경에서 **서로 다른 작업 이름**끼리 키를 공유할 때 쓰며 브랜치 범위나 CPU 차이를 없애지 않는다. 지금은 분리 전후 `backend`가 같은 키를 썼고, 새 키를 정하면 첫 복원이 오히려 빗나갈 수 있다. [액션 설정][rust-cache-action] · [키 코드][rust-cache-config] · [저장 코드][rust-cache-save] |
| **sccache: 컴파일 결과를 부품별로 재사용** | 기존 `target` 캐시가 잘 맞는 실행에서 추가 절감은 **0분을 기본 가정**으로 둔다. 큰 캐시가 빗나가도 sccache가 따로 맞으면, Intel의 앱 이전 의존성 구간 **6.1~8.3분 중 일부**를 줄일 후보가 된다. 서버 대기 절감은 **0분**. | [공식 GHA 백엔드][sccache-gha]는 GitHub Actions 캐시를 사용한다. 공유 범위·저장 용량은 별도로 해결되지 않으며 두 캐시를 같이 쓰면 용량·네트워크 비용이 늘 수 있다. [Rust 지원 범위][sccache-rust]상 증분 컴파일, 최종 링크가 필요한 실행 파일·`cdylib`·절차 매크로, `link` 출력 없는 검사 호출은 캐시되지 않는다. Swift 스크립트와 테스트 실행도 그대로다. 주 캐시의 적중을 확인한 뒤, 저장·복원 시간과 재사용 건수를 따로 잴 가치가 있다. |
| **`test`·`clippy` 한 단계로 합치거나 순서 바꾸기** | 둘 다 통과하는 작업에서 **약 0분 예상**. Clippy를 먼저 두면 검사에서 실패했을 때 기존 Intel의 테스트 단계 **9.4~11분**을 건너뛸 수 있다. | 지금도 한 서버·한 `target`에서 도는데 검사 방식이 달라 재처리가 남는다. 두 명령을 한 단계에 써도 같은 산출물이 생기지 않는다. 둘을 별도 서버에서 동시에 돌리면 두 번째 배정과 의존성 작업이 필요하다. 테스트 대상을 줄이면 검사 범위가 달라지므로 단순한 속도 개선이 아니다. [Clippy 소스][clippy-src] · [Cargo check][cargo-check] · [현재 설정][backend-src] |
| **공통 검사만 Ubuntu로 옮기고 Mac 검사는 남기기** | 정상 배정 Intel의 대기만 보면 이론상 **최대 0.45분**(4~27초 관측). Mac 대기 취소 날에는 **실패한 15분을 기다리지 않고** 공통 결과를 받을 여지가 있다. Ubuntu에서 Rust 검사 자체가 몇 분 더 빠른지는 미확인이다. | 공개 저장소 표준 Ubuntu x64는 **4 vCPU·16 GB·14 GB SSD**, Intel Mac은 **4 vCPU·14 GB·14 GB SSD**, Apple 칩은 **3코어 M1·7 GB·14 GB SSD**다. 사양만으로 컴파일 속도를 비교할 수 없다. Tauri 설치 준비와 플랫폼 검사 공백은 아래 참고. [GitHub 서버 사양][runner-doc] · [Tauri Linux 준비물][tauri-prereq] |
| **데스크톱의 `crate-type`을 `rlib`만 남기는 방안** | 앱의 `Compiling` 이후 테스트 빌드가 끝날 때까지 **1.5분(Apple 칩) / 4.6분 안팎(Intel)**이었다. 파일 형식 두 개에 드는 시간이 이 중 얼마나 되는지는 미확인이다. Intel에서 **1분**을 줄이려면 해당 링크·파일 생성에 이 구간의 약 **22%**가 들어야 한다. | Rust 앱이 아닌 프로그램에 건네는 라이브러리 파일 두 종류(`staticlib`·`cdylib`)는 데스크톱 진입점 자체에 필요하지 않다. 앱이 가져다 쓰는 `rlib`까지 빼면 안 된다. 제거 후보이지만 모바일 지원이나 외부 소비자가 있다면 깨질 수 있고, 실제 빌드 확인이 필요하다. [Tauri 프로젝트 구조][tauri-structure] · [이슈 #7007][tauri-7007] · [수정 #7014][tauri-7014] · [Tauri CLI 2.10.1 소스][tauri-cli] · [`src-tauri/src/main.rs:4–6`][app-main] |
| **CI의 테스트·개발 빌드에서 디버그 정보 줄이기** | 예를 들어 테스트 준비·빌드 구간이 **10%** 줄면 Intel 미적중은 **약 0.9~1.0분**, Apple 칩 부분 적중은 **약 0.2분** 단축이다. 10%는 계산 기준일 뿐 실측이 아니다. Clippy에서 같은 비율을 기대할 근거는 없다. | 실제 로그는 `unoptimized + debuginfo`다. [Cargo 프로필][cargo-profile]에서 `test`는 `dev`를 물려받고 `debug = 0`은 오류 추적에 쓰는 정보를 뺀다. 생성·링크가 가벼워지는 대신 오류를 추적할 때 파일과 줄 번호가 줄 수 있다. `line-tables-only`로 줄 번호만 남기는 선택도 있다. 테스트나 경고 검사를 빼는 변경은 아니지만 설정 변경 시 관련 빌드 결과는 새로 만들어야 한다. [Cargo 빌드 성능 가이드][cargo-performance] · [프로필 환경 변수][cargo-env] |

**Linux에서 확인할 것.** [Tauri 공식 준비물][tauri-prereq]의 Ubuntu/Debian 계열에는 `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `libxdo-dev`, `libssl-dev`, `librsvg2-dev`, `build-essential` 및 `curl`, `wget`, `file`이 있다. 이 저장소의 `keyring`은 `4.2.0`([`src-tauri/Cargo.lock:2423–2433`][lock-keyring])이며 [기본 기능 `v1`][keyring-features]이 Linux에서는 **zbus 기반 Secret Service**(운영체제 비밀값 저장소), Mac에서는 Keychain Services를 선택한다([4.2.0 소스][keyring-v1]). 이 설정의 Linux 기본값은 `keyutils`가 아니다. 화면 없는 CI에서 실제 Secret Service를 쓰려면 D-Bus 세션과 저장소 서비스가 필요할 수 있다([백엔드의 공식 안내][secret-service]). 이 앱 테스트는 메모리 저장소를 쓰므로 해당 서비스가 반드시 필요한지는 별개다([`src-tauri/src/secrets.rs:98–117, 136–146`][secrets]). Ubuntu에서 통과해도 macOS 전용 `cfg`(Mac일 때만 포함되는 코드)의 Swift 링크와 번역 기능, 파일 관리자 열기 등은 검사되지 않는다([`src-tauri/build.rs:1–21`][build-src]; [`src-tauri/src/commands/on_device_translation.rs:25–42, 68–94`][translation]; `src-tauri/src/commands/skills.rs:1642–1668`). Linux는 공통 검사 보완으로 검토하고 Mac 검사 전체를 대신한다고 판단하지 않는다.

**라이브러리 파일 구분.** 이 프로젝트는 데스크톱 앱으로 안내하며(`README.md:3`), `main`에서 Rust 앱 라이브러리의 `run()`을 부른다([`src-tauri/src/main.rs:4–6`][app-main]). `lib.rs:16–17`에는 모바일 진입점도 표시돼 있다. [Tauri 2 문서][tauri-structure]와 [CLI 2.10.1 `build_options`][tauri-cli]가 데스크톱 앱 실행 파일(`--bins`)과 모바일 라이브러리(`--lib`)를 구분한다. [#7007][tauri-7007]은 데스크톱에서 추가 라이브러리를 만드는 비용을 제기했고, [#7014][tauri-7014]는 CLI가 데스크톱 실행 파일을 대상으로 고르게 한 변경이다. 현재 CI는 Tauri CLI가 아닌 **`cargo test`와 `cargo clippy`**를 직접 부른다. CLI의 변경만으로 CI에서 어떤 파일이 몇 번 생성됐는지 판단할 수는 없다. [현재 설정][backend-src] · [Rust 라이브러리 형식 문서][linkage]

## 4. 서버 배정 실패와 일반 취소 구분

**확인 범위.** 11-15~19(UTC 날짜, 11-19 한국시간 새벽 조회 시점까지)의 `CI`와 `Backend` 워크플로 **20개 실행 ID**를 조회했다. 한국 날짜 경계(11-14 15:00~23:59 UTC)에는 추가 실행이 없었다. 재실행까지 포함해 Mac Backend가 **23번** 기록됐다. 작업의 `created_at` → `completed_at`을 썼다. 배정 실패 때 API의 `started_at`이 `created_at`과 같더라도, `runner_name`이 비어 있고 단계가 0개여서 실행되지 않은 것으로 분류했다. Ubuntu 재실행 기록 중 앞선 성공을 이어받아 시작 시각이 재실행 작업 생성 시각보다 이른 사례는 새 대기로 세지 않았다. [워크플로·작업 API][jobs-api]

**확인: 취소된 작업 전부(14건).** 링크는 해당 실행과 작업이다. `미배정` 11건은 모두 동일한 주석, `The job was not acquired by Runner of type hosted even after multiple attempts`(여러 번 요청해도 GitHub 서버를 배정받지 못함)가 있었다. Apple 칩 7건에는 Mac arm64 용량 부족으로 대기가 길어질 수 있다는 GitHub 안내도 붙었다. `새 요청으로 취소` 3건은 별개로, 같은 실행 그룹에 우선하는 새 요청이 있다는 주석이 있었다. [검사 주석 API][annotations-api]

| 실행 ID / 시도 | 이벤트 | 작업 / 라벨 | 생성 → 완료 (UTC) | 결과 |
|---|---|---|---|---|
| [43855638009 / 1](https://github.com/122yjs/skills-manage/actions/runs/43855638009/job/133259058972) | `pull_request` | Backend / `macos-15` | 11-16 11:12:09 → 11:27:09 | 미배정, 15:00 |
| [43855638009 / 2](https://github.com/122yjs/skills-manage/actions/runs/43855638009/job/133266995988) | `pull_request` | Backend / `macos-15` | 11-16 11:27:45 → 11:42:46 | 미배정, 15:01 |
| [43855638009 / 3](https://github.com/122yjs/skills-manage/actions/runs/43855638009/job/133274891674) | `pull_request` | Backend / `macos-15` | 11-16 11:43:31 → 11:58:31 | 미배정, 15:00 |
| [43855638009 / 4](https://github.com/122yjs/skills-manage/actions/runs/43855638009/job/133303646504) | `pull_request` | Backend / `macos-15` | 11-16 12:40:01 → 12:55:02 | 미배정, 15:01 |
| [43928365970 / 1](https://github.com/122yjs/skills-manage/actions/runs/43928365970/job/133513810549) | `pull_request` | Backend / `macos-15` | 11-16 18:23:53 → 18:38:53 | 미배정, 15:00 |
| [43928365970 / 2](https://github.com/122yjs/skills-manage/actions/runs/43928365970/job/133523590202) | `pull_request` | Backend / `macos-15` | 11-16 18:39:39 → 18:54:40 | 미배정, 15:01 |
| [43855638009 / 5](https://github.com/122yjs/skills-manage/actions/runs/43855638009/job/134949632802) | `pull_request` | Backend / `macos-15` | 11-18 16:21:46 → 16:36:47 | 미배정, 15:01 |
| [44351261739 / 1](https://github.com/122yjs/skills-manage/actions/runs/44351261739/job/134967210627) | `pull_request` | Backend / `macos-15-intel` | 11-18 16:47:21 → 17:02:22 | 미배정, 15:01 |
| [44357468717 / 1](https://github.com/122yjs/skills-manage/actions/runs/44357468717/job/134989385064) | `push` | Backend / `macos-15-intel` | 11-18 17:19:59 → 17:20:50 | 새 요청으로 취소, 서버 없음 |
| [44357468717 / 1](https://github.com/122yjs/skills-manage/actions/runs/44357468717/job/134989384270) | `push` | Frontend / `ubuntu-latest` | 11-18 17:19:59 → 17:21:02 | 새 요청으로 취소, 테스트 도중 |
| [44357599044 / 1](https://github.com/122yjs/skills-manage/actions/runs/44357599044/job/134989810956) | `pull_request` | Backend / `macos-15-intel` | 11-18 17:20:37 → 17:35:38 | 미배정, 15:01 |
| [44357638409 / 1](https://github.com/122yjs/skills-manage/actions/runs/44357638409/job/134990092809) | `push` | Backend / `macos-15-intel` | 11-18 17:21:03 → 17:36:04 | **미배정, 15:01. PR 아님** |
| [44358184389 / 1](https://github.com/122yjs/skills-manage/actions/runs/44358184389/job/134991903859) | `pull_request` | Backend / `macos-15-intel` | 11-18 17:23:43 → 17:38:44 | 미배정, 15:01 |
| [44361062536 / 1](https://github.com/122yjs/skills-manage/actions/runs/44361062536/job/135002249596) | `push` | Backend / `macos-15-intel` | 11-18 17:39:21 → 17:39:29 | 새 요청으로 취소, 서버 없음 |

**확인: 배정받은 Mac 작업(10건).** 가장 긴 대기는 27초였다. 조회 당시 대기 상태로 남은 작업은 없었다. 같은 시간대 Ubuntu 작업 중 새로 실행된 것은 **1~3초** 만에 배정됐으며, 위 Frontend 취소 한 건을 빼면 완료됐다. [작업 API][jobs-api]

| 실행 ID | 이벤트 / 라벨 | 작업 생성 → 완료 (UTC) | 배정 대기 |
|---|---|---|---:|
| [43733554289](https://github.com/122yjs/skills-manage/actions/runs/43733554289/job/132871083990) | `pull_request` / `macos-15` | 11-15 16:06:06 → 16:11:19 | 2초 |
| [43743552010](https://github.com/122yjs/skills-manage/actions/runs/43743552010/job/132901773140) | `push` / `macos-15` | 11-15 17:52:35 → 17:58:08 | 2초 |
| [43769097540](https://github.com/122yjs/skills-manage/actions/runs/43769097540/job/132980744115) | `pull_request` / `macos-15` | 11-15 22:24:14 → 22:26:51 | 2초 |
| [43769379243](https://github.com/122yjs/skills-manage/actions/runs/43769379243/job/132981640073) | `push` / `macos-15` | 11-15 22:27:21 → 22:30:03 | 2초 |
| [43770562867](https://github.com/122yjs/skills-manage/actions/runs/43770562867/job/132985308434) | `pull_request` / `macos-15` | 11-15 22:40:10 → 22:42:57 | 3초 |
| [43770847031](https://github.com/122yjs/skills-manage/actions/runs/43770847031/job/132986188842) | `push` / `macos-15` | 11-15 22:43:15 → 22:46:47 | 2초 |
| [43771558941](https://github.com/122yjs/skills-manage/actions/runs/43771558941/job/132988377208) | `pull_request` / `macos-15` | 11-15 22:51:01 → 22:53:32 | 2초 |
| [43777877544][arm-warm] | `pull_request` / `macos-15` | 11-16 00:00:57 → 00:05:02 | 2초 |
| [44354642420][intel-branch] | `workflow_dispatch` / `macos-15-intel` | 11-18 17:05:13 → 17:21:43 | 4초 |
| [44361080164][intel-main] | `workflow_dispatch` / `macos-15-intel` | 11-18 17:39:29 → 17:58:52 | 27초 |

11-18에는 [PR 44351261739](https://github.com/122yjs/skills-manage/actions/runs/44351261739)와 [기능 브랜치 수동 실행][intel-branch]의 PR 원본 커밋(`head_sha`)이 `c267711`로 같았는데 수동 실행만 서버를 받았다. PR은 임시로 합친 코드를 쓰므로 체크아웃 내용까지 같다는 뜻은 아니다. 수동 실행이 도는 도중에 다른 [PR 44357599044](https://github.com/122yjs/skills-manage/actions/runs/44357599044)가, 종료 직전에 [push 44357638409](https://github.com/122yjs/skills-manage/actions/runs/44357638409)가 대기에 들어가 모두 미배정으로 끝났다. [#48의 같은 시각 Ubuntu 검사](https://github.com/122yjs/skills-manage/actions/runs/44358184172)는 통과했다. **이벤트 종류에 따라 실패가 몰린 것은 관측됐지만 PR에만 실패가 생기는 규칙이나 수동 실행 우선권은 입증되지 않았다.** [GitHub 이벤트의 ref 규칙][events] · [작업 API][jobs-api]

### 원인 후보: 근거가 강한 순서

1. **서버 배정 단계에서 실패(확인), 순간적으로 Mac 서버가 부족하거나 배정 시스템에 문제가 있었을 가능성(추정).** 11건 모두 빈 서버 이름, 0개 단계, 미배정 주석이라는 증거가 일치한다. Apple 칩은 용량 부족 안내도 있으므로 공급 부족 근거가 더 강하다. Intel은 그 안내가 없어 남은 서버가 없었던 것인지, 배정 내부 오류인지 구분할 수 없다. GitHub도 [서버 사양 문서][runner-doc]에서 Mac 수요가 높으면 시작이 늦어진다고 안내한다. [커뮤니티 #211885][community-arm]의 Apple 칩, [#212069][community-intel]의 Apple 칩·Intel 신고에도 같은 문구가 있다. 둘은 다른 프로젝트의 경험담이며 이 저장소에서의 원인이나 15분 공통 제한을 확정하는 발표는 아니다. 작은 표본 안에서 수동 실행 일부와 Ubuntu가 빨랐다는 점도 전체 서비스가 계속 멈춘 상황과는 다르다.
2. **같은 그룹의 새 요청이 이전 작업을 취소(3건에 한해 확인).** GitHub 주석이 `ci-refs/heads/main` 2건과 `backend-refs/heads/main` 1건을 지목한다. 설정의 `cancel-in-progress: true`는 같은 그룹의 오래된 요청을 멈춘다. 기존 `CI`의 `main` 푸시 두 건끼리, 새 `Backend`의 `main` 푸시와 수동 실행이 각각 같은 그룹을 쓴다. PR의 `refs/pull/.../merge`와 수동 실행의 `refs/heads/...`는 다르다. 이 원인을 미배정 주석이 붙은 11건에까지 적용할 근거는 없다. [GitHub 동시 실행 그룹 문서][concurrency] · [이벤트 ref][events] · [기존 CI 설정][old-ci-src] · [현재 Backend 설정][backend-src]
3. **GitHub 공개 장애의 영향(일부 시각과 겹침, 직접 원인 미확인).** [11-16 공식 장애 기록][status-incident]은 11:59 UTC에 PR 문제를 처음 알렸으며, 15:33에 Actions 저하를 명시하고, 16:07에 완화, 16:26에 종료했다. 12:40의 취소 한 건은 장애 전체 시간 안에 있지만 Actions 첫 언급보다 앞선다. 다른 11-16 취소는 장애 보고 전이나 종료 후였고, [공식 사건 목록][status-api]에는 11-18 해당 시간대 Actions 장애가 없다(11-18 기록은 Copilot 모델 문제). 공개에 나오지 않은 배정 장애까지 없었다는 뜻은 아니다.
4. **플랜 동시 실행 한도(가능하지만 미확인).** [GitHub 한도][limits]에서 무료 플랜은 표준 서버 작업 20개, 그중 macOS 최대 5개 동시 실행이다. 공개 저장소의 표준 서버 무료 이용과 동시 실행 한도는 별개다([서버 사양][runner-doc]). 당시 계정 전체의 사용 슬롯은 조회하지 않았다. `workflow_dispatch`가 두 번 빨리 시작한 사실만으로 순간적으로 슬롯이 꽉 찬 적이 없었다고 말할 수 없고, 반대로 그 한도가 15분 취소 원인이라는 증거도 없다.
5. **포크 승인 또는 PR 전용 차단(근거 약함).** 저장소 자체는 공개 포크지만, 문제를 낸 PR [#43](https://github.com/122yjs/skills-manage/pull/43)·[#47](https://github.com/122yjs/skills-manage/pull/47)·[#48](https://github.com/122yjs/skills-manage/pull/48)은 모두 **이 저장소 내 브랜치**에서 열렸다(`head.repo.id == base.repo.id`). [외부 포크 승인 문서][fork-approval]가 설명하는 승인 대기와 맞지 않고, 동일 오류의 `push`도 있다. 저장소별 또는 이벤트별 배정 규칙은 공개 기록에서 확인되지 않았다. [이벤트 문서][events] · [저장소·PR API][pr-api]

## 5. 확인하지 못한 것과 다음 측정

- **미확인:** `main` Intel 캐시의 실제 재사용 시간, 캐시 상태와 코드를 맞춘 Apple 칩 대 Intel 비교, `rustc` 호출 횟수·옵션별 시간, Swift·링크만의 비용, Linux에서의 Rust 검사 시간과 Mac 전용 검사 대체 범위. 다른 저장소의 Intel 서버가 같은 시각에 35초 만에 배정됐다는 보고도 이 조사에서는 확인하지 않았다.
- **추가 측정 제안 1:** 다음 정상 PR과 `main` 실행에서 `Cache up-to-date` 또는 복원 키·`full match` 값, 대기·테스트 준비·Clippy·저장 시간을 기록한다. 이후 **동일 커밋·도구 버전·캐시 상태**를 맞춰야 Intel과 Apple 칩의 차이, 혹은 캐시 효과를 따로 비교할 수 있다.
- **추가 측정 제안 2:** 승인을 받은 별도 측정에서 `cargo test -vv --timings`와 `cargo clippy --all-targets -vv --timings`로 컴파일러 호출, `--test`·기능 조합·산출물 종류와 겹친 시간을 본다. 필요할 때만 디버그 정보와 `crate-type`을 하나씩 바꿔 비교한다. [Cargo 시간 기록 문서][timings] 이번 조사에서는 빌드·테스트 명령이나 CI 재실행을 하지 않았다.
- **추가 측정 제안 3:** 서버 미배정이 다시 생기면 실행 ID·시도 번호·라벨·이벤트·UTC 시각·주석을 모아 같은 시간대에 성공한 `push`·수동 실행과 비교한다. Intel도 계속 미배정이면 이 자료로 GitHub에 계정 한도 또는 서버 배정 사유를 확인한다.

### 원본을 다시 보는 명령

- `gh api repos/122yjs/skills-manage/actions/jobs/<job-id>/logs`: 위 세 작업의 타임스탬프가 붙은 원본 로그. 각 링크의 마지막 번호가 작업 ID다.
- `gh api --paginate 'repos/122yjs/skills-manage/actions/workflows/<ci.yml 또는 backend.yml>/runs?created=2026-11-15..2026-11-19&per_page=100'`: `ci.yml` 17개, `backend.yml` 3개 실행. `gh api --paginate 'repos/122yjs/skills-manage/actions/runs/<run-id>/jobs?filter=all&per_page=100'`로 이전 시도도 읽었다.
- `gh api repos/122yjs/skills-manage/check-runs/<job-id>/annotations`: Mac 서버 미배정 11건과 실행 그룹에 의한 취소 3건을 구분한 근거.
- `gh api repos/122yjs/skills-manage/actions/caches` 및 `gh api repos/122yjs/skills-manage/actions/cache/usage`: 브랜치별 캐시 9개와 총 3,973,789,591바이트. `gh api repos/122yjs/skills-manage/pulls/<번호>`: 위 PR의 원본과 대상 저장소가 같은지 확인.

[arm-warm]: https://github.com/122yjs/skills-manage/actions/runs/43777877544/job/133008203700
[intel-branch]: https://github.com/122yjs/skills-manage/actions/runs/44354642420/job/134979336931
[intel-main]: https://github.com/122yjs/skills-manage/actions/runs/44361080164/job/135002335983
[backend-src]: https://github.com/122yjs/skills-manage/blob/f1c1ba11353314e37f478cf6202643f9b241b98b/.github/workflows/backend.yml#L5-L56
[ci-src]: https://github.com/122yjs/skills-manage/blob/f1c1ba11353314e37f478cf6202643f9b241b98b/.github/workflows/ci.yml#L9-L53
[old-ci-src]: https://github.com/122yjs/skills-manage/blob/6347f22167deb15827009dadb34f26b59e9081e4/.github/workflows/ci.yml#L9-L82
[manifest]: ../../src-tauri/Cargo.toml#L9-L53
[app-main]: ../../src-tauri/src/main.rs#L4-L6
[build-src]: ../../src-tauri/build.rs#L1-L21
[translation]: ../../src-tauri/src/commands/on_device_translation.rs#L25-L94
[secrets]: ../../src-tauri/src/secrets.rs#L98-L146
[lock-keyring]: ../../src-tauri/Cargo.lock#L2423-L2433
[gha-cache]: https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching
[cache-api]: https://docs.github.com/en/rest/actions/cache
[jobs-api]: https://docs.github.com/en/rest/actions/workflow-jobs
[annotations-api]: https://docs.github.com/en/rest/checks/runs#list-check-run-annotations
[pr-api]: https://docs.github.com/en/rest/pulls/pulls#get-a-pull-request
[runner-doc]: https://docs.github.com/en/actions/reference/runners/github-hosted-runners#standard-github-hosted-runners-for-public-repositories
[limits]: https://docs.github.com/en/actions/reference/limits#job-concurrency-limits-for-github-hosted-runners
[events]: https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#pull_request
[fork-approval]: https://docs.github.com/en/actions/how-tos/manage-workflow-runs/approve-runs-from-forks
[concurrency]: https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency
[status-incident]: https://www.githubstatus.com/incidents/2zfqmghmg3s3
[status-api]: https://www.githubstatus.com/api/v2/incidents.json
[community-arm]: https://github.com/orgs/community/discussions/211885
[community-intel]: https://github.com/orgs/community/discussions/212069
[rust-cache-config]: https://github.com/Swatinem/rust-cache/blob/d1527c8f67ef9af357290475d4af4b0fab8f12e0/src/config.ts
[rust-cache-restore]: https://github.com/Swatinem/rust-cache/blob/d1527c8f67ef9af357290475d4af4b0fab8f12e0/src/restore.ts
[rust-cache-action]: https://github.com/Swatinem/rust-cache/blob/d1527c8f67ef9af357290475d4af4b0fab8f12e0/action.yml
[rust-cache-save]: https://github.com/Swatinem/rust-cache/blob/d1527c8f67ef9af357290475d4af4b0fab8f12e0/src/save.ts
[rust-cache-readme]: https://github.com/Swatinem/rust-cache/blob/d1527c8f67ef9af357290475d4af4b0fab8f12e0/README.md#cache-details
[sccache-gha]: https://github.com/mozilla/sccache/blob/main/docs/GHA.md
[sccache-rust]: https://github.com/mozilla/sccache/blob/main/docs/Rust.md
[cargo-queue]: https://github.com/rust-lang/cargo/blob/6d1bd93c/src/cargo/core/compiler/job_queue/mod.rs
[cargo-test]: https://doc.rust-lang.org/cargo/commands/cargo-test.html
[cargo-check]: https://doc.rust-lang.org/cargo/commands/cargo-check.html
[cargo-features]: https://doc.rust-lang.org/cargo/reference/features.html#feature-unification
[cargo-profile]: https://doc.rust-lang.org/cargo/reference/profiles.html
[cargo-env]: https://doc.rust-lang.org/cargo/reference/environment-variables.html
[cargo-performance]: https://doc.rust-lang.org/stable/cargo/guide/build-performance.html#reduce-amount-of-generated-debug-information
[clippy-src]: https://github.com/rust-lang/rust/blob/1.100.0/src/tools/clippy/src/main.rs
[linkage]: https://doc.rust-lang.org/reference/linkage.html
[timings]: https://doc.rust-lang.org/cargo/reference/timings.html
[tauri-prereq]: https://v2.tauri.app/start/prerequisites/#linux
[tauri-structure]: https://v2.tauri.app/start/project-structure/
[tauri-cli]: https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.10.1/crates/tauri-cli/src/interface/rust.rs
[tauri-7007]: https://github.com/tauri-apps/tauri/issues/7007
[tauri-7014]: https://github.com/tauri-apps/tauri/pull/7014
[keyring-features]: https://docs.rs/crate/keyring/4.2.0/source/Cargo.toml.orig
[keyring-v1]: https://docs.rs/crate/keyring/4.2.0/source/src/v1.rs
[secret-service]: https://docs.rs/zbus-secret-service-keyring-store/1.0.1/zbus_secret_service_keyring_store/#headless-usage
