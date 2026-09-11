# Read Atlas

[English README](README.md) · [使用指南](docs/user-guide.md) · [开发指南](docs/development.md) · [隐私与费用](PRIVACY.md)

面向论文与教材 PDF 的本地优先阅读和研究工作台。整理文献、在原版 PDF 上阅读，并按需使用 AI 解释公式、分析图表、生成阅读路线和页边旁批。

**当前阶段：Windows Alpha。** 其他平台尚未验证。AI 和 OCR 功能需要用户自行配置服务商及 API Key，费用由对应服务商收取。查看 [已知限制和发布验收](docs/release-checklist.md)。

## 基础功能与实际界面

以下截图来自正在运行的 Windows 桌面应用，使用已导入的真实论文及已有阅读成果，拍摄于 Read Desktop 更名为 Read Atlas 之前。界面切换为英文后，历史成果仍保留原来的语言。

### 选中 OCR 区块，边读边讨论

在原 PDF 中选中段落、公式、图片或表格，即可使用对应的引用、翻译、解释和 Lens 分析操作。右侧保留论文讨论，通过引用返回原文。

![真实论文的 OCR 选区、操作工具条与侧边讨论](docs/assets/ocr-block-selection.png)

### 在原文旁阅读批注

页边旁批锚定原文区块，展示不同角色的阅读观察；右侧讨论区仍可用于更深入的问题。

![真实 PDF、已有页边旁批与论文讨论](docs/assets/reader-discussion.png)

### 查看论证／知识地图

以节点和连线查看论文的概念、论据与结论关系。支持全幅画布、平移缩放、节点／关系详情与原文定位。截图保留这份已有地图的实际检查状态。

![真实论文的全幅论证地图](docs/assets/argument-map.png)

### 按精读路线逐步理解

无需离开论文即可打开精读路线，查看分阶段任务、阅读深度、自检问题和原文定位，逐项标记完成进度。

![已有精读路线、阅读任务与自检提示](docs/assets/reading-roadmap.png)

### 管理本地文库

按文件夹、标签和阅读状态整理论文及教材章节，在卡片和表格间切换，筛选文献或多选执行批量操作。

![包含已导入论文和教材的真实文库](docs/assets/library-workspace.png)

此外支持 Brief、术语表、符号表、成果版本、可恢复任务、用量记录、主题、自定义提示词和读者背景。

截图中的论文为 Louis Pezon、Valentin Schmutz 和 Wulfram Gerstner 的 *Linking neural manifolds to circuit structure in recurrent networks*（[DOI](https://doi.org/10.1016/j.neuron.2025.12.047)），文献与生成内容均保留实际状态。

## 开始使用

从 [Releases](https://github.com/freesky3/read-atlas/releases) 下载 Windows x64 安装包。首发为 **0.1.1 Alpha**，使用按用户安装的 NSIS 包。目前未签名，Windows 可能显示未知发布者或 SmartScreen 提示。安装前可核对附带的 SHA-256；Alpha 版本请先用工作区副本试用。

MSI 构建在完成管理员安装环境的生命周期验证前，不加入公开下载。

1. 打开应用，在设置中选择一个专用工作区。
2. 导入 PDF；论文存入 `Papers/`，教材存入 `Textbooks/`。
3. 如需 AI，在设置中配置 Gemini、Gemini Proxy、OpenAI-compatible 或 Grok，并完成相应能力检查；OCR 单独配置 Mistral。
4. 打开文档阅读。OCR、生成成果和讨论会在对应操作后调用服务商。

**生成旁批会消耗较长时间和大量 token**，开始前会显示确认提示。支持的服务商配置不代表任意模型都具备原生 PDF 能力，详见 [配置与使用](docs/user-guide.md)。

### 获取 API Key

模型与 OCR 设置的密钥输入框旁提供官方获取入口。前往服务商页面创建密钥，再返回应用填写并保存。

| 服务 | 官方获取页面 |
| --- | --- |
| Mistral OCR | [Mistral Console](https://console.mistral.ai) |
| Gemini | [Google AI Studio](https://aistudio.google.com/api-keys) |
| OpenAI | [OpenAI API keys](https://platform.openai.com/api-keys) |
| Grok | [xAI API keys](https://console.x.ai/team/default/api-keys) |
| DeepSeek 端点 | [DeepSeek API keys](https://platform.deepseek.com/api_keys) |

自定义兼容接口应使用对应服务商的密钥，本机代理使用代理管理页配置的密钥；不同提供方的 API Key 不能通用。链接不代表所选模型已经支持原生 PDF。

## 从源码运行（Windows）

安装 Node.js **24.17.0**、Rust **1.96.1**（MSVC）、Visual Studio C++ Build Tools、Windows SDK 和 Microsoft Edge WebView2 Runtime。完整步骤见 [开发指南](docs/development.md)。

在本仓库根目录执行：

```powershell
npm ci
npm run tauri -- dev
```

浏览器中的 `npm run dev` 是 UI 演示，使用内存数据；真实文件、OCR、凭据和任务恢复请在桌面应用中验证。

检查与打包：

```powershell
npm run check:repo
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --locked
npm run notices
npm run tauri -- build --bundles nsis,msi
```

## 数据在哪里

PDF、讨论、生成成果和阅读状态保存在所选工作区，API Key 保存在 Windows Credential Manager。AI 功能会把所需材料发送给你配置的服务商，包括部分功能所需的完整 PDF；本地存储不代表模型推理在本机完成。

详见 [隐私与费用](PRIVACY.md) 和 [备份与恢复](docs/user-guide.md#备份与恢复)。更换版本或处理旧工作区前，先退出应用并备份整个工作区。

## 项目与贡献

技术栈：Tauri 2、React / TypeScript、Rust、SQLite 和 PDF.js。

- [文档索引](docs/README.md)：产品合同、架构决策与实现说明。
- [贡献指南](CONTRIBUTING.md)：开发、测试和提交变更。
- [路线图](ROADMAP.md)：当前重点和后续方向。
- [安全问题报告](SECURITY.md)：敏感漏洞请使用私下报告渠道。
- [变更记录](CHANGELOG.md)：开源准备与版本记录。

## 许可证

项目原创代码和文档采用 [MIT License](LICENSE)。依赖及其他第三方内容保留各自权利和许可证，见 [第三方声明](THIRD_PARTY_NOTICES.md)。MIT 授权不代表取得第三方角色、商标或素材的使用授权。
