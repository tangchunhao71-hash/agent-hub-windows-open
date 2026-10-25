import { after, test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";

const modules = await createServer({
  configFile: false,
  root: fileURLToPath(new URL("../src", import.meta.url)),
  server: { middlewareMode: true, watch: null },
  appType: "custom",
  logLevel: "silent",
});
after(() => modules.close());
const { createSeedRobotSheet } = (await modules.ssrLoadModule(
  "/bots/seed-robot.ts",
)) as typeof import("../src/bots/seed-robot.ts");
const { clawdClips } = (await modules.ssrLoadModule(
  "/bots/clawd-clips.ts",
)) as typeof import("../src/bots/clawd-clips.ts");
const { loadClawd } = (await modules.ssrLoadModule(
  "/bots/clawd.ts",
)) as typeof import("../src/bots/clawd.ts");
const { botSpecs } = (await modules.ssrLoadModule(
  "/bots/bot-spec.ts",
)) as typeof import("../src/bots/bot-spec.ts");

test("every scheduled clip has original, bounded and visible pixel frames", () => {
  const sheet = createSeedRobotSheet();
  assert.deepEqual(Object.keys(sheet), Object.keys(clawdClips.frameCounts));
  for (const [name, count] of Object.entries(clawdClips.frameCounts)) {
    const sprite = sheet[name];
    assert.equal(sprite.frames.length, count, name);
    for (const frame of sprite.frames) {
      assert.equal(frame.length, sprite.width * sprite.height, name);
      assert(
        frame.some((ink) => ink !== 255),
        name,
      );
      assert(frame.every((ink) => ink === 255 || ink < sprite.palette.length));
      // Keep transparent borders: an out-of-canvas pose cannot silently pass.
      assert(frame.subarray(0, sprite.width).every((ink) => ink === 255));
      assert(frame.subarray(-sprite.width).every((ink) => ink === 255));
      assert(frame.every((ink, i) => i % sprite.width !== 0 || ink === 255));
      assert(
        frame.every(
          (ink, i) => i % sprite.width !== sprite.width - 1 || ink === 255,
        ),
      );
    }
    assert(
      new Set(sprite.frames.map((frame) => frame.toString())).size > 1,
      name,
    );
  }
  assert.notDeepEqual(sheet.Thinking.frames[0], sheet.Laptop.frames[0]);
  assert.notDeepEqual(sheet.Walking.frames[0], sheet.Waving.frames[0]);
});

test("art generation is deterministic and loads once without a sprite fetch", async () => {
  assert.deepEqual(createSeedRobotSheet(), createSeedRobotSheet());
  const first = await loadClawd();
  assert.equal(await loadClawd(), first);
  assert.equal(first.Walking.palette[1], "#d97757");
  assert(
    !existsSync(
      fileURLToPath(
        new URL("../src/assets/clawd-sprites.json", import.meta.url),
      ),
    ),
  );
  const loader = readFileSync(
    new URL("../src/bots/clawd.ts", import.meta.url),
    "utf8",
  );
  assert(!loader.includes("clawd-sprites.json"));
});

test("the small-render fallback is round and orange with no screen or antenna", () => {
  for (const light of [true, false]) {
    const spec = botSpecs(light).claude;
    assert.equal(spec.shape.kind, "circle");
    assert(spec.paint && typeof spec.paint !== "string");
    assert.deepEqual(spec.paint.colors, ["#f5b59a", "#d97757", "#b95f42"]);
    assert.equal(spec.limbs?.legs.length, 2);
    assert.equal(spec.decor, undefined);
    assert.equal(spec.patch, undefined);
    assert.equal(spec.eyeColor, "#3b2520");
  }
});
