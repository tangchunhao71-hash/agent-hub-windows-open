import { useState } from "react";
import type { BotAvatarStyle, BotIdentity } from "@agent-hud/protocol";
import type { ActiveBotIdentity } from "./bot-identities.ts";
import { GrokFoundationAvatar } from "./GrokFoundationAvatar";
import { TaskProgressMark } from "./TaskProgressMark";
import type { GrokAvatarState } from "./grok-avatar-motion.ts";

export function BotIdentityMark({
  identity,
  size = 28,
  working = false,
  waiting = false,
  light = false,
  reduceMotion = false,
  pointer,
  seed,
  state,
}: {
  identity: BotIdentity;
  size?: number;
  working?: boolean;
  waiting?: boolean;
  light?: boolean;
  reduceMotion?: boolean;
  pointer?: { x: number; y: number } | null;
  seed?: string;
  state?: GrokAvatarState;
}) {
  const [failed, setFailed] = useState<string | null>(null),
    avatar = identity.avatar;
  const photo =
    avatar.kind === "photo" &&
    failed !== avatar.dataUrl &&
    avatar.dataUrl.length <= 2_800_000 &&
    /^data:image\/(png|jpeg|gif|webp);base64,/.test(avatar.dataUrl)
      ? avatar.dataUrl
      : null;
  const style: BotAvatarStyle =
    avatar.kind === "photo" ? avatar.fallback : avatar;
  return (
    <span
      title={identity.name}
      aria-label={identity.name}
      data-bot-identity=""
      style={{
        width: size,
        height: size,
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        position: "relative",
        flexShrink: 0,
      }}
    >
      {photo ? (
        <img
          src={photo}
          alt=""
          onError={() => setFailed(photo)}
          draggable={false}
          style={{
            width: size * 0.744,
            height: size * 0.744,
            borderRadius: "50%",
            objectFit: "cover",
            display: "block",
          }}
        />
      ) : (
        <GrokFoundationAvatar
          shape={style.shape}
          color={style.color}
          size={size}
          seed={seed ?? identity.name}
          state={state ?? (waiting ? "waiting" : working ? "working" : "idle")}
          light={light}
          reduceMotion={reduceMotion}
          pointer={pointer}
        />
      )}
    </span>
  );
}

export function ActiveBotMarks({
  bots,
  extra,
  tile,
  mark,
  gap,
  size,
  light,
  reduceMotion,
  pointer,
}: {
  bots: ActiveBotIdentity[];
  extra: ActiveBotIdentity[];
  tile: number;
  mark: number;
  gap: number;
  size: number;
  light: boolean;
  reduceMotion: boolean;
  pointer?: { x: number; y: number } | null;
}) {
  return (
    <span style={{ display: "inline-flex", gap, alignItems: "flex-start" }}>
      {bots.map((bot) => (
        <span
          key={bot.sessionId}
          title={bot.identity.name}
          style={{
            width: tile,
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            gap: 4,
          }}
        >
          <span
            data-hud-mark-slot=""
            style={{
              width: mark,
              height: mark,
              position: "relative",
              display: "grid",
              placeItems: "center",
            }}
          >
            <BotIdentityMark
              identity={bot.identity}
              size={size}
              working={bot.state === "running"}
              waiting={bot.state === "waiting"}
              light={light}
              reduceMotion={reduceMotion}
              pointer={pointer}
              seed={bot.sessionId}
            />
            <TaskProgressMark
              progress={{
                mode: bot.state === "running" ? "working" : "waiting",
                running: bot.state === "running" ? 1 : 0,
                waiting: bot.state === "waiting" ? 1 : 0,
                label: bot.identity.name,
                detail: bot.identity.name,
              }}
              size={Math.max(13, Math.min(19, mark * 0.42))}
              light={light}
              reduceMotion={reduceMotion}
            />
          </span>
          <span
            data-hud-bot-name=""
            style={{
              fontSize: 10,
              lineHeight: "12px",
              maxWidth: tile,
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
              color: light ? "#111" : "#fff",
              textShadow: light ? "0 1px 2px #ffffffbb" : "0 1px 3px #000b",
            }}
          >
            {bot.identity.name}
          </span>
        </span>
      ))}
      {extra.length > 0 ? (
        <span
          title={extra.map((b) => b.identity.name).join("\n")}
          aria-label={extra.map((b) => b.identity.name).join(", ")}
          style={{
            width: tile,
            height: mark,
            display: "grid",
            placeItems: "center",
            fontSize: 11,
            color: light ? "#111" : "#fff",
          }}
        >
          +{extra.length}
        </span>
      ) : null}
    </span>
  );
}
