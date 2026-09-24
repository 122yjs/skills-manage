# 플랫폼별 스킬 제어 조사 — 2026-09-21

## 결론과 확인 범위

플랫폼별 스킬 제외 기능은 일부 플랫폼에 이미 있지만 현재 앱은 대부분 연결하지 않았다. 모든 등록 플랫폼을 같은 화면에서 다루는 목표는 유지한다. 다만 전용 폴더도 다른 플랫폼의 호환 탐색 경로와 겹칠 수 있어 모든 조합에서 독립 제어가 가능하다고 아직 보장할 수 없다.

기준 코드는 8027712. 등록 62개 중 universal·central을 제외한 실제 플랫폼 **60개**를 모두 기록했다. 이는 조사 범위의 완전성이지 실행 검증 완료를 뜻하지 않는다. 최신 공식 문서와 이동 가능한 main 소스를 확인했으며 출시 버전 고정 검증이 남아 있다.

로컬에서 실행한 것은 버전·도움말 조회다: OMP 18.2.7, AGY 1.2.7. 실제 스킬 끄기, 모델 목록/본문 측정, 실사용 설정 수정은 수행하지 않았다. 토큰 절감 수치나 설치본의 동작 보장을 제공하지 않는다.

DeepSeek 조사 모델은 bai/deepseek-v4.1-flash로 지정했다. 공급자 429로 실패한 범위는 부모가 공식 자료로 보완했다. [주요 7개 플랫폼 검토](platform-skills-priority.md)와 [IDE 상세 보고서](platform-skills-ide.md)는 별도다. 공식 문서에 없다는 이유로 미지원이라고 단정하지 않는다.

## 코드 근거

- [등록 정보](../../src-tauri/src/db.rs): 공용을 기본 경로로 둔 실제 플랫폼 7개. 등록은 실제 지원 증거가 아니다.
- [탐색기](../../src-tauri/src/commands/scanner.rs): scan_roots_for_agent/loading_paths_for_agent. compatibility 경로와 공용 설치를 구분해야 한다.
- [플랫폼 제어](../../src-tauri/src/commands/platform_skill_control.rs): Claude·Codex와 CodeBuddy·OMP·OpenClaw·Factory Droid·Command Code·Mistral Vibe·Hermes·OpenCode 설정 Adapter를 연결했다. Unsupported는 앱 미구현/미확인이다.
- [중복 비교](../../src-tauri/src/commands/skill_duplicates.rs): 실제 경로 및 보조 파일·실행 비트 비교, 추가 중복 설치 방지. 에이전트의 목록을 재작성하지는 않는다.
- [사용 상태](../../src-tauri/src/commands/usage.rs)·[설치](../../src-tauri/src/commands/linker.rs): 영향 확인·이동/복원·설치 전 검사를 재사용할 수 있다. 여러 단계 전환의 충돌·중단 복구는 보강해야 한다.

## 표 읽는 법

**N**은 개별 제외 설정/CLI 근거, **U**는 UI 토글은 있으나 저장 방식·효과 미확인, **L**은 실행 옵션·탐색 범위 제어, **P**는 경로 변경 후보, **?**는 근거 부족이다. N도 앱 자동 연결·실행 검증을 마친 상태가 아니다. P는 겹치는 모든 경로와 다른 플랫폼 보존을 검증하기 전 지원으로 표시하지 않는다.

등록 경로는 현재 앱 값이다. 프로젝트의 —는 앱에서 미지정이라는 뜻이다. 별도 표기가 없는 realpath 중복 제거·즉시 반영·설치 버전 적용 가능성은 미확인이다.

| ID / 플랫폼 | 앱 등록 전역 / 프로젝트 | 등급 | 확인 범위와 남은 확인 | 중복·반영 시점 | 근거 |
|---|---|---|---|---|---|
| claude-code / Claude Code | ~/.claude/skills<br>.claude/skills | N/기존 연결 | enterprise·개인·프로젝트·플러그인; skillOverrides[name]=off, 플러그인 제외 | 동일 링크 대상 1회; 동명 출처·플러그인 이름 공간 구분. 반영 시점 버전 검증 | [공식 자료](https://code.claude.com/docs/en/skills) |
| codex / Codex CLI | ~/.agents/skills<br>— | N/기존 연결 | 공용 + 조상 프로젝트 .agents + .codex·플러그인; skills.config 경로 제외 | 동명 자동 병합 안 함. 설정 뒤 재시작 검증; 디렉터리/SKILL.md 경로 문서 차이 확인 | [공식 자료](https://learn.chatgpt.com/docs/build-skills) |
| cursor / Cursor | ~/.cursor/skills<br>— | P/미확인 | 개인·프로젝트 .agents/.cursor/.claude/.codex; 별도 제외 키 미확인 | 중복 규칙 미확인; 시작 시 탐색, 하위 프로젝트 범위 적용. 전용 폴더 이동만으로 격리 보장 못함 | [공식 자료](https://cursor.com/docs/skills) |
| antigravity / Antigravity | ~/.gemini/config/skills<br>.agents/skills | P/부분 | 2.0/IDE 글로벌 .gemini/config/skills, 프로젝트 .agents, 구형 .agent 호환; off 미확인 | 동명·realpath 규칙 미확인; 새 대화 목록 확인 필요 | [공식 자료](https://www.antigravity.google/docs/skills/) |
| cline / Cline | ~/.agents/skills<br>— | U/부분 | 문서 기본은 ~/.cline/skills, 프로젝트 .cline/.clinerules/.claude; UI 개별 토글, 저장 키 미확인 | 동명 글로벌 우선. 앱의 공용 기본값과 차이; 자동 감지·토글 반영 검증 필요 | [공식 자료](https://docs.cline.bot/customization/skills) |
| deep-agents / Deep Agents | ~/.agents/skills<br>— | ?/부분 | SDK의 skills 배열은 명시 경로, 생략시 없음. CLI 기본 경로와 분리 조사 필요 | SDK는 뒤 출처 우선. CLI에 동일 규칙 적용 여부·갱신 미확인 | [공식 자료](https://docs.langchain.com/oss/python/deepagents/skills) |
| dexto / Dexto | ~/.agents/skills<br>— | ?/부분 | 공식 페이지는 Desktop Skills Library 설명; 로컬 CLI .agents 경로·개별 제외 키 미확인 | 동명·realpath·갱신 미확인; Desktop/CLI 제품 구분 필요 | [공식 자료](https://www.dexto.ai/docs/features/skills/) |
| firebender / Firebender | ~/.agents/skills<br>— | P/부분 | 전용 .firebender 외 글로벌 .goose/.claude/.codex/.cursor/.agents 호환; off 키 미확인 | 동명 팀 우선. 호환 경로간 우선순위·realpath·갱신 미확인 | [공식 자료](https://docs.firebender.com/multi-agent/skills) |
| gemini-cli / AGY CLI | ~/.gemini/config/skills<br>.agents/skills | N/명시 출처 한정 | skills.json entries/inherits의 exclude·include_only. 자동 발견 출처 전체 제외는 미확인 | 1.2.7 내장 문서는 config, 최신 웹은 antigravity-cli 경로. 동명 우선순위·resolved path dedup 문서 확인, 로더 대조 필요 | [공식 자료](https://www.antigravity.google/docs/skills/) |
| kimi-code-cli / Kimi Code CLI | ~/.agents/skills<br>— | L/부분 | --skills-dir로 자동 탐색 경로 대체; extra_skill_dirs는 추가. 기본 경로 별도 버전 검증 | 실행 옵션은 그 실행에만 적용; 기본 실행·중복·갱신 검증 필요 | [공식 자료](https://www.kimi.com/code/docs/en/kimi-code-cli/reference/kimi-command) |
| aider-desk / AiderDesk | ~/.aider-desk/skills<br>.aider-desk/skills | P/부분 | 전용 경로·내장·확장. Skills Tools는 전체 스킬 도구 게이트이며 개별 off 아님 | 확장>프로젝트>글로벌>내장. realpath·재스캔 미확인 | [공식 자료](https://aiderdesk.hotovo.com/docs/agent-mode/skills) |
| trae / Trae | ~/.trae/skills<br>.trae/skills | ?/미확인 | 공식 웹 문서 본문 취득 실패. CN판 설정을 그대로 적용하지 않음 | 경로·개별 제외·중복·갱신 미확인 | [공식 자료](https://docs.trae.ai/ide/skills) |
| factory-droid / Factory Droid | ~/.factory/skills<br>.factory/skills | N/앱 연결 | 전용 + .agents/.agent + 플러그인·mission. settings.json disabledSkills 이름 목록 | 사용자 설정 Adapter 왕복 검증. 프로젝트 차단 합집합과 실행 세션 반영 검증은 남음 | [공식 자료](https://docs.factory.ai/harness/skills) |
| junie / Junie | ~/.junie/skills<br>.junie/skills | U/부분 | 전용 + .agents; 개별 비활성 기능, 추가 skill-locations. 저장 키 미확인 | 프로젝트 동명 우선; --skill-default-locations false는 여러 스킬에 영향. 갱신 미확인 | [공식 자료](https://junie.jetbrains.com/docs/agent-skills.html) |
| qwen / Qwen Code | ~/.qwen/skills<br>.qwen/skills | U/부분 | 전용 경로·확장; /skills 토글. 공용 지원·저장 키 미확인 | 확장 이름 공간 분리; 로컬 동명·realpath·갱신 미확인 | [공식 자료](https://raw.githubusercontent.com/QwenLM/qwen-code/main/docs/users/features/skills.md) |
| trae-cn / Trae CN | ~/.trae-cn/skills<br>.trae/skills | U/부분 | IDE 전용 경로. 프로젝트 비활성 목록 .trae/skill-config.json; 전역 키 미확인 | CLI .traecli와 IDE 구분. 중복·갱신 미확인 | [공식 자료](https://docs.trae.cn/ide_skills.md) |
| windsurf / Windsurf | ~/.codeium/windsurf/skills<br>.windsurf/skills | P/부분 | 전용 + .agents, 선택적 Claude 호환, 시스템 배포. 개별 제외 키 미확인 | 현재 공식 URL은 Devin Desktop 문서로 이동; Terminal 제품과 별개. 중복·갱신 미확인 | [공식 자료](https://docs.windsurf.com/windsurf/cascade/skills) |
| qoder / Qoder | ~/.qoder/skills<br>.qoder/skills | P/부분 | 전용·플러그인. 경로 조건·조직 feature flag는 개별 off 대체 아님 | 사용자>프로젝트>플러그인>내장; /skills reload. realpath 미확인 | [공식 자료](https://docs.qoder.com/cli/Skills.md) |
| augment / Augment | ~/.augment/skills<br>.augment/skills | U/부분 | 전용·Claude·공용 순의 탐색, 사용자 우선; Auto/Manual/Disabled 모드 | 동명 우선순위 있음. Disabled의 목록 제거 여부·설정 키·반영 미확인 | [공식 자료](https://docs.augmentcode.com/using-augment/skills.md) |
| opencode / OpenCode | ~/.opencode/skills<br>— | N/앱 연결 | 실제 글로벌 ~/.config/opencode/skills + .claude/.agents; v1 permission.skill 및 v2 permissions deny | 두 형식을 기존 설정에서 식별할 때 이름 단위 왕복 검증. JSONC 주석·형식 미식별은 쓰기 거부; 프로젝트/관리 설정과 실행 반영은 남음 | [공식 자료](https://opencode.ai/docs/skills/) |
| kilocode / Kilo Code | ~/.kilocode/skills<br>.kilocode/skills | P/버전 확인 | 현재 문서 .kilo/skills + .agents + 선택적 .claude; 앱은 .kilocode. skills.paths는 추가 | 프로젝트 동명 우선; 호환 출처간 중복 규칙·개별 off 미확인. 세션 시작 탐색 | [공식 자료](https://kilo.ai/docs/customize/skills) |
| ob1 / OB1 | ~/.ob1/skills<br>— | ?/미확인 | OB-1과 다른 동명 제품 식별부터 필요; 등록 폴더를 런타임 증거로 쓰지 않음 | 개별 제외·중복·갱신 미확인 | [공식 자료](https://github.com/Overbrilliant/ob-1) |
| amp / Amp | ~/.amp/skills<br>— | L/문서 | ~/.config/agents, ~/.agents, ~/.config/amp, 프로젝트 .agents/.claude, 플러그인·추가 경로 | 동명 첫 출처 우선; disableGlobalAgentsSkills/disableClaudeCodeSkills는 범위 전체. reload_skills | [공식 자료](https://ampcode.com/docs/customize/skills) |
| kiro / Kiro CLI | ~/.kiro/skills<br>.kiro/skills | P/L 부분 | 전용 경로; custom agent resources skill://. 개별 제외 키 미확인 | 프로젝트 동명 우선; 기본 resource 상속 차단은 steering에도 영향, 전체 Kiro에 적용 불가. 갱신 확인 | [공식 자료](https://kiro.dev/docs/skills/) |
| codebuddy / CodeBuddy | ~/.codebuddy/skills<br>.codebuddy/skills | N/앱 연결 | 전용·프로젝트·플러그인; skillOverrides[name]=off, 플러그인은 별도 | 기본 사용자 설정 Adapter와 round-trip 검증. project-local>project>user 및 실행 세션 반영 검증은 남음 | [공식 자료](https://www.codebuddy.ai/docs/cli/skills) |
| bob / IBM Bob | ~/.bob/skills<br>.bob/skills | U/부분 | 공식 IDE 스킬 UI·Allow Bob to use this skill 확인; 저장 키·공용 로딩 미확인 | 동명·realpath·갱신 미확인 | [공식 자료](https://bob.ibm.com/docs/ide/tutorials/use-skills) |
| codearts-agent / CodeArts Agent | ~/.codeartsdoer/skills<br>.codeartsdoer/skills | U/부분 | 공식 IDE 프로젝트 ProjectSkillStatus.txt의 이름=true/false; 전역 저장 키 미확인 | IDE는 enterprise>team>project>user 동명 우선. CLI·realpath·끄기 반영 미확인 | [공식 자료](https://support.huaweicloud.com/intl/en-us/usermanual-codeartsagent/codeartsagent_ug_0024.html) |
| codemaker / Codemaker | ~/.codemaker/skills<br>.codemaker/skills | ?/미확인 | 검색: Codemaker skills SKILL.md official. 제3자 설치 목록만으로 지원 판정 안 함 | 공식 로더·개별 제외·중복·갱신 미확인 | 공식 로더 근거 미확보 |
| codestudio / Code Studio | ~/.codestudio/skills<br>.codestudio/skills | P/부분 | Syncfusion Code Studio 전용 및 .agents/.claude/.github/.copilot 호환 경로. 개별 off 미확인 | 개별 제어의 저장 방식·동명·realpath·갱신 미확인 | [공식 자료](https://help.syncfusion.com/code-studio/reference/configure-properties/skills) |
| command-code / Command Code | ~/.commandcode/skills<br>.commandcode/skills | N/앱 연결 | 전용 글로벌/프로젝트; settings.json disabledSkills 이름 목록 | 사용자 설정 Adapter 왕복 검증. 프로젝트 합집합·동명 우선순위와 다음 세션 반영 검증은 남음 | [공식 자료](https://commandcode.ai/docs/skills) |
| continue / Continue | ~/.continue/skills<br>.continue/skills | P/부분 | CLI .continue/.claude + 글로벌; IDE 경로 차이 있음. CONTINUE_GLOBAL_DIR는 전체 홈 변경 | 개별 off·동명·realpath 미확인; loadMarkdownSkills 도구 초기화 시점 | [공식 자료](https://raw.githubusercontent.com/continuedev/continue/main/extensions/cli/src/util/loadMarkdownSkills.ts) |
| cortex / Cortex Code | ~/.snowflake/cortex/skills<br>.cortex/skills | ?/부분 | 공식 Cortex Desktop skills.json 확인; 앱 대상 CLI와 동일하다는 증거 미확인 | 개별 제외·중복·갱신·제품 범위 확인 필요 | [공식 자료](https://docs.snowflake.com/en/user-guide/cortex-code/cortex-code-desktop/skills) |
| crush / Crush | ~/.config/crush/skills<br>.crush/skills | P/부분 | options.skills_paths 및 글로벌 스킬 경로; CRUSH_SKILLS_DIR. 경로 추가와 차단 구분 필요 | 개별 off·기본 프로젝트 탐색·중복·갱신은 버전 고정 후 검증 | [공식 자료](https://github.com/charmbracelet/crush) |
| devin / Devin for Terminal | ~/.config/devin/skills<br>.devin/skills | ?/미확인 | Devin for Terminal과 Windsurf/Devin Desktop 자료 구분 필요 | 검색: Devin terminal skills site:docs.devin.ai. 등록 경로·off·중복·갱신 미확인 | 공식 로더 근거 미확보 |
| forgecode / ForgeCode | ~/.forge/skills<br>.forge/skills | P/부분 | 공식 글로벌 ~/forge/skills(점 없음), 프로젝트 .forge 및 글로벌 .agents. 앱 경로와 차이 | 프로젝트>agents>글로벌>내장; :skill 목록. off·realpath·갱신 미확인 | [공식 자료](https://forgecode.dev/docs/skills/) |
| goose / Goose | ~/.config/goose/skills<br>.goose/skills | ?/부분 | 공식 CLI의 skills 목록 명령·스킬 기능 확인; 전체 탐색 경로 추가 확인 | 개별 제외·중복·갱신 미확인 | [공식 자료](https://github.com/aaif-goose/goose/blob/main/documentation/docs/guides/goose-cli-commands.md) |
| iflow-cli / iFlow CLI | ~/.iflow/skills<br>.iflow/skills | P/부분 | 공식 ~/.iflow/skills 설치·SKILL.md 지원. 세부 로더 경로 확인 필요 | 개별 제외·중복·갱신 미확인 | [공식 자료](https://platform.iflow.cn/cli/examples/skill) |
| kode / Kode | ~/.kode/skills<br>.kode/skills | ?/부분 | 공식 Kode 계열 저장소 확인. Kode-Agent/CLI/SDK 중 탐지 대상 식별 필요 | 개별 제외·중복·갱신 미확인 | [공식 자료](https://github.com/shareAI-lab/Kode-Agent) |
| mcpjam / MCPJam | ~/.mcpjam/skills<br>.mcpjam/skills | P/문서 경로 | 글로벌/프로젝트 .claude, .mcpjam, .agents; 업로드·원격 출처 별도 | 동명 탐색순 첫 출처 우선. 개별 off·갱신·realpath 미확인 | [공식 자료](https://docs.mcpjam.com/inspector/skills) |
| mistral-vibe / Mistral Vibe | ~/.vibe/skills<br>.vibe/skills | N/앱 연결 | skill_paths + 신뢰한 프로젝트 .vibe/.agents + ~/.vibe; enabled_skills/disabled_skills | denylist 기본 모드 왕복·TOML 주석 보존 검증. allowlist·광범위 패턴은 쓰기 거부; /reload 실행 검증은 남음 | [공식 자료](https://docs.mistral.ai/vibe/code/cli/skills) |
| mux / Mux | ~/.mux/skills<br>.mux/skills | ?/미확인 | 검색: Mux agent skills coder, site:github.com/coder/mux skills SKILL.md | 제품 버전·등록 경로·개별 제외·중복·갱신 미확인 | 공식 로더 근거 미확보 |
| openhands / OpenHands | ~/.openhands/skills<br>.openhands/skills | P/부분 | 공식 글은 프로젝트 .openhands/skills 또는 .agents/skills 안내; SDK/CLI 분리 필요 | 글로벌 경로·개별 제외·중복·갱신 미확인 | [공식 자료](https://www.openhands.dev/blog/20260227-creating-effective-agent-skills) |
| pi / Pi | ~/.pi/agent/skills<br>.pi/skills | L/문서 | 전용 + .agents + packages/settings; --no-skills와 명시 --skill 경로 | 실행 한정 격리 후보; 기본 실행 전체 제어 아님. 버전별 중복·/reload 확인 필요 | [공식 자료](https://raw.githubusercontent.com/badlogic/pi-mono/main/packages/coding-agent/docs/skills.md) |
| omp / Oh My Pi | ~/.omp/agent/skills<br>.omp/skills | N/앱 연결 | native·plugins·agents·Claude·Codex 등 공급자; skills.ignoredSkills 또는 disabledExtensions skill:name | 이름 단위 Adapter와 주석 보존 round-trip 검증. realpath 중복 제거는 문서 확인, 18.2.7 실행 세션 반영은 남음 | [공식 자료](https://raw.githubusercontent.com/can1357/oh-my-pi/main/docs/skills.md) |
| rovodev / Rovo Dev | ~/.rovodev/skills<br>.rovodev/skills | P/부분 | 공식 사용자 ~/.rovodev/skills 또는 ~/.agents/skills; 프로젝트 추가 확인 | 개별 off·동명·realpath·갱신 미확인 | [공식 자료](https://support.atlassian.com/rovo/docs/extend-rovo-dev-cli-with-agent-skills/) |
| roo / Roo Code | ~/.roo/skills<br>.roo/skills | P/문서 경로 | 전용·공용 프로젝트/글로벌 경로; 개별 off 키 미확인 | 동명 전용 경로 우선; realpath·갱신 미확인 | [공식 자료](https://docs.roocode.com/features/skills) |
| tabnine-cli / Tabnine CLI | ~/.tabnine/agent/skills<br>.tabnine/agent/skills | ?/미확인 | 검색: Tabnine skills SKILL.md site:docs.tabnine.com. 제3자 등록표 외 로더 증거 부족 | 개별 제외·중복·갱신 미확인 | 공식 로더 근거 미확보 |
| zencoder / Zencoder | ~/.zencoder/skills<br>.zencoder/skills | P/문서 경로 | 프로젝트·글로벌 .agents 및 프로젝트 .claude. .zencoder는 구형 호환 경로 | disable-model-invocation은 원본 속성. 독립 설정·중복·갱신 미확인 | [공식 자료](https://docs.zencoder.ai/features/skills) |
| neovate / Neovate | ~/.neovate/skills<br>.neovate/skills | ?/부분 | 공식 저장소·skills 명령 관련 활동 확인; 정확한 loader 확인 필요 | 개별 제외·중복·갱신 미확인 | [공식 자료](https://github.com/neovateai/neovate-code) |
| pochi / Pochi | ~/.pochi/skills<br>.pochi/skills | P/문서 경로 | 프로젝트 .pochi>.agents>글로벌 .pochi>.agents; 원본 frontmatter 호출 제어 존재 | 동명 상위 출처 우선; 플랫폼 독립 off 키·realpath·갱신 미확인 | [공식 자료](https://docs.getpochi.com/skills/) |
| adal / AdaL | ~/.adal/skills<br>.adal/skills | P/부분 | 개인·프로젝트 .adal/skills 및 plugin 문서 확인; 추가 탐색·개별 off 미확인 | 동명·realpath·갱신 미확인 | [공식 자료](https://docs.sylph.ai/features/plugins-and-skills/) |
| copilot / GitHub Copilot | ~/.copilot/skills<br>— | N/CLI 부분 | Copilot CLI /skills 및 copilot skill enable/disable. .copilot/.agents 등; IDE는 별도 | 정확한 영속 키·중복·재로드 필요. CLI 근거로 모든 Copilot 제품을 지원 처리하지 않음 | [공식 자료](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference) |
| warp / Warp | ~/.agents/skills<br>— | P/부분 | 공식 파일 기반 skills 및 .agents 설치 안내. Cloud Oz와 로컬 Warp 구분 | 개별 off·호환 경로 전체·중복·갱신 검증 필요 | [공식 자료](https://github.com/warpdotdev/oz-skills/blob/main/README.md) |
| aider / Aider | ~/.aider/skills<br>— | ?/미확인 | 검색: Aider SKILL.md skills site:aider.chat. 일반 파일 읽기와 스킬 자동 탐색 구분 | 자동 로더·등록 경로·개별 제외·중복·갱신 미확인 | 공식 로더 근거 미확보 |
| hermes / Hermes | ~/.hermes/skills<br>— | N/앱 연결 | skills.disabled; skills.platform_disabled는 Telegram 등 채널 의미. 전용·trusted project·외부 경로 | 글로벌 disabled Adapter와 YAML 주석 보존 왕복 검증. 프로필·채널별 설정과 게이트웨이/세션 반영은 남음 | [공식 자료](https://hermes-agent.nousresearch.com/docs/reference/faq) |
| openclaw / OpenClaw | ~/.openclaw/skills<br>skills | N/앱 연결 | skills.entries[key].enabled=false. key는 name 또는 metadata.openclaw.skillKey; 공용도 탐색 | 기본 설정 Adapter와 metadata key round-trip 검증. JSON5 주석 파일은 손실 방지로 쓰기 거부; 실행 세션 반영은 남음 | [공식 자료](https://docs.openclaw.ai/tools/skills-config) |
| qclaw / QClaw | ~/.qclaw/skills<br>— | ?/미확인 | 검색: QClaw 技能 官方, site:qclaw.qq.com skills. OpenClaw 설정을 복제 적용하지 않음 | 개별 제외·경로·중복·갱신 미확인 | 공식 로더 근거 미확보 |
| easyclaw / EasyClaw | ~/.easyclaw/skills<br>— | ?/부분 | 공식 스킬 카탈로그 확인; 런타임 로딩과 개별 설정 근거 부족 | 경로·개별 제외·중복·갱신 미확인 | [공식 자료](https://easyclaw.com/pt/skills/) |
| autoclaw / AutoClaw | ~/.openclaw-autoclaw/skills<br>— | ?/미확인 | 검색: AutoClaw skills official, site:autoglm.zhipuai.cn skills. 동명 서비스와 앱 ID 구분 필요 | 경로·개별 제외·중복·갱신 미확인 | 공식 로더 근거 미확보 |
| workbuddy / WorkBuddy | ~/.workbuddy/skills-marketplace/skills<br>— | ?/미확인 | Skills Market 공식 URL 조회 실패. 제품·개별 제외·실제 설치 규칙 미확인 | CodeBuddy CLI 설정 공유 여부 미확인; 동일 회사라는 이유로 추정하지 않음 | [공식 자료](https://www.codebuddy.cn/docs/workbuddy/From-Beginner-to-Expert-Guide/Function-Description/Skills-Market) |

## 해석의 경계

1. SDK/CLI/IDE·원격 실행·플러그인·추가 경로·조직 정책을 구분한다.
2. 동명 우선순위는 내용 비교가 아니다. 다른 버전을 조용히 가릴 수 있어 대표 출처와 실제 선택 결과를 대조한다.
3. 공용에서 ~/.claude/skills로 옮겨도 Cursor·Firebender가 읽을 수 있다. 전용이라는 이름은 독립성 증거가 아니다.
4. 공용 SKILL.md의 disable-model-invocation 변경은 다른 플랫폼에 영향을 줄 수 있고 완전 비활성도 아니다.
5. 앱의 저장소 목록이 한 에이전트의 동시 로딩 범위는 아니다. 현재 작업 폴더·조상·하위 범위·프로필로 판단한다.
6. 추가 경로 옵션과 기본 경로 차단을 혼동하지 않는다.
7. 실사용 중인 대화에 이미 들어간 내용을 제거하거나 모든 반복 읽기를 방지한다고 보장하지 않는다.

## 우선 확인

- OMP: 설치 버전 스키마 및 ignoredSkills/disabledExtensions 반영 경로. 이름 제외만으로 동명 버전을 선택할 수 없다.
- AGY: 내부 ID gemini-cli와 실제 제품 AGY를 구분한다. 내장 문서와 웹 경로 차이, 매니페스트 제외가 자동 발견 출처까지 적용되는지 확인한다.
- Cursor/Kiro: 독립 설정 키 또는 겹치지 않는 설치 경로를 확보한다. Kiro [custom agent 설정](https://kiro.dev/docs/custom-agents/configuration-reference/)의 기본 리소스 전체 상속 차단은 스킬 하나의 기본 해법이 아니다.
- OpenClaw: [로딩 순서](https://docs.openclaw.ai/tools/skills), state-dir/profile, 링크 대상 제한을 확인한다.
- Codex: [설정 참조](https://learn.chatgpt.com/docs/config-file/config-reference)와 스킬 안내의 path 표현 차이를 버전별 fixture로 검증한다.
- Mistral Vibe: allowlist가 있으면 deny 추가만으로 제외되지 않는다. 다른 스킬의 허용 범위를 보존한다.

## 남은 근거를 채우는 순서

미확인 행은 공식 문서/소스 → 설치 버전 도움말·스키마 → 임시 HOME/프로젝트의 로더 목록 순으로 확인한다. 버전, 설정 경로/키, 범위, 목록 제외, 직접 호출 효과, 동명/realpath, 반영 시점, 복원 검증을 채워야 자동 제어를 켤 수 있다. 검색·HTTP 실패는 기능 부재가 아니다.

[구현 명세](../platform-skill-isolation-spec.md)와 [작업 목록](../platform-skill-isolation-tasks.md)을 함께 읽는다.
