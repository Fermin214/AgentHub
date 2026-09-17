import { defineConfig } from '@playwright/test';
import path from 'node:path';

const evidence = process.env.AGENTHUB_ACCEPTANCE_OUTPUT || 'output/ui-smoke';
export default defineConfig({
  testDir: './tests/ui',
  testMatch: '*.spec.ts',
  workers: 1,
  retries: 0,
  // Cold Vite transforms can take longer on Windows; action assertions stay short.
  timeout: 90_000,
  outputDir: path.join(evidence, 'screenshots'),
  reporter: [['list'], ['json', { outputFile: path.join(evidence, 'ui-results.json') }]],
  use: {
    channel: 'msedge',
    headless: true,
    actionTimeout: 10_000,
    viewport: { width: 1280, height: 860 },
    baseURL: 'http://127.0.0.1:1437',
    screenshot: 'on',
    trace: 'retain-on-failure',
  },
  webServer: {
    command: 'node node_modules/vite/bin/vite.js --config scripts/acceptance/ui-vite.config.ts --port 1437 --strictPort',
    url: 'http://127.0.0.1:1437/tests/ui/index.html',
    reuseExistingServer: false,
    timeout: 30_000,
  },
});
