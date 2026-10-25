import test from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { AgentHUDSessionObserver } from "./observer.js";

async function fixture(t) {
  const directory = mkdtempSync(join(tmpdir(), "agenthud-observer-"));
  const old = process.env.XDG_DATA_HOME;
  process.env.XDG_DATA_HOME = directory;
  t.after(() => {
    if (old === undefined) delete process.env.XDG_DATA_HOME;
    else process.env.XDG_DATA_HOME = old;
    rmSync(directory, { recursive: true, force: true });
  });
  const observer = await AgentHUDSessionObserver({ directory: "/fixture" });
  const send = (type, properties) => observer.event({ event: { type, properties } });
  const records = () => {
    try {
      return readdirSync(join(directory, "opencode", "agent-hud", "turns"))
        .filter(name => name.endsWith(".json"))
        .map(name => JSON.parse(readFileSync(join(directory, "opencode", "agent-hud", "turns", name), "utf8")));
    } catch { return []; }
  };
  return { send, records };
}

test("busy is published before a reply and explicit idle restores it", async t => {
  const { send, records } = await fixture(t);
  await send("session.status", { sessionID: "s", status: { type: "busy" } });
  assert.equal(records()[0].state, "running");
  assert.ok(records()[0].turnID);
  assert.ok(records()[0].observedAtMs >= records()[0].startedAtMs);
  const turn = records()[0].turnID;
  await send("session.idle", { sessionID: "s" });
  assert.equal(records()[0].state, "ended");
  assert.equal(records()[0].turnID, turn);
});

test("only a closed successful reply matching the newest prompt completes", async t => {
  const { send, records } = await fixture(t);
  await send("message.updated", { info: { id: "p", sessionID: "s", role: "user", time: { created: 10 } } });
  await send("session.status", { sessionID: "s", status: { type: "busy" } });
  const turn = records()[0].turnID;
  const created = Date.now();
  await send("message.updated", { info: { id: "r", parentID: "p", sessionID: "s", role: "assistant", finish: "stop", time: { created, completed: created + 1 } } });
  assert.equal(records()[0].state, "running", "a closed message cannot stop a busy session with more tools");
  await send("session.status", { sessionID: "s", status: { type: "idle" } });
  assert.equal(records()[0].state, "completed");
  assert.equal(records()[0].turnID, turn);
});

test("error and aborted replies stop work without claiming success", async t => {
  const { send, records } = await fixture(t);
  for (const [sessionID, reply] of [
    ["error", { finish: "stop", error: { name: "APIError" } }],
    ["abort", { error: { name: "MessageAbortedError" } }],
    ["tools", { finish: "tool-calls" }],
  ]) {
    await send("session.status", { sessionID, status: { type: "busy" } });
    const created = Date.now();
    await send("message.updated", { info: { id: "r", parentID: "p", sessionID, role: "assistant", ...reply, time: { created, completed: created + 1 } } });
    await send("session.idle", { sessionID });
  }
  assert.deepEqual(records().map(row => row.state), ["ended", "ended", "ended"]);
  const bySession = Object.fromEntries(records().map(row => [row.sessionID, row]));
  assert.equal(bySession["opencode:error"].stopReason, "error");
  assert.equal(bySession["opencode:abort"].stopReason, "aborted");
});

test("idle before reply closure ends work then preserves explicit completion", async t => {
  const { send, records } = await fixture(t);
  await send("session.status", { sessionID: "s", status: { type: "busy" } });
  const created = Date.now();
  await send("message.updated", { info: { id: "r", parentID: "p", sessionID: "s", role: "assistant", time: { created } } });
  await send("session.idle", { sessionID: "s" });
  assert.equal(records()[0].state, "ended");
  await send("message.updated", { info: { id: "r", parentID: "p", sessionID: "s", role: "assistant", finish: "stop", time: { created, completed: created + 1 } } });
  assert.equal(records()[0].state, "completed");
});

test("deleted and child sessions do not retain running observations", async t => {
  const { send, records } = await fixture(t);
  await send("session.created", { info: { id: "child", parentID: "parent" } });
  await send("session.status", { sessionID: "child", status: { type: "busy" } });
  assert.equal(records().length, 0);
  await send("session.deleted", { info: { id: "child" } });
  await send("session.status", { sessionID: "parent", status: { type: "busy" } });
  assert.equal(records()[0].state, "running");
  await send("session.deleted", { info: { id: "parent" } });
  assert.equal(records().length, 0);
});

test("busy heartbeats retain identity and stop immediately on idle", async t => {
  t.mock.timers.enable({ apis: ["setInterval", "Date"], now: 1_792_901_000_000 });
  const { send, records } = await fixture(t);
  await send("session.status", { sessionID: "s", status: { type: "busy" } });
  const first = records()[0];
  t.mock.timers.tick(15000);
  assert.equal(records()[0].state, "running");
  assert.equal(records()[0].turnID, first.turnID);
  assert.equal(records()[0].observedAtMs, first.observedAtMs + 15000);
  await send("session.idle", { sessionID: "s" });
  const ended = records()[0];
  t.mock.timers.tick(30000);
  assert.deepEqual(records()[0], ended, "idle must not be revived by the heartbeat");
});

test("historical completed reply cannot complete a later busy stretch", async t => {
  t.mock.timers.enable({ apis: ["Date"], now: 1000 });
  const { send, records } = await fixture(t);
  await send("message.updated", { info: { id: "old-p", sessionID: "s", role: "user", time: { created: 1000 } } });
  await send("message.updated", { info: { id: "old-r", parentID: "old-p", sessionID: "s", role: "assistant", finish: "stop", time: { created: 1100, completed: 1200 } } });
  t.mock.timers.tick(4000);
  await send("session.status", { sessionID: "s", status: { type: "busy" } });
  t.mock.timers.tick(100);
  await send("session.idle", { sessionID: "s" });
  assert.equal(records()[0].state, "ended");
});

test("replayed old messages during busy cannot establish completion", async t => {
  t.mock.timers.enable({ apis: ["Date"], now: 5000 });
  const { send, records } = await fixture(t);
  const old = { id: "old-r", parentID: "old-p", sessionID: "s", role: "assistant", finish: "stop", time: { created: 5000, completed: 5000 } };
  await send("message.updated", { info: old });
  await send("session.status", { sessionID: "s", status: { type: "busy" } });
  await send("message.updated", { info: old });
  await send("message.updated", { info: { ...old, id: "previously-unseen-old", time: { created: 1100, completed: 1200 } } });
  await send("session.idle", { sessionID: "s" });
  assert.equal(records()[0].state, "ended", "even same-clock replay of an old known id is historical");
});

test("new busy stretch cannot adopt a pending closure from the preceding stretch", async t => {
  t.mock.timers.enable({ apis: ["Date"], now: 5000 });
  const { send, records } = await fixture(t);
  await send("message.updated", { info: { id: "p1", sessionID: "s", role: "user", time: { created: 5000 } } });
  await send("session.status", { sessionID: "s", status: { type: "busy" } });
  await send("message.updated", { info: { id: "r1", parentID: "p1", sessionID: "s", role: "assistant", time: { created: 5000 } } });
  const oldTurn = records()[0].turnID;
  await send("session.idle", { sessionID: "s" });
  t.mock.timers.tick(100);
  await send("message.updated", { info: { id: "p2", sessionID: "s", role: "user", time: { created: 5100 } } });
  await send("session.status", { sessionID: "s", status: { type: "busy" } });
  await send("message.updated", { info: { id: "r1", parentID: "p1", sessionID: "s", role: "assistant", finish: "stop", time: { created: 5000, completed: 5200 } } });
  await send("message.updated", { info: { id: "r2", parentID: "p2", sessionID: "s", role: "assistant", finish: "stop", time: { created: 5100, completed: 5200 } } });
  await send("session.idle", { sessionID: "s" });
  const prior = records().find(row => row.turnID === oldTurn);
  const current = records().find(row => row.turnID !== oldTurn);
  assert.equal(prior.state, "ended");
  assert.equal(current.state, "completed");
});

test("repeated busy notifications keep the current valid reply bound", async t => {
  t.mock.timers.enable({ apis: ["Date"], now: 5000 });
  const { send, records } = await fixture(t);
  await send("session.status", { sessionID: "s", status: { type: "busy" } });
  const turn = records()[0].turnID;
  await send("message.updated", { info: { id: "r", parentID: "p", sessionID: "s", role: "assistant", finish: "stop", time: { created: 5000, completed: 5100 } } });
  await send("session.status", { sessionID: "s", status: { type: "busy" } });
  await send("session.idle", { sessionID: "s" });
  assert.equal(records()[0].turnID, turn);
  assert.equal(records()[0].state, "completed");
});
