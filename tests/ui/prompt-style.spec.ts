import { expect, test, type Locator, type Page, type TestInfo } from '@playwright/test';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

/// Prompt title/action layout is a rendered contract, so these tests measure
/// real geometry in Edge instead of asserting class names.
///
/// `prompt-cards.html` mounts the real PromptsPage with fictional long titles.
/// The shared preview fixture cannot be used for this: its `prompts.save`
/// response violates `Contract<{ prompt: Prompt }, Prompt>`, which crashes the
/// page as soon as a Prompt is created (reported as an out-of-scope finding).

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const sharedStyles = readFileSync(path.join(repoRoot, 'src', 'styles.css'), 'utf8');
const libraryStyles = readFileSync(path.join(repoRoot, 'src', 'skillLibrary.css'), 'utf8');

const TITLES = {
  chinese: '请把这段很长的中文标题写完整一些用于检查卡片标题在连续中文下的换行表现是否依然不会遮挡右侧按钮并且保持可读',
  english: 'Summarize the incident timeline, the follow-up actions, and every open question for each stakeholder in one reusable prompt',
  unbroken: 'AntiDisestablishmentarianismPneumonoultramicroscopicsilicovolcanoconiosisFloccinaucinihilipilificationPneumonoultramicroscopicsilicovolcanoconiosisFloccinaucinihilipilificationAntiDisestablishmentarianism',
  mixed: '混合 Mixed 标题 with supercalifragilisticexpialidociousunbreakabletoken 以及中文',
  short: '把问题说明白',
} as const;

type Rect = { left: number; right: number; top: number; bottom: number; width: number; height: number };

async function cardGeometry(card: Locator) {
  return card.evaluate((node) => {
    const rect = (target: Element): Rect => {
      const box = target.getBoundingClientRect();
      return { left: box.left, right: box.right, top: box.top, bottom: box.bottom, width: box.width, height: box.height };
    };
    const title = node.querySelector('h3');
    const copy = node.querySelector('.prompt-card__copy');
    const actions = node.querySelector('.prompt-card__actions');
    if (!title || !copy || !actions) throw new Error('Prompt card structure changed');
    return {
      card: rect(node),
      title: rect(title),
      copy: rect(copy),
      actions: rect(actions),
      titleText: (title.textContent || '').trim(),
      titleLines: title.getClientRects().length,
      buttons: [...actions.querySelectorAll('button')].map((button) => ({ label: button.getAttribute('aria-label') || '', box: rect(button) })),
      nestedButtons: node.querySelectorAll('button button').length,
      previewLabel: copy.getAttribute('aria-label') || '',
      // The preview button is a sibling of the action row, so no action can be
      // its descendant and a click on an action cannot trigger the preview.
      actionsInsidePreview: !!copy.querySelector('.prompt-card__actions'),
    };
  });
}

function expectInsideCard(card: Rect, box: Rect, what: string) {
  expect(box.left, what + ' starts inside the card').toBeGreaterThanOrEqual(card.left - 0.5);
  expect(box.right, what + ' ends inside the card').toBeLessThanOrEqual(card.right + 0.5);
}

async function measureCard(page: Page, id: string, screenshotName: string, testInfo: TestInfo) {
  const card = page.locator(`.prompt-card:has(h3:text-is(${JSON.stringify(TITLES[id as keyof typeof TITLES])}))`);
  await expect(card, id + ' card is present').toHaveCount(1);
  const geometry = await cardGeometry(card);
  const { card: cardBox, title, copy, actions } = geometry;

  expect(geometry.titleText, id + ' renders the full title').toBe(TITLES[id as keyof typeof TITLES]);
  expectInsideCard(cardBox, title, id + ' title');
  expectInsideCard(cardBox, actions, id + ' action row');
  expectInsideCard(cardBox, copy, id + ' preview button');
  // The header action row is its own flex row above the preview button, so a
  // long title can never sit beside the buttons or push them sideways: the two
  // boxes must not intersect.
  const boxesOverlap = title.left < actions.right && title.right > actions.left && title.top < actions.bottom && title.bottom > actions.top;
  expect(boxesOverlap, id + ' title and action row do not intersect').toBe(false);
  expect(title.left, id + ' title starts below the action row').toBeGreaterThanOrEqual(cardBox.left);
  expect(geometry.buttons, id + ' keeps favorite, edit, and delete').toHaveLength(3);
  for (const button of geometry.buttons) {
    expectInsideCard(cardBox, button.box, id + ' action ' + (button.label || 'button'));
    expect(button.box.width, id + ' action ' + (button.label || 'button') + ' keeps a hit area').toBeGreaterThanOrEqual(24);
    expect(button.box.height, id + ' action ' + (button.label || 'button') + ' keeps a hit area').toBeGreaterThanOrEqual(24);
  }
  expect(geometry.nestedButtons, id + ' has no nested buttons').toBe(0);
  expect(geometry.actionsInsidePreview, id + ' keeps actions out of the preview button').toBe(false);
  expect(geometry.previewLabel, id + ' preview label carries the full title').toBe('查看 ' + TITLES[id as keyof typeof TITLES]);

  await card.screenshot({ path: testInfo.outputPath(screenshotName) });
  return geometry;
}

/// Production rules are inserted from the real stylesheets instead of being
/// imported through Vite, so no assertion can be satisfied by a stale module.
/// Injection happens after navigation because `document.head` does not exist
/// yet while an init script runs.
async function applyProductionStyles(page: Page) {
  await page.addStyleTag({ content: sharedStyles });
  await page.addStyleTag({ content: libraryStyles });
}

test.beforeEach(async ({ page }) => {
  await page.goto('/tests/ui/prompt-cards.html', { waitUntil: 'load' });
  await expect(page.locator('.prompt-card')).toHaveCount(Object.keys(TITLES).length);
  await applyProductionStyles(page);
  // Guard against a silent style failure: the two-column grid is the layout the
  // rest of the measurements depend on.
  expect(await page.locator('.prompt-grid').evaluate((node) => getComputedStyle(node).gridTemplateColumns.split(' ').length)).toBe(2);
});

for (const [width, height] of [[1280, 860], [860, 640]]) {
  test(`Prompt card titles keep their buttons reachable at ${width}x${height}`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width, height });
    // A title with no break opportunity used to stay on one line wider than the
    // card and be clipped by the card's own overflow.
    for (const id of ['chinese', 'english', 'unbroken', 'mixed', 'short'] as const) {
      const geometry = await measureCard(page, id, `prompt-card-${width}-${id}.png`, testInfo);
      expect(geometry.title.width, id + ' title wraps inside the card').toBeLessThanOrEqual(geometry.card.width);
      expect(geometry.title.width, id + ' title keeps usable width').toBeGreaterThan(80);
      // An unbreakable token used to stay on one line and be clipped by the card.
      const overflow = await page.locator(`.prompt-card:has(h3:text-is(${JSON.stringify(TITLES[id])})) h3`).evaluate((node) => ({ scrollWidth: node.scrollWidth, clientWidth: node.clientWidth }));
      expect(overflow.scrollWidth, id + ' title text is not clipped').toBeLessThanOrEqual(overflow.clientWidth + 1);
    }

    const grid = await page.locator('.prompt-grid').evaluate((node) => ({ scrollWidth: node.scrollWidth, clientWidth: node.clientWidth }));
    expect(grid.scrollWidth, 'the grid does not scroll horizontally').toBeLessThanOrEqual(grid.clientWidth + 1);
    const cards = await page.locator('.prompt-card').evaluateAll((nodes) => nodes.map((node) => { const box = node.getBoundingClientRect(); return { left: box.left, right: box.right }; }));
    for (const card of cards) {
      expect(card.left, 'every card starts inside the viewport').toBeGreaterThanOrEqual(0);
      expect(card.right, 'every card ends inside the viewport').toBeLessThanOrEqual(width + 0.5);
    }
    const pageOverflow = await page.evaluate(() => ({ scrollWidth: document.documentElement.scrollWidth, clientWidth: document.documentElement.clientWidth }));
    expect(pageOverflow.scrollWidth, 'the page does not scroll horizontally').toBeLessThanOrEqual(pageOverflow.clientWidth);
  });
}

test('Prompt card actions are separate from the preview and keyboard reachable', async ({ page }) => {
  const card = page.locator('.prompt-card').filter({ hasText: 'AntiDisestablishmentarianism' });
  await expect(card).toHaveCount(1);

  // The action row is not inside the preview button, so toggling a favorite
  // cannot open the preview dialog.
  await card.locator('.prompt-card__actions button').first().click();
  await expect(page.getByRole('dialog')).toHaveCount(0);

  const preview = card.locator('.prompt-card__copy');
  await preview.focus();
  await expect(preview).toBeFocused();
  await page.keyboard.press('Enter');
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('heading', { name: TITLES.unbroken, exact: true })).toBeVisible();
  await dialog.locator('.modal__footer').getByRole('button', { name: '关闭', exact: true }).click();
  await expect(dialog).toBeHidden();
});

test('Skill check summary keeps status text and last-check time on one aligned line', async ({ page }) => {
  // The summary has no record in the preview fixture; the shared beforeEach
  // already applied the real stylesheets, so this measures the same markup slice
  // SkillPage composes.
  const measured = await page.evaluate(() => {
    const host = document.createElement('article');
    host.className = 'library-row';
    host.innerHTML = '<div class="library-row__main"><h3>写作助手 · 长文示例</h3><p class="form-help skill-check-summary"><span>尚未检查更新</span><span class="check-time">上次检查：2026-09-01 09:00</span></p></div>';
    document.body.append(host);
    const summary = host.querySelector('.skill-check-summary')!;
    const status = summary.querySelector('span:not(.check-time)')!;
    const time = summary.querySelector('.check-time')!;
    const read = (node: Element) => {
      const style = getComputedStyle(node);
      const box = node.getBoundingClientRect();
      return { fontFamily: style.fontFamily, fontSize: style.fontSize, fontWeight: style.fontWeight, lineHeight: style.lineHeight, color: style.color, fontVariantNumeric: style.fontVariantNumeric, top: box.top, left: box.left, right: box.right, bottom: box.bottom, height: box.height };
    };
    const statusBox = read(status);
    const timeBox = read(time);
    return { summary: read(summary), status: statusBox, time: timeBox, horizontalOverlap: statusBox.right > timeBox.left, statusColor: statusBox.color, timeColor: timeBox.color };
  });

  // Same type identity keeps the two runs on one line and looking like one
  // component instead of two different ones.
  expect(measured.time.fontFamily).toBe(measured.status.fontFamily);
  expect(measured.time.fontSize).toBe(measured.status.fontSize);
  expect(measured.time.fontWeight).toBe(measured.status.fontWeight);
  expect(measured.time.lineHeight).toBe(measured.status.lineHeight);
  expect(measured.time.fontSize).toBe(measured.summary.fontSize);
  // Digits stay tabular so repeated checks do not reflow the row.
  expect(measured.time.fontVariantNumeric).toBe('tabular-nums');
  expect(measured.horizontalOverlap).toBe(false);
  expect(Math.abs(measured.status.bottom - measured.time.bottom), 'runs share one baseline').toBeLessThanOrEqual(1);
  // A deliberately quieter timestamp is the one recorded difference.
  expect(measured.statusColor).not.toBe(measured.timeColor);
});
