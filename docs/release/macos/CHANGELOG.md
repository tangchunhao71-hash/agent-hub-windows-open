# Mac 候选修改说明

版本：0.1.2 候选，基于 Windows 已冻结提交 70e0a8cd8e166cce5d38592a0b1f5d7788bf81d7。

- 复用原作者 jazzenchen/agent-hud-open 的 macOS 原生窗口、系统交互、客户端读取与前端实现。
- 保留已完成的 Agent Bot 品牌、一排 Agent、常用主 Agent 置顶、橙色 Bot 动效与完成提醒实现。
- 使用独立应用 ID app.agentbot.macos 与数据目录；不自动导入原版配置或历史。
- 将 Windows 可执行文件资源移到 Windows 专用配置；Mac 随包保留原生 engine，不随包提供实验 MCP relay。
- 增加 Mac 构建、许可证生成、架构检查与 ZIP 打包入口；保留上游许可证及素材声明。

已合入公开的 Windows 0.1.1 基线。上游曾有 Mac 验证声明，不代表 Agent Bot 本次修改版通过实机验收。

0.1.2：补充原创部分 MIT、许可范围及第三方取证；替换应用图标和壁纸；将 react-remove-scroll-bar 固定到可核对来源的 2.3.7；增加实际签名检查和严格打包门槛。未持有 Developer ID，候选包不承诺 Gatekeeper 放行或实机验收通过。
