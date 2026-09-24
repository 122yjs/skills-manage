# 주요 플랫폼 스킬 제어 근거 검토

2026-09-21. 대상: OMP, AGY CLI, Antigravity, Cursor, Kiro, CodeBuddy, OpenClaw.
DeepSeek v4.1 조사 뒤 부모가 공식 원문과 대조하여 정정했다. 실제 설정·스킬 변경과 런타임 제외 시험은 하지 않았다. [전체 조사표](platform-skill-capabilities-2026-09-21.md)와 [구현 명세](../platform-skill-isolation-spec.md)를 기준으로 사용한다.

## OMP 18.2.7

설치 버전은 18.2.7. [버전 고정 문서](https://raw.githubusercontent.com/can1357/oh-my-pi/v18.2.7/docs/skills.md), [설정](https://raw.githubusercontent.com/can1357/oh-my-pi/v18.2.7/docs/config-usage.md), [스키마](https://raw.githubusercontent.com/can1357/oh-my-pi/v18.2.7/packages/coding-agent/src/config/settings-schema.ts), [로더](https://raw.githubusercontent.com/can1357/oh-my-pi/v18.2.7/packages/coding-agent/src/extensibility/skills.ts)를 확인했다.

- 글로벌 ~/.omp/agent/config.yml 또는 config.yaml. 프로젝트 설정·PI_CONFIG_FILES·--config·실행 중 덮어쓰기의 순위를 따진다.
- skills.ignoredSkills는 **스킬 이름** glob 배열. skills.includeSkills는 허용목록, disabledExtensions의 skill:name도 제외에 쓰인다. 경로별 사본 선택 기능과 다르다.
- 네이티브 .omp, 플러그인, Claude, agents, Codex, OpenCode, GitHub, 관리 스킬 공급자가 있다. 일부 사용자 공급자는 선택적으로 켜진다.
- 공급자 우선순위로 이름 하나를 선택하고 같은 실제 파일 경로(realpath)도 제거한다. 이는 서로 다른 두 파일의 내용 해시 비교가 아니다. 내용이 같다는 이유만으로 모든 사본을 자동 합친다고 표현하지 않는다.
- customDirectories는 추가 경로이며 같은 이름의 기본 공급자보다 우선한다. 여러 사용자 추가 경로 안에서는 먼저 나온 출처가 우선이다.
- hide/disableModelInvocation은 목록에서만 숨기며 skill://와 명시 호출은 남을 수 있다.
- 전역 제외가 프로젝트·실행 옵션에 의해 바뀌는 경우 및 현재 세션 재로드는 실제 버전 fixture로 검증해야 한다.

앱의 프로젝트 등록값은 프로젝트 기준 .omp/skills다. ~/.omp/skills로 잘못 해석하지 않는다. 네이티브 경로만 확인해 전체 호환 탐색이 끝났다고 보지 않는다.

## AGY CLI 1.2.7

앱 내부 ID gemini-cli는 현재 AGY CLI를 뜻한다. Google Gemini CLI의 설정을 대신 적용하면 안 된다.

최신 [공식 웹 문서](https://www.antigravity.google/docs/skills/)는 CLI 글로벌을 ~/.gemini/antigravity-cli/skills, 플러그인을 그 하위 plugins로 설명한다. 반면 **설치본 1.2.7에 포함된 문서**는 글로벌 탐색을 ~/.gemini/config로 적는다. 웹 문서를 “1.2.7에서 검증된 경로”라고 부르면 안 된다. 실제 로더가 어느 경로를 읽는지 추가 확인이 필요하다.

확인한 설치본 공식 문서:

- [커스터마이징 가이드](/Users/jsm2/.gemini/antigravity-cli/builtin/skills/agy-customizations/SKILL.md)
- [JSON 설정](/Users/jsm2/.gemini/antigravity-cli/builtin/skills/agy-customizations/docs/json_configs.md)
- [스킬 문서](/Users/jsm2/.gemini/antigravity-cli/builtin/skills/agy-customizations/docs/skills.md)

JSON 문서는 skills.json의 entries/inherits 각 항목에 path, include_only, exclude를 지원한다고 설명한다. 필터는 해당 경로의 디렉터리 이름을 대상으로 한다. 명시 설정 경로의 최상위 exclude는 사용자 환경에서 상속한 커스터마이징을 걸러내지 않는다고 적혀 있다.

따라서 **skills.json을 추가하면 자동 발견된 공용 스킬까지 전역 차단된다**고 해석할 수 없다. 명시 등록 출처에서는 후보지만, 기본 발견 경로·상속 출처 전체에 대한 독립 제외는 미확인이다. include/exclude의 패턴 문법과 우회 출처까지 검증해야 한다.

설치본 문서는 작업공간 발견 > 작업공간 선언 > 글로벌 발견 > 내장 > 글로벌 선언의 동명 우선순위를 설명한다. 또한 resolved file paths로 커스터마이징을 중복 제거한다고 명시한다. 규칙 파일의 한 턴 중복 주입 방지를 예로 들며, 서로 다른 사본 내용 비교나 SKILL.md 본문 재독까지 막는 증거는 아니다.

--disable-slash-commands는 print 모드의 명령/스킬 확장 제어다. 플랫폼의 스킬 하나를 끄는 설정으로 사용하지 않는다. 재로드 시점은 미확인이다.

## Antigravity 2.0 / IDE

[공식 문서](https://www.antigravity.google/docs/skills/)의 2.0·IDE 글로벌은 ~/.gemini/config/skills, 프로젝트는 .agents/skills다. 구형 .agent 및 IDE legacy 경로도 버전에 따라 확인해야 한다.

CLI 내장 skills.json 문서를 근거로 IDE에서도 같은 제외가 된다고 단정하지 않는다. 공유 설정 파일이면 CLI/IDE 간 독립성을 별도 검증해야 한다. 개별 제외·동명 처리·즉시 갱신은 현재 미확인이다.

## Cursor

[공식 문서](https://cursor.com/docs/skills/)는 사용자·프로젝트의 .cursor/.agents와 Claude/Codex 호환 경로를 설명한다. 시작 시 발견하며 하위 프로젝트 스킬에는 파일 범위가 적용된다.

원본 frontmatter의 disable-model-invocation은 자동 선택을 줄이지만 공용 원본 변경은 다른 플랫폼에도 영향을 줄 수 있다. 플랫폼 전역 개별 제외 키·동명/realpath 규칙은 미확인이다.

공용에서 Claude 폴더로 옮겨도 Cursor가 계속 읽을 수 있다. 따라서 전용 복사본 생성만으로 격리가 안전하다고 판단하지 않는다. 다른 플랫폼 보존과 모든 겹치는 경로를 검증한 경우에만 경로 전환을 제안한다.

## Kiro

[공식 스킬 문서](https://kiro.dev/docs/skills/)는 .kiro/skills와 ~/.kiro/skills, 동명 프로젝트 우선을 설명한다. custom agent의 resources에 skill:// 경로를 넣을 수 있다.

[설정 참조](https://kiro.dev/docs/custom-agents/configuration-reference/)의 리소스 구성은 해당 agent 범위다. 기본 리소스 상속을 끄면 스킬 외 지침에도 영향이 있을 수 있다. 이를 모든 Kiro 실행의 스킬 하나 제외로 해석하지 않는다. 개별 제외 키·realpath·반영 시점은 미확인이다.

## CodeBuddy

[공식 문서](https://www.codebuddy.ai/docs/cli/skills)는 skillOverrides[name]=off로 모델 목록·명령 메뉴에서 숨기고 이름 기반 호출도 거부한다고 명시한다. 플러그인은 예외다.

설정 우선순위는 .codebuddy/settings.local.json > .codebuddy/settings.json > ~/.codebuddy/settings.json. 이름별 상위 설정을 확인해야 하며 전역 파일 수정만으로 프로젝트 제외를 보장하지 않는다. /skills는 Esc 때 프로젝트 local 설정을 저장한다. 외부 편집 후 세션 반영 시점은 검증이 남았다. 회사가 같아도 WorkBuddy에 같은 설정을 그대로 적용하지 않는다.

## OpenClaw

[설정 문서](https://docs.openclaw.ai/tools/skills-config)의 **skills.entries[key].enabled=false**가 스킬 하나 제외의 우선 후보다. key는 보통 이름이지만 metadata.openclaw.skillKey가 있으면 그 값을 따른다.

[로딩 문서](https://docs.openclaw.ai/tools/skills)는 workspace skills > 프로젝트 .agents > 개인 .agents(기본 state) > 관리/state skills 등의 동명 우선순위를 설명한다. snapshots와 watch가 있으나 실제 설치 버전의 설정 반영은 별도 확인해야 한다.

에이전트별 agents.entries.<id>.skills는 기본 목록을 대체한다. 키를 **생략**하면 defaults를 상속하고 **빈 배열 []**이면 스킬이 없다. 한 스킬을 끄려고 나머지 모든 스킬의 허용목록을 새로 고정하는 방식을 기본으로 삼지 않는다.

이 조사에서는 OpenClaw 실행 확인에 실패했다. 하위 에이전트가 실행한 환경에서 Node 버전 조건 오류를 보고했으며, 정상 PATH의 설치본도 고장났다고 확대하지 않는다.

## 조사 실패와 제한

DeepSeek 공급자 429가 있었으며 다른 모델로 대체하지 않았다. 웹 검색이 없는 하위 에이전트는 공식 URL 직접 HTTP 조회를 사용했다. 오래된 URL의 404나 페이지 검색 결과 부재를 기능 부재로 판정하지 않았다. 주요 시도 URL은 본문의 공식 링크 및 구형 cursor.com/docs/agent/skills다.

문서상 기능, 앱 연결, 설정 적용, 새 세션 목록, 본문 로딩 검증은 서로 다른 단계다. 이번 결과는 문서·코드 조사와 명세이며 실제 컨텍스트 절감 검증은 남아 있다.
