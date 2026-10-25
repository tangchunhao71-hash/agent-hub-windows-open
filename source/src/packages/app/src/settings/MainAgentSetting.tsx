import { useState } from "react";
import { agentVendorName } from "@agent-hud/hud/model";
import {
  Menu,
  Row,
  Section,
  useText,
  type SettingsPaneProps,
} from "./SettingsPrimitives";

export function MainAgentSetting({
  snapshot,
  config,
  update,
}: SettingsPaneProps) {
  const t = useText(snapshot);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const options: [string, string][] = [
    ["none", t("Keep current order", "保持现有顺序")],
    ...snapshot.providers.map(
      (p) => [p.id, agentVendorName(p.id)] as [string, string],
    ),
  ];
  if (config.mainAgent && !options.some(([id]) => id === config.mainAgent))
    options.push([config.mainAgent, config.mainAgent]);
  return (
    <Section title={t("Main agent", "主 Agent")}>
      <Row title={t("Preferred main agent", "常用主 Agent")}>
        <Menu
          label={t("Main agent", "主 Agent")}
          value={config.mainAgent || "none"}
          options={options}
          disabled={pending}
          change={async (value) => {
            setPending(true);
            setError("");
            try {
              if (
                (await update({ mainAgent: value === "none" ? "" : value })) ===
                false
              )
                throw Error(
                  t("Could not save main agent", "主 Agent 保存失败"),
                );
            } catch (failure) {
              setError(String(failure));
            } finally {
              setPending(false);
            }
          }}
        />
      </Row>
      <p>
        {t(
          "Pins this agent first. Use the collaboration panel below to authorize individual sessions and delegate tasks.",
          "将此 Agent 固定排第一。派活请在下方协作面板单独允许具体会话。",
        )}
      </p>
      {error && <p role="alert">{error}</p>}
    </Section>
  );
}
