# 文库 Workspace 实现手册（D-063）

- 状态：PR 0–6 **代码已落地**；实机 Explorer drop / Windows P95 仍属发布门槛
- 日期：2026-09-03（本页可追加，不覆盖旧条）
- ADR：[decisions.md D-063](decisions.md)
- 计划合同（切片、Gate、DoD）：[library-workspace-plan-2026-08.md](library-workspace-plan-2026-08.md)（动手前读 **§11.0**）
- 排序子合同：[hub-sort.md](hub-sort.md)（D-062）
- 线格式：[data-protocols.md](data-protocols.md)「Schema 8 revision 与 `library_read`」
- 领域词：[CONTEXT.md](../CONTEXT.md)

**本页是实现手册与踩坑日志，不是产品愿景。** 代码与本页冲突时先改本页或 ADR，再改代码。下一个 agent：先读 **§0、§1、§2、§7、§10、§11 最新一条**，再动文件。

## 0. 现状一句话

Hub 只学 `view + dispatch`。桌面 seam 只有 `library_read` / `library_act` / `watch`（事件名仍是 `read-event`）。卡片列表走 `hub_page` 分页虚拟窗口；手排提交走 `collection_layer` 的整层 live id（随 `hub_page.revision` 刷新）；批量走持久 Batch；多选拖进物理目录也走 Move Batch；Explorer drop 用指针下的物理目录，非 PDF 原样进 Import Batch。Undo Token 明文只在前端记 10 分钟（`undoTokenStore`），任务中心与 8 秒 Toast 共用。阅读状态与 Reader 视口分开；智能集合是查询不是文件夹。`list_documents` **还在**，只给侧栏树计数和 Reader 恢复目标，不是 Hub 卡片源。

不要根据旧 handoff / 计划正文里的「尚未开始」推断现状——以本页 §0 与计划 **§11.0** 为准。

## 1. 不变量（改任何文库代码前对照）

违反任一条就是回归，不要用「先打通再修」绕过。

| # | 不变量 | 反例（禁止） |
| --- | --- | --- |
| I1 | D-062：物理叶子文件夹的手动顺序是 **全部 live Paper 的精确置换** | 用当前页 / 搜索结果 / 虚拟窗口 id 当 `paperIds` |
| I2 | Smart Collection 是版本化查询，不复制成员，不是目录 | 当拖放目标、参与手排、物化成磁盘文件夹 |
| I3 | 未读 / 阅读中 / 已读 **绝不**按 90% / 95% 静默完成 | `furthestPage / pageCount ≥ 0.9` 自动标已读 |
| I4 | 读失败 ≠ 空文库 | `workspace_unavailable` 时清空卡片 |
| I5 | 事件只当提示；是否失效只看 `library_change_seq` | `watch` 只认 `kind === "library"` |
| I6 | 费用 unknown 不得显示为 0；预览不发网络、不泄露 Key / route / 绝对路径 | 缺价目就写 `¥0` |
| I7 | 共享 Provider Job：唯一 created owner；`cancel_remaining` 不取消别人正在用的 Job | 按 Job id 一律 cancel |
| I8 | 旧浅 IPC（`list_documents` 等）只在仍有调用者时保留 | 为「干净」一次删掉仍被 App 树计数使用的命令 |
| I9 | 导入只接受普通 PDF；一次最多 **500** 源；非 PDF / 目录是逐项跳过 | 递归拖文件夹；静默丢掉超额文件 |
| I10 | `manual` / `chapter` 的 `hub_page` 必须带 **精确** collection 过滤（`recursive: false`） | 在「全部」或含子孙视图上跑 chapter SQL |
| I11 | 拖已选多篇到物理目录必须走 Move Batch；`bulkDisabledReason` 在 client 接通后是空串，不能当失败提示 | `memberIds.length > 1` 时 `showHint("")` 然后 return |
| I12 | Explorer / picker 一次最多 500 **全部源 path**；目录和非 PDF 进 Item `skipped_*`，前端不得先 `filter(.pdf)` | 混拖 `.pdf`+`.txt` 只导入 PDF、txt 从批次里消失 |
| I13 | Undo Token 明文只在 Start 返回；库里只存 hash。前端用 `undoTokenStore` 记满 10 分钟，Toast 与任务中心共用 | 任务中心文案写「可撤销」却没有按钮；8 秒后无法撤销 |
| I14 | `collection_layer` 必须随 `hub_page.revision` 重读 | 手排后下一刀仍用进夹时的旧 layer id |
| I15 | `read-desktop.reader.md` 与 `*.read-desktop.reader.md` 是读者上下文，不是论文；reconcile / 导入必须忽略。Hub 移动/回收站带着 sidecar 走 | 当未管理 PDF 导入；删文件夹时把仅含 md 的目录直接 `remove_dir_all` |

## 2. 分层与文件入口

```
LibraryHub (view + dispatch)
  → useLibraryWorkspace          选择 / 焦点 / 树 / planReorder
  → LibraryWorkspaceClient       read / act / watch
       ├─ Tauri  → library_read / library_act / read-event
       └─ Memory → 浏览器预览（不是第二份权威）
            → LibraryQueryModule     hub_page / handshake / collection_layer
            → LibraryWorkflowModule  Batch / Lifecycle / Smart Collection / Job 编排
```

### 2.1 前端（`src/library/`）

| 要动的事 | 先看 | 测试 |
| --- | --- | --- |
| 线格式镜像（改字段必改 Rust + fixture） | `libraryWorkspaceTypes.ts` | `libraryWorkspaceContract.test.ts` |
| Tauri / Memory Adapter | `libraryWorkspaceClient.ts` | 同上 |
| `act` 判别联合、确认流 | `libraryActTypes.ts`、`memoryLibraryAct.ts` | `libraryAct.memory.test.ts` |
| Selection Set / Snapshot | `selectionModel.ts` | `selectionModel.test.ts` |
| 落点 / 唯一 reorder 口 | `dropPolicy.ts` | `dropPolicy.test.ts` |
| 目录树展开记忆 | `treeModel.ts` | `treeModel.test.ts` |
| Hub 筛选 AST | `hubFilters.ts` | `hubFilters.test.ts` |
| `hub_page` 卡片 → `DocumentCard` | `hubDocument.ts` | Hub 组件测 |
| 分页读 + watch 刷新 | `useHubPageList.ts` | `LibraryHub.interaction.test.tsx` PR 6 |
| 交互控制器 | `useLibraryWorkspace.ts` | 组件测 |
| 虚拟窗口 | `virtualWindow.ts` | `virtualWindow.test.ts` |
| Explorer / Webview drop 判定 | `importDrop.ts` | `importDrop.test.ts` |
| Undo Token 明文缓存（10 分钟） | `undoTokenStore.ts` | `undoTokenStore.test.ts` |
| 字节合同夹具 | `__fixtures__/library_read_v1.json` | Rust `the_shared_fixture_is_reproducible…` |

`LibraryHub.tsx` 只消费 binding，不要再往 `App.tsx` 堆批次编排。`App.tsx` 负责注入 `libraryClient`、`onImportPaths`、树用的 `list_documents` / `list_collections`。

### 2.2 后端（`src-tauri/src/`）

| 要动的事 | 先看 | 测试入口 |
| --- | --- | --- |
| schema 8 / v7→v8 | `v2_workspace.rs` | `v2_workspace` migration 测 |
| 连接 PRAGMA + `chapter_sort_key` | `db.rs` `configure` | 随 `library_query` |
| 章节排序键（与 `hubSort.ts` 同算法） | `chapter_sort.rs` | `chapter_sort::tests` |
| `library_read` 分派 | `library_query.rs` | `cargo test --locked --lib library_query` |
| 全局 / 领域 revision | `library_workflow.rs` `bump_library_revisions` | 写侧 `every_hub_visible_write…` |
| Batch 状态机 | `library_batch.rs` | `library_batch::tests` |
| Lifecycle / Smart Collection | `library_lifecycle.rs` | 同 crate 内测 |
| 费用预览 | `library_cost.rs` | `library_cost` |
| Prepared Job Handle | `job_module.rs` `prepare_exact_on` / `consume_prepared_on` | JobModule prepared 测 |
| 浅 IPC（树、单篇） | `library_commands.rs` + `paper_module.rs` | 既有 paper 测 |
| 读者上下文 sidecar（D-065，不是文库成员） | [reader-context.md](reader-context.md)；`reader_context.rs` | `reader_context` / `move_and_trash_paper_take_reader_sidecar` |
| 命令注册 | `lib.rs` `#[tauri::command]` 必须留在这里 | — |

`rusqlite` 必须带 `functions` feature（`Cargo.toml`），否则 `chapter_sort_key` 注册失败。

## 3. 读路径

### 3.1 `hub_page`

- 默认 `limit=100`，硬上限 `200`。
- 一次有界聚合：COUNT + 一页 + extras（OCR / Job / Brief / metadata / lifecycle / engagement）+ 标签批读。**语句数不随文库增大**（200 与 10,000 都是 11 条）。
- 卡片只有相对路径。绝对路径、Key、endpoint、route 身份不准出现在投影 / 事件 / 诊断里。
- `queryDigest` 标识查询 AST（不含 cursor）。`selectionDigest` 标识成员集合（§5.1 Snapshot 用这个，**不要**把带 sort/limit 的 `queryDigest` 填进去）。
- `sort ∈ recent | year | title | manual | last_opened | chapter`。
- `chapter` 从当前 metadata **artifact** 的 `$.chapterNumber` 取值，不是 `paper_metadata.chapter_number`（该列不存在）。

### 3.2 `collection_layer`

物理叶子 collection 的 **全部** live id，按手动 `position` 再 `created_at`。给 D-062 精确置换。不带卡片。分页窗口再大也不能替代这一读。

Memory Adapter 的 `collectionId` 是路径字符串；Rust 是 collections 表主键。Hub **只用 `paperIds`**，`collectionId` 仍来自 `list_collections` 的 `activeCollection.id`。不要拿两边的 `collectionId` 做跨 Adapter 相等断言。

### 3.3 watch

1. 先 `listen("read-event")`，握手完成前把到达的事件记成 `pending`。
2. 再 `watch_handshake`。
3. `armed` 之后若 `pending`，立刻再握手一次。
4. 只有全局 revision **前进**才 `listener(next)`。
5. 握手失败必须 `unlisten`，不要留下悬空订阅。

Hub 的 `useHubPageList` 自己再订一份 watch：revision 前进则重读第一页。筛选变化才清空列表；纯刷新保留上一页避免闪空。

`invokeImpl` 测试替身是模块级函数，不是 `vi.fn()`。每个 `beforeEach` 必须 `invokeImpl = fakeInvoke`，否则握手缓冲测会污染后续用例。

### 3.4 虚拟窗口

- 宿主是 `.hub-paper-list`（`flex:1; overflow-y:auto`），**不是**整个 `.main-glass-stage`（里面还有搜索条和工具栏）。
- `count ≤ 48` 或 `viewportHeight === 0`（jsdom / 首帧）→ 不虚拟化。
- `ensureIndex` 必须包住键盘焦点，否则方向键落到未挂载的 `option`。
- `aria-setsize` = `hub_page.totalCount`（命中篇数），`aria-posinset` = 全列表下标，不是窗口内下标。
- Shift 范围 = **已加载页**的稳定顺序。Ctrl+A = `all_matching`，计数用 `totalCount`，不要 `visibleIds.length`。

jsdom 的 `Element.scrollTo` 经常不是函数。调用前检查 `typeof scrollTo === "function"`，否则 `scrollTop = 0`。

## 4. 写路径

`library_act` 判别联合：`change` / `plan_batch` / `start_batch` / `control_batch` / `record_reader_activity`。禁止 `{ command, payload }`。

本地动作（标签 patch、移动、回收站、导入、导出、生命周期）与 Provider 动作（OCR / Brief）都产生持久 Batch 与逐项结果。部分失败保留成功项。Undo 是补偿 Batch + Token，不是时间旅行，更不是退款。Token 明文只在 Start 返回一次，库里只存 hash；前端用 `undoTokenStore` 记满默认 10 分钟，8 秒 Toast 与任务中心「撤销整批」共用。

OCR / Brief：Plan 拿不透明 Prepared Job Handle；Start 在同一 `BEGIN IMMEDIATE` 里 consume / enqueue / coalesce / link / Item queued。不要再包一层同名方法——会无限递归（见 §7）。

导入：多文件 picker（`multiple: true`）与 Explorer / Webview drop 都进 `App.importPdfPaths` → 同一 Import Batch。落点：物理目录行 → 该目录；主列表 → 当前物理目录；根 → `{Root}/Inbox`；智能集合 / 「全部」→ 拒绝，不猜。一次最多 500 个源；目录和非 PDF **原样送进 Batch**，记为逐项 `skipped_not_pdf`，前端不得先滤掉。

## 5. Hub UI 接线要点

| 数据 | 有 `libraryClient` | 无 client（组件测 / 预览降级） |
| --- | --- | --- |
| 卡片 | `useHubPageList` → `documentCardFromHub` | `documents` prop + `applyHubSort` |
| Snapshot | `hub_page.selectionDigest` | `null` → Ctrl+A 不能提交 |
| 手排层 id | `collection_layer.paperIds` | `documents` 里该叶子的全部 id |
| 树计数 | 仍用 `documents`（`list_documents`） | 同左 |
| 打开 Reader | `paperById`（hub 页覆盖 list_documents 的 revision） | `documents.find` |

有 client 时 `useHubPageList` 的 `loading` **初始值必须是 true**，否则第一帧会闪「暂无文档」。组件测要点卡片时 `await waitFor`，不要假定同步画完。

`querySnapshotFromPage` / `hubPage.snapshot` 必须 `useMemo` 在 `page` 上。每帧 `new object` 会让 Hub 的 `useEffect` 死循环。

`loadMore` 与 watch 重读抢状态时用 `generation` 计数器丢掉过期响应，不要把旧 cursor 的第二页拼到新第一页后面。

卡片左侧 `.hub-card-controls`：把手在上、勾选在下，悬停 / 焦点 / 选中才显示。点卡片正文打开 Reader，不要改成长按。勾选框包在 `.hub-select-hit` 里并 `stopPropagation`，否则点热区会打开 Reader。尺寸只改 CSS 变量，见 **§6.7**。

## 6. 如何扩展（菜谱，可追加本节条目）

改完对应代码后，**必须**同步：本页相关小节、[data-protocols.md](data-protocols.md)、必要时 [feature-specs.md](feature-specs.md)、计划 §11.0。线格式一变就跑 §8 的 fixture 再生。

### 6.1 给 `library_read` 加一种请求

1. Rust `LibraryReadRequest` / `LibraryReadResult`（`deny_unknown_fields`，`rename_all = camelCase`）。
2. TS `libraryWorkspaceTypes.ts` 镜像；`libraryWorkspaceContract.test.ts` 的 `REQUEST_KEYS` 与 `asResult` 键集合。
3. Memory Adapter `read` 分支（语义对齐，实现可以简单）。
4. 若结果稳定、不含挂钟：加入 `build_library_fixture` 一条 case。含 `~\t` 这类需 JSON 转义的 sortKey **不要**进夹具（会打破「fixture 字节不含 `\\`」断言）。
5. `LIBRARY_FIXTURE=update` 再生夹具（§8）。

### 6.2 给 `hub_page` 加一种 sort

1. 前后端同一份键算法（能进 SQLite 就注册确定性函数，参考 `chapter_sort_key`）。
2. `HubSort` + `memorySortKey` + `hubSort.ts`。
3. 若只在一层内有定义：`validate_query` 要求精确 collection。
4. 下拉、`hubSortAvailability`、[hub-sort.md](hub-sort.md) §1 / §6。

### 6.3 给 Batch 加一种 command

1. `library_batch.rs` 策略：plan 资格、start 逐项、crash reconcile、补偿权限。不要假批次（UI 转圈但只改内存）。
2. `libraryActTypes.ts` 判别联合穷尽匹配。
3. Hub 工具栏 / 菜单：有 client 才可点；否则「即将支持」+ **该动作真正的阻塞原因**（不要复制粘贴 `BULK_DISABLED_REASON`）。
4. Provider 类动作必须走 JobModule prepared handle，禁止第二套限流器。

### 6.4 给筛选加谓词

白名单 AST + 绑定变量。相对日期用后端给的 `evaluationAnchor`（本周 = 该锚点所在周周一 00:00 UTC）。前端 `hubFilters.ts` / Memory `cardMatchesLibraryFilter` 必须同一语义。

### 6.5 让一次 Batch 能在任务中心撤销

1. Start / Control 成功后调用 `rememberUndoToken(batch.id, undoTokenOf(result), batch.undo?.expiresAt)`。
2. 8 秒 Toast 只是快捷入口；`OperationsDrawer` 用 `peekUndoToken` 在 `undo.state === "available"` 时显示「撤销整批」。
3. 撤销成功再 `consumeUndoToken`。不要把明文写进 SQLite 或投影。
4. `App.tsx` 里 Hub 视图和 Reader 视图各有一个 Drawer，两个都要接 `onUndoBatch`。

### 6.6 改 Explorer / Webview 导入落点

1. 纯判定放 `importDrop.ts`（`importDestinationFromHit` / `decideImportDrop`），不要在 Hub 里再滤一遍 PDF。
2. 有物理坐标：`cssPointFromPhysical` → `elementFromPoint` → `importHitFromElement`。
3. 目录行用该目录（根 → `{Root}/Inbox`）；主列表用当前物理目录；智能集合 / 「全部」返回 `null`，提示用户先选，不要猜。

### 6.7 改 Hub 卡片把手 / 勾选框尺寸

权威尺寸只在 `src/styles.css` 的 `.hub-paper-list` 上（网格和表格都在这个容器里，会继承）：

| 变量 | 用途 | 当前值 |
| --- | --- | --- |
| `--hub-handle-w` / `--hub-handle-h` | 拖拽把手按钮 | 36×32 |
| `--hub-checkbox` | 勾选框视觉边 | 20px |
| `--hub-select-hit` | 勾选框命中区（`<label class="hub-select-hit">`） | 32px |
| `--hub-controls-inset` | 卡片左侧控件距边 | 6px |

改尺寸时同步这四处，不要只改像素：

1. 上面四个变量。
2. `.liquid-paper-card` 的 `padding-left`：`calc(var(--hub-controls-inset) + var(--hub-handle-w) + 10px)`，给左侧叠放留白。
3. `.liquid-table-row` 的 `grid-template-columns` 前两列：`var(--hub-handle-w) var(--hub-select-hit) …`。
4. 勾选框必须包在 `.hub-select-hit` 里，label 上 `stopPropagation`。测用 `getByLabelText("选择 Alpha")`，不要拿掉 input 的 `aria-label`。

不要：`transform: scale` 放大原生 checkbox（布局盒不变，表格列仍按未缩放宽度排，视觉会溢出或被裁）。不要把把手做成永远可见（悬停 / 焦点 / 选中才显示）。不要把点按卡片改成长按拖拽。

## 7. 踩坑日志（只追加，不删旧条）

改代码时若再踩坑，在表末加一行：日期、症状、错误做法、正确做法。D-062 排序专坑继续追加到 [hub-sort.md §5](hub-sort.md)，本表不搬迁。

| 日期 | 症状 | 不要再做 | 正确做法 |
| --- | --- | --- | --- |
| 2026-08-31 | Hub 快照被事件 `kind` 骗成陈旧 | `watch` 只处理 `kind === "library"` | 任何合法 payload 都触发 `watch_handshake`；失效只看全局 seq |
| 2026-08-31 | OCR 进度让 Hub 狂刷 | checkpoint / `priority` 也 bump `jobs` 域 | 只有移动 `jobs.state` 的写走 `bump_jobs_state` |
| 2026-08-31 | 全选当前结果提交漂移 | 用篇数拼 cursor；用 `queryDigest` 当 Snapshot | Snapshot = `selectionDigest` + 依赖向量 + `evaluationAnchor` |
| 2026-09-03 | `hub_page` chapter 编译失败 | `ORDER BY m.chapter_number` | 章节在 metadata artifact JSON；`chapter_sort_key((SELECT json_extract…))` |
| 2026-09-03 | 分页后手排缺 id / 精确置换失败 | 用 `visibleIds` 或窗口 id 提交 | `collection_layer` 取整层 live id |
| 2026-09-03 | 有 client 的 Hub 测点不到卡片 | `getByLabelText` 写在第一帧 | `loading` 初始 true；`await waitFor` |
| 2026-09-03 | 第一帧闪「暂无文档」 | `useState(false)` 再在 effect 里改 true | 有 client 时 `useState(() => true)` |
| 2026-09-03 | 握手缓冲测污染后续 desktop 测 | `invokeImpl` 换成 delay 后不复位 | `beforeEach` 里 `invokeImpl = fakeInvoke` |
| 2026-09-03 | `vi.mocked(invoke)` 不是 mock | 以为 `vi.mock` 的工厂里的函数自动是 `vi.fn()` | 模块级 `invokeImpl` 变量，mock 工厂转调它 |
| 2026-09-03 | watch 窗口丢 Job 完成 | 先 `read` 再 `listen` | 先 listen 缓冲，再 handshake，pending 则重放 |
| 2026-09-03 | 虚拟化跳过屏幕外的搜索条高度 | `listRef` 绑在 `.main-glass-stage` | 绑 `.hub-paper-list` |
| 2026-09-03 | jsdom 全灭：`scrollTo is not a function` | 假定 DOM Element 有 `scrollTo` | 有函数才调用，否则 `scrollTop = 0` |
| 2026-09-03 | Hub `useEffect` 死循环 | 每帧 `querySnapshotFromPage(page)` 新对象当 dep | `useMemo(..., [page])` |
| 2026-09-03 | 刷新后第二页拼到新第一页 | `loadMore` 与 watch 重读无代次 | `generation` ref，过期响应丢弃 |
| 2026-09-03 | fixture「不含反斜杠」失败 | 把 `chapter` 空号页（sortKey `~\t`）写进 golden JSON | chapter 序用 Rust/Memory 单测锁；夹具只放稳定、可打印的 case |
| 2026-09-03 | `prepare_exact` 栈溢出 | `JobModule` 上的 wrapper 与自由函数同名再互相调用 | 删 wrapper，自由函数 `pub(crate)`，模块路径调用 |
| 2026-09-03 | consume 测 FOREIGN KEY | `revision_id: Some("rev-1")` 但没有 paper 行 | 测 handle 时用 `None`，或先插齐 paper / revision |
| 2026-09-03 | CostPreview 反序列化失败 | 结构体无 `Default`，serde 缺字段 | `#[serde(default)]` + `Default` |
| 2026-09-03 | 费用确认页看到 0 | 缺价目当 exact 0 | `unknownItemCount > 0` 走 unknown 文案 |
| 2026-09-03 | 取消剩余把别人的 OCR 停了 | 按 Job 取消 | 只取消本批 Item；joined Job 留给 created owner |
| 2026-09-03 | 打开 Reader 用了过期 revision | `onOpenPaper` 只 `documents.find` | `paperById`：hub 页覆盖 `list_documents` |
| 2026-09-03 | 键盘在虚拟列表里「消失」 | 窗口不含 `focusedId` | `virtualWindow({ ensureIndex })` + `scrollIntoView` |
| 2026-09-03 | rusqlite 没有 `create_scalar_function` | `Cargo.toml` 只有 `bundled` | 加上 `functions`，在 `db::configure` 注册且 `DETERMINISTIC` |
| 2026-09-03 | 多选拖到文件夹没有任何效果 | `memberIds.length > 1` 时 `showHint(bulkDisabledReason)`；client 接通后 reason 是空串 | 有 `libraryClient` 时走 Move Batch（`explicit` paperIds）；跨根仍开「移动到……」 |
| 2026-09-03 | 混进 .txt 的 Explorer 拖入整批无该项记录 | `decideImportDrop` 先 `filter(pdf)` | 全部 path 进 Import Batch；非 PDF 由后端记 `skipped_not_pdf` |
| 2026-09-03 | Explorer 拖到侧栏另一目录却导入当前选中夹 | overlay 只用 `selectedFolder` | `importDestinationFromHit`：目录行用该目录，列表用当前物理目录 |
| 2026-09-03 | 手排后下一刀用过期 layer id | `collection_layer` 只在进夹时读一次 | 依赖 `hub_page.revision`，watch / 写后重读整层 |
| 2026-09-03 | 任务中心写着可撤销但没有按钮 | Token 明文只在 Start 返回；库里只有 hash | `undoTokenStore` 记 10 分钟；Toast 与 OperationsDrawer 共用 |
| 2026-09-03 | `tauri dev` 启动即 panic：`this act response is not a batch`，随后 COM 回调 `panic_cannot_unwind` / `0xc0000409` | `record_receipt` 一律 `response.batch().id`；收藏 / 打开 Reader / 保存智能集合没有 batch | `receipt_ref()`：Batch 用 id，change / activity 用目标 id；生产路径禁止 `batch()` |
| 2026-09-04 | 把手 / 勾选框用户点不到 | 只把 `width`/`height` 微调几像素，或对 checkbox 用 `transform: scale` | 改 `.hub-paper-list` 上的 `--hub-*` 变量；勾选框用 `.hub-select-hit` 32px 热区，**不要** scale |
| 2026-09-04 | 点勾选框周围空白却打开了 Reader | 只在 `<input>` 上 `stopPropagation` | 热区是外层 `<label class="hub-select-hit">`，label 的 click 也要停冒泡 |

## 8. 验证

改投影 / 线格式：

```powershell
cd src-tauri
$env:LIBRARY_FIXTURE="update"
cargo test --locked --lib the_shared_fixture_is_reproducible_from_the_rust_reader
# 然后把 src/library/__fixtures__/library_read_v1.json 的 diff 一并提交
```

改 Hub / 文库前端：

```powershell
npx tsc -b
npx vitest run --no-file-parallelism src/library src/hubSort.test.ts src/components/LibraryHub.test.tsx src/components/LibraryHub.interaction.test.tsx
```

改查询 / 排序 / 分页：

```powershell
cd src-tauri
cargo test --locked --lib library_query
cargo test --locked --lib chapter_sort
```

改 Batch / 费用 / prepared job：

```powershell
cd src-tauri
cargo test --locked --lib library_batch
cargo test --locked --lib library_cost
cargo test --locked --lib prepare_exact
```

全量前端（提交前）：`npx vitest run --no-file-parallelism`。全量 `cargo test --locked` 很重，Windows 上先读 [windows-dev-build.md](windows-dev-build.md)。

本轮（2026-09-03）记录：`tsc` 0 错误；vitest **57 文件 / 385**；`library_query` 20 项（10k 语句数仍 11、chapter `1.2`<`1.10`、`collection_layer`）。新次数追加到 [handoff.md](handoff.md)「最近验证」，不要改历史行的含义。

## 9. 明确未完成（不得写成已上线）

这些是**发布机时**，不是「代码没写」：

- 实机 Windows Explorer 拖入；一次 500 文件；跨根 / 回收站 / 部分撤销 / 关闭重启。
- 1,000 Paper Hub warm P95 ≤ 50 ms / cold ≤ 150 ms；10,000 query P95 ≤ 250 ms（本 PR 用语句数不随库增大 + debug 页读取 < 8s 代替）。
- 屏幕阅读器实机念读（jsdom 只锁 role / name / live）。
- 目录树仍内联在 `LibraryHub.tsx`，未抽 `HubFolderTree.tsx`（行为已有测，不阻塞）。
- `LibraryHub` 仍接受浅 IPC 回退 props；有 `libraryClient` 时移动 / 导入 / 批量动作走 `library_act`。
- Worker 真正打 Provider 仍走现有 JobModule；批次只编排。
- Memory Adapter 不模拟真实 hash/copy journal、也不模拟 Job 并发。

无 Tauri 窗口时不要勾掉上述项，也不要编 P95 数字。

## 10. 如何维护本页

| 发生了什么 | 改哪里 |
| --- | --- |
| 新踩坑 | **只追加** §7 一行。不要改旧行的「正确做法」去迁就新代码；若旧正确做法已作废，在新行写清替代，旧行留作考古。 |
| 新文件 / 新 seam | §2 表加一行。 |
| 新不变量 | §1 表加一行，编号续 I11… |
| 新扩展菜谱 | §6 加 `6.x`。 |
| Hub 把手 / 勾选尺寸 | 只改 §6.7 变量；卡片 padding 与表格前两列跟变量走。 |
| 现状变了（例如终于删掉 `list_documents`） | 改 §0 一句话 + §5 表 + 计划 §11.0；不要默默删 §7 旧坑。 |
| 验证次数变了 | [handoff.md](handoff.md)「最近验证」；本页 §8 只保留「怎么跑」。 |
| D-062 排序专坑 | [hub-sort.md](hub-sort.md) §5，不要复制到本表。 |
| 完成一轮有用户可见行为的修补 | **§11 最上追加一条**（日期、改了什么、测了什么、还剩什么）。不要改旧条的正文去迁就新代码。 |

动文库代码却不改本页 / 计划 §11.0 / 线格式文档，视为未完成。

## 11. 变更日志（只追加，新条目放最上面）

给下一个 agent 的工作记录。每条固定四段：**做了什么 / 文件 / 验证 / 还剩什么**。不要把发布机时写进「做了什么」。

### 2026-09-04 — Hub 把手 / 勾选框加大命中区

**做了什么**
- 把手由 22×20 / 12px 字号改为 36×32 / 18px，垂直居中的 `⋮⋮` 按钮。
- 勾选框视觉边 15px → 20px；外层 `.hub-select-hit` 热区 32×32。网格、表格同一套。
- 尺寸收到 `.hub-paper-list` 的 `--hub-handle-w/h`、`--hub-checkbox`、`--hub-select-hit`、`--hub-controls-inset`。卡片 `padding-left` 与表格前两列跟变量走。
- 不用 `transform: scale` 冒充放大。label 热区 `stopPropagation`，避免点空白打开 Reader。

**文件**
- `src/styles.css`、`src/components/LibraryHub.tsx`
- 文档：本页 §5 / §6.7 / §7 / §10 / §11；[windows-dev-build.md](windows-dev-build.md) §4.1（cargo PATH，不是 Hub 代码）；[agent-onboarding.md](agent-onboarding.md) §46 / §47；[handoff.md](handoff.md)「最近验证」

**验证**
- `npx tsc -b` 0 错误。
- `npx vitest run --no-file-parallelism src/components/LibraryHub.interaction.test.tsx`：**34 通过**（选择集仍走 `getByLabelText("选择 Alpha")`）。
- 无 Tauri 窗口：未实机点按。

**还剩什么**
- 实机 `tauri dev` 看网格 / 表格是否好点、第一排悬停是否裁切。cargo 找不到时先读 [windows-dev-build.md](windows-dev-build.md) **§4.1**，不要重装 Rust。

---

### 2026-09-03 — Hub 控件/插入线与标注搬家

**做了什么**
- 卡片把手和多选框叠在左侧垂直居中，悬停/焦点/选中才显示。
- 列表顶留白，避免第一排悬停被裁。
- 第一张卡左侧空隙可插到最前；网格插入线保持竖线。
- 表格摘要格悬停出完整 Brief 浮窗。
- 笔记弹窗左右分栏 + Markdown/LaTeX 预览。
- 「本文标注」从 PDF 夹层搬进全篇成果芯片流（论证地图前）；精读边缘条不动。

**文件**
- `LibraryHub.tsx`、`styles.css`、`ReaderAnnotationComposer.tsx`、`ArtifactPanel.tsx`、`App.tsx`

**验证**
- `LibraryHub.interaction.test.tsx`（插到最前、表格浮窗）

**还剩什么**
- 需 `tauri dev` 实机看裁切与插入线。

---

### 2026-09-03 — `library_act` change/activity 收据 panic

**做了什么**
- `record_receipt` 不再对 `change` / `record_reader_activity` 调 `batch().id`（启动打开 Reader 或改收藏会崩，且发生在 WebView2 不能 unwind 的回调里，表现为 `0xc0000409`）。
- 生产路径的 `batch()` 收进 `#[cfg(test)]`。

**文件**
- `src-tauri/src/library_batch.rs`（`receipt_ref` + 测 `change_and_reader_activity_write_receipts_without_a_batch_id`）

**验证**
- `cargo test --locked --lib change_and_reader_activity_write_receipts_without_a_batch_id` 通过。

**还剩什么**
- 需要重启 `tauri dev`（改了 Rust 命令）。

---

### 2026-09-03 — 对照计划补合同缺口（本轮）

对照 [library-workspace-plan-2026-08.md](library-workspace-plan-2026-08.md) 做验收：PR 0–6 代码合同大体已在工作树，但有几处「计划写成已接通、界面实际静默失败」。本轮只修这些缺口和过期文档，不新开 PR 切片。

**做了什么**

- 多选拖到物理目录走 `library_act` Move Batch；跨根仍开「移动到……」。有 client 时单篇移动也走 Batch，无 client 才回退浅 IPC + 8 秒反向 Toast。
- Explorer drop：指针下的物理目录优先；智能集合 / 「全部」不猜。全部源 path 进 Import Batch，非 PDF 由后端 `skipped_not_pdf`。
- `collection_layer` 依赖 `hub_page.revision`，watch / 写后重读整层 live id。
- `undoTokenStore`：Start 明文记 10 分钟；Hub Toast「查看详情 / 撤销整批」与两处 `OperationsDrawer` 共用。
- 卡片 / 表格展示优先级、复习日期、最近打开。把手改回悬停/聚焦才出现。非法落点 `not-allowed`。列表边缘自动滚动。
- 计划 §1 标成实施前快照；§4.2 事件名改为 `read-event`；§10.1 补上 `watch_handshake` / `collection_layer`；PR 1「未落地」加「当时 / 已被后续 PR 接通」。`data-protocols` 补 `selectionDigest` 等页字段；D-062 章节排序不再写「纯前端 / user_version=7」。README / onboarding 次数改指向 handoff。

**文件（本轮动过的入口）**

- 前端：`src/library/importDrop.ts`、`undoTokenStore.ts`、`useHubPageList.ts`、`useLibraryWorkspace.ts`、`libraryWorkspaceClient.ts`、`src/components/LibraryHub.tsx`、`src/OperationsDrawer.tsx`、`src/App.tsx`、`src/styles.css`
- 测：`importDrop.test.ts`、`undoTokenStore.test.ts`、`LibraryHub.interaction.test.tsx`、`OperationsDrawer.test.tsx`
- 文档：本页、计划 §11.0、[data-protocols.md](data-protocols.md)、[decisions.md](decisions.md) D-062/D-063、[hub-sort.md](hub-sort.md)、[handoff.md](handoff.md)、[agent-onboarding.md](agent-onboarding.md) §47、根 [README.md](../README.md)、[CONTEXT.md](../CONTEXT.md)

**验证**

- `npx tsc -b` 0 错误。
- `npx vitest run --no-file-parallelism src/library/undoTokenStore.test.ts src/library/importDrop.test.ts src/OperationsDrawer.test.tsx src/components/LibraryHub.interaction.test.tsx`：**49 通过**。
- 无 Tauri 窗口：未跑实机 Explorer / P95 / 屏幕阅读器。

**还剩什么（下一个 agent 不要重新实现 PR 0–6）**

- 发布机时：实机 Explorer 拖入、一次 500 文件、跨根 / 回收站 / 部分撤销 / 关重启、Hub P95、SR 念读。
- 卫生项：`HubFolderTree.tsx` 仍未抽；`LibraryHub` 仍接受浅 IPC 回退 props，没有变成 `binding={hub}`。
- 不要从计划 §1 或 PR 1「当时未落地」推断现状。

---

### 模板（复制到最上）

```md
### YYYY-MM-DD — 一句话标题

**做了什么**
- …

**文件**
- …

**验证**
- …

**还剩什么**
- …
```


### 2026-09-11：安装升级验收中的标签显示修复

实际安装版中，没有 Brief 的文献添加标签后，数据库保存正常，卡片却不显示。原因是 documentCardFromHub 将生成关键词映射为可编辑标签，忽略 hub_page.tags。现改为读取持久 tags；空 tags 不从旧 Brief 关键词回填，避免删除后复活。新增两条最小回归，文库相关 81 项测试通过，继续以实际升级后的安装版验证。

