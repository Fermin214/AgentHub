# Third-party notices

AgentHub is MIT licensed. Third-party components retain their own licenses and copyright notices; the project's MIT license does not replace those terms.

## Dependency licenses

`licenses/inventory.json` records dependency versions, sources, and license-text digests. `licenses/DEPENDENCY-NOTICES.md` preserves the original upstream license texts. They are generated from Cargo.lock, the Windows x64 desktop dependency graph, and production dependencies in package-lock.json, distinguishing runtime dependencies from build dependencies involved in generated code. A build entry does not imply distribution of a compiler, Node.js, or the internal test CLI.

Dependency sources are unmodified. Source for the exact versions of MPL-2.0 components is available from the crates.io links in the inventory. OR in a license expression provides a choice under upstream terms; AND means the conditions apply together.

The complete Rust 1.98.1 standard-library notices are preserved in `licenses/RUST-STDLIB-NOTICES.html`. SQLite's original source is in the public domain; see https://www.sqlite.org/copyright.html . Windows system components and WebView2 are provided separately by Microsoft under its terms. Application packages do not bundle the full WebView2 Runtime.

The installer and portable packages include the project LICENSE, this notice, DEPENDENCY-NOTICES.md, inventory.json, RUST-STDLIB-NOTICES.html, and NSIS-COPYING.txt under `data/runtime/`.

## Installer

The custom NSIS template is derived from Tauri CLI v2.11.4. Modifications cover data retention, file-ownership records, installation location, and shortcut refresh. The modified template is included with this project. Upstream source:
https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.11.4/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi

Tauri and nsis-tauri-utils are MIT / Apache-2.0 licensed. Terms for NSIS stubs, plugins, and compression modules are in NSIS-COPYING.txt, including CPL-1.0 terms for the LZMA module. Original NSIS source and modules are available from https://sourceforge.net/projects/nsis/files/NSIS%203/ and are not modified by this project.

## Icons and fonts

The Codex, Claude Code, DeepSeek, Hermes Agent, and Z.ai SVGs come from [Lobe Icons](https://github.com/lobehub/lobe-icons/tree/a94750e3f5f8fc33757b839d85030e742284e43a/packages/static-svg/icons) under the MIT license, Copyright (c) 2023 LobeHub. Local files preserve the upstream artwork and colors at commit `a94750e3f5f8fc33757b839d85030e742284e43a`:

| Local file in `src/assets/agents/` | Upstream file | Agent |
| --- | --- | --- |
| `codex.svg` | `codex-color.svg` | Codex |
| `claudecode.svg` | `claudecode-color.svg` | Claude Code |
| `deepseek.svg` | `deepseek-color.svg` | DeepSeek Harness |
| `hermesagent.svg` | `hermesagent.svg` | Hermes |
| `zai.svg` | `zai.svg` | ZCode (Z.ai brand) |

ZCode uses its publisher's Z.ai mark; this is not a ZCode-specific logo. Product relationship: [official ZCode site](https://zcode.z.ai/en). Trademarks belong to their respective owners; identification icons do not imply endorsement. The AgentHub application icon is provided by the project maintainer. The interface uses system fonts and does not bundle font files.

The original Lobe Icons license follows. Other upstream terms are also preserved in their original form.

MIT License

Copyright (c) 2023 LobeHub

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
