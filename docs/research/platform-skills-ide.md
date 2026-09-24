# 플랫폼별 스킬(SKILL.md) 탐색 경로·개별 제어 공식 근거 조사

- 작성일: 2026-09-21 (Asia/Seoul)
- 수정: 2026-09-21 2차 개정 — 단정 표현을 "미확인/검증 후보"로 완화하고, IBM Bob·CodeArts Agent·Syncfusion Code Studio 공식 근거를 추가했다.
- 대상 저장소: /Users/jsm2/.codex/worktrees/a35b/skills-manage
- 소유 파일: docs/research/platform-skills-ide.md (이 문서 하나만 신규 작성·수정. 코드·설정·실제 스킬·커밋 변경 없음)
- 대상 플랫폼 ID(14): aider-desk, trae, trae-cn, junie, qwen, windsurf, qoder, augment, kilocode, bob, codearts-agent, codestudio, continue, roo

## 0. 목적과 전제

목표는 두 가지다.

1. 플랫폼마다 **스킬별 on/off**를 어떤 방식으로 할 수 있는지, 공식 자료에 근거해 확인한다.
2. **같은 에이전트가 여러 저장소에서 동일한 스킬을 중복으로 읽는 낭비**를 줄일 수 있는지 판단한다. 여기서 낭비는 (a) 목록 컨텍스트에 같은 스킬이 두 번 들어가는 비용, (b) 같은 실제 파일을 두 경로에서 다시 읽는 비용을 말한다.

사용자가 확정한 결정(이 문서는 이 결정을 전제로 분석한다):

- 필요하면 **공용 설치 → 플랫폼 전용 설치** 전환을 포함한다.
- 플랫폼 밖에 **별도 복사본이 남아 있어도 허용**한다.
- 내용이 같으면 **대표 출처 1개**로 정리한다. 내용이 다르면 사용자가 선택한다.
- 업데이트로 내용이 갈라지면(divergence) **대표 출처를 유지하고 알림**만 준다.
- **새로 감지된 플랫폼에는 자동 설치하지 않는다.**

중요한 전제: `src-tauri/src/db.rs`의 `builtin_agents()`에 적힌 플랫폼 이름과 경로는 **이 앱이 스스로 가정한 값**이지, 해당 런타임이 그 경로를 실제로 읽는다는 증거가 아니다. 그래서 이 문서는 앱 가정과 공식 문서를 항상 따로 표시하고, 둘이 다르면 차이를 명시한다.

## 1. 조사 방법과 증거 등급

- 공식 문서 사이트, 공식 GitHub 저장소의 소스/문서 파일을 직접 내려받아 확인했다. 블로그·요약글·제3자 사이트는 근거로 쓰지 않았다.
- 검색 도구가 이 턴에서 제한(429/한도)에 걸려, 대부분은 공식 URL을 직접 HTTP로 요청해 본문을 확인하는 방식으로 조사했다. 실패한 URL도 부록 A에 그대로 기록한다.
- 비밀정보(토큰·키·쿠키·자격증명)는 읽지 않았고, 인증이 필요한 페이지는 조사 대상에서 제외했다.
- GitHub의 `main` 브랜치 raw 파일은 **이동하는 참조**(고정 리비전 아님)다. 고정 리비전(permalink)을 확인한 경우에만 커밋 해시를 적고, 아니면 "main, 이동 가능"이라고 표시한다.

증거 등급:

- **확인**: 공식 문서 또는 공식 소스 코드에서 해당 동작을 직접 읽었다.
- **부분확인**: 경로나 기능의 존재는 확인했으나, 세부(정확한 설정 키·우선순위 규칙 등)는 확인하지 못했다.
- **미확인**: 공식 자료를 찾지 못했거나, 이번 조사에서 해당 항목을 확인하지 못했다. **"불가능하다"·"지원하지 않는다"는 뜻이 아니다.**

### 1.1 표현 원칙 (이번 개정에서 적용)

- **공식 문서에 서술이 없다 ≠ 지원하지 않는다.** 어떤 경로를 읽는다는 서술을 찾지 못한 경우 전부 **미확인**으로 적고, "겹치는 탐색 경로 검증 후 후보"로 다룬다. 문서화되지 않은 경로를 실제로 읽을 가능성을 배제할 근거가 없기 때문이다.
- **어떤 제어 수단의 존재 ≠ 스킬 단위 off.** 다음은 개별 스킬 on/off로 보지 않는다.
  - 도구 전체 게이트(예: AiderDesk의 "Skills Tools" on/off) → 프로필/태스크 단위이며 개별 스킬 제어가 아니다.
  - 조직/정책 단위 feature flag(예: Qoder의 `/skills` 지원·관리자 권한) → 조직 범위이지 개별 스킬 제어가 아니다.
  - 경로 추가 설정(예: Kilo의 `skills.paths`, `skills.urls`) → **추가 경로를 더하는 수단**이며, 기본 경로 탐색을 차단한다는 증거가 아니다.
- **경로 겹침(동일 실체가 여러 탐색 루트에 걸리는 경우)의 중복 여부는 미확인**이다. realpath 기준 중복 제거를 명시한 플랫폼을 찾지 못했으므로, "중복이 남는다"고 단정하지 않고 "검증 후 후보"로 적는다.
- **path isolation(기본 탐색 경로에서 파일을 제거·이름 변경)**은 스킬을 목록에서 빼는 **후보 수단**일 뿐, 공식적으로 보장된 off 방법이 아니다. 차단 효과는 미확인이며, 겹치는 탐색 경로(특히 공용 `.agents/skills`)를 함께 검증한 뒤에만 후보로 채택한다.

## 2. 요약 표

기본 경로는 "사용자(전역) / 프로젝트" 순서다. `~`는 사용자 홈.

| ID | 전역(사용자) 경로 | 프로젝트 경로 | 공용 `.agents/skills` | 스킬별 on/off | 제안 방식 | 확인 수준 |
|---|---|---|---|---|---|---|
| aider-desk | `~/.aider-desk/skills/` | `.aider-desk/skills/` | 서술 없음(미확인) | 없음(도구 전체 게이트만) → 미확인 | 미확인(검증 후보) | 확인(경로·우선순위) |
| trae | `~/.trae/skills/` (앱 가정) | `.trae/skills/` (앱 가정) | 미확인 | 미확인 | 미확인(검증 후보) | 미확인 |
| trae-cn | `~/.trae-cn/skills/` | `.trae/skills/` | 서술 없음(미확인) | 프로젝트 스킬 native 비활성(`.trae/skill-config.json`) | native(프로젝트 한정) | 확인 |
| junie | `~/.junie/skills/` | `.junie/skills/` | 예(프로젝트+사용자, 문서 명시) | native(비활성 시 자동선택·`/`·`$`·컨텍스트 제외) | native + (경로 옵션) | 확인 |
| qwen | `~/.qwen/skills/` | `.qwen/skills/` | 서술 없음(미확인) | `/skills` 패널 토글(native) | native | 확인 |
| windsurf | `~/.codeium/windsurf/skills/` | `.windsurf/skills/` | 예(문서 명시) | 미확인 | 미확인(검증 후보) | 확인(경로) / 미확인(제어) |
| qoder | `~/.qoder/skills/` | `.qoder/skills/` | 서술 없음(미확인) | 조건부 스킬(경로 미매칭 시 목록 제외)만 확인. 조직 flag는 개별 off 아님 | native(조건부) / 그 외 미확인 | 확인(경로·우선순위·조건부) |
| augment | `~/.augment/skills/` | `.augment/skills/` | 예(문서 명시, 최하위 우선순위) | Skill Modes(Auto/Manual/Disabled) | native | 확인(모드) / 부분확인(키) |
| kilocode | `~/.kilo/skills/` | `.kilo/skills/` | 예(기본 로드, 문서 명시) | 미확인 (`skills.paths`는 추가 경로일 뿐) | 미확인(검증 후보) | 확인(경로·우선순위) |
| bob | `~/.bob/skills/` | `.bob/skills/` | 서술 없음(미확인) | "Allow Bob to use this skill" 토글(native) | native | 확인 |
| codearts-agent | `~/.codeartsdoer/skills/` | `.codeartsdoer/skills/` | 서술 없음(미확인) | native 토글 + `ProjectSkillStatus.txt`(`이름=true|false`) | native | 확인 |
| codestudio | `~/.codestudio/skills/` | `.codestudio/skills/` | 예(문서 명시) | 미확인 | 미확인(검증 후보) | 확인(경로) / 미확인(제어) |
| continue | `~/.continue/skills/` | `.continue/skills/`, `.claude/skills/` | 서술 없음(미확인) | 미확인 (`CONTINUE_GLOBAL_DIR`는 전역 루트 이동) | 미확인(검증 후보) | 확인(경로·로드 시점) |
| roo | `~/.roo/skills/` | `.roo/skills/`, `.agents/skills/` | 예(문서 명시) | 미확인 | 미확인(검증 후보) | 확인(경로·우선순위) |

## 3. 공통 개념

### 3.1 `.agents/skills` 공용 표준

여러 플랫폼이 **같은 폴더(`.agents/skills`, `~/.agents/skills`)를 함께 읽는 관행**을 채택하고 있다. 이번 조사에서 이를 **공식 문서에 명시한** 곳은 Roo, Junie, Augment, Windsurf(Devin Desktop), Kilo Code, Syncfusion Code Studio다. 한 스킬을 이 공용 폴더에 두면 위 플랫폼들이 동시에 읽을 수 있고, 각자 자기 전용 폴더에도 같은 스킬이 있으면 **중복으로 목록에 올라갈 가능성**이 있다(실제 중복 여부는 미확인, 검증 후보).

이 문서에서 "공용 설치"는 이 공용 경로에 스킬을 두는 것을 뜻하고, "전용 설치"는 특정 플랫폼의 자기 폴더(예: `~/.qwen/skills`)에 두는 것을 뜻한다.

### 3.2 progressive disclosure(점진적 공개)

Roo, Windsurf, AiderDesk, Kilo Code, CodeArts Agent 등이 공통으로 설명하는 동작이다. 모델에게 처음에는 스킬의 `name`과 `description`만 보여 주고, 실제로 그 스킬이 선택될 때 `SKILL.md` 본문을 읽는다. 그래서 스킬을 많이 깔아도 **본문 토큰 비용은 작지만, 이름·설명 목록 비용은 스킬 수에 비례**한다. 중복 제거의 실익은 주로 이 "목록 비용"과 "같은 파일 재독"에서 나온다.

### 3.3 native invocation 차단 vs 목록 컨텍스트 제외 (구분)

두 가지는 다르다. 이 구분이 플랫폼별 on/off 설계에 직접 영향을 준다.

- **목록 컨텍스트 제외**: 그 스킬의 이름·설명이 모델의 "사용 가능 스킬" 목록에 아예 들어가지 않는다. 목록 토큰이 줄어든다. 예: Qoder의 조건부 스킬은 파일 경로가 맞지 않으면 **목록에 나타나지 않는다**고 공식 문서가 설명한다.
- **native invocation 차단**: 모델이 이름을 알아도 그 스킬을 실행·활성화할 수 없다. 더 강한 차단이다. 예: Junie는 비활성 스킬을 자동 선택뿐 아니라 `/` 명령 목록, `$` 제안, 컨텍스트 주입에서 모두 제외한다고 명시한다.
- **개별 off로 보지 않는 것**: AiderDesk의 "Skills Tools" on/off는 프로필/태스크 전체 도구 게이트다. Qoder의 `/skills` 지원·관리자 권한 flag는 조직/정책 범위다. 둘 다 **특정 스킬 하나를 끄는 수단이 아니다.**
- 중간 사례: Augment의 `Disabled` 모드는 "loaded but not active"(로드되지만 활성 아님)로 설명된다. 즉 목록에는 남을 수 있으나 활성화가 막히는 형태로 읽힌다. 정확한 내부 동작(목록 포함 여부)은 **부분확인**이다.

즉 "스킬을 껐다"는 말이 (a) 목록에서만 뺐는지 (b) 실행까지 막았는지에 따라 비용과 위험이 다르다. 아래 플랫폼별 항목에서 어느 쪽인지 표시한다.

## 4. 플랫폼별 상세

각 항목은 다음 순서로 적는다: 앱 가정 → 기본 경로 → 추가/공용 경로 → 개별 제외 설정 → 동명·동일 realpath 중복 처리와 우선순위 → 갱신 시점 → 제안 방식 → 근거 URL → 확인 수준.

### 4.1 roo (Roo Code) — 확인

- 앱 가정(db.rs): 전역 `.roo/skills`, 프로젝트 `.roo/skills`.
- 기본 경로(공식): 전역 `~/.roo/skills/{name}/SKILL.md`, 프로젝트 `<root>/.roo/skills/`. Windows는 `%USERPROFILE%\.roo\skills\...`.
- 추가/공용 경로(공식): 전역 `~/.agents/skills/{name}/SKILL.md`, 프로젝트 `<root>/.agents/skills/`. 즉 **Roo는 공용 경로를 직접 읽는다(문서 명시).** 앱 가정에는 이 경로가 빠져 있다.
- 개별 제외 설정: 공식 문서에서 스킬 단위 비활성 키를 찾지 못했다 → **미확인**. 대신 이름/디렉터리 규칙(이름은 디렉터리·심볼릭 링크 이름과 일치, 1~64자 소문자/숫자/하이픈, description 1~1024자)이 로드 조건이다.
- 동명·동일 realpath 중복: **같은 이름이면 Roo 전용 디렉터리(`.roo/skills`)가 `.agents`보다 우선**한다고 명시(이름 기준). **realpath 기준 중복 제거는 문서에 없음 → 미확인.** `.roo/skills/foo`와 `.agents/skills/foo`가 같은 실체를 가리킬 때 실제로 중복 목록이 되는지는 **겹치는 탐색 경로 검증 후 후보**다.
- 갱신 시점: progressive disclosure만 확인(세션 시작 시 목록 로드). 재스캔/리로드 시점은 **미확인**.
- 제안 방식: native 개별 off가 미확인이므로 **미확인**. 경로 겹침을 검증한 뒤 path isolation을 후보로 검토한다.
- 근거: https://docs.roocode.com/features/skills
- 확인 수준: 경로·이름 우선순위 **확인**, 개별 off·realpath 중복 **미확인**.

### 4.2 qwen (Qwen Code) — 확인

- 앱 가정(db.rs): 전역 `.qwen/skills`, 프로젝트 `.qwen/skills`.
- 기본 경로(공식): 개인 `~/.qwen/skills/`, 프로젝트 `.qwen/skills/`. `/learn`은 `.qwen/skills/learned-skill-<name>/SKILL.md`에 `source: learned` 프런트매터로 저장.
- 추가/공용 경로: 확인한 공식 문서에 `.agents/skills` 서술이 **없다 → 미확인**(지원하지 않는다는 뜻이 아님). 확장(extension) 제공 스킬은 `owner:skill`(예: `rust:pdf`) 형식으로 구분된다.
- 개별 제외 설정: `/skills` 패널이 **탐색·검색·토글·실행**을 지원한다 → **native 스킬 단위 토글 확인**. 저장 위치(설정 파일 키)는 **미확인**.
- 동명·동일 realpath 중복: 확장 스킬은 `owner:` 접두사로 네임스페이스가 갈린다. 동일 이름 로컬 스킬 간 우선순위·realpath 중복 규칙은 **미확인**.
- 갱신 시점: **미확인**.
- 제안 방식: **native**(`/skills` 토글).
- 근거: https://raw.githubusercontent.com/QwenLM/qwen-code/main/docs/users/features/skills.md (main 브랜치, 이동 가능)
- 확인 수준: 경로·토글 **확인**, 설정 키·우선순위·공용 경로 **미확인**.

### 4.3 windsurf (Windsurf / Devin Desktop) — 확인(경로) / 미확인(제어)

- 앱 가정(db.rs): 전역 `.codeium/windsurf/skills`, 프로젝트 `.windsurf/skills`.
- 기본 경로(공식): 워크스페이스 `.windsurf/skills/`, 전역 `~/.codeium/windsurf/skills/`. 엔터프라이즈용 System 경로는 OS별이며 **읽기 전용**(문서가 구체 경로를 잘라서 보여 줌 → **부분확인**).
- 추가/공용 경로(공식): "Devin Desktop also discovers skills in `.agents/skills/` and `~/.agents/skills/`." 또한 Claude Code 설정 읽기를 켜면 `.claude/skills/`와 `~/.claude/skills/`도 스캔. → **공용 경로를 읽으므로 중복 목록 가능성이 있다(실제 중복은 미확인).**
- 개별 제외 설정: 문서 본문에서 스킬 단위 비활성/토글 키를 찾지 못했다 → **미확인**. 수동 호출은 `@skill-name`, 자동 호출은 description 매칭이다.
- 동명·동일 realpath 중복: 이름·realpath 우선순위 규칙이 문서에 없다 → **미확인**. `.windsurf/skills/foo`와 `.agents/skills/foo`가 같은 실체면 중복 목록이 될 수 있으나, 이는 **겹치는 탐색 경로 검증 후 후보**다.
- 갱신 시점: **미확인**.
- 제안 방식: **미확인**(검증 후보).
- 근거: https://docs.windsurf.com/windsurf/cascade/skills (문서는 현재 Devin/Cognition 계열로 제공됨)
- 확인 수준: 경로·공용 경로·수동 호출 **확인**, 개별 off·중복 규칙 **미확인**.

### 4.4 augment (Augment) — 확인(모드) / 부분확인(키)

- 앱 가정(db.rs): 전역 `.augment/skills`, 프로젝트 `.augment/skills`.
- 기본 경로(공식, 우선순위 순): `~/.augment/skills/`(User, 최상위) → `<workspace>/.augment/skills/` → `~/.claude/skills/` → `<workspace>/.claude/skills/` → `~/.agents/skills/` → `<workspace>/.agents/skills/`.
- 추가/공용 경로: 위 목록대로 `.claude/skills`와 `.agents/skills`를 **둘 다** 읽는다. 공용 경로는 **가장 낮은 우선순위**다.
- 개별 제외 설정: **Skill Modes**가 Auto(매 대화마다 주입) / Manual(`/` 메뉴) / **Disabled(loaded but not active)** 로 존재 → **native 스킬 단위 상태 확인**. 다만 정확한 설정 키/스키마(어느 파일의 어떤 필드인지)는 확인하지 못했다 → **부분확인**. VSCode 확장 0.789.0+ Public Beta 옵트인 기능이라는 조건이 붙는다.
- 동명·동일 realpath 중복: "여러 위치에 같은 이름이 있으면 **우선순위가 높은 위치의 스킬을 사용**한다"고 명시 → **이름 기준 중복 해소 규칙 확인**. **realpath 기준 중복 제거는 미확인.**
- 갱신 시점: Auto 모드는 "매 대화마다 주입"이므로 대화 시작 시점이 갱신 지점으로 읽힌다 → **부분확인**.
- 제안 방식: **native**(Skill Modes)가 원칙. 키 미확인 상태에서는 키 확보를 우선 검증한다.
- 근거: https://docs.augmentcode.com/using-augment/skills.md (문서 인덱스: https://docs.augmentcode.com/llms.txt)
- 확인 수준: 경로·우선순위·모드 존재 **확인**, 설정 키 **미확인**.

### 4.5 junie (JetBrains Junie) — 확인

- 앱 가정(db.rs): 전역 `.junie/skills`, 프로젝트 `.junie/skills`.
- 기본 경로(공식): 프로젝트 `<projectRoot>/.junie/skills/<name>/`, 사용자 `~/.junie/skills/<name>/` (Windows `%USERPROFILE%\.junie\skills\`).
- 추가/공용 경로(공식): 프로젝트·사용자 양쪽의 `.agents/skills/`를 로드. 확장(extension) 제공 스킬, `--skill-location` 또는 `config.json`의 `skill-locations`로 추가한 커스텀 스킬, 내장 스킬도 있다. 또한 `.cursor/skills/`, `.claude/skills/`, `.codex/skills/`를 **감지해서 `.junie/skills/`로 가져오도록 제안**한다(자동 복사가 아니라 제안).
- 개별 제외 설정: **native 확인** — 비활성 스킬은 자동 선택, `/` 명령, `$` 제안에서 제외되고 그 지시문이 컨텍스트에 추가되지 않는다. 또한 `--skill-default-locations false`는 기본 프로젝트/사용자 위치를 끄는데, **`.agents/skills`도 함께 꺼진다**(커스텀·확장·내장은 유지). 정확한 비활성 키 이름/스키마는 `/skills` 관리 섹션을 완전히 확인하지 못해 **미확인**.
- 동명·동일 realpath 중복: **프로젝트 `.junie/skills/<name>`이 사용자 레벨 같은 이름을 이기고, 사용자 스킬은 무시된다**고 명시 → 이름 기준 규칙 확인. **realpath 중복 제거는 미확인.**
- 갱신 시점: **미확인**.
- 제안 방식: **native**(비활성 상태 + `--skill-default-locations`).
- 근거: https://junie.jetbrains.com/docs/agent-skills.html (페이지 빌드 표시: 2026-09-21T12:30:08Z)
- 확인 수준: 경로·공용 경로·비활성 동작·이름 우선순위 **확인**, 비활성 키 이름 **미확인**.

### 4.6 aider-desk (AiderDesk) — 확인

- 앱 가정(db.rs): 전역 `.aider-desk/skills`, 프로젝트 `.aider-desk/skills`.
- 기본 경로(공식): 프로젝트 `.aider-desk/skills/`, 홈 `~/.aider-desk/skills/`, 내장(bundled) 스킬, 확장 제공 스킬.
- 추가/공용 경로: 확인한 공식 문서에 `.agents/skills` 서술이 **없다 → 미확인**.
- 개별 제외 설정: **개별 스킬 on/off가 아니다.** "Skills only work when **Skills Tools** are enabled" — 활성 에이전트 프로필에서 Settings → Agent → (프로필) → Use Skills Tools, 또는 태스크별 AgentSelector → Use skills tools. 이는 **프로필/태스크 전체 도구 게이트**이며, 특정 스킬 하나를 끄는 수단이 아니다. 끄면 에이전트가 스킬을 발견·로드하지 못한다. 스킬 단위 off 키는 **미확인**.
- 동명·동일 realpath 중복: 우선순위가 **extension > project > global(home) > built-in**으로 명시 → 이름 기준 규칙 확인. **realpath 중복 제거는 미확인.**
- 갱신 시점: 메타데이터만 미리 로드한다는 설명 확인(progressive disclosure). 재스캔 시점은 **미확인**.
- 제안 방식: **개별 off 아님**. 개별 제어는 **미확인**(검증 후보).
- 근거: https://raw.githubusercontent.com/hotovo/aider-desk/main/docs-site/docs/agent-mode/skills.md (main, 이동 가능), 정식 문서 https://aiderdesk.hotovo.com/docs/agent-mode/skills
- 확인 수준: 경로·우선순위·도구 게이트 **확인**, 스킬 단위 off **미확인**.

### 4.7 trae-cn (Trae CN) — 확인

- 앱 가정(db.rs): 전역 `.trae-cn/skills`, 프로젝트 `.trae/skills`.
- 기본 경로(공식): 项目技能 `<project>/.trae/skills/`, 全局技能 macOS/Linux `~/.trae-cn/skills`, Windows `%userprofile%/.trae-cn/skills`. → **앱 가정과 일치.**
- 추가/공용 경로(공식): Trae CN CLI는 별도로 전역 `~/.traecli/skills`, 프로젝트 `.traecli/skills/`를 쓴다 → **앱 db.rs에 없는 추가 경로**. `.agents/skills` 서술은 문서에 없다 → **미확인**.
- 개별 제외 설정: **확인** — 启用/禁用技能 토글이 `<project>/.trae/skill-config.json`에 **비활성 프로젝트 스킬 목록**을 만든다. **중요한 비대칭**: 비활성화된 **전역** 스킬은 이 파일에 나열되지 않는다 → 전역 스킬 off는 다른 위치에 저장되며 그 위치는 **미확인**. 정확한 JSON 필드명은 문서 서술만으로는 **미확인**.
- 동명·동일 realpath 중복: 문서에 우선순위 규칙이 명시되지 않음 → **미확인**.
- 갱신 시점: **미확인**.
- 제안 방식: **native**(프로젝트 스킬 한정 `skill-config.json`). 전역 스킬은 **미확인**.
- 근거: https://docs.trae.cn/ide_skills.md, https://docs.trae.cn/cli_skills.md (관련: enterprise_skills.md, enterprise_skill-controls.md — 기업 技能管控, 旗舰版套餐)
- 확인 수준: 경로·프로젝트 비활성 **확인**, 전역 비활성 위치·필드명·우선순위 **미확인**.

### 4.8 qoder (Qoder) — 확인

- 앱 가정(db.rs): 전역 `.qoder/skills`, 프로젝트 `.qoder/skills`.
- 기본 경로(공식): 사용자 `~/.qoder/skills/{skill-name}/SKILL.md`, 프로젝트 `.qoder/skills/{skill-name}/SKILL.md`. → **앱 가정과 일치.** 스킬은 반드시 `skills/<name>/SKILL.md` 구조여야 한다.
- 추가/공용 경로: `.agents/skills` 서술은 확인된 문서에 없다 → **미확인**. 설치 경로는 Extensions → Skills 마켓/Add Skills(Create with Qoder, ZIP 또는 `SKILL.md` 업로드).
- 개별 제외 설정: `/skills`(목록)·`/skills reload`(재로드). **조건부 스킬(Conditional Skill)** 은 파일 경로가 맞을 때만 활성화되고 아니면 **목록에 나타나지 않는다** → 목록 컨텍스트 제외의 실제 예. **주의: `/skills`가 요구하는 "스킬 지원 + 관리자 권한 feature flag"는 조직/정책 범위이며 개별 스킬 off가 아니다.** `agents.overrides`는 에이전트용이며 **스킬용 대응 키는 미확인**.
- 동명·동일 realpath 중복: **출처 우선순위 built-in < plugins < project-level < user-level, 이름 충돌 시 상위가 하위를 덮어씀** → 이름 기준 규칙 확인. **realpath 중복 제거는 미확인.**
- 갱신 시점: `/skills reload`로 명시적 재로드 **확인**.
- 제안 방식: **native(조건부 스킬)**. 그 외 개별 off는 **미확인**.
- 근거: https://docs.qoder.com/qoder/skills.md, https://docs.qoder.com/cli/Skills.md, https://docs.qoder.com/cli/troubleshoot-loading.md (추가: docs.qoder.com/extensions/skills.md, docs.qoder.com/cli/builtins-reference.md, docs.qoder.com/cloud-agents/skills.md)
- 확인 수준: 경로·이름 우선순위·조건부·재로드 **확인**, 스킬 단위 off 키 **미확인**.

### 4.9 kilocode (Kilo Code) — 확인

- 앱 가정(db.rs): 전역 `.kilocode/skills`, 프로젝트 `.kilocode/skills`.
- 기본 경로(공식): 전역 `~/.kilo/skills/` (Mac/Linux), Windows `\Users\<user>\.kilo\skills\`; 프로젝트 `.kilo/skills/`. → **앱 가정(`.kilocode`)과 다르다.** 공식 문서는 `.kilo`를 쓴다. (Kilo 자체 저장소 트리에는 `.kilo/skills/`와 `.kilocode/skills/`가 모두 존재하므로, 과거/병행 경로일 가능성이 있으나 공식 문서 기준은 `.kilo`다.)
- 추가/공용 경로(공식): `.agents/skills/`는 **Open agent standard로 기본 로드**, `.claude/skills/`는 **Claude Code Compatibility를 켰을 때** 로드.
- 개별 제외 설정: 스킬 단위 on/off 키는 문서에서 찾지 못함 → **미확인**. **주의: `kilo.jsonc`(프로젝트 또는 전역)의 `skills.paths`(절대경로, `~/` 홈 상대, 프로젝트 상대)와 `skills.urls`(예: `https://example.com/.well-known/skills/`)는 "추가 경로·원격 소스를 더하는" 수단이다. 이것이 기본 경로(`.kilo/skills`, `~/.kilo/skills`, `.agents/skills`) 탐색을 차단한다는 증거는 확인되지 않았다.** `KILO_DISABLE_SKILL_SHELL`은 스킬 내부 셸 명령 실행을 끄는 킬 스위치일 뿐, 스킬 목록 제외가 아니다.
- 동명·동일 realpath 중복: **같은 이름이면 프로젝트 `.kilo/skills/`가 전역 `~/.kilo/skills/`보다 우선**한다고 명시(이름 기준). 다만 **호환 디렉터리(`.claude/skills`, `.agents/skills`)와 추가 설정 경로는 프로젝트·전역 스킬과 "나란히 로드"**된다고 서술 → 이름이 같아도 **중복 목록이 될 가능성**이 있다(실제 중복은 미확인, 검증 후보).
- 갱신 시점: "Skills are discovered when a session starts" → 세션 시작 시 스캔 **확인**.
- 제안 방식: 개별 off는 **미확인**. `skills.paths`는 추가 경로 수단이므로 기본 경로 차단 근거로 쓰지 않는다.
- 근거: https://kilo.ai/docs/customize/skills
- 확인 수준: 경로·공용 경로·설정 키·이름 우선순위 **확인**, 스킬 단위 off·기본 경로 차단 **미확인**.

### 4.10 continue (Continue) — 확인 (공식 소스 기준)

- 앱 가정(db.rs): 전역 `.continue/skills`, 프로젝트 `.continue/skills`.
- 기본 경로(공식 소스): CLI는 `<cwd>/.continue/skills/`, `<cwd>/.claude/skills/`, `env.continueHome/skills`(= `~/.continue/skills`)를 읽는다. IDE 코어는 워크스페이스 `.claude/skills/`와 전역 `getGlobalFolderWithName("skills")`(= `~/.continue/skills`)를 읽는다. `SKILLS_DIR = "skills"`.
- 추가/공용 경로: `.claude/skills`를 읽지만 **`.agents/skills` 서술은 소스·문서에 없다 → 미확인**(미지원 단정 아님).
- 개별 제외 설정: 스킬 단위 off는 소스·문서에 없음 → **미확인**. `CONTINUE_GLOBAL_DIR` 환경변수로 전역 루트 자체를 바꿀 수 있다(`CONTINUE_GLOBAL_DIR`가 있으면 그 경로, 없으면 `~/.continue`). 이는 **전역 루트를 옮기는 설정**이지 기본 경로 차단 수단이 아니다.
- 동명·동일 realpath 중복: `.continue/skills`와 `.claude/skills`를 함께 스캔하므로 **동일 스킬이 두 번 로드될 가능성**이 있다. 이름 충돌 우선순위·realpath 중복 규칙은 소스에서 확인하지 못함 → **미확인**.
- 갱신 시점: `loadMarkdownSkills()`가 Skills 도구를 만들 때 호출된다 → **도구 초기화 시점 로드 확인**(매 대화/세션마다 재스캔에 가까움).
- 제안 방식: **미확인**(검증 후보).
- 근거: https://raw.githubusercontent.com/continuedev/continue/main/core/config/markdown/loadMarkdownSkills.ts, https://raw.githubusercontent.com/continuedev/continue/main/extensions/cli/src/util/loadMarkdownSkills.ts, https://raw.githubusercontent.com/continuedev/continue/main/core/util/paths.ts (모두 main, 이동 가능). 공식 문서 사이트에는 skills 문서가 확인되지 않음(docs.continue.dev/customize/skills → 404, docs.continue.dev/llms.txt에 "skill" 항목 없음).
- 확인 수준: 경로·전역 루트 재정의·로드 시점 **확인**, 스킬 단위 off·우선순위 **미확인**.

### 4.11 trae (Trae, 국제판) — 미확인

- 앱 가정(db.rs): 전역 `.trae/skills`, 프로젝트 `.trae/skills`.
- 시도한 공식 URL: https://docs.trae.ai/ide/skills, https://docs.trae.ai/llms.txt, https://docs.trae.ai/sitemap.xml
- 결과: 문서 사이트가 SPA(arcosite)로 렌더링되어 본문 텍스트를 얻지 못했다. `llms.txt`는 거대한 `_ROUTER_DATA` 블롭을 반환했고, sitemap.xml에는 "skill"이 포함된 URL이 없었다. 따라서 국제판 Trae의 전역 경로(`~/.trae/skills`), 프로젝트 경로, 개별 off, 우선순위는 모두 **미확인**이다. CN판 문서(`docs.trae.cn/ide_skills.md`)의 전역 경로가 `~/.trae-cn/skills`이므로, 국제판이 `~/.trae/skills`일 것이라는 추정은 **근거 없는 가정**이다.
- 시도한 검색 키워드: "Trae IDE skills 스킬 글로벌 경로", "trae skills global path".
- 확인 수준: 전 항목 **미확인**.

### 4.12 bob (IBM Bob) — 확인

- 앱 가정(db.rs): 전역 `.bob/skills`, 프로젝트 `.bob/skills`.
- 기본 경로(공식): 프로젝트 `.bob/skills/`(git으로 코드와 함께 추적됨), 전역(Global, all workspaces) `~/.bob/skills/`(저장소 밖).
- 추가/공용 경로: `.agents/skills` 서술은 확인한 페이지에 없다 → **미확인**.
- 개별 제외 설정: **native 확인** — 스킬 생성/편집 폼에 **"Allow Bob to use this skill"** on/off가 있다. 이 스위치가 켜져 있으면 Bob이 프롬프트에 맞을 때 스스로 그 스킬을 쓰거나 사용자가 직접 호출할 수 있고, 꺼져 있으면 이름으로 호출하지 않는다(= 자동 사용 및 명시 호출 제어). 또한 **Bob Settings → Skills** 탭에서 현재·전역 워크스페이스의 모든 스킬을 보고, 프로젝트(`.bob/skills/`)인지 전역(`~/.bob/skills/`)인지 확인하며, 편집·삭제할 수 있다.
- 동명·동일 realpath 중복: 우선순위 규칙은 확인한 페이지에 **없음 → 미확인**.
- 갱신 시점: **미확인**.
- 제안 방식: **native**("Allow Bob to use this skill" 토글).
- 근거: https://bob.ibm.com/docs/ide/tutorials/use-skills (문서 사이트는 JS 앱이며, 이번 환경에서 Python TLS가 막혀 curl로 200 응답을 받아 본문을 확인했다.)
- 확인 수준: 경로·스킬 단위 토글·설정 탭 **확인**, 우선순위·공용 경로 **미확인**.

### 4.13 codearts-agent (Huawei CodeArts Agent) — 확인

- 앱 가정(db.rs): 전역 `.codeartsdoer/skills`, 프로젝트 `.codeartsdoer/skills`. → **공식 문서와 일치.**
- 기본 경로(공식): 프로젝트 `./.codeartsdoer/skills/`(프로젝트 루트 기준, 로컬 저장), 사용자 `%USERPROFILE%/.codeartsdoer/skills`(로컬 저장). 별도로 클라우드(콘솔) 스킬도 있다.
- 추가/공용 경로: `.agents/skills` 서술은 확인한 페이지에 없다 → **미확인**.
- 개별 제외 설정: **native 확인** — 설정 페이지 CodeArts Agent IDE에서 **Skills and Rules → Project → Skills** 또는 **Skills and Rules → User → Skills**로 들어가 대상 스킬을 on/off 토글한다. **Batch Operate**로 여러 스킬을 한 번에 Enable/Disable할 수 있다. 프로젝트 스킬은 `./.codeartsdoer/skills/ProjectSkillStatus.txt`에 각 스킬의 활성 상태를 한 줄씩 **`스킬이름=true`(활성) / `스킬이름=false`(비활성)** 형식으로 기록한다. 스킬을 삭제하면 해당 기록도 정리된다. 또한 `/` 명령으로 스킬을 호출하려면 그 스킬이 **enabled 상태**여야 하고, 입력창에서 한 번에 최대 **20개**까지 선택할 수 있다.
- 동명·동일 realpath 중복: **이름이 같으면 호출 우선순위가 cloud enterprise skills > cloud team skills > local project skills > local user skills**로 명시 → 이름 기준 규칙 확인. **realpath 중복 제거는 미확인.**
- 갱신 시점: **미확인**.
- 제안 방식: **native**(토글 + `ProjectSkillStatus.txt`).
- 근거: https://support.huaweicloud.com/intl/en-us/usermanual-codeartsagent/codeartsagent_ug_0024.html (Updated on 2026-09-18 GMT+08:00)
- 확인 수준: 경로·스킬 단위 토글·상태 파일 형식·이름 우선순위 **확인**, 갱신 시점·realpath 중복 **미확인**.

### 4.14 codestudio (Syncfusion Code Studio) — 확인(경로) / 미확인(제어)

- 앱 가정(db.rs): 전역 `.codestudio/skills`, 프로젝트 `.codestudio/skills`. → **공식 문서와 일치**(정확한 제품명은 Syncfusion Code Studio).
- 기본 경로(공식): **Project Skills(저장소 내)** 는 `.codestudio/skills/`, `.github/skills/`, `.claude/skills/`, `.agents/skills/`. **Personal Skills(사용자 프로필)** 는 `~/.codestudio/skills/`, `~/.copilot/skills/`, `~/.claude/skills/`, `~/.agents/skills/`.
- 추가/공용 경로: 위 목록대로 `.agents/skills`와 `.claude/skills`, `.github/skills`, `.copilot/skills`를 **여러 개** 읽는다 → 공용·타 도구 경로를 폭넓게 스캔하므로 **중복 목록 가능성이 상대적으로 크다(실제 중복은 미확인)**.
- 개별 제외 설정: `/skills`(채팅 입력에서 Configure Skills 메뉴 열기), `+ New Skill`, `/create-skill`(AI로 생성). 스킬은 슬래시 명령으로도, 프롬프트가 맞으면 자동으로도 로드된다. **스킬 단위 비활성/토글 키는 확인한 페이지에서 찾지 못했다 → 미확인.**
- 동명·동일 realpath 중복: 이름 규칙(소문자+하이픈, 최대 64자, 디렉터리명=`name` 일치, description 최대 1024자)만 확인. 우선순위·realpath 중복 규칙은 **미확인**.
- 갱신 시점: **미확인**.
- 제안 방식: **미확인**(검증 후보).
- 근거: https://help.syncfusion.com/code-studio/reference/configure-properties/skills (15 May 2026)
- 확인 수준: 경로·공용 경로·이름 규칙 **확인**, 스킬 단위 off·우선순위 **미확인**.

## 5. 중복 처리·우선순위 종합

공식 문서에 **동명(이름 기준) 우선순위가 명시된** 곳:

| 플랫폼 | 우선순위(높음 → 낮음) |
|---|---|
| roo | `.roo/skills` > `.agents/skills` |
| junie | 프로젝트 `.junie/skills` > 사용자 `~/.junie/skills`(같은 이름이면 사용자 무시) |
| augment | `~/.augment` > `<ws>/.augment` > `~/.claude` > `<ws>/.claude` > `~/.agents` > `<ws>/.agents` |
| qoder | user-level > project-level > plugins > built-in |
| aider-desk | extension > project > global(home) > built-in |
| kilocode | 프로젝트 `.kilo/skills` > 전역 `~/.kilo/skills` (단, 호환 디렉터리·추가 경로는 "나란히 로드") |
| codearts-agent | cloud enterprise > cloud team > local project > local user |

**이름 기준 우선순위가 문서에 없는** 곳: windsurf, trae, trae-cn, qwen, bob, codestudio, continue → **미확인**.

**realpath(실제 파일 동일성) 기준 중복 제거**를 명시한 플랫폼은 이 보고서의 14개 대상에서 **찾지 못했다 → 미확인**. 따라서 여러 스캔 루트가 같은 실제 파일을 가리킬 때 목록에 두 번 올라가는지 여부는 **플랫폼별로 검증해야 하는 후보**다. "중복이 남는다"고 단정하지 않는다. 검증 없이 안전을 기하려면 심볼릭 링크로 한 파일을 여러 스캔 루트에 걸치지 않는 편이 낫다.

## 6. 공용 → 전용 설치 전환 영향 분석

### 6.1 공용 경로를 읽는다고 문서에 명시된 플랫폼

Roo, Junie, Augment, Windsurf(Devin Desktop), Kilo Code, Syncfusion Code Studio는 `.agents/skills`(또는 프로젝트 `.agents/skills`)를 읽는다고 문서에 적혀 있다. 따라서:

- 대표 출처를 공용 경로에 **그대로 두고** 전용 경로에 복사본을 추가하면, 이 플랫폼들은 공용 경로에서 계속 읽으므로 **동작 변화가 예상된다(없음에 가깝다)**. 다만 같은 이름이 공용과 전용에 동시에 있으면, 이름 우선순위가 명시된 플랫폼(Roo/Augment/Junie/CodeArts Agent)은 하나만 쓰고, **우선순위가 없거나 "나란히 로드"인 곳(Kilo, Windsurf, Code Studio)** 은 중복 목록이 될 **가능성**이 있다(실제 중복은 미확인).
- 대표 출처를 공용 경로에서 **옮기면(공용 사본 제거)**, 이 플랫폼들은 그 스킬을 더 이상 보지 못할 **가능성**이 크다. 공용 사본 제거는 이 목록을 확인한 뒤에만 판단한다.

### 6.2 공용 경로를 읽는다는 서술을 찾지 못한 플랫폼

Qwen, Qoder, Continue, AiderDesk, Trae CN, Bob, CodeArts Agent는 확인한 문서·소스에서 `.agents/skills` 서술을 찾지 못했다. 이는 **미확인**이며 "지원하지 않는다"가 아니다. 이들에게 공용 설치가 보이는지는 **겹치는 탐색 경로 검증 후 후보**다. 공용 사본을 지웠을 때 이들이 영향받는지도 미확인이다.

### 6.3 전환이 다른 플랫폼에 영향을 주지 않게 하는 조건

- 전환은 **추가(add)** 중심으로 하고, 공용 사본 제거는 그 공용 경로를 실제로 읽는 플랫폼 목록을 확인한 뒤에만 한다. 사용자 결정(외부 별도 복사본 잔존 허용)과도 맞는다.
- 심볼릭 링크로 한 파일을 여러 스캔 루트에 걸치지 않는다. 걸치면 동일 실체 중복이 생길 수 있고, 이를 제거하는 공식 규칙이 확인되지 않았다.
- 대표 출처 1개를 정하고, 나머지 복사본은 내용 동일성을 주기적으로 비교한다. 내용이 갈라지면 대표 출처를 유지하고 알림만 준다(사용자 결정).
- 플랫폼 전용 경로에 넣는 변경은 그 플랫폼만 읽으므로 다른 플랫폼에 영향이 없다. 유일한 예외는 **공용 경로 자체를 건드리는 경우**다.

### 6.4 공용 경로를 읽는 도구와 그렇지 않은 도구의 한계

공용 경로(`.agents/skills`)를 문서에 명시한 도구는 그 경로가 비면 스킬을 잃을 수 있다. 반대로 공용 경로 서술이 확인되지 않은 도구(`6.2)는 공용 설치가 보이는지 자체가 미확인이므로, "공용 설치 하나로 모든 플랫폼 커버"는 **어느 방향으로도 단정할 수 없다.** 플랫폼별 on/off를 하려면 각 플랫폼의 전용 경로 또는 native 제어를 개별로 다루고, 겹치는 탐색 경로를 플랫폼별로 검증해야 한다.

## 7. 한계와 미확인 범위

- **trae(국제판)**: 문서가 SPA라 본문 확인 실패. 전역/프로젝트 경로, 개별 off, 우선순위 전부 미확인. 앱의 `~/.trae/skills` 가정은 검증되지 않았다.
- **공용 경로 서술 없음**: Qwen, Qoder, Continue, AiderDesk, Trae CN, Bob, CodeArts Agent에서 `.agents/skills` 서술을 찾지 못했다. **미확인**이며 미지원 단정이 아니다.
- **스킬 단위 off 키/방식**: roo, windsurf, kilocode, continue, codestudio는 미확인. qwen(저장 키), augment(키 이름), junie(키 이름), trae-cn(전역 off 위치·필드명), qoder(스킬 off 키)도 미확인. 확인된 native는 qwen(패널 토글), junie(비활성 동작), augment(Skill Modes), trae-cn(프로젝트 한정), bob(Allow Bob to use this skill), codearts-agent(토글 + `ProjectSkillStatus.txt`), qoder(조건부 노출이며 개별 off는 아님)다.
- **개별 off로 보지 않는 것**: AiderDesk의 Skills Tools 게이트(프로필/태스크 단위), Qoder의 조직 feature flag, Kilo의 `skills.paths`/`skills.urls`(추가 경로). 이들은 개별 스킬 off 수단이 아니다.
- **realpath 기준 중복 제거**: 이 보고서의 14개 대상에서는 명시를 찾지 못했다. 중복 여부는 검증 후보이며 "중복이 남는다"고 단정하지 않는다.
- **갱신(재스캔) 시점**: kilocode(세션 시작), qoder(`/skills reload`), continue(도구 초기화 시)만 확인. 나머지는 미확인.
- **GitHub `main` 참조**: Qwen, Continue, AiderDesk 근거는 이동하는 `main` 브랜치 파일이다. 고정 리비전(커밋 해시)은 이번 조사에서 확정하지 못했다. Junie 문서는 빌드 타임스탬프(2026-09-21T12:30:08Z), CodeArts Agent 문서는 Updated 2026-09-18 GMT+08:00, Code Studio 문서는 15 May 2026을 확인했다.
- **앱 가정과 공식 문서 불일치**: kilocode(앱 `.kilocode` vs 공식 `.kilo`), roo/junie/augment/windsurf/kilocode/codestudio의 `.agents/skills` 미모델링, trae-cn CLI의 `~/.traecli/skills` 미모델링. 앱의 경로 가정을 그대로 믿으면 안 된다는 근거다.
- **정정 이력**: 1차 문서에서 bob·codearts-agent·codestudio를 "미확인"으로 두고 codestudio의 제품 정체를 불명확하다고 적었으나, 2차에서 세 곳 모두 공식 문서를 확인했다. codestudio는 **Syncfusion Code Studio**로 정정한다.

## 부록 A. 시도한 공식 URL·검색 키워드(실패 포함)

성공(본문 확인):

- https://docs.roocode.com/features/skills
- https://raw.githubusercontent.com/QwenLM/qwen-code/main/docs/users/features/skills.md
- https://docs.windsurf.com/windsurf/cascade/skills
- https://docs.augmentcode.com/using-augment/skills.md
- https://junie.jetbrains.com/docs/agent-skills.html
- https://raw.githubusercontent.com/hotovo/aider-desk/main/docs-site/docs/agent-mode/skills.md
- https://docs.trae.cn/ide_skills.md, https://docs.trae.cn/cli_skills.md
- https://docs.qoder.com/qoder/skills.md, https://docs.qoder.com/cli/Skills.md, https://docs.qoder.com/cli/troubleshoot-loading.md, https://docs.qoder.com/llms.txt
- https://kilo.ai/docs/customize/skills
- https://raw.githubusercontent.com/continuedev/continue/main/core/config/markdown/loadMarkdownSkills.ts, .../extensions/cli/src/util/loadMarkdownSkills.ts, .../core/util/paths.ts
- https://bob.ibm.com/docs/ide/tutorials/use-skills (Python urllib은 TLS 오류, curl로 200 응답·본문 확인)
- https://support.huaweicloud.com/intl/en-us/usermanual-codeartsagent/codeartsagent_ug_0024.html
- https://help.syncfusion.com/code-studio/reference/configure-properties/skills

실패 또는 부분 실패:

- https://docs.trae.ai/ide/skills (200, SPA HTML), https://docs.trae.ai/llms.txt (라우터 블롭), https://docs.trae.ai/sitemap.xml (skill URL 0건)
- https://bob.ibm.com/, https://bob.ibm.com/docs, https://bob.ibm.com/docs/skills (Python TLS 오류. bob.ibm.com/docs/ide/tutorials/use-skills는 curl로 성공)
- https://support.huaweicloud.com/intl/en-us/codeartsdoer/index.html, https://support.huaweicloud.com/codeartsdoer/, https://support.huaweicloud.com/usermanual-codeartsdoer/codeartsdoer_01_0001.html, https://www.huaweicloud.com/product/codeartsdoer.html (404 — 잘못된 제품/문서 경로 추정이었고, 올바른 경로는 usermanual-codeartsagent)
- https://codestudio.dev/ (주차 페이지 — Syncfusion Code Studio 문서가 아님)
- https://docs.continue.dev/customize/skills (404), https://docs.continue.dev/llms.txt ("skill" 항목 없음)
- https://kilo.ai/docs/customize/skills.md (404, `.md` 없이 재시도해 성공)
- https://kilocode-docs.vercel.app/api/raw-markdown?path=%2Fcustomize%2Fskills (404)

검색 키워드: "IBM Bob agent skills SKILL.md", "IBM Bob AI 코딩 에이전트 skills SKILL.md", "Huawei CodeArts Doer 에이전트 스킬 SKILL.md", "华为 CodeArts 智能体 skills", "CodeArts Doer 技能 SKILL.md", "Trae IDE skills 스킬 글로벌 경로", "trae skills global path", "Code Studio AI skills SKILL.md".

## 부록 B. 앱 내부 가정(db.rs)과 공식 문서 차이 요약

`src-tauri/src/db.rs`의 `builtin_agents()`는 각 플랫폼을 `(id, display_name, category, 전역(home) 상대경로, 프로젝트 상대경로, 아이콘)` 형태로 정의한다. 즉 4번째 인자가 **홈 기준 전역 경로**, 5번째가 **프로젝트 경로**다.

| ID | 앱 가정(전역 / 프로젝트) | 공식 문서 | 차이 |
|---|---|---|---|
| aider-desk | `.aider-desk/skills` / `.aider-desk/skills` | `~/.aider-desk/skills`, `.aider-desk/skills` | 일치 |
| trae | `.trae/skills` / `.trae/skills` | 미확인(SPA) | 검증 안 됨 |
| trae-cn | `.trae-cn/skills` / `.trae/skills` | `~/.trae-cn/skills`, `<proj>/.trae/skills` | 일치(CLI `~/.traecli/skills` 미모델링) |
| junie | `.junie/skills` / `.junie/skills` | `~/.junie/skills`, `<proj>/.junie/skills` | 일치(`.agents/skills` 미모델링) |
| qwen | `.qwen/skills` / `.qwen/skills` | `~/.qwen/skills`, `.qwen/skills` | 일치 |
| windsurf | `.codeium/windsurf/skills` / `.windsurf/skills` | `~/.codeium/windsurf/skills`, `.windsurf/skills` | 일치(`.agents`/`.claude` 미모델링) |
| qoder | `.qoder/skills` / `.qoder/skills` | `~/.qoder/skills`, `.qoder/skills` | 일치 |
| augment | `.augment/skills` / `.augment/skills` | `~/.augment/skills`, `<ws>/.augment/skills` | 일치(`.claude`/`.agents` 미모델링) |
| kilocode | `.kilocode/skills` / `.kilocode/skills` | `~/.kilo/skills`, `.kilo/skills` | **불일치 가능** |
| bob | `.bob/skills` / `.bob/skills` | `~/.bob/skills`, `.bob/skills` | 일치 |
| codearts-agent | `.codeartsdoer/skills` / `.codeartsdoer/skills` | `%USERPROFILE%/.codeartsdoer/skills`, `./.codeartsdoer/skills/` | 일치 |
| codestudio | `.codestudio/skills` / `.codestudio/skills` | `~/.codestudio/skills/`, `.codestudio/skills/` | 일치(Syncfusion Code Studio. `.agents`/`.claude`/`.github`/`.copilot` 미모델링) |
| continue | `.continue/skills` / `.continue/skills` | `~/.continue/skills`, `.continue/skills`, `.claude/skills` | 일치(`.claude` 미모델링) |
| roo | `.roo/skills` / `.roo/skills` | `~/.roo/skills`, `.roo/skills`, `.agents/skills` | 일치(`.agents` 미모델링) |

이 표는 앱이 경로를 가정한 값이지 외부 런타임의 증거가 아님을 다시 강조한다.
