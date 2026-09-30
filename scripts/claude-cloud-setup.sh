#!/usr/bin/env bash
# Claude Code 클라우드 세션이 시작·재개될 때 개발 의존성을 준비한다.
# .claude/settings.json 의 SessionStart 훅이 부른다.
# - 로컬(내 Mac)에서는 아무것도 하지 않는다. CLAUDE_CODE_REMOTE=true 일 때만 동작한다.
# - 여러 번 실행해도 안전하다. 잠금 파일이 바뀌지 않았으면 설치를 건너뛴다.
# - 설치가 실패해도 세션 시작을 막지 않도록 항상 0으로 끝난다.
# Cursor Cloud Agent용 .cursor/install.sh(ticket-notice-monitor PR #14)를 옮겨 온 것이다.
set -uo pipefail

[ "${CLAUDE_CODE_REMOTE:-}" = "true" ] || exit 0
cd "${CLAUDE_PROJECT_DIR:-$(dirname "$0")/..}" || exit 0

stamp_dir=".claude/.setup-stamps"
mkdir -p "$stamp_dir"

# 잠금 파일 해시가 지난번과 같으면 0(건너뜀), 다르면 1(설치 필요)
unchanged() {
  local lock="$1" name="$2" hash
  hash="$(sha256sum "$lock" | cut -d' ' -f1)"
  [ -f "$stamp_dir/$name" ] && [ "$(cat "$stamp_dir/$name")" = "$hash" ]
}
mark() { sha256sum "$1" | cut -d' ' -f1 > "$stamp_dir/$2"; }

# 세션의 이후 셸 명령에도 적용할 환경변수
persist_env() {
  [ -n "${CLAUDE_ENV_FILE:-}" ] && echo "$1" >> "$CLAUDE_ENV_FILE"
}

# Python: requirements.txt → .venv
if [ -f requirements.txt ]; then
  [ -d .venv ] || python3 -m venv .venv
  if ! unchanged requirements.txt pip || [ ! -x .venv/bin/python ]; then
    .venv/bin/pip install -q -U pip && .venv/bin/pip install -q -r requirements.txt && mark requirements.txt pip
  fi
  persist_env "export VIRTUAL_ENV=\"$PWD/.venv\""
  persist_env "export PATH=\"$PWD/.venv/bin:\$PATH\""
fi

# Node: 잠금 파일에 맞는 패키지 매니저로 설치
if [ -f pnpm-lock.yaml ]; then
  if ! unchanged pnpm-lock.yaml pnpm || [ ! -d node_modules ]; then
    corepack enable >/dev/null 2>&1 || npm i -g pnpm >/dev/null 2>&1
    pnpm install --frozen-lockfile && mark pnpm-lock.yaml pnpm
  fi
elif [ -f package-lock.json ]; then
  if ! unchanged package-lock.json npm || [ ! -d node_modules ]; then
    npm ci --no-audit --no-fund && mark package-lock.json npm
  fi
fi

# 로컬 개발용 .env: 없을 때만 예시 파일을 복사한다
if [ ! -f .env ] && [ -f .env.example ]; then
  cp .env.example .env
fi

# 저장소별 추가 준비
[ -f .claude/cloud-setup.extra.sh ] && . .claude/cloud-setup.extra.sh

echo "claude-cloud-setup: 준비 완료"
exit 0
