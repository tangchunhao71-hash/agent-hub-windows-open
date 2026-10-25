import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, realpathSync, copyFileSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { sourceSnapshot } from '../source/src/tooling/build-provenance.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const notices = ['LICENSE', 'THIRD_PARTY.md', 'DEPENDENCY_NOTICES.txt', 'RELEASE_LICENSE_SUPPLEMENT.txt', 'THIRD_PARTY_NOTICES.txt', 'LobeIcons-LICENSE.txt'];
export function verifyBuildArtifact(bytes, path, artifacts) {
  const record = artifacts?.find(row => row.path === path);
  if (!record || record.sha256 !== hash(bytes) || record.bytes !== bytes.length)
    throw Error('App file differs from the build receipt: ' + path);
}
export function machoArchitecture(bytes) {
  if (bytes.length < 32 || bytes.readUInt32LE(0) !== 0xfeedfacf)
    throw Error('Expected a native 64-bit Mach-O executable, not a Windows file or universal bundle');
  const cpu = bytes.readUInt32LE(4);
  if (cpu === 0x0100000c) return 'arm64';
  if (cpu === 0x01000007) return 'x64';
  throw Error('Unsupported Mach-O CPU');
}
function run(command, args) {
  return execFileSync(command, args, { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 }).trim();
}
function appFiles(folder) {
  const entries = [];
  function visit(path) {
    const rel = relative(folder, path);
    if (/(^|[\\/])(?:\.git|\.env(?:\..*)?|node_modules|local-only|__pycache__)([\\/]|$)|\.(?:exe|dll|sqlite3?|db|jsonl|log)$/i.test(rel))
      throw Error('Private or wrong-platform app resource: ' + rel);
    if (lstatSync(path).isSymbolicLink()) {
      const target = relative(folder, realpathSync(path));
      if (target === '..' || target.startsWith('..' + sep) || resolve(folder, target) !== realpathSync(path))
        throw Error('App symlink escapes bundle: ' + rel);
      return;
    }
    if (lstatSync(path).isDirectory()) {
      for (const name of readdirSync(path).sort()) visit(join(path, name));
    } else {
      const bytes = readFileSync(path);
      entries.push({ path: rel.split(sep).join('/'), size: bytes.length, sha256: hash(bytes) });
    }
  }
  visit(folder);
  return entries;
}
export function packageMac(receiptFile) {
  if (process.platform !== 'darwin') throw Error('Packaging requires macOS to preserve the app bundle and permissions');
  const receipt = JSON.parse(readFileSync(resolve(receiptFile), 'utf8'));
  if (receipt.exitCode !== 0 || receipt.status !== 'built' || !receipt.sourceStable || receipt.plan?.platform !== 'macos' || !receipt.plan.bundle)
    throw Error('A successful, stable macOS bundle build receipt is required');
  const now = sourceSnapshot(join(root, 'source'));
  if (now.head !== receipt.source.head || now.inputSha256 !== receipt.source.inputSha256)
    throw Error('Source differs from the build receipt');
  const app = join(root, 'source/target/release/bundle/macos/Agent Bot.app');
  const plist = join(app, 'Contents/Info.plist');
  const value = key => run('/usr/bin/plutil', ['-extract', key, 'raw', '-o', '-', plist]);
  if (value('CFBundleIdentifier') !== 'app.agentbot.macos') throw Error('Unexpected application identity');
  const version = value('CFBundleShortVersionString');
  const expected = JSON.parse(readFileSync(join(root, 'source/src/apps/desktop/src-tauri/tauri.conf.json'), 'utf8')).version;
  if (version !== expected || !/^\d+\.\d+\.\d+$/.test(version)) throw Error('Unexpected app version');
  const executable = value('CFBundleExecutable');
  if (!/^[\w.-]+$/.test(executable)) throw Error('Unsafe app executable path');
  for (const name of ['Contents/Info.plist', 'Contents/MacOS/' + executable, 'Contents/Resources/agent-hud-engine']) {
    const path = join(app, name);
    verifyBuildArtifact(readFileSync(path), relative(join(root, 'source'), path).split(sep).join('/'), receipt.artifacts);
  }
  const arch = machoArchitecture(readFileSync(join(app, 'Contents/MacOS', executable)));
  if (arch !== process.arch) throw Error('App architecture differs from this build host');
  if (machoArchitecture(readFileSync(join(app, 'Contents/Resources/agent-hud-engine'))) !== arch)
    throw Error('Engine and desktop architecture mismatch');
  for (const name of notices)
    if (!readFileSync(join(app, 'Contents/Resources', name)).length) throw Error('Missing license: ' + name);
  const inputs = appFiles(app);
  const parent = join(root, 'local-only/macos-delivery');
  mkdirSync(parent, { recursive: true });
  const label = 'Agent-Bot-' + version + '-macos-' + arch + '-review';
  const stage = join(parent, label), archive = stage + '.zip';
  if (existsSync(stage) || existsSync(archive)) throw Error('Output exists; preserve old releases and choose a fresh workspace');
  mkdirSync(stage);
  run('/usr/bin/ditto', [app, join(stage, 'Agent Bot.app')]);
  if (JSON.stringify(inputs) !== JSON.stringify(appFiles(join(stage, 'Agent Bot.app')))) throw Error('App copy differs from build');
  for (const [from, to] of [['INSTALL.md', '安装与更新.md'], ['CHANGELOG.md', '更新说明.md'], ['PRIVACY.md', '隐私说明.md']])
    copyFileSync(join(root, 'docs/release/macos', from), join(stage, to));
  for (const name of notices)
    copyFileSync(join(app, 'Contents/Resources', name), join(stage, name));
  const provenance = {
    product: 'Agent Bot', version, platform: 'macos', architecture: arch,
    status: 'LOCAL_REVIEW_CANDIDATE', trustedDeveloperSignatureVerified: false,
    notarizationVerified: false, nativeInstallationTested: false,
    source: { commit: now.head, dirty: now.dirty, inputSha256: now.inputSha256 },
    buildReceiptSha256: hash(readFileSync(resolve(receiptFile))),
    bundleFiles: inputs,
  };
  writeFileSync(join(stage, 'provenance.json'), JSON.stringify(provenance, null, 2) + '\n');
  const sums = readdirSync(stage).filter(name => lstatSync(join(stage, name)).isFile())
    .sort().map(name => hash(readFileSync(join(stage, name))) + '  ' + name).join('\n');
  writeFileSync(join(stage, 'SHA256SUMS.txt'), sums + '\n');
  run('/usr/bin/ditto', ['-c', '-k', '--sequesterRsrc', '--keepParent', stage, archive]);
  writeFileSync(archive + '.sha256', hash(readFileSync(archive)) + '  ' + label + '.zip\n');
  console.log(JSON.stringify({ archive, sha256: hash(readFileSync(archive)), ...provenance.source }));
}
if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  try {
    if (process.argv.length !== 3) throw Error('Usage: node scripts/package-macos.mjs build-receipt.json');
    packageMac(process.argv[2]);
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
