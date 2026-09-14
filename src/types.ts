export type Prompt = {
  id: string;
  title: string;
  body: string;
  purpose?: string;
  category: string;
  tags: string[];
  favorite: boolean;
  createdAt: string;
  updatedAt: string;
};

export type SourceKind = 'git' | 'local' | 'zip' | 'https' | 'unknown';

export type Source = {
  kind: SourceKind;
  locator: string;
  subpath?: string;
  revision?: string;
};

export type AgentKey = string;
export type DeploymentScope = 'global' | 'project' | 'profile';
export type ScanRoot = { id: string; agent: AgentKey; path: string; scope: DeploymentScope; profile?: string };
export type Settings = { scanRoots: ScanRoot[]; executables: { codex: string; claude: string; dsh: string; hermes?: string; zcode?: string }; language?: string };
export type Skill = { id: string; name: string; description: string; path: string; source: Source; sourceDigest?: string; tags: string[]; favorite: boolean; createdAt: string; updatedAt: string };
export type SkillDeployment = { id: string; skillId?: string; name: string; description: string; agent: string; scope: DeploymentScope; profile?: string; projectId?: string; path: string; source: Source; owner: string; status: string; ignored: boolean; baselineDigest?: string; present?: boolean; pathKey?: string };
export type LocalProject = { id: string; name: string; path: string; gitTrusted: boolean; archived: boolean; createdAt: string; updatedAt: string };
export type UpdateState = 'available' | 'current' | 'local' | 'unsupported' | 'failed' | 'modified' | 'diverged' | 'different';
export type FileDifference = { path: string; change: string; oldSize?: number; newSize?: number };
export type SkillLocation = { id: string; path: string; label: string; agents: string[]; exists: boolean; differences: FileDifference[] };
export type UpdateCheck = { skillId?: string; projectId?: string; name: string; status: UpdateState; message: string; checkId?: string; checkedAt?: string; fetchedAt?: string; needsRefresh?: boolean; locations?: SkillLocation[] };
export type SkillChangeAction = 'install' | 'remove' | 'update' | 'delete';
export type SkillChangePlan = { id: string; skillId: string; action: SkillChangeAction; summary: string; canExecute: boolean; blockedReason?: string; createdAt: string; locations: SkillLocation[]; agents?: string[] };
export type ChangeResult = { id: string; status: 'succeeded' | 'failed' | 'partial'; summary: string; createdAt: string; locations?: string[]; warnings?: string[] };
export type BackupRecord = { id: string; name: string; path: string; createdAt: string; originalPath: string; locations?: Array<{path: string; label: string}> };
export type RecoveryState = { status: 'ready' | 'restricted'; code?: 'RECOVERY_REQUIRED'; detail?: string; issues: Array<{ id?: string; state: string; paths: string[]; recordValid?: boolean }> };
export type Snapshot = { recovery?: RecoveryState; dataScope?: string; dataDir?: string; prompts: Prompt[]; skills: Skill[]; skillUpdates?: UpdateCheck[]; deployments: SkillDeployment[]; projects: LocalProject[]; settings: Settings; operations: ChangeResult[] };
export type ScanResult = { deployments: SkillDeployment[]; warnings: string[]; completedRoots: number; totalRoots: number };
export type SkillCandidate = { name: string; description: string; subpath: string; skillId?: string; contentComparison?:{same:string[];different:string[];unknown:string[]} };
// Agent product names are brand names and stay untranslated; `shared` is our own
// synthetic label, so it comes from the dictionary via `agentLabels`.
export const AGENT_LABELS: Record<string, string> = { codex: 'Codex', claude: 'Claude Code', dsh: 'DeepSeek Harness', hermes: 'Hermes', zcode: 'ZCode' };
export const agentLabels = (t: (key: string) => string): Record<string, string> => ({ ...AGENT_LABELS, shared: t('agent.shared') });
export const scopeLabels = (t: (key: string) => string): Record<DeploymentScope, string> => ({ global: t('scope.global'), project: t('scope.project'), profile: t('scope.profile') });
