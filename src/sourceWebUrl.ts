import type { Source } from './types';

/** A repository home link, never a filesystem path or an inferred branch URL. */
export function sourceWebUrl(source?: Source): string | undefined {
  if (source?.kind !== 'git') return;
  let locator = source.locator.trim();
  if (/^[\w.-]+\/[\w.-]+$/.test(locator)) locator = `https://github.com/${locator}`;
  try {
    const url = new URL(locator);
    if (!['https:', 'http:'].includes(url.protocol) || url.username || url.password || url.search || url.hash) return;
    if (url.hostname === 'github.com') {
      const parts = url.pathname.split('/').filter(Boolean);
      if (parts.length < 2) return;
      url.pathname = `/${parts[0]}/${parts[1].replace(/\.git$/, '')}`;
    } else {
      url.pathname = url.pathname.replace(/\.git\/?$/, '').replace(/\/$/, '');
    }
    return url.toString();
  } catch { return; }
}
