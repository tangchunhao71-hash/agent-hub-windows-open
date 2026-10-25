import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createServer } from "node:http";
import vm from "node:vm";
import { createBridge } from "./index.js";

async function fixture(t, timeout = 300) {
  const directory = await mkdtemp(join(tmpdir(), "hud-dsh-test-"));
  const bridge = await createBridge({ directory, timeout });
  const client = createServer(bridge.client);
  await new Promise((resolve) => client.listen(0, "127.0.0.1", resolve));
  t.after(async () => {
    await bridge.close();
    await new Promise((resolve) => client.close(resolve));
    await rm(directory, { recursive: true });
  });
  const base = `http://127.0.0.1:${bridge.descriptor.port}`;
  const browser = `http://127.0.0.1:${client.address().port}`;
  const native = (path, body, headers = {}) =>
    fetch(base + path, {
      method: body ? "POST" : "GET",
      headers: {
        authorization: `Bearer ${bridge.descriptor.token}`,
        ...headers,
      },
      body: body ? JSON.stringify(body) : undefined,
    });
  const poll = (body, headers = {}) =>
    fetch(browser, {
      method: "POST",
      headers: { "x-agent-hub-client": "1", ...headers },
      body: JSON.stringify({ desktop: true, selected: null, ...body }),
    });
  return { directory, bridge, native, poll };
}
test("native open waits for exact desktop acknowledgement, and rejects stale/wrong receipts", async (t) => {
  const f = await fixture(t);
  await f.poll({});
  const open = f.native("/open", { requestId: "r1", sessionId: "s1" });
  let pending;
  for (let i = 0; i < 20 && !pending; i++) {
    pending = (await (await f.poll({})).json()).pending;
    if (!pending) await new Promise((r) => setTimeout(r, 5));
  }
  assert.deepEqual(pending, { requestId: "r1", sessionId: "s1" });
  assert.equal(
    (await f.native("/open", { requestId: "r2", sessionId: "s2" })).status,
    409,
  );
  await f.poll({ requestId: "stale", selected: "s1", ok: true });
  assert.equal((await (await f.native("/status")).json()).selected, "s1");
  await f.poll({ requestId: "r1", selected: "s1", ok: true });
  assert.deepEqual(await (await open).json(), {
    ok: true,
    requestId: "r1",
    sessionId: "s1",
  });
  const wrong = f.native("/open", { requestId: "r3", sessionId: "s3" });
  await new Promise((r) => setTimeout(r, 10));
  await f.poll({ requestId: "r3", selected: "s1", ok: true });
  assert.equal((await wrong).status, 504);
});
test("unavailable desktop, timeouts, origins, secrets, arbitrary commands and oversized input fail closed", async (t) => {
  const f = await fixture(t, 50);
  assert.equal(
    (await f.native("/open", { requestId: "r", sessionId: "s" })).status,
    503,
  );
  assert.equal(
    (await f.native("/status", null, { authorization: "Bearer wrong" })).status,
    403,
  );
  assert.equal(
    (await f.native("/status", null, { origin: "https://example.com" })).status,
    403,
  );
  assert.equal(
    (await f.poll({}, { origin: "https://example.com" })).status,
    403,
  );
  assert.equal(
    (await f.poll({}, { "sec-fetch-site": "cross-site" })).status,
    403,
  );
  await f.poll({});
  for (const sessionId of ["../s", "s?url=evil", "s\n", "x".repeat(2000)]) {
    assert.equal(
      (await f.native("/open", { requestId: "r", sessionId })).status,
      400,
    );
  }
  assert.equal(
    (
      await f.native("/open", {
        requestId: "r",
        sessionId: "s",
        command: "evil",
      })
    ).status,
    400,
  );
  assert.equal(
    (await f.native("/open", { requestId: "r", sessionId: "s" })).status,
    504,
  );
  const text = JSON.stringify(await (await f.native("/status")).json());
  assert.ok(!text.includes(f.bridge.descriptor.token));
});
test("actual browser plugin selects through uiWorkspace and checks opened state before ACK", async () => {
  const source = await readFile(
    new URL("./client.js", import.meta.url),
    "utf8",
  );
  let plugin,
    selected = "old",
    disposed,
    ack;
  const rows = {
    old: { id: "old", retainedBy: { mainView: 1 } },
    target: { id: "target", retainedBy: {} },
  };
  const requests = [];
  const context = {
    window: {
      location: { protocol: "dsh-app:" },
      __ModuleLoader__: {
        load: (value) => {
          plugin = value.factory();
        },
      },
    },
    AbortSignal,
    setTimeout,
    clearTimeout,
    fetch: async (_url, options) => {
      const body = JSON.parse(options.body);
      requests.push(body);
      if (body.requestId) {
        ack = body;
        disposed();
        return { ok: true, json: async () => ({ pending: null }) };
      }
      return {
        ok: true,
        json: async () => ({
          pending: { requestId: "r", sessionId: "target" },
        }),
      };
    },
  };
  vm.runInNewContext(source, context);
  plugin.apply({
    sessions: {
      list: { getSnapshot: () => ({ byId: rows }) },
      binding: () => ({
        session: { getSnapshot: () => ({ openState: "open" }) },
      }),
    },
    uiWorkspace: {
      openSession(id) {
        selected = id;
        rows.old.retainedBy.mainView = 0;
        rows.target.retainedBy.mainView = 1;
      },
    },
    effect(fn) {
      disposed = fn();
    },
  });
  for (let i = 0; i < 20 && !ack; i++)
    await new Promise((r) => setTimeout(r, 5));
  assert.equal(selected, "target");
  assert.equal(ack.ok, true);
  assert.equal(ack.selected, "target");
  assert.deepEqual(requests.map((r) => r.requestId).filter(Boolean), ["r"]);
  let touched = false;
  context.window.location.protocol = "http:";
  plugin.apply({
    effect() {
      touched = true;
    },
  });
  assert.equal(
    touched,
    false,
    "ordinary browser tabs must not consume a desktop request",
  );
});
