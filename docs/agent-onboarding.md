# Agent onboarding（V2 基线之后）

Lens 追问已采用论文／教材两份完整中文稿，仍为 v1 两字段，追问不会写回原 Lens。修改相关代码请先读 [Lens 追问合同](lens-qa-generation.md)。

论文 Lens 六份提示词及输出字段已升级，动相关代码先读 [Lens 生成合同](lens-generation.md)，完整正文见 [定稿](note/lens-paper-prompt.md)。教材与 Lens QA 未在本轮改稿。

下一个 agent 从这里开始。先读本页、[handoff.md](handoff.md) 和 **[backend-hardening.md](backend-hardening.md)**，再改代码。产品合同仍以 [v2-product-spec-2026-08.md](v2-product-spec-2026-08.md) 为准。动 `src-tauri` 工作区/SQLite/worker 前必须读 hardening 页，不要靠猜 `lib.rs`。动设置 / 论文模型 / Chat Completions 前先读本页 **§29**。动旁批读 **§30** 和 [reading-guide-v1.md](reading-guide-v1.md)。动 PDF 滚动 / 虚拟列表读 **§6**——spacer 方案已经作废。动成果/讨论 Markdown（挤在一起的列表、双星号粗体）读 **§44** 和 [markdown-rendering.md](markdown-rendering.md)。动选区 `Ctrl+C` / 公式复制读 **§48** 与 [markdown-rendering.md](markdown-rendering.md) **§9**。动 Hub 排序读 [hub-sort.md](hub-sort.md) 与本页 **§45**。Windows 上 `tauri dev` 编不过读 [windows-dev-build.md](windows-dev-build.md)。动文库 Hub / `library_read` / `library_act` / Batch / 虚拟列表读本页 **§47** 和 [library-workspace.md](library-workspace.md)。动读者上下文读本页 **§50** 和 [reader-context.md](reader-context.md)。

工作树基线提交是 `7883e80 feat: establish Read Desktop V2 baseline`。其后未提交的实现已收成一次增量：Chat/Gemini/OCR 合同、Markdown 讨论树、失败轮次不落库、阅读器滚动与双指缩放。

## 先不要做的事

- 不要实现搜索、Semantic View、Author Research、多文档标签。Reading Guide 合同见 **§30** 和 [reading-guide-v1.md](reading-guide-v1.md)；不要做独立 Block View、句子级高亮、请离场、批注内聊天。不要复用 `reading_roadmap` 的 slot / Job / UI。不要把旁批做成第三工作台。
- 文库现在有 `Papers/` 与 `Textbooks/` 两根。相对路径是 **Workspace 相对**（`Papers/Inbox/a.pdf`），种类由第一段推导。不要再 `join("Papers").join(relative)`。不要把 `export/` 当文库。旧路径迁移在 schema 5，不要 `reset_required`。跨根改种类必须走「移动到……」预览（D-055）；不要静默摘 head。
- 提示词 `prompt-settings.json` 是 schema 2：每槽 `paper` + `textbook`。保存/恢复必须带 `kind`。`load_prompt_text` 按文档种类取。教材 factory 目前仍复制论文稿（D-054）。
- 读者上下文（D-065）是磁盘 md，不是系统提示词，不要写进 19 槽、不要进 SQLite、不要 recursive 监视 Workspace 根。IPC 是 `get_reader_context`，不要和 `get_reading_context` 搞混。包装语只进 user_input。编辑器不要套 `.scrim`。完整合同与踩坑：[reader-context.md](reader-context.md)。
- 不要把 PDF 虚拟列表改回前后 spacer，不要用 `scrollIntoView` 追页，不要把 `initialScrollOffset` 放进“校正滚动”的 effect 依赖。现场已经因此黑屏、叠页、上滑弹回第 N 页。
- 论文 LLM provider 为 Gemini、OpenAI-compatible、Grok；论文根仍要求整本 PDF 进上下文。
- Full Outline **代码已落地**。先读本页 **§28** 和 [full-outline-v1.md](full-outline-v1.md)。不要做成目录、Brief 加长版、第三种聊天或通用窗口管理器。不要重做 Job / 提示词栏 / 删除重生成，除非产品改口。
- 不要删除 `workspace.lock` 的 `File` 句柄（`v2_workspace.rs` 里那个字段看起来 unused，是用来持有独占锁的）。
- 不要为了消 unused 警告去删“好像没用”的 API。`workspace.lock` 的 `File` 必须留着。`AppState` 不要加回多把模块锁。
- 不要给 `list_remote_tombstones` 加 `LIMIT`，不要把 Discussion 流式通道改成丢增量，不要 `join` 旧 worker。详见 [backend-hardening.md](backend-hardening.md)。
- 不要把 `zoomHotkeysEnabled` 改回 `true` 来“修复缩放”。那会缩放整个 WebView（库、对话、工具栏一起变大）。
- 不要给 WebView2 加 `--disable-pinch`。手势会被彻底丢掉，JS 也收不到。
- 不要扁平调用 `invoke("send_chat", { revisionId, ... })`。Tauri 2 对 Rust 结构体参数要求再包一层，见下。
- 不要把失败的 assistant 消息写成 `status=failed` 留在树上当上下文。失败轮次要删掉。
- 不要对用户 Workspace、Credential Manager 或 `tmp/` 做破坏性清理来腾磁盘。不要把「修好的」markdown 写回 `<Workspace>/.read-desktop` 或任何用户工作区 SQLite。

## 改代码时从哪进

| 要动的事 | 先看 |
| --- | --- |
| 打开 / 切换 Workspace | `src-tauri/src/workspace_lifecycle.rs`；命令只读 `current_runtime()` |
| SQLite 打开 | `src-tauri/src/db.rs` |
| 发 Chat / 编辑 / 重生成 | `src/sendChat.ts` → `src-tauri/src/lib.rs` `send_chat` / `ChatRequest` |
| 当前论文 provider / 设置模型页 | 本页 **§29**；`model_settings.rs`、`chat_completions.rs`、`SettingsWorkbench.tsx` |
| Gemini 请求体、流式文本、文件 URI | `src-tauri/src/provider_ports.rs` `body` / `stream_body` / 文本解析 |
| OpenAI-compatible / Grok Chat Completions | `src-tauri/src/chat_completions.rs`；工厂 `lib.rs` `open_paper_adapter` |
| Mistral OCR bbox | `src-tauri/src/provider_ports.rs` bbox 规范化（数组或 `top_left_*` / `bottom_right_*`） |
| 讨论树、一轮一节点、删子树 | `src/discussionTree.ts` `layoutDiscussionTurns`、`src/ConversationTree.tsx`、`lib.rs` `delete_discussion_turn` |
| 关闭/恢复/删除/改名对话 | `src/components/ThreadTabs.tsx`、`src/threadTitle.ts`、`lib.rs` `close_thread` / `restore_thread` / `delete_thread` / `rename_thread` |
| 聊天气泡 Markdown / 选区复制 GFM / 公式单击复制 / `[p.N]` | `src/markdown.ts`、`src/MarkdownBody.tsx`；合同 [markdown-rendering.md](markdown-rendering.md) **§9** |
| 成果/讨论列表换行、双星号粗体 | [markdown-rendering.md](markdown-rendering.md)；本页 **§18** 公式、**§44** 列表与粗体 |
| Block 单击选中与工具条 | `src/PdfReader.tsx`、`.ocr-block-hit` / `.ocr-block-actions` |
| Brief 入口、成果生成/重生成 | `src/App.tsx` `openBrief`、`src/ArtifactPanel.tsx` |
| 讨论/成果字号 | `src/discussionFont.ts`、`src/components/FontSizeStepper.tsx`、`.workspace-grid[data-discussion-font]` |
| 气泡上的模型名 | `src/modelLabel.ts` + `message.usage.model`，禁止写死 2.5 Flash |
| Gemini 结构化输出 | `src-tauri/src/provider_ports.rs` `gemini_response_format` |
| PDF 滚动跳动、虚拟页窗 | `src/PdfReader.tsx` 绝对定位槽 + 实测画布高度；**不要** spacer / `scrollIntoView` / 用 `initialScrollOffset` 回吸 |
| 双指缩放 | `src/readerZoom.ts`、`src/PdfReader.tsx`、`src-tauri/src/webview_pinch.rs` |
| 阅读区/对话分割条 | `src/App.tsx` `chatWidth` + `.workspace-split` |
| 保存 Key 的可见反馈 | `src/SettingsWorkbench.tsx` `savedNotice` |
| OCR staging 复用、归档对话 | `lib.rs` `reusable_ocr_staging`、`archive_thread` |
| 前端唯一 IPC 入口 | `src/desktopClient.ts`（浏览器走 memory adapter） |
| Full Outline 地图 | 本页 **§28**；合同 [full-outline-v1.md](full-outline-v1.md)；`outline_module.rs` / `outline_protocol.rs` / `outline_validate.rs` / `prompt_settings.rs` / `src/outline/` |
| Reading Guide 旁批 | 本页 **§30**；合同 [reading-guide-v1.md](reading-guide-v1.md)；`guide_*.rs`、`src/guide/`、`PdfReader.tsx` 页边槽 |
| 精读路线（Reading Roadmap） | 本页 **§32**；`ReadingRoadmap.tsx` / `roadmap_module.rs` / `lib.rs` / `types.ts` |
| Hub 排序（manual/chapter） | [hub-sort.md](hub-sort.md)；本页 **§45**；`hubSort.ts` + `paper_module.rs` + `LibraryHub.tsx` |
| Windows `tauri dev` 编译 OOM / 栈溢出 | [windows-dev-build.md](windows-dev-build.md)；本页 **§46** |
| 文库 Workspace（Hub 卡片、Batch、Lifecycle、Smart Collection、`hub_page`） | [library-workspace.md](library-workspace.md)；本页 **§47**；计划 [library-workspace-plan-2026-08.md](library-workspace-plan-2026-08.md) §11.0 |
| 读者上下文（全部文档 / 文件夹 / PDF 笔记） | 本页 **§50**；手册 [reader-context.md](reader-context.md)；ADR D-065 |

## 这轮增量解决了什么

### 1. `send_chat` missing required key request

Tauri 2 把命令参数按结构体字段名反序列化。`send_chat(request: ChatRequest)` 需要：

```ts
invoke("send_chat", { request: { revisionId, threadId, parentId, question, page, blockIds, regenerateFromId } })
```

扁平传字段会得到 `invalid args ... missing required key request`。组装逻辑只放在 `buildSendChatInvokeArgs`，测试锁住这个形状。

### 2. 空的失败气泡 + Gemini 400

Gemini Interactions 现在要扁平 input：`type: "text" | "document" | "image"`，不要 `input_text` / `input_file` / `turn_list`。文档附件用 `document.uri` + `mime_type`。流式文本从 `steps[].model_output` / `delta.type === "text"` 取，不要假定旧的 `candidates[0].content.parts`。

流式时不要用过期 snapshot 覆盖正在写的 assistant 行，否则气泡会被冲成空的再标 failed。

### 3. Mistral OCR “page 1 Block 0 has no bbox”

OCR 4 的 bbox 经常是对象：`top_left_x/y` + `bottom_right_x/y`，不是 `[x0,y0,x1,y1]`。规范化必须两种都收；值在 0–1 时乘 1000。没有定位信息的 block 才失败。

崩溃后若 revision 上已有 raw staging，`reusable_ocr_staging` 接着规范化，不重新付费。注意 staging 的外键：先 fail 掉 running job 再插新行，顺序反了会 FK 失败。

### 4. Discussion 要真树，而且树是「一轮 = 一问一答」

2026-09-09：讨论与压缩的四份完整中文稿、三字段恢复、消息完成状态及旧默认稿迁移，见 [学术讨论与压缩合同](discussion-generation.md)。讨论默认稿不再追加英文 F2。原有消息树与重生成 sibling user turn 机制保留。

`parentId` 是事实。聊天流只用 `layoutDiscussionTree` + `visibleDiscussionNodes`（当前路径）。对话树 UI 用 `layoutDiscussionTurns`：一张卡片绑死 user + 其最新 assistant。

点卡片 = `set_active_branch` 到该轮的回答（没有回答则到问句），**不要关树**。气泡上不要「Set current branch」。不要在聊天里做 `< 1/2 >` 切兄弟分支。

重新生成必须 `insert_user: true`：复制原问句、同一 `user_parent_id`、新 user id，长出一轮兄弟。旧实现 `insert_user: false` 会把多条回答挂在同一个 user 下，和「一问一答」冲突。`prepare_chat_turn` 的测试锁的是 sibling user。

删除走 `delete_discussion_turn`：从该轮 user 收集整棵子树后 `DELETE`。`parent_id` 是 `ON DELETE SET NULL`，只删中间一条会把子孙变成新根。生成中禁止删。当前 head 落在被删集合里则退回到该轮 user 的 parent（上一轮回答）。没有消息回收站。

`generate_brief` 入队 Orientation Pack Job，不要在 UI 线程同步烧一篇 Brief。

### 5. failed 轮次不记账

产品要求：失败回答不进持久化上下文、不进树、界面上被下一次成功顶掉。

实现：`finish_discussion_with_error` 对非 cancel **删除** streaming assistant（cancel 才保留 partial）。`list_messages` 带 `status != 'failed'`。前端树再滤一遍 failed。发布成功时清掉同级残留 failed。错误只通过 generation delta 告诉 UI，不要留下空 failed 气泡。

### 6. 阅读器虚拟列表（2026-08-22 已改，旧 spacer 方案禁止再引入）

仍只挂 `currentPage ± 2`（`pageWindow`）。**不要**再用前后 spacer 撑滚动高度。spacer 在拖滚动条时把视口留在空白上（黑屏，底栏仍写 `Pages 31–35 kept active`）；为防抖动再跳过 `scrollIntoView` 会让黑屏卡住。

现行做法（`src/PdfReader.tsx`）：

- 每页一个 `position: absolute` 的 `.pdf-reader-slot`。`top` 用 **累计页高**（`scrollOffsetForPage(..., heights)`），不是统一 920。
- 画布真正 `setPageSize` 之后由 `onPageExtent` 上报高度。禁止用 `.pdf-reader-page` 的 `min-height: 220` 去估 `pitch`——量早了会得到 248，页会叠在一起。
- 未量到的页用已量页的平均 stride（页高 + `READER_PAGE_GAP` 28）占位。
- `onScroll` 用 `pageFromScrollOffset` 更新 `viewPage` 并 `onVisiblePage`。跳到未挂载区域也会立刻换窗。
- **只有外部翻页**（工具栏页码、生成完成后跳第一条旁批）才写 `scroller.scrollTop`。用户滚动、父组件把 `pageOffset` 回传成 `initialScrollOffset`，都不得回吸。
- 页边槽高度锁在画布高度内，卡片多了栏内滚，不得撑高这一行（否则行高 ≠ stride，又叠页）。

`App.tsx` 的 `onScrollOffset` 会 `setPageOffset`，于是 `initialScrollOffset={pageOffset || restoreOffset}` **每滚一下都变**。曾经把它放进校正 effect 的 deps，再加“偏离当前页 2.5×pitch 就吸回去”，加上 `programmaticScroll` 200ms 内不报页码：用户上滑 → offset 回传 → effect 再跑 → 吸回生成完成跳转的那一页（现场是第 33 页）→ 永远弹回。**不要再引入 drift recovery。**

### 7. 两指缩放（Windows 上最容易再踩的坑）

用户要的是 **只放大 PDF**（张开变大、捏合变小），不是整窗浏览器缩放。

wry 0.55 把 `zoomHotkeysEnabled` 同时写到：

- `IsZoomControlEnabled`
- `ICoreWebView2Settings5::IsPinchZoomEnabled`

配置里必须保持 `zoomHotkeysEnabled: false`，否则 Ctrl+/- 和捏合会缩放整个 chrome。但这也会让 WebView2 **直接吞掉捏合**，页面收不到 `wheel + ctrlKey`。

正确拆法：

1. 配置继续 `false`。
2. `webview_pinch::install` 在 setup 里用 COM **单独** `SetIsPinchZoomEnabled(true)`。
3. 监听 `ZoomFactorChanged`：读到 ≠ 1 就 `SetZoomFactor(1.0)`，把 factor 以事件 `webview-pinch-zoom` 发给前端。
4. `readerZoom.ts` / `PdfReader` 把 factor 乘进 PDF zoom（60–180%）。
5. 同时听 `wheel`（必须 `ctrlKey`/`metaKey`，避免偷两指滚动）、双指针、`touch*`、Safari `gesture*`。
6. 用 `clientX/Y` 判断是否在阅读器上。WebView2 的 pinch `wheel.target` 经常是 `document`，`contains(target)` 会误判。
7. **累计浮点 zoom，只在 publish 时 `Math.round`**。每个 tick 都 round 的话，`deltaY = 0.3` 会永远停在 100%。
8. 不要因为 `pointerType === "mouse"` 就丢弃。部分精密封控板报 mouse。
9. `.pdf-reader { touch-action: none }` 让触摸屏第二指能进 Pointer/Touch 事件；触控板两指滑动仍是 `wheel`，滚动不受影响。

改了 `webview_pinch.rs` 或 `lib.rs` setup 后必须 **重启 `tauri dev`**。热更新前端不够。

前端热路径测不了 COM。锁住的是：`ctrl+wheel`、target=document 但坐标在阅读器内、小增量累计、双 mouse pointer、`webview-pinch-zoom` CustomEvent。

### 8. 分割条和 Key 保存反馈

阅读区与对话之间的 8px `.workspace-split` 拖宽度，范围 300–760，写入 `localStorage["read-desktop.chatWidth"]`。中央带有显式磨砂抓手药丸，鼠标悬停时点亮为科技蓝。

Settings 保存 Gemini/Mistral key 后必须留在可见成功态（`savedNotice` + “All changes saved”）。不要保存完立刻把输入清空成“没发生过”的样子。

### 9. 双阶段流转体系（Library Hub vs Focus Reader）

- **冷启动与导航**：冷启动第一入口为 `LibraryHub.tsx`（文献大厅）。点击任意论文进入 `reader` 模式。
- **返回大厅**：在阅读模式下，顶部悬浮岛带有 `‹ 返回文库 (Esc)` 按钮；全局监听了键盘 `Esc` 键（非输入框聚焦时），直接退回文献大厅。
- **全屏纯 PDF 模式**：`Ctrl+\` / `Ctrl+B` 或点击 `⤢ 仅看 PDF` 按钮可一键折叠右侧研讨区，将整屏 100% 宽度给 PDF。

### 10. 克制现代 Liquid Glass 设计语言与 Theme 引擎

- **主题系统**：
  - `liquid-light`：清爽现代学术浅色（纯净 Slate/White、多层毛玻璃 `backdrop-filter: blur(24px)`、1px 镜面内高光 `inset 0 1px 1.5px rgba(255,255,255,0.95)`）。
  - `liquid-dark`：深邃沉浸学术暗色（黑曜石/Deep Slate 背景与微光镜面）。
  - `warm-editorial`：经典暖色学术纸张主题。
- **持久化同步**：主题存储在 `localStorage["read-desktop.theme"]`，并在 `document.documentElement` 上绑定 `data-theme` 属性。
- **严禁复古小说杂色**：彻底移除 `.grain` 噪点纹理与复古小说 Serif 衬线体，采用清晰、冷色调、高可读性的现代排版体系。

### 11. 对话气泡拓扑（Discussion Bubble Topology）

- **用户问题（User Question）**：
  - 必须**框起来**，采用 Liquid Glass 磨砂圆角卡片（`border-radius: 18px 18px 4px 18px`）。
  - 必须**靠右对齐**（`align-self: flex-end; margin-left: auto; max-width: 85%`），用户头像与时间戳靠右排布。
- **AI 回答（Assistant Response）**：
  - 必须**全宽无框（Frameless Editorial）**，去除任何气泡边框束缚，最大化利用宽度阅读长篇论文解析、居中 LaTeX 公式卡片与代码块。

### 12. OCR 选区与微交互设计

- **选区高亮颜色与边框**：
  - 采用学术薄荷浅绿（`rgba(34, 197, 94, 0.09)`）配合 **4px 圆角实线边框**（普通态 `1px solid rgba(34, 197, 94, 0.5)`，引用/聚焦态高亮翡翠绿 `1.5px solid #16a34a` + 微光投影）。
  - 严禁使用破损破碎感的虚线（Dashed），保持严肃专业的学术质感。
- **持久化资产绿勾（✓）**：
  - OCR 悬浮微操作胶囊（`ocr-block-actions`）体积保持小巧；若对应 Block 本地已生成 Lens / Translation 成果，在按钮后自动渲染绿色小勾（`✓`）。

### 13. 视口锁定与双画布独立滚动机制

- **视口锁定**：`html, body, #root, .app-shell` 严格约束 `height: 100vh; max-height: 100vh; overflow: hidden;`，禁止整体窗口发生任何上下滚动晃动。
- **独立滚动**：
  - PDF 区域由 `.reader-stage` 内部的 `.pdf-reader` 单独滚动。
  - Chat 区域由 `.message-stream` 单独滚动。
  - 顶栏 44px 悬浮岛与底栏 28px 状态栏永远吸附在屏幕上下边缘。

### 14. Block 选中、工具条消失、Esc 分层

工具条绝对定位在 hit box **上方**（`top: -8px; transform: translateY(-100%)`）。如果只用 `:hover` 显示，鼠标往上移就离开 hit，按钮立刻卸掉。正确模型：

- 悬停：只加浅绿，**不**显示 `.ocr-block-actions`
- 单击：`focusedBlockId` 切换/选中，`.is-focused` 才 `display:flex` 工具条
- 按钮 `stopPropagation`，否则会当成再点一次 Block 而取消选中
- 点 canvas / page surface 空白：`onClearBlockSelection`
- chrome `z-index` 低于正文，避免页眉抢走点击
- 贴顶的 Block 加 `actions-below`

`Esc` 一层一层吃：弹层（设置/任务中心/树/已关闭/重命名）→ 取消 Block 选中 → 回文献大厅。一次 Esc 不能既清 Block 又踢回大厅。`showBrief` 弹窗已删，不要再把它加回 Esc 守卫。

### 15. Brief 只有一条路

旧 `showBrief` 弹窗 + `get_brief` 展示路径已废。顶栏「速览 Brief」= `setRightTab("artifacts")` + `preferBrief`。没有 Brief 时即使 artifacts 非空也要停在生成态，不要误选一篇翻译。重新生成只放 Brief 详情，不放索引顶栏（易误触）。

### 16. Thread 关闭 / 恢复 / 删除 / 标题

- `close_thread`：最后一条 active 拒绝；无 user 消息则 DELETE（discard）；否则 `status=archived`
- `list_archived_threads` / `restore_thread` / `delete_thread`（只删 archived）
- 空对话不进已关闭列表
- 占位标题见 `threadTitle.ts`；只有**还没有 user 消息**时才用第一句问句改名，否则会把已有「Main discussion」长对话误改名
- 标签行可横滑，禁止 `threads.slice(0, 3)`

新命令改了 `lib.rs` 后必须重启 Tauri，旧 exe 没有这些 invoke。

### 17. Gemini Orientation Pack 400：`json_schema` 不是 Interactions 的 type

`response_format.type = "json_schema"` 会 400：`The value 'json_schema' is not supported for 'type' at 'response_format'`。

Interactions 要：

```json
{ "type": "text", "mime_type": "application/json", "schema": { "type": "object", ... } }
```

调用方常传入 OpenAI 外壳 `{ name, strict, schema }`。`gemini_response_format` 必须剥出内层 `schema`。翻译/解释/Lens/压缩走同一 `body`/`text_body`，一起修。测试：`structured_output_uses_interactions_text_json_not_openai_json_schema`。

### 18. 公式渲染与复制（讨论 + 全部阅读成果）

- 渲染入口只有 `MarkdownBody`。`prepareMarkdown`：`preprocessLaTeX` → 非公式片段上 `decodeLiteralBreaks` / `repairInlineEmphasis` / `splitPackedListItems` → `wrapBareInlineMath`（正文里的 `z_i`、`h_i(t)`）→ citation 链接。列表与粗体的合同、样本、禁止项见 [markdown-rendering.md](markdown-rendering.md) 与 **§44**。
- 符号表/术语标题、Lens `reconstructedLatex` 还要先 `ensureInlineMath` / `ensureDisplayMath`，它们经常是裸 identifier，不是 Markdown。
- 选中复制：`clipboardMarkdownFromSelection` 把选区 DOM 序列化成 GFM（列表 marker、`$`/`$$`、粗体、标题、表格、`[p.N]`）。`handleMarkdownCopyEvent` 只写 `text/plain`。跨根监听在 `.message-stream`、`.artifact-detail-body`、`.block-tab-generated`。合同 [markdown-rendering.md](markdown-rendering.md) **§9**。
- 单击公式：`formattedLatexFromKatexNode` + `navigator.clipboard`；悬停类名 `.latex-hit`。mouseup 时选区已非折叠则不写剪贴板。行间单击复制为独占行 `$$\ntex\n$$`。
- `rehype-katex` 必须 `output: "htmlAndMathml"`。纯 `html` 没有 `annotation[encoding="application/x-tex"]`，选区复制和单击都拿不到 LaTeX。
- 不要假设「走过 Markdown 组件」就等于公式已包好。不要 `cloneContents()` 再取 `textContent` 当复制实现。

### 19. 字号 `A−`/`A+` 数字变、正文不变

根因曾是 `.workspace-grid` 上写了**两个** `style={}`，后者盖掉 `--discussion-font-size`。另外成果页大量写死 `10px`/`11px`/`13px`，变量到不了符号表。

正确做法：一个 `style` 对象同时带 `--discussion-font-size` 和 `--chat-width`；`data-discussion-font={14|15|17}` 配 `.workspace-grid[data-discussion-font="17"] .markdown-body { font-size: 17px !important; }`。控制条要同时出现在讨论顶栏和成果 INDEX。PDF 顶栏的 147% 是 PDF zoom，不是字号。

### 20. 气泡模型名

禁止写死 `Gemini 2.5 Flash`。用 `formatModelLabel(message.usage?.model, models, paperModel)`。底部状态栏的 `gemini-3.6-flash` 才是当前配置；旧气泡必须显示**该条回执**里的模型。

### 21. 聊天记录跳转刻度轴（`ChatTimelineNavigator`）

- **功能**：在研讨区消息流右侧放置轻量 18px 垂直刻度轴。自动提取所有的用户提问轮次。
- **悬停预览**：鼠标悬停在刻度上即刻弹出高斯模糊卡片 Tooltip（`260px`），包含轮次时间、粗体问题标题和 AI 回答前 2 行纯文本摘要。
- **点击定位与脉冲动效**：点击刻度平滑滚动目标消息至视口中心，并触发 `.target-highlight` 光晕脉冲动画。
- **活动刻度追踪**：监听 `onScroll` 计算距离视口中心最近的消息，动态高亮对应刻度为翡翠绿。

### 24. 38px 单行紧凑输入胶囊与即时流式中断

- **单行胶囊架构**：将占用大量空间的 TextArea 改为 38px 紧凑液态玻璃单行胶囊（`.composer-compact-capsule`），回车即发送。
- **流式中动态切换中断按钮**：AI 正在流式生成时，发送按钮无缝变为带有脉冲动画的红色 `■` 停止按钮（`.stop-generation`），点击即刻调用 `cancel_discussion` 终止模型输出并保留当前已输出文字。
- **微型辅助信息条（12px）**：输入框正下方极微信息条实时显示当前模型指示灯（`● gemini-3.6-flash · Native PDF`）及当前引用选区数量（`📎 N`）。

### 25. 状态栏上移与底部 28px 冗余栏彻底清除

- **顶岛整合**：将原本位于底部的状态圆点、状态文字以及 Token 统计胶囊整合进顶部 44px 悬浮岛（`.reader-floating-island`）。
- **底栏清除**：彻底删除 `<footer className="statusbar">`，`.workspace-grid` 高度调整为 `calc(100vh - 54px)`，为 PDF 画布与研讨区直接释放了 **98px+** 垂直阅读空间。

### 26. 纯净多模态用户问题气泡（公式/图片置顶，跳转药丸置底）

- **无多余标签（No Header Fluff）**：公式与图片直接渲染，绝不包裹“📐 引用公式 [p.3 · #2]”等冗余说明。
- **公式（Equation）**：直接通过 KaTeX 在提问正文上方以半透明磨砂卡片排版标准数学公式。
- **图片（Figure / Image）**：直接在提问上方呈现微缩图卡片，**点击缩略图即可弹出全屏/大图 Lightbox 放大查看**。
- **Block 跳转药丸置底（微型字号）**：所有引用的 Block 药丸统一收敛在问题下方（`p.X · [blockType] #N`），设定为 **10.5px** 微型字号，点击秒级跳转 PDF 原文对应页面并高亮选区。
- **极简紧凑气泡**：去除冗余空行，编辑按钮 `✎` 改为右上角悬停平滑显示。

### 27. Hub 文献大厅一句话学术贡献（`briefTakeaway`）数据流水线

- **背景**：当生成 Brief 完成后，产物会保存在 SQLite 数据库的 `artifacts` 表中。
- **Rust DTO 映射**：`DocumentCard` 必须显式携带 `pub brief_takeaway: Option<String>` 与 `pub has_ocr: bool`。
- **后端自动提取**：`document_card_from_paper()` 必须在 Brief 就绪时解析 `artifacts` 的 `content_json`（提取 `summary`/`findings`），保证 LibraryHub 能够完整展示一句话学术结论（TL;DR），而不会退回到默认 Fallback 提示语。

## 避坑与防御性指南（AI 必读）

1. **Vitest 并行 OOM 问题**：
   - 运行前端测试时，**严禁**直接无参运行 `npx vitest run`（在多核 Windows 机器上并行启动多个 jsdom 实例极易导致 OOM 崩溃）。
   - **必须**使用串行命令：`npx vitest run --maxWorkers=1 --fileParallelism=false`。
2. **Tauri 2 命令参数解构规范**：
   - `send_chat` 等 Rust 端只接受结构体的命令，前端 payload 必须是 `{ request: { ...fields } }`，切忌扁平传递。
3. **冗余工具栏与内嵌老控件**：
   - PDF 画布内部不要再内嵌老式的 `.reader-toolbar` 或底部的 `.reader-controls`，所有控制已统一集成在 44px 悬浮顶岛中。
4. **CSS 变量与背景渗透**：
   - 任何新增组件请使用 `--glass-surface`、`--glass-card`、`--liquid-glass-shadow` 等变量，避免写死 `#f8f5ee` 等暖黄色杂色。
5. **Brief 一句话摘要展示断开**：
   - 如果新增或修改了文档投影字段，必须检查 `src-tauri/src/lib.rs` 的 `DocumentCard` 和 `document_card_from_paper()` 是否从 SQLite 关联查询了 `artifacts` 的 `content_json`，切忌让前端拿到 `undefined` 触发 fallback 提示。
6. **研讨区横向滚动溢出**：
   - 研讨区绝对不能出现横向滚动条。所有消息内容、代码块和公式必须具备 `word-break: break-word`、`overflow-x: auto`（内嵌代码/公式独立横滚），消息外层容器必须设置 `max-width: 100%`。
8. **设置与任务中心主题孤岛（避免硬编码深色/米黄色）**：
   - 全局设置（`SettingsWorkbench`）与任务抽屉（`OperationsDrawer`）必须严格遵循当前激活的主题（`liquid-light`、`liquid-dark`、`warm-editorial`）。
   - 禁止在卡片或面板上硬编码 `#132238`、`#f8f4eb` 或 `#fffdf8`，所有容器必须采用 `--glass-surface`、`--glass-card`、`--glass-border` 等液态玻璃变量。
   - 标题必须使用 `color: var(--ink)`，切忌给主标题设置固定白色（`#ffffff`）导致在浅色主题下无法阅读。
9. **成果 Markdown 列表/粗体**：
   - 不要用「汉字紧贴 `**` 就插空格」的正则；必须先配对 `**`。
   - 不要无条件 `replace("\\n")`（会吃掉 `\nu` / `\text`）。
   - 不要把修好的 markdown 写回用户工作区 SQLite。合同 [markdown-rendering.md](markdown-rendering.md)。

## 验证怎么跑

```powershell
npx vitest run --maxWorkers=1 --fileParallelism=false
npx tsc -b --pretty false
npm run build
cd src-tauri
cargo check --locked
cargo test --locked
```

- 全量 `npx vitest run` 必须串行。次数以 [handoff.md](handoff.md)「最近验证」为准（含 `MarkdownBody.test.tsx`）。
- 改 Markdown 列表/粗体后至少再跑：`src/markdown.test.ts`、`src/MarkdownBody.test.tsx`、`cargo test --locked normalize_markdown`。改选区复制后再跑上述两个前端文件 + `src/ArtifactPanel.test.tsx`；动了 `htmlAndMathml` 则跑全量前端测。
- 改后端后跑：`cd src-tauri; cargo test --locked`。`cargo clippy --locked --all-targets -- -D warnings` 现在要求通过。
- 改论文 provider / Chat Completions 后至少再跑：`model_settings`、`chat_completions`、`src/SettingsWorkbench.test.tsx`、`src/desktopClient.test.ts`。
- 改 Outline 后至少再跑：`src/outline`、`prompt_settings`、`outline_epoch`、`reading_state_round`。
- 改 Reading Guide 后至少再跑：`guide_module`、`guide_validate`、`guide_batching`、`v2_workspace` 迁移、`reading_state_round`、`src/desktopClient.test.ts`、`src/PdfReader.test.tsx`、`src/guide/`。
- 系统 C 盘若只剩几十 MB，把进程内 `TEMP`/`TMP` 指到 D 盘临时目录。不要删用户 Workspace。
- `v2_workspace.rs` 的 `ActiveWorkspaceLock.file` 看起来 unused，是在持有独占锁。

## 产品合同提醒

只支持原生 PDF；论文 LLM 允许 `gemini`、`openai_compatible`、`grok`；OCR 仍只 Mistral。OCR 必须用户点。OCR 后只操作 Block。翻译不发 PDF。解释/Lens/Outline 从论文根旁支。Lens QA 与 Outline 不进 Discussion。普通成果只读，术语/符号可 override。未知 usage 保持 `null`。Outline 新合同见 [outline-generation.md](outline-generation.md) 与 D-069；v3 历史见 [full-outline-v1.md](full-outline-v1.md) 与 D-031 / D-033 / D-034。Reading Guide 合同见 [reading-guide-v1.md](reading-guide-v1.md) 与 D-040 / D-068。

## 28. Full Outline（地图 V2，2026-09-10）

产品：论证／知识地图，不是目录、不是 Brief、不是聊天。恰好两层。新生成协议：

- `outline-map-v4`（构图 `outline-draft-v4`，复核 `outline-review-v4`，局部 `outline-deep-dive-v4`）
- 旧任务／旧成果：`outline-extract-v3` / `outline-compose-v3` / `outline-deep-dive-v3`

ADR：D-031／D-033／D-034 为 v3；**D-069** 为联合构图、自由关系、独立复核与无损迁移。合同 [outline-generation.md](outline-generation.md)；v3 历史 [full-outline-v1.md](full-outline-v1.md)。

**不要**把 `OUTLINE_EPOCH` 从 3 改成 4。**不要**复用 `migrate_outline_prompt_generation` 删除三个槽。

### 文件入口

| 层 | 路径 | 职责 |
| --- | --- | --- |
| v4 协议 | `outline_map.rs` | `outline-map-v4` 图、引用、draft/review schema、任务分派 |
| v3 协议 | `outline_protocol.rs` | extract/compose-v3、角色/关系枚举 |
| 校验 | `outline_validate.rs` | v3 DAG；v4 只做结构与定位硬校验 |
| 计划 | `outline_plan.rs` | 冻结计划、digest、局部父节点快照 |
| 生成 | `outline_generation.rs` | 来源根分支、共享一次修复、call_once 重放 |
| 目录 | `outline_catalog.rs` | 瘦 OCR 目录；v4 局部图用全文白名单 + 优先页 |
| 存储 | `outline_module.rs` | 候选不移 head；正式事务发布 + expectedHeadId |
| 迁移 | `v2_workspace.rs` | `OUTLINE_EPOCH=3` 保持不变 |
| 提示词 | `prompt_settings.rs` + `prompts/outline-*.md` | bundle v3/v4；`outline_map_generation` 升级已知旧默认 |
| 入队 / worker | `lib.rs` | `plan_outline` / `start_outline`（v4 需 planId） |
| 画布 | `src/outline/` | 自由关系、连线详情、回路／多分量布局 |

命令参数跟邻近 Tauri 2 命令：多数是 `{ request: { camelCase } }`。`start_outline` 与 `start_outline_deep_dive` 的 v4 路径都需要计划卡返回的 `planId` 和 `planDigest`。


### 产品行为（不要改回去）

- v4：联合构图 + 独立复核；自由角色／关系；不要求连通或无环。v3 旧任务仍是抽单元 → 构图 DAG。
- **不设**「少于 N 个节点就不能发布」。
- 卡片上只有小号节点编号、可选角色、两行标题。takeaway 只在右侧详情。编号不是阅读顺序。
- 未选中时详情栏收起。节点或连线选中后打开详情。跳 PDF 只走「跳到原文」和引用药丸。
- 节点可拖，坐标只留在这次打开。边不能手连。不要 MiniMap。
- 删除二次确认且不可恢复。v4 开始必须带冻结 `planId`。
- 提示词存在应用账号 `prompt-settings.json`。入队冻结全文。自定义旧稿走 v3，不要当 v4 猜。


### 两个选中 id，不要混

- `outlineNodeId`：总图选中，也是局部图的**父节点**（重生成/删除局部图用这个）。
- `deepDiveNodeId`：局部图里当前点开的节点，只用来开详情。
- `planOutlineNodeSelect`：`view === "deep_dive"` 时 `loadDeepDiveFor` 必须是 `null`，`view` 必须仍是 `deep_dive`。

曾经的 bug：局部图点节点调用了 `setOutlineView("overview")`，再用局部节点 id 去 `get_outline_deep_dive`，把头指针清掉，界面跳回总图。

### 无损迁移与恢复

地图 V2 不再沿用历史的清旧图／擦提示词槽操作。migrate_outline_epoch 只更新兼容标记，保留旧总图、局部图、计划、任务、checkpoint、usage 和阅读状态；失败回滚，重复打开保持数据。

提示词 current／previous 带协议来源，论文与教材整套独立升级。自定义旧稿保留 v3；混合协议和未知未来协议在生成前阻止。设置“使用新版整套默认”经 restore_outline_prompt_bundle 一次写入，重复恢复默认不能覆盖历史。

总图和局部图开始均提交 planId／planDigest；局部图先调用 plan_outline_deep_dive。outline_runtime.rs 在真实发送点限制来源根额度、保存已返回的付费结果并按逻辑槽去重收据。旧未完成计划在删除后失效；已完成发布标记随计划保留，不依赖被替换的 revision 行。

完整修复与验证见 [地图 V2 复审报告](outline-map-v2-review.md)。

### 画布高度为 0

详情栏改成「选中才出现」后，`.outline-canvas-shell` 从 grid 换成横向 flex。`.outline-flow` 若只有 `height: 100%`、没有 `flex: 1`，未选中时唯一子元素高度算成 0，再被 `.panel { overflow: hidden }` 裁掉——工具条还在，地图和缩放按钮全没了。

现行：`.outline-flow` 必须 `flex: 1 1 0%`（组件上也有内联兜底）。不要改回「永远占 220px 空详情栏」的 grid。

### 其它坑

- `@types/dagrejs__dagre` 在 npm 上 404。`@dagrejs/dagre` 3.x 自带类型。
- `@xyflow/react` + dagre 必须 `React.lazy`。计划卡路径不得出现 `rf__wrapper`。第一次 lazy 在慢机器上会超过 `findBy` 默认超时，已发布图测试等到 10s。
- 边标签测试需要 `src/test/setup.ts` 里的 `SVGElement.getBBox` polyfill。
- 改 Rust 命令 / 协议 / 提示词目录后必须重启 `tauri dev`。纯 CSS / `OutlineCanvas` 热更新即可。
- 打开旧工作区保留已有地图；使用新版结构须明确重新生成，不通过迁移改写旧成果。
- 不要从旧 HEAD 开 worktree 做 Outline：`master` 上曾有未提交底盘，worktree 会丢文件。就在当前树做。
- 不要把节点坐标写入 `graph_json` 或阅读状态。
- 不要为公式/图/表共用一条 `{kind}` Lens 提示词；Lens 是 3 份生成 + 3 份 repair + 1 份 QA。

## 30. Reading Guide（AI 好友批注，D-040）

产品：预计算多角色页边批注，不是聊天、不是路线图、不是第三工作台。协议：`reading-guide-desktop-v1`。ADR：D-040。权威合同：[reading-guide-v1.md](reading-guide-v1.md)。

壳仍只有 `pdf_discussion` / `pdf_outline` / `outline_only`。讨论栏（`isPdfOnly`）与旁批总开关独立。失败重跑保留旧 head。改 Rust 命令后必须重启 `tauri dev`。

### 文件入口

| 层 | 路径 | 职责 |
| --- | --- | --- |
| 协议 | `guide_protocol.rs` | 版本常量、inks JSON schema、Job kind `reading_guide` |
| 人格 | `guide_personas.rs` | 阿林 / 老周 / 小夏；前端 `src/guide/personas.ts` 必须同色同名 |
| 校验 | `guide_validate.rs` | 宽松 decode、IoU/blockId snap、`to_value` 扁平 JSON、压缩 catalog/context |
| 切批 | `guide_batching.rs` | 6 页目标 / 8 页硬顶；稀疏批可修 |
| 存储 | `guide_module.rs` | revision / head / plan；inks 用 `GuideInk::to_value` 再存 TEXT |
| 入队 | `guide_commands.rs`；`lib.rs` 只留 `#[tauri::command]` | `{ request: { revisionId } }`（D-019） |
| Job | `lib.rs` `execute_reading_guide_job` | 上下文可带 PDF；**分批旁批禁止再传整本 PDF** |
| 前端墨水 | `src/guide/inks.ts` | `normalizeGuideInks` / `locateGuideInks` / `firstGuideAnchor` |
| 页边 | `marginLayout.ts`、`GuideCard.tsx`、`PdfReader.tsx` | 方案 B：每页右侧 240px 槽 |
| 顶岛 | `GuideIsland.tsx` | `[✎ 旁批 · N] [▾]`；菜单：重新生成 / 显示或隐藏 / 删除 |

### 生成管线（不要改回去）

1. 先 `GuideContext`：可以 `interact()` 带论文 PDF（压缩阅读上下文）。
2. 再按批 `GuideAnnotate`：必须 `interact_text()`。45 页论文把 PDF 再贴到每一批会超时，任务中心停在「annotating n/m」，费用不涨（provider 在 HTTP 前已 committed）。
3. Catalog 必须带 excerpt（约 240 字），每页最多约 8 块。没有 excerpt 时模型会写一篇总评，全书只剩 1 条旁批。
4. 解析要认 `kind`/`type` 别名、内部标签 `{ Note: {...} }`、`speaker_id`、缺 `kind` 但有 body。存盘一律 `to_value`（`{ kind, speakerId, anchor, body }`），不要 `serde` 默认外部标签。
5. 全书 0 条可定位墨水 → Job 失败。允许 partial。进度键是 `batch`/`batches`。
6. **不要**为了刷新旁批提示词去加 `PROMPT_SETTINGS_SCHEMA`。用 `GUIDE_PROMPT_GENERATION` 只擦旁批两槽，19 个槽的用户改稿才能保住。

### 阅读面

- 层开时每页都留 240px 槽，安静页写「本页无旁批 · 下一处 p.N」。
- 卡片高度锁在画布高度内（栏内滚动）。槽被卡片撑高会破坏虚拟列表 stride。
- 顶岛主按钮显示条数（`✎ 旁批 · 部分 · 10`）。`旁批已就绪` 后面没有「从第 N 页」= 前端 `firstGuideAnchor` 为空（解析失败或只有 traces）。
- 生成完成按 **head id** 只跳一次第一条旁批。同一 head 的后续 `paper`/`reading_guide` 事件不得再 `setPage`。

### 踩过的坑（不要再引入）

1. **分批仍 `interact()` 整本 PDF** → 后几批 `error sending request` / 卡在 annotating、费用不动。旁批用 `interact_text`。
2. **Timeout 用同一巨大 payload 再试** → 再空等 ~120s。Timeout 不要原样重放。
3. **serde 内部/外部标签** → 前端 `normalizeGuideInks` 丢掉全部 note。存盘走 `to_value`；读取当 `serde_json::Value`。
4. **spacer 虚拟列表 + 跳过 `scrollIntoView`** → 拖滚动条黑屏。改绝对定位。
5. **统一 pitch 920 / 量到 min-height 220** → 页重叠。用画布上报高度做累计 `top`。
6. **`initialScrollOffset` 进校正 effect + drift recovery** → 上滑弹回第一条旁批所在页。只在 `currentPage` 由外部改变时写 `scrollTop`。
7. **每次 `reading_guide` 事件都 `setPage(first)`** → 用户正在读也被拽走。按 `head.id` 跳一次。
8. **页边 `contentHeight` 撑高 row** → stride 按画布、行按卡片，又叠页。槽 `maxHeight = pageSize.height`。
9. **不要复用 `reading_roadmap`。** 那是 Keshav 三遍 Todo，另一套成果。
10. **引线 `y2` 用卡片布局 `top`、不减 `gutter.scrollTop`** → 槽内一滑，线和卡片错位。SVG 钉在页行上；跟滚走 `guideLeaderLine`。

## 29. OpenAI-compatible / Grok 论文 provider（2026-08-18）

代码已落地。合同：D-017 2026-08-18 修订；计划 [2026-08-18-openai-compatible-and-grok-providers.md](superpowers/plans/2026-08-18-openai-compatible-and-grok-providers.md)。**不要**再把 OpenAI/xAI 当「明确不做」。也**不要**做成 Azure / Responses / Assistants / 自定义 Header / 跨 provider 翻译。

### 产品行为（不要改回去）

- 同一时间 **一个当前论文 LLM provider**：`gemini` | `openai_compatible` | `grok`。论文模型和翻译模型必须同一家。Mistral 仍只做 OCR。
- **保存 ≠ 设为当前。** 三张卡都能存 Key/模型；只有 `set_current_paper_provider` 改 `currentProvider`。试 OpenAI 不会踢掉正在用的 Gemini。
- 当前失效（清 Key、探针作废）**钉在那一家**，显示未就绪。不自动回 Gemini。
- 「设为当前」要确认。进行中的流式 Chat / 已入队 Job **继续走开工快照的 provider**，下一轮用户发送才走新当前。
- 论文根必须是 **整本 PDF 进模型上下文**。官方 Grok 附文件会走 `attachment_search`：探针必须失败，卡片可存，不能「设为当前」。
- 论文模型三道门（`paper-probe-v1`，测连接时两轮付费）：① 内置小 PDF nonce `RD-PDF-NONCE-7F3A` + 小图字母 `R`，且响应里不能有 `attachment_search` / `file_search` / `document_search`；② `response_format` schema `{ ok: boolean }` 为 true。翻译模型不打这三道门。
- Gemini 测连接仍是 Files + Interactions 能力位，**不要**拿 Chat Completions 探针去打 Gemini。
- 每张卡记住自己的论文/翻译模型。切回 Gemini 必须回到 Gemini 那一对，禁止把 `gpt-4.1` 套到 Gemini 上。
- OpenAI-compatible：用户填 Base URL + Key，默认 `https://api.openai.com/v1`。Grok：Base URL 写死 `https://api.x.ai/v1`，只填 Key。
- 接线只保证 `/v1/chat/completions`；有 `/v1/files` 就上传复用 file id，404 再 inline base64。**不要**发 `previous_interaction_id` / `previous_response_id`。非 Gemini 的 Discussion **每轮用 `recovery_input`（整段分支历史）+ PDF**。
- 模型 id：兼容口可手填，允许 `A-Za-z0-9._:-/`。Gemini id 规则不变（不能 `/`）。

### 文件入口

| 层 | 路径 | 职责 |
| --- | --- | --- |
| 存盘 schema 3 | `src-tauri/src/model_settings.rs` | `currentProvider` + `gemini` / `openaiCompatible` / `grok` 槽；v1/v2 迁进 Gemini 槽；**禁止**读文件后再把 provider 写死回 gemini |
| 凭据 | `lib.rs` keyring | 服务名仍 `com.skywalker.read-desktop`；用户名 `gemini-api-key` / `openai-compatible-api-key` / `grok-api-key` / `mistral-api-key`。Key 不进 Workspace/SQLite |
| Chat Completions adapter | `src-tauri/src/chat_completions.rs` | `PaperModelPort`；探针；`GET /models` |
| Gemini adapter | `src-tauri/src/provider_ports.rs` | 不要改成 Chat Completions。结构化输出仍是 Interactions 的 `text` + `mime_type` + `schema`，不是 OpenAI `json_schema` type |
| 工厂 | `lib.rs` `open_paper_adapter` / `PaperAdapter` | Job/Chat 按 **快照的 provider** 选 Gemini 或 Chat Completions，不要读当时的 current 去改进行中的请求 |
| 就绪检查 | `lib.rs` `require_current_paper_provider` | 未就绪文案带当前 provider 名 |
| 讨论输入 | `lib.rs` `discussion_user_input` | 仅 Gemini+已有 previous → Incremental；openai/grok 永远 Recovery |
| SQL 论文根 | `reading_artifact_module.rs` / `outline_module.rs` / `lib.rs` | `context_roots` / `provider_nodes` 的 provider 是绑定参数。Gemini 根和 OpenAI 根可并存 |
| 设置 UI | `src/SettingsWorkbench.tsx` | 顶上当前条 + 三张卡 + 底部 Mistral。确认框文案锁定在组件常量里 |
| 前端类型 / IPC | `src/types.ts`、`src/desktopClient.ts` | 新命令走 D-019：`{ request: { ...camelCase } }` |
| 空态文案 | `src/App.tsx`、`src/modelLabel.ts` | `{ProviderLabel} is not configured…`；气泡模型名仍读 `usage.model` |

### 命令（改完必须重启 `tauri dev`）

```text
test_openai_compatible_connection { request: { apiKey?, baseUrl?, paperModel } }
test_grok_connection              { request: { apiKey?, paperModel } }
save_openai_compatible_settings   { request: { apiKey?, baseUrl, paperModel, translationModel } }
save_grok_settings                { request: { apiKey?, paperModel, translationModel } }
clear_openai_compatible_credential
clear_grok_credential
set_current_paper_provider        { request: { provider } }
```

Gemini 的 `test_gemini_connection` / `save_model_settings` / `clear_gemini_credential` 仍在；`save_model_settings` **不得**改 `currentProvider`。

### 探针指纹

```text
sha256(normalize(baseUrl) + "\n" + key + "\n" + paperModelId + "\n" + "paper-probe-v1")
```

`normalize` = trim + 去掉尾 `/`，必须和 `ChatCompletionsAdapter::new` 同一套。改 URL / Key / 论文模型才作废。**不要**每次启动或每次发送重打探针。运行时 PDF/图/schema 4xx 清掉该槽探针，但 current 保持。

### 踩过的坑（不要再引入）

1. **`read_model_settings` 曾把 `provider` 写死成 `gemini`。** schema 3 文件会被冲掉。解析在 `model_settings.rs`；v3 按盘上的 `currentProvider` 走。
2. **Grok「能传 PDF」≠ 论文根。** 官方附文件会变成检索 tool。探针看到 `attachment_search` 必须失败，文案：检索式 PDF 不能作为论文根。
3. **Chat Completions 没有 Gemini 会话续接。** 把 `previous_interaction_id` 塞进 body 是错的。非 Gemini 必须重放本地分支（`recovery_input`）并再次带上 PDF。
4. **死掉的 file id 常常是 HTTP 400 不是 404。** `chat_completions.rs` 要把「400 + 消息像 file not found」映射成 `StaleRemoteResource`，否则论文根/讨论不会重新上传。
5. **Job 执行读 payload 里的 `provider`，不要读当时的 current。** `root_key` 是 `{revisionId}:{provider}:{paperModel}`。Key 在 Credential Manager，不要把 secret 写入 Job JSON。
6. **新设置命令是结构体参数。** 前端必须 `command("save_openai_compatible_settings", { request: { ... } })`。扁平字段会 `missing required key request`（D-019，和 `send_chat` 同一类坑）。
7. **指纹和 adapter 的 Base URL 必须同一规范化。** 用户可能带尾 `/`。两边不一致会导致「刚测过、一保存探针无效」。
8. **浏览器 memory adapter 不假装探针通过。** `npm run dev` 不能「设为当前」到 OpenAI/Grok。真路径只在 `tauri dev`。
9. **Gemini 的 `json_schema` 外壳仍要剥。** 那是 Interactions 合同（§17）。Chat Completions 论文调用才用 OpenAI 的 `response_format.json_schema`。两套不要抄反。
13. **Windows 默认 1 MiB 主线程栈在 ~80 个 IPC 命令展开下易报 `0xc00000fd` 栈溢出。** `src-tauri/build.rs` 已加上 `/STACK:8388608`（8 MiB 保留空间），`AppState` 严禁在栈上放置大数组或未 `Arc`/`Box` 的重数据结构（有回归测试 `app_state_fits_comfortably_on_a_small_stack_frame` 限制 < 4 KiB）。编译期 `0xc0000409` / LLVM OOM 是另一件事，见 [windows-dev-build.md](windows-dev-build.md)。
14. **Windows 原生窗口顶栏不随 WebView2 自动变暗。** 必须通过 `set_window_theme` 调用 Win32 DWM API 同步 `DWMWA_USE_IMMERSIVE_DARK_MODE` (20/19)、`DWMWA_CAPTION_COLOR` (35) 与 `DWMWA_TEXT_COLOR` (36)，否则暗夜主题下会出现刺眼的白顶栏。
15. **任务中心展示必须最新置顶。** `JobModule::list` 的 SQL 和前端 `OperationsDrawer` 均保证 `running -> queued -> paused -> others` 且同一优先级下按 `created_at DESC` 降序，不要再改成默认的升序。

## 33. Windows 栈保护、原生顶栏主题同步与任务体验优化

### 30.1 8 MiB 栈保留与 AppState 瘦身
- **背景**：Tauri 2 的 `generate_handler!` 宏展开处理 80 多个命令的 match 分支，在 Windows 默认 1 MiB 栈上限下，启动初始化阶段会遭遇 `STATUS_STACK_OVERFLOW (0xc00000fd)` 崩溃。
- **治理规范**：
  - `src-tauri/build.rs` 配置 `println!("cargo:rustc-link-arg=/STACK:8388608");`。
  - `AppState` 内部全部重资源均采用 `Arc<WorkspaceRuntime>`、`Arc<RwLock<...>>` 堆指针间接持有。
  - 维持回归测试 `cargo test --locked app_state_fits_comfortably_on_a_small_stack_frame`。

### 30.2 原生窗口顶栏沉浸式变色
- **背景**：单纯切换 HTML `data-theme` 无法改变 Windows OS 原生窗口标题栏及右上角控制按钮（最小化/最大化/关闭）的底色与字色。
- **治理规范**：
  - 前端 `App.tsx` 监听 `themeMode`，调用 `desktopClient.command("set_window_theme", { theme: themeMode })`。
  - 后端 `sync_window_theme` 对 Windows DWM 下发沉浸式深色模式（`DWMWA_USE_IMMERSIVE_DARK_MODE`）与自适应 RGB 颜色（`#090d16` 暗夜、`#e2e6eb` 明亮、`#eee9df` 暖阳）。

### 30.3 成果栏（ArtifactPanel）论证地图直达
- 成果栏首项及空态均提供「论证地图」快捷按钮，触发 `onOpenOutline` 切换为 `pdf_outline` 分屏。

## 31. LaTeX 容错渲染、中西文字体独立配置与任务调度中心液态玻璃重构

### 31.1 KaTeX 希腊字母文本模式与控制字符破坏修复（踩坑指南）
- **现象与根本原因**：
  1. 模型返回数学公式时，经常输出 `\textbf{\kappa}(t)` 或 `\text{\kappa}`。`\textbf` 是文本加粗命令，KaTeX 在文本字体下匹配希腊字母时会直接报错并输出红字 `<span class="mord text" style="color:#cc0000;">\kappa</span>`。
  2. JSON 转义损坏：反斜杠转义在 raw unescape 后把 `\b` 变成了退格（`\x08`）、`\f` 变成了换页（`\x0c`）、`\t` 变成了 Tab（`\x09`）、`\r` 变成了 CR（`\x0d`），导致 `\textbf` 破坏为 `\t` + `extbf`，`\beta` 破坏为 `\x08eta` 等。
- **治理流水线**（`src/markdown.ts` `preprocessLaTeX`）：
  - 自动修复控制字符：`\x08([a-zA-Z]+) -> \b$1`、`\x0c([a-zA-Z]+) -> \f$1`、`\t([a-zA-Z]+) -> \t$1`。
  - 自动将全部希腊字母加粗宏（`\textbf{\kappa}`、`\mathbf{\kappa}`、`\bold{\kappa}`、`\bf{\kappa}`、`\bm{\kappa}`）统一规范化为数学粗体 **`\boldsymbol{\kappa}`**。
  - 自动剥离文本模式宏（`\text{\kappa}`、`\mathrm{\kappa}`、`\textit{\kappa}`），还原为合法数学符号。
  - `rehypeKatex` 启用 `{ strict: false, trust: true }`，最大化渲染宽容度。

### 31.2 中西文排版字体独立配置与 CSS 变量级联
- **现象与根本原因**：之前在设置中切换字体在阅读器不生效，原因是 CSS 主题（`:root`, `[data-theme="liquid-light"]`, `[data-theme="liquid-dark"]`, `[data-theme="warm-editorial"]` 和 `body`）硬编码了 `font-family: -apple-system, ...`，覆盖了 `--app-font`。
- **设计与实现**（`src/fontFamily.ts`, `src/SettingsWorkbench.tsx`, `src/styles.css`）：
  - 独立双选择器：
    - **西文 / 英文排版字体**：系统默认 (System Default)、Consolas (等宽学术风)、Times New Roman (经典衬线)、Inter (现代无衬线)、Computer Modern (LaTeX 罗马风)。
    - **中文阅读排版字体**：系统默认 (微软雅黑/苹方)、现代黑体 (苹方/Heiti)、霞鹜文楷 / 楷体 (KaiTi)、宋体 / 典雅衬线 (SongTi)。
  - 利用 CSS 字体回退级联：`font-family: [西文字体], [中文字体], sans-serif;` 写入 `--app-font`。
  - 设置面板提供 **中英混排实时效果预览卡片 (Combined Live Typography Preview)**，直观展现西文、中文、代码与数学符号的混合排版质感。
  - 全局主题与组件全面绑定 `font-family: var(--app-font, inherit);`，切换即刻全局无缝生效。

### 31.3 Library Hub 核心结论（Takeaway）即时悬浮气泡
- 移除了 Brief 提示词中对一句话学术结论的人为字数硬编码截断，保留完整核心贡献描述。
- 替换浏览器原生延迟 `title` 属性，构建零延迟液态玻璃悬浮气泡（`.liquid-summary-wrapper` + `.liquid-summary-popover`），卡片悬停即刻完整展开显示。
- 全面统一人文标签样式，移除冗余 `#` 前缀，并统一 Hub 与阅读器的字号与配色规范。

### 31.4 任务与调度中心（Operations Drawer）Liquid Glass 重构
- 淘汰旧式硬边框下划线 Tab，改用现代悬浮分段胶囊控制条（`⚡ 任务队列`、`💾 存储占用`、`🗑️ 回收站`、`🩺 诊断审计`）。
- 全面重构 `.job-card`：配备多模型专属渐变芯片徽章（Gemini/Mistral/OpenAI/Grok/Local）、状态指示胶囊（已完成/运行中呼吸灯/排队/暂停/失败）、所属论文标签、中文业务副标题、安全扣费指示以及优雅半透明错误警示卡片。

## 32. 精读路线（Reading Roadmap / Three-Pass Coach）精读引导系统

> 2026-09-09 更新：以下为历史实现笔记。当前默认稿改为导师式三遍阅读，第三遍属于默认深入路径；附加栏目可选，生成使用 PDF 来源根的独立 Artifact 旁支，已不再将路线作为 Root 调用。见 [精读路线生成合同](reading-roadmap-generation.md)。

### 32.1 核心设计理念
- **学术教练而非代读者**：将学术界经典的 **Three-Pass Approach（三遍阅读法）** 与 AI 生成的论文特异性提示深度结合。
- **与论证地图（Outline）的明确分工**：
  - **论证地图**：客观结构化刻画论文论点与证据之间的逻辑关系（论文的 X 光片）。
  - **精读路线**：主观行动路径，按 Pass 0（前置对齐）→ Pass 1（鸟瞰决策）→ Pass 2（抓住内容）→ Pass 3（虚拟复现）引导读者一步步搞懂论文。
- **特异性与证据穿透**：每个任务必须点名论文具体对象（Figure X, Algorithm Y, Table Z），包装为可点击的证据药丸（`[📄 p.N · Figure X]`），点击平滑跳转 PDF 并高亮 OCR Block。

### 32.2 核心架构与数据流
- **前端组件**：`src/ReadingRoadmap.tsx`（液态玻璃左侧浮动面板，不遮挡右侧讨论区，支持多 Pass 折叠、自检问题、任务勾选、30秒电梯稿与核心图卡片）。
- **类型定义**：`src/types.ts`（`RoadmapTask`, `RoadmapPass`, `ReadingRoadmapContent`, `ReadingRoadmapProjection`, `RoadmapProgressEntry`）。
- **后端持久化**：`src-tauri/src/roadmap_module.rs`（`roadmap_progress` 表，持久化任务勾选状态）：
  ```sql
  CREATE TABLE IF NOT EXISTS roadmap_progress (
    paper_id     TEXT NOT NULL,
    roadmap_id   TEXT NOT NULL,
    task_id      TEXT NOT NULL,
    completed    INTEGER NOT NULL DEFAULT 0,
    completed_at TEXT,
    PRIMARY KEY (paper_id, roadmap_id, task_id)
  );
  ```
- **生成产物**：AI 生成的 Roadmap 保存为 `reading_roadmap` 类型的不可变 Artifact，`artifact_heads` 表记录当前最新 head。
- **提示词槽位**：`PromptSlotId::ReadingRoadmap`（第 17 个生产槽位，可在设置中心自由配置）。
- **IPC 命令**：
  - `get_roadmap(revisionId)`: 获取当前论文版本的精读路线成果。
  - `start_roadmap_job(request: { revisionId })`: 异步发起生成 Job，入队后台 worker 执行。
  - `toggle_roadmap_task(request: { paperId, roadmapId, taskId, completed })`: 记录单个任务的勾选/取消。
  - `list_roadmap_progress(request: { paperId, roadmapId })`: 读取当前论文的所有任务完成记录。

### 32.3 踩坑与注意事项（下一个 AI 必读）
1. **JSON Schema 递归深度（Rust 编译坑）**：
   - Rust 的 `json!({...})` 宏在深层嵌套时容易触发 `$crate::json_internal` 递归上限（`recursion limit reached while expanding $crate::json_internal!`）。
   - 对于大型严苛 JSON Schema，使用 `let response_schema: serde_json::Value = serde_json::from_str(r#"... "#).expect("valid roadmap schema");`，编译更快且彻底免除宏展开递归风险。
2. **MarkdownBody 组件参数**：
   - `MarkdownBody` 接收 `children: string`（`<MarkdownBody>{text}</MarkdownBody>`）而非 `markdown: string`。
3. **PaperInteractionRequest 参数规范**：
   - 调用 `adapter.interact` 时，使用标准字段：`model`, `context_epoch`, `pdf_path`, `display_name`, `remote_file_id`, `system_instruction`, `user_input`, `response_schema`, `inline_images: Vec::new()`, `kind: PaperInteractionKind::Root`。
   - `adapter` 必须经由 `job_paper_model_adapter(open_job_paper_adapter(job, &api_key)...)` 包装。
4. **前端乐观更新（Optimistic UI）**：
   - 用户在浮动面板中点击任务勾选框时，`App.tsx` 的 `onToggleTask` 首先在前端本地状态中乐观翻转 `roadmapProgress`，再发起 `toggle_roadmap_task` IPC 调用，确保毫秒级勾选反馈无卡顿。
5. **前置依赖提示**：
   - 生成 Reading Roadmap 前，若当前论文尚未生成 Brief，浮动面板会展示温馨提示建议先生成 Brief 以强化前置概念对齐；但两者彼此独立，不会强依赖 Outline。
6. **`provider_node_id` 外键约束（FOREIGN KEY constraint failed 踩坑）**：
   - `ArtifactDraft` 中的 `provider_node_id` 必须是 `provider_nodes` 表中的本地 UUID 主键，而不是大模型返回的远程 interaction id（如 `"interactions/123"` 或 `"chatcmpl-1"`）。
   - 在将 `ArtifactDraft` 发给 `artifact_module.publish` 之前，必须先将远程 id 作为 `provider_node_id` 字段插入 `provider_nodes` 表，生成本地 UUID 并赋给 `ArtifactDraft.provider_node_id`。
7. **浮动液态玻璃卡片与屏幕左缘触碰唤出（Edge Hover & Pin 交互）**：
   - Roadmap 面板采用悬浮四角圆角设计（`top: 52px; left: 14px; bottom: 16px; border-radius: 20px`），不再贴死左底边框；
   - 支持 **左侧边缘触碰唤出（Edge Trigger Zone）**：鼠标移动到屏幕最左侧（或点击边缘胶囊）即可丝滑划出面板；
   - 支持 **移开自动收起与钉住（Pin Toggle）**：面板未固定（`pinned === false`）时，鼠标移出面板 280ms 后平滑自动收起；点击顶栏 `📌 钉住` 按钮即可固定常驻，状态自动保存至 `localStorage`。

## 33. 阅读成果（Artifact & Lens）精准联动、证据高亮与输入交互重构

### 33.1 Lens 点击自动直达与 `preferBrief` 清理
- **现象与根因**：点击 PDF Reader 选区工具条上的 Formula/Figure/Table Lens，界面切换至阅读成果标签页，但视图仍然停留在 Brief 或旧成果上。原因是：
  1. `handleBlockAction` 仅切换了 `rightTab = "artifacts"`，未调用 `selectArtifact` 选中对应已存在的 Lens 成果；
  2. 若之前打开过 Brief，`preferBrief` 标志位为 `true`，导致 `ArtifactPanel` 强制渲染 Brief。
- **解决方案**：
  - 在 `handleBlockAction` 中显式设置 `setWorkspaceLayout(applyWorkspaceLayoutIntent(workspaceLayout, "open_artifacts"))`，并将 `isPdfOnly` 设为 `false`，确保右侧成果栏展开；
  - 在 `handleBlockAction` 与 `pendingArtifactJob` 完成回调中显式设置 `setPreferBrief(false)`；
  - 触发 block action 时先匹配 `artifacts.find(a => a.objectKey === block.id)`，若已生成则直接 `selectArtifact(existing)`；
  - 若尚未生成（异步生成中），将裁剪图片 `material.displayCropDataUrl` 赋给 `activeArtifactCropSrc`，并在 `ArtifactPanel` 中立即渲染「Lens 分析中...」卡片与左侧 `working` 占位，避免停留于旧成果让用户困惑；
  - 异步生成完成后，通过 `pendingArtifactJob.blockId` 精准选中并无缝替换为正式成果。

### 33.2 证据跳转（Evidence Jump）精准聚焦高亮 Block
- **交互规范**：点击阅读成果底部的证据时，不仅跳转到指定页码（`setPage(page)`），同时传递 `blockId` 并调用 `setFocusedBlockId(blockId)`，使 PDF 阅读器平滑滚动到该 Block 并激活高亮框。
- **证据药丸（Evidence Pill）轻量化**：淘汰以往冗长、甚至带有未渲染 Markdown 图片语法的庞大按钮，重构为小巧的液态玻璃胶囊（`📄 p.N ↗`），悬停展示详细引用摘录。

### 33.3 全局输入框自动增高、上下滚动与快捷键体验
- **讨论区 Composer 与 Lens QA Composer 规范统一**：
  - 均采用单行/多行弹性 `<textarea>` 配合 `.composer-compact-capsule`（`min-height: 38px; height: auto; border-radius: 19px`）；
  - 自动高度适应：监听输入内容根据 `scrollHeight` 动态扩张（上限 140px），达到上限后自动开启垂直平滑滚动；
  - 键盘监听统一实现：**Enter 触发发送**，**Shift+Enter 插入换行**，发送后瞬时重置回单行胶囊高度；
  - 提供现代渐变悬浮发送/中断按钮（`↑` / `■` / `…`），在胶囊底部右侧自然对齐。

### 验证

```powershell
npx vitest run --maxWorkers=1 --fileParallelism=false
npm run build
cd src-tauri
cargo test --locked
```

## 34. Lens 多版本管理系统 (`< 1/2 >`)、选区归并与安全删除 (2026-08)

### 34.1 业务背景与问题根因
- **问题 1：成果栏散落多个重复卡片**：
  在之前的实现中，多次生成同一个图表/选区的 Lens 或重新生成后，成果栏中会散落多个独立的 Lens 卡片（例如 2 张图各生成 2 次，共显示 4 个卡片），无法方便对比历史生成版本且列表杂乱。
- **问题 2：重新生成报 Block OCR 不匹配错误**：
  重新生成时直接传了旧成果的 `artifact.objectKey`，但当用户重新运行过 OCR 后，旧 Block ID 在当前 OCR 树中不存在，Rust 后端抛出 `The selected Block does not belong to the requested OCR and Document Revision`。
- **问题 3：点击已有 Lens 的 Block 重复触发生成**：
  在 PDF 中点击选区时，找到已有成果后没有 `return`，依然向后端派发了重复生成任务。

### 34.2 解决方案与架构实现

1. **成果选区空间归并算法 (`groupArtifacts`)**：
   - 无论底层数据库历史存在多少个独立的 `artifact` 记录（甚至跨 OCR 版本、不同模型生成的多次结果），只要属于 **同一论文、同一种类（如 `lens_figure`）、同一物理页码且坐标空间重叠（`IoU > 0.4`）**，均自动归纳为**同一个成果组 (`ArtifactGroup`)**。
   - 顶部 INDEX 成果栏针对每个成果组 **仅渲染 1 个卡片**；
   - 若存在多个版本，卡片右上角展示角标（如 `2 版本`）。
2. **详情页 Liquid Glass 版本切换胶囊 (`‹ 版本 1/2 ›`)**：
   - 在详情页顶部（模型标签与生成时间旁）提供毛玻璃切换胶囊；
   - 提供 `‹` 与 `›` 按钮无缝切换不同生成版本，并即时更新正文、模型标签、时间戳以及独立的 Lens QA 问答记录；
   - 重新生成后自动聚焦跳转至最新生成的版本。
3. **单版本删除与平滑回退 (`🗑️` + `DeleteVersionConfirmModal`)**：
   - 当选区存在多个版本时，支持点击 `🗑️` 唤起二次确认弹窗删除特定版本；
   - 确认后调用后端 `delete_artifact` 指令，安全清理该版本的数据库记录并自动平滑切换至相邻版本。
4. **重新生成 OCR 块空间定位与去重机制**：
   - 在 `PdfReader` 中点击已有 Lens 的 Block 直接聚焦跳转，不重复派发任务；
   - 在 `ArtifactPanel` 中点击重新生成时，通过多层匹配（ID 精确匹配 -> 空间矩形 IoU 重合匹配 -> 页面/类型回退）解析出当前 OCR 树中的真实 `block.id`，保证 `generate_reading_artifact` 100% 通过 Rust 数据库外键校验。

### 34.3 踩坑记录与关键开发注意事项

1. **SQLite 外键约束与 `artifact_heads` 级联更新顺序**：
   - `artifact_heads.artifact_id` 具有外键约束，指向 `artifacts.id`。
   - 在 Rust 后端 `artifact_module.delete` 时，若要删除的 artifact 正好是当前 `artifact_heads` 指向的 head，必须**先更新或删除 `artifact_heads` 中的记录**，然后才能执行 `DELETE FROM artifacts`，否则 SQLite 会抛出 `FOREIGN KEY constraint failed` 错误。
   - overrides 的存储表是 `term_overrides` 和 `symbol_overrides`（不是 `artifact_overrides`）。
2. **单版本删除后的 Head 自动转移算法**：
   - 删除当前最新版本（Head）后，必须按 `ORDER BY created_at DESC, version DESC LIMIT 1` 查询该选区的次新版本，并将 `artifact_heads.artifact_id` 指向次新版本，同时将次新版本的 `superseded_at` 置为 `NULL`，恢复其活跃状态；若无剩余版本，则删除对应的 head 记录。
3. **历史遗留多版本与 OCR 重新运行后的 Block ID 漂移**：
   - 当用户重新运行 OCR 时，Mistral 生成的 block ID 会变化（如 `block-old` 变成 `block-new`）。因此不能只用 `artifact.objectKey === block.id` 去做唯一性判断或重新生成，必须结合 `artifact.evidence[0].bbox` 与 `block.bbox` 进行空间重叠率（`IoU > 0.4`）匹配，才能稳定定位物理选区。
4. **二次确认弹窗设计**：
   - 删除版本使用 `DeleteVersionConfirmModal`，支持 `Escape` 快捷键、遮罩层点击取消以及危险红色确认按钮，杜绝用户误删学术资产。

## 35. Claude 暖色纸张主题全面校准与去刺眼白 (Warm Editorial Design)

### 35.1 核心设计理念与色彩层级系统
Claude 设计风格以**温润手作纸张感（Warm Paper / Editorial Linen）、极佳阅读信任感与优雅衬线排版**著称。
在 `[data-theme="warm-editorial"]` 下，**严禁在暖色底色上直接使用纯白（`#ffffff`）硬色块作为侧边栏、大纲画布或抽屉背景**。

色彩层级规范：
- `--glass-surface`: `#fbf9f5`（暖纸基底色）
- `--glass-surface-subtle`: `#f4efe6`（温润亚麻纸）
- `--glass-card`: `#fdfcf9`（柔和象牙白卡片）
- `--glass-border`: `#e8e0d4`（书本细缝线）
- `--canvas`: `#f7f2ea` / `#ece5db`（思维导图与 PDF 阅读舞台淡褐底纸）
- `--coral`: `#c15f3e`（Claude Terracotta 赤陶红强调色）
- `--ink`: `#262320`（浓缩咖啡墨黑字）

### 35.2 踩坑与注意点
1. **Token 污染排查**：不要在组件内直接写死 `background: #ffffff`，必须使用 CSS 变量或在 `[data-theme="warm-editorial"]` 作用域下使用象牙白 `#fdfcf9`。
2. **多层背景反光**：PDF 阅读器周围的 `reader-stage` 必须使用 `#ece5db` 衬底，使白色的 PDF 页面作为独立纸张自然凸显，避免全屏眩光。

---

## 36. 多布局切换常驻渲染与 PDF 阅读器零重置 (Zero-Unmount & View State Preservation)

### 36.1 业务背景与问题根因
- **现象**：在全屏地图（`outline_only`）或隐藏 PDF 视图下，点击“显示PDF”或切换回双栏模式，PDF 阅读器没有停留在之前的页码和滚动位置，而是重新刷新跳回了第 1 页。
- **根因分析**：
  1. **DOM 卸载（Unmount）**：原 `App.tsx` 使用 `{layoutShowsPdf(workspaceLayout) ? <section className="reader-panel">...</section> : null}`，导致隐藏 PDF 时组件被彻底卸载，销毁了 PDF.js 实例、Worker 渲染任务和 DOM 滚动条位置。
  2. **IntersectionObserver 竞态**：重新挂载时初始 `scrollTop = 0`，DOM 页面刚渲染时，视口监听器先触发了第 1/2 页的交叠事件，调用 `onVisiblePage(1)` 强行覆盖了目标页。
  3. **恢复偏移未同步**：滚动过程中记录的 `pageOffset` 没有实时更新至 remount 后的 `initialScrollOffset`。

### 36.2 解决方案与架构规范
1. **常驻挂载 + CSS 响应式显隐（Zero-Unmount）**：
   - 在 `App.tsx` 中使用 `<section className="reader-panel panel" style={{ display: layoutShowsPdf(workspaceLayout) ? "flex" : "none" }}>`；
   - 切换视图时 PDF 阅读器在内存中持续驻留，再次切回 PDF 视图时 **0 毫秒即时展示**，当前页码、精确滚动像素、缩放倍数 100% 保持不变。
2. **滚动状态机防重置锁 (`src/PdfReader.tsx`)**：
   - 增加 `programmaticScroll` 与 `hasRestoredInitialScroll` 锁：初次挂载与程序化定位期间屏蔽 `IntersectionObserver` 回调，直到精准平滑定位到目标位置。
   - 实时同步 `onScrollOffset={(offset) => { setPageOffset(offset); setRestoreOffset(offset); }}`。
3. **跨视图跳转自动唤出 PDF**：
   - 在 `jumpOutlineEvidence`、`jumpToBlockQuote`、`jumpFromMarkdownCitation` 等跳转回调中，若当前处于隐藏 PDF 状态，自动调用 `applyWorkspaceLayoutIntent(workspaceLayout, "show_pdf")` 呼出双栏。

---

## 37. 多模态图片引用缩略图提取与全屏 Lightbox 交互 (Figure Quote Lightbox)

### 37.1 架构实现
1. **Canvas 截屏提取 (`QuoteFigureThumbnail`)**：
   - 当用户引用的 Block 为图表（`isFigureQuote(quote.blockType)`）时，通过已加载的 `PDFDocumentProxy` 异步渲染目标页面并按照 Block 的相对坐标 `bbox` 裁剪出 Base64 图片（`cropDataUrl`）。
2. **提问气泡与引用篮子缩略图展示**：
   - 提问气泡（`MessageBubble`）与引用篮子中直接渲染高清微缩图，不再只显示枯燥的文本标签。
3. **全屏 Lightbox 模态框 (`LightboxModal`)**：
   - 点击缩略图弹出毛玻璃 Lightbox，支持清晰查看图表原貌并提供 `跳转至 PDF 第 N 页 ↗` 快捷按钮。

---

## 38. 界面微交互与细节优化 (UI Polish)

1. **大纲详情“收起”按钮显著化**：
   - 统一使用 `.outline-collapse-btn`，样式为赤陶色高对比度圆角药丸（`◂ 收起`），悬浮具备平滑放大（`scale(1.08)`）与柔和光晕，彻底替代原先隐蔽的浅灰标签。
2. **相关 Block 证据标签边界与悬浮高亮**：
   - `.outline-evidence-pills button` 与 `.outline-fallback-list button` 增加 `1px solid` 纸缝边框，悬浮时边框与字体转为赤陶色、背景微亮并产生微上浮交互动效。
3. **对话分支下拉菜单外部点击关闭 (Click Outside)**：
   - 为 `.thread-title-picker-wrap` 绑定全局 `mousedown` / `touchstart` / `Escape` 监听，点击任意外部空白区域或按下 ESC 键即可平滑收起弹窗。

---

## 39. 阅读成果空间极致压缩与导航闭环架构 (Reading Artifacts Space Compression & Navigation)

### 39.1 业务痛点与重构动因
- **原先 4 层冗余堆叠**（总高度 ~170px）：
  1. 外层 Tab：`[ 讨论 | 阅读成果 (9) ]` (38px)
  2. 内层 Scope：`[ 🌐 全篇总览 5 | 🔍 选区 Lens 2 ]` + `[ A- 15 A+ ]` (36px)
  3. Micro-chip 列表：`[ 🗺️ 论证地图 | ✦ Brief | ... ]` (34px)
  4. 成果正文 Header：`BRIEF / Brief / 由 Gemini 生成 ...` + `🔄 重新生成` (60px)
- **无用占位**：精读路线（`reading_roadmap`）在成果页渲染无内容的空白卡片；
- **地图单向导航**：从成果页点入「论证地图」后，顶栏只有「返回讨论」，无法一键退回上一次研读的成果界面；
- **顶栏文字折行**：`✓ OCR 就绪` 在较窄窗口或特定字号下折成了两行。

### 39.2 架构实施与细节合同
1. **外层与内层合并为三态分段（Eliminate Layers 1 & 2）**：
   - 顶栏统一直出：`[ 💬 讨论 ]   [ 🌐 全篇成果 (N) ]   [ 🔍 Lens (M) ]` + 右侧 `[ A- 15 A+ ]`；
   - 在 PDF 中点击公式或图表触发 Lens 时，自动调用 `setArtifactScope("block")` 与 `setRightTab("artifacts")`，右侧栏平滑切换至对应选区成果。
2. **消除正文上方第 3 行（Eliminate Layer 4）**：
   - 将 `由 XX 模型生成 · 23:39` 与 `[ 🔄 重新生成 ]` 上移合并至 `compact-rail-right`（当 `rightTab === "artifacts"` 时渲染）；
   - 正文 Markdown/表格直接顶格在 Chip 列表下方渲染，**净省 106px+ 垂直高度**。
3. **论证地图 Chip 范围收敛与双向直达**：
   - `[ 🗺️ 论证地图 Outline ]` Chip 仅在 `scopeTab === "global"` 时展示，Lens 选区分析流下不展示；
   - `OutlinePane` 顶栏新增 `[ ‹ 返回成果 ]` 按钮，通过 `onOpenArtifacts` 回调一键切换至 `open_artifacts`。
4. **顶栏 OCR 按钮与高度收敛**：
   - 按钮样式显式配置 `whiteSpace: "nowrap"; flexShrink: 0;`，杜绝折行；
   - 顶栏悬浮岛 `.reader-floating-island` 高度由 44px 降为 38px，上边距调优为 8px。

### 39.3 踩坑与避坑指南 (Pitfalls to Avoid)
1. **测试中 `getByText` 查模型标签的多元素报错**：
   - 在 `ArtifactPanel.tsx` 中，当 `!hideScopeTabs`（独立测试模式）时，避免在 Chip 栏与 Compact Header 中同时输出 `由 ${modelLabel} 生成`。
2. **`activeArtifact` 重复声明**：
   - `App.tsx` 中已有 `const activeArtifact = useMemo(...)` 位于消息树逻辑下方，不要在顶层 state 区重复声明同名变量。
3. **`scopeTab` 自动计算竞态**：
   - `ArtifactPanel` 接收外部传入的 `scopeTab` 与 `onScopeTabChange`，在 `hideScopeTabs=true` 下由 `App.tsx` 的统一顶栏驱动，禁止在子组件内部强行 override 父级状态。

## 40. 顶栏模型胶囊常驻、讨论输入区纵向净空扩展与 Roadmap 边界严格收敛 (2026-08)

### 40.1 业务背景与改动动因
1. **输入区「就绪」与底栏空间浪费**：
   - 之前在讨论输入框下方渲染了一整行 `.composer-subline`（左侧「就绪 / 提示」，右侧「模型名称」），占用 ~32px 垂直高度，挤占了正文讨论区的可读行数；
   - 用户在配置大模型时已有强校验和连接确认机制，底部的「就绪」状态指示器属于低价值冗余；
2. **Roadmap 跨页面浮现缺陷**：
   - 用户在设置面板、任务中心等非阅读页面时，鼠标移至屏幕左侧会意外触发 `ReadingRoadmap` 浮出。

### 40.2 架构实施与细节
1. **模型徽章上移至 `compact-rail-header`**：
   - 在右侧顶栏 `compact-rail-right` 中常驻渲染液态玻璃模型徽章 `.rail-model-badge`（带彩色状态点与模型名称，如 `Gemini 3.7 Flash`）；
   - 点击徽章可直接打开设置中的「模型」面板进行切换或重新校验；
   - 当凭据未配置时，徽章自动呈现橙色警告状态及对应配置提示。
2. **彻底移除 `.composer-subline`**：
   - 移除讨论输入框底部的整个子行，输入框紧凑吸底，讨论消息流垂直可视区域净增 30px+。
3. **Roadmap 严格限定在阅读视图**：
   - 边缘触发区与浮动面板显示条件统一收敛为：`selectedDoc && !showSettings && !showOperations && !showContext && !showTree && !exitChoiceOpen`，确保在设置、任务中心、讨论树等全屏/弹窗状态下绝不误弹出。


## 41. 全屏设置工作台重构、动态多 Provider 实例系统与避坑指南 (2026-08)

### 41.1 业务背景与重构动因
1. **原设置弹窗空间局促且排版松散**：
   - 之前为浮动居中 Modal，垂直内容极长，充斥着大量冗余说明文字，排版松散且缺乏层次；
   - 字体选择仅为原生 `<select>` 下拉框，用户无法直观预览中西文字体排版与 LaTeX 公式渲染效果；
2. **固定 Provider 限制与多端点需求**：
   - 旧架构固定只有 3 个 Provider 槽位（`gemini`、`openai_compatible`、`grok`），无法同时配置多个不同的 OpenAI 兼容端点（如 DeepSeek 官方、本地 Ollama、OpenRouter、SiliconFlow 等）；
3. **提示词模板编辑体验差**：
   - 原先提示词配置内嵌在长页面中，编辑框高度受限，且页面会发生外层滚动晃动，影响沉浸体验。

### 41.2 架构实施与细节合同
1. **全屏设置桌面工作台（`SettingsWorkbench.tsx`）**：
   - 淘汰居中小窗口，采用 100% 视口高度全屏桌面工作台；
   - 左侧配备固定导航栏（`SettingsRail`），支持三核心功能区切换：
     - `01 工作区与排版（workspace）`：工作区存储占用、中西文字体排版卡片、KaTeX 公式实时预览与重置数据区；
     - `02 AI 模型服务（models）`：多 Provider 实例动态管理、连接测试与密钥配置；
     - `03 提示词模板（prompts）`：19 个注入槽位 Master-Detail 提示词编辑器；
   - 支持全局 `Esc` 键或点击 `← 返回阅读` 秒级返回主阅读器。
2. **动态多 Provider 实例管理（`SettingsModelsPage.tsx`）**：
   - 支持动态创建最多 10 个 Provider 实例，每个实例拥有独立的 UUID、用户自定义名称、Kind（`gemini` | `openai_compatible` | `grok`）、模型与独立存储在 Credential Manager 中的 API Key；
   - 支持对 Provider 进行动态重命名、复制（Duplicate）、拖拽排序（Reorder）与安全删除；
   - **保存自动设为当前（Auto-Activate Current Provider）**：在用户保存某个 Provider 设置、新增 Provider 或复制 Provider 时，系统自动调用 `set_current_provider` 设为当前激活模型，免去用户手动再点一次的遗忘成本。
3. **视觉化中西文字体卡片与 KaTeX 双向公式预览（`SettingsWorkspacePage.tsx`）**：
   - 提供 5 种西文字体卡片（Consolas、JetBrains Mono、Inter、Source Serif 4、Computer Modern）与 4 种中文字体卡片（霞鹜文楷、思源黑体、思源宋体、微软雅黑）；
   - 配合 KaTeX 渲染实时数学公式预览。
4. **提示词视口锁定（Viewport-Locked Master-Detail Layout）**：
   - 进入提示词标签页时，外层容器开启 `.settings-content-prompts`（`overflow: hidden; height: 100%;`），杜绝外层页面滚动；
   - 左侧槽位列表（`.prompt-groups-scroll`）独立滚动，底部设置 `padding-bottom: 32px;` 留白；
   - 右侧编辑器自适应撑满，`<textarea className="prompt-monaco-textarea">` 配置：
     ```css
     box-sizing: border-box;
     white-space: pre-wrap;
     word-break: break-word;
     overflow-wrap: break-word;
     overflow-y: auto;
     resize: none;
     ```
     彻底杜绝长文字在右侧边缘被截断的问题。

---

---

## 42. OCR 级联删除、Orientation 深度编辑/图钉锁定与视域切换避坑指南 (2026-08)

### 42.1 业务背景与改动清单
1. **OCR 误识别与重试需求**：
   - 用户需要删除并重新识别 OCR，但不能破坏已有的研讨对话（Discussion）、全篇摘要（Brief）、元数据（Metadata）及学术术语/符号表；
   - 落地 `delete_ocr_cascade` 命令与双栏确认弹窗 `CascadeOcrDeleteConfirmModal.tsx`。
2. **Brief 标签与 Library Hub 标签联动**：
   - Brief 标签移至 Takeaway 下方，并在 Hub 与 Brief 统一支持 `update_paper_tags` 增删管理。
3. **Metadata、术语表与符号表深度编辑与图钉 📌 锁定**：
   - 全各列支持就地 ✎ 编辑，保存后自动 📌 图钉锁定；支持整行删除与末尾新增词条；
   - 真实落库 `artifacts.content_json`（含 `_pinned` 列表），重生成时后端自动合并，保证锁定的条目永不被覆盖且后续模型上下文（Translation 术语对齐、Reading Guide 旁批、Discussion 对话）完全读取最新内容。
4. **Lens QA 追问建议与术语同义词/符号作用域的 LaTeX 公式渲染**：
   - 追问建议芯片与表格各列接入 KaTeX 富文本渲染；点击建议芯片填入输入框而非直接发送。
5. **讨论长对话渐进式分页加载与 Compaction 呼吸动画**：
   - 初始窗口只挂 15 轮（30 条节点），向上滚动自动扩充，时间轴刻度跳转智能扩窗；
   - 后台压缩上下文时触发 `.discussion-compaction-pulse-indicator` 呼吸脉冲提示。
6. **全篇成果与 Lens 顶栏即时视域切换与状态记忆 (Memory Retention)**：
   - 点击顶栏 Tab 直接刷新主视图，记住上一次查看的具体成果与 Lens 选区，消除二次点击。

---

### 42.2 踩坑与避坑指南 (Critical Pitfalls to Avoid)

1. **OCR 级联删除范围边界（Cascade Deletion Boundaries）**：
   - ❌ **错误做法**：将 `artifacts` 表中该论文的所有 Artifact 一锅端删除。这会导致用户费尽心思校正的 Brief、学术标签、术语表、符号表以及几十轮深入研讨的聊天记录全被清空！
   - ✅ **正确做法**：OCR 删除严格只清理依赖 OCR 空间坐标的子资产：
     - **清理项**：`translation`、`explanation`、`lens_formula`、`lens_figure`、`lens_table` 以及 `ocr_revisions`；
     - **保留项**：`brief`、`metadata`、`glossary`、`symbol_table`、`messages` 与 `discussion_threads`；
     - 清理后将 `artifact_heads` 中对应的 OCR 依赖 kind 指针安全删除。
2. **术语表/符号表图钉 📌 锁定与模型重生成合并（Pinned Entries Merge）**：
   - ❌ **错误做法**：重新生成 Orientation Pack 时直接抹掉原有 `content_json`，或者前端只在本地 state 里缓存 `overrides` 而不写入后端。
   - ✅ **正确做法**：
     - 修改通过 `update_orientation_table` 直接持久化至 SQLite `artifacts.content_json`（格式为 `{"entries": [...], "_pinned": ["term1", "term2"]}`）；
     - 重新生成时，后端先读取已有 Head 的 `_pinned` 列表与旧 entries，与大模型新生成的内容进行合并：同名以用户钉住的为准，模型新产出的条目追加合并，模型缺失的用户钉住条目完整保留。
3. **jsdom 对 CSS 伪类/选择器解析引发的测试崩溃（jsdom style-rules Crash）**：
   - ❌ **错误做法**：在原生 CSS 文件中写 SCSS 风格的选择器，或者在 `style={{ ... }}` 中传入导致 jsdom 计算样式失败的特殊对象。这会导致 `@testing-library/react` 的 `getByRole` 在调用 `isInaccessible -> window.getComputedStyle` 时报 `TypeError: Array.prototype.forEach called on null or undefined`。
   - ✅ **正确做法**：保持原生 CSS 标准语法（如 `.meta-mini-btn.danger:hover`），并在测试中为动态生成的按钮显式添加 `aria-label`。
4. **视域切换（Scope Tab）与主视图活跃成果（Active Group）不同步**：
   - ❌ **错误做法**：切换视域时仅修改顶栏高亮状态，而 `activeArtifactId` 依然停留在另一个视域。由于 `activeGroup` 贪婪匹配 `activeArtifactId`，导致主视图依然停留在旧视域的卡片上，用户必须点下方的小标签才能刷新。
   - ✅ **正确做法**：
     - 在 `App.tsx` 中分别追踪 `lastGlobalArtifactId` 与 `lastBlockArtifactId`；点击顶栏 Tab 时自动调用 `selectArtifact` 恢复对应视域上次浏览的成果；
     - 在 `ArtifactPanel.tsx` 中，`activeGroup` 的计算逻辑优先从当前 `scopeTab` 所属的分组（`globalGroups` 或 `blockGroups`）中寻找活跃或记忆的成果，确保视域与主视图 100% 绝对一致。
5. **Lens QA 追问建议芯片的 LaTeX 渲染与输入框填入（Suggested Questions Chip）**：
   - ❌ **错误做法**：直接把 LaTeX 原文字符串作为纯文本渲染（导致 `$z_i$` 原样显示），或者点击时直接调用 `onAskLens` 发送（用户无法在发送前修改）。
   - ✅ **正确做法**：渲染时使用 `<Markdown>{wrapBareInlineMath(q)}</Markdown>` 保证 KaTeX 公式排版精美；点击时调用 `setQuestion(q)` 并聚焦输入框，保留用户在发送前的二次编辑权。

## 42. 侧边栏顶栏响应式弹性收纳、去冗余与防穿透重叠重构 (2026-08)

### 42.1 业务背景与问题根因
- **现象**：当用户拖动改变侧边栏宽度至较窄状态（300px~450px）时，顶栏（`compact-rail-header`）中的 Tab 组、分支下拉、模型徽章、生成元信息长文本、重新生成按钮、字号调节器以及全屏讨论树按钮直接在同一行上互相穿透重叠，视觉极度混乱。
- **根因**：
  1. **元信息冗余**：成果 Tab 下同时显示了 `[ 🟢 Gemini 3.7 Flash ]` 模型徽章与 `由 Gemini 3.7 Flash 生成 · 13:04`，模型名称重复占用 ~160px；
  2. **缺少响应式收纳**：字号调节器与全屏讨论树在窄屏下没有折叠机制；
  3. **Flexbox 缺少容器保护**：没有设置 `min-width: 0` 与弹性截断，导致元素超出容器边界。

### 42.2 架构实施与细节
1. **生成元信息下移沉底**：彻底从顶栏移除生成元信息与时间戳，统一放置在阅读成果内容的最底部（`.artifact-detail-meta-footer`），既为顶栏释放了大量横向空间，又彻底避免了用户将生成时间误认为当前系统时间；
2. **Tab 组图标化与超窄屏自适应**：
   - 「讨论」Tab 新增 `💬` 聊天图标；
   - 宽模式（`chatWidth >= 460px`）：展示完整文字 `💬 讨论`、`🌐 全篇成果 (N)`、`🔍 Lens (M)`；
   - 窄模式（`chatWidth < 460px`）：文字隐藏，自适应展示纯图标与数量标 `💬`、`🌐 N`、`🔍 M`，极限节省顶栏空间；
3. **响应式折叠菜单与弹窗防裁剪修复**：
   - 核心功能直出（Tab 组、模型徽章、重新生成按钮、分支切换）；
   - 低频次要操作（字号调节器 `A- 15 A+`、全屏讨论树 `⛶`、新建分支 `＋`）自动收纳进 `···`（更多操作）下拉菜单中；
   - 必须保持 `.compact-rail-header` 的 `overflow: visible`，防止绝对定位的 `···` 下拉浮层被 38px 顶栏容器裁剪。
4. **Flexbox 容器防重叠保障**：
   - 内部 `.compact-rail-left`、`.compact-rail-right` 设置 `min-width: 0`；
   - 模型徽章与分支下拉设置合理的 `flex-shrink` 与省略号截断（`text-overflow: ellipsis`）。

### 42.3 踩坑指南与前车之鉴（必读）
1. **父容器 `overflow: hidden` 误伤绝对定位下拉菜单（Absolute Popover Clipping Bug）**：
   - ❌ **错误做法**：为了防止 Flexbox 子项宽度外推溢出，在顶栏容器（`.compact-rail-header`、`.compact-rail-left`、`.thread-title-picker-wrap` 等）上盲目声明 `overflow: hidden`。这会导致直接子级绝对定位的下拉浮层（如 `thread-dropdown-menu`、`rail-more-dropdown`）被 38px 固定高度的顶栏容器直接裁剪截断，用户点击按钮没有任何视觉反应；
   - ✅ **正确做法**：包含绝对定位弹出层的父级容器链**必须保持 `overflow: visible`**；溢出省略号截断必须精确下发至最内层的内容 `<span>`（例如 `.thread-title-text { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }`），并为弹出菜单赋予足够高的 `z-index`（如 `70`）。
2. **多态组件测试与可访问性名称映射（Accessible Name Mapping in jsdom）**：
   - ❌ **错误做法**：当 Tab 按钮在窄屏下自适应收缩为纯 Emoji 图标（如 `💬`）时，如果在测试中通过 `getByRole("button", { name: "讨论" })` 查询，jsdom 的 Accessible Name 计算仅包含可见文本，导致测试失败；
   - ✅ **正确做法**：为所有支持纯图标模式的按钮显式赋予 `aria-label="讨论"` 和 `title="讨论 (Discussion)"`，这样既提升了无障碍可访问性，又确保单元测试在宽/窄屏模式下均能稳定定位元素。
3. **时间戳歧义与信息层级归位**：
   - ❌ **错误做法**：在顶栏或卡片头部同时显示模型胶囊与时间戳。用户容易将其误认为当前系统的实时时间，且占用了极为宝贵的顶栏宽度；
   - ✅ **正确做法**：将生成模型与生成时间下移统一沉底至内容最末尾（`.artifact-detail-meta-footer`），顶栏仅保留功能操作与视域切换。

## 43. Gemini Proxy (Antigravity-Manager) 深度集成、PDF 内联协议转换与思考强度架构 (2026-08)

### 43.1 业务背景与架构选型
- **目标**：为 Read Desktop 引入对本地 **Antigravity-Manager** 代理服务（默认端口 `http://localhost:8045/v1`）的一等公民支持，让用户能够直接使用本地代理转发的 Google Gemini 3.x 等最新大模型研读长文与 PDF，无需直接绑定官方 API 密钥。
- **架构选型（为什么走 `ChatCompletionsAdapter` 而非 Native Gemini）**：
  1. Antigravity-Manager 是一个 OpenAI-Compatible 代理服务，暴露 `/v1/chat/completions` 与 `/v1/models`；
  2. 原生 Gemini 依赖 Google Files API 与服务端的交互 Session；而 OpenAI-Compatible 协议栈在 Read Desktop 中已具备完整的本地 SQLite 对话树回溯、历史消息修剪与多轮会话恢复能力；
  3. 因此，将 `GeminiProxy`（`"gemini_proxy"`）作为独立的 `ProviderKind`，复用成熟的 `ChatCompletionsAdapter` 架构，并在其底层做针对 Antigravity-Manager 特性的协议层兼容转换。

### 43.2 踩坑指南与前车之鉴（必读 ⚠️）

1. **PDF 附件上传触发 400 Bad Request（/v1/files 缺失与 OpenAI 文件格式缺陷）**：
   - ❌ **错误做法**：在 OpenAI 兼容模式下，默认先调用 `/v1/files` 上传 PDF，或者在 `messages` 里发送 `type: "file"`。Antigravity-Manager 没有实现 `/v1/files` 端点，且其 OpenAI 翻译层不支持 `type: "file"`，会导致直接报错 400。
   - ✅ **正确做法**：
     - 在 `gemini_proxy` 模式下直接跳过 `/files` 探测与上传，全量走客户端 Base64 内联；
     - 将 PDF 打包为 `type: "image_url"` 发送（`data:application/pdf;base64,...`）；
     - Antigravity-Manager 的 Data URI 解析器能够自动拦截 `data:application/pdf;base64` 并将其无缝映射为 Gemini 官方原生支持的 `inlineData { mimeType: "application/pdf" }`。

2. **结构化输出 `json_schema` 静默失效或报错**：
   - ❌ **错误做法**：向代理发送带有 schema 定义的 `response_format: {"type": "json_schema", "json_schema": {...}}`。Antigravity-Manager 仅支持 `type: "json_object"`，会丢失 schema 约束。
   - ✅ **正确做法**：
     - 在 `gemini_proxy` 模式下，将 JSON Schema 格式化后直接注入到 System Prompt 的前置强制指令中（`[IMPORTANT: You MUST respond with a valid JSON object strictly adhering to this JSON Schema...]`）；
     - 将请求体的 `response_format` 降级为 `{"type": "json_object"}`，保证代理服务器和上游 API 都能稳定解析。

3. **非 JSON 报错引发晦涩的 `error decoding response body`**：
   - ❌ **错误做法**：使用 `response.json::<Value>().await` 直接反序列化 HTTP 响应。当代理服务在 400/500/503 时返回纯文本或 HTML 错误页时，反序列化器会抛出 `error decoding response body`，将真实的错误原因掩盖；
   - ✅ **正确做法**：在 `checked_json` 中优先读取 Raw Bytes（`response.bytes().await`），若 JSON 解析失败，则通过 `String::from_utf8_lossy` 提取真实响应文本并格式化为 `Provider HTTP {status}: {text}`，让错误原因一目了然。

4. **HTTP 503 `Proxy service is currently disabled` 状态诊断**：
   - ❌ **错误做法**：直接将 503 报错原样抛给用户，用户不清楚为什么已启动 Antigravity-Manager 却仍然无法连接；
   - ✅ **正确做法**：Antigravity-Manager 的 Axum 服务器由 `service_status_middleware` 控制，当应用打开但主界面的代理总开关未开启时，会对所有 API 返回 503 `Proxy service is currently disabled`。我们在后端针对该特征返回中文明确提示：
     > `“Antigravity-Manager 代理服务尚未开启。请在 Antigravity-Manager 软件中点击开启‘代理服务 (Enable Proxy)’开关。”`

5. **Gemini 3.x 家族思考强度（Thinking Level / Variant）分级与命名规范**：
   - ❌ **错误做法**：使用过时的 Gemini 2.5 列表，或者将思考强度作为无法选取的隐式参数；
   - ✅ **正确做法**：
     - Flash 系列（`Gemini 3.7 Flash` / `Gemini 3.6 Flash` / `Gemini 3.5 Flash`）支持 `Low` / `Medium` / `High` 三档思考；
     - Pro 系列（`Gemini 3.1 Pro`）支持 `Low` / `High` 两档深度思考；
     - 辅助轻量模型（`Gemini 3.1 Flash Lite`）作为极速无思考版本推荐给翻译/生词；
     - 前端表单提供分段按钮，实际下发模型 ID 自动组合为规范后缀（如 `gemini-3.7-flash-high`、`gemini-3.1-pro-high`），完美契合 Antigravity-Manager 的 `variant_mapping` 规则。

---

### 33. Hub 整理（右键菜单与拖拽）—— 空目录一等公民
- **数据来源**：Hub 树由 `list_collections`（SQLite `collections`）驱动，含空目录；计数仍按 `documents` 前缀匹配；`reconcile` 同时登记/清理空目录。
- **三档权限**：`全部文档` 虚拟过滤不可落、仅导入/打开Workspace；`Papers/Textbooks` 根可加子目录/导入/打开、禁改名/禁入废纸篓/禁被拖；普通子目录完整菜单。
- **文件夹废纸篓=级联 trash_paper**：子树每篇进 30 天回收站（停任务、登记 tombstone），空目录立刻删磁盘+collections；确认框写目录名+N。
- **禁止跨根**：Hub 校验失败显示禁止光标；Explorer 跨根走 watcher + kindChange。
- **PDF 重命名**：词干校验同目录名规则，`.pdf` 锁死，提交后同步 `paper_metadata.title`。
- **IPC**：`list_collections` / `create_collection` / `rename_collection` / `move_collection` / `trash_collection` / `rename_paper`（`fileName` 已进 `DocumentCard`）；`open_resource_dir` 对文件 `explorer /select`，对目录 `explorer`；前端 `LibraryHub` 用 pointer 8px 阈值、幽灵预览、高亮/禁止样式、8s Toast 撤销（仅最后一次）。
- **接线**：`App.tsx` `refreshDocuments` 同拉 `list_documents` + `list_collections`，`library` 事件同刷新；Hub 选中 `selectedFolder` / `hubSelection` 区分，右键先选中不打开。

### 34. Lens / Brief Markdown 写入归一化

写入层仍在 `normalize_markdown_field`（Lens 校验后、Brief `parse_orientation_pack`）。**显示层才是历史成果的修复面。** 规则表、样本、禁止项和测试锁以 [markdown-rendering.md](markdown-rendering.md) 为准；本页增量记录见 **§44**。不要继续扩写本节，避免和 §44 再分叉。

不要：无条件 `replace("\\n")`；不配对就给 CJK 插空格；看见 `N. ` 就换行；把用户工作区 SQLite 写回「修好的」markdown。

## 44. 阅读成果 Markdown：挤在一起的列表与双星号粗体（2026-08-29）

现场：Brief「主要发现与结论」`1. …；2. …；10. …` 全粘在第一条；Figure Lens 里 `**监督学习 **` 星号露出来。库里 Lens 文本其实是合法 `**监督学习**：`。

- **显示层**（必须）：`prepareMarkdown` 配对修 `**`、按终止符/连续编号拆列表。所有 `MarkdownBody` 立刻受益，包括已落库 Brief。
- **写入层**：同一套拆行/转义规则用于新 Brief 与新 Lens。`is_list_terminator` 现在会用；`?` / `,` 只在后面是列表标记时拆。
- **不要做**：改 `<Workspace>/.read-desktop`；用旧 `fixCjkEmphasis` 那种「汉字+星号+标点 ⇒ 开标签」正则；对整个 `src-tauri` 跑 `cargo fmt --`。

验收：刷新 WebView 即可，不必重生成。合同 [markdown-rendering.md](markdown-rendering.md)。

## 45. Hub 手动排序与教材章节排序（D-062）

完整合同、文件入口、踩坑日志、如何加排序：[hub-sort.md](hub-sort.md)。ADR [decisions.md D-062](decisions.md)。

现场：叶子文件夹里拖卡片手排（写入 SQLite，不升 schema 8）；教材按 Metadata `chapterNumber` 视图排序。默认仍是「导入时间」。不要从文件名猜章节，不要让同层 F2 改名删 order 行。

验证命令见 [hub-sort.md §7](hub-sort.md)。

## 46. Windows 开发构建（栈 vs LLVM OOM）

完整说明：[windows-dev-build.md](windows-dev-build.md)。

- 应用启动 `0xc00000fd` → 主线程栈，已由 `build.rs` `/STACK:8388608` 处理。
- `tauri dev` 编译 `rustc-LLVM ERROR: out of memory` / `0xc0000409` → rustc 内存，不是栈。dev profile + `build.jobs = 2` 已落地；仍炸则删 `src-tauri/target/debug/incremental`。
- `tauri dev`：`cargo metadata` / `program not found` → **PATH 里没有 cargo**，不是缺依赖。本机 cargo 在 `<CARGO_HOME>\bin`。只设 `CARGO_HOME` 不够，旧终端看不到新 PATH。见 [windows-dev-build.md](windows-dev-build.md) **§4.1**。

## 47. 文库 Workspace（D-063）

完整实现手册、不变量、文件入口、扩展菜谱、踩坑、**可追加变更日志**：[library-workspace.md](library-workspace.md)（先读 **§0、§1、§11 最新一条**）。计划切片与 Gate：[library-workspace-plan-2026-08.md](library-workspace-plan-2026-08.md) **§11.0**。线格式：[data-protocols.md](data-protocols.md)。排序子合同：[hub-sort.md](hub-sort.md)。

现场（2026-09-04）：Hub 卡片走 `hub_page` 分页虚拟窗口；手排提交走 `collection_layer`（随 `hub_page.revision` 刷新）；`chapter` 用 SQLite `chapter_sort_key`（`1.2` < `1.10`）；watch 先 listen 再握手；Explorer drop 与多文件 picker 同一 Import Batch（500 源，非 PDF 逐项 skipped）；多选拖目录走 Move Batch；Undo Token 明文在 `undoTokenStore` 记 10 分钟，任务中心可「撤销整批」。卡片把手 / 勾选尺寸在 `.hub-paper-list` 的 `--hub-*` 变量（手册 **§6.7**），勾选热区是 `.hub-select-hit`。`list_documents` 仍给树计数，不要删。

不要：用当前页 id 当 D-062 置换；把 Smart Collection 当文件夹；按 90%/95% 自动已读；`watch` 只认 `kind === "library"`；`ORDER BY m.chapter_number`；把 `queryDigest` 当 Selection Snapshot；前端 `filter(.pdf)` 丢掉非 PDF；`memberIds.length > 1` 时 `showHint(bulkDisabledReason)`（client 接通后是空串）；无 Tauri 窗口却把实机 Explorer / P95 写成已验收。不要把计划 §1 当 HEAD。

验证命令见 [library-workspace.md §8](library-workspace.md)。踩坑只追加到该页 §7。本轮工作记录追加到该页 **§11**。

## 48. 选区复制为 GFM（2026-09-04）

现场：Lens / 讨论 / Brief 里划有序列表，`Ctrl+C` 没有 `1.`；划公式没有 `$` / `$$`。

完整合同、文件入口、如何加语法/表面、**只追加的踩坑表 P1–P10**：[markdown-rendering.md](markdown-rendering.md) **§9**。本页 **§18** 只留入口，不要和 §9 再分叉。

- **显示层 DOM → GFM**，不是源切片，不写回 SQLite。
- 列表 marker 与公式 delimiter 碰到就补全；粗体按选区切。
- 跨根只挂讨论流 / 成果详情 / Lens 卡片与 QA。
- `rehype-katex` 必须 `htmlAndMathml`。不要为了 jsdom 改回 `output: "html"`。

- 验收：`src/markdown.test.ts` + `MarkdownBody.test.tsx` + ArtifactPanel 跨节 copy；实机划 Brief 列表和公式。合同 §9.9。

## 49. Lens 区块卡片流与深度探讨（D-064 / BlockCardStream，2026-09-04）

现场：原先段落翻译、解释、Lens 细碎平铺成横向超长 Chip 列表，视觉上下文割裂且列表冗长；进入独立讨论后又存在贴边拥挤、黑框与图表丢失问题。

完整架构手册、不变量、三级图表截屏链路、吸底 3 行自适应算法与**踩坑实录 P1–P8**：[lens-block-cards.md](lens-block-cards.md)。

- **物理阅读流聚合**：卡片流严格按 `pageNumber` + `blockIndex` 阅读顺序排列；
- **富预览与防误触**：公式 KaTeX / 图表缩略图 / 文本行号预览；未生成 Tab 呈现预检卡片，绝不静默调 API 消耗 Token；
- **双层轻量架构**：外层卡片流保持紧凑不内嵌多轮对话；点击「💬 深入探讨此区块 →」推入沉浸式专属问答页 (`BlockCardQaDetail`)；
- **三级高可用图表加载**：预载图 -> 本地持久化缓存 (`get_artifact_asset_path`) -> 精确 Bbox PDF Canvas 动态裁剪；
- **固定吸底与多行撑高**：输入框固定在最底部并极致压缩底边空白；1~3 行无滚动条动态自适应撑高（`overflow-y: hidden` 消除 Windows 滚动小箭头），超 3 行激活 4px 极细滚动条。

验收：`src/components/BlockCardStream.test.tsx` + `src/ArtifactPanel.test.tsx`（17 测试通过）以及全工程 59 个测试套件（400 测试 100% 通过）。

## 50. 读者上下文（D-065，2026-09-07）

用户在全部文档 / 物理文件夹 / 单篇 PDF 写「已掌握、阅读目的」。磁盘 md 是唯一权威；F2 任务在 **user_input** 里读带标签包装块；19 槽仍是唯一 system。

完整不变量、文件入口、G2 叠层、Job 冻结、sidecar 生命周期、编辑器、**踩坑表、如何加消费者、变更日志**：[reader-context.md](reader-context.md)。动手前读该页 **§0、§1、§2、§7**。

- 文件名：工作区根与文件夹 `read-desktop.reader.md`；PDF `{stem}.read-desktop.reader.md`。空不建文件。
- 不要和 `get_reading_context`（阅读器滚动）撞名。不要 recursive 监视 Workspace 根。不要给对话框加 `.scrim`（会把弹窗推到右边）。
- 按槽打磨出厂稿时不要再改这套机制代码。

## 验证命令清单

文库专项命令以 [library-workspace.md §8](library-workspace.md) 为准。提交前最低限度：

```powershell
npx vitest run --maxWorkers=1 --fileParallelism=false
npx tsc -b
npm run build
cd src-tauri
cargo test --locked
```








## 教材22槽适配（2026-09-11）

先读 [教材生成合同](textbook-generation.md) 和 [22槽完整教材稿](note/textbook-prompts.md)。教材Brief独立textbook-v2九字段，不复用论文研究评价字段；教材Lens新默认使用v2。textbook_contract.rs维护模型字段与旧教材源稿，prompt_settings.rs维护教材迁移及协议历史。单篇和Hub批量分别在lib.rs/document_artifacts.rs、library_batch.rs冻结协议；Lens通过document_kind选择对应版本默认。旧稿/current/previous与论文侧必须保留；不要从正文相似度猜测已记录的协议身份，不恢复额外英文F2段。

本轮完整Rust552项、相关前端68项与生产构建通过。真实模型内容和桌面视觉待验收；不要把自动化通过等同于学术或教学质量保证。
