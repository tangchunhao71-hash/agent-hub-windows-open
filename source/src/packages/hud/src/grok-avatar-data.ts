/** Motion/eye parameters observed in the installed Grok foundation-avatar renderer.
 * Rendering and playback are implemented locally; no client module is executed. */
export interface AvatarKeyframe {
  t: number;
  v: number[];
  ease?: number[];
}
export interface AvatarExpression {
  t: number;
  v: string;
  ease?: number[];
}
export interface AvatarClip {
  duration: number;
  rotation: AvatarKeyframe[];
  position: AvatarKeyframe[];
  scale: AvatarKeyframe[];
  squash: AvatarKeyframe[];
  expression: AvatarExpression[];
}
export interface AvatarEyeSeat {
  width: number;
  height: number;
  gap: number;
  shiftY: number;
}
export const GROK_AVATAR_CLIPS: Record<
  "idle" | "working" | "done",
  AvatarClip
> = {
  idle: {
    duration: 4,
    rotation: [
      {
        t: 0.901037,
        v: [0, 0, 0],
        ease: [0.42, 0, 0.289277, 0.962424],
      },
      {
        t: 1.616369,
        v: [0.120321, -8.658513, -2.542642],
        ease: [0.874781, -0.036259, 0.25467, 0.898023],
      },
      {
        t: 2.490205,
        v: [0.965955, 5.6589, 1.712924],
        ease: [0.602721, -0.23085, 0.29025, 0.981547],
      },
      {
        t: 3.45705,
        v: [0, 0, 0],
        ease: [0.42, 0, 0.58, 1],
      },
    ],
    position: [
      {
        t: 1.605482,
        v: [0, 0],
        ease: [0.42, 0, 0.58, 1],
      },
    ],
    scale: [
      {
        t: 0,
        v: [0.6],
        ease: [0.618045, 0.001243, 0.183913, 0.93587],
      },
      {
        t: 1.605482,
        v: [0.64],
        ease: [0.865871, -0.079955, 0.299791, 1.019512],
      },
      {
        t: 3.841942,
        v: [0.6],
        ease: [0.618045, 0.001243, 0.183913, 0.940553],
      },
    ],
    squash: [],
    expression: [
      {
        t: 1.481103,
        v: "blink",
        ease: [0.4, 0.22, 0.28, 1.34],
      },
      {
        t: 3.45705,
        v: "blink",
        ease: [0.4, 0.22, 0.28, 1.34],
      },
    ],
  },
  working: {
    duration: 4.8,
    rotation: [
      {
        t: 0.177467,
        v: [0, 0, 0],
        ease: [0.807067, 0.026673, 0.053973, 1.051967],
      },
      {
        t: 1.414107,
        v: [1.431495, -16.14032, -36.051754],
        ease: [0.705469, 0.152908, 0, 0.95078],
      },
      {
        t: 2.75,
        v: [-3.793664, 16.145027, 23.111556],
        ease: [0.814518, 0.058684, 0.718739, 0.822074],
      },
      {
        t: 3.30037,
        v: [-0.658471, 32.094525, 23.002842],
        ease: [0.952938, -0.034122, 0.714317, 0.525402],
      },
      {
        t: 3.89633,
        v: [-0.030182, -273.02901, 5.757593],
        ease: [0.112703, 0.66903, 0.231666, 0.973837],
      },
      {
        t: 4.782402,
        v: [-0.02728, -360, 0],
        ease: [0.42, 0, 0.58, 1],
      },
    ],
    position: [
      {
        t: 0.177467,
        v: [0, 0],
        ease: [0.807067, 0.026673, 0.053973, 1.051967],
      },
      {
        t: 1.414107,
        v: [-0.1, -0.000015],
        ease: [0.705469, 0.152908, 0, 0.901118],
      },
      {
        t: 2.75,
        v: [0.1, -0.001953],
        ease: [0.82402, -0.247927, 0.299393, 1],
      },
      {
        t: 4.782402,
        v: [0, 0],
        ease: [0.42, 0, 0.070717, 1.303216],
      },
    ],
    scale: [
      {
        t: 0.177792,
        v: [0.6],
        ease: [0.807067, 0.026673, 0.053973, 0.901002],
      },
      {
        t: 1.414107,
        v: [0.421584],
        ease: [0.758518, -0.08985, 0.30745, 0.907065],
      },
      {
        t: 2.25,
        v: [0.528152],
        ease: [1, 0.125291, 0.385163, 0.842635],
      },
      {
        t: 2.988336,
        v: [0.615339],
        ease: [0.618593, -0.268751, 0.341687, 1],
      },
      {
        t: 4.608224,
        v: [0.6],
        ease: [0.807067, 0.026673, 0.053973, 1.051967],
      },
    ],
    squash: [],
    expression: [
      {
        t: 0.746649,
        v: "inspect",
        ease: [0.741681, 0.0322, 0.264597, 1.021643],
      },
      {
        t: 2.556109,
        v: "surprised",
        ease: [0.741681, 0.0322, 0.264597, 1.021643],
      },
      {
        t: 3.759215,
        v: "neutral",
        ease: [0.741681, 0.0322, 0.264597, 1.021643],
      },
    ],
  },
  done: {
    duration: 1.8,
    rotation: [],
    position: [],
    scale: [],
    squash: [
      {
        t: 0,
        v: [1, 1],
        ease: [0.42, 0, 0.58, 1],
      },
      {
        t: 0.24,
        v: [1.12, 0.88],
        ease: [0.42, 0, 0.58, 1],
      },
      {
        t: 0.46,
        v: [0.93, 1.09],
        ease: [0.42, 0, 0.58, 1],
      },
      {
        t: 0.68,
        v: [1.04, 0.97],
        ease: [0.42, 0, 0.58, 1],
      },
      {
        t: 0.92,
        v: [1, 1],
        ease: [0.42, 0, 0.58, 1],
      },
    ],
    expression: [
      {
        t: 0.1,
        v: "happy",
        ease: [0.4, 0.22, 0.28, 1.34],
      },
    ],
  },
};
export const GROK_AVATAR_EYES: Record<string, AvatarEyeSeat> = {
  cloud: {
    width: 0.21294,
    height: 0.46683,
    gap: 0.470016,
    shiftY: 0.07,
  },
  square: {
    width: 0.255326,
    height: 0.559753,
    gap: 0.529408,
    shiftY: 0.12,
  },
  sparkle: {
    width: 0.18382,
    height: 0.40299,
    gap: 0.41216,
    shiftY: 0,
  },
  clover: {
    width: 0.23296,
    height: 0.51072,
    gap: 0.559104,
    shiftY: -0.02,
  },
  heartChubby: {
    width: 0.186576,
    height: 0.409032,
    gap: 0.446208,
    shiftY: 0.052,
  },
  starFlower: {
    width: 0.19292,
    height: 0.42294,
    gap: 0.382976,
    shiftY: 0.12,
  },
  teardrop: {
    width: 0.207792,
    height: 0.455544,
    gap: 0.452608,
    shiftY: 0.18,
  },
  tablet: {
    width: 0.2132,
    height: 0.4674,
    gap: 0.587776,
    shiftY: 0.02,
  },
  wedge: {
    width: 0.2132,
    height: 0.4674,
    gap: 0.467712,
    shiftY: 0.18,
  },
  house: {
    width: 0.229512,
    height: 0.503162,
    gap: 0.512512,
    shiftY: 0.184995,
  },
  star6: {
    width: 0.184782,
    height: 0.44966,
    gap: 0.419133,
    shiftY: 0.12,
  },
  pebble: {
    width: 0.2392,
    height: 0.5244,
    gap: 0.546304,
    shiftY: 0.12,
  },
  bean: {
    width: 0.2028,
    height: 0.4446,
    gap: 0.4608,
    shiftY: 0.12,
  },
  egg: {
    width: 0.2392,
    height: 0.5244,
    gap: 0.433664,
    shiftY: 0.112999,
  },
  squircle: {
    width: 0.2392,
    height: 0.5244,
    gap: 0.5632,
    shiftY: 0.12,
  },
  capsule: {
    width: 0.2106,
    height: 0.4617,
    gap: 0.360448,
    shiftY: 0.12,
  },
  cylinder: {
    width: 0.2392,
    height: 0.5244,
    gap: 0.47872,
    shiftY: 0.12,
  },
  hex: {
    width: 0.26,
    height: 0.57,
    gap: 0.64,
    shiftY: 0.12,
  },
  gem: {
    width: 0.2392,
    height: 0.5244,
    gap: 0.501248,
    shiftY: 0.12,
  },
  crystal: {
    width: 0.1612,
    height: 0.3534,
    gap: 0.3712,
    shiftY: 0.12,
  },
  shield: {
    width: 0.267904,
    height: 0.587328,
    gap: 0.444928,
    shiftY: 0.12,
  },
  dome: {
    width: 0.2132,
    height: 0.4674,
    gap: 0.47872,
    shiftY: 0.105998,
  },
  arch: {
    width: 0.1924,
    height: 0.4218,
    gap: 0.448,
    shiftY: 0.16,
  },
  leaf: {
    width: 0.1872,
    height: 0.4104,
    gap: 0.448,
    shiftY: 0.04,
  },
};
export const GROK_AVATAR_COLORS: Record<
  string,
  { light: string; dark: string }
> = {
  "#000000": {
    light: "#000000",
    dark: "#FFFFFF",
  },
  "#936439": {
    light: "#A27952",
    dark: "#855C36",
  },
  "#FF263C": {
    light: "#FF3E51",
    dark: "#E02135",
  },
  "#FF6700": {
    light: "#FF781C",
    dark: "#FF6700",
  },
  "#FF9800": {
    light: "#FFAF38",
    dark: "#FF9800",
  },
  "#00C972": {
    light: "#00C972",
    dark: "#009957",
  },
  "#00BCA6": {
    light: "#1CC3B0",
    dark: "#00A592",
  },
  "#1084FE": {
    light: "#2A92FE",
    dark: "#0E74E0",
  },
  "#9159FE": {
    light: "#A97EFE",
    dark: "#804EE0",
  },
  "#FF309B": {
    light: "#FF5EB1",
    dark: "#E02A88",
  },
  "#777777": {
    light: "#959595",
    dark: "#777777",
  },
};
