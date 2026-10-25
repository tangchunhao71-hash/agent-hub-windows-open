// Agent Bot modifications, 2026-10-26; upstream portions retain their original licenses.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, lstatSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const git = (...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8', windowsHide: true, maxBuffer: 32 * 1024 * 1024 });
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
// Explicit source roots; outer docs, local-only, Git history and promotional
// screenshots/videos are never inputs. Export is a review candidate, not a publish action.
const roots = ['source/src/', 'source/vendor/tao/', 'source/.cargo/', '.github/workflows/', 'source/integrations/dsh-navigation/', 'docs/release/'];
const files = new Set(['LICENSE', 'LICENSE-MIT', 'NOTICE', 'LICENSING.md', '.gitignore', '.gitattributes', 'source/LICENSE', 'source/CONTRIBUTING.md',
  'source/Cargo.toml', 'source/Cargo.lock', 'source/package.json',
  'source/pnpm-lock.yaml', 'source/pnpm-workspace.yaml', 'source/tsconfig.base.json', 'source/rustfmt.toml',
  'source/.editorconfig', 'source/.gitattributes', 'source/.gitignore',
  'source/.prettierignore', 'source/.prettierrc.json',
  'source/docs/third-party.md', 'source/docs/dependency-notices.txt', 'source/docs/release-license-supplement.txt', 'source/docs/tao-panel-patch.md',
  'source/docs/license-map.json', 'source/docs/license-evidence.json', 'source/docs/artwork-provenance.json', 'source/docs/modifications.json',
  'scripts/package-netdisk-delivery.ps1',
  'scripts/export-public-source.mjs', 'scripts/collect-release-notices.mjs', 'scripts/build-windows.ps1', 'scripts/build-macos.sh', 'scripts/package-macos.mjs', 'scripts/package-windows.mjs', 'scripts/signature-report.mjs', 'scripts/windows-signature-report.ps1', 'scripts/zip-windows-delivery.ps1', 'scripts/create-release-artwork.mjs', 'scripts/install-dsh-navigation.ps1', 'scripts/render-orange-bot.mjs', 'scripts/render-orange-bot-four-frames.mjs']);
const excluded = /(^|\/)(\.git|node_modules|target|dist|\.build|local-only|screenshots|videos|__pycache__)(\/|$)|^source\/vendor\/tao\/\.github\/|(^|\/)\.env(?:\.|$)|\.(?:exe|dll|pdb|sqlite3?|db|jsonl|log)$/i;
const selected = git('ls-files', '-z').split('\0').filter(p => p &&
  (files.has(p) || roots.some(prefix => p.startsWith(prefix))) && !excluded.test(p));
if (!process.argv[2]) throw Error('Usage: node scripts/export-public-source.mjs <new-output-directory>');
const output = resolve(process.argv[2]);
if (existsSync(output)) throw Error('Output must not exist; exports never overwrite');
if (!relative(root, output).startsWith('local-only' + (process.platform === 'win32' ? '\\' : '/')))
  throw Error('Review exports must be inside this repository local-only/ directory');
const revision = git('rev-parse', 'HEAD').trim();
if (git('status', '--porcelain', '--untracked-files=normal').trim())
  throw Error('Commit reviewed source changes before export; source must be clean');
const rows = selected.sort().map(path => {
  const input = resolve(root, path);
  if (!lstatSync(input).isFile()) throw Error(`Non-regular input: ${path}`);
  const bytes = readFileSync(input);
  return { path, bytes, sha256: sha(bytes) };
});
mkdirSync(output, { recursive: true });
for (const row of rows) {
  const destination = resolve(output, row.path);
  mkdirSync(dirname(destination), { recursive: true });
  writeFileSync(destination, row.bytes, { flag: 'wx' });
}
writeFileSync(resolve(output, 'README.md'), readFileSync(resolve(root, 'docs/release/README.md')));
const manifest = { revision, status: 'LOCAL_REVIEW_CANDIDATE_NOT_RELEASE',
  files: rows.map(({ bytes, ...row }) => ({ ...row, size: bytes.length })),
  product: 'Agent Bot', excludedFeatures: ['local experimental cross-agent delegation', 'legacy client hook and plugin takeover'],
  pending: ['clean Windows install/upgrade/uninstall/rollback', 'native display and navigation acceptance', 'trusted platform signatures and macOS notarization', 'distribution approval'] };
writeFileSync(resolve(output, 'SOURCE-MANIFEST.json'), JSON.stringify(manifest, null, 2) + '\n');
for (const row of rows) {
  if (sha(readFileSync(resolve(output, row.path))) !== row.sha256 ||
      sha(readFileSync(resolve(root, row.path))) !== row.sha256)
    throw Error(`Source or export changed during export: ${row.path}`);
}
console.log(JSON.stringify({ output, revision, files: rows.length, manifestSha256:
  sha(readFileSync(resolve(output, 'SOURCE-MANIFEST.json'))) }, null, 2));
