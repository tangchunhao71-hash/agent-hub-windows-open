// Inputs are local package-manager metadata, never application/account data.
import { readFileSync, readdirSync, writeFileSync, existsSync } from 'node:fs';
import { resolve, dirname, relative, isAbsolute } from 'node:path';
import { createHash } from 'node:crypto';

const [npmFile, cargoFile, destination] = process.argv.slice(2);
if (!destination) throw Error('Usage: collect-release-notices.mjs npm-licenses.json cargo-metadata.json destination.txt');
const npm = JSON.parse(readFileSync(npmFile, 'utf8').replace(/^\uFEFF/, ''));
const cargo = JSON.parse(readFileSync(cargoFile, 'utf8').replace(/^\uFEFF/, ''));
const rows = [
  ...Object.values(npm).flat().map(p => ({ name:p.name, version:p.versions.join(', '), license:p.license, url:p.homepage || '', directories:p.paths })),
  ...cargo.packages.filter(p => p.source || p.name === 'tao').map(p => ({ name:p.name, version:p.version, license:p.license || 'See notice', url:p.repository || '', directories:[dirname(p.manifest_path)], licenseFile:p.license_file })),
].sort((a,b)=>(a.name+a.version).localeCompare(b.name+b.version));
const notices = new Map();
const records = rows.map(row => {
  const ids = new Set();
  for (const directory of row.directories) {
    const names = readdirSync(directory, {withFileTypes:true}).filter(f=>f.isFile() && /^(licen[cs]e|copying|notice|copyright)([._-]|$)/i.test(f.name)).map(f=>f.name);
    if (row.licenseFile) {
      const candidate=relative(directory,resolve(directory,row.licenseFile));
      if(!candidate.startsWith('..')&&!isAbsolute(candidate)) names.push(candidate);
    }
    for(const name of new Set(names)) {
      const file=resolve(directory,name);
      if(!existsSync(file)) continue;
      const text=readFileSync(file,'utf8').replace(/\r\n/g,'\n').replace(/[\t ]+$/gm,'').trim();
      if(!text||text.includes('\0')) continue;
      const id=createHash('sha256').update(text).digest('hex');
      ids.add(id);notices.set(id,text);
    }
  }
  return {name:row.name,version:row.version,license:row.license,url:row.url,notices:[...ids]};
});
const header='Agent Bot dependency inventory and available license notices\n\nGenerated from the frozen pnpm production inventory and the Windows Cargo dependency graph, including build/test dependencies. This is broader than the linked runtime. Package paths and private machine metadata are omitted. Packages without a separate license file retain their declared license and source URL below; this inventory is not a legal approval or a claim of trademark rights. Upstream project and provider notices are shipped separately.\n\n';
const body=records.map(p=>`${p.name} ${p.version}\nLicense: ${p.license}\nSource: ${p.url}\nNotices: ${p.notices.join(', ')||'No separate license file found; review upstream source before public distribution.'}\n`).join('\n');
writeFileSync(destination,header+body+'\nFULL NOTICE TEXTS\n\n'+[...notices].map(([id,text])=>`NOTICE ${id}\n${text}\n`).join('\n'));
console.log(JSON.stringify({packages:records.length,noticeTexts:notices.size,withoutSeparateNotice:records.filter(r=>!r.notices.length).map(r=>`${r.name}@${r.version}`)}));
