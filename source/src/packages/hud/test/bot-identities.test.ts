import { after, test } from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import type { Session } from "@agent-hud/protocol";
import type { HUDConfig, HUDEdge, HUDVendor } from "../src/model.ts";
import {
  activeBotIdentities,
  identityMarkBox,
  stripIdentityBoxes,
} from "../src/bot-identities.ts";

// Load the real geometry and its model dependencies in memory. Middleware mode
// does not start an HTTP listener or produce a build; no runtime config is read.
const modules = await createServer({
  configFile: false,
  root: fileURLToPath(new URL("../src", import.meta.url)),
  server: { middlewareMode: true, watch: null },
  appType: "custom",
  logLevel: "silent",
});
after(() => modules.close());
const { stripMetrics, stripMarkCenter } = (await modules.ssrLoadModule(
  "/geometry.ts",
)) as typeof import("../src/geometry.ts");
const { defaultHUDConfig } = (await modules.ssrLoadModule(
  "/model.ts",
)) as typeof import("../src/model.ts");

test("gaze, drop and return anchors stay on the icon row, never its name", () => {
  for (const mode of ["logos", "simple"] as const)
    for (const edge of ["top", "bottom", "left", "right"] as const) {
      const c = config(mode, edge),
        metrics = stripMetrics(c, identityVendors(2)),
        strip = { x: 40, y: 60, width: metrics.width, height: metrics.height };
      const a = stripMarkCenter(c, strip, metrics, 0),
        b = stripMarkCenter(c, strip, metrics, 1);
      if (edge === "top" || edge === "bottom") {
        assert.equal(a.y, b.y);
        assert.ok(a.y < strip.y + strip.height / 2);
      } else {
        assert.equal(a.y, strip.y + metrics.centers[0]);
        assert.equal(b.y, strip.y + metrics.centers[1]);
      }
    }
});
const bot = (id: string, name: string, state: Session["state"] = "running") =>
  ({
    id,
    source: "grok-bot",
    state,
    botIdentity: {
      name,
      avatar: { kind: "shape", shape: "blob", color: "#00C972" },
    },
  }) as Session;
test("only known active Bot identities replace the source mark", () => {
  const sessions = [
    bot("b", "Beta"),
    bot("a", "Alpha", "waiting"),
    bot("done", "Done", "idle"),
    { ...bot("foreign", "Foreign"), source: "codex" },
  ];
  assert.deepEqual(
    activeBotIdentities({ working: 2, sessions }).map((b) => b.identity.name),
    ["Alpha", "Beta"],
  );
  assert.equal(activeBotIdentities({ working: 0, sessions }).length, 0);
  assert.equal(
    activeBotIdentities({
      working: 1,
      sessions: [
        { id: "missing", source: "grok-bot", state: "running" } as Session,
      ],
    }).length,
    0,
  );
});
test("duplicate rows are one Bot and heartbeat order cannot shuffle avatars", () => {
  const a = bot("a", "Same"),
    b = bot("b", "Same");
  assert.deepEqual(
    activeBotIdentities({ working: 3, sessions: [b, a, a] }).map(
      (b) => b.sessionId,
    ),
    ["a", "b"],
  );
});
test("names have real layout space and overflow keeps all full names", () => {
  const empty = identityMarkBox({ working: 0, sessions: [] }, 28);
  assert.equal(empty.width, 28);
  assert.equal(empty.height, 28);
  const sessions = [1, 2, 3, 4, 5].map((n) => bot(String(n), "Bot " + n));
  const box = identityMarkBox({ working: 5, sessions }, 28);
  assert.equal(box.visible.length, 3);
  assert.equal(box.extra.length, 2);
  assert.ok(box.width >= box.tile * 3 + 28);
  assert.equal(box.height, 44);
  assert.deepEqual(
    box.extra.map((b) => b.identity.name),
    ["Bot 4", "Bot 5"],
  );
});

const vendor = (key: string, sessions: Session[] = []): HUDVendor => ({
  key,
  name: key,
  used: 37,
  level: "normal",
  working: sessions.length,
  waiting: 0,
  balance: null,
  quotas: [],
  sessions,
});
const identityVendors = (count = 1) => [
  vendor("codex"),
  vendor(
    "grok-bot",
    Array.from({ length: count }, (_, index) =>
      bot(String(index), `Named Bot ${index}`),
    ),
  ),
  vendor("workbuddy"),
];
const config = (
  mode: "logos" | "simple",
  edge: HUDEdge,
  overrides: Partial<HUDConfig> = {},
): HUDConfig => ({
  ...defaultHUDConfig,
  mode,
  edge,
  logoSize: 28,
  logoEdgeDistance: 4,
  gapScale: 0.5,
  simpleSize: 28,
  simpleGapScale: 0.5,
  showsRings: true,
  showsNumbers: false,
  ...overrides,
});

test("one active identity creates equal icon and name rows for every Agent", () => {
  const boxes = stripIdentityBoxes(identityVendors(), 28, 13);
  assert.deepEqual(
    boxes.map(({ width, height, tile, label, gap }) => ({
      width,
      height,
      tile,
      label,
      gap,
    })),
    Array.from({ length: 3 }, () => ({
      width: 52,
      height: 44,
      tile: 52,
      label: 16,
      gap: 13,
    })),
  );
  assert.deepEqual(
    boxes.map((box) => box.visible.length),
    [0, 1, 0],
  );
  assert.deepEqual(
    boxes.map((box) => box.extra.length),
    [0, 0, 0],
  );
});

test("multiple Bots and overflow use the same Agent column width and Hub gap", () => {
  for (const mark of [20, 28, 60]) {
    for (const gap of [3, 11, 24]) {
      const boxes = stripIdentityBoxes(identityVendors(5), mark, gap);
      const [ordinary, group, other] = boxes;
      assert.equal(ordinary.tile, group.tile);
      assert.equal(other.tile, group.tile);
      assert.equal(ordinary.height, group.height);
      assert.equal(group.visible.length, 3);
      assert.equal(group.extra.length, 2);
      assert.equal(group.width, 4 * group.tile + 3 * gap);
      assert.equal(ordinary.width, group.tile);
      assert.equal(group.gap, gap);
      assert.equal(group.height - group.label, mark);
    }
  }
  const short = identityVendors(),
    long = identityVendors();
  long[1].sessions[0].botIdentity!.name = "Very long Bot name ".repeat(12);
  assert.equal(
    stripIdentityBoxes(short, 28, 9)[1].width,
    stripIdentityBoxes(long, 28, 9)[1].width,
  );
});

test("all four edges keep body centers separate from the name row in both modes", () => {
  for (const mode of ["logos", "simple"] as const) {
    for (const edge of ["top", "right", "bottom", "left"] as const) {
      const metrics = stripMetrics(config(mode, edge), identityVendors(2));
      const boxes = metrics.boxes!;
      assert.ok(boxes.every((box) => box.label === 16 && box.height === 44));
      assert.ok(
        boxes.every((box) => box.tile === 52 && box.gap === metrics.gap),
      );
      const horizontal = edge === "top" || edge === "bottom";
      const start =
        mode === "simple"
          ? horizontal
            ? metrics.padding.left
            : metrics.padding.top
          : 0;
      if (horizontal) {
        assert.equal(metrics.centers[0], start + 26);
        assert.equal(
          metrics.centers[1],
          start + 52 + metrics.gap + boxes[1].width / 2,
        );
      } else {
        assert.deepEqual(metrics.centers, [
          start + 14,
          start + 44 + metrics.gap + 14,
          start + 88 + 2 * metrics.gap + 14,
        ]);
        assert.notEqual(metrics.centers[0], start + boxes[0].height / 2);
      }
    }
  }
});

test("Simple numeric quota keeps its width outside the shared body columns", () => {
  for (const edge of ["top", "right", "bottom", "left"] as const) {
    const noNumber = stripMetrics(config("simple", edge), identityVendors(2));
    const numbered = stripMetrics(
      config("simple", edge, { showsNumbers: true }),
      identityVendors(2),
    );
    assert.deepEqual(
      numbered.boxes!.map((box) => [box.tile, box.label, box.height]),
      noNumber.boxes!.map((box) => [box.tile, box.label, box.height]),
    );
    assert.ok(numbered.width > noNumber.width);
    assert.equal(numbered.height, noNumber.height);
    assert.equal(numbered.centers[0], noNumber.centers[0]);
  }
});

test("without identities both modes retain their original four-edge geometry", () => {
  const ordinary = [vendor("codex"), vendor("grok-bot"), vendor("workbuddy")];
  for (const mode of ["logos", "simple"] as const) {
    for (const edge of ["top", "right", "bottom", "left"] as const) {
      const metrics = stripMetrics(config(mode, edge), ordinary);
      const horizontal = edge === "top" || edge === "bottom";
      assert.ok(
        metrics.boxes!.every(
          (box) =>
            box.width === 28 &&
            box.height === 28 &&
            box.tile === 28 &&
            box.label === 0 &&
            box.gap === 0,
        ),
      );
      if (mode === "logos") {
        assert.equal(metrics.width, horizontal ? 112 : 36);
        assert.equal(metrics.height, horizontal ? 36 : 112);
        assert.deepEqual(metrics.centers, [14, 56, 98]);
      } else {
        assert.equal(metrics.width, horizontal ? 116 : 43);
        assert.equal(metrics.height, horizontal ? 41 : 128);
        assert.deepEqual(
          metrics.centers,
          horizontal ? [20, 58, 96] : [22, 64, 106],
        );
      }
    }
  }
});
