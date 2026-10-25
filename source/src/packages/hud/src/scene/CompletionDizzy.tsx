import { motion } from "motion/react";

const star = "M0-4 1.2-1.2 4 0 1.2 1.2 0 4-1.2 1.2-4 0-1.2-1.2Z";
const spiral = "M0 0C1-1 2 0 1.5 1S-1 2-2 0 0-4 3-2 4 3 0 4-6 0-3-4 5-6 6 0";

/** Parent owns the brief dizzy lifetime; this overlay never captures a drag. */
export function BotDizzy({
  size,
  reduceMotion,
  light,
}: {
  size: number;
  reduceMotion: boolean;
  light: boolean;
}) {
  const ink = light ? "#7654a2" : "#e0c5ff";
  const gold = light ? "#ad740b" : "#ffe19a";
  return (
    <svg
      aria-hidden="true"
      className="hud-bot-dizzy"
      viewBox="0 0 100 100"
      width={size}
      height={size}
      style={{
        position: "absolute",
        left: 0,
        top: 0,
        overflow: "visible",
        pointerEvents: "none",
      }}
    >
      <ellipse
        cx={50}
        cy={2}
        rx={34}
        ry={9}
        fill="none"
        stroke={gold}
        strokeWidth={1.8}
        opacity={0.6}
      />
      {[0, (Math.PI * 2) / 3, (Math.PI * 4) / 3].map((angle, index) => {
        const x = 50 + 34 * Math.cos(angle);
        const y = 2 + 9 * Math.sin(angle);
        const orbit = Array.from({ length: 17 }, (_, step) => {
          const phase = angle + (step * Math.PI * 2) / 16;
          return {
            x: 50 + 34 * Math.cos(phase) - x,
            y: 2 + 9 * Math.sin(phase) - y,
          };
        });
        return (
          <g key={index} transform={`translate(${x} ${y})`}>
            <motion.path
              d={star}
              fill={gold}
              initial={false}
              animate={
                reduceMotion
                  ? { x: 0, y: 0 }
                  : { x: orbit.map((p) => p.x), y: orbit.map((p) => p.y) }
              }
              transition={
                reduceMotion
                  ? { duration: 0 }
                  : { duration: 1.8, repeat: Infinity, ease: "linear" }
              }
            />
          </g>
        );
      })}
      {[38, 62].map((x, index) => (
        <g key={x} transform={`translate(${x} 43)`}>
          <circle r={7} fill={light ? "#faf5ff" : "#292233"} />
          <motion.path
            d={spiral}
            fill="none"
            stroke={ink}
            strokeWidth={1.8}
            strokeLinecap="round"
            initial={false}
            animate={
              reduceMotion ? { rotate: 0 } : { rotate: index ? -360 : 360 }
            }
            transition={
              reduceMotion
                ? { duration: 0 }
                : { duration: 1.2, repeat: Infinity, ease: "linear" }
            }
            style={{ transformOrigin: "0px 0px" }}
          />
        </g>
      ))}
    </svg>
  );
}

export function DialogDizzy({
  reduceMotion,
  light,
  label = "晕乎乎…",
}: {
  reduceMotion: boolean;
  light: boolean;
  label?: string;
}) {
  const ink = light ? "#7654a2" : "#e0c5ff";
  return (
    <div
      className="hud-dialog-dizzy"
      style={{
        position: "absolute",
        inset: 0,
        zIndex: 3,
        borderRadius: "inherit",
        pointerEvents: "none",
      }}
    >
      <span
        role="status"
        style={{
          position: "absolute",
          top: -10,
          left: 26,
          display: "inline-flex",
          alignItems: "center",
          gap: 4,
          padding: "1px 7px",
          borderRadius: 9,
          border: `1px solid ${light ? "#dcc8ef" : "#67507f"}`,
          background: light ? "#faf5ff" : "#30263e",
          color: ink,
          fontSize: 10,
          fontWeight: 500,
          lineHeight: "16px",
          whiteSpace: "nowrap",
        }}
      >
        <svg aria-hidden="true" width={12} height={12} viewBox="-8 -8 16 16">
          <motion.path
            d={spiral}
            fill="none"
            stroke={ink}
            strokeWidth={1.5}
            strokeLinecap="round"
            initial={false}
            animate={reduceMotion ? { rotate: 0 } : { rotate: 360 }}
            transition={
              reduceMotion
                ? { duration: 0 }
                : { duration: 1.5, repeat: Infinity, ease: "linear" }
            }
            style={{ transformOrigin: "0px 0px" }}
          />
        </svg>
        {label}
      </span>
      <motion.svg
        aria-hidden="true"
        width={25}
        height={25}
        viewBox="-12 -12 24 24"
        initial={false}
        animate={
          reduceMotion
            ? { y: 0, rotate: 0 }
            : { y: [0, -3, 0], rotate: [0, 9, 0] }
        }
        transition={
          reduceMotion
            ? { duration: 0 }
            : { duration: 1.1, repeat: Infinity, ease: "easeInOut" }
        }
        style={{
          position: "absolute",
          top: -17,
          right: 10,
          overflow: "visible",
        }}
      >
        <path d={star} fill={light ? "#ad740b" : "#ffe19a"} />
        <path d={star} transform="translate(8 -5) scale(.55)" fill={ink} />
      </motion.svg>
    </div>
  );
}
