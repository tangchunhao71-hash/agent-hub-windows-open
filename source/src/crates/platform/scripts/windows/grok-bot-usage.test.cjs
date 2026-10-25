const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

test("official daily usage request groups by Bot using protobuf enum values", async () => {
  const source = fs.readFileSync(path.join(__dirname, "grok-bot-usage.cjs"), "utf8");
  const startIndex = source.indexOf("const reply = await request(");
  const endIndex = source.indexOf("if (!Array.isArray(reply.dailySpend))", startIndex);
  assert.ok(startIndex >= 0 && endIndex > startIndex);
  // Execute only the real request expression with synthetic inputs. No account
  // files, credential getter, or network APIs are available in this context.
  let captured;
  await vm.runInNewContext(
    `(async () => { ${source.slice(startIndex, endIndex)} })()`,
    {
      me: { userId: 123 }, team: undefined, token: "fixture-only",
      start: new Date("2026-10-17T00:00:00Z"), end: 1792800000000,
      request: async (route, body) => { captured = { route, body }; return {}; },
    },
  );
  assert.equal(captured.route, "GetDailySpendByCategory");
  assert.equal(captured.body.groupBy, 4);
  assert.equal(captured.body.spendType, 3);
  assert.equal(captured.body.clientType, "grok-bot");
  assert.deepEqual(Array.from(captured.body.products), ["sand"]);
  assert.equal(captured.body.includeCreditUsage, true);
  assert.equal(captured.body.periodStartMs, "1792195200000");
  assert.equal(captured.body.periodEndMs, "1792800000000");
});
