/** Logical pixels and seconds. The bot is approximated by a solid disk (I = mr²/2). */
export const COMPLETION_PHYSICS = {
  gravity: 2400,
  restitution: 0.42,
  wallRestitution: 0.5,
  launchSpeed: 150,
  rollingDeceleration: 180,
  minimumBounceHeight: 0.7,
} as const;

export interface CompletionPose {
  x: number;
  y: number;
  angle: number; // Radians, clockwise in screen coordinates.
  vx: number;
  vy: number;
  omega: number;
}
export interface CompletionSegment {
  at: number;
  duration: number;
  phase: "flight" | "roll";
  start: CompletionPose;
  ax: number;
}
export interface CompletionCollision {
  at: number;
  kind: "floor" | "wall" | "ceiling";
  before: CompletionPose;
  after: CompletionPose;
}
export interface CompletionMotion {
  radius: number;
  gravity: number;
  duration: number;
  segments: CompletionSegment[];
  collisions: CompletionCollision[];
  initial: CompletionPose;
  final: CompletionPose;
}

function advance(
  segment: CompletionSegment,
  t: number,
  radius: number,
  gravity: number,
): CompletionPose {
  const s = segment.start;
  const dx = s.vx * t + (segment.ax * t * t) / 2;
  return segment.phase === "flight"
    ? {
        x: s.x + dx,
        y: s.y + s.vy * t + (gravity * t * t) / 2,
        angle: s.angle + s.omega * t,
        vx: s.vx,
        vy: s.vy + gravity * t,
        omega: s.omega,
      }
    : {
        x: s.x + dx,
        y: s.y,
        angle: s.angle + dx / radius,
        vx: s.vx + segment.ax * t,
        vy: 0,
        omega: (s.vx + segment.ax * t) / radius,
      };
}

/** Exact constant-acceleration arcs, split at collisions. No frame-rate-dependent integration. */
export function completionMotion(
  origin: { x: number; y: number },
  screen: { width: number; height: number },
  size: number,
  release: Partial<Pick<CompletionPose, "vx" | "vy" | "omega" | "angle">> = {},
): CompletionMotion {
  const p = COMPLETION_PHYSICS;
  const radius = Math.max(0.5, Math.min(size, screen.width, screen.height) / 2);
  const left = radius,
    right = Math.max(left, screen.width - radius);
  const floor = Math.max(radius, screen.height - radius);
  const speed =
    right - left < 1
      ? 0
      : p.launchSpeed * (origin.x <= screen.width / 2 ? 1 : -1);
  let state: CompletionPose = {
    x: Math.max(left, Math.min(right, origin.x)),
    y: Math.max(radius, Math.min(floor, origin.y)),
    angle: release.angle ?? 0,
    vx: release.vx ?? speed,
    vy: release.vy ?? 0,
    omega: release.omega ?? (release.vx ?? speed) / radius,
  };
  const initial = { ...state };
  const segments: CompletionSegment[] = [],
    collisions: CompletionCollision[] = [];
  let at = 0,
    grounded = state.y >= floor && state.vy === 0;
  for (let event = 0; event < 128; event++) {
    if (grounded && Math.abs(state.vx) < 1e-8) break;
    const wallDistance = Math.max(
      0,
      state.vx > 0 ? right - state.x : state.x - left,
    );
    const ax = grounded ? -Math.sign(state.vx) * p.rollingDeceleration : 0;
    const stopTime = grounded
      ? Math.abs(state.vx) / p.rollingDeceleration
      : Infinity;
    const stopDistance = grounded
      ? (state.vx * state.vx) / (2 * p.rollingDeceleration)
      : Infinity;
    const wallTime =
      Math.abs(state.vx) < 1e-8 || wallDistance > stopDistance
        ? Infinity
        : grounded
          ? (Math.abs(state.vx) -
              Math.sqrt(
                Math.max(
                  0,
                  state.vx * state.vx -
                    2 * p.rollingDeceleration * wallDistance,
                ),
              )) /
            p.rollingDeceleration
          : wallDistance / Math.abs(state.vx);
    const floorTime = grounded
      ? Infinity
      : (-state.vy +
          Math.sqrt(
            state.vy * state.vy + 2 * p.gravity * Math.max(0, floor - state.y),
          )) /
        p.gravity;
    const ceilingDiscriminant =
      state.vy * state.vy - 2 * p.gravity * (state.y - radius);
    const ceilingTime =
      !grounded && state.vy < 0 && ceilingDiscriminant >= 0
        ? (-state.vy - Math.sqrt(ceilingDiscriminant)) / p.gravity
        : Infinity;
    const duration = Math.max(
      0,
      Math.min(wallTime, floorTime, ceilingTime, stopTime),
    );
    const segment: CompletionSegment = {
      at,
      duration,
      phase: grounded ? "roll" : "flight",
      start: { ...state },
      ax,
    };
    segments.push(segment);
    state = advance(segment, duration, radius, p.gravity);
    at += duration;
    if (grounded && stopTime <= wallTime) {
      state.vx = state.omega = 0;
      break;
    }
    const before = { ...state };
    if (wallTime < floorTime && wallTime <= ceilingTime) {
      state.x = Math.max(left, Math.min(right, state.x));
      state.vx *= -p.wallRestitution;
      // A ground contact keeps rolling; a frictionless airborne wall keeps spin.
      if (grounded) state.omega = state.vx / radius;
      collisions.push({ at, kind: "wall", before, after: { ...state } });
    } else if (ceilingTime < floorTime) {
      state.y = radius;
      state.vy *= -p.wallRestitution;
      collisions.push({ at, kind: "ceiling", before, after: { ...state } });
    } else {
      state.y = floor;
      const rebound = state.vy * p.restitution;
      grounded = (rebound * rebound) / (2 * p.gravity) < p.minimumBounceHeight;
      state.vy = grounded ? 0 : -rebound;
      // Rough-floor impulse removes contact slip without adding kinetic energy.
      const impulse = -(state.vx - state.omega * radius) / 3;
      state.vx += impulse;
      state.omega -= (2 * impulse) / radius;
      collisions.push({ at, kind: "floor", before, after: { ...state } });
    }
  }
  return {
    radius,
    gravity: p.gravity,
    duration: at,
    segments,
    collisions,
    initial,
    final: { ...state },
  };
}

/** Sampling by absolute elapsed time also handles a delayed/background animation frame. */
export function completionPose(
  motion: CompletionMotion,
  elapsed: number,
): CompletionPose {
  if (elapsed >= motion.duration) return { ...motion.final };
  const t = Math.max(0, elapsed);
  const segment = motion.segments.find((s) => t < s.at + s.duration);
  return segment
    ? advance(segment, t - segment.at, motion.radius, motion.gravity)
    : { ...motion.initial };
}

export function completionKeyframes(motion: CompletionMotion) {
  const count = Math.max(1, Math.ceil(motion.duration * 120));
  const times = Array.from(
    { length: count + 1 },
    (_, i) => (motion.duration * i) / count,
  );
  // Include exact impact times so interpolation never cuts through the floor.
  times.push(...motion.collisions.map((c) => c.at));
  times.sort((a, b) => a - b);
  const unique = times.filter((t, i) => i === 0 || t > times[i - 1] + 1e-9);
  const poses = unique.map((t) => completionPose(motion, t));
  return {
    times: unique.map((t) => (motion.duration ? t / motion.duration : 0)),
    x: poses.map((p) => p.x),
    y: poses.map((p) => p.y),
    rotate: poses.map((p) => (p.angle * 180) / Math.PI),
  };
}

/** A powered jump back to the dock, then the dock catches the bot. */
export function completionReturnMotion(
  from: CompletionPose,
  target: { x: number; y: number },
  radius: number,
): CompletionMotion {
  const gravity = COMPLETION_PHYSICS.gravity;
  const distance = Math.hypot(target.x - from.x, target.y - from.y);
  // Limit flight time so the parabolic apex cannot cross the top screen edge.
  const ceilingTime =
    Math.sqrt((2 * Math.max(0, from.y - radius)) / gravity) +
    Math.sqrt((2 * Math.max(0, target.y - radius)) / gravity);
  const duration = Math.max(
    0.001,
    Math.min(0.5 + Math.min(0.3, distance / 2500), ceilingTime * 0.96),
  );
  const angle = Math.round(from.angle / (Math.PI * 2)) * Math.PI * 2;
  const initial = {
    ...from,
    vx: (target.x - from.x) / duration,
    vy: (target.y - from.y) / duration - (gravity * duration) / 2,
    omega: (angle - from.angle) / duration,
  };
  const final = { ...from, ...target, angle, vx: 0, vy: 0, omega: 0 };
  return {
    radius,
    gravity,
    duration,
    initial,
    final,
    collisions: [],
    segments: [
      {
        at: 0,
        duration,
        phase: "flight",
        start: initial,
        ax: 0,
      },
    ],
  };
}
