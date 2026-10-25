# Mac 构建与 ZIP

这是源码准备包，不能直接安装。macOS 二进制尚未生成；不得把此源码 ZIP 当作最终用户安装包。

基于已公开的 Windows 0.1.1 和原作者现有 Mac 实现。主许可证、来源与第三方素材声明继续保留。

在对应芯片的 Mac 上预先准备 Xcode Command Line Tools、Git、原生 stable Rust、Node.js 24、source/package.json 钉定的 pnpm 和 Bun。此脚本不会自动安装工具或登录账号。

源码 ZIP 不包含私人 Git 历史。首次解压后，在解压根目录建立仅含本包内容的本地构建基线；不需要 GitHub 登录，也不要在现有私人仓库内执行下面的初始化。

```bash
git init
git add LICENSE README.md SOURCE-MANIFEST.json source scripts docs .github .gitattributes .gitignore
git -c user.name="Local build" -c user.email="build@localhost" commit -m "Local Mac source baseline"
cd source
pnpm install --frozen-lockfile
cd ..
bash scripts/build-macos.sh
```

入口先核对工具，再生成本机目标的依赖声明、运行构建工具测试、使用 custom-protocol 内嵌前端构建 .app，并将程序、安装说明、更新说明、许可证、脱敏来源收据与校验值压缩为 ZIP。分别在 Apple Silicon 与 Intel 的 Mac 上运行，生成对应架构，当前不提供通用二进制。

输出：local-only/macos-delivery/Agent-Bot-0.1.1-macos-{arm64|x64}-review.zip；该目录的单独 .sha256 校验 ZIP。原始构建日志和带本机路径的收据留在 source/.build，不进入用户包。

本地检查只验证源码与打包逻辑。Mac 编译、Apple 签名、公证、干净机器安装、提醒与跳转、升级卸载均未完成；任何一项失败要保留收据，不能用旧版应用替充新产物。产物仅为本地待审候选，不自动上传、发布或售卖。
