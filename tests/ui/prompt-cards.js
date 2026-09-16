// Deterministic Prompt card harness: the real PromptsPage component with the
// real stylesheets, mounted against fictional long titles. No filesystem, IPC,
// or network access; the preview/edit/delete handlers only record calls.
import React from 'react';
import { createRoot } from 'react-dom/client';
import { PromptsPage } from '/src/PromptsPage.tsx';
import { LanguageProvider } from '/src/i18n.tsx';

const date = '2025-01-01T00:00:00Z';

/// Titles exist only to exercise wrapping; none of them are product copy.
const titles = {
  chinese: '请把这段很长的中文标题写完整一些用于检查卡片标题在连续中文下的换行表现是否依然不会遮挡右侧按钮并且保持可读',
  english: 'Summarize the incident timeline, the follow-up actions, and every open question for each stakeholder in one reusable prompt',
  unbroken: 'AntiDisestablishmentarianismPneumonoultramicroscopicsilicovolcanoconiosisFloccinaucinihilipilificationPneumonoultramicroscopicsilicovolcanoconiosisFloccinaucinihilipilificationAntiDisestablishmentarianism',
  mixed: '混合 Mixed 标题 with supercalifragilisticexpialidociousunbreakabletoken 以及中文',
  short: '把问题说明白',
};

const prompts = Object.entries(titles).map(([key, title], index) => ({
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

window.__promptCardHarness = { titles, calls: [] };

function Harness() {
  return React.createElement(
    LanguageProvider,
    { lang: 'zh' },
    React.createElement(PromptsPage, {
      prompts,
      notify: (message) => window.__promptCardHarness.calls.push({ kind: 'notify', message }),
      onSnapshot: () => undefined,
    }),
  );
}

createRoot(document.getElementById('root')).render(React.createElement(Harness));
