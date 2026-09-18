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

// ─── GitHub Origin / Update Types ────────────────────────────────────────────

export interface GitHubSkillOriginSummary {
  owner: string;
  repo: string;
  sourcePath: string;
  refName: string;
}

export interface SkillOriginInfo {
  bindingId: string;
  targetKey: string;
  targetPath: string;
  repositoryId?: string | null;
  owner: string;
  repo: string;
  sourcePath: string;
  refName: string;
  baselineState: "verified" | "unknown" | string;
  baseCommitOid?: string | null;
  lastAppliedCommitOid?: string | null;
  lastAppliedAt?: string | null;
  lastCheckedAt?: string | null;
  lastRemoteCommitOid?: string | null;
  lastError?: string | null;
  bindingVersion: number;
  canUpdate: boolean;
}

export type OriginSyncState =
  | "unknown_baseline"
  | "up_to_date"
  | "remote_update"
  | "local_changes"
  | "diverged"
  | "local_matches_remote";

export interface ManifestChangeSummary {
  added: number;
  modified: number;
  removed: number;
}

export interface SkillOriginStatus {
  origin: SkillOriginInfo;
  state: OriginSyncState;
  localVsRemote: ManifestChangeSummary;
  localVsBase: ManifestChangeSummary;
  remoteVsBase: ManifestChangeSummary;
  remoteCommitOid: string;
}

export interface SkillUpdatePlan {
  operationId: string;
  bindingId: string;
  targetPath: string;
  remoteCommitOid: string;
  state: OriginSyncState;
  changes: ManifestChangeSummary;
  requiresLocalChangeConfirmation: boolean;
}

export interface SkillUpdateResult {
  operationId: string;
  bindingId: string;
  appliedCommitOid: string;
  recoveryEntryId?: string | null;
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
  available_sources?: Record<string, string[]>;
  /** GitHub repository origin recorded when this skill was imported. */
  origin?: GitHubSkillOriginSummary | null;
}

// ─── Skill Usage Types ──────────────────────────────────────────────────────

/** 앱이 관리하는 설치 한 건의 실제 활성 상태다. */
export interface UsageSkillStatus {
  skill_id: string;
  name: string;
  enabled: boolean;
  /** 플랫폼 전체 비활성으로 멈췄으며, 전체 복원 때만 다시 활성화되는 항목이다. */
  paused_by_bulk: boolean;
}

/** 한 플랫폼에서 앱이 관리하는 설치와 외부 설치를 구분한 요약이다. */
export interface UsageStatus {
  agent_id: string;
  active_count: number;
  paused_count: number;
  /** 공용 설치·플러그인처럼 이 화면에서 바꿀 수 없는 관측 항목 수다. */
  external_count: number;
  skills: UsageSkillStatus[];
}

/** 공용 설치를 실제로 읽는다고 확인된 플랫폼이다. */
export interface SharedSkillConfirmedPlatform {
  agent_id: string;
  display_name: string;
}

/** 같은 원본을 가리키지만 독립적으로 유지되는 별도 설치다. */
export interface SharedSkillSeparateInstall {
  agent_id: string;
  display_name: string;
  source_path: string;
}

/** 공용 설치 한 건의 확인된 영향 범위다. reason이 있으면 토글할 수 없다. */
export interface SharedSkillImpact {
  shared_install_id: string;
  skill_id: string;
  skill_name: string;
  enabled: boolean;
  confirmed_platforms: SharedSkillConfirmedPlatform[];
  separate_installs: SharedSkillSeparateInstall[];
  reason: string | null;
  management_path: string;
  confirmation_token: string;
}

export interface SharedSkillUsageResult {
  applied: boolean;
  impact: SharedSkillImpact;
}

export interface SharedSkillConfirmation {
  shared_install_id: string;
  confirmation_token: string;
}

export interface SharedPlatformUsageResult {
  applied: boolean;
  impacts: SharedSkillImpact[];
  failed: Array<{ skill_id: string; error: string }>;
}

/** 플랫폼 설정 Adapter가 확인한 한 출처의 실제 제어 상태다. */
export interface PlatformSkillControlStatus {
  agent_id: string;
  skill_id: string;
  row_id: string;
  skill_name: string;
  source_path: string;
  source_kind?: ClaudeSourceKind | null;
  state: "active" | "inactive" | "deleted" | "unsupported" | string;
  supported: boolean;
  can_toggle: boolean;
  can_delete: boolean;
  can_reapply: boolean;
  reason?: string | null;
  requires_reload: boolean;
  scope: "path" | "name" | string;
  affected_source_count: number;
  adapter: string;
  config_path?: string | null;
  /** 공용 설치 영향 범위. 공용 설치가 아니면 null이다. */
  shared_install?: SharedSkillImpact | null;
  /** 이 플랫폼에서만 제외된 실제 상태. 관리 설치면 false다. */
  excluded_here?: boolean;
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

export interface SkillTransferSource {
  skill_id: string;
  source_agent_id?: string;
  row_id?: string;
}

// ─── Settings Types ───────────────────────────────────────────────────────────

export type { RecoveryEntry } from "./recovery";

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

export type GitHubSkillConflictKind =
  | "central"
  | "non_central"
  | "unmanaged_path";

export interface GitHubSkillConflict {
  existingSkillId: string;
  existingName: string;
  existingCanonicalPath?: string | null;
  /** Path that already owns the conflicting id; the colliding target for unmanaged paths. */
  existingPath: string;
  /** Ownership of the conflicting path; only `central` conflicts can be overwritten. */
  conflictKind: GitHubSkillConflictKind;
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

export interface GitHubImportFailure {
  /** `blocked` wrote nothing; `failed` committed earlier skills in the same batch. */
  code: "blocked" | "failed";
  message: string;
  sourcePath?: string | null;
  skillId?: string | null;
  existingPath?: string | null;
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
