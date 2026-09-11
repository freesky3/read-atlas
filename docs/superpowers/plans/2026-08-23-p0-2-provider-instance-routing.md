# Read Desktop P0-2：Provider 实例级安全路由与任务恢复实施计划

> 状态：已获方向确认，待按本计划实施
> 日期：2026-08-23
> 目标分支：codex/optimize-read-desktop-p0
> 范围：仅 read_desktop；不得修改相邻 read_addon
> 实施纪律：测试先行；每个独立关注点一个可回滚的小提交；每个任务完成后更新本文勾选项与 Change Log。

## 1. 目标

把论文 Provider 的执行身份从“Provider 类型”提升为“不可变的 Provider 实例路由”，保证后台 Job、即时讨论、Context Root、Provider Node、usage receipt 与远端清理始终使用同一个精确实例、冻结端点和冻结模型。任何无法确认归属的任务都停止并进入任务中心引导，绝不从 Key Map 任取凭据。

本计划同时修复 Gemini Proxy 自定义 Base URL 没有进入 durable Job 快照、重启后回退 localhost:8045 的问题。

## 2. 当前代码事实与根因

- model_settings.rs 的 ProviderInstance 已有稳定 UUID；ReadyPaperProvider 也已携带 instance_id、kind、模型、API key 和解析后的 Base URL。
- lib.rs 的 resolve_provider_key(instance_id) 已能按 UUID 精确取凭据；根因是 durable route 把 UUID 降成了 kind。
- PaperJobRouting 只写 provider kind；Base URL 只对 openai_compatible 落盘，gemini_proxy 被遗漏。
- resolve_job_paper_key 错把 kind 当 instance ID，失败后取 provider_keys 第一项。HashMap 顺序不稳定，可能串账号或端点。
- root_key、dedupe_key、context_roots、provider_nodes 与 remote_tombstones 只含 kind/model；即使 Job 取对 Key，也可能复用或删除另一个同 kind 实例的远端资源。
- invalidate_paper_probe_slot 接受 kind 或 id，会命中第一个同 kind 实例，可能清错探针。
- Reading Artifact 同时使用 `paper_model` 与 `translation_model`；单个 `operation_model` 快照不足以重建 durable 请求。
- `ReadyPaperProvider` 当前可 `Debug` 且含 API key；沿用派生实现存在调试输出泄密风险。
- worker claim 后仍会反复读取全局 active runtime；remote cleanup、legacy reconcile 和失败清理也有同类风险。它属于 P0-3，但这是本计划启用生产路由前的真实依赖。

因此，本项不能只给 payload 增加 providerInstanceId；那只能修一条凭据路径，不能修复 Context Root、远端所有权与 dedupe 碰撞。

## 3. 前置门禁与已锁定的不变量

### 3.1 启用门禁

- P0-1 完成后记录 `npm test`、`npm run build`、`cargo test --locked` 与 `cargo check --locked` 基线。若 P0-1 尚未完成，只推进不依赖其不稳定面的纯 Rust 模块/迁移测试，不得把已知基线失败误记为本项回归。
- P0-3 不并入 P0-2 的代码提交，但以下最小切片必须先完成：
  - `spawn_job_workers`、`spawn_remote_cleanup_worker`、legacy reconcile、Job 失败后的 staging/tombstone cleanup 都接收并持有同一 `Arc<WorkspaceRuntime>`；
  - claim、HTTP、usage、tombstone 与 attempt 收尾不得重读全局 active root/module；
  - 快速切 Workspace 的测试覆盖领取、执行、回写和清理。
- schema 7 migration 可先实现并用 fixture 测试，但生产 `SQLITE_SCHEMA_VERSION` 继续为 6。
- 只有 P0-3 门禁通过、所有 Job/Context/usage/tombstone 写入路径准备完毕后，才在一个原子 activation 提交里同时提升 schema 版本、启用 migration/reconcile、切生产路径并启动 workers。
- 禁止出现“schema 已是 7，但仍有生产路径写 NULL route”的可运行提交。activation 提交可以大于普通小提交；它是不可再拆的安全边界。

### 3.2 身份与行为不变量

1. 新 AI 请求绑定精确 Provider 实例 UUID；kind 只用于展示、兼容统计和暂时保守的同 kind 并发配额。
2. 捕获时冻结完整模型集合；修改当前 Provider、显示名、模型或 Base URL 不能改变既有 Job。
3. API key 只存在于 Credential Manager、既有受控缓存和短生命周期 adapter；SQLite、payload、IPC、日志、诊断不得出现明文 Key。
4. 严禁 kind fallback、current-provider fallback、`HashMap` first-entry fallback、未知快照回退 Gemini。
5. “账号/端点身份”和“执行路由身份”分离：
   - endpoint scope 包含实例 UUID、kind、规范端点和精确 Key 的摘要；
   - route ID 由 endpoint scope 与完整冻结模型集合派生。
6. 同一实例换 Key 后视为新 endpoint scope；旧 Job 与远端删除都 fail closed，只有恢复原 Key才可重检原身份。
7. endpoint scope、route ID 与 Key 摘要都是 secret-derived 内部元数据，不进入前端、日志或诊断。
8. 新 scoped 且 `provider_committed=true` 的 Job 不允许改绑；只能重检原 route、取消或从原功能入口重新生成。
9. legacy committed/interrupted Job 没有原实例证据，不得声称能恢复原凭据，不得改绑或自动继续。
10. 未 committed Job 仅允许显式绑定同 kind、完整冻结模型完全匹配且当前 ready 的实例。
11. Job dedupe、Context Root、Provider Node 与 usage 按 route ID 隔离；tombstone 按 endpoint scope 隔离。
12. 旧 Context/Node 没有历史实例证据，不得被新 route 复用；本地消息与成果保留。
13. legacy tombstone 不自动归属，也不允许普通人工猜选实例后联网删除；只能永久隔离或显式放弃自动清理。
14. `jobs.provider` 继续保存 kind；“按 kind 最多 2 个并发”本项不改，且不参与身份安全。
15. endpoint/route snapshot 及其外键只能通过 `provider_routing` 的单一持久化 seam 写入。
16. 不新增依赖；复用 `sha2`、`wiremock`、`rusqlite` 与 `tempfile`。
17. 不改变 Artifact、Outline、Guide 的全局 current-head 语义。
18. 不修改 `read_addon`。

## 4. 计划如何迭代

- 可直接调整：内部类型名、函数签名、测试夹具、迁移 SQL 的机械细节。
- 可在前序任务完成后调整：尚未开始的任务拆分、文件归属与提交边界；必须写入 Change Log。
- 不可直接调整：第 3 节不变量、用户可见恢复语义、存储兼容原则和 committed 改绑规则。
- 新高风险问题先补风险表与红测试；超出 P0-2 的内容移出范围。
- 任一 Task 的 Done 条件未满足，不得删除兼容 helper。

## 5. 用户可见行为

| 场景 | 当前行为 | 目标行为 |
| --- | --- | --- |
| 两个同 kind 实例 | 可能任取 Key/端点 | 始终使用入队时实例 |
| Gemini Proxy 自定义 URL 重启 | 可能回退 localhost:8045 | 使用冻结 URL |
| 旧 Job 有多个候选 | 可能错误执行/失败 | 任务中心显示“需要处理”并要求选择 |
| 目标 Key 缺失或变化 | 可能取其他 Key | 不发请求，显示恢复/改绑引导 |
| committed 后实例不可用 | 可能错误重试 | 禁止改绑，只允许恢复、取消或重生成 |
| 切换同 kind 实例继续讨论 | 可能复用旧远端节点 | 保留本地历史，建立新 route 远端上下文 |
| 旧 Context Root | 可能复用 | 首次新请求重新建根 |
| 任务中心 | 只显示 kind/paused | 显示 kind + 实例名与“需要处理”卡片 |

后两项可能增加一次上传、Token 或首次等待时间；这是隔离旧未知上下文的预期代价。

## 6. Design-it-twice 结论

| 方案 | 优点 | 缺点 | 结论 |
| --- | --- | --- | --- |
| payload 最小补丁 | diff 小 | Job 以外仍分散，容易半修 | 不采用 |
| 最大 Driver Registry | OAuth/driver 扩展强 | P0 引入 CredentialRef、quota lane 等过度抽象 | 暂不采用 |
| 调用方优先深模块 | 隐藏 Key、规范化、adapter 与 legacy 规则 | 需要 schema 7 和全链路传播 | 采用 |

新模块为 `src-tauri/src/provider_routing.rs`，位于 Workspace/Job/Context 与外部 Provider adapter 之间。调用方只表达“选择哪个实例、冻结哪些模型”；模块内部完成精确凭据读取、两级身份计算、adapter 构造、持久化与 legacy 判定。

复核后对初稿做四项收紧：

- `capture` 返回可立即调用的 opaque bound handle，避免即时请求再次读取 Key。
- endpoint scope 与 route ID 分层，避免 tombstone 被模型维度重复或串用。
- durable route 冻结完整模型集合，不假设每个任务只有一个模型。
- migration 先休眠，P0-3 过门后再原子启用，避免半迁移状态。

## 7. 目标架构与接口

### 7.1 数据流

    ProviderSelection + FrozenModels
                  |
                  v
       ProviderRouting.capture
                  |
          BoundProviderRoute
          /                 \
         v                   v
    immediate request     freeze()
                              |
                    persist_frozen_route(tx)
                              |
                       Job / Context / usage
                              |
                         restart / worker
                              |
                    ProviderRouting.bind
                       /              \
                      v                v
             opaque adapter     ProviderRequirement
                                      |
                              paused + Task Center

调用方不会接触 API key，也不能自行计算 endpoint scope、route ID 或拼装 snapshot。

### 7.2 核心类型草图

```rust
enum ProviderSelection {
    Current,
    Instance(ProviderInstanceId),
}

enum ModelRole {
    Paper,
    Translation,
}

struct FrozenModels {
    version: u32,
    paper: String,
    translation: Option<String>,
}

struct ProviderEndpointSnapshot {
    version: u32,
    endpoint_scope: EndpointScope,
    provider_instance_id: ProviderInstanceId,
    provider_name_at_capture: String,
    provider_kind: String,
    base_url: Option<NormalizedBaseUrl>,
}

enum RemoteEndpointOwner {
    PaperProvider(ProviderEndpointSnapshot),
    MistralOcrSystem {
        endpoint_scope: EndpointScope,
    },
}

struct FrozenProviderRoute {
    endpoint: ProviderEndpointSnapshot,
    route_id: ProviderRouteId,
    models: FrozenModels,
    operation: ModelRole,
}

struct BoundProviderRoute {
    frozen: FrozenProviderRoute,
    adapter: Arc<dyn PaperModelPort>,
}

enum ProviderRouteDecision<T> {
    Ready(T),
    ActionRequired(ProviderRequirement),
}
```

`BoundProviderRoute` 字段对模块外私有，只暴露领域调用、安全 identity 访问器与 `freeze()`。它不派生 `Serialize`，`Debug` 必须脱敏。

### 7.3 三条主流程

```rust
capture(
    selection: ProviderSelection,
    models: FrozenModels,
) -> ProviderRouteDecision<BoundProviderRoute>

bind(
    frozen: &FrozenProviderRoute,
) -> ProviderRouteDecision<BoundProviderRoute>

adopt_legacy(
    facts: LegacyProviderFacts,
    explicit_instance: Option<ProviderInstanceId>,
) -> ProviderRouteDecision<FrozenProviderRoute>
```

- `capture` 通过内部 `ProviderCredentialPort` 按 UUID 精确读取一次 Key，用同一 Key 计算 endpoint scope 并构造 adapter，然后尽快释放中间 secret。
- 即时请求直接使用 bound handle，保证身份计算和实际请求使用同一凭据。
- Job 只持久化 `freeze()` 的安全 snapshot；worker 重启后用 `bind` 精确重建。
- `adopt_legacy` 只执行第 9 节的证据规则，不调用 current/fallback。

### 7.4 两级身份

endpoint scope：

```text
sha256(
  canonical_length_prefixed(
    "read-desktop/provider-endpoint/v1",
    instance_id,
    provider_kind,
    normalized_base_url_or_fixed_marker,
    sha256(exact_api_key)
  )
)
```

route ID：

```text
sha256(
  canonical_length_prefixed(
    "read-desktop/provider-route/v1",
    endpoint_scope,
    canonical_frozen_models_json,
    operation_role
  )
)
```

- 使用长度前缀编码或版本化 canonical JSON，禁止冒号拼接。
- endpoint scope 不含显示名；重命名不改身份。
- route ID 包含完整 `FrozenModels` 与独立 `operation_role`；Reading Artifact 的 paper/translation 任一变化或操作角色变化都会形成新 route。
- Job identity 是 `(route_id, logical_dedupe_key)`，不把 scope 拼进公共字符串。
- Context/Node/usage 引用 route ID；tombstone 引用 endpoint scope。
- identity 使用 newtype；调用方不得 `strip_prefix` 或手工重算。

### 7.5 Base URL 合同

新 route 只能从已通过当前探针的 ready 实例捕获：

- 仅允许 `http` / `https`。
- 禁止 username、password、query、fragment。
- scheme/host、默认端口和尾斜杠使用唯一 canonical 规则。
- 保留有语义的路径，例如 `/v1`。
- 保持现有本地 HTTP 兼容，不在 P0 禁止 Proxy localhost。
- Gemini/Grok 使用固定 endpoint marker；OpenAI-compatible/Gemini Proxy 保存已验证的规范 URL。
- 任务中心只显示安全 label/host，不返回完整 URL 或路径。

legacy OpenAI URL 只有与候选当前已验证 URL完全一致才可自动归属；Proxy 历史缺 URL 仅在同 kind 唯一 ready 候选时采用其当前 URL。

### 7.6 凭据、日志与比较

- `ProviderCredentialPort` 只支持按实例 UUID 读取，不提供按 kind 列表或“第一项”接口。
- bind 用当前 exact Key 重算 endpoint scope，并以固定长度、无早退比较验证；不匹配即 requirement。
- `ReadyPaperProvider` 删除派生 `Debug` 或实现安全字段的脱敏 Debug。
- `BoundProviderRoute`、credential adapter、`PaperAdapter` 不派生 `Serialize`。
- route/error 的 `Display` 不得包含 Key、Authorization、完整 endpoint、userinfo/query、scope 或 route ID。
- `last_error` 只保存安全文案；恢复逻辑只依赖 typed code。
- 数据库持久化使用显式参数绑定，不通过通用 route JSON serialize。

### 7.7 单一持久化 owner

```rust
persist_frozen_route(
    tx: &Transaction,
    route: &FrozenProviderRoute,
) -> Result<PersistedProviderRouteIds>
```

它在一个事务内幂等 upsert paper endpoint 与 route snapshot。Mistral OCR 通过同模块的 `persist_mistral_endpoint` 写入通用 remote endpoint 表；两条路径共享 canonical scope 与外键规则。只有以下边界可调用：

- `JobModule::enqueue`；
- Discussion/Context publish transaction；
- Artifact/Orientation/Roadmap/Guide/Outline publish transaction；
- usage/remote resource 登记 transaction。

所有外键用 `ON DELETE RESTRICT`；snapshot 被引用时不可删除，P0 不做 snapshot GC。

### 7.8 Job 内部合同

```rust
enum JobExecutionRoute {
    Local,
    MistralOcr,
    Paper(FrozenProviderRoute),
}
```

使用枚举而不是 `Option<FrozenProviderRoute>`，避免新增 Paper Job 忘传 route 却合法写 NULL。

- `JobRecord` 是唯一内部 row model，包含 raw payload、route、模型、requirement 与 origin；worker/executor 只接收 JobRecord。持久层 route 判别使用 `StoredJobRoute = Executable(JobExecutionRoute) | LegacyUnattributed`，禁止仅凭 NULL route 猜成 Local 或 Mistral。
- `JobProjection` 只含逻辑 key、必要业务字段、安全 route summary 和 typed requirement。
- `get/list/list_active/claim_next` 复用一个 row mapper，再显式投影。
- `list_jobs` 不访问 Keyring；readiness 只在 capture/reconcile/bind/recheck 时检查。
- 生成/远端继续必须使用 Bound/Frozen route；纯读取已有 head 使用非秘密 selection，不因 Key 缺失阻断本地成果显示。

### 7.9 paused + requirement

不新增 JobState；继续使用 `paused` + `job_provider_requirements`。`JobModule::resume` 内部强制 requirement guard。

运行中发现 route 不可绑定时，一个事务内：

1. 关闭当前 attempt；
2. `running -> paused`；
3. `stage -> provider_action_required`；
4. 写 typed requirement；
5. 保留 `provider_committed` 与 checkpoint；
6. HTTP=0。

rebind 仅允许 `paused + uncommitted`、同 kind、完整冻结模型完全匹配且 readiness 有效。模型不匹配时取消并从原入口新建。

rebind 前先检测目标 route active dedupe。冲突返回 `AlreadyActiveOnRoute { existing_job_id }`，原 paused Job完全不变；禁止隐式取消、覆盖或合并。

## 8. Workspace schema 7

### 8.1 新表

```sql
CREATE TABLE remote_endpoint_snapshots (
  endpoint_scope TEXT PRIMARY KEY,
  version INTEGER NOT NULL,
  owner_type TEXT NOT NULL
    CHECK (owner_type IN ('paper_provider', 'mistral_ocr')),
  provider_instance_id TEXT,
  provider_name_at_capture TEXT,
  provider_kind TEXT,
  base_url TEXT,
  created_at TEXT NOT NULL,
  CHECK (
    (owner_type = 'paper_provider'
      AND provider_instance_id IS NOT NULL
      AND provider_kind IS NOT NULL)
    OR
    (owner_type = 'mistral_ocr'
      AND provider_instance_id IS NULL
      AND provider_kind IS NULL)
  )
);

CREATE TABLE provider_route_snapshots (
  route_id TEXT PRIMARY KEY,
  endpoint_scope TEXT NOT NULL
    REFERENCES remote_endpoint_snapshots(endpoint_scope) ON DELETE RESTRICT,
  version INTEGER NOT NULL,
  models_json TEXT NOT NULL,
  operation_role TEXT NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE(endpoint_scope, models_json, operation_role)
);

CREATE TABLE job_provider_requirements (
  job_id TEXT PRIMARY KEY REFERENCES jobs(id) ON DELETE CASCADE,
  code TEXT NOT NULL,
  provider_kind TEXT,
  provider_instance_id TEXT,
  can_rebind INTEGER NOT NULL CHECK (can_rebind IN (0, 1)),
  safe_details_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
```

`models_json` 是版本化 canonical JSON，只含模型名/角色。`safe_details_json` 只允许非敏感枚举和实体 ID，不保存 Key、digest、scope、route ID 或完整 URL。

### 8.2 现有表与索引

- `jobs`：增加 nullable `provider_route_id`、`provider_route_origin`；保留 `provider` kind 与现有 `dedupe_key` 公共语义。
- `context_roots`：增加 nullable `provider_route_id`；新 route 的 `context_epoch` 使用版本化 `semantic_epoch + route_id` 派生。
- `provider_nodes`、`usage_receipts`：增加 nullable `provider_route_id`。
- `remote_tombstones`：增加 nullable `endpoint_scope` 与 `ownership_status`（`exact / legacy_unattributed / abandoned`）。
- 所有非空 route/endpoint 列引用 snapshot 并 `ON DELETE RESTRICT`。

至少新增/替换：

```sql
CREATE UNIQUE INDEX jobs_active_route_dedupe
ON jobs(provider_route_id, dedupe_key)
WHERE provider_route_id IS NOT NULL
  AND state IN ('queued', 'running', 'paused', 'interrupted_unknown');

CREATE UNIQUE INDEX jobs_active_legacy_dedupe
ON jobs(dedupe_key)
WHERE provider_route_id IS NULL
  AND state IN ('queued', 'running', 'paused');

CREATE INDEX jobs_legacy_interrupted_dedupe
ON jobs(dedupe_key)
WHERE provider_route_id IS NULL
  AND state = 'interrupted_unknown';

CREATE INDEX jobs_route_state_updated
ON jobs(provider_route_id, state, updated_at);

CREATE INDEX context_roots_route_lookup
ON context_roots(revision_id, provider_route_id, model, context_epoch);

CREATE INDEX provider_nodes_route_remote
ON provider_nodes(provider_route_id, provider_node_id);

CREATE UNIQUE INDEX remote_tombstones_exact_unique
ON remote_tombstones(endpoint_scope, resource_kind, remote_id)
WHERE endpoint_scope IS NOT NULL;

CREATE INDEX remote_tombstones_route_state
ON remote_tombstones(endpoint_scope, state, updated_at);
```

现有仅 `dedupe_key` 的 active unique index必须被替换，否则跨 route 仍会错误 coalesce。v6 合法允许同一 dedupe 同时存在 `interrupted_unknown` 与 queued/running/paused，因此 legacy unique 不能在 migration 时纳入 interrupted；v7 enqueue/rebind/abandon 必须在事务中通过非唯一 lookup 显式把 interrupted 当冲突，并只在显式 abandon/cancel 后释放。

### 8.3 只重建 remote_tombstones

- `context_roots` 被 `discussions.context_root_id` 引用，P0 不重建。
- 新 scoped root 的 `context_epoch` 纳入 route ID，从而绕开旧表级 unique；查询仍强制 route ID 相等。测试证明两个同 revision/kind/model、不同 route 的 root 可并存且 Discussion FK 完整。
- `remote_tombstones` 的旧 table-level unique 必须 create-copy-drop-rename 真正移除。
- legacy tombstone 迁为 `legacy_unattributed`，`attempts`、`last_error`、创建/更新时间和 lifecycle state 原样保留。
- Mistral 不伪造 Provider UUID：`owner_type='mistral_ocr'`，scope 由固定 Mistral endpoint marker 与创建资源时的精确 Mistral credential 摘要派生。Key 变化同样 fail closed，不依赖 NULL unique。

### 8.4 一致备份、事务与降级

不新增 rusqlite backup feature，也不复制活跃 WAL/SHM：

1. 取得 Workspace 生命周期独占锁，停止 workers/cleanup 并释放该 Workspace 的其他连接。
2. `wal_checkpoint(TRUNCATE)`；失败则中止。
3. 关闭全部连接，用标准库复制 main DB 为 pre-schema7 backup。
4. 只读打开备份并 `quick_check`；失败则中止。
5. 重开原 DB，用单独 `BEGIN IMMEDIATE` 完成新表、增列、tombstone rebuild、索引和结构回填。
6. 最后才同时更新 `schema_meta` 与 `PRAGMA user_version=7` 并 commit。
7. commit 后 `foreign_key_check` / `quick_check`；失败则停止启动并引导恢复备份，绝不启动 worker。
8. migration 幂等；已完成 v7 不重复备份。
9. activation 前把 `initialize_database` 改为单调版本推进：禁止无条件回写较低 `user_version`；exact v7 只验证并返回，meta/user_version 混态 fail closed。

不得在事务开始后切 `PRAGMA foreign_keys=OFF`。本计划仅重建没有入向 FK 的 tombstone 表，原则上无需关闭外键。

旧二进制可能把 v7 当未来数据库并显示新空库；这不是成功降级。发布回滚只能恢复 pre-v7 backup 或向前修复，不能直接安装旧二进制继续写 v7。验收必须包含关键 DDL 故障注入、备份存在性与恢复演练。

## 9. Legacy 规则

### 9.1 启动顺序

每个 Workspace runtime 固定执行：

1. 结构 migration；
2. `JobModule::recover_interrupted`；
3. legacy active Job 语义分类/回填；
4. 断言每个 active AI Job 要么有 ready route，要么已 paused + requirement；
5. 启动该 runtime 的 Job worker 与 remote cleanup worker。

### 9.2 自动归属条件

legacy uncommitted Job 只有同时满足以下条件才可自动归属：

- 能解析明确 kind 和原始完整 `FrozenModels`；
- 同 kind 恰好一个 ready 实例；
- 实例存在、kind 未变、凭据存在；
- 原始 paper/translation 模型与候选对应角色完全一致且 readiness 有效；
- 旧 Job 有 URL 时，与候选已验证规范 URL完全一致；
- Proxy 历史缺 URL 时，候选仍为同 kind 唯一 ready 实例；
- `provider_committed=false`；
- scoped dedupe 不与已有 active Job 冲突。

不得以候选当前模型覆盖旧模型。任何条件不满足：`paused + provider_action_required`，HTTP=0。

明确删除：缺 provider 默认 Gemini、使用 current Provider、按 kind 找 Key、Map 任取 Key、用新模型覆盖旧任务。

### 9.3 Job 分类

| 情况 | 处理 |
| --- | --- |
| completed/failed/cancelled | 保留历史，NULL route；不参与新 Context/cleanup |
| queued/paused、未 committed、唯一严格匹配 | 自动归属，`origin=legacy_unique_verified` |
| 0 或 2+ 候选 | requirement；允许选择严格匹配实例 |
| provider/model 缺失或冲突 | invalid/legacy_unattributed；取消并重新生成 |
| URL 不一致 | endpoint_mismatch；不发请求 |
| legacy committed 或 legacy interrupted_unknown | 永久 quarantine；不 rebind、不 claim |
| future/corrupt snapshot | invalid_snapshot；不回退 |

新 scoped committed Job 若只是凭据不可用，可在恢复原 Key 后重检 route；但 `interrupted_unknown` 还必须按请求是否可能已发出分流：

- scoped + `provider_committed=false`：证明尚未提交外部请求；原 route 重检成功后可安全回到 queued，未 committed 规则仍允许严格 rebind。
- scoped + `provider_committed=true`：即使 route 当前 ready，也不得自动重试；保持 interrupted/quarantine，防止重复计费。只允许保留未决、显式放弃本地任务，或回原入口重新生成并二次提示。
- legacy interrupted/committed：没有 endpoint scope，不能证明原 Key；采用同一 quarantine 文案，不提供 recheck/rebind。
- `interrupted_unknown` 继续计入 active dedupe；只有显式 abandon/cancel 完成后才释放 dedupe。abandon 只做本地状态转换，不触发未知远端 cleanup。

### 9.4 Context/Node

- NULL-route root/node 只保留历史，不参与新查询。
- 本地 Discussion 消息、引用与成果保留。
- 新 route 首次请求建立新 root，只使用本地可验证历史，不发送旧 `provider_node_id`。
- 查询、lock、root、parent resume、compaction 与 receipt 全部要求相同 route，不回退 `provider + model`。

### 9.5 tombstone

- 新 tombstone 总是持久化 exact endpoint scope。
- Worker 只领取 `ownership_status='exact'` 且身份完整的记录。
- bind 按原实例取 Key 并重算 scope；实例/Key/kind/version 不匹配时 action_required，DELETE HTTP=0。
- legacy tombstone 即使同 kind 唯一也不自动 DELETE。
- P0 不提供“选一个 Provider 后删除”，因为 remote ID 碰撞仍可能误删。
- 用户可“放弃自动清理”，标记 `abandoned` 并提示远端资源可能仍存在；该动作无网络请求。
- cleanup 的集合键、UNION 和 `mark_remote_resource_deleted` UPDATE 都必须带 endpoint scope。
- 同 endpoint、不同模型的同 remote ID 只产生一个 tombstone。
- 本项不实现 cleanup singleflight/批量/并发优化。

## 10. 任务中心合同

### 10.1 安全投影

```ts
providerRoute: {
  instanceId: string | null;
  instanceName: string | null;
  kind: ProviderKind | null;
  models: {
    paper: string | null;
    translation: string | null;
    operation: "paper" | "translation" | null;
  };
  endpointLabel: string | null;
  routeStatus: "ready" | "legacy" | "action_required";
} | null;

providerRequirement: {
  code: ProviderRequirementCode;
  providerKind: ProviderKind | null;
  providerInstanceId: string | null;
  canRebind: boolean;
  providerCommitted: boolean;
} | null;
```

routeStatus 只表达持久 snapshot/requirement 状态；`list_jobs` 不逐行访问 Keyring。credential readiness 通过 persisted requirement 和显式 recheck 更新。

不投影 endpoint scope、route ID、Key/摘要、probe fingerprint、credential locator、完整 URL/路径/query 或 snapshot 原文。RemoteTombstoneProjection 只显示安全实例摘要、ownership status 与风险文案。

### 10.2 分状态文案与 CTA

uncommitted：

> 为避免使用错误账号，本任务尚未向不确定的 Provider 发出请求。

committed/scoped：

> 请求可能已经发出。只能恢复创建任务时的 Provider 凭据后继续；为避免重复请求或错误计费，不能改绑。

legacy committed/interrupted：

> 无法确认当时使用的 Provider，且请求可能已发出或产生费用。可保留未决、放弃本地任务，或从原功能入口重新生成。

交互：

- requirement 显示琥珀色“需要处理”，隐藏 Resume/Priority。
- uncommitted 只列同 kind 且完整模型严格匹配的实例，展示名称、kind、模型、安全 endpoint label。
- bind 前二次确认；后端再次验证。
- credential missing 提供“打开模型设置”并聚焦精确实例、“重新检查原 Provider”。
- `AlreadyActiveOnRoute` 聚焦已有任务，原 paused Job不变。
- committed 不显示选择器。
- legacy tombstone 只提供“放弃自动清理”与风险确认，不提供联网认领/删除。
- 候选过期或实例删除时保留 requirement，不自动跳第一项。

### 10.3 IPC

```ts
resolveJobProviderRequirement({
  jobId,
  action: "recheck" | "bind",
  providerInstanceId?: string
})

abandonLegacyProviderJob({
  jobId,
  confirmedPotentialCharge: true
})

abandonRemoteCleanup({
  tombstoneId,
  confirmedRemoteMayRemain: true
})
```

后端校验 state、requirement、committed guard、kind、完整模型、readiness、endpoint、checkpoint 与 dedupe，不信任前端候选。

### 10.4 设置聚焦

- `App` 保存一次性 provider focus request。
- `SettingsWorkbench` 接受 `initialProviderId/focusNonce` 并切换 models。
- `SettingsModelsPage` 仅在 nonce 变化且实例存在时更新 `activeInstanceId`，不得在 render 覆盖手动选择。
- 实例已删除时明确提示，不跳列表第一项。

## 11. 详细实施任务

### Task 0：ADR、基线与调用点清单

Files:

- `docs/decisions.md`
- `docs/data-protocols.md`
- 本计划
- 只读扫描 `src-tauri/src/lib.rs`、`job_module.rs` 与相关 modules

Commit:

    docs: lock provider route safety invariants

- [x] 锁定两级身份、完整模型、fail closed、paused + requirement、legacy tombstone quarantine。
- [x] 列全六类 Job、Discussion/Lens、Context、usage、probe、tombstone 与 worker 启动点。
- [x] `rg` 登记 `keys.iter().next`、kind-as-instance、unknown-to-Gemini、kind-first probe fallback。
- [x] 记录当前 npm/cargo 基线；P0-1 尚无实现提交，但现有全量基线为绿，既有 warning 单列。
- [x] 新发现写入 Change Log，不静默扩范围。

Baseline（2026-08-24）：`npm test` 41 files / 186 tests passed（6.87s）；`cargo test --locked` 205 passed（总构建 1m44s，测试 11.57s）；`npm run build` passed（dist 4,140,688 bytes）；`cargo check --locked` passed。既有 warning：`roadmap_module::clear_progress` dead code；lib test 另有 `app_state_fits_comfortably_on_a_small_stack_frame` dead code。

### Task 1：纯 provider_routing 深模块

Files:

- Create `src-tauri/src/provider_routing.rs`
- Modify `src-tauri/src/lib.rs`（仅 module 注册/机械移动）
- Modify `src-tauri/src/model_settings.rs`（最小 credential/readiness port、脱敏 Debug）
- Tests in `provider_routing.rs`

Commit:

    feat(provider): add frozen instance route module

Tests:

- [x] 四种 kind endpoint，尤其 Proxy 自定义 URL。
- [x] 拒绝非 http(s)、userinfo/query/fragment；保留 `/v1` 与 localhost HTTP。
- [x] 不同 UUID/Key/endpoint -> 不同 endpoint/route；rename 不改；任一模型变化 -> route 变化。
- [x] canonical 编码无字段边界碰撞。
- [x] capture 与 adapter 使用同一 exact Key。
- [x] 目标 Key 缺失但缓存有其他 Key -> requirement，HTTP=0。
- [x] Key/instance/kind/version 变化 -> typed requirement。
- [x] Debug/Display/Serialize 不含 sentinel Key、scope、route 或 secret URL。

完成证据（2026-08-24）：`cargo test --locked provider_routing` 11/11 通过；`cargo check --locked --lib` 通过；`provider_routing.rs` 单文件 `rustfmt --check` 与三文件 `git diff --check` 通过。提交前复核补上 `FrozenModels.version` fail-closed、capture 模型与 ready 实例一致性、三层版本/损坏快照、localhost/root/trailing slash 及完整模型 identity 矩阵。

Verify:

    cd src-tauri
    cargo test --locked provider_routing
    cargo check --locked
    cargo fmt --all -- --check

### Task 2：schema 7 migration（先休眠）

Files:

- `src-tauri/src/v2_workspace.rs`
- `src-tauri/src/db.rs`（一致备份 helper）
- Tests in `v2_workspace.rs`

Commit:

    feat(db): prepare provider route schema migration

- [x] 实现 `migrate_v6_to_v7`，仅由 fixture 显式调用。
- [x] 本 Task 不修改生产 `SQLITE_SCHEMA_VERSION=6`，不改用户 Workspace。
- [x] v6 fixture 含 Job/root/node/receipt/Discussion FK/tombstone，迁移后全保留；覆盖同 dedupe 的 legacy `interrupted_unknown + queued`。
- [x] context_roots 不重建；两个 route root 可并存。
- [x] tombstone 真正移除旧 unique；legacy attempts/error/timestamps 原样保留。
- [x] 所有 FK RESTRICT；索引/query plan 正确。
- [x] 每个关键 DDL 故障注入后结构/版本仍为 v6。
- [x] checkpoint-close-copy backup 可打开、恢复；幂等与检查通过；v7 重开不得回写 4/6，meta/user_version 混态 fail closed。

### Task 3：JobModule route/requirement 基础（生产未启用）

Files:

- `src-tauri/src/job_module.rs`
- `src-tauri/src/job_commands.rs`
- Tests in `job_module.rs`

Commit:

    feat(jobs): add route-aware records and requirements

- [x] 新增 `JobExecutionRoute`、`StoredJobRoute::LegacyUnattributed`、`JobRecord` 与安全 `JobProjection`。
- [x] 新增独立 v7 JobRecord row mapper 与 route-aware enqueue/claim API；本 Task 不替换现有 v6 production mapper、`claim_next` 或 executor。
- [x] 实现 route-aware enqueue、persist route、复合 dedupe；origin 固定为 `captured / legacy_unique_verified / explicit_rebind`。
- [x] 实现 block/clear/recheck/rebind/abandon 事务。
- [x] `resume` 内部 requirement guard；list 不访问 Keyring。
- [x] 显式 v7 fixture 测试；现有生产 commands、row mapper、claim/executor 全部保持 v6 路径，统一切换只能发生在 Task 8。
- [x] rebind conflict 返回 `AlreadyActiveOnRoute`，原 Job byte-for-byte 不变。
- [x] running -> paused 原子关闭 attempt；unknown route abandon 不触发 cleanup；覆盖 scoped interrupted 的 committed/uncommitted 分流与 active dedupe 释放时机。

### Task 4：P0-3 hard gate

P0-3 独立实施；P0-2 只消费稳定 runtime 接口。

- [x] Job worker、remote cleanup、legacy reconcile、失败清理持有同一 runtime。
- [x] 快速切 Workspace 测试覆盖 claim、HTTP、DB 回写与 tombstone。
- [x] 相关路径不再读取 `active_job_module` / `active_workspace_root`。
- [x] 未通过则停止，不启用 Tasks 5–8。

### Task 5：六类 durable Job 接线准备

Owner: `src-tauri/src/lib.rs` 由主 agent 独占。

覆盖 Reading Artifact、Reading Guide、Outline Overview、Outline Deep Dive、Orientation Pack、Reading Roadmap 的 enqueue/worker。

- [x] 六类分别冻结完整 models/route/dedupe。
- [x] 两同 kind 实例不 coalesce；切 current 后旧 Job仍用原实例。
- [x] Proxy URL：enqueue -> restart -> 原 mock。
- [x] 目标 Key 缺失但其他 Key 存在 -> paused，HTTP=0。
- [x] future/corrupt snapshot 不回退。
- [x] `provider_committed` 在首次网络提交前持久化；restart 后 scoped interrupted 按 committed 位分流。
- [x] Reading Artifact 重启仍有 paper + translation 模型。
- [x] payload 只留领域输入；worker 只从 JobRecord bind。
- [x] 暂不删除 cleanup 仍引用的旧 helper。

### Task 6：即时请求、Context 与只读投影准备

Files:

- `reading_artifact_module.rs`
- `outline_module.rs`
- `guide_module.rs`
- `roadmap_module.rs`（若实际调用）
- `discussion_commands.rs`（若实际调用）
- `src-tauri/src/lib.rs`（主 agent）
- 对应 tests

- [x] 两 route 建独立 roots；ensure/lookup/persist/lock 都要求 route。
- [x] parent node 跨 route 拒绝；legacy NULL 不命中。
- [x] Discussion 切 route 保留本地历史、不发送旧 node。
- [x] Lens/compaction/Orientation/Roadmap/Guide/Outline root checks 用 route。
- [x] 只读 head 在 Key 缺失时仍可展示。
- [x] receipt/node 与实际请求使用同一 route。
- [x] Discussion 与 Job 的 probe invalidation 同时改 exact instance。

### Task 7：usage、probe 与 remote cleanup 准备

Files:

- `src-tauri/src/lib.rs`（主 agent）
- `provider_ports.rs`（最小包装）
- `job_module.rs/job_commands.rs`
- 对应 tests

- [x] receipt 带 route但不泄露。
- [x] probe 只失效 exact UUID。
- [x] 两 endpoint 同 remote ID -> 两 tombstone；同 endpoint 不同 model -> 一条。
- [x] retry 只解析原 endpoint；不可用时 DELETE HTTP=0。
- [x] legacy tombstone 无自动/手动联网删除；abandon 只改本地。
- [x] Mistral 使用 `mistral_ocr` owner，不伪造 Provider UUID；Key 变化 fail closed。
- [x] queue/mark SQL 都过滤 endpoint scope。
- [x] cleanup 从 endpoint snapshot 恢复，不按 kind 查 key/url。

### Task 8：原子 activation

Single commit:

    fix(provider): activate exact instance routing end to end

同一提交必须：

- [x] `SQLITE_SCHEMA_VERSION=7`，启用 migration/backup，并把生产 get/list/list_active/claim_next/executor 原子切到统一 v7 JobRecord mapper。
- [x] 切换所有生产 Job、即时 Context、usage、probe、tombstone。
- [x] 启动顺序 migrate -> recover -> reconcile -> invariant assertion -> workers。
- [x] 删除运行路径的 current/kind/first-key fallback。
- [x] v7 后新 Paper Job/Context/Node/receipt/tombstone 不允许 NULL route。
- [x] worker/cleanup 全程使用 P0-3 runtime。
- [x] 双端点、restart、legacy、migration 测试全过。
- [x] 不留可切回不安全旧路径的 runtime flag。

Tasks 5–8 属于一个 activation train：可按文件并行准备，但不能形成半接线生产提交。整组暂存、完整 Rust 验证、全库 `rg` 后一次提交。

### Task 9：任务中心 UI、设置聚焦与安全序列化

Files:

- `src/types.ts`
- `src/desktopClient.ts` / tests
- `src/OperationsDrawer.tsx` / tests
- `src/App.tsx`
- `src/SettingsWorkbench.tsx` / tests
- `src/SettingsModelsPage.tsx`
- scoped `src/styles.css`

Commit:

    feat(operations): guide provider route recovery

- [x] requirement “需要处理”；隐藏 Resume/Priority。
- [x] uncommitted exact bind；committed/legacy committed 无改绑。
- [x] missing credential 精确打开实例/recheck。
- [x] conflict 聚焦已有任务，原任务不变。
- [x] legacy tombstone 只有 abandon + 风险确认。
- [x] focus nonce 不覆盖手动选择。
- [x] memory adapter 不伪造安全绑定。
- [x] DOM/IPC/read-event/diagnostics/error 无 sentinel Key、scope、route、完整 URL。

Verify:

    npx vitest run src/OperationsDrawer.test.tsx src/SettingsWorkbench.test.tsx src/desktopClient.test.ts --maxWorkers=1 --fileParallelism=false
    npm run build

### Task 10：删旧路由、文档与闭环

Files:

- `src-tauri/src/lib.rs`
- `README.md`、`docs/README.md`、`docs/decisions.md`、`docs/data-protocols.md`
- `docs/backend-hardening.md`、`docs/agent-onboarding.md`、`docs/handoff.md`
- 本计划

Commits:

    refactor(provider): remove kind-only routing fallbacks
    docs: document provider route recovery contract

- [x] 删除 `PaperJobRouting`、`snapshotted_job_*`、`resolve_job_paper_key`、`open_job_paper_adapter` 与无引用 fallback。
- [x] 只有 remote cleanup 已切换后才删除其仍调用的 helper。
- [x] `rg` 证明无 first-key、unknown-to-Gemini、kind-as-instance。
- [x] 同步 schema、Projection、Task Center、Proxy、tombstone 与降级合同。
- [x] 将实际验证、量化数据和未做项写回本文。

## 12. 多智能体写入边界

| Owner | 独占写入范围 |
| --- | --- |
| Route agent | `provider_routing.rs`；必要的 `model_settings.rs` 小接口 |
| Data/Job agent | `v2_workspace.rs`、`db.rs`、`job_module.rs`、`job_commands.rs` |
| Frontend agent | types、client、Drawer、Settings、对应 tests/scoped styles |
| Primary agent | `lib.rs`、`workspace_lifecycle.rs`、跨 module activation、最终 docs |

- 禁止两个 agent 同时写同一文件。
- 开始前报告 branch/status/文件清单；结束报告改动文件、测试与风险。
- `lib.rs` 始终由主 agent 独占。
- Tasks 5–8 是同一 activation train；子 agent 不自行启用 schema 或提交半接线状态。
- 文件边界调整先记 Change Log，再重新分配 owner。

## 13. 提交顺序与回滚

1. `docs: lock provider route safety invariants`
2. `feat(provider): add frozen instance route module`
3. `feat(db): prepare provider route schema migration`
4. `feat(jobs): add route-aware records and requirements`
5. P0-3 独立提交先通过 hard gate
6. `fix(provider): activate exact instance routing end to end`
7. `feat(operations): guide provider route recovery`
8. `refactor(provider): remove kind-only routing fallbacks`
9. `docs: document provider route recovery contract`

每个基础提交编译/测试通过，测试与实现同提交，不保留红测试提交。activation 后：

- 可回滚后续 UI/docs/refactor，不能单独回滚某条 route 写入。
- 生产回滚需恢复 pre-v7 backup，或整体回滚 activation 与数据。
- 禁止 `reset --hard`、删除 Workspace 或清库。
- 拿不准是否死代码的 helper 先保留并询问；只有引用扫描与测试共同证明后才删。

## 14. 验证矩阵

### 14.1 自动化

Route/安全：

- exact instance；四种 endpoint；URL canonicalization。
- Key/instance/kind/version/model 变化。
- 完整 paper/translation 模型冻结。
- capture 与 adapter 同一 Key。
- no secret Serialize/Debug/Display/IPC/DOM/diagnostics。
- 无 first-key/current/kind fallback。

Job/restart：

- same route coalesce / cross-route no coalesce。
- 六类 Job、current switch、Proxy URL restart。
- provider/model 缺失不默认 Gemini。
- scoped committed recheck / rebind 拒绝。
- legacy 0/1/2、URL match/conflict、committed quarantine。
- rebind conflict 原 Job不变。
- running -> paused 原子关闭 attempt。
- reconcile 后才启动 worker。

Context/resource：

- root/node/lock/receipt 隔离；legacy 不复用。
- Discussion/Lens switch；纯读取不依赖 Key。
- exact probe invalidation。
- endpoint-scoped tombstone。
- 同 endpoint 不同 model 不重复 tombstone。
- legacy tombstone 无 DELETE；abandon 零网络。
- Mistral `mistral_ocr` owner、无虚构 Provider UUID、Key 变化 fail closed。
- queue/mark 都过滤 endpoint scope。

Schema：

- v6 -> v7 保留。
- context_roots 不重建、Discussion FK 完整。
- tombstone rebuild 移除旧 unique。
- backup/restore drill。
- 每个 DDL fault injection 回滚。
- idempotence、quick/foreign-key check、future version。

Frontend：

- 分状态文案/CTA。
- bind/recheck/settings/abandon/cancel。
- committed guard、safe endpoint、keyboard/focus。
- tombstone 风险确认。
- memory adapter 语义诚实。
- secret sentinel 覆盖 IPC/read-event/diagnostics/error/DOM。

### 14.2 双 mock endpoint E2E

1. 两个同 kind endpoint 分别只接受 Authorization A/B。
2. 相同逻辑任务分别捕获 A/B，切 current 并 restart。
3. Job A 只到 A；Job B 只到 B。
4. 清 A Key、保留 B：Job A requirement，两个 server 请求数不增加。
5. 两 server 返回同 remote ID：两条 endpoint tombstone，各由原 endpoint 删除。
6. 同 endpoint 两模型引用同 remote ID：只一条 tombstone。
7. Proxy 非默认 URL restart 后保持。
8. 快速切 Workspace 后 HTTP/DB 回写仍属于领取 Job 的 runtime。

### 14.3 最终命令

    npm test
    npm run build

    cd src-tauri
    cargo fmt --all -- --check
    cargo test --locked
    cargo check --locked
    cargo clippy --locked --all-targets -- -D warnings

    git diff --check
    git status --short

Tauri 手工：

    npm run tauri dev

检查双实例、Proxy URL、ambiguous、committed、provider switch 后本地历史、任务中心与诊断脱敏。

打包：

    npm run tauri build

若签名、WebView2 或工具链阻塞，报告实际证据；不能用 Tauri build 替代 npm test/build。

只使用 memory adapter、temp SQLite 与 wiremock，不调用真实付费 Provider。live smoke test 必须先说明费用并获同意。

## 15. 性能与体积回归

本项是正确性/安全修复，不宣称性能提升，但要量化回归：

- 固定 1,000 Job + 1,000 Root + 1,000 tombstone fixture，记录 migration、legacy reconcile、list_jobs 五次中位数、DB 字节数。
- list_jobs 留索引/query plan 证据。
- 中位数回归超过 10% 且绝对增加超过 10 ms 时先定位；记录五次原始值。
- 不新增依赖；npm build 记录 bundle 总大小差异。

## 16. 风险

| 风险 | 等级 | 缓解 |
| --- | --- | --- |
| schema 提前启用造成 NULL route | 高 | dormant migration + P0-3 gate + 原子 activation |
| tombstone 误删 | 高 | endpoint scope；legacy 永久隔离；零 fallback |
| 跨 Workspace 执行/清理 | 高 | P0-3 hard gate；全链路同一 runtime |
| migration/降级造成“空库”错觉 | 高 | 一致备份、故障注入、明确恢复合同 |
| 只修 Job 漏 Context/resource | 高 | activation 清单、全库 route 搜索、E2E |
| secret-derived identity 泄露 | 高 | 不 Serialize、脱敏 Debug、sentinel tests |
| Reading Artifact 丢 translation model | 高 | 完整 FrozenModels + restart 测试 |
| Key 轮换停止旧任务/cleanup | 中，预期 | fail closed；恢复原 Key；UI 引导 |
| legacy Context 不复用增加费用 | 中，预期 | 保留本地历史；量化一次性影响 |
| UI 候选过期 | 中 | 后端重验；不自动选第一项 |
| rebind dedupe 冲突 | 中 | typed conflict；原任务不变 |
| snapshot 累积 | 低 | FK RESTRICT；P0 不冒险 GC |

## 17. 不在 P0-2

- P0-3 WorkspaceRuntime 的完整实现；本项只把其稳定 runtime 隔离接口列为生产启用前置门禁。
- 模型设置 schema 3 -> 4 迁移与 duplicate/auto-current Bug。
- PDF page_count NULL / PDF.js 回写。
- 应用内移动/改名/树（P1）。
- deepDiveScope.ts 删除与 Context Inspector 调试入口。
- asset protocol scope 动态安全验证。
- cleanup singleflight/批量/并发优化。
- kind -> instance/quota lane 并发改造。
- 新 driver、OAuth/env credentials。
- read_addon 任何变更。

## 18. 最终验收

- [x] 两个同 kind 实例不串 Key/URL、不合并 Job、不复用 Context、不互删 remote ID。
- [x] Proxy 自定义 URL 在捕获、restart、恢复、cleanup 全链路保持。
- [x] 目标 Key 缺失而其他 Key 存在时 HTTP=0。
- [x] Key 轮换不复用旧 endpoint/route；UI 有诚实恢复路径。
- [x] 完整 paper/translation 模型可从 durable snapshot 重建。
- [x] scoped committed 无法改绑；scoped interrupted 按 committed 位分流；legacy committed 不伪装成可恢复。
- [x] legacy 0/1/2 符合矩阵；legacy tombstone 永不自动或猜测删除。
- [x] schema 6 -> 7 保留、幂等、失败可恢复；不重建 context_roots。
- [x] schema 7 启用后不存在新 unscoped Paper 记录。
- [x] P0-3 runtime hard gate 有测试证据。
- [x] Key/scope/route ID/敏感 URL 不出现在 IPC、DOM、日志、诊断。
- [x] npm test/build 与 Rust test/check/clippy/fmt 全绿。
- [x] Tauri dev 清单完成；Tauri build 完成或有明确环境阻塞。
- [x] README/docs/ADR 与代码一致，量化数据已回填。

## 19. Change Log

| 日期 | 发现/决定 | 影响 |
| --- | --- | --- |
| 2026-08-23 | 比较最小补丁、最大 Registry、调用方优先深模块 | 采用 `provider_routing` 深模块 |
| 2026-08-23 | 即时请求不能在 capture 后二次读取凭据 | capture 返回 opaque bound handle |
| 2026-08-23 | 远端所有权与执行模型维度不同 | 拆分 endpoint scope 与 route ID |
| 2026-08-23 | Reading Artifact 同时使用两个模型 | 冻结完整 `FrozenModels` |
| 2026-08-23 | snapshot 持久化 owner 未闭合 | `persist_frozen_route` 统一 upsert |
| 2026-08-23 | action_required 扩大状态修改面 | 保留 paused + requirement |
| 2026-08-23 | rebind dedupe 冲突不可隐式合并 | `AlreadyActiveOnRoute`，原任务不变 |
| 2026-08-23 | `context_roots` 有 Discussion 入向 FK | 不重建；新 epoch 纳入 route |
| 2026-08-23 | legacy tombstone 猜选实例仍可误删 | 永久隔离或显式放弃 |
| 2026-08-23 | schema 分步启用会制造 NULL route | migration 先休眠，P0-3 后原子 activation |
| 2026-08-24 | Mistral OCR 没有 Provider 实例 UUID | 通用 `remote_endpoint_snapshots` + `mistral_ocr` owner |
| 2026-08-24 | scoped interrupted_unknown 不能只按 route readiness 恢复 | 按 `provider_committed` 分流并保持 active dedupe |
| 2026-08-24 | v7 row mapper 不能在 schema 6 生产路径提前替换 | Task 3 独立休眠 API，Task 8 原子切换 |
| 2026-08-24 | v6 可同时存在同 dedupe 的 interrupted_unknown 与 queued | legacy unique 保持 v6 范围，事务显式检查 interrupted |
| 2026-08-24 | initializer 会无条件回写 user_version=4 | activation 前改为单调版本推进 |
| 2026-08-24 | NULL route 无法区分 local/Mistral/legacy Paper | 增加 `StoredJobRoute::LegacyUnattributed` |
| 2026-08-24 | route origin 字符串未锁定 | 固定 `captured / legacy_unique_verified / explicit_rebind` |
| 2026-08-24 | Task 1 复核发现模型快照版本漏检与 capture 可接受未验证模型 | 显式校验 `FrozenModels.version`；capture 只接受实例当前 paper/translation 选择，Translation 操作必须冻结 translation 模型 |
