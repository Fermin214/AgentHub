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
  await expect(saving).toHaveCSS('cursor', 'pointer');
  await expect(saving).toHaveCSS('opacity', '1');
  expect(await page.locator('button[data-favorite-pending="true"]').count()).toBe(1);

  // B-1: no other control may be disabled or dimmed by a favorite save.
  await expect(page.getByRole('button', { name: '取消收藏 写作助手 · 长文示例', exact: true })).toBeEnabled();
  await expect(page.getByRole('button', { name: 'Skill 仓库', exact: true })).toBeEnabled();
  await expect(page.getByRole('button', { name: '添加 Skill', exact: true })).toBeEnabled();
  expect(await page.getByRole('button', { name: '从库删除', exact: true }).evaluateAll(nodes => nodes.every(node => !(node as HTMLButtonElement).disabled))).toBe(true);
  const icons = page.locator('.skill-sync__agent');
  expect(await icons.count()).toBeGreaterThan(0);
  expect(await icons.evaluateAll(nodes => nodes.every(node => !(node as HTMLButtonElement).disabled))).toBe(true);
  expect(await icons.evaluateAll(nodes => nodes.every(node => getComputedStyle(node).opacity === '1'))).toBe(true);
  expect(await page.getByRole('button', { name: '取消收藏 写作助手 · 长文示例', exact: true }).evaluate(node => getComputedStyle(node).opacity)).toBe('1');
  expect(await page.getByRole('button', { name: '从库删除', exact: true }).first().evaluate(node => getComputedStyle(node).opacity)).toBe('1');

  // Releasing the parked save commits it, so the row reports the saved favorite
  // instead of falling back to the previous value.
  await page.evaluate(() => (window as unknown as { releaseFavorite: () => void }).releaseFavorite());
  await expect(page.getByRole('button', { name: '取消收藏 本机 Skill', exact: true })).toBeEnabled();
  expect(await page.locator('button[data-favorite-pending="true"]').count()).toBe(0);
});

for (const lang of ['zh', 'en'] as const) {
  for (const [width, height] of [[1280, 860], [860, 640]]) {
    test(`Skill actions remain on one line at ${width}x${height} (${lang})`, async ({ page }, testInfo) => {
      await page.setViewportSize({ width, height });
      if (lang === 'en') {
        await page.getByRole('button', { name: '设置 Agent 与本地偏好', exact: true }).click();
        await page.getByRole('tab', { name: '关于', exact: true }).click();
        await page.getByRole('combobox', { name: '界面语言', exact: true }).selectOption('en');
        await page.getByRole('button', { name: 'Skills Content and install locations', exact: true }).click();
      }
      const rows = page.locator('.skill-row');
      expect(await rows.count()).toBeGreaterThan(0);
      for (const row of await rows.all()) {
        const geometry = await row.evaluate(node => {
          const actions = node.querySelector('.library-actions')!;
          const buttons = [...actions.querySelectorAll('button')].map(button => button.getBoundingClientRect());
          const rowBox = node.getBoundingClientRect();
          return { centers: buttons.map(box => box.top + box.height / 2), left: buttons[0].left, right: buttons.at(-1)!.right, rowLeft: rowBox.left, rowRight: rowBox.right };
        });
        expect(Math.max(...geometry.centers) - Math.min(...geometry.centers)).toBeLessThanOrEqual(1);
        expect(geometry.left).toBeGreaterThanOrEqual(geometry.rowLeft);
        expect(geometry.right).toBeLessThanOrEqual(geometry.rowRight + 1);
      }
      await page.screenshot({ path: testInfo.outputPath(`skill-actions-${lang}-${width}.png`), fullPage: true });
    });
  }
}

test('a rejected favorite save reports the error and permits a retry', async ({ page }) => {
  await page.evaluate(() => { (window as unknown as { favoriteRequest: string }).favoriteRequest = 'reject'; });
  await page.getByRole('button', { name: '收藏 本机 Skill', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('磁盘不可写');
  await expect(page.getByRole('button', { name: '收藏 本机 Skill', exact: true })).toBeEnabled();

  await page.getByRole('button', { name: '收藏 本机 Skill', exact: true }).click();
  await expect(page.getByRole('button', { name: '取消收藏 本机 Skill', exact: true })).toBeEnabled();
});

test('the detail viewer shows library files with no read-location entry', async ({ page }) => {
  await page.getByRole('button', { name: '写作助手 · 长文示例', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByRole('heading', { name: '文件与说明', exact: true })).toBeVisible();
  await expect(dialog.getByRole('navigation', { name: 'Skill 文件', exact: true })).toBeVisible();
  const content = dialog.getByRole('region', { name: '文件内容', exact: true });
  await expect(content).toBeVisible();
  // The removed control must be gone even though this Skill has an Agent install.
  await expect(dialog.getByText('阅读位置', { exact: true })).toHaveCount(0);
  await expect(dialog.locator('select')).toHaveCount(0);
  await expect(dialog.getByText('C:/AcceptanceFixture/agents/codex/skills/long', { exact: true })).toHaveCount(0);
  // Wait for rendered content instead of a fixed delay before capturing evidence.
  await expect(content.getByText('这是独立预览中的示例正文', { exact: false }).first()).toBeVisible();
  await dialog.screenshot({ path: 'output/ui-skill-interactions/skill-detail-library-only.png' });
});

test('the English interface reads the library copy while an installed copy exists', async ({ page }) => {
  // The fixture answers the library and an install with different text, so the
  // rendered body proves which copy was read. This Skill has a real install path.
  await page.evaluate(() => {
    const internals = (window as unknown as { __TAURI_INTERNALS__: { invoke: (c: string, a: unknown) => Promise<unknown> } }).__TAURI_INTERNALS__;
    const original = internals.invoke;
    (window as unknown as { skillIpc: string[] }).skillIpc = [];
    internals.invoke = async (command, args) => {
      const method = (args as { method?: string })?.method;
      const payload = (args as { args?: Record<string, unknown> })?.args;
      if (method === 'skills.read' || method === 'skills.files') (window as unknown as { skillIpc: string[] }).skillIpc.push(JSON.stringify({ method, args: payload }));
      return await original(command, args);
    };
  });
  await page.getByRole('button', { name: '设置 Agent 与本地偏好', exact: true }).click();
  await page.getByRole('tab', { name: '关于', exact: true }).click();
  await page.getByRole('combobox', { name: '界面语言', exact: true }).selectOption('en');
  await page.getByRole('button', { name: 'Skills Content and install locations', exact: true }).click();
  await page.getByRole('button', { name: '写作助手 · 长文示例', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByRole('heading', { name: 'Files', exact: true })).toBeVisible();
  await expect(dialog.getByRole('navigation', { name: 'Skill files', exact: true })).toBeVisible();
  await expect(dialog.getByText('Read from', { exact: true })).toHaveCount(0);
  await expect(dialog.locator('select')).toHaveCount(0);
  const content = dialog.getByRole('region', { name: 'File contents', exact: true });
  await expect(content.getByText('库副本正文', { exact: false })).toBeVisible();
  await expect(content.getByText('安装副本正文', { exact: false })).toHaveCount(0);
  // Every request must be the library read: no install id anywhere in the payload.
  const ipc = await page.evaluate(() => (window as unknown as { skillIpc: string[] }).skillIpc);
  expect(ipc.length).toBeGreaterThan(0);
  expect(ipc.some(entry => entry.includes('"method":"skills.read"'))).toBe(true);
  expect(ipc.join(' ')).not.toContain('deploymentId');
  await dialog.screenshot({ path: 'output/ui-skill-interactions/skill-detail-english-library-only.png' });
});

test('a saved Prompt stays in the list under the prompts.save contract', async ({ page }) => {
  await page.getByRole('button', { name: 'Prompts 收藏与复用', exact: true }).click();
  await page.getByRole('button', { name: '添加', exact: true }).click();
  await page.getByLabel('正文', { exact: true }).fill('新建的预览正文');
  await page.getByRole('button', { name: '保存 Prompt', exact: true }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByRole('button', { name: '查看 新建的预览正文', exact: true })).toBeVisible();
});
