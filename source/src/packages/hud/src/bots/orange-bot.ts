import timeline from "../assets/orange-bot-animation.json" with { type: "json" };
import type { BotForm } from "./bot-form";
import type { BotMotion } from "./bot-motion";
import { paintSign, signBox } from "./bot-signs";
import { layHairline } from "./bot-hairline";
import { reuseCanvas } from "./scratch";

export { orangeFrameAt } from "./orange-bot-timing";
let loading: Promise<HTMLImageElement> | undefined;
export function loadOrangeBot() {
  return (loading ??= new Promise<HTMLImageElement>((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(Error("Orange Bot sheet unavailable"));
    image.src = new URL("../assets/orange-bot-strip.png", import.meta.url).href;
  }));
}
/** Idle/static appearance is always the source artwork. Only the dressed Bot uses the sheet. */
export function renderOrangeBot(
  form: BotForm,
  motion: BotMotion,
  strip: HTMLImageElement,
  frame: number,
  side: number,
  outline = 0,
  into?: HTMLCanvasElement,
) {
  const extent = Math.ceil(side * 1.6) + outline * 2;
  const canvas = reuseCanvas(into, extent, extent),
    context = canvas.getContext("2d")!;
  const blend = Math.max(0, Math.min(1, motion.morph));
  context.globalAlpha = 1 - blend;
  context.drawImage(
    form.artwork,
    outline + side * 0.3,
    outline + side * 0.3,
    side,
    side,
  );
  context.globalAlpha = blend;
  const cell = timeline.frames[frame];
  context.drawImage(
    strip,
    cell.x,
    cell.y,
    timeline.frameWidth,
    timeline.frameHeight,
    outline + side * 0.175,
    outline + side * 0.175,
    side * 1.25,
    side * 1.25,
  );
  context.globalAlpha = 1;
  if (motion.sign)
    paintSign(
      context,
      motion.sign,
      signBox(form, side, outline),
      form.light,
      motion.lit,
    );
  if (outline > 0) layHairline(context, outline, form.light);
  return canvas;
}
