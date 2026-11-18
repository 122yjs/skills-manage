# 휴대폰에서 skills-manage를 관리하는 안전한 구조 — 2026-11-19

조사 티켓 #54 (지도 이슈 #49). 기술 선택 비교(#53)와 코드 이전 범위(#52)는 다루지 않고, **접속 구조와 안전장치**만 다룬다.

## 한눈에 보기 (쉬운 설명)

- **첫 단계로 가장 알맞은 길은 "Paseo 대화로 Mac의 에이전트에게 맡기기"(C1)다.**
  - 휴대폰은 이미 쓰는 Paseo 앱으로 Mac에 붙는다.
  - 이 연결은 Paseo 릴레이(중계 서버)를 거치고, 종단 간 암호화(보내는 쪽과 받는 쪽만 내용을 읽는 암호화)를 쓴다.
  - 휴대폰에 Tailscale을 켤 필요가 없어서 "필요할 때만 켜고 싶다"는 선호와 맞는다.
- **다만 지금은 에이전트가 쓸 skills-manage 명령줄 도구(CLI, 글자로 명령을 내리는 프로그램)가 없다.**
  - 이대로 맡기면 에이전트가 파일을 직접 고친다. 그러면 "관리 설치만 지운다"(ADR-0002), 원자적 복사, 복구 백업 같은 보호를 모두 건너뛴다.
  - 그래서 먼저 **보기 전용 CLI**를 만들고, 기존 보호 코드를 거치는 쓰기 명령은 그다음에 하나씩 연다.
- **휴대폰 화면으로 직접 보고 싶다면 "Tailscale Serve + Mac의 읽기 전용 웹 서버"(A)가 다음 후보다.**
  - Tailscale Serve는 내 사설망 안에서만 HTTPS(암호화된 웹 주소)로 여는 기능이다.
  - iPhone의 Tailscale 앱은 `*.ts.net` 주소에 접속할 때만 VPN을 켜는 설정(VPN On Demand)을 공식 지원한다.
  - 다만 이 기능이 동작하지 않는다는 사용 경험과 버그 보고가 계속 있다. Android 앱에는 이 기능이 없다.
- **하지 말 것**
  - Tailscale Funnel이나 공개 터널로 인터넷에 여는 것
  - 개발용 Vite 서버를 그대로 바깥에 여는 것
  - 휴대폰으로 비밀값을 보내는 것
  - 휴대폰에서 확인 없이 바로 삭제하는 것
- **기본 전제는 "Mac이 대개 켜져 있다"이다.** 상시 가동을 지향하지만 보장되지는 않는다.
  - 그래서 휴대폰이 그때그때 Mac에 직접 묻는 구조로 충분하다.
  - Mac이 꺼졌거나 잠들었으면 휴대폰에 "Mac 오프라인"과 마지막 통신 시각을 보여 주고, 사용자가 다시 시도하게 한다.
  - 쓰기 요청은 쌓아 두지 않는다. 사설망 너머에서 Mac을 깨우는 간단한 방법도 없다.
- **항상 켜진 원격 서버를 중간 관문으로 쓰는 방법(F)은 지금은 이득이 작다.**
  - 스킬 파일은 각 기기에 있고 비밀값은 Mac 키체인에 있다. 그래서 관문을 거쳐도 결국 Mac이 켜져 있어야 Mac의 스킬을 다룰 수 있다.
  - 관문 서버는 새로 믿어야 하는 지점이 된다. 그 서버가 뚫리면 Mac에도 명령이 갈 수 있다.
  - 이 서버는 "자기 스킬을 가진 또 하나의 기기"로 보는 편이 낫다.
- **여러 기기(Mac, 원격 서버, 추가 PC)로 넓히기는 C1이 가장 쉽다.** 기기마다 Paseo 데몬과 같은 CLI를 두면 휴대폰 앱 하나로 여러 기기를 오갈 수 있다. 여러 기기를 관리할지는 아직 정하지 않았으므로 "확장 고려"로만 다룬다(2-1절).

## 전제

앞의 네 줄(Mac 가동, 원격 서버, 추가 PC, 여러 기기 관리 여부)은 사용자가 2026-11-19에 직접 말한 내용이다.

- Mac은 상시 가동을 지향하고 실제로 거의 항상 켜져 있다. 다만 보장은 아니다.
- Mac과 별도로 **항상 켜진 원격 서버**가 한 대 있다.
- 앞으로 **추가 PC**를 한 대 들일 예정이다. 시기와 모델은 정하지 않았다.
- 여러 기기의 스킬을 휴대폰에서 함께 관리할지는 아직 정하지 않았다.
- 스킬 원본과 설치 폴더는 각 기기에 있다. skills-manage의 비밀값은 Mac 키체인에 있다.
- 휴대폰에서 Tailscale을 항상 켜 두는 구조는 원하지 않는다. 과거 대화에서 나온 선호이며 사용자 확인 전이다.

## 확인 범위와 표시 규칙

- 기준 코드: `origin/main` f1c1ba1.
- 표시 규칙
  - **[사실]**: 공식 문서, 소스 코드, 실행 결과로 확인했다.
  - **[추정]**: 확인한 사실에서 이끌어 낸 판단이다.
  - **[경험담]**: 사용자 글이나 GitHub 이슈에 나온 보고다.
- 실행한 명령은 모두 조회용이다.
  - Paseo CLI(`/Applications/Paseo.app/Contents/Resources/bin/paseo`): `--version`, `--help`, `permit --help`, `plugin --help`, `daemon --help`, `host --help`, `daemon status --json`(값을 가린 채 항목 이름만 확인)을 실행했다.
  - `~/.paseo/config.json`에서는 릴레이 켜짐 여부와 플러그인 스위치만 읽었다.
  - Tailscale은 설치된 앱의 종류만 확인했다. 앱 폴더의 영수증 파일로 Mac App Store판임을 알았다.
  - `man caffeinate`를 읽었다.
- 하지 않은 일
  - Tailscale 설정을 바꾸는 명령(`serve`, `funnel`, `up`, `down`, `set`)은 실행하지 않았다.
  - 웹 서버를 띄우거나 포트를 열지 않았다.
  - 휴대폰에서 직접 시험하지 않았다.
- Reddit은 검색 결과의 본문 일부만 읽었다. 개별 글을 열려고 하니 차단 페이지가 나와서 더 시도하지 않았다.

## 1. 지금 코드에서 출발점이 되는 것

| 확인한 내용 | 근거 | 표시 |
|---|---|---|
| 웹 대시보드는 DB를 읽기 전용으로 연다 | `web-dashboard/snapshot.ts:397` (`readOnly: true`) | [사실] |
| 대시보드 API는 `GET /api/dashboard` 하나다. 다른 방식의 요청에는 405 오류를 준다 | `vite.config.ts:18-31` | [사실] |
| 대시보드는 `127.0.0.1`에만 묶인 **Vite 개발 서버**다 | `vite.config.ts:68-71`, `README.md:201-212` | [사실] |
| 대시보드는 비밀값을 읽지 않고 설정 여부만 보낸다 | `web-dashboard/snapshot.ts:171-177`, `:658`, `:669-671` | [사실] |
| 대시보드 응답에 **홈 폴더가 든 절대 경로**(DB 경로, 설치 경로 등)가 들어간다 | `web-dashboard/snapshot.ts:649-657` | [사실] |
| 원격으로 보내면 계정 이름이 든 경로가 휴대폰까지 간다 | 위 사실에서 이끌어 냄 | [추정] |
| 비밀값은 OS 키체인에만 두고, 화면에는 `{configured, keychainAvailable}`만 준다 | `src-tauri/src/secrets.rs:1-19`, `src/types/index.ts:16-19`, ADR-0003 | [사실] |
| 데스크톱 앱 운영 코드에는 HTTP 서버가 없다. `TcpListener`는 테스트 코드에만 있다 | `src-tauri/Cargo.toml` 의존성, `src-tauri/src/commands/marketplace.rs:2722`(`#[cfg(test)]`) 이후 | [사실] |
| skills-manage CLI는 없다. 실행 파일은 앱을 띄우기만 한다 | `src-tauri/src/main.rs:4-6`, `src-tauri/src/bin` 없음 | [사실] |
| 관리 설치가 아니면 바꾸지 않는 검사가 있다 | `src-tauri/src/db.rs:2383-2403` (`ensure_installation_managed`) | [사실] |
| 관리 설치 삭제는 공용 설치와 연결 링크를 먼저 확인한다 | `src-tauri/src/commands/usage.rs:688-726` | [사실] |
| 복사 설치를 지우기 전에 백업한다 | `src-tauri/src/commands/recovery.rs:845-874` | [사실] |
| 복사는 임시 폴더에서 완성한 뒤 이름 바꾸기로 한 번에 확정한다(원자적 복사) | `src-tauri/src/commands/linker.rs:150-189` (`copy_dir_all`) | [사실] |
| 파일 읽기는 스킬 폴더 안으로 제한한다 | `src-tauri/src/commands/skills.rs:1544-1579` (`read_skill_file_impl`) | [사실] |
| 복원과 영구 삭제가 겹치지 않게 막는 잠금은 **같은 프로세스 안에서만** 동작한다 | `src-tauri/src/commands/recovery.rs:51-54` | [사실] |
| 그래서 CLI나 웹 서버 같은 별도 프로세스가 쓰기를 하면, 데스크톱 앱과 동시에 실행될 때 이 잠금이 지켜 주지 못한다 | 위 사실에서 이끌어 냄 | [추정] |
| 누가 언제 무엇을 바꿨는지 남기는 작업 기록(audit log) 테이블은 없다 | `src-tauri/src/db.rs`에서 `audit`, `history`, `operation_log`, `activity` 검색 결과 없음 | [사실] |

정리하면 보호 장치는 이미 Rust 함수 안에 있다. 휴대폰 경로는 이 함수를 **반드시 거치게** 만들어야 한다. 파일을 직접 다루는 지름길을 만들면 안 된다.

## 2. 선택지 비교표

| 선택지 | 접속 방식 | 누가 들어올 수 있나 | 휴대폰 Tailscale | 초보자 설정 | 비밀값 | 주요 새 위험 | Mac이 잠들면 | 판단 |
|---|---|---|---|---|---|---|---|---|
| **A. Tailscale Serve + 로컬 웹 서버** | 사설망 안에서만 HTTPS. Serve가 Mac의 localhost로 넘긴다 | 사설망의 기기. 기본 정책은 모든 기기 허용이라 정책으로 좁혀야 한다 | 필요. VPN On Demand로 `*.ts.net` 접속 때만 켤 수 있지만 버그 보고가 있다 | 중간 | Mac에 남는다. 서버 응답 설계에 달렸다 | CSRF, DNS rebinding, 사설망의 다른 기기, 인증서 공개 기록에 기기 이름이 남음 | 접속 안 됨 | 보기 전용 화면의 다음 후보 |
| **B. Tailscale Funnel** | 공개 인터넷에 노출 | 주소를 아는 누구나. Funnel 문서에 방문자 인증이 없다 | 불필요 | 쉬움(그래서 위험) | 앱이 직접 막아야 한다 | 공개 스캔, 로그인 직접 구현 | 접속 안 됨 | 쓰지 않는다 |
| **C1. Paseo 대화 → 에이전트 → CLI** | Paseo 릴레이(종단 간 암호화) 또는 직접 연결 | 페어링한 기기. QR 코드가 열쇠 역할이다 | 불필요 | 쉬움(이미 쓰는 중) | Mac에 남는다. CLI가 비밀값을 출력하지 않아야 한다 | 에이전트의 넓은 권한, 스킬 본문을 통한 프롬프트 주입, CLI 미구현 | 접속 안 됨 | **1단계 추천** (CLI 선행) |
| **C2. Paseo 플러그인 화면** | C1과 같은 연결. 플러그인 화면은 휴대폰에도 보인다 | C1과 같다 | 불필요 | 어려움(TypeScript와 React Native) | Mac의 플러그인 서버 코드에 남는다 | 샌드박스 없는 신뢰 코드, 별도 프로세스의 동시 실행 | 접속 안 됨 | 나중 후보 |
| **D. Cloudflare Tunnel + Access** | Mac에서 Cloudflare로 나가는 연결만 쓴다. 공개 주소가 생긴다 | Access 허용 정책에 맞는 로그인 사용자 | 불필요 | 어려움(도메인, 계정, 정책) | Cloudflare에서 TLS가 한 번 끝난다 | 공개 주소, 설정 실수가 곧 노출, Access 우회 직접 접근 | 오류 페이지 | 초보자에게 비추천 |
| **E. 휴대폰 SSH 앱 + CLI** | 사설망 위의 SSH(원격 셸) | SSH 키가 있는 사용자 | 필요. SSH 앱이 VPN On Demand를 못 깨울 수 있다 | 중간 | Mac에 남는다 | 셸 전체 권한, 작은 화면의 오타, 휴대폰 분실 시 피해 범위 | 접속 안 됨 | 초보자에게 비추천 |
| **F. 항상 켜진 원격 서버를 중계·관문으로** | 휴대폰 → 원격 서버 → 사설망 또는 Paseo → Mac | 원격 서버의 로그인 방식에 달렸다 | 서버를 사설망에만 두면 필요, 공개로 두면 불필요 | 어려움(서버 운영이 더해짐) | 비밀값은 Mac에 남지만, 서버가 오가는 내용을 풀어 보는 위치가 된다 | 새로 믿어야 하는 지점, 공개 시 B·D와 같은 위험 | Mac의 스킬은 다룰 수 없다. 마지막 상태 사본만 보여 줄 수 있다 | 지금은 쓰지 않는다 |

### 2-1. 여러 기기로 넓히기 (확장 고려)

여러 기기를 관리할지는 아직 정하지 않았다. 아래는 나중에 넓힐 때 얼마나 쉬운지만 한 줄로 적는다. Mac, 항상 켜진 원격 서버, 추가 PC가 대상이다.

| 선택지 | 여러 기기로 넓힐 때 |
|---|---|
| A | 기기마다 읽기 전용 서버와 Serve 주소를 두고, 사설망 정책도 기기마다 쓴다. 한 화면에 모으려면 따로 모음 기능이 필요하다 [추정] |
| B | 기기마다 공개 주소가 하나씩 늘어 위험도 그만큼 는다. 넓힐 대상이 아니다 [추정] |
| C1 | 기기마다 Paseo 데몬과 같은 CLI를 두면 휴대폰 앱 하나에서 호스트를 바꿔 가며 쓴다. Paseo는 여러 기기에서 에이전트를 돌리는 구성을 공식 지원한다 [사실: [README](https://github.com/getpaseo/paseo)]. 가장 넓히기 쉽다 [추정] |
| C2 | 플러그인은 데몬마다 설치한다. 같은 화면이 여러 호스트에 있으면 하나만 보이고 호스트 선택기가 붙는다. 고른 호스트가 꺼져 있으면 다른 호스트로 넘어가지 않는다 [사실: [플러그인 참고](https://paseo.sh/docs/plugins/reference.md)]. 여러 기기를 한 화면에 합치려면 플러그인 코드가 직접 모아야 한다 [추정] |
| D | 기기마다 터널과 Access 설정을 더해야 하고, 공개 주소가 는다 [추정] |
| E | 기기마다 SSH 접속 정보를 더하면 되지만, 셸 전체 권한도 기기 수만큼 는다 [추정] |
| F | 원격 서버가 여러 기기의 읽기 전용 결과를 모으는 "모음 화면"이 될 수 있다. 그러나 모든 기기에 닿는 권한이 한 서버에 모여, 그 서버가 뚫리면 모든 기기가 영향을 받는다. 모은다면 읽기 전용 사본만 모은다 [추정] |

## 3. 선택지별 상세

### A. 사설망(Tailscale Serve) + Mac의 로컬 웹 서버

- **접속**
  - [사실] Serve는 사설망(tailnet, 내 계정에 묶인 기기끼리만 통하는 망) 안에서만 보인다. 인터넷에 여는 기능은 따로 있는 Funnel이다. HTTPS 인증서도 발급해 준다. ([Serve](https://tailscale.com/kb/1312/serve))
  - [사실] `tailscale serve 3000` 같은 예는 `http://127.0.0.1:3000`으로 넘긴다. ([Serve](https://tailscale.com/kb/1312/serve))
- **인증**
  - [사실] 사설망의 기본 정책은 모든 기기 사이의 통신을 허용한다. 규칙을 쓰면 기본 거부로 바뀐다. ([ACL](https://tailscale.com/kb/1018/acls))
  - [추정] 그래서 정책으로 "휴대폰 → Mac의 Serve 포트" 하나만 열어야 한다. 그래야 같은 계정의 다른 기기가 기본으로 접속하지 못한다.
  - [사실] Serve는 백엔드로 `Tailscale-User-Login` 같은 신원 헤더(요청에 붙는 "누가 보냈는지" 표시)를 붙인다. 요청에 이미 같은 헤더가 있으면 위조를 막으려고 지운다. 태그 기기에서 온 요청에는 붙이지 않는다. ([Serve](https://tailscale.com/kb/1312/serve))
  - [사실] 같은 문서는 백엔드가 localhost에서만 듣게 하라고 경고한다. 그렇지 않으면 직접 접속한 쪽이 헤더를 마음대로 넣을 수 있다. ([Serve](https://tailscale.com/kb/1312/serve))
  - [추정] 서버는 신원 헤더가 내 계정일 때만 응답하게 만들 수 있다. 다만 Mac 안의 다른 프로그램은 여전히 localhost로 들어올 수 있다. 그래서 이 헤더는 여러 겹 중 한 겹으로만 쓴다.
- **휴대폰에서 필요할 때만 켜기**
  - [사실] iOS Tailscale 1.48부터 VPN On Demand가 있다. ([VPN On Demand](https://tailscale.com/kb/1291/ios-vpn-on-demand))
  - [사실] 같은 문서에 따르면 `*.ts.net`으로 끝나는 주소에 접속하면 자동으로 연결하는 규칙이 있다. 이 규칙은 Wi-Fi나 셀룰러 규칙이 "Do Nothing"일 때만 쓸 수 있다. 앱이 켜져 있는 동안은 넓은 On Demand 정책을 자동으로 건다. ([VPN On Demand](https://tailscale.com/kb/1291/ios-vpn-on-demand))
  - [사실] 자동으로 **꺼지는** 시점은 문서에 없다(미확인).
  - [경험담] `*.ts.net` 자동 연결이 iOS 18.1에서 동작하지 않았다는 보고가 있다. ([Reddit 2024-10](https://www.reddit.com/r/Tailscale/comments/1gfsoqr/vpn_on_demand_magicdns_trigger_not_working_on_ios/))
  - [경험담] 앱을 한 번 열어야 규칙이 적용된다는 보고가 있다. ([Reddit 2026-07](https://www.reddit.com/r/Tailscale/comments/1un90t1/vpn_in_demand_ios/))
  - [경험담] MagicDNS를 끄면 On Demand가 정상 동작했다는 보고가 있다. ([tailscale#20755](https://github.com/tailscale/tailscale/issues/20755))
  - [경험담] 주소를 직접 해석하는 앱은 자동 연결을 깨우지 못할 수 있다고 Tailscale iOS 개발자가 답했다. ([Reddit 2023-08](https://www.reddit.com/r/Tailscale/comments/16026mt/ios_vpn_on_demand_question/))
  - [사실] Android에는 VPN On Demand가 없고 기능 요청 이슈가 열려 있다. ([tailscale#12086](https://github.com/tailscale/tailscale/issues/12086))
  - [추정] 초보자에게 가장 확실한 방법은 쓸 때 앱에서 직접 켜고, 다 쓰면 끄는 것이다.
- **Mac 쪽에서 필요할 때만 열기**
  - [사실] `--bg`로 켠 Serve는 끌 때까지 남고, 재부팅 뒤에도 다시 살아난다. `--bg` 없이 켜면 재시작 때 꺼진다. 끌 때는 켤 때 쓴 명령 끝에 `off`를 붙이거나 `tailscale serve reset`을 쓴다. ([Serve CLI](https://tailscale.com/kb/1242/tailscale-serve))
- **새 위험**
  - [사실] HTTPS를 켜면 기기의 전체 주소 이름이 인증서 공개 기록(Certificate Transparency, 발급된 인증서를 누구나 볼 수 있게 남기는 장부)에 남는다. 이름에 민감한 정보를 넣지 말라고 한다. ([HTTPS](https://tailscale.com/kb/1153/enabling-https))
  - [사실] Vite 개발 서버는 기본으로 `localhost`, `.localhost`, IP 주소만 허용한다. `allowedHosts: true`로 풀면 DNS rebinding(악성 사이트가 자기 주소를 내 Mac으로 바꿔 가리켜 요청을 보내는 공격)에 열린다고 경고한다. ([Vite server options](https://vite.dev/config/server-options))
  - [사실] Vite는 `vite preview`도 운영 서버로 쓰지 말라고 한다. ([Vite CLI](https://vite.dev/guide/cli))
  - [추정] 개발 서버를 바깥에 여는 것보다 기능이 적은 별도 읽기 전용 서버가 낫다. 어떤 기술로 만들지는 #53이 정한다.
- **설치된 앱 종류**
  - [사실] 이 Mac의 Tailscale은 Mac App Store판이다.
  - 이 앱 종류에서 `serve`를 쓰는 정확한 절차는 확인하지 않았다(미확인).

### B. Tailscale Funnel(인터넷 공개) — 위험 평가

- [사실] Funnel은 로컬 서비스를 공개 인터넷에 연다. 443, 8443, 10000 포트만 쓸 수 있고, 아직 베타다. ([Funnel](https://tailscale.com/kb/1223/funnel))
- [사실] TLS(암호화 연결)는 내 기기에서 끝나고, Funnel 중계 서버는 내용을 풀지 않는다. ([Funnel](https://tailscale.com/kb/1223/funnel))
- [사실] Funnel 문서에는 공개 방문자를 위한 인증이나 신원 헤더가 없다. ([Funnel](https://tailscale.com/kb/1223/funnel))
- [추정] 그래서 로그인, 속도 제한, 토큰 관리를 앱이 모두 직접 만들어야 한다.
- [경험담] Funnel로 연 서비스가 곧바로 해외 IP의 스캔을 받았다는 보고가 있다. 댓글에는 인증 없는 공개가 기본값이라는 비판이 있다. ([Hacker News](https://news.ycombinator.com/item?id=39902737))
- [추정] 위 공개 기록에 이름이 남는 점까지 생각하면 주소를 숨겨서 지킬 수 없다.
- **판단**: 이 프로젝트에서는 쓰지 않는다. 키체인 비밀값과 사용자 파일을 다루는 앱을 인터넷에 열 이유가 없다.

### C1. Paseo 대화로 Mac의 에이전트에게 맡기기

- **접속과 보호**
  - [사실] Paseo는 데몬(Mac에서 계속 도는 관리 프로그램)과 클라이언트 구조다. 릴레이를 쓰면 데몬이 바깥으로 나가는 연결만 만들고, 포트를 열지 않는다. ([Paseo 보안](https://paseo.sh/docs/security.md))
  - [사실] 휴대폰과 데몬 사이는 종단 간 암호화된다(Curve25519 키 교환, NaCl `box`/XSalsa20-Poly1305). 릴레이가 보는 것은 IP 주소, 시간, 메시지 크기, 세션 ID뿐이다. ([Paseo 보안](https://paseo.sh/docs/security.md))
  - [사실] 릴레이는 Tailscale, 포트 포워딩, 네트워크 설정 없이 동작한다. ([Paseo 연결](https://paseo.sh/docs/connectivity.md))
  - [사실] 이 Mac의 Paseo 설정에서는 릴레이가 켜져 있다. `~/.paseo/config.json`의 `daemon.relay.enabled` 값을 읽어 확인했다.
  - [추정] 그래서 휴대폰 Tailscale을 항상 켜 둘 필요가 없다.
- **인증**
  - [사실] QR 코드나 페어링 링크가 신뢰의 기준이다. 비밀번호처럼 다뤄야 하고, 비밀번호를 따로 정하지 않으면 링크를 가진 누구나 접속한다. 비밀번호를 정하면 릴레이 클라이언트도 암호화된 채널 안에서 보내야 한다. ([Paseo 보안](https://paseo.sh/docs/security.md))
  - [사실] 페어링한 기기를 하나씩 해제하는 방법은 공개 문서와 CLI 도움말에서 찾지 못했다. 기기 목록과 일회용 페어링을 도입하자는 설계 이슈가 열려 있다. ([paseo#416](https://github.com/getpaseo/paseo/issues/416))
  - [추정] 휴대폰을 잃어버렸을 때 Paseo만으로 그 기기를 바로 끊는 수단은 확인되지 않았다. 그래서 쓰기 작업에는 아래의 추가 확인이 필요하다.
- **동작과 승인**
  - [사실] Paseo 에이전트는 Mac 사용자 권한과 기존 자격 증명으로 돈다. ([Paseo 보안](https://paseo.sh/docs/security.md))
  - [사실] 에이전트의 권한 요청은 Paseo에서 처리할 수 있다. CLI에는 `paseo permit ls/allow/deny`가 있다. 공식 소개는 "어디서든 승인에 답한다"고 적는다. ([Paseo CLI](https://paseo.sh/docs/cli.md), [Paseo 소개 색인](https://paseo.sh/llms.txt))
  - [사실] Claude Code의 권한 규칙은 거부 → 확인 → 허용 순서로 판정한다. 명령 인자를 제한하는 Bash 규칙은 깨지기 쉽다고 공식 문서가 경고한다. ([Claude Code 권한](https://code.claude.com/docs/en/permissions))
  - [추정] 에이전트 권한 확인은 "실수 방지" 장치일 뿐 보안 경계가 아니다. 진짜 보호는 CLI 안에 두어야 한다.
- **지금의 빈틈**
  - [사실] skills-manage CLI가 없다. (`src-tauri/src/main.rs:4-6`)
  - [추정] 지금 맡기면 에이전트가 스킬 폴더나 DB를 직접 고친다. 그러면 위 1절의 보호 함수(관리 설치 판정, 백업, 원자적 복사)가 호출되지 않는다.
- **새 위험: 프롬프트 주입**
  - [사실] 간접 프롬프트 주입은 모델이 웹이나 파일 같은 외부 입력을 받을 때 생긴다. 권한을 최소로 하고, 위험한 작업에는 사람의 승인을 두라고 권한다. ([OWASP LLM01](https://genai.owasp.org/llmrisk/llm01-prompt-injection/))
  - [추정] 마켓플레이스나 GitHub에서 받은 `SKILL.md`는 외부 입력이다. 에이전트가 스킬 본문을 읽다가 그 안의 지시를 따를 수 있다.
- **경험담**
  - [경험담] 휴대폰 앱이 백그라운드에서 돌아온 뒤 다시 연결되기까지 약 1분이 걸린다는 버그 보고가 있다. ([paseo#8826](https://github.com/getpaseo/paseo/issues/8826))
- **설정 난이도**
  - [추정] 이미 쓰는 앱이라 새로 배울 것이 가장 적다. 남은 일은 CLI 준비뿐이다(#52).

### C2. Paseo 플러그인으로 화면·패널 추가

- **휴대폰에서 보이나**
  - [사실] 플러그인의 클라이언트 기능은 그 데몬에 연결된 모든 Paseo 클라이언트에서 동작하고, 휴대폰도 포함된다. ([플러그인 시작](https://paseo.sh/docs/plugins.md))
  - [사실] 화면은 React Native로 그리고, iOS·Android·브라우저에서 돈다. ([플러그인 참고](https://paseo.sh/docs/plugins/reference.md))
- **신뢰 모델**
  - [사실] 플러그인은 신뢰하는 코드이고 샌드박스(격리 실행)가 없다. 서버 코드는 데몬 사용자의 권한으로 파일, 프로세스, 자격 증명, 네트워크에 접근한다. ([플러그인 시작](https://paseo.sh/docs/plugins.md), [플러그인 참고](https://paseo.sh/docs/plugins/reference.md))
  - [사실] 전역 스위치 `pluginsEnabled`가 있다. 이 Mac에서는 켜져 있다(설정 조회로 확인).
- **연결 보호**
  - [추정] 플러그인 화면과 RPC(앱이 Mac 쪽 함수를 부르는 통로)는 Paseo의 기존 연결을 쓰므로 C1과 같은 보호를 받는다.
  - [사실] 공식 문서는 RPC 입력과 출력을 양쪽에서 검증하고, 자격 증명과 파일 접근은 서버 핸들러에 두라고 한다. ([플러그인 참고](https://paseo.sh/docs/plugins/reference.md))
- **새 위험**
  - [추정] 플러그인 서버 코드는 데스크톱 앱과 다른 프로세스다. 그래서 1절의 같은 프로세스 잠금을 공유하지 못한다.
  - [추정] 쓰기 기능을 넣으려면 프로세스 사이의 잠금이 필요하다.
- **경험담**
  - [경험담] 플러그인 RPC가 시간 초과될 때 원인을 알기 어렵다는 보고가 있다. ([paseo#10070](https://github.com/getpaseo/paseo/issues/10070))
  - [경험담] 플러그인 권한 훅보다 알림이 먼저 간다는 보고가 있다. ([paseo#9219](https://github.com/getpaseo/paseo/issues/9219))
- **판단**
  - [추정] 화면 경험은 가장 좋다. 다만 초보자가 TypeScript와 React Native 플러그인을 유지해야 하고, 결과가 Paseo에 묶인다.
  - [추정] CLI가 안정된 뒤 "CLI를 부르는 얇은 화면"으로 검토할 후보다.

### D. 터널 서비스 + 로그인 관문 (Cloudflare Tunnel + Access)

- **접속**
  - [사실] `cloudflared`가 Mac에서 Cloudflare로 나가는 연결만 만든다. ([Cloudflare Tunnel](https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/))
  - [사실] 공개 앱으로 내려면 Cloudflare 계정과 Cloudflare에 올린 도메인이 필요하다. ([Tunnel 설정](https://developers.cloudflare.com/tunnel/setup/))
- **인증**
  - [사실] Access 앱은 기본 거부이고, 허용 정책에 맞는 사용자만 들어간다. ([Access 자체 호스팅 앱](https://developers.cloudflare.com/cloudflare-one/access-controls/applications/http-apps/self-hosted-public-app/))
  - [사실] Access를 거치지 않은 요청을 막으려면 원 서버가 Access 토큰을 검증하거나, Tunnel의 "Protect with Access"를 켜야 한다. ([Access 자체 호스팅 앱](https://developers.cloudflare.com/cloudflare-one/access-controls/applications/http-apps/self-hosted-public-app/))
  - [사실] 빠른 터널(Quick Tunnel)은 테스트용이고, URL을 아는 누구나 접속한다. ([Quick Tunnels](https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/do-more-with-tunnels/trycloudflare/))
- **비밀값과 내용 노출**
  - [사실] Cloudflare는 "방문자 ↔ Cloudflare"와 "Cloudflare ↔ 원 서버" 두 연결을 따로 관리한다. ([SSL 모드](https://developers.cloudflare.com/ssl/origin-configuration/ssl-modes/))
  - [사실] Paseo 문서도 Cloudflare가 TLS를 끝낸다고 적는다. ([Paseo 웹 UI](https://paseo.sh/docs/web-ui.md))
  - [추정] 따라서 Cloudflare는 풀린 요청 내용을 볼 수 있는 위치에 있다.
- **판단**
  - [추정] 휴대폰 VPN이 필요 없다는 장점은 C1도 이미 준다. 공개 주소, 도메인, 로그인 정책 관리가 더해져 초보자에게 맞지 않다.

### E. 휴대폰 SSH 앱 + 명령줄 도구

- **접속**
  - [사실] macOS 원격 로그인은 SSH와 SFTP 접속을 허용하고, 허용할 사용자를 고를 수 있다. ([Apple](https://support.apple.com/guide/mac-help/allow-a-remote-computer-to-access-your-mac-mchlp1066/mac))
- **Tailscale SSH**
  - [사실] Tailscale SSH의 macOS 서버 기능은 오픈소스 `tailscaled` 변형에서만 된다. "check" 규칙은 접속 전에 다시 로그인하게 하며, 기본 유효 시간은 12시간이다. ([Tailscale SSH](https://tailscale.com/kb/1193/tailscale-ssh))
  - [추정] 이 Mac은 App Store판이라 Tailscale SSH 서버를 그대로 쓰지 못하고, macOS 기본 SSH를 사설망 위에서 써야 한다.
- **새 위험**
  - [추정] SSH는 셸 전체 권한이다. 잃어버린 휴대폰의 SSH 키는 Mac 전체 접근과 같다. 작은 화면에서 오타 하나가 큰 삭제로 이어질 수 있다.
  - [추정] 위의 VPN On Demand 경험담처럼 SSH 앱이 자동 연결을 깨우지 못할 수도 있다.
  - [추정] CLI가 없다는 점은 C1과 같다.
- **판단**: 초보자에게 권하지 않는다. C1이 같은 CLI를 더 좁은 권한과 승인 흐름으로 쓰게 해 준다.

### F. 항상 켜진 원격 서버를 중계·관문으로 쓰기

원격 서버는 늘 켜져 있으니 휴대폰이 늘 접속할 곳으로 쓰자는 발상이다. 두 가지 사실을 함께 놓고 본다. 스킬 파일은 각 기기에 있고, 비밀값은 Mac 키체인에 있다.

- **F1. 원격 서버에 웹 관문을 두고 Mac으로 넘기기**
  - [추정] Mac의 스킬을 보거나 바꾸려면 결국 Mac이 켜져 있어야 한다. 관문이 늘 켜져 있어도 Mac이 꺼지면 할 수 있는 일은 같다.
  - [추정] 관문을 공개 주소로 두면 B·D와 같은 공개 노출 위험이 생긴다. 사설망에만 두면 휴대폰 Tailscale이 필요하다. 그러면 A에 거쳐 가는 단계 하나만 더해진다.
  - [추정] 관문은 오가는 요청을 풀어서 다시 보낸다. 그래서 서버가 내용을 보고, 서버가 뚫리면 Mac에 명령을 보낼 수 있다. 지금 Paseo 릴레이는 믿지 않아도 되게 설계되어 있다([Paseo 보안](https://paseo.sh/docs/security.md)). 직접 만든 관문은 그 성질을 잃는다.
  - [추정] 관문이 Mac의 읽기 전용 사본(마지막 상태)을 들고 있으면 Mac이 꺼져 있어도 "마지막으로 본 목록"은 보여 줄 수 있다. 그 대신 스킬 목록과 경로가 서버에 남는다. 비밀값과 홈 폴더 경로는 빼고, 오래된 정보라는 표시를 붙여야 한다.
  - 비밀값은 원격 서버로 복사하지 않는다(ADR-0003). 원격 서버가 GitHub 같은 외부 서비스를 써야 한다면, 그 서버만의 권한 좁은 토큰을 따로 둔다 [추정].
- **F2. 원격 서버에서 Paseo 릴레이를 직접 운영하기**
  - [사실] 공식 릴레이는 공개 소스 프로젝트이고, 종단 간 암호화 덕분에 릴레이 자체를 믿지 않아도 되는 구조다. ([Paseo 보안](https://paseo.sh/docs/security.md), [paseo-relay](https://github.com/getpaseo/paseo-relay))
  - [사실] 다만 이 릴레이 프로젝트는 아직 활발히 개발 중이고 내부 프로토콜이 예고 없이 바뀔 수 있다고 적혀 있다. ([paseo-relay](https://github.com/getpaseo/paseo-relay))
  - 데몬이 직접 운영 릴레이를 가리키게 하는 설정 이름은 공개 문서에서 찾지 못했다(미확인). 설정 문서는 "relay endpoints"가 재시작이 필요한 설정이라고만 적는다. ([Paseo 설정](https://paseo.sh/docs/configuration.md))
  - [추정] 얻는 것은 접속 기록(IP, 시간)을 자기 서버에 둔다는 정도다. 운영 부담에 비해 초보자에게 이득이 작다.
- **판단**
  - [추정] 지금은 관문으로 쓰지 않는다.
  - [추정] 원격 서버는 "자기 스킬을 가진 또 하나의 기기"로 다루는 편이 낫다. 여러 기기 관리를 정하면 C1처럼 그 서버에도 Paseo 데몬과 CLI를 두는 방식을 검토한다(2-1절).

## 4. Mac이 꺼졌거나 잠들었을 때 (예외 상황)

Mac은 대개 켜져 있다. 그래서 휴대폰이 그때그때 Mac에 묻는 구조를 기본으로 두고, 꺼진 경우는 예외로 처리한다.

- **무슨 일이 생기나**
  - [경험담] Mac이 잠들면 네트워크가 멈추고 Tailscale 연결도 끊긴다는 보고가 있다. 개인 위키 글이고 검색 요약으로만 봤으므로 일반화하지 않는다. ([커뮤니티 위키](https://git.majorshouse.com/majorlinux/MajorWiki/src/branch/main/05-troubleshooting/ollama-macos-sleep-tailscale-disconnect.md))
  - [추정] A~F 모두 그동안 Mac의 스킬을 다루지 못한다. F의 관문도 마지막 상태 사본만 보여 줄 수 있다.
- **안내**
  - [사실] Paseo 플러그인 API는 꺼진 호스트에 "Paseo host is disconnected" 오류를 내고, 다시 연결되면 재시도하라고 안내한다. 다른 호스트로 몰래 넘어가지 않는다. ([플러그인 참고](https://paseo.sh/docs/plugins/reference.md))
  - [추정] 휴대폰 화면에는 "Mac 오프라인"과 마지막으로 Mac과 통신한 시각을 보여 준다.
- **재시도**
  - [경험담] Paseo 휴대폰 앱은 끊기면 스스로 다시 연결을 시도한다. 실패할수록 대기 간격이 길어져 최대 30초까지 늘어난다는 분석이 이슈에 있다. ([paseo#8826](https://github.com/getpaseo/paseo/issues/8826))
  - [추정] 읽기는 자동으로 다시 시도해도 된다.
  - [추정] 쓰기는 자동으로 다시 시도하지 않는다. 쓰는 도중 끊겼다면 다시 연결된 뒤 현재 상태를 읽어 보여 주고, 사용자가 다시 누르게 한다. CLI 쓰기 명령은 같은 명령을 두 번 실행해도 결과가 같게 만든다. 예를 들어 이미 지운 설치를 다시 지우라고 하면 "이미 없음"으로 끝낸다.
- **쌓아 두지 않기**
  - [추정] Mac이 꺼져 있는 동안 받은 쓰기 요청을 쌓아 두었다가, 깨어난 뒤 자동으로 실행하지 않는다.
  - 3단계의 "휴대폰에서 요청, Mac에서 승인"만 예외다. 이 요청은 Mac 화면에 보이기만 하고, 사용자가 Mac에서 승인해야 실행한다.
- **잠들지 않게 하기**
  - [사실] `caffeinate -i`는 유휴 잠자기만 막는다. `-s`는 전원이 연결됐을 때만 시스템 잠자기를 막는다. (`man caffeinate`)
  - [추정] 노트북 뚜껑을 닫는 것처럼 다른 조건으로 잠드는 경우는 `-i`로 막지 못할 수 있다. 정확한 조건은 확인하지 않았다(미확인).
- **원격으로 깨우기**
  - [사실] Tailscale은 Wake-on-LAN 신호(같은 네트워크에서 꺼진 컴퓨터를 깨우는 신호)를 보낼 수 없다. Wake-on-LAN은 보통 같은 로컬 네트워크에서만 동작한다. 같은 네트워크에 늘 켜진 다른 기기를 두는 방법을 권한다. ([Tailscale 블로그](https://tailscale.com/blog/wake-on-lan-tailscale-upsnap))
  - [추정] 항상 켜진 원격 서버는 Mac과 같은 로컬 네트워크에 있지 않으므로 이 역할을 못 한다.
  - [추정] 추가 PC가 Mac과 같은 로컬 네트워크에서 늘 켜져 있다면 깨우기 신호를 보내는 후보가 된다. 이 Mac이 그 신호로 깨어나는지는 확인하지 않았다(미확인).
  - [사실] Apple의 "네트워크 접근 시 깨우기"는 공유 자원 접근을 위한 기능이다. 조건으로 802.11n을 지원하는 Apple 무선 기기를 든다. ([Apple](https://support.apple.com/guide/mac-help/share-your-mac-resources-when-its-in-sleep-mh27905/mac))

## 5. 새로 생기는 위험과 방어 원칙

공격 재현 방법은 적지 않는다. 바꿀 동작과 방어 원칙만 적는다.

| 위험 | 생기는 곳 | 바꿀 동작 / 방어 원칙 | 근거 |
|---|---|---|---|
| CSRF(다른 사이트가 내 브라우저를 시켜 몰래 쓰기 요청을 보냄) | A, D | 상태를 바꾸는 요청은 `GET`으로 받지 않는다. `Origin`이나 `Sec-Fetch-Site`를 검사하고 요청마다 토큰을 확인한다. 보기 전용 단계에서는 쓰기 경로를 아예 만들지 않는다 | [OWASP CSRF](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html), `vite.config.ts:27-31` |
| DNS rebinding | A, D | `Host` 헤더를 허용 목록(내 Serve 주소, localhost)으로 검사한다. "아무 호스트나 허용"은 쓰지 않는다 | [Vite](https://vite.dev/config/server-options), [Paseo 보안](https://paseo.sh/docs/security.md) |
| 경로 조작(`../` 같은 경로로 범위 밖 파일에 접근) | A, C, D, E | 휴대폰에서 파일 경로 문자열을 받지 않고 `skill_id`, `agent_id` 같은 식별자만 받는다. 파일 읽기는 `read_skill_file_impl`을 재사용한다 | `src-tauri/src/commands/skills.rs:1544-1579` |
| 토큰 탈취 | A, C, D | 세션 토큰은 URL에 넣지 않는다(기록과 방문 기록에 남는다). 유효 시간을 짧게 둔다. 신원 헤더는 localhost 백엔드에서만 믿는다 | [OWASP CSRF](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html), [Serve](https://tailscale.com/kb/1312/serve) |
| 키체인 비밀값 유출 | 모두 | 휴대폰 응답 형식에 비밀값 칸을 두지 않고 `{configured, keychainAvailable}`만 둔다. 비밀값 입력과 변경은 Mac 화면에서만 한다. CLI에도 비밀값 출력 명령을 만들지 않는다. 작업 기록에도 남기지 않는다 | ADR-0003, `src/types/index.ts:16-19`, `web-dashboard/snapshot.ts:171-177` |
| 개인정보가 든 경로 노출 | A, C2 | 응답의 홈 경로를 `~/...`로 바꿔 보낸다 | `web-dashboard/snapshot.ts:649-657` |
| 휴대폰 분실 | 모두 | 휴대폰 잠금을 켠다. Tailscale은 관리 콘솔에서 기기를 지운다(절차는 미확인). 기기 키는 기본 180일 뒤 만료된다. Paseo는 기기별 해제를 확인하지 못했다. 그래서 위험 작업에는 Mac 승인이나 다시 인증을 둔다 | [키 만료](https://tailscale.com/kb/1028/key-expiry), [paseo#416](https://github.com/getpaseo/paseo/issues/416) |
| 사설망의 다른 기기 | A, E | 기본 "모두 허용" 정책을 "휴대폰 → Mac의 한 포트"로 좁힌다. 서버는 신원 헤더로 내 계정인지 한 번 더 본다 | [ACL](https://tailscale.com/kb/1018/acls), [Serve](https://tailscale.com/kb/1312/serve) |
| 공개 노출 | B, D | Funnel과 빠른 터널을 쓰지 않는다. 주소를 숨겨서 지키려 하지 않는다(인증서 공개 기록) | [Funnel](https://tailscale.com/kb/1223/funnel), [HTTPS](https://tailscale.com/kb/1153/enabling-https) |
| 프롬프트 주입 | C1 | 스킬 본문을 읽는 명령과 바꾸는 명령을 분리한다. 바꾸는 명령은 사람이 승인한다. 에이전트에게 파일 편집 대신 CLI만 쓰게 한다 | [OWASP LLM01](https://genai.owasp.org/llmrisk/llm01-prompt-injection/) |
| 프로세스 간 동시 실행 | C1, C2, E, A(쓰기) | 쓰기는 한 프로세스에서만 하거나 프로세스 사이의 잠금을 더한다. 어느 쪽인지는 #52·#53에서 정한다 | `src-tauri/src/commands/recovery.rs:51-54` |
| 관문·모음 서버가 뚫림 | F, 여러 기기 확장 | 원격 서버에 비밀값, 쓰기 권한, 모든 기기로 가는 접근을 모으지 않는다. 모은다면 읽기 전용 사본만 둔다 | ADR-0003, [Paseo 보안](https://paseo.sh/docs/security.md) |
| ADR-0002 우회 | 모두 | 삭제와 비활성은 `ensure_installation_managed`, `delete_managed_installation_locked`를 거친다. 관찰 항목은 휴대폰에서도 "가져오기"만 보인다 | `src-tauri/src/db.rs:2383-2403`, `src-tauri/src/commands/usage.rs:688-726`, ADR-0002 |

## 6. "보기 전용"과 "설치·삭제까지"의 보호 장치 차이

| 보호 장치 | 1단계: 보기 전용 | 2단계: 가벼운 조작 (활성/비활성 전환, 관찰 항목 가져오기처럼 되돌릴 수 있는 작업) | 3단계: 설치·삭제·덮어쓰기 |
|---|---|---|---|
| 접속 인증 | 페어링된 Paseo 기기 또는 사설망 + 신원 헤더 | 1단계와 같음 | 1단계와 같음 |
| 다시 인증 | 없음 | 없음 또는 세션 시작 때 한 번 | 작업마다, 또는 짧은 시간 안에서만 유효 |
| 확인 단계 | 없음 | 대상과 결과를 보여 주고 한 번 확인 | 미리 보기 결과를 보고 대상 이름을 다시 확인 |
| 미리 보기(dry-run, 실제로 바꾸지 않고 결과만 보여 주기) | 해당 없음 | 기본값으로 켬 | 필수. 미리 보기 뒤 대상이 바뀌면 다시 확인(공용 설치 삭제 규칙과 같음) |
| Mac에서 승인 | 필요 없음 | 필요 없음 | 보관함 원본 삭제와 플랫폼 전체 삭제는 Mac에서 승인. 단일 관리 설치 제거는 휴대폰 승인도 가능 |
| 작업 기록(audit log) | 선택 | 필수: 누가, 언제, 무엇을, 결과 | 필수: 2단계 내용 + 백업 항목 ID, 미리 보기 결과 |
| 복구 | 해당 없음 | 비활성은 파일을 보존하므로 그대로 되돌림 | 기존 복구 백업(`backup_copy_installation`)과 휴지통을 반드시 거침 |
| 관리 설치 판정(ADR-0002) | 표시만 | 비활성 전환 전에 판정 | 제거·덮어쓰기 전에 판정. 관찰 항목은 거부 |
| 비밀값 | 설정 여부만 | 설정 여부만 | 설정 여부만. 토큰이 필요한 설치는 Mac 프로세스 안에서만 씀 |
| 웹 방어(CSRF, Host 검사) | Host 검사. 쓰기 경로 없음 | Host 검사 + CSRF 방어 | 2단계 + 요청별 일회용 확인 토큰 |
| 동시 실행 잠금 | 읽기 전용 DB 연결이면 불필요 | 필요 | 필요 |
| Mac이 꺼졌을 때 | "마지막 갱신 시각" 표시 | 요청을 쌓지 않음 | 요청은 Mac에서 승인할 때만 실행 |

## 7. 초보자에게 맞는 추천 구조

1. **1단계: 보기 전용**
   - 읽기 전용 CLI를 먼저 만든다. 예: 목록, 스킬 정보, 설치 상태를 JSON(프로그램이 읽기 쉬운 글자 형식)으로 출력한다.
   - CLI는 DB를 읽기 전용으로 열고 비밀값 칸은 설정 여부만 준다. 대시보드의 방식(`web-dashboard/snapshot.ts:397`, `:171-177`)과 같다.
   - 휴대폰 Paseo 대화에서 에이전트에게 이 CLI 실행을 맡긴다. 에이전트 권한 규칙에서는 이 CLI만 허용하고, 스킬 폴더 편집은 확인을 거치게 한다.
   - 휴대폰 Tailscale은 필요 없다.
   - 화면으로 보고 싶으면 그다음에 A를 더한다.
     - 별도의 최소 읽기 전용 서버를 localhost에 둔다.
     - Tailscale Serve는 쓸 때만 켠다(`--bg` 없이 켜거나 다 쓰면 `off`).
     - 사설망 정책은 휴대폰에서 그 포트로만 열고, Host 검사를 둔다.
   - 개발용 Vite 대시보드를 그대로 열지 않는다.
2. **2단계: 가벼운 조작**
   - 되돌릴 수 있는 명령만 CLI에 연다. 예: 활성/비활성 전환, 관찰 항목 가져오기.
   - 명령은 기본으로 미리 보기 결과만 낸다. 실제 실행은 별도 옵션과 Paseo 권한 승인을 거친다.
   - 모든 실행을 작업 기록에 남긴다.
   - 데스크톱 앱과 동시에 쓰지 않도록 잠금을 정한다.
3. **3단계: 설치·삭제**
   - 기존 Rust 함수(관리 설치 판정, 백업, 원자적 복사)를 그대로 부른다.
   - 미리 보기, 대상 이름 재확인, 다시 인증을 거친다.
   - 보관함 원본 삭제와 플랫폼 전체 삭제는 "휴대폰에서 요청, Mac에서 승인"으로 시작한다.
   - 한 번에 하나씩 실행하고 결과와 백업 ID를 기록한다.

C2(플러그인 화면)는 CLI가 안정된 뒤 "CLI를 부르는 얇은 화면"으로 다시 검토한다. B, D, E, F는 이 단계 계획에 넣지 않는다.

여러 기기(항상 켜진 원격 서버, 추가 PC) 관리를 정하게 되면, 기기마다 같은 CLI와 Paseo 데몬을 두는 C1 확장부터 검토한다. 각 기기의 스킬 파일과 비밀값은 그 기기에 둔 채로 다룬다(2-1절).

## 8. 하지 말아야 할 것

- Tailscale Funnel, Cloudflare 빠른 터널, 포트 포워딩으로 인터넷에 열지 않는다.
- Vite 개발 서버의 `host`를 `0.0.0.0`으로 바꾸거나 `allowedHosts: true`, `cors: true`로 풀지 않는다.
- 휴대폰으로 API 키, GitHub 토큰, 키체인 내용을 보내거나 휴대폰에서 입력받지 않는다.
- Mac 키체인의 비밀값을 항상 켜진 원격 서버나 추가 PC로 복사하지 않는다.
- 관문 서버 하나에 여러 기기의 쓰기 권한을 모으지 않는다.
- 휴대폰에서 받은 파일 경로를 그대로 열거나 지우지 않는다.
- 에이전트가 스킬 폴더를 직접 `rm`·복사·편집하게 두지 않는다. 보호 함수를 거치는 CLI만 쓰게 한다.
- 상태를 바꾸는 요청을 `GET`으로 받거나 토큰을 URL에 넣지 않는다.
- Mac이 꺼져 있을 때 받은 쓰기 요청을 깨어난 뒤 자동 실행하지 않는다.
- Paseo 페어링 QR이나 링크를 화면 공유, 채팅, 공개 저장소에 올리지 않는다.
- 기기 이름에 개인정보를 넣은 채 Tailscale HTTPS를 켜지 않는다.

## 9. 확인하지 못한 것

- VPN On Demand가 `*.ts.net` 접속 뒤 언제 자동으로 꺼지는지 문서에 없다. 실제 iPhone에서도 시험하지 않았다.
- 휴대폰의 운영체제(iOS 또는 Android)를 확인하지 않았다. Android라면 VPN On Demand가 없다.
- Mac App Store판 Tailscale에서 `serve`를 쓰는 절차와 제약을 확인하지 않았다.
- Paseo에서 잃어버린 휴대폰 하나만 끊는 방법을 찾지 못했다. 데몬 키를 바꾸는 방법의 영향도 확인하지 않았다.
- macOS 키체인 항목을 다른 프로그램(에이전트, CLI)이 읽으려 할 때 macOS가 묻는지 여부를 확인하지 않았다.
- 뚜껑 닫힘 같은 잠자기 조건과 `caffeinate` 사이의 관계를 확인하지 않았다.
- Paseo 플러그인 화면을 실제 휴대폰에서 띄워 보지 않았다.
- Tailscale 관리 콘솔에서 기기를 지우는 공식 절차 문서를 확인하지 않았다.
- Paseo 데몬이 직접 운영하는 릴레이를 가리키게 하는 설정 이름을 찾지 못했다.
- Paseo 휴대폰 앱에서 여러 호스트를 오가는 실제 화면을 확인하지 않았다.
- 이 Mac이 Wake-on-LAN 신호로 깨어나는지 확인하지 않았다.

## 10. 출처

**저장소**

- `web-dashboard/snapshot.ts:171-177`, `:397`, `:649-671`(경로 649-657, 비밀값 설정 여부 658·669-671)
- `vite.config.ts:12-47`, `:68-71`
- `README.md:201-212`
- `src-tauri/src/main.rs:4-6`
- `src-tauri/src/secrets.rs:1-19`
- `src/types/index.ts:16-19`
- `src-tauri/src/db.rs:2383-2403`
- `src-tauri/src/commands/usage.rs:688-726`
- `src-tauri/src/commands/recovery.rs:51-54`, `:845-874`
- `src-tauri/src/commands/linker.rs:150-189`
- `src-tauri/src/commands/skills.rs:1544-1579`
- `src-tauri/src/commands/observed_import.rs:17-55`
- `docs/adr/0002-remove-only-managed-installations.md`, `docs/adr/0003-secrets-in-os-keychain.md`, `GLOSSARY.md`
- 이슈 #18

**Paseo**

- 문서 색인: https://paseo.sh/llms.txt
- 보안: https://paseo.sh/docs/security.md
- 연결: https://paseo.sh/docs/connectivity.md
- CLI: https://paseo.sh/docs/cli.md
- 설정: https://paseo.sh/docs/configuration.md
- 웹 UI: https://paseo.sh/docs/web-ui.md
- 플러그인 시작: https://paseo.sh/docs/plugins.md
- 플러그인 참고: https://paseo.sh/docs/plugins/reference.md
- README: https://github.com/getpaseo/paseo
- 릴레이 README: https://github.com/getpaseo/paseo-relay
- 이슈: https://github.com/getpaseo/paseo/issues/416, https://github.com/getpaseo/paseo/issues/8826, https://github.com/getpaseo/paseo/issues/9219, https://github.com/getpaseo/paseo/issues/10070
- 실행한 명령: Paseo CLI 0.16.1의 `--help` 계열 명령, `daemon status --json`(값 가림)

**Tailscale**

- Serve: https://tailscale.com/kb/1312/serve
- Serve CLI: https://tailscale.com/kb/1242/tailscale-serve
- Funnel: https://tailscale.com/kb/1223/funnel
- VPN On Demand: https://tailscale.com/kb/1291/ios-vpn-on-demand
- ACL: https://tailscale.com/kb/1018/acls
- 키 만료: https://tailscale.com/kb/1028/key-expiry
- HTTPS: https://tailscale.com/kb/1153/enabling-https
- Tailscale SSH: https://tailscale.com/kb/1193/tailscale-ssh
- Wake-on-LAN 블로그: https://tailscale.com/blog/wake-on-lan-tailscale-upsnap
- 이슈: https://github.com/tailscale/tailscale/issues/20755, https://github.com/tailscale/tailscale/issues/19837, https://github.com/tailscale/tailscale/issues/12086

**Cloudflare**

- Tunnel: https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/
- Tunnel 설정: https://developers.cloudflare.com/tunnel/setup/
- 빠른 터널: https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/do-more-with-tunnels/trycloudflare/
- Access 자체 호스팅 앱: https://developers.cloudflare.com/cloudflare-one/access-controls/applications/http-apps/self-hosted-public-app/
- SSL 모드: https://developers.cloudflare.com/ssl/origin-configuration/ssl-modes/

**그 밖의 공식 문서**

- Vite: https://vite.dev/config/server-options, https://vite.dev/guide/cli
- OWASP CSRF: https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html
- OWASP LLM01: https://genai.owasp.org/llmrisk/llm01-prompt-injection/
- Claude Code 권한: https://code.claude.com/docs/en/permissions
- Apple 원격 로그인: https://support.apple.com/guide/mac-help/allow-a-remote-computer-to-access-your-mac-mchlp1066/mac
- Apple 잠자기 중 공유: https://support.apple.com/guide/mac-help/share-your-mac-resources-when-its-in-sleep-mh27905/mac
- `man caffeinate`(macOS)

**경험담**

- Reddit
  - https://www.reddit.com/r/Tailscale/comments/1gfsoqr/vpn_on_demand_magicdns_trigger_not_working_on_ios/
  - https://www.reddit.com/r/Tailscale/comments/1un90t1/vpn_in_demand_ios/
  - https://www.reddit.com/r/Tailscale/comments/16026mt/ios_vpn_on_demand_question/
  - 검색 결과의 본문 일부만 읽었다.
- Hacker News: https://news.ycombinator.com/item?id=39902737
- 커뮤니티 위키(검색 요약만 봄): https://git.majorshouse.com/majorlinux/MajorWiki/src/branch/main/05-troubleshooting/ollama-macos-sleep-tailscale-disconnect.md
