# Handoff

下一个 agent 先读 [agent-onboarding.md](agent-onboarding.md)（Outline **§28**，论文 provider **§29**，旁批 **§30**，PDF 虚拟列表 **§6**，成果 Markdown **§44**，选区复制 **§48** / [markdown-rendering.md](markdown-rendering.md) **§9**，Hub 排序 **§45** / [hub-sort.md](hub-sort.md)，Windows 构建 **§46** / [windows-dev-build.md](windows-dev-build.md)，文库 Workspace **§47** / [library-workspace.md](library-workspace.md)，读者上下文 **§50** / [reader-context.md](reader-context.md)）和 [backend-hardening.md](backend-hardening.md)，再动代码。后端工作区/SQLite/worker 的坑只写在 hardening 页，不要从 `lib.rs` 猜。设置 / Chat Completions 的坑在 §29。旁批合同 [reading-guide-v1.md](reading-guide-v1.md)。挤在一起的列表 / 双星号粗体 / **选区 Ctrl+C** 合同 [markdown-rendering.md](markdown-rendering.md)。动 Hub / `library_read` / Batch 先读文库手册 **§0–§2、§7**，不要用下面「当前基线」里过期的 PR 1 叙述当现状。

## 当前基线

V2 深模块已在 `7883e80`。本轮工作在分支 `codex/optimize-read-desktop-p0`：D-062 Hub 排序 + Windows `tauri dev` LLVM OOM 缓解 + **D-063 PR 0–6 代码**（schema 8、`library_read` / `library_act`、本地 Batch、Reading Lifecycle、Smart Collection、Provider OCR / Brief、Hub `hub_page` 分页虚拟列表、`chapter` 排序、`collection_layer`、Explorer drop 合同）。**现状与踩坑以 [library-workspace.md](library-workspace.md) 为准**，不要用本段的考古句子当「尚未实现」。后续修改仍须保护用户工作树，禁止 reset、checkout 或覆盖式重写。下一个 agent：文库先读 [library-workspace.md](library-workspace.md) 与计划 [library-workspace-plan-2026-08.md](library-workspace-plan-2026-08.md) **§11.0**；Hub 排序读 [hub-sort.md](hub-sort.md) **§5**；线格式读 [data-protocols.md](data-protocols.md)；其余读 [agent-onboarding.md](agent-onboarding.md) 对应节。

## 已实现

- Workspace V2、嵌套 Collection、watcher/reconciliation、不可变 revision/head、Trash、空间统计。
- durable Job、checkpoint/recovery、provider/OCR 并发、暂停/恢复/取消、priority。
- PDF.js Reader（worker、±2 pages、**绝对定位槽 + 实测画布高度**、仅外部翻页才写 `scrollTop`、render cancel、no Text Layer、system fallback）。禁止再引入 spacer / `scrollIntoView` 追页 / `initialScrollOffset` drift recovery。
- 阅读器缩放：工具栏 ±、触控板捏合、触摸双指、WebView2 `ZoomFactor` 桥；只缩放 PDF，不缩放 chrome。分割条拖对话宽度。
- Mistral OCR staging/Blocks/remap/cleanup；OCR 4 扁平 bbox；可复用 raw staging。
- Gemini Files + Interactions 论文根（扁平 `text`/`document`/`image` input）、Orientation Pack、流式 Discussion、active branch、stale ID 恢复、context compaction。
- OpenAI-compatible / Grok 论文 provider：`model-settings.json` schema 3；Chat Completions adapter；保存 ≠ 设为当前；`paper-probe-v1`；Job/Chat 按快照 provider 路由。入口 [agent-onboarding.md](agent-onboarding.md) §29。
- `send_chat` 经 `{ request: ChatRequest }` 调用；编辑/重生成走 `regenerateFromId`。
- 真实 `parentId` 讨论树：聊天只渲染当前路径；树上去重为「一轮一问一答」卡片，单击载入、可删子树。重生成是兄弟轮，不是同一 user 下第二条 assistant。失败 assistant **删除**，不进 `list_messages`、不进树。
- 气泡无「Set current branch」；切路径只在树。助手模型名来自 `usage.model`。
- Thread：`×` 归档或丢掉空对话；已关闭可恢复/硬删；自动标题 + 双击改名；标签横滑。
- Block：单击选中钉住工具条；悬停只预览；空白/Esc 取消。
- Brief 无独立弹窗；成果页生成 Orientation Pack；Brief 详情里才有重新生成。
- Markdown/KaTeX：裸 `z_i` 会包进 `$...$`；选区 `Ctrl+C` 复制 GFM（列表带 `1. `，公式带 `$`/`$$`）；单击公式复制 LaTeX（划开选区时让路）。字号 14/15/17 作用在整个 `.workspace-grid`。显示层 `prepareMarkdown` 配对修复 `**`、拆开 `1. …；2. …`；写入层 `normalize_markdown_field` 覆盖新 Brief 与新 Lens。复制合同 [markdown-rendering.md](markdown-rendering.md) **§9**。
- Gemini Interactions 结构化输出：`response_format = { type: "text", mime_type: "application/json", schema }`，不是 OpenAI 的 `json_schema`。
- canonical Block 引用篮：跨页/非连续/排序/移除/持久化；Rust 拒绝 forged/stale ID；Message snapshot 重 OCR 后不变；Block citation 回跳 bbox。
- 翻译、解释、Formula/Figure/Table Lens、typed repair、Lens QA、阅读成果标签。
- Usage 分解、remote tombstone（列表无 LIMIT；claim 32 条一批；重登记清 attempts）、Operations Drawer、诊断 preview/export、`archive_thread`。
- 统一 `db.rs`；`WorkspaceRuntime` 一次换上；命令不再读多把模块锁；切换不 join 旧 worker。
- 流式 PDF hash/copy、OCR staging 流式落盘、Gemini orphan file tombstone、Context Root 进程锁。
- Settings 保存 Key 后保留可见成功态。
- PDF.js 和 Markdown/KaTeX lazy chunk；浏览器 memory adapter 提供空诊断投影。
- 旧 Workspace `reset_required` 前端闭环。
- Settings → Prompts：19 个生产槽位可编（含 `guide_context` / `guide_annotate`），应用账号 `prompt-settings.json`，入队冻结全文。
- 读者上下文（D-065）：工作区根 / 文件夹 / PDF sidecar md 为唯一权威；F2 任务读带标签附加段；Job 冻结；讨论现读。编辑器居中 80% 左右分栏。手册 [reader-context.md](reader-context.md)。
- Full Outline（协议 v3）：见 [agent-onboarding.md](agent-onboarding.md) §28。lazy React Flow + dagre TB；分叉出厂稿；`outline_epoch=3` 一次性清旧图；紧凑卡片；节点可拖（不持久化）；选中才出可拖详情栏；点选不自动跳 PDF；局部图点节点不得回总图。
- LaTeX / KaTeX 希腊字母加粗与控制字符容错流水线（`preprocessLaTeX`）：`\textbf{\kappa}` 自动转 `\boldsymbol{\kappa}`，修复 JSON unescape 控制字符破坏。
- 中西文排版字体独立配置与全局生效（`src/fontFamily.ts`）：西文 5 种 + 中文 4 种独立选择，CSS `--app-font` 级联，提供中英混排实时效果预览卡片。
- Library Hub 一句话核心结论（Takeaway）零延迟液态玻璃悬浮气泡，移除 `#` 标签前缀并统一排版尺寸。
- 任务与调度中心（Activity Ledger / Operations Drawer）液态玻璃全量重构：多模型彩色渐变芯片徽章、状态呼吸胶囊、所属论文标签与半透明错误告警框。
- 精读路线（Reading Roadmap / Three-Pass Coach）：基于 Three-Pass Approach 的论文特异性精读引导 Todo List，四角悬浮液态玻璃圆角卡片（`src/ReadingRoadmap.tsx`，`border-radius: 20px`，不贴底不四方），支持 **左侧屏幕边缘移入触碰自动唤出**、**移开鼠标 280ms 自动收起**、**顶栏 📌 钉住按钮固定常驻**（持久化至 `localStorage`）、多 Pass 折叠、自检问题、任务勾选持久化（`roadmap_progress` 表）、证据药丸穿透跳转、30秒电梯稿与核心图卡片。新增第 17 个生产提示词槽位 `reading_roadmap`。
- 阅读成果与 Lens 交互修复：点击 Lens 自动展开右侧栏（`open_artifacts` + `isPdfOnly=false`）、关闭 `preferBrief` 并激活展示对应 Lens 成果；生成中即刻展示选区裁剪图与「Lens 分析中...」卡片；生成完成后精准选中目标 Lens。
- 证据药丸（Evidence Pill）与跳转高亮：重构成果底栏证据为轻量胶囊（`📄 p.N ↗`），点击同时触发页码跳转与 Block 聚焦高亮（`setFocusedBlockId`）。
- 全局 Composer 液态玻璃升级与自动高度伸缩：讨论区与 Lens QA 统一采用单行/多行弹性 `<textarea>`，根据输入内容在 38px~140px 间平滑自动增高，超出上限支持垂直滚动，全面支持 Enter 发送、Shift+Enter 换行。
- Roadmap 后端外键修复：修复 `provider_node_id` 外键约束，插入 `provider_nodes` 表生成本地 UUID 后再发布 Artifact。
- Reading Guide V1（AI 好友旁批，D-040）：`reading_guide` Job、阿林/老周/小夏英文页边、240px 右侧槽、顶岛「旁批 · N」。上下文可带 PDF，**分批旁批走 `interact_text`**。合同 [reading-guide-v1.md](reading-guide-v1.md)，踩坑 [agent-onboarding.md](agent-onboarding.md) §30。
- 阅读成果空间极致收敛与导航重构（D-046）：过滤 `reading_roadmap` 无效成果；合并顶栏为 `[ 💬 讨论 ] [ 🌐 全篇成果 (N) ] [ 🔍 Lens (M) ]` 三态控制器；模型元信息与重新生成合并至顶栏第 1 行右侧，彻底消除正文上方第 3 行冗余 Header，净省 106px+ 垂直阅读空间；论证地图 Chip 仅在全篇成果中显示；`OutlinePane` 顶栏增加 `[ ‹ 返回成果 ]` 双向通道；顶栏 OCR 增加防折行并压缩悬浮岛至 38px。
- 顶栏模型徽章与讨论区净空扩展：模型徽章 `.rail-model-badge` 上移常驻于右侧顶栏 `compact-rail-right`（可直达模型设置）；讨论输入框底栏 `.composer-subline`（「就绪」与模型标签）彻底移除，讨论流视口净增 30px+；Roadmap 严格绑定阅读模式，设置/任务中心/弹窗下绝不误弹出。
- 侧边栏顶栏响应式弹性收纳、图标化与生成元信息沉底（D-048）：成果元信息下移至内容最底部（`.artifact-detail-meta-footer`），顶栏空间彻底释放；Tab 组新增 `💬` 图标并在窄屏（`< 460px`）下自适应展示纯图标 `💬`、`🌐 N`、`🔍 M`；字号调节与全屏讨论树折叠进 `···` 更多操作菜单；保持 `.compact-rail-header`、`.compact-rail-left` 与 `.thread-title-picker-wrap` 均为 `overflow: visible`，防止讨论分支下拉菜单与 `···` 更多操作菜单被裁剪。
- D-063 PR 1（schema 8 + Query foundation，后端）：`SQLITE_SCHEMA_VERSION = 8`，打开 Workspace 时 v7→v8 原子迁移并强制先做 `pre-schema-8` 备份；`library_change_seq`（全局单调游标）+ `library_domain_revisions`（8 个域）由 `bump_library_revisions` 在写事务内同笔推进，缺表 / 缺行一律 fail closed；`library_query.rs::hub_page` 一次有界聚合查询给出 keyset 游标、`queryDigest` 与依赖 revision 向量，`library_read` 命令是唯一只读 seam（判别联合请求，错误只有 `invalid_query` / `workspace_unavailable`）；前端 `libraryWorkspaceTypes.ts` + `libraryWorkspaceClient.ts`（Tauri / Memory 双 Adapter）由 `libraryWorkspaceContract.test.ts` 用**同一份 Rust 生成的 fixture 字节**钉死。
- D-063 PR 2（文库交互层，纯前端）：`src/library/` 四个纯模块 + `useLibraryWorkspace` 交互控制器；`LibraryHub` 只学 `view + dispatch`。复选框 / `Ctrl` / `Shift` / `Ctrl+A` / `Space` / `Esc` 选择集；`⋮⋮` 把手独占拖拽（点正文仍是打开 Reader）；插入线只在合法落点、非法落点 `not-allowed` + `aria-live` 原因；`Alt+↑/↓`、「移到最前 / 最后」、右键菜单与拖拽共享唯一 `planReorder`（D-062 精确置换只有一处守卫）；真实可折叠目录树按 `workspaceId + uiSchemaVersion` 记忆且跨库不串；`M` → `HubMoveDialog` 预览跨根 kind change；首次 coach mark 与设置 →「重新播放」。
- D-063 PR 3（本地 Batch + 多文件导入）：`library_act` 是唯一写 seam（`plan_batch` / `start_batch` / `control_batch`）；PatchTags / Move / Trash / Import / Export 有逐项状态、幂等 receipt、crash reconcile 与 Undo Token。Hub 用 `hub_page` 的 `selectionDigest` 填 Selection Snapshot；本地批量工具栏接通。多文件 picker 与 Webview drop overlay 走同一 Import Batch。任务中心增加批次分组。
- D-063 PR 4（Reading Lifecycle + Smart Collection）：`paper_lifecycle` / `reading_engagement` / `smart_collections` 有 writer；`library_act change` 与 `record_reader_activity`、批次 `patch_lifecycle`；`library_read` 增加 `smart_collections` / `reading_context` 与 lifecycle 筛选。Hub 侧栏六个内置智能集合 + 用户保存查询；卡片显示状态 / 收藏 / 稍后 / OCR 失败；Reader 打开记活动，末页询问已读。`save_reading_state` 仍不污染最近打开。
- D-063 PR 5（Provider 批量 + 费用预览）：JobModule `job_preparations` 的 prepared handle；`library_act` OCR / Brief 批次在同一事务 consume / enqueue / coalesce / link；`CostPreview` 区分 exact / estimate / unknown；长 PDF 与费用确认；共享 Job 唯一 created owner；取消剩余不伤害 joined 消费者。Hub 工具栏 OCR / Brief 可点，确认页展示费用。不承诺撤销费用。
- D-063 PR 6（规模、可访问性与实机门槛的代码部分）：Hub 卡片列表走 `hub_page` 分页虚拟窗口（`.hub-paper-list`）；`chapter` 进 `hub_page`（SQLite `chapter_sort_key`，`1.2` < `1.10`，必须精确 collection）；`collection_layer` 给 D-062 整层 live id；watch 先 listen 再握手并缓冲窗口事件；Explorer drop 与多文件 picker 共用 Import Batch（500 源上限）。侧栏树计数仍读 `list_documents`。实机 Explorer 拖入与 Windows P95 仍属发布门槛。
- D-063 对照计划修补（2026-09-03，不是新切片）：多选拖目录走 Move Batch；Explorer 落点用指针下的物理目录；非 PDF 原样进 Import Batch；`collection_layer` 随 revision 刷新；`undoTokenStore` + 任务中心「撤销整批」；卡片展示优先级 / 复习 / 最近打开。工作记录与踩坑：[library-workspace.md](library-workspace.md) **§7 / §11**。

## 最近验证

- 读者上下文 D-065（2026-09-07）：`npx tsc --noEmit` 0 错误；vitest `ReaderContextDialog` 2、`LibraryHub.interaction` 含右键入口、`desktopClient` memory 读写；Rust `reader_context` 8、`prompt_settings` 15、`move_and_trash_paper_take_reader_sidecar`、`library_watcher` 2。未跑全量 vitest / 全量 cargo / 实机 Explorer 孤儿 / live provider。细节 [reader-context.md](reader-context.md) §6 / §9。
- 前端（2026-09-04 选区复制 GFM）：`npx tsc -b` 0 错误；`npx vitest run --maxWorkers=1 --fileParallelism=false` **59 文件 / 423 测试**。复制锁在 `markdown.test.ts` / `MarkdownBody.test.tsx` / ArtifactPanel 跨节 copy。未跑：实机 WebView 贴到记事本 / Word（§9.9）。
- 前端（2026-09-04 把手 / 勾选加大）：`npx tsc -b` 0 错误；`LibraryHub.interaction.test.tsx` **34 通过**（选择集仍走 `getByLabelText`）。全量次数仍以最近一次完整 `npx vitest run --no-file-parallelism` 为准（上次记录 **57 文件 / 385 测试**）。
- 前端（2026-09-03）：`LibraryHub.interaction` + `importDrop` + `undoTokenStore` + `OperationsDrawer` **49** 通过（含多选拖入走 Move Batch、非 PDF 原样进批次、卡片优先级 / 复习 / 最近打开、任务中心 10 分钟撤销）。
- Rust：`cargo test --locked --lib library_query` **20 项**（含 10k 语句数仍为 11、chapter `1.2` < `1.10`、`collection_layer`、fixture 再生）通过；`chapter_sort` 2 项通过。全量 `cargo test --locked` 本轮未重跑。
- 改 Rust 命令、Gemini 组包或 Chat Completions adapter 后必须重启 `tauri dev`。Markdown 显示层改完刷新 WebView 即可看到历史 Brief/Lens，不必重生成、不必写库。
- 未跑：Windows P95 / 内存 / 装包体积、六类真实 PDF、live Gemini/Mistral/OpenAI/Grok smoke、Tauri 窗口里点设置三卡。旁批生成管线需重启 `tauri dev` 后对 45 页论文再走一遍。**实机** Windows Explorer 拖入、一次 500 文件、跨根 / 回收站 / 部分撤销 / 关闭重启仍属发布门槛（jsdom / 无 Tauri 窗口证不了）。

## 刚修过的现场 bug（不要再引入）

1. **生成完成右侧空白**：工具条有「重生成/删除」但没有节点和缩放钮。原因是 `.outline-flow` 在横向 flex 里高度为 0，被 `overflow: hidden` 裁掉。
2. **局部图点节点回到总图**：`onSelectNode` 里 `setOutlineView("overview")` + 用局部 id 拉 Deep dive 把头清掉。必须走 `planOutlineNodeSelect`。
3. **旁批生成完成阅读区全黑**（p.33/45，底栏 `Pages 31–35 kept active`）：spacer 虚拟列表把视口留在空白上。见 onboarding §6。
4. **旁批完成但看不到卡片**：`firstGuideAnchor` 为空（解析过严 / page 0 / 只有 traces）。顶岛没有「· N」就是这个。见 §30。
5. **页重叠 + 上滑弹回第 33 页**：统一 pitch 920；`initialScrollOffset` 回传触发 drift recovery。见 §6。
6. **重新生成卡在 annotating、费用不涨**：分批仍上传整本 PDF / Timeout 原样重试。旁批必须 `interact_text`。
7. **顶栏 `✓ OCR 就绪` 文字在中文字符处折行**：缺少 `white-space: nowrap; flex-shrink: 0;`。已在 `App.tsx` 显式锁定。
8. **成果面板 Header 占用 4 层高度（~170px）**：通过外层 Tab 与 Scope 扁平合并，并将模型与重生成操作移入第一行顶栏右侧，彻底消灭第 3 行，成果正文直接顶格渲染。
9. **`activeArtifact` 重复声明冲突**：`App.tsx` 顶部 state 区不要重复定义 `activeArtifact`，统一使用消息树后方声明的 memo。
10. **Lens 标签下误出现全篇论证地图 Chip**：论证地图为全篇大纲，已限定仅在 `scopeTab === "global"`（全篇成果）下渲染。
11. **Brief 有序列表挤在第一条**：模型把 `1. …；2. …；10. …` 写进同一段，CommonMark 只认行首 `1. `。显示层按终止符或连续编号拆行；不要看见 `Figure 1.` 就换行。见 [markdown-rendering.md](markdown-rendering.md) L1–L5。
12. **Lens `**监督学习 **` 星号露出来**：库内文本已是合法 `**监督学习**：`。旧 `fixCjkEmphasis` 把 `字**：` 当成开标签，在闭合 `**` 前插入空格。必须先配对再决定外侧空格。见 [markdown-rendering.md](markdown-rendering.md) B1/B4。
13. **Hub 快照被事件 `kind` 骗成陈旧**：命令按自己的语义发 `library | paper | job | generation`，而 tags / artifacts / jobs 的写同样会推进全局 revision。`watch` 若只认 `kind === "library"`，OCR 完成或改标签后缓存页永远不失效。事件一律只当提示：任何一条合法 payload 都触发一次 `watch_handshake`，失效与否只由 `library_change_seq` 判定。
14. **`jobs.state` 变了但 `jobs` 域没 bump**：`hub_page` 声明 `jobs` 依赖，卡片状态就是从 `jobs.state` 派生。凡是移动 state 的写（enqueue、claim、完成 / 失败、取消、quarantine、rebind、中断恢复、暂停落盘）必须在**同一事务**里走 `bump_jobs_state`；只有 `stage` / checkpoint 载荷和 `priority` 刻意不 bump——它们不改投影读到的列，而每次 checkpoint 都 bump 会让页面在 OCR 上报进度时反复失效。
15. **选区复制丢掉列表编号和 `$`**：旧路径 `cloneContents` + `textContent`。编号是 `::marker`；公式 annotation 是选区外的兄弟节点。见 [markdown-rendering.md](markdown-rendering.md) §9 P1/P2。
16. **`rehype-katex` `output: "html"` 让复制和单击都拿不到 LaTeX**：没有 `annotation`。必须 `htmlAndMathml`。jsdom 对 `<math>` 的修复在 `src/test/setup.ts`，不要改回纯 html。见 §9 P3/P4。
17. **Brief 跨节复制退回浏览器默认**：`onCopy` 只挂在单个 `MarkdownBody`。跨根要挂在 `.artifact-detail-body` / `.message-stream` / `.block-tab-generated`。Lens 选区 Tab 没有 `.artifact-detail-body`，跨节测用 Brief。见 §9 P5/P6。

系统 C 盘若几乎满盘，默认 Temp 在 C 会导致 linker/Vitest `ENOSPC`。进程内把 `TEMP`/`TMP` 指到 D 盘临时目录，测完删该目录。不要删 Workspace、源码或用户文件。

## 必做下一步

1. 重启 `tauri dev` 后实机确认 PDF 两指缩放与两指滚动互不抢手势。
2. 实装/升级/卸载 MSI/NSIS，并确认签名策略。
3. 用六类真实 PDF 完成 Reader P95、长文内存、OCR/Lens 定位与强杀恢复验收。
4. 仅在显式付费开关下运行 Gemini/Mistral live smoke，并验证远端 file/cache TTL。
5. 托盘三分支需要桌面人工点击验收。
6. 使用专门的可丢弃 V0.1 fixture 点两阶段重置；不要用用户真实 Workspace。
7. 重启 `tauri dev` 后用已 OCR 论文点「地图」：打开工作区会清 v2 旧图；重新生成后应看到分叉或单链、细标签、选中出详情、局部图点节点仍留在局部图。live Gemini 需显式付费开关。提示词改完再入队才生效。
8. 有 Key 时实机走一遍 Settings → Models：存 OpenAI-compatible 不得把 Gemini 踢成非当前；探针失败的 Grok 不能「设为当前」；切当前要确认；下一轮 Chat 才换家。
9. 重启 `tauri dev` 后对已 OCR 长文点「旁批」：顶岛出现 `· N`、卡片钉在页右、页与页不重叠、从第一条所在页上滑不会弹回。卡住 annotating 先看是不是又给分批贴了整本 PDF。
10. D-063 PR 2 实机验收（jsdom 证不了的部分）：真实鼠标拖把手时把手只在 hover/focus 出现且不抖动布局、拖拽中途快速划过多个落点后松手落在**最后看到的高亮**上、焦点环在键盘操作时始终可见、屏幕阅读器念得出无效落点原因、目录树收起后换 Workspace 不串状态、设置 →「重新播放」能再次弹出导览。
11. D-063 PR 1 实机验收（代码证据替代不了）：拿一个**已有 v7 Workspace** 启动，确认 `PRAGMA user_version` 变为 8、`library_change_seq` 有唯一的 `1 → 0` 行、`library_domain_revisions` 有 8 行、`pre-schema-8` 备份确实落在备份目录；随后导入 / 移动 / 改标签 / 跑一次 OCR，确认 Hub 卡片状态跟着刷新（靠握手，不靠事件 `kind`），并且旧 Workspace 的 Paper、排序与成果一个都没少。
12. D-063 PR 3 实机验收：多选打标签 / 跨根移动 / 回收站确认 / 一次导入多个 PDF / 从 Explorer 拖入；部分失败 Toast 不得显示全成功；任务中心能看到批次并在 10 分钟窗口内撤销；重启后 `recent_batches` 仍能列出 interrupted 批次。
13. D-063 PR 4 实机验收：打开一篇未读 PDF 应变为阅读中且 furthest page 不因回翻降低；显式已读后重开第一页仍是已读；新 PDF revision 进度从未知开始；末页出现「标记已读」询问而不是 95% 自动完成；侧栏六个内置智能集合可点且不能手排 / 不能当拖放目标；保存 / 重命名 / 删除用户集合不复制 Paper。
14. D-063 PR 5 实机验收：选多篇点 OCR / Brief，确认页看到费用或 unknown（不得为 0）；无 Mistral Key 时 OCR 失败 closed；已有 OCR 的篇被跳过；任务中心能看到 queued Job 与批次；取消剩余不得取消别人正在用的共享 Job。
15. D-063 PR 6 实机验收：1,000 Paper Hub 滚动应只挂窗口附近的卡片；从 Explorer 拖入 PDF 与多文件 picker 产生同一 Import Batch；一次超过 500 个源被拒绝并提示分批；键盘-only 能全选 / 移动焦点 / Esc；屏幕阅读器能听到加载状态、option 的位置和无效落点原因。无 Tauri 窗口时这些项不能勾掉。
16. 选区复制实机：[markdown-rendering.md](markdown-rendering.md) **§9.9**。Brief 列表、讨论公式、跨节 `###`、composer 不受影响。jsdom 证不了 Word 只吃到 `text/plain`。

## 已锁定产品约束

仅原生 PDF；OCR 用户主动触发；OCR 后只操作 Block；Formula/Figure/Table 用 Lens；翻译不发送 PDF；解释/Lens 从论文根旁支；Lens QA 不污染 Discussion；普通成果只读，仅术语/符号 override；论文 LLM 允许 `gemini` / `openai_compatible` / `grok`，OCR 仍只 Mistral；不做搜索、Semantic View、Author Research、多文档或旧数据迁移；未知 usage 保持 null。Outline 合同见 [full-outline-v1.md](full-outline-v1.md) 与 D-031 / D-033 / D-034。Reading Guide 合同见 [reading-guide-v1.md](reading-guide-v1.md) 与 D-040。
