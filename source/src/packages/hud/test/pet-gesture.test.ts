import { test } from "node:test";
import assert from "node:assert/strict";
import {
  petThrowVelocity,
  petDizzyImpact,
  petPoint,
  petNativeTransition,
} from "../src/scene/pet-gesture.ts";
import {
  completionMotion,
  completionPose,
} from "../src/scene/completion-physics.ts";

test("a released drag uses recent pointer velocity and caps extreme flings", () => {
  assert.deepEqual(
    petThrowVelocity(
      [
        { x: 10, y: 100, at: 0 },
        { x: 30, y: 60, at: 100 },
      ],
      100,
    ),
    { vx: 200, vy: -400 },
  );
  assert.deepEqual(
    petThrowVelocity(
      [
        { x: 0, y: 0, at: 0 },
        { x: 1000, y: 1000, at: 16 },
      ],
      16,
    ),
    { vx: 900, vy: 1400 },
  );
  assert.deepEqual(
    petThrowVelocity(
      [
        { x: 10, y: 100, at: 0 },
        { x: 30, y: 60, at: 100 },
      ],
      300,
    ),
    { vx: 0, vy: 0 },
  );
  assert.deepEqual(petThrowVelocity([{ x: 10, y: 10, at: 20 }], 20), {
    vx: 0,
    vy: 0,
  });
});

test("raising and releasing a bot preserves its angle and starts dizziness at the impact", () => {
  const motion = completionMotion(
    { x: 300, y: 200 },
    { width: 1000, height: 800 },
    32,
    { vx: 0, vy: 0, omega: 0, angle: 2 },
  );
  assert.equal(completionPose(motion, 0).angle, 2);
  assert.equal(completionPose(motion, 0).x, 300);
  assert.equal(completionPose(motion, 0).y, 200);
  assert.ok(petDizzyImpact(motion)! > 0);
  const gentle = completionMotion(
    { x: 300, y: 780 },
    { width: 1000, height: 800 },
    32,
    { vx: 0, vy: 0, omega: 0 },
  );
  assert.equal(petDizzyImpact(gentle), null);
});

test("a strong upward throw hits the ceiling without leaving the viewport", () => {
  const motion = completionMotion(
    { x: 300, y: 100 },
    { width: 600, height: 800 },
    32,
    { vx: 300, vy: -1400, omega: 0, angle: 1 },
  );
  assert.ok(motion.collisions.some((c) => c.kind === "ceiling"));
  for (let i = 0; i < 1000; i++) {
    const pose = completionPose(motion, (motion.duration * i) / 1000);
    assert.ok(pose.x >= 16 - 1e-7 && pose.x <= 584 + 1e-7);
    assert.ok(pose.y >= 16 - 1e-7 && pose.y <= 784 + 1e-7);
  }
});

test("dragging clamps the bot center inside all screen edges", () => {
  assert.deepEqual(
    petPoint({ x: -10, y: -10 }, { width: 600, height: 800 }, 16),
    { x: 16, y: 16 },
  );
  assert.deepEqual(
    petPoint({ x: 700, y: 900 }, { width: 600, height: 800 }, 16),
    { x: 584, y: 784 },
  );
});

test("a stale native up cannot cancel the grab before its down is observed", () => {
  assert.deepEqual(petNativeTransition(false, 0), {
    downSeen: false,
    action: "ignore",
  });
  const down = petNativeTransition(false, 1);
  assert.deepEqual(down, { downSeen: true, action: "move" });
  assert.deepEqual(petNativeTransition(down.downSeen, 0), {
    downSeen: true,
    action: "release",
  });
  assert.equal(petNativeTransition(true, null).action, "ignore");
});

test("a resized viewport can recompute an in-flight throw inside its new boundaries", () => {
  const original = completionMotion(
    { x: 900, y: 200 },
    { width: 1000, height: 800 },
    32,
    { vx: 300, vy: 0, angle: 1 },
  );
  const current = completionPose(original, 0.2);
  const screen = { width: 600, height: 500 };
  const updated = completionMotion(
    petPoint(current, screen, 16),
    screen,
    32,
    current,
  );
  assert.ok(updated.final.x <= 584 && updated.final.y <= 484);
  for (let i = 0; i <= 100; i++) {
    const pose = completionPose(updated, (updated.duration * i) / 100);
    assert.ok(
      pose.x >= 16 - 1e-7 &&
        pose.x <= 584 + 1e-7 &&
        pose.y >= 16 - 1e-7 &&
        pose.y <= 484 + 1e-7,
    );
  }
});
