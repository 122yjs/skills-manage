import { existsSync, readdirSync, readFileSync, realpathSync } from "node:fs";
import { homedir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { DatabaseSync } from "node:sqlite";

// ─── DB 행 타입 ─────────────────────────────────────────────────────────────

interface DashboardSkillEntry {
  id: string;
  name: string;
  description: string | null;
  path: string | null;
  sourceLabel: string | null;
  linkType: string | null;
}

interface DashboardLibraryGroup {
  id: string;
  label: string;
  path: string;
  skills: DashboardSkillEntry[];
}

interface DashboardPlatform {
  id: string;
  displayName: string;
  category: string;
  globalSkillsDir: string;
  isDetected: boolean;
  isEnabled: boolean;
  skills: DashboardSkillEntry[];
}

interface DashboardCollection {
  id: string;
  name: string;
  description: string | null;
  skills: DashboardSkillEntry[];
}

interface DashboardProject {
  projectPath: string;
  projectName: string;
  platforms: string[];
  skills: DashboardSkillEntry[];
}

interface DashboardMarketplaceSource {
  id: string;
  name: string;
  url: string;
  skillCount: number;
}

interface DashboardMarketplaceSkill {
  id: string;
  registryId: string;
  name: string;
  description: string | null;
  isInstalled: boolean;
}

interface DashboardObsidianVault {
  id: string;
  name: string;
  path: string;
  skillCount: number;
}

interface ScanDirectoryRow {
  id: number;
  path: string;
  label: string | null;
  is_active: number;
  is_builtin: number;
}

interface CentralSkillRow {
  id: string;
  name: string;
  description: string | null;
  canonical_path: string | null;
  linked_agent_ids: string | null;
}

interface AgentRow {
  id: string;
  display_name: string;
  category: string;
  global_skills_dir: string;
  project_skills_dir: string | null;
  is_builtin: number;
  is_enabled: number;
}

interface InstallationRow {
  skill_id: string;
  agent_id: string;
  name: string;
  description: string | null;
  installed_path: string | null;
  link_type: string | null;
}

interface ObservationRow {
  skill_id: string;
  agent_id: string;
  name: string;
  description: string | null;
  dir_path: string;
  source_kind: string;
  source_label: string | null;
}

interface CollectionRow {
  id: string;
  name: string;
  description: string | null;
  created_at: string;
}

interface CollectionSkillRow {
  collection_id: string;
  id: string;
  name: string;
  description: string | null;
}

interface DiscoveredRow {
  id: string;
  name: string;
  description: string | null;
  dir_path: string;
  project_path: string;
  project_name: string;
  platform_id: string;
}

interface RegistryRow {
  id: string;
  name: string;
  url: string;
}

interface MarketplaceSkillRow {
  id: string;
  registry_id: string;
  name: string;
  description: string | null;
  is_installed: number;
}

function queryRows<TRow>(database: DatabaseSync, sql: string): TRow[] {
  return database.prepare(sql).all() as unknown as TRow[];
}

function tableExists(database: DatabaseSync, table: string): boolean {
  const row = database
    .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?")
    .get(table);
  return Boolean(row);
}

function getSetting(database: DatabaseSync, key: string): string | null {
  const row = database
    .prepare("SELECT value FROM settings WHERE key = ?")
    .get(key) as { value: string } | undefined;
  return row?.value ?? null;
}

/** 비밀값을 읽지 않고 값이 설정되어 있는지만 확인한다. */
function isSecretConfigured(database: DatabaseSync, key: string): boolean {
  const row = database
    .prepare("SELECT CASE WHEN LENGTH(TRIM(value)) > 0 THEN 1 ELSE 0 END AS configured FROM settings WHERE key = ?")
    .get(key) as { configured: number } | undefined;
  return Boolean(row?.configured);
}

function isPlatformDetected(globalSkillsDir: string): boolean {
  return existsSync(globalSkillsDir) || existsSync(dirname(globalSkillsDir));
}

// ─── Obsidian vault 탐지 (데스크톱 앱 discover.rs 와 동일한 규칙) ──────────
//
// vault 란 Obsidian 메모 보관 폴터로, 안에 `.obsidian` 폴터가 있으면 인식한다.
// 등록된 vault 목록은 macOS 기준 ~/Library/Application Support/obsidian/obsidian.json
// 에 있고, 없으면 iCloud 동기화 폴터를 대신 뒤진다.

interface ObsidianRegistryFile {
  vaults?: Record<string, { path?: string }>;
}

function isObsidianVaultDir(path: string): boolean {
  return existsSync(join(path, ".obsidian"));
}

/** 경로 문자열에서 FNV-1a 64비트 해시를 만든다. Rust 측 stable_path_hash 와 동일. */
export function stablePathHash(path: string): string {
  let hash = 0xcbf29ce484222325n;
  const prime = 0x100000001b3n;
  const mask = 0xffffffffffffffffn;
  for (const byte of Buffer.from(path, "utf8")) {
    hash ^= BigInt(byte);
    hash = (hash * prime) & mask;
  }
  return hash.toString(16).padStart(16, "0");
}

function readObsidianRegistryVaultPaths(registryPath: string): string[] {
  let parsed: ObsidianRegistryFile;
  try {
    parsed = JSON.parse(readFileSync(registryPath, "utf8")) as ObsidianRegistryFile;
  } catch {
    return [];
  }
  const paths = Object.values(parsed.vaults ?? {})
    .map((vault) => vault.path)
    .filter((path): path is string => typeof path === "string" && path.length > 0)
    .map((path) => resolve(path))
    .filter((path) => existsSync(path) && isObsidianVaultDir(path));
  return [...new Set(paths)].sort();
}

function directObsidianVaultChildren(root: string): string[] {
  try {
    return readdirSync(root, { withFileTypes: true })
      .filter((entry) => entry.isDirectory())
      .map((entry) => join(root, entry.name))
      .filter((path) => isObsidianVaultDir(path))
      .sort();
  } catch {
    return [];
  }
}

function obsidianSourceVaultPaths(home: string): string[] {
  const registryPaths = readObsidianRegistryVaultPaths(
    join(home, "Library", "Application Support", "obsidian", "obsidian.json"),
  );
  if (registryPaths.length > 0) return registryPaths;
  return directObsidianVaultChildren(
    join(home, "Library", "Mobile Documents", "iCloud~md~obsidian", "Documents"),
  );
}

/** vault 안의 스킬 폴터(.skills, .agents/skills, .claude/skills)에서 SKILL.md 개수를 센다. */
function countSkillsUnder(root: string, depth = 0, budget = { count: 0 }): number {
  if (depth > 4 || budget.count > 5000) return budget.count;
  let entries;
  try {
    entries = readdirSync(root, { withFileTypes: true });
  } catch {
    return budget.count;
  }
  for (const entry of entries) {
    if (entry.name === "SKILL.md") {
      budget.count += 1;
      continue;
    }
    if (entry.isDirectory() && !entry.name.startsWith(".")) {
      countSkillsUnder(join(root, entry.name), depth + 1, budget);
    }
  }
  return budget.count;
}

function loadObsidianVaults(home: string): DashboardObsidianVault[] {
  return obsidianSourceVaultPaths(home)
    .map((vaultPath) => {
      const skillCount = [".skills", join(".agents", "skills"), join(".claude", "skills")]
        .map((rel) => countSkillsUnder(join(vaultPath, rel)))
        .reduce((total, count) => total + count, 0);
      if (skillCount === 0) return null;
      return {
        id: stablePathHash(vaultPath),
        name: basename(vaultPath),
        path: vaultPath,
        skillCount,
      };
    })
    .filter((vault): vault is DashboardObsidianVault => vault !== null)
    .sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
}

// ─── 라이브러리 그룹 (플러그인/소스별 묶음) ─────────────────────────────────
//
// 같은 스킬이 플러그인 폴터와 공용 폴터(~/.agents/skills)에 동시에 있을 수 있어서,
// 폴터 실제 경로(realpath) 기준으로 묶는다. 이름이 같아도 경로가 다륝면 다른 그룹이다.

function buildLibraryGroups(
  centralSkills: CentralSkillRow[],
  agents: AgentRow[],
  observations: ObservationRow[],
): DashboardLibraryGroup[] {
  const groups = new Map<string, DashboardLibraryGroup>();

  const addGroup = (id: string, label: string, path: string): DashboardLibraryGroup => {
    const existing = groups.get(id);
    if (existing) return existing;
    const group: DashboardLibraryGroup = { id, label, path, skills: [] };
    groups.set(id, group);
    return group;
  };

  // 1) 중앙 보관함(DB 기준 원본)
  const canonicalDirs = centralSkills
    .map((skill) => skill.canonical_path)
    .filter((path): path is string => Boolean(path))
    .map((path) => dirname(path));
  const vaultDir =
    canonicalDirs.length > 0
      ? canonicalDirs.sort((a, b) => a.length - b.length)[0]
      : join(homedir(), ".agents", "skills");
  const vaultGroup = addGroup("central", "Skill Vault", vaultDir);
  for (const skill of centralSkills) {
    vaultGroup.skills.push({
      id: skill.id,
      name: skill.name,
      description: skill.description,
      path: skill.canonical_path,
      sourceLabel: null,
      linkType: "central",
    });
  }

  // 2) 중앙 원본이 플랫폼에 링크된 폴터(예: universal = ~/.agents/skills)
  const linkedAgentIds = new Set(
    centralSkills.flatMap((skill) => (skill.linked_agent_ids ?? "").split(",")).filter(Boolean),
  );
  for (const agent of agents) {
    if (!linkedAgentIds.has(agent.id)) continue;
    const group = addGroup(
      `agent:${agent.id}`,
      agent.display_name,
      agent.global_skills_dir,
    );
    if (group.skills.length === 0) {
      for (const skill of centralSkills) {
        if (!(skill.linked_agent_ids ?? "").split(",").includes(agent.id)) continue;
        group.skills.push({
          id: skill.id,
          name: skill.name,
          description: skill.description,
          path: skill.canonical_path,
          sourceLabel: null,
          linkType: "symlink",
        });
      }
    }
  }

  // 3) 플러그인 등 읽기 전용으로 관측된 스킬 묶음 (경로별로 분리 → 이름 충돌 방지)
  const seenPluginSkill = new Set<string>();
  for (const obs of observations) {
    if (obs.source_kind !== "plugin") continue;
    const dedupeKey = `${obs.agent_id}:${obs.skill_id}:${obs.dir_path}`;
    if (seenPluginSkill.has(dedupeKey)) continue;
    seenPluginSkill.add(dedupeKey);

    let realDir = obs.dir_path;
    try {
      realDir = realpathSync(obs.dir_path);
    } catch {
      // 경로가 사라졌으면 원래 문자열을 그대로 쓴다.
    }
    const sourceLabel = obs.source_label ?? obs.agent_id;
    const group = addGroup(`plugin:${realDir}`, sourceLabel, realDir);
    group.skills.push({
      id: obs.skill_id,
      name: obs.name,
      description: obs.description,
      path: obs.dir_path,
      sourceLabel,
      linkType: "read-only",
    });
  }

  for (const group of groups.values()) {
    group.skills.sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
  }
  return [...groups.values()]
    .filter((group) => group.skills.length > 0)
    .sort((a, b) => {
      if (a.id === "central") return -1;
      if (b.id === "central") return 1;
      return b.skills.length - a.skills.length;
    });
}

// ─── 메인 스냅샷 ────────────────────────────────────────────────────────────

export function loadDashboardSnapshot(
  databasePath: string,
  options: { home?: string } = {},
) {
  const home = options.home ?? homedir();
  const database = new DatabaseSync(databasePath, { readOnly: true });

  try {
    const centralSkills = queryRows<CentralSkillRow>(
      database,
      `SELECT s.id,
              s.name,
              s.description,
              s.canonical_path,
              (SELECT GROUP_CONCAT(DISTINCT si.agent_id)
                 FROM skill_installations si
                WHERE si.skill_id = s.id) AS linked_agent_ids
         FROM skills s
        WHERE s.is_central = 1
        ORDER BY LOWER(s.name), s.id`,
    );

    const agentRows = queryRows<AgentRow>(
      database,
      `SELECT id, display_name, category, global_skills_dir, project_skills_dir,
              is_builtin, is_enabled
         FROM agents
        ORDER BY category, LOWER(display_name)`,
    );

    const installations = queryRows<InstallationRow>(
      database,
      `SELECT si.skill_id,
              si.agent_id,
              COALESCE(s.name, si.skill_id) AS name,
              s.description,
              si.installed_path,
              si.link_type
         FROM skill_installations si
         LEFT JOIN skills s ON s.id = si.skill_id`,
    );

    const observations = tableExists(database, "agent_skill_observations")
      ? queryRows<ObservationRow>(
          database,
          `SELECT skill_id, agent_id, name, description, dir_path, source_kind, source_label
             FROM agent_skill_observations`,
        )
      : [];

    const collections = queryRows<CollectionRow>(
      database,
      `SELECT id, name, description, created_at
         FROM collections
        ORDER BY created_at, LOWER(name)`,
    );

    const collectionSkills = queryRows<CollectionSkillRow>(
      database,
      `SELECT cs.collection_id,
              s.id,
              s.name,
              s.description
         FROM collection_skills cs
         JOIN skills s ON s.id = cs.skill_id`,
    );

    const discoveredRows = queryRows<DiscoveredRow>(
      database,
      `SELECT id, name, description, dir_path, project_path, project_name, platform_id
         FROM discovered_skills
        ORDER BY LOWER(name), id`,
    );

    const registries = tableExists(database, "skill_registries")
      ? queryRows<RegistryRow>(
          database,
          `SELECT id, name, url FROM skill_registries WHERE is_enabled = 1 ORDER BY LOWER(name)`,
        )
      : [];

    const marketplaceSkillRows = tableExists(database, "marketplace_skills")
      ? queryRows<MarketplaceSkillRow>(
          database,
          `SELECT id, registry_id, name, description, is_installed
             FROM marketplace_skills
            ORDER BY LOWER(name), id`,
        )
      : [];

    const language = getSetting(database, "app_language");

    // ── 설정 스냅샷 ──
    // API 키와 GitHub 토큰은 값을 읽지 않고 설정 여부만 계산한다.
    const aiProvider = getSetting(database, "ai_provider");
    const scanDirectories = tableExists(database, "scan_directories")
      ? queryRows<ScanDirectoryRow>(
          database,
          `SELECT id, path, label, is_active, is_builtin
             FROM scan_directories
            ORDER BY is_builtin, id`,
        ).map((row) => ({
          id: row.id,
          path: row.path,
          label: row.label,
          isActive: Boolean(row.is_active),
          isBuiltin: Boolean(row.is_builtin),
        }))
      : [];
    const customPlatforms = agentRows
      .filter((agent) => !agent.is_builtin && agent.category !== "central")
      .map((agent) => ({
        id: agent.id,
        displayName: agent.display_name,
        category: agent.category,
        globalSkillsDir: agent.global_skills_dir,
        projectSkillsDir: agent.project_skills_dir,
        isEnabled: Boolean(agent.is_enabled),
      }));

    // ── 플랫폼별 스킬 목록 조립 ──
    const platforms: DashboardPlatform[] = agentRows
      .filter((agent) => agent.category !== "central")
      .map((agent) => {
        const installed: DashboardSkillEntry[] = installations
          .filter((row) => row.agent_id === agent.id)
          .map((row) => ({
            id: row.skill_id,
            name: row.name,
            description: row.description,
            path: row.installed_path,
            sourceLabel: null,
            linkType: row.link_type,
          }));

        const observed: DashboardSkillEntry[] = observations
          .filter((row) => row.agent_id === agent.id)
          .filter(
            (row) =>
              !installed.some((entry) => entry.id === row.skill_id),
          )
          .map((row) => ({
            id: row.skill_id,
            name: row.name,
            description: row.description,
            path: row.dir_path,
            sourceLabel: row.source_label,
            linkType: row.source_kind === "plugin" ? "read-only" : row.source_kind,
          }));

        const skills = [...installed, ...observed].sort((a, b) =>
          a.name.toLowerCase().localeCompare(b.name.toLowerCase()),
        );

        return {
          id: agent.id,
          displayName: agent.display_name,
          category: agent.category,
          globalSkillsDir: agent.global_skills_dir,
          isDetected: isPlatformDetected(agent.global_skills_dir),
          isEnabled: Boolean(agent.is_enabled),
          skills,
        };
      });

    // ── 라이브러리 그룹 ──
    const libraryGroups = buildLibraryGroups(
      centralSkills,
      agentRows.filter((agent) => agent.category !== "central"),
      observations,
    );

    // ── 컬렉션 ──
    const dashboardCollections: DashboardCollection[] = collections.map((collection) => ({
      id: collection.id,
      name: collection.name,
      description: collection.description,
      skills: collectionSkills
        .filter((row) => row.collection_id === collection.id)
        .map((row) => ({
          id: row.id,
          name: row.name,
          description: row.description,
          path: null,
          sourceLabel: null,
          linkType: null,
        }))
        .sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase())),
    }));

    // ── 발견된 프로젝트 ──
    const projectMap = new Map<string, DashboardProject>();
    for (const row of discoveredRows) {
      const project = projectMap.get(row.project_path) ?? {
        projectPath: row.project_path,
        projectName: row.project_name,
        platforms: [],
        skills: [],
      };
      if (!project.platforms.includes(row.platform_id)) {
        project.platforms.push(row.platform_id);
      }
      project.skills.push({
        id: row.id,
        name: row.name,
        description: row.description,
        path: row.dir_path,
        sourceLabel: row.platform_id,
        linkType: null,
      });
      projectMap.set(row.project_path, project);
    }
    const discoveredProjects = [...projectMap.values()].sort((a, b) =>
      a.projectName.toLowerCase().localeCompare(b.projectName.toLowerCase()),
    );
    for (const project of discoveredProjects) {
      project.platforms.sort();
    }

    // ── 마켓플레이스 캐시 ──
    const marketplaceSources: DashboardMarketplaceSource[] = registries.map((registry) => ({
      id: registry.id,
      name: registry.name,
      url: registry.url,
      skillCount: marketplaceSkillRows.filter((row) => row.registry_id === registry.id).length,
    }));
    const marketplaceSkills: DashboardMarketplaceSkill[] = marketplaceSkillRows.map((row) => ({
      id: row.id,
      registryId: row.registry_id,
      name: row.name,
      description: row.description,
      isInstalled: Boolean(row.is_installed),
    }));

    const detectedPlatformCount = platforms.filter(
      (platform) => platform.isEnabled && platform.isDetected,
    ).length;

    return {
      generatedAt: new Date().toISOString(),
      appName: "skills-manage",
      language,
      summary: {
        centralSkillCount: centralSkills.length,
        detectedPlatformCount,
        collectionCount: dashboardCollections.length,
        discoveredProjectCount: discoveredProjects.length,
        discoveredSkillCount: discoveredRows.length,
        marketplaceSkillCount: marketplaceSkills.length,
      },
      libraryGroups,
      platforms,
      collections: dashboardCollections,
      discoveredProjects,
      marketplaceSources,
      marketplaceSkills,
      obsidianVaults: loadObsidianVaults(home),
      settings: {
        centralSkillsPath:
          getSetting(database, "central_skills_path") ??
          libraryGroups.find((group) => group.id === "central")?.path ??
          join(home, ".agents", "skills"),
        migrationState: getSetting(database, "central_vault_migration_state"),
        databasePath,
        scanDirectories,
        customPlatforms,
        githubPatConfigured: isSecretConfigured(database, "github_pat"),
        aiProvider,
        aiRegion: getSetting(database, "ai_region"),
        aiModel: aiProvider ? getSetting(database, `ai_model__${aiProvider}`) : null,
        aiProtocol: aiProvider
          ? getSetting(database, `ai_protocol__${aiProvider}`)
          : null,
        aiApiUrl: aiProvider
          ? getSetting(database, `ai_custom_base_url__${aiProvider}`) ||
            getSetting(database, `ai_api_url__${aiProvider}`)
          : null,
        aiApiKeyConfigured: aiProvider
          ? isSecretConfigured(database, `ai_api_key__${aiProvider}`)
          : false,
      },
    };
  } finally {
    database.close();
  }
}
