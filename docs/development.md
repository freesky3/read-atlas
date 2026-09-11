# Development / 开发指南

当前支持目标为 **Windows x64 / MSVC Alpha**。macOS 和 Linux 尚未做发布验收。Node 版本见根目录 `.node-version`，Rust 版本与组件见 `rust-toolchain.toml`。

## 环境准备

1. 安装 Node.js 24.17.0，确认 `node --version` 与 `npm --version` 可用。
2. 安装 Visual Studio 2022 Build Tools，选择“使用 C++ 的桌面开发”，包含 MSVC 和 Windows SDK。
3. 安装 rustup，使用 `x86_64-pc-windows-msvc` 工具链；进入仓库时 rustup 按配置使用 Rust 1.96.1。
4. 安装 Microsoft Edge WebView2 Evergreen Runtime。
5. GitHub 下载或 clone 本项目，进入含 `package.json` 的目录。

依赖前置要求可参考 [Tauri Windows prerequisites](https://v2.tauri.app/start/prerequisites/#windows)。项目不依赖相邻的浏览器扩展仓库。

## 安装与启动

```powershell
npm ci
npm run tauri -- dev
```

`npm ci` 严格使用锁文件。首次运行需要网络下载 npm 与 Cargo 依赖。不要把用户工作区建在源码目录中。

`npm run dev` 只启动浏览器 UI 演示，使用内存 adapter，不会模拟真实 OCR、系统凭据或已通过的服务商能力检查。修改 Rust 后需重启桌面开发进程。

## 检查

```powershell
npm run check:repo
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --locked
git diff --check
```

`npm test` 已默认串行，防止 jsdom 并行占用过多内存。Rust 默认最多两个编译任务。更具体的 Windows 栈溢出、OOM 与 PATH 排查见 [Windows 构建排障](windows-dev-build.md)。

CI 验证仓库规则、前端测试与构建、Rust 测试。不要将浏览器测试通过写成 Windows 拖拽、真实 Provider 或安装升级已经通过。

## 安全与依赖

```powershell
npm audit
powershell -ExecutionPolicy Bypass -File tools/check-secrets.ps1
cargo install cargo-audit --locked
cargo audit --file src-tauri/Cargo.lock
```

密钥扫描脚本下载固定版本 Gitleaks，校验下载包后扫描完整可达 Git 历史，以及由 Git 跟踪／未忽略文件组成的当前候选快照。输出已脱敏，报告仅写入被忽略的 `tmp/`。

不要使用自动强制升级跳过评估。升级 PDF.js 必须验证 Reader、worker 和生产构建；升级 Vitest 必须重跑前端测试。

## 合成 PDF 和人工验收

需要 Python 及 `reportlab`、`Pillow`（`python -m pip install reportlab Pillow`）。可通过 `READ_DESKTOP_PYTHON` 指定解释器。

```powershell
npm run fixtures
```

生成的 PDF 在 `tests/fixtures/generated/`，不提交到 Git。它们覆盖文本、图表／公式示意、扫描图像、500 页、重复副本与移动源文件，并附 SHA-256 清单。合成夹具用于工程回归，不能替代真实论文的 OCR 或内容质量验收。

## 构建安装包

```powershell
npm run notices
npm run tauri -- build --bundles nsis,msi
```

输出位于 `src-tauri/target/release/bundle/`。NSIS 是安装程序 `.exe`，MSI 是 Windows Installer 包。许可证和第三方声明随应用资源一起分发。

GitHub 的 Windows installer 工作流只生成下载用 artifact，不自动发布 Release。签名证书不进入仓库。正式分发前需验证签名／SmartScreen 提示、干净机器安装、升级、卸载和工作区保留，见 [发布检查表](release-checklist.md)。

## 本地调试工具

`tools/inspect_lens.ts` 与 `tools/apply_normalize.ts` 为可选的只读诊断工具。运行前明确设置 `READ_DESKTOP_DB`，指向希望检查的数据库。它们需要单独安装 `tsx` 与 `better-sqlite3`；这两个诊断依赖不是应用构建必需依赖。

这些工具会在终端输出成果内容，只能用于你有权读取的工作区，不要把输出作为公开日志上传。

## English setup summary

Install the pinned Node.js and Rust MSVC toolchain, Visual Studio C++ Build Tools, Windows SDK, and WebView2. Run `npm ci` and `npm run tauri -- dev` from the repository root. The browser development server is a memory-backed demo.

Run the checks above before submitting a change. Use synthetic PDFs and a separate workspace. Live provider tests are opt-in, can incur charges, and must not run in pull-request CI.
