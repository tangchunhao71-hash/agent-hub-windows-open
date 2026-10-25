// Independently drawn Agent Hub artwork, Apache-2.0. No extracted sprite data.
// Clip keys and lengths are the existing animation player's compatibility API.
import { clawdClips } from "./clawd-clips";
import type { ClawdSheet } from "./clawd";

export const seedRobot = {
  name: "Orange companion",
  license: "Apache-2.0",
  palette: [
    "#693b2e", // outline and feet
    "#d97757", // orange robot shell; Claude source remains labelled separately
    "#f5b59a", // shell highlight
    "#3b2520", // face panel
    "#fff6ed", // eyes
    "#ffe3c9", // antenna indicator
    "#f4c66d", // status light
    "#f6f1df", // paper / desk
    "#8eace7", // screen / ideas
  ],
} as const;

const size = 40;
const height = 48;
const headroom = 8;
type Clip = keyof typeof clawdClips.frameCounts;

/** All pixels are drawn from simple geometry, not decoded or traced from art. */
function picture(name: Clip, index: number, count: number): Uint8Array {
  const pixels = new Uint8Array(size * height).fill(255);
  const point = (x: number, y: number, ink: number) => {
    x = Math.round(x);
    y = Math.round(y) + headroom;
    if (x >= 0 && y >= 0 && x < size && y < height) pixels[y * size + x] = ink;
  };
  const rect = (x: number, y: number, w: number, h: number, ink: number) => {
    for (let yy = 0; yy < h; yy++)
      for (let xx = 0; xx < w; xx++) point(x + xx, y + yy, ink);
  };
  const star = (x: number, y: number, ink: number) => {
    rect(x - 2, y, 5, 1, ink);
    rect(x, y - 2, 1, 5, ink);
  };
  const wave = Math.sin((index / Math.max(1, count - 1)) * Math.PI * 2);
  const pulse = Math.sin((index / Math.max(1, count - 1)) * Math.PI);
  const walking = name === "Walking";
  const joyful = ["DancingHappy", "Excited", "JumpingHappy"].includes(name);
  const jump = name === "JumpingHappy" ? Math.round(pulse * 7) : 0;
  const float = name === "Meditating" ? Math.round(pulse * 2) : 0;
  const dx = ["DancingHappy", "Swaying", "Dizzy"].includes(name)
    ? Math.round(wave * 2)
    : 0;
  const dy = -jump - float - (joyful ? Math.round(Math.abs(wave) * 2) : 0);
  const body = (x: number, y: number, w: number, h: number, ink: number) =>
    rect(x + dx, y + dy, w, h, ink);

  // One plump, round orange body. No mechanical head, screen or antenna.
  const ellipse = (
    cx: number,
    cy: number,
    rx: number,
    ry: number,
    ink: number,
  ) => {
    for (let y = -ry; y <= ry; y++)
      for (let x = -rx; x <= rx; x++)
        if ((x * x) / (rx * rx) + (y * y) / (ry * ry) <= 1)
          body(cx + x, cy + y, 1, 1, ink);
  };
  ellipse(20, 23, 12, 11, 0);
  ellipse(20, 22, 11, 10, 1);
  ellipse(17, 17, 6, 3, 2);
  ellipse(17, 16, 3, 1, 5);

  const look = ["LookingAround", "LookingAroundEyesOnly", "Turning"].includes(
    name,
  )
    ? Math.round(wave * 2)
    : 0;
  if (name === "Meditating" || (index > 0 && index % 23 === 19)) {
    body(16 + look, 22, 3, 1, 3);
    body(23 + look, 22, 3, 1, 3);
  } else if (name === "Dizzy") {
    body(17, 21, 1, 3, 8);
    body(16, 22, 3, 1, 8);
    body(22, 21, 1, 3, 8);
    body(21, 22, 3, 1, 8);
  } else {
    body(16 + look, 20, 3, joyful ? 3 : 5, 3);
    body(23 + look, 20, 3, joyful ? 3 : 5, 3);
  }
  body(20, 27, 3, 1, 3);
  body(12, 25, 3, 1, 2);
  body(27, 25, 3, 1, 2);

  const stride = walking ? Math.round(wave * 2) : 0;
  body(13 - stride, 34, 5, 2, 1);
  body(23 + stride, 34, 5, 2, 1);
  const raised = ["Waving", "Pointing", "Excited", "DancingHappy"].includes(
    name,
  );
  body(6, joyful ? 19 : 24 + stride, 4, 3, 1);
  body(31, raised ? 18 + Math.round(wave) : 24 - stride, 4, 3, 1);

  if (name === "Facepalm") {
    body(26, 21, 3, 3, 1);
    body(23, 20, 5, 2, 2);
  }
  if (name === "Pointing") {
    body(29, 18, 6, 2, 2);
    body(34, 16, 2, 3, 6);
  }
  if (name === "Laptop" || name === "Desktop") {
    const screenX = name === "Laptop" ? 25 : 28;
    rect(screenX, 18, 10, 10, 0);
    rect(screenX + 1, 19, 8, 7, 8);
    rect(screenX + 2, 21, 3 + (index % 3), 1, 4);
    rect(screenX + 2, 23, 4, 1, 3);
    rect(screenX - 2, 28, 13, 2, 0);
    if (name === "Desktop") {
      rect(9, 30, 28, 2, 7);
      rect(10, 32, 2, 4, 0);
      rect(34, 32, 2, 4, 0);
    }
  }
  if (name === "Thinking") {
    rect(28, 11, 2, 2, 8);
    rect(31, 8, 2, 2, 8);
    rect(33, 4, 3, 3, index % 10 < 5 ? 5 : 6);
  }
  if (name === "Lightbulb") {
    rect(31, 4, 4, 1, 6);
    rect(30, 5, 6, 4, 6);
    rect(32, 9, 2, 3, 7);
    if (index % 12 < 6) {
      rect(27, 6, 2, 1, 5);
      rect(37, 6, 2, 1, 5);
    }
  }
  if (name === "Spark") {
    star(33, 10 + Math.round(wave * 2), 6);
    star(7, 12 - Math.round(wave * 2), 8);
    rect(28, 5 + (index % 4), 1, 2, 5);
  }
  return pixels;
}

/** Reuses animation timing contracts only; every clip contains new pixel frames. */
export function createSeedRobotSheet(): ClawdSheet {
  return Object.fromEntries(
    Object.entries(clawdClips.frameCounts).map(([name, count]) => [
      name,
      {
        width: size,
        height,
        anchorX: 20,
        anchorY: 34 + headroom,
        palette: [...seedRobot.palette],
        frames: Array.from({ length: count }, (_, index) =>
          picture(name as Clip, index, count),
        ),
      },
    ]),
  );
}
