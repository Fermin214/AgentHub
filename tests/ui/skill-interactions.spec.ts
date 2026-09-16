import { expect, test } from '@playwright/test';

// Controlled transport only: the fixture in tests/ui/fixture.js answers the
// favorite save, so no real file, Agent, or network write happens here.
test.beforeEach(async ({ page }) => {
  await page.goto('/tests/ui/index.html', { waitUntil: 'domcontentloaded' });
  await page.getByRole('button', { name: 'Skill 内容与安装位置', exact: true }).click();
  await expect(page.getByRole('button', { name: '收藏 本机 Skill', exact: true })).toBeVisible();
});

test('a pending favorite save keeps other Skills, Agent icons, and top actions unchanged', async ({ page }) => {
  await page.evaluate(() => { (window as unknown as { favoriteRequest: string }).favoriteRequest = 'pending'; });
  await page.getByRole('button', { name: '收藏 本机 Skill', exact: true }).click();
  const saving = page.getByRole('button', { name: '正在保存 本机 Skill 的收藏…', exact: true });
  await expect(saving).toBeVisible();
  await expect(saving).toBeDisabled();
  expect(await page.locator('button[data-favorite-pending="true"]').count()).toBe(1);

  // Other Skills remain favorite-able; only the clicked row shows a pending save.
  await expect(page.getByRole('button', { name: '取消收藏 写作助手 · 长文示例', exact: true })).toBeEnabled();
  // Agent icons keep their normal appearance; they must not dim or become disabled
  // because an unrelated favorite is saving.
  const icons = page.locator('.skill-sync__agent');
  expect(await icons.count()).toBeGreaterThan(0);
  expect(await icons.evaluateAll(nodes => nodes.every(node => !(node as HTMLButtonElement).disabled))).toBe(true);
  expect(await icons.evaluateAll(nodes => nodes.every(node => getComputedStyle(node).opacity === '1'))).toBe(true);
  // The idle favorite control of the other Skill is not dimmed either.
  expect(await page.getByRole('button', { name: '取消收藏 写作助手 · 长文示例', exact: true }).evaluate(node => getComputedStyle(node).opacity)).toBe('1');

  // Releasing the parked save clears the pending state without a stale result.
  await page.evaluate(() => (window as unknown as { releaseFavorite: (value: unknown) => void }).releaseFavorite({ skills: [] }));
  await expect(page.getByRole('button', { name: '收藏 本机 Skill', exact: true })).toBeEnabled();
  expect(await page.locator('button[data-favorite-pending="true"]').count()).toBe(0);
});

test('the detail viewer shows library files with no read-location entry', async ({ page }) => {
  await page.getByRole('button', { name: '写作助手 · 长文示例', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByRole('heading', { name: '文件与说明', exact: true })).toBeVisible();
  await expect(dialog.getByRole('navigation', { name: 'Skill 文件', exact: true })).toBeVisible();
  await expect(dialog.getByRole('region', { name: '文件内容', exact: true })).toBeVisible();
  // The removed control must be gone even though this Skill has an Agent install.
  await expect(dialog.getByText('阅读位置', { exact: true })).toHaveCount(0);
  await expect(dialog.locator('select')).toHaveCount(0);
  await expect(dialog.getByText('C:/AcceptanceFixture/agents/codex/skills/long', { exact: true })).toHaveCount(0);
  // Let the dialog's entrance animation settle so the capture is deterministic.
  await expect.poll(() => dialog.evaluate(node => getComputedStyle(node).opacity)).toBe('1');
  await page.waitForTimeout(400);
  await dialog.screenshot({ path: 'output/ui-skill-interactions/skill-detail-library-only.png' });
});

test('a rejected favorite save reports the error and permits a retry', async ({ page }) => {
  await page.evaluate(() => { (window as unknown as { favoriteRequest: string }).favoriteRequest = 'reject'; });
  await page.getByRole('button', { name: '收藏 本机 Skill', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('磁盘不可写');
  await expect(page.getByRole('button', { name: '收藏 本机 Skill', exact: true })).toBeEnabled();

  await page.evaluate(() => { (window as unknown as { favoriteRequest: string }).favoriteRequest = 'ok'; });
  await page.getByRole('button', { name: '收藏 本机 Skill', exact: true }).click();
  await expect(page.getByRole('button', { name: '取消收藏 本机 Skill', exact: true })).toBeEnabled();
});
