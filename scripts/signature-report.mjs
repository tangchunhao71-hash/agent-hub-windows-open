// SPDX-License-Identifier: MIT
import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
export function signatureReport(path, platform = process.platform, run = spawnSync) {
  if (platform === 'win32') {
    const result = run('powershell.exe', ['-NoProfile', '-File', resolve(root, 'scripts/windows-signature-report.ps1'), '-Path', resolve(path)], {encoding:'utf8',windowsHide:true});
    if (result.error || result.status !== 0) return {platform:'windows',trusted:false,status:'verification-failed'};
    try {
      const value = JSON.parse(result.stdout.replace(/^\uFEFF/,''));
      return {platform:'windows',trusted:value.status === 'Valid' && value.trusted === true,status:value.status,subject:value.subject,thumbprint:value.thumbprint};
    } catch { return {platform:'windows',trusted:false,status:'invalid-verification-result'}; }
  }
  if (platform === 'darwin') {
    const options = {encoding:'utf8'};
    const identity = run('/usr/bin/codesign', ['-dvv', path], options);
    const check = run('/usr/bin/codesign', ['--verify','--deep','--strict',path], options);
    const assessment = run('/usr/sbin/spctl', ['--assess','--type','execute',path], options);
    const ticket = run('/usr/bin/xcrun', ['stapler','validate',path], options);
    const developerId = /^Authority=Developer ID Application:/m.test(identity.stderr || '');
    const trusted = identity.status === 0 && developerId && check.status === 0 && assessment.status === 0;
    return {platform:'macos',trusted,notarized:trusted && ticket.status === 0,
      status:trusted ? 'developer-id-assessed' : 'untrusted-or-unsigned',
      codesignVerified:check.status === 0,gatekeeperAccepted:assessment.status === 0,
      stapledTicketValid:ticket.status === 0};
  }
  return {platform, trusted:false, status:'unsupported-platform'};
}
if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  if (!process.argv[2]) throw Error('Usage: node scripts/signature-report.mjs file-or-app [--require-trusted]');
  const result = signatureReport(process.argv[2]);
  console.log(JSON.stringify(result));
  if (process.argv.includes('--require-trusted') && (!result.trusted || (process.platform === 'darwin' && !result.notarized))) process.exitCode = 3;
}
