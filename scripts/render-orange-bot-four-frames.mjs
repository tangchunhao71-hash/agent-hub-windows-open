// Code-native original artwork. Pass a local sharp entry; no download or image API.
import { writeFileSync, mkdirSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
if (!process.argv[2]) throw Error('Pass the installed sharp module entry');
const { default: sharp } = await import(pathToFileURL(resolve(process.argv[2])));
const frames = [
  { name: 'rest', sx: 1, sy: 1, dy: 0, eyes: 1, durationMs: 500 },
  { name: 'blink', sx: 1.04, sy: .96, dy: 3, eyes: .12, durationMs: 120 },
  { name: 'rise', sx: .97, sy: 1.03, dy: -4, eyes: .85, durationMs: 180 },
  { name: 'settle', sx: .99, sy: 1.01, dy: -2, eyes: 1, durationMs: 500 },
];
const body = (frame, id) => {
  const eh = 30 * frame.eyes, ey = 134 - eh / 2;
  return `<defs><radialGradient id="${id}" cx="30%" cy="20%" r="85%"><stop stop-color="#f5b59a"/><stop offset=".58" stop-color="#d97757"/><stop offset="1" stop-color="#b95f42"/></radialGradient></defs>
  <g transform="translate(128 ${128+frame.dy}) scale(${frame.sx} ${frame.sy}) translate(-128 -128)">
  <g fill="#d97757"><ellipse cx="51" cy="152" rx="13" ry="16"/><ellipse cx="205" cy="152" rx="13" ry="16"/><ellipse cx="101" cy="201" rx="16" ry="9"/><ellipse cx="155" cy="201" rx="16" ry="9"/></g>
  <circle cx="128" cy="130" r="76" fill="url(#${id})"/>
  <g fill="#3b2520"><rect x="97" y="${ey}" width="16" height="${eh}" rx="${Math.min(8,eh/2)}"/><rect x="144" y="${ey}" width="16" height="${eh}" rx="${Math.min(8,eh/2)}"/></g></g>`;
};
const asset = resolve(root,'source/src/packages/hud/src/assets');
const preview = resolve(root,'local-only/orange-bot-four-frames');
mkdirSync(preview,{recursive:true});
const layer = [];
for (let i=0;i<4;i++) {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256">${body(frames[i],`b${i}`)}</svg>`;
  const png = await sharp(Buffer.from(svg)).png().toBuffer();
  writeFileSync(resolve(preview,`frame-${i+1}.png`),png);
  layer.push({input:png,left:i*256,top:0});
}
await sharp({create:{width:1024,height:256,channels:4,background:'#00000000'}}).composite(layer).png().toFile(resolve(asset,'orange-bot-strip.png'));
writeFileSync(resolve(asset,'orange-bot-animation.json'),JSON.stringify({source:'claude',license:'Apache-2.0',status:'development-preview',frameWidth:256,frameHeight:256,frames:frames.map((f,i)=>({name:f.name,x:i*256,y:0,durationMs:f.durationMs})),loop:true},null,2)+'\n');
const sheet = `<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="320"><rect width="1024" height="320" rx="20" fill="#171717"/>${frames.map((f,i)=>`<g transform="translate(${i*256} 0)">${body(f,`s${i}`)}<text x="128" y="278" text-anchor="middle" fill="#fff6ed" font-family="Segoe UI" font-size="18">${i+1} · ${f.name}</text></g>`).join('')}<text x="512" y="310" text-anchor="middle" fill="#cdb5a7" font-family="Segoe UI" font-size="14">Claude · four-frame orange Bot · 500 / 120 / 180 / 500 ms</text></svg>`;
writeFileSync(resolve(root,'docs/orange-bot-four-frames.svg'),sheet);
console.log('Created transparent 1024x256 four-frame sheet and four preview PNGs.');
