import { useEffect, useRef, useState } from "react";
import type { Approval, IslandEvent, Snapshot } from "@agent-hud/protocol";
import { usePanelServices } from "./PanelContext";
import { agentVendorName, sourceID, vendorKey } from "./model";
import { CompletionAlertView } from "./alerts/CompletionAlertView";
import { QuotaAlertView, ResetCreditAlertView } from "./alerts/QuotaAlertViews";
import { snapshotLanguage } from "@agent-hud/ui/format";
import { reminderTimeout } from "./completion-reminder";
import { useCompletionInbox } from "./use-completion-inbox";
import {
  reconcileIslandCards,
  dismissIslandCardByID,
  type IslandCard,
} from "./island-queue";

export { ReminderMark } from "./alerts/ReminderMark";
export {
  CompactAlertView,
  compactAlertDockWidth,
  compactAlertLabels,
  compactQuotaStatus,
} from "./alerts/CompactAlertView";
export {
  CompletionClosedRow,
  PendingEventRows,
  PermissionClosedRow,
  PermissionRowHead,
  WaitedLabel,
  permissionToolBadge,
} from "./alerts/PendingEventRows";

export {
  reconcileIslandCards,
  dismissIslandCard,
  type IslandCard,
} from "./island-queue";

export function useIslandNews(
  snapshot: Snapshot,
  held: boolean,
  requests: Approval[],
  shown: string[],
  silenced = false,
  underPointer = true,
  inUsagePanel = false,
  completionNotifications = true,
) {
  const services = usePanelServices(),
    seen = useRef<Set<string> | null>(null),
    seenRequests = useRef<Set<string> | null>(null);
  const [queue, setQueue] = useState<IslandCard[]>([]);
  const inbox = useCompletionInbox(
    services?.completionInbox,
    snapshot.islandEvents?.revision,
    completionNotifications,
    underPointer && !silenced,
    JSON.stringify([
      snapshot.mode,
      snapshot.accounts?.map((a) => [a.provider, a.id, a.current]).sort(),
    ]),
  );
  const nativeInbox = Boolean(services?.completionInbox);
  const items = snapshot.islandEvents?.items ?? [];
  const requestIDs = requests.map((request) => request.id).join("\0");
  useEffect(() => {
    const fresh =
      seen.current === null
        ? []
        : items.filter(
            (event) =>
              !seen.current!.has(event.id) &&
              (!nativeInbox || event.kind !== "completion"),
          );
    if (seen.current === null) seen.current = new Set();
    for (const event of items) seen.current.add(event.id);
    if (seen.current.size > 128)
      seen.current = new Set(items.map((event) => event.id));
    const freshRequests =
      seenRequests.current === null
        ? requests
        : requests.filter((request) => !seenRequests.current!.has(request.id));
    if (seenRequests.current === null) seenRequests.current = new Set();
    const activeRequestIds = new Set(requests.map((request) => request.id));
    for (const id of [...seenRequests.current]) {
      if (!activeRequestIds.has(id)) seenRequests.current.delete(id);
    }
    for (const request of requests) seenRequests.current.add(request.id);
    const visible =
      silenced || !underPointer
        ? []
        : fresh.filter(
            (event) =>
              event.islandAlert !== false &&
              shown.includes(vendorKey(event.source)) &&
              (event.kind !== "completion" ||
                (services?.sources?.[sourceID(snapshot, event.source)]?.live !==
                  false &&
                  services?.sources?.[sourceID(snapshot, event.source)]
                    ?.reminders !== false)),
          );
    const restored =
      nativeInbox && !silenced
        ? inbox.items.filter(
            (event) =>
              shown.includes(vendorKey(event.source)) &&
              services?.sources?.[sourceID(snapshot, event.source)]?.live !==
                false &&
              services?.sources?.[sourceID(snapshot, event.source)]
                ?.reminders !== false,
          )
        : [];
    setQueue((previous) =>
      reconcileIslandCards(
        silenced
          ? previous.filter((card) => card.type === "request")
          : nativeInbox
            ? previous.filter(
                (card) =>
                  card.type !== "news" ||
                  card.event.kind !== "completion" ||
                  restored.some((event) => event.id === card.id),
              )
            : previous,
        requests,
        [...visible, ...restored],
        underPointer ? freshRequests : [],
        inUsagePanel,
        completionNotifications,
      ),
    );
  }, [
    snapshot.islandEvents?.revision,
    requestIDs,
    silenced,
    underPointer,
    completionNotifications,
    inbox.items,
    services?.sources,
    shown.join("\0"),
  ]);
  // Hide an old account's rows in the same render, before the reconciliation effect.
  const presented = nativeInbox
    ? queue.filter(
        (card) =>
          card.type !== "news" ||
          card.event.kind !== "completion" ||
          inbox.items.some((event) => event.id === card.id),
      )
    : queue;
  const current = presented[0];
  const dismissID = async (id: string) => {
    const card = queue.find((card) => card.id === id);
    if (
      nativeInbox &&
      card?.type === "news" &&
      card.event.kind === "completion"
    ) {
      if (!(await inbox.dismiss([id]))) return false;
    }
    setQueue((previous) => dismissIslandCardByID(previous, id));
    return true;
  };
  const dismiss = () => {
    if (current) void dismissID(current.id);
  };
  useEffect(() => {
    if (!current || held || current.type === "request") return;
    const timeout = reminderTimeout(current.event.kind);
    if (timeout === null) return;
    const timer = setTimeout(dismiss, timeout);
    return () => clearTimeout(timer);
  }, [current?.id, held]);
  const select = (id: string) =>
    setQueue((previous) => {
      const at = previous.findIndex((card) => card.id === id);
      if (at <= 0) return previous;
      const next = [...previous];
      [next[0], next[at]] = [next[at], next[0]];
      return next;
    });
  return {
    event: current?.type === "news" ? current.event : undefined,
    inUsagePanel:
      current?.type === "news" ? Boolean(current.inUsagePanel) : false,
    request:
      current?.type === "request"
        ? requests.find((request) => request.id === current.id)
        : undefined,
    queue: presented,
    select,
    dismiss,
    dismissID,
    dismissCompletions: async () => {
      const ids = queue
        .filter(
          (card) => card.type === "news" && card.event.kind === "completion",
        )
        .map((card) => card.id);
      if (nativeInbox && !(await inbox.dismiss(ids))) return false;
      setQueue((previous) => previous.filter((card) => !ids.includes(card.id)));
      return true;
    },
    persistenceError: inbox.error,
    persistenceLimitReached: inbox.limitReached,
    openCompletion: nativeInbox ? inbox.open : undefined,
    completionCount: presented.filter(
      (card) => card.type === "news" && card.event.kind === "completion",
    ).length,
  };
}

export const newsTitle = (event: IslandEvent, zh = false) =>
  event.kind === "completion"
    ? zh
      ? "有新回复"
      : "New reply"
    : event.kind === "quota-reset"
      ? zh
        ? "额度已重置"
        : "Quota reset"
      : event.kind === "reset-credits"
        ? zh
          ? "获得使用重置"
          : "Usage resets added"
        : zh
          ? "额度预警"
          : "Running low";

export const compactDuration = (seconds: number) => {
  const total = Math.max(0, Math.round(seconds)),
    h = Math.floor(total / 3600),
    m = Math.floor((total % 3600) / 60);
  return h > 0 ? `${h}h${String(m).padStart(2, "0")}m` : `${m}m`;
};

export function clientDisplayName(source: string): string {
  const normalized = source.trim().toLowerCase();
  if (normalized === "codex" || normalized === "codex-cli") return "Codex";
  return agentVendorName(source);
}

export function projectName(cwd?: string | null): string | null {
  if (!cwd) return null;
  const trimmed = cwd.replace(/[\\/]+$/, "");
  const name = trimmed.split(/[\\/]/).pop();
  return name && name.length > 0 ? name : null;
}

export function shortenHomePath(path?: string | null): string | null {
  if (!path) return null;
  return path.replace(/^\/Users\/[^/]+/, "~").replace(/^\/home\/[^/]+/, "~");
}

export function NewsContent({
  event,
  snapshot,
  dismiss,
  dismissAll,
  completionCount,
  openCompletion,
  onStats,
  handoff,
  light = false,
  botLogos = true,
}: {
  event: IslandEvent;
  snapshot: Snapshot;
  dismiss: () => void;
  dismissAll?: () => void;
  completionCount?: number;
  openCompletion?: (id: string) => Promise<boolean>;
  onStats?: () => void;
  handoff?: (callback?: () => void) => void;
  light?: boolean;
  botLogos?: boolean;
}) {
  const zh = snapshotLanguage(snapshot) === "zh";
  if (event.kind === "completion") {
    return (
      <CompletionAlertView
        key={event.id}
        event={event}
        snapshot={snapshot}
        dismiss={dismiss}
        dismissAll={dismissAll}
        completionCount={completionCount}
        openCompletion={event.reminderNavigation ? openCompletion : undefined}
        onStats={onStats}
        handoff={handoff}
        light={light}
        botLogos={botLogos}
      />
    );
  }
  if (event.kind === "reset-credits") {
    return (
      <ResetCreditAlertView
        event={event}
        snapshot={snapshot}
        dismiss={dismiss}
        onStats={onStats}
        light={light}
        botLogos={botLogos}
        zh={zh}
      />
    );
  }
  return (
    <QuotaAlertView
      event={event}
      dismiss={dismiss}
      onStats={onStats}
      light={light}
      botLogos={botLogos}
      zh={zh}
    />
  );
}
