import { after, before, test } from "node:test";
import assert from "node:assert/strict";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { createServer, type ViteDevServer } from "vite";
import { chromium, type Browser, type Page } from "@playwright/test";

// Real React consumer. The transport fixture represents the independently
// tested native inbox; it deliberately rejects writes instead of pretending ack.
const entry = `
import React from 'react';
import {createRoot} from 'react-dom/client';
import {useIslandNews} from '/src/IslandNews.tsx';
import {PanelServicesProvider} from '/src/PanelContext.tsx';
import {HUDScene} from '/src/HUDScene.tsx';
import {defaultHUDConfig} from '/src/model.ts';
const root=createRoot(document.getElementById('root'));
window.pool=[];window.rejectAck=false;window.failRead=false;window.calls=[];window.dropped=0;
window.options={notifications:true,drops:false,animations:false,revision:1,account:'a'};
window.snapshot={schemaVersion:1,generatedAt:new Date().toISOString(),mode:'demo',language:'en',
 connection:{},settings:{animations:false,language:'en',appearance:'dark'},providers:[{id:'codex',name:'Codex',enabled:true,detected:true}],quotas:[],balances:[],accounts:[{provider:'codex',id:'a'}],
 billing:[],sessions:[],timeline:[],totals:{input:0,output:0,cacheRead:0,cacheWrite:0,reasoning:0,total:0,cost:0},approvals:[],capabilities:[],islandEvents:{revision:1,items:[]}};
const backend=async params=>{window.calls.push(params);if(window.failRead)throw Error('Read failure');
 if(params.operation==='navigate'){return {items:[],error:null,dropped:window.dropped,opened:!window.rejectNavigation};}
 if(params.operation==='dismiss'){if(window.rejectAck)throw Error('Disk full');if(window.holdAck)await new Promise(resolve=>window.finishAck=resolve);window.pool=window.pool.filter(e=>!params.ids.includes(e.id));}
 return {items:params.operation==='release'?[]:structuredClone(window.pool),error:null,dropped:window.dropped};};
const services={query:async()=>null,completionInbox:backend};
function Harness(){const o=window.options;const snap={...window.snapshot,settings:{...window.snapshot.settings,animations:o.animations},accounts:[{provider:'codex',id:o.account}],islandEvents:{revision:o.revision,items:[]}};
 const news=useIslandNews(snap,false,[],['openai'],false,true,false,o.notifications);
 window.news=news;
 return React.createElement('div',null,React.createElement('output',{id:'queue'},JSON.stringify(news.queue.map(c=>c.id))),React.createElement('output',{id:'error'},news.persistenceError||''),
 React.createElement('button',{id:'dismiss',onClick:()=>news.dismissID(news.event.id)},'Dismiss'),React.createElement('button',{id:'all',onClick:news.dismissCompletions},'All'),
 React.createElement(HUDScene,{snapshot:snap,config:{...defaultHUDConfig,mode:'logos',animations:o.animations,completionNotifications:o.notifications,completionDrops:o.drops},height:700}));}
window.mount=()=>root.render(React.createElement(PanelServicesProvider,{value:services},React.createElement(Harness)));
window.unmount=()=>root.render(null);
window.publish=(patch={})=>{Object.assign(window.options,patch);window.options.revision++;window.mount();};
window.add=(id)=>{window.pool.push({id,kind:'completion',source:'codex',sessionId:'session-'+id,turnId:'turn-'+id,at:new Date().toISOString(),title:'Fixture reply',navigationTarget:{kind:'codex-thread',id:'thread-'+id}});window.publish();};
window.mount();
`;
let server: ViteDevServer, browser: Browser, page: Page;
before(async () => {
  server = await createServer({
    configFile: false,
    optimizeDeps: {
      noDiscovery: true,
      include: [
        "react",
        "react-dom",
        "react-dom/client",
        "react/jsx-runtime",
        "react/jsx-dev-runtime",
        "motion/react",
        "lucide-react",
        "shaders",
        "@agent-hud/ui > cn",
        "@agent-hud/ui > clsx",
        "@agent-hud/ui > tailwind-merge",
        "@agent-hud/ui > class-variance-authority",
        "@agent-hud/ui > radix-ui",
      ],
    },
    cacheDir: fileURLToPath(
      new URL(
        `../../../../../local-only/completion-reliability-20261025/vite-cache-${process.pid}`,
        import.meta.url,
      ),
    ),
    root: dirname(dirname(fileURLToPath(import.meta.url))),
    server: { host: "127.0.0.1", port: 0, watch: null },
    appType: "custom",
    logLevel: "silent",
    plugins: [
      {
        name: "completion-inbox-fixture",
        resolveId(id) {
          if (id === "/fixture.js") return "\0fixture.js";
        },
        load(id) {
          if (id === "\0fixture.js") return entry;
        },
        configureServer(owner) {
          owner.middlewares.use((req, res, next) => {
            if (req.url === "/favicon.ico") {
              res.statusCode = 204;
              res.end();
              return;
            }
            if (req.url !== "/") return next();
            res.setHeader("Content-Type", "text/html");
            res.end(
              '<div id="root"></div><script type="module" src="/fixture.js"></script>',
            );
          });
        },
      },
    ],
  });
  await server.listen();
  const address = server.httpServer!.address();
  if (!address || typeof address === "string") throw Error("Missing port");
  browser = await chromium.launch({
    headless: true,
    ...(process.env.AGENTHUD_TEST_CHROMIUM
      ? { executablePath: process.env.AGENTHUD_TEST_CHROMIUM }
      : {}),
  });
  page = await browser.newPage();
  page.on("pageerror", (error) =>
    console.error("Fixture page error:", error.message),
  );
  page.on("console", (message) => {
    if (message.type() === "error")
      console.error("Fixture console:", message.text());
  });
  page.on("requestfailed", (request) =>
    console.error(
      "Fixture request:",
      request.url(),
      request.failure()?.errorText,
    ),
  );
  await page.goto("http://127.0.0.1:" + address.port + "/");
  try {
    await page.locator("#queue").waitFor({ timeout: 30000 });
  } catch (e) {
    console.error((await page.content()).slice(-2500));
    throw e;
  }
});
after(async () => {
  await browser?.close();
  await server?.close();
});
async function call(code: string) {
  await page.evaluate(code);
}
async function queued(ids: string[]) {
  await page.waitForFunction(
    (expected) =>
      document.querySelector("#queue")?.textContent ===
      JSON.stringify(expected),
    ids,
  );
}
test("the real hook restores on remount, persists dismissals, and leaves later replies alone", async () => {
  await call('window.add("one")');
  await queued(["one"]);
  await call("window.unmount()");
  await page.locator("#queue").waitFor({ state: "detached" });
  await call("window.mount()");
  await queued(["one"]);
  await page.locator("#dismiss").click();
  await queued([]);
  await call("window.unmount()");
  await page.locator("#queue").waitFor({ state: "detached" });
  await call("window.mount()");
  await queued([]);
  await call('window.add("two");window.add("three")');
  await queued(["two", "three"]);
  await page.locator("#all").click();
  await queued([]);
  await call('window.add("later")');
  await queued(["later"]);
});
test("acknowledgement failure keeps the unread card and reports the storage failure", async () => {
  await call("window.rejectAck=true");
  await page.locator("#dismiss").click();
  await queued(["later"]);
  await page.waitForFunction(
    () => document.querySelector("#error")!.textContent!.length > 0,
  );
  assert.match(await page.locator("#error").innerText(), /could not be saved/);
  await call("window.rejectAck=false");
  await page.locator("#dismiss").click();
  await queued([]);
});
test("drop animation off keeps completion cards without a pet; delivery off hides all completion cards", async () => {
  await call('window.add("quiet")');
  await queued(["quiet"]);
  await page.waitForFunction(
    () => document.querySelectorAll("article").length > 0,
  );
  assert.equal(await page.locator(".hud-completion-pet").count(), 0);
  await call("window.publish({notifications:false})");
  await queued([]);
  await call("window.publish({notifications:true,drops:true})");
  await queued(["quiet"]);
  await page.waitForFunction(
    () => document.querySelectorAll(".hud-completion-pet").length > 0,
  );
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.waitForTimeout(100);
  assert.ok((await page.locator("article").count()) > 0);
});
test("real HUD returns the dismissed Bot after durable ack and a delayed bulk ack leaves new replies alone", async () => {
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await call(
    "window.pool=[];window.publish({notifications:true,drops:true,animations:true})",
  );
  await queued([]);
  await call('window.add("flying")');
  await queued(["flying"]);
  await page
    .getByRole("button", { name: "Dismiss reply", exact: true })
    .waitFor({ state: "visible" });
  await call("window.rejectAck=true");
  await page
    .getByRole("button", { name: "Dismiss reply", exact: true })
    .click();
  assert.equal(await page.locator(".hud-completion-return").count(), 0);
  await call("window.rejectAck=false");
  await page
    .getByRole("button", { name: "Dismiss reply", exact: true })
    .click();
  await page.waitForFunction(
    () => document.querySelectorAll(".hud-completion-return").length === 1,
  );
  await page.waitForFunction(() => !(window as any).pool.length);
  await page.waitForFunction(
    () => document.querySelectorAll(".hud-completion-return").length === 0,
  );
  await call('window.add("bulk-old")');
  await page
    .getByRole("button", { name: "Dismiss reply", exact: true })
    .waitFor({ state: "visible" });
  await call('window.add("queued-old");window.holdAck=true');
  await queued(["bulk-old", "queued-old"]);
  await page.getByRole("button", { name: /Dismiss all/ }).click();
  await page.waitForFunction(
    () => typeof (window as any).finishAck === "function",
  );
  await call(
    'window.add("arrived-during-ack");window.holdAck=false;window.finishAck()',
  );
  await page.waitForFunction(
    () =>
      JSON.stringify((window as any).pool.map((e: any) => e.id)) ===
      JSON.stringify(["arrived-during-ack"]),
  );
  await queued(["arrived-during-ack"]);
});
test("retention notices are distinct from save failures and use Chinese retention limits", async () => {
  await call(
    'window.snapshot.language="zh";window.dropped=44;window.publish({animations:false,drops:false})',
  );
  const notice = page.getByRole("alert").filter({ hasText: "256" }).first();
  await notice.waitFor({ state: "visible" });
  assert.equal(await page.locator("#error").innerText(), "");
  assert.match(await notice.innerText(), /7 天.*256 条/);
  assert.doesNotMatch(await notice.innerText(), /未能保存/);
  await call('window.dropped=0;window.snapshot.language="en";window.publish()');
});
test("a restored safe destination uses inbox navigation, keeps the card on failure and acknowledges success", async () => {
  await call(
    "window.pool=[];window.dropped=0;window.snapshot.language='en';window.publish({animations:false,drops:false})",
  );
  await queued([]);
  await call(
    'window.add("restored-safe");window.pool[0].reminderNavigation=true;window.publish();window.rejectNavigation=true',
  );
  await queued(["restored-safe"]);
  await page.getByRole("button", { name: /Return to session/ }).click();
  await queued(["restored-safe"]);
  assert.equal(
    await page.evaluate(
      () =>
        (window as any).calls
          .filter((c: any) => c.operation === "navigate")
          .at(-1).id,
    ),
    "restored-safe",
  );
  await call("window.rejectNavigation=false");
  await page.getByRole("button", { name: /Retry session/ }).click();
  await queued([]);
  assert.ok(
    await page.evaluate(() =>
      (window as any).calls.some(
        (c: any) =>
          c.operation === "dismiss" && c.ids.includes("restored-safe"),
      ),
    ),
  );
});
