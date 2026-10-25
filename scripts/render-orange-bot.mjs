import { createRequire } from 'node:module';
import { writeFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const require = createRequire(resolve(root, 'source/src/packages/hud/test/seed-robot.test.ts'));
const { createServer } = await import(pathToFileURL(require.resolve('vite')));
const server = await createServer({ configFile: false, root: resolve(root,'source/src/packages/hud/src'), server:{middlewareMode:true,watch:null},appType:'custom',logLevel:'silent' });
try {
  const { botSpecs } = await server.ssrLoadModule('/bots/bot-spec.ts');
  const spec = botSpecs(false).claude;
  if(spec.shape.kind !== 'circle' || typeof spec.paint === 'string') throw Error('Expected round gradient Bot');
  const colors = spec.paint.colors;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="420" height="420" viewBox="0 0 420 420">
  <defs><radialGradient id="body" cx="30%" cy="20%" r="85%"><stop stop-color="${colors[0]}"/><stop offset=".58" stop-color="${colors[1]}"/><stop offset="1" stop-color="${colors[2]}"/></radialGradient></defs>
  <rect width="420" height="420" rx="28" fill="#171717"/>
  <ellipse cx="210" cy="326" rx="92" ry="12" fill="#000" opacity=".25"/>
  <g fill="${colors[1]}"><ellipse cx="101" cy="244" rx="17" ry="22" transform="rotate(22 101 244)"/><ellipse cx="319" cy="244" rx="17" ry="22" transform="rotate(-22 319 244)"/><ellipse cx="172" cy="317" rx="22" ry="12"/><ellipse cx="248" cy="317" rx="22" ry="12"/></g>
  <circle cx="210" cy="214" r="108" fill="url(#body)"/>
  <g fill="${spec.eyeColor}"><rect x="166" y="195" width="22" height="43" rx="11"/><rect x="233" y="195" width="22" height="43" rx="11"/></g>
  <g font-family="Segoe UI,Microsoft YaHei,sans-serif" text-anchor="middle"><text x="210" y="52" fill="#fff6ed" font-size="23">Claude · Orange Bot</text><text x="210" y="382" fill="#cdb5a7" font-size="16">圆乎乎的橙色搭档</text></g></svg>`;
  writeFileSync(resolve(root,'docs/orange-bot-preview.svg'),svg);
  console.log('Rendered shape preview from current BotSpec; not a native screenshot.');
} finally { await server.close(); }
