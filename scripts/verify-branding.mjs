import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';

const retiredName = /toolbox|agent[-_ ]?tool[-_ ]?box|工具箱/i;
const sourceDirectories = ['src', 'crates', 'src-tauri', 'scripts', 'docs', '.github'];
const textExtensions = new Set(['.rs', '.ts', '.tsx', '.js', '.mjs', '.json', '.toml', '.md', '.ps1', '.nsi', '.nsh', '.yml', '.yaml', '.html', '.css', '.svg']);
const files = [];
function walk(directory) {
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    assert(!entry.isSymbolicLink(), `Source directory contains a link: ${directory}/${entry.name}`);
    const file = path.join(directory, entry.name).replaceAll('\\', '/');
    assert(!retiredName.test(file), `Retired product name in file path: ${file}`);
    if (entry.isDirectory()) walk(file);
    else if (textExtensions.has(path.extname(file))) files.push(file);
  }
}
sourceDirectories.forEach(walk);
for (const entry of fs.readdirSync('.', { withFileTypes: true })) {
  if (entry.isFile() && (textExtensions.has(path.extname(entry.name)) || entry.name === 'Cargo.lock')) files.push(entry.name);
}
for (const file of files) {
  // The rule definition itself and original copyright/license notices are not product branding.
  if (file === 'scripts/verify-branding.mjs') continue;
  assert(!retiredName.test(fs.readFileSync(file, 'utf8')), `Retired product name in source: ${file}`);
}
const config = JSON.parse(fs.readFileSync('src-tauri/tauri.conf.json', 'utf8'));
assert.equal(config.productName, 'AgentHub');
assert.equal(config.identifier, 'dev.agenthub.desktop');
assert.equal(config.bundle.resources['resources/agenthub-layout.json'], 'data/runtime/agenthub-layout.json');
console.log(`AgentHub source names verified across ${files.length} files; original license notices preserved.`);
