import { test } from "node:test";
import assert from "node:assert/strict";
import type { Session } from "@agent-hud/protocol";
import { taskProgress } from "../src/task-progress.ts";

const session = (
  state: Session["state"],
  work: string | null = null,
): Session =>
  ({
    id: "task-1",
    title: "Build the app",
    state,
    liveFacts: {
      activity: work ? { work, openCall: null } : null,
    },
  }) as Session;

test("working to waiting to completed removes the busy indicator without inventing a percentage", () => {
  const running = taskProgress([session("running", "shell")], true)!;
  assert.equal(running.mode, "working");
  assert.match(running.detail, /执行命令/);
  assert.match(running.detail, /未提供.*百分比/);
  const waiting = taskProgress([session("waiting", "shell")], true)!;
  assert.equal(waiting.mode, "waiting");
  assert.match(waiting.detail, /等待回复/);
  assert.doesNotMatch(waiting.detail, /执行命令/);
  assert.equal(taskProgress([session("idle")], true), null);
  assert.equal(taskProgress([session("unverified")], true), null);
});

test("one Bot can show concurrent running and waiting tasks without calling them all running", () => {
  const progress = taskProgress(
    [session("running", "edit"), session("waiting"), session("idle")],
    false,
  )!;
  assert.equal(progress.running, 1);
  assert.equal(progress.waiting, 1);
  assert.equal(progress.mode, "working");
  assert.match(progress.label, /1 running, 1 waiting/);
  assert.match(progress.detail, /Editing/);
});

test("missing activity stays working and retry/compaction/subagent facts get their own stage", () => {
  assert.match(taskProgress([session("running")], false)!.detail, /Working/);
  for (const [activity, text] of [
    [{ retrying: true }, "Retrying"],
    [{ compactingSince: "2026-10-24T10:00:00Z" }, "Compacting context"],
    [{ delegating: true }, "Subagents working"],
  ] as const) {
    const value = session("running");
    Object.assign(value.liveFacts!, { activity });
    assert.ok(taskProgress([value], false)!.detail.includes(text));
  }
  assert.equal(taskProgress([], false), null);
});
