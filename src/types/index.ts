// ─── Agent Types ─────────────────────────────────────────────────────────────

export interface AgentWithStatus {
  id: string;
  display_name: string;
  category: string;
  global_skills_dir: string;
  project_skills_dir?: string;
  icon_name?: string;
  is_detected: boolean;
  is_builtin: boolean;
  is_enabled: boolean;
}

export interface CustomAgentConfig {
  id?: string;
  display_name: string;
  category?: string;
  global_skills_dir: string;
}

export interface UpdateCustomAgentConfig {
  display_name: string;
  category?: string;
  global_skills_dir: string;
}

export interface DevToolSetupState {
  completed: boolean;
  tools: AgentWithStatus[];
}

// ─── Scan Types ───────────────────────────────────────────────────────────────

export interface ScanResult {
  total_skills: number;
  agents_scanned: number;
  skills_by_agent: Record<string, number>;
}

export type ClaudeSourceKind = "user" | "plugin" | "compatibility" | "unmanaged";

export interface ScannedSkill {
  id: string;
  row_id?: string;
  name: string;
  description?: string;
  file_path: string;
  dir_path: string;
  link_type: string;
  symlink_target?: string;
  is_central: boolean;
  source_kind?: ClaudeSourceKind | null;
  source_root?: string | null;
  source_label?: string | null;
  is_read_only?: boolean;
  conflict_group?: string | null;
  conflict_count?: number;
}

// ─── Skill Types ──────────────────────────────────────────────────────────────

export interface Skill {
  id: string;
  name: string;
  description?: string;
  file_path: string;
  canonical_path?: string;
  is_central: boolean;
  source?: string;
  content?: string;
  scanned_at: string;
}

export interface SkillInstallation {
  skill_id: string;
  agent_id: string;
  installed_path: string;
  link_type: string;
  symlink_target?: string;
  /** ISO 8601 timestamp of when the skill was first installed. */
  installed_at?: string;
}

export interface SkillDetail extends Omit<Skill, "content"> {
  row_id?: string;
  dir_path?: string;
  source_kind?: ClaudeSourceKind | null;
  source_root?: string | null;
  source_label?: string | null;
  is_read_only?: boolean;
  conflict_group?: string | null;
  conflict_count?: number;
  /** Agent IDs that can see this central skill through a read-only compatibility root. */
  read_only_agents?: string[];
  installations: SkillInstallation[];
  /** Collections this skill currently belongs to. */
  collections?: Collection[];
}

export interface SelectedSkillFile {
  path: string;
  relativePath: string;
}

export interface SkillDirectoryNode {
  name: string;
  path: string;
  relative_path: string;
  is_dir: boolean;
  children: SkillDirectoryNode[];
}

export interface SkillDetailRequest {
  skillId: string;
  agentId?: string;
  rowId?: string;
}

export interface SkillWithLinks {
  id: string;
  name: string;
  description?: string;
  file_path: string;
  canonical_path?: string;
  is_central: boolean;
  source?: string;
  scanned_at: string;
  created_at?: string;
  updated_at?: string;
  /** Agent IDs that currently have this skill installed (symlink or copy). */
  linked_agents: string[];
  /** Agent IDs that can see this skill through a read-only compatibility root. */
  read_only_agents?: string[];
}

// ─── Skill Description Translation Types ────────────────────────────────────

/** 저장소의 SKILL.md 또는 README가 직접 제공한 언어별 설명. */
export type LocalizedSkillDescriptions = Record<string, string>;

export type SkillDescriptionResolutionSource =
  | "requested-locale"
  | "english-fallback"
  | "original-fallback";

export interface ResolvedSkillDescription {
  text: string;
  /** 저장소 제공 설명에서 고른 실제 언어. 원문 fallback은 알 수 없을 수 있다. */
  locale?: string;
  source: SkillDescriptionResolutionSource;
}

export type SkillDescriptionTranslationEngine = "apple" | "api";

export interface SkillDescriptionTranslation {
  resource_id: string;
  source_hash: string;
  source_text: string;
  source_locale?: string | null;
  target_locale: string;
  engine: SkillDescriptionTranslationEngine | string;
  translated_text: string;
  created_at: string;
  updated_at: string;
}

/** 카드와 상세 화면이 저장소 설명 및 번역 캐시를 찾을 때 쓰는 안정적인 식별 정보. */
export interface SkillDescriptionTranslationMeta {
  resourceId: string;
  filePath?: string;
  sourceLocale?: string;
  localizedDescriptions?: LocalizedSkillDescriptions;
}

export interface SkillDescriptionTranslationResult {
  translatedText: string;
  engine: SkillDescriptionTranslationEngine | string;
  targetLocale: string;
  cached: boolean;
}

export interface BatchInstallResult {
  succeeded: string[];
  failed: Array<{ agent_id: string; error: string }>;
}

export interface SkillBundleInstallResult extends BatchInstallResult {
  imported: string[];
  skipped: string[];
}

export interface DeleteCentralSkillOptions {
  cascadeUninstall: boolean;
}

export interface DeleteCentralSkillResult {
  skillId: string;
  removedCanonicalPath: string;
  uninstalledAgents: string[];
  skippedReadOnlyAgents: string[];
}

export interface CentralSkillBundle {
  name: string;
  relativePath: string;
  path: string;
  isSymlink: boolean;
  skillCount: number;
  linkedAgentCount: number;
  readOnlyAgentCount: number;
}

export interface CentralSkillBundleDeletePreview {
  bundle: CentralSkillBundle;
  skills: SkillWithLinks[];
  affectedAgents: string[];
  skippedReadOnlyAgents: string[];
}

export interface CentralSkillBundleDetail {
  bundle: CentralSkillBundle;
  skills: SkillWithLinks[];
}

export interface DeleteCentralSkillBundleOptions {
  cascadeUninstall: boolean;
}

export interface DeleteCentralSkillBundleResult {
  relativePath: string;
  removedBundlePath: string;
  removedKind: "directory" | "symlink" | string;
  removedSkillIds: string[];
  uninstalledAgents: string[];
  skippedReadOnlyAgents: string[];
}

// ─── Collection Types ─────────────────────────────────────────────────────────

export interface Collection {
  id: string;
  name: string;
  description?: string;
  created_at: string;
  updated_at: string;
}

export interface CollectionWithSkills extends Collection {
  skill_ids: string[];
}

export interface CollectionDetail extends Collection {
  /** Full skill objects that are members of this collection. */
  skills: Skill[];
}

export interface CollectionBatchInstallResult {
  succeeded: string[];
  failed: Array<{ agent_id: string; error: string }>;
}

// ─── Settings Types ───────────────────────────────────────────────────────────

export interface ScanDirectory {
  id: number;
  path: string;
  label?: string;
  is_active: boolean;
  is_builtin: boolean;
  added_at: string;
}

export type MigrationState = "pending" | "deferred" | "completed";

export interface CentralVaultStatus {
  central_path: string;
  default_central_path: string;
  legacy_path: string;
  universal_path: string;
  migration_state: MigrationState;
  migration_required: boolean;
  legacy_skill_count: number;
}

export interface StoragePreview {
  current_path: string;
  new_path: string;
  skill_count: number;
  conflicts: string[];
  can_proceed: boolean;
}

export interface StorageChangeResult {
  central_path: string;
  skill_count: number;
}

// ─── Discover Types ───────────────────────────────────────────────────────────

export interface ScanRoot {
  path: string;
  label: string;
  exists: boolean;
  enabled: boolean;
}

export interface ObsidianVault {
  id: string;
  name: string;
  path: string;
  skill_count: number;
}

export interface DiscoveredSkill {
  id: string;
  name: string;
  description?: string;
  file_path: string;
  dir_path: string;
  platform_id: string;
  platform_name: string;
  project_path: string;
  project_name: string;
  is_already_central: boolean;
}

export interface DiscoveredProject {
  project_path: string;
  project_name: string;
  skills: DiscoveredSkill[];
}

export interface DiscoverResult {
  total_projects: number;
  total_skills: number;
  projects: DiscoveredProject[];
}

export interface DiscoverProgressPayload {
  percent: number;
  current_path: string;
  skills_found: number;
  projects_found: number;
}

export interface DiscoverFoundPayload {
  project: DiscoveredProject;
}

export interface DiscoverCompletePayload {
  total_projects: number;
  total_skills: number;
}

export type ImportTarget =
  | { type: "central" }
  | { type: "platform"; agent_id: string };

export interface DiscoverImportResult {
  skill_id: string;
  target: string;
}

// ─── Marketplace Types ───────────────────────────────────────────────────────

export interface SkillRegistry {
  id: string;
  name: string;
  source_type: "github" | "http_json";
  url: string;
  normalized_url?: string | null;
  is_builtin: boolean;
  is_enabled: boolean;
  last_synced: string | null;
  last_attempted_sync?: string | null;
  last_sync_status?: "never" | "success" | "error";
  last_sync_error?: string | null;
  cache_updated_at?: string | null;
  cache_expires_at?: string | null;
  etag?: string | null;
  last_modified?: string | null;
  created_at: string;
}

export interface MarketplaceSkill {
  id: string;
  registry_id: string;
  name: string;
  description?: string;
  download_url: string;
  is_installed: boolean;
  synced_at: string;
  cache_updated_at?: string | null;
}

export interface SkillsShSkill {
  id: string;
  skill_id: string;
  name: string;
  source: string;
  installs: number;
  stars?: number | null;
}

export interface SkillsShFileEntry {
  name: string;
  path: string;
  is_dir: boolean;
}

export interface GitHubRepoRef {
  owner: string;
  repo: string;
  branch: string;
  normalizedUrl: string;
}

export interface GitHubSkillConflict {
  existingSkillId: string;
  existingName: string;
  existingCanonicalPath?: string | null;
  proposedSkillId: string;
  proposedName: string;
}

export interface GitHubSkillPreview {
  sourcePath: string;
  skillId: string;
  skillName: string;
  description?: string | null;
  rootDirectory: string;
  skillDirectoryName: string;
  downloadUrl: string;
  conflict?: GitHubSkillConflict | null;
}

export interface GitHubRepoPreview {
  repo: GitHubRepoRef;
  skills: GitHubSkillPreview[];
}

export type DuplicateResolution = "overwrite" | "skip" | "rename";

export interface GitHubSkillImportSelection {
  sourcePath: string;
  resolution: DuplicateResolution;
  renamedSkillId?: string | null;
}

export interface ImportedGitHubSkillSummary {
  sourcePath: string;
  originalSkillId: string;
  importedSkillId: string;
  skillName: string;
  targetDirectory: string;
  resolution: DuplicateResolution;
}

export interface GitHubRepoImportResult {
  repo: GitHubRepoRef;
  importedSkills: ImportedGitHubSkillSummary[];
  skippedSkills: string[];
}

export type GitHubImportProgressPhase = "preparing" | "writing" | "finalizing";

export interface GitHubImportProgressPayload {
  phase: GitHubImportProgressPhase;
  currentSkill?: string | null;
  currentPath?: string | null;
  completedFiles: number;
  totalFiles: number;
  completedBytes: number;
  totalBytes: number;
}
