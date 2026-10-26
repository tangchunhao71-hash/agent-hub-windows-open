# Mac 安装与更新

当前为待验证的 Mac 发行候选。基于原作者已有的 Mac 适配，保留状态面板、常用 Agent 置顶、Bot 动效与完成提醒。修改版尚未在 Mac 实机验证。

最终用户包将按芯片分为 arm64（Apple Silicon）与 x64（Intel），不能混用。最低系统目标为 macOS 12；登录启动能力另依赖 macOS 13 及系统授权。安装不需要 Node、Rust、pnpm、Bun 或 Windows WebView2。

1. 校验 ZIP 旁的 SHA-256，解压。
2. 把 Agent Bot.app 拖入“应用程序”，再双击打开。
3. 自行安装并登录要使用的 AI 客户端。不同客户端的数据支持不同，未发现数据时可能显示为空。

尚未核验 Apple 开发者签名与公证。系统可能阻止打开；不要关闭 Gatekeeper 或安全防护，不将此候选当作已验收的售卖安装包。

更新：退出 Agent Bot，保留上一版及设置备份，用新 Agent Bot.app 替换旧版。当前没有自动更新服务或固定网盘地址。回退前确认数据格式兼容。

卸载：退出程序，将 Agent Bot.app 移入废纸篓。数据默认保留在 ~/Library/Application Support/app.agentbot.macos；需要清理时先备份，再自行删除。勿删除原 Agent HUD 或 AI 客户端的目录。

暂不提供：互相派活、旧版配置与额度历史迁移、接管原版 hooks/插件、完整客户端适配保证。精确会话跳转、提醒、拖拽、缩放、多屏、升级与卸载均需 Mac 实机验证。
