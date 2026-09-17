// Isolated fictional metadata for a native header review, not a Git transport test.
import { DatabaseSync } from 'node:sqlite';
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const root = fs.realpathSync(process.argv[2]);
assert.equal(process.env.COMPUTERNAME?.toUpperCase(), 'TEST');
assert.equal(process.env.USERNAME?.toLowerCase(), 'try');
assert.match(root, /^C:\\AgentHub-VM-Test\\cases\\[a-z0-9-]+\\fixture$/i);
const session = JSON.parse(fs.readFileSync(path.join(root, 'session.json'), 'utf8').replace(/^\uFEFF/, ''));
assert.equal(path.resolve(session.root).toLowerCase(), root.toLowerCase());
assert.equal(path.resolve(session.dataDir).toLowerCase(), path.join(root, 'data').toLowerCase());
// store.rs defines skills(id, payload_json). Preserve every other saved field.
const db = new DatabaseSync(path.join(root, 'data', 'agenthub.sqlite3'));
try {
  const skills = db.prepare('SELECT id, payload_json FROM skills').all();
  const row = skills.find(row => JSON.parse(row.payload_json).name === 'acceptance-second');
  assert.ok(row, 'Expected the fictional fixture Skill');
  const skill = JSON.parse(row.payload_json);
  skill.source = { ...skill.source, kind: 'git', locator: 'https://github.com/agenthub-fixtures/layout-only.git' };
  db.prepare('UPDATE skills SET payload_json = ? WHERE id = ?').run(JSON.stringify(skill), row.id);
} finally { db.close(); }
