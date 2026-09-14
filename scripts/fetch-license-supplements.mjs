// Only missing license texts from exact upstream crate revisions; never downloads executable code.
import fs from 'node:fs';
import path from 'node:path';
const metadata = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const sources = [];
const overrides = {
  'alloc-stdlib': ['dropbox/rust-alloc-no-stdlib', 'LICENSE'],
  'clipboard-win': ['DoumanAsh/clipboard-win', 'LICENSE'],
  'defmt-parser': ['knurling-rs/defmt', 'LICENSE-MIT'],
  'tauri-plugin': ['tauri-apps/tauri', 'LICENSE_MIT'],
};
for (const p of metadata.packages) {
  let origin = overrides[p.name];
  if (p.name.startsWith('unic-')) origin = ['open-i18n/rust-unic', 'LICENSE-MIT'];
  if (p.name.startsWith('webview2-com')) origin = ['wravery/webview2-rs', 'LICENSE'];
  if (!origin) continue;
  const vcs = JSON.parse(fs.readFileSync(path.join(path.dirname(p.manifest_path), '.cargo_vcs_info.json'), 'utf8'));
  const url = `https://raw.githubusercontent.com/${origin[0]}/${vcs.git.sha1}/${origin[1]}`;
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${p.name}: ${response.status} ${url}`);
  const body = await response.text();
  fs.writeFileSync(`licenses/supplemental/${p.name}.txt`, `Source: ${url}\nCrate: ${p.name} ${p.version}\n\n${body}`);
  sources.push({ name: p.name, version: p.version, url });
  console.log(p.name);
}
fs.writeFileSync('licenses/supplemental/sources.json', JSON.stringify(sources, null, 2) + '\n');
const mpl = metadata.packages.find(p => p.name === 'cssparser');
fs.writeFileSync('licenses/supplemental/selectors.txt', 'License: MPL-2.0 (selectors source headers declare this license).\nCanonical terms: https://www.mozilla.org/MPL/2.0/\nUnmodified selectors 0.36.1 source: https://crates.io/crates/selectors/0.36.1\nText also shipped by the locked cssparser dependency:\n\n' + fs.readFileSync(path.join(path.dirname(mpl.manifest_path), 'LICENSE'), 'utf8'));
