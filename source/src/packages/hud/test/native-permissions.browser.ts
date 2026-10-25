import { after, before, test } from "node:test";
import assert from "node:assert/strict";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { createServer, type ViteDevServer } from "vite";
import { chromium, type Browser, type Page } from "@playwright/test";

// Mount the shipped card, not a parallel rendering of its logic. No account,
// native IPC or real approval is used by this explicit headless browser suite.
const entry = `
import React from 'react';
import {createRoot} from 'react-dom/client';
import {PermissionToolCard} from '/src/permissions/PermissionToolCard.tsx';
const common={id:'fixture',source:'codex',vendor:'Codex',sessionId:'fixture',title:'Fixture',summary:'Fixture',kind:'permission',cwd:null,createdAt:new Date().toISOString(),allowsAlways:false,questions:[],answersQuestion:false,answersPlan:false};
const paths=Array.from({length:32},(_,i)=>(i%2?'写入':'读取')+': C:/fixture/path-'+i+'-with-a-long-readable-name/file.txt');
window.permissionLines=paths;window.decisions=[];
const cards=[
 ['files',{...common,badge:'FILES',toolName:'request_permissions',input:paths.join('\\n'),detail:'仅当前回合；不会授予会话或永久权限。\\nRead the dedicated fixture only.'}],
 ['command',{...common,badge:'CMD',toolName:'exec_command',input:'Get-Content -LiteralPath C:/fixture/approval.txt',detail:'Read only the dedicated test file'}],
 ['legacy',{...common,badge:'WEB',toolName:'access_browser_origin',input:'Legacy fallback input',detail:'Legacy detail'}]
];
createRoot(document.getElementById('root')).render(React.createElement('main',null,...cards.map(([key,request])=>React.createElement('section',{'data-case':key,key,style:{width:380,padding:12,background:'#222',margin:10}},React.createElement(PermissionToolCard,{request,light:false,zh:true,disabled:false,pending:false,answer:d=>window.decisions.push({key,...d})})))));
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
        name: "native-permission-fixture",
        resolveId(id) {
          if (id === "/native-fixture.js") return "\0native-fixture.js";
        },
        load(id) {
          if (id === "\0native-fixture.js") return entry;
        },
        configureServer(owner) {
          owner.middlewares.use((req, res, next) => {
            if (req.url !== "/") return next();
            res.setHeader("Content-Type", "text/html");
            res.end(
              '<div id="root"></div><script type="module" src="/native-fixture.js"></script>',
            );
          });
        },
      },
    ],
  });
  await server.listen();
  const address = server.httpServer!.address();
  if (!address || typeof address === "string")
    throw Error("Fixture has no port");
  browser = await chromium.launch({
    headless: true,
    ...(process.env.AGENTHUD_TEST_CHROMIUM
      ? { executablePath: process.env.AGENTHUD_TEST_CHROMIUM }
      : {}),
  });
  page = await browser.newPage({ viewport: { width: 700, height: 1000 } });
  await page.goto(`http://127.0.0.1:${address.port}/`);
  await page
    .locator('[data-case="files"] [data-native-approval-detail]')
    .waitFor();
});
after(async () => {
  await browser?.close();
  await server?.close();
});

test("every requested path stays readable beyond six lines and reason stays separate", async () => {
  const result = await page
    .locator('[data-case="files"] [data-native-approval-detail]')
    .evaluate((element) => {
      const body = element as HTMLElement,
        style = getComputedStyle(body);
      const expected = (window as unknown as { permissionLines: string[] })
        .permissionLines;
      body.scrollTop = body.scrollHeight;
      return {
        text: body.textContent,
        expected,
        clamp: style.webkitLineClamp,
        overflow: style.overflowY,
        scrollable: body.scrollHeight > body.clientHeight,
        reachedBottom:
          body.scrollTop + body.clientHeight >= body.scrollHeight - 1,
      };
    });
  assert.equal(result.text, result.expected.join("\n"));
  assert.notEqual(result.clamp, "6");
  assert.equal(result.overflow, "auto");
  assert(result.scrollable && result.reachedBottom);
  assert.match(
    await page
      .locator('[data-case="files"] [data-native-approval-scope]')
      .innerText(),
    /仅当前回合/,
  );
  assert.equal(
    await page
      .locator('[data-case="files"]')
      .getByRole("button", { name: "总是允许", exact: true })
      .count(),
    0,
  );
});

test("native command and reason both render while legacy detail preference stays unchanged", async () => {
  assert.equal(
    await page
      .locator('[data-case="command"] [data-native-approval-detail]')
      .innerText(),
    "Get-Content -LiteralPath C:/fixture/approval.txt",
  );
  assert.equal(
    await page
      .locator('[data-case="command"] [data-native-approval-scope]')
      .innerText(),
    "Read only the dedicated test file",
  );
  const legacy = page.locator('[data-case="legacy"]');
  assert(await legacy.getByText("Legacy detail", { exact: true }).isVisible());
  assert.equal(
    await legacy.getByText("Legacy fallback input", { exact: true }).count(),
    0,
  );
  assert.equal(
    await legacy
      .getByText("Legacy detail", { exact: true })
      .evaluate((e) => getComputedStyle(e).webkitLineClamp),
    "6",
  );
  await page
    .locator('[data-case="files"]')
    .getByRole("button", { name: "批准一次", exact: true })
    .click();
  assert.deepEqual(
    await page.evaluate(
      () => (window as unknown as { decisions: object[] }).decisions,
    ),
    [{ key: "files", kind: "allow" }],
  );
});
