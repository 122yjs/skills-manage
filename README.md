# skills-manage

`skills-manage` is a Tauri desktop app for managing AI coding agent skills across multiple platforms from one place.

[中文文档](README_CN.md)

> **Disclaimer**
>
> `skills-manage` is an independent, unofficial desktop application for managing local skill directories and importing public skill metadata. It is not affiliated with, endorsed by, or sponsored by Anthropic, OpenAI, GitHub, MiniMax, or any other supported platform, publisher, or trademark owner.

## Overview

`skills-manage` follows the [Agent Skills](https://github.com/anthropics/agent-skills) open pattern. The central skill library (the "Skill Vault") lives at `~/.skillsmanage/skills/` by default and can be moved in Settings; platforms that share skills read the shared location `~/.agents/skills/`. Installs are symlinks to (or copies of) the central source, so one copy can drive multiple AI coding tools.

## Highlights

- Central skill library plus per-platform install and uninstall flows.
- Claude Code can surface native skills and read-only marketplace plugin skills in one platform view.
- Full skill detail view with Markdown preview, raw source view, and AI explanation generation.
- Collections for organizing skills and batch-installing them to platforms.
- Discover scan for project-level skill libraries, including an Obsidian sidebar category for vault skills (`.skills/`, `.agents/skills/`, `.claude/skills/`).
- Marketplace browsing and GitHub repository import with authenticated requests and retry fallback.
- Fast search for large skill libraries with deferred queries, lazy indexing, and virtualization.
- Trilingual UI (Korean, English, Chinese), Catppuccin themes, accent colors, onboarding, and responsive navigation.

## Screenshots

### Central skills and platform installs

![Central skills library view](images/01.png)

### Review installed skills on a specific platform

![Platform skill view](images/06.png)

### Discover local project skill libraries

![Discover project skill libraries](images/03.png)

### Browse marketplace publishers and skills

![Marketplace view](images/04.png)

### Import skills from a GitHub repository

![GitHub repository import wizard](images/02.png)

### Organize reusable collections

![Skill collections view](images/05.png)

## Download

- Latest release: <https://github.com/iamzhihuix/skills-manage/releases/latest>
- Current prebuilt packages: Apple Silicon macOS (`.dmg` and `.app.zip`)
- Other platforms: run from source for now

### macOS Unsigned Build

The current public macOS build is not notarized. If macOS shows a warning such as:

![macOS damaged app warning](images/app-damaged.png)

- `"skills-manage" is damaged and can't be opened`
- `"skills-manage" cannot be opened because Apple could not verify it`

the app is usually not actually corrupted; it is being blocked by Gatekeeper quarantine on an unsigned build.

After moving the app to `/Applications`, run:

```bash
xattr -dr com.apple.quarantine "/Applications/skills-manage.app"
```

Then launch the app again from Finder. If your app is stored somewhere else, replace the path with the actual `.app` path.

## Supported Platforms

The table lists the built-in defaults for each platform's global (user-level) skills directory. Paths can be changed in Settings, and custom platforms can be added there.

| Category | Platform | Global Skills Directory |
|----------|----------|-------------------------|
| Coding | Claude Code | `~/.claude/skills/` |
| Coding | Codex CLI | `~/.codex/skills/` |
| Coding | Cursor | `~/.cursor/skills/` |
| Coding | Antigravity | `~/.gemini/config/skills/` |
| Coding | AGY CLI | `~/.gemini/config/skills/` |
| Coding | Cline | `~/.agents/skills/` |
| Coding | Deep Agents | `~/.agents/skills/` |
| Coding | Dexto | `~/.agents/skills/` |
| Coding | Firebender | `~/.agents/skills/` |
| Coding | Kimi Code CLI | `~/.agents/skills/` |
| Coding | Warp | `~/.agents/skills/` |
| Coding | AiderDesk | `~/.aider-desk/skills/` |
| Coding | Trae | `~/.trae/skills/` |
| Coding | Trae CN | `~/.trae-cn/skills/` |
| Coding | Factory Droid | `~/.factory/skills/` |
| Coding | Junie | `~/.junie/skills/` |
| Coding | Qwen Code | `~/.qwen/skills/` |
| Coding | Windsurf | `~/.codeium/windsurf/skills/` |
| Coding | Qoder | `~/.qoder/skills/` |
| Coding | Augment | `~/.augment/skills/` |
| Coding | OpenCode | `~/.opencode/skills/` |
| Coding | Kilo Code | `~/.kilocode/skills/` |
| Coding | OB1 | `~/.ob1/skills/` |
| Coding | Amp | `~/.amp/skills/` |
| Coding | Kiro CLI | `~/.kiro/skills/` |
| Coding | CodeBuddy | `~/.codebuddy/skills/` |
| Coding | IBM Bob | `~/.bob/skills/` |
| Coding | CodeArts Agent | `~/.codeartsdoer/skills/` |
| Coding | Codemaker | `~/.codemaker/skills/` |
| Coding | Code Studio | `~/.codestudio/skills/` |
| Coding | Command Code | `~/.commandcode/skills/` |
| Coding | Continue | `~/.continue/skills/` |
| Coding | Cortex Code | `~/.snowflake/cortex/skills/` |
| Coding | Crush | `~/.config/crush/skills/` |
| Coding | Devin for Terminal | `~/.config/devin/skills/` |
| Coding | ForgeCode | `~/.forge/skills/` |
| Coding | Goose | `~/.config/goose/skills/` |
| Coding | iFlow CLI | `~/.iflow/skills/` |
| Coding | Kode | `~/.kode/skills/` |
| Coding | MCPJam | `~/.mcpjam/skills/` |
| Coding | Mistral Vibe | `~/.vibe/skills/` |
| Coding | Mux | `~/.mux/skills/` |
| Coding | OpenHands | `~/.openhands/skills/` |
| Coding | Pi | `~/.pi/agent/skills/` |
| Coding | Oh My Pi | `~/.omp/agent/skills/` |
| Coding | Rovo Dev | `~/.rovodev/skills/` |
| Coding | Roo Code | `~/.roo/skills/` |
| Coding | Tabnine CLI | `~/.tabnine/agent/skills/` |
| Coding | Zencoder | `~/.zencoder/skills/` |
| Coding | Neovate | `~/.neovate/skills/` |
| Coding | Pochi | `~/.pochi/skills/` |
| Coding | AdaL | `~/.adal/skills/` |
| Coding | GitHub Copilot | `~/.copilot/skills/` |
| Coding | Aider | `~/.aider/skills/` |
| Lobster | Hermes | `~/.hermes/skills/` |
| Lobster | OpenClaw (开爪) | `~/.openclaw/skills/` |
| Lobster | QClaw (千爪) | `~/.qclaw/skills/` |
| Lobster | EasyClaw (简爪) | `~/.easyclaw/skills/` |
| Lobster | AutoClaw | `~/.openclaw-autoclaw/skills/` |
| Lobster | WorkBuddy (打工搭子) | `~/.workbuddy/skills-marketplace/skills/` |
| Shared | Universal (.agents) | `~/.agents/skills/` |
| Central | Skill Vault (central library) | `~/.skillsmanage/skills/` |

> Notes:
>
> - The shared location `~/.agents/skills/` is a single install target read by several tools (Cline, Deep Agents, Dexto, Firebender, Kimi Code CLI, Warp, and others). It is separate from the central library at `~/.skillsmanage/skills/`.
> - Codex CLI also reads the shared `~/.agents/skills/` and uses `$CWD/.agents/skills/` for project-level skills; its global path is `~/.codex/skills/` so it stays distinguishable from the shared install.
> - Claude Code also surfaces marketplace plugin directories under `~/.claude/plugins/marketplaces/*` as read-only rows in the Claude view. Those entries are display-only and are not managed like native skills in `~/.claude/skills/`.

Custom platforms can be added through Settings.


## Privacy & Security

- **Local-first storage** — metadata, collections, scan results, settings, and cached AI explanations stay in `~/.skillsmanage/db.sqlite` or the local skill directories you manage.
- **No telemetry** — the app does not include analytics, crash reporting, or usage tracking.
- **Network access is feature-driven** — outbound requests only happen when you explicitly use marketplace sync/download, GitHub import, or AI explanation generation.
- **Credentials are stored locally** — GitHub PAT and AI API keys are kept in the local SQLite settings table and are not encrypted at rest by the app.
- Never post real secrets in issues, pull requests, screenshots, or logs.

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Desktop framework | Tauri v2 |
| Frontend | React 18, TypeScript, Tailwind CSS 4 |
| UI components | shadcn/ui, Lucide icons |
| State management | Zustand |
| Markdown | react-markdown |
| i18n | react-i18next, i18next-browser-languagedetector |
| Theming | Catppuccin 4-flavor palette |
| Backend | Rust (serde, sqlx, chrono, uuid) |
| Database | SQLite via sqlx (WAL mode) |
| Routing | react-router-dom v7 |

## Development

### Prerequisites

- [Node.js](https://nodejs.org/) 22.13 or newer
- [pnpm](https://pnpm.io/)
- [Rust toolchain](https://rustup.rs/) (stable)
- Tauri v2 system dependencies: <https://v2.tauri.app/start/prerequisites/>

### Install Dependencies

```bash
pnpm install
```

### Run in Development

```bash
pnpm tauri dev
```

The Vite dev server runs on port `24200`.

### Run the Read-only Web Dashboard (Skill Hub)

```bash
pnpm web:dev
```

Open <http://127.0.0.1:24200>. This local-only dashboard ("Skill Hub") reads
the existing `~/.skillsmanage/db.sqlite` database and mirrors the desktop app
layout: a sidebar with the Skill Library, per-platform views, Discover,
Marketplace (last-sync cache), Collections, and Settings. It never exposes
install, delete, or settings commands — everything is read-only. Run the
desktop app once first so the database has been initialized.

### Validation

```bash
pnpm test
pnpm typecheck
pnpm lint
cd src-tauri && cargo test
cd src-tauri && cargo clippy --all-targets -- -D warnings
cd src-tauri && cargo fmt --check
```

## Project Structure

```text
skills-manage/
├── src/                        # React frontend
│   ├── components/             # UI components
│   ├── i18n/                   # Locale files and i18n setup
│   ├── lib/                    # Frontend helpers
│   ├── pages/                  # Route views
│   ├── stores/                 # Zustand stores
│   ├── test/                   # Vitest + RTL tests
│   └── types/                  # Shared TypeScript types
├── src-tauri/                  # Rust backend
│   └── src/
│       ├── commands/           # Tauri IPC handlers
│       ├── db.rs               # SQLite schema, migrations, queries
│       ├── lib.rs              # Tauri app setup
│       └── main.rs             # Desktop entry point
├── public/                     # Static assets
├── CHANGELOG.md                # English changelog
├── CHANGELOG.zh.md             # Chinese changelog
└── release-notes/              # GitHub release notes
```

## Database

The SQLite database lives at `~/.skillsmanage/db.sqlite` and is initialized automatically on first launch.

## Changelog

- English: [CHANGELOG.md](CHANGELOG.md)
- Chinese: [CHANGELOG.zh.md](CHANGELOG.zh.md)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, validation commands, and pull request expectations.

## Community

Join the Discord community: <https://discord.gg/fuGURex5fV>

## Security

See [SECURITY.md](SECURITY.md) for vulnerability reporting and data-handling notes.

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=iamzhihuix/skills-manage&type=Date)](https://www.star-history.com/#iamzhihuix/skills-manage&Date)

## License

This project is licensed under the Apache License 2.0. See [LICENSE](LICENSE).
