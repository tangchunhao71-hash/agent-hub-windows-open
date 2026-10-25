import { motion } from "motion/react";
import type { TaskProgress } from "./task-progress";

/** Separate from the quota ring: this small satellite describes task activity. */
export function TaskProgressMark({
  progress,
  size,
  light,
  reduceMotion,
}: {
  progress: TaskProgress;
  size: number;
  light: boolean;
  reduceMotion: boolean;
}) {
  const working = progress.mode === "working";
  const color = working ? (light ? "#1769b4" : "#82b9ff") : "#d79336";
  const count = progress.running + progress.waiting;
  return (
    <span
      role="progressbar"
      aria-label={progress.label}
      aria-valuetext={progress.detail}
      title={progress.detail}
      data-task-progress={progress.mode}
      style={{
        position: "absolute",
        right: 0,
        top: 0,
        width: size,
        height: size,
        display: "grid",
        placeItems: "center",
        borderRadius: "50%",
        background: light ? "#f8f8fa" : "#17191e",
        color,
        boxShadow: `0 0 0 1px ${light ? "#00000015" : "#ffffff20"}`,
        pointerEvents: "auto",
      }}
    >
      <motion.svg
        viewBox="0 0 20 20"
        aria-hidden="true"
        style={{
          position: "absolute",
          inset: 0,
          width: "100%",
          height: "100%",
        }}
        animate={{ rotate: working && !reduceMotion ? 360 : 0 }}
        transition={
          working && !reduceMotion
            ? { duration: 1.25, repeat: Infinity, ease: "linear" }
            : { duration: 0 }
        }
      >
        <circle
          cx="10"
          cy="10"
          r="8"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          opacity="0.2"
        />
        <circle
          cx="10"
          cy="10"
          r="8"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          pathLength="100"
          strokeDasharray={working ? "28 72" : "100 0"}
        />
      </motion.svg>
      <span
        aria-hidden="true"
        style={{ fontSize: size * 0.48, fontWeight: 700, lineHeight: 1 }}
      >
        {count > 1 ? (count > 9 ? "9+" : count) : working ? "" : "Ⅱ"}
      </span>
    </span>
  );
}
