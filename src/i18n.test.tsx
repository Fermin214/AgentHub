import { beforeEach, expect, it, vi } from 'vitest';
import { render, screen, within, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import * as api from './api';
import App from './App';
import { makeTranslate, resolveLang } from './i18n';
import type { Settings, Snapshot } from './types';

const settings: Settings = { scanRoots: [], executables: { codex: '', claude: '', dsh: '' } };
const snapshot = (language: string): Snapshot => ({ dataScope: 'fixture', prompts: [], skills: [], deployments: [], projects: [], settings: { ...settings, language }, operations: [] });
beforeEach(() => { vi.restoreAllMocks(); vi.spyOn(api, 'isTauriRuntime').mockReturnValue(false); vi.spyOn(api, 'listBackups').mockResolvedValue({ backups: [] }); vi.spyOn(api, 'dispatch').mockImplementation(async (...[method]) => (method === 'targets.list' ? { targets: [] } : {}) as never); });

it('prefers the stored language over the system locale', () => {
  expect(resolveLang('en')).toBe('en');
  expect(resolveLang('zh')).toBe('zh');
  // An empty or unknown value follows the system, pinned to zh-CN in test-setup.
  expect(resolveLang('')).toBe('zh');
  expect(resolveLang('de')).toBe('zh');
});

it('translates core messages and falls back to the original text', () => {
  const en = makeTranslate('en');
  expect(en.backend('Skill 不存在')).toBe('That Skill no longer exists.');
  expect(en.backend('Error: Skill 不存在')).toBe('That Skill no longer exists.');
  expect(en.backend('磁盘不可写')).toBe('磁盘不可写');
  expect(en.backend('添加 Prompt · 中文标题 · 原样保留')).toBe('Added prompt: 中文标题 · 原样保留');
  expect(en.backend('删除仓库收藏 · example')).toBe('Deleted saved repository: example');
  expect(en.backend('更新 Skill完成：我的 Skill')).toBe('Updated Skill 我的 Skill');
  expect(en.backend('更改项目目录：本机项目 · 项目名称')).toBe('Changed local project folder: 项目名称');
  expect(makeTranslate('zh').backend('Skill 不存在')).toBe('Skill 不存在');
});

it('uses stable error codes when diagnostics change and preserves affected paths', () => {
  const en = makeTranslate('en');
  for (const detail of ['旧诊断', '完全不同的新诊断']) {
    const wire = JSON.stringify({code:'GIT_TRUST_REQUIRED',params:{path:'C:/项目'},detail});
    expect(en.backend('Error: ' + wire)).toBe('Confirm that you trust this project before running Git checks.\nC:/项目\n' + detail);
  }
  expect(en.backend(JSON.stringify({code:'CORE_FAILURE',params:{},detail:'Skill 不存在'}))).toBe('That Skill no longer exists.');
});

it('opens in the stored language and switches the whole shell when it changes', async () => {
  let stored = 'en';
  vi.spyOn(api, 'getSnapshot').mockImplementation(async () => snapshot(stored));
  const save = vi.spyOn(api, 'saveSettings').mockImplementation(async (...[next]) => { stored = next.language || ''; return next; });
  render(<App/>);
  const navigation = within(await screen.findByRole('navigation', { name: 'Main navigation' }));
  expect(navigation.getAllByRole('button').map(b => b.querySelector('strong')?.textContent)).toEqual(['Prompts', 'Skills', 'Projects', 'Settings']);
  await waitFor(() => expect(document.documentElement.lang).toBe('en'));

  await userEvent.click(screen.getByRole('button', { name: 'Settings Agents and local preferences' }));
  await userEvent.click(screen.getByRole('tab', { name: 'About' }));
  await userEvent.selectOptions(await screen.findByLabelText('Language'), 'zh');
  expect(save).toHaveBeenCalledWith(expect.objectContaining({ language: 'zh' }));
  expect(await screen.findByRole('navigation', { name: '主导航' })).toBeVisible();
  await waitFor(() => expect(document.documentElement.lang).toBe('zh-CN'));
  expect(screen.getByRole('status')).toHaveTextContent('界面语言已切换');
  await userEvent.selectOptions(screen.getByLabelText('界面语言'), 'en');
  expect(await screen.findByRole('navigation', { name: 'Main navigation' })).toBeVisible();
  expect(screen.getByRole('status')).toHaveTextContent('Language changed');
});

it('keeps the selected language and reports the failure when saving is rejected', async () => {
  vi.spyOn(api, 'getSnapshot').mockResolvedValue(snapshot('en'));
  vi.spyOn(api, 'saveSettings').mockRejectedValue(new Error('磁盘不可写'));
  render(<App/>);
  await userEvent.click(await screen.findByRole('button', { name: 'Settings Agents and local preferences' }));
  await userEvent.click(screen.getByRole('tab', { name: 'About' }));
  await userEvent.selectOptions(await screen.findByLabelText('Language'), 'zh');
  expect(await screen.findByRole('status')).toHaveTextContent('磁盘不可写');
  expect(screen.getByLabelText('Language')).toHaveValue('en');
  expect(document.documentElement.lang).toBe('en');
});
