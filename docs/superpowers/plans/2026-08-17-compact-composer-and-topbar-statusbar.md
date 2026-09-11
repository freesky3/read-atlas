# Compact Composer & Topbar Statusbar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor bottom chat composer into a 38px single-line 3D Liquid Glass capsule with streaming interrupt, integrate status & token metrics into the top floating island, and delete the 28px bottom statusbar.

**Architecture:** Update `src/App.tsx` and `src/styles.css` to replace `.composer` and `.context-line` with `.composer-compact-capsule` and `.composer-subline`. Embed status pip and token metrics into `.reader-floating-island`.

**Tech Stack:** React, TypeScript, Vanilla CSS (Liquid Glass Tokens), Vitest.

**Spec:** `docs/superpowers/specs/2026-08-17-compact-composer-and-topbar-statusbar-spec.md`

---

### Task 1: Refactor Chat Composer into Single-Line Compact Capsule with Streaming Interrupt

**Files:**
- Modify: `src/App.tsx` (replace `.composer-wrap` contents with `.composer-compact-wrap`)
- Modify: `src/styles.css` (add `.composer-compact-wrap`, `.composer-compact-capsule`, `.composer-input-line`, `.composer-action-btn`, `.composer-subline`)
- Test: `tests/compactComposer.test.tsx` (NEW)

- [ ] **Step 1: Write test for Compact Composer and Interrupt button**
- [ ] **Step 2: Run test and verify it fails**
- [ ] **Step 3: Implement `.composer-compact-capsule` in `src/App.tsx` and `src/styles.css`**
- [ ] **Step 4: Run test and verify it passes**
- [ ] **Step 5: Commit changes**

---

### Task 2: Integrate Status & Token Metrics into Topbar Island and Remove Bottom Statusbar

**Files:**
- Modify: `src/App.tsx` (add status & token capsule to `.reader-floating-island`, remove `<footer className="statusbar">`)
- Modify: `src/styles.css` (add `.topbar-status-pip`, `.topbar-token-capsule`, update app-shell height)

- [ ] **Step 1: Add topbar status and token capsule to `src/App.tsx`**
- [ ] **Step 2: Remove `<footer className="statusbar">` from `src/App.tsx`**
- [ ] **Step 3: Add CSS for `.topbar-status-pip` and `.topbar-token-capsule`**
- [ ] **Step 4: Run full Vitest suite and build**
- [ ] **Step 5: Commit changes**

---

### Task 3: Regression Verification & Documentation Update

**Files:**
- Modify: `README.md`
- Modify: `docs/agent-onboarding.md`

- [ ] **Step 1: Run `npx vitest run --maxWorkers=1 --fileParallelism=false`**
- [ ] **Step 2: Run `npm run build`**
- [ ] **Step 3: Update `README.md` and `docs/agent-onboarding.md`**
- [ ] **Step 4: Commit changes**
