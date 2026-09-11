# 数据协议

## 公共投影与命令

- `open_api_key_page({ request: { resource } })`：只接受 `gemini/openai/grok/mistral/deepseek` 固定资源标识，在系统默认浏览器打开随应用打包的官方获取页面；不接受任意 URL、不读取或传递 API Key，不启动模型探针。前后端共用 `src/providerKeyResources.json`。

前端通过 `DesktopClient` 使用 `open → command → watch`，不在组件中散落 provider 或数据库调用。浏览器 adapter 只提供内存 UI 预览。

- `WorkspaceInfo`：`rootPath/databasePath/libraryPath/available/statusDetail`。旧布局使用稳定状态 `available=false, statusDetail=reset_required`；该投影只用于确认流程，不代表 active Workspace。
- `LibraryProjection`：cursor、Collection 树、Paper、冲突。
- `PaperProjection`：Document Revision、Reader/OCR/Artifact/Discussion 状态。
- `JobProjection`：kind/provider、paper/revision、dedupe、state/stage、providerCommitted、priority、payload、lastError。
- `StorageReport`：Workspace/Papers/内部真实总量；论文/Artifact 只报告逻辑 payload。
- `ReadEvent`：`library | paper | job | generation`，含 cursor/entity/delta/status。漏事件时重新读取投影快照。

### 独立文档成果（D-066）

- `generate_brief({ revisionId })` 只生成 Brief。`generate_document_artifact({ request: { revisionId, kind, useBrief? } })` 生成指定成果，kind 为 `brief | glossary | symbol_table | metadata`，useBrief 仅允许术语表且默认 false。
- 新任务冻结与实际稿匹配的 `documentArtifactProtocol: "v1" | "v2"`、`responseSchema`、`documentArtifactKind`、`prompts.orientation`（所选单项系统稿）、`prompts.paperRoot`、`prompts.documentKind`；术语表选择参考 Brief 时另存 `briefSource` 的 artifactId / version / revisionId / 四项主题线索。
- Brief 保留 `orientation_pack` job kind；其他三类使用 `document_artifact`，独立 artifact key / dedupe key / head。无独立成果协议标记的历史任务仍走旧 Pack 处理器。
- 单项生成响应按严格 Schema 校验，Brief 恰含九字段。完整响应先落 checkpoint；校验通过才发布 Artifact。已收到响应但本地发布失败时可沿原任务恢复；模型输出不合约时可重新生成该成果。付费重试继续遵守现有 Job 恢复策略，不自动重复请求。
- `pdf-source-v2` 纳入实际根稿 SHA-256 后经 exact route 哈希形成 epoch，仅接纳最小确认根。原始 PDF 保留；不把四类成果隐式写入共享历史。schema 8 无变更。详见 [brief-generation.md](brief-generation.md) 与 [auxiliary-generation.md](auxiliary-generation.md)。

## SQLite V2

数据库位于 `.read-desktop/read-desktop.sqlite3`，启用 foreign keys、WAL 和 busy timeout。核心表：

- 文库：`collections`、`papers`、`document_revisions`、`paper_heads`、`paper_metadata`、`tags`、`paper_tags`、`reading_states`。
- 上下文：`provider_nodes`、`context_roots`、`discussions`、`messages`、`message_contexts`、`discussion_heads`。
- OCR/成果：`ocr_revisions`、`ocr_pages`、`ocr_blocks`、`artifacts`、`artifact_heads`、`term_overrides`、`symbol_overrides`、`lens_qa`。
- 任务/审计：`jobs`、`job_attempts`、`job_checkpoints`、`usage_receipts`、`operation_journal`、`reconciliation_conflicts`、`trash_entries`、`remote_tombstones`。
- Outline：`outline_revisions`、`outline_heads`、`outline_deep_dive_heads`、`outline_plans`。`schema_meta.outline_epoch`（现行 `3`）落后则一次性清空上述表和 Outline Job；**不要为 v4 提示词换代递增该常量**。Job kind：`outline_overview`、`outline_deep_dive`。协议：旧任务 `outline-extract-v3` / `outline-compose-v3` / `outline-deep-dive-v3`；新任务 `outline-map-v4`（构图 `outline-draft-v4`，复核 `outline-review-v4`，局部 `outline-deep-dive-v4`）。详见 [outline-generation.md](outline-generation.md) 与历史 [full-outline-v1.md](full-outline-v1.md)。

Document/OCR/Artifact revision 不可变；发布在事务内写 revision 后移动唯一 head。依赖快照与任务 payload 在入队时固化，worker 不读取会漂移的 UI 选择。

## Block 与引用快照

入库 bbox 一律为 `[x0,y0,x1,y1]`，坐标 0–1000。Mistral OCR 4 原始字段可能是数组，也可能是 `top_left_x/y` + `bottom_right_x/y`；规范化层两者都收。旋转/缩放/DPI 只经过 Reader 坐标转换。

`BlockQuoteSnapshot`：

```text
revisionId, ocrRevisionId, blockId,
pageNumber, blockIndex, blockType,
textContent, contentDigest, bbox
```

`send_chat` 的 IPC 形状是 `{ request: ChatRequest }`，请求里只带 `blockIds`（外加 revision/thread/parent/question/page/`regenerateFromId`）。Rust 从当前 `ready` OCR 重新读取 canonical snapshot，保序去重，最多 32 个；伪造、旧 OCR 或其他 revision 的 ID 被拒绝。快照写入 `message_contexts`，重 OCR 不改写历史消息。`list_messages` 不返回 `failed` 行。

Citation 可使用合法页码或本轮 Block。Block citation 投影 page、region、digest 和 excerpt，供 UI 回跳 page/bbox；未被 provider 合法标记的回答保持 uncited。

## Artifact

Artifact 使用 `(paper_id, kind, object_key, version)` 唯一版本和 `(paper_id, kind, object_key)` head。`dependency_snapshot_json` 至少固定 revision、OCR revision/Block digest、model/context epoch 和相关 Artifact version。

Lens schema 先严格验证；第一次失败最多一次 repair，并分别记录 receipt。Lens 重生成只有成功发布后才切 head 和删除旧 QA；失败不动旧结果。

## Provider seam

`PaperModelPort` 暴露 capability、PDF/文本 Interaction、stream、远端删除；`OcrPort` 暴露整篇 OCR、进度/取消和远端删除。UI 不接触 provider file/interaction ID。

V2 production adapter = Gemini Interactions **或** Chat Completions（OpenAI-compatible / Grok），OCR 仍只 Mistral；测试使用 fake/HTTP fixture。Gemini `input` 使用 `text` / `document` / `image`，不要再发 `input_text` / `input_file`。

### P0-2 Provider route 目标合同

此段自 schema 7 activation 起生效。D-063 PR 1 已把生产升到 schema 8，但本段一条规则都没动：v8 迁移只新增文库侧的 revision / Batch / Lifecycle / Smart Collection 表，`assert_seeded_provider_route_graph_is_preserved` 逐条断言 route 图在迁移后原样保留。

- `ProviderSelection` 只接受 current 或精确 instance UUID；`ProviderCredentialPort` 只允许按 UUID 读 Key，不提供 kind/list/first fallback。
- `FrozenModels` 固定 paper/translation 模型；operation role 独立持久化。`endpoint_scope = hash(instance + kind + canonical endpoint + exact-key digest)`，`route_id = hash(endpoint_scope + frozen models + operation role)`；两者用版本化长度前缀编码。
- `BoundProviderRoute` 是不序列化、脱敏 Debug 的内存执行句柄；即时请求直接使用它。Durable Job 只保存 `FrozenProviderRoute` 的规范化 endpoint/route snapshot，worker 用 exact Key 重算 scope 后 bind。
- `JobExecutionRoute = Local | MistralOcr | Paper(FrozenProviderRoute)`；内部 worker 读取 `JobRecord`，前端只接收安全 `JobProjection`。Projection 不含 Key、digest、scope、route ID、完整 Base URL 或 snapshot 原文。
- `JobProjection.providerRoute` 只投影 instance ID/name、kind、冻结模型、安全 endpoint label 与 `ready | legacy | action_required`；`providerRequirement` 投影 typed code、可空 kind/instance、`canRebind` 与 `providerCommitted`。
- requirement 使用 `paused + job_provider_requirements`；`JobModule::resume` 自身必须阻止未解决 requirement。Rebind 冲突返回 `AlreadyActiveOnRoute(existingJobId)`，原 paused Job保持不变。
- `remote_endpoint_snapshots.owner_type` 区分 `paper_provider` 与没有 Provider UUID 的 `mistral_ocr`。Paper 与 Mistral 都以创建远端资源时的精确 credential identity 失败关闭；legacy tombstone 永不自动或猜测 DELETE。
- 新 URL 只接受 `http/https`，拒绝 userinfo/query/fragment，保留合法路径；任务中心只显示安全 host/label。

## Schema 8 revision 与 `library_read`（D-063 PR 1）

> 权威 ADR：[decisions.md D-063](decisions.md)。设计与切片：[library-workspace-plan-2026-08.md](library-workspace-plan-2026-08.md)。本节只写已经跑在生产代码上的合同。

### Revision 计数器

`PRAGMA user_version = 8` 建两张计数器表：

```sql
CREATE TABLE library_change_seq (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  value INTEGER NOT NULL CHECK (value >= 0)
);
CREATE TABLE library_domain_revisions (
  domain TEXT PRIMARY KEY CHECK (
    domain IN ('structure','tags','lifecycle','engagement',
               'artifacts','jobs','smart_collections','sort')),
  value INTEGER NOT NULL CHECK (value >= 0)
);
```

- 规则：**改变 Hub 可见状态的写，必须与被改数据同一事务**调用 `library_workflow::bump_library_revisions`，它同时推进全局 seq 与所列域。表缺失即失败关闭，不返回「看起来仍是最新」的 revision。
- `jobs` 域的子规则见 `job_module::bump_jobs_state` 文档注释：移动 `jobs.state` 的写全部 bump（enqueue、transition、mark_provider_committed、claim、complete/fail、block、resume、rebind、abandon、recovery、pause/cancel exit、startup 清理）；只写 `stage`/checkpoint 或只写 `priority` 的进展类写不 bump。
- 明确**不** bump：任何读、`save_reading_state`（Reader 视口与草稿，见 D-063 决定 5）、同路径 `move_paper`、顺序未变的 `reorder_collection_papers`、命中已存在冲突的 `create_conflict`、`cleanup_expired_trash`、`ensure_root_collections`。
- 八个域都有写入方。`lifecycle` 由 `apply_lifecycle_patch` / 批次 PatchLifecycle 与第一次打开 Reader 推进；`engagement` 由 `record_reader_activity` 推进（`save_reading_state` 仍不 bump）；`smart_collections` 由创建 / 重命名 / 删除用户查询推进。整批 lifecycle 只 bump 一次。

### `library_read`

只有一个读命令：`library_read`，参数 `{ request }`，`request` 是带 `protocolVersion: 1` 的判别联合。写侧是另一个命令 `library_act`（见下一节）。两者都没有 `{ command, payload }` 形状——判别只在 `kind` 上，字段一律 `deny_unknown_fields`。

| 请求 | 语义 |
|------|------|
| `{ kind: "hub_page", protocolVersion: 1, page: { sort, filters, limit, cursor } }` | 有界 keyset 分页投影 |
| `{ kind: "watch_handshake", protocolVersion: 1, after }` | 只回 `currentRevision` 与 `invalidated`，不读卡片 |
| `{ kind: "batch", protocolVersion: 1, batchId }` | 单个批次聚合投影 |
| `{ kind: "batch_items", protocolVersion: 1, batchId, afterOrdinal?, states?, limit? }` | 逐项结果按 ordinal 键集分页，缺省 100 / 上限 200，`states` 是 `ItemState` 白名单 |
| `{ kind: "recent_batches", protocolVersion: 1, limit? }` | 任务中心批次分组：按 `updated_at` 倒序，缺省 50 / 上限 100；读前先 reconcile |
| `{ kind: "smart_collections", protocolVersion: 1 }` | 六个内置查询（代码提供，不入库）+ 用户保存的 query AST |
| `{ kind: "reading_context", protocolVersion: 1, paperId, revisionId }` | Paper 级 Lifecycle + 该 revision 的 Engagement；无行即默认 unread / 无进度 |
| `{ kind: "collection_layer", protocolVersion: 1, collectionPath }` | 物理叶子 collection 的全部 live Paper id，按手动顺序（无存档则 `created_at`）；给 D-062 精确置换，不带卡片投影 |

`sort ∈ recent | year | title | manual | last_opened | chapter`。`manual` 与 `chapter` 必须带精确（`recursive: false`）collection 过滤，否则 `invalid_query`。`chapter` 用 SQLite 确定性函数 `chapter_sort_key`，从当前 metadata artifact 的 `$.chapterNumber` 取值，分段整数零填充（`1.2` < `1.10`），无法解析的排在后面。`filters` 另含 `status` / `favorite` / `read_later` / `review_due` / `imported{this_week}` / `ocr_failed` / `opened`，上限 8 条，一律绑定变量。`this_week` 按 `evaluationAnchor` 所在周周一 00:00 UTC 求值。`ocr_failed`：当前 head revision 没有 `ocr_revisions`，且该 revision 最新 `jobs.kind='ocr'` 为 `failed|interrupted_unknown`。`limit` 缺省 100、上限 200。

响应 `page`：`papers, totalCount, pageSize, sort, hasMore, nextCursor, queryDigest, selectionDigest, selectionDependencies, evaluationAnchor, evaluationTimezone, revision, dependencies, dependencyRevision, protocolVersion`。卡片只带库内相对路径（`relativePath` / `collectionPath`），绝不含绝对路径或盘符。`selectionDigest` 标识成员集合（Selection Snapshot 用这个），**不要**把带 sort/limit 的 `queryDigest` 填进去。

- `queryDigest = sha256(canonical JSON of the query AST)`，**不含 cursor**：同一查询的任意页共享 digest。因此它只能标识查询，不能标识内容——同数量替换下 digest 与 `totalCount` 都不变，只有 `revision` 变，缓存键必须是 revision。
- `nextCursor` 是 `{"digest","key","id"}` 的 base64url（无填充）。`key` 是该 sort 的 TEXT 排序键（`recent`→`created_at`，`year`→零填充可比较值，`title`→小写标题，`manual`→零填充 position，`last_opened`→`eng.last_opened_at`，`chapter`→`chapter_sort_key(...)`，缺失位置为 `i64::MAX` / 空串），tie-breaker 恒为 `id` 升序。带错 digest 的 cursor 一律 `invalid_query`，不复用「省一次首页读取」。
- `dependencyRevision` 形如 `artifacts=0;engagement=0;jobs=0;lifecycle=0;sort=0;structure=0;tags=0`，**按数据库名排序**；`hub_page` 声明这七个域（不含 `smart_collections`）。卡片另含 `lifecycleStatus` / `favorite` / `priority` / `readLater` / `reviewAt` / `lifecycleVersion` / `furthestPage` / `lastOpenedAt` / `ocrFailed`。
- 失败只抛 `{ code: "invalid_query" | "workspace_unavailable", message }` 两个码之一。判定只看 `code`；`message` 是给人看的诊断，可能带底层 SQLite 原文——SQL 全部由白名单拼装、输入一律绑定变量，所以它不含 Key、Provider 身份或绝对路径。revision 计数器读不到归 `workspace_unavailable`（Workspace 状态问题，不是请求不合法）。`workspace_unavailable` **不等于**「没有结果」：UI 必须保留上一份快照并提示重试，不能把读失败的库显示成空文库。

### watch

Tauri 侧仍是 `read-event`：事件只当提示，任何 kind 都触发一次 `watch_handshake`，只有全局 revision 真的前进才通知订阅方（§10.3）。前端 seam **先 `listen` 并缓冲，再握手**；握手窗口里到达的事件必须在 snapshot 之后重放，不能丢掉。实现在 `src/library/libraryWorkspaceClient.ts`，线格式镜像在 `libraryWorkspaceTypes.ts`，浏览器预览 Adapter 与桌面共用 `src/library/libraryWorkspaceContract.test.ts`，字节合同来自 Rust 生成的 `src/library/__fixtures__/library_read_v1.json`（改投影后用 `LIBRARY_FIXTURE=update cargo test library_query` 重新生成）。预览端没有 sha256，digest 是 `mem-` 前缀的 FNV-1a，**只与自身比较**。

`LibraryHub` 在注入 `libraryClient` 时用 `hub_page` 分页画卡片（默认 100 / 上限 200，虚拟窗口，滚动近底加载下一页），用 `collection_layer` 取整层 live id 做 D-062 精确置换。`list_documents` 仍给 App 侧栏树计数与 Reader 恢复目标。Explorer / 多文件 picker 都走同一条 Import Batch（一次最多 500 个源；目录和非 PDF 记为逐项 `skipped_not_pdf`，前端不得先滤掉）。落在物理目录行上时用该目录，落在主列表时用当前物理目录，智能集合 / 「全部」不能猜。

### `library_act`（D-063 PR 3）

唯一写命令：`library_act`，参数 `{ request }`，`request` 是带 `protocolVersion: 1` 与调用方 `idempotencyKey` 的判别联合。禁止 `{ command, payload }`。

| 请求 | 语义 |
|------|------|
| `{ kind: "plan_batch", command, target }` | `BEGIN IMMEDIATE` 解析成员、冻结 precondition、写 Batch / Items / receipt，不改业务表 |
| `{ kind: "start_batch", batchId, planDigest, acceptedRequirementIds, maximumAcceptedEstimateByCurrency }` | 校验确认项与 planDigest；纯 SQLite 动作同事务 SAVEPOINT；Move / Trash / Import / Export 先提交 queued 再事务外逐项执行 |
| `{ kind: "control_batch", batchId, control }` | `cancel_remaining` / `retry_failed{itemIds}` / `undo{token}` |

另有 `{ kind: "change", change }`（`patch_lifecycle` / `create_smart_collection` / `rename_smart_collection` / `delete_smart_collection`）和 `{ kind: "record_reader_activity", activity }`。`command.kind ∈ patch_tags | move | trash | import | export | patch_lifecycle | ocr | brief`。`target.query` 可带 `evaluationAnchor` / `evaluationTimezone`，相对日期在快照时刻求值。`stale_library_snapshot` 表示 Lifecycle version 已变。

### Provider 批量 OCR / Brief（D-063 PR 5）

- Plan 调用 JobModule `prepare_exact`：只持久化冻结 route 与 spec，返回不透明 `preparedJobHandle`。Handle 不是可 claim Job，与 Plan 同 TTL（15 分钟）。
- Start 在同一个 `BEGIN IMMEDIATE` 里 exact bind 当前 route、consume handle、enqueue/coalesce、写 `library_batch_job_links`（`created` / `joined`，一个 Job 最多一个 created owner）、把 Item 置 `queued`。提交后才唤醒 worker。
- `BatchProjection.costPreview`：`marginalEstimates[]`（`exact` / `upper_bound` / `estimate` + 十进制金额 + `priceCatalogVersion`）、`unknownItemCount`、`unknownReasonCodes`、`joinedExistingJobCount`。未知费用不显示为 0。多币种分开。预览不发网络请求。
- Start 的 `maximumAcceptedEstimateByCurrency` 只防止确认后估算上升，不是账单上限。价目版本变化或估算上升返回 `stale_batch_plan`。
- 新增错误码：`provider_route_unavailable`、`cost_confirmation_required`、`possible_duplicate_charge`。
- OCR 跳过当前 revision 已有 `ocr_revisions` 的 Paper；Brief 跳过没有 OCR 或已有 Brief 的 Paper。长 PDF（≥80 页）是确认项。
- OCR / Brief 的 `undo_policy` 是 `cancel_only`。不显示「撤销费用」。取消剩余：joined 只 detach；created 且未 commit、无其他活跃消费者才取消 Job。
- Job 状态变化在同一事务同步到仍 active 的 Batch Item。投影、事件与诊断不含 Key、route ID、完整 Base URL。

响应是 typed `BatchProjection`；Start / Control 可附带一次性明文 `undoToken`（库里只存 hash，默认 10 分钟）。错误码见计划 §10.2，另有 `batch_not_found` / `confirmation_required` / `batch_too_large` / `invalid_source`。投影、事件与诊断不含绝对源路径、Key 或 route。

父 Batch / Item 终态不因 retry / compensation child 改写。Item 态是权威，`library_batches.state` 是缓存；`act` 与 `recent_batches` 都会 `reconcile_interrupted`。

## Hub 排序（manual / chapter）—— D-062 数据合同

> 权威 ADR：[decisions.md D-062](decisions.md)。上手与踩坑：[hub-sort.md](hub-sort.md)。本节是可执行的表/IPC/投影/不变量合同。

### 投影

`CollectionProjection` 在 `list_collections` 中新增：

| 字段 | 类型 | 含义 | 缺省 |
|------|------|------|------|
| `sortMode` | `"recent" \| "year" \| "title" \| "manual" \| "chapter"` | 该文件夹上次选用的排序模式；无偏好行时 `"recent"` | `"recent"` |
| `paperOrder` | `string[]` | 已持久化的手排 paper id 有序数组；无行时 `[]` | `[]` |

画面回落到 `recent` 时**不把回退写成偏好**（`collection_sort_prefs` 不变）。

### 表结构（幂等，不得升版）

`v2_workspace::ensure_hub_sort_tables` 用 `CREATE TABLE IF NOT EXISTS` 建表，`PaperModule::open` 与 `initialize_database` 均调用。D-062 当时约定这两张表**不单独升版**；当前生产库是 **schema 8**（D-063），validator 把这两张表纳入完整签名，新 Workspace 的 `PRAGMA user_version` 是 8，不是 7：

```sql
CREATE TABLE IF NOT EXISTS collection_sort_prefs(
  collection_id TEXT PRIMARY KEY REFERENCES collections(id) ON DELETE CASCADE,
  sort_mode     TEXT NOT NULL,
  updated_at    TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS collection_paper_order(
  collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
  paper_id      TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
  position      INTEGER NOT NULL,
  PRIMARY KEY (collection_id, paper_id)
);
-- 建议索引：(collection_id, position) 覆盖排序读取
```

### IPC 合同

| 命令 | 请求形状 | 成功 | 失败（不落库） |
|------|----------|------|----------------|
| `reorder_collection_papers` | `{ request: { collectionId: string, paperIds: string[] } }` | 覆盖写入该 `collection_id` 全部 live id，`position = 0..n-1`，发 `library` 事件 | `paperIds` 非当层全部 live 的**精确置换**（缺/多/未知 id）、未知 `collectionId` → 报错字符串含 `paperIds must be exact permutation` / `Collection does not exist`；顺序未变 → no-op 不写 |
| `set_collection_sort_mode` | `{ request: { collectionId: string, sortMode: HubSortMode } }` | `INSERT OR REPLACE` 到 `collection_sort_prefs`，发 `library` 事件 | 非白名单 `sortMode` / 未知 `collectionId` → 报错 |

`HubSortMode` 白名单：`"recent" | "year" | "title" | "manual" | "chapter"`。

### 不变量与顺序维护规则

| 规则 | 说明 | 强制点 |
|------|------|--------|
| 新建/挪入/恢复补位 | 不在 `collection_paper_order` 的 live paper 按 `importedAt` 升序接在已存顺序后 | `hubSort.ts:applyHubSort`，后端不自动补位 |
| 同文件夹改名不删行 | F2、同 `collection_id` 的 `move_paper`、reconcile 同层改名不 `DELETE` | `paper_module::move_paper` 仅 `collection_id` 变化时删 |
| 跨文件夹/跨根删行 | 新 `collection_id` 生效，旧 order 行被删 | 同上 |
| 回收站 | `trash_paper` 删行；`restore_paper` 不恢复旧 `position` | `paper_module::trash_paper` / `restore_paper` |
| 首次 manual 不写库 | 无 order 行时前端不发 `reorder` | `LibraryHub` 拖拽提交守卫 |
| 章节排序 | 无 `libraryClient` 时纯前端 `hubSort.ts`；有 client 时 `hub_page` 用 SQLite `chapter_sort_key`（同一分段整数算法）。至少一份可解析才启用 `chapter` | `hubSort.ts` / `chapter_sort.rs` |

### 前端排序优先级

`hubSortAvailability(documents, selectedFolder)` 决定可用模式；`effectiveMode` 按「偏好 → 可用性」回落。`applyHubSort(docs, effectiveMode, paperOrder)` 是唯一排序入口，网格与表格共用。

### 扩展检查清单

- [ ] 新增 `HubSortMode` 是否同步改了 `set_collection_sort_mode` 白名单、`hubSort.ts` 比较器、前端下拉与 `data-protocols` 白名单？
- [ ] 新表是否 `IF NOT EXISTS` 且评估了是否需升 `user_version`（默认不升）？
- [ ] `reorder` 是否仍要求精确置换且未被搜索子集绕过？

## Usage

Receipt 字段均按 provider 事实保存：input/cached/uncached/output/reasoning token、latency、decimal-string cost、file reuse、session resume、paper root branch、provider/model/context epoch。未知值为 SQL `NULL` / JSON `null`，不转换为 0。

只保留脱敏 usage envelope，不保存完整 Gemini/Mistral response。Mistral raw JSON 只在 OCR staging 生命周期内用于崩溃恢复，发布/清理遵循 Job checkpoint。

## 时间、金额与隐私

时间统一 UTC RFC3339；金额是 decimal string。普通配置和 Workspace 不保存密钥。诊断 JSON 只输出聚合计数/状态/字节，不输出绝对路径、论文身份、Prompt/PDF/OCR/Discussion 正文、provider ID/response 或任务错误正文。


## 辅助成果 v2（2026-09-08）

完整模型契约与应用保存格式分别维护，见 [auxiliary-generation.md](auxiliary-generation.md)。术语表增加 usage 和 sources，符号表增加 sources；metadata 统一为 document、container、relatedVersions、sources、issues。旧 v1 Schema 保留，已入队任务和响应检查点按原协议恢复。

应用数据使用 `_format: auxiliary-v2`，`_model` 保存模型原稿；人工覆盖、稳定身份、隐藏条目、展示选择和关联状态位于应用字段。编辑发布新版本，不改写历史原稿。`update_paper_metadata` 接收 `_editBaseArtifactId`，`update_orientation_table` 接收可选 `artifactId`，新版界面均发送当前所见版本，过期编辑被拒绝。

文库卡片可带 `displayMetadata`（缺省时按旧卡片读取）；显示年份与 publicationYear 分开，前者包含事件或来源标签，后者不借用来保存发布年或所属出版物年。`hub_page.page.direction` 可选 asc / desc，默认保持既有排序；方向进入分页指纹，年份未知值和不可解析章节号在两个方向都位于末尾。界面与内存客户端按同一选择值排序。


## 翻译 v2（2026-09-09）

翻译新默认输出 `status / sourceLanguage / targetLanguage / translation / notes / terms`，四种状态与空值规则详见 [翻译生成合同](translation-generation.md)。`unavailable` 允许空译文并要求具体译注和空术语数组，作为正常成果发布；网络及协议错误仍走任务失败。

Job 的 `prompts.translationProtocol` 与提示词在入队时一起冻结。缺失该字段的旧 Job 使用固定 v1 五字段合同；历史自定义稿保留 v1，新默认与基于新默认编辑的稿件使用 v2。成果仅在模型校验后由应用附加 `translationProtocol` 标记，旧成果不补写状态。

## 解释完整中文稿与既有五字段协议（2026-09-09）

解释的 `block_explanation` schema 仍为五字段 `title / explanation / keyPoints / paperConnection / evidenceIds`，协议 v1，不增加状态字段。后端在发布前校验完整既有结构及当前块引用白名单。`keyPoints=[]`、`paperConnection=""` 合法；无法可靠解释时由非空标题和正文说明材料限制，不自动生成新的状态。

配置增加 `explanationGeneration=1`，仅升级已知旧默认稿并备份原始配置；自定义稿、上一版、旧冻结任务与符合既有 schema 的成果保持兼容。详见 [解释生成合同](explanation-generation.md)。

## 2026-09-09：导师式精读路线

路线结构仍为 version 1，保留阶段与任务字段；论文的 elevatorPitch、oneChart 放宽为可选，界面分别显示复述自检提示和优先精读对象，旧成果兼容。新任务冻结 schema、文档种类、路线／根提示词、可选 Brief 与读者背景；从 PDF 来源根生成独立旁支。发布前验证类型、任务标识唯一性与页码边界；无 OCR 目录时拒绝 blockId。prompt-settings schema 2 增加 roadmapGeneration=1，只升级已知旧默认稿并备份原始文件。完整约定见 [精读路线生成合同](reading-roadmap-generation.md)。

## 学术讨论与压缩（2026-09-09）

讨论保持自由 Markdown；压缩保持三个模型输出字段 `summaryMarkdown: string`、`retainedClaims: string[]`、`unresolvedQuestions: string[]`，无新协议版本。严格拒绝缺键、多键、错误类型与空白内容。应用自行追加源消息审计元数据；源状态记录生成是否完成，不表示内容已核实。

成功压缩后的当轮恢复携带完整三字段、当前问题、读者背景和本轮块白名单。历史引用不扩充当前权限。设置 schema 仍为 2，新增 `discussionGeneration=1`，只升级已知旧默认稿，保留原始备份、自定义与上一版。详细合同见 [discussion-generation.md](discussion-generation.md)。

## 2026-09-09：论文 Lens 完整中文稿与 v2

论文 Lens v2 使用 status／limitations、明确的图表 readingGuideMarkdown 与 focusPoints，删除未定义内部结构的旧数组，推导和计算进入 sections。字段与状态在发布前本地校验；v2 正文不经旧写入归一化。prompts.lensProtocol 随生成稿冻结，旧任务缺值默认 v1；旧自定义稿、教材和旧成果保留兼容。Workspace 与数据库 schema 不变，详见 [Lens 合同](lens-generation.md)。

## 2026-09-09：Lens 追问完整中文稿

Lens QA 输出仍是 answerMarkdown 与 evidenceIds，v1 和配置 schema 2 均不变。本地开始完整检查既有 strict schema，包括额外键。无效模型输出不发布 assistant，保留返回的 invalid provider 节点和对应 receipt；不新增 repair。已知旧默认稿经 lensQaGeneration 一次迁移，保留自定义及上一版。详见 [追问合同](lens-qa-generation.md)。

## 2026-09-11：教材 Brief 与 Lens 适配

教材新Brief使用独立briefProtocol=textbook-v2，正文九字段为takeaway、keywords、learningScope、motivation、prerequisites、knowledgeStructure、coreKnowledge、masteryGoals、connections；应用元数据不进入模型输出。旧论文与旧教材结构保留，未标记的旧成果不自动转换。单篇及Hub批量冻结相同协议和schema，get_brief返回新字段与现有公共元数据，summary兼容别名来自归一化后的takeaway。

教材Lens新默认复用既有v2 schema，任务冻结documentKind，生成/修复的默认选择使用文档类型及协议。配置schema仍为2，textbookGeneration与textbookProtocols分别记录迁移和current/previous协议身份；不增加工作区数据库schema。完整合同见 [教材生成](textbook-generation.md)。
