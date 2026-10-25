import { test } from "node:test";
import assert from "node:assert/strict";
import {
  HUD_CONTOUR_SELECTOR,
  hudHitRegions,
} from "../src/lib/hud-hit-regions.ts";

function fixture(
  entries: {
    className: string;
    rect: { x: number; y: number; width: number; height: number };
  }[],
) {
  return {
    querySelectorAll(selector: string) {
      const classes = selector.split(",").map((part) => part.trim().slice(1));
      return entries
        .filter((entry) => classes.includes(entry.className))
        .map((entry) => ({ getBoundingClientRect: () => entry.rect }));
    },
  } as unknown as Pick<ParentNode, "querySelectorAll">;
}

test("only the pet button joins existing HUD regions; scenery and its moving parent stay click-through", () => {
  const root = fixture([
    {
      className: "hud-completion-drop",
      rect: { x: 0, y: 0, width: 1000, height: 800 },
    },
    {
      className: "hud-completion-pet-contour",
      rect: { x: 0, y: 0, width: 1000, height: 800 },
    },
    {
      className: "hud-completion-pet",
      rect: { x: 400, y: 768, width: 32, height: 32 },
    },
    {
      className: "hud-expanded-surface",
      rect: { x: 180, y: 400, width: 470, height: 300 },
    },
  ]);
  assert.deepEqual(hudHitRegions(root), [
    { x: 400, y: 768, width: 32, height: 32 },
    { x: 180, y: 400, width: 470, height: 300 },
  ]);
  assert.ok(HUD_CONTOUR_SELECTOR.includes(".hud-completion-pet-contour"));
});

test("dragging reports the current small button instead of a union spanning the desktop", () => {
  const button = {
    className: "hud-completion-pet",
    rect: { x: 400, y: 768, width: 32, height: 32 },
  };
  const root = fixture([button]);
  hudHitRegions(root);
  button.rect = { x: 700, y: 200, width: 32, height: 32 };
  assert.deepEqual(hudHitRegions(root), [button.rect]);
  button.rect = { x: 700, y: 200, width: 0, height: 0 };
  assert.deepEqual(hudHitRegions(root), []);
});
