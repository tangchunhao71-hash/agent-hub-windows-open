import { test } from "node:test";
import assert from "node:assert/strict";
import { hudCompletionFloor } from "../src/lib/hud-completion-floor.ts";
import type {
  NativeHUDGeometry,
  NativeHUDMonitor,
} from "../src/lib/use-hud-geometry.ts";

function fixture(scale = 1, origin = 0) {
  const geometry: NativeHUDGeometry = {
    screenId: "screen",
    monitorIndex: 0,
    scaleFactor: scale,
    coordinateSpace: "monitor-scaled-physical-top-left",
    screen: {
      x: 0,
      y: origin / scale,
      width: 1920 / scale,
      height: 1080 / scale,
    },
    canvas: {
      x: 100 / scale,
      y: origin / scale,
      width: 940,
      height: 1080 / scale,
    },
    menuBarHeight: 24,
    hasNotch: false,
    notch: null,
    notchInViewport: null,
    placementConstrained: false,
  };
  const monitor: NativeHUDMonitor = {
    screenId: "screen",
    persistentId: null,
    runtimeId: "screen",
    windowLabel: "hud",
    index: 0,
    name: null,
    primary: true,
    scaleFactor: scale,
    notch: null,
    bounds: { x: 0, y: origin, width: 1920, height: 1080 },
    workArea: { x: 0, y: origin, width: 1920, height: 1032 },
  };
  return { geometry, monitor };
}

test("taskbar floor clears the entire minimum grab target at every bot size", () => {
  const { geometry, monitor } = fixture();
  const floor = hudCompletionFloor(geometry, [monitor])!;
  assert.equal(floor, 1008);
  for (const size of [20, 24, 32, 44, 60]) {
    const center: number = floor - size / 2;
    // At 45 degrees the rotating square's native bounding box is largest.
    const grabBottom: number = center + (Math.max(44, size) * Math.SQRT2) / 2;
    assert.ok(grabBottom <= monitor.workArea!.height - 2);
  }
});

test("physical insets scale once on a negative-origin 150% monitor", () => {
  const { geometry, monitor } = fixture(1.5, -1080);
  assert.equal(hudCompletionFloor(geometry, [monitor]), 1032 / 1.5 - 24);
  geometry.canvas.y += 20;
  geometry.canvas.height -= 20;
  assert.equal(hudCompletionFloor(geometry, [monitor]), 1032 / 1.5 - 44);
});

test("current monitor and live work-area changes determine the floor without a geometry event", () => {
  const { geometry, monitor } = fixture();
  const other = {
    ...monitor,
    screenId: "other",
    workArea: { ...monitor.workArea!, height: 600 },
  };
  assert.equal(hudCompletionFloor(geometry, [other, monitor]), 1008);
  monitor.workArea!.height = 1000;
  assert.equal(hudCompletionFloor(geometry, [other, monitor]), 976);
});

test("logical screen anchoring handles a different global coordinate origin", () => {
  const { geometry, monitor } = fixture(2, -1080);
  geometry.coordinateSpace = "global-logical-top-left";
  geometry.screen.y = 200;
  geometry.canvas.y = 220;
  assert.equal(hudCompletionFloor(geometry, [monitor]), 472);
});

test("missing work area keeps the canvas fallback and preview has no native floor", () => {
  const { geometry, monitor } = fixture();
  delete monitor.workArea;
  assert.equal(hudCompletionFloor(geometry, [monitor]), 1056);
  assert.equal(hudCompletionFloor(geometry, []), 1056);
  assert.equal(hudCompletionFloor(null, []), undefined);
  geometry.canvas.height = 300;
  assert.equal(hudCompletionFloor(geometry, []), 276);
});
