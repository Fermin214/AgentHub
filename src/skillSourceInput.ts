import { makeTranslate, type Translate } from './i18n';
import type { Source } from './types';

export type SkillSourceInput = { source: Source; skillNames: string[]; fromCommand: boolean };

// Parsing runs outside React, so the translator is passed in. It defaults to
// Chinese for the same reason the language context does: a caller that has not
// been wired up yet keeps its original wording instead of showing a key.
const zh = makeTranslate('zh');

function sourceValue(value: string, kind: Source['kind'], revision: string | undefined, t: Translate): Source {
  const shorthand = /^([\w.-]+)\/([\w.-]+)(?:\/(.+))?$/.exec(value);
  if (kind === 'git' && shorthand && !value.startsWith('.')) {
    const subpath = shorthand[3];
    if (subpath?.split('/').some(p => !p || p === '.' || p === '..' || /[\\:]/.test(p))) throw new Error(t('source.badSubpath'));
    return { kind, locator: `https://github.com/${shorthand[1]}/${shorthand[2]}`, ...(subpath ? { subpath } : {}), ...(revision ? { revision } : {}) };
  }
  return { kind, locator: value, ...(revision ? { revision } : {}) };
}

// Extract known Skills CLI source syntax; this never evaluates an installer command.
export function parseSkillSourceInput(input: string, kind: Source['kind'] = 'git', revision?: string, t: Translate = zh): SkillSourceInput {
  const value = input.trim();
  if (!/^npx(?:\s|$)/i.test(value)) {
    if (/^(?:npm|git\s+clone|curl|wget|powershell|pwsh|bash|cmd)\s/i.test(value) || /[\r\n]/.test(value)) {
      throw new Error(t('source.pasteAddress'));
    }
    return { source: sourceValue(value, kind, revision, t), skillNames: [], fromCommand: false };
  }
  if (/[\r\n;&|`$<>]/.test(value)) throw new Error(t('source.singleCommand'));
  const tokens: string[] = [];
  let rest = value;
  while (rest.trim()) {
    rest = rest.trimStart();
    const token = /^(?:"([^"\r\n]*)"|'([^'\r\n]*)'|([^\s"']+))(?=\s|$)/.exec(rest);
    if (!token) throw new Error(t('source.unbalancedQuotes'));
    tokens.push(token[1] ?? token[2] ?? token[3]);
    rest = rest.slice(token[0].length);
  }
  let cursor = 1;
  while (['-y', '--yes'].includes(tokens[cursor])) cursor++;
  if (!/^skills(?:@[\w.+-]+)?$/.test(tokens[cursor] || '') || tokens[cursor + 1] !== 'add') {
    throw new Error(t('source.onlySkillsAdd'));
  }
  const locator = tokens[cursor + 2];
  if (!locator || locator.startsWith('-') || !/^(?:https:\/\/\S+|[\w.-]+\/[\w./-]+|[A-Za-z]:[\\/]\S+|\.{0,2}\/\S+)$/.test(locator)) {
    throw new Error(t('source.needsLocator'));
  }
  const skillNames: string[] = [];
  for (cursor += 3; cursor < tokens.length; cursor++) {
    const flag = tokens[cursor];
    if (['-g', '--global', '-y', '--yes', '--copy'].includes(flag)) continue;
    if (['--skill', '-s', '--agent', '-a'].includes(flag)) {
      const values: string[] = [];
      while (cursor + 1 < tokens.length && !tokens[cursor + 1].startsWith('-')) values.push(tokens[++cursor]);
      if (!values.length) throw new Error(t('source.flagNeedsValue', { flag }));
      if (flag === '--skill' || flag === '-s') skillNames.push(...values);
      continue;
    }
    throw new Error(t('source.unsupportedFlag', { flag }));
  }
  const sourceKind = /^(?:[A-Za-z]:[\\/]|\.{0,2}\/)/.test(locator) ? 'local' : /\.zip(?:[?#]|$)/i.test(locator) ? 'zip' : 'git';
  return { source: sourceValue(locator, sourceKind, revision, t), skillNames, fromCommand: true };
}

export function selectCommandSkills<T extends { name: string; subpath: string }>(candidates: T[], names: string[], t: Translate = zh): T[] {
  if (!names.length || names.includes('*')) return candidates;
  const selected = candidates.filter(c => names.some(n => n.toLowerCase() === c.name.toLowerCase() || n === c.subpath));
  const missing = names.filter(n => !selected.some(c => n.toLowerCase() === c.name.toLowerCase() || n === c.subpath));
  if (missing.length) throw new Error(t('source.missingNamed', { names: missing.join(t.lang === 'zh' ? '、' : ', ') }));
  return selected;
}
