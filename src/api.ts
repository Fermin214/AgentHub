import { invoke } from '@tauri-apps/api/core';
import { writeText as writeNativeText } from '@tauri-apps/plugin-clipboard-manager';
import { open as openNativeFile, save as saveNativeFile } from '@tauri-apps/plugin-dialog';
import { writeTextFile } from '@tauri-apps/plugin-fs';
import { dispatchDemo } from './demo';
import type { Prompt, Settings } from './types';
import type { DispatchCall, Method, Response } from './contracts';

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

export const isTauriRuntime = () => typeof window !== 'undefined' && Boolean(window.__TAURI_INTERNALS__);

export async function dispatch<M extends Method>(...call: DispatchCall<M>): Promise<Response<M>> {
  const [method, args = {}] = call;
  if (isTauriRuntime()) {
    // A real Tauri error is intentionally allowed to bubble up to the UI.
    try {
      return await invoke<Response<M>>('dispatch', { method, args });
    } catch (error) {
      if (error && typeof error === 'object' && 'code' in error && typeof error.code === 'string' && 'detail' in error && typeof error.detail === 'string') {
        // Existing callers pass Error.message or String(error) to t.backend.
        // Preserve stable fields through both routes without choosing a locale here.
        throw new Error(JSON.stringify(error));
      }
      throw error;
    }
  }
  return dispatchDemo(...call);
}

export const getSnapshot = () => dispatch('snapshot');
export const savePrompt = (prompt: Prompt) => dispatch('prompts.save', { prompt });
export const deletePrompt = (id: string) => dispatch('prompts.delete', { id });
export const exportPrompts = (format: 'json' | 'markdown') => dispatch('prompts.export', { format });
export const saveSettings = (settings: Settings) => dispatch('settings.save', { settings });
export const scan = () => dispatch('skills.scan');
export const checkUpdates = () => dispatch('updates.check', {});
export async function openExternal(url: string): Promise<void> {
  if (isTauriRuntime()) { await dispatch('links.open', { url }); }
  else { window.open(url, '_blank', 'noopener,noreferrer'); }
}
// The dialog filter label is the one piece of UI text this transport owns, so the
// caller supplies it translated; the default keeps callers that predate i18n working.
export const pickExecutable = async (filterName = '应用程序') => isTauriRuntime() ? openNativeFile({multiple:false,directory:false,filters:[{name:filterName,extensions:['exe','js','mjs','cjs']}]}) : null;
export const listBackups = () => dispatch('backups.list');
export const restoreBackup = (id: string) => dispatch('backups.restore', { id, confirmed: true });

export async function saveExportFile(content: string, filename: string): Promise<void> {
  if (!isTauriRuntime()) {
    downloadText(content, filename);
    return;
  }
  const selected = await saveNativeFile({
    defaultPath: filename,
    filters: [{ name: filename.endsWith('.md') ? 'Markdown' : 'JSON', extensions: [filename.endsWith('.md') ? 'md' : 'json'] }],
  });
  if (!selected) return;
  await writeTextFile(selected, content);
}

export async function copyText(text: string): Promise<void> {
  if (isTauriRuntime()) {
    // Clipboard failures are surfaced as errors; never silently switch to the demo transport.
    await writeNativeText(text);
    return;
  }
  if (!navigator.clipboard?.writeText) throw new Error('当前浏览器不支持剪贴板访问，请使用 HTTPS 或安全上下文');
  await navigator.clipboard.writeText(text);
}

const downloadText = (content: string, filename: string) => {
  const blob = new Blob([content], { type: 'text/plain;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  window.setTimeout(() => URL.revokeObjectURL(url), 0);
};
