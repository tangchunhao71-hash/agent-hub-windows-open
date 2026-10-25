# Agent Bot for Windows

基于 [Agent HUD](https://github.com/jazzenchen/agent-hud-open) 修改的 Windows 状态面板。集中查看已有 AI 客户端的状态、额度和任务完成提醒；保留一排 Agent、常用主 Agent 置顶及 Bot 动效。

当前版本为 **0.1.1**，本仓库提供源码；安装包仍为本地发行候选。支持目标为 Windows 11 x64；干净机器安装与完整原生交互验收未完成，不能当作已通过商业发行认证的成品。

## 首发范围

- 读取已有客户端日志，展示有来源依据的状态、额度及完成提醒；无可用数据时不编造实时数值。不同客户端与版本的可用程度不同。
- Claude 静态标识保持原海胆形图标，活动时使用原创圆胖橙色四帧 Bot。图标只用于识别实际来源，不表示厂商背书。
- 完成卡可请求打开对应客户端；DSH 精确跳转依赖另行安装本地 DSH 插件。没有回执时不把“应用已打开”当作“会话已定位”。
- 网盘手动更新：“关于”页显示版本和本机客户端检测；仅在配置真实 HTTPS 分享地址后显示“下载新版”。首次使用提示已发现的客户端线索。安装器不包含账号、聊天、个人设置或本地测试配置。

互相派活仅保留为本地实验，不属于本版承诺。客户端回调安装、旧版插件迁移、HUD 人工审批、商业售卖认证、Windows 10/ARM64 与多屏兼容保证也不在首发范围内。

## 安装与使用

见 [安装、升级与卸载](docs/release/INSTALL.md)、[隐私与本地数据](docs/release/PRIVACY.md)、[发行验收](docs/release/ACCEPTANCE.md)。校验下载文件时使用同目录 `SHA256SUMS.txt`。未签名；不要关闭系统防护来运行未知来源副本。

程序名与安装名均为 Agent Bot；应用 ID 为 `app.agentbot.windows`。与原 Agent HUD 使用不同数据目录，不自动迁移旧设置或接管旧版 hooks。内部 Rust crate、npm 包和配套程序仍保留上游技术名称。

## 源码构建

工作区为 `source/`，CI 位于仓库根。需要 Windows x64 Rust/MSVC 工具链、Node.js 24、锁定版本 pnpm，以及 Bun（构建 MCP 配套程序）。普通使用者只需要 WebView2，不需要这些开发工具。

```powershell
cd source
pnpm install --frozen-lockfile
cd ..
pwsh -NoProfile -File scripts/build-windows.ps1 -Check
pwsh -NoProfile -File scripts/build-windows.ps1 -Bundle -Receipt source/.build/release-receipt.json
```

不要直接以缺少 `custom-protocol` 的 Cargo release 构建交付：正式程序必须内嵌前端，不能依赖 localhost 开发服务器。构建入口会生成源码与产物指纹；只分发通过验收且哈希一致的包。

## 来源与许可

上游主代码为 Apache-2.0，见 [LICENSE](LICENSE)；修改说明见 [CHANGELOG](docs/release/CHANGELOG.md)。第三方说明在 [source/docs/third-party.md](source/docs/third-party.md)，品牌图标保留原声明和 LobeHub 许可。React Bits 入场动效已用已有 Motion 依赖重新实现，React Bits 与 GSAP 已移除；旧 Clawd 精灵表不随包分发。

公开源码来自独立导出，不带私人 Git 历史、账号、运行数据库、截图和内部交接。源码公开不代表安装包已完成销售验收；网盘交付地址尚未配置，仓库没有提供正式安装包下载。

第三方代码及素材遵循各自条款，主代码的 Apache-2.0 不覆盖第三方商标与素材权利。依赖许可补充及未关闭事项见 [RELEASE_LICENSE_SUPPLEMENT](source/docs/release-license-supplement.txt)。
