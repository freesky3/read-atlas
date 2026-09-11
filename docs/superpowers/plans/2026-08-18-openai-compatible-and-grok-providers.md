# OpenAI-compatible 与 Grok 论文 Provider 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在设置里并存 Gemini、OpenAI-compatible、Grok 三家 LLM；用户显式「设为当前」后，Chat / Orientation Pack / 解释 / Lens / Outline 走当前家的论文模型，翻译走同一家的翻译模型。Mistral OCR 不动。

**Architecture:** 现有 `PaperModelPort` 保持外部 seam。新增 `ChatCompletionsAdapter`（OpenAI-compatible 与 Grok 共用接线，Grok 锁死 Base URL）。`PaperAdapter` 枚举工厂按 **Job/会话快照的 provider** 构造 Gemini 或 Chat Completions，不读「当前」去改进行中的请求。`model-settings.json` 升到 schema 3：`currentProvider` + 每家独立槽（模型、探针指纹）。调用方把 SQL 里写死的 `'gemini'` 改成绑定参数。

**Tech Stack:** Tauri 2, Rust, reqwest, wiremock, Windows Credential Manager, React 19, TypeScript, Vitest.

**Spec:** 本计划即合同（grilling 锁定 16 条 + 已批准的配置段）。实现前读 `docs/decisions.md` D-009 / D-016 / D-017、`docs/data-protocols.md` Provider seam、`docs/agent-onboarding.md`。

## Global Constraints

- Vitest 必须：`npx vitest run --maxWorkers=1 --fileParallelism=false`。
- Tauri 结构体命令：`invoke(name, { request: { ...camelCase } })`（D-019）。
- 密钥只进 Windows Credential Manager 与短暂内存；不进 Workspace / SQLite / 诊断（D-016）。
- 未知 usage 保持 `null`；不编造费用。
- 不自动切换 provider。保存配置 ≠ 设为当前。
- 论文根必须整本进上下文：响应里出现 `attachment_search` / `file_search` 等检索 tool → 探针失败，不得当论文模型。
- OpenAI-compatible 只保证 `/v1/chat/completions`；有 `/v1/files` 则复用 file id。不做 Azure `api-version`、Responses、Assistants、自定义 Header。
- Gemini Files + Interactions 路径不得改成 Chat Completions。
- 非 Gemini 的 Discussion **每轮使用 `recovery_input`（含分支历史）+ 本轮 PDF**，不要依赖 `previous_interaction_id` 会话续接。
- Job payload 必须快照 `provider`；执行时用该 provider 的凭据，不读当时的 `currentProvider`。
- 进行中的流式 Chat / 已入队 Job 不因「设为当前」取消。
- 不要动 `workspace.lock` 的 `File` 持有者。不要借机清 Rust unused 警告。
- 样式只用现有 `--glass-*` / `--liquid-glass-*`。
- 提交前：串行 Vitest、`npx tsc -b`、相关 `cargo test --locked`、`cargo check --locked`、`git diff --check`。

## 锁定的产品合同（执行时不得削弱）

1. 新 API 必须能扛完整论文栈，不是纯文本旁路，也不是只加设置框。
2. D-017 改为授权 OpenAI-compatible / Grok adapter；Gemini 留下。
3. 同一时间一个当前 LLM provider；论文模型与翻译模型必须同一家。
4. 论文根 = 整本进上下文。Grok `attachment_search` 不算。
5. 两张新卡：OpenAI-compatible（Base URL + Key，默认 `https://api.openai.com/v1`）和 Grok（固定 `https://api.x.ai/v1`）。
6. 论文模型三道门：一轮 PDF nonce + 读图（拒绝检索 tool），一轮 `response_format` schema。翻译模型不打这三道门。
7. 保存 ≠ 启用。`set_current_paper_provider` 另点；未就绪钉在当前家，不回退 Gemini。
8. Models 页：顶上当前条 + 三张 LLM 卡 + 底部 Mistral。
9. 「设为当前」要确认；进行中请求走开工快照，新发送走新当前。
10. 探针指纹持久化：`sha256(baseUrl + "\n" + key + "\n" + paperModelId + "\n" + "paper-probe-v1")`。改 URL / Key / 论文模型才作废。
11. 模型 id 可手填。Gemini id 规则不变；兼容口允许 `A-Za-z0-9._:-/`。
12. 每张卡记住自己的论文/翻译模型。切回 Gemini 恢复 Gemini 那一对。
13. Provider 稳定 id：`gemini` | `openai_compatible` | `grok`。

## 文件地图

| 文件 | 职责 |
|---|---|
| `docs/decisions.md` 等 | D-017 修订与范围文档 |
| `src-tauri/src/chat_completions.rs` | Chat Completions adapter、探针、模型列表 |
| `src-tauri/src/lib.rs` | settings schema 3、凭据、命令、工厂、Discussion/Job 接线 |
| `src-tauri/src/provider_ports.rs` | 不改 Gemini 合同；`PaperModelPort` 保持 |
| `src-tauri/src/reading_artifact_module.rs` | context_roots / provider_nodes 的 provider 参数化 |
| `src-tauri/src/outline_module.rs` | `has_active_paper_root` 按当前/指定 provider 查 |
| `src/types.ts` | `ModelSettings` 多 provider 投影 |
| `src/desktopClient.ts` | 新命令 + memory adapter |
| `src/SettingsWorkbench.tsx` | 三卡 UI、确认切当前 |
| `src/App.tsx` | 空态按当前 provider 文案 |
| `src/modelLabel.ts` 等 | 不要写死 Gemini |

---

### Task 0: 修订产品合同（D-017）

**Files:**
- Modify: `docs/decisions.md`（D-017）
- Modify: `docs/product-scope.md`
- Modify: `docs/data-protocols.md` Provider seam 段
- Modify: `docs/agent-onboarding.md`（禁止 OpenAI/xAI 的句子）
- Modify: `docs/backlog.md`、`docs/handoff.md`、`docs/README.md`（若仍写「明确不做 OpenAI/xAI」）

**Produces:** 后续任务可以合法实现 adapter；文档不再与代码对打。

- [ ] **Step 1:** 把 D-017 改成：V2 论文 LLM 允许 `gemini`、`openai_compatible`、`grok`；OCR 仍只 Mistral；搜索 / Semantic View / 多文档 / 旧库迁移仍不做。修订说明写「2026-08-18：授权 OpenAI-compatible 与 Grok adapter；论文根仍要求整本进上下文；Grok 检索式附件不算论文根。」
- [ ] **Step 2:** `product-scope.md` 首发范围补上设置里的三家 LLM；「明确不做」删掉 OpenAI/xAI 整行。
- [ ] **Step 3:** `data-protocols.md`：production adapter = Gemini Interactions **或** Chat Completions（OpenAI-compatible / Grok）；UI 仍不接触 file/interaction ID。
- [ ] **Step 4:** onboarding / backlog / handoff 同步；保留「不要做搜索、Semantic View、多文档」。
- [ ] **Step 5:** Commit：`docs: authorize OpenAI-compatible and Grok paper providers in D-017`

---

### Task 1: `model-settings.json` schema 3 与投影类型

**Files:**
- Modify: `src-tauri/src/lib.rs`（`StoredModelSettings`、`ModelSettingsView`、`read_model_settings`、`write_model_settings`、`model_settings_view`）
- Modify: `src/types.ts`
- Test: `src-tauri/src/lib.rs` 的 `#[cfg(test)]` 或同文件 settings 测试模块（若 lib.rs 测试不便，在 `src-tauri/src/model_settings.rs` 抽出纯函数并测）

**Interfaces:**

```rust
pub const PROVIDER_GEMINI: &str = "gemini";
pub const PROVIDER_OPENAI_COMPATIBLE: &str = "openai_compatible";
pub const PROVIDER_GROK: &str = "grok";
pub const DEFAULT_OPENAI_BASE: &str = "https://api.openai.com/v1";
pub const GROK_API_BASE: &str = "https://api.x.ai/v1";
pub const PAPER_PROBE_VERSION: &str = "paper-probe-v1";

struct PaperProbeRecord {
    fingerprint: String,      // hex sha256
    passed_at: String,        // RFC3339
    paper_model: String,
}

struct ProviderModelSlot {
    paper_model: String,
    translation_model: String,
    models: Vec<GeminiModelOption>, // 复用现有模型条目形状；兼容口 supportsNativePdf/supportsInteractions 由探针写入
    models_fetched_at: Option<String>,
    connection_verified_at: Option<String>, // Gemini 仍用这个；兼容口以 paper_probe 为准
    paper_probe: Option<PaperProbeRecord>,
    base_url: Option<String>, // 仅 openai_compatible
}

struct StoredModelSettings {
    schema_version: u32, // 3
    current_provider: String,
    gemini: ProviderModelSlot,
    openai_compatible: ProviderModelSlot,
    grok: ProviderModelSlot,
    // 兼容读 v1/v2：顶层 paper_model/provider 映射进 gemini 槽，current_provider=gemini
}

struct ProviderSlotView { /* camelCase 投影，含 credentialConfigured、paperProbePassed、isCurrent */ }

struct ModelSettingsView {
    provider: String,           // currentProvider
    paper_model: String,        // 当前槽
    translation_model: String,
    models: Vec<GeminiModelOption>,
    models_fetched_at: Option<String>,
    connection_verified_at: Option<String>,
    credential_configured: bool, // 当前槽
    credential_store: String,
    mistral_credential_configured: bool,
    mistral_credential_store: String,
    ocr_model: String,
    config_id: String,
    gemini: ProviderSlotView,
    openai_compatible: ProviderSlotView,
    grok: ProviderSlotView,
}
```

前端 `ModelSettings` 与 `ModelSettingsView` 对齐（camelCase）。`provider` 类型：`"gemini" | "openai_compatible" | "grok"`。

**`read_model_settings` 迁移：**
- 文件不存在 → Default：`current_provider=gemini`，Gemini 默认 `gemini-2.5-flash` / `gemini-2.5-flash-lite`，另两槽空模型、无探针。
- `schema_version` 1 或 2 → 升 3：现有顶层字段写入 `gemini` 槽；`current_provider=gemini`；**禁止再把 provider 写死回 gemini 然后丢掉其它槽**。
- 拒绝 schema > 3。

顶层 `paper_model` / `translation_model` / `models` 始终等于 `slot(current_provider)`。

- [ ] **Step 1:** 写失败测试：读 v2 JSON（`provider: gemini`, `paperModel`, `translationModel`）得到 schema 3，`currentProvider=gemini`，gemini 槽保留原模型，openai/grok 槽空。
- [ ] **Step 2:** 写失败测试：schema 3 文件 `currentProvider=openai_compatible` 时 view 的 `paperModel` 来自 openai 槽而不是 gemini 槽。
- [ ] **Step 3:** 跑测试确认失败。
- [ ] **Step 4:** 实现结构体、Default、read/write/migrate、view。
- [ ] **Step 5:** `cargo test --locked` 相关用例通过。
- [ ] **Step 6:** Commit：`feat: version model-settings.json to schema 3 with per-provider slots`

**Done when:** 旧 Gemini 用户升级后行为不变；新槽不会被 `provider = GEMINI_PROVIDER` 那行冲掉。

---

### Task 2: Chat Completions adapter（PaperModelPort）

**Files:**
- Create: `src-tauri/src/chat_completions.rs`
- Modify: `src-tauri/src/lib.rs`（`mod chat_completions;`）

**Interfaces:**

```rust
pub struct ChatCompletionsAdapter {
    client: reqwest::Client,
    provider: String, // "openai_compatible" | "grok"
    api_key: String,
    api_base: String, // 已规范化，无尾斜杠
}

impl ChatCompletionsAdapter {
    pub fn new(provider: impl Into<String>, api_key: impl Into<String>, api_base: impl Into<String>) -> ProviderResult<Self>;
    fn chat_url(&self) -> String;      // {api_base}/chat/completions
    fn files_url(&self) -> String;     // {api_base}/files
    fn models_url(&self) -> String;    // {api_base}/models
}

impl PaperModelPort for ChatCompletionsAdapter { /* interact, interact_stream, interact_text, delete_remote, capabilities */ }
```

**接线规则：**
- Header：`Authorization: Bearer {key}`，`Content-Type: application/json`。
- `interact` / `interact_stream`：messages = `[{role:system, content: system_instruction}, {role:user, content: [...]}]`。
- user content 顺序：PDF 部分（见下）→ `inline_images` 为 `image_url` data URL → `{type:text, text: user_input}`。
- PDF：若 `remote_file_id` 有值则 `{type: file, file: {file_id}}`（若 4xx 再试 `input_file`/`file_id` 变体只作为二次，**测试锁第一种**）；无 id 则先 `POST {api_base}/files` multipart `purpose=user_data`（失败不致命），再退回 `file` part：`filename` + `file_data: data:application/pdf;base64,...`。
- **不要**把 `previous_interaction_id` 发给 Chat Completions。`session_resume` receipt = `false`。`file_reuse` = `remote_file_id.is_some()`。`paper_root_branch` = `previous_interaction_id.is_none()`（与 Gemini 语义对齐：调用方认为这是根分叉）。
- `provider_node_id`：用响应 `id`（如 `chatcmpl-...`）；没有则生成 `chatcmpl-local-{uuid}`。
- `provider_file_id`：上传得到的 id，或已有 remote id，或占位 `inline-pdf`（inline base64 时）。delete_remote：`kind==file` 且 id 不是 `inline-pdf` 则 `DELETE {api_base}/files/{id}`；其它 kind 成功 no-op。
- schema：`response_format` 优先 `{type: json_schema, json_schema: {name: "read_desktop", strict: true, schema: <object>}}`。若调用方传入 OpenAI 外壳 `{name, strict, schema}`，剥内层 `schema`（与 Gemini 一样）。
- stream：SSE `data:` 行，拼 `choices[0].delta.content`；忽略空 delta；`[DONE]` 结束。取消走 `CancellationFlag`。
- `interact_text`：无 PDF，temperature 0.1；有 schema 同样 `response_format`。
- `capabilities`：对该 adapter 返回 `native_pdf/interactions/structured_output/streaming = true`（资格在设置探针，不在模型名前缀）。
- `UsageEnvelope.provider` = adapter.provider。token 从 `usage` 映射；没有则 null。
- 规范化 Base URL：trim、去尾 `/`；若用户写成无 `/v1` 的 `https://api.openai.com` **不要自动乱加**（用户填什么用什么）。Grok 卡不给改，代码里写死 `https://api.x.ai/v1`。

**测试（wiremock，照 `provider_ports.rs` Gemini 测试风格）：**

- `chat_completions_uploads_pdf_then_sends_chat`：files 200 + chat 200；body 含 Bearer、PDF file_id、system+user。
- `chat_completions_falls_back_to_inline_pdf_when_files_404`：files 404，chat 仍 200，user content 含 base64 PDF。
- `chat_completions_ignores_previous_interaction_id_in_body`：传入 previous id，chat JSON **不含** `previous_interaction_id` / `previous_response_id`。
- `chat_completions_stream_emits_text_deltas`：SSE 两段 content。
- `chat_completions_structured_output_uses_json_schema`：body `response_format.type == json_schema`。
- `chat_completions_includes_image_url_parts`。
- `chat_completions_delete_remote_file`。

- [ ] **Step 1:** 写上述失败测试（用最小 PDF fixture，可复制 `provider_ports.rs` 的 `pdf_fixture`）。
- [ ] **Step 2:** `cargo test --locked chat_completions` 失败。
- [ ] **Step 3:** 实现 adapter。
- [ ] **Step 4:** 测试通过。
- [ ] **Step 5:** Commit：`feat: add Chat Completions paper adapter`

**Done when:** 不碰 Gemini 测试；新 adapter 单独用 MockServer 闭环。

---

### Task 3: 论文探针 + `/v1/models` + 凭据

**Files:**
- Modify: `src-tauri/src/chat_completions.rs`（probe / list_models）
- Modify: `src-tauri/src/lib.rs`（keyring 用户名、resolve key、pending test）

**Interfaces:**

```rust
const OPENAI_COMPATIBLE_CREDENTIAL_USER: &str = "openai-compatible-api-key";
const GROK_CREDENTIAL_USER: &str = "grok-api-key";
// 服务名仍 com.skywalker.read-desktop

fn paper_probe_fingerprint(base_url: &str, api_key: &str, paper_model: &str) -> String;

struct PaperProbeOutcome {
    passed: bool,
    failure: Option<String>, // 人类可读，不含 key
}

async fn list_openai_models(adapter: &ChatCompletionsAdapter) -> AppResult<Vec<GeminiModelOption>>;
// GET /models；失败返回 Err 但不作为整卡死锁（命令层改成空列表 + 警告字段）

async fn run_paper_probe(adapter: &ChatCompletionsAdapter, paper_model: &str) -> AppResult<PaperProbeOutcome>;
```

**探针协议 `paper-probe-v1`（两轮付费）：**

1. **PDF+图：** 内置最小 PDF（文本层含固定 nonce `RD-PDF-NONCE-7F3A`）+ 内置小 PNG（像素上是字母 `R` 的 data URL，已存在仓库或测试常量）。一条 chat：要模型返回 JSON 文本（此轮不要 schema）或纯文本，必须同时含 nonce 与字母 `R`。
   - 4xx/5xx → 失败。
   - 响应 `tool_calls` / `choices[].message.tool_calls` / 顶层 `output` 里 type 含 `attachment_search` | `file_search` | `document_search` → 失败（文案：检索式 PDF 不能作为论文根）。
   - 正文不含 nonce 或不含独立字母 R → 失败。
2. **schema：** `interact_text` 或 chat + `response_format`，schema `{type:object, properties:{ok:{type:boolean}}, required:["ok"], additionalProperties:false}`，user：`Return ok true.`。解析 JSON 后 `ok==true` 才过。不要接受「请只输出 JSON」的纯 prompt 过门。

`list_openai_models`：把 `/v1/models` 的 `data[].id` 变成 `GeminiModelOption`；`supportsGenerateContent=true`；`supportsNativePdf/supportsInteractions` 仅当 id == 刚通过探针的 paper_model 时为 true，其它 false。手填的论文模型若通过探针，即使不在列表也要插入一条。

凭据：`read/write/delete/resolve` 各一套，镜像 Gemini。`AppState` 增加 `openai_compatible_api_key`、`grok_api_key` Mutex。

- [ ] **Step 1:** 测试 `paper_probe_fingerprint` 稳定、改 key 即变。
- [ ] **Step 2:** wiremock：chat 返回含 nonce+R 且无 tool → 第一轮过；第二轮返回 `{"ok":true}` → passed。
- [ ] **Step 3:** wiremock：chat 200 但 message.tool_calls name=`attachment_search` → 失败。
- [ ] **Step 4:** wiremock：`/models` 200 列出 id。
- [ ] **Step 5:** 实现；`cargo test --locked` 探针相关通过。
- [ ] **Step 6:** Commit：`feat: add paper-probe-v1 and OpenAI/Grok credential slots`

---

### Task 4: 设置命令（测、存、清、设为当前）

**Files:**
- Modify: `src-tauri/src/lib.rs`（commands + `invoke_handler`）
- Modify: `src-tauri/src/lib.rs`：`save_model_settings` **不得**修改 `current_provider`

**Commands（camelCase IPC）：**

```text
test_openai_compatible_connection { request: { apiKey?, baseUrl?, paperModel } }
test_grok_connection            { request: { apiKey?, paperModel } }
save_openai_compatible_settings { request: { apiKey?, baseUrl, paperModel, translationModel } }
save_grok_settings              { request: { apiKey?, paperModel, translationModel } }
clear_openai_compatible_credential
clear_grok_credential
set_current_paper_provider      { request: { provider } }
```

返回一律 `ModelSettingsView`（test 命令额外返回 `OpenAIConnectionTest`）：

```rust
struct OpenAIConnectionTest {
    models: Vec<GeminiModelOption>,
    tested_at: String,
    using_stored_credential: bool,
    paper_probe_passed: bool,
    paper_probe_error: Option<String>,
    models_fetch_error: Option<String>,
}
```

**行为：**
- test：draft key 或已存 key；跑 list_models（失败 → 空列表 + `models_fetch_error`）+ `run_paper_probe(paperModel)`。把 pending test（key hash + base url + paper model + 结果）放进 `AppState`，供 save 校验。
- save：新 key 必须有匹配 pending test。翻译模型非空即可（可手填，不要求在列表）。论文模型必须本次或已持久化探针通过且指纹匹配。写入对应槽，**不改变 current_provider**。
- Gemini `save_model_settings`：只写 gemini 槽，逻辑保持「新 key 必须先 test」；同样不改 current。
- clear：删 keyring + 内存；该槽 `paper_probe=None`、`connection_verified_at=None`。若 current 就是这家，current **保持**，view 显示未就绪。
- `set_current_paper_provider`：
  - `gemini`：需要 gemini key + `connection_verified_at` + 论文模型仍在列表且 native PDF+interactions + 翻译非空。
  - `openai_compatible` / `grok`：需要 key + 指纹有效的 paper_probe（paper_model 一致）+ 翻译非空。
  - 否则 Err，中文/英文与现有命令风格一致。
  - 成功只改 `current_provider`。不取消 discussion/job。

**`current_llm_ready` 辅助函数**（后续 Task 6 用）：

```rust
fn require_current_paper_provider(app, state) -> AppResult<ReadyPaperProvider> {
  // ReadyPaperProvider { provider, paper_model, translation_model, api_key, base_url? }
  // 未就绪 Err，文案含当前 provider 名，不再写死 Gemini
}
```

- [ ] **Step 1:** 单元测试：save openai 成功后 `current_provider` 仍是 gemini。
- [ ] **Step 2:** 单元测试：探针未过时 `set_current_paper_provider(openai_compatible)` 失败。
- [ ] **Step 3:** 单元测试：clear 当前家 key 后 current 仍是该家、`credentialConfigured=false`。
- [ ] **Step 4:** 实现命令并注册。settings 命令测试可用 tempfile 配置目录（若现有测试已有 app harness 则跟上；否则抽纯函数测 slot 更新，命令层用最小 AppHandle 模式）。
- [ ] **Step 5:** Commit：`feat: add OpenAI-compatible and Grok settings commands`

**Done when:** 能存 Grok 而不成为当前；Gemini 用户保存模型不会被切走。

---

### Task 5: 论文模块 SQL 去写死 `'gemini'`

**Files:**
- Modify: `src-tauri/src/reading_artifact_module.rs`
- Modify: `src-tauri/src/outline_module.rs`
- Modify: `src-tauri/src/lib.rs`（discussion context_roots 查询、remote cleanup UNION）

**规则：** 所有 **论文根 / provider_nodes / context_roots** 查询里的 `provider = 'gemini'` 改为绑定参数，来源是 **这次调用的 provider**（Job payload 或 DiscussionGeneration.provider），不是全局 current。OCR / Mistral 查询不要改。

`ensure_root` INSERT 使用该 provider，不再 `VALUES (?1, ?2, 'gemini', ...)`。

`queue_paper_remote_cleanup`：去掉 `cr.provider = 'gemini'` 过滤，按行里真实 `provider` 登记 tombstone。Chat Completions 的 interaction id 删除是 no-op，file 删除走对应 adapter。

`outline_module` `has_active_paper_root`：签名改为 `(revision_id, provider, model)`。

- [ ] **Step 1:** 给 `reading_artifact_module` 增加测试（或扩展现有）：provider=`openai_compatible` 时 `context_roots.provider` 写入该值；lookup 不会命中 gemini 根。
- [ ] **Step 2:** 失败后改 SQL 与函数签名。所有内部 `call_from_paper_root` / `ensure_root` / Lens QA 查 node 带 provider。
- [ ] **Step 3:** outline / discussion / cleanup 同步。
- [ ] **Step 4:** `cargo test --locked reading_artifact`、`outline`、相关 lib 测试。
- [ ] **Step 5:** Commit：`fix: parameterize paper-root provider instead of hardcoding gemini`

**Done when:** 同一 revision 下 Gemini 根与 OpenAI 根可并存（UNIQUE 已是 `revision_id, provider, model, context_epoch`）。

---

### Task 6: 工厂 + Job/Chat/Pack/Lens/Outline 接线

**Files:**
- Modify: `src-tauri/src/lib.rs`：`JobPaperModelAdapter`、`DiscussionGeneration`、`send_chat`、`run_discussion_generation`、`execute_reading_artifact_job`、`generate_reading_artifact`、`ask_lens`、`generate_brief` / orientation pack、`start_outline` / deep dive workers
- Modify: `src-tauri/src/lib.rs` remote cleanup worker 里构造 adapter 的分支

**Interfaces:**

```rust
#[derive(Clone)]
enum PaperAdapter {
    Gemini(GeminiInteractionsAdapter),
    Chat(ChatCompletionsAdapter),
}

impl PaperModelPort for PaperAdapter { /* 转发 */ }

fn open_paper_adapter(provider: &str, api_key: &str, base_url: Option<&str>) -> AppResult<PaperAdapter> {
    match provider {
        "gemini" => Ok(PaperAdapter::Gemini(GeminiInteractionsAdapter::production(api_key)?)),
        "openai_compatible" => Ok(PaperAdapter::Chat(ChatCompletionsAdapter::new(
            "openai_compatible", api_key, base_url.unwrap_or(DEFAULT_OPENAI_BASE),
        )?)),
        "grok" => Ok(PaperAdapter::Chat(ChatCompletionsAdapter::new(
            "grok", api_key, GROK_API_BASE,
        )?)),
        other => Err(format!("Unsupported paper provider: {other}")),
    }
}

struct JobPaperModelAdapter {
    inner: PaperAdapter, // 不再写死 GeminiInteractionsAdapter
    jobs: JobModule,
    job_id: String,
    cancellation: CancellationFlag,
    committed: Arc<AtomicBool>,
}
```

**DiscussionGeneration 增加：** `provider: String`, `api_key` 仍为快照 key，`base_url: Option<String>`。

**`send_chat`：**
- 用 `require_current_paper_provider` 代替 `resolve_gemini_key` + Gemini verified。
- context_roots 查询用 `generation.provider` 不是 `'gemini'`。
- `previous_remote_provider_id`：仅当 provider==gemini 时用于 incremental；**否则 `initial_input` 永远 `recovery_input`，previous 仅作本地记录、adapter 忽略。**
- 入队/开跑前插入模型变化分界：若该 discussion 上一条 assistant 的 `usage.provider` 与当前不同，沿用现有「模型变化」行为（已有则接上）。

**Job enqueue：** `provider: Some(current.provider)`，payload 增加 `"provider": ...`，必要时 `"baseUrl"`（openai）。`root_key` 改为 `{revision_id}:{provider}:{paper_model}`。

**Job execute：** 从 payload 读 provider，resolve **那一家** 的 key（不是 current）。key 已清除 → job 失败文案 `Configure {provider} before resuming this job`。

**运行时 4xx 且信息表明 PDF/图/schema 不被支持：** 清掉该槽 `paper_probe`（不要改 current_provider）。下一轮 `require_current` 会未就绪。

**远程 cleanup：** 按 tombstone.provider 选 Gemini 或 ChatCompletionsAdapter。

- [ ] **Step 1:** 编译期把 `JobPaperModelAdapter.inner` 换成 `PaperAdapter`，所有 `GeminiInteractionsAdapter::production` 论文调用点改为 `open_paper_adapter`。
- [ ] **Step 2:** 为 `send_chat` 路径加 rust 测试或抽出 `discussion_user_input(provider, has_previous) -> enum Incremental|Recovery` 并测：gemini+previous → Incremental；openai → 永远 Recovery。
- [ ] **Step 3:** Job payload 快照测试：enqueue reading_artifact 含 provider。
- [ ] **Step 4:** `cargo test --locked` 与 `cargo check --locked`。
- [ ] **Step 5:** Commit：`feat: route paper jobs and chat through snapshotted provider adapters`

**Done when:** 不改 current 也能跑完已入队的 Gemini job；新 chat 在 openai 当前时不会对 Gemini Files API 发请求。

---

### Task 7: 前端类型、DesktopClient、Settings UI

**Files:**
- Modify: `src/types.ts`
- Modify: `src/desktopClient.ts`、`src/desktopClient.test.ts`
- Modify: `src/SettingsWorkbench.tsx`、`src/SettingsWorkbench.test.tsx`
- Modify: `src/styles.css`（只加必要的 provider 卡/当前条，复用 `.provider-card`）

**UI 合同：**
- Models 页顶：**当前论文 provider** 条：名字、就绪/未就绪、当前论文模型、翻译模型。
- 三张卡：Gemini（现有）、OpenAI-compatible、Grok。当前卡徽标 `Current`。
- OpenAI 卡字段：Base URL、API KEY、测连接、论文模型（select+手填 input）、翻译模型（select+手填）、Save。Grok 无 Base URL。
- 「设为当前」仅当该卡 `paperProbePassed`（Gemini 则是现有 verified + native PDF 模型）且翻译非空且已配 key。点击后确认框文案：
  - 标题：`Switch current paper provider?`
  - 正文：`The next send will create a new paper root for this provider. Local discussion history is kept. In-flight replies and jobs keep the provider they started with.`
  - 按钮：`Switch` / `Cancel`
- 进行中不禁用切换（产品锁定）。
- 测连接结果：列出 `paper_probe_error` / `models_fetch_error`。探针失败时 Save 论文模型仍可存（配置），但「设为当前」不可用。
- Gemini 卡测连接/保存流程保持；保存成功 `savedNotice` 仍要可见（onboarding §）。
- 离开未保存确认沿用现有 dirty 逻辑，三张卡的 draft 都算 dirty。

**测试：**
- 保存 OpenAI 卡不会把 Gemini 标成非 current（mock `save_openai_compatible_settings` 返回 current 仍 gemini）。
- 探针未过时没有 `Set as current` 或按钮 disabled。
- 点 `Set as current` 先出现确认，未确认不 invoke。
- 手填模型 id 可见。
- memory adapter：新命令不 throw；`set_current_paper_provider` 可改内存 settings.provider。

- [ ] **Step 1:** 扩展 `ModelSettings` 类型；修所有测试夹具（`SettingsWorkbench.test.tsx`、`App` 测试、`desktopClient.test.ts`）。
- [ ] **Step 2:** 写 Settings 新用例（先红）。
- [ ] **Step 3:** `npx vitest run --maxWorkers=1 --fileParallelism=false src/SettingsWorkbench.test.tsx` 失败。
- [ ] **Step 4:** 实现 UI + desktopClient 命令名。
- [ ] **Step 5:** Vitest + `npx tsc -b` 通过。
- [ ] **Step 6:** Commit：`feat: add OpenAI-compatible and Grok cards in Settings`

**Done when:** Models 页能并存三家；不点确认不能切换 current。

---

### Task 8: App 空态与状态文案去 Gemini 写死

**Files:**
- Modify: `src/App.tsx`（所有 `Gemini is not configured` / `Connect Gemini` / `Gemini ready` 等，按 `modelSettings.provider` 与当前槽 `credentialConfigured` / 探针）
- Modify: 触及这些字符串的测试：`tests/viewNavigation.test.tsx`、`tests/compactComposer.test.tsx` 等
- Modify: `src/modelLabel.ts` 仅当仍假设 Gemini 列表时

**规则：**
- 就绪：当前 provider 有 key 且（gemini verified 或兼容口 paperProbePassed）。
- 未就绪文案模板：`{ProviderLabel} is not configured · open Model configuration to continue`。Label：Gemini / OpenAI-compatible / Grok。
- 状态条：`{ProviderLabel} ready · {paperModel}` / `{ProviderLabel} credential cleared`。
- composer placeholder：`Configure {ProviderLabel} to ask this paper…`
- **不要**改助手气泡模型名逻辑（已读 `usage.model`）。

- [ ] **Step 1:** 搜索 `Gemini` 用户可见字符串，列清单，测试覆盖至少：未配置空态、配置成功状态条。
- [ ] **Step 2:** 实现。
- [ ] **Step 3:** 串行 Vitest 相关文件 + `tsc -b`。
- [ ] **Step 4:** Commit：`fix: make reader empty states follow current paper provider`

---

### Task 9: 全量验证与回归

- [ ] **Step 1:** `npx vitest run --maxWorkers=1 --fileParallelism=false`
- [ ] **Step 2:** `npx tsc -b`
- [ ] **Step 3:** `cd src-tauri; cargo test --locked`（允许原有 unused 警告，禁止新增失败）
- [ ] **Step 4:** `cargo check --locked`
- [ ] **Step 5:** `git diff --check`
- [ ] **Step 6:** 手动核对（无浏览器工具时用 vitest + 说明）：升级路径（旧 model-settings.json）、设为当前确认、Grok 探针因 tool call 失败时不能当前、Gemini 卡仍能测/存。
- [ ] **Step 7:** Commit 仅当有修正：`test: verify multi-provider paper settings and adapters`

---

## 明确不做（本计划范围外）

- Azure OpenAI、自定义 Header、Organization、Responses API、Assistants。
- 跨 provider 翻译（论文一家、翻译另一家）。
- 把 Grok 检索式 PDF 当成论文根。
- 自动切换 / 自动回退 Gemini。
- 每次启动或每次发送重打探针。
- 多套自定义兼容端点列表（只有一张 OpenAI-compatible 卡）。
- 搜索、Semantic View、多文档、旧 Workspace 迁移。

## 实现备注（避免踩坑）

- `lib.rs` 里 `read_model_settings` 现有 `settings.provider = GEMINI_PROVIDER` **必须删掉**，否则 schema 3 无法存活。
- `normalize_model_id` 禁止 `/`；兼容口用新函数，Gemini 仍走旧函数。
- `JobPaperModelAdapter` 当前 `Clone` + 具体 Gemini 类型；改枚举后继续 Clone（两个 adapter 都是 Clone）。
- 浏览器 `npm run dev` memory adapter 不得假装真探针成功到「可当前」，除非测试显式 mock。
- 改 `webview_pinch.rs` 无关，不要碰。改 Tauri 命令后需重启 `tauri dev`。
