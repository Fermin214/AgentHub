// Deterministic isolated entry for the Prompt card and Skill check summary
// layout contracts. It mounts the real App shell (sidebar, page wrap, styles)
// against a fictional snapshot served through the simulated IPC transport, so
// the measurements use the production load path and the real content width
// instead of a hand written page. No filesystem, network, or real Agent access.
//
// The page reads `?lang=zh|en` and `?view=prompts|skills`:
//   zh|en   selects the interface language the snapshot requests
//   prompts opens the default Prompts page
//   skills  opens the Skill page so its check summary renders
import React from 'react';
import { createRoot } from 'react-dom/client';
import App from '/src/App.tsx';

const params = new URLSearchParams(location.search);
const lang = params.get('lang') === 'en' ? 'en' : 'zh';
const view = params.get('view') === 'skills' ? 'skills' : 'prompts';
const date = '2025-01-01T00:00:00Z';

/// Titles exist only to exercise wrapping; none of them are product copy.
const promptTitles = {
  chinese: '请把这段很长的中文标题写完整一些用于检查卡片标题在连续中文下的换行表现是否依然不会挤压右侧按钮并且保持可读',
  english: 'Summarize the incident timeline, the follow-up actions, and every open question for each stakeholder in one reusable prompt',
  unbroken: 'AntiDisestablishmentarianismPneumonoultramicroscopicsilicovolcanoconiosisFloccinaucinihilipilificationPneumonoultramicroscopicsilicovolcanoconiosisFloccinaucinihilipilificationAntiDisestablishmentarianism',
  mixed: '混合 Mixed 标题 with supercalifragilisticexpialidociousunbreakabletoken 以及中文',
  short: '把问题说明白',
};

const prompts = Object.entries(promptTitles).map(([key, title], index) => ({
  id: key,
  title,
  body: '第一行示例正文。\n第二行示例正文，用于检查预览高度与截断。\n第三行示例正文。',
  purpose: index % 2 ? '长用途说明示例：用于检查用途行的截断表现。' : '',
  category: '',
  tags: index % 2 ? ['分析', '一个比较长的标签用于检查换行'] : ['分析'],
  favorite: index === 0,
  createdAt: date,
  updatedAt: date,
}));

const skills = [{
  id: 'skill-long',
  name: '写作助手 · 长文示例',
  description: '示例 Skill，用于检查检查摘要的排版。',
  path: 'C:/AcceptanceFixture/library/long',
  source: { kind: 'git', locator: 'https://example.invalid/fixture/skills' },
  tags: ['写作', '示例'],
  favorite: true,
  createdAt: date,
  updatedAt: date,
}];

/// A real check record gives the summary its `.check-time` run, which the shared
/// preview fixture never has.
const skillUpdates = [{
  skillId: 'skill-long',
  name: skills[0].name,
  status: 'current',
  message: 'Skill 内容没有变化',
  checkId: 'fixture-check',
  checkedAt: '2026-09-01T09:00:00Z',
  fetchedAt: '2026-09-01T09:00:00Z',
  locations: [],
}];

const snapshot = {
  dataScope: 'isolated-preview-prompt-cards',
  dataDir: '仅用于预览的示例数据',
  prompts,
  skills,
  skillUpdates,
  deployments: [],
  projects: [],
  settings: { scanRoots: [], executables: { codex: '', claude: '', dsh: '', zcode: '', hermes: '' }, language: lang },
  operations: [],
};

window.__TAURI_INTERNALS__ = {
  invoke: async (command, { method } = {}) => {
    if (command !== 'dispatch') throw new Error('这个操作请在正式桌面版中使用。');
    if (method === 'snapshot') return structuredClone(snapshot);
    if (method === 'targets.list') return { targets: [] };
    if (method === 'repositories.list') return { repositories: [] };
    if (method === 'backups.list') return { backups: [] };
    if (method === 'maintenance.get') return { automaticChecks: false, intervalHours: 24, retainUpdateBackup: true, maxBackups: null };
    if (method === 'network.get') return { proxyUrl: '' };
    throw new Error('这个操作不在本次预览场景中。');
  },
};

window.__promptCardHarness = { lang, view, promptTitles, skillId: skills[0].id };

// The App is the production load path; the spec opens the Skill page through the
// real sidebar navigation instead of remounting a page component directly.
createRoot(document.getElementById('root')).render(React.createElement(App));
