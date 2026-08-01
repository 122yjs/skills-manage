export interface DashboardSummary {
  centralSkillCount: number;
  detectedPlatformCount: number;
  collectionCount: number;
  discoveredProjectCount: number;
  discoveredSkillCount: number;
  marketplaceSkillCount: number;
}

export interface DashboardSkillEntry {
  id: string;
  name: string;
  description: string | null;
  path: string | null;
  /** 읽기 전용(플러그인 등)이거나 관리 대상이 아니어서 토글할 수 없는 경우 사유 */
  sourceLabel: string | null;
  linkType: string | null;
}

export interface DashboardPlatform {
  id: string;
  displayName: string;
  category: string;
  globalSkillsDir: string;
  isDetected: boolean;
  isEnabled: boolean;
  skills: DashboardSkillEntry[];
}

export interface DashboardLibraryGroup {
  id: string;
  label: string;
  path: string;
  skills: DashboardSkillEntry[];
}

export interface DashboardCollection {
  id: string;
  name: string;
  description: string | null;
  skills: DashboardSkillEntry[];
}

export interface DashboardProject {
  projectPath: string;
  projectName: string;
  platforms: string[];
  skills: DashboardSkillEntry[];
}

export interface DashboardMarketplaceSource {
  id: string;
  name: string;
  url: string;
  skillCount: number;
}

export interface DashboardMarketplaceSkill {
  id: string;
  registryId: string;
  name: string;
  description: string | null;
  isInstalled: boolean;
}

export interface DashboardObsidianVault {
  id: string;
  name: string;
  path: string;
  skillCount: number;
}

export interface DashboardScanDirectory {
  id: number;
  path: string;
  label: string | null;
  isActive: boolean;
  isBuiltin: boolean;
}

export interface DashboardCustomPlatform {
  id: string;
  displayName: string;
  category: string;
  globalSkillsDir: string;
  projectSkillsDir: string | null;
  isEnabled: boolean;
}

export interface DashboardSettings {
  centralSkillsPath: string;
  migrationState: string | null;
  databasePath: string;
  scanDirectories: DashboardScanDirectory[];
  customPlatforms: DashboardCustomPlatform[];
  githubPatConfigured: boolean;
  aiProvider: string | null;
  aiRegion: string | null;
  aiModel: string | null;
  aiProtocol: string | null;
  aiApiUrl: string | null;
  aiApiKeyConfigured: boolean;
}

export interface DashboardSnapshot {
  generatedAt: string;
  appName: string;
  language: string | null;
  summary: DashboardSummary;
  libraryGroups: DashboardLibraryGroup[];
  platforms: DashboardPlatform[];
  collections: DashboardCollection[];
  discoveredProjects: DashboardProject[];
  marketplaceSources: DashboardMarketplaceSource[];
  marketplaceSkills: DashboardMarketplaceSkill[];
  obsidianVaults: DashboardObsidianVault[];
  settings: DashboardSettings;
}
