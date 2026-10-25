// SPDX-License-Identifier: MIT
import { createHash } from 'node:crypto';
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { sourceSnapshot } from '../source/src/tooling/build-provenance.mjs';
import { signatureReport } from './signature-report.mjs';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const hash=bytes=>createHash('sha256').update(bytes).digest('hex');
if(process.platform!=='win32')throw Error('Windows packaging requires a Windows host');
const receipt=JSON.parse(readFileSync(resolve(process.argv[2] || 'source/.build/windows-release.json'),'utf8').replace(/^\uFEFF/,''));
if(receipt.exitCode!==0||!receipt.sourceStable||receipt.plan?.platform!=='windows'||!receipt.plan.bundle)throw Error('Successful stable Windows installer build required');
const source=sourceSnapshot(join(root,'source'));
if(source.head!==receipt.source.head||source.inputSha256!==receipt.source.inputSha256)throw Error('Source changed after build');
const config=JSON.parse(readFileSync(join(root,'source/src/apps/desktop/src-tauri/tauri.conf.json'),'utf8'));
const name=`${config.productName}_${config.version}_x64-setup.exe`;
const installer=join(root,'source/target/release/bundle/nsis',name);
const bytes=readFileSync(installer);
const record=receipt.artifacts.find(row=>row.path===`target/release/bundle/nsis/${name}`);
if(!record||record.sha256!==hash(bytes))throw Error('Installer differs from build receipt');
const signature=signatureReport(installer);
if(process.env.AGENTBOT_REQUIRE_TRUSTED_SIGNATURE==='1'&&!signature.trusted)throw Error('Trusted Authenticode signature required');
const label=`Agent-Bot-${config.version}-windows-x64-review`;
const folder=join(root,'local-only/windows-delivery',label),zip=folder+'.zip';
if(existsSync(folder)||existsSync(zip))throw Error('Preserve prior deliverables; output already exists');
mkdirSync(folder,{recursive:true});copyFileSync(installer,join(folder,name));
const inputs={
  'LICENSE':'LICENSE','LICENSE-MIT':'LICENSE-MIT','NOTICE':'NOTICE','LICENSING.md':'LICENSING.md',
  'source/docs/license-map.json':'LICENSE_MAP.json',
  'docs/release/TERMS.md':'TERMS.md','docs/release/PRIVACY.md':'PRIVACY.md',
  'source/docs/third-party.md':'THIRD_PARTY.md','source/docs/dependency-notices.txt':'DEPENDENCY_NOTICES.txt',
  'source/docs/release-license-supplement.txt':'RELEASE_LICENSE_SUPPLEMENT.txt',
  'source/src/packages/hud/src/assets/THIRD_PARTY_NOTICES.txt':'THIRD_PARTY_NOTICES.txt',
  'source/src/packages/hud/src/assets/LobeIcons-LICENSE.txt':'LobeIcons-LICENSE.txt',
  'docs/release/INSTALL.md':'安装与更新.md','docs/release/CHANGELOG.md':'更新说明.md'};
for(const [from,to]of Object.entries(inputs))copyFileSync(join(root,from),join(folder,to));
const provenance={product:'Agent Bot',version:config.version,platform:'windows',architecture:'x64',
  status:'LOCAL_REVIEW_CANDIDATE',signature,nativeInstallationTested:false,
  source:{commit:source.head,dirty:source.dirty,inputSha256:source.inputSha256},
  installerSha256:hash(bytes),buildReceiptSha256:hash(readFileSync(resolve(process.argv[2] || 'source/.build/windows-release.json')))};
writeFileSync(join(folder,'provenance.json'),JSON.stringify(provenance,null,2)+'\n');
const names=[name,...Object.values(inputs),'provenance.json'];
writeFileSync(join(folder,'SHA256SUMS.txt'),names.sort().map(file=>hash(readFileSync(join(folder,file)))+'  '+file).join('\n')+'\n');
const result=spawnSync('powershell.exe',['-NoProfile','-File',join(root,'scripts/zip-windows-delivery.ps1'),'-Directory',folder,'-Destination',zip],{stdio:'inherit',windowsHide:true});
if(result.error||result.status!==0)throw Error('Windows ZIP creation failed');
writeFileSync(zip+'.sha256',hash(readFileSync(zip))+'  '+label+'.zip\n');
console.log(JSON.stringify({zip,sha256:hash(readFileSync(zip)),signature}));
