import type { HUDRect } from "../geometry";

/** The bot touches the screen floor; its reply grows upwards from that landing. */
export function bottomCompletionPlacement(
  screen: { width: number; height: number },
  origin: { x: number; y: number },
  card: { width: number; height: number },
  botSize: number,
  bottomDockHeight = 0,
  sideDock?: { edge: "left" | "right"; frame: HUDRect },
) {
  const margin = Math.min(12, screen.width / 4);
  const size = Math.max(1, Math.min(botSize, screen.width, screen.height));
  const point = {
    x: Math.max(size / 2, Math.min(screen.width - size / 2, origin.x)),
    y: screen.height - size / 2,
  };
  const width = Math.max(1, Math.min(card.width, screen.width - margin * 2));
  const bottom = Math.max(
    0,
    screen.height - Math.max(size + 10, bottomDockHeight + 8),
  );
  const height = Math.max(1, Math.min(card.height, bottom - margin));
  const frame: HUDRect = {
    x: Math.max(
      margin,
      Math.min(screen.width - width - margin, point.x - width / 2),
    ),
    y: Math.max(0, bottom - height),
    width,
    height,
  };
  if (
    sideDock &&
    frame.y < sideDock.frame.y + sideDock.frame.height &&
    frame.y + frame.height > sideDock.frame.y
  ) {
    // Long side docks can overlap a tall reply; reserve the dock's inside edge.
    const left =
      sideDock.edge === "left"
        ? Math.min(
            screen.width - margin - 1,
            sideDock.frame.x + sideDock.frame.width + 8,
          )
        : margin;
    const right =
      sideDock.edge === "right"
        ? Math.max(left + 1, sideDock.frame.x - 8)
        : screen.width - margin;
    frame.width = Math.min(frame.width, right - left);
    frame.x = Math.max(left, Math.min(right - frame.width, frame.x));
  }
  return { point, frame, size };
}
