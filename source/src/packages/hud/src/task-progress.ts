import type { Session } from "@agent-hud/protocol";

export interface TaskProgress {
  mode: "working" | "waiting";
  running: number;
  waiting: number;
  label: string;
  detail: string;
}

/** A source reports activity, not a fraction of an unknowable task total. */
export function taskProgress(
  sessions: readonly Session[],
  zh: boolean,
): TaskProgress | null {
  const active = sessions.filter(
    (session) => session.state === "running" || session.state === "waiting",
  );
  if (!active.length) return null;
  const running = active.filter(
    (session) => session.state === "running",
  ).length;
  const waiting = active.length - running;
  const phase = (session: Session): string => {
    if (session.state === "waiting") return zh ? "等待回复" : "Waiting for you";
    const activity = session.liveFacts?.activity;
    if (activity?.retrying) return zh ? "正在重试" : "Retrying";
    if (activity?.compactingSince)
      return zh ? "整理上下文" : "Compacting context";
    if (activity?.delegating)
      return zh ? "子 Agent 工作中" : "Subagents working";
    switch (activity?.openCall?.work ?? activity?.work) {
      case "read":
        return zh ? "读取资料" : "Reading";
      case "edit":
        return zh ? "修改文件" : "Editing";
      case "shell":
      case "command":
        return zh ? "执行命令" : "Running a command";
      case "agents":
        return zh ? "子 Agent 工作中" : "Subagents working";
      case "question":
        return zh ? "处理问题" : "Preparing a question";
      default:
        return zh ? "工作中" : "Working";
    }
  };
  const label = zh
    ? [running && `${running} 个任务运行中`, waiting && `${waiting} 个等待回复`]
        .filter(Boolean)
        .join("，")
    : [running && `${running} running`, waiting && `${waiting} waiting for you`]
        .filter(Boolean)
        .join(", ");
  return {
    mode: running ? "working" : "waiting",
    running,
    waiting,
    label,
    detail: [
      label,
      ...active.map(
        (session) => `${session.title || session.id} · ${phase(session)}`,
      ),
      zh
        ? "任务未提供可计算的完成百分比"
        : "Task completion percentage is unavailable",
    ].join("\n"),
  };
}
