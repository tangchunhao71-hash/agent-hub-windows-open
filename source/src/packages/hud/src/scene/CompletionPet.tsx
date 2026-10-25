import { useMemo } from "react";
import { motion } from "motion/react";
import type { BotIdentity } from "@agent-hud/protocol";
import { CompletionDrop } from "./CompletionDrop";
import { BotDizzy } from "./CompletionDizzy";
import {
  completionKeyframes,
  type CompletionMotion,
} from "./completion-physics";
import type { PetControls } from "./useCompletionPet";

/** Interaction is a small sibling hit target; the shared identity renderer stays intact. */
export function CompletionPet({
  source,
  trajectory,
  moving,
  size,
  light,
  botLogos,
  botIdentity,
  controls,
  dizzy,
  reduceMotion,
}: {
  source: string;
  trajectory: CompletionMotion;
  moving: boolean;
  size: number;
  light: boolean;
  botLogos: boolean;
  botIdentity?: BotIdentity;
  controls?: PetControls;
  dizzy: boolean;
  reduceMotion: boolean;
}) {
  const held = controls?.pose;
  const body = useMemo(
    () =>
      held
        ? {
            ...trajectory,
            duration: 0,
            initial: held,
            final: held,
            segments: [],
            collisions: [],
          }
        : trajectory,
    [held, trajectory],
  );
  const frames = useMemo(() => completionKeyframes(trajectory), [trajectory]);
  const pose = held ?? trajectory.final;
  const animating = moving && !held && trajectory.duration > 0;
  const hit = Math.max(44, size);
  return (
    <>
      <CompletionDrop
        source={source}
        motion={body}
        moving={animating}
        size={size}
        light={light}
        botLogos={botLogos}
        botIdentity={botIdentity}
      />
      <motion.div
        className={controls ? "hud-completion-pet-contour" : undefined}
        initial={
          animating
            ? {
                x: trajectory.initial.x,
                y: trajectory.initial.y,
                rotate: (trajectory.initial.angle * 180) / Math.PI,
              }
            : false
        }
        animate={
          animating
            ? { x: frames.x, y: frames.y, rotate: frames.rotate }
            : { x: pose.x, y: pose.y, rotate: (pose.angle * 180) / Math.PI }
        }
        transition={{
          duration: animating ? trajectory.duration : 0,
          times: animating ? frames.times : undefined,
          ease: "linear",
        }}
        style={{
          position: "absolute",
          left: -size / 2,
          top: -size / 2,
          width: size,
          height: size,
          zIndex: 9,
          pointerEvents: "none",
        }}
      >
        {controls && (
          <button
            type="button"
            className="hud-completion-pet"
            aria-label={controls.label}
            title={controls.label}
            aria-pressed={controls.holding}
            onPointerDown={controls.onPointerDown}
            onPointerMove={controls.onPointerMove}
            onPointerUp={controls.onPointerUp}
            onPointerCancel={controls.onPointerCancel}
            onLostPointerCapture={controls.onLostPointerCapture}
            onClick={controls.onClick}
            style={{
              position: "absolute",
              left: (size - hit) / 2,
              top: (size - hit) / 2,
              width: hit,
              height: hit,
              border: 0,
              padding: 0,
              background: "transparent",
              borderRadius: "50%",
              pointerEvents: "auto",
              cursor: controls.holding ? "grabbing" : "grab",
              touchAction: "none",
              userSelect: "none",
            }}
          />
        )}
        {dizzy && (
          <BotDizzy size={size} reduceMotion={reduceMotion} light={light} />
        )}
      </motion.div>
    </>
  );
}
