import type { PanelLayout, SourcePreferences } from "@agent-hud/protocol";
import type { NativeHUDMonitor } from "./lib/use-hud-geometry";
import SourcesSettings from "./SourcesSettings";
import { MainAgentSetting } from "./settings/MainAgentSetting";
import { CollaborationSetting } from "./settings/CollaborationSetting";
import type { AgentPluginItem } from "./settings/AgentPluginsPane";
import { DisplayPane } from "./settings/DisplayPane";
import { GeneralPane } from "./settings/GeneralPane";
import type { SettingsPaneProps } from "./settings/SettingsPrimitives";

export type { SettingsPaneProps } from "./settings/SettingsPrimitives";
export { DisplayPane } from "./settings/DisplayPane";
export { GeneralPane } from "./settings/GeneralPane";

export default function SettingsPane({
  page,
  settingsProps,
  ready,
  selectedId,
  monitors,
  selectScreen,
  chinese,
  layout,
  onLayout,
  preferences,
  onPreferences,
  request,
}: {
  page: string;
  settingsProps: SettingsPaneProps;
  ready: boolean;
  selectedId: string;
  monitors: NativeHUDMonitor[];
  selectScreen: (id: string) => void;
  chinese: boolean;
  layout: PanelLayout;
  onLayout: (layout: PanelLayout) => Promise<unknown>;
  preferences: Record<string, SourcePreferences>;
  onPreferences: (source: string, patch: SourcePreferences) => Promise<unknown>;
  agentPlugins?: AgentPluginItem[];
  request?: (method: string, params?: unknown) => Promise<unknown>;
}) {
  if (page === "general") return <GeneralPane {...settingsProps} />;
  if (page === "display")
    return ready && selectedId ? (
      <DisplayPane
        key={selectedId}
        {...settingsProps}
        monitors={monitors}
        selectedScreenId={selectedId}
        onSelectScreen={selectScreen}
      />
    ) : (
      <div className="connection-placeholder">
        {chinese ? "正在读取显示器…" : "Reading displays…"}
      </div>
    );
  if (page === "agents")
    return (
      <div style={{ display: "grid", gap: 24, width: "100%" }}>
        <MainAgentSetting {...settingsProps} />
        <CollaborationSetting
          snapshot={settingsProps.snapshot}
          request={request}
        />
        <SourcesSettings
          snapshot={settingsProps.snapshot}
          action={settingsProps.action}
          layout={layout}
          onLayout={onLayout}
          preferences={preferences}
          onPreferences={onPreferences}
        />
      </div>
    );
  if (page === "agent-plugins")
    return (
      <section className="preference-group">
        <h2>{chinese ? "客户端接入" : "Client integrations"}</h2>
        <p>
          {chinese
            ? "Agent Bot 当前通过已有客户端日志读取状态，不接管原版 Agent HUD 的回调或插件。插件安装、人工审批和互相派活不属于本版功能。"
            : "Agent Bot reads existing client logs. It does not take over Agent HUD hooks or plugins. Plugin installation, approval actions and agent delegation are outside this release."}
        </p>
      </section>
    );
  return null;
}
