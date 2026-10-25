import {
  GROK_AVATAR_CLIPS,
  GROK_AVATAR_EYES,
  GROK_AVATAR_COLORS,
  type AvatarKeyframe,
  type AvatarEyeSeat,
} from "./grok-avatar-data.ts";

export type GrokAvatarState = "idle" | "working" | "waiting" | "done";
export type GrokAvatarPoke = "bounce" | "nod" | "spin";
export const GROK_AVATAR_SHAPES = [
  "blob",
  "pebble",
  "bean",
  "egg",
  "squircle",
  "tablet",
  "capsule",
  "cylinder",
  "hex",
  "gem",
  "crystal",
  "wedge",
  "shield",
  "dome",
  "arch",
  "cloud",
  "teardrop",
  "leaf",
  "square",
  "sparkle",
  "clover",
  "flower",
  "heart",
  "house",
  "star",
  "pumpkin",
] as const;
export const clampAvatar = (n: number, lo = -1, hi = 1) =>
  Math.max(lo, Math.min(hi, n));
const radians = (n: number) => (n * Math.PI) / 180;
const mod = (a: number, b: number) => (a >= 0 ? a % b : ((a % b) + b) % b);
export function avatarSeed(value: string) {
  let seed = 2166136261;
  for (const char of value)
    seed = Math.imul(seed ^ char.codePointAt(0)!, 16777619) >>> 0;
  return seed / 4294967296;
}
/** Invert the time axis, including the original clips' deliberate overshoot. */
export function avatarEase(
  t: number,
  e: readonly number[] = [0.42, 0, 0.58, 1],
) {
  t = clampAvatar(t, 0, 1);
  if (t <= 1e-12) return 0;
  if (t >= 1 - 1e-12) return 1;
  const curve = (u: number, a: number, b: number) =>
    3 * (1 - u) * (1 - u) * u * a + 3 * (1 - u) * u * u * b + u * u * u;
  let lo = 0,
    hi = 1;
  for (let i = 0; i < 16; i++) {
    const mid = (lo + hi) / 2;
    if (curve(mid, e[0], e[2]) < t) lo = mid;
    else hi = mid;
  }
  return curve((lo + hi) / 2, e[1], e[3]);
}
function track(frames: AvatarKeyframe[], t: number, fallback: number[]) {
  if (!frames.length) return fallback;
  if (t <= frames[0].t) return frames[0].v;
  for (let i = 1; i < frames.length; i++)
    if (t < frames[i].t) {
      const a = frames[i - 1],
        b = frames[i],
        u = avatarEase((t - a.t) / (b.t - a.t), a.ease);
      return a.v.map((v, j) => v + (b.v[j] - v) * u);
    }
  return frames.at(-1)!.v;
}
export function avatarBlink(t: number, starts: readonly number[]) {
  let closed = 0;
  for (const start of starts) {
    const u = (t - start) / 0.26;
    if (u < 0 || u > 1) continue;
    closed = Math.max(
      closed,
      u < 0.45 ? avatarEase(u / 0.45) : 1 - avatarEase((u - 0.45) / 0.55),
    );
  }
  return 1 - closed;
}
export interface GrokAvatarPose {
  pitch: number;
  yaw: number;
  roll: number;
  x: number;
  y: number;
  scale: number;
  squashX: number;
  squashY: number;
  open: number;
  inspect: number;
  surprised: number;
  happy: number;
}
/** Small marks use Grok's compact Idle_B; working and completion use their own clips. */
export function sampleGrokAvatar(
  elapsed: number,
  state: GrokAvatarState,
  phase = 0,
  reduceMotion = false,
  poke?: { kind: GrokAvatarPoke; age: number },
): GrokAvatarPose {
  const still: GrokAvatarPose = {
    pitch: 0,
    yaw: 0,
    roll: 0,
    x: 0,
    y: 0,
    scale: 1,
    squashX: 1,
    squashY: 1,
    open: 1,
    inspect: 0,
    surprised: 0,
    happy: 0,
  };
  if (reduceMotion) return still;
  const kind =
    state === "working"
      ? "working"
      : state === "done" && elapsed < 1.8
        ? "done"
        : "idle";
  const clip = GROK_AVATAR_CLIPS[kind],
    t =
      kind === "done"
        ? Math.max(0, elapsed)
        : mod(elapsed + phase * clip.duration, clip.duration);
  const [pitch, yaw, roll] = track(clip.rotation, t, [0, 0, 0]),
    [x, y] = track(clip.position, t, [0, 0]),
    [zoom] = track(clip.scale, t, [0.6]),
    [squashX, squashY] = track(clip.squash, t, [1, 1]);
  const pose: GrokAvatarPose = {
    ...still,
    pitch,
    yaw,
    roll,
    x,
    y,
    scale: zoom / 0.6,
    squashX,
    squashY,
    open: avatarBlink(
      t,
      clip.expression.filter((k) => k.v === "blink").map((k) => k.t),
    ),
  };
  let previous = "neutral";
  for (const key of clip.expression) {
    if (key.v === "blink") continue;
    if (key.t > t) break;
    const u = avatarEase(clampAvatar((t - key.t) / 0.28, 0, 1));
    for (const name of ["inspect", "surprised", "happy"] as const)
      pose[name] =
        (previous === name ? 1 : 0) * (1 - u) + (key.v === name ? 1 : 0) * u;
    previous = key.v;
  }
  if (poke && poke.age >= 0) {
    const duration =
        poke.kind === "bounce" ? 0.62 : poke.kind === "nod" ? 0.5 : 0.9,
      u = poke.age / duration;
    if (u <= 1) {
      if (poke.kind === "spin") pose.yaw += 360 * avatarEase(u);
      else if (poke.kind === "nod") pose.pitch += 16 * Math.sin(Math.PI * u);
      else {
        const air = Math.sin(Math.PI * u);
        pose.y += 0.3 * air;
        const squash = track(
          [
            { t: 0, v: [1, 1] },
            { t: 0.12, v: [1.08, 0.9] },
            { t: 0.5, v: [0.96, 1.06] },
            { t: 0.82, v: [1.08, 0.9] },
            { t: 1, v: [1, 1] },
          ],
          u,
          [1, 1],
        );
        pose.squashX *= squash[0];
        pose.squashY *= squash[1];
      }
    }
  }
  return pose;
}
export function smoothAvatarGaze(
  current: readonly number[],
  target: readonly number[],
  dt: number,
): [number, number] {
  const remain = Math.pow(0.86, clampAvatar(dt, 0, 0.1) * 60);
  return [0, 1].map(
    (i) => current[i] + (clampAvatar(target[i]) - current[i]) * (1 - remain),
  ) as [number, number];
}
export function grokAvatarColor(base: string, light: boolean) {
  const color = GROK_AVATAR_COLORS[base.toUpperCase()];
  return color
    ? light
      ? color.light
      : color.dark
    : /^#[0-9a-f]{6}$/i.test(base)
      ? base
      : "#777777";
}
/** A lost native sample clears gaze; undefined retains local DOM pointer mode. */
export function avatarGazeTarget(
  pointer: { x: number; y: number } | null | undefined,
  rect: {
    left: number;
    right: number;
    top: number;
    bottom: number;
    width: number;
    height: number;
  },
  local: readonly [number, number],
): [number, number] {
  if (pointer === undefined) return [...local];
  if (
    pointer === null ||
    rect.width <= 0 ||
    rect.height <= 0 ||
    pointer.x < rect.left ||
    pointer.x > rect.right ||
    pointer.y < rect.top ||
    pointer.y > rect.bottom
  )
    return [0, 0];
  return [
    clampAvatar((pointer.x - rect.left - rect.width / 2) / (rect.width * 0.5)),
    clampAvatar((rect.top + rect.height / 2 - pointer.y) / (rect.width * 0.5)),
  ];
}
export function grokEyeSeat(shape: string): AvatarEyeSeat {
  const alias: Record<string, string> = {
    flower: "starFlower",
    heart: "heartChubby",
    star: "star6",
  };
  return (
    GROK_AVATAR_EYES[alias[shape] ?? shape] ?? {
      width: 0.26,
      height: 0.57,
      gap: 0.64,
      shiftY: 0.12,
    }
  );
}

type Vertex = readonly [number, number];
export interface GrokAvatarModel {
  outline: Vertex[];
  depth: number;
  sphere: boolean;
}
const superellipse = (a: number, b: number, n: number, count = 64): Vertex[] =>
  Array.from({ length: count }, (_, i) => {
    const t = (i / count) * Math.PI * 2,
      c = Math.cos(t),
      s = Math.sin(t);
    return [
      a * Math.sign(c) * Math.abs(c) ** (2 / n),
      b * Math.sign(s) * Math.abs(s) ** (2 / n),
    ];
  });
function roundedPolygon(vertices: Vertex[], round = 0.18): Vertex[] {
  return vertices.flatMap((v, i) => {
    const before = vertices[(i + vertices.length - 1) % vertices.length],
      after = vertices[(i + 1) % vertices.length];
    const a = [
        v[0] + (before[0] - v[0]) * round,
        v[1] + (before[1] - v[1]) * round,
      ],
      b = [v[0] + (after[0] - v[0]) * round, v[1] + (after[1] - v[1]) * round];
    return Array.from({ length: 8 }, (_, j) => {
      const u = j / 7;
      return [
        (1 - u) ** 2 * a[0] + 2 * u * (1 - u) * v[0] + u * u * b[0],
        (1 - u) ** 2 * a[1] + 2 * u * (1 - u) * v[1] + u * u * b[1],
      ] as Vertex;
    });
  });
}
function petals(count: number, depth: number): Vertex[] {
  return Array.from({ length: 96 }, (_, i) => {
    const a = (i / 96) * Math.PI * 2,
      r = 1 - depth + depth * Math.cos(count * a);
    return [r * Math.sin(a), -r * Math.cos(a)];
  });
}
/** Parameterized silhouettes, not images or code extracted from the client. */
export function grokAvatarModel(shape: string): GrokAvatarModel {
  let outline: Vertex[],
    depth = 0.36;
  switch (shape) {
    case "blob":
      outline = superellipse(1, 1, 2);
      depth = 1;
      break;
    case "pebble":
      outline = superellipse(0.92, 1, 2.5);
      depth = 0.65;
      break;
    case "egg":
      outline = superellipse(0.87, 1, 2).map(([x, y]) => [
        x * (1 - 0.13 * y),
        y,
      ]);
      depth = 0.65;
      break;
    case "bean":
      outline = superellipse(0.85, 0.95, 2).map(([x, y]) => [
        x + 0.19 * (1 - y * y),
        y * (1 + 0.12 * x),
      ]);
      depth = 0.5;
      break;
    case "squircle":
      outline = superellipse(107 / 114.2705, 107 / 114.2705, 3.2);
      depth = 0.28;
      break;
    case "square":
      outline = superellipse(0.94, 0.94, 5.5);
      break;
    case "tablet":
      outline = superellipse(1, 74 / 114, 3.3);
      depth = 0.5;
      break;
    case "capsule":
      outline = superellipse(72 / 114, 1, 3.1);
      depth = 0.55;
      break;
    case "cylinder":
      outline = superellipse(0.83, 1, 6);
      depth = 0.7;
      break;
    case "hex":
      outline = roundedPolygon(
        Array.from({ length: 6 }, (_, i) => [
          Math.cos((i * Math.PI) / 3 + Math.PI / 6),
          Math.sin((i * Math.PI) / 3 + Math.PI / 6),
        ]),
        0.17,
      );
      break;
    case "gem":
      outline = roundedPolygon(
        [
          [-0.65, -0.9],
          [0.65, -0.9],
          [1, -0.35],
          [0, 1],
          [-1, -0.35],
        ],
        0.12,
      );
      depth = 0.6;
      break;
    case "crystal":
      outline = roundedPolygon(
        [
          [0, -1],
          [0.68, -0.4],
          [0.53, 0.7],
          [0, 1],
          [-0.53, 0.7],
          [-0.68, -0.4],
        ],
        0.02,
      );
      depth = 0.4;
      break;
    case "wedge":
      outline = roundedPolygon(
        [
          [0, -1],
          [1, 0.8],
          [-1, 0.8],
        ],
        0.24,
      );
      depth = 0.55;
      break;
    case "shield":
      outline = roundedPolygon(
        [
          [-0.9, -0.9],
          [0.9, -0.9],
          [0.85, 0.3],
          [0, 1],
          [-0.85, 0.3],
        ],
        0.28,
      );
      depth = 0.45;
      break;
    case "dome":
      outline = superellipse(1, 1, 2).map(([x, y]) => [
        x,
        Math.max(-0.8, Math.min(0.65, y)),
      ]);
      depth = 0.6;
      break;
    case "arch":
      outline = superellipse(0.8, 1, 2.5).map(([x, y]) => [
        x,
        Math.min(0.7, y),
      ]);
      depth = 0.6;
      break;
    case "cloud":
      outline = petals(5, 0.12).map(([x, y]) => [x * 1.08, y * 0.73 + 0.06]);
      depth = 0.36;
      break;
    case "teardrop":
      outline = superellipse(1, 1, 2).map(([x, y]) => [
        x * (0.66 + 0.34 * y),
        y,
      ]);
      depth = 0.6;
      break;
    case "leaf":
      outline = superellipse(0.8, 1, 1.4).map(([x, y]) => [x + 0.14 * y, y]);
      depth = 0.45;
      break;
    case "sparkle":
      outline = petals(4, 0.33);
      break;
    case "clover":
      outline = petals(4, 0.16);
      break;
    case "flower":
      outline = petals(6, 0.12);
      break;
    case "heart":
      outline = Array.from({ length: 96 }, (_, i) => {
        const t = (i / 96) * Math.PI * 2;
        return [
          Math.sin(t) ** 3,
          -(
            13 * Math.cos(t) -
            5 * Math.cos(2 * t) -
            2 * Math.cos(3 * t) -
            Math.cos(4 * t)
          ) / 16,
        ];
      });
      depth = 0.31;
      break;
    case "house":
      outline = roundedPolygon(
        [
          [0, -1],
          [-0.88, -0.22],
          [-0.88, 0.92],
          [0.88, 0.92],
          [0.88, -0.22],
        ],
        0.16,
      );
      break;
    case "star":
      outline = roundedPolygon(
        Array.from({ length: 12 }, (_, i) => {
          const a = (i * Math.PI) / 6 - Math.PI / 2,
            r = i % 2 ? 0.58 : 1;
          return [r * Math.cos(a), r * Math.sin(a)];
        }),
        0.13,
      );
      depth = 0.3;
      break;
    case "pumpkin":
      outline = petals(7, 0.055).map(([x, y]) => [x, y * 0.9]);
      depth = 0.6;
      break;
    default:
      return grokAvatarModel("blob");
  }
  return { outline, depth, sphere: shape === "blob" };
}
const pointPath = (points: readonly Vertex[]) =>
  points.length
    ? points
        .map(([x, y], i) => `${i ? "L" : "M"}${x.toFixed(3)} ${y.toFixed(3)}`)
        .join("") + "Z"
    : "";
export function projectGrokAvatar(
  model: GrokAvatarModel,
  shape: string,
  pose: GrokAvatarPose,
  gaze: readonly number[],
) {
  const gx = clampAvatar(gaze[0]),
    gy = clampAvatar(gaze[1]),
    yaw = radians(pose.yaw + gx * (model.sphere ? 18 : 26)),
    pitch = radians(pose.pitch + gy * (model.sphere ? 14 : 19)),
    roll = radians(pose.roll);
  const cy = Math.cos(yaw),
    sy = Math.sin(yaw),
    cp = Math.cos(pitch),
    sp = Math.sin(pitch),
    cr = Math.cos(roll),
    sr = Math.sin(roll);
  const project = (x: number, y: number, z: number): Vertex => {
    const px = x * cy + z * sy,
      pz = -x * sy + z * cy,
      py = y * cp - pz * sp;
    return [
      (px * cr - py * sr) * 30 * pose.scale * pose.squashX +
        (pose.x + gx * (model.sphere ? 0.03 : 0.07)) * 30,
      (px * sr + py * cr) * 30 * pose.scale * pose.squashY -
        (pose.y + gy * (model.sphere ? 0.024 : 0.055)) * 30,
    ];
  };
  let front: Vertex[],
    back: Vertex[],
    sides = "";
  if (model.sphere) {
    front = model.outline.map(([x, y]) => [
      x * 30 * pose.scale * pose.squashX + pose.x * 30,
      y * 30 * pose.scale * pose.squashY - pose.y * 30,
    ]);
    back = front;
  } else {
    front = model.outline.map(([x, y]) => project(x, y, model.depth));
    back = model.outline.map(([x, y]) => project(x, y, -model.depth));
    for (let i = 0; i < front.length; i++) {
      const j = (i + 1) % front.length;
      sides += pointPath([front[i], front[j], back[j], back[i]]);
    }
  }
  const eye = grokEyeSeat(shape),
    eyes: string[] = [];
  if (cy * cp > 0.04) {
    for (const side of [-1, 1]) {
      const width =
          eye.width * (1 + 0.14 * pose.surprised - 0.18 * pose.inspect),
        height =
          eye.height *
          (1 + 0.1 * pose.surprised - 0.15 * pose.inspect) *
          Math.max(0.012, pose.open),
        cx = (side * eye.gap) / 2,
        ey = eye.shiftY;
      const path: Vertex[] = Array.from({ length: 32 }, (_, i) => {
        const a = (i / 32) * Math.PI * 2;
        const radius = Math.min(width, height) / 2,
          c = Math.cos(a),
          s = Math.sin(a);
        let x = c * radius + Math.sign(c) * (width / 2 - radius),
          y = s * radius + Math.sign(s) * (height / 2 - radius);
        if (pose.happy > 0.01) {
          const u = x / (width / 2);
          y =
            (y * 0.28 - (1 - u * u) * height * 0.2) * pose.happy +
            y * (1 - pose.happy);
        }
        const turn = side * pose.inspect * 0.16,
          nx = x * Math.cos(turn) - y * Math.sin(turn) + cx,
          ny = x * Math.sin(turn) + y * Math.cos(turn) + ey;
        const z = model.sphere
          ? Math.sqrt(Math.max(0.02, 1 - nx * nx - ny * ny))
          : model.depth + 0.006;
        return project(nx, ny, z);
      });
      eyes.push(pointPath(path));
    }
  }
  return {
    front: pointPath(cy * cp >= 0 ? front : back),
    back: pointPath(cy * cp >= 0 ? back : front),
    sides,
    eyes: eyes.join(""),
  };
}
