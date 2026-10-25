import { after, before, test } from "node:test";
import assert from "node:assert/strict";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { createServer, type ViteDevServer } from "vite";
import { chromium, type Browser, type Page } from "@playwright/test";

const entry = `
import React from 'react';
import { createRoot } from 'react-dom/client';
import { BotMark } from '/src/BotMark.tsx';
import { loadClawd, ClawdBaker } from '/src/bots/clawd.ts';
import { makeForm } from '/src/bots/bot-form.ts';
import { loadArtwork } from '/src/bots/artwork.ts';
import { BotMotion } from '/src/bots/bot-motion.ts';
import { renderBot } from '/src/bots/bot-renderer.ts';

const sheet = await loadClawd();
const baker = new ClawdBaker(sheet);
function paint(ctx, frame, scale, extent) {
  // BotMark explicitly passes the configured outline (zero here), unlike the
  // legacy drawClawd helper which derives an outline from all spare space.
  ctx.drawImage(baker.image(frame, 28, scale, extent, false, 0), 0, 0);
}
window.frameResults = [];
const gallery = document.getElementById('gallery');
for (const [name, sprite] of Object.entries(sheet)) {
  const tile = document.createElement('article');
  const title = document.createElement('p'); title.textContent = name;
  tile.append(title); gallery.append(tile);
  for (const scale of [1, 2]) {
    const extent = 64 * scale;
    const canvas = document.createElement('canvas');
    canvas.width = canvas.height = extent;
    canvas.style.width = canvas.style.height = '96px';
    const ctx = canvas.getContext('2d');
    for (let index = 0; index < sprite.frames.length; index++) {
      ctx.clearRect(0, 0, extent, extent);
      paint(ctx, {name, index}, scale, extent);
      const data = ctx.getImageData(0, 0, extent, extent).data;
      let ink = 0, edge = false;
      for (let y = 0; y < extent; y++) for (let x = 0; x < extent; x++) {
        if (!data[(y * extent + x) * 4 + 3]) continue;
        ink++;
        if (x === 0 || y === 0 || x === extent - 1 || y === extent - 1) edge = true;
      }
      window.frameResults.push({name, index, scale, ink, edge});
    }
    ctx.clearRect(0, 0, extent, extent);
    paint(ctx, {name, index: Math.floor(sprite.frames.length / 3)}, scale, extent);
    tile.append(canvas);
  }
}
window.fallbackResults = [];
for (const light of [false, true]) {
  const artwork = await loadArtwork('claude', light);
  const form = makeForm('claude', artwork, light);
  const motion = new BotMotion();
  motion.fit(form.spec); motion.morph = 1;
  const image = renderBot(form, motion, 32, 0);
  image.style.width = image.style.height = '120px';
  const tile = document.createElement('div');
  tile.className = light ? 'fallback light' : 'fallback';
  const label = document.createElement('p'); label.textContent = light ? '浅色 / 小尺寸备用' : '深色 / 小尺寸备用';
  tile.append(label, image); document.getElementById('fallbacks').append(tile);
  const bytes = image.getContext('2d').getImageData(0, 0, image.width, image.height).data;
  let teal = 0; for (let i = 0; i < bytes.length; i += 4)
    if (bytes[i+3] && bytes[i+1] > bytes[i] + 50 && bytes[i+2] > bytes[i] + 40) teal++;
  window.fallbackResults.push({light, teal});
}
createRoot(document.getElementById('live')).render(React.createElement(React.Fragment, null,
  ...['idle', 'working', 'asking', 'done'].map(state => React.createElement('div', {className: 'state', key: state},
    React.createElement('p', null, {idle:'待命',working:'工作',asking:'等待确认',done:'完成'}[state]),
    React.createElement(BotMark, {source:'claude', size:64, state, staysBot:true, enabled:true, awake:true})
  ))
));
window.seedReady = true;
`;
const html = `<!doctype html><html lang="zh-CN"><meta charset="utf-8"><style>
body{margin:0;background:#101c22;color:#d9eee9;font:16px 'Segoe UI',sans-serif;padding:36px}
header{max-width:1080px;padding:30px 42px;background:#182b32;border:1px solid #314a50;border-radius:28px}
h1{font-size:32px;margin:0 0 10px}header>p{color:#9fb7b8}.tag{color:#d9ec7b;font-size:13px;letter-spacing:3px}
#live{display:flex;justify-content:space-around;min-height:165px;padding-top:5px}
.state{width:150px;display:flex;align-items:center;flex-direction:column;gap:20px}.state p{font-size:14px;color:#9fb7b8}
#fallbacks{display:flex;border-top:1px solid #314a50;margin-top:20px;padding-top:20px;gap:20px}
.fallback{display:flex;align-items:center;justify-content:space-around;width:50%;border-radius:16px;background:#102026}
.fallback.light{background:#f0f6ee;color:#173a3c}.fallback p{font-size:13px}
#gallery{display:grid;grid-template-columns:repeat(6,1fr);gap:10px;max-width:1160px;margin-top:25px}
article{background:#182b32;border-radius:16px;padding:10px;text-align:center}article p{font-size:12px}canvas{image-rendering:pixelated}
</style><header data-preview-card><div class="tag">AGENT HUB · ORIGINAL CHARACTER</div><h1>Sprout / 芽芽</h1><p>青绿色胶囊机身 · 嫩芽天线 · 两只小脚 · 原创像素动画</p><div id="live"></div><div id="fallbacks"></div></header><main id="gallery"></main><script type="module" src="/seed-fixture.js"></script></html>`;
let server: ViteDevServer;
let browser: Browser;
let page: Page;
const errors: string[] = [];
const spriteRequests: string[] = [];
before(async () => {
  server = await createServer({
    configFile: false,
    // Other browser suites serve this workspace concurrently. Their optimized
    // dependencies must not replace this fixture's React module graph.
    cacheDir: fileURLToPath(
      new URL("../node_modules/.vite-seed-robot", import.meta.url),
    ),
    root: dirname(dirname(fileURLToPath(import.meta.url))),
    server: { host: "127.0.0.1", port: 0, watch: null },
    appType: "custom",
    logLevel: "silent",
    plugins: [
      {
        name: "seed-robot-test",
        resolveId(id) {
          if (id === "/seed-fixture.js") return "\0seed-fixture.js";
        },
        load(id) {
          if (id === "\0seed-fixture.js") return entry;
        },
        configureServer(owner) {
          owner.middlewares.use((req, res, next) => {
            if (req.url !== "/") return next();
            res.setHeader("Content-Type", "text/html; charset=utf-8");
            res.end(html);
          });
        },
      },
    ],
  });
  await server.listen();
  const address = server.httpServer!.address();
  if (!address || typeof address === "string") throw new Error("No local port");
  browser = await chromium.launch({
    headless: true,
    ...(process.env.AGENTHUD_TEST_CHROMIUM
      ? { executablePath: process.env.AGENTHUD_TEST_CHROMIUM }
      : {}),
  });
  page = await browser.newPage({
    viewport: { width: 1240, height: 1000 },
    reducedMotion: "no-preference",
  });
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("request", (request) => {
    if (request.url().includes("clawd-sprites"))
      spriteRequests.push(request.url());
  });
  await page.goto(`http://127.0.0.1:${address.port}/`);
  try {
    await page.waitForFunction(
      () => (window as unknown as { seedReady: boolean }).seedReady === true,
    );
  } catch (error) {
    throw new Error(`Sprout fixture failed to load: ${errors.join("; ")}`, {
      cause: error,
    });
  }
  await page.waitForFunction(
    () =>
      Array.from(document.querySelectorAll<HTMLCanvasElement>("#live canvas"))
        .length === 4 &&
      Array.from(
        document.querySelectorAll<HTMLCanvasElement>("#live canvas"),
      ).every((c) => c.style.visibility !== "hidden"),
  );
});
after(async () => {
  await browser?.close();
  await server?.close();
});

test("all pixel frames render without clipping at 1x and 2x", async () => {
  const frames = await page.evaluate(
    () =>
      (
        window as unknown as {
          frameResults: {
            name: string;
            index: number;
            scale: number;
            ink: number;
            edge: boolean;
          }[];
        }
      ).frameResults,
  );
  assert(frames.length > 1000);
  assert.deepEqual(
    frames.filter((frame) => !frame.ink || frame.edge),
    [],
  );
  assert.deepEqual(spriteRequests, []);
  assert.deepEqual(errors, []);
});
test("real BotMark and both fallback themes render the original character", async () => {
  const fallback = await page.evaluate(
    () =>
      (
        window as unknown as {
          fallbackResults: { light: boolean; teal: number }[];
        }
      ).fallbackResults,
  );
  assert.equal(fallback.length, 2);
  assert(fallback.every((frame) => frame.teal > 10));
  assert.equal(await page.locator("#live canvas").count(), 4);
  // Let the product's initial logo-to-character crossfade settle before export.
  // This waits for rendering only and never touches native app settings.
  await page.waitForTimeout(650);
  if (process.env.AGENTHUB_ART_PREVIEW)
    await page.locator("[data-preview-card]").screenshot({
      path: process.env.AGENTHUB_ART_PREVIEW,
      animations: "disabled",
    });
  assert.deepEqual(errors, []);
});
