import { expect, it } from 'vitest';
import { sourceWebUrl } from './sourceWebUrl';
it('opens repository homes without guessing revisions and rejects non-web sources', () => {
  expect(sourceWebUrl({ kind: 'git', locator: 'owner/repo' })).toBe('https://github.com/owner/repo');
  expect(sourceWebUrl({ kind: 'git', locator: 'https://github.com/owner/repo/tree/main/skills' })).toBe('https://github.com/owner/repo');
  expect(sourceWebUrl({ kind: 'git', locator: 'https://gitlab.com/group/team/repo.git' })).toBe('https://gitlab.com/group/team/repo');
  for (const locator of ['C:\\test', 'file:///test', 'javascript:alert(1)', 'https://user:secret@example.com/repo', 'https://example.com/repo?token=secret']) expect(sourceWebUrl({ kind: 'git', locator })).toBeUndefined();
  expect(sourceWebUrl({ kind: 'local', locator: 'https://example.com' })).toBeUndefined();
});
