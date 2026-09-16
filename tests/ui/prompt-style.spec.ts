import { expect, test, type Locator, type Page, type TestInfo } from '@playwright/test';

/// Prompt card title/action alignment and the Skill check summary typography are
/// rendered contracts, so these tests measure real geometry in Edge on the real
/// App shell (`tests/ui/prompt-cards.html`) instead of asserting class names.
///
/// The shared preview fixture is not used: its `prompts.save` response violates
/// `Contract<{ prompt: Prompt }, Prompt>` and crashes the page as soon as a
/// Prompt is created. That fixture finding belongs to the Skill/shared-fixture
/// owner; this harness only supplies fictional data through the simulated IPC
/// transport and never touches `tests/ui/fixture.js`.

const TITLES = {
  chinese: '请把这段很长的中文标题写完整一些用于检查卡片标题在连续中文下的换行表现是否依然不会挤压右侧按钮并且保持可读',
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
    const heading = node.querySelector('h3');
    const copy = node.querySelector('.prompt-card__copy');
    const actions = node.querySelector('.prompt-card__actions');
    if (!heading || !copy || !actions) throw new Error('Prompt card structure changed');
    const titleButton = heading.closest('button');
    const firstLine = heading.getClientRects()[0];
    const headingStyle = getComputedStyle(heading);
    const lineHeight = parseFloat(headingStyle.lineHeight);
    return {
      card: rect(node),
      titleButton: titleButton ? rect(titleButton) : null,
      title: rect(heading),
      // The vertical band of the first wrapped line of the title. Getting this
      // from the heading itself keeps the alignment check independent of where
      // the preview control's wrapper sits.
      firstLine: { left: firstLine.left, right: firstLine.right, top: firstLine.top, bottom: firstLine.bottom, height: firstLine.height },
      titleFontSize: parseFloat(headingStyle.fontSize),
      lineHeight,
      copy: rect(copy),
      actions: rect(actions),
      titleText: (heading.textContent || '').trim(),
      buttons: [...actions.querySelectorAll('button')].map((button) => ({ label: button.getAttribute('aria-label') || '', box: rect(button) })),
      nestedButtons: node.querySelectorAll('button button').length,
      previewLabel: copy.getAttribute('aria-label') || '',
    };
  });
}

function expectInside(card: Rect, box: Rect, what: string, axis: 'horizontal' | 'both' = 'horizontal') {
  expect(box.left, what + ' starts inside the card').toBeGreaterThanOrEqual(card.left - 0.5);
  expect(box.right, what + ' ends inside the card').toBeLessThanOrEqual(card.right + 0.5);
  if (axis === 'both') {
    expect(box.top, what + ' starts inside the card').toBeGreaterThanOrEqual(card.top - 0.5);
    expect(box.bottom, what + ' ends inside the card').toBeLessThanOrEqual(card.bottom + 0.5);
  }
}

function overlaps(a: Rect, b: Rect) {
  return a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top;
}

/// The preview label is localized ("查看 <title>" / "View <title>"); it lives on
/// the body/preview control and must carry the whole title.
function expectPreviewLabel(label: string, title: string, id: string) {
  expect(label, id + ' preview label carries the full title').toMatch(new RegExp(`^(查看|View) ${title.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}$`));
}

/// A wrapped title must grow downward, exactly one line box per wrapped line.
async function expectTitleLineBoxes(card: Locator, id: string) {
  const measured = await card.evaluate((node) => {
    const heading = node.querySelector('h3');
    if (!heading) throw new Error('Prompt card structure changed');
    // Measure the real line box height by laying the same style out with a
    // single character, so no `line-height` value has to be assumed here.
    const probe = heading.cloneNode(false) as HTMLElement;
    probe.textContent = 'x';
    probe.style.cssText += ';position:absolute;visibility:hidden;white-space:nowrap;width:auto;';
    heading.parentElement?.append(probe);
    const lineHeight = probe.getBoundingClientRect().height;
    probe.remove();
    const height = heading.getBoundingClientRect().height;
    return { height, lines: Math.max(1, Math.round(height / lineHeight)), lineHeight };
  });
  expect(measured.lines, id + ' title wraps onto at least one line').toBeGreaterThanOrEqual(1);
  // The heading box covers its whole wrapped content, never a clipped single line.
  expect(measured.height, id + ' heading box covers every wrapped line').toBeGreaterThanOrEqual(measured.lines * measured.lineHeight - 1);
  return measured;
}

/// The core alignment contract: the title and the favorite/edit/delete actions
/// live on one row. The BASE layout put the actions in their own header row and
/// the title below it, which fails every assertion here.
async function expectSameRowAlignment(card: Locator, id: string) {
  const geometry = await cardGeometry(card);
  const { card: cardBox, firstLine, actions } = geometry;

  expectInside(cardBox, firstLine, id + ' title first line');
  expectInside(cardBox, actions, id + ' action row');
  expect(overlaps(firstLine, actions), id + ' title first line and action row must not intersect').toBe(false);
  // Same row: the title's first line and the action row share a vertical band,
  // and the title stays entirely left of the buttons.
  expect(
    firstLine.bottom > actions.top + 1 && firstLine.top < actions.bottom - 1,
    id + ' title first line is on the same row as the action row',
  ).toBe(true);
  expect(firstLine.right, id + ' title first line stays left of the buttons').toBeLessThanOrEqual(actions.left + 0.5);
  expect(firstLine.height, id + ' first line height is a real text line').toBeGreaterThanOrEqual(geometry.titleFontSize - 1);
  await expectTitleLineBoxes(card, id);
  // Right alignment: the action row is flush with the card content edge.
  expect(actions.right, id + ' action row sits at the right content edge').toBeGreaterThan(cardBox.right - 40);
  expect(geometry.title.right, id + ' title box keeps a gap from the buttons').toBeLessThanOrEqual(actions.left + 0.5);

  expect(geometry.titleText, id + ' renders the full title').toBe(TITLES[id as keyof typeof TITLES]);
  expect(geometry.buttons, id + ' keeps favorite, edit, and delete').toHaveLength(3);
  for (const button of geometry.buttons) {
    expectInside(cardBox, button.box, id + ' action ' + (button.label || 'button'), 'both');
    expect(button.box.width, id + ' action ' + (button.label || 'button') + ' keeps a hit area').toBeGreaterThanOrEqual(24);
    expect(button.box.height, id + ' action ' + (button.label || 'button') + ' keeps a hit area').toBeGreaterThanOrEqual(24);
  }
  expect(geometry.nestedButtons, id + ' has no nested buttons').toBe(0);
  expectPreviewLabel(geometry.previewLabel, TITLES[id as keyof typeof TITLES], id);
  // The heading itself is a button, so the title stays clickable for preview.
  expect(geometry.titleButton, id + ' title is an interactive preview control').not.toBeNull();
  return geometry;
}

async function openHarness(page: Page, lang: 'zh' | 'en', view: 'prompts' | 'skills' = 'prompts') {
  await page.goto(`/tests/ui/prompt-cards.html?lang=${lang}&view=${view}`, { waitUntil: 'load' });
  await expect(page.locator('.app-shell')).toBeVisible();
  if (view === 'skills') {
    // Open the Skill page through the real sidebar navigation, the same way a
    // user reaches it in the desktop app.
    await page.locator('.nav-item').filter({ hasText: lang === 'zh' ? '内容与安装位置' : 'Content and install locations' }).click();
    await expect(page.locator('.skill-check-summary').first()).toBeVisible();
  } else {
    await expect(page.locator('.prompt-card')).toHaveCount(Object.keys(TITLES).length);
  }
}

for (const lang of ['zh', 'en'] as const) {
  for (const [width, height] of [[1280, 860], [860, 640]]) {
    test(`Prompt card titles align with their buttons in the real App at ${width}x${height} (${lang})`, async ({ page }, testInfo) => {
      await openHarness(page, lang);
      await page.setViewportSize({ width, height });

      for (const id of ['chinese', 'english', 'unbroken', 'mixed', 'short'] as const) {
        const card = page.locator(`.prompt-card:has(h3:text-is(${JSON.stringify(TITLES[id])}))`);
        await expect(card, `${id} card is present`).toHaveCount(1);
        await card.scrollIntoViewIfNeeded();
        const geometry = await expectSameRowAlignment(card, id);
        await card.screenshot({ path: testInfo.outputPath(`prompt-card-${lang}-${width}-${id}.png`) });
        // The title still wraps inside the card instead of being clipped.
        const overflow = await card.locator('h3').evaluate((node) => ({ scrollWidth: node.scrollWidth, clientWidth: node.clientWidth }));
        expect(overflow.scrollWidth, id + ' title text is not clipped').toBeLessThanOrEqual(overflow.clientWidth + 1);
        expect(overflow.clientWidth, id + ' title keeps usable width').toBeGreaterThan(60);
      }

      const grid = await page.locator('.prompt-grid').evaluate((node) => ({ scrollWidth: node.scrollWidth, clientWidth: node.clientWidth }));
      expect(grid.scrollWidth, 'the grid does not scroll horizontally').toBeLessThanOrEqual(grid.clientWidth + 1);
      // The two-column grid must use the real content area: each card is about
      // half of `.page-wrap` minus the grid gap, so the measurement is not taken
      // in a narrower synthetic layout.
      const contentWidth = await page.locator('.page-wrap').evaluate((node) => node.getBoundingClientRect().width);
      const cardWidth = await page.locator('.prompt-card').first().evaluate((node) => node.getBoundingClientRect().width);
      expect(cardWidth, 'the card uses the real content area width').toBeGreaterThan(contentWidth * 0.4);
      expect(cardWidth, 'the card is not wider than the content area').toBeLessThanOrEqual(contentWidth);

      await page.screenshot({ path: testInfo.outputPath(`prompt-grid-${lang}-${width}x${height}.png`), fullPage: true });
    });
  }
}

test('Prompt card actions and title stay separate and keyboard reachable', async ({ page }) => {
  await openHarness(page, 'zh');
  const card = page.locator('.prompt-card').filter({ hasText: 'AntiDisestablishmentarianism' });
  await expect(card).toHaveCount(1);

  // The action row is a sibling of the title button, so toggling a favorite can
  // never open the preview dialog.
  await card.locator('.prompt-card__actions button').first().click();
  await expect(page.getByRole('dialog')).toHaveCount(0);

  const titleButton = card.locator('.prompt-card__title');
  await titleButton.focus();
  await expect(titleButton).toBeFocused();
  await page.keyboard.press('Enter');
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('heading', { name: TITLES.unbroken, exact: true })).toBeVisible();
  await dialog.locator('.modal__footer').getByRole('button', { name: '关闭', exact: true }).click();
  await expect(dialog).toBeHidden();

  // Edit and delete stay wired to their own dialogs.
  await card.locator('.prompt-card__actions button').nth(1).click();
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: '取消', exact: true }).click();
  await expect(dialog).toBeHidden();
  await card.locator('.prompt-card__actions button').nth(2).click();
  await expect(dialog).toBeVisible();
  await dialog.locator('.modal__footer').getByRole('button', { name: '取消', exact: true }).click();
  await expect(dialog).toBeHidden();

  // The body preview stays its own control.
  const copy = card.locator('.prompt-card__copy');
  await expect(copy).toBeVisible();
  await copy.click();
  await expect(dialog).toBeVisible();
  await dialog.locator('.modal__footer').getByRole('button', { name: '关闭', exact: true }).click();
});

test('Skill check summary typography on the real Skill page', async ({ page }) => {
  // The summary needs a check record, so the harness snapshot carries a fictional
  // `checkedAt`. The measurement runs against the real SkillPage rendering, not a
  // copied markup slice.
  await openHarness(page, 'zh', 'skills');
  const measured = await page.locator('.skill-check-summary').first().evaluate((summary) => {
    const status = summary.querySelector('span:not(.check-time)');
    const time = summary.querySelector('.check-time');
    if (!status || !time) throw new Error('Skill check summary structure changed');
    const read = (node: Element) => {
      const style = getComputedStyle(node);
      const box = node.getBoundingClientRect();
      return { text: (node.textContent || '').trim(), fontFamily: style.fontFamily, fontSize: style.fontSize, fontWeight: style.fontWeight, lineHeight: style.lineHeight, color: style.color, fontVariantNumeric: style.fontVariantNumeric, left: box.left, right: box.right, top: box.top, bottom: box.bottom };
    };
    const summaryBox = summary.getBoundingClientRect();
    return { summary: { ...read(summary), width: summaryBox.width }, status: read(status), time: read(time) };
  });

  // Both runs come from the same component and must share one type identity.
  expect(measured.time.text, 'the last-check run renders from a real check record').toContain('2026');
  expect(measured.time.fontFamily).toBe(measured.status.fontFamily);
  expect(measured.time.fontSize).toBe(measured.status.fontSize);
  expect(measured.time.fontWeight).toBe(measured.status.fontWeight);
  expect(measured.time.lineHeight).toBe(measured.status.lineHeight);
  expect(measured.time.fontSize).toBe(measured.summary.fontSize);
  expect(measured.time.fontVariantNumeric).toBe('tabular-nums');
  expect(measured.status.right, 'the two runs do not overlap').toBeLessThanOrEqual(measured.time.left + 0.5);
  expect(Math.abs(measured.status.bottom - measured.time.bottom), 'runs share one baseline').toBeLessThanOrEqual(1);
  // The quieter timestamp colour is the recorded, deliberate difference.
  expect(measured.status.color).not.toBe(measured.time.color);
});

test('Skill check summary stays consistent in the English interface', async ({ page }) => {
  await openHarness(page, 'en', 'skills');
  const measured = await page.locator('.skill-check-summary').first().evaluate((summary) => {
    const status = summary.querySelector('span:not(.check-time)');
    const time = summary.querySelector('.check-time');
    if (!status || !time) throw new Error('Skill check summary structure changed');
    const read = (node: Element) => ({ text: (node.textContent || '').trim(), fontSize: getComputedStyle(node).fontSize, lineHeight: getComputedStyle(node).lineHeight, color: getComputedStyle(node).color });
    return { status: read(status), time: read(time) };
  });
  expect(measured.time.text).toContain('2026');
  expect(measured.time.fontSize).toBe(measured.status.fontSize);
  expect(measured.time.lineHeight).toBe(measured.status.lineHeight);
});
