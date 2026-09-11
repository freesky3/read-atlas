# 架构决策记录（ADR）

## D-001：Tauri + React/TypeScript + Rust + SQLite

- 状态：accepted
- 决定：React 负责投影 UI；Rust 负责 Workspace、IO、SQLite、Job 和 provider adapter；SQLite 是本地事实来源。
- 后果：前端统一通过 `DesktopClient open/command/watch`，浏览器只用内存 adapter。

## D-002：Workspace 的 PDF 与应用数据分离

- 状态：accepted
- 决定：用户文件位于 `Papers/`，数据库、Artifact、staging、trash 和 lock 位于隐藏 `.read-desktop/`。
- 后果：用户在 Explorer 浏览 PDF 时不会看到应用内部文件；应用数据按稳定 Paper/Revision ID 关联路径。

## D-003：嵌套 Collection 与物理目录双向对齐

- 状态：accepted
- 决定：Collection 任意嵌套，结构与 `Papers/` 同步；任一侧移动/改名触发另一侧对账。metadata 不随文件名/路径改变。
- 后果：watcher 使用 debounce、稳定检测、journal 和冲突对象，不静默覆盖或删除。

## D-004：不可变 Document Revision 与无旧库迁移

- 状态：accepted
- 决定：同路径内容 hash 变化创建新 revision；旧结果保留历史。V2 遇到 V0.1 根数据库/`library/` 要求明确重置，不迁移数据。
- 后果：应用级设置与 Credential Manager 不随 Workspace 重置。

## D-005：PDF.js 是唯一内嵌 Reader，且不做文本重排

- 状态：accepted
- 决定：PDF.js worker 按可见页 ±2 渲染，不创建 Text Layer，不用 iframe/WebView PDF viewer。
- 后果：OCR 前只读原 PDF；PDF.js 失败交给系统 PDF 应用。

## D-006：OCR 用户主动触发，Mistral Blocks 是语义交互入口

- 状态：accepted
- 决定：不使用本地 PDF 文本布局/分析，不自动 OCR。用户点击后调用 Mistral 整篇 OCR；OCR 后不提供自由框选。
- 后果：缺少可定位 Blocks 的响应失败；bbox 统一 0–1000。

## D-007：Block 操作矩阵固定

- 状态：accepted
- 决定：所有 Block 可引用；普通文本可翻译/解释；Formula/Figure/Table 只用 Lens。表格不翻译，公式不普通解释。
- 后果：后续文案可打磨，但不得改变版本、证据和上下文隔离合同。

## D-008：引用篮保存 canonical 不可变快照

- 状态：accepted
- 决定：前端只传 Block ID，后端从当前已发布 OCR 重新加载；最多 32 个，保序去重。发送时把 revision/OCR/page/bbox/text digest 写入 Message context。
- 后果：伪造/stale ID 被拒绝，重 OCR 不改写历史引用。

## D-009：Gemini 论文根与本地消息树分工

- 状态：accepted
- 决定：`document revision + model` 建立 Files + Interactions 论文根；本地 Discussion 树是事实来源，provider ID 只是续接索引。
- 后果：ID 失效从本地 PDF/路径重建；system/schema 每轮重发。

## D-010：分支只移动 active head，不 promote

- 状态：accepted（取代旧 D-009）
- 决定：用户从任意消息创建分支；“设为当前分支”只移动 `discussion_heads`。不把分支内容复制/promote 到所谓主线。
- 后果：相邻问题可以继承上一回答，独立 Discussion 之间不共享历史。

## D-011：Orientation Pack 是同批独立 Artifact

- 状态：accepted
- 决定：Brief、Glossary、Symbol Table、Metadata 一次生成、四个 revision 原子发布。普通成果只读，只有术语/符号字段 override。
- 后果：模型切换复用本地 Pack，新建 epoch，不重新生成成果。

## D-012：翻译、解释、Lens 上下文隔离

- 状态：accepted
- 决定：翻译用便宜独立模型且不发送 PDF；解释从论文根一次性旁支；Lens 从论文根旁支，Lens QA 从 Lens node 续接。
- 后果：解释/翻译不追问，Lens 可追问；这些历史都不污染 Discussion。

## D-013：Artifact/Job 原子发布与不可变依赖

- 状态：accepted
- 决定：成果 revision 不可变、head 原子移动；任务入队固化依赖。付费请求提交后未知结果进入 `interrupted_unknown`。
- 后果：不因崩溃自动重复计费；Lens repair 最多一次且单独 receipt。

## D-014：Usage 未知保持未知

- 状态：accepted
- 决定：token、费用、延迟等 provider 未返回时存 `NULL`；file reuse、session resume、token cache、paper root branch 分开。

## D-015：删除、空间与远端资源

- 状态：accepted
- 决定：Paper 进入 30 天回收站；恢复不覆盖。Workspace 总量是真实磁盘值，SQLite 逐论文只声明逻辑占用。远端删除失败写 tombstone 重试。

## D-016：凭据与诊断默认脱敏

- 状态：accepted
- 决定：Gemini/Mistral secret 只进 Windows Credential Manager 和短暂内存。诊断必须用户先预览再导出，只含聚合状态。
- 排除：密钥/Authorization、PDF/Prompt/Block/Discussion 正文、绝对路径/论文身份、provider 原始响应/ID 和任务错误正文。

## D-017：V2 首发范围

- 状态：accepted
- 决定：只支持原生 PDF；论文 LLM 允许 `gemini`、`openai_compatible`、`grok`；OCR 仍只 Mistral。不做搜索、Semantic View、多文档或旧数据迁移。
- 修订（2026-08-17）：Full Outline 经单独产品授权，合同见 [full-outline-v1.md](full-outline-v1.md)。
- 修订（2026-08-18）：授权 OpenAI-compatible 与 Grok adapter；论文根仍要求整本进上下文；Grok 检索式附件不算论文根。

## D-018：托盘与安全退出

- 状态：accepted
- 决定：有 active Job 或流式 Discussion 时关闭窗口必须显示“托盘继续 / 暂停并退出 / 取消并退出”。暂停退出只保留未提交或已有可靠 staging 的任务；已提交但结果未知的付费请求进入 interrupted_unknown。
- 后果：托盘继续不改变任务；暂停/取消退出保留流式 Chat 的 partial 并标为 cancelled；任何模式都不得静默重复付费。

## D-019：Tauri 结构体命令必须包一层

- 状态：accepted
- 决定：`send_chat(request: ChatRequest)` 的前端 invoke 为 `{ request: { ...camelCase fields } }`。组装只走 `buildSendChatInvokeArgs`。
- 后果：扁平传 `revisionId` 等字段会得到 `missing required key request`。其它带单一结构体参数的命令同样处理。

## D-020：Gemini Interactions 用现行扁平 input

- 状态：accepted
- 决定：请求 `input` 只使用 `text` / `document` / `image`。PDF 用 file URI + `mime_type`。流式/完成文本从 `steps[].model_output` 与 `delta.type === "text"` 解析。
- 后果：`input_text` / `input_file` / `turn_list` 会 400。测试锁住 body 形状。

## D-021：失败的 assistant 轮次删除而不是落库

- 状态：accepted
- 决定：非取消失败时删除 streaming assistant。`list_messages` 排除 `failed`。树与上下文忽略 failed。成功发布清掉同级残留 failed。
- 后果：用户看不到空的失败气泡，失败内容不进入下一轮 prompt。取消仍保留 partial。

## D-022：阅读器拥有捏合缩放，WebView 缩放因子锁定为 1

- 状态：accepted
- 决定：`zoomHotkeysEnabled` 保持 `false`，避免 chrome 被 Ctrl+/- 或原生捏合放大。setup 里用 WebView2 COM 单独打开 `IsPinchZoomEnabled`，`ZoomFactorChanged` 时拨回 1.0 并把 factor 交给 PDF zoom（60–180%）。JS 同时处理 `ctrl/meta + wheel`、双指针和 touch。
- 后果：不能靠把 `zoomHotkeysEnabled` 设为 `true` 或加 `--disable-pinch` 来修手势。改 COM 桥后必须重启 Tauri。详见 [agent-onboarding.md](agent-onboarding.md) 第 7 节。

## D-023：双阶段流转架构（文献大厅与超聚焦阅读工作台）

- 状态：accepted
- 决定：应用冷启动直接进入 `LibraryHub.tsx`（文献大厅），左侧按 `Papers/` 渲染层级文件夹树，右侧展示卡片网格与结构表格，卡片完整展示一句话学术结论（TL;DR）。点击论文进入 `reader` 阅读模式；阅读模式隐藏持久化侧边栏，将 100% 宽度分配给 PDF 画布（60%）+ AI 研讨（40%），并提供 44px 悬浮顶岛与 `Esc` / `Ctrl+\` 快捷导航。
- 后果：文献管理与文献阅读体验解耦，阅读时免受大列表与冗余控制项干扰。

## D-024：克制现代 Liquid Glass 拟态设计语言与 Theme 引擎

- 状态：accepted
- 决定：全站移除复古小说 Serif 字体与高噪点 `.grain` 纹理，升级为 Apple visionOS 启发的 Liquid Glass 拟态材质（`backdrop-filter: blur(24px)` + 1px 镜面顶部内高光）。提供 `liquid-light`、`liquid-dark`、`warm-editorial` 三套主题并写入 `localStorage`。
- 后果：统一全平台视觉风格，保持严肃清爽的学术美感与高品质质感。

## D-025：对话气泡拓扑（右侧框选用户问题与全宽无框 AI 回答）

- 状态：accepted
- 决定：用户提问采用圆角磨砂玻璃气泡框（`border-radius: 18px 18px 4px 18px`），居右排布；AI 的深度解析采用左侧全宽无框（Frameless Editorial）排版流，居中强化数学公式与代码块。
- 后果：明确用户与 AI 的空间与视觉区分，同时最大化利用研讨宽度阅读长篇学术内容。

## D-026：学术级 OCR 浅绿高亮与 4px 实线圆角

- 状态：accepted
- 决定：OCR 选区高亮弃用黄色/橙色杂色，改用半透明薄荷浅绿（普通态）与翡翠绿（引用/聚焦态）；边框统一采用 **4px 圆角实线（Solid）**，坚决不用破碎杂乱的虚线（Dashed）。
- 后果：在密集的论文排版中提供柔和、高对比且不刺眼的交互指引。

## D-027：视口锁定与双画布独立滚动隔离

- 状态：accepted
- 决定：`html, body, #root, .app-shell` 严格约束为 `100vh / max-height: 100vh / overflow: hidden`，彻底杜绝整体应用窗口上下晃动。PDF 画布与 Chat 消息流内部独立声明滚动容器。
- 后果：顶栏与底栏恒定吸附，多面板协同滚动顺畅无冲突。

## D-028：单行 38px 紧凑输入胶囊、即时流式中断与全局状态栏上移

- 状态：accepted
- 决定：将原先高度过大的多行 Composer 压缩为 38px 悬浮单行胶囊，支持 `Enter` 发送；AI 流式生成过程中，发送按钮无缝切换为红色 `■` 停止按钮，点击即刻调用 `cancel_discussion` 终止流式且安全保留已生成 partial。同时彻底移除底部 28px `<footer className="statusbar">`，将 Workspace 状态与 Token 计数器上移并入顶部 44px 悬浮岛。
- 后果：为 PDF 画布与研讨区直接释放了 98px+ 的垂直阅读空间，整体界面更空灵紧凑。

## D-029：纯净多模态用户气泡排版（公式/图片置顶，跳转药丸置底）

- 状态：accepted
- 决定：用户提问气泡剥离冗余的头像/时间行和说明文字，高度随行数紧凑自适应。当用户引用公式或图片时：
  1. **公式（Equation）**：直接以 KaTeX 渲染标准数学公式，置于提问正文上方（不附加任何“引用公式”多余标签）。
  2. **图片（Figure / Image）**：直接以微缩图卡片置于提问正文上方，点击弹出高清 Lightbox 模态框放大查看。
  3. **Block 跳转药丸**：统一收敛置于提问文本下方，以 10.5px 微型字号呈现，点击即可秒级联动跳转对应 PDF 页面并高亮选区。
  4. **编辑按钮 `✎`**：悬停右上角轻量显现，不占用气泡底部整行空白。
- 后果：问题气泡结构极简、多模态信息层级分明，视觉无噪点。

## D-030：SQLite Artifacts 一句话摘要向 Hub DocumentCard 的流水线映射

- 状态：accepted
- 决定：在 Rust 后端 `DocumentCard` 实体中增加 `brief_takeaway: Option<String>` 与 `has_ocr: bool`。在 `document_card_from_paper()` 查询构建 DocumentCard 时，若检测到 `brief_status == "ready"`，自动从 `artifacts` 表读取 `content_json` 并解析出 `summary`/`findings` 作为核心一句话贡献返回前端。
- 后果：彻底打通了后台生成 Brief 与前台文献大厅（Library Hub）展示真实一句话学术结论的数据流，杜绝了 Fallback 占位文本对已生成结果的遮盖。

## D-031：Full Outline 是深入阅读论证地图

- 状态：accepted（2026-08-17）
- 决定：Outline 独占「整篇论证怎么走、证据在哪」。节点是论证单元不是章节；必须已发布 OCR；读懂靠论文根 PDF，定位靠瘦 OCR 目录。恰好两层（Overview 粗地图 + Deep dive 局部关系图）。总图一个 Job 两个 Unit；局部图一次调用。论文根旁支，隔离 Discussion。旧版只读，不自动重跑、不迁边。壳只用 `pdf_discussion` / `pdf_outline` / `outline_only` 三个预设。独立 `OutlineModule`，复用 `JobModule`。
- 后果：取消「不做 Outline」。不迁入插件 Dexie/MV3。不做通用窗口管理器、Author Research。Reading Guide 已被 D-040 单独授权。合同见 [full-outline-v1.md](full-outline-v1.md)。已被 D-033、D-034 补充。

## D-032：设置（SettingsWorkbench）与任务中心（OperationsDrawer）全量接入 VisionOS Liquid Glass 设计系统

- 状态：accepted（2026-08-17）
- 决定：
  1. 彻底清除 `SettingsWorkbench` 与 `OperationsDrawer` 中遗留的硬编码深色底（`#132238`）、泛黄米底（`#f8f4eb`、`#fffdf8`）与 Serif 字体覆盖（如导致标题在浅色底上白字不可见的严重对比度 Bug）。
  2. 全面接入 VisionOS 液态玻璃设计令牌（`--bg-base`、`--glass-surface`、`--glass-card`、`--glass-border`、`--liquid-glass-shadow`、`--ink`、`--muted`、`--blue`）。
  3. 设置左侧导航栏、设置卡片、工作区重置警告框、任务中心 Drawer、Job 卡片、存储看板、Trash 列表、诊断卡片与退出确认弹窗均实现三套主题（`liquid-light`、`liquid-dark`、`warm-editorial`）自适应。
- 后果：全局界面一致性得到彻底统一，不再存在未适配主题或文字对比度失效的“孤岛页面”。

## D-033：Outline 质量靠可编辑提示词；地图可删可重生成

- 状态：accepted（2026-08-17）
- 决定：不设节点数发布门闩。抽单元 / 构图 / 局部图默认稿改为阅读地图合同。全部生产系统提示词可在 Settings 编辑，存在应用账号，入队冻结。总图与局部图提供删除（二次确认、不可恢复）和重生成（旧图留到成功）。任务中心按 `updatedAt` 新到旧。
- 后果：旧「只能生成一次」作废。协议曾 bump 到 extract/compose/deep-dive v2。不提供版本浏览。已被 D-034 补充（分叉合同、v3、一次性清旧图）。

## D-034：论证地图按并行结构分叉；旧 Outline 不兼容

- 状态：accepted（2026-08-17）
- 决定：提示词要求有并行就分叉汇合，不禁止单链，不设发布门闩。画布用紧凑卡片、细标签、可拖节点（坐标不持久化）、可收起可拖的详情栏。点选节点不再自动跳 PDF。打开落后于 `outline_epoch=3` 的工作区时清空全部 Outline 修订和 Outline Job，并把三份 Outline 提示词强制回出厂稿。
- 后果：已发布的 v1/v2 图不会再出现。协议号改为 extract/compose/deep-dive v3。论文 / OCR / 讨论 / Lens 不动。

## D-035：当前工作区只有一份 WorkspaceRuntime

- 状态：accepted（2026-08-19）
- 决定：`AppState` 不再并列多把 paper/job/artifact/outline 锁。`activate_workspace` 在锁外组好模块，再 `swap_runtime` 换上一个 `Arc<WorkspaceRuntime>`。切换只 stop+notify，不 join。墓碑列表不分页；Discussion 流式通道保持无界。
- 后果：命令读 `current_runtime()`。旧 worker 只写自己的 Arc。细节见 [backend-hardening.md](backend-hardening.md)。

## D-036：Windows 8 MiB 栈空间预留与 AppState 瘦身

- 状态：accepted（2026-08-18）
- 决定：在 `src-tauri/build.rs` 中为 Windows 目标注入 `println!("cargo:rustc-link-arg=/STACK:8388608");` 链接参数，将主线程栈空间由 Windows 默认的 1 MiB 提升至 8 MiB 保留空间；同时加入自动化回归测试 `app_state_fits_comfortably_on_a_small_stack_frame`，确保 `AppState` 内部全部重资源由堆指针（`Arc` / `Box`）间接持有，栈帧占用限制在 4 KiB 以内。
- 后果：彻底杜绝了 Tauri 2 在处理 ~80 个 IPC 命令展开递归匹配宏时引发的 `STATUS_STACK_OVERFLOW / 0xc00000fd` 栈溢出崩溃。

## D-037：任务中心（Operations Center）最新任务置顶排列

- 状态：accepted（2026-08-17）
- 决定：
  1. 后端 `JobModule::list` 查询的 SQL 排序修正为 `ORDER BY CASE jobs.state WHEN 'running' THEN 0 WHEN 'queued' THEN 1 WHEN 'paused' THEN 2 ELSE 3 END, priority DESC, created_at DESC, id DESC`。
  2. 前端 `OperationsDrawer.tsx` 建立状态优先级与 `createdAt` 降序记忆化排序（`sortedJobs`）。
- 后果：新发起的 OCR、论证地图与阅读成果 Job 始终呈现在任务中心列表最顶端，用户无需反复翻到最底部查找最新任务。

## D-038：原生窗口标题栏（Window Titlebar）随应用主题动态同步与 DWM 沉浸暗色

- 状态：accepted（2026-08-18）
- 决定：
  1. 在 Rust 后端提供 `set_window_theme(theme)` 命令与 `sync_window_theme`，在 Windows 下直接调用 Win32 DWM API（`DwmSetWindowAttribute`），同步设置 `DWMWA_USE_IMMERSIVE_DARK_MODE` (20/19)、`DWMWA_CAPTION_COLOR` (35) 与 `DWMWA_TEXT_COLOR` (36)。
  2. 当主题为 `liquid-dark` 时，标题栏变暗黑（`#090d16`）且最小化/最大化/关闭按钮由系统自适应白字；`liquid-light` 变为冰川灰（`#e2e6eb`）；`warm-editorial` 变为暖阳米白（`#eee9df`）。
  3. 前端在应用启动及主题切换时即时调用 IPC 同步窗口状态。
- 后果：彻底解决了暗夜主题下原生顶栏刺眼白色亮条的不一致问题，实现桌面级沉浸式体验。

## D-039：成果面板（ArtifactPanel）提供论证地图（Outline）快捷导航

- 状态：accepted（2026-08-19）
- 决定：在阅读器成果侧栏（ArtifactPanel）顶部索引列表及空态面板中增加“论证地图（Outline）”快捷打开按钮，调用 `onOpenOutline` 切换到 `pdf_outline` 分屏视图。
- 后果：打通了阅读成果面板与全局论证地图的无缝跳转通道。

## D-040：Reading Guide 是预计算多角色页边批注层

- 状态：accepted（2026-08-20）；角色冻结与英文旁批限制已由 D-068 取代
- 决定：授权桌面端 Reading Guide。它是 Focus Reader 上的一层，不是第三工作台，不新增 `pdf_guide`。V1 三人（阿林 / 老周 / 小夏）预写英文旁批，锚在 OCR Block 整框；卡片钉在该页 PDF 右侧。讨论栏与批注总开关独立。有批注的 Block 选中色变浅红，引用 / 翻译 / 解释 / Lens 照常。独立 `GuideModule` + SQLite revision/head，复用 `JobModule`。不迁入 Dexie/MV3/独立 Block View。第一版当时不做请离场、改人设、句子级高亮、批注内聊天。
- 后果：取消「不做 Reading Guide」。合同见 [reading-guide-v1.md](reading-guide-v1.md)。V2 角色库与中文旁批见 D-068。D-031 仍禁止通用窗口管理器和 Author Research。

## D-041：Lens 阅读成果按物理选区空间归并与多版本切换（< 1/2 >）

- 状态：accepted（2026-08-22）
- 决定：
  1. 相同论文、同种类（如 `lens_figure`）、同物理页且空间重合率 `IoU > 0.4` 的多次生成资产自动归并为单一 `ArtifactGroup`。
  2. 阅读成果（顶部 INDEX）按选区组渲染，每个物理选区仅呈现 1 个卡片（带 `N 版本` 角标），杜绝列表散落重复卡片。
  3. 详情页顶栏提供毛玻璃版本切换胶囊 `‹ 版本 X/Y ›`，切换时无缝切换分析正文、模型标签、时间及独立的 Lens QA 问答记录。
  4. 多版本时支持单版本删除（`🗑️`），通过 `DeleteVersionConfirmModal` 二次确认，删除后安全转移 Head 并平滑回退至相邻版本。
  5. 重新生成时通过空间 IoU 匹配动态定位当前 OCR 树中的真实 `block.id`，保证 100% 通过数据库外键校验。
- 后果：彻底解决了重复生成散落卡片与 Block ID 漂移报错的问题，大幅提升了学术成果浏览与版本对比体验。

## D-042：Claude 暖色纸张主题全面去刺眼白与色温校准

- 状态：accepted（2026-08-22）
- 决定：
  1. 彻底排查并消除暖色主题（`warm-editorial`）下的大面积纯白（`#ffffff`）硬色块，建立符合 Anthropic Claude 设计哲学的真实温润纸张色彩层级：
     - 底层基色（Base）：`#fbf9f5` / `#f6f1e8`（象牙暖米底纸）；
     - 侧边栏/沉底抽屉：`#f5efe6`（温润亚麻纸）；
     - 导图画布/舞台背景：`#f7f2ea` / `#ece5db`（淡褐纸色绘图底）；
     - 浮动卡片/面板：`#fdfcf9`（柔和象牙白）；
     - 边框分割线：`#e8e0d4`（书本细缝线）；
     - 主强调色：`#c15f3e`（Claude Terracotta 赤陶红）。
  2. 修复文献库侧边栏（`sidebar-glass-island`）、思维导图大纲画布（`Outline Canvas`）与右侧检查器（`Outline Inspector`）、精读路线（`Reading Roadmap`）、PDF 阅读器舞台（`reader-stage`）等区域的反光刺眼问题。
- 后果：暖色主题下不再产生高反差纯白眩光，带来沉浸、温润的书卷阅读质感。

## D-043：多布局切换常驻挂载与 PDF 阅读位置/滚动零重置

- 状态：accepted（2026-08-22）
- 决定：
  1. **常驻挂载 + CSS 响应式显隐（Zero-Unmount）**：在 `App.tsx` 中，切换到“仅地图（`outline_only`）”、“仅讨论（`pdf_discussion` 仅看对话）”或“阅读成果”等隐藏 PDF 视图的布局时，通过 CSS `display: none / flex` 控制 `<section className="reader-panel">` 的展示，严禁从 DOM 中卸载 `<PdfReader>`。
  2. **解决 IntersectionObserver 竞态**：在 `src/PdfReader.tsx` 中增加 `programmaticScroll` 保护锁与 `hasRestoredInitialScroll`，在初次加载和程序滚动恢复期间屏蔽视口交叠事件，防止 `scrollTop = 0` 时错误将当前阅读页覆盖重置为第 1 页。
  3. **实时同步 `pageOffset` 与 `restoreOffset`**：滚动事件同时更新 `pageOffset` 与 `restoreOffset`，重新挂载或切换时精准还原。
  4. **跨视图跳转自动唤出 PDF**：在地图证据跳转（`jumpOutlineEvidence`）、引用气泡跳转（`jumpToBlockQuote`）或 Markdown 论文引用跳转时，若当前处于单栏/隐藏 PDF 模式，自动将布局切回双栏并定位至目标页与目标 Block。
- 后果：在任何视图切换（仅地图 ↔ 双栏 ↔ 仅PDF）后重新显示 PDF 时，**0 延迟无白屏，且 100% 保持当前页码、精确滚动像素与缩放倍数**。

## D-044：多模态图片引用缩略图提取与全屏 Lightbox 交互

- 状态：accepted（2026-08-22）
- 决定：
  1. 用户在 PDF 中引用图片/图表 Block（`blockType === "figure" | "picture" | "table"`）发起提问时，前端通过 `PdfReader` 的 `PDFDocumentProxy` 异步渲染并提取选区 Canvas 高清截图（`cropDataUrl`）。
  2. 用户提问气泡（`MessageBubble`）与引用篮子（`QuoteBasket`）自动渲染图片缩略图卡片并置顶显示。
  3. 点击缩略图弹出全屏毛玻璃 Lightbox 模态框（`LightboxModal`），支持放大清晰查看原图并一键平滑跳转到 PDF 对应页面。
- 后果：极大提升了多模态提问与图表证据审阅的交互体验。

## D-045：界面微交互与细节优化（赤陶色收起、Block 证据边界高亮、下拉菜单外部点击关闭）

- 状态：accepted（2026-08-22）
- 决定：
  1. **大纲检查器“收起”按钮明显化**：升级为横排 `◂ 收起`，采用纯正 Claude 赤陶色（`#c15f3e`）圆角高对比度胶囊与悬浮光晕。
  2. **相关 Block 证据标签边界清晰化**：补充 `1px solid #e2dad0` 纸缝边框与 `#f4eee3` 底色，增加悬浮时赤陶色边框、赤陶色文字与微上浮（`-1px`）交互动效。
  3. **对话分支下拉菜单支持外部点击关闭**：为 `.thread-title-picker-wrap` 绑定全局 `mousedown` / `touchstart` 与 `Escape` 键盘事件，点击外部任意区域或按 ESC 即可平滑收起。
- 后果：解决了各面板边缘操作对比度不足与弹窗难以关闭的交互痛点。

## D-046：阅读成果空间极致收敛、顶栏轻量化与论证地图双向导航

- 状态：accepted（2026-08-22）
- 决定：
  1. **无用 Roadmap 卡片全局过滤**：在成果过滤管道中全面剔除 `reading_roadmap` 与 `roadmap`，确保全篇成果仅收纳 `✦ Brief`、`📑 术语表`、`∑ 符号表`、`ℹ️ Metadata`，彻底消灭空白占位卡片。
  2. **右侧栏顶部三态聚合分段（消灭原 1、2 层）**：
     - 将外层 Tab 与内层 Scope 统一为顶栏单行分段控制器：`[ 💬 讨论 ]   [ 🌐 全篇成果 (N) ]   [ 🔍 Lens (M) ]` + 右侧 `[ A- 15 A+ ]`；
     - 在 PDF 画布点击公式或框选图表触发 Lens 时，自动平滑切至 `[ 🔍 Lens ]` 标签并定位到目标成果；
  3. **消除正文上方冗余第 3 行**：
     - 将原先位于成果正文上方的 `由 XX 模型生成 · 23:39` 与 `[ 🔄 重新生成 ]` 上移合并至顶部第 1 行 `compact-rail-header` 的右侧空白区域；
     - 成果正文（Markdown/公式/表格）直接顶格展示在 Chip 胶囊栏正下方，净省 **106px+ 垂直阅读空间**；
  4. **论证地图 Chip 范围纯粹化**：
     - `[ 🗺️ 论证地图 Outline ]` Chip 仅在 `[ 🌐 全篇成果 ]` 激活时展示；在 `[ 🔍 Lens ]` 标签下隐藏，只展示对应选区分析流；
  5. **论证地图双向直达通道**：
     - 在 `OutlinePane` 顶栏右上角增加 `[ ‹ 返回成果 ]` 按钮，与 `[ 返回讨论 ]` 并列，支持从地图秒级原路退回成果页；
  6. **顶栏 OCR 单行防折行与高度收敛**：
     - 为 `✓ OCR 就绪` 按钮添加 `white-space: nowrap; flex-shrink: 0;`，解决中文字符折行问题；
     - 顶栏悬浮胶囊（`.reader-floating-island`）高度由 44px 压缩至 **38px**，上边距调优为 8px。
- 后果：右侧栏垂直空间利用率达到极致，各研读视图（讨论/全篇成果/Lens/论证地图）间流转无缝闭环。

## D-047：顶栏模型胶囊常驻、讨论输入区纵向净空扩展与 Roadmap 边界收敛

- 状态：accepted（2026-08-22）
- 决定：
  1. **顶栏模型胶囊常驻**：在右侧顶栏 `compact-rail-right` 中常驻显示当前论文模型徽章（`.rail-model-badge`），带状态指示与快捷点击前往配置，无论在「讨论」还是「全篇成果/Lens」下均可随时直观查看与切换；
  2. **讨论区纵向净空扩展**：彻底移除讨论输入框下方的 `.composer-subline`（「就绪」及冗余状态行），输入框紧凑吸底，讨论消息流垂直可视区域净增 30px+；
  3. **Roadmap 边界收敛**：精读路线仅在论文阅读视图下响应边缘悬停与唤出，在设置中心、任务中心及全屏讨论树等弹窗下严格静默，避免界面冲突。
- 后果：讨论区空间得到进一步释放，模型状态直观可控。

## D-048：全屏设置工作台、动态多 Provider 实例管理与排版/提示词体验升级

- 状态：accepted（2026-08-22）
- 决定：
  1. **全屏设置工作台（Full-Page Settings Workbench）**：
     - 淘汰狭窄的居中小弹窗，改为 100% 视口高度全屏桌面工作台（`SettingsWorkbench.tsx`），配备左侧固定导航栏（`SettingsRail`，支持 `ESC` 或点击返回阅读）与右侧三核心页面（工作区与排版、AI 模型与 Provider、系统提示词模板）。
  2. **动态多 Provider 实例管理与自动化流转（Multi-Provider Instances）**：
     - 重构底层 Provider 架构，允许用户动态添加最多 10 个独立 Provider 实例（支持 Gemini、OpenAI-compatible、xAI Grok），支持自定义名称、重命名、复制与拖拽排序；
     - **自动激活当前模型**：用户在保存某个 Provider 设置、新增 Provider 或复制 Provider 时，系统自动调用 `set_current_provider` 设为当前生效模型，避免用户遗忘设置。
  3. **视觉化中西文字体排版卡片与 KaTeX 实时公式预览**：
     - 精选 5 种西文字体与 4 种中文字体，采用独立视觉卡片（`.font-option-card`）展示真实渲染效果；
     - 底部集成实时双向 KaTeX 数学公式渲染预览卡片，排版效果立等可读。
  4. **提示词全视口锁定（Viewport-Locked Master-Detail Layout）**：
     - 提示词模板页面高度严格锁定在视口内（`overflow: hidden; height: 100%`），杜绝外层页面上下/左右滚动晃动；
     - 左侧按功能组呈现槽位列表（独立平滑滚动），右侧自适应撑满多行 Monospace 编辑器（配置 `box-sizing: border-box; white-space: pre-wrap; word-break: break-word;`），文本永不截断。
  5. **全局控件作用域与徽标防折行保护**：
     - 表单样式（如 `width: 100%` 的 `<select>`）严格限制在表单包装容器 `.form-control-wrap` 内，严防污染大厅 Hub 排序等行内控件；
     - 所有状态徽标、操作胶囊与标签统一配置 `white-space: nowrap; flex-shrink: 0;`。
- 后果：设置中心成为专业、美观、高效的桌面级管理工作台，彻底解决了配置割裂与排版阅读负担。

## D-049：OCR 级联删除安全保护与确认机制

- 状态：accepted（2026-08-23）
- 决定：
  1. **级联清理边界与数据安全保留**：
     - 在 Reader 顶栏 `✓ OCR 就绪` 旁提供 `🗑️` 删除 OCR 按钮；
     - 触发时弹出 `CascadeOcrDeleteConfirmModal`，向用户清晰对比“将被清理的依赖资产”（当前 OCR 块、段落精译、段落解析、Lens 深度分析及重 OCR 记录）与“将完整保留的资产”（全篇 Brief、论文元数据、学术术语表、数学符号表及全部研讨问答记录）；
     - 后端 `delete_ocr_cascade` 命令原子清理依赖 Artifacts，并将关联 active heads 安全置空。
- 后果：赋予用户在 OCR 识别不佳时重新识别的灵活性，同时保障核心研读与讨论数据绝对安全。

## D-050：Brief 与 Hub 标签双向联动及 Metadata/术语表/符号表深度编辑与图钉 📌 锁定

- 状态：accepted（2026-08-23）
- 决定：
  1. **Brief 学术标签（Keywords）排版与 Hub 双向同步**：
     - Brief 中的学术标签移至核心突破（Takeaway）正下方，支持在 Brief 中以及 Library Hub 卡片上就地 `×` 移除标签与 `+` 添加新标签，通过 `update_paper_tags` 实时同步生效。
  2. **Metadata、术语表与符号表就地 ✎ 编辑与 📌 图钉锁定**：
     - Metadata 各字段（标题、作者、年份、期刊/会议、DOI、摘要等）与术语表/符号表各行均支持就地编辑；
     - 用户修改保存后**自动变更为图钉锁定 📌**（防重新生成时被模型覆盖），亦支持手动点击图钉切换锁定；
     - 术语表与符号表支持整行删除（`🗑️`）与末尾新增行（`➕ 添加新学术术语 / 数学符号`）；
     - **真实持久化与大模型 Context 同步**：通过 `update_paper_metadata` 和 `update_orientation_table` 直接写入 SQLite `artifacts.content_json`，后续所有 LLM 任务（Translation 术语对齐、Reading Guide 旁批、Discussion 研讨对话）在读取 `head_content` 时均获取到用户真实保存并锁定的最新数据。
     - **智能合并重生成**：重新生成 Orientation Pack 时，后端自动读取已有 `_pinned` 项，同名词条以用户锁定的为准，新词条自动追加，用户锁定的词条永不丢失。
- 后果：实现了人机协同对学术成果的精准校正，锁定的内容深度参与后续推理。

## D-051：讨论渐进式加载 (Progressive Loading) 与上下文压缩脉冲指示器

- 状态：accepted（2026-08-23）
- 决定：
  1. **讨论长对话渐进分段加载（Progressive Discussion Window）**：
     - 初始只加载并渲染最近 15 轮（30 条问答节点），避免超长对话导致的初次渲染与 DOM 性能开销；
     - 顶部提供 `↑ 加载更早的 15 轮研讨 (剩余 N 轮)` 按钮，用户向上滚动到顶部时自动触发分页扩充；
     - 点击聊天时间轴导航器（Chat Timeline Navigator）历史刻度跳转时，若目标节点在窗口外，系统自动计算所需轮次并即时扩充视窗。
  2. **后台上下文压缩脉冲指示器（Compaction Pulse Indicator）**：
     - 当触发大模型上下文压缩（Compaction）时，后端通过事件推送 `discussion_compaction_started` / `discussion_compaction_finished`；
     - 前端在输入框上方渲染轻量优雅的呼吸脉冲提示徽章（`.discussion-compaction-pulse-indicator`），提示“⚡ 正在压缩历史研讨上下文以保持最佳推理速度...”，操作完全非阻塞。
- 后果：保障了千轮超长学术研讨下的极致流畅度与人机反馈透明度。

## D-052：全篇成果与 Lens 顶栏即时视域切换与状态记忆 (Memory Retention)

- 状态：accepted（2026-08-23）
- 决定：
  1. **顶栏标签即时界面切换**：
     - 点击右侧栏顶部的 `🌐 全篇成果` 或 `🔍 Lens` 时，主内容区直接同步刷新并渲染对应视域的成果视图，彻底消除“只切了顶栏高亮而主界面依然停留在另一视域、必须额外点击下方小胶囊”的繁琐交互；
  2. **视域状态记忆（Memory Retention）**：
     - 在 `App.tsx` 与 `ArtifactPanel.tsx` 中分别维护 `lastGlobalArtifactId` 与 `lastBlockArtifactId`；
     - 从 Lens 或讨论切换至全篇成果时，自动恢复上次浏览的具体成果（Brief、术语表、符号表或 Metadata）；
     - 从全篇成果或讨论切换至 Lens 时，自动恢复上次查看的具体选区分析成果（公式、图表或翻译）；
     - `ArtifactPanel` 中的 `activeGroup` 解析严格受控于当前活跃 `scopeTab`，确保视域与主视图状态 100% 一致。
- 后果：成果浏览与讨论的视域切换体验达成完全统一与顺畅。

## D-053：路径即种类；`Papers/` 与 `Textbooks/` 双根文库

- 状态：accepted（2026-08-23）
- 决定：
  1. 工作区用户 PDF 根从单一 `Papers/` 变为 `Papers/`（论文）与 `Textbooks/`（教材）。应用数据仍只在 `.read-desktop/`。`export/` 是导出区，不入库。
  2. 文档种类由相对路径第一段推导：`Papers/...` → `paper`，`Textbooks/...` → `textbook`。不另存可编辑 kind 列。
  3. SQLite schema 5：既有相对路径一次性加前缀 `Papers/`；根 collection `collection-root` 的 `relative_path` 从 `''` 改为 `Papers`；新增 `collection-textbooks-root`。现有 V2 工作区打开时静默补 `Textbooks/`，不 `reset_required`。
  4. Watcher 监视两根。其它顶层目录与工作区根上的 PDF 不导入，只通过 Workspace 投影给出「未管理的 PDF」提示。
  5. Hub 一棵树两根目录。应用内导入跟当前选中文件夹走；选中「全部」时先选论文或教材。
- 后果：代码层仍用 `papers` 表 / `paper_id`。跨根移动改种类（摘 Brief/地图/精读/旁批 head）是后续 PR，本 ADR 先锁路径合同。

## D-054：提示词每槽两份（论文 / 教材）

- 状态：accepted（2026-08-23）
- 决定：`prompt-settings.json` schema 2。每个槽位存 `paper` 与 `textbook` 两份正文；设置页用分段切换。旧 schema 1 自定义迁到论文侧，教材侧填 factory。Job payload 写入 `documentKind` 与该种类冻结提示词。恢复默认只动当前种类。教材 factory 暂与论文相同，后续 PR 再换语义。
- 后果：IPC `save_prompt_slot` / restore 带 `kind`（缺省 `paper`）。槽位 ID 不翻倍。

## D-055：跨根移动改种类；80 页提醒；schema 6

- 状态：accepted（2026-08-23）
- 决定：
  1. 跨 `Papers/` ↔ `Textbooks/` 移动 = 改种类。`move_paper` 带 `confirmKindChange`，种类变化未确认时拒绝并返回 `kind_change_confirmation_required`；确认后 `apply_kind_change` 摘掉 Brief / 术语 / 符号 / Metadata / 地图 / 精读 / 旁批 head，作废 context root，取消在途 Orientation / Outline / Roadmap / Guide Job。OCR、翻译 / 解释 / Lens、讨论记录保留。
  2. 磁盘上直接跨根移动（watcher → reconcile）同样触发摘头，并把 `kindChangeNotices` 通过 library 事件带到前端提示。
  3. 同根内改名 / 移动只改 Collection，不摘头。
  4. 所有 PDF 超过 80 页（`LONG_PDF_PAGE_LIMIT`）：OCR / Orientation Pack / 首次建文档根的聊天前软提醒，返回 `long_pdf_warning_required`；用户确认后 `ack_long_pdf_warning` 记一次「已坚持」，按文档持久化，之后不再弹。≤80 页不弹。
  5. SQLite schema 6：`reading_states` 增加 `long_pdf_warning_acked` 列（默认 0）。既有工作区原地迁移，不重置。
- 后果：`start_ocr` / `generate_brief` / 首次建根的 `send_chat` 会先查 ack。前端 OCR / Brief / 聊天入口捕获 `long_pdf_warning_required` 后弹确认，确认后自动 ack 并重试。

## D-056：教材 Orientation Pack 语义分叉

- 状态：accepted（2026-08-23）
- 决定：
  1. `orientation_pack` 提示词按种类分叉：教材 factory 面向「理解与掌握」，takeaway 语义为「这一章学什么」，brief 各段以教学组织（概念引入 / 推导举例 / 必须掌握 / 自测）展开；论文 factory 保持学术评审结构不变。
  2. 教材 metadata 的严格 JSON schema 允许 `bookName` / `isbn` / `chapterNumber`，不再要求 `venue` / `doi`；论文 schema 不变。
  3. 前端 Brief / Metadata 按 `documentKind` 渲染：教材 takeaway 标题为「这一章学什么」，空标题为「未命名章节」；Hub 卡片显示教材种类 chip、章节徽章（`第 N 章`）与「这一章学什么」摘要。
- 后果：论文槽位未改动时仍是原 factory；教材出厂稿与论文不同。`DocumentCard` 增加 `chapterNumber`（来自 metadata 工件）。

## D-057：地图教学角色（教材）

- 状态：accepted（2026-08-23）
- 决定：
  1. 论证角色按种类分叉：论文 `NODE_ROLES` 不变；教材新增 `TEXTBOOK_NODE_ROLES`（concept_intro / definition / example_illustration / derivation / algorithm_procedure / exercise_practice / caution_pitfall / application_example / summary_recap / other）。
  2. `normalize_role_class_for_kind` 按种类归一：论文角色在教材里归 `other`（如 `claim_hypothesis` → `other`），与既有未知角色策略一致；仍可发布，只加「N 个节点归为 other」覆盖率警告。
  3. 三份 outline factory（Extract / Compose / DeepDive）教材稿为教学向；论文 factory 不变。
  4. 不 bump `OUTLINE_EPOCH`，不清已有论文地图。
- 后果：`parse_extract` / `parse_compose` 带 `DocumentKind`；outline 执行时从 revision 推导种类。前端 outline 节点加中文角色标签。

## D-058：精读 / 旁批 / 讨论口吻（教材）

- 状态：accepted（2026-08-23）
- 决定：
  1. 教材 Discussion factory：引用可选（可不用 `[p. N]` / `[block:]`），讲解概念、走例题、指向练习即可；论文讨论仍强制引用。
  2. 教材 PaperRoot / DiscussionCompaction / Explanation / Lens 全系列（Formula / Figure / Table + 三个 Repair + LensQa）factory 为教学向；翻译槽仍共享论文稿。
  3. GuideContext 教材稿为教学上下文；GuideAnnotate 教材稿带 `{output_language}`，enqueue 时以 `zh-CN` 填充；论文旁批仍英文。
  4. ReadingRoadmap 教材 factory + schema：`learningObjectives` / `prerequisites` 可选字段；`elevatorPitch` / `oneChart` 对教材不再 required（论文 schema 不变）。
- 后果：`default_text_for_kind` 重构为按种类分派；roadmap response_schema 按种类生成。前端 roadmap 面板渲染「前置知识 / 学习目标」。

## D-059：导出阅读成果

- 状态：accepted（2026-08-23）
- 决定：
  1. `export_reading_bundle(revisionId)` 把一份文档的阅读成果导出为单个 UTF-8 Markdown，路径 `<Workspace>/export/<PDF 名不含扩展名>.md`。
  2. 写入：文头（标题 / 种类 / 相对路径 / 页数 / SHA-256 / 时间）、Brief、Metadata、术语表、符号表、地图文字大纲、Lens 文字与页码、讨论当前路径。
  3. 不写：OCR、图片、翻译、单块解释、精读、旁批、阅读进度、Job、费用、远端 id、Key、bbox JSON。
  4. 同名冲突：先写 `<stem>.md`；若该文件属于不同 SHA-256，则写 `<父文件夹>_<stem>.md`；同一 SHA 再导出覆盖。
- 后果：前端阅读工作台顶栏新增「导出」按钮；导出目录懒创建，永不入库。

## D-060：Gemini Proxy (Antigravity-Manager) 独立接入、PDF Inline 映射、Schema 注入与思考强度分级

- 状态：accepted（2026-08-23）
- 决定：
  1. **独立 Provider 类型**：在 `ProviderKind` 中新增 `GeminiProxy`（`"gemini_proxy"`），默认 Base URL 为 `http://localhost:8045/v1`，默认模型为 `gemini-3.7-flash-high` 与 `gemini-3.1-flash-lite`。
  2. **协议复用与架构路由**：底层走 `ChatCompletionsAdapter` 协议栈，继承成熟的多轮对话树分支恢复和客户端历史组装能力。
  3. **PDF Inline 传输（绕过 /v1/files 400 缺陷）**：
     - Antigravity-Manager 未实现 `/v1/files` 并且其 OpenAI 映射层不支持 `type: "file"`；
     - 解决方案：直接内联 PDF Base64，封装为 `type: "image_url"` 发送（`data:application/pdf;base64,...`）；
     - Antigravity-Manager 的 Data URI 转换层会自动将其还原映射为 Gemini 原生 `inlineData { mimeType: "application/pdf" }`。
  4. **结构化输出降级与 System Prompt 注入**：
     - Antigravity-Manager 仅支持 `type: "json_object"`，未将 `type: "json_schema"` 转发给上游；
     - 解决方案：将 JSON Schema 序列化并注入 System Prompt 前缀，同时把 `response_format` 降级为 `{"type": "json_object"}`。
  5. **思考强度（Thinking Level / Variant）分级选择**：
     - Flash 系列（`Gemini 3.7 Flash` / `Gemini 3.6 Flash` / `Gemini 3.5 Flash`）支持 `Low` / `Medium` / `High` 三档思考；
     - Pro 系列（`Gemini 3.1 Pro`）支持 `Low` / `High` 两档深度思考；
     - 轻量辅助模型（`Gemini 3.1 Flash Lite`）无需思考档位；
     - 前端表单提供直观的思考分段按钮，实际下发模型 ID 自动组合为规范后缀（如 `gemini-3.7-flash-high`）。
  6. **连接诊断与容错**：
     - 精准解析 Antigravity-Manager 服务端 503 `Proxy service is currently disabled` 状态并提示用户开启代理开关；
     - 当 `/v1/models` 返回空列表时自动填充默认 Gemini 3.x 模型目录；
     - 探针测试若模型输入为空时自动回退为默认模型并防止绿色/红色提示框冲突。
- 后果：用户可无缝通过本地 Antigravity-Manager 代理享用最新的 Gemini 3.x 思考模型研读长文与 PDF，无需直接绑定官方 API 密钥。

## D-061：Provider 实例级安全路由、冻结执行身份与失败关闭恢复

- 状态：accepted（2026-08-24）
- 决定：
  1. 论文 Provider 的执行身份不再等同于 `ProviderKind`。每次生成捕获精确实例 UUID、已验证规范端点、完整模型集合与当时的精确凭据身份；kind 仅保留为展示、兼容统计和暂时保守的并发配额。
  2. 身份分两层：`endpoint_scope` 由实例、kind、规范端点和精确 Key 摘要派生；`route_id` 再加入冻结模型集合与 operation role。Job、Context Root、Provider Node、usage receipt 按 route 隔离；远端 tombstone 按 endpoint 隔离。两种 identity 均只留在 SQLite 内部，不进入 IPC、日志或诊断。
  3. 新 `provider_routing` 深模块独占精确凭据读取、URL canonicalization、identity 计算、adapter 构造、snapshot 持久化和 legacy 判定。调用方不得按 kind/current/Map 第一项选择 Key，也不得手工拼接或解析 scope。
  4. Durable Job 入队时冻结 paper/translation 模型与 endpoint；重启后只能用原实例和能重算出原 endpoint scope 的 Key bind。`provider_committed` 后禁止改绑；uncommitted rebind 也必须同 kind、冻结模型完全匹配且重新通过 readiness。
  5. 无法证明归属的 active Job 使用既有 `paused` 状态并写一等 `job_provider_requirements`；普通 Resume 必须被后端拒绝。Legacy committed/interrupted 永久 quarantine，不能伪装成可恢复。Legacy tombstone 不猜实例、不联网删除，只能保留或显式放弃自动清理。
  6. SQLite schema 7 的 migration 先以休眠入口和 fixture 验证；只有所有写入路径完成、且 Job/cleanup/reconcile 全程持有同一个 `Arc<WorkspaceRuntime>` 后，才在一个 activation 提交中同时提升版本、迁移、reconcile 并启动 worker。
- 后果：同 kind 多实例不会串 Key、串端点、复用远端上下文或互删资源；Gemini Proxy 自定义 URL 可跨重启保持。Key 轮换会让旧 route 失败关闭，用户需恢复原 Key，或在未提交任务上显式安全改绑。Legacy Context 首次继续会重建远端根，可能增加一次等待与费用。

实施状态（2026-08-24）：上述 ADR 已进入当前生产合同。`schema v7`、精确实例 UUID、冻结 route / exact key、legacy jobs 与 tombstones 的人工确认隔离、Mistral OCR endpoint scope、cleanup 批次 32、attempts 重登记归零、任务中心 recovery 引导，以及 memory adapter 不伪造 probe，均已落地。

## D-062：Hub 手动排序与教材章节排序（稀疏 order 表 + 纯前端章节切分）

- 状态：accepted（2026-08-29）
- 背景：Hub（文献大厅）需要用户在书/论文文件夹内手排卡片顺序，并对教材按章节号排序；默认排序仍是「导入时间」(`recent`)。

### 1. 上下文与约束

- 仅叶子文件夹可排序；父文件夹若混进子孙 PDF 则整层回落 `recent`（D-062 之前无排序持久化）。
- 不得提升 `SQLITE_SCHEMA_VERSION` / `PRAGMA user_version`（当前 7，见 D-061）；新增表必须 `IF NOT EXISTS` 幂等。
- 章节号唯一可信来源是 `paper_metadata` → `DocumentCard.chapterNumber`，不得从文件名推断。

### 2. 决定

| # | 决定 | 关键实现 |
|---|------|----------|
| 1 | 排序只对**选中且本层没有子孙 PDF** 的 Collection 生效；`全部` 或混进子孙 PDF 时回落 `recent`，**不把回退写成偏好** | `hubSortAvailability()` + `effectiveMode` 回落 |
| 2 | 持久化两层：`collection_sort_prefs(collection_id, sort_mode, updated_at)` + `collection_paper_order(collection_id, paper_id, position)`，`IF NOT EXISTS` 幂等，**不升版** | `v2_workspace::ensure_hub_sort_tables`，`PaperModule::open` 亦 ensure |
| 3 | 手排键 `(collection_id, paper_id)`；无存档 = `recent`（`importedAt` 降序）；有存档后缺席 live paper 按 `importedAt` 升序接末尾；每次成功 reorder 写当层全部 live id（稠密 `0..n-1`） | `hubSort.ts` `mergeManualOrder` / `reorder_collection_papers` |
| 4 | **首次进入 manual 不写库**，仅真正换序的拖拽才调 `reorder_collection_papers` | `LibraryHub` 拖拽提交前对比 `paperOrder` |
| 5 | `reorder_collection_papers` 要求当层全部 live 论文的**精确置换**；否则报错不落库；顺序未变 no-op | 后端精确置换守卫 + 有序写入 |
| 6 | 同文件夹改名（F2 / 同 collection 的 `move_paper` / reconcile 同层改名）**不删** order 行；仅 `collection_id` 改变或 `trash_paper` 才删；`restore_paper` 不还魂 | `move_paper` / reconcile 仅 `collection_id` 变化时 `DELETE` |
| 7 | 章节排序算法锁定为 `/[.\-]/` 分段整数比较（`1 < 2 < 3.10 < 10`），本层至少一份可解析章节号才启用。无 `libraryClient` 时仍走 `hubSort.ts`；有 client 时 `hub_page` 用 SQLite `chapter_sort_key`（同一算法） | `hubSort.ts` / `chapter_sort.rs` |
| 8 | `全部` / 混子孙 PDF → 手动/按章节禁用；搜索时手动禁拖拽（不出插入线、不写盘）、按章节仍可对可见结果排序 | `LibraryHub` availability + search guard |

### 3. 后果与合同

- `CollectionProjection` 新增 `sortMode: "recent"|"year"|"title"|"manual"|"chapter"` + `paperOrder: string[]`；IPC `reorder_collection_papers` / `set_collection_sort_mode` 见 [data-protocols.md](data-protocols.md)。
- 不得回改精确置换契约，否则会静默重排。
- 实施状态：代码已落地。命令、测例与踩坑以 [hub-sort.md](hub-sort.md) 为准；Windows 编译 OOM 见 [windows-dev-build.md](windows-dev-build.md)。

### 4. 验证矩阵（下个 AI 复核用）

| 场景 | 断言 |
|------|------|
| `ensure_hub_sort_tables` 重复调用 | 两表存在；D-062 补丁本身不升版。当前生产库是 schema 8 |
| 同文件夹 F2 改名 | order 行数与 `paperOrder` 不变 |
| 跨文件夹/跨根 `move_paper` | 旧 `collection_id` 的 order 行被删 |
| `trash_paper` / `restore_paper` | 删行；恢复后不出现旧 `position` |
| `reorder` 非精确置换 / 未知 collection | 报错且当层 `paperOrder` 不变 |
| 章节排序 | `1 < 2 < 3.10 < 10`，无 `chapterNumber` 按 `importedAt` |
| 全部视图 / 混子孙 PDF | 下拉手动/按章节 `disabled`，提示文案聚合在 `select[title]` |

### 5. 扩展指南（新增排序模式时）

1. 在 `hubSort.ts` 加新 `HubSortMode` 分支与比较器，**不得**把章节解析改成文件名推断。
2. `collection_sort_prefs` 白名单在 `set_collection_sort_mode` 校验，新增 mode 同步改白名单与前端下拉。
3. 若需持久化新模式的辅助数据，新增表仍用 `IF NOT EXISTS` 且评估是否需提升 `user_version`（默认不升）；若必升则单独 ADR。
4. 更新 [feature-specs.md](feature-specs.md) 的状态表与 [data-protocols.md](data-protocols.md) 的 IPC 白名单，保持三处一致。

### 6. 踩坑（已沉淀）

- 同文件夹改名曾被实现为 `DELETE FROM collection_paper_order WHERE paper_id=?` 而误删手排（修复：仅 `collection_id` 变化时删）。
- `reorder` 若接受可见子集 `paperIds`，会把未传入的 live paper 静默丢弃（修复：后端精确置换守卫）。
- `paperOrder` 曾被 `prepare_cached` 后的 `Statement` 借用导致 `cannot move out of connection` 编译错（修复：把 `Statement` 限在块作用域）。
- `LibraryHub` 曾把 `paperOrder` 存 `useState` 导致拖拽中途重渲染丢 `insertIndex`（修复：insert 索引放 `dragState` ref，state 仅画虚线）。

## D-063：文库 Workflow、持久 Batch、阅读生命周期与 Smart Collection

- 状态：accepted（2026-08-31 锁定目标；PR 0–6 代码已落地：schema 8、`library_read` / `library_act`、本地 Batch、Reading Lifecycle / Smart Collection、Provider 批量与费用预览、Hub `hub_page` 分页虚拟列表 / `chapter` / `collection_layer`。实机 Explorer drop 与 Windows P95 仍属发布门槛。逐 PR 进度见 [实施计划](library-workspace-plan-2026-08.md) §11.0）
- 背景：Hub 已有可靠的单篇导入、单项命令、Pointer 拖拽和 D-062 手排，但选择仍是单个 Paper，导入仍是单文件，撤销只存在于 8 秒前端 Toast；100+ Paper 时逐篇投影查询与缺少批量管理会成为主要体验瓶颈。

### 决定

1. **建立纵向深 Module，不再扩张浅回调**
   - 前端 `LibraryHub` 只学习 `view + dispatch`；
   - 桌面边界收敛为版本化 `read / act / watch`；
   - 后端内部由 `LibraryQueryModule` 与 `LibraryWorkflowModule` 隐藏查询快照、批次状态机、文件 journal、Job 编排、费用确认、重试和补偿。

2. **Selection Set 瞬时，Selection Snapshot 与相关 revision 权威**
   - Shift anchor、focus、显式 IDs、all-matching query 与排除项只在前端内存；
   - Plan 使用一个 `BEGIN IMMEDIATE` 校验 query dependency revisions、解析目标、冻结 Paper / Revision / 每动作 precondition digest，并写 Batch / Items / idempotency receipt；
   - 新增全局单调 `Library Revision` 用于事件序与 domain revision vector 用于精确陈旧检查。当前由数组数量拼出的 cursor 不再用于防陈旧；
   - 只让 query 相关领域变化使 Selection 失效；query / precondition 漂移时 fail closed，不把后来出现的 Paper 静默加入批次。

3. **多目标操作使用持久 Batch / Batch Item**
   - Import、Move、Tag patch、Trash、Export、OCR、Brief 和批量 Lifecycle 都有逐项状态与结果；
   - 部分失败不抹掉成功项；重试创建 child Batch；取消只停止剩余 / 可安全停止的项；
   - 整批撤销是带前置条件的 Compensation child Batch，不改写父 Batch / Item 的原终态，也不是跨 SQLite、文件系统与 Provider 的事务回滚；
   - Undo Token 持久、单次、有限期；Provider 已发生费用永不承诺可撤销。
   - change / plan / start / control 使用与业务效果同事务的 idempotency receipt，重复请求不能重复副作用。

4. **批量 Provider 操作只能编排现有 JobModule**
   - OCR / Brief 每项关联冻结 route 的 durable Job；Batch 不读取 Key、不直接调用 Provider、不另造限流器；
   - Plan 只能保存 JobModule 持久化冻结 route 后返回的不透明 Prepared Job Handle；Start 必须在同一事务消费 handle、enqueue/coalesce、写 link 和更新 Item，禁止“先入队后关联”；
   - Job 可能 coalesce，使用 Batch Item ↔ Job 多对多 link、唯一 created owner 与 consumer state，不能在 `jobs` 上只放单一 `batch_id`；
   - 费用预览必须区分 exact / upper-bound / estimate / unknown；unknown 不显示为 0。created owner 归属 Receipt，joined Batch 显示无新增归属费用，不能重复汇总同一费用。

5. **Reading Lifecycle 与 Reader Session State 分离**
   - Lifecycle 保存未读 / 阅读中 / 已读、收藏、优先级、稍后阅读和复习日期；
   - Engagement 按 Paper + Revision 保存首次 / 最近打开和最远页；
   - 现有 `reading_states` 继续只负责当前页、缩放、旋转、布局、草稿等 Reader 恢复状态；
   - 首次打开可 `unread → reading`，但不得只按百分比自动标为已读。

6. **Smart Collection 是版本化查询，不是目录**
   - 只保存 whitelist query AST，后端编译参数化 SQL；不保存 raw SQL 或成员 IDs；
   - Smart Collection 不参与手排，也不能作为物理拖放 / Move 目标；
   - 首批内置“本周导入但未读”“阅读中”“稍后阅读”“需要复习”“OCR 失败”“最近打开”。

7. **扩展拖拽可发现性，但保持 D-062 精确置换**
   - 增加 hover/focus 把手、明确插入线、folder target 高亮、无效原因、Move 对话框、`Alt+↑/↓`、首次 coach mark 和真实可折叠目录树；
   - Ctrl/Shift 多选与键盘 / 显式菜单成为一等入口；
   - 拖拽、键盘和菜单 reorder 都必须构造物理叶子 collection 全部 live Paper 的完整精确 permutation；搜索 / Smart Collection 不得提交子集。

8. **正式升级 Schema 8**
   - 新增全局 / domain revisions、idempotency receipt、Lifecycle、Engagement、Smart Collection、Job preparation、Batch、Batch Item、Job link 与 Undo Token 持久化域；
   - 使用 v7→v8 原子 migration、备份、validator 与 fail-closed fixture；
   - D-062 的 no-bump 只适用于当时 Hub Sort 辅助表，不限制本 ADR 的正式 schema 演进。

### 后果

- `App.tsx` 不再承担批量循环、费用确认和撤销编排；现有浅 IPC 通过兼容 Adapter 逐步迁移后删除。
- Hub 投影必须改成一次有界聚合查询，消除逐 Paper 重开数据库的 N+1。
- 文件系统操作只能逐项 journal / reconcile，因此 UI 必须诚实显示 partial success、partial retry 与 partial undo。
- 详细 Interface、Schema 草案、PR 切片、风险和验收矩阵见 [library-workspace-plan-2026-08.md](library-workspace-plan-2026-08.md)。

## D-064：Lens 区块卡片流与双层专属深度探讨架构

- 状态：accepted（2026-09-04 落地；59 files / 400 tests 100% 通过；实现手册与踩坑见 [lens-block-cards.md](lens-block-cards.md)）
- 背景：原 Lens 选区面板将段落翻译、解释、Lens 细碎平铺成横向超长 Chip 列表，不仅割裂了同一物理区块的各项认知维度，还导致列表无限延展、滚动体验极差。此外，当在卡片内展开多轮 Q&A 问答时，单卡片被无限拉长，破坏了列表流的浏览节奏。

### 决定

1. **以 PDF 物理区块为中心（Block-Centric）的卡片流**
   - 按论文真实阅读物理顺序（`pageNumber` 升序，同页内按 `blockIndex` / 垂直坐标）组织聚合卡片流；
   - 每个物理区块作为独立卡片沙盒，聚合展示该区块的公式 KaTeX、图表/表格高清截屏缩略图（支持 Lightbox 放大）、段落 Markdown 预览；
   - 卡片内聚合多维度功能 Tab（[ 翻译 ]、[ 解释 ]、[ Lens 深度分析 ]），支持多版本切换与历史版本删除。

2. **双层轻量架构：外层卡片流 vs 内层专属研读页**
   - **外层卡片流 (`BlockCardStream`)**：严禁内嵌完整对话流和庞大提问框；仅展示结论概览与「💬 深入探讨此区块 (N) →」入口，保持列表紧凑与滚动轻盈；支持多卡片独立展开对比阅读（设置 `flex-shrink: 0` 保证展开高度独立不坍塌）；
   - **内层专属问答页 (`BlockCardQaDetail`)**：点击入口推入沉浸式全屏研读页。顶部渲染高清选区原图，随后自然平铺全部结论与证据锚点，支持多轮深入推演，点击「← 返回区块列表」瞬间平滑定位回原卡片并触发高亮脉冲。

3. **安全防误触预检卡片机制 (Pre-flight Confirmation)**
   - 点击尚未生成的 Tab 时，**绝不自动调用 API 消耗付费 Token**；
   - 呈现结构化预检卡片，说明功能价值，待用户明确点击「✨ 开始生成」后再触发任务。

4. **三级高可用图表截屏加载链路**
   - 判定规则扩展至所有 `lens_*` 产物，不单凭 OCR blockType 是否为 figure 做拦截；
   - 级联顺序：已载入传参/缓存 -> Tauri 本地磁盘持久化缓存 (`get_artifact_asset_path`) -> 动态按 `artifact.evidence[0].bbox` 与页码执行 Canvas 裁剪。

5. **输入框固定常驻吸底与 3 行无感自适应**
   - 专属页输入框从滚动流中解耦，固定在最底部（`.block-qa-bottom-bar`），底边空白极致精简（`8px 16px 10px`），最大化留白给上方阅读区；
   - 1~3 行内随 Shift+Enter 换行平滑撑高，并强制隐藏滚动条与 Windows 原生上下小箭头（`overflow-y: hidden`）；4 行以上高度锁定 88px 并激活 4px 极细滚动条。

## D-065：读者上下文（磁盘 md，不是系统提示词）

- 状态：accepted（2026-09-07）
- 背景：用户需要告诉模型「已掌握什么、这篇/这夹在干什么」，但不能覆盖 19 个生产槽的 schema / 引用 / 旁批身份。
- 决定：
  1. 对象是**读者上下文**，不是系统提示词。Settings → Prompts 仍只编 19 槽。
  2. 磁盘是唯一权威：工作区根 `read-desktop.reader.md`（全部文档）、文件夹内同名、PDF 旁 `{stem}.read-desktop.reader.md`。无 SQLite 正文副本，无双向同步。应用内编辑器写这些文件。
  3. 注入：带标签附加段进 **user_input**，不进 `system_instruction`。空层不发。
  4. 谁读：讨论、精读、旁批、解释、Lens 生成、Lens QA。Pack / 地图 / 翻译 / 压缩 / 论文根 / Lens repair 不读。
  5. 叠法：全部文档 → 祖先文件夹 → PDF。Job 入队冻结 `payload.readerContext`；讨论与 Lens QA 每轮现读。
  6. Hub 改名/移动/回收站/恢复带着 sidecar；删文件夹前把目录 md 送进 `.read-desktop/trash/reader-folders/`。Explorer 只改 PDF 名不认领孤儿。
  7. Watcher 对 workspace 根 **NonRecursive**；读者上下文文件不触发 PDF reconcile。不要 recursive 监视整个 Workspace。
  8. F2 出厂稿加默认读者缝（J4）与「有 Reader context 则让位」。`f2ReaderSeamGeneration=1` 只升级仍等于旧出厂的槽。
  9. IPC 叫 `get_reader_context` / `save_reader_context`，不要占用已有的 `get_reading_context`（阅读器页码/滚动）。
  10. 编辑器居中、约占视口 80%，左编辑右 Markdown 实时预览。遮罩不复用任务抽屉的 `.scrim`。
- 后果：菜单文案必须写「读者上下文」。智能集合无文件、无入口。≥2000 字只警告不截断。
- 实现手册（不变量、踩坑、如何扩展、变更日志）：[reader-context.md](reader-context.md)。


## D-066：Brief 与辅助成果独立生成、中文 Brief 定稿

- 状态：accepted（2026-09-08，用户讨论确认并授权实施）。
- 决定：Brief、术语表、符号表、metadata 各自生成与发布；共享只含原生 PDF 和最小确认的 `pdf-source-v1` 来源根，各自独立分支。术语表可选参考冻结的 Brief 主题线索，其余两项直接读 PDF。
- 提示词：九字段论文 Brief 采用已确认中文定稿；设置从 19 槽扩为 22 槽，内部 `orientation_pack` ID 作为 Brief 保留。旧设置迁移先备份；旧排队任务保留旧协议。其余中文稿逐项讨论，不推断已定稿。
- 合同及源码：[brief-generation.md](brief-generation.md)、[prompts.md](prompts.md)、`src-tauri/src/document_artifacts.rs`、`src-tauri/prompts/brief.paper.md`。不变更 SQLite schema 8、Provider route 和凭据冻结边界。


## D-067：辅助提示词完整定稿与结构化输出 v2

- 状态：accepted（2026-09-08，用户确认并授权直接修改）。
- 正文以 `docs/note/auxiliary-prompts.md` 的有效确认片段为准；四份文件完整编译加载，用来源清单和保真测试防止缩写替代原稿。论文 Brief 保持已确认原文。
- 独立任务绑定提示词、Schema 与协议；根缓存纳入实际根稿指纹。旧默认稿升级、旧自定义稿保留并沿用其协议，旧响应检查点保留恢复能力。
- 模型原稿、人工设置和有效展示分开保存；编辑新增成果版本，发布与文库投影在同一事务完成。旧四合一结果不覆盖独立成果，旧 PDF 修订的迟到结果仅保留历史。
- 元数据书目结构统一，年份、摘要选择和旧记录保留各自来源。出处与问题在人工修改后仅对可靠对应的目标保留关联。
- 复用 schema 8 的 JSON 与成果版本存储，应用格式以 `_format` 版本化，不新增数据库表或重写既有迁移。详细合同及离线验证范围见 [auxiliary-generation.md](auxiliary-generation.md)。

## D-068：旁批角色可配置，出厂五人与中文 V2 协议

- 状态：accepted（2026-09-09，用户确认并授权实施）。
- 决定：
  1. 取消 D-040「角色冻结、不能改人设」。用户可在设置中管理角色、头像、颜色、详细设定和默认阵容。
  2. 出厂五人：千反田爱瑠、折木奉太郎、芙莉莲、空条承太郎（第四部取向）、江户川柯南。默认阵容为前三人。人物 ID 不以显示名充当。
  3. 单篇生成选择本次阵容并记住文档偏好；入队冻结完整人物快照。旧旁批保留生成时的名字、头像、颜色和性格版本。
  4. 论文／教材四份中文提示词为生产稿。备忘与墨迹使用 `reading-guide-desktop-v2` / `reading-guide-memo-v1`。JSON 字段名保持英文。
  5. 已知旧出厂稿成对升级；任一槽自定义则保留并走 V1。不擦槽。备份 `before-guide-characters-v2.json` 不覆盖。
  6. V1 三人只读历史映射永不指向新动漫人物。缺少协议的旧任务仍按 V1 执行。未知未来协议拒绝。
  7. 不增加批注内聊天、跨篇记忆、独立模型路由、句子级高亮或工作区 schema 升级。偏好与备忘表幂等新增。
- 合同：[reading-guide-generation.md](reading-guide-generation.md)、[guide-characters.md](guide-characters.md)、[reading-guide-v1.md](reading-guide-v1.md)（历史）、[reading-guide-characters-plan.md](reading-guide-characters-plan.md)。

## D-069：地图 V2 联合构图、自由关系与无损迁移

- 状态：accepted（2026-09-10，按 [outline-map-v2-plan.md](outline-map-v2-plan.md) 实施）。
- 决定：
  1. 新图语义为 `outline-map-v4`。总图为整体构图 → 独立检查与定稿；局部图一次展开。不再先冻论证单元再被动连线。
  2. 节点角色与关系标签为自由中文；不要求连通或无环；允许无向边、自环、平行边与多分量。硬校验只覆盖结构完整性与原文定位。
  3. 新路径使用 PDF-only 来源根，默认不注入 Brief／术语表／Discussion／旁批。总图共享最多一次结构修复；复核失败不覆盖旧 head。
  4. 保留三个提示词槽 ID，按文档类型 bundle 区分 v3／v4。已知旧出厂稿升级为六份中文稿；自定义旧稿保留并走 v3。不 bump `OUTLINE_EPOCH`，不删除用户地图或自定义稿。
  5. 未知未来协议在付费前拒绝。v3 任务与成果继续可读。
- 合同：[outline-generation.md](outline-generation.md)、[note/outline-prompts.md](note/outline-prompts.md)、[full-outline-v1.md](full-outline-v1.md)（历史 v3）。

