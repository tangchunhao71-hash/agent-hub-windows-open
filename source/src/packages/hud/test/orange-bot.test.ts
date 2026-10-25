import assert from "node:assert/strict";
import test from "node:test";
import { orangeFrameAt } from "../src/bots/orange-bot-timing.ts";

test("four-frame playback respects blink duration and loops without a blank boundary", () => {
  assert.deepEqual(
    [0, 499, 500, 619, 620, 799, 800, 1299, 1300, 1800].map(
      (t) => orangeFrameAt(t).index,
    ),
    [0, 0, 1, 1, 2, 2, 3, 3, 0, 1],
  );
  assert.equal(orangeFrameAt(500).remainingMs, 120);
  assert.equal(orangeFrameAt(1299).remainingMs, 1);
  assert.equal(orangeFrameAt(-1).index, 3);
});
