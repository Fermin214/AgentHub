import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';

const packageDirectory = process.argv[2] ? resolve(process.argv[2]) : null;
const installed = process.argv.includes('--installed');
const sha256 = file => createHash('sha256').update(readFileSync(file)).digest('hex');

function listFiles(directory, base = directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    assert(!entry.isSymbolicLink(), `Package contains a link: ${entry.name}`);
    const file = join(directory, entry.name);
    return entry.isDirectory() ? listFiles(file, base) : [{ path: relative(base, file).replaceAll('\\', '/'), sha256: sha256(file) }];
  }).sort((a, b) => a.path.localeCompare(b.path));
}

try {
  assert(packageDirectory, 'Usage: node scripts/verify-portable.mjs <package-directory> [--installed]');
  assert(existsSync(packageDirectory), 'Package directory does not exist');
  const manifest = listFiles(packageDirectory);
  const expected = ['AgentHub.exe', 'data/runtime/LICENSE', 'data/runtime/THIRD-PARTY-NOTICES.md', 'data/runtime/agenthub-layout.json', 'data/runtime/DEPENDENCY-NOTICES.md', 'data/runtime/NSIS-COPYING.txt', 'data/runtime/inventory.json', 'data/runtime/RUST-STDLIB-NOTICES.html'];
  if (installed) expected.push('uninstall.exe', 'data/runtime/owned-files.ini');
  assert.deepEqual(manifest.map(file => file.path), expected.sort((a, b) => a.localeCompare(b)), 'Desktop package must contain only the desktop program, layout marker and license notices');
  const layout = JSON.parse(readFileSync(join(packageDirectory, 'data/runtime/agenthub-layout.json'), 'utf8'));
  assert.equal(layout.schemaVersion, 1);
  assert.equal(layout.dataDirectory, 'data');
  assert.equal(layout.libraryDirectory, 'data/library/skills');
  for (const name of ['LICENSE', 'THIRD-PARTY-NOTICES.md']) assert.equal(sha256(join(packageDirectory, 'data/runtime', name)), sha256(name));
  for (const name of ['DEPENDENCY-NOTICES.md', 'NSIS-COPYING.txt', 'inventory.json', 'RUST-STDLIB-NOTICES.html']) assert.equal(sha256(join(packageDirectory, 'data/runtime', name)), sha256(join('licenses', name)));
  console.log(JSON.stringify({ status: 'passed', packageDirectory, installed, checks: ['desktop-only-package', 'relative-data-layout'], manifest, desktopLaunchTested: false, scope: 'Only package structure and file hashes are checked. Run verify-packaged-startup.ps1 for the real desktop startup check.' }, null, 2));
} catch (error) {
  console.log(JSON.stringify({ status: 'failed', packageDirectory, installed, error: error.message }, null, 2));
  process.exitCode = 1;
}
