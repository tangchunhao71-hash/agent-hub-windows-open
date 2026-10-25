import assert from "node:assert/strict";
import test from "node:test";
import { mainAgentFirst } from "../src/main-agent-order.ts";

test("pinning keeps other agents and their original order without mutating input", () => {
  const agents = ["claude", "codex", "grok-bot", "workbuddy"].map((key) => ({
    key,
  }));
  assert.deepEqual(
    mainAgentFirst(agents, "grok-bot").map((a) => a.key),
    ["grok-bot", "claude", "codex", "workbuddy"],
  );
  assert.deepEqual(
    agents.map((a) => a.key),
    ["claude", "codex", "grok-bot", "workbuddy"],
  );
  assert.equal(mainAgentFirst(agents, "missing"), agents);
  assert.equal(mainAgentFirst(agents, ""), agents);
  assert.equal(mainAgentFirst(agents, "claude"), agents);
  assert.deepEqual(mainAgentFirst([], "codex"), []);
});
