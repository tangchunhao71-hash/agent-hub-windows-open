import {
  useEffect,
  useRef,
  useState,
  type PointerEvent,
  type MouseEvent,
} from "react";
import {
  completionMotion,
  completionPose,
  type CompletionMotion,
  type CompletionPose,
} from "./completion-physics";
import {
  PET_DRAG_DISTANCE,
  PET_DIZZY_MS,
  petThrowVelocity,
  petDizzyImpact,
  petPoint,
  petNativeTransition,
  type PetSample,
} from "./pet-gesture";
import type { HUDNativePointer } from "./useHUDInteraction";

export interface PetControls {
  holding: boolean;
  pose: CompletionPose | null;
  dizzy: boolean;
  label: string;
  onPointerDown(event: PointerEvent<HTMLButtonElement>): void;
  onPointerMove(event: PointerEvent<HTMLButtonElement>): void;
  onPointerUp(event: PointerEvent<HTMLButtonElement>): void;
  onPointerCancel(event: PointerEvent<HTMLButtonElement>): void;
  onLostPointerCapture(event: PointerEvent<HTMLButtonElement>): void;
  onClick(event: MouseEvent<HTMLButtonElement>): void;
}
interface PetState {
  id: string | null;
  motion: CompletionMotion | null;
  held: CompletionPose | null;
  sequence: number;
  startedAt: number;
  open: boolean;
  dizzyAt: number;
  dizzyUntil: number;
}
interface Grab {
  id: string;
  pointerId: number;
  target: HTMLButtonElement;
  press: { x: number; y: number };
  offset: { x: number; y: number };
  pose: CompletionPose;
  samples: PetSample[];
  moved: boolean;
  nativeAtPress: HUDNativePointer | undefined;
  radius: number;
  pointerType: string;
  nativeDownSeen: boolean;
}
const empty = (id: string | null): PetState => ({
  id,
  motion: null,
  held: null,
  sequence: 0,
  startedAt: 0,
  open: true,
  dizzyAt: Infinity,
  dizzyUntil: 0,
});

/** UI-only play state is owned by a completion id; provider/session facts never change. */
export function useCompletionPet(
  id: string | null,
  screen: { width: number; height: number },
  options: {
    disabled: boolean;
    reduceMotion: boolean;
    nativePointer?: HUDNativePointer;
  },
) {
  const [state, setState] = useState(() => empty(id));
  const latest = useRef(state);
  latest.current = state;
  const grab = useRef<Grab | null>(null);
  const suppressClick = useRef(false);
  const [, tick] = useState(0);
  const environment = useRef({ id, screen, options });
  environment.current = { id, screen, options };
  const current = state.id === id ? state : empty(id);
  const local = (event: PointerEvent<HTMLButtonElement>) => {
    const bounds = event.currentTarget
      .closest(".hud-scene")!
      .getBoundingClientRect();
    return { x: event.clientX - bounds.left, y: event.clientY - bounds.top };
  };
  const clearGrab = () => {
    const held = grab.current;
    grab.current = null;
    if (held?.target.hasPointerCapture(held.pointerId))
      held.target.releasePointerCapture(held.pointerId);
  };
  const move = (point: { x: number; y: number }) => {
    const held = grab.current,
      env = environment.current;
    if (!held || held.id !== env.id) return;
    const now = performance.now(),
      radius = held.radius;
    const next = petPoint(
      { x: point.x - held.offset.x, y: point.y - held.offset.y },
      env.screen,
      radius,
    );
    if (
      !held.moved &&
      Math.hypot(point.x - held.press.x, point.y - held.press.y) <
        PET_DRAG_DISTANCE
    )
      return;
    held.moved = true;
    held.pose = { ...held.pose, ...next, vx: 0, vy: 0, omega: 0 };
    held.samples = [
      ...held.samples.filter((s) => now - s.at <= 120),
      { ...next, at: now },
    ];
    setState((previous) => ({
      ...(previous.id === held.id ? previous : empty(held.id)),
      held: held.pose,
      open: true,
    }));
  };
  const release = (cancelled = false) => {
    const held = grab.current,
      env = environment.current;
    if (!held) return;
    clearGrab();
    if (!held.moved || held.id !== env.id) return;
    suppressClick.current = true;
    const radius = held.radius;
    const velocity = cancelled
      ? { vx: 0, vy: 0 }
      : petThrowVelocity(held.samples, performance.now());
    const motion = completionMotion(held.pose, env.screen, radius * 2, {
      ...velocity,
      angle: held.pose.angle,
      omega: velocity.vx / radius,
    });
    const now = Date.now(),
      impact = petDizzyImpact(motion);
    setState((previous) => ({
      ...empty(held.id),
      motion,
      sequence: (previous.id === held.id ? previous.sequence : 0) + 1,
      startedAt: now,
      dizzyAt:
        impact === null
          ? Infinity
          : now + (env.options.reduceMotion ? 0 : impact * 1000),
      dizzyUntil:
        impact === null
          ? 0
          : now +
            (env.options.reduceMotion ? 0 : motion.duration * 1000) +
            PET_DIZZY_MS,
    }));
  };
  useEffect(() => {
    if (grab.current?.id !== id || options.disabled) {
      clearGrab();
      setState((previous) =>
        previous.id !== id
          ? empty(id)
          : previous.held
            ? { ...previous, held: null }
            : previous,
      );
    }
  }, [id, options.disabled]);
  useEffect(() => {
    const held = grab.current,
      pointer = options.nativePointer;
    // Do not treat the pre-press native snapshot as a release.
    if (
      !held ||
      !pointer ||
      pointer === held.nativeAtPress ||
      pointer.buttons === null
    )
      return;
    if (held.pointerType !== "mouse") return;
    const next = petNativeTransition(held.nativeDownSeen, pointer.buttons);
    held.nativeDownSeen = next.downSeen;
    if (next.action === "ignore") return;
    if (pointer.point) move(pointer.point);
    if (next.action === "release") release();
  }, [options.nativePointer]);
  const viewport = useRef(`${screen.width}:${screen.height}`);
  useEffect(() => {
    const key = `${screen.width}:${screen.height}`;
    if (viewport.current === key) return;
    viewport.current = key;
    const heldRadius = grab.current?.radius;
    clearGrab();
    setState((previous) => {
      if (previous.id !== id || (!previous.motion && !previous.held))
        return previous;
      const now = Date.now();
      const radius = heldRadius ?? previous.motion?.radius ?? 13;
      const pose =
        previous.held ??
        completionPose(previous.motion!, (now - previous.startedAt) / 1000);
      const point = petPoint(pose, screen, radius);
      const motion = completionMotion(point, screen, radius * 2, {
        vx: pose.vx,
        vy: pose.vy,
        omega: pose.omega,
        angle: pose.angle,
      });
      return {
        ...previous,
        motion,
        held: null,
        sequence: previous.sequence + 1,
        startedAt: now,
      };
    });
  }, [screen.width, screen.height]);
  useEffect(() => {
    const blur = () => release(true);
    window.addEventListener("blur", blur);
    return () => {
      window.removeEventListener("blur", blur);
      clearGrab();
    };
  }, []);
  useEffect(() => {
    if (current.dizzyUntil <= Date.now()) return;
    const timers = [current.dizzyAt, current.dizzyUntil]
      .filter((t) => Number.isFinite(t) && t > Date.now())
      .map((at) =>
        setTimeout(() => tick((n) => n + 1), Math.max(0, at - Date.now())),
      );
    return () => timers.forEach(clearTimeout);
  }, [current.id, current.sequence, current.dizzyAt, current.dizzyUntil]);
  const dizzy =
    Date.now() >= current.dizzyAt &&
    Date.now() < current.dizzyUntil &&
    !current.held;
  return {
    motion: current.motion,
    holding: Boolean(current.held),
    held: current.held,
    sequence: current.sequence,
    open: current.open,
    dizzy,
    getPose: (fallback: CompletionMotion) =>
      grab.current?.pose ??
      (latest.current.id === id && latest.current.motion
        ? completionPose(
            latest.current.motion,
            options.reduceMotion
              ? Infinity
              : (Date.now() - latest.current.startedAt) / 1000,
          )
        : fallback.final),
    cancel: clearGrab,
    controls: (
      pose: CompletionPose,
      size: number,
      label: string,
    ): PetControls => ({
      holding: Boolean(current.held),
      pose: current.held,
      dizzy,
      label,
      onPointerDown: (event) => {
        if (
          event.button !== 0 ||
          event.isPrimary === false ||
          grab.current ||
          !id ||
          options.disabled
        )
          return;
        event.stopPropagation();
        event.preventDefault();
        suppressClick.current = false;
        const point = local(event);
        grab.current = {
          id,
          pointerId: event.pointerId,
          target: event.currentTarget,
          press: point,
          offset: { x: point.x - pose.x, y: point.y - pose.y },
          pose,
          samples: [{ x: pose.x, y: pose.y, at: performance.now() }],
          moved: false,
          nativeAtPress: options.nativePointer,
          radius: size / 2,
          pointerType: event.pointerType || "mouse",
          nativeDownSeen: false,
        };
        event.currentTarget.setPointerCapture(event.pointerId);
      },
      onPointerMove: (event) => {
        if (grab.current?.pointerId === event.pointerId) {
          event.stopPropagation();
          move(local(event));
        }
      },
      onPointerUp: (event) => {
        if (grab.current?.pointerId === event.pointerId) {
          event.stopPropagation();
          move(local(event));
          release();
        }
      },
      onPointerCancel: (event) => {
        if (
          grab.current?.pointerId === event.pointerId &&
          grab.current.target === event.currentTarget
        )
          release(true);
      },
      onLostPointerCapture: (event) => {
        if (
          grab.current?.pointerId === event.pointerId &&
          grab.current.target === event.currentTarget
        )
          release(true);
      },
      onClick: (event) => {
        event.stopPropagation();
        if (suppressClick.current) {
          suppressClick.current = false;
          return;
        }
        setState((previous) => ({
          ...(previous.id === id ? previous : empty(id)),
          open: !(previous.id === id ? previous.open : true),
        }));
      },
    }),
  };
}
