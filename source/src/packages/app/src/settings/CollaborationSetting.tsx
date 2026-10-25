import { useEffect, useRef, useState } from "react";
import type { Snapshot } from "@agent-hud/protocol";
import { Button } from "@agent-hud/ui/components/button";
import { Menu, Row, Section, useText } from "./SettingsPrimitives";

type Binding = { client: string; sessionId: string; valid: boolean };
type Task = {
  taskId: string;
  source: string;
  target: string;
  state: string;
  prompt: string;
  result?: string;
  note?: string;
  sequence: number;
  cancelRequested: boolean;
};
type Overview = {
  enabled: boolean;
  endpoints: { id: string; ready: boolean; note?: string }[];
  bindings: Binding[];
  tasks: Task[];
};
const label = (client: string) => (client === "codex" ? "Codex" : "CodeBuddy");
const key = (binding: { client: string; sessionId: string }) =>
  `${binding.client}/${binding.sessionId}`;

export function CollaborationSetting({
  snapshot,
  request,
}: {
  snapshot: Snapshot;
  request?: (method: string, params?: unknown) => Promise<unknown>;
}) {
  const t = useText(snapshot);
  const requestRef = useRef(request);
  requestRef.current = request;
  const [overview, setOverview] = useState<Overview | null>(null);
  const [unavailable, setUnavailable] = useState(false);
  const [error, setError] = useState("");
  const [pending, setPending] = useState(false);
  const [selection, setSelection] = useState("");
  const [prompt, setPrompt] = useState("");
  // Retain this id after ANY submission error: a lost UI response is not evidence
  // that dispatch failed. The next click must deduplicate the same task.
  const draft = useRef<{ id: string; owner: string; prompt: string } | null>(
    null,
  );

  const load = async () => {
    if (!requestRef.current) return;
    const state = (await requestRef.current(
      "collaboration_overview",
    )) as Overview;
    setOverview(state);
    setUnavailable(false);
  };
  useEffect(() => {
    let alive = true;
    let busy = false;
    const refresh = async () => {
      if (busy || !requestRef.current) return;
      busy = true;
      try {
        const value = (await requestRef.current(
          "collaboration_overview",
        )) as Overview;
        if (alive) {
          setOverview(value);
          setUnavailable(false);
        }
      } catch {
        if (alive) setUnavailable(true);
      } finally {
        busy = false;
      }
    };
    void refresh();
    const timer = setInterval(() => void refresh(), 3000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, []);

  const sessions = snapshot.sessions.filter(
    (s) =>
      (s.source === "codex" || s.source === "codebuddy") &&
      !s.parentId &&
      !s.id.startsWith("ah-helper-"),
  );
  const choices = new Map<
    string,
    { client: string; sessionId: string; title: string }
  >();
  for (const s of sessions)
    choices.set(`${s.source}/${s.id}`, {
      client: s.source,
      sessionId: s.id,
      title: s.title || s.id,
    });
  for (const b of overview?.bindings ?? [])
    if (!choices.has(key(b))) choices.set(key(b), { ...b, title: b.sessionId });
  const selected = choices.get(selection);
  const bound = overview?.bindings.find((b) => key(b) === selection && b.valid);
  const target = selected?.client === "codex" ? "codebuddy" : "codex";
  const targetReady = overview?.endpoints.some(
    (e) => e.id === target && e.ready,
  );
  const run = async (action: () => Promise<unknown>) => {
    setPending(true);
    setError("");
    try {
      await action();
      await load();
    } catch (failure) {
      setError(String(failure));
    } finally {
      setPending(false);
    }
  };
  const states: Record<string, string> = {
    queued: t("Queued", "排队中"),
    running: t("Running", "执行中"),
    completed: t("Result received", "已收到结果"),
    cancelled: t("Cancelled before dispatch", "已取消派发"),
    failed: t("Not delivered", "未送达"),
    deliveryUnknown: t(
      "Completion unknown — do not replay",
      "完成状态不明，请勿重试",
    ),
  };

  if (unavailable || !overview?.enabled) return null;

  return (
    <Section
      title={t("Agent collaboration · local experiment", "互相派活 · 本地实验")}
      subtitle={t(
        "Codex ⇄ CodeBuddy, using existing logins. One text task per helper; no further delegation. Results stay here and can be read by the originating agent.",
        "Codex ⇄ CodeBuddy，沿用现有登录态。每个辅助只做一次文字任务，不再转派。结果保留在这里，也可由发起 Agent 读取。",
      )}
    >
      <div style={{ padding: 16, display: "grid", gap: 12 }}>
        {unavailable ? (
          <p role="status">
            {t(
              "The local collaboration service is unavailable. Start the configured preview to use it.",
              "协作服务尚未连接，请使用已配置的预览版。",
            )}
          </p>
        ) : !overview ? (
          <p>{t("Reading collaboration status…", "正在读取协作状态…")}</p>
        ) : (
          <>
            <p>
              {overview.endpoints
                .map(
                  (e) =>
                    `${label(e.id)}：${e.ready ? t("login available", "登录态可用") : e.note}`,
                )
                .join(" · ")}
            </p>
            <Row title={t("Originating session", "发起会话")}>
              <Menu
                label={t("Originating session", "发起会话")}
                value={selection || "none"}
                options={[
                  ["none", t("Choose an existing session", "选择已有会话")],
                  ...Array.from(
                    choices,
                    ([id, s]) =>
                      [id, `${label(s.client)} · ${s.title.slice(0, 48)}`] as [
                        string,
                        string,
                      ],
                  ),
                ]}
                disabled={pending || !!draft.current}
                change={(value) => setSelection(value === "none" ? "" : value)}
              />
            </Row>
            {selected && (
              <div style={{ display: "flex", gap: 12, alignItems: "center" }}>
                <span>
                  {bound
                    ? t("This session may dispatch", "此会话已允许派活")
                    : t(
                        "Allow only this session to dispatch",
                        "仅允许此会话派活",
                      )}
                </span>
                <Button
                  size="sm"
                  disabled={pending}
                  onClick={() =>
                    void run(() =>
                      requestRef.current!("collaboration_bind", {
                        client: selected.client,
                        sessionId: selected.sessionId,
                        enabled: !bound,
                      }),
                    )
                  }
                >
                  {bound
                    ? t("Revoke", "撤销允许")
                    : t("Allow this session", "允许此会话")}
                </Button>
              </div>
            )}
            {bound && (
              <>
                <label htmlFor="collaboration-prompt">
                  {t(
                    `Text task for ${label(target)}`,
                    `派给 ${label(target)} 的文字任务`,
                  )}
                </label>
                <textarea
                  id="collaboration-prompt"
                  rows={3}
                  maxLength={8192}
                  value={prompt}
                  disabled={pending || !!draft.current}
                  onChange={(e) => setPrompt(e.target.value)}
                  style={{
                    width: "100%",
                    padding: 10,
                    border: "1px solid var(--border)",
                    borderRadius: 8,
                    background: "var(--background)",
                  }}
                />
                <div style={{ display: "flex", gap: 10 }}>
                  <Button
                    size="sm"
                    disabled={
                      pending ||
                      !prompt.trim() ||
                      !targetReady ||
                      !overview.enabled
                    }
                    onClick={() =>
                      void run(async () => {
                        const current = draft.current ?? {
                          id: `ah-${crypto.randomUUID()}`,
                          owner: selection,
                          prompt,
                        };
                        draft.current = current;
                        const receipt = (await requestRef.current!(
                          "collaboration_submit_ui",
                          {
                            client: selected!.client,
                            sessionId: selected!.sessionId,
                            taskId: current.id,
                            target,
                            prompt: current.prompt,
                          },
                        )) as Task;
                        if (!receipt.taskId)
                          throw new Error("Missing task receipt");
                        draft.current = null;
                        setPrompt("");
                      })
                    }
                  >
                    {pending
                      ? t("Sending…", "正在提交…")
                      : draft.current
                        ? t("Recover same receipt", "读取同一任务回执")
                        : t("Send one task", "派发一次任务")}
                  </Button>
                  {draft.current && <span>{draft.current.id}</span>}
                </div>
                <small>
                  {t(
                    "Only the text above is submitted. Maximum 120 seconds. No automatic retry. This uses the recipient's account quota.",
                    "只提交上面的任务文本，最多 120 秒，不自动重试。调用使用接收方账号额度。",
                  )}
                </small>
              </>
            )}
            {overview.tasks.length === 0 ? (
              <p>{t("No delegated tasks yet.", "还没有派发任务。")}</p>
            ) : (
              overview.tasks
                .slice()
                .sort((a, b) => b.sequence - a.sequence)
                .slice(0, 20)
                .map((task) => (
                  <article
                    key={task.taskId}
                    style={{
                      borderTop: "1px solid var(--border)",
                      paddingTop: 12,
                      display: "grid",
                      gap: 8,
                    }}
                  >
                    <strong>
                      {label(task.source)} → {label(task.target)} ·{" "}
                      {states[task.state] || task.state}
                    </strong>
                    <small>{task.taskId}</small>
                    <p
                      style={{
                        whiteSpace: "pre-wrap",
                        overflowWrap: "anywhere",
                      }}
                    >
                      {task.prompt}
                    </p>
                    {(task.result || task.note) && (
                      <pre
                        style={{
                          whiteSpace: "pre-wrap",
                          overflowWrap: "anywhere",
                          maxHeight: 240,
                          overflowY: "auto",
                          fontFamily: "inherit",
                        }}
                      >
                        {task.result || task.note}
                      </pre>
                    )}
                    {(task.state === "queued" || task.state === "running") && (
                      <Button
                        size="sm"
                        disabled={pending || task.cancelRequested}
                        onClick={() =>
                          void run(() =>
                            requestRef.current!("collaboration_cancel_ui", {
                              taskId: task.taskId,
                            }),
                          )
                        }
                      >
                        {task.cancelRequested
                          ? t("Stopping…", "正在停止…")
                          : t("Cancel task", "取消任务")}
                      </Button>
                    )}
                  </article>
                ))
            )}
          </>
        )}
        {error && (
          <p role="alert" style={{ overflowWrap: "anywhere" }}>
            {error}
          </p>
        )}
      </div>
    </Section>
  );
}
