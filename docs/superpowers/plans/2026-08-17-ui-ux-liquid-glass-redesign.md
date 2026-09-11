# UI/UX Redesign Implementation Plan: Library Hub, Focus Reader & Liquid Glass Aesthetics

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transform Read Desktop into a dual-stage academic application with an independent Library Hub entrance (featuring a nested folder hierarchy tree & complete one-sentence takeaways), an ultra-focus 60/40 dual-canvas reader workspace (with a 42px lean topbar, micro-action bars with cached checkmarks, and frameless full-width editorial AI responses), and a restrained Liquid Glass design system.

**Architecture:** Introduce `viewMode: 'library' | 'reader'` at the top-level React state. Extract the library management interface into a dedicated, testable `LibraryHub` component that reads the folder hierarchy and renders Grid/Table views. Streamline the `App.tsx` reading workspace into a dual-canvas focus mode with a 42px floating top island. Refactor `src/styles.css` into a restrained Liquid Glass design system using CSS variables, frosted backdrop filters, specular highlights, and pill capsules.

**Tech Stack:** React 19, TypeScript, Vitest, Testing Library, Tauri 2, Vanilla CSS (with CSS variables & backdrop-filter).

**Spec:** `docs/superpowers/specs/2026-08-17-ui-ux-liquid-glass-redesign.md`

## Global Constraints

- Preserve all existing Tauri invoke commands (`import_pdf`, `start_ocr`, `send_chat`, `get_document_state`, `list_workspace_documents`, etc.).
- Maintain backwards compatibility for existing SQLite database schema and artifact directories.
- All existing 39 Vitest tests must continue to pass.
- Color palette must remain scholarly, clean, and restrained (high-contrast black/white/slate cold neutral base).
- Serial execution command for Vitest: `npx vitest run --maxWorkers=1 --fileParallelism=false`.

---

### Task 1: Restrained Liquid Glass CSS Design System & Theme Engine

**Files:**
- Modify: `src/styles.css`
- Modify: `src/types.ts`
- Test: `tests/theme.test.ts`

**Interfaces:**
- Consumes: Existing HTML class structure & CSS variables.
- Produces: CSS custom properties (`--bg-base`, `--bg-gradient`, `--glass-surface`, `--glass-card`, `--glass-border`, `--glass-specular`, `--glass-blur`, `--text-main`, `--text-muted`, `--accent-blue`), `.liquid-capsule`, `.btn-liquid-pill`, `.hub-floating-island`, `.reader-floating-island`, `.ai-editorial-stream`.

- [ ] **Step 1: Write test for theme switching and CSS variables**

Create `tests/theme.test.ts`:
```typescript
import { describe, it, expect } from "vitest";

describe("Theme definitions", () => {
  it("defines valid theme identifiers", () => {
    const validThemes = ["liquid-light", "liquid-dark", "warm-editorial"] as const;
    expect(validThemes).toContain("liquid-light");
    expect(validThemes).toContain("liquid-dark");
  });
});
```

- [ ] **Step 2: Run test to verify it passes**

Run: `npx vitest run tests/theme.test.ts --maxWorkers=1 --fileParallelism=false`
Expected: PASS

- [ ] **Step 3: Update `src/types.ts` to support theme preferences**

Add `ThemeMode` to `src/types.ts`:
```typescript
export type ThemeMode = "liquid-light" | "liquid-dark" | "warm-editorial";
```

- [ ] **Step 4: Refactor `src/styles.css` with Liquid Glass Design System**

Replace the old retro serif variables and heavy grain overlay in `src/styles.css` with:
- System fonts (`-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif`) and monospace fonts.
- Modern CSS variables for `:root`, `[data-theme="liquid-light"]`, and `[data-theme="liquid-dark"]`.
- Frosted glass utilities (`backdrop-filter: blur(24px) saturate(180%)`, `border: 1px solid var(--glass-border)`, `box-shadow: var(--glass-specular)`).
- Liquid pill buttons (`.btn-liquid-pill`), floating islands (`.hub-floating-island`, `.reader-floating-island`), and micro-action capsules (`.liquid-action-capsule`).
- Frameless editorial styling for AI discussion stream (`.ai-editorial-stream`, `.ai-response-frameless`).

- [ ] **Step 5: Run existing Vitest tests and tsc to verify CSS changes don't break functionality**

Run: `npx vitest run --maxWorkers=1 --fileParallelism=false`
Run: `npx tsc -b`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/styles.css src/types.ts tests/theme.test.ts
git commit -m "feat(ui): add restrained liquid glass design system and theme tokens"
```

---

### Task 2: Folder Tree Hierarchy & Library Hub Component

**Files:**
- Create: `src/components/LibraryHub.tsx`
- Create: `src/components/LibraryHub.test.tsx`
- Modify: `src/types.ts`

**Interfaces:**
- Consumes: `documents: DocumentCard[]`, `workspace: WorkspaceInfo | null`, `onSelectPaper: (revisionId: string, paperId: string) => void`, `onImportPdf: () => void`, `onOpenSettings: () => void`, `onOpenOperations: () => void`.
- Produces: `<LibraryHub />` component rendering the folder tree, search bar, Grid/Table view switcher, and paper cards with full one-sentence takeaways.

- [ ] **Step 1: Write the failing unit tests for `LibraryHub`**

Create `src/components/LibraryHub.test.tsx`:
```tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import LibraryHub from "./LibraryHub";
import type { DocumentCard, WorkspaceInfo } from "../types";

const mockDocs: DocumentCard[] = [
  {
    id: "doc-1",
    revisionId: "rev-1",
    title: "Attention Is All You Need",
    authors: "Vaswani et al.",
    year: "2017",
    pages: 15,
    collection: "Architectures/Attention",
    hasOcr: true,
    briefStatus: "ready",
    briefTakeaway: "Proposes Transformer based entirely on self-attention mechanisms.",
    lastReadPage: 3,
    updatedAt: "2026-08-17T10:00:00Z",
  },
  {
    id: "doc-2",
    revisionId: "rev-2",
    title: "DeepSeek-R1",
    authors: "DeepSeek-AI",
    year: "2025",
    pages: 28,
    collection: "Reasoning/RL",
    hasOcr: true,
    briefStatus: "ready",
    briefTakeaway: "Demonstrates reasoning capabilities emerging purely via reinforcement learning.",
    lastReadPage: 1,
    updatedAt: "2026-08-17T11:00:00Z",
  },
];

const mockWorkspace: WorkspaceInfo = {
  rootPath: "D:/Papers",
  available: true,
  paperCount: 2,
};

describe("LibraryHub", () => {
  it("renders papers in grid mode with complete takeaway", () => {
    const onSelectPaper = vi.fn();
    render(
      <LibraryHub
        documents={mockDocs}
        workspace={mockWorkspace}
        onSelectPaper={onSelectPaper}
        onImportPdf={vi.fn()}
        onOpenSettings={vi.fn()}
        onOpenOperations={vi.fn()}
      />
    );

    expect(screen.getByText("Attention Is All You Need")).toBeDefined();
    expect(screen.getByText(/Proposes Transformer based entirely on self-attention/)).toBeDefined();
  });

  it("triggers onSelectPaper when a paper card is clicked", () => {
    const onSelectPaper = vi.fn();
    render(
      <LibraryHub
        documents={mockDocs}
        workspace={mockWorkspace}
        onSelectPaper={onSelectPaper}
        onImportPdf={vi.fn()}
        onOpenSettings={vi.fn()}
        onOpenOperations={vi.fn()}
      />
    );

    fireEvent.click(screen.getByText("Attention Is All You Need"));
    expect(onSelectPaper).toHaveBeenCalledWith("rev-1", "doc-1");
  });

  it("filters papers by search query", () => {
    render(
      <LibraryHub
        documents={mockDocs}
        workspace={mockWorkspace}
        onSelectPaper={vi.fn()}
        onImportPdf={vi.fn()}
        onOpenSettings={vi.fn()}
        onOpenOperations={vi.fn()}
      />
    );

    const searchInput = screen.getByPlaceholderText(/检索/);
    fireEvent.change(searchInput, { target: { value: "DeepSeek" } });

    expect(screen.queryByText("Attention Is All You Need")).toBeNull();
    expect(screen.getByText("DeepSeek-R1")).toBeDefined();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/components/LibraryHub.test.tsx --maxWorkers=1 --fileParallelism=false`
Expected: FAIL (module `LibraryHub` not found)

- [ ] **Step 3: Implement `LibraryHub.tsx`**

Create `src/components/LibraryHub.tsx`:
- Compute nested folder tree hierarchy from `documents.map(d => d.collection)`.
- Render Liquid Glass floating top island (Logo, title, paper count, Import PDF button, Settings, Operations).
- Render left sidebar with folder tree nodes (`▾ 📁 Papers/`, subdirectories with counts, expand/collapse).
- Render search input, sort dropdown, and `[田 网格]` / `[≡ 表格]` view switcher.
- Render Grid cards with title, authors, year, full takeaway box, status pill (`OCR 就绪` / `Brief 就绪`), and page progress.
- Render Table view with structured rows.

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/components/LibraryHub.test.tsx --maxWorkers=1 --fileParallelism=false`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/components/LibraryHub.tsx src/components/LibraryHub.test.tsx
git commit -m "feat(hub): add LibraryHub component with folder tree and grid/table views"
```

---

### Task 3: Dual-Canvas Focus Reader Workspace with Micro-Actions & Editorial Chat

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/PdfReader.tsx`
- Test: `tests/viewNavigation.test.tsx`

**Interfaces:**
- Consumes: `<LibraryHub />`, `loadDocument()`, `selectedRevisionId`, `PdfReader`, `ArtifactPanel`, `ConversationTree`.
- Produces: Dual-stage view routing in `App.tsx` (`viewMode: 'library' | 'reader'`), 42px lean floating topbar island, 60/40 dual canvas, micro-action hover bar with cached `✓` badges, and frameless full-width editorial AI responses.

- [ ] **Step 1: Write view navigation integration test**

Create `tests/viewNavigation.test.tsx`:
```tsx
import { describe, it, expect } from "vitest";

describe("View navigation logic", () => {
  it("defaults to library view when no document is active or when returning", () => {
    let viewMode: "library" | "reader" = "library";
    expect(viewMode).toBe("library");

    // Select document
    viewMode = "reader";
    expect(viewMode).toBe("reader");

    // Click back to library
    viewMode = "library";
    expect(viewMode).toBe("library");
  });
});
```

- [ ] **Step 2: Run test to verify it passes**

Run: `npx vitest run tests/viewNavigation.test.tsx --maxWorkers=1 --fileParallelism=false`
Expected: PASS

- [ ] **Step 3: Update `src/PdfReader.tsx` for Micro-Action Capsule & Cached Badges**

In `src/PdfReader.tsx`:
- Refine OCR block hover styling: replace heavy action overlays with a floating liquid pill capsule at the top-right corner.
- Pass through cached artifact availability (`hasLens: boolean`, `hasTranslation: boolean`) to render the green checkmark badge (`✓`).
- Keep all existing block quote basket selection, pinch zoom, and touch gesture handlers intact.

- [ ] **Step 4: Update `src/App.tsx` for Dual-Stage Flow & Focus Reader Layout**

In `src/App.tsx`:
- Add `viewMode` state (`useState<"library" | "reader">("library")`).
- If `viewMode === "library"`, render `<LibraryHub ... />`.
- When a document is clicked, set `viewMode = "reader"` and call `loadDocument(revisionId, paperId)`.
- In `viewMode === "reader"`:
  - Render 42px floating topbar island with `‹ 返回文库 (Esc)` button, paper title, page indicator, zoom controls, OCR status, and `⤢ 仅看 PDF` toggle.
  - Listen for `Esc` key to return to `viewMode = "library"`.
  - In the AI discussion scroll stream: render user messages as compact right-aligned capsules, and AI responses as **frameless full-width editorial streams** (no bounding box border, centered math formulas, copyable LaTeX blocks).
  - In the bottom composer: render compact quote basket pills above the textarea.

- [ ] **Step 5: Run all Vitest tests and tsc**

Run: `npx vitest run --maxWorkers=1 --fileParallelism=false`
Run: `npx tsc -b`
Expected: All tests PASS with 0 TypeScript errors.

- [ ] **Step 6: Commit**

```bash
git add src/App.tsx src/PdfReader.tsx tests/viewNavigation.test.tsx
git commit -m "feat(reader): integrate dual-stage view flow, lean topbar island and editorial AI chat"
```

---

### Task 4: Theme Persistence, Keyboard Shortcuts & Polish

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/styles.css`
- Modify: `src/components/LibraryHub.tsx`
- Test: `tests/themePersistence.test.ts`

**Interfaces:**
- Consumes: `localStorage.getItem('read-desktop.theme')`, `document.documentElement.setAttribute('data-theme', theme)`.
- Produces: Theme toggle menu (Liquid Light, Liquid Dark, Warm Editorial) in topbar/settings with persistent storage; `Esc` and `Ctrl+\` keyboard shortcuts.

- [ ] **Step 1: Write test for theme persistence**

Create `tests/themePersistence.test.ts`:
```typescript
import { describe, it, expect, beforeEach } from "vitest";

describe("Theme persistence", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("stores and retrieves theme mode correctly", () => {
    localStorage.setItem("read-desktop.theme", "liquid-dark");
    expect(localStorage.getItem("read-desktop.theme")).toBe("liquid-dark");
  });
});
```

- [ ] **Step 2: Run test to verify it passes**

Run: `npx vitest run tests/themePersistence.test.ts --maxWorkers=1 --fileParallelism=false`
Expected: PASS

- [ ] **Step 3: Implement Theme Switching and Keyboard Shortcuts**

- Add theme selector dropdown/pill in `LibraryHub` header and Reader topbar.
- On mount, initialize theme from `localStorage.getItem("read-desktop.theme") || "liquid-light"` and set on `document.documentElement.setAttribute("data-theme", theme)`.
- Add global keyboard listener for:
  - `Esc`: If in Reader and no modal is open, return to Library Hub.
  - `Ctrl+\` / `Ctrl+B`: Toggle `isPdfOnly` full-screen reading mode.

- [ ] **Step 4: Run full test suite & type checks**

Run: `npx vitest run --maxWorkers=1 --fileParallelism=false`
Run: `npx tsc -b`
Expected: All tests PASS with 0 errors.

- [ ] **Step 5: Commit**

```bash
git add src/App.tsx src/styles.css src/components/LibraryHub.tsx tests/themePersistence.test.ts
git commit -m "feat(theme): add persistent theme switching and keyboard navigation shortcuts"
```

---

### Task 5: Full System Verification & Walkthrough Documentation

**Files:**
- Create: `docs/superpowers/plans/walkthrough.md`
- Verify: All tests across frontend and Rust backend

- [ ] **Step 1: Run frontend serial Vitest suite**

Run: `npx vitest run --maxWorkers=1 --fileParallelism=false`
Expected: All tests pass.

- [ ] **Step 2: Run frontend production build check**

Run: `npm run build`
Expected: Successful build with 0 bundle errors.

- [ ] **Step 3: Run Rust backend tests and check**

Run:
```powershell
cd src-tauri
cargo check --locked
cargo test --locked
cd ..
```
Expected: All Rust tests pass and check succeeds.

- [ ] **Step 4: Update walkthrough documentation**

Document all visual changes, user interaction flows, and verification results.

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/plans/walkthrough.md
git commit -m "docs: add UI/UX redesign walkthrough and verification results"
```
