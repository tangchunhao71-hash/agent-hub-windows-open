import { test } from "node:test";
import assert from "node:assert/strict";
import {
  avatarBlink,
  avatarSeed,
  avatarGazeTarget,
  GROK_AVATAR_SHAPES,
  grokAvatarColor,
  grokAvatarModel,
  grokEyeSeat,
  projectGrokAvatar,
  sampleGrokAvatar,
  smoothAvatarGaze,
} from "../src/grok-avatar-motion.ts";

const near = (a: number, b: number, eps = 1e-5) =>
  assert.ok(Math.abs(a - b) < eps, `${a} != ${b}`);
test("native pointer loss clears gaze while browser pointer mode retains its last target", () => {
  const rect = {
    left: 10,
    top: 20,
    right: 50,
    bottom: 60,
    width: 40,
    height: 40,
  };
  assert.deepEqual(avatarGazeTarget(null, rect, [1, 1]), [0, 0]);
  assert.deepEqual(avatarGazeTarget(undefined, rect, [1, -1]), [1, -1]);
  assert.deepEqual(
    avatarGazeTarget({ x: 40, y: 30 }, rect, [0, 0]),
    [0.5, 0.5],
  );
  assert.deepEqual(avatarGazeTarget({ x: 90, y: 30 }, rect, [1, 1]), [0, 0]);
});
test("Grok compact idle keeps its original 4s clip, breath and two 260ms blinks", () => {
  const start = 1.481103;
  near(avatarBlink(start, [start]), 1);
  near(avatarBlink(start + 0.26 * 0.45, [start]), 0);
  near(avatarBlink(start + 0.26, [start]), 1);
  near(sampleGrokAvatar(start + 0.117, "idle").open, 0);
  const middle = sampleGrokAvatar(1.616369, "idle");
  near(middle.yaw, -8.658513);
  near(middle.roll, -2.542642);
  assert.ok(middle.scale > 1);
  assert.deepEqual(
    sampleGrokAvatar(0.4, "idle"),
    sampleGrokAvatar(4.4, "idle"),
  );
});
test("working changes the native clip and completion squashes once before returning to idle", () => {
  const work = sampleGrokAvatar(1.414107, "working");
  near(work.yaw, -16.14032);
  near(work.roll, -36.051754);
  near(work.scale, 0.421584 / 0.6);
  const done = sampleGrokAvatar(0.24, "done");
  near(done.squashX, 1.12);
  near(done.squashY, 0.88);
  assert.ok(done.happy > 0);
  assert.deepEqual(sampleGrokAvatar(2, "done"), sampleGrokAvatar(2, "idle"));
});
test("the same Bot retains its phase while different Bots do not blink in lockstep", () => {
  assert.equal(avatarSeed("fixture-a"), avatarSeed("fixture-a"));
  assert.notEqual(avatarSeed("fixture-a"), avatarSeed("fixture-b"));
  assert.notDeepEqual(
    sampleGrokAvatar(0, "idle", avatarSeed("a")),
    sampleGrokAvatar(0, "idle", avatarSeed("b")),
  );
});
test("Grok gaze retains 86% per 60Hz frame, is bounded, and is independent of frame cadence", () => {
  near(smoothAvatarGaze([0, 0], [1, -1], 1 / 60)[0], 0.14);
  const twice = smoothAvatarGaze(
      smoothAvatarGaze([0, 0], [1, -1], 1 / 60),
      [1, -1],
      1 / 60,
    ),
    once = smoothAvatarGaze([0, 0], [1, -1], 1 / 30);
  near(twice[0], once[0]);
  near(twice[1], once[1]);
  assert.ok(smoothAvatarGaze([0, 0], [100, -100], 1)[0] <= 1);
});
test("all 26 foundation shapes project finite eyes and bodies, including spins and unknown fallbacks", () => {
  assert.equal(GROK_AVATAR_SHAPES.length, 26);
  for (const shape of GROK_AVATAR_SHAPES) {
    const model = grokAvatarModel(shape);
    assert.ok(model.outline.length >= 24);
    for (const yaw of [-360, -45, 0, 45, 180, 360]) {
      const paths = projectGrokAvatar(
        model,
        shape,
        { ...sampleGrokAvatar(0, "idle", 0, true), yaw },
        [1, -1],
      );
      assert.ok(paths.front.startsWith("M"));
      assert.doesNotMatch(JSON.stringify(paths), /NaN|Infinity/);
      if (yaw === 180) assert.equal(paths.eyes, "");
    }
  }
  assert.deepEqual(grokAvatarModel("bad-shape"), grokAvatarModel("blob"));
  near(grokEyeSeat("house").gap, 0.512512);
});
test("dark Grok foundation colors remain visible and do not invent letter avatars", () => {
  assert.equal(grokAvatarColor("#000000", true), "#000000");
  assert.equal(grokAvatarColor("#000000", false), "#FFFFFF");
  assert.equal(grokAvatarColor("#00C972", false), "#009957");
  assert.equal(grokAvatarColor("url(bad)", true), "#777777");
});
test("reduced motion freezes the whole performer and pointer pokes finish at rest", () => {
  const still = sampleGrokAvatar(0, "idle", 0, true);
  assert.deepEqual(
    sampleGrokAvatar(900, "working", 0.5, true, { kind: "spin", age: 0.4 }),
    still,
  );
  for (const kind of ["bounce", "nod", "spin"] as const) {
    const sample = sampleGrokAvatar(0.3, "idle", 0, false, { kind, age: 0.25 });
    assert.notDeepEqual(sample, sampleGrokAvatar(0.3, "idle"));
    assert.deepEqual(
      sampleGrokAvatar(0.3, "idle", 0, false, { kind, age: 2 }),
      sampleGrokAvatar(0.3, "idle"),
    );
  }
  near(
    sampleGrokAvatar(0.3, "idle", 0, false, { kind: "bounce", age: 0.62 })
      .squashX,
    1,
  );
});
