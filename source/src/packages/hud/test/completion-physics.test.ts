import { test } from "node:test";
import assert from "node:assert/strict";
import {
  COMPLETION_PHYSICS as physics,
  completionMotion,
  completionPose,
  completionKeyframes,
  completionReturnMotion,
  type CompletionPose,
} from "../src/scene/completion-physics.ts";
import { bottomCompletionPlacement } from "../src/scene/completion-placement.ts";

const close = (actual: number, expected: number, epsilon = 1e-7) =>
  assert.ok(Math.abs(actual - expected) < epsilon, `${actual} != ${expected}`);
const kinetic = (pose: CompletionPose, radius: number) =>
  (pose.vx ** 2 + pose.vy ** 2) / 2 + (radius ** 2 * pose.omega ** 2) / 4;

test("free fall takes the gravitational time for its height and conserves mechanical energy", () => {
  const motion = completionMotion(
    { x: 300, y: 100 },
    { width: 2000, height: 600 },
    32,
  );
  const floor = 600 - motion.radius;
  const impact = motion.collisions.find((c) => c.kind === "floor")!;
  close(impact.at, Math.sqrt((2 * (floor - 100)) / physics.gravity));
  const initialEnergy =
    kinetic(motion.initial, motion.radius) + physics.gravity * (floor - 100);
  for (const fraction of [0.2, 0.5, 0.8]) {
    const pose = completionPose(motion, impact.at * fraction);
    close(
      kinetic(pose, motion.radius) + physics.gravity * (floor - pose.y),
      initialEnergy,
      1e-6,
    );
  }
  const shorter = completionMotion(
    { x: 300, y: 400 },
    { width: 2000, height: 600 },
    32,
  );
  assert.ok(shorter.collisions.find((c) => c.kind === "floor")!.at < impact.at);
});

test("bounce heights shrink by restitution squared, and collisions cannot add energy", () => {
  const motion = completionMotion(
    { x: 300, y: 100 },
    { width: 2000, height: 900 },
    32,
  );
  const bounces = motion.collisions.filter(
    (c) => c.kind === "floor" && c.after.vy < 0,
  );
  assert.ok(bounces.length >= 2);
  for (const bounce of bounces)
    close(bounce.after.vy, -bounce.before.vy * physics.restitution);
  const height = (vy: number) => vy ** 2 / (2 * physics.gravity);
  close(
    height(bounces[1].after.vy) / height(bounces[0].after.vy),
    physics.restitution ** 2,
  );
  for (const collision of motion.collisions)
    assert.ok(
      kinetic(collision.after, motion.radius) <=
        kinetic(collision.before, motion.radius) + 1e-6,
    );
});

test("rolling resistance stops the bot without reversal and rotation follows distance over radius", () => {
  const motion = completionMotion(
    { x: 500, y: 584 },
    { width: 1000, height: 600 },
    32,
  );
  const stopTime = physics.launchSpeed / physics.rollingDeceleration;
  const distance = physics.launchSpeed ** 2 / (2 * physics.rollingDeceleration);
  close(motion.duration, stopTime);
  close(motion.final.x - motion.initial.x, distance);
  close(motion.final.angle, distance / motion.radius);
  let previousSpeed = Infinity;
  for (let i = 0; i <= 20; i++) {
    const pose = completionPose(motion, (stopTime * i) / 20);
    assert.ok(pose.vx >= -1e-9 && pose.vx <= previousSpeed + 1e-9);
    close(pose.omega, pose.vx / motion.radius);
    previousSpeed = pose.vx;
  }
  assert.equal(motion.final.vx, 0);
  assert.equal(motion.final.omega, 0);
});

test("wall rebounds and floor contacts keep a narrow-screen trajectory bounded and dissipative", () => {
  const motion = completionMotion(
    { x: 30, y: 10 },
    { width: 60, height: 800 },
    26,
  );
  assert.ok(motion.collisions.some((c) => c.kind === "wall"));
  for (let i = 0; i <= 1000; i++) {
    const pose = completionPose(motion, (motion.duration * i) / 1000);
    assert.ok(
      pose.x >= motion.radius - 1e-7 && pose.x <= 60 - motion.radius + 1e-7,
    );
    assert.ok(
      pose.y >= motion.radius - 1e-7 && pose.y <= 800 - motion.radius + 1e-7,
    );
  }
  for (const collision of motion.collisions)
    assert.ok(
      kinetic(collision.after, motion.radius) <=
        kinetic(collision.before, motion.radius) + 1e-6,
    );
  assert.equal(motion.final.vx, 0);
  assert.equal(motion.final.vy, 0);
});

test("render cadence and background jumps do not change motion or its final orientation", () => {
  const motion = completionMotion(
    { x: 500, y: 20 },
    { width: 1000, height: 800 },
    32,
  );
  const expected = completionPose(motion, 1.25);
  for (const fps of [30, 60, 144]) {
    for (let t = 0; t < 1.25; t += 1 / fps) completionPose(motion, t);
    assert.deepEqual(completionPose(motion, 1.25), expected);
  }
  assert.deepEqual(completionPose(motion, motion.duration + 60), motion.final);
  const frames = completionKeyframes(motion);
  close(frames.rotate.at(-1)!, (motion.final.angle * 180) / Math.PI);
  close(frames.times.at(-1)!, 1);
  for (const collision of motion.collisions)
    assert.ok(
      frames.times.some(
        (t) => Math.abs(t * motion.duration - collision.at) < 1e-7,
      ),
    );
});

test("the reply anchors to the rolled endpoint and waits for the same trajectory duration", () => {
  const screen = { width: 1000, height: 800 };
  const motion = completionMotion({ x: 400, y: 20 }, screen, 32);
  const landing = bottomCompletionPlacement(
    screen,
    motion.final,
    { width: 470, height: 250 },
    32,
  );
  close(landing.point.x, motion.final.x);
  close(landing.point.y, motion.final.y);
  close(landing.frame.x + landing.frame.width / 2, motion.final.x);
  assert.ok(
    motion.duration > motion.collisions.find((c) => c.kind === "floor")!.at,
  );
  assert.equal(completionPose(motion, motion.duration).vy, 0);
  assert.equal(completionPose(motion, motion.duration).vx, 0);
});

test("the return jump starts at the settled pose and reaches each dock without crossing the ceiling", () => {
  const motion = completionMotion(
    { x: 500, y: 20 },
    { width: 1000, height: 800 },
    32,
  );
  for (const target of [
    { x: 500, y: 20 },
    { x: 20, y: 400 },
    { x: 980, y: 400 },
    { x: 500, y: 780 },
  ]) {
    const back = completionReturnMotion(motion.final, target, motion.radius);
    const initial = completionPose(back, 0);
    close(initial.x, motion.final.x);
    close(initial.y, motion.final.y);
    close(initial.angle, motion.final.angle);
    assert.ok(back.duration >= 0.1, "returning is an actual trajectory");
    for (let i = 0; i < 100; i++) {
      const pose = completionPose(back, (back.duration * i) / 100);
      assert.ok(pose.x >= 16 - 1e-7 && pose.x <= 984 + 1e-7);
      assert.ok(pose.y >= 16 - 1e-7 && pose.y <= 784 + 1e-7);
    }
    const final = completionPose(back, back.duration);
    close(final.x, target.x);
    close(final.y, target.y);
    close(final.angle / (Math.PI * 2), Math.round(final.angle / (Math.PI * 2)));
    const almost = completionPose(back, back.duration - 1e-7);
    close(almost.angle, final.angle, 1e-5);
  }
});

test("return flight follows gravity instead of a straight position tween", () => {
  const motion = completionMotion(
    { x: 500, y: 20 },
    { width: 1000, height: 800 },
    32,
  );
  const back = completionReturnMotion(motion.final, { x: 500, y: 780 }, 16);
  const t = back.duration / 2,
    midpoint = completionPose(back, t);
  close(
    midpoint.y,
    back.initial.y + back.initial.vy * t + (physics.gravity * t * t) / 2,
  );
  assert.ok(midpoint.y < Math.min(back.initial.y, back.final.y));
  close(midpoint.vy, back.initial.vy + physics.gravity * t);
});

test("a redirected return continues from the current position and orientation", () => {
  const down = completionMotion(
    { x: 500, y: 20 },
    { width: 1000, height: 800 },
    32,
  );
  const first = completionReturnMotion(down.final, { x: 500, y: 20 }, 16);
  const current = completionPose(first, first.duration * 0.4);
  const redirected = completionReturnMotion(current, { x: 980, y: 400 }, 16);
  const start = completionPose(redirected, 0);
  close(start.x, current.x);
  close(start.y, current.y);
  close(start.angle, current.angle);
  const final = completionPose(redirected, redirected.duration);
  close(final.x, 980);
  close(final.y, 400);
});
