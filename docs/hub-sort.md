# Hub 排序（D-062）

文献大厅（Library Hub）的排序下拉、手排持久化、教材按章节号排序。**本页是实现合同与踩坑日志。** 代码与本页冲突时先改本页或 ADR，再改代码。产品决策见 [decisions.md D-062](decisions.md)；表/IPC 形状见 [data-protocols.md](data-protocols.md)；UI 状态矩阵见 [feature-specs.md](feature-specs.md)。

下一个 agent：先读 **§1 合同摘要** 和 **§5 踩坑日志**，再动文件。D-062 的 Hub Sort 补丁本身不要单独升 `user_version`，也不要从文件名猜章节号。当前生产库已是 **schema 8**（D-063）；有 `libraryClient` 时 `chapter` 走 SQLite `chapter_sort_key`，无 client 的预览仍用 `hubSort.ts`。手排提交必须是整层 live id（`collection_layer`），不要用当前页。

## 1. 合同摘要

| 项 | 事实 |
| --- | --- |
| 下拉五项 | 导入时间 `recent`（默认）/ 出版年份 `year` / 论文标题 `title` / 手动 `manual` / 按章节 `chapter` |
| 手动、按章节何时可用 | 当前列表里**每一份 PDF 都正好在选中文件夹**（不是「全部」，没有子孙 PDF） |
| 不可用时 | 选项灰掉；画面回落 `recent`；**不把回退写入** `collection_sort_prefs` |
| 按章节 | **纯视图**。只信教材 Metadata `chapterNumber`。不写 `collection_paper_order` |
| 手动 | 只有拖拽改序才写库。没拖过的文件夹不写 order 行 |
| 无存档 + 手动 | 与「导入时间」相同：`importedAt` **降序**（新的在前） |
| 有存档 + 手动 | 先画 `paperOrder` 里仍在的 id；缺席 live paper 按 `importedAt` **升序**接末尾 |
| 第一次真正换序 | 前端提交**当层全部 live id** 的新排列；后端校验精确置换后整表替换为稠密 `0..n-1` |
| 搜索 | 按章节仍可对可见结果排；手动只展示可见项，**禁止拖拽**（不出插入线、不写盘） |
| 同文件夹改名 | **不删** order 行（F2 / 同 `collection_id` 的 `move_paper` / reconcile 同层改名） |
| 跨文件夹 / 废纸篓 | `collection_id` 变化或 `trash_paper` 才删行；`restore_paper` 不还魂 |
| Schema | 两张表 `CREATE TABLE IF NOT EXISTS`；D-062 补丁本身不升版。当前生产库是 schema 8，validator 把这两张表纳入签名 |

上表描述当前已落地的 D-062 行为。D-063 PR 2 已落地把手、`Alt+↑/↓`、移到最前 / 最后和右键菜单，四者都通过 `src/library/dropPolicy.ts` 的 `planReorder` 提交同一份完整精确置换；PR 3 的批量移动走 `library_act` 并带持久 Undo Token，单篇拖到目录仍可用 8 秒前端 Toast 作为快捷入口。

章节比较：`parseChapterNumber` 按 `/[.\-]/` 切，每段必须是纯数字，否则当缺号。`3` < `3.2` < `3.10` < `10`。缺号排在有号后面。

## 2. 文件入口

| 要动的事 | 先看 |
| --- | --- |
| 建表、不升版 | `src-tauri/src/v2_workspace.rs` `ensure_hub_sort_tables`；`initialize_database` 四条路径 + `PaperModule::open` |
| 投影 / 写盘 / 改名守卫 | `src-tauri/src/paper_module.rs` `list_collections`、`reorder_collection_papers`、`set_collection_sort_mode`、`remove_paper_from_manual_order` |
| IPC | `library_commands.rs` impl + `lib.rs` `#[tauri::command]`；payload 必须 `{ request: { camelCase } }` |
| 纯排序引擎 | `src/hubSort.ts` + `src/hubSort.test.ts`；后端 `src-tauri/src/chapter_sort.rs`（SQLite `chapter_sort_key`） |
| Hub 分页 / 整层 id | `library_read hub_page` / `collection_layer`；前端 `useHubPageList.ts` |
| 选择集（纯函数） | `src/library/selectionModel.ts` + `selectionModel.test.ts` |
| 落点合法性 / reorder 提交口 | `src/library/dropPolicy.ts` + `dropPolicy.test.ts` |
| 目录树构建与展开记忆 | `src/library/treeModel.ts` + `treeModel.test.ts` |
| 交互控制器（Hub 唯一 binding） | `src/library/useLibraryWorkspace.ts` |
| 下拉 / 拖拽 / 虚线 | `src/components/LibraryHub.tsx`、`src/styles.css` |
| App 接线 | `src/App.tsx` `handleSetSortMode` / `handleReorderPapers`；树计数仍拉 `list_documents` + `list_collections`；Hub 卡片走 `hub_page` |
| 浏览器预览 | `src/desktopClient.ts` memory `collectionMap`（第一次 reorder 允许空表写入；拒绝重复 id） |
| 组件测 | `src/components/LibraryHub.test.tsx` |

## 3. 数据层

两张表（列名以 SQL 为准，不是 `mode`）：

```sql
collection_sort_prefs(collection_id PK, sort_mode, updated_at)
collection_paper_order(collection_id, paper_id, position, PRIMARY KEY (collection_id, paper_id))
```

- `list_collections` 批量拼 `sortMode` / `paperOrder`，查询失败用 `?` 往外传，不要 `if let Ok` 吞成「没有偏好」。
- `reorder_collection_papers`：`paperIds` 必须等于该 collection 当前 `deleted_at IS NULL` 的 id 集合（可重排）。缺/多/重复/未知 collection → 报错且不落库。与已存顺序相同 → no-op。
- 删 order 行**只**走 `remove_paper_from_manual_order`，且仅当 `collection_id` 真的变了，或 `trash_paper`。不要在同层 rename 里无条件 `DELETE WHERE paper_id=?`。

## 4. 拖拽

同一套 pointer 拖拽（8px 阈值），不要换 HTML5 DnD。

落点分流：

1. 侧栏 `.folder-pill-node` → 搬家（现有同根规则）；`insert = null`。
2. 手动且本层可用且搜索为空：
   - 另一张带 `data-paper-id` 的卡片/行 → 半区 before/after。
   - 网格/表**空白 gutter**（指针不在任何卡片/行上）→ 插到末尾。
   - 自身卡片、表头、搜索框、排序下拉 → **不是** end-drop。
3. 否则走原来的舞台空白搬家。

落点必须写在 `dragState` **ref** 上，`pointerup` 只读 ref。React state 只用来画虚线。不要只靠 `useState` 的 `insert` 做提交（会读到上一帧）。

`insert-at-end` class 只加在 `.paper-cards-grid` / `.liquid-table-container`，不要加在整个 `.main-glass-stage`。

## 5. 踩坑日志（只追加，不删旧条）

改代码时若再踩坑，在表末加一行：日期、症状、错误做法、正确做法。

| 日期 | 症状 | 不要再做 | 正确做法 |
| --- | --- | --- | --- |
| 2026-08-29 | F2 改名后手排跳到末尾 | 同层 `move_paper` 无条件 `DELETE FROM collection_paper_order WHERE paper_id=?` | 仅 `prev_collection_id != collection_id` 时删 |
| 2026-08-29 | 旧工作区第一次 reorder 炸 | 只在全新库建表；或 bump schema 8 | `ensure_hub_sort_tables` 挂 `initialize_database` 四路径 **和** `PaperModule::open`；不升版 |
| 2026-08-29 | 松手改序/搬家用错落点 | `insert` 只放 `useState` | 同步写 `dragState` ref |
| 2026-08-30 | 松在自己的卡片上变成「插到末尾」 | `closest(网格)` 就当 end，不管指针还在卡片上 | 指针在 `.liquid-paper-card` / `.liquid-table-row` 上且不是另一张卡 → `insert=null` |
| 2026-08-30 | 搜索框/排序条亮「插到末尾」 | `insert-at-end` 套 `.main-glass-stage` | 热区只认网格/表容器 |
| 2026-08-30 | memory 第一次手排必炸 | 用「已存 order 长度」当置换守卫；`collectionMap` 从不写入 | 未知 collection `ensureCollection`；空表允许第一次完整 `paperIds`；只拒重复 id |
| 2026-08-30 | `list_collections` 看起来丢了手排 | `if let Ok(prepare)` 失败当空 | `?` 传播 |
| 2026-08-30 | 文档写「无存档按 importedAt 升序」 | 和「导入时间」新→旧打架 | 无存档 = `recent` 降序；升序只用于**已有存档之后的新手** |
| 2026-08-30 | walkthrough 写搜索仍显示插入线 | 和 grilling 合同相反 | 搜索时 `reorderActive=false`，不出线、不写盘 |
| 2026-08-30 | 想用 D-059 记 Hub 排序 | D-059 已是「导出阅读成果」 | 用 **D-062** |
| 2026-08-31 | 已排在最前时「移到最前」提示「已经在该目录中」 | reorder 分支复用移动语义的 `same_destination` 文案 | reorder 的自落点返回 `label: "顺序未变化"`，原因码不变、文案按语境分 |
| 2026-08-31 | 改用 `hub.treeRows` 后侧栏丢了 Papers / Textbooks 根 | 以为根来自 collection 行 | 树数据源必须补两个虚拟根节点（`treeRows` 内合成），空库也要看得见 |
| 2026-08-31 | 单篇「标签…」「导出…」提示「批量操作需要持久批次」 | 复制粘贴 `BULK_DISABLED_REASON`，不分作用范围 | 原因必须匹配该动作真正的阻塞点：单篇标签在标签芯片、单篇导出在阅读器顶栏；只有批量项才引用后端 Batch |
| 2026-09-03 | `hub_page` chapter 用 `m.chapter_number` 编译失败 | 以为 schema 有 `paper_metadata.chapter_number` | 章节号在 metadata artifact JSON `$.chapterNumber`；ORDER BY 走已注册的 `chapter_sort_key(...)` 相关子查询 |
| 2026-09-03 | 分页后手排提交缺 id | 用当前页 `visibleIds` 当 D-062 置换 | 另读 `collection_layer`，提交整层 live id |

## 6. 如何加一种排序

1. `src/hubSort.ts`：`HubSortMode` + `applyHubSort` 分支 + 单测。
2. `set_collection_sort_mode` 白名单。
3. `LibraryHub` 下拉 `option` + `hubSortAvailability`（若有新的启用条件）。
4. 改本页 §1、[data-protocols.md](data-protocols.md) 白名单、[feature-specs.md](feature-specs.md) 状态矩阵。
5. 新表继续 `IF NOT EXISTS`。默认**不**升 `user_version`；真要升版单开 ADR。

## 7. 验证

```powershell
npx tsc -b
npx vitest run --maxWorkers=1 --fileParallelism=false src/hubSort.test.ts src/components/LibraryHub.test.tsx src/components/LibraryHub.interaction.test.tsx src/library
cd src-tauri
cargo test --locked --lib ensure_hub_sort_tables_is_idempotent
cargo test --locked --lib reorder_requires_exact_permutation
cargo test --locked --lib set_collection_sort_mode_persists
cargo test --locked --lib manual_order
```

`manual_order` 覆盖：同层改名保留行、跨层移动删行、trash 删行且 restore 不还魂。

实机（`npm run tauri dev`，改过 `src-tauri` 必须重启）：叶子教材文件夹 → 按章节 `1 → 3 → 3.10`；切手动无存档会跳回导入时间；拖一张到另一张右侧虚线松手后刷新仍在；拖到侧栏仍搬家；「全部」下手排灰掉。Windows 编不过见 [windows-dev-build.md](windows-dev-build.md)。

## 8. D-062 历史边界与 D-063 扩展

D-062 继续明确不做：从文件名/标题猜章节号；改 PDF 文件名当顺序；全局一份 permutation；按章节写入手排；搜索或 Smart Collection 把可见子集当 `paperIds`。

D-063 PR 2 已取代本页原先对“拖拽手柄 / 修饰键 / Undo Toast”的排除，现在存在：

- hover / focus 才出现的 `⋮⋮` 把手，只有把手启动拖拽，点正文仍是打开 Reader；
- `Alt+↑/↓`、移到最前 / 最后和所选块 reorder；
- Ctrl/Shift 多选、Space / Esc / Ctrl+A、复选框与粘性批量工具栏；
- 显式「移动到……」对话框（`src/components/HubMoveDialog.tsx`）预览跨根 kind change。

这些入口共享 `src/library/dropPolicy.ts` 的 `planReorder`，继续向后端提交物理叶子 collection 的全部 live Paper 精确置换。分页后的置换 id 来自 `collection_layer`，不是当前 `hub_page`。批量、Undo Token、Smart Collection 与 Provider OCR / Brief 已由 D-063 PR 3–5 接通；搜索 / 智能集合仍禁止手排。D-063 的 schema 8 用于 Batch / Lifecycle / Smart Collection，与 D-062 排序表的历史“保持 schema 7”约束不冲突。

文库其余实现（`hub_page`、Batch、watch、虚拟列表）见 [library-workspace.md](library-workspace.md)。计划进度见 [library-workspace-plan-2026-08.md](library-workspace-plan-2026-08.md) §11.0。排序专坑继续只追加本页 §5。
