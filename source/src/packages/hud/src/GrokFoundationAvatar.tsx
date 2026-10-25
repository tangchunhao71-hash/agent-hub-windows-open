import { useEffect, useId, useMemo, useRef } from "react";
import {
  avatarSeed,
  avatarGazeTarget,
  clampAvatar,
  grokAvatarColor,
  grokAvatarModel,
  projectGrokAvatar,
  sampleGrokAvatar,
  smoothAvatarGaze,
  type GrokAvatarPoke,
  type GrokAvatarState,
} from "./grok-avatar-motion.ts";

/** Grok foundation shapes have their own performer, separate from Hub's source robots. */
export function GrokFoundationAvatar({
  shape,
  color,
  size,
  seed,
  state = "idle",
  light = false,
  reduceMotion = false,
  pointer,
}: {
  shape: string;
  color: string;
  size: number;
  seed: string;
  state?: GrokAvatarState;
  light?: boolean;
  reduceMotion?: boolean;
  pointer?: { x: number; y: number } | null;
}) {
  const id = useId().replace(/:/g, ""),
    model = useMemo(() => grokAvatarModel(shape), [shape]);
  const root = useRef<SVGSVGElement>(null),
    front = useRef<SVGPathElement>(null),
    back = useRef<SVGPathElement>(null),
    sides = useRef<SVGPathElement>(null),
    eyes = useRef<SVGPathElement>(null),
    shine = useRef<SVGPathElement>(null);
  const input = useRef({ reduceMotion, pointer, state });
  input.current = { reduceMotion, pointer, state };
  const target = useRef<[number, number]>([0, 0]);
  const poke = useRef<{ kind: GrokAvatarPoke; at: number } | null>(null),
    pokes = useRef(0);
  const fill = grokAvatarColor(color, light);
  useEffect(() => {
    const svg = root.current;
    if (!svg) return;
    const media = matchMedia("(prefers-reduced-motion: reduce)"),
      phase = avatarSeed(seed),
      started = performance.now() / 1000;
    let raf = 0,
      last = started,
      intersects = true,
      disposed = false,
      gaze: [number, number] = [0, 0];
    const stop = () => {
      cancelAnimationFrame(raf);
      raf = 0;
    };
    const paint = () => {
      raf = 0;
      if (disposed) return;
      const now = performance.now() / 1000,
        paused = media.matches || input.current.reduceMotion;
      if (
        document.hidden ||
        !intersects ||
        getComputedStyle(svg).visibility === "hidden"
      ) {
        last = now;
        return;
      }
      const p = input.current.pointer;
      if (p !== undefined) {
        target.current = avatarGazeTarget(
          p,
          svg.getBoundingClientRect(),
          target.current,
        );
      }
      gaze = paused
        ? [0, 0]
        : smoothAvatarGaze(gaze, target.current, now - last);
      last = now;
      const action = poke.current,
        pose = sampleGrokAvatar(
          now - started,
          input.current.state,
          phase,
          paused,
          action ? { kind: action.kind, age: now - action.at } : undefined,
        );
      const paths = projectGrokAvatar(model, shape, pose, gaze);
      front.current?.setAttribute("d", paths.front);
      shine.current?.setAttribute("d", paths.front);
      back.current?.setAttribute("d", paths.back);
      sides.current?.setAttribute("d", paths.sides);
      eyes.current?.setAttribute("d", paths.eyes);
      if (!paused) raf = requestAnimationFrame(paint);
    };
    const wake = () => {
      if (!disposed && !raf) raf = requestAnimationFrame(paint);
    };
    const visible = () => {
      stop();
      last = performance.now() / 1000;
      wake();
    };
    const observer = new IntersectionObserver((entries) => {
      intersects = entries.some((e) => e.isIntersecting);
      visible();
    });
    observer.observe(svg);
    // Hidden measurement copies inherit visibility. Animated paths never wake reports.
    const ancestors = new MutationObserver(wake);
    for (let n: Element | null = svg.parentElement; n; n = n.parentElement)
      ancestors.observe(n, {
        attributes: true,
        attributeFilter: ["style", "class"],
      });
    document.addEventListener("visibilitychange", visible);
    media.addEventListener("change", visible);
    wake();
    return () => {
      disposed = true;
      stop();
      observer.disconnect();
      ancestors.disconnect();
      document.removeEventListener("visibilitychange", visible);
      media.removeEventListener("change", visible);
    };
  }, [model, shape, seed, state, reduceMotion]);
  return (
    <svg
      ref={root}
      viewBox="-50 -50 100 100"
      aria-hidden="true"
      data-grok-foundation-avatar={shape}
      data-grok-avatar-state={state}
      style={{
        display: "block",
        width: size * 1.24,
        height: size * 1.24,
        flexShrink: 0,
        overflow: "visible",
      }}
      onPointerMove={(e) => {
        if (input.current.pointer) return;
        const r = e.currentTarget.getBoundingClientRect();
        target.current = [
          clampAvatar((e.clientX - r.x - r.width / 2) / (r.width * 0.5)),
          clampAvatar((r.y + r.height / 2 - e.clientY) / (r.width * 0.5)),
        ];
      }}
      onPointerLeave={() => {
        target.current = [0, 0];
      }}
      onPointerDown={(event) => {
        if (event.button !== 0) return;
        const kinds: GrokAvatarPoke[] = ["bounce", "nod", "spin"];
        poke.current = {
          kind: kinds[pokes.current++ % kinds.length],
          at: performance.now() / 1000,
        };
      }}
    >
      <defs>
        <mask
          id={`${id}-eyes`}
          maskUnits="userSpaceOnUse"
          x="-60"
          y="-60"
          width="120"
          height="120"
        >
          <rect x="-60" y="-60" width="120" height="120" fill="white" />
          <path ref={eyes} fill="black" />
        </mask>
        <linearGradient id={`${id}-surface`} x1="0" y1="0" x2="1" y2="1">
          <stop stopColor="white" stopOpacity=".22" />
          <stop offset="1" stopColor="black" stopOpacity=".12" />
        </linearGradient>
      </defs>
      <g mask={`url(#${id}-eyes)`}>
        <path ref={back} fill={fill} />
        <path ref={sides} fill={fill} />
        <path ref={front} fill={fill} data-avatar-front="" />
        <path ref={shine} fill={`url(#${id}-surface)`} />
      </g>
    </svg>
  );
}
