# Outline Fork Canvas UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让论证地图按论文的并行结构分叉（而不是一条竖链），同时修好标签黑块、紧凑节点、可拖节点、可收起可拖的详情栏，并一次性丢掉旧 Outline 数据。

**Architecture:** 图的形状仍由提示词决定，不设发布门闩。dagre 继续 `rankdir: TB`，只加大同层间距、缩小固定卡片高度。React Flow 节点可拖，坐标只留在这次打开，不写回图合同。详情栏从固定 220px 改成选中才打开，宽度按论文记在 `reading_states`。旧图不做兼容：工作区 `schema_meta.outline_epoch` 落后则清空 Outline 表和 Outline Job；应用账号三份 Outline 提示词按 `outlinePromptGeneration` 强制回到新出厂稿。

**Tech Stack:** Tauri 2 + Rust + rusqlite, React 19, `@xyflow/react` + `@dagrejs/dagre`, Vitest + Testing Library。

**Spec:** 本会话 grilling 收口（2026-08-17 Q1–Q12）；`docs/full-outline-v1.md`；`docs/decisions.md` D-031 / D-033。本计划不重做 Job / 删除 / 重生成 / Settings 提示词栏，只改出厂稿、画布壳、一次性迁移。

**Also save a copy to:** `docs/superpowers/plans/2026-08-17-outline-fork-canvas-ux.md` at the start of execution (plan mode cannot write that path).

## Global Constraints

- Vitest 必须串行：`npx vitest run --maxWorkers=1 --fileParallelism=false`。
- 改 Rust 后跑对应 `cargo test <filter>`；不要为清 unused/dead_code 警告去改无关代码。
- Tauri 2 invoke 参数风格与邻近命令一致。
- schema / OCR 目录 / PDF 附件仍由程序注入，禁止写进可编辑提示词。
- 不设「没有分叉就不能发布」的门闩。单链论文可以仍是一条线。
- 不做 MiniMap、layout Worker、手动画边、节点内展开、参考图四色分类（概念/案例/问题/结论）、版本浏览、自动重跑生成。
- 坐标不写入 `graph_json`，也不写入阅读状态。
- 样式只用已有 `--glass-*` / `--liquid-glass-*` / `--ink` / `--muted` / `--line` / `--blue`。不要 Tailwind。
- 提交前：聚焦测试绿；每个 Task 结束 `git diff --check`。最后一个 Task 跑全量串行 Vitest、`npx tsc -b`、相关 cargo test。
- 当前就在 `master` 工作区，Outline 底盘已在树上。原地做，不要从旧 HEAD 开 worktree。

## Locked product decisions (do not re-grill)

1. 抽单元 / 构图 / 局部图出厂稿都写清：有并行就分叉汇合，没有并行就一条线。阅读序号不决定边。
2. 详情栏：未选中收起；选中打开；分割条可拖 200–420px；宽度按论文记住；收起用分割条按钮或再点已选中节点。
3. 边标签：细线 + 小字，去掉实心 `--glass-card` 黑块。交叉引用默认隐藏，打开后同样处理。
4. 布局：dagre TB，加大 `nodesep`。节点可拖。Fit / Reset / 换图 / 重生成回到 dagre。边不能手连。
5. 阅读序号保留拓扑序，改成小号角标。
6. 一次性迁移：清空全部 Outline 修订 / 头指针 / 计划缓存 / Outline Job；三份 Outline 提示词（含上一份）强制回新出厂稿。论文 / OCR / 讨论 / Lens / 其它提示词不动。不自动入队。
7. 卡片：小号角标 + `roleLabel` + 最多两行标题。不要 takeaway、不要箭头、不要节点内展开。单击只开右侧详情；「跳到原文」只在详情里。点选时不自动跳 PDF。
8. 协议只保留一套现行号（v3）。代码不再为了展示去读旧协议图——旧图会在迁移时删掉。

## File map

| 文件 | 职责 |
| --- | --- |
| `src-tauri/src/v2_workspace.rs` | `OUTLINE_EPOCH`；打开工作区时一次性清空 Outline 表和 Outline Job |
| `src-tauri/src/prompt_settings.rs` | 三份 Outline 新出厂稿；`outlinePromptGeneration` 一次性清槽位 |
| `src-tauri/src/outline_protocol.rs` | `EXTRACT/COMPOSE/DEEP_DIVE` 常量改为 v3 |
| `src-tauri/src/paper_module.rs` | `ReadingState.outline_inspector_width`；读写 + 校验 200–420 |
| `src/types.ts` | `ReadingState.outlineInspectorWidth` |
| `src/outline/outlineInspectorWidth.ts` | 夹取常量 |
| `src/outline/outlineSelect.ts` | 再点已选中节点则清空 |
| `src/outline/outlineLayout.ts` | 更矮卡片、更大 `nodesep`、可拖、细标签样式 |
| `src/outline/OutlineCanvas.tsx` | 紧凑卡片、详情栏/分割条、session 拖、Reset 回 dagre |
| `src/outline/OutlinePane.tsx` | 把宽度 / 清空选中传给画布 |
| `src/App.tsx` | 点选不再 `jumpOutlineEvidence`；记住详情宽度 |
| `src/styles.css` | 紧凑卡片、小号角标、细边标签、分割条 |
| `docs/full-outline-v1.md` | 分叉合同、画布壳、epoch 迁移 |
| `docs/decisions.md` | D-034 |
| `docs/handoff.md` | 现状 |

不改 Job 入队、删除/重生成按钮、Settings 提示词 UI。

---

### Task 1: 一次性清旧图 + 协议 v3 + 分叉出厂稿

**Files:**

- Modify: `src-tauri/src/v2_workspace.rs`
- Modify: `src-tauri/src/prompt_settings.rs`
- Modify: `src-tauri/src/outline_protocol.rs`
- Test: `src-tauri/src/v2_workspace.rs` 与 `src-tauri/src/prompt_settings.rs` 的 `#[cfg(test)]`

**Interfaces:**

```rust
pub const OUTLINE_EPOCH: i64 = 3;
pub const OUTLINE_EPOCH_KEY: &str = "outline_epoch";
fn migrate_outline_epoch(connection: &Connection) -> Result<(), String>;

pub const OUTLINE_PROMPT_GENERATION: u32 = 3;
// PromptStoreFile.outline_prompt_generation: u32, missing = 0

pub const EXTRACT_PROTOCOL: &str = "outline-extract-v3";
pub const COMPOSE_PROTOCOL: &str = "outline-compose-v3";
pub const DEEP_DIVE_PROTOCOL: &str = "outline-deep-dive-v3";
```

`migrate_outline_epoch` 在 `initialize_database()` 末尾、`migrate_outline_schema()` 之后调用。读 `schema_meta.outline_epoch`；缺省或 `< 3` 则按顺序删除 `outline_deep_dive_heads`、`outline_heads`、`outline_plans`、`outline_revisions`，再 `DELETE FROM jobs WHERE kind IN ('outline_overview', 'outline_deep_dive')`，`UPDATE reading_states SET active_outline_node_id = NULL, outline_view = 'overview'`，写入 epoch `3`。已是 3 则什么都不删。

`load_store` / `read_file` 若 generation `< 3`：从 slots 删除 `outline_extract` / `outline_compose` / `outline_deep_dive`（含 previous），写回 generation `3`。其它槽位不动。不要走 `restore_default`。`PROMPT_SETTINGS_SCHEMA` 保持 `1`。

出厂稿必须包含这些句子（可润色换行，不可改掉诉求）：

`OutlineExtract` 追加：When the paper presents parallel claims, methods, evidence lines, or competing explanations, emit a separate unit for each. Do not flatten them into one chronological card just to keep a single reading order. A genuinely linear paper may still produce a linear unit list.

`OutlineCompose` 改为：Compose a two-tier argument map. narrative edges must form one weakly connected DAG and every narrative edge MUST have a short Chinese relation label. If two units are alternatives, concurrent methods, or sibling evidence, connect them as narrative siblings under the same parent, or join them later at a synthesis node. Do not serialize parallel units into a single chain just to invent a reading order. A paper that truly is one line may stay one line. Reading order is computed later and must not determine the edges. cross_link edges are optional asides. Only cite catalog block IDs. Do not invent nodes. Return only the strict schema.

`OutlineDeepDive` 同样写清：局部图里并行机制/证据做成兄弟；不要只为阅读序串成一条；仍禁止第三层。

- [ ] **Step 1: 写失败测试（epoch 清空）**

`WorkspaceModule::open` 空目录，插入最小合法 papers / document_revisions / ocr_revisions / outline_revisions / outline_heads / 一条 `jobs.kind = 'outline_overview'`，把 `outline_epoch` 写成 `2`。再 `WorkspaceModule::new().open(同一路径)`。断言 revisions 与该类 jobs 为 0，epoch 为 `"3"`，论文行仍在。epoch 已是 3 时新插入的 revision 在再次 open 后必须还在。同一 module 对同一路径会跳过 `initialize_database`，第二次必须 `WorkspaceModule::new()`。

- [ ] **Step 2: 写失败测试（提示词世代）**

```rust
#[test]
fn outline_prompt_generation_wipes_three_slots_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = prompt_settings_path(dir.path());
    save_slot(&path, PromptSlotId::OutlineExtract, "old extract").unwrap();
    save_slot(&path, PromptSlotId::Explanation, "keep me").unwrap();
    let mut raw: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    raw["outlinePromptGeneration"] = serde_json::json!(2);
    std::fs::write(&path, serde_json::to_vec_pretty(&raw).unwrap()).unwrap();

    let loaded = load_store(&path).unwrap();
    assert!(loaded.slots["outline_extract"].is_default);
    assert!(loaded.slots["outline_extract"].previous_text.is_none());
    assert!(loaded.slots["outline_extract"].text.contains("parallel"));
    assert_eq!(loaded.slots["explanation"].text, "keep me");

    save_slot(&path, PromptSlotId::OutlineCompose, "user fork").unwrap();
    let again = load_store(&path).unwrap();
    assert_eq!(again.slots["outline_compose"].text, "user fork");
}
```

另加：`default_text(OutlineCompose)` 含 `siblings` 或 `parallel`；`EXTRACT_PROTOCOL == "outline-extract-v3"`。

- [ ] **Step 3: 跑测试，确认失败**

```
cargo test --manifest-path src-tauri/Cargo.toml outline_epoch -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml outline_prompt_generation -- --nocapture
```

Expected: FAIL。

- [ ] **Step 4: 实现迁移、世代、出厂稿、v3 常量**

删除顺序先头指针再 revisions。`job_attempts` 有 CASCADE。若 `outline_module` 测试写死 `outline-compose-v2`，改成 `COMPOSE_PROTOCOL` 常量。

- [ ] **Step 5: 跑测试确认通过**

```
cargo test --manifest-path src-tauri/Cargo.toml outline_epoch -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml outline_prompt_generation -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml prompt_settings -- --nocapture
```

Expected: PASS。

- [ ] **Step 6: Commit**

```
git add src-tauri/src/v2_workspace.rs src-tauri/src/prompt_settings.rs src-tauri/src/outline_protocol.rs src-tauri/src/outline_module.rs
git commit -m "feat(outline): wipe pre-v3 maps and ship forked default prompts"
```

---

### Task 2: dagre 间距、矮卡片、细标签、节点可拖

**Files:**

- Modify: `src/outline/outlineLayout.ts`
- Modify: `src/outline/outlineLayout.test.ts`
- Modify: `src/styles.css`（只加 edge text 规则）

**Interfaces:**

```ts
export const OUTLINE_NODE_WIDTH = 224;
export const OUTLINE_NODE_HEIGHT = 84;
export const OUTLINE_NODE_SEP = 148;
export const OUTLINE_RANK_SEP = 112;

export function narrativeEdgeStyle(tier: string): { strokeWidth: number; strokeDasharray?: string };
export function narrativeLabelStyle(): { fill: string; fontSize: number; fontWeight: number };
export function narrativeLabelBgStyle(): { fill: string; fillOpacity: number };
```

`setGraph` 使用上述 sep。`rank` 计算里的 `104` 改成 `OUTLINE_RANK_SEP`。`toFlowElements`：`draggable: true`。边：`fontSize: 10`、`fill: var(--muted)`、`fill: var(--glass-surface)`、`fillOpacity: 0.35`、`labelBgPadding: [1, 3]`。禁止 `--glass-card`。

- [ ] **Step 1: 改测试**

`draggable === true`。分叉图 `q→m1, q→m2, m1→r, m2→r`：`Math.abs(m1.x - m2.x) > OUTLINE_NODE_WIDTH`，`boxesOverlap(m1, m2, 24) === false`。`labelStyle.fontSize === 10`，`labelBgStyle.fill` 不含 `--glass-card`。

- [ ] **Step 2: 跑测试，确认失败**

```
npx vitest run --maxWorkers=1 --fileParallelism=false src/outline/outlineLayout.test.ts
```

Expected: FAIL。

- [ ] **Step 3: 实现常量和样式**

CSS：`.outline-flow .react-flow__edge-text { fill: var(--muted); font-size: 10px; }` 与 `.react-flow__edge-textbg { fill: var(--glass-surface); fill-opacity: 0.35; }`。

- [ ] **Step 4: 跑测试确认通过**

同一 vitest 命令。Expected: PASS。

- [ ] **Step 5: Commit**

```
git add src/outline/outlineLayout.ts src/outline/outlineLayout.test.ts src/styles.css
git commit -m "feat(outline): widen sibling ranks and thin edge labels"
```

---

### Task 3: 紧凑卡片 + 可收起详情栏 + session 拖拽

**Files:**

- Create: `src/outline/outlineInspectorWidth.ts`
- Create: `src/outline/outlineInspectorWidth.test.ts`
- Modify: `src/outline/OutlineCanvas.tsx`
- Modify: `src/outline/OutlineCanvas.test.tsx`
- Modify: `src/styles.css`
- Modify: `src/outline/OutlinePane.tsx`

**Interfaces:**

```ts
export const OUTLINE_INSPECTOR_MIN = 200;
export const OUTLINE_INSPECTOR_MAX = 420;
export const OUTLINE_INSPECTOR_DEFAULT = 280;
export function clampOutlineInspectorWidth(width: number): number;

// OutlineCanvas 增量 props
inspectorWidth?: number;
onInspectorWidthChange?: (width: number) => void;
onClearSelection?: () => void;
```

行为：

- `nodesDraggable={true}`，`nodesConnectable={false}`，`deleteKeyCode={null}`。
- `useNodesState` / `useEdgesState`。只在 `graph` + `canvasKey` 变化时重置 nodes。`selectedNodeId` 只更新 `data.selected`，不得重跑 dagre。
- Reset：「回到可读起点」先 `toFlowElements` 写回 nodes，再 `setCenter` 到顶层入口。Fit 仍是 `fitView`。
- 卡片：角标 + `roleLabel` + 两行 `title`。不渲染 takeaway，无 chevron。
- 未选中：不渲染 inspector。
- 选中：右侧详情，宽度 clamp。左侧 `role="separator"` name「调整详情宽度」，往左拖变宽 `startWidth + (origin - clientX)`。按钮「收起详情」→ `onClearSelection`。
- 详情：角色、标题、takeaway、「跳到原文」（`primaryEvidenceId`，无证据禁用）、交叉引用、证据 pills、Deep dive。
- `onNodeClick`：已选中则 clear，否则 select。不跳 PDF。`onPaneClick` → clear。

删掉 shell 的 `grid-template-columns: ... 220px`，改 flex。角标 16×16 / 9px。

- [ ] **Step 1: 写失败测试**

`clampOutlineInspectorWidth(120)===200`，`(500)===420`，`NaN===280`。

未选中：无 takeaway 文本、无 complementary、节点有 `draggable`。选中 n1：complementary 含 takeaway；点「跳到原文」→ `onJump("fig-1")`；点「收起详情」→ `onClear`。删除旧的 `not.toHaveClass("draggable")` 与 `outline-node-takeaway` 断言。

- [ ] **Step 2: 跑测试，确认失败**

```
npx vitest run --maxWorkers=1 --fileParallelism=false src/outline/OutlineCanvas.test.tsx src/outline/outlineInspectorWidth.test.ts
```

Expected: FAIL。

- [ ] **Step 3: 实现画布**

- [ ] **Step 4: 跑测试确认通过**

```
npx vitest run --maxWorkers=1 --fileParallelism=false src/outline/OutlineCanvas.test.tsx src/outline/outlineInspectorWidth.test.ts src/outline/outlineLayout.test.ts
```

Expected: PASS。

- [ ] **Step 5: Commit**

```
git add src/outline/OutlineCanvas.tsx src/outline/OutlineCanvas.test.tsx src/outline/OutlinePane.tsx src/outline/outlineInspectorWidth.ts src/outline/outlineInspectorWidth.test.ts src/styles.css
git commit -m "feat(outline): compact cards, collapsible inspector, ephemeral drag"
```

---

### Task 4: 点选不再跳 PDF；详情宽度按论文持久化

**Files:**

- Modify: `src-tauri/src/paper_module.rs`
- Modify: `src-tauri/src/v2_workspace.rs`
- Modify: `src/types.ts`
- Modify: `src/App.tsx`
- Modify: `src/outline/OutlinePane.tsx`
- Create: `src/outline/outlineSelect.ts`
- Create: `src/outline/outlineSelect.test.ts`

**Interfaces:**

```rust
pub outline_inspector_width: i64, // 200..=420
// ALTER TABLE reading_states ADD COLUMN outline_inspector_width INTEGER NOT NULL DEFAULT 280;
```

不在范围内 → `"Reading state is outside the supported range"`。

```ts
export function nextOutlineSelection(currentId: string | null, clickedId: string): string | null {
  return currentId === clickedId ? null : clickedId;
}
```

`App.tsx`：`onSelectNode` 用 `nextOutlineSelection`；清空则 return；删除 `jumpOutlineEvidence(first)`。接线 `onClearSelection` / `inspectorWidth` / `onInspectorWidthChange`。`readingStateSaver.push` 带 `outlineInspectorWidth`。恢复时 clamp。

- [ ] **Step 1: 写失败测试**

Rust：默认 280；存 320 读回 320；存 100 Err。TS：`(null,"n1")==="n1"`；`("n1","n1")===null`；`("n1","n2")==="n2"`。不要渲染整个 Reader。

- [ ] **Step 2: 跑测试，确认失败**

```
cargo test --manifest-path src-tauri/Cargo.toml save_reading -- --nocapture
npx vitest run --maxWorkers=1 --fileParallelism=false src/outline/outlineSelect.test.ts
```

Expected: FAIL。

- [ ] **Step 3: 实现列、校验、App 接线**

测试桩 `ReadingState` 补 `outlineInspectorWidth: 280`。

- [ ] **Step 4: 跑测试确认通过**

```
cargo test --manifest-path src-tauri/Cargo.toml save_reading -- --nocapture
npx vitest run --maxWorkers=1 --fileParallelism=false src/outline src/desktopClient.test.ts
```

Expected: PASS。

- [ ] **Step 5: Commit**

```
git add src-tauri/src/paper_module.rs src-tauri/src/v2_workspace.rs src/types.ts src/App.tsx src/outline src/desktopClient.ts src/desktopClient.test.ts
git commit -m "feat(outline): persist inspector width and stop auto PDF jump"
```

---

### Task 5: 文档 + 全量验收

**Files:**

- Modify: `docs/full-outline-v1.md`
- Modify: `docs/decisions.md`
- Modify: `docs/handoff.md`
- Modify: `docs/implementation-plan.md`（若仍写「节点不可拖 / 固定 220px」，改成现状）
- Create: `docs/superpowers/plans/2026-08-17-outline-fork-canvas-ux.md`（把本 plan 复制进仓库）

D-034：论证地图按并行结构分叉；旧 Outline 不兼容。`outline_epoch=3` 清空 Outline 修订和 Outline Job；三份 Outline 提示词强制回出厂稿。点选不再自动跳 PDF。协议 v3。

`full-outline-v1.md`：§5.2 并行处兄弟并汇合；§6 协议 v3；补画布壳。删「节点不可拖」。D-031 / D-033 加「已被 D-034 补充」。

- [ ] **Step 1: 改文档**

- [ ] **Step 2: 全量门闩**

```
npx vitest run --maxWorkers=1 --fileParallelism=false
npx tsc -b
cargo test --manifest-path src-tauri/Cargo.toml outline_epoch -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml outline_prompt_generation -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml prompt_settings -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml save_reading -- --nocapture
```

Expected: Vitest 全绿；`tsc` 通过；所列 cargo 通过。

- [ ] **Step 3: Commit**

```
git add docs
git commit -m "docs: record outline fork canvas and epoch wipe as D-034"
```

---

## Self-review

| 锁定项 | 任务 |
| --- | --- |
| 分叉提示词、不禁止单链、不设门闩 | Task 1 |
| 一次性清空 Outline + 强制三槽出厂稿、不自动入队 | Task 1 |
| 协议只留 v3 | Task 1 |
| 加大同层间距、TB、细标签 | Task 2 |
| 可拖、不持久化坐标、Reset 回 dagre | Task 3 |
| 紧凑卡片、无展开、无四色 | Task 3 |
| 详情可收起可拖 | Task 3 |
| 单击不跳 PDF；跳到原文在详情 | Task 3 + 4 |
| 宽度按论文记住 | Task 4 |
| 小号阅读角标 | Task 3 CSS |
| 文档 | Task 5 |

无 TBD。`toFlowElements` / `clampOutlineInspectorWidth` / `nextOutlineSelection` / `OUTLINE_EPOCH` 名称前后一致。
