# 이슈 트래커: GitHub

이 저장소의 이슈와 명세는 GitHub 이슈(`122yjs/skills-manage`)로 관리한다. 모든 작업에 `gh` CLI를 쓴다.

## 사용법

- **이슈 만들기**: `gh issue create --title "..." --body "..."`. 여러 줄 본문은 heredoc으로 넘긴다.
- **이슈 읽기**: `gh issue view <번호> --json number,title,body,labels,comments`
- **이슈 목록**: `gh issue list --state open --json number,title,body,labels,comments --jq '[.[] | {number, title, body, labels: [.labels[].name], comments: [.comments[].body]}]'`에 필요한 `--label`, `--state` 조건을 붙인다.
- **하위 이슈로 연결**: `gh issue create --parent <부모> ...`로 만들거나, 만든 뒤 `gh issue edit <부모> --add-sub-issue <자식>`으로 연결한다(`gh` 2.94 이상). 이전 버전은 `gh api --method POST repos/<owner>/<repo>/issues/<부모>/sub_issues -F sub_issue_id=<자식 database id>`를 쓴다. database id는 아래 **막힘 관계**에서 얻는 값과 같다. 하위 이슈 기능을 못 쓰면 자식 본문 맨 위에 `Part of #<부모>`를 적는다.
- **댓글**: `gh issue comment <번호> --body "..."`
- **라벨 붙이기·떼기**: `gh issue edit <번호> --add-label "..."` / `--remove-label "..."`
- **닫기**: `gh issue close <번호> --comment "..."`

저장소는 `git remote -v`로 정해진다. clone 안에서 `gh`를 실행하면 자동으로 이 저장소를 쓴다.

## 공개 이슈 작성 원칙

이 저장소는 공개 포크이므로 이슈도 공개된다. 보안 관련 이슈에는 재현 절차나 공격 방법을 적지 않고 바꿀 동작만 적는다. 비밀값, 실제 사용자 경로, 개인 정보는 적지 않는다.

## 이슈·PR 언어와 쉬운 요약

**Issue/PR language: ko.** _(이 포크 저장소의 이슈·PR 제목과 본문은 한국어로 작성한다. 영어 저장소나 `upstream` 기여용 작업은 `en`을 따른다.)_

- 이슈와 PR 본문을 작성할 때는 `Blocked by:`, `Part of #`, `Closes #`, `PRs as a request surface`, 라벨 이름 같은 제어용 표식과 기존 스킬 템플릿 뼈대는 깨뜨리지 않고 유지한다.
- 본문 맨 위에는 비개발자도 바로 이해할 수 있는 `## 한눈에 보기 (쉬운 설명)` 섹션을 먼저 적어 무엇을 왜 바꾸는지 쉬운 말로 풀어 쓴다.

## PR을 분류 대상으로 볼지

**PRs as a request surface: no.** _(외부 PR을 기능 요청처럼 분류하려면 `yes`로 바꾼다. `/triage`가 이 값을 읽는다.)_

`yes`로 바꾸면 PR도 이슈와 같은 라벨과 상태로 다루며 `gh pr` 명령을 쓴다.

- **PR 읽기**: `gh pr view <번호> --comments`, 변경 내용은 `gh pr diff <번호>`
- **분류할 외부 PR 목록**: `gh api --paginate 'repos/{owner}/{repo}/pulls?state=open' --jq '.[] | select(.author_association | IN("OWNER","MEMBER","COLLABORATOR") | not) | {number, title, author: .user.login, author_association, labels: [.labels[].name]}'`
- **댓글·라벨·닫기**: `gh pr comment`, `gh pr edit --add-label`/`--remove-label`, `gh pr close`

GitHub는 이슈와 PR에 같은 번호 체계를 쓴다. `#42`가 무엇인지 모르면 `gh pr view 42`로 먼저 확인하고, 아니면 `gh issue view 42`로 읽는다.

## 스킬이 "이슈 트래커에 게시"하라고 할 때

GitHub 이슈를 만든다.

## 스킬이 "관련 티켓을 가져오라"고 할 때

위 **이슈 읽기** 방법으로 읽는다.

## 막힘 관계와 진행 순서

`/to-tickets`와 `/wayfinder`가 쓴다.

- **지도**: `/wayfinder`의 지도는 `wayfinder:map` 라벨을 붙인 이슈 하나다. 본문에 Notes, Decisions-so-far, Fog를 둔다. `gh issue create --label wayfinder:map`으로 만든다.
- **하위 티켓**: 지도나 부모 이슈의 하위 이슈로 연결한다(위 **하위 이슈로 연결**). 하위 이슈 기능을 못 쓰면 부모 본문의 작업 목록에 넣고 자식 본문 맨 위에 `Part of #<부모>`를 적는다. `/wayfinder` 티켓의 라벨은 `wayfinder:<type>`(`research`/`prototype`/`grilling`/`task`)이며, 맡은 뒤에는 작업하는 개발자를 담당자로 지정한다.
- **막힘 관계**: GitHub의 기본 이슈 의존 관계를 쓴다. `gh api --method POST repos/<owner>/<repo>/issues/<자식>/dependencies/blocked_by -F issue_id=<막는 이슈의 database id>`로 추가한다. database id는 `gh api repos/<owner>/<repo>/issues/<n> --jq .id`로 얻는다. `#번호`나 `node_id`가 아니다. GitHub는 열린 막는 이슈 수를 `issue_dependencies_summary.blocked_by`로 알려 준다. 의존 관계를 못 쓰면 자식 본문 맨 위에 `Blocked by: #<n>, #<n>`을 적는다. 막는 이슈가 모두 닫히면 진행할 수 있다.
- **진행 가능한 티켓 찾기**: 부모의 열린 하위 이슈를 나열하고(`gh issue list --state open`), 열린 막는 이슈가 있거나(`issue_dependencies_summary.blocked_by > 0` 또는 `Blocked by` 줄의 열린 이슈) 담당자가 있는 티켓은 뺀다. 남은 것 중 목록 순서가 가장 앞선 티켓부터 한다.
- **맡기**: `gh issue edit <n> --add-assignee @me`. 세션의 첫 쓰기 작업으로 한다.
- **해결**: `gh issue comment <n> --body "<답>"` 다음 `gh issue close <n>`. 지도가 있으면 Decisions-so-far에 요지와 링크를 덧붙인다.
