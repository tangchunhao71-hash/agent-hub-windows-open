import type { CompletionMotion } from "./completion-physics";

export interface PetSample {
  x: number;
  y: number;
  at: number;
}
export const PET_DRAG_DISTANCE = 4;
export const PET_DIZZY_MS = 2600;

/** An out-of-order up sample cannot end a grab until its down has been observed. */
export function petNativeTransition(downSeen: boolean, buttons: number | null) {
  if (buttons === null) return { downSeen, action: "ignore" as const };
  if ((buttons & 1) !== 0) return { downSeen: true, action: "move" as const };
  return {
    downSeen,
    action: downSeen ? ("release" as const) : ("ignore" as const),
  };
}

/** Release momentum uses recent motion; holding still does not replay an old fling. */
export function petThrowVelocity(
  samples: readonly PetSample[],
  releasedAt: number,
) {
  const last = samples.at(-1);
  if (!last || releasedAt - last.at > 100) return { vx: 0, vy: 0 };
  const first = samples.find((sample) => last.at - sample.at <= 100);
  if (!first || first === last) return { vx: 0, vy: 0 };
  const dt = Math.max(0.016, (last.at - first.at) / 1000);
  return {
    vx: Math.max(-900, Math.min(900, (last.x - first.x) / dt)),
    vy: Math.max(-1400, Math.min(1400, (last.y - first.y) / dt)),
  };
}

/** A substantial ground impact starts dizziness, independently of render cadence. */
export function petDizzyImpact(motion: CompletionMotion): number | null {
  return (
    motion.collisions.find(
      (c) => c.kind === "floor" && Math.abs(c.before.vy) >= 360,
    )?.at ?? null
  );
}

export function petPoint(
  point: { x: number; y: number },
  screen: { width: number; height: number },
  radius: number,
) {
  return {
    x: Math.max(radius, Math.min(screen.width - radius, point.x)),
    y: Math.max(radius, Math.min(screen.height - radius, point.y)),
  };
}
