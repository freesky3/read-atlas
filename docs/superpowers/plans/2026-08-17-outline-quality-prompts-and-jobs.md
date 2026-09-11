# Outline Quality, Prompt Catalog, Delete/Regenerate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让总图靠可编辑提示词成为能带人读完全文的论证地图；补上删除 / 重生成；修任务中心「新任务消失」；局部图按同一合同拆开当前节点。

**Architecture:** 全部生产系统提示词存在应用配置目录 `prompt-settings.json`（与 `model-settings.json` 并列，不进工作区、不按论文）。Settings 第三栏编辑；保存前校验；入队时把本趟全文写入 Job payload，执行和 repair 只读 payload。总图重生成时旧图继续显示，成功后切头指针并物理删除上一版总图修订（CASCADE 旧局部图）。删除必须二次确认，生成中禁用，取消只走任务中心。任务中心按 `updatedAt` 新→旧，不按状态整组置顶。

**Tech Stack:** Tauri 2 + Rust + rusqlite, React 19, TypeScript, Vitest + Testing Library, 现有 SettingsWorkbench / OperationsDrawer / OutlinePane / JobModule / OutlineModule。

**Spec:** 本会话 grilling 收口（2026-08-17 Q1–Q16 = A）；[docs/full-outline-v1.md](../../full-outline-v1.md)；[docs/decisions.md](../../decisions.md) D-031。本计划不重做 React Flow 画布。

## Global Constraints

- Vitest 必须串行：`npx vitest run --maxWorkers=1 --fileParallelism=false`。
- 改 Rust 后跑对应 `cargo test <filter>`；不要为清 unused/dead_code 警告去改无关代码。
- Tauri 2 invoke：命令参数用 `{ request: { camelCase } }` 的，保持该包装；新命令若只有几个标量，与邻近命令保持同一风格。
- schema / OCR 目录 / PDF 附件由程序注入，禁止写进可编辑提示词文本，也禁止让用户编辑 JSON schema。
- 不对发布设节点数 / 主链硬门闩。质量靠默认稿，瘦图照发。
- 不引入 MiniMap、layout Worker、按论文存提示词、提示词历史栈（只有一层「上一份」）、版本浏览 UI。
- 生成中不静默合并、不排队第二条 Outline Job。取消只走任务中心。
- 删除不可恢复，必须二次确认。生成中删除 / 重生成按钮禁用。
- 样式只用已有 `--glass-*` / `--liquid-glass-*` / `--ink` / `--muted` / `--line`。不要 Tailwind。
- 提交前：聚焦测试绿；每个 Task 结束 `git diff --check`。Task 8 跑全量串行 Vitest、`npx tsc -b`、相关 cargo test。

## Locked product decisions (do not re-grill)

1. 抽单元 + 构图 + 局部图默认稿按「能带人读完 / 拆开该论点」写；narrative 边必须有中文关系短标签；画布默认画出来。
2. 全部生产槽位可编；翻译一条模板 + `{output_language}`；Lens 公式 / 图 / 表各一份生成 + 各一份 repair；Lens QA 共用。
3. 存在应用账号 `prompt-settings.json`；重置工作区不恢复默认提示词。
4. 空稿或翻译 / Lens 生成·repair 丢掉 `{output_language}` → 拒绝保存；运行时不回退。
5. 入队快照全文；执行和 repair 只读 payload。
6. 总图重生成：旧图留着，成功后切头并删旧修订（CASCADE 局部图）。总图删除：摘头、删修订、取消该论文全部局部图 Job。
7. 局部图删除 / 重生成只动当前节点。
8. 生成中两按钮禁用；取消只走任务中心。
9. 任务中心一律 `updatedAt` 新→旧。
10. 局部图仍一次调用；目录 = 该节点全部证据页 + narrative 邻居证据页（**不要 ±1 slack**）。

## File map

| 文件 | 职责 |
| --- | --- |
| `src-tauri/src/prompt_settings.rs` | 槽位枚举、出厂默认稿、校验、读写 `prompt-settings.json`、占位符替换 |
| `src-tauri/src/lib.rs` | `mod prompt_settings`；get/save/restore 命令；所有生产调用改为 resolve + 快照；delete/start 守卫 |
| `src-tauri/src/outline_protocol.rs` | 协议版本 bump 到 v2（提示词语义变了） |
| `src-tauri/src/outline_module.rs` | `delete_overview` / `delete_deep_dive`；`publish_*` 成功后删上一版 |
| `src-tauri/src/outline_catalog.rs` | `outline_local_pages` 从 `lib.rs` 挪来，slack 默认 0 |
| `src-tauri/src/job_module.rs` | `list` 按 `updated_at DESC`；`active_of(kind, revision_id)`；可选 `cancel_active_of` |
| `src-tauri/src/reading_artifact_module.rs` | 请求可带已解析的 system / repair 文本；缺省才回退内置函数 |
| `src/types.ts` | `PromptSlotId`、`PromptSlotProjection`、`PromptSettings` |
| `src/desktopClient.ts` | 新 projection / command 名；memory stub |
| `src/promptCatalog.ts` | 槽位 UI 元数据、前端校验镜像 |
| `src/PromptCatalogSection.tsx` | Settings 第三栏编辑器 |
| `src/SettingsWorkbench.tsx` | `SettingsSection` 增加 `"prompts"` |
| `src/outline/outlinePlan.ts` | 已发布图 + 在途总图 Job → 仍是 published（横幅），不是整页 generating |
| `src/outline/OutlinePane.tsx` | 重生成 / 删除、确认框、生成中横幅叠在旧图上 |
| `src/outline/outlineLayout.ts` | narrative 边带 `label` |
| `src/OperationsDrawer.tsx` | 只按 `updatedAt` 排序 |
| `src/App.tsx` | delete/regenerate 命令、在途禁用、入队失败 `setStatus` |
| `docs/full-outline-v1.md` | 删除 / 重生成 / 提示词目录 / 局部图目录规则 |
| `docs/decisions.md` | D-033 |
| `docs/handoff.md` | 现状与下一步 |

不新建独立窗口。不把提示词塞进 `model-settings.json`。

---

### Task 1: 提示词目录（存储 / 默认稿 / 校验）

**Files:**

- Create: `src-tauri/src/prompt_settings.rs`
- Modify: `src-tauri/src/lib.rs`（`mod prompt_settings;` 以及三个 Tauri 命令，先不要改生成调用点）
- Modify: `src-tauri/src/outline_protocol.rs`（协议常量）
- Test: `src-tauri/src/prompt_settings.rs` 的 `#[cfg(test)]`

**Interfaces:**

```rust
// prompt_settings.rs
pub const PROMPT_SETTINGS_SCHEMA: u32 = 1;
pub const PROMPT_SETTINGS_FILE: &str = "prompt-settings.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptSlotId {
    PaperRoot,
    OrientationPack,
    Discussion,
    DiscussionCompaction,
    Translation,
    Explanation,
    LensFormula,
    LensFigure,
    LensTable,
    LensRepairFormula,
    LensRepairFigure,
    LensRepairTable,
    LensQa,
    OutlineExtract,
    OutlineCompose,
    OutlineDeepDive,
}

impl PromptSlotId {
    pub fn as_str(self) -> &'static str;
    pub fn requires_output_language(self) -> bool; // translation + 6 lens gen/repair
    pub fn all() -> &'static [PromptSlotId];
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptSlotState {
    pub text: String,
    pub previous_text: Option<String>,
    pub updated_at: Option<String>,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptSettingsProjection {
    pub schema_version: u32,
    pub slots: BTreeMap<String, PromptSlotState>,
}

pub fn default_text(slot: PromptSlotId) -> &'static str;
pub fn validate_slot_text(slot: PromptSlotId, text: &str) -> Result<(), String>;
pub fn apply_placeholders(text: &str, output_language: Option<&str>) -> String;
pub fn prompt_settings_path(config_dir: &Path) -> PathBuf;
pub fn load_store(path: &Path) -> Result<PromptSettingsProjection, String>;
pub fn resolved_text(store: &PromptSettingsProjection, slot: PromptSlotId) -> String;
pub fn save_slot(path: &Path, slot: PromptSlotId, text: &str) -> Result<PromptSettingsProjection, String>;
pub fn restore_previous(path: &Path, slot: PromptSlotId) -> Result<PromptSettingsProjection, String>;
pub fn restore_default(path: &Path, slot: PromptSlotId) -> Result<PromptSettingsProjection, String>;
```

出厂稿必须逐字如下（其它槽位 = 当前线上原文）。

`OutlineExtract`:

```text
You extract evidence-anchored argument units that can guide a reader through the entire paper. This is not a 4-sentence abstract and not a table of contents.

Cover the paper's question, setup, method, key evidence, results, and boundaries. Prefer more units over a handful of summary cards, but every unit must be a real argumentative claim in the paper.

Rules:
- Only cite block IDs from the provided OCR catalog. Never invent blocks, never retell the table of contents as units, and never fabricate claims to hit a count.
- Each unit needs a title, a takeaway a reader can hold in one breath, a role_class from the schema, and evidenceIds from the catalog.
- Return only the strict schema.
```

`OutlineCompose`:

```text
Compose a two-tier argument map that a reader can follow from entry to conclusion.

Rules:
- narrative edges must form one weakly connected DAG. They are the reading spine.
- Every narrative edge MUST have a short Chinese label naming the relation (examples: 据此推出, 用来验证, 在此设定下, 结果支持, 边界限制). Do not leave narrative labels empty.
- cross_link edges are optional asides; they must not be required to walk the spine; give them Chinese labels too when present.
- Only cite catalog block IDs. Do not invent nodes to absorb leftover objects.
- Prefer a map someone can read the paper by, not a 4-node summary.
- Return only the strict schema.
```

`OutlineDeepDive`:

```text
Compose a detailed local argument map that unpacks exactly one Overview node so a reader can finish that claim.

Rules:
- Stay inside the supplied local OCR catalog. Do not generate a third Outline layer.
- Every narrative edge MUST have a short Chinese relation label. narrative must be a weakly connected DAG.
- Expand the selected node into enough argument units to understand its setup, mechanism, evidence, and caveats. Do not collapse it into one restated card.
- Do not invent blocks or claims. Return only the strict schema.
```

`Translation`（保持现语义，改成占位符）：

```text
Translate only the target OCR Block into {output_language}. Use the supplied same-page neighbors, Brief, and matched glossary/symbol entries only to disambiguate terminology. Do not summarize, explain the paper, or infer from an unseen PDF. Return only the strict schema.
```

`LensFormula`:

```text
Create a Formula Lens in {output_language} using the complete PDF context and the exact OCR formula anchor. Keep it isolated from Discussion. Provide a quick takeaway and coherent sections. Include every non-trivial symbol with provenance. Reconstruct the LaTeX and explain what the formula does and where to start. Evidence IDs must come only from the supplied whitelist. Return only the strict schema.
```

`LensFigure`:

```text
Create a Figure Lens in {output_language} using the complete PDF context and the exact OCR figure anchor. Keep it isolated from Discussion. Provide a quick takeaway and coherent sections. Give an overall explanation, identify panels when present, and point out the visual hotspots a reader should look at first. Evidence IDs must come only from the supplied whitelist. Return only the strict schema.
```

`LensTable`:

```text
Create a Table Lens in {output_language} using the complete PDF context and the exact OCR table anchor. Keep it isolated from Discussion. Provide a quick takeaway and coherent sections. Explain how rows and columns relate and any calculation a reader must not misread. Evidence IDs must come only from the supplied whitelist. Return only the strict schema.
```

`LensRepairFormula` / `LensRepairFigure` / `LensRepairTable`：在现有 repair 句上把 `{kind}` 写死为 Formula / Figure / Table，并保留 `{output_language}`。例如 Formula：

```text
Repair the previous Formula Lens into the exact strict schema in {output_language}. Correct only schema, completeness, and evidence-whitelist violations. Do not add ungrounded claims. This is the single allowed repair attempt.
```

下列槽位 **逐字复制** 当前函数，不要改写：

- `PaperRoot` ← `paper_root_system_instruction()`
- `OrientationPack` ← generate_brief 里那句 `"Use the complete PDF as the sole source..."`
- `Discussion` ← `send_chat` 里 `"Use the complete PDF as the authoritative source..."`
- `DiscussionCompaction` ← compaction 那句 `"Use the complete PDF and the supplied local Discussion path..."`
- `Explanation` ← `explanation_system_instruction()`
- `LensQa` ← `lens_qa_system_instruction()`

`validate_slot_text`：`text.trim().is_empty()` → `"Prompt cannot be empty"`；`requires_output_language` 且不含字面量 `{output_language}` → `"This prompt must contain {output_language}"`。

`save_slot`：先 validate；把当前 `resolved_text` 推进 `previous_text`；写入新 `text`；`is_default = text == default_text(slot)`。

`restore_previous`：没有 `previous_text` 时返回错误 `"No previous prompt to restore"`，不改文件。

`restore_default`：当前稿推进 previous，再写入出厂稿。

`apply_placeholders`：仅替换 `{output_language}`；其它 `{...}` 原样保留。

协议常量改为：

```rust
pub const EXTRACT_PROTOCOL: &str = "outline-extract-v2";
pub const COMPOSE_PROTOCOL: &str = "outline-compose-v2";
pub const DEEP_DIVE_PROTOCOL: &str = "outline-deep-dive-v2";
```

已发布的 v1 图仍合法显示，不自动重生成。

Tauri 命令（本 Task 只加命令，不改生成路径）：

```rust
#[tauri::command]
fn get_prompt_settings(app: tauri::AppHandle) -> AppResult<PromptSettingsProjection>;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavePromptSlotRequest { slot: PromptSlotId, text: String }

#[tauri::command]
fn save_prompt_slot(app: tauri::AppHandle, request: SavePromptSlotRequest) -> AppResult<PromptSettingsProjection>;

#[tauri::command]
fn restore_prompt_previous(app: tauri::AppHandle, request: SavePromptSlotRequest /* slot only; ignore text */) -> AppResult<PromptSettingsProjection>;
```

`restore_prompt_previous` / `restore_prompt_default` 用更小的 request：

```rust
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PromptSlotRequest { slot: PromptSlotId }
```

路径：`app.path().app_config_dir()?.join("prompt-settings.json")`。

- [ ] **Step 1: Write the failing Rust tests**

在 `prompt_settings.rs` 底部加入测试，使用 `tempfile::tempdir`（crate 已有）：

```rust
#[test]
fn rejects_empty_and_translation_without_placeholder() {
    assert!(validate_slot_text(PromptSlotId::OutlineExtract, "   ").is_err());
    assert!(validate_slot_text(PromptSlotId::Translation, "Translate the block.").is_err());
    assert!(validate_slot_text(PromptSlotId::Translation, "Translate into {output_language}.").is_ok());
}

#[test]
fn save_pushes_previous_and_restore_default_roundtrips() {
    let dir = tempfile::tempdir().unwrap();
    let path = prompt_settings_path(dir.path());
    let saved = save_slot(&path, PromptSlotId::OutlineExtract, "custom extract").unwrap();
    assert_eq!(saved.slots["outline_extract"].text, "custom extract");
    assert_eq!(
        saved.slots["outline_extract"].previous_text.as_deref(),
        Some(default_text(PromptSlotId::OutlineExtract))
    );
    let restored = restore_default(&path, PromptSlotId::OutlineExtract).unwrap();
    assert_eq!(
        restored.slots["outline_extract"].text,
        default_text(PromptSlotId::OutlineExtract)
    );
    assert_eq!(
        restored.slots["outline_extract"].previous_text.as_deref(),
        Some("custom extract")
    );
}

#[test]
fn apply_placeholders_fills_language_only() {
    let filled = apply_placeholders("into {output_language} keep {kind}", Some("zh-CN"));
    assert_eq!(filled, "into zh-CN keep {kind}");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test rejects_empty_and_translation_without_placeholder -- --nocapture`

Expected: compile fail（`prompt_settings` 模块不存在）或 test fail。

- [ ] **Step 3: Implement the module and wire commands**

实现上述 API；`lib.rs` 增加 `mod prompt_settings;`、三个/四个命令，并在 `invoke_handler` 注册 `get_prompt_settings`、`save_prompt_slot`、`restore_prompt_previous`、`restore_prompt_default`。`load_store` 在文件不存在时返回全部槽位的出厂投影（不强制写盘）。未知 slot key 忽略。缺的槽位用出厂稿补齐，`is_default: true`。

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test prompt_settings -- --nocapture`

Expected: 新测试 PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/prompt_settings.rs src-tauri/src/lib.rs src-tauri/src/outline_protocol.rs
git commit -m "feat: add workspace-account prompt catalog with outline defaults"
```

---

### Task 2: Settings 第三栏「提示词」

**Files:**

- Create: `src/promptCatalog.ts`
- Create: `src/PromptCatalogSection.tsx`
- Create: `src/PromptCatalogSection.test.tsx`
- Modify: `src/types.ts`
- Modify: `src/desktopClient.ts`、`src/desktopClient.test.ts`
- Modify: `src/SettingsWorkbench.tsx`、`src/SettingsWorkbench.test.tsx`
- Modify: `src/styles.css`（沿用 settings 表单卡，不要新主题）

**Interfaces:**

```ts
export type PromptSlotId =
  | "paper_root"
  | "orientation_pack"
  | "discussion"
  | "discussion_compaction"
  | "translation"
  | "explanation"
  | "lens_formula"
  | "lens_figure"
  | "lens_table"
  | "lens_repair_formula"
  | "lens_repair_figure"
  | "lens_repair_table"
  | "lens_qa"
  | "outline_extract"
  | "outline_compose"
  | "outline_deep_dive";

export type PromptSlotProjection = {
  text: string;
  previousText: string | null;
  updatedAt: string | null;
  isDefault: boolean;
};

export type PromptSettings = {
  schemaVersion: number;
  slots: Record<string, PromptSlotProjection>;
};

export function validatePromptDraft(slot: PromptSlotId, text: string): string | null;
```

`SettingsSection = "workspace" | "models" | "prompts"`。

前端 invoke：

```ts
desktopClient.open<PromptSettings>("get_prompt_settings");
desktopClient.command<PromptSettings>("save_prompt_slot", {
  request: { slot, text },
});
desktopClient.command<PromptSettings>("restore_prompt_previous", {
  request: { slot },
});
desktopClient.command<PromptSettings>("restore_prompt_default", {
  request: { slot },
});
```

`createMemoryDesktopClient` 返回内存里一份出厂 `PromptSettings`（可用固定短字符串，不必复制 Rust 长稿；但 `translation` 必须含 `{output_language}`）。

UI：左侧第三钮「Prompts」。右侧：槽位列表（分组：Outline / Lens / Reading / Discussion / Roots）+ textarea + `保存` / `恢复上一次` / `恢复默认`。保存失败展示命令错误。`恢复上一次` 在 `previousText == null` 时 disabled。

- [ ] **Step 1: Write the failing UI tests**

`src/promptCatalog.ts` 先写校验函数测试可放在 `PromptCatalogSection.test.tsx`：

```tsx
it("refuses to save an empty prompt or a translation without the language placeholder", async () => {
  const user = userEvent.setup();
  const onSave = vi.fn();
  render(<PromptCatalogSection settings={factorySettings} busy={false}
    onSave={onSave} onRestorePrevious={vi.fn()} onRestoreDefault={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: /抽单元|Extract/i }));
  await user.clear(screen.getByRole("textbox"));
  await user.click(screen.getByRole("button", { name: "保存" }));
  expect(onSave).not.toHaveBeenCalled();
  expect(screen.getByText(/不能为空|cannot be empty/i)).toBeInTheDocument();
});

it("saves the selected slot and can restore default", async () => {
  // mock desktopClient or inject callbacks
});
```

`SettingsWorkbench.test.tsx` 增加：点「Prompts」后能看到槽位列表。

- [ ] **Step 2: Run tests to verify they fail**

Run: `npx vitest run --maxWorkers=1 --fileParallelism=false src/PromptCatalogSection.test.tsx src/SettingsWorkbench.test.tsx`

Expected: FAIL（文件或按钮不存在）。

- [ ] **Step 3: Implement the section and nav**

`App.tsx` 不必在本 Task 改打开入口以外的东西；`SettingsWorkbench` 自己在切到 prompts 时 `get_prompt_settings`。dirty textarea 未保存时切槽位要先提示或丢弃草稿——本 Task 用「切槽位即丢未保存草稿」，不要做未保存拦截。

- [ ] **Step 4: Run tests to verify they pass**

Run: `npx vitest run --maxWorkers=1 --fileParallelism=false src/PromptCatalogSection.test.tsx src/SettingsWorkbench.test.tsx src/desktopClient.test.ts`

Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src/promptCatalog.ts src/PromptCatalogSection.tsx src/PromptCatalogSection.test.tsx src/types.ts src/desktopClient.ts src/desktopClient.test.ts src/SettingsWorkbench.tsx src/SettingsWorkbench.test.tsx src/styles.css
git commit -m "feat: add editable prompt catalog to Settings"
```

---

### Task 3: 入队快照并接到全部生产调用

**Files:**

- Modify: `src-tauri/src/lib.rs`（`start_outline`、`start_outline_deep_dive`、`generate_reading_artifact`、`execute_*`、`send_chat`、compaction、`generate_brief` / orientation、paper root）
- Modify: `src-tauri/src/reading_artifact_module.rs`（`GenerateReadingArtifactRequest` 增加可选已解析提示词；`ask_lens` 增加可选 QA 提示词）
- Test: `src-tauri/src/lib.rs` 或 `prompt_settings.rs` 的 helper 单测 + 现有 reading artifact 测试仍过

**Interfaces:**

```rust
// lib.rs helper
fn load_prompt_text(app: &tauri::AppHandle, slot: PromptSlotId, output_language: Option<&str>) -> AppResult<String> {
    let store = prompt_settings::load_store(&prompt_settings_path_for_app(app)?)?;
    let raw = prompt_settings::resolved_text(&store, slot);
    prompt_settings::validate_slot_text(slot, &raw)?; // 磁盘被手改坏了就失败，不回退
    Ok(prompt_settings::apply_placeholders(&raw, output_language))
}
```

Job payload 增加 `prompts` 对象，执行只读它：

```json
{
  "prompts": {
    "extract": "...",
    "compose": "..."
  }
}
```

```json
{
  "prompts": { "deepDive": "..." }
}
```

```json
{
  "prompts": { "system": "...", "repair": "..." }
}
```

`GenerateReadingArtifactRequest`：

```rust
#[serde(default)]
pub system_instruction: Option<String>,
#[serde(default)]
pub repair_system_instruction: Option<String>,
```

模块里：`request.system_instruction.clone().unwrap_or_else(|| translation_system_instruction(...))`。测试不填这些字段时行为与现在相同。

`start_outline` 在 enqueue 前：

```rust
let extract = load_prompt_text(&app, PromptSlotId::OutlineExtract, None)?;
let compose = load_prompt_text(&app, PromptSlotId::OutlineCompose, None)?;
```

写入 payload。`execute_outline_overview_job` 用 `job.payload["prompts"]["extract"]` / `compose`，缺字段则 `ProviderError::local_state("Outline job is missing frozen prompts")`。repair 也用同一份 `compose` 文本（不要重读设置）。

`send_chat` / compaction / orientation / paper root：命令入口读一次，整次请求复用该 `String`。

`ask_lens`：入口读 `LensQa` 一次，传入模块。

- [ ] **Step 1: Write the failing test**

在 `reading_artifact_module.rs` 测试里加：当 `system_instruction: Some("FROZEN")` 时，fake port 收到的 `system_instruction` 是 `FROZEN` 而不是内置翻译句。若现有测试用假 port 不好断言，就在 `prompt_settings` 测 `load` + 在 `lib.rs` 抽 `fn outline_prompts_from_payload(payload: &Value) -> Result<(String,String), String>` 并单测缺字段失败。

```rust
#[test]
fn outline_prompts_from_payload_requires_frozen_texts() {
    let err = outline_prompts_from_payload(&serde_json::json!({"model":"x"})).unwrap_err();
    assert!(err.contains("frozen prompts"));
    let (extract, compose) = outline_prompts_from_payload(&serde_json::json!({
        "prompts": {"extract": "E", "compose": "C"}
    })).unwrap();
    assert_eq!(extract, "E");
    assert_eq!(compose, "C");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test outline_prompts_from_payload_requires_frozen_texts -- --nocapture`

Expected: FAIL（函数不存在）。

- [ ] **Step 3: Wire every production call site**

替换这些硬编码（不要留一份旧路径）：

| 位置 | 槽位 |
| --- | --- |
| `execute_outline_overview_job` extract / compose | payload `prompts.extract` / `compose` |
| `execute_outline_deep_dive_job` | payload `prompts.deepDive` |
| `start_outline` / `start_outline_deep_dive` | 入队前 `load_prompt_text` |
| `generate_reading_artifact` payload | `Translation` / `Explanation` / 对应 `Lens*` + repair |
| `execute_reading_artifact_job` | 把 payload prompts 填进 Request |
| `ask_lens` | `LensQa` |
| `send_chat` | `Discussion` |
| compaction | `DiscussionCompaction` |
| orientation / generate_brief | `OrientationPack` |
| paper root | `PaperRoot` |

内置 `*_system_instruction` 函数保留作出厂稿来源（Task 1 已复制），生成路径不再直接 format kind。

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test outline_prompts_from_payload_requires_frozen_texts -- --nocapture` ；再跑 `cargo test lens_validation_rejects_unknown_evidence_ids -- --nocapture`

Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/reading_artifact_module.rs
git commit -m "feat: freeze prompt texts into jobs at enqueue"
```

---

### Task 4: 地图质量 UX（边标签 / 重生成时留旧图 / 局部图目录）

**Files:**

- Modify: `src/outline/outlineLayout.ts`、`src/outline/outlineLayout.test.ts`
- Modify: `src/outline/outlinePlan.ts`、`src/outline/outlinePlan.test.ts`
- Modify: `src/outline/OutlinePane.tsx`、`src/outline/OutlinePane.test.tsx`
- Modify: `src-tauri/src/outline_catalog.rs`（迁入并测试 `outline_local_pages`）
- Modify: `src-tauri/src/lib.rs`（deep dive 调用改为 slack=0 的新函数；删除 `lib.rs` 里旧私有函数）
- Modify: `src/styles.css`（边标签可读，不要挡住节点）

**Interfaces:**

```ts
// outlinePlan.ts — 只有「没有可渲染总图」时，在途 Job 才把整页打成 generating
export function outlinePaneKind(...): OutlinePaneKind
```

新规则：

1. `projection.head?.graph` 存在 → 返回 `published` 或 `stale`（按原 status），**即使** `activeJob` 是 running 的 `outline_overview`。
2. 没有 graph 且有在途 `outline_overview` → `generating`。
3. 在途 `outline_deep_dive` **绝不**把总图打成 generating。

`OutlinePane`：`kind` 为 published/stale 且 `activeJob` 是总图在途时，在画布上方加 `role="status"` 横幅，复用 `outlineProgressCopy`。

`toFlowElements`：narrative 边设置 `label: edge.label.trim() || undefined`，并 `labelStyle` / `labelBgStyle` 用 `--ink` / `--glass-card`。cross_link 仅在开关打开时同样显示 label。

`outline_local_pages(graph, node_id, catalog)`：**不再接收 slack**，或 slack 固定 0。已包含该节点 + narrative 邻居的证据页。Deep dive `execute` 改用这个函数。

- [ ] **Step 1: Write the failing tests**

```ts
// outlinePlan.test.ts
it("keeps a published map visible while an overview job regenerates", () => {
  expect(
    outlinePaneKind({
      projection: projection({
        status: "generating",
        head: { id: "h1", kind: "overview", status: "published", /* ... */ graph },
      }),
      plan: plan(),
      planError: null,
      activeJob: runningOverviewJob,
    }),
  ).toBe("published");
});
```

```ts
// outlineLayout.test.ts
it("puts narrative edge labels on flow edges by default", () => {
  const { edges } = toFlowElements(graph, null, false);
  const narrative = edges.find((edge) => edge.id === "n1");
  expect(narrative?.label).toBe("then");
  expect(edges.some((edge) => edge.id === "c1")).toBe(false);
});
```

```rust
// outline_catalog.rs
#[test]
fn local_pages_include_node_and_narrative_neighbors_without_slack() {
    // node A evidence page 5, neighbor B page 8, unrelated C page 9
    // result is {5, 8} not {4,5,6,7,8,9}
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npx vitest run --maxWorkers=1 --fileParallelism=false src/outline/outlinePlan.test.ts src/outline/outlineLayout.test.ts` ；`cargo test local_pages_include_node -- --nocapture`

Expected: FAIL（旧逻辑把有图 + job 判成 generating；边没有 label；函数还在 lib.rs 且 slack=1）。

- [ ] **Step 3: Implement the three behavior changes**

`outlinePaneKind` 按上面规则改，并改掉现有用例 `"treats an active job as generating even if the plan card is still present"`：无 head 时仍为 generating；有 graph 时改为 published。

`OutlinePane` 在 published 画布上叠横幅，文案含「返回讨论不会中断生成」。

Deep dive 调用：

```rust
let pages = crate::outline_catalog::outline_local_pages(&graph, &node_id, &catalog);
let local_catalog = crate::outline_catalog::filter_catalog_to_pages(&catalog, &pages);
```

- [ ] **Step 4: Run tests to verify they pass**

Run: 同 Step 2，外加 `npx vitest run --maxWorkers=1 --fileParallelism=false src/outline/OutlinePane.test.tsx`

Expected: PASS。有图 + 在途 Job 的 pane 测试应 `findByRole('status')` 看到进度文案，且仍能看到节点标题。

- [ ] **Step 5: Commit**

```bash
git add src/outline src-tauri/src/outline_catalog.rs src-tauri/src/lib.rs src/styles.css
git commit -m "feat: show edge labels and keep map visible during regenerate"
```

---

### Task 5: 删除 / 重生成后端

**Files:**

- Modify: `src-tauri/src/outline_module.rs`
- Modify: `src-tauri/src/job_module.rs`
- Modify: `src-tauri/src/lib.rs`（`start_outline` / `start_outline_deep_dive` 守卫；`delete_outline` / `delete_outline_deep_dive`；publish 成功后删旧修订）
- Test: `outline_module.rs` 与 `job_module.rs` 单测

**Interfaces:**

```rust
// job_module.rs
impl JobModule {
    pub fn active_of(&self, kind: &str, revision_id: &str) -> JobResult<Option<JobProjection>>;
    pub fn list_active_of(&self, kind: &str, revision_id: &str) -> JobResult<Vec<JobProjection>>;
}

// outline_module.rs
impl OutlineModule {
    pub fn delete_overview(&self, revision_id: &str) -> OutlineResult<()>;
    pub fn delete_deep_dive(&self, revision_id: &str, node_id: &str) -> OutlineResult<()>;
}
```

`active_of` SQL：

```sql
SELECT ... FROM jobs
WHERE kind = ?1 AND revision_id = ?2
  AND state IN ('queued', 'running', 'paused')
ORDER BY updated_at DESC LIMIT 1
```

`start_outline`：若 `active_of("outline_overview", revision)` 有值 → `Err("An Outline is already generating. Cancel it from the task center first.")`。不要走 enqueue 合并。

`start_outline_deep_dive`：同样，按 `outline_deep_dive` + 同一 `revision_id`，再检查 payload/artifact_key 是否同一 `node_id`。已有该节点在途 → 同样错误。**允许**总图已发布时再入队（这就是重生成）。

`delete_outline` 命令：

1. 若总图在途 → 同一句错误（前端按钮本应禁用，后端再挡一层）。
2. `list_active_of("outline_deep_dive", revision_id)`，对每个 id 走与 `cancel_job` 相同的取消（含 `artifact_cancellations`）。
3. `outline.delete_overview(revision_id)`：`DELETE FROM outline_revisions WHERE revision_id = ?1`（FK CASCADE `outline_heads` / `outline_deep_dive_heads`）。
4. 返回 `get_outline` 投影，此时应是 `ready_to_plan`。

`delete_outline_deep_dive`：

1. 该节点局部图在途 → 错误，要求去任务中心取消。
2. `delete_deep_dive`：按 head 找到该 node 的 deep_dive 行并 `DELETE FROM outline_revisions WHERE id = ?`，同时删 `outline_deep_dive_heads` 对应行。
3. 总图不动。

`publish_overview`：插入新行并更新 `outline_heads` **之后**，若存在 `previous_id != new_id`，`DELETE FROM outline_revisions WHERE id = previous_id`（CASCADE 旧局部图）。

`publish_deep_dive`：upsert head 之后删除该 node 上一份 deep_dive revision（若 id 不同）。

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn delete_overview_removes_head_and_deep_dives() { /* insert overview + deep dive, delete, both counts 0 */ }

#[test]
fn publish_overview_deletes_previous_revision() { /* publish A, publish B, only B remains */ }

#[test]
fn delete_deep_dive_keeps_overview() { /* ... */ }
```

`job_module`：

```rust
#[test]
fn active_of_ignores_completed_jobs() { /* enqueue, complete, active_of is None */ }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test delete_overview_removes_head -- --nocapture`

Expected: FAIL。

- [ ] **Step 3: Implement delete / publish cleanup / start guards**

注册：

```rust
delete_outline,
delete_outline_deep_dive,
```

请求体：

```rust
struct OutlineRequest { revision_id: String } // 已有
struct OutlineDeepDiveRequest { revision_id: String, node_id: String } // 已有
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test delete_overview_removes_head -- --nocapture` ；`cargo test publish_overview_deletes_previous -- --nocapture` ；`cargo test delete_deep_dive_keeps_overview -- --nocapture` ；`cargo test active_of_ignores_completed -- --nocapture`

Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/outline_module.rs src-tauri/src/job_module.rs src-tauri/src/lib.rs
git commit -m "feat: delete and regenerate Outline revisions"
```

---

### Task 6: 删除 / 重生成 UI

**Files:**

- Modify: `src/desktopClient.ts`（`delete_outline`、`delete_outline_deep_dive`）
- Modify: `src/outline/OutlinePane.tsx`、`src/outline/OutlinePane.test.tsx`
- Modify: `src/outline/OutlineCanvas.tsx`（局部图工具条需要删除 / 重生成；inspector 的「生成局部图」在已有局部图且未在看局部图时仍是「打开」）
- Modify: `src/App.tsx`
- Modify: `src/styles.css`（确认框用现有 scrim / operations 确认样式）

**Interfaces:**

```ts
// OutlinePane props 增量
onRegenerateOverview?: () => void;
onDeleteOverview?: () => void;
overviewBusy?: boolean;          // 总图在途
onRegenerateDeepDive?: () => void;
onDeleteDeepDive?: () => void;
deepDiveBusy?: boolean;          // 当前节点局部图在途
```

已发布总图工具条（`compact-rail` 右侧、讨论按钮左边）：

- 「重生成」→ `onRegenerateOverview`；`overviewBusy` 时 disabled，title=`正在生成，请到任务中心取消`。
- 「删除」→ 打开确认；`overviewBusy` 时 disabled。

确认文案必须含不可恢复：

- 总图：`将删除当前论证地图和所有局部图，且不可恢复。确定删除？`
- 局部图：`将删除这个节点的局部图，总图保留，且不可恢复。确定删除？`

按钮：`取消` / `确认删除`。未点确认不得调用命令。

局部图标题行同样两个按钮，绑定 `onRegenerateDeepDive` / `onDeleteDeepDive`。`deepDiveBusy` 时禁用。

`App.tsx`：

- `startOutline` 在已有 graph 时就是重生成（同一命令）。失败 `setStatus`。
- `deleteOutline` → `delete_outline` `{ request: { revisionId } }`，成功后清 `deepDiveHead` / `outlineView`，`get_outline`。
- 局部图重生成：即使 `get_outline_deep_dive` 已有 cache，也调用 `start_outline_deep_dive`（不要走现在的「有 cache 就打开」短路）。把「打开」和「重生成」分开：inspector 「打开局部图」仍只切视图；工具条「重生成」才入队。
- 入队 / 删除失败都 `setStatus`，不要吞掉。

- [ ] **Step 1: Write the failing tests**

```tsx
it("asks for confirmation before deleting a published overview", async () => {
  const onDelete = vi.fn();
  render(<OutlinePane ... projection={published} onDeleteOverview={onDelete} />);
  await user.click(screen.getByRole("button", { name: "删除" }));
  expect(onDelete).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "确认删除" }));
  expect(onDelete).toHaveBeenCalledTimes(1);
});

it("disables regenerate and delete while an overview job is running", () => {
  render(<OutlinePane ... projection={published} overviewBusy activeJob={runningJob} />);
  expect(screen.getByRole("button", { name: "重生成" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "删除" })).toBeDisabled();
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npx vitest run --maxWorkers=1 --fileParallelism=false src/outline/OutlinePane.test.tsx`

Expected: FAIL。

- [ ] **Step 3: Implement toolbar, confirm, and App handlers**

`buildStartOutlineInvokeArgs` 不变。新增：

```ts
export function buildDeleteOutlineInvokeArgs(revisionId: string) {
  return { request: { revisionId } };
}
export function buildDeleteOutlineDeepDiveInvokeArgs(revisionId: string, nodeId: string) {
  return { request: { revisionId, nodeId } };
}
```

并补 `outlinePlan.test.ts` 的 invoke 包装断言。

- [ ] **Step 4: Run tests to verify they pass**

Run: `npx vitest run --maxWorkers=1 --fileParallelism=false src/outline/OutlinePane.test.tsx src/outline/outlinePlan.test.ts src/outline/OutlineCanvas.test.tsx`

Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src/outline src/App.tsx src/desktopClient.ts src/desktopClient.test.ts src/styles.css
git commit -m "feat: add Outline delete and regenerate controls"
```

---

### Task 7: 任务中心按更新时间新→旧

**Files:**

- Modify: `src/OperationsDrawer.tsx`、`src/OperationsDrawer.test.tsx`
- Modify: `src-tauri/src/job_module.rs`（`list` 的 ORDER BY，与 UI 一致）
- Modify: `src/App.tsx`（`startOutline` / `startOutlineDeepDive` 失败已有 status；入队后 `refreshJobs()`，不要只本地 prepend 造成顺序和服务器不一致）

**Interfaces:**

```ts
// OperationsDrawer sortedJobs
[...jobs].sort((a, b) => {
  const aTime = new Date(a.updatedAt || a.createdAt).getTime();
  const bTime = new Date(b.updatedAt || b.createdAt).getTime();
  if (!Number.isNaN(aTime) && !Number.isNaN(bTime) && aTime !== bTime) {
    return bTime - aTime;
  }
  return b.id.localeCompare(a.id);
});
```

**删除** `stateOrder` 整组置顶。进行中只靠现有 `.job-card.state-running` 轨道颜色和徽章，不靠排序。

`list` SQL：

```sql
ORDER BY jobs.updated_at DESC, jobs.id DESC
```

现有测试 `"renders newest jobs at the top of the task list"` 改成用 `updatedAt`（新完成的 completed 排在旧 running 之上）。再加一条：三张卡片全部在 document 里（防裁切把后几条卸掉）。`operations-body` 已是 `overflow: auto`；不要给 `.job-ledger` 加 `overflow: hidden`。

`startOutline` 成功后 `await refreshJobs()`。入队抛错时 `setStatus(\`Outline failed · ${String(error)}\`)`（已有则核对文案仍可见）。

- [ ] **Step 1: Write the failing test**

```tsx
it("orders jobs by updatedAt newest first even when an older job is still running", () => {
  const runningOld = { ...job, id: "old-run", state: "running",
    updatedAt: "2026-08-16T09:00:00Z", createdAt: "2026-08-16T09:00:00Z" };
  const completedNew = { ...job, id: "new-done", state: "completed",
    updatedAt: "2026-08-16T12:00:00Z", createdAt: "2026-08-16T11:00:00Z",
    payload: { action: "explain" } };
  render(<OperationsDrawer open jobs={[runningOld, completedNew]} ... />);
  const cards = screen.getAllByRole("article");
  expect(cards[0]).toHaveTextContent("Block explanation");
  expect(cards[1]).toHaveTextContent(/* running job label */);
});
```

改掉旧测试里「进行中永远在上」的隐含假设（若有）。

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run --maxWorkers=1 --fileParallelism=false src/OperationsDrawer.test.tsx`

Expected: FAIL（旧 comparator 把 running 置顶）。

- [ ] **Step 3: Change comparator and SQL**

同步改 `job_module::list`。若有 `job_module` 列表顺序测试，一起改。

- [ ] **Step 4: Run tests to verify they pass**

Run: `npx vitest run --maxWorkers=1 --fileParallelism=false src/OperationsDrawer.test.tsx` ；`cargo test job_module -- --nocapture`（若太宽，改为该文件里 list 相关测试名）

Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src/OperationsDrawer.tsx src/OperationsDrawer.test.tsx src-tauri/src/job_module.rs src/App.tsx
git commit -m "fix: sort task center by updated time newest first"
```

---

### Task 8: 文档与全量验收

**Files:**

- Modify: `docs/full-outline-v1.md`
- Modify: `docs/decisions.md`（追加 D-033）
- Modify: `docs/handoff.md`
- Modify: `docs/feature-specs.md`、`docs/backlog.md` 中仍写「不能重生成 / 不做 Outline」的过期句

**文档要点（写进去，不要只提一句「更新文档」）：**

`full-outline-v1.md` 增加一节「删除与重生成」：总图 / 局部图行为、生成中禁用、取消走任务中心、成功后物理删除旧修订。改「局部图目录 ±1 页」为「节点证据页 + narrative 邻居证据页」。增加「提示词」：应用账号目录、入队冻结、出厂稿可恢复。画布默认绘制 narrative `label`。

`decisions.md`：

```markdown
## D-033：Outline 质量靠可编辑提示词；地图可删可重生成

- 状态：accepted（2026-08-17）
- 决定：不设节点数发布门闩。抽单元 / 构图 / 局部图默认稿改为阅读地图合同。全部生产系统提示词可在 Settings 编辑，存在应用账号，入队冻结。总图与局部图提供删除（二次确认、不可恢复）和重生成（旧图留到成功）。任务中心按 updatedAt 新到旧。
- 后果：旧「只能生成一次」作废。协议 bump 到 extract/compose/deep-dive v2。不提供版本浏览。
```

`handoff.md`「已实现」补：提示词目录、删除/重生成、任务中心排序、边标签、局部图目录。删掉「已锁定产品约束」里过期的「不做 Outline」。

- [ ] **Step 1: Update the three docs and grep stale claims**

搜 `不做 Outline`、`不能重生成`、`±1`、`10–18` 硬门闩表述，只改仍声称当前产品做不到删除/重生成或禁止 Outline 的句子。不要顺手改无关历史 ADR 正文里的当时决定（D-031 可加一句「已被 D-033 补充」）。

- [ ] **Step 2: Run the full verification gate**

```bash
npx vitest run --maxWorkers=1 --fileParallelism=false
npx tsc -b
cargo test prompt_settings -- --nocapture
cargo test delete_overview_removes_head -- --nocapture
cargo test local_pages_include_node -- --nocapture
```

Expected: 全绿。`npx tsc -b` 无错。

本 Task 不要求 `npm run build`（未改 chunk 策略）。不要跑 live Gemini。

- [ ] **Step 3: Commit**

```bash
git add docs/full-outline-v1.md docs/decisions.md docs/handoff.md docs/feature-specs.md docs/backlog.md
git commit -m "docs: record prompt catalog and Outline delete/regenerate"
```

---

## Self-review

**Spec coverage**

| 锁定项 | Task |
| --- | --- |
| 默认稿 A（抽单元 / 构图 / 局部图） | 1 |
| 全部槽位可编 + 翻译模板 + Lens 6+1 | 1–2 |
| 保存 / 上一份 / 默认；拒空；拒缺占位符 | 1–2 |
| 应用账号存储 | 1 |
| 入队冻结 | 3 |
| 边标签默认绘制 | 4 |
| 重生成时旧图可见 | 4 |
| 局部图目录无 ±1 | 4 |
| 总图 / 局部图删除重生成数据 | 5 |
| 生成中禁用、确认删除、取消走任务中心 | 6 |
| start 不静默合并 | 5 |
| 删除总图取消局部图 Job | 5 |
| 任务中心 updatedAt 新→旧 | 7 |
| 协议 v2 + ADR | 1、8 |

**Placeholder scan:** 无 TBD；出厂稿全文写在 Task 1。

**Type consistency:** `PromptSlotId` serde `snake_case` 与前端字面量一致；invoke 用 `{ request: { slot, text } }`；Job `prompts` 键名 extract/compose/deepDive/system/repair 在 Task 3 与 Task 5 共用。
