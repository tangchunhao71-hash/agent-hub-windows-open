import { test } from "node:test";
import assert from "node:assert/strict";
import type { Session } from "@agent-hud/protocol";
import { sessionFactsKey } from "../src/lib/use-sessions.ts";

test("official Bot totals refresh an opened detail on change or removal", () => {
  const session = {
    id: "fixture-bot", logPath: null, updatedAt: "2026-10-24T00:00:00Z",
    tokens: 0, cost: null, contextPercent: null, parentId: null,
  } as Session;
  const baseline = sessionFactsKey(session);
  session.reportedUsage = {
    totalTokens: 120, periodStart: "2026-10-17T00:00:00Z",
    periodEnd: "2026-10-24T00:00:00Z", observedAt: "2026-10-24T00:00:00Z",
    source: "official-bot-usage", breakdownAvailable: false,
  };
  const first = sessionFactsKey(session);
  assert.notEqual(first, baseline);
  session.reportedUsage.totalTokens = 150;
  assert.notEqual(sessionFactsKey(session), first);
  delete session.reportedUsage;
  assert.equal(sessionFactsKey(session), baseline);
});
