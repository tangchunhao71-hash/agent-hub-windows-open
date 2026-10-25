/** Animated scenery is observed for movement but never made interactive. */
export const HUD_HIT_SELECTOR =
  ".hud-resting-strip, .hud-expanded-surface, .hud-grab-surface, .hud-hover-floor, .hud-completion-pet";
export const HUD_CONTOUR_SELECTOR = `.hud-scene, ${HUD_HIT_SELECTOR}, .hud-completion-pet-contour`;

export function hudHitRegions(root: Pick<ParentNode, "querySelectorAll">) {
  return [...root.querySelectorAll(HUD_HIT_SELECTOR)]
    .map((element) => {
      const rect = element.getBoundingClientRect();
      return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
    })
    .filter((rect) => rect.width > 0 && rect.height > 0);
}
