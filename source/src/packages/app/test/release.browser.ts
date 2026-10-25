import { before, after, test } from "node:test";
import assert from "node:assert/strict";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { createServer, type ViteDevServer } from "vite";
import { chromium, type Browser, type Page } from "@playwright/test";

let server: ViteDevServer, browser: Browser, page: Page;
const fixture = `
import React from 'react';import {createRoot} from 'react-dom/client';
import {ReleasePane,FirstRunClients} from '/src/settings/ReleasePane.tsx';
const root=createRoot(document.getElementById('root'));
window.calls=0;window.refreshed=0;window.fail=false;window.url=null;
const providers=[{id:'codex',name:'Codex',detected:true,discovery:{installed:true,configured:true,usage:false}},
{id:'claude',name:'Claude',detected:false,dataStatus:'detection-only'}];
window.render=()=>root.render(React.createElement(React.Fragment,null,
React.createElement(FirstRunClients,{providers,chinese:true,onReview:()=>window.reviewed=true}),
React.createElement(ReleasePane,{version:'0.1.1',downloadUrl:window.url,chinese:true,providers,
openDownload:async()=>{window.calls++;if(window.fail)throw Error('Browser unavailable');},refresh:async()=>{window.refreshed++;}})));
window.render();`;
before(async () => {
  server = await createServer({
    configFile: false,
    root: dirname(dirname(fileURLToPath(import.meta.url))),
    optimizeDeps: {
      noDiscovery: true,
      include: [
        "react",
        "react-dom/client",
        "react/jsx-runtime",
        "react/jsx-dev-runtime",
        "radix-ui",
        "lucide-react",
        "@agent-hud/ui > cn",
        "@agent-hud/ui > clsx",
        "@agent-hud/ui > tailwind-merge",
        "@agent-hud/ui > class-variance-authority",
        "@agent-hud/ui > radix-ui",
      ],
    },
    cacheDir: fileURLToPath(
      new URL(
        `../../../../../local-only/netdisk-delivery/vite-cache-${process.pid}`,
        import.meta.url,
      ),
    ),
    server: { host: "127.0.0.1", port: 0, watch: null },
    appType: "custom",
    logLevel: "silent",
    plugins: [
      {
        name: "release-fixture",
        resolveId: (id) => (id === "/fixture.js" ? "\0fixture" : null),
        load: (id) => (id === "\0fixture" ? fixture : null),
        configureServer(s) {
          s.middlewares.use((req, res, next) => {
            if (req.url === "/favicon.ico") {
              res.statusCode = 204;
              res.end();
              return;
            }
            if (req.url !== "/") return next();
            res.setHeader("Content-Type", "text/html; charset=utf-8");
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
  if (!address || typeof address === "string") throw Error("Missing address");
  browser = await chromium.launch({
    headless: true,
    ...(process.env.AGENTHUD_TEST_CHROMIUM
      ? { executablePath: process.env.AGENTHUD_TEST_CHROMIUM }
      : {}),
  });
  page = await browser.newPage();
  page.on("pageerror", (error) => console.error(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") console.error(message.text());
  });
  await page.goto(`http://127.0.0.1:${address.port}`);
  await page.getByRole("heading", { name: "Agent Bot 0.1.1" }).waitFor();
});
after(async () => {
  await browser?.close();
  await server?.close();
});
test("missing channel hides download and does not initiate a check", async () => {
  assert.equal(await page.getByRole("button", { name: "下载新版" }).count(), 0);
  assert.equal(await page.evaluate(() => (window as any).calls), 0);
  assert.match(await page.locator("body").innerText(), /未检测到本地来源/);
  assert.match(await page.locator("body").innerText(), /发现安装 · 发现配置/);
  assert.match(await page.locator("body").innerText(), /仅支持发现/);
});
test("download opens only on explicit click and exposes failure", async () => {
  await page.evaluate(() => {
    (window as any).url = "https://pan.example.org/share";
    (window as any).render();
  });
  await page.getByRole("button", { name: "下载新版" }).click();
  assert.equal(await page.evaluate(() => (window as any).calls), 1);
  await page.evaluate(() => {
    (window as any).fail = true;
  });
  await page.getByRole("button", { name: "下载新版" }).click();
  await page.getByRole("alert").waitFor();
  assert.match(
    await page.getByRole("alert").innerText(),
    /Browser unavailable/,
  );
});
test("refresh does not open the download and clears the previous error", async () => {
  await page.getByRole("button", { name: "重新检测" }).click();
  assert.equal(await page.evaluate(() => (window as any).refreshed), 1);
  assert.equal(await page.evaluate(() => (window as any).calls), 2);
  assert.equal(await page.getByRole("alert").count(), 0);
});
test("welcome is dismissed only by acknowledgment and persists across restart", async () => {
  await page.getByRole("button", { name: "查看客户端设置" }).click();
  assert.equal(await page.evaluate(() => (window as any).reviewed), true);
  assert.equal(
    await page.getByRole("heading", { name: "欢迎使用 Agent Bot" }).count(),
    1,
  );
  await page.getByRole("button", { name: "知道了" }).click();
  await page.reload();
  await page.getByRole("heading", { name: "Agent Bot 0.1.1" }).waitFor();
  assert.equal(
    await page.getByRole("heading", { name: "欢迎使用 Agent Bot" }).count(),
    0,
  );
});
