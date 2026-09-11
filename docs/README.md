# Read Atlas 文档索引

首发安装／升级／卸载与备份恢复实测：[Windows 生命周期验收](windows-install-validation.md)。

新用户从 [使用指南](user-guide.md) 开始；贡献者阅读 [开发指南](development.md)。公开准备与安装包验收见 [发布检查表](release-checklist.md) 和 [本轮验证记录](release-validation-2026-09-11.md)。原 README 的实现细节保留在 [开发历史](development-history.md)。

下方包含历史设计与实现记录；版本和当前支持范围以仓库首页及各功能最新合同为准。

- [中英界面与生成语言合同](bilingual-ui.md)：语言切换、任务冻结、原文保护、提示词隔离与导出本地化。

Lens 追问的两份完整中文稿、逐轮输入、双字段校验与旧稿兼容见 [追问合同](lens-qa-generation.md) 和 [完整定稿](note/lens-qa-prompt.md)。

论文 Lens 的直观讲解、图表阅读顺序、v2 字段和旧稿兼容见 [Lens 生成合同](lens-generation.md) 与 [六份完整定稿](note/lens-paper-prompt.md)。

本目录记录 V2 的产品、架构、协议、实现状态和发布验收。代码事实与文档冲突时，先修正文档或 ADR，不以旧工作稿覆盖实现。

P0-2 Provider 实例级安全路由已经落地；当前生产使用 schema 8（D-063 PR 1 激活，v7→v8 迁移前先做 pre-schema-8 备份）、精确实例 UUID、冻结 route / exact key 与 endpoint-scoped tombstone，路由合同本身未变。相关恢复入口与实现背景分别见 [data-protocols.md](data-protocols.md)、[backend-hardening.md](backend-hardening.md)、[handoff.md](handoff.md) 和 [agent-onboarding.md](agent-onboarding.md) **§43**。

- [V2 产品规格](v2-product-spec-2026-08.md)：完整产品合同与已锁定默认值。
- [产品范围](product-scope.md)：首发能力、明确不做与发布边界。
- [架构决策](decisions.md)：Workspace、Reader、OCR、上下文、Artifact、Job、隐私等 ADR。
- [上下文管理](context-management.md)：论文根、Discussion 分支、解释/Lens/翻译隔离、缓存与压缩。
- [22 个生产提示词](prompts.md)：Settings → Prompts 每槽职责、论文/教材出厂稿、入队冻结与功能接线。打磨提示词、不看代码讨论改稿时读这个。
- [教材提示词全面适配](textbook-generation.md)：22槽覆盖、教材Brief九字段、Lens v2、无损迁移；[完整教材定稿](note/textbook-prompts.md)。
- [Brief 独立生成与中文定稿](brief-generation.md)：四类成果拆分、PDF 来源缓存、冻结 Brief 线索及旧任务兼容。
- [数据协议](data-protocols.md)：公共投影、SQLite、provider seam、receipt、坐标和事件。
- [功能规格](feature-specs.md)：面向 UI/验收的功能合同。
- [UI 视觉](ui-visual-design.md)：Reader、右侧标签、引用篮、树和任务中心。
- [P0 用户体验修复规格](p0-ux-remediation-plan-2026-08.md)：数据安全、设置主路径、错误恢复、Reader 基础控制、键盘/焦点与响应式的发布门槛。
- [文库 Workspace 实现手册](library-workspace.md)：D-063 **动手入口**。现状一句话、不变量、文件入口、扩展菜谱、**可追加踩坑日志**、**§11 变更日志**。动 Hub / `library_read` / `library_act` / Batch / 虚拟列表必读。
- [文库批量管理、阅读生命周期与拖拽可发现性实施计划](library-workspace-plan-2026-08.md)：D-063 切片与 Gate。PR 0–6 **代码已落地**。**动手前先读 §11.0 落地进度**，实现细节以 [library-workspace.md](library-workspace.md) 为准。
- [Claude 设计风格全解与规范](claude-design-guide.md)：Anthropic Claude Warm Editorial 暖色纸张色彩体系、排版字阶、对话拓扑、组件微交互与反模式避坑全解。
- [实施计划](implementation-plan.md)：M0–M6 完成；M7 Full Outline 代码已落地，待 live Gemini 实机验收。
- [地图 V2 复审与修复报告](outline-map-v2-review.md)：生成恢复、冻结计划、关系交互、无损迁移；544 项 Rust、73 项相关前端测试通过，P8 内容与桌面验收待完成。
- [Full Outline V1](full-outline-v1.md)：v3 历史合同（抽单元／构图、固定枚举）。新生成见下。
- [大纲与脉络图谱生成合同](outline-generation.md)：地图 V2 / `outline-map-v4`（D-069）。六份中文稿 [note/outline-prompts.md](note/outline-prompts.md)。执行计划：[outline-map-v2-plan.md](outline-map-v2-plan.md)。
- [Reading Guide V1](reading-guide-v1.md)：历史三人英文旁批合同（D-040）。V2 见下。
- [Reading Guide V2 生成](reading-guide-generation.md) 与 [批注角色](guide-characters.md)：中文备忘／落笔、五人角色库、快照与迁移（D-068）。执行计划：[reading-guide-characters-plan.md](reading-guide-characters-plan.md)。第二轮复审与验证：[reading-guide-characters-followup.md](reading-guide-characters-followup.md)。
- [Full Outline 实施计划](superpowers/plans/2026-08-17-full-outline-implementation.md)：Job / 目录 / 两层图。
- [提示词目录与删除/重生成](superpowers/plans/2026-08-17-outline-quality-prompts-and-jobs.md)：当初落地 19 槽存储与 Job 冻结的实施计划；**现行出厂稿与接线以 [prompts.md](prompts.md) 为准**。
- [React Flow 画布](superpowers/plans/2026-08-17-outline-react-flow-canvas.md)
- [分叉画布与 epoch 清空](superpowers/plans/2026-08-17-outline-fork-canvas-ux.md)
- [Backlog](backlog.md)：发布前实测与 V2 之后的工作。
- [Handoff](handoff.md)：当前工作树、验证结果、已知环境限制和下一步。
- [Markdown 渲染与归一化](markdown-rendering.md)：讨论/Brief/Lens 的显示层与写入层合同。规则表（粗体、挤在一起的列表、字面 `\n`）可追加；**§9 选区 Ctrl+C → GFM** 含合同、如何加语法、只追加踩坑表。现场样本只读用户工作区。动 `prepareMarkdown`、`normalize_markdown_field` 或复制路径必读。
- [Hub 排序](hub-sort.md)：手动拖拽顺序 + 教材按章节号。合同、文件入口、**可追加的踩坑日志**、如何加一种排序。ADR D-062。动 Hub 下拉/手排必读。
- [Windows 开发构建](windows-dev-build.md)：运行时栈溢出 `0xc00000fd` vs 编译器 LLVM OOM `0xc0000409`。dev profile / `jobs=2` / 清 incremental。`tauri dev` 编不过先读这里。
- [Agent onboarding](agent-onboarding.md)：V2 基线之后的增量、文件入口和踩坑。Outline 从 **§28** 读；论文 provider 从 **§29** 读；Reading Guide 旁批从 **§30** 读；成果 Markdown 从 **§44** 与 [markdown-rendering.md](markdown-rendering.md) 读；选区复制从 **§48** 与 markdown-rendering **§9** 读；Hub 排序从 **§45** 与 [hub-sort.md](hub-sort.md) 读；Windows 构建从 **§46** 读；文库 Workspace 从 **§47** 与 [library-workspace.md](library-workspace.md) 读；Lens 区块卡片流与专属问答从 **§49** 与 [lens-block-cards.md](lens-block-cards.md) 读；读者上下文从 **§50** 与 [reader-context.md](reader-context.md) 读。下一个 agent 从这里开工。
- [Lens 区块卡片流与深度探讨](lens-block-cards.md)：以 PDF 物理区块为中心的卡片流（BlockCardStream）、多模态富预览（公式/图表/表格/正文）、预检卡片防误触、推入式专属问答页（BlockCardQaDetail）、三级图表截屏高可用加载、吸底 3 行自适应无滚动条输入框与踩坑日志。ADR D-064。动 Lens / 选区分析 / ArtifactPanel 必读。
- [读者上下文](reader-context.md)：全部文档 / 文件夹 / PDF 的磁盘 md 笔记（不是系统提示词）。G2 叠层、Job 冻结、sidecar 跟随搬家、居中 80% 分栏编辑器、不变量与可追加踩坑/变更日志。ADR D-065。动 Hub 右键「读者上下文」、F2 注入、`read-desktop.reader.md` 必读。
- [后端加固](backend-hardening.md)：SQLite / WorkspaceRuntime / tombstone / 拆 `lib.rs` 的合同、踩坑和验证。Hub 排序见 **§A.7** 与 [hub-sort.md](hub-sort.md)。文库 schema 8 / `library_read` 见 [library-workspace.md](library-workspace.md)（本页里「schema v7」是历史段落）。动 `src-tauri` 必读。
- [OpenAI-compatible / Grok 实施计划](superpowers/plans/2026-08-18-openai-compatible-and-grok-providers.md)：schema 3、Chat Completions adapter、探针、设置命令与 UI。
- [P0-2 Provider 实例级安全路由实施计划](superpowers/plans/2026-08-23-p0-2-provider-instance-routing.md)：已完成的实施计划，记录精确实例凭据、冻结端点/模型、route-scoped Job/Context、endpoint-scoped tombstone 与任务中心恢复引导。

相邻 `read_addon` 项目作为 Lens 合同、bbox/crop、论证图语义/枚举/校验思路和测试向量的参考来源；Dexie、MV3 runtime 和消息总线不迁入桌面应用。

- [翻译生成合同](translation-generation.md)：完整中文定稿、六字段协议、异常状态和旧稿兼容。
- [解释生成合同](explanation-generation.md)：论文／教材完整中文稿、五字段职责、上下文隔离与旧默认稿迁移。

- [精读路线生成合同](reading-roadmap-generation.md)：导师式三遍阅读、完整中文稿、可选阅读对象、来源根隔离与默认稿升级。

- [学术讨论与压缩合同](discussion-generation.md)：四份完整中文定稿、认识状态、引用边界、三字段恢复与旧默认稿升级。
