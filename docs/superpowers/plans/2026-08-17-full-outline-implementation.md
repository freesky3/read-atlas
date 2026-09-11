# Full Outline V1 实施计划

> **For agentic workers:** 按任务顺序做。每个 Task 先写失败测试再写实现。Vitest 必须串行。改 Rust 命令后重启 `tauri dev`。不要迁入 `read_addon` 的 Dexie / MV3 / 消息总线。

**Goal:** 在 Read Desktop 落地深入阅读论证地图：OCR 门槛、论文根旁支、Overview 两 Unit 生成、Deep dive 局部关系图、`PDF | 地图` 预设、单击跳 Block。

**Architecture:**

1. 新深模块 `OutlineModule`（SQLite head / revision）+ 复用 `JobModule`。
2. Worker 注册 `outline_overview`（抽单元 checkpoint → 构图）和 `outline_deep_dive`（一次结构化调用）。
3. 前端经 `DesktopClient` 打开地图表面；画布 `@xyflow/react` + `dagre` lazy chunk。
4. 同一产品期含两层；对内先合 Overview 真闭环，再接 Deep dive。

**Tech Stack:** Tauri 2, Rust, SQLite, Gemini Interactions, React 19, TypeScript, Vitest, `@xyflow/react`, `@dagrejs/dagre`.

**Spec:** [docs/full-outline-v1.md](../../full-outline-v1.md)

## Global Constraints

- Vitest：`npx vitest run --maxWorkers=1 --fileParallelism=false`。
- Tauri 2 结构体命令：`invoke(name, { request: { ...camelCase } })`。
- 视口锁定：`100vh` / `overflow: hidden`；PDF 与地图栏各自滚动。
- `zoomHotkeysEnabled` 保持 `false`。不要动 `workspace.lock` 的 `File`。
- 未知 usage 保持 `null`。禁止静默付费。禁止 OpenAI `json_schema` type。
- 样式只用 `--glass-*` / `--liquid-glass-*` 变量。
- 产品第一期必须能点开局部关系图；禁止用精读卡冒充 Deep dive。
- 提交前：串行 Vitest、`npx tsc -b`、`cargo test --locked`、`cargo check --locked`、`git diff --check`。

---

### Task 0: 合同已写入（本文档配套）

**Files:** `docs/full-outline-v1.md` 及索引 / ADR / onboarding 同步。

- [x] 产品合同与「V2 不做 Outline」的旧表述已替换为授权后的 V1 合同。
- [ ] 实现 agent 开工前重读 `docs/full-outline-v1.md`，不要只读本计划。

---

### Task 1: OutlineModule 表、投影与瘦目录（零付费）

**Files:**

- Create: `src-tauri/src/outline_module.rs`
- Create: `src-tauri/src/outline_catalog.rs`
- Create: `src-tauri/src/outline_catalog.rs` 的测试（可同文件 `#[cfg(test)]`）
- Modify: `src-tauri/src/v2_workspace.rs`（建表）
- Modify: `src-tauri/src/lib.rs`（`mod outline_module`）

**Interfaces:**

```text
OutlineCatalogEntry { id, page, type, blockIndex, bbox, excerpt?, nearbyCaptionBlockId? }
build_outline_catalog(ocr_revision) -> { entries, digest, tokenEstimate }
OutlineProjection { status: missing_ocr | ready_to_plan | planned | generating | partial | published | stale
                    head?, activeJob?, coverageWarnings[] }
```

- [x] **Step 1:** 写 `build_outline_catalog` 测试：chrome 剔除、正文 excerpt 截断、公式优先 LaTeX、无 caption 的 figure 只有定位行、同页题注挂 `nearbyCaptionBlockId`。
- [ ] **Step 2:** `cargo test --locked outline_catalog` 预期失败。
- [ ] **Step 3:** 建 `outline_revisions` / `outline_heads` / `outline_deep_dive_heads`（及可选 `outline_plans`）。删除 OCR / Paper 时 cascade 与现有 OCR 依赖族一致。
- [ ] **Step 4:** 实现 catalog builder + 空 `get_outline` 投影（无 OCR → `missing_ocr`）。
- [ ] **Step 5:** `cargo test --locked` 相关用例通过。

**Done when:** 不调用 Gemini 也能对一篇已 OCR 论文给出稳定目录 digest；无 OCR 投影可测。

---

### Task 2: DesktopClient 命令与计划卡（仍不花钱）

**Files:**

- Modify: `src/desktopClient.ts`, `src/types.ts`
- Modify: `src/desktopClient.test.ts`
- Modify: `src-tauri/src/lib.rs`（`plan_outline` / `get_outline`）
- Create: `src/outline/outlinePlan.ts` + `src/outline/outlinePlan.test.ts`

**Commands:**

```text
get_outline(revisionId)
plan_outline({ revisionId })           // 本地估算，不入队
```

`plan_outline` 必须失败于：无 OCR、模型不支持原生 PDF、目录+PDF 超窗口。成功返回调用次数（2+1 repair）、模型、ocrRevisionId、catalogDigest、粗费用（未知则 null）。

- [ ] **Step 1:** 锁 `plan_outline` invoke 形状为 `{ request: { revisionId } }`。
- [ ] **Step 2:** 前端纯函数测试：把 plan 投影打成计划卡文案；缺 OCR / 超窗口 / 缺论文根三种空态。
- [ ] **Step 3:** 实现命令。确认路径不 `enqueue`。
- [ ] **Step 4:** 串行 Vitest + `cargo test` 通过。

**Done when:** 计划卡所需数据全部来自本地，测试证明零 Job。

---

### Task 3: 阅读壳预设 `pdf_discussion` | `pdf_outline` | `outline_only`

**Files:**

- Modify: `src/App.tsx`, `src/styles.css`
- Create: `src/outline/workspaceLayout.ts` + test
- Modify: `src-tauri` reading_states（新增 `workspace_layout`, `active_outline_node_id`, `outline_view`）
- Modify: `tests/viewNavigation.test.tsx`
- Create: `src/outline/OutlinePane.tsx`（先空态/计划卡，无画布）
- Modify: 顶岛：与「速览 Brief」并列的「地图」按钮

**Rules:**

- 点「地图」→ `pdf_outline`（PDF 留着，右侧换地图栏）。不要第三永久标签。
- 默认打开论文仍是 `pdf_discussion`。
- `outline_only` 仅用户再选。
- Esc：弹层 → 取消 Outline 节点选中 → 回大厅。一次 Esc 不能两件事。
- 成果索引可放一行 Outline，调用同一 `openOutline()`。

- [ ] **Step 1:** 写 layout reducer / 持久化测试。
- [ ] **Step 2:** 写顶岛按钮与空态（缺 OCR、计划卡）组件测试。
- [ ] **Step 3:** 接 `get_outline` / `plan_outline`。生成按钮此 Task **disabled** 或只调用 plan（不要 start）。
- [ ] **Step 4:** 串行 Vitest 通过；`tsc -b` 通过。

**Done when:** 不花钱也能打开地图栏并看到正确空态；讨论布局可来回切。

---

### Task 4: Overview Job（抽单元 → checkpoint → 构图）

**Files:**

- Create: `src-tauri/src/outline_protocol.rs`（schema、parse、validate）
- Create: `src-tauri/src/outline_validate.rs`
- Modify: `src-tauri/src/lib.rs`（`start_outline`, `execute_outline_overview_job`）
- Modify: `src-tauri/src/provider_ports.rs`（仅复用 `gemini_response_format` / paper root 旁支）
- Modify: `src/outline/OutlinePane.tsx`（阶段卡）
- Test: `outline_validate`、`structured_output_uses_interactions_text_json_*` 同类、恢复不重抽

**Job:** `kind = outline_overview`  
`dedupe_key = outline-overview:{revisionId}:{ocrRevisionId}:outline-extract-v1+outline-compose-v1`

流水线按规格 §6.1。旁支：完整 PDF + 目录 + 只读 Brief/术语；不读 Chat。

- [ ] **Step 1:** 校验测试：目录外 ID 丢弃、narrative 环 → 拒绝图、单元有效图无效 → `partialArguments`、空 repair 拒绝、角色误填 relationClass → `other`。
- [ ] **Step 2:** 测试 `gemini_response_format` 用于 extract/compose schema。
- [ ] **Step 3:** 测试 checkpoint：构图失败再 resume 不第二次抽单元（可用 fixture / fake port）。
- [ ] **Step 4:** 实现 `start_outline` + worker。取消不切 head。
- [ ] **Step 5:** UI 阶段卡：`extracting` / `composing` / `repairing` / `partial` / `published`。禁止 provisional 图。
- [ ] **Step 6:** `cargo test --locked` + 串行 Vitest。

**Done when:** 假 port 能走通发布与 partial；head 语义可单测。此 Task 合入后 Overview 可演示（接真实 Gemini 需重启 tauri）。

**Merge gate（对内）:** Overview 真闭环可进主线。Deep dive 按钮可显示但未点生成前不入队。

---

### Task 5: React Flow 画布 + 单击跳 PDF

**Files:**

- Create: `src/outline/OutlineCanvas.tsx`
- Create: `src/outline/outlineLayout.ts` + worker `src/outline/outlineLayout.worker.ts`（可选，图小可先主线程）
- Create: `src/outline/OutlineCanvas.test.tsx`
- Modify: `src/PdfReader.tsx` / `src/App.tsx`（高亮一组 evidence blockIds，跳主证据）
- Modify: `vite.config.ts` / 动态 `import()` 保证不进首屏静态图
- Modify: `package.json`（`@xyflow/react`, `@dagrejs/dagre`）

- [ ] **Step 1:** 安装依赖。确认 sidepanel/首屏 bundle 不静态 import 画布。
- [ ] **Step 2:** 布局测试：只按 narrative 分层；cross_link 不改变 rank。
- [ ] **Step 3:** 单击节点测试：选出 nodeId、请求 `page + bbox` 的主证据、把它的全部 evidenceIds 标 focused。
- [ ] **Step 4:** ErrorBoundary 测试：布局抛错渲染列表，不卸掉 Reader。
- [ ] **Step 5:** inspector 展示 takeaway、角色、证据药丸；Deep dive 按钮此时可先 `disabled` 并标明下一 Task。
- [ ] **Step 6:** 串行 Vitest + `tsc -b` + `npm run build`。

**Done when:** 已发布 Overview 可画、可点、可跳；画布崩溃不空白。

---

### Task 6: Deep dive 局部图（同一里程碑）

**Files:**

- Modify: `src-tauri/src/outline_module.rs`, `outline_catalog.rs`, `outline_protocol.rs`, `lib.rs`
- Modify: `src/outline/OutlinePane.tsx`, `OutlineCanvas.tsx`
- Create: `src/outline/deepDiveScope.ts` + test
- Modify: `src/desktopClient.ts`（`start_outline_deep_dive`）

**Job:** `kind = outline_deep_dive`  
`dedupe_key = outline-deep-dive:{overviewRevisionId}:{nodeId}`

- 局部白名单 = 节点证据页 ∪ 一跳 narrative 邻居页 ±1。
- 一次结构化调用 + 最多一次 repair。
- 无 Overview head → 命令拒绝。
- 缓存命中不入队。
- 成功只写 `outline_deep_dive_heads`。
- UI：inspector「生成局部图」才 `start`；打开后地图栏换局部图 + 面包屑回 Overview。
- 局部图单击同样跳 PDF。点局部节点**禁止**再入队 Outline。

- [ ] **Step 1:** `deepDiveScope` 测试：页集合、±1、不整篇重寄、邻居 takeaway 进入上下文但不扩大白名单到邻居未覆盖页之外（白名单按页公式，以规格为准）。
- [ ] **Step 2:** 命令测试：无 head 拒绝；重复 start 合并；失败不改 Overview head。
- [ ] **Step 3:** 实现 worker + UI 面包屑。
- [ ] **Step 4:** 串行 Vitest + `cargo test --locked`。

**Done when:** 用户能从粗节点生成并阅读内部关系图，再回到总图。没有精读卡冒充。

---

### Task 7: 版本、过期、删除、任务中心

**Files:**

- Modify: `outline_module.rs`, OCR 删除路径, `OperationsDrawer.tsx`
- Test: 重 OCR 后投影为 `stale` / 新 head 空；旧 revision 只读可 `get_outline({ outlineRevisionId })`；旧 Deep dive 不出现在新 head。
- 删除 OCR 预览列出 Outline。
- Job 出现在现有任务中心；暂停/取消/托盘规则复用，不新写退出栈。

- [ ] **Step 1:** 过期与 cascade 测试。
- [ ] **Step 2:** 实现。禁止自动 `start_outline`。
- [ ] **Step 3:** Operations Drawer 能看到 `outline_overview` / `outline_deep_dive` 阶段。

**Done when:** 重 OCR / 删 OCR / 取消 Job 的语义与规格 §7 一致。

---

### Task 8: 收口测试与文档

**Files:** `docs/handoff.md`, `docs/agent-onboarding.md` 验证数字, 本计划复选框。

- [ ] 串行全量 Vitest。
- [ ] `npx tsc -b`、`npm run build`。
- [ ] `cargo test --locked`、`cargo check --locked`（既有 unused 警告不借机大删）。
- [ ] `git diff --check`。
- [ ] 更新 onboarding：文件入口、Esc 分层、Outline 命令需重启 tauri。
- [ ] Live Gemini 不在默认 CI；只在显式付费开关下手工跑一篇已 OCR 短文。

**Done when:** 自动门槛全绿；规格中的验收表可在假 port 上全部勾掉。

---

## 建议合入顺序（同一功能发布）

```text
Task 1–2  数据与计划（可先合）
Task 3    壳预设 + 空态（可先合）
Task 4    Overview Job     ← 对内第一个演示门闩
Task 5    画布 + 跳转      ← Overview 真闭环
Task 6    Deep dive        ← 产品期完成条件
Task 7–8  版本与收口
```

不要等 Task 6 才开始合 Task 4。不要把 Task 3 做成通用窗口管理器。

## 非目标（做的时候若伸手就停）

搜索、Semantic View、Reading Guide、Author Research、OpenAI/xAI、三栏拖拽、Outline 专用模型设置页、自动生成、旧边迁移、插件数据导入。
