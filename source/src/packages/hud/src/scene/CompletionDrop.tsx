import { useEffect, useMemo, useRef, useState } from "react";
import { motion } from "motion/react";
import type { BotIdentity, IslandEvent } from "@agent-hud/protocol";
import { BotMark } from "../BotMark";
import { BotIdentityMark } from "../BotIdentityMark";
import {
  completionKeyframes,
  completionPose,
  completionReturnMotion,
  type CompletionMotion,
} from "./completion-physics";

/** Each queued turn falls once, including when the person switches cards. */
export function useCompletionDrop(
  event: IslandEvent | undefined,
  enabled: boolean,
  trajectory: CompletionMotion | null,
) {
  const seen = useRef(new Set<string>());
  const active = useRef<{
    id: string;
    deadline: number;
    trajectory: CompletionMotion | null;
  } | null>(null);
  const [arriving, setArriving] = useState<string | null>(null);
  const id = event?.kind === "completion" ? event.id : null;
  const durationMs = (trajectory?.duration ?? 0) * 1000;
  const fresh = id !== null && !seen.current.has(id);
  const dropping = enabled && id !== null && (fresh || arriving === id);
  useEffect(() => {
    if (id === null || !enabled) {
      if (id !== null) seen.current.add(id);
      active.current = null;
      setArriving(null);
      return;
    }
    if (seen.current.has(id) && active.current?.id !== id) {
      active.current = null;
      setArriving(null);
      return;
    }
    if (!seen.current.has(id)) {
      seen.current.add(id);
      active.current = { id, deadline: Date.now() + durationMs, trajectory };
    } else if (active.current && active.current.trajectory !== trajectory) {
      // A viewport change can move the path without changing its duration.
      active.current = { id, deadline: Date.now() + durationMs, trajectory };
    }
    setArriving(id);
    // StrictMode restarts effects; keep the deadline so its second setup still lands.
    const timer = setTimeout(
      () => {
        active.current = null;
        setArriving(null);
      },
      Math.max(0, (active.current?.deadline ?? Date.now()) - Date.now()),
    );
    return () => clearTimeout(timer);
  }, [id, enabled, trajectory]);
  return dropping;
}

export interface CompletionReturnFlight {
  botIdentity?: BotIdentity;
  id: string;
  source: string;
  motion: CompletionMotion;
  size: number;
  endsAt: number;
  startedAt: number;
  dockKey: string;
}

/** More than one read reply can be returning while the next reply is shown. */
export function useCompletionReturns(
  disabled: boolean,
  targets: Record<string, { x: number; y: number }>,
  screen: { width: number; height: number },
) {
  const [flights, setFlights] = useState<CompletionReturnFlight[]>([]);
  useEffect(() => {
    setFlights((current) => {
      let changed = false;
      const updated = current.map((flight) => {
        const target = targets[flight.dockKey];
        if (
          !target ||
          Math.hypot(
            target.x - flight.motion.final.x,
            target.y - flight.motion.final.y,
          ) < 0.1
        )
          return flight;
        const now = Date.now(),
          radius = flight.motion.radius;
        const pose = completionPose(
          flight.motion,
          (now - flight.startedAt) / 1000,
        );
        const motion = completionReturnMotion(
          {
            ...pose,
            x: Math.max(radius, Math.min(screen.width - radius, pose.x)),
            y: Math.max(radius, Math.min(screen.height - radius, pose.y)),
          },
          target,
          radius,
        );
        changed = true;
        return {
          ...flight,
          motion,
          startedAt: now,
          endsAt: now + motion.duration * 1000,
        };
      });
      return changed ? updated : current;
    });
  }, [targets, screen.width, screen.height]);
  useEffect(() => {
    if (disabled) {
      setFlights((current) => (current.length ? [] : current));
      return;
    }
    if (!flights.length) return;
    const timer = setTimeout(
      () =>
        setFlights((current) => current.filter((f) => f.endsAt > Date.now())),
      Math.max(0, Math.min(...flights.map((f) => f.endsAt)) - Date.now()),
    );
    return () => clearTimeout(timer);
  }, [flights, disabled]);
  return {
    flights: disabled ? [] : flights,
    launch: (flight: CompletionReturnFlight) =>
      setFlights((current) =>
        current.some((f) => f.id === flight.id)
          ? current
          : [...current, flight],
      ),
  };
}

export function CompletionDrop({
  source,
  motion: trajectory,
  moving,
  size,
  light,
  botLogos,
  returning = false,
  botIdentity,
}: {
  botIdentity?: BotIdentity;
  source: string;
  motion: CompletionMotion;
  moving: boolean;
  size: number;
  light: boolean;
  botLogos: boolean;
  returning?: boolean;
}) {
  // Keep the bot on the screen floor after its fall; the reply opens above it.
  const from = trajectory.initial,
    end = trajectory.final;
  const frames = useMemo(() => completionKeyframes(trajectory), [trajectory]);
  return (
    <motion.div
      aria-hidden="true"
      className={returning ? "hud-completion-return" : "hud-completion-drop"}
      initial={
        moving
          ? { x: from.x, y: from.y, rotate: (from.angle * 180) / Math.PI }
          : false
      }
      animate={
        moving
          ? {
              x: frames.x,
              y: frames.y,
              rotate: frames.rotate,
            }
          : { x: end.x, y: end.y, rotate: (end.angle * 180) / Math.PI }
      }
      transition={{
        duration: moving ? trajectory.duration : 0,
        times: moving ? frames.times : undefined,
        ease: "linear",
      }}
      style={{
        position: "absolute",
        left: -size / 2,
        top: -size / 2,
        width: size,
        height: size,
        zIndex: 8,
        pointerEvents: "none",
      }}
    >
      {botIdentity ? (
        <BotIdentityMark
          identity={botIdentity}
          size={size}
          light={light}
          state="done"
        />
      ) : (
        <BotMark
          source={source}
          size={size}
          state="done"
          light={light}
          enabled={botLogos}
          staysBot
          awake
        />
      )}
    </motion.div>
  );
}
