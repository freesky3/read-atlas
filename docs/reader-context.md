# 读者上下文实现手册（D-065）

- 状态：机制 + F2 出厂缝 + 居中 80% 分栏编辑器 **代码已落地**
- 日期：2026-09-07（本页可追加，不覆盖旧条）
- ADR：[decisions.md D-065](decisions.md)
- 索引：[README.md](README.md) · [agent-onboarding.md §50](agent-onboarding.md)
- 核心代码：
  - 路径 / G2 叠层 / 包装 / 磁盘读写：[`src-tauri/src/reader_context.rs`](../src-tauri/src/reader_context.rs)
  - IPC 与 F2 注入：[`src-tauri/src/lib.rs`](../src-tauri/src/lib.rs) `get_reader_context` / `save_reader_context` / `resolve_reader_wrapper_for_revision`
  - Hub 搬家：[`src-tauri/src/paper_module.rs`](../src-tauri/src/paper_module.rs)
  - Watcher：[`src-tauri/src/library_watcher.rs`](../src-tauri/src/library_watcher.rs) · [`library_commands.rs`](../src-tauri/src/library_commands.rs)
  - F2 出厂缝：[`src-tauri/src/prompt_settings.rs`](../src-tauri/src/prompt_settings.rs)
  - 解释 / Lens 生成 / Lens QA：[`src-tauri/src/reading_artifact_module.rs`](../src-tauri/src/reading_artifact_module.rs)
  - 编辑器：[`src/components/ReaderContextDialog.tsx`](../src/components/ReaderContextDialog.tsx)
  - 入口：[`src/components/LibraryHub.tsx`](../src/components/LibraryHub.tsx) · [`src/App.tsx`](../src/App.tsx)
- 测试：`reader_context` / `prompt_settings` / `paper_module::move_and_trash_paper_take_reader_sidecar`；`ReaderContextDialog.test.tsx` · `LibraryHub.interaction.test.tsx` · `desktopClient.test.ts`

**本页是实现手册与踩坑日志，不是 grilling 草稿。** 代码与本页冲突时先改本页或 ADR，再改代码。下一个 agent：先读 **§0、§1、§2、§7、§8 最新一条**，再动文件。

---

## 0. 现状一句话

用户可以在 **全部文档 / 物理文件夹 / 单篇 PDF** 各写一份自由文本「读者上下文」（已掌握什么、阅读目的、课程背景）。**磁盘 md 是唯一权威**。F2 任务把它当成带标签的附加段读；19 个生产槽仍是唯一 system。空则整段不发。

它**不是**系统提示词，**不是** Orientation Pack / 术语表，**不是** Settings → Prompts。

---

## 1. 不变量（改任何相关代码前对照）

违反任一条就是回归。不要用「先打通再修」绕过。

| # | 不变量 | 反例（禁止） |
| --- | --- | --- |
| **I1** | 对象是读者笔记，不是系统提示词。菜单、文件名、对话框标题都写「读者上下文」，不准叫「提示词」。 | 右键「编辑此 PDF 提示词」；文件叫 `prompt.md`。 |
| **I2** | 19 个生产槽仍只在账号 `prompt-settings.json`。读者上下文**不准**覆盖、拼接进、或替换 `system_instruction`。 | 把用户 md 拼进 Discussion system；每个文件夹再落一份 19 槽。 |
| **I3** | 磁盘是唯一权威。没有 SQLite 正文副本，没有双向同步。应用内编辑器写的就是这些文件。 | `prompt-settings.json` 再存一份；SQLite 表当权威再导出 md。 |
| **I4** | 空层不建文件、不注入。保存 trim 后为空则删文件（文件不存在当成功）。 | 每个 `Papers/` 子目录预创建空 `read-desktop.reader.md`。 |
| **I5** | 包装块只进 **user_input / incremental_input / recovery_input**。至少一层非空才发出。 | 写进 `system_instruction`；空字符串也发一个标题。 |
| **I6** | 谁读：讨论、精读、旁批读懂/落笔、解释、Lens **生成**、Lens QA。 | Translate / Lens **repair** / 讨论压缩 / 论文根 / Outline / Orientation Pack 也带上。 |
| **I7** | 叠法 G2：全部文档 → 从 `Papers/` 或 `Textbooks/` 走到文件的祖先文件夹 → PDF sidecar。读者自我描述冲突时更具体层赢；论文主张只来自 PDF。 | 只发最具体一层；把 `Papers/` 根当成「全部文档」（进不了 Textbooks）。 |
| **I8** | Job（旁批 / 精读 / 解释 / Lens 生成）入队时把拼好的包装块冻进 `payload.readerContext`，执行和 repair **只读 payload**。讨论与 Lens QA **每轮现读磁盘**。 | 旁批分批中途改笔记导致前后页读到两份自我介绍；讨论还抱着进线程时的旧快照。 |
| **I9** | Hub 改名 / 移动 / 回收站 / 恢复 PDF 必须带着 `{stem}.read-desktop.reader.md`。删空目录前必须先 stash 该目录的 `read-desktop.reader.md`。Explorer 只改 PDF 名：**不认领**孤儿。 | `trash_collection` 里 `walk_has_file` 看到 md 就删不掉空目录，或直接 `remove_dir_all` 把笔记毁掉。 |
| **I10** | Watcher 对 Workspace 根只能 **NonRecursive**，且只认根上那份 `read-desktop.reader.md`。读者上下文文件不得触发 PDF reconcile / 导入。 | `watch(workspace_root, Recursive)` 扫到 `.read-desktop/*.sqlite3`。 |
| **I11** | IPC 名是 `get_reader_context` / `save_reader_context` / `restore_reader_folder_context`。已有投影 `get_reading_context` 是阅读器页码/滚动，**不准占用**。 | 新命令也叫 `get_reading_context`。 |
| **I12** | 智能集合没有物理目录，无文件、无菜单。 | 给 smart collection 挂一份 md。 |
| **I13** | 编辑器必须居中、约占视口 80%，左右分栏（左编辑、右 `MarkdownBody` 实时预览）。遮罩**不要**复用 `.scrim`。 | 套 `.scrim`（`justify-content: flex-end`）把弹窗推到右下；小卡片 640px。 |
| **I14** | ≥2000 字只黄字警告，不截断、不拒保存。 | 硬顶导致深层文件夹保存失败却说不清该删哪一层。 |

---

## 2. 文件与入口

### 2.1 磁盘布局

```
<Workspace>/
├─ read-desktop.reader.md          ← 全部文档（Hub 虚拟节点 ""）
├─ Papers/
│  ├─ read-desktop.reader.md       ← Papers 根，只覆盖论文根
│  └─ RL/foo.pdf
│     └─ foo.read-desktop.reader.md
├─ Textbooks/
│  └─ …
└─ .read-desktop/
   └─ trash/
      ├─ <trash_id>/foo.pdf
      ├─ <trash_id>/foo.read-desktop.reader.md
      └─ reader-folders/<id>/      ← 被删空目录的文件夹笔记
           ├─ read-desktop.reader.md
           └─ meta.json            ← originalRelativePath, deletedAt, purgeAfter
```

常量：`FOLDER_FILE_NAME = "read-desktop.reader.md"`；PDF sidecar = `{stem}.read-desktop.reader.md`（stem 去 `.pdf`，大小写不敏感）。

工作区根文件**不**进 `.read-desktop/`，避免和 SQLite/锁撞名，也方便 Explorer 直接改。

### 2.2 代码入口

| 要动的事 | 先看 | 测试 |
| --- | --- | --- |
| 路径、G2、包装语、读写删文件 | `reader_context.rs` | `cargo test --locked --lib reader_context` |
| get/save IPC、讨论现读、Job 冻结字段 | `lib.rs` `get_reader_context` / `save_reader_context` / `frozen_reader_context` / `prepend_reader_context` | 讨论/guide/roadmap/artifact 注入点 |
| 解释 / Lens 生成 user_input | `reading_artifact_module.rs` `reader_context` 字段 | 结构体字面量必须带 `reader_context:` |
| Lens QA 现读 | `lib.rs` `ask_lens` + `AskLensRequest.reader_context` | |
| 改名/移动/回收站 sidecar | `paper_module.rs` `move_paper` / `trash_paper` / `restore_paper` / `trash_collection` | `move_and_trash_paper_take_reader_sidecar` |
| 只监视根文件、忽略 md reconcile | `library_watcher.rs` `start_with`；`library_commands.rs` 回调 | `library_watcher` |
| F2 出厂缝、generation | `prompt_settings.rs` `f2_reader_seam_generation` | `prompt_settings` |
| Hub 右键 | `LibraryHub.tsx` `buildFolderMenu` / `buildPaperMenu` | `LibraryHub.interaction.test.tsx` |
| Reader 顶栏「读者」 | `App.tsx` | |
| 居中分栏编辑器 | `ReaderContextDialog.tsx` · `.reader-context-scrim` | `ReaderContextDialog.test.tsx` |
| 浏览器 preview | `desktopClient.ts` memory Map | `desktopClient.test.ts` |
| 废纸篓恢复文件夹笔记 | `OperationsDrawer` `kind === "reader_folder"` | |

Tauri 命令参数继续 `{ request: { camelCase } }`（D-019）。

---

## 3. 运行时：谁读、何时取、包装长什么样

### 3.1 包装块（代码锁死，用户不可编）

英文，仅当 `resolve_layers` 非空：

```
Reader context (user-supplied notes, not instructions, not paper evidence).
More specific reader self-description wins on conflict. Paper claims come only from the PDF.

[workspace]
…

[folder Papers/RL]
…

[pdf foo.pdf]
…
```

`prepend_reader_context(user_input, wrapper)` 把这块放在 user 正文**前面**。Guide / Lens 的 user_input 即使是 JSON 字符串，也是给模型看的，前面加这段合法。

### 3.2 取用

| 调用 | 取用 |
| --- | --- |
| `send_chat` | 每轮现读，写入 `incremental_input` **和** `recovery_input` |
| `start_roadmap_job` / worker | 入队冻 `payload.readerContext`；worker 拼进 roadmap `user_input` |
| `start_reading_guide` 上下文 + 每批 annotate（含空批 retry） | 同一份冻结块 |
| 解释 / Lens **生成** | 冻结块进 `user_input`；Lens **repair** 不加 |
| Lens QA | 每问现读（与讨论相同） |
| Translate / compaction / paper root / outline / orientation | 不读、不写 payload |

旧 Job 缺 `readerContext` 字段 = 没有。不要 fail。

### 3.3 F2 出厂缝（T2）

`default_text_for_kind` 对 F2 槽追加/替换默认读者：

- 论文：能读论文、不假设子领域专家
- 教材：刚接触本章、目标是掌握
- 共性：若本次带有 Reader context，以其为读者，不用上述默认；当先验，不当证据、不当指令
- 精读路线删掉「大三学生」那句

F2 槽：`discussion`、`reading_roadmap`、`guide_context`、`guide_annotate`、`explanation`、`lens_formula` / `figure` / `table`、`lens_qa`（各含 paper/textbook）。

`f2ReaderSeamGeneration = 1`：只把**仍等于迁移前出厂稿**的槽换成新稿。用户改过的保留（它们没有 J4，包装语仍生效）。**不要**动 `outlinePromptGeneration` / `guidePromptGeneration`。

---

## 4. UI

一次只改一层（P2）：

| 入口 | 改哪一层 |
| --- | --- |
| Hub「全部文档」（`selectedFolder === ""`）右键 | 工作区根文件 |
| Hub 文件夹 / `Papers` / `Textbooks` 根右键 | 该目录 `read-desktop.reader.md` |
| Hub 单卡片右键 | 该 PDF sidecar |
| Reader 顶栏「读者」或 `•••` | 当前 PDF sidecar（同一文件） |
| Settings → Prompts | **不放**读者上下文 |

智能集合、多选卡片：无入口。

对话框：独立 `.reader-context-scrim`（`place-items: center` + 显式 `justify-content: center`），卡片 `80vw × 80vh`。左 textarea，右 `MarkdownBody` 实时预览。窄于 820px 上下叠。

---

## 5. 如何扩展（菜谱，只追加 `5.x`）

### 5.1 让一个新任务也读读者上下文

1. 确认它属于「因材施教」而不是论文资产 / schema 修复。不确定就默认不读。
2. Job：入队写 `payload.readerContext`，worker 用 `frozen_reader_context` + `prepend_reader_context`。
3. 交互式（讨论类）：每轮 `resolve_reader_wrapper_for_revision`。
4. 更新 §1 I6 表和本页 §3.2。加一个断言：`system_instruction` 仍等于 19 槽 resolve 结果。

### 5.2 改文件名 / 加一层

只改 `reader_context.rs` 常量与 `is_reader_context_path`。搬家逻辑必须跟新名字走。更新 I9 / §2.1。不要用 `prompt` 当文件名。

### 5.3 改默认读者文案

只动 `prompt_settings.rs` 的 seam 字符串，并 **bump `F2_READER_SEAM_GENERATION`**，迁移仍只替换「等于上一版出厂」的槽。不要重写整个 F2 任务正文（那是以后按槽打磨）。

### 5.4 改编辑器外观

只动 `ReaderContextDialog.tsx` 与 `.reader-context-*`。**不要**给遮罩加 class `scrim`。

---

## 6. 验证

```powershell
npx tsc --noEmit
npx vitest run --maxWorkers=1 --fileParallelism=false src/components/ReaderContextDialog.test.tsx src/components/LibraryHub.interaction.test.tsx src/desktopClient.test.ts
cd src-tauri
cargo test --locked --lib reader_context
cargo test --locked --lib prompt_settings
cargo test --locked --lib move_and_trash_paper_take_reader_sidecar
cargo test --locked --lib library_watcher
```

改了 Tauri 命令后必须重启 `tauri dev`。实机应看到：右键「全部文档」保存几句 → 打开一篇论文发讨论，user 侧出现 `[workspace]` 段；Settings Prompts 仍是 19 槽。

未覆盖（发布门槛）：Explorer 只改 PDF 名的孤儿；live provider；编辑器打开时外部改 md 的热重载（`read-event kind=reader_context` 已发，对话框尚未订阅）。

---

## 7. 踩坑日志（只追加，新行放最上面）

| 日期 | 坑 | 正确做法 |
| --- | --- | --- |
| 2026-09-07 | 对话框套了 `className="scrim hub-move-scrim"`。`.scrim` 是任务抽屉用的 `display:flex; justify-content:flex-end`，specificity 与后出现的 grid 叠在一起，**弹窗被推到右下**。 | 只用 `.reader-context-scrim`。必须同时写 `place-items:center` 和 `justify-content:center`。 |
| 2026-09-07 | `trash_collection` 在 `walk_has_file` 之后才想删目录。目录里若只有 `read-desktop.reader.md`，`has_file==true` 目录删不掉；若强行 `remove_dir_all`，笔记进不了 30 天废纸篓。 | `cnt==0`（无 live paper）时**先** `stash_folder_reader_file`，再判断空目录。 |
| 2026-09-07 | 对整棵 Workspace `Recursive` 监视，才能看到根上的 `read-desktop.reader.md`。结果 SQLite 每次 checkpoint 都当文库变更。 | `LibraryWatcher::start_with`：Papers/Textbooks Recursive；workspace root **NonRecursive**。仅根文件名匹配才 emit `reader_context`。纯 md 变更不要 reconcile。 |
| 2026-09-07 | 新 IPC 差点叫 `get_reading_context`。这个名字已经是阅读器页码/滚动投影。 | 读者笔记一律 `reader_context`（reader 不是 reading）。 |
| 2026-09-07 | 只 stash 一次再扫空目录，会把**仍有论文**的父文件夹笔记也收走。 | stash 必须放在 `cnt == 0` 分支内部。 |
| 2026-09-07 | `folder Papers/` 被当成「全部文档」。Textbooks 里的 PDF 吃不到。Hub 已有 `folder===""` 的「全部文档」节点。 | 全部文档 = 工作区根文件。`Papers/`、`Textbooks/` 仍是普通文件夹层。不要再造第二个虚拟根。 |
| 2026-09-07 | 改 `default_text()` 期望老用户自动拿到 J4。但 `prompt-settings.json` 里已持久化旧出厂稿，`is_default` 会变成 false。 | `f2_reader_seam_generation` 只替换「正文仍等于旧 base」的槽；自定义保留。 |
| 2026-09-07 | 把包装语拼进 `system_instruction`，Settings 里打磨的 19 槽不再是唯一 system，schema/旁批身份可被用户笔记带跑。 | 只 `prepend` 到 user 侧。测试：system 仍等于 slot resolve。 |
| 2026-09-07 | `AskLensRequest` / `GenerateReadingArtifactRequest` 加了字段后，测试里的结构体字面量漏字段，lib test 编不过。 | 所有字面量补 `reader_context: None` 或冻结值。 |

---

## 8. 如何维护本页

| 发生了什么 | 改哪里 |
| --- | --- |
| 新踩坑 | **只追加** §7 一行。旧行留作考古。 |
| 新不变量 | §1 表加一行，编号续 I15… |
| 新文件 / 新 IPC | §2.2 表加一行。 |
| 新消费任务 | §5.1 走一遍 + 改 I6 / §3.2。 |
| 改文件名或叠层 | §5.2 + §2.1。 |
| 改出厂默认读者 | §5.3，bump generation。 |
| 改编辑器 | §5.4，不要动 `.scrim`。 |
| 完成一轮用户可见修补 | **§9 最上追加一条**（日期、做了什么、文件、验证、还剩什么）。 |

动读者上下文代码却不改本页，视为未完成。

---

## 9. 变更日志（只追加，新条目放最上面）

### 2026-09-07 — 居中 80% 左右分栏编辑器

**做了什么**
- 对话框改为视口 80%×80%、强制居中。
- 左 Markdown 编辑、右 `MarkdownBody` 实时预览（空时占位文案）。
- 去掉 `.scrim`，避免任务抽屉的 `flex-end` 把卡片推到右下。

**文件**
- `src/components/ReaderContextDialog.tsx`、`.reader-context-*`（`src/styles.css`）
- 测试：`ReaderContextDialog.test.tsx`

**验证**
- 该文件 2 项 vitest 通过（2000 字警告；`**线性代数**` 预览为 `<strong>`）。
- 无 Tauri 窗口：未实机点按。

**还剩什么**
- 实机确认遮罩盖住 PDF+聊天、宠物浮层是否挡按钮。

### 2026-09-07 — D-065 机制落地

**做了什么**
- 三层磁盘 md + get/save IPC + memory adapter。
- G2 叠层包装进 F2 user 侧；Job 冻结；讨论/Lens QA 现读。
- Hub 移动/回收站/恢复带 sidecar；空目录删除前 stash 文件夹笔记；废纸篓可恢复。
- Watcher NonRecursive 根监视；md 不 reconcile。
- F2 出厂缝 + `f2ReaderSeamGeneration=1`。
- Hub 右键 / Reader「读者」入口。一次一层。

**文件**
- 新：`src-tauri/src/reader_context.rs`、`ReaderContextDialog.tsx`
- 改：`lib.rs`、`paper_module.rs`、`prompt_settings.rs`、`reading_artifact_module.rs`、`library_watcher.rs`、`library_commands.rs`、`LibraryHub.tsx`、`App.tsx`、`OperationsDrawer.tsx`、`desktopClient.ts`、`types.ts`
- 文档：本页；ADR D-065；onboarding §50；文库 I15

**验证**
- Rust：`reader_context` 8、`prompt_settings` 15、`move_and_trash_paper_take_reader_sidecar`、`library_watcher` 2。
- 前端：Hub 菜单、对话框、memory adapter；`npx tsc --noEmit` 0 错误。
- 未跑：全量 vitest / 全量 cargo / 实机 Explorer 孤儿 / live provider。

**还剩什么**
- 打开编辑器时订阅 `read-event kind=reader_context` 做热重载。
- 按槽打磨 19 段出厂稿（A）时把 J4 写进用户可见文案；机制不要再改。

## 2026-09-09：解释读者规则纳入完整中文稿

解释的论文／教材默认稿已分别内置完整中文读者规则，不再在默认稿后重复追加英文 F2 读者段。原有 Reader context 的磁盘来源、层级优先级、入队冻结与 user input 包装保持有效；读者笔记不写入来源根，也不成为论文证据。其他槽的 F2 行为未随此次修改。完整接线与迁移见 [解释生成合同](explanation-generation.md)。

## 2026-09-09：精读路线的完整中文读者规则

精读路线默认稿已内置中文读者适配，F2 不再替换或追加默认背景句。路线始终引导逐步深入阅读；读者背景调整起点、节奏与提示深度。入队冻结读者背景与可选 Brief，二者只进入路线旁支，不写入 PDF 来源根。见 [精读路线生成合同](reading-roadmap-generation.md)。

## 2026-09-09：讨论完整中文稿

Discussion 默认稿已内置中文读者规则，不再自动追加英文 F2 段；仍每轮现读并包装到增量与恢复输入。当前明确问题和知识说明优先于较早的笼统背景，不将未写出的知识视作缺失。压缩仍不额外注入 Reader context，只保留实际对话中出现且影响续接的偏好与背景，不推断“讲解过即掌握”。完整合同见 [discussion-generation.md](discussion-generation.md)。

## 2026-09-09：论文 Lens 完整中文稿与 v2

三份论文 Lens 生成默认稿已内置中文读者规则，不再追加英文 F2 段；Reader context 仍在入队冻结并注入初始生成的 user_input。修复不重新附读者包装，仅读取实际仍可见的分支材料。教材生成和 Lens QA 的原稿及输入方式保持不变，见 [Lens 合同](lens-generation.md)。

## 2026-09-09：Lens 追问完整中文稿

论文／教材 Lens QA 默认稿已内置完整中文读者规则，不再附加英文 F2 段；Reader context 的逐轮请求注入保持有效。本轮具体困难和深度要求优先于笼统旧设定，不将“解释过”视为“已掌握”。见 [追问合同](lens-qa-generation.md)。
