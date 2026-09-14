// Deliberately damage only a disposable startup fixture, never application data.
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { lstatSync, realpathSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { DatabaseSync } from 'node:sqlite';

const root = realpathSync(resolve(dirname(fileURLToPath(import.meta.url)), '..'));
const data = realpathSync(process.argv[2] || '');
const within = relative(join(root, '.test-data'), data);
assert(within && !within.startsWith('..') && !within.includes(':') && within.split(sep)[0].startsWith('packaged-startup-'), 'Only disposable packaged-startup fixtures may be changed');
const file = join(data, 'agenthub.sqlite3');
assert(lstatSync(file).isFile() && !lstatSync(file).isSymbolicLink(), 'Fixture database must be a regular file');
const db = new DatabaseSync(file);
try {
  assert(db.prepare('SELECT COUNT(*) AS count FROM prompts').get().count > 0, 'Seed the normal startup fixture first');
  db.prepare("INSERT INTO skill_change_plans(id,payload_json,state) VALUES(?,?,'executing')").run(randomUUID(), 'damaged recovery fixture');
} finally {
  db.close();
}
