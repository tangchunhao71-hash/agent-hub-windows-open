import { useState } from "react";
import type { Provider } from "@agent-hud/protocol";
import { Button } from "@agent-hud/ui/components/button";

export function ClientAvailability({
  providers,
  chinese,
}: {
  providers: Provider[];
  chinese: boolean;
}) {
  return (
    <div>
      <p>
        {chinese
          ? "检测只反映本机安装、配置和日志线索，不验证登录或订阅。客户端和账号需自行准备。"
          : "Detection reflects local installation, configuration and logs, not login or subscription validity. Bring your own clients and accounts."}
      </p>
      {providers.length === 0 && (
        <p>
          {chinese
            ? "暂未收到客户端检测结果，请稍后重试。"
            : "No client detection results yet. Try again shortly."}
        </p>
      )}
      <ul style={{ display: "grid", gap: 8, paddingLeft: 20 }}>
        {providers.map((p) => {
          const evidence = [
            p.discovery?.installed &&
              (chinese ? "发现安装" : "Installation found"),
            p.discovery?.configured &&
              (chinese ? "发现配置" : "Configuration found"),
            p.discovery?.usage && (chinese ? "发现日志" : "Logs found"),
          ].filter(Boolean);
          const status = evidence.length
            ? evidence.join(" · ")
            : p.detected
              ? chinese
                ? "发现本地来源"
                : "Local source found"
              : chinese
                ? "未检测到本地来源"
                : "No local source detected";
          const scope =
            p.dataStatus === "detection-only"
              ? chinese
                ? "仅支持发现"
                : "Detection only"
              : p.dataStatus === "account-only"
                ? chinese
                  ? "仅支持账号信息"
                  : "Account information only"
                : "";
          return (
            <li key={p.id}>
              <strong>{p.name}</strong>：{status}
              {scope ? `（${scope}）` : ""}
            </li>
          );
        })}
      </ul>
      <p>
        {chinese
          ? "未检测到不等于未安装：自定义安装位置或尚未产生使用记录时，可能没有可读数据。"
          : "No detection does not prove a client is absent: custom locations or no activity can leave no readable data."}
      </p>
    </div>
  );
}

export function ReleasePane({
  version,
  downloadUrl,
  chinese,
  providers,
  openDownload,
  refresh,
}: {
  version: string;
  downloadUrl: string | null;
  chinese: boolean;
  providers: Provider[];
  openDownload: () => Promise<unknown>;
  refresh: () => Promise<unknown>;
}) {
  const [error, setError] = useState("");
  const [pending, setPending] = useState(false);
  async function perform(job: () => Promise<unknown>) {
    setPending(true);
    setError("");
    try {
      await job();
    } catch (e) {
      setError(String(e));
    } finally {
      setPending(false);
    }
  }
  return (
    <section className="preference-group" style={{ display: "grid", gap: 16 }}>
      <h2>Agent Bot {version}</h2>
      <p>Windows 11 x64</p>
      <p>
        {chinese
          ? "手动更新：从购买时提供的固定网盘目录下载新版，退出 Agent Bot 后运行安装器。保留旧安装包和数据备份。"
          : "Manual updates: download a newer installer from the shared folder supplied with your purchase, quit Agent Bot and run it. Keep your previous installer and a data backup."}
      </p>
      {downloadUrl && (
        <Button
          variant="outline"
          disabled={pending}
          onClick={() => void perform(openDownload)}
        >
          {chinese ? "下载新版" : "Download a newer version"}
        </Button>
      )}
      <h3>{chinese ? "本机客户端" : "Local clients"}</h3>
      <ClientAvailability providers={providers} chinese={chinese} />
      <Button
        variant="outline"
        disabled={pending}
        onClick={() => void perform(refresh)}
      >
        {chinese ? "重新检测" : "Refresh detection"}
      </Button>
      {error && <p role="alert">{error}</p>}
    </section>
  );
}

const welcomeKey = "agentbot.windows.client-introduction.v1";
export function FirstRunClients({
  providers,
  chinese,
  onReview,
}: {
  providers: Provider[];
  chinese: boolean;
  onReview: () => void;
}) {
  const [dismissed, setDismissed] = useState(() => {
    try {
      return localStorage.getItem(welcomeKey) === "seen";
    } catch {
      return false;
    }
  });
  if (dismissed) return null;
  return (
    <section
      aria-label={chinese ? "首次使用" : "Getting started"}
      style={{
        border: "1px solid var(--border)",
        borderRadius: 12,
        padding: 16,
        marginBottom: 24,
      }}
    >
      <h2>{chinese ? "欢迎使用 Agent Bot" : "Welcome to Agent Bot"}</h2>
      <ClientAvailability providers={providers} chinese={chinese} />
      <div style={{ display: "flex", gap: 12 }}>
        <Button variant="outline" onClick={onReview}>
          {chinese ? "查看客户端设置" : "Client settings"}
        </Button>
        <Button
          onClick={() => {
            try {
              localStorage.setItem(welcomeKey, "seen");
            } catch {
              /* Still allow use when storage is unavailable. */
            }
            setDismissed(true);
          }}
        >
          {chinese ? "知道了" : "Got it"}
        </Button>
      </div>
    </section>
  );
}
