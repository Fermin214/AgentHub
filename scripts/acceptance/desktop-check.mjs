// Only connects to the isolated, PID-verified endpoint launched by desktop.ps1.
import { chromium } from '@playwright/test';
import fs from 'node:fs/promises';
import path from 'node:path';
import assert from 'node:assert/strict';
import { layout } from './desktop-layout.mjs';
import { favorites } from './desktop-favorites.mjs';
import { installFaults } from './desktop-faults.mjs';

const [endpoint, dataDir, evidence, phase, expectedUrl] = process.argv.slice(2);
const browser = await chromium.connectOverCDP(endpoint, { timeout: 20_000 });
let page;
try {
  const pages = browser.contexts().flatMap(context => context.pages());
  assert.equal(pages.length, 1, 'Expected exactly one WebView in the owned instance');
  page = pages[0];
  page.on('console', message => console.log('WEBVIEW', message.type(), message.text()));
  page.on('pageerror', error => console.error('WEBVIEW ERROR', error.message));
  page.setDefaultTimeout(15_000);
  // WebView2 CDP can attach after the browser's load event has fired. Verify
  // the actual origin and ready app instead of waiting for a missed lifecycle event.
  await page.waitForFunction(origin => location.origin === origin, new URL(expectedUrl).origin);
  await page.waitForFunction(() => Boolean(window.__TAURI_INTERNALS__?.invoke));
  const snapshot = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('dispatch', { method: 'snapshot', args: {} }));
  const normalize = value => path.resolve(value).toLowerCase();
  assert.equal(normalize(snapshot.dataDir), normalize(dataDir), 'Wrong native DataDir');
  const targets = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('dispatch', { method: 'targets.list', args: {} }));
  for (const target of targets.targets) {
    assert.ok(normalize(target.globalPath).startsWith(normalize(path.dirname(dataDir)) + path.sep), 'Agent target outside fixture');
  }
  await page.locator('.nav-item').first().waitFor({ timeout: 45_000 });
  let result;
  if (phase.startsWith('layout-')) {
    const [width, height] = phase.slice(7).split('x').map(Number);
    await page.waitForFunction(({ width, height }) => innerWidth === width && innerHeight === height, { width, height });
    result = await layout(page, path.join(evidence, 'screenshots'));
  } else if (phase === 'favorites') {
    await page.evaluate(installFaults, dataDir);
    result = await favorites(page, path.join(evidence, 'screenshots'));
  } else if (phase === 'restart') {
    await page.locator('.nav-item').nth(1).click();
    await page.getByRole('button', { name: 'Remove acceptance-writer from favorites', exact: true }).waitFor();
    await page.getByRole('button', { name: 'Add acceptance-second to favorites', exact: true }).waitFor();
    assert.equal(snapshot.skills.find(skill => skill.name === 'acceptance-writer').favorite, true);
    assert.equal(snapshot.skills.find(skill => skill.name === 'acceptance-second').favorite, false);
    assert.equal(await page.evaluate(() => Boolean(window.__acceptanceFaults)), false);
    await page.screenshot({ path: path.join(evidence, 'screenshots/native-restart.png') });
    result = { persisted: true, injectionAbsent: true, dataDir: snapshot.dataDir };
  } else throw new Error('Unknown phase');
  await fs.writeFile(path.join(evidence, `${phase}.json`), JSON.stringify(result, null, 2));
  console.log(JSON.stringify({ phase, status: 'passed', result }));
} catch (error) {
  if (page) console.error('WEBVIEW DOM', await page.evaluate(() => ({ url: location.href, ready: document.readyState, body: document.body?.innerText, html: document.documentElement.outerHTML.slice(0, 4000) })).catch(String));
  if (page) await page.screenshot({ path: path.join(evidence, `screenshots/failed-${phase}.png`) }).catch(() => {});
  throw error;
} finally {
  // For CDP, close disconnects this client; desktop.ps1 owns app shutdown.
  await browser.close();
}
