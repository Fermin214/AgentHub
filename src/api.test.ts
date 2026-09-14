import { afterEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { dispatch } from './api';
import { makeTranslate } from './i18n';
import type { Response } from './contracts';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
afterEach(() => { delete window.__TAURI_INTERNALS__; vi.resetAllMocks(); });

it('preserves coded IPC failures through Error.message and localization', async () => {
  window.__TAURI_INTERNALS__ = {};
  const fault = { code: 'RECOVERY_REQUIRED', params: { paths: ['C:/fixture'] }, detail: 'unexpected incoming content' };
  vi.mocked(invoke).mockRejectedValue(fault);
  const error = await dispatch('skills.scan').catch(cause => cause as Error);
  expect(error).toBeInstanceOf(Error);
  expect(JSON.parse((error as Error).message)).toEqual(fault);
  expect(makeTranslate('en').backend(String(error))).toContain('Skill file recovery is required');
  expect(invoke).toHaveBeenCalledWith('dispatch', {method:'skills.scan',args:{}});
});

it('does not convert legacy native failures into a demo success', async () => {
  window.__TAURI_INTERNALS__ = {};
  vi.mocked(invoke).mockRejectedValue('native failure');
  await expect(dispatch('snapshot')).rejects.toBe('native failure');
  expect(invoke).toHaveBeenCalledOnce();
  expect(invoke).toHaveBeenCalledWith('dispatch', { method: 'snapshot', args: {} });
});

it('forwards a typed request without reshaping the native response', async () => {
  window.__TAURI_INTERNALS__ = {};
  const result: Response<'skills.read'> = { path: 'SKILL.md', content: '# Writer' };
  vi.mocked(invoke).mockResolvedValue(result);
  expect(await dispatch('skills.read', { skillId: 'writer', path: 'SKILL.md' })).toBe(result);
  expect(invoke).toHaveBeenCalledOnce();
  expect(invoke).toHaveBeenCalledWith('dispatch', {
    method: 'skills.read', args: { skillId: 'writer', path: 'SKILL.md' },
  });
});

it('keeps browser preview read-only through the same dispatch entry', async () => {
  const snapshot = await dispatch('snapshot');
  expect(snapshot.dataScope).toBe('static-preview');
  await expect(dispatch('skills.remove', { planId: 'plan', confirmed: true })).rejects.toThrow('只读预览');
  expect(invoke).not.toHaveBeenCalled();
});
