# 开源整理前的开发记录（2026-09-11）

> 历史快照：保留早期实现说明和交接记录。当前功能、安装方式和验收状态请以仓库 README、开发指南与发布检查表为准。

# Read Desktop

中英界面和新生成内容共享应用语言开关；历史成果与自定义内容保留原文，已提交任务冻结语言。实现边界与验收见 [中英界面与生成语言合同](bilingual-ui.md)。

Lens 追问已采用论文／教材两份完整中文稿，针对理解卡点调整讲法并保留有依据的多轮纠正，继续使用两字段输出，详见 [Lens 追问合同](lens-qa-generation.md)。

论文 Lens 六份提示词已采用完整中文稿：公式以直观理解为主，图表提供读法和重点位置；v2 支持材料局限及旧成果兼容，详见 [Lens 生成合同](lens-generation.md)。

学术讨论与对话已采用论文／教材四份完整中文稿，覆盖直接回应、独立判断、多轮纠正及保留认识状态的讨论压缩；恢复时完整传递三个压缩字段，详见 [学术讨论与压缩合同](discussion-generation.md)。

精读路线已采用论文／教材两份完整中文导师式提示词，按理解依赖安排三遍阅读、深度与回看位置，从文档根独立生成；详见 [精读路线生成合同](reading-roadmap-generation.md)。

解释已采用论文／教材两份完整中文定稿，明确概念、逻辑、条件与解释性补充的边界，沿用五字段协议；详见 [解释生成合同](explanation-generation.md)。

翻译已采用八部分完整中文定稿，新增四种完成状态并保留旧稿与旧成果兼容；详见 [翻译生成合同](translation-generation.md)。

文档根、术语表、符号表和元数据已采用讨论确认的完整中文提示词，保留独立生成、原始版本与人工设置；详情见 [辅助成果生成与兼容说明](auxiliary-generation.md)。


Brief 已采用九字段中文定稿，并与术语表、符号表、元数据分开生成。四类成果共享只含 PDF 的来源根，术语表可选参考当前 Brief 的主题线索；详见 [生成合同](brief-generation.md) 与 [提示词手册](prompts.md)。

Read Desktop 是面向学术 PDF 的本地优先桌面阅读与研究工作台。应用使用 Tauri 2、React 19 / TypeScript、Rust 和 SQLite；PDF 保存在用户选择的 Workspace，凭据保存在 Windows Credential Manager。

当前代码已进入 **V2 沉浸式双阶段基线（Library Hub 文献大厅 + Focus Reader 超聚焦工作台）**，并完成了**克制现代 Liquid Glass 拟态设计语言与 Claude Warm Editorial 暖色纸张系统**全面升级。**Full Outline 论证地图已落地**（协议 v3，见 [full-outline-v1.md](full-outline-v1.md)）。**Reading Guide 旁批已落地**（D-040，见 [reading-guide-v1.md](reading-guide-v1.md)）。后端已完成 **P0-2 Provider 实例级安全路由与任务恢复**，并在 **D-063 升到 schema 8**：Provider route / exact key 冻结、legacy jobs 与 tombstones 隔离并人工确认、Mistral OCR 走独立 endpoint scope、`WorkspaceRuntime` 继续单一隔离、cleanup 按 32 条批次重登记并重置 attempts、任务中心提供 recovery 引导、memory adapter 不伪造 probe；细节见 [backend-hardening.md](backend-hardening.md)、[data-protocols.md](data-protocols.md) 和 [agent-onboarding.md](agent-onboarding.md) **§43**。文库 Hub / Batch 见 [library-workspace.md](library-workspace.md)。**设置里并存 Gemini、Gemini Proxy (Antigravity-Manager)、OpenAI-compatible、Grok**（支持 Gemini 3.x Flash/Pro 思考强度分级选择与 PDF 原生内联），见 [agent-onboarding.md](agent-onboarding.md) **§29** 与 **§43**。详细产品合同见 [V2 产品规格](v2-product-spec-2026-08.md)。下一个 agent 请从 [docs/agent-onboarding.md](agent-onboarding.md) 开工；动 Rust 先读 hardening 页，Outline 先读 **§28**，论文 provider 先读 **§29**、**§43** 与 **§44**，旁批先读 **§30**，Lens 多版本先读 **§34**，Claude 暖色先读 **§35**，多布局零重置先读 **§36**，图片 Lightbox 先读 **§37**，微交互先读 **§38**，Hub 排序先读 [docs/hub-sort.md](hub-sort.md)，文库 Hub / Batch / `library_read` 先读 [docs/library-workspace.md](library-workspace.md) 与 onboarding **§47**，`tauri dev` 编译 OOM 先读 [docs/windows-dev-build.md](windows-dev-build.md)。

教材提示词已完成22槽适配：教材Brief使用独立九字段，教材Lens接入v2，论文稿与旧教材成果兼容保留。详见 [教材生成合同](textbook-generation.md) 与 [完整教材定稿](note/textbook-prompts.md)。

## 核心架构与界面交互

- **双阶段流转体系（Dual-Stage Architecture）**：
  - **文献大厅（Library Hub）**：冷启动第一入口。左侧按 `Papers/` 物理目录层级展示可折叠/展开的文件夹树（Folder Tree Hierarchy，展开状态按 Workspace 记忆）；右侧支持 `田 卡片网格`（完整展示一句话学术贡献 TL;DR）与 `≡ 结构表格` 即时切换，卡片列表走 `hub_page` 分页虚拟窗口。选择升级为 Selection Set（复选框、`Ctrl/⌘` 切换、`Shift` 连选、`Space` 切换焦点项、`Ctrl+A` 选中当前筛选结果、`Esc` 清空）；拖拽改由悬停/聚焦才出现的 `⋮⋮` 把手启动，合法落点画高对比插入线、非法落点给出 `aria-live` 原因，`Alt+↑/↓` 与「移到最前/最后」和鼠标拖拽共享同一份完整精确置换，`M` 打开「移动到……」预览跨根改类后果。本地批量（标签 patch、移动、回收站、导出、阅读状态、多文件导入与 Explorer 拖入）与 Provider 批量 OCR / Brief（费用预览、Prepared Job Handle）走持久 `library_act` 批次；智能集合是版本化查询而不是文件夹（合同见 [docs/library-workspace-plan-2026-08.md](library-workspace-plan-2026-08.md) §11.0）。
  - **超聚焦阅读工作台（Focus Reader Workspace）**：点击论文即刻进入。隐藏持久化文库抽屉，将 100% 窗口宽度分配给 **PDF 画布（60%）+ AI 研讨（40%）**。支持 `‹ 返回文库` 或键盘 `Esc` 秒级返回大厅，`Ctrl+\` / `Ctrl+B` 切换 `⤢ 仅看 PDF` 全屏阅读。
- **38px 单行极简多功能顶栏与 Thread 双击重命名**：
  - 将原先堆叠的 3 层头部（`~144px`）精简为单行 **38px**，节省 `~106px` 垂直空间。
  - 单击展开分支切换下拉气泡，**双击标题文本直接内联编辑重命名**，回车或失焦自动持久化至 SQLite 数据库。
- **38px 单行紧凑输入胶囊与流式即时中断**：
  - 彻底淘汰占用过多阅读空间的多行输入框，改为 38px 悬浮液态玻璃单行胶囊，回车即发。
  - AI 生成过程中，发送按钮无缝切换为红色 `■` 停止按钮，点击即刻调用 `cancel_discussion` 终止模型输出并保留当前已输出文字。
  - 12px 极微辅助信息行实时指示模型就绪状态与引用选区数量。
- **全局状态栏上移顶岛 & 彻底清除 28px 底部状态栏**：
  - 状态指示与 Token 统计全量并入顶部 44px 悬浮岛，直接释放 98px+ 垂直阅读空间。
- **纯净多模态用户问题气泡（公式/图片置顶，跳转药丸置底）**：
  - **公式**：直接通过 KaTeX 渲染标准数学公式，置于提问正文上方（不附加任何冗余文字标签）。
  - **图片**：直接在提问上方呈现微缩图卡片，**点击缩略图即可弹出全屏/大图 Lightbox 放大查看**。
  - **Block 跳转药丸**：所有引用的 Block 药丸统一收敛在问题下方（`p.X · [blockType] #N`，**10.5px** 微型字号），点击秒级跳转 PDF 原文对应页面并高亮选区。
  - **编辑按钮 `✎`**：悬停右上角轻量显现，不占用气泡底部整行空白。
- **Hub 文献大厅一句话学术结论（TL;DR）数据流打通**：
  - 后端 `DocumentCard` 自动从 SQLite `artifacts` 表中提取 Brief 核心结论（`summary`/`findings`），保证 Hub 卡片和表格视图展示真实学术摘要，不再显示 Fallback 占位词。
- **聊天记录跳转刻度轴（Chat Timeline Mini-Map Navigator）**：
  - 消息流右侧嵌入轻量 18px 弹性刻度轴，每轮 Q&A 映射为一个刻度。
  - 悬停浮现高保真液态玻璃预览卡片（**轮次时间、粗体问题标题、回答前 2 行摘要**）。
  - 点击秒级平滑定位目标消息并触发 `.target-highlight` 光晕脉冲动画；滚动时动态追踪活跃刻度。
- **克制现代 Liquid Glass 拟态设计语言**：
  - 彻底移除了高噪点 `.grain` 纹理与复古小说衬线体。
  - 采用多层厚毛玻璃折射（`backdrop-filter: blur(24px) saturate(180%)`）、4 层内阴影立体折射（`--liquid-glass-shadow`）与 45° 对角高光光斑（`--liquid-glass-sheen`）。
  - 内置三套主题引擎：冷色调学术浅色（`liquid-light`）、深邃学术暗色（`liquid-dark`）与经典暖色（`warm-editorial`），并通过 `localStorage` 保持持久化同步。
- **对话排版哲学（Discussion Bubble Topology）**：
  - **用户问题**：采用磨砂玻璃圆角气泡框（`18px 18px 4px 18px`），**靠右侧排布**，头像与时间戳右对齐。
  - **AI 深度回答**：采用**全宽无框流式排版（Frameless Editorial）**，去除气泡框限制，居中强化数学公式卡片，最大化阅读舒适度。
- **学术级 OCR 选区与持久化标识**：
  - OCR 选区高亮采用柔和学术薄荷绿底色与 **4px 圆角实线边框**（普通态浅绿，引用/聚焦态高亮翡翠绿）。
  - **悬停只预览，单击才选中并钉住**右上角引用/翻译/解释/Lens；工具条不要只靠 `:hover`（会在移向按钮时消失）。
  - 当本地已生成对应持久化资产（Lens、翻译等）时，按钮旁标注绿色勾号徽标（`✓`）。
- **讨论与阅读成果**：
  - 聊天只显示当前分支；切路径、删一轮问答都在对话树（一轮 = 一问一答）。
  - 公式行内 `$...$`、行间 `$$...$$`。选区 `Ctrl+C` 复制 GFM（列表带 `1. `，公式带 `$`/`$$`）；单击公式复制对应 LaTeX。符号表标题和正文里的裸 `z_i` 也要渲染。
  - 有序列表必须各占一行；`**粗体**` 内侧不能有空格。历史成果靠 `prepareMarkdown` 显示时修复，不要写回工作区 SQLite。合同 [docs/markdown-rendering.md](markdown-rendering.md)。
  - `A−`/`A+`（14/15/17）同时改讨论和阅读成果，不改 PDF zoom。变量必须设在 `.workspace-grid`。
  - 助手气泡上的模型名读该条 `usage.model`，不要写死 Gemini 2.5 Flash。
- **视口锁定与双画布独立滚动**：
  - 根级视口严格锁定为 `100vh`，整体应用窗口绝不发生上下晃动或溢出。
  - PDF 画布与 Chat 消息流各自拥有独立的内部滚动容器。
  - 中间分栏分割线宽 8px，带居中可见磨砂抓手药丸，鼠标悬停即刻点亮科技蓝。
- **Full Outline 论证／知识地图（顶岛「地图」）**：
  - 不是目录、不是 Brief、不是第三永久标签。预设 `PDF | 地图`。必须已 OCR。
  - 新路径（`outline-map-v4`，D-069）：总图为整体构图 → 独立检查与定稿；局部图一次展开。自由角色与关系，不强制连通／无环。旧 v3 任务与成果仍可读。
  - 画布：lazy `@xyflow/react`；节点与连线都可打开详情并跳原文；节点可拖（不存坐标）。
  - Settings → Prompts 三槽显示为整体构图／检查与定稿／局部展开。自定义旧稿保留抽取流程。不要靠递增 `OUTLINE_EPOCH` 清空用户地图。
  - 合同：[docs/outline-generation.md](outline-generation.md)；v3 历史 [docs/full-outline-v1.md](full-outline-v1.md)；踩坑 [docs/agent-onboarding.md](agent-onboarding.md) §28。
- **Reading Guide（AI 好友旁批，顶岛「旁批」，D-040）**：
  - 预计算页边层，不是第三工作台，不复用精读路线。三人阿林 / 老周 / 小夏，锚在 OCR Block 整框；层开时每页右侧预留 240px 槽。
  - Job `reading_guide`：先压缩阅读上下文（可带 PDF），再 6/8 页分批；**分批禁止再传整本 PDF**（`interact_text`）。顶岛显示条数；生成完成按 head id 只跳一次第一条。
  - PDF 虚拟列表：`currentPage ± 2` 绝对定位槽，用画布实测高度累计 `top`。不要 spacer、不要 `scrollIntoView` 追页、不要把滚动 offset 回传再吸回当前页。
  - 踩坑与文件入口：[docs/agent-onboarding.md](agent-onboarding.md) §6 / §30，合同 [docs/reading-guide-v1.md](reading-guide-v1.md)。
- **精读路线（Reading Roadmap，顶岛「精读」）**：
  - 像导师一样指导读者先读哪里、怎样读、读到什么程度，以及何时返回处理细节。论文以三遍逐步深入，教材按概念、推导、例题与独立应用适配；前置准备按需要加入。
  - 完整 PDF 来源根创建独立旁支；Brief 与读者背景只作本次辅助输入。无 Brief 也能生成，路线成果不回写来源根。
  - 原文定位支持页码跳转，历史成果保留块定位。当前请求没有 OCR 目录，模型不得编造块 ID。
  - 任务勾选独立持久化；可选“复述自检提示”与“优先精读对象”，对象可为定理、算法、图表或段落。提示词仍通过 `reading_roadmap` 槽编辑。

- **阅读成果与 Lens 交互联动、证据高亮及统一输入框体验**：
  - **Lens 智能直达**：点击选区工具条的 Formula/Figure/Table Lens，自动切换阅读成果标签并立即展示对应 Lens 成果；
  - **证据跳转高亮**：成果底栏证据精简为轻量液态玻璃药丸（`📄 p.N ↗`），点击同时平滑翻页并高亮聚焦目标 Block；
  - **全能输入体验**：讨论区与 Lens QA 输入框统一升级为弹性胶囊（`.composer-compact-capsule`），全面支持 **Enter 发送、Shift+Enter 换行**。
- **Lens 成果选区自动归并与 `< 版本 1/2 >` 多版本管理系统（D-041）**：
  - **选区空间自动归并（1 选区 1 卡片）**：无论历史生成过多少次或跨 OCR 版本，同种类、同页码且空间重叠率 `IoU > 0.4` 的 Lens 自动合并为一个成果组，顶部 INDEX 成果栏仅展示 1 个卡片（带 `2 版本` 等微型角标），彻底消除重复卡片散落问题；
  - **详情页毛玻璃版本切换器（`‹ 版本 1/2 ›`）**：顶栏提供精致的 Liquid Glass 版本切换胶囊，支持前后翻页无缝对比不同次、不同模型生成的分析内容与独立 Lens QA 提问记录；
  - **单版本安全清理（`🗑️`）**：多版本时支持删除不满意的生成版本，配合 `DeleteVersionConfirmModal` 二次确认弹窗防误删，删除后安全转移 Head 并平滑回退至相邻版本；
  - **OCR 块空间动态定位**：重新生成时基于坐标 IoU 匹配动态解析当前 OCR 树中的有效 Block ID，杜绝跨版本 OCR ID 漂移报错；PDF 中点击已有成果的选区直接跳转聚焦，杜绝重复派发任务。
- **Claude 暖色纸张主题全面色温校准与去刺眼白（D-042）**：
  - 彻底清除暖色主题下的纯白（`#ffffff`）硬色块，建立温润象牙暖米纸（`#fbf9f5` / `#fdfcf9`）、亚麻抽屉（`#f5efe6`）、淡褐绘图底（`#f7f2ea` / `#ece5db`）、书本纸缝线（`#e8e0d4`）与赤陶红（`#c15f3e`）色彩体系，杜绝反光眩光。
- **多布局切换常驻渲染与 PDF 阅读器零重置（Zero-Unmount & View State Preservation, D-043）**：
  - 在全屏地图（`outline_only`）或隐藏 PDF 视图下，改用 CSS `display: none / flex` 进行常驻挂载；
  - 配合 `programmaticScroll` 保护锁，彻底解决 DOM 卸载与 `IntersectionObserver` 竞态导致的页面刷新回第 1 页问题，切换视图 0 延迟且 100% 保持页码、滚动像素与缩放比例。
- **多模态图片引用缩略图提取与全屏 Lightbox 交互（D-044）**：
  - 引用图表发起提问时自动异步截取 Canvas 高清缩略图卡片并置顶于提问气泡；点击缩略图即可弹出全屏毛玻璃 Lightbox 放大查阅并支持一键定位 PDF。
- **界面微交互与细节体验升级（D-045）**：
  - 大纲详情“收起”按钮升级为醒目的 `◂ 收起` 赤陶色高对比度药丸与悬浮光晕；
  - 相关 Block 证据标签增加 1px 优雅纸缝边框与赤陶色悬浮微上浮动效；
  - 对话分支下拉菜单（`Main discussion ▾`）支持页面任意空白处点击（Click Outside）与 Escape 键平滑收起。
- **阅读成果空间极致收敛与导航闭环重构（D-046）**：
  - **三态分段与第 1、2 层合并**：顶栏统一直出 `[ 💬 讨论 ] [ 🌐 全篇成果 (N) ] [ 🔍 Lens (M) ]` + `[ A- 15 A+ ]`；点击 PDF 选区自动切至 `[ 🔍 Lens ]`；
  - **消灭第 3 行冗余 Header**：将模型生成元信息（`由 XX 生成 · 23:39`）与 `[ 🔄 重新生成 ]` 上移合并至顶栏第 1 行右侧，成果正文直接顶格渲染，**净省 106px+ 垂直阅读高度**；
  - **论证地图双向导航**：地图顶栏提供 `[ ‹ 返回成果 ]` 直达通道，大纲 Chip 仅在全篇成果下展示；
  - **顶栏 OCR 按钮防折行**：固定单行排版并将顶栏悬浮胶囊收敛至 **38px**，更加轻盈紧凑。
- **顶栏模型胶囊常驻、讨论输入区纵向净空扩展与 Roadmap 边界收敛（D-047）**：
  - **顶栏模型胶囊**：在右侧 `compact-rail-right` 中常驻显示当前论文模型徽章（`.rail-model-badge`），带状态指示与快捷点击前往配置，无论在「讨论」还是「全篇成果/Lens」下均可随时直观查看与切换；
  - **讨论区纵向净空扩展**：彻底移除讨论输入框下方的 `.composer-subline`（「就绪」及冗余状态行），输入框紧凑吸底，讨论消息流垂直可视区域净增 30px+；
  - **Roadmap 边界收敛**：精读路线仅在论文阅读视图下响应边缘悬停与唤出，在设置中心、任务中心及全屏讨论树等弹窗下严格静默，避免界面冲突。
- **侧边栏顶栏响应式弹性收纳、图标化与生成元信息沉底（D-049）**：
  - **元信息下移沉底**：将成果生成模型与生成时间彻底从顶栏移除，统一沉底至内容最下方（`.artifact-detail-meta-footer`），释放顶栏空间且杜绝与当前系统时间混淆；
  - **Tab 组图标化与自适应纯图标模式**：「讨论」新增 `💬` 聊天图标；宽模式下平铺展示 `💬 讨论`、`🌐 全篇成果 (N)`、`🔍 Lens (M)`；窄模式（`chatWidth < 460px`）自适应展示纯图标与角标 `💬`、`🌐 N`、`🔍 M`，极限节省顶栏空间；
  - **自适应折叠菜单与防裁剪**：字号调节器与全屏讨论树自动收纳进 `···` 更多操作菜单；修复顶栏容器 `overflow: visible` 确保下拉菜单完美弹出；
  - **Flexbox 容器防重叠**：强化 `min-width: 0` 与省略号截断规则，彻底解决侧边栏窄屏下的元素穿透重叠与折行。


- **原生窗口顶栏沉浸式变色与主题动态同步**：
  - 接入 Windows DWM（Desktop Window Manager）原生 API，在切换到 `liquid-dark` 暗夜主题时，系统标题栏自动下发深色沉浸模式（`DWMWA_USE_IMMERSIVE_DARK_MODE`），顶栏背景与应用底色同步为 `#090d16`，最小化/最大化/关闭按钮自适应白字；在 `liquid-light` 下同步为 `#e2e6eb`，在 `warm-editorial` 下同步为 `#eee9df`，彻底告别深色界面下的白色刺眼顶栏。
- **任务中心（Operations Center）最新任务置顶排列**：
  - 任务中心（Activity Ledger）在展示持久化 Job 时，按状态优先级（`running -> queued -> paused -> others`）与创建时间 `createdAt` 降序排列，最新发起或恢复的 OCR、论证地图与成果任务始终置于最顶端，无需向下滚动查找。
- **Windows 8 MiB 栈空间安全保留与 AppState 堆间接持有**：
  - 在 Windows 构建参数中预留 8 MiB 主线程栈空间，杜绝 Tauri 2 在处理 80 多个 IPC 命令递归匹配宏展开时发生的 `STATUS_STACK_OVERFLOW / 0xc00000fd` 崩溃；配合 `app_state_fits_comfortably_on_a_small_stack_frame` 自动化测试确保核心状态栈占用在 4 KiB 以内。
- **全屏设置桌面工作台（Full-Page Settings Workbench）与多 Provider 实例架构（D-048）**：
  - **全屏桌面工作台**：告别拥挤的居中小弹窗，采用 100% 视口高度全屏桌面工作台（`SettingsWorkbench.tsx`），支持键盘 `Esc` 或 `← 返回阅读` 秒级返回；
  - **动态多 Provider 实例管理**：支持动态添加最多 10 个独立模型实例（Gemini、OpenAI 兼容端点如 DeepSeek/Ollama/OpenRouter、xAI Grok），支持自定义命名、重命名、复制、拖拽排序与独立安全凭据管理；保存/新增/复制模型实例时**自动设为当前活跃模型**；
  - **中西文排版字体视觉卡片与 KaTeX 双向公式预览**：精选 5 种西文字体与 4 种中文字体，采用独立视觉卡片展示真实排版效果，配合 KaTeX 实时数学公式渲染预览；
  - **提示词全视口锁定（Viewport-Locked Master-Detail Layout）**：22 个提示词槽位 Master-Detail 编辑器，全屏视口锁定零外层滚动晃动，多行 Monospace 文本自适应换行，永不截断。
- **LaTeX 希腊字母加粗与控制字符容错流水线（KaTeX Math Resilience）**：
  - 自动拦截并修复模型输出中包裹希腊字母的文本模式加粗（如 `\textbf{\kappa}` 自动转为数学粗体 `\boldsymbol{\kappa}`，剥离 `\text{\kappa}`），彻底杜绝 KaTeX 红字语法报错；
  - 机械修复 JSON 反序列化反斜杠转义产生的控制字符损坏（`\x08`、`\x0c`、`\t`、`\r`），确保学术公式高保真、优雅呈现。
- **Library Hub 核心结论（Takeaway）零延迟液态玻璃悬浮气泡**：
  - 移除 Brief 生成提示词中的字数限制，保留完整学术贡献；
  - 替换系统延迟 `title` 提示，搭载即时响应的液态玻璃悬浮气泡（`.liquid-summary-popover`），悬停即可秒级查阅完整学术发现。
- **任务与调度中心（Activity Ledger / Operations Drawer）液态玻璃全量重构**：
  - 采用现代悬浮分段胶囊控制条（`⚡ 任务队列`、`💾 存储占用`、`🗑️ 回收站`、`🩺 诊断审计`）；
  - 全新 `.job-card` 架构：多模型专属渐变芯片徽章（Gemini/Mistral/OpenAI/Grok/Local）、状态呼吸灯胶囊、所属论文标签、中文业务副标题、安全扣费指示以及优雅半透明错误警示卡片。
- **OCR 级联删除确认与安全保护（D-049）**：
  - Reader 顶栏 `✓ OCR 就绪` 旁提供 `🗑️` 删除 OCR 按钮；
  - 弹出 `CascadeOcrDeleteConfirmModal` 双栏弹窗，清晰确认将要清理的 OCR 依赖（翻译、解释、Lens、OCR 版本）与完整保留的核心资产（Brief、Metadata、术语表、符号表、聊天记录），通过 `delete_ocr_cascade` 命令安全原子清理。
- **Brief 与 Hub 标签双向同步及 Metadata/术语表/符号表深度就地编辑与 📌 图钉锁定（D-050）**：
  - **学术标签双向同步**：Brief 标签移至 Takeaway 下方，支持在 Brief 和 Library Hub 上实时删除与添加标签（`update_paper_tags`）；
  - **整行就地 ✎ 编辑与自动/手动 📌 图钉锁定**：Metadata 与术语表（Glossary）、数学符号表（Symbol Table）支持就地编辑与锁定，支持整行删除与末尾新增词条；
  - **大模型 Context 真实生效与重生成防覆盖**：修改通过 `update_paper_metadata` 与 `update_orientation_table` 直接写入 SQLite `artifacts.content_json`，需要这些辅助信息的任务读取锁定内容；独立重生成术语表、符号表或元数据时合并已有 `_pinned`，共享 PDF 根不携带这些成果。
- **讨论渐进式加载（Progressive Loading）与后台上下文压缩脉冲指示器（D-051）**：
  - 长对话初始仅渲染最近 15 轮（30 条节点），向上滚动自动分页扩充，时间轴刻度跳转智能扩窗；
  - 后台触发上下文压缩时，输入框上方浮现优雅的呼吸脉冲提示徽章（`.discussion-compaction-pulse-indicator`）。
- **全篇成果与 Lens 顶栏即时视域切换与状态记忆（Memory Retention, D-052）**：
  - 点击顶栏 `🌐 全篇成果` 或 `🔍 Lens` 即时刷新主视图，无需在下方重复点击小胶囊；
  - 自动记忆上一次浏览的具体成果（Brief、术语表、符号表、Metadata）与 Lens 选区分析，来回切换无缝恢复。
- **Lens 追问建议与表格各列 LaTeX 公式渲染**：
  - Lens QA 追问建议芯片与术语表同义词/符号表作用域完整接入 KaTeX 渲染；点击建议芯片填入输入框供用户二次编辑。

## 开发与验证

```powershell
npm install
npx vitest run --maxWorkers=1 --fileParallelism=false
npx tsc -b
npm run build
npm run tauri dev
```

Rust 后端验证：

```powershell
cd src-tauri
cargo test --locked
cargo check --locked
cargo clippy --locked --all-targets -- -D warnings
```

> **注意**：
> 1. Vitest 必须串行执行（`--maxWorkers=1 --fileParallelism=false`），以避免 jsdom 并发时发生内存溢出（OOM）。
> 2. 浏览器 `npm run dev` 使用内存 adapter，仅用于 UI 和测试，不伪装真实桌面文件、OCR、provider 或 WebView2 捏合。memory adapter **不会**把 OpenAI/Grok 探针标成通过。改了 `src-tauri/src/webview_pinch.rs` 或 Tauri 命令后必须重启 `tauri dev`。

## Workspace 目录规范

```text
<Workspace>/
├─ Papers/
│  └─ <nested collections>/paper.pdf
├─ Textbooks/
│  └─ <nested collections>/chapter.pdf
├─ export/                 # 懒创建，不入库
└─ .read-desktop/
   ├─ read-desktop.sqlite3
   ├─ artifacts/
   ├─ staging/
   ├─ trash/
   └─ workspace.lock
```

检测到旧根目录数据库或 `library/` 时，应用返回不可用的 `reset_required` 状态，不激活该 Workspace，并自动打开 Settings 的 Workspace 分区。重置采用两阶段确认，确认后删除旧数据并初始化 V2 布局。

## 当前验证状态

次数以 [docs/handoff.md](handoff.md)「最近验证」为准，不要把本段当考古数字。

- **前端测试**：必须串行 `npx vitest run --maxWorkers=1 --fileParallelism=false`（或 `--no-file-parallelism`），并行会 OOM。
- **前端类型**：`npx tsc --noEmit` / `npx tsc -b` 应为 0 错误。
- **Rust 后端**：改查询 / Batch 至少跑 `cargo test --locked --lib library_query` 与对应模块；全量 `cargo test --locked` 很重，Windows 先读 [docs/windows-dev-build.md](windows-dev-build.md)。
- **未跑 / 发布门槛**：实机 Windows Explorer 拖入、Hub P95、官方 OpenAI / Grok live 探针、长文旁批实机（需重启 `tauri dev`）。
- 下一个 agent 从 [docs/agent-onboarding.md](agent-onboarding.md) 开工；动 `src-tauri` 先读 [docs/backend-hardening.md](backend-hardening.md)。文库 Hub / Batch 看 [docs/library-workspace.md](library-workspace.md) 与 onboarding **§47**。成果 Markdown 看 **§44** 与 [docs/markdown-rendering.md](markdown-rendering.md)。选区 `Ctrl+C` 看 **§48** 与 markdown-rendering **§9**。Outline 看 **§28**；论文 provider 看 **§29** 与 **§43**；旁批看 **§30**；PDF 滚动看 **§6**。
