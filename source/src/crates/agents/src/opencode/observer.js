// Agent HUD OpenCode session observer
import { mkdirSync, writeFileSync, renameSync, readdirSync, readFileSync, statSync, unlinkSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { createHash, randomUUID } from "node:crypto";

export const AgentHUDSessionObserver = async (input) => {
  const directory = join(process.env.XDG_DATA_HOME || join(homedir(), ".local", "share"), "opencode", "agent-hud", "turns");
  const sessions = new Map();
  let heartbeat;
  try {
    for (const name of readdirSync(directory)) {
      if (/^[a-f0-9]{64}\.json$/.test(name) && statSync(join(directory, name)).mtimeMs < Date.now() - 7 * 86400000) {
        unlinkSync(join(directory, name));
      }
    }
  } catch { /* The inbox is created on the first activity. */ }

  function session(id) {
    let value = sessions.get(id);
    if (!value) sessions.set(id, value = {});
    return value;
  }

  // A session deleted in OpenCode takes its records along, so it leaves Agent HUD's list as it left OpenCode's.
  function forget(id) {
    try {
      for (const name of readdirSync(directory)) {
        if (!/^[a-f0-9]{64}\.json$/.test(name)) continue;
        try {
          if (JSON.parse(readFileSync(join(directory, name), "utf8")).sessionID === "opencode:" + id) unlinkSync(join(directory, name));
        } catch { /* A record that cannot be read is pruned with the week's. */ }
      }
    } catch { /* There are no records yet. */ }
  }

  function navigationTarget() {
    const id = process.env.ITERM_SESSION_ID;
    return process.env.TERM_PROGRAM === "iTerm.app" && !process.env.TMUX && id
      ? { kind: "iTermSession", id } : undefined;
  }

  function publish(id, value, state, stopReason) {
    const { reply, active } = value;
    if (!active || value.child) return;
    const record = {
      version: 1, sessionID: "opencode:" + id, workspace: value.directory || input.directory, title: value.title,
      model: reply?.modelID, providerID: reply?.providerID, turnID: active.id,
      startedAtMs: active.startedAtMs, observedAtMs: Date.now(), state, stopReason,
      navigationTarget: navigationTarget(),
    };
    try {
      mkdirSync(directory, { recursive: true, mode: 0o700 });
      const name = createHash("sha256").update(record.sessionID + "\0" + record.turnID).digest("hex");
      const file = join(directory, name + ".json");
      const temporary = file + "." + process.pid + ".tmp";
      writeFileSync(temporary, JSON.stringify(record), { mode: 0o600 });
      renameSync(temporary, file);
    } catch { /* Observability must never interrupt OpenCode. */ }
  }

  function stopHeartbeatIfIdle() {
    if ([...sessions.values()].some(value => value.busy)) return;
    clearInterval(heartbeat);
    heartbeat = undefined;
  }

  function start(id, value) {
    if (!value.busy) {
      value.active = {
        id: randomUUID(), startedAtMs: Date.now(), priorReplyIds: new Set(value.replyIds),
      };
      // Cached history and a reply awaiting closure from an earlier busy
      // stretch are not evidence that this new stretch completed.
      value.reply = undefined;
    }
    value.busy = true;
    value.waiting = undefined;
    publish(id, value, "running");
    if (!heartbeat) {
      heartbeat = setInterval(() => {
        try {
          for (const [key, value] of sessions) if (value.busy) publish(key, value, "running");
        } catch { /* Observability must never interrupt OpenCode. */ }
      }, 15000);
      heartbeat.unref?.();
    }
  }

  // Idle is authoritative for stopping work. A reply can close after idle;
  // only its explicit successful stop can then upgrade the ended observation.
  function settle(id, value) {
    if (!value.active) return;
    value.busy = false;
    stopHeartbeatIfIdle();
    const { reply, prompt } = value;
    if (reply && !reply.completedAtMs && (!prompt || reply.parentID === prompt.id)) {
      value.waiting = true;
      publish(id, value, "ended", reply.stopReason);
      return;
    }
    const completed = reply && !reply.error && !reply.summary && reply.finish === "stop"
      && reply.completedAtMs && (!prompt || reply.parentID === prompt.id);
    publish(id, value, completed ? "completed" : "ended", reply?.stopReason);
    value.reply = value.prompt = value.active = value.waiting = undefined;
  }

  return {
    // Never throws or rejects: OpenCode 1.18 runs this inside the call that marks a session idle.
    event: async (payload) => {
      try {
        const event = payload?.event, properties = event?.properties || {}, info = properties.info || {};
        switch (event?.type) {
          case "session.created":
          case "session.updated":
            if (info.id) Object.assign(session(info.id), { child: !!info.parentID, title: info.title, directory: info.directory });
            break;
          case "session.deleted":
            if (info.id) { sessions.delete(info.id); forget(info.id); stopHeartbeatIfIdle(); }
            break;
          case "message.updated": {
            // OpenCode updates earlier messages too, so the newest prompt and reply go by when they were created. The
            // message is live and keeps changing, so the fields are copied now.
            if (!info.sessionID || !info.id) break;
            const value = session(info.sessionID), createdAtMs = info.time?.created || 0;
            const newest = (known) => !known || known.id === info.id || createdAtMs >= known.createdAtMs;
            if (info.role === "user" && newest(value.prompt)) value.prompt = { id: info.id, createdAtMs };
            if (info.role === "assistant") {
              const replyIds = value.replyIds ||= new Set();
              replyIds.add(info.id);
              if (replyIds.size > 128) replyIds.delete(replyIds.values().next().value);
            }
            if (info.role === "assistant" && value.active
              && !value.active.priorReplyIds.has(info.id)
              && Number.isSafeInteger(createdAtMs) && createdAtMs >= value.active.startedAtMs
              && newest(value.reply)) {
              value.reply = {
                id: info.id, parentID: info.parentID, modelID: info.modelID, providerID: info.providerID, finish: info.finish,
                error: !!info.error, summary: info.summary === true, createdAtMs,
                completedAtMs: Number.isSafeInteger(info.time?.completed) && info.time.completed >= createdAtMs
                  ? info.time.completed : undefined,
                stopReason: info.error?.name === "MessageAbortedError" ? "aborted" : info.error ? "error" : undefined,
              };
              if (value.waiting && value.reply.completedAtMs) settle(info.sessionID, value);
            }
            break;
          }
          case "message.removed": {
            const value = sessions.get(properties.sessionID);
            if (value?.prompt?.id === properties.messageID) value.prompt = undefined;
            if (value?.reply?.id === properties.messageID) value.reply = undefined;
            break;
          }
          case "session.status":
          case "session.idle": {
            const value = sessions.get(properties.sessionID), type = event.type === "session.idle" ? "idle" : properties.status?.type;
            if (type === "busy" && properties.sessionID) {
              const busy = session(properties.sessionID);
              start(properties.sessionID, busy);
            }
            if (type === "idle" && value) settle(properties.sessionID, value);
            break;
          }
        }
      } catch { /* Observability must never interrupt OpenCode. */ }
    },
  };
};
