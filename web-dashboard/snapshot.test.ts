import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";

import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { loadDashboardSnapshot, stablePathHash } from "./snapshot";

describe("loadDashboardSnapshot", () => {
  let testDirectory: string;
  let databasePath: string;

  beforeEach(() => {
    testDirectory = mkdtempSync(join(tmpdir(), "skills-manage-web-dashboard-"));
    databasePath = join(testDirectory, "db.sqlite");

    const database = new DatabaseSync(databasePath);
    database.exec(`
      CREATE TABLE skills (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        description TEXT,
        canonical_path TEXT,
        is_central INTEGER NOT NULL
      );
      CREATE TABLE skill_installations (
        skill_id TEXT,
        agent_id TEXT,
        installed_path TEXT,
        link_type TEXT
      );
      CREATE TABLE agent_skill_observations (
        row_id TEXT PRIMARY KEY,
        skill_id TEXT,
        agent_id TEXT,
        name TEXT,
        description TEXT,
        dir_path TEXT,
        source_kind TEXT,
        source_label TEXT
      );
      CREATE TABLE agents (
        id TEXT PRIMARY KEY,
        display_name TEXT NOT NULL,
        category TEXT NOT NULL,
        global_skills_dir TEXT NOT NULL,
        project_skills_dir TEXT,
        is_builtin INTEGER NOT NULL DEFAULT 1,
        is_enabled INTEGER NOT NULL
      );
      CREATE TABLE collections (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        description TEXT,
        created_at TEXT NOT NULL
      );
      CREATE TABLE collection_skills (collection_id TEXT, skill_id TEXT);
      CREATE TABLE discovered_skills (
        id TEXT,
        name TEXT,
        description TEXT,
        dir_path TEXT,
        project_path TEXT,
        project_name TEXT,
        platform_id TEXT
      );
      CREATE TABLE skill_registries (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        url TEXT NOT NULL,
        is_enabled INTEGER NOT NULL DEFAULT 1
      );
      CREATE TABLE marketplace_skills (
        id TEXT PRIMARY KEY,
        registry_id TEXT NOT NULL,
        name TEXT NOT NULL,
        description TEXT,
        is_installed INTEGER NOT NULL DEFAULT 0
      );
      CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT);
      CREATE TABLE scan_directories (
        id INTEGER PRIMARY KEY,
        path TEXT NOT NULL,
        label TEXT,
        is_active INTEGER NOT NULL,
        is_builtin INTEGER NOT NULL
      );
    `);

    const detectedPlatformRoot = join(testDirectory, "claude");
    mkdirSync(detectedPlatformRoot);

    database
      .prepare(
        "INSERT INTO skills (id, name, description, canonical_path, is_central) VALUES (?, ?, ?, ?, 1)",
      )
      .run("reviewer", "Code Reviewer", "Reviews code", "/vault/reviewer/SKILL.md");
    database
      .prepare(
        "INSERT INTO skill_installations (skill_id, agent_id, installed_path, link_type) VALUES (?, ?, ?, ?)",
      )
      .run("reviewer", "claude-code", "/vault/reviewer", "symlink");
    database
      .prepare(
        "INSERT INTO skill_installations (skill_id, agent_id, installed_path, link_type) VALUES (?, ?, ?, ?)",
      )
      .run(
        "reviewer",
        "universal",
        join(testDirectory, ".agents", "skills", "reviewer"),
        "symlink",
      );
    database
      .prepare(
        "INSERT INTO agents (id, display_name, category, global_skills_dir, is_enabled) VALUES (?, ?, ?, ?, 1)",
      )
      .run("claude-code", "Claude Code", "coding", join(detectedPlatformRoot, "skills"));
    database
      .prepare(
        "INSERT INTO agents (id, display_name, category, global_skills_dir, is_enabled) VALUES (?, ?, ?, ?, 1)",
      )
      .run(
        "universal",
        "Universal (.agents)",
        "shared",
        join(testDirectory, ".agents", "skills"),
      );
    database
      .prepare(
        "INSERT INTO collections (id, name, description, created_at) VALUES (?, ?, ?, ?)",
      )
      .run("daily", "Daily Skills", "Everyday tools", "2026-07-31T00:00:00Z");
    database
      .prepare("INSERT INTO collection_skills (collection_id, skill_id) VALUES (?, ?)")
      .run("daily", "reviewer");
    database
      .prepare(
        "INSERT INTO discovered_skills (id, name, description, dir_path, project_path, project_name, platform_id) VALUES (?, ?, ?, ?, ?, ?, ?)",
      )
      .run("d1", "Lint Helper", null, "/projects/demo/skills/lint", "/projects/demo", "Demo", "codex");
    database
      .prepare(
        "INSERT INTO discovered_skills (id, name, description, dir_path, project_path, project_name, platform_id) VALUES (?, ?, ?, ?, ?, ?, ?)",
      )
      .run("d2", "Test Helper", null, "/projects/demo/skills/test", "/projects/demo", "Demo", "claude-code");
    database
      .prepare(
        "INSERT INTO agent_skill_observations (row_id, skill_id, agent_id, name, description, dir_path, source_kind, source_label) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
      )
      .run("o1", "plug-skill", "claude-code", "Plugin Skill", "From a plugin", "/plugins/a/skills/plug", "plugin", "karpathy-skills");
    database
      .prepare(
        "INSERT INTO skill_registries (id, name, url, is_enabled) VALUES (?, ?, ?, 1)",
      )
      .run("official", "Official", "https://example.com/repo");
    database
      .prepare(
        "INSERT INTO marketplace_skills (id, registry_id, name, description, is_installed) VALUES (?, ?, ?, ?, ?)",
      )
      .run("m1", "official", "Remote Skill", "Cached", 1);
    database
      .prepare("INSERT INTO settings (key, value) VALUES (?, ?)")
      .run("ai_api_key__test", "super-secret-value");
    database.exec(`
      INSERT INTO settings (key, value) VALUES
        ('central_skills_path', '/vault'),
        ('central_vault_migration_state', 'done'),
        ('github_pat', 'github-secret-value'),
        ('ai_provider', 'chatgpt'),
        ('ai_region', 'intl'),
        ('ai_model__chatgpt', 'gpt-test'),
        ('ai_protocol__chatgpt', 'openai'),
        ('ai_api_url__chatgpt', 'https://api.example.test/v1'),
        ('ai_api_key__chatgpt', 'api-secret-value');
      INSERT INTO scan_directories (id, path, label, is_active, is_builtin)
      VALUES (1, '/projects', 'Projects', 1, 0);
      INSERT INTO agents (
        id, display_name, category, global_skills_dir, project_skills_dir,
        is_builtin, is_enabled
      ) VALUES (
        'custom-agent', 'Custom Agent', 'coding', '/custom/skills', '.custom/skills',
        0, 1
      );
    `);
    database.close();
  });

  afterEach(() => {
    rmSync(testDirectory, { recursive: true, force: true });
  });

  it("실제 저장 구조를 읽기 전용 대시보드 스냅샷으로 합친다", () => {
    const snapshot = loadDashboardSnapshot(databasePath, { home: testDirectory });

    expect(snapshot.summary).toMatchObject({
      centralSkillCount: 1,
      detectedPlatformCount: 1,
      collectionCount: 1,
      discoveredProjectCount: 1,
      discoveredSkillCount: 2,
      marketplaceSkillCount: 1,
    });

    // 라이브러리 그룹: 중앙 보관함 + 링크된 universal + 플러그인 그룹
    const groupIds = snapshot.libraryGroups.map((group) => group.id);
    expect(groupIds).toContain("central");
    expect(groupIds).toContain("agent:universal");
    expect(groupIds.some((id) => id.startsWith("plugin:"))).toBe(true);
    const pluginGroup = snapshot.libraryGroups.find((group) =>
      group.id.startsWith("plugin:"),
    );
    expect(pluginGroup?.label).toBe("karpathy-skills");
    expect(pluginGroup?.skills[0]).toMatchObject({
      name: "Plugin Skill",
      linkType: "read-only",
    });

    const claude = snapshot.platforms.find((p) => p.id === "claude-code");
    expect(claude).toMatchObject({ isDetected: true });
    expect(claude?.skills.map((skill) => skill.name).sort()).toEqual([
      "Code Reviewer",
      "Plugin Skill",
    ]);

    expect(snapshot.collections[0]).toMatchObject({ name: "Daily Skills" });
    expect(snapshot.collections[0].skills[0]).toMatchObject({ name: "Code Reviewer" });

    expect(snapshot.discoveredProjects[0]).toMatchObject({
      projectName: "Demo",
      platforms: ["claude-code", "codex"],
    });
    expect(snapshot.discoveredProjects[0].skills).toHaveLength(2);

    expect(snapshot.marketplaceSources[0]).toMatchObject({
      name: "Official",
      skillCount: 1,
    });
    expect(snapshot.marketplaceSkills[0]).toMatchObject({
      name: "Remote Skill",
      isInstalled: true,
    });

    expect(snapshot.settings).toMatchObject({
      centralSkillsPath: "/vault",
      migrationState: "done",
      githubPatConfigured: true,
      aiProvider: "chatgpt",
      aiRegion: "intl",
      aiModel: "gpt-test",
      aiProtocol: "openai",
      aiApiUrl: "https://api.example.test/v1",
      aiApiKeyConfigured: true,
    });
    expect(snapshot.settings.scanDirectories[0]).toMatchObject({
      path: "/projects",
      isActive: true,
      isBuiltin: false,
    });
    expect(snapshot.settings.customPlatforms[0]).toMatchObject({
      id: "custom-agent",
      displayName: "Custom Agent",
    });

    // API 키 같은 비밀 값은 스냅샷에 절대 포함되지 않아야 한다.
    expect(JSON.stringify(snapshot)).not.toContain("super-secret-value");
    expect(JSON.stringify(snapshot)).not.toContain("github-secret-value");
    expect(JSON.stringify(snapshot)).not.toContain("api-secret-value");
  });

  it("Obsidian 레지스트리에서 볼트를 읽어 스킬 수를 센다", () => {
    const vaultDir = join(testDirectory, "MyVault");
    mkdirSync(join(vaultDir, ".obsidian"), { recursive: true });
    mkdirSync(join(vaultDir, ".skills", "note-skill"), { recursive: true });
    writeFileSync(join(vaultDir, ".skills", "note-skill", "SKILL.md"), "---\nname: note\n---\n");

    const appSupport = join(testDirectory, "Library", "Application Support", "obsidian");
    mkdirSync(appSupport, { recursive: true });
    writeFileSync(
      join(appSupport, "obsidian.json"),
      JSON.stringify({ vaults: { abc: { path: vaultDir } } }),
    );

    const snapshot = loadDashboardSnapshot(databasePath, { home: testDirectory });
    expect(snapshot.obsidianVaults).toHaveLength(1);
    expect(snapshot.obsidianVaults[0]).toMatchObject({
      name: "MyVault",
      skillCount: 1,
    });
    expect(snapshot.obsidianVaults[0].id).toBe(stablePathHash(vaultDir));
  });
});
