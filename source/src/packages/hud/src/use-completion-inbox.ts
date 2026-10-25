import { useEffect, useRef, useState } from "react";
import type { IslandEvent } from "@agent-hud/protocol";
import type { PanelServices } from "./PanelContext";
const EMPTY: IslandEvent[] = [];

/** The native owner serializes persistence and leases across every HUD window. */
export function useCompletionInbox(
  backend: PanelServices["completionInbox"],
  revision: number | undefined,
  enabled: boolean,
  eligible: boolean,
  context: string,
) {
  const owner = useRef(crypto.randomUUID());
  const [saved, setSaved] = useState<{ items: IslandEvent[]; context: string }>(
    { items: [], context },
  );
  const [error, setError] = useState<string | null>(null);
  const [dropped, setDropped] = useState(0);
  const latest = useRef({ enabled, eligible });
  latest.current = { enabled, eligible };
  const serial = useRef<Promise<unknown>>(Promise.resolve());
  const mounted = useRef(true);
  const run = (operation: "sync" | "dismiss", ids?: string[]) => {
    if (!backend) return Promise.resolve(false);
    const result = serial.current
      .catch(() => undefined)
      .then(() =>
        backend({
          owner: owner.current,
          operation,
          ids,
          eligible: latest.current.eligible,
        }),
      );
    serial.current = result;
    return result.then(
      (view) => {
        if (mounted.current) {
          setSaved({ items: view.items, context });
          setError(view.error);
          setDropped(view.dropped);
        }
        return true;
      },
      () => {
        // A failed durable acknowledgement keeps its card on screen.
        if (mounted.current)
          setError(
            "Completion reminders could not be saved. Unread reminders may return after restart; try again.",
          );
        return false;
      },
    );
  };
  const currentRun = useRef(run);
  currentRun.current = run;
  useEffect(() => {
    mounted.current = true;
    if (!backend) return;
    void currentRun.current("sync");
    const timer = setInterval(() => void currentRun.current("sync"), 2_000);
    return () => {
      mounted.current = false;
      clearInterval(timer);
      const releasedOwner = owner.current;
      serial.current = serial.current
        .catch(() => undefined)
        .then(() => backend({ owner: releasedOwner, operation: "release" }))
        .catch(() => undefined);
    };
  }, [backend]);
  useEffect(() => {
    if (backend) void currentRun.current("sync");
  }, [backend, revision, enabled, eligible, context]);
  return {
    items: enabled && saved.context === context ? saved.items : EMPTY,
    error,
    limitReached: dropped > 0,
    dismiss: (ids: string[]) => currentRun.current("dismiss", ids),
    open: async (id: string) => {
      if (!backend) return false;
      const result = serial.current
        .catch(() => undefined)
        .then(() =>
          backend({ owner: owner.current, operation: "navigate", id }),
        );
      serial.current = result;
      try {
        return (await result).opened === true;
      } catch {
        return false;
      }
    },
  };
}
