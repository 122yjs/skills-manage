import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";

import { WebDashboardApp } from "@/web-dashboard/WebDashboardApp";
import type { DashboardSnapshot } from "@/web-dashboard/types";

const snapshot: DashboardSnapshot = {
  generatedAt: "2026-07-31T06:00:00.000Z",
  appName: "skills-manage",
  language: null,
  summary: {
    centralSkillCount: 1,
    detectedPlatformCount: 1,
    collectionCount: 1,
    discoveredProjectCount: 1,
    discoveredSkillCount: 2,
    marketplaceSkillCount: 1,
  },
  libraryGroups: [
    {
      id: "central",
      label: "Skill Vault",
      path: "/vault",
      skills: [
        {
          id: "reviewer",
          name: "Code Reviewer",
          description: "Reviews code",
          path: "/vault/reviewer/SKILL.md",
          sourceLabel: null,
          linkType: "central",
        },
      ],
    },
    {
      id: "plugin:/plugins/karpathy",
      label: "karpathy-skills",
      path: "/plugins/karpathy",
      skills: [
        {
          id: "plug-skill",
          name: "Plugin Skill",
          description: "From a plugin",
          path: "/plugins/karpathy/skills/plug",
          sourceLabel: "karpathy-skills",
          linkType: "read-only",
        },
      ],
    },
  ],
  platforms: [
    {
      id: "claude-code",
      displayName: "Claude Code",
      category: "coding",
      globalSkillsDir: "/home/test/.claude/skills",
      isDetected: true,
      isEnabled: true,
      skills: [
        {
          id: "reviewer",
          name: "Code Reviewer",
          description: "Reviews code",
          path: "/home/test/.claude/skills/reviewer",
          sourceLabel: null,
          linkType: "symlink",
        },
      ],
    },
  ],
  collections: [
    {
      id: "daily",
      name: "Daily Skills",
      description: "Everyday tools",
      skills: [
        {
          id: "reviewer",
          name: "Code Reviewer",
          description: "Reviews code",
          path: null,
          sourceLabel: null,
          linkType: null,
        },
      ],
    },
  ],
  discoveredProjects: [
    {
      projectPath: "/projects/demo",
      projectName: "Demo Project",
      platforms: ["claude-code"],
      skills: [
        {
          id: "d1",
          name: "Lint Helper",
          description: null,
          path: "/projects/demo/skills/lint",
          sourceLabel: "claude-code",
          linkType: null,
        },
      ],
    },
  ],
  marketplaceSources: [
    { id: "official", name: "Official", url: "https://example.com/repo", skillCount: 1 },
  ],
  marketplaceSkills: [
    {
      id: "m1",
      registryId: "official",
      name: "Remote Skill",
      description: "Cached",
      isInstalled: false,
    },
  ],
  obsidianVaults: [],
  settings: {
    centralSkillsPath: "/vault",
    migrationState: "done",
    databasePath: "/home/test/.skillsmanage/db.sqlite",
    scanDirectories: [
      {
        id: 1,
        path: "/projects",
        label: "Projects",
        isActive: true,
        isBuiltin: false,
      },
    ],
    customPlatforms: [
      {
        id: "custom-agent",
        displayName: "Custom Agent",
        category: "coding",
        globalSkillsDir: "/custom/skills",
        projectSkillsDir: ".custom/skills",
        isEnabled: true,
      },
    ],
    githubPatConfigured: true,
    aiProvider: "chatgpt",
    aiRegion: "intl",
    aiModel: "gpt-test",
    aiProtocol: "openai",
    aiApiUrl: "https://api.example.test/v1",
    aiApiKeyConfigured: true,
  },
};

function successfulResponse() {
  return Promise.resolve({
    ok: true,
    status: 200,
    json: async () => snapshot,
  });
}

function renderApp(initialEntries: string[] = ["/central"]) {
  return render(
    <MemoryRouter initialEntries={initialEntries}>
      <WebDashboardApp />
    </MemoryRouter>,
  );
}

describe("WebDashboardApp", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("사이드바와 라이브러리 그룹을 보여주고 설치 버튼 같은 변경 기능은 없다", async () => {
    const fetchMock = vi.fn(successfulResponse);
    vi.stubGlobal("fetch", fetchMock);

    renderApp(["/central"]);

    // 사이드바 메뉴가 실제로 렌더링된다 (zh 로케일 기준).
    expect(await screen.findByRole("navigation")).toBeInTheDocument();
    expect(screen.getAllByText("技能仓库").length).toBeGreaterThan(0);
    expect(screen.getByRole("link", { name: /技能集/ })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Claude Code/ })).toBeInTheDocument();

    // 중앙 보관함과 플러그인 그룹이 함께 보인다.
    expect(await screen.findByText("Code Reviewer")).toBeInTheDocument();
    expect(screen.getAllByText("karpathy-skills").length).toBeGreaterThan(0);
    expect(screen.getByText("Plugin Skill")).toBeInTheDocument();

    expect(screen.queryByRole("button", { name: /安装|删除/ })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "刷新数据" }));
    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
  });

  it("플랫폼 페이지에서 해당 플랫폼의 스킬 목록을 보여준다", async () => {
    vi.stubGlobal("fetch", vi.fn(successfulResponse));

    renderApp(["/platform/claude-code"]);

    expect(
      (await screen.findAllByRole("heading", { name: "Claude Code" })).length,
    ).toBeGreaterThan(0);
    expect(await screen.findAllByText("Code Reviewer")).not.toHaveLength(0);
    expect(screen.getByText("/home/test/.claude/skills")).toBeInTheDocument();
  });

  it("전체 플랫폼을 펼친 뒤 다시 빈 플랫폼을 접을 수 있다", async () => {
    window.localStorage.removeItem("skills-manage:web-show-all-platforms");
    vi.stubGlobal(
      "fetch",
      vi.fn(() =>
        successfulResponse().then((response) => ({
          ...response,
          json: async () => ({
            ...snapshot,
            platforms: [
              ...snapshot.platforms,
              {
                id: "empty-agent",
                displayName: "Empty Agent",
                category: "coding",
                globalSkillsDir: "/home/test/.empty/skills",
                isDetected: true,
                isEnabled: true,
                skills: [],
              },
            ],
          }),
        })),
      ),
    );

    renderApp(["/central"]);

    const showAll = await screen.findByRole("button", { name: "显示所有平台" });
    expect(screen.queryByRole("link", { name: "Empty Agent" })).not.toBeInTheDocument();

    fireEvent.click(showAll);
    expect(screen.getByRole("link", { name: "Empty Agent" })).toBeInTheDocument();

    const hideEmpty = screen.getByRole("button", { name: "隐藏空平台" });
    fireEvent.click(hideEmpty);
    expect(screen.queryByRole("link", { name: "Empty Agent" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "显示所有平台" })).toBeInTheDocument();
  });

  it("마켓플레이스 페이지에서 동기화 캐시를 보여준다", async () => {
    vi.stubGlobal("fetch", vi.fn(successfulResponse));

    renderApp(["/marketplace"]);

    fireEvent.click(await screen.findByRole("tab", { name: /我的来源/ }));
    expect(await screen.findByText("Remote Skill")).toBeInTheDocument();
    expect(screen.getByText("Official")).toBeInTheDocument();
  });

  it("동기화 캐시가 없어도 기본 추천 마켓플레이스를 보여준다", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() =>
        successfulResponse().then((response) => ({
          ...response,
          json: async () => ({
            ...snapshot,
            summary: { ...snapshot.summary, marketplaceSkillCount: 0 },
            marketplaceSources: [],
            marketplaceSkills: [],
          }),
        })),
      ),
    );

    renderApp(["/marketplace"]);

    expect(await screen.findByText("web-artifacts-builder")).toBeInTheDocument();
    expect(screen.getAllByText("Anthropic").length).toBeGreaterThan(0);
  });

  it("웹 설정에서도 데스크톱 설정의 주요 영역을 빠짐없이 보여준다", async () => {
    vi.stubGlobal("fetch", vi.fn(successfulResponse));

    renderApp(["/settings"]);

    expect(await screen.findByText("技能仓库位置")).toBeInTheDocument();
    expect(screen.getByText("GitHub 导入访问令牌")).toBeInTheDocument();
    expect(screen.getByText("AI 提供商")).toBeInTheDocument();
    expect(screen.getByText("扫描目录")).toBeInTheDocument();
    expect(screen.getByText("关于")).toBeInTheDocument();
  });

  it("스냅샷 요청 실패 시 다시 시도할 수 있는 오류 화면을 표시한다", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: false,
        status: 500,
        json: async () => ({ error: "database is unavailable" }),
      }),
    );

    renderApp();

    expect(await screen.findByText("无法加载仪表盘数据")).toBeInTheDocument();
    expect(screen.getByText("database is unavailable")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重试" })).toBeInTheDocument();
  });
});
