import timeline from "../assets/orange-bot-animation.json" with { type: "json" };
export function orangeFrameAt(milliseconds: number) {
  const duration = timeline.frames.reduce(
    (sum, frame) => sum + frame.durationMs,
    0,
  );
  let at = ((milliseconds % duration) + duration) % duration;
  for (let index = 0; index < timeline.frames.length; index++) {
    if (at < timeline.frames[index].durationMs)
      return { index, remainingMs: timeline.frames[index].durationMs - at };
    at -= timeline.frames[index].durationMs;
  }
  return { index: 0, remainingMs: timeline.frames[0].durationMs };
}
