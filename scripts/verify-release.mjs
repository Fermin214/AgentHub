import assert from 'node:assert/strict';
import fs from 'node:fs';
import './verify-branding.mjs';
const read = p => fs.readFileSync(p, 'utf8').replaceAll('\r\n', '\n');
const pkg = JSON.parse(read('package.json'));
const lock = JSON.parse(read('package-lock.json'));
const tauri = JSON.parse(read('src-tauri/tauri.conf.json'));
assert.equal(pkg.version, tauri.version);
assert.equal(pkg.version, lock.version);
assert.equal(pkg.version, lock.packages[''].version);
assert.equal(pkg.version, read('Cargo.toml').match(/\[workspace.package\]\s+version = "([^"]+)"/)[1]);
for (const name of ['agenthub-core', 'agenthub-cli', 'agenthub-desktop']) {
  assert.equal(read('Cargo.lock').match(new RegExp(`name = "${name}"\\nversion = "([^"]+)"`))[1], pkg.version);
}
const inventory = JSON.parse(read('licenses/inventory.json'));
assert(inventory.packages.length > 300);
for (const p of inventory.packages) assert(p.notices.length, `Missing notices: ${p.name}`);
assert(tauri.bundle.resources['../licenses/DEPENDENCY-NOTICES.md']);
assert(tauri.bundle.resources['../licenses/NSIS-COPYING.txt']);
assert(tauri.bundle.resources['../licenses/RUST-STDLIB-NOTICES.html']);
assert(read('licenses/RUST-STDLIB-NOTICES.html').includes('Copyright notices for The Rust Standard Library'));
assert(read('rust-toolchain.toml').includes('1.98.1'));
console.log(`Release versions and ${inventory.packages.length} dependency notice entries verified: ${pkg.version}`);
