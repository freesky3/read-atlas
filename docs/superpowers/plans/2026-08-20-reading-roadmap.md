# Reading Roadmap Implementation Plan

> 2026-09-09：本页保留历史设计。当前目标、完整提示词、字段和来源根接线以 [导师式精读路线生成合同](../../reading-roadmap-generation.md) 为准；原从零掌握、固定图表／电梯稿、第三遍仅按需、根调用等描述已调整。

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a "Reading Roadmap" (精读路线) feature that generates a paper-specific, Three-Pass guided reading checklist with AI, presented in a floating left-side panel with clickable evidence pills and persistent checkbox progress.

**Architecture:** New artifact kind `reading_roadmap` stores the AI-generated pass structure. A new `roadmap_progress` SQLite table persists per-task checkbox state. The frontend adds a floating overlay panel triggered from a top island button, rendering Pass 0–3 tasks with collapsible sections, evidence pills that jump the PDF reader, and progress indicators.

**Tech Stack:** Tauri 2 / Rust (backend), React 19 / TypeScript (frontend), SQLite, Vitest + jsdom (testing)

**Spec:** [`docs/superpowers/specs/2026-08-20-reading-roadmap-design.md`](../specs/2026-08-20-reading-roadmap-design.md)

## Global Constraints

- Vitest must run with `--maxWorkers=1 --fileParallelism=false` to avoid jsdom OOM.
- All Tauri commands use `{ request: { ...fields } }` wrapping (D-019 pattern).
- LaTeX in text fields must pass through `preprocessLaTeX` before KaTeX rendering.
- New CSS must use `var(--app-font, inherit)` and respond to all three themes (`liquid-light`, `liquid-dark`, `warm-editorial`).
- Prompt slot text containing `{output_language}` must have it validated. Reading Roadmap prompt does not require `{output_language}`.
- `PromptSlotId::all()` array and `PromptSlotId::parse()` must stay in sync.
- Frontend `PromptSlotId` union type in `types.ts` must stay in sync with Rust enum.

---

### Task 1: TypeScript Types & Data Contract

**Files:**
- Modify: `src/types.ts`
- Test: `src/ReadingRoadmap.test.tsx` (created in Task 4, types tested implicitly)

**Interfaces:**
- Produces: `RoadmapEvidencePill`, `RoadmapTask`, `RoadmapPass`, `ReadingRoadmapContent`, `ReadingRoadmapProjection`, `RoadmapProgressEntry` — used by all subsequent frontend tasks.

- [ ] **Step 1: Add Reading Roadmap types to `src/types.ts`**

Append after the existing `PromptSettings` type (line ~524):

```typescript
/* ── Reading Roadmap (精读路线) ── */

export type RoadmapEvidencePill = {
  label: string;
  page: number;
  blockId?: string;
  bbox?: [number, number, number, number];
};

export type RoadmapTask = {
  id: string;
  text: string;
  timeMinutes: number;
  required: boolean;
  completionCriteria: string;
  selfCheckQuestions: string[];
  evidence: RoadmapEvidencePill[];
};

export type RoadmapPass = {
  passNumber: 0 | 1 | 2 | 3;
  title: string;
  subtitle: string;
  timeBudget: string;
  exitCriteria: string;
  tasks: RoadmapTask[];
};

export type ReadingRoadmapContent = {
  version: number;
  generatedAt: string;
  paperTitle: string;
  passes: RoadmapPass[];
  elevatorPitch: string;
  oneChart: {
    label: string;
    page: number;
    blockId?: string;
    reason: string;
  };
};

export type ReadingRoadmapProjection = {
  id: string;
  paperId: string;
  revisionId: string;
  status: "generating" | "published" | "failed";
  content: ReadingRoadmapContent | null;
  activeJobId: string | null;
  lastError: string | null;
  createdAt: string;
};

export type RoadmapProgressEntry = {
  taskId: string;
  completed: boolean;
  completedAt: string | null;
};
```

- [ ] **Step 2: Add `reading_roadmap` to the PromptSlotId union**

In `src/types.ts`, find the `PromptSlotId` type (line ~495) and add `"reading_roadmap"`:

```typescript
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
  | "outline_deep_dive"
  | "reading_roadmap";
```

- [ ] **Step 3: Verify types compile**

Run: `npx tsc -b --noEmit`
Expected: PASS with zero errors

- [ ] **Step 4: Commit**

```bash
git add src/types.ts
git commit -m "feat(roadmap): add Reading Roadmap TypeScript type definitions"
```

---

### Task 2: Rust Backend — Prompt Slot & Roadmap Module

**Files:**
- Modify: `src-tauri/src/prompt_settings.rs`
- Create: `src-tauri/src/roadmap_module.rs`
- Modify: `src-tauri/src/lib.rs` (register module + commands)

**Interfaces:**
- Consumes: Nothing (self-contained backend)
- Produces: `PromptSlotId::ReadingRoadmap`, `roadmap_module::create_tables()`, `roadmap_module::get_roadmap()`, `roadmap_module::set_task_progress()`, `roadmap_module::list_progress()`, `roadmap_module::publish_roadmap()` — used by lib.rs IPC commands and Task 3.

- [ ] **Step 1: Add `ReadingRoadmap` variant to Rust `PromptSlotId` enum**

In `src-tauri/src/prompt_settings.rs`, add the variant to `enum PromptSlotId` (after `OutlineDeepDive`, line ~29):

```rust
    OutlineDeepDive,
    ReadingRoadmap,
```

Add to `as_str` match (after `OutlineDeepDive` arm, line ~50):

```rust
            Self::ReadingRoadmap => "reading_roadmap",
```

Add to `all()` array (after `Self::OutlineDeepDive`, line ~84):

```rust
            Self::ReadingRoadmap,
```

Add to `default_text` match (after `OutlineDeepDive` arm, line ~176):

```rust
        PromptSlotId::ReadingRoadmap => {
            "你是论文精读教练，不是摘要机器人。\n\n\
            任务：为下面这篇论文生成一份「从零掌握」的 Todo-list。\n\
            要求：\n\
            1. 以 Keshav 的 Three-Pass Approach 为骨架，增加 Pass 0（前置对齐）。\n\
            2. 不要写成章节大纲。每个任务必须是可执行动作，并绑定这篇论文的具体内容（图号、表号、算法、假设、符号、关键实验）。\n\
            3. 面向读者从零开始。先判断这篇论文默认的背景知识，给出最小补课清单。\n\
            4. 每一遍都包含：时间预算、必做任务、完成标准、停止/继续决策。\n\
            5. 每个任务后附 1–3 个「读完必须能回答」的问题。问题要针对这篇论文，不要泛泛而谈。\n\
            6. Pass 1 不超过 8 项，Pass 2 不超过 12 项。\n\
            7. 区分「必做」和「选做」。\n\
            8. 语言像教练布置作业，不像在写综述。\n\
            9. 「图表优先」：第二遍的核心任务围绕 2–3 张最关键的图表展开。\n\
            10. 先输出论文负荷点抽取（核心主张、新方法步骤、关键图表、脆弱假设、读者卡点预测），再基于抽取生成 Todo-list。\n\
            11. 最后给一个 30 秒电梯稿模板，以及「如果只读一张图，该读哪张、为什么」。\n\
            12. 每个 Pass 都给退出条件（硬性停止点）。\n\n\
            默认读者背景：刚接触该领域、具有基本数理知识的大三学生，目标是快速了解。"
        }
```

- [ ] **Step 2: Create `src-tauri/src/roadmap_module.rs`**

```rust
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoadmapProgressRow {
    pub task_id: String,
    pub completed: bool,
    pub completed_at: Option<String>,
}

pub fn create_tables(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS roadmap_progress (
            paper_id    TEXT NOT NULL,
            roadmap_id  TEXT NOT NULL,
            task_id     TEXT NOT NULL,
            completed   INTEGER NOT NULL DEFAULT 0,
            completed_at TEXT,
            PRIMARY KEY (paper_id, roadmap_id, task_id)
        );",
    )?;
    Ok(())
}

pub fn set_task_progress(
    conn: &Connection,
    paper_id: &str,
    roadmap_id: &str,
    task_id: &str,
    completed: bool,
) -> rusqlite::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO roadmap_progress (paper_id, roadmap_id, task_id, completed, completed_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(paper_id, roadmap_id, task_id)
         DO UPDATE SET completed = ?4, completed_at = ?5",
        params![
            paper_id,
            roadmap_id,
            task_id,
            completed as i32,
            if completed { Some(&now) } else { None::<&str> },
        ],
    )?;
    Ok(())
}

pub fn list_progress(
    conn: &Connection,
    paper_id: &str,
    roadmap_id: &str,
) -> rusqlite::Result<Vec<RoadmapProgressRow>> {
    let mut stmt = conn.prepare(
        "SELECT task_id, completed, completed_at
         FROM roadmap_progress
         WHERE paper_id = ?1 AND roadmap_id = ?2",
    )?;
    let rows = stmt.query_map(params![paper_id, roadmap_id], |row| {
        Ok(RoadmapProgressRow {
            task_id: row.get(0)?,
            completed: row.get::<_, i32>(1)? != 0,
            completed_at: row.get(2)?,
        })
    })?;
    rows.collect()
}

pub fn clear_progress(
    conn: &Connection,
    paper_id: &str,
    roadmap_id: &str,
) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM roadmap_progress WHERE paper_id = ?1 AND roadmap_id = ?2",
        params![paper_id, roadmap_id],
    )
}
```

- [ ] **Step 3: Register module and IPC commands in `lib.rs`**

Add `mod roadmap_module;` near the top of `src-tauri/src/lib.rs` with the other module declarations.

Add `roadmap_module::create_tables(&conn)?;` in the database initialization section (alongside existing `create_tables` calls).

Add four Tauri commands (following existing patterns — `{ request: { ... } }` wrapping):

```rust
#[tauri::command]
async fn get_roadmap(
    state: tauri::State<'_, AppState>,
    request: GetRoadmapRequest,
) -> Result<Option<serde_json::Value>, String> {
    // Query artifact with kind="reading_roadmap" for the given revision
    // Return the artifact content + metadata as ReadingRoadmapProjection
}

#[tauri::command]
async fn toggle_roadmap_task(
    state: tauri::State<'_, AppState>,
    request: ToggleRoadmapTaskRequest,
) -> Result<(), String> {
    // Call roadmap_module::set_task_progress
}

#[tauri::command]
async fn list_roadmap_progress(
    state: tauri::State<'_, AppState>,
    request: ListRoadmapProgressRequest,
) -> Result<Vec<roadmap_module::RoadmapProgressRow>, String> {
    // Call roadmap_module::list_progress
}

#[tauri::command]
async fn start_roadmap_job(
    state: tauri::State<'_, AppState>,
    request: StartRoadmapJobRequest,
) -> Result<serde_json::Value, String> {
    // Enqueue a job with kind="reading_roadmap"
    // Check Orientation Pack exists, otherwise error
    // Create paper root branch for roadmap generation
}
```

Register all four in the `generate_handler!` macro call.

- [ ] **Step 4: Run Rust tests**

Run: `cd src-tauri && cargo test --locked`
Expected: All existing tests pass + new module compiles

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/prompt_settings.rs src-tauri/src/roadmap_module.rs src-tauri/src/lib.rs
git commit -m "feat(roadmap): add Reading Roadmap Rust backend module, prompt slot, and IPC commands"
```

---

### Task 3: Frontend — ReadingRoadmap Floating Panel Component

**Files:**
- Create: `src/ReadingRoadmap.tsx`
- Create: `src/ReadingRoadmap.test.tsx`

**Interfaces:**
- Consumes: `ReadingRoadmapProjection`, `RoadmapProgressEntry`, `RoadmapTask`, `RoadmapPass` from `types.ts`
- Produces: `<ReadingRoadmapPanel>` component with props:
  ```typescript
  {
    open: boolean;
    projection: ReadingRoadmapProjection | null;
    progress: RoadmapProgressEntry[];
    activeJobId: string | null;
    onClose: () => void;
    onToggleTask: (taskId: string, completed: boolean) => void;
    onGenerate: () => void;
    onJumpToPage: (page: number, blockId?: string) => void;
  }
  ```

- [ ] **Step 1: Write failing tests in `src/ReadingRoadmap.test.tsx`**

```typescript
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { ReadingRoadmapPanel } from "./ReadingRoadmap";
import type { ReadingRoadmapProjection, RoadmapProgressEntry } from "./types";

const mockProjection: ReadingRoadmapProjection = {
  id: "roadmap-1",
  paperId: "paper-1",
  revisionId: "rev-1",
  status: "published",
  activeJobId: null,
  lastError: null,
  createdAt: "2026-08-20T00:00:00Z",
  content: {
    version: 1,
    generatedAt: "2026-08-20T00:00:00Z",
    paperTitle: "Test Paper",
    passes: [
      {
        passNumber: 0,
        title: "前置对齐",
        subtitle: "让读者不在正文里迷路",
        timeBudget: "10–20 分钟",
        exitCriteria: "能用自己的话回答论文在解决什么问题",
        tasks: [
          {
            id: "p0-t1",
            text: "了解 Transformer 的基本概念",
            timeMinutes: 5,
            required: true,
            completionCriteria: "能说出 self-attention 的作用",
            selfCheckQuestions: ["什么是 self-attention？"],
            evidence: [{ label: "p.1 · Abstract", page: 1 }],
          },
        ],
      },
    ],
    elevatorPitch: "这篇论文提出了...",
    oneChart: { label: "Figure 3", page: 7, reason: "展示了核心结果对比" },
  },
};

describe("ReadingRoadmapPanel", () => {
  it("renders nothing when closed", () => {
    const { container } = render(
      <ReadingRoadmapPanel
        open={false}
        projection={mockProjection}
        progress={[]}
        activeJobId={null}
        onClose={vi.fn()}
        onToggleTask={vi.fn()}
        onGenerate={vi.fn()}
        onJumpToPage={vi.fn()}
      />,
    );
    expect(container.querySelector(".reading-roadmap-overlay")).toBeNull();
  });

  it("renders pass titles and tasks when open", () => {
    render(
      <ReadingRoadmapPanel
        open={true}
        projection={mockProjection}
        progress={[]}
        activeJobId={null}
        onClose={vi.fn()}
        onToggleTask={vi.fn()}
        onGenerate={vi.fn()}
        onJumpToPage={vi.fn()}
      />,
    );
    expect(screen.getByText(/前置对齐/)).toBeTruthy();
    expect(screen.getByText(/Transformer/)).toBeTruthy();
  });

  it("calls onToggleTask when checkbox is clicked", () => {
    const onToggle = vi.fn();
    render(
      <ReadingRoadmapPanel
        open={true}
        projection={mockProjection}
        progress={[]}
        activeJobId={null}
        onClose={vi.fn()}
        onToggleTask={onToggle}
        onGenerate={vi.fn()}
        onJumpToPage={vi.fn()}
      />,
    );
    const checkbox = screen.getByRole("checkbox");
    fireEvent.click(checkbox);
    expect(onToggle).toHaveBeenCalledWith("p0-t1", true);
  });

  it("calls onJumpToPage when evidence pill is clicked", () => {
    const onJump = vi.fn();
    render(
      <ReadingRoadmapPanel
        open={true}
        projection={mockProjection}
        progress={[]}
        activeJobId={null}
        onClose={vi.fn()}
        onToggleTask={vi.fn()}
        onGenerate={vi.fn()}
        onJumpToPage={onJump}
      />,
    );
    const pill = screen.getByText(/Abstract/);
    fireEvent.click(pill);
    expect(onJump).toHaveBeenCalledWith(1, undefined);
  });

  it("shows generate button when no projection", () => {
    render(
      <ReadingRoadmapPanel
        open={true}
        projection={null}
        progress={[]}
        activeJobId={null}
        onClose={vi.fn()}
        onToggleTask={vi.fn()}
        onGenerate={vi.fn()}
        onJumpToPage={vi.fn()}
      />,
    );
    expect(screen.getByRole("button", { name: /生成精读路线/ })).toBeTruthy();
  });

  it("marks task as completed from progress", () => {
    const progress: RoadmapProgressEntry[] = [
      { taskId: "p0-t1", completed: true, completedAt: "2026-08-20T01:00:00Z" },
    ];
    render(
      <ReadingRoadmapPanel
        open={true}
        projection={mockProjection}
        progress={progress}
        activeJobId={null}
        onClose={vi.fn()}
        onToggleTask={vi.fn()}
        onGenerate={vi.fn()}
        onJumpToPage={vi.fn()}
      />,
    );
    const checkbox = screen.getByRole("checkbox") as HTMLInputElement;
    expect(checkbox.checked).toBe(true);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npx vitest run src/ReadingRoadmap.test.tsx --maxWorkers=1 --fileParallelism=false`
Expected: FAIL — module `./ReadingRoadmap` not found

- [ ] **Step 3: Implement `src/ReadingRoadmap.tsx`**

Build the `ReadingRoadmapPanel` component with:
- Overlay wrapper (`.reading-roadmap-overlay`) that renders `null` when `open === false`
- Slide-in animation from left side
- Panel header with title, progress summary (`12/28 任务完成`), and close button
- Collapsible `<details>` blocks per Pass (Pass 0–3)
- Each task: checkbox + Markdown text (via `MarkdownBody`) + completion criteria + self-check questions + evidence pills
- Evidence pills: clickable `<button className="evidence-pill-roadmap">` that call `onJumpToPage(pill.page, pill.blockId)`
- Generate state: when `projection === null`, show a large "生成精读路线" button calling `onGenerate`
- Generating state: when `activeJobId !== null`, show a spinner/progress message
- Error state: when `projection?.status === "failed"`, show error + retry button
- Bottom section: elevator pitch card + "只读一张图" card
- Progress calculation from the `progress` prop (count completed / total tasks across all passes)

- [ ] **Step 4: Run tests to verify they pass**

Run: `npx vitest run src/ReadingRoadmap.test.tsx --maxWorkers=1 --fileParallelism=false`
Expected: All 6 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/ReadingRoadmap.tsx src/ReadingRoadmap.test.tsx
git commit -m "feat(roadmap): add ReadingRoadmapPanel floating component with tests"
```

---

### Task 4: CSS — Floating Panel Styling

**Files:**
- Modify: `src/styles.css`

**Interfaces:**
- Consumes: CSS class names from `ReadingRoadmap.tsx` (`.reading-roadmap-overlay`, `.roadmap-panel`, `.roadmap-pass-section`, `.roadmap-task`, `.evidence-pill-roadmap`, etc.)
- Produces: Complete visual styling for the floating panel, responsive to all three themes

- [ ] **Step 1: Add floating panel styles to `src/styles.css`**

Add after the existing Operations Drawer styles. Key classes:

```css
/* ── Reading Roadmap Floating Panel ── */

.reading-roadmap-overlay {
  position: fixed;
  top: 44px; /* below top island */
  left: 0;
  bottom: 0;
  width: 44%;
  z-index: 900;
  transform: translateX(-100%);
  transition: transform 0.3s cubic-bezier(0.4, 0, 0.2, 1);
  pointer-events: none;
}

.reading-roadmap-overlay.open {
  transform: translateX(0);
  pointer-events: auto;
}

.roadmap-panel {
  height: 100%;
  background: var(--liquid-glass-bg);
  backdrop-filter: blur(24px) saturate(180%);
  border-right: 1px solid var(--liquid-glass-border);
  box-shadow: var(--liquid-glass-shadow);
  overflow-y: auto;
  padding: 16px 20px;
  font-family: var(--app-font, inherit);
}

/* ... pass sections, task items, evidence pills, progress bar, etc. */
```

Include:
- `.roadmap-panel-header`: title + progress + close button
- `.roadmap-pass-section`: collapsible pass blocks with progress indicator
- `.roadmap-task`: checkbox row with completion criteria and self-check
- `.roadmap-task.completed`: strikethrough/muted style for completed tasks
- `.evidence-pill-roadmap`: small liquid glass pills matching existing `[p.N]` style
- `.roadmap-bottom-cards`: elevator pitch and one-chart cards
- Theme responsiveness for all three themes

- [ ] **Step 2: Verify build compiles**

Run: `npm run build`
Expected: Build succeeds with zero errors

- [ ] **Step 3: Commit**

```bash
git add src/styles.css
git commit -m "style(roadmap): add Reading Roadmap floating panel liquid glass styles"
```

---

### Task 5: Integration — Wire Panel into App.tsx

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/desktopClient.ts` (if needed for IPC command typing)

**Interfaces:**
- Consumes: `<ReadingRoadmapPanel>` from Task 3, IPC commands from Task 2, types from Task 1
- Produces: Fully working end-to-end feature (top island button + floating panel + checkbox persistence + evidence jump)

- [ ] **Step 1: Add state and import in `App.tsx`**

```typescript
import { ReadingRoadmapPanel } from "./ReadingRoadmap";
import type { ReadingRoadmapProjection, RoadmapProgressEntry } from "./types";

// Inside the main App component, add state:
const [roadmapOpen, setRoadmapOpen] = useState(false);
const [roadmapProjection, setRoadmapProjection] = useState<ReadingRoadmapProjection | null>(null);
const [roadmapProgress, setRoadmapProgress] = useState<RoadmapProgressEntry[]>([]);
```

- [ ] **Step 2: Add top island button**

After the `🗺 地图` button (around line 2444), add:

```tsx
<button
  className={`btn-liquid-pill ${roadmapOpen ? "primary" : ""}`}
  style={{ padding: "4px 10px", fontSize: 11.5 }}
  onClick={() => setRoadmapOpen((prev) => !prev)}
  title="打开精读路线"
>
  📋 精读
</button>
```

- [ ] **Step 3: Add panel mount in JSX**

At the end of the reader view JSX (near the OperationsDrawer mount), add:

```tsx
<ReadingRoadmapPanel
  open={roadmapOpen}
  projection={roadmapProjection}
  progress={roadmapProgress}
  activeJobId={/* derive from jobs list */}
  onClose={() => setRoadmapOpen(false)}
  onToggleTask={async (taskId, completed) => {
    if (!roadmapProjection) return;
    await desktopClient.command("toggle_roadmap_task", {
      request: {
        paperId: selectedDoc!.id,
        roadmapId: roadmapProjection.id,
        taskId,
        completed,
      },
    });
    // Update local state
    setRoadmapProgress((prev) =>
      prev.some((p) => p.taskId === taskId)
        ? prev.map((p) => p.taskId === taskId ? { ...p, completed, completedAt: completed ? new Date().toISOString() : null } : p)
        : [...prev, { taskId, completed, completedAt: completed ? new Date().toISOString() : null }],
    );
  }}
  onGenerate={async () => {
    if (!selectedDoc) return;
    // Check orientation pack, then start job
    await desktopClient.command("start_roadmap_job", {
      request: { revisionId: selectedDoc.revisionId },
    });
    // Refresh projection
  }}
  onJumpToPage={(page, blockId) => {
    setCurrentPage(page);
    if (blockId) setFocusedBlockId(blockId);
  }}
/>
```

- [ ] **Step 4: Load roadmap on paper selection**

In the paper selection handler (where other per-paper data is loaded), add:

```typescript
// After loading artifacts, load roadmap
try {
  const rm = await desktopClient.open<ReadingRoadmapProjection | null>("get_roadmap", {
    revisionId: doc.revisionId,
  });
  setRoadmapProjection(rm);
  if (rm) {
    const prog = await desktopClient.open<RoadmapProgressEntry[]>("list_roadmap_progress", {
      request: { paperId: doc.id, roadmapId: rm.id },
    });
    setRoadmapProgress(prog);
  } else {
    setRoadmapProgress([]);
  }
} catch {
  setRoadmapProjection(null);
  setRoadmapProgress([]);
}
```

- [ ] **Step 5: Run full test suite**

Run: `npx vitest run --maxWorkers=1 --fileParallelism=false`
Expected: All tests pass (existing + new)

Run: `npm run build`
Expected: Build succeeds

- [ ] **Step 6: Commit**

```bash
git add src/App.tsx src/desktopClient.ts
git commit -m "feat(roadmap): integrate Reading Roadmap panel into App with top island button"
```

---

### Task 6: Documentation & Final Verification

**Files:**
- Modify: `docs/agent-onboarding.md`
- Modify: `docs/handoff.md`
- Modify: `README.md`

**Interfaces:**
- Consumes: All previous tasks
- Produces: Updated documentation for next agent

- [ ] **Step 1: Update `docs/agent-onboarding.md`**

Add §32 section documenting the Reading Roadmap feature:
- File entry table row: `| 精读路线 | src/ReadingRoadmap.tsx, src-tauri/src/roadmap_module.rs |`
- Prompt slot: `PromptSlotId::ReadingRoadmap` (slot 17)
- Pitfall: Reading Roadmap 依赖 Orientation Pack 已存在

- [ ] **Step 2: Update `docs/handoff.md`**

Add to "已实现" list:
```
- 精读路线（Reading Roadmap）：基于 Three-Pass Approach 的论文特异性精读引导 Todo List，左侧浮动面板，证据药丸跳转，勾选进度持久化。
```

Update test count.

- [ ] **Step 3: Update `README.md`**

Add feature description in the core architecture section.

- [ ] **Step 4: Run full verification**

```powershell
npx vitest run --maxWorkers=1 --fileParallelism=false
npm run build
cd src-tauri
cargo test --locked
```

Expected: All tests pass, build succeeds.

- [ ] **Step 5: Final commit**

```bash
git add -A
git commit -m "docs: add Reading Roadmap to onboarding, handoff, and README"
```
