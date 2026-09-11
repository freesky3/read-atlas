# 功能规格

## Workspace 与文库

用户选择单一 Workspace。新目录初始化 `Papers/`、`Textbooks/`、`.read-desktop/`、SQLite WAL 和独占锁。旧 V2 工作区打开时静默补 `Textbooks/`，相对路径迁到 Workspace 相对（schema 5）。旧数据库/`library/` 返回 `available=false, statusDetail=reset_required`，不得成为 active Workspace；应用自动打开 Workspace 设置、锁定导入，并通过“查看删除范围 → 明确确认”的两阶段操作调用 `reset_workspace_v2`。重置只影响 Workspace 内旧数据，不清除应用设置或 Credential Manager。

Collection 任意嵌套，应用移动/重命名和 Explorer 变化都通过 journal + watcher reconciliation 同步。重复 hash、目标占用或身份不明生成冲突，不自动覆盖。导入失败除底部状态栏外还必须在文库区域显示 `role=alert` 的可见反馈。空文件夹为一等公民：`collections` 含空目录，Hub 树由 `list_collections` 驱动；Hub 右键菜单（论文/子目录/根/全部空白分别对应不同项，快捷键 Enter/F2/Delete）、内联改名（目录名与 PDF 词干校验：禁 `/\:*?"<>|`、`.`/`..`、尾空格/点、Windows 保留名；PDF 去扩展名后与 `title` 同步）、同根拖拽移动（8px 阈值、合法落点为同根文件夹与当前目录空白、跨根/自身/子孙/全部文档禁止）与文件夹废纸篓（级联 `trash_paper`、空目录立即删除、确认显示目录名+N、8s Toast 撤销仅最后一次）已落地；Hub 禁止跨根移动与 Explorer 跨根走 watcher+kindChange。PDF 重命名仅改词干，标题同步；`open_resource_dir` 对文件用 `explorer /select`、对目录直接打开。

同路径 PDF 内容变化创建新 Document Revision，原 OCR/Artifact/Discussion 保留为历史。

## Reader

PDF.js worker 只保留当前页 ±2。未挂载页用 spacer 维持滚动高度；当前页已在视口内时不 `scrollIntoView`。缩放、旋转或文档切换取消旧 render；不创建 Text Layer。OCR 前只有原 PDF 和基本阅读控件。Reader 状态每 500 ms 保存 page/offset/zoom/rotation/right tab/active Artifact/Discussion draft/quote basket。

缩放范围 60–180%。工具栏 ±10%、触控板捏合、触摸双指都只改变 PDF zoom，不改变 WebView 整页缩放。阅读区与讨论区边界可拖，宽度 300–760。

PDF.js 失败显示“系统 PDF 应用打开”操作，不使用 iframe 回退。

## OCR

OCR 必须由用户点击。流水线：上传 → signed URL → Mistral 整篇 OCR → raw JSON staging → 逐页规范化/校验 → 原子发布 → 删除远端临时文件。请求固定 `include_blocks=true`、HTML table、关闭 image base64；只有 Markdown、没有定位 Blocks 的响应失败。bbox 接受 `[x0,y0,x1,y1]` 或 OCR 4 的 `top_left_x/y` + `bottom_right_x/y`。

崩溃后存在 raw staging 时从 staging 继续规范化，不重复付费。重 OCR 只对唯一高置信候选迁移锚点：同页同类型、digest 相同且 IoU ≥ 0.5，或文本相似度 ≥ 0.9 且 IoU ≥ 0.8。

## Block 操作矩阵

| Block            | 引用 | 翻译 | 解释 | Lens |
| ---------------- | ---: | ---: | ---: | ---: |
| 普通文本         |   是 |   是 |   是 |   否 |
| Formula/Equation |   是 |   否 |   否 |   是 |
| Figure/Image     |   是 |   否 |   否 |   是 |
| Table            |   是 |   否 |   否 |   是 |

引用篮跨页、非连续、可排序/移除并持久化。发送后清空当前篮，但 user Message 显示永久快照。翻译/解释原子发布、不可追问；打开同一 Block 类型的新结果会移动 head，旧 revision 仍可审计。

Reader 里悬停 Block 只出浅绿预览，不出现工具条。单击选中后浅绿保持、右上角引用/翻译/解释/Lens 钉住；再点同一块取消，点另一块切换。选中 ≠ 引用。点 PDF 空白或 Esc 取消选中。滚动、翻页、点讨论区、点工具条不取消。从引用篮或对话引用跳到 Block 视为选中。chrome（页眉/页脚/参考文献）可点，但与正文重叠时点击归正文；整页 hover 不要把 chrome 变成热区。工具条会被页边裁掉时翻到 Block 下方。

## Orientation 与 metadata

首次初始化从论文根一次生成 Brief、Glossary、Symbol Table、Metadata，四个 Artifact 在一个 batch 中原子发布。metadata 不可编辑；Glossary/Symbol Table 只允许字段 override。模型切换复用本地成果，新建 context epoch，不自动重算。

打开没有 Brief 的论文应显示生成入口，不静默调用付费模型。顶栏「速览 Brief」只打开右侧「阅读成果」并定位 Brief；没有独立 Brief 弹窗。即使已有翻译/解释/Lens，缺失 Brief 时索引顶部仍要有「生成 Orientation Pack」。重新生成只出现在打开 Brief 之后的成果详情里，不进索引顶栏。

## Discussion

支持多 Discussion、多轮、从任意消息分支、编辑/重生成产生的新分支、模型 epoch 分界和全屏 Conversation Tree。树由真实 `parentId` 计算，失败节点不出现。

聊天流只显示当前路径。气泡上不再有「设为当前分支」；切路径只在对话树：点一轮问答卡片即载入该支，树先不关。树节点绑死为一问一答。重新生成 = 新的兄弟轮（复制问句 + 新回答），不覆盖旧轮。删除一轮会连子孙一起硬删，二次确认；当前路径被删则 head 退回父轮。生成中的轮次不能删。没有消息回收站。

标签 `×` = 关闭：有用户消息则归档，空对话直接丢掉；最后一条不能关。已关闭列表只列本篇论文，可恢复或二次确认后硬删。第一条用户消息自动当标题（占位标题：`新对话` / `New exploration` / `Main discussion`）；双击或右键可改名。活动标签横滑，不要 `slice(0, 3)`。

气泡按 Markdown + KaTeX 渲染；行内 `$...$`、行间 `$$...$$`。选中复制或单击公式复制 LaTeX。`[p.N]` / block 引用跳到 Reader。助手气泡标题必须用该条 `usage.model`（没有回执时才用当前 `paperModel`），禁止写死 Gemini 2.5 Flash。用量条默认收起。正文字号 14/15/17（`A−`/`A+`），讨论与阅读成果共用，PDF 不跟着变。

Chat 流式保存；Stop 保留 partial。provider 失败删除该 streaming assistant，不把 failed 行留给上下文；下一次成功顶掉失败态。provider ID 失效用本地 PDF + 当前消息路径重建；超限使用独立计费的审计压缩 Artifact。Citation 只来自合法 provider page marker 或本轮 canonical Block marker。

## 阅读成果与 Lens

右侧默认仍是互斥的“讨论 / 阅读成果”。Brief、翻译、解释、Lens 共用成果索引和单详情视图。所有成果正文经 `MarkdownBody` / `prepareMarkdown` 渲染：配对修复 `**`、拆开挤在一段里的 `1. …；2. …`，不写回 SQLite。新生成的 Lens / Brief 另走写入层 `normalize_markdown_field`。合同 [markdown-rendering.md](markdown-rendering.md)；接线见 [agent-onboarding.md](agent-onboarding.md) **§44**。归一化不会把 JSON 通道里的 `?` / `�` 还原成中文标点。

## Full Outline

顶岛「地图」把工作台切到预设 `pdf_outline`（PDF | 地图），不是第三永久标签。必须已发布 OCR。生成合同、两层地图、可编辑提示词、删除/重生成、授权见 [full-outline-v1.md](full-outline-v1.md)。点选节点只开右侧详情；跳 PDF 走详情里的「跳到原文」。成果索引若列出 Outline，必须打开同一表面。

论证角色按种类分叉：论文 `NODE_ROLES` 不变；教材用教学角色集（concept_intro / definition / example_illustration / derivation / algorithm_procedure / exercise_practice / caution_pitfall / application_example / summary_recap），未知角色归一为 `other` 并给出覆盖率警告。教材三份 outline factory（Extract / Compose / DeepDive）为教学向，论文 factory 不变；不 bump outline epoch 清论文地图。

Formula/Figure/Table Lens 输入完整 PDF、OCR 对象与 crop。Figure/Table 保存高分辨率 display crop，模型使用独立成本优化 crop。Lens 初始结果严格 typed；失败最多一次 repair。Lens QA 从 Lens node 续接，不进入 Chat。成功重生成保留旧 Lens revision、切换 head、删除旧 QA；失败保留全部。

## Job、空间与删除

Job 状态/attempt/checkpoint 持久化。每 provider 最多两个付费请求；同论文 OCR 串行；同 root/Artifact 请求合并。已提交但结果未知标为 `interrupted_unknown`，不自动重复计费。任务中心支持暂停 queued、恢复、取消和 queued/paused priority。

Workspace 总量是真实磁盘占用；论文与 Artifact 显示逻辑占用。Paper 删除进入 30 天回收站，立即停止任务并登记 Gemini file/interaction tombstone；恢复冲突不覆盖。Mistral 临时文件在 OCR 结束即清理。

## 导出

阅读工作台顶栏「导出」把当前 PDF 的阅读成果导出为单个 UTF-8 Markdown，路径 `<Workspace>/export/<PDF 名不含扩展名>.md`。写入文头（标题 / 种类 / 相对路径 / 页数 / SHA-256 / 时间）、Brief、Metadata、术语表、符号表、地图文字大纲、Lens 文字与页码、讨论当前路径；不写 OCR、图片、翻译、单块解释、精读、旁批、阅读进度、Job、费用、bbox JSON。同名冲突：先写 `<stem>.md`，若该文件属于不同 SHA-256 则写 `<父文件夹>_<stem>.md`；同一 SHA 再导出覆盖。导出目录懒创建，永不入库。

## Hub 排序（manual / chapter）—— D-062 UI 规格

> 上手与踩坑：[hub-sort.md](hub-sort.md)。权威 ADR：[decisions.md D-062](decisions.md)；数据合同 [data-protocols.md](data-protocols.md)。

Hub 顶栏排序下拉五项：导入时间 / 出版年份 / 论文标题 / 手动 / 按章节。默认 `recent`（导入时间）。

### 状态矩阵

| 选中位置 | `recent/year/title` | `manual` | `chapter` | 下拉提示 |
|----------|---------------------|----------|-----------|----------|
| `全部` | 可用 | 禁用（灰） | 禁用（灰） | `select[title]="点进该书文件夹才能手排/按章节"` |
| 叶子文件夹（无子孙 PDF）且本层有教材 `chapterNumber` | 可用 | 可用 | 可用 | `title="手动 / 按章节可用；搜索时禁止拖拽改序"` |
| 叶子文件夹但无可解析 `chapterNumber` | 可用 | 可用 | 禁用 | `title="手动可用；按章节需本层至少 1 份教材 Metadata chapterNumber"` |
| 父文件夹（列表混进子孙 PDF） | 可用 | 禁用 | 禁用 | 同「全部」 |
| 任意位置 + 搜索激活 | 可用 | 展示已存顺序的可见项，**禁拖拽（不出插入线、不写盘）** | 对可见结果排序 | 同左 |

不支持时画面回落到 `recent`，**不把回退写成偏好**（`collection_sort_prefs` 不变）。

### 手排（手动）

- 拖拽：网格在卡片左右出虚线、松在网格空白插末尾；表格在行顶/底出虚线、松在表空白插末尾。
- 写入：`reorder_collection_papers(collectionId, paperIds)` 要求当层全部 live 的精确置换；顺序未变 no-op；首次进入 `manual` 不写库，仅真正换序才写。
- 显示：无存档时与「导入时间」相同（`importedAt` **降序**，新的在前）；有存档后，缺席 live paper 按 `importedAt` **升序**接末尾。
- 搬家：侧栏文件夹热区仍是跨文件夹移动，清掉插入线。

### 章节（按章节）

- 纯视图，不写 `collection_paper_order`。有 `libraryClient` 时由 `hub_page` `sort=chapter` 排序，与 SQLite `chapter_sort_key` 同一算法。
- 只信教材 Metadata artifact 的 `chapterNumber`（不是独立列），`/[.\-]/` 分段整数比较（`1.2` < `1.10`），不可解析的排在可解析之后。
- 本层至少一份可解析 `chapterNumber` 才启用 `chapter`。`hub_page` 的 chapter/manual 必须带精确 collection 过滤。

### Hub 规模（D-063 PR 6）

- 卡片列表走 `hub_page`（默认 100 / 上限 200），超过 48 项且视口高度已知时只渲染窗口附近的卡片/行；`aria-setsize` / `aria-posinset` 按命中篇数而不是窗口长度。
- Shift 使用当前已加载页的稳定顺序；Ctrl+A 是 `all_matching`，计数用 `hub_page.totalCount`。
- 手排提交用 `collection_layer` 的整层 live id，不拿分页窗口当置换。
- 多文件 picker 与 Explorer drop 都产生同一条 Import Batch；一次最多 500 个 PDF；智能集合与「全部」不是导入落点。

### 一致性与边界

| 场景 | 期望 |
|------|------|
| 同文件夹 F2 改名 / 同 `collection_id` 的 `move_paper` / reconcile 同层改名 | 手排行不变 |
| 跨文件夹 / 跨根移动 | 旧 `collection_id` 的 order 行被删 |
| `trash_paper` | 删行 |
| `restore_paper` | 不恢复旧 `position`，按 `importedAt` 补位 |
| 新建 / 挪入论文 | 不入 order 表，尾部按 `importedAt` 升序补位 |
| 搜索过滤 | `manual` 禁拖拽写盘；`chapter` 对可见结果排序 |

### 扩展（新增排序模式）

步骤清单见 [hub-sort.md §6](hub-sort.md)。改完后同步本节状态矩阵与 [data-protocols.md](data-protocols.md) 白名单。
