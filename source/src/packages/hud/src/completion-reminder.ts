import type { IslandEvent, Session } from "@agent-hud/protocol";

/** Keep the navigation attached to the turn that produced this reminder. */
export function completionSession(
  event: IslandEvent,
  sessions: readonly Session[],
): Session | undefined {
  const session = sessions.find((entry) => entry.id === event.sessionId);
  if (session) {
    return {
      ...session,
      botIdentity: event.botIdentity ?? session.botIdentity,
      navigationTarget: event.navigationTarget ?? session.navigationTarget,
    };
  }
  if (!event.sessionId || !event.navigationTarget) return undefined;
  return {
    id: event.sessionId,
    title: event.title ?? "",
    source: event.source,
    model: event.model ?? "",
    cwd: null,
    updatedAt: event.at,
    state: "idle",
    tokens: 0,
    cost: null,
    contextPercent: null,
    logPath: null,
    parentId: null,
    navigationTarget: event.navigationTarget,
    botIdentity: event.botIdentity,
  };
}

/** A completion stays until read; informational quota notices can expire. */
export function reminderTimeout(kind: IslandEvent["kind"]): number | null {
  return kind === "completion" ? null : 4000;
}
