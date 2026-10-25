import type { NativeHUDGeometry, NativeHUDMonitor } from "./use-hud-geometry";

/** Clears the rotated 44px grab rectangle, even for the smallest 20px bot. */
export const COMPLETION_FLOOR_GAP = 24;

export function hudCompletionFloor(
  geometry: NativeHUDGeometry | null,
  monitors?: readonly NativeHUDMonitor[],
): number | undefined {
  if (!geometry) return undefined;
  const monitor = monitors?.find(
    (entry) => entry.screenId === geometry.screenId,
  );
  const work = monitor?.workArea;
  const scale = monitor?.scaleFactor ?? geometry.scaleFactor;
  // Use a physical inset, then anchor it to the measured logical screen. This also
  // handles negative monitor origins and macOS's different global coordinate space.
  const inset =
    monitor && work && work.height > 0 && Number.isFinite(scale) && scale > 0
      ? Math.max(
          0,
          (monitor.bounds.y + monitor.bounds.height - work.y - work.height) /
            scale,
        )
      : 0;
  const bottom = geometry.screen.y + geometry.screen.height - geometry.canvas.y;
  const floor = Math.min(geometry.canvas.height, bottom - inset);
  return Math.max(
    1,
    (Number.isFinite(floor) ? floor : geometry.canvas.height) -
      COMPLETION_FLOOR_GAP,
  );
}
