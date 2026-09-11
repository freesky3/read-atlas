# 后端性能与工作区生命周期（2026-08-18/19）

下一个 agent 先读本页，再动 `src-tauri`。这轮只改后端：速度、内存、可维护性。**没有改产品合同、IPC 命令名、JSON 形状。**

P0-2 的 Provider 实例级安全路由与任务恢复已经落地并进入当前生产合同：`schema v7`、精确实例 UUID、冻结 route / exact key、legacy jobs 与 tombstones 的人工确认隔离、Mistral OCR 独立 endpoint scope、`REMOTE_CLEANUP_BATCH_SIZE = 32`、`attempts` 重登记归零、任务中心 recovery 引导，以及 memory adapter 不伪造 probe，全部按现行实现生效。不要再把这些内容当作“计划中能力”处理。

对应工作：Codex 会话 `01a01445-88de-7120-8686-266c6d292c71` 的大部分实现，加上后续 Grok 补齐的原子 `WorkspaceRuntime`、命令 seam 拆分和测试。

## 先不要做的事

- 不要给 `list_remote_tombstones` 加硬 `LIMIT`。任务中心抽屉要看到全部墓碑。有界批次只存在 **claim / retry**（`REMOTE_CLEANUP_BATCH_SIZE = 32`）。
- 不要把 Discussion 流式 `mpsc` 改成有界并丢增量。现在是 `unbounded_channel` + 50ms / 2KiB 合并 + 250ms checkpoint。
- 不要在切换 Workspace 时 `join` / `await` 旧 worker。只 `stop.store(true)` + `notify_waiters()`，界面立刻切。旧 worker 只写自己抓住的那份 `Arc<WorkspaceRuntime>`。
- 不要恢复 `AppState` 上的 `workspace` / `paper_module` / `job_module` / `artifact_module` / `outline_module` 多把锁。命令只读 `current_runtime()`。
- 不要删 `v2_workspace.rs` 里 `ActiveWorkspaceLock` 的 `File` 句柄。看起来 unused，是在持有独占锁。
- 不要对用户 Workspace、`tmp/`、Credential Manager 做破坏性清理。
- 没有六类/500 页样本、没有显式付费开关，不要跑 P95 / 装包 / live Gemini-Mistral smoke，也不要编数字。
- `send_chat` 生成循环仍在 `lib.rs`。`discussion_commands.rs` 目前只是 seam 占位，不要假装它已经搬走了。
- 不要把 `request: { camelCase }` 改成扁平字段。Tauri 2 命令参数结构体要保留。
- **不要在 LLM 端的 `content_json` 字符串里信任** 标点字符：JSON 通道会把 `\uFF1A`（中文 `：`）/`\uFF0C`（中文 `，`）/`\u3002`（中文 `。`）/`\uFF1B`（中文 `；`）降级为单字节 `?`/`,`/`.`/`;`。Lens markdown 归一化器会**绕过这些乱码**（见 §A.4），但**不能**把它们反推为中文标点。

## 文件入口

| 要动的事 | 先看 |
| --- | --- |
| 打开 / 切换 Workspace | `workspace_lifecycle.rs` `activate_workspace` / `swap_runtime` / `current_runtime` |
| SQLite 打开参数 | `db.rs`（WAL 只在初始化；日常 `open` = foreign_keys + busy_timeout） |
| 书库 / PDF / Trash / watcher | `library_commands.rs` + `paper_module.rs` |
| Job 入队 / worker / tombstone IPC | `job_commands.rs`（薄包装）+ `lib.rs` 里的 worker / SQL |
| Artifact / OCR 投影 IPC | `artifact_commands.rs` + `artifact_module.rs` |
| Outline IPC | `outline_commands.rs` + `outline_module.rs` |
| 诊断 preview / export / stats | `diagnostic_commands.rs` |
| Discussion 发送 / 发布 | `lib.rs` `send_chat` / `publish_discussion_outcome` / `discussion_can_publish` |
| Provider 流式 / OCR staging | `provider_ports.rs`（Gemini / Mistral） |
| OpenAI-compatible / Grok | `chat_completions.rs` + `model_settings.rs`；工厂在 `lib.rs` `open_paper_adapter`。合同与坑：[agent-onboarding.md](agent-onboarding.md) §29 |
| Schema 与索引 | `v2_workspace.rs` `SQLITE_SCHEMA_VERSION=3`，Workspace 格式仍是 `2` |
| 命令注册表 | `lib.rs` `invoke_handler`（命令名不要改） |
| Hub 整理（右键菜单/拖拽） | `agent-onboarding.md` §33 |
| Hub 排序（manual/chapter） | 合同 [hub-sort.md](hub-sort.md)；ADR [decisions.md D-062](decisions.md)；本页 §A.7 |
| 文库 Workspace（`library_read` / `library_act` / schema 8） | 实现手册 [library-workspace.md](library-workspace.md)；计划 §11.0。生产 schema 现为 **8**（本页下文「schema v7 / VERSION=3」是历史段落，不要当现状） |
| Lens / Brief markdown 归一化 | `reading_artifact_module.rs` `normalize_markdown_field` + §A.4；显示层合同 [markdown-rendering.md](markdown-rendering.md) |

`lib.rs` 仍很大。`#[tauri::command]` 必须留在 `lib.rs`。实现按 seam 往外搬，不要把命令属性一起搬走。

## 这轮改了什么

### 1. 统一 SQLite

`db.rs` 集中 `foreign_keys=ON`、`busy_timeout=5000`。初始化才开 WAL。热点路径用 `prepare_cached`。不要再在各模块手写一套 PRAGMA。

### 2. Schema 与索引

`v2_workspace.rs`：
- Workspace 目录格式版本仍是 **2**。
- SQLite `PRAGMA user_version` / `schema_meta.schema_version` 是 **4**（Reading Guide 表）。诊断读 `PRAGMA user_version`，不要读成字符串再解析错。
- 热点索引：OCR、Context Root、Discussion/Message、Artifact、Job、Usage、Trash、Remote Tombstone、Outline。
- `migrate_outline_epoch` 仅事务更新兼容标记，保留旧地图、任务及恢复材料；失败回滚标记。无损迁移与重复打开均有测试（地图 V2 复审，2026-09-11）。
- 无法识别的未来/损坏 schema：先把 db + WAL/SHM 挪到带时间戳的备份目录，再重建。V0.1 仍走 `reset_required`。

### 2.1 Hub 排序表（schema 不提升）

`v2_workspace.rs::ensure_hub_sort_tables` 用 `CREATE TABLE IF NOT EXISTS` 幂等建两张表，**不得**提升 `PRAGMA user_version` / `schema_meta.schema_version`（保持 7，见 D-061/D-062）：

- `collection_sort_prefs(collection_id, sort_mode, updated_at)`：该文件夹上次选用的排序模式。
- `collection_paper_order(collection_id, paper_id, position)`：手排行，键 `(collection_id, paper_id)`。

每次 `initialize_database` 和 `PaperModule::open` 都调用 `ensure_hub_sort_tables`，覆盖 V7/V6/Legacy/全新路径。手排行只在 `collection_id` 变化或 `trash_paper` 时删除（同文件夹改名不删，`restore_paper` 不恢复）。`reorder_collection_papers` 要求精确置换，未知 id / 集合不存在一律报错不落库。`list_collections` 批量加载 `sortMode`/`paperOrder` 时用 `?` 传播错误，不吞错。详见 [decisions.md D-062](decisions.md) 与 §A.7。

### 3. 查询与 IO

- `list_documents` / storage / Artifact list-head / 远程清理：单连接批量 SQL，不要 N+1。
- Discussion 路径 / 子树删除：递归 CTE，head 更新和删除在同一事务。
- PDF 导入：流式复制 + hash + `.part` staging rename。失败或冲突清 `.part`。不要 `fs::read` 整本 PDF。
- 去掉 `lopdf` 依赖（`Cargo.toml`）。页数不再静默降成 1。
- Mistral OCR 成功体先流式写入 staging，再 `BufReader` 解析。不要 `response.json::<Value>()` 再写盘。
- Gemini SSE：有上限的切片缓冲，不要整段 `drain` 复制。
- `OcrOutcome` 不再保留调用方用不到的完整 raw response。
- Trash：只清 30 天过期项和确认的孤儿 staging。不删未到期 Trash、持久 Artifact、历史 Revision。路径必须在 `.read-desktop/trash`，拒绝 symlink。

### 4. WorkspaceRuntime（正确性，不是“更快”）

`AppState` 只留一把：

```text
runtime: Mutex<Option<Arc<WorkspaceRuntime>>>
```

`WorkspaceRuntime` 内含：`generation`、`root`、`paper_module`、`job_module`、`artifact_module`、`outline_module`、`stop`、`notify`、`worker_count`。

`activate_workspace`：

1. `cancel_runtime_work`（取消 OCR / artifact / discussion flag）
2. drop 旧 `library_watcher`
3. 锁外打开四个模块、reconcile
4. `swap_runtime` **一次**换上新 `Arc`（旧的 stop + notify，不 join）
5. 再挂 watcher；失败则 take 新 runtime、stop、返回错误
6. `spawn_job_workers` / remote cleanup

命令用 `current_runtime(state)`，不要再读已删除的模块锁。

钥匙、取消表、`workspace_module`（打开器，不是当前工作区）、`library_watcher` 仍在 `AppState`。

测试锁住：
- `current_runtime_is_absent_until_swapped`
- `swap_runtime_installs_one_complete_snapshot_and_stops_the_previous`
- `swap_runtime_does_not_wait_for_old_worker_count`（stop 已立但 `worker_count` 仍可为 2）

### 5. Provider / 付费语义

- Gemini/Mistral 按 provider 复用 reqwest client。
- Context Root 按 `(database, revision, model, epoch)` 进程内锁；provider 返回后再 lookup，避免并发重复付费。Discussion 发布也走这把锁。
- Gemini 本地上传成功、后续 interact/SSE 失败：`ProviderError.orphaned_resource` 记 `gemini/file`。Discussion 失败路径必须 `record_remote_tombstone`。
- 发布 Context Root 时若 `COALESCE` 丢掉新 file id，把丢掉的 file 记墓碑。
- `interrupted_unknown`：Paper 删除时 `provider_committed=1` 的 Job 仍按原恢复规则，不要改成“确定没付费”。
- `PaperInteractionRequest.kind` 是调用方标签（Root / Discussion / Artifact / …）。Adapter 暂时不按它分支。构造请求时必须填；缺省用 `Discussion`。

### 6. Tombstone

- 登记：`ON CONFLICT` 时 `attempts = 0`，否则旧 backoff 会拖死重新排队的资源。
- 未知 provider/kind：`quarantined`，不要永远 pending。
- `queue_paper_remote_cleanup`：LIMIT/OFFSET 分批发现，不要无界 `Vec` 再逐条插。
- 列表 IPC：**无 LIMIT**。

## 踩过的坑

1. **`lib.rs` 会被别的工作树覆盖。** 本机同时存在 Chat Completions / model-settings schema 3 的提交。如果只留下 `workspace_lifecycle.rs` 却没改 `lib.rs` 的 `AppState`，模块是死的。提交前必须确认 `lib.rs` 有 `mod db`、`mod workspace_lifecycle`，且 `AppState` **没有** `paper_module: Mutex<...>`。
2. **Windows 上 `python3` 不存在**，用 `py -3`。Codex `apply_patch` 可能报 `CryptUnProtectData failed`，改走 escalate 的 apply-patch。
3. **`cargo fmt` / 并行编辑会撕 `lib.rs`。** 大文件拆分时先让 `swap_runtime` 测试绿，再搬函数。不要快照还红着就拆文件。
4. **`list_documents` 的 card helper 签名会变。** 当前 HEAD 的 `document_card_from_paper(root, paper)` 是两参数。不要假设一定有 `Connection` 第三参。
5. **Tauri 2 命令参数**仍要 `{ request: { camelCase } }`。本轮没改这个。
6. **Vitest 必须串行**，并行 jsdom 会 OOM。
7. **C 盘满**会导致 linker / Vitest `ENOSPC`。把进程 `TEMP`/`TMP` 指到 D 盘临时目录。不要删用户 Workspace。
8. 改 `webview_pinch.rs` 或 `lib.rs` setup 后必须重启 `tauri dev`。
9. `workspace.lock` 的 `File` 不是死代码。
10. `schema_version`（SQLite 4）≠ `WORKSPACE_FORMAT_VERSION`（2）。测试断言读错会红。
11. **JSON 通道会把中文标点降级为拉丁标点**。LLM 返回的 markdown 里 `，`/`。`/`：`/`；` 常被替换为 `,`/`.`/`:`/`;`，甚至降级为 `?`。`?` / `,` 可以当列表终止符，但**只有后面紧跟列表标记时**才拆（`1. 甲?2. 乙`），不要按每个 `?` 切段。不能把 `?` / `\uFFFD` 反推成中文标点。显示层合同 [markdown-rendering.md](markdown-rendering.md)。
12. **无条件 `replace("\\n")` / `replace("\\t")` 会吃掉 `\nu`、`\text`、`\rho`。** 仅当转义后面不是 ASCII 字母时才解码。
13. **不要对整个 `src-tauri` 跑 `cargo fmt --`。** 会误格式化 `library_commands.rs` / `paper_module.rs` / `lib.rs` 里无关的 collection IPC。只 fmt 正在改的文件。

14. **Hub 手排：同文件夹改名不要 `DELETE FROM collection_paper_order WHERE paper_id=?`**。只在 `collection_id` 变化时删；否则 F2 会清空顺序。见 `paper_module::move_paper` 与 reconcile 同层改名分支。
15. **Hub 手排：`reorder` 不要接受搜索过滤子集**。`paperIds` 必须是当层全部 live 的精确置换，否则未传入的 live paper 会被静默丢弃；前端拖拽始终发完整层顺序。
16. **Hub 排序：不要提升 `user_version` / `schema_version`**。两张表 `IF NOT EXISTS` 幂等建；`PaperModule::open` 也 ensure，覆盖全新/Legacy/V6/V7 四路径。
17. **`Statement` 借用会卡 `drop(connection)`**。`ensure_hub_sort_tables` 测试里把 `Statement` 限在块作用域再 `drop(connection)`，否则 `cannot move out of connection because it is borrowed`。
18. **拖拽 insert 索引不要只放 `useState`**。`LibraryHub` 用 `dragState` ref 存落点（松手读 ref），state 仅画虚线；热区收窄到 `.paper-cards-grid` / `.liquid-table-container`，搜索/排序条不是 end-drop。

## 验证（本轮实测）

在 `src-tauri`：

```powershell
cargo test --locked
cargo check --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --all -- --check
```

仓库根：

```powershell
npx tsc -b
npx vitest run --maxWorkers=1 --fileParallelism=false
git diff --check
```

最近一次（2026-08-30）：Rust **275** 测试；Vitest **42 文件 / 220** 测试；`npx tsc -b` 0 错；`npm run build` 成功；clippy `-D warnings` 仍有既有 `dead_code`/`type_complexity` 非绿（非本轮引入）。

未跑：冷启动 P95、空闲/单 PDF 内存、装包体积、六类真实 PDF、live provider smoke。

## 还没搬完的代码

- `send_chat` / generation persistence / publish 仍在 `lib.rs`。
- `run_job_worker` 仍在 `lib.rs`；当前实现仍可能在 `claim_next` 空/错时退出，不如加固版稳健。改 worker 时以 `workspace_lifecycle` 的 generation 隔离为准：worker 必须抓住 `Arc<WorkspaceRuntime>`，不要每次循环读“当前全局 root”。
- `*_commands.rs` 里部分 `*_impl` 还没接到对应 `#[tauri::command]`（命令体仍在 `lib.rs`）。接的时候只换函数体，不要改 IPC 名。

## 附录 A — 模块细则

> 这部分按"被改动后容易反复踩坑"的模块排列。下一轮 agent 改动前先扫相关小节。

### A.1 AppState / WorkspaceRuntime 拆分

- `AppState` **只**保留 `runtime: Mutex<Option<Arc<WorkspaceRuntime>>>`。`workspace_module` 只是打开器，不要把它当成当前工作区。
- 任何想要"加锁读 paper_module"的代码都改成 `current_runtime(state)?.paper_module.clone()`。
- 跨 runtime 切换（重启 Tauri / 切换 Workspace）时不要 `join` 旧 worker；只 `stop.store(true)` + `notify_waiters()`。

### A.2 Job / Tombstone 行为

- 一次 `provider_committed=1` 的失败不能改成"确定没付费" → 仍走 `interrupted_unknown` 路径。
- `queue_paper_remote_cleanup` 接收的是 paper id 列表，必须用 `LIMIT/OFFSET` 分批发现 context root 里的 file id，**不要**先 `Vec::new()` 再全量插。
- `record_remote_tombstone` 的 `attempts = 0` 写在 `ON CONFLICT` 分支里。重排队会覆盖旧 backoff。

### A.3 路径 / Symlink

- 任何接受外部路径的 IPC（`open_resource_dir` / `import_pdf`）必须：
  1. `symlink_metadata` 检 symlink，命中即拒。
  2. 路径若在 Workspace 内，`canonicalize` 后 `starts_with(canonical_root)`。
  3. 路径含 `..` 组件（用 `Path::components().any(Component::ParentDir)`）即拒——不要 `str.contains("..")`，会误伤 `foo..bar`。
  4. 相对路径（"Papers/X"）必须 `root.join` 后再递归检查，不能直接 `explorer path`。
- `trash_paper` 只设 `deleted_at`，不删 `collection_id`。后续 `DELETE FROM collections` 受 `ON DELETE RESTRICT` 阻挡，必须先把引用此 collection 的 paper（含 deleted）`UPDATE collection_id = root_id` 再删。

### A.4 Markdown 写入归一化（`normalize_markdown_field`）

权威规则表、现场样本、禁止项：[markdown-rendering.md](markdown-rendering.md)。这里只记后端接线。

- Lens：`normalize_lens_markdown_fields` 在 `validate_lens` **之后**立刻跑 `quickTakeaway` / `overallMarkdown` / `explanationMarkdown` / `summaryMarkdown` / `sections[].markdown`，**不能**延迟到 publish。
- Brief：`lib.rs` `parse_orientation_pack` 对 takeaway / findings / evaluation 等字符串字段调用同一函数。keywords 不走。
- 步骤：① `decode_literal_escapes`（`\n`/`\r`/`\t` 仅当后面不是 ASCII 字母）；② 折叠水平空白、保留换行；③ `repair_inline_bold` **按 char 迭代**；④ `split_packed_list_items`：行首、或终止符后、或连续编号 `n`→`n+1` 才拆。编号前是拉丁词（`Figure 1.`）不拆。`**…**` 内部不拆。
- `is_list_terminator` 含 `。！？!?；;：:，,` 和 U+FFFD。这些字符本身不切段，只作为「下一个 `2. ` 可以换行」的条件。
- 历史成果不在写入层修，靠显示层 `prepareMarkdown`。不要写回用户 SQLite。
- 测试：`cargo test --locked normalize_markdown`（含 `preserves_latex_commands`、`does_not_split_figure_prose`、live DB 只读）。

### A.5 Hub 整理（右键/拖拽）— 见 `agent-onboarding.md` §33

- `library_commands.rs` 现在持有所有 collection / paper 命令实现；`lib.rs` 仅留 `#[tauri::command]` + request 结构体透传。
- `trash_collection_impl:307` 在循环 `trash_paper` 后对每篇已 trash 的 paper 调 `queue_paper_remote_cleanup` + `spawn_remote_cleanup_worker`，**不**等所有都成功——部分成功也要登记 tombstone。
- `open_resource_dir_impl:361` 用 `Path::components().any(Component::ParentDir)` 拒 `..`；绝对路径必须在 Workspace 内；相对路径 `root.join` 回调自身。
- `rename_paper` **只**写 `paper_metadata.title`（`document_revisions` 没有 `title/updated_at` 列）。
- 前端 `LibraryHub.tsx` 的拖拽阈值是 8px；`suppressNextClickRef` 用 `setTimeout 350ms` 兜底清理，避免吞掉下一篇论文的单击。

### A.7 Hub 排序（manual / chapter）—— D-062

完整合同、踩坑日志、如何加排序：[hub-sort.md](hub-sort.md)。Windows 编译 OOM 不要写进本模块，见 [windows-dev-build.md](windows-dev-build.md)。

- 建表：`v2_workspace::ensure_hub_sort_tables`，`initialize_database` 四路径 + `PaperModule::open` 都要跑；**不升** `user_version`。
- 写盘：`reorder_collection_papers` 精确置换；同层改名不删 order 行；删行只走 `remove_paper_from_manual_order`。
- 前端：`hubSort.ts` 是唯一排序入口。拖拽落点在 `dragState` ref，热区只有网格/表。
- 改本模块必跑 [hub-sort.md §7](hub-sort.md) 的命令；搜索时拖拽不写盘是易回归点。

### A.8 变更测试模式

- 改 schema → 在 `v2_workspace.rs` `migrate_outline_epoch` 测试附近加版本号路径断言；Hub 排序表在 `ensure_hub_sort_tables` 附近，断言仍 7。
- 改 Job worker → 不写跨 runtime 的全局 `current_*` 读取；以 `Arc<WorkspaceRuntime>` 为单元。
- 改 LLM adapter → **必跑** `provider_routing` 路径；`chat_completions.rs` 的 fingerprint 算法一旦改动，所有 cache 必须清。
- 改 Tauri command → 同步改 `desktopClient.ts` 的类型 union；前端 `tsc` 会指出漏接。
- 改 Hub 排序 → 必跑 `hubSort` + `LibraryHub` + `paper_module` 6 测；搜索时拖拽不写盘是易回归点。

---

## 附录 B — Git / 工作流

- 不要 squash `lib.rs` 大改到其它分支的 WIP commit；`lib.rs` 在多分支并行编辑易冲突。
- `cargo fmt` 后 `cargo check` 再提交。`cargo fmt --all -- --check` 必须绿。
- PR title 用 `feat(scope): ...` / `fix(scope): ...` 风格；scope 限定模块（`hub` / `lens` / `provider` / `db`）。
## API 获取链接（2026-09-11）

`api_key_links.rs` 从编译期嵌入的官方资源目录按 ID 解析链接，通过系统浏览器打开；命令不接收 URL、凭据或用户路径。`SettingsModelsPage` 只发送资源 ID。浏览器预览使用普通安全新窗口链接，桌面走 `open_api_key_page`。

## 正式安装包本地资源 CSP（2026-09-11）

安装版加载 PDF 时需在 connect-src 明确允许 asset: / http://asset.localhost 和 ipc: / http://ipc.localhost。img-src 同样包含 Windows 资产域，保持 assetProtocol.scope 默认空，运行时只放行当前文库。开发服务器通过并不证明发布 CSP 可用。问题由实际 NSIS 安装版的 securitypolicyviolation 事件复现；tests/desktopCsp.test.ts 覆盖必需协议与禁止泛化通配的边界。
