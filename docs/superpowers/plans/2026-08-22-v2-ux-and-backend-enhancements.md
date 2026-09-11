# Read Desktop V2 用户体验与后端深度强化实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现 Read Desktop V2 的 6 项深度体验与后端加固闭环：OCR 级联安全删除、Lens QA 追问建议填入、标签双向同步与 Takeaway 布局重排、Metadata 内联编辑与 📌 自动钉住、长对话 15 轮分批加载与时间线联动、Compaction 上下文压缩状态指示与对话流分界线。

**Architecture:** 
- 后端 Rust（`src-tauri`）：事务级级联清理 OCR、Orientation Pack 钉住字段防覆写、标签持久化与 Compaction 事件广播。
- 前端 React 19（`src`）：Liquid Glass 拟态组件、受控内联编辑与图钉、长对话 DOM 窗口化切片、二次确认模态窗。

**Tech Stack:** Tauri 2, React 19, TypeScript, Rust, SQLite (rusqlite), Vitest, Tailwind/CSS Custom Properties.

**Spec:** [docs/superpowers/specs/2026-08-22-v2-ux-and-backend-enhancements-design.md](../specs/2026-08-22-v2-ux-and-backend-enhancements-design.md)

## Global Constraints

- **Vitest 串行约束**：前端测试必须执行 `npx vitest run --maxWorkers=1 --fileParallelism=false`，禁止多 worker 并行以防 jsdom OOM。
- **数据保留准则**：删除 OCR 时必须级联清理翻译、解释、Lens、Lens QA 和旁批，但**绝对不可删除**原始 PDF、Chat 讨论历史（含不可变引用快照）、Brief 与 Token 计费审计。
- **设计风格一致性**：所有新增 UI 必须严格遵循 Liquid Glass 拟态规范（`backdrop-filter`, `--liquid-glass-*`）与 Claude Warm Editorial 暖色纸张系统，杜绝刺眼纯白和无边框生硬元素。

---

### Task 1: Backend - OCR 级联删除命令 `delete_ocr_cascade` 与单元测试

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/artifact_commands.rs`
- Modify: `src-tauri/src/reading_artifact_module.rs`
- Test: `src-tauri/src/reading_artifact_module.rs` (内置测试)

**Interfaces:**
- Produces: `delete_ocr_cascade(revision_id: String) -> AppResult<bool>`
- Deletes in transaction:
  - `lens_qa` for all lens artifacts in revision
  - `artifacts` where `revision_id = ?1` and `kind IN ('translation', 'explanation', 'lens_formula', 'lens_figure', 'lens_table', 'reading_guide')`
  - `ocr_revisions`, `ocr_pages`, `ocr_blocks` for revision

- [ ] **Step 1: Write unit tests in Rust verifying cascade deletion**
- [ ] **Step 2: Run `cargo test --locked test_delete_ocr_cascade` to verify failure**
- [ ] **Step 3: Implement `delete_ocr_cascade` in `reading_artifact_module.rs` and expose in `lib.rs`**
- [ ] **Step 4: Run `cargo test --locked` to verify pass**

---

### Task 2: Backend - 标签管理 `update_paper_tags` 与 Metadata 图钉锁定 `update_paper_metadata`

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/v2_workspace.rs`
- Modify: `src-tauri/src/paper_module.rs`
- Test: `src-tauri/src/paper_module.rs` (内置测试)

**Interfaces:**
- Produces: `update_paper_tags(paper_id: String, tags: Vec<String>) -> AppResult<()>`
- Produces: `update_paper_metadata(revision_id: String, metadata: Value, pinned_fields: Vec<String>) -> AppResult<()>`
- Modifies: `run_orientation_generation` to skip updating fields in `pinned_fields_json`

- [ ] **Step 1: Write unit test in Rust for metadata pinning protection during orientation generation**
- [ ] **Step 2: Run `cargo test --locked test_metadata_pinned_protection` to verify failure**
- [ ] **Step 3: Implement `update_paper_tags`, `update_paper_metadata`, and orientation pack protection logic**
- [ ] **Step 4: Run `cargo test --locked` to verify pass**

---

### Task 3: Backend - Compaction 运行时事件广播

**Files:**
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Emits: `discussion_compaction_started` with payload `{ threadId: String, paperId: String }`
- Emits: `discussion_compaction_finished` with payload `{ threadId: String, paperId: String }`

- [ ] **Step 1: In `run_discussion_generation`, emit Tauri events before and after `compact_discussion`**
- [ ] **Step 2: Run `cargo check --locked` to verify compilation**

---

### Task 4: Frontend - OCR 级联删除二次确认模态窗与顶栏垃圾桶按钮

**Files:**
- Create: `src/components/CascadeOcrDeleteConfirmModal.tsx`
- Create: `src/components/CascadeOcrDeleteConfirmModal.test.tsx`
- Modify: `src/desktopClient.ts`
- Modify: `src/App.tsx`
- Modify: `src/styles.css`

**Interfaces:**
- Consumes: `delete_ocr_cascade`
- Produces: `CascadeOcrDeleteConfirmModal` component

- [ ] **Step 1: Write test for `CascadeOcrDeleteConfirmModal` checking list of affected items and confirm action**
- [ ] **Step 2: Run test with vitest to verify failure**
- [ ] **Step 3: Implement `CascadeOcrDeleteConfirmModal.tsx` and integrate into `App.tsx` next to `✓ OCR 就绪`**
- [ ] **Step 4: Run `npx vitest run --maxWorkers=1 --fileParallelism=false src/components/CascadeOcrDeleteConfirmModal.test.tsx` to verify pass**

---

### Task 5: Frontend - Lens QA Suggested Questions 快捷填入

**Files:**
- Modify: `src/ArtifactPanel.tsx`
- Modify: `src/ArtifactPanel.test.tsx`
- Modify: `src/styles.css`

**Interfaces:**
- Reads: `currentArtifact.content.suggestedQuestions`
- Action: Populates `question` state and focuses textarea

- [ ] **Step 1: Add test in `ArtifactPanel.test.tsx` for clicking a suggested question chip**
- [ ] **Step 2: Run test to verify failure**
- [ ] **Step 3: Render suggested question chips above the composer in `ArtifactPanel.tsx`**
- [ ] **Step 4: Run `npx vitest run --maxWorkers=1 --fileParallelism=false src/ArtifactPanel.test.tsx` to verify pass**

---

### Task 6: Frontend - Brief 标签移至 Takeaway 下方 & Hub 标签双向增删与一键筛选

**Files:**
- Modify: `src/ArtifactPanel.tsx`
- Modify: `src/components/LibraryHub.tsx`
- Modify: `src/components/LibraryHub.test.tsx`
- Modify: `src/styles.css`

**Interfaces:**
- Consumes: `update_paper_tags`

- [ ] **Step 1: Write tests in `LibraryHub.test.tsx` and `ArtifactPanel.test.tsx` for adding/removing tags and clicking tag to filter**
- [ ] **Step 2: Run tests to verify failure**
- [ ] **Step 3: Move tags below Takeaway in `ArtifactPanel.tsx` with `×` remove and `+ 添加` input; add tag deletion and tag click filter in `LibraryHub.tsx`**
- [ ] **Step 4: Run vitest tests to verify pass**

---

### Task 7: Frontend - Metadata 就地 ✎ 内联编辑与 📌 自动/手动钉住

**Files:**
- Modify: `src/ArtifactPanel.tsx`
- Modify: `src/ArtifactPanel.test.tsx`
- Modify: `src/styles.css`

**Interfaces:**
- Consumes: `update_paper_metadata`

- [ ] **Step 1: Write tests in `ArtifactPanel.test.tsx` for editing metadata fields and toggling pin state**
- [ ] **Step 2: Run test to verify failure**
- [ ] **Step 3: Implement inline field editing and 📌 icon buttons with auto-pin on save**
- [ ] **Step 4: Run vitest to verify pass**

---

### Task 8: Frontend - 长对话分批加载 (15 轮 Progressive Loading) 与 Timeline 刻度轴联动

**Files:**
- Modify: `src/discussionTree.ts`
- Modify: `src/discussionTree.test.ts`
- Modify: `src/App.tsx`
- Modify: `src/components/ChatTimelineNavigator.tsx`
- Modify: `src/styles.css`

**Interfaces:**
- State: `visibleTurnLimit` (default 15)
- UI: `↑ 加载更早的 15 轮研讨 (剩余 M 轮)` button & scroll top listener

- [ ] **Step 1: Write tests in `discussionTree.test.ts` for slicing visible turns by limit**
- [ ] **Step 2: Run test to verify failure**
- [ ] **Step 3: Implement progressive turn rendering, top load button, and Timeline jump integration in `App.tsx`**
- [ ] **Step 4: Run vitest to verify pass**

---

### Task 9: Frontend - Compaction 呼吸提示与消息流分界线

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/styles.css`

**Interfaces:**
- Listens to: `discussion_compaction_started`, `discussion_compaction_finished`
- Renders: Compaction in-flight pulse badge & `── 📦 上下文已压缩 ──` divider in chat stream

- [ ] **Step 1: Add compaction state listener and divider component in `App.tsx`**
- [ ] **Step 2: Add styling in `src/styles.css`**
- [ ] **Step 3: Verify with unit tests**

---

### Task 10: 全面集成与回归验证

- [ ] **Step 1: Run serial Vitest test suite (`npx vitest run --maxWorkers=1 --fileParallelism=false`)**
- [ ] **Step 2: Run TypeScript compiler check (`npx tsc -b`)**
- [ ] **Step 3: Run Rust compiler & test suite (`cd src-tauri; cargo check --locked; cargo test --locked`)**
- [ ] **Step 4: Run web build (`npm run build`)**
