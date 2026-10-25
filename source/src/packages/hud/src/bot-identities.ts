import type { BotIdentity, Session } from "@agent-hud/protocol";

export interface ActiveBotIdentity {
  sessionId: string;
  identity: BotIdentity;
  state: "running" | "waiting";
}
export interface IdentityVendor {
  working: number;
  sessions: readonly Session[];
}
/** Stable order avoids swapping people's avatars every time a heartbeat arrives. */
export function activeBotIdentities(
  vendor: IdentityVendor,
): ActiveBotIdentity[] {
  if (!vendor.working) return [];
  const seen = new Set<string>();
  return vendor.sessions
    .flatMap((session): ActiveBotIdentity[] => {
      if (
        session.source !== "grok-bot" ||
        !session.botIdentity?.name ||
        (session.state !== "running" && session.state !== "waiting") ||
        seen.has(session.id)
      )
        return [];
      seen.add(session.id);
      return [
        {
          sessionId: session.id,
          identity: session.botIdentity,
          state: session.state,
        },
      ];
    })
    .sort(
      (a, b) =>
        a.identity.name.localeCompare(b.identity.name) ||
        a.sessionId.localeCompare(b.sessionId),
    );
}
export interface IdentityMarkBox {
  width: number;
  height: number;
  tile: number;
  label: number;
  gap: number;
  visible: ActiveBotIdentity[];
  extra: ActiveBotIdentity[];
}

function markBox(
  active: ActiveBotIdentity[],
  mark: number,
  gap: number,
  sharedNames: boolean,
): IdentityMarkBox {
  if (!sharedNames)
    return {
      width: mark,
      height: mark,
      tile: mark,
      label: 0,
      gap: 0,
      visible: active,
      extra: [] as ActiveBotIdentity[],
    };
  const visible = active.slice(0, 3),
    extra = active.slice(3);
  const tile = Math.max(
    mark,
    Math.min(84, Math.max(52, Math.round(mark * 1.65))),
  );
  const columns = Math.max(1, visible.length) + (extra.length ? 1 : 0);
  return {
    width: columns * tile + (columns - 1) * gap,
    height: mark + 16,
    tile,
    label: 16,
    gap,
    visible,
    extra,
  };
}

/** Standalone/default callers keep their compact box when no identity is active. */
export function identityMarkBox(
  vendor: IdentityVendor,
  mark: number,
  gap = 4,
): IdentityMarkBox {
  const active = activeBotIdentities(vendor);
  return markBox(active, mark, gap, active.length > 0);
}

/** One Hub grid: every mark reserves the same icon row and optional name row. */
export function stripIdentityBoxes(
  vendors: readonly IdentityVendor[],
  mark: number,
  gap: number,
): IdentityMarkBox[] {
  const identities = vendors.map(activeBotIdentities);
  const sharedNames = identities.some((active) => active.length > 0);
  return identities.map((active) => markBox(active, mark, gap, sharedNames));
}
