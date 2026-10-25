import { test } from "node:test";
import assert from "node:assert/strict";
import type { Approval, IslandEvent, Session } from "@agent-hud/protocol";
import {
  completionSession,
  reminderTimeout,
} from "../src/completion-reminder.ts";
import {
  reconcileIslandCards,
  dismissIslandCard,
  dismissIslandCardByID,
  dismissCompletionCards,
} from "../src/island-queue.ts";

const reply = (id: string): IslandEvent => ({
  id,
  kind: "completion",
  source: "codex",
  sessionId: `session-${id}`,
  at: "2026-10-24T15:00:00Z",
  title: `Task ${id}`,
  navigationTarget: { sessionId: `session-${id}` },
});

test("simultaneous replies keep their session binding and dismiss in order", () => {
  const first = reply("a"),
    second = reply("b");
  const queue = reconcileIslandCards([], [], [first, second]);
  assert.deepEqual(
    queue.map((card) => card.id),
    ["a", "b"],
  );
  const next = dismissIslandCard(queue);
  assert.equal(next[0].type, "news");
  if (next[0].type === "news")
    assert.equal(next[0].event.sessionId, "session-b");
  assert.equal(reconcileIslandCards(queue, [], [first]).length, 2);
});

test("one action clears an overnight backlog across sources without changing task facts", () => {
  const replies = Array.from({ length: 150 }, (_, i) => ({
    ...reply(String(i)),
    source: i % 2 ? "grok-bot" : "codex",
  }));
  const before = structuredClone(replies);
  const queue = reconcileIslandCards([], [], replies);
  assert.equal(queue.length, 150);
  assert.deepEqual(dismissCompletionCards(queue), []);
  assert.deepEqual(replies, before);
  assert.equal(queue.length, 150);
  const next = reconcileIslandCards([], [], [reply("new")]);
  assert.equal(next[0].id, "new");
  assert.equal(dismissIslandCardByID(next, "0"), next);
});

test("bulk dismissal preserves approvals and quota notices, prioritizing the oldest approval", () => {
  const queue = reconcileIslandCards(
    [],
    [],
    [reply("a"), { ...reply("quota"), kind: "quota-exhaustion" }, reply("b")],
  );
  queue.push(
    { type: "request", id: "newer", at: "2026-10-24T16:00:00Z" },
    { type: "request", id: "older", at: "2026-10-24T15:00:00Z" },
  );
  const remaining = dismissCompletionCards(queue);
  assert.deepEqual(
    remaining.map((card) => card.id),
    ["older", "quota", "newer"],
  );
  assert.equal(dismissCompletionCards(remaining), remaining);
});

test("bulk dismissal keeps an active approval in place", () => {
  const request = { id: "approval", createdAt: reply("a").at } as Approval;
  const queue = reconcileIslandCards([], [request], [reply("a"), reply("b")]);
  const remaining = dismissCompletionCards(queue);
  assert.equal(remaining[0], queue[0]);
  assert.equal(remaining.length, 1);
});

test("disabling Bot drops clears queued completions and suppresses incoming replies only", () => {
  const request = { id: "approval", createdAt: reply("a").at } as Approval;
  const queue = reconcileIslandCards(
    [],
    [],
    [reply("a"), reply("b"), { ...reply("quota"), kind: "quota-reset" }],
  );
  const next = reconcileIslandCards(
    queue,
    [request],
    [reply("c")],
    [request],
    false,
    false,
  );
  assert.deepEqual(
    next.map((card) => card.id),
    ["approval", "quota"],
  );
});

test("new quota alerts can still appear with Bot drops disabled", () => {
  const next = reconcileIslandCards(
    [],
    [],
    [reply("a"), { ...reply("quota"), kind: "quota-exhaustion" }],
    [],
    false,
    false,
  );
  assert.deepEqual(
    next.map((card) => card.id),
    ["quota"],
  );
});

test("a new permission request interrupts a persistent reply without losing it", () => {
  const queue = reconcileIslandCards([], [], [reply("a")]);
  const request = {
    id: "approval",
    createdAt: "2026-10-24T15:01:00Z",
  } as Approval;
  const next = reconcileIslandCards(queue, [request], [], [request]);
  assert.deepEqual(
    next.map((card) => card.id),
    ["approval", "a"],
  );
  const answered = reconcileIslandCards(next, [], [], []);
  assert.equal(answered[0].id, "a");
});

test("bursts of notices cannot evict unread replies", () => {
  const replies = Array.from({ length: 40 }, (_, index) =>
    reply(String(index)),
  );
  const queue = reconcileIslandCards([], [], replies);
  assert.equal(queue.length, 40);
  assert.equal(reminderTimeout("completion"), null);
  assert.equal(reminderTimeout("quota-reset"), 4000);
});

test("navigation keeps the reminder's target when the live session target changes", () => {
  const event = reply("a");
  const live = {
    id: event.sessionId,
    navigationTarget: { sessionId: "newer-target" },
  } as Session;
  assert.deepEqual(
    completionSession(event, [live])?.navigationTarget,
    event.navigationTarget,
  );
  assert.deepEqual(live.navigationTarget, { sessionId: "newer-target" });
});

test("a session that has left the bounded snapshot can still open its exact conversation", () => {
  const event = reply("a");
  const session = completionSession(event, []);
  assert.equal(session?.id, "session-a");
  assert.deepEqual(session?.navigationTarget, { sessionId: "session-a" });
  assert.equal(
    completionSession({ ...event, sessionId: undefined }, []),
    undefined,
  );
  assert.equal(
    completionSession({ ...event, navigationTarget: undefined }, []),
    undefined,
  );
});

test("a late or repeated navigation result cannot dismiss the next reply", () => {
  const queue = reconcileIslandCards([], [], [reply("a"), reply("b")]);
  const afterA = dismissIslandCardByID(queue, "a");
  assert.equal(afterA[0].id, "b");
  assert.equal(dismissIslandCardByID(afterA, "a"), afterA);
});

test("Bot reminders retain their full identity after roster changes or paging", () => {
  const botIdentity = {
    name: "A full Bot name that must survive truncation",
    avatar: { kind: "shape" as const, shape: "house", color: "#00C972" },
  };
  const event = { ...reply("bot"), source: "grok-bot", botIdentity };
  assert.deepEqual(completionSession(event, [])?.botIdentity, botIdentity);
  const live = {
    id: event.sessionId,
    botIdentity: { ...botIdentity, name: "Renamed after completion" },
  } as Session;
  assert.deepEqual(completionSession(event, [live])?.botIdentity, botIdentity);
  assert.equal(live.botIdentity?.name, "Renamed after completion");
});

test("finishing navigation for an offscreen reply preserves the reply currently shown", () => {
  const queue = reconcileIslandCards(
    [],
    [],
    [reply("b"), reply("a"), reply("c")],
  );
  assert.deepEqual(
    dismissIslandCardByID(queue, "a").map((card) => card.id),
    ["b", "c"],
  );
});
