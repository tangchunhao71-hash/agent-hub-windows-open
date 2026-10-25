import { useRef, useState } from "react";
import {
  ArrowUpRight,
  ChartNoAxesColumn,
  MessageSquareText,
  RotateCcw,
  X,
} from "lucide-react";
import type { IslandEvent, Snapshot } from "@agent-hud/protocol";
import { usePanelServices } from "../PanelContext";
import { clientDisplayName, compactDuration, newsTitle } from "../IslandNews";
import { ReminderMark } from "./ReminderMark";
import { dateFormat, snapshotLanguage } from "@agent-hud/ui/format";
import { alertFill, alertInk, alertText, onAlertFill } from "../tokens";
import { useGround } from "../ground";
import { cardColumn } from "../card-column";
import { completionSession } from "../completion-reminder";

export function CompletionAlertView({
  event,
  snapshot,
  dismiss,
  dismissAll,
  completionCount = 1,
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
  const services = usePanelServices(),
    zh = snapshotLanguage(snapshot) === "zh",
    [failed, setFailed] = useState(false),
    [closeHover, setCloseHover] = useState(false);
  const opening = useRef(false);
  const [pending, setPending] = useState(false);
  const session = completionSession(event, snapshot.sessions);
  const hasSession = Boolean(
    openCompletion || (session?.navigationTarget && services?.openSession),
  );
  const ground = useGround(light);
  const ink = (alpha: number) => alertInk(alpha, ground);
  const turnAccent = alertFill("turn", ground);
  const turnText = alertText("turn", ground);
  const onTurnAccent = onAlertFill(ground);

  const startedRaw = event.startedAt ?? session?.liveFacts?.turnStartedAt;
  const started = startedRaw ? Date.parse(startedRaw) : NaN;
  const ended = Date.parse(event.at);
  const message = event.message ?? session?.liveFacts?.message;

  const openUsage = () => {
    dismiss();
    const run = () => {
      if (session && services?.selectSession) services.selectSession(session);
      else onStats?.();
    };
    if (handoff) handoff(run);
    else run();
  };
  const openSession = async () => {
    if (!hasSession) return;
    if (!openCompletion && (!session || !services?.openSession)) return;
    if (opening.current) return;
    opening.current = true;
    setPending(true);
    setFailed(false);
    try {
      const opened = openCompletion
        ? await openCompletion(event.id)
        : await services!.openSession!(session!);
      if (opened === false) setFailed(true);
      else {
        dismiss();
        handoff?.();
      }
    } catch {
      setFailed(true);
    } finally {
      opening.current = false;
      setPending(false);
    }
  };

  return (
    <article style={{ ...cardColumn(18), fontSize: 12, color: "inherit" }}>
      <header style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <ReminderMark
          event={event}
          size={24}
          light={light}
          botLogos={botLogos}
        />
        <div style={{ flex: 1, ...cardColumn(3), minWidth: 0 }}>
          <strong
            style={{
              fontSize: 13,
              fontWeight: 600,
              color: ink(0.94),
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: event.botIdentity ? "normal" : "nowrap",
              overflowWrap: "anywhere",
            }}
          >
            {event.botIdentity?.name ?? clientDisplayName(event.source)}
          </strong>
          {event.model && event.model !== "Unknown" && (
            <span
              style={{
                fontSize: 11,
                color: ink(0.45),
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
              }}
            >
              {event.model}
            </span>
          )}
        </div>
        <MessageSquareText size={12} strokeWidth={2.2} color={turnText} />
        <span style={{ color: turnText, fontSize: 12, fontWeight: 500 }}>
          {event.botIdentity
            ? zh
              ? "已完成"
              : "Completed"
            : zh
              ? "有新回复"
              : "New reply"}
        </span>
        <button
          type="button"
          aria-label={zh ? "关闭回复提醒" : "Dismiss reply"}
          title={zh ? "关闭回复提醒" : "Dismiss reply"}
          onClick={dismiss}
          onPointerEnter={() => setCloseHover(true)}
          onPointerLeave={() => setCloseHover(false)}
          style={{
            width: 24,
            height: 24,
            display: "grid",
            placeItems: "center",
            border: 0,
            borderRadius: 6,
            background: closeHover ? ink(0.08) : "transparent",
            color: ink(closeHover ? 0.92 : 0.65),
            cursor: "pointer",
          }}
        >
          <X size={10} strokeWidth={2.4} />
        </button>
      </header>
      {dismissAll && (
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <span style={{ flex: 1, fontSize: 11, color: ink(0.6) }}>
            {zh
              ? `${completionCount} 条完成提醒待收起`
              : `${completionCount} completion reminder${completionCount === 1 ? "" : "s"}`}
          </span>
          <button
            type="button"
            onClick={dismissAll}
            aria-label={
              zh
                ? `全部收起 ${completionCount} 条完成提醒`
                : `Dismiss all ${completionCount} completion reminders`
            }
            title={
              zh
                ? "仅收起完成提醒，回复保留在原会话中"
                : "Dismiss completion reminders; replies stay in their conversations"
            }
            style={{
              border: `1px solid ${ink(0.18)}`,
              borderRadius: 6,
              background: "transparent",
              color: ink(0.8),
              padding: "4px 9px",
              fontSize: 11,
              cursor: "pointer",
              whiteSpace: "nowrap",
            }}
          >
            {zh ? "全部收起" : "Dismiss all"}
          </button>
        </div>
      )}
      <div style={cardColumn(8)}>
        <strong
          style={{
            fontSize: 15,
            fontWeight: 500,
            color: ink(0.95),
            overflowWrap: "anywhere",
            display: "-webkit-box",
            WebkitLineClamp: 2,
            WebkitBoxOrient: "vertical",
            overflow: "hidden",
          }}
        >
          {event.title ?? newsTitle(event, zh)}
        </strong>
        {message && (
          <p
            style={{
              margin: 0,
              fontSize: 12,
              lineHeight: "15px",
              color: ink(0.68),
              display: "-webkit-box",
              WebkitLineClamp: 4,
              WebkitBoxOrient: "vertical",
              overflow: "hidden",
              whiteSpace: "pre-wrap",
              overflowWrap: "anywhere",
            }}
          >
            {message}
          </p>
        )}
      </div>
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          fontSize: 11,
          fontVariantNumeric: "tabular-nums",
          color: ink(0.5),
        }}
      >
        <time>
          {dateFormat(undefined, { hour: "numeric", minute: "2-digit" }).format(
            new Date(event.at),
          )}
        </time>
        {Number.isFinite(started) && ended > started && (
          <span>{compactDuration((ended - started) / 1000)}</span>
        )}
      </div>
      <div style={cardColumn(8)}>
        {failed && hasSession ? (
          <span style={{ fontSize: 11, color: ink(0.65) }}>
            {zh
              ? "无法回到会话，请重试或查看 Token 用量"
              : "Couldn't return to session. Retry or view token usage."}
          </span>
        ) : (
          !hasSession && (
            <span style={{ fontSize: 11, color: ink(0.5) }}>
              {zh ? "暂时无法回到原会话" : "Session return unavailable"}
            </span>
          )
        )}
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          <button
            type="button"
            onClick={openUsage}
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 4,
              height: 22,
              padding: "0 9px",
              borderRadius: 6,
              border: `1px solid ${ink(0.18)}`,
              background: "transparent",
              color: ink(0.6),
              fontSize: 11,
              fontWeight: 500,
              whiteSpace: "nowrap",
              cursor: "pointer",
            }}
          >
            <ChartNoAxesColumn size={9} strokeWidth={2.6} />
            {zh ? "查看 Token 用量" : "View token usage"}
          </button>
          <span style={{ flex: 1, minWidth: 8 }} />
          {hasSession && (
            <button
              type="button"
              onClick={openSession}
              disabled={pending}
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 4,
                height: 22,
                padding: "0 9px",
                borderRadius: 6,
                border: 0,
                background: turnAccent,
                color: onTurnAccent,
                fontSize: 11,
                fontWeight: 500,
                whiteSpace: "nowrap",
                cursor: "pointer",
              }}
            >
              {failed ? (
                <RotateCcw size={9} strokeWidth={2.6} />
              ) : (
                <ArrowUpRight size={9} strokeWidth={2.6} />
              )}
              {failed
                ? zh
                  ? "重试会话跳转"
                  : "Retry session"
                : zh
                  ? "回到会话"
                  : "Return to session"}
            </button>
          )}
        </div>
      </div>
    </article>
  );
}
