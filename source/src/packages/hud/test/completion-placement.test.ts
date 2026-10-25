import { test } from "node:test";
import assert from "node:assert/strict";
import { bottomCompletionPlacement } from "../src/scene/completion-placement.ts";

test("completion falls to the screen floor from top, sides and bottom docks", () => {
  const screen = { width: 1000, height: 800 };
  for (const from of [
    { x: 500, y: 20 },
    { x: 20, y: 400 },
    { x: 980, y: 400 },
    { x: 500, y: 780 },
  ]) {
    const { point, frame, size } = bottomCompletionPlacement(
      screen,
      from,
      { width: 470, height: 250 },
      32,
    );
    assert.equal(point.y + size / 2, screen.height);
    assert.ok(point.y >= from.y, "the bot does not fly upward");
    assert.ok(
      frame.y + frame.height < point.y - size / 2,
      "reply opens above the landed bot",
    );
    assert.ok(frame.x >= 0 && frame.x + frame.width <= screen.width);
    assert.ok(frame.y >= 0 && frame.y + frame.height <= screen.height);
  }
});

test("a small screen clamps the card and bot to its available area", () => {
  const { point, frame, size } = bottomCompletionPlacement(
    { width: 320, height: 240 },
    { x: 400, y: 10 },
    { width: 470, height: 500 },
    48,
  );
  assert.equal(point.y + size / 2, 240);
  assert.ok(point.x + size / 2 <= 320);
  assert.ok(frame.x + frame.width <= 320 && frame.y >= 0);
  assert.ok(frame.y + frame.height < point.y - size / 2);
});

test("a large bottom dock cannot cover the reply's action buttons", () => {
  // Simple size 60 with quota rings has a 33px Bot in an approximately 85px dock.
  const { point, frame, size } = bottomCompletionPlacement(
    { width: 1000, height: 800 },
    { x: 500, y: 757 },
    { width: 470, height: 250 },
    33,
    85,
  );
  assert.equal(point.y + size / 2, 800);
  assert.ok(frame.y + frame.height <= 800 - 85 - 8);
});

test("long side docks cannot cover a tall reply", () => {
  for (const edge of ["left", "right"] as const) {
    const dock = {
      x: edge === "left" ? 0 : 915,
      y: 100,
      width: 85,
      height: 600,
    };
    const { frame } = bottomCompletionPlacement(
      { width: 1000, height: 800 },
      { x: edge === "left" ? 42 : 958, y: 400 },
      { width: 470, height: 400 },
      33,
      0,
      { edge, frame: dock },
    );
    if (edge === "left") assert.ok(frame.x >= dock.x + dock.width + 8);
    else assert.ok(frame.x + frame.width <= dock.x - 8);
  }
});
