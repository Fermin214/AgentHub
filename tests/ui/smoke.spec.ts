import { expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.goto('/tests/ui/index.html', { waitUntil: 'domcontentloaded' });
  await expect(page.getByRole('button', { name: 'Prompts 收藏与复用', exact: true })).toBeVisible();
});

test('Prompt tags are plain text pills', async ({ page }) => {
  const tags = page.locator('.tag-list span');
  await expect(tags).toHaveCount(5);
  expect(await tags.allTextContents()).toEqual(['分析', '表达', '常用', '编辑', '长标签用于检查换行与留白']);
  expect(await tags.evaluateAll(nodes => nodes.every(node => parseFloat(getComputedStyle(node).borderRadius) > 0))).toBe(true);
});

test('Skill icons load and source link belongs to the viewer heading', async ({ page }) => {
  await page.getByRole('button', { name: 'Skill 内容与安装位置', exact: true }).click();
  const icons = page.locator('button[aria-label*="写作助手 · 长文示例"] .agent-brand-icon');
  await expect(icons).toHaveCount(5);
  await expect.poll(() => icons.evaluateAll(nodes => nodes.length > 0 && nodes.every(node => (node as HTMLImageElement).complete && (node as HTMLImageElement).naturalWidth > 0))).toBe(true);
  expect(await icons.evaluateAll(nodes => new Set(nodes.map(node => (node as HTMLImageElement).src)).size)).toBe(5);
  expect(await icons.evaluateAll(nodes => nodes.every(node => getComputedStyle(node).filter === 'none'))).toBe(true);
  await page.getByRole('button', { name: '写作助手 · 长文示例', exact: true }).click();
  const link = page.getByRole('link', { name: '打开来源仓库 ↗' });
  await expect(link).toHaveAttribute('href', 'https://example.invalid/fixture/skills');
  expect(await link.evaluate(node => !!node.closest('header'))).toBe(true);
  // Placement and muted styling are contracts; aesthetic judgment remains manual.
  expect(await link.evaluate(node => getComputedStyle(node).color)).toBe('rgb(100, 116, 139)');
  await page.getByRole('button', { name: '关闭', exact: true }).click();
  await page.getByRole('button', { name: '本机 Skill', exact: true }).click();
  await expect(link).toHaveCount(0);
});

for (const width of [1280, 860]) {
  test(`Viewer independent keyboard scrolling at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: width === 860 ? 640 : 860 });
    await page.getByRole('button', { name: 'Skill 内容与安装位置', exact: true }).click();
    await page.getByRole('button', { name: '写作助手 · 长文示例', exact: true }).click();
    const content = page.getByRole('region', { name: '文件内容', exact: true });
    const files = page.getByRole('navigation', { name: 'Skill 文件', exact: true });
    await content.focus();
    await content.press('PageDown');
    await expect.poll(() => content.evaluate(node => node.scrollTop)).toBeGreaterThan(0);
    expect(await files.evaluate(node => node.scrollTop)).toBe(0);
    await content.press('Control+Home');
    await expect.poll(() => content.evaluate(node => node.scrollTop)).toBe(0);
    for (const target of [content, files, page.getByRole('button', { name: '关闭', exact: true })]) {
      expect(await target.evaluate(node => { const r = node.getBoundingClientRect(); return r.width > 0 && r.height > 0 && r.left >= 0 && r.right <= innerWidth && r.top >= 0 && r.bottom <= innerHeight; })).toBe(true);
    }
  });
}

test('Source cancellation releases the pending request and permits retry', async ({ page }) => {
  await page.getByRole('button', { name: 'Skill 内容与安装位置', exact: true }).click();
  await page.getByRole('button', { name: '添加 Skill', exact: true }).click();
  await page.getByRole('textbox', { name: 'Skill 来源', exact: true }).fill('fixture/skills');
  await page.getByRole('button', { name: '查找 Skill', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: '获取仓库' })).toBeVisible();
  await page.getByRole('button', { name: '取消', exact: true }).click();
  await expect(page.getByRole('alert').filter({ hasText: '来源查找已取消' })).toBeVisible();
  await page.evaluate(() => { (window as unknown as { fixtureScenario: string }).fixtureScenario = 'success'; });
  await page.getByRole('button', { name: '查找 Skill', exact: true }).click();
  await expect(page.getByText('示例 Skill', { exact: true })).toBeVisible();
});

test('Bookmark notes do not navigate or open a browser', async ({ page, context }) => {
  await page.getByRole('button', { name: '项目 本机目录与仓库收藏', exact: true }).click();
  await page.getByRole('tab', { name: '仓库收藏', exact: true }).click();
  const notes = page.getByText('可选中这段备注；点击卡片空白处不会打开网页。', { exact: true });
  await expect(notes).toBeVisible();
  const title = page.getByRole('link', { name: 'AgentHub', exact: true });
  await expect(title).toBeVisible();
  const url = page.url();
  const count = context.pages().length;
  await notes.click();
  await expect(notes).toBeVisible();
  expect(page.url()).toBe(url);
  expect(context.pages()).toHaveLength(count);
  expect(await title.evaluate(node => ['none', 'normal'].includes(getComputedStyle(node, '::after').content))).toBe(true);
});
