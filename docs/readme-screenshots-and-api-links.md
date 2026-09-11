# README 截图与 API 获取入口（2026-09-11）

## 实际截图

本轮删除 docs/assets/pdf-reader-demo.png 及中英文首页的演示截图介绍，改用正在运行的 Windows Read Desktop 窗口。未合成对话或生成成果，未把浏览器组件预览当成桌面截图。

截图使用用户已经导入的 *Linking neural manifolds to circuit structure in recurrent networks* 以及该文献已有的 OCR、讨论、地图、旁批和精读路线。仅切换视图、缩放和选择已有区块，没有发起 OCR、模型生成或修改文献正文。

- library-workspace.png：真实文库。
- reader-discussion.png：原 PDF、已有页边旁批和侧边对话。
- ocr-block-selection.png：选中 Highlights OCR 区块后的引用／翻译／解释工具条。
- argument-map.png：已有地图的全幅画布，保留实际旧版／检查状态。
- reading-roadmap.png：已有精读任务、阅读深度、自检问题和原文链接。

截图直接捕获应用窗口，不包含其他应用窗口。历史内容保留原语言；英文界面下中文成果仍可显示。源 PDF、工作区数据库和临时捕获文件不纳入提交。

## API 获取入口

Settings → AI models & providers 的 API Key 输入框旁显示相关官方入口；Mistral OCR 卡片提供独立入口。官方资源目录为 src/providerKeyResources.json，前后端共用。

- Gemini → Google AI Studio
- OpenAI → OpenAI API keys
- Grok → xAI API keys
- Mistral → Mistral Console
- 已识别的 DeepSeek 兼容端点 → DeepSeek API keys

自定义地址不根据字符串包含关系猜测提供方，只匹配完整 hostname；无法识别时提示前往实际服务商后台。Gemini Proxy 明确提示使用代理管理页的密钥。

桌面命令只接受固定 resource ID，通过系统默认浏览器打开，禁止任意 URL／路径。浏览器预览使用普通新窗口链接。点击链接不运行连接探针，也不读取或传递密钥。

官方页面依据 Google、Mistral、xAI、DeepSeek 文档和 OpenAI 官方 quickstart 中的控制台链接核对：
[OpenAI quickstart](https://developers.openai.com/api/docs/quickstart)、
[Gemini API key](https://ai.google.dev/gemini-api/docs/api-key)、
[xAI quickstart](https://docs.x.ai/developers/quickstart)、
[Mistral docs](https://docs.mistral.ai/)、
[DeepSeek docs](https://api-docs.deepseek.com/)。

## 验证

- API 入口、SettingsWorkbench 和中英文覆盖：3 个文件，14 项测试通过。
- Rust 固定官方地址边界测试：1 项通过；不通过测试打开浏览器。
- TypeScript 与生产构建通过；保留已有 chunk 提示。
- 桌面实测点击 Mistral 链接：打开系统默认浏览器中的官方 AI Studio - Mistral AI 页面，Read Desktop 保持自身窗口。
- 仓库链接与改动空白检查通过。

本轮截图证明已有内容的显示和交互，不代表重新验证模型生成质量或重新发布安装包。
