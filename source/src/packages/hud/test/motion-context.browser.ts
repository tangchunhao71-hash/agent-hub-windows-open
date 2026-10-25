import { after, before, test } from "node:test";
import assert from "node:assert/strict";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { createServer, type ViteDevServer } from "vite";
import { chromium, type Browser, type Page } from "@playwright/test";

// Explicit browser suite: exercise the real React consumer, Canvas renderer and its timer loop. The
// fixture supplies settings only; it never opens or mutates native user config.
const entry = `
import React from 'react';
import {createRoot} from 'react-dom/client';
import {BotMark} from '/src/BotMark.tsx';
import {BotActor} from '/src/bots/bot-actor.ts';
import {HUDMotionContext} from '/src/motion-context.tsx';
window.motionStats={steps:0};
const step=BotActor.prototype.step;
BotActor.prototype.step=function(...args){window.motionStats.steps++;return step.apply(this,args);};
const root=createRoot(document.getElementById('root'));
window.renderMotion=({reduced=false,enabled=true,standalone=false}={})=>{
  const marks=React.createElement('div',{'data-reduced':String(reduced),'data-enabled':String(enabled),'data-standalone':String(standalone)},
    ...['dock','reminder','drop'].map(location=>React.createElement('section',{key:location,'data-location':location},
      React.createElement(BotMark,{source:'WorkBuddy',size:42,state:'working',enabled,staysBot:true}))));
  root.render(standalone?marks:React.createElement(HUDMotionContext.Provider,{value:reduced},marks));
};
window.renderMotion({reduced:true});
`;

let server: ViteDevServer;
let browser: Browser;
let page: Page;

before(async () => {
  server = await createServer({
    configFile: false,
    root: dirname(dirname(fileURLToPath(import.meta.url))),
    server: { host: "127.0.0.1", port: 0, watch: null },
    appType: "custom",
    logLevel: "silent",
    plugins: [
      {
        name: "hud-motion-test-fixture",
        resolveId(id) {
          if (id === "/motion-fixture.js") return "\0motion-fixture.js";
        },
        load(id) {
          if (id === "\0motion-fixture.js") return entry;
        },
        configureServer(owner) {
          owner.middlewares.use((req, res, next) => {
            if (req.url !== "/") return next();
            res.setHeader("Content-Type", "text/html");
            res.end(
              '<div id="root"></div><script type="module" src="/motion-fixture.js"></script>',
            );
          });
        },
      },
    ],
  });
  await server.listen();
  const address = server.httpServer!.address();
  if (!address || typeof address === "string")
    throw new Error("Missing fixture port");
  browser = await chromium.launch({
    headless: true,
    ...(process.env.AGENTHUD_TEST_CHROMIUM
      ? { executablePath: process.env.AGENTHUD_TEST_CHROMIUM }
      : {}),
  });
  page = await browser.newPage({ reducedMotion: "no-preference" });
  await page.goto(`http://127.0.0.1:${address.port}/`);
  await page.waitForFunction(
    () => document.querySelectorAll("[data-bot-source] canvas").length === 3,
  );
  await page.waitForFunction(() =>
    Array.from(
      document.querySelectorAll<HTMLCanvasElement>("[data-bot-source] canvas"),
    ).every((canvas) => canvas.style.visibility !== "hidden"),
  );
});

after(async () => {
  await browser?.close();
  await server?.close();
});

async function render(reduced: boolean, enabled = true, standalone = false) {
  await page.evaluate(
    ({ reduced, enabled, standalone }) => {
      (
        window as unknown as { renderMotion: (value: object) => void }
      ).renderMotion({ reduced, enabled, standalone });
    },
    { reduced, enabled, standalone },
  );
  await page.waitForFunction(
    ({ reduced, enabled, standalone }) => {
      const fixture = document.querySelector("[data-reduced]");
      return (
        fixture?.getAttribute("data-reduced") === String(reduced) &&
        fixture.getAttribute("data-enabled") === String(enabled) &&
        fixture.getAttribute("data-standalone") === String(standalone)
      );
    },
    { reduced, enabled, standalone },
  );
  // Context commit precedes its effect; let that effect paint the still frame.
  await page.waitForTimeout(60);
}

async function steps() {
  return page.evaluate(
    () =>
      (window as unknown as { motionStats: { steps: number } }).motionStats
        .steps,
  );
}

async function assertStill() {
  const before = await steps();
  await page.waitForTimeout(250);
  assert.equal(
    await steps(),
    before,
    "the renderer must not advance any BotActor while reduced",
  );
}

async function assertMoving() {
  const before = await steps();
  await page.waitForFunction(
    (before) =>
      (window as unknown as { motionStats: { steps: number } }).motionStats
        .steps >
      before + 3,
    before,
  );
}

test("HUD context stops all descendant marks and live toggling resumes and stops their actors", async () => {
  await assertStill();
  for (const location of ["dock", "reminder", "drop"]) {
    assert.equal(
      await page.locator(`[data-location="${location}"] canvas`).count(),
      1,
    );
  }
  await render(false);
  await assertMoving();
  await render(true);
  await assertStill();
  await page.locator("[data-bot-source]").first().hover();
  await assertStill();
});

test("system reduced motion and mark enabled=false each remain effective", async () => {
  await render(false);
  await assertMoving();
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.waitForTimeout(60);
  await assertStill();
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await assertMoving();
  await render(false, false);
  await assertStill();
});

test("standalone marks keep their existing animation behavior without a provider", async () => {
  await render(false, true, true);
  await assertMoving();
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.waitForTimeout(60);
  await assertStill();
});
