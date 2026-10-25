const assert = require("node:assert/strict"),
  fs = require("node:fs"),
  path = require("node:path"),
  test = require("node:test"),
  vm = require("node:vm");
const source = fs.readFileSync(
  path.join(__dirname, "grok-bot-usage.cjs"),
  "utf8",
);
function reader({
  roster = { rows: [{ id: "bot-a" }] },
  response = { statuses: [] },
  check = () => {},
} = {}) {
  const start = source.indexOf("async function readActivity("),
    end = source.indexOf("async function run(", start);
  assert.ok(start >= 0 && end > start);
  let calls = 0,
    body;
  const read = vm.runInNewContext(source.slice(start, end) + ";readActivity", {
    cached: () => roster,
    request: async (route, params, token) => {
      calls++;
      assert.equal(route, "ListGrokBotAgentStatuses");
      assert.equal(token, "fixture-token");
      body = JSON.parse(JSON.stringify(params));
      return response;
    },
    Date,
    Error,
    Set,
  });
  return {
    read: () => read("fixture-cache", "fixture-slot", "fixture-token", check),
    facts: () => ({ calls, body }),
  };
}
test("status request sends only known Bot IDs and emits no chat, previews or credentials", async () => {
  const r = reader({
    response: {
      statuses: [
        {
          agentId: "bot-a",
          client: { lastMessagePreview: "private" },
          live: {
            agentId: "bot-a",
            isRunning: true,
            updatedAtMs: "100",
            staleAfterMs: "90000",
            activity: {
              kind: "tool",
              tool: "shell",
              detail: "private",
              target: "private",
              callId: "private",
            },
            awaiting: { reason: "private" },
            lastTurnSettlement: {
              turnId: "fixture-turn-1",
              outcome: "success",
              settledAtMs: "90",
            },
          },
        },
      ],
    },
  });
  const body = await r.read();
  assert.deepEqual(r.facts(), { calls: 1, body: { agentIds: ["bot-a"] } });
  const serialized = JSON.stringify(body);
  assert.equal(serialized.includes("private"), false);
  assert.equal(serialized.includes("fixture-token"), false);
  assert.equal(body.statuses[0].live.waiting, true);
  assert.equal(body.statuses[0].live.activity.kind, "tool");
  assert.equal(body.accountSlot, "fixture-slot");
  assert.equal(body.success, true);
  assert.equal(
    body.statuses[0].live.lastTurnSettlement.turnId,
    "fixture-turn-1",
  );
  assert.equal(body.statuses[0].live.lastTurnSettlement.settledAtMs, "90");
});
test("empty or malformed roster does not query all accounts and account changes fail closed", async () => {
  const empty = reader({ roster: { rows: [] } });
  assert.equal((await empty.read()).statuses.length, 0);
  assert.equal(empty.facts().calls, 0);
  const duplicate = reader({
    roster: { rows: [{ id: "bot-a" }, { id: "bot-a" }] },
  });
  await assert.rejects(duplicate.read(), /invalid-response/);
  assert.equal(duplicate.facts().calls, 0);
  const changed = reader({
    check: () => {
      throw Error("account-changed");
    },
  });
  await assert.rejects(changed.read(), /account-changed/);
});

test("completion identity metadata keeps explicit outcome but never forwards nested content", async () => {
  const response = {
    statuses: [
      {
        agentId: "bot-a",
        live: {
          agentId: "bot-a",
          updatedAtMs: "100",
          staleAfterMs: "90000",
          lastTurnSettlement: {
            turnId: "fixture-turn-2",
            outcome: "error",
            settledAtMs: "99",
            summary: "private",
          },
        },
      },
    ],
  };
  const completion = (await reader({ response }).read()).statuses[0].live
    .lastTurnSettlement;
  assert.equal(completion.turnId, "fixture-turn-2");
  assert.equal(completion.outcome, "error");
  assert.equal(completion.settledAtMs, "99");
  assert.equal(JSON.stringify(completion).includes("private"), false);
  response.statuses[0].live.lastTurnSettlement = {
    turnId: { content: "private" },
    outcome: { content: "private" },
    settledAtMs: { content: "private" },
  };
  const malformed = (await reader({ response }).read()).statuses[0].live
    .lastTurnSettlement;
  assert.equal(JSON.stringify(malformed).includes("private"), false);
  assert.equal(malformed.turnId, undefined);
  assert.equal(malformed.outcome, undefined);
  assert.equal(malformed.settledAtMs, undefined);
});
test("only fixed official origins, services and read-only methods are available", async () => {
  const start = source.indexOf("async function request("),
    end = source.indexOf("async function readActivity(", start);
  let calls = [];
  const request = vm.runInNewContext(source.slice(start, end) + ";request", {
    fetch: async (url, options) => {
      calls.push({ url, options });
      return {
        ok: true,
        body: (async function* () {
          yield Buffer.from("{}");
        })(),
      };
    },
    AbortSignal,
    Buffer,
    Error,
  });
  await request(
    "ListGrokBotAgentStatuses",
    { agentIds: ["bot-a"] },
    "fixture-token",
  );
  assert.equal(
    calls[0].url,
    "https://api2.cursor.sh/aiserver.v1.GrokBotService/ListGrokBotAgentStatuses",
  );
  assert.equal(calls[0].options.redirect, "error");
  await request("GetMe", {}, "fixture-token");
  assert.equal(
    calls[1].url,
    "https://api2.cursor.sh/aiserver.v1.DashboardService/GetMe",
  );
  await assert.rejects(
    request("SendMessage", {}, "fixture-token"),
    /invalid-response/,
  );
  assert.equal(calls.length, 2);
});
