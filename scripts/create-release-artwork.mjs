// SPDX-License-Identifier: MIT
// Original geometric artwork for Agent Bot. No external image or font inputs.
import { createRequire } from 'node:module';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { mkdirSync, writeFileSync } from 'node:fs';
const require = createRequire(import.meta.url);
const sharp = require(process.argv[2] || 'sharp');
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const icon = `<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 256 256">
<rect x="12" y="12" width="232" height="232" rx="62" fill="#171923"/>
<path d="M51 140Q26 143 29 162Q31 179 52 172M205 140Q230 143 227 162Q225 179 204 172" fill="#ef985b"/>
<path d="M85 203v15q0 10 13 10t13-10v-15M145 203v15q0 10 13 10t13-10v-15" fill="#ef985b"/>
<rect x="48" y="51" width="160" height="158" rx="66" fill="#f7ad71"/>
<path d="M70 88q13-20 36-22" stroke="#ffce9e" stroke-width="9" stroke-linecap="round" fill="none"/>
<rect x="87" y="116" width="13" height="19" rx="6.5" fill="#242330"/>
<rect x="156" y="116" width="13" height="19" rx="6.5" fill="#242330"/>
<path d="M116 150q12 12 24 0" stroke="#6f4238" stroke-width="7" stroke-linecap="round" fill="none"/>
</svg>`;
const wallpaper = `<svg xmlns="http://www.w3.org/2000/svg" width="1600" height="1000" viewBox="0 0 1600 1000">
<defs><linearGradient id="g" x2="1" y2="1"><stop stop-color="#19304b"/><stop offset=".6" stop-color="#49384e"/><stop offset="1" stop-color="#d39379"/></linearGradient>
<radialGradient id="r"><stop stop-color="#e6b79b" stop-opacity=".65"/><stop offset="1" stop-color="#e6b79b" stop-opacity="0"/></radialGradient></defs>
<rect width="1600" height="1000" fill="url(#g)"/><ellipse cx="1230" cy="310" rx="600" ry="600" fill="url(#r)"/>
<path d="M-100 810Q420 340 950 760T1800 640" fill="none" stroke="#bed0dd" stroke-opacity=".13" stroke-width="130"/>
</svg>`;
const app = resolve(root, 'source/src/packages/app/src/assets');
const ui = resolve(root, 'source/src/packages/ui/src/assets');
mkdirSync(app, {recursive:true}); mkdirSync(ui, {recursive:true});
writeFileSync(resolve(app, 'agent-bot-icon.svg'), icon+'\n');
writeFileSync(resolve(ui, 'agent-bot-wallpaper.svg'), wallpaper+'\n');
await sharp(Buffer.from(icon)).png().toFile(resolve(app, 'agent-bot-icon.png'));
console.log('Generated original Agent Bot icon and wallpaper');
