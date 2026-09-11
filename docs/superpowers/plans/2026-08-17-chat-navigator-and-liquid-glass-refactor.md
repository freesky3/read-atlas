# 聊天记录跳转刻度轴、38px 极简顶栏与增强型液态玻璃重构实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现聊天记录快速跳转刻度轴（Mini-Map Navigator）、38px 单行极简多功能顶栏（含 Thread 双击内联重命名），并将全站组件升级为增强型液态玻璃折射拟态材质（多层内高光、45° 对角高光斑与立体边缘）。

**Architecture:** 
1. 在 `styles.css` 中引入基于多层内阴影、对角高光线性渐变与深浅边缘的增强型液态玻璃 Token，应用至顶岛、胶囊、气泡、输入框与卡片；
2. 构建独立的 `ChatTimelineNavigator.tsx` 组件，自动解析消息列表中的用户提问，渲染弹性等比刻度轴，支持悬停磨砂卡片预览（粗体问题 + 2 行回答摘要）、点击平滑定位与滚动高亮追踪；
3. 将 `App.tsx` 右侧堆叠的 3 层头部重构为 38px 单行 `CompactRailHeader`，支持单击展开分支下拉列表与双击直接内联重命名 Thread 并持久化保存。

**Tech Stack:** React 19, TypeScript, Vitest, CSS3 Variables & backdrop-filter, Tauri 2 IPC.

**Spec:** [docs/superpowers/specs/2026-08-17-chat-navigator-and-liquid-glass-refactor-design.md](../../superpowers/specs/2026-08-17-chat-navigator-and-liquid-glass-refactor-design.md)

## Global Constraints

- **测试执行**：必须串行执行 Vitest（`npx vitest run --maxWorkers=1 --fileParallelism=false`），严禁无参并行导致 Windows 上 jsdom 内存溢出。
- **视口锁定**：保持 `html, body, #root, .app-shell` 为 `100vh / max-height: 100vh / overflow: hidden`，PDF 画布与 Chat 消息流内部独立滚动。
- **IPC 结构体规范**：任何 Tauri 2 结构体调用必须包装为 `{ request: { ...camelCaseFields } }`。
- **代码规范**：所有样式必须使用主题 CSS 变量（`--glass-surface`、`--liquid-glass-shadow` 等），禁止硬编码暖黄杂色。

---

### Task 1: 增强型液态玻璃材质 Token 与全站核心组件样式升级

**Files:**
- Modify: `src/styles.css`
- Test: `tests/theme.test.ts`

**Interfaces:**
- Produces: CSS 变量 `--liquid-glass-shadow`, `--liquid-glass-hover-shadow`, `--liquid-glass-sheen`, `--liquid-glass-specular` 及其在 `liquid-light`, `liquid-dark`, `warm-editorial` 下的适配；
- Updates: `.reader-floating-island`, `.hub-floating-island`, `.btn-liquid-pill`, `.liquid-tab-btn`, `.micro-action-pill`, `.user-bubble-right`, `.composer-wrap`, `.composer`, `.quote-basket` 样式。

- [ ] **Step 1: 编写主题 Token 存在性与更新测试**

在 `tests/theme.test.ts` 中补充对增强型液态玻璃 Token 规则的断言：

```ts
import { describe, it, expect } from "vitest";
import fs from "fs";
import path from "path";

describe("Enhanced Liquid Glass CSS Tokens", () => {
  it("defines multi-layer liquid glass shadow tokens in styles.css", () => {
    const css = fs.readFileSync(path.resolve(__dirname, "../src/styles.css"), "utf-8");
    expect(css).toContain("--liquid-glass-shadow");
    expect(css).toContain("--liquid-glass-sheen");
    expect(css).toContain("--liquid-glass-hover-shadow");
  });
});
```

- [ ] **Step 2: 运行测试验证失败**

Run: `npx vitest run tests/theme.test.ts --maxWorkers=1 --fileParallelism=false`
Expected: FAIL (Token 尚未定义)

- [ ] **Step 3: 在 `src/styles.css` 中实现增强型液态玻璃材质与组件样式**

在 `src/styles.css` 的 `:root` 与主题选择器中添加多层内高光阴影与 45° Sheen 渐变，并升级浮岛、按钮、提问气泡和输入框：

```css
:root, [data-theme="liquid-light"] {
  --liquid-glass-shadow:
    inset 2px -2px 1.5px -1px rgba(255, 255, 255, 0.95),
    inset -2px 2px 1.5px -1px rgba(255, 255, 255, 0.95),
    inset 6px -6px 2px -6px rgba(255, 255, 255, 0.6),
    inset -6px 6px 2px -6px rgba(255, 255, 255, 0.6),
    inset 0 0 2px rgba(15, 23, 42, 0.08),
    0 4px 12px rgba(15, 23, 42, 0.05);

  --liquid-glass-hover-shadow:
    inset 2px -2px 2px -1px rgba(255, 255, 255, 1),
    inset -2px 2px 2px -1px rgba(255, 255, 255, 1),
    inset 8px -8px 3px -6px rgba(255, 255, 255, 0.75),
    inset -8px 8px 3px -6px rgba(255, 255, 255, 0.75),
    inset 0 0 2px rgba(15, 23, 42, 0.12),
    0 8px 24px rgba(15, 23, 42, 0.1);

  --liquid-glass-sheen: linear-gradient(
    45deg,
    rgba(255, 255, 255, 0.75) 0%,
    transparent 22%,
    transparent 78%,
    rgba(255, 255, 255, 0.75) 100%
  );
}

[data-theme="liquid-dark"] {
  --liquid-glass-shadow:
    inset 1px -1px 1px 0 rgba(255, 255, 255, 0.25),
    inset -1px 1px 1px 0 rgba(255, 255, 255, 0.25),
    inset 4px -4px 2px -4px rgba(255, 255, 255, 0.15),
    inset 0 0 2px rgba(0, 0, 0, 0.6),
    0 6px 20px rgba(0, 0, 0, 0.45);

  --liquid-glass-hover-shadow:
    inset 1px -1px 1.5px 0 rgba(255, 255, 255, 0.35),
    inset -1px 1px 1.5px 0 rgba(255, 255, 255, 0.35),
    inset 6px -6px 3px -4px rgba(255, 255, 255, 0.25),
    inset 0 0 2px rgba(0, 0, 0, 0.7),
    0 8px 24px rgba(0, 0, 0, 0.55);

  --liquid-glass-sheen: linear-gradient(
    45deg,
    rgba(255, 255, 255, 0.2) 0%,
    transparent 22%,
    transparent 78%,
    rgba(255, 255, 255, 0.2) 100%
  );
}
```

- [ ] **Step 4: 运行测试验证通过**

Run: `npx vitest run tests/theme.test.ts --maxWorkers=1 --fileParallelism=false`
Expected: PASS

- [ ] **Step 5: 提交 Task 1 代码**

```bash
git add src/styles.css tests/theme.test.ts
git commit -m "feat(theme): add enhanced liquid glass multi-layer refraction tokens and styles"
```

---

### Task 2: 构建聊天记录跳转刻度轴组件 (`ChatTimelineNavigator`)

**Files:**
- Create: `src/components/ChatTimelineNavigator.tsx`
- Create: `src/components/ChatTimelineNavigator.test.tsx`
- Modify: `src/styles.css`

**Interfaces:**
- Component: `ChatTimelineNavigator`
  ```ts
  export interface TimelineTurn {
    userMessageId: string;
    question: string;
    timestamp: string;
    answerSnippet?: string;
    turnIndex: number;
  }
  export interface ChatTimelineNavigatorProps {
    messages: Array<{ id: string; role: string; text?: string; createdAt?: string }>;
    activeMessageId?: string | null;
    onJumpToMessage: (messageId: string) => void;
  }
  ```

- [ ] **Step 1: 编写 `ChatTimelineNavigator` 单元测试**

在 `src/components/ChatTimelineNavigator.test.tsx` 中编写测试：

```tsx
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import React from "react";
import { ChatTimelineNavigator } from "./ChatTimelineNavigator";

describe("ChatTimelineNavigator", () => {
  const sampleMessages = [
    { id: "m1", role: "user", text: "What is the core idea of this paper?", createdAt: "2026-08-17T01:15:00Z" },
    { id: "m2", role: "assistant", text: "The paper proposes a low-rank RNN framework linking neural dynamics.", createdAt: "2026-08-17T01:16:00Z" },
    { id: "m3", role: "user", text: "How is the connection matrix constructed?", createdAt: "2026-08-17T01:22:00Z" },
    { id: "m4", role: "assistant", text: "The matrix J is given by the outer product sum.", createdAt: "2026-08-17T01:23:00Z" },
  ];

  it("extracts user turns and renders correct number of ticks", () => {
    render(<ChatTimelineNavigator messages={sampleMessages} onJumpToMessage={() => {}} />);
    const ticks = screen.getAllByRole("button", { name: /跳转至问题/i });
    expect(ticks).toHaveLength(2);
  });

  it("calls onJumpToMessage when clicking a tick", () => {
    const handleJump = vi.fn();
    render(<ChatTimelineNavigator messages={sampleMessages} onJumpToMessage={handleJump} />);
    const ticks = screen.getAllByRole("button", { name: /跳转至问题/i });
    fireEvent.click(ticks[1]);
    expect(handleJump).toHaveBeenCalledWith("m3");
  });

  it("displays preview card content on hover/render", () => {
    render(<ChatTimelineNavigator messages={sampleMessages} onJumpToMessage={() => {}} />);
    expect(screen.getByText("What is the core idea of this paper?")).toBeInTheDocument();
    expect(screen.getByText(/The paper proposes a low-rank RNN framework/)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: 运行测试验证失败**

Run: `npx vitest run src/components/ChatTimelineNavigator.test.tsx --maxWorkers=1 --fileParallelism=false`
Expected: FAIL (组件不存在)

- [ ] **Step 3: 实现 `ChatTimelineNavigator.tsx`**

编写 `src/components/ChatTimelineNavigator.tsx`，计算 Q&A 轮次并渲染带 Hover Tooltip 预览的刻度条：

```tsx
import React, { useMemo } from "react";

export interface MessageItem {
  id: string;
  role: string;
  text?: string;
  content?: string;
  createdAt?: string;
}

export interface ChatTimelineNavigatorProps {
  messages: MessageItem[];
  activeMessageId?: string | null;
  onJumpToMessage: (messageId: string) => void;
}

export function ChatTimelineNavigator({
  messages,
  activeMessageId,
  onJumpToMessage,
}: ChatTimelineNavigatorProps) {
  const turns = useMemo(() => {
    const result: Array<{
      userMessageId: string;
      question: string;
      timeStr: string;
      answerSnippet: string;
      turnIndex: number;
    }> = [];

    let turnCount = 0;
    for (let i = 0; i < messages.length; i++) {
      const msg = messages[i];
      if (msg.role === "user") {
        turnCount++;
        const nextMsg = messages[i + 1];
        const answerText = nextMsg && nextMsg.role === "assistant"
          ? (nextMsg.text || nextMsg.content || "")
          : "";
        
        // 格式化时间
        let timeStr = "";
        if (msg.createdAt) {
          try {
            const d = new Date(msg.createdAt);
            timeStr = `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
          } catch {
            timeStr = "";
          }
        }

        // 清理 Markdown 标记用于纯文本摘要
        const cleanSnippet = answerText
          .replace(/[#*`$\\]/g, "")
          .replace(/\[\^?\d+\]/g, "")
          .replace(/\s+/g, " ")
          .trim()
          .slice(0, 110);

        result.push({
          userMessageId: msg.id,
          question: (msg.text || msg.content || "用户提问").trim(),
          timeStr,
          answerSnippet: cleanSnippet ? `${cleanSnippet}...` : "等待回答中...",
          turnIndex: turnCount,
        });
      }
    }
    return result;
  }, [messages]);

  if (turns.length === 0) {
    return null;
  }

  return (
    <aside className="chat-timeline-minimap" aria-label="聊天记录跳转导航">
      {turns.map((turn) => {
        const isActive = activeMessageId === turn.userMessageId;
        return (
          <button
            key={turn.userMessageId}
            type="button"
            className={`timeline-tick ${isActive ? "active" : ""}`}
            onClick={() => onJumpToMessage(turn.userMessageId)}
            aria-label={`跳转至问题 #${turn.turnIndex}: ${turn.question}`}
            title={`#${turn.turnIndex} ${turn.question}`}
          >
            <div className="timeline-preview-card" onClick={(e) => e.stopPropagation()}>
              <div className="preview-header">
                <span className="preview-index">#{turn.turnIndex} 问题</span>
                {turn.timeStr && <span>{turn.timeStr}</span>}
              </div>
              <div className="preview-question">{turn.question}</div>
              <div className="preview-snippet">{turn.answerSnippet}</div>
            </div>
          </button>
        );
      })}
    </aside>
  );
}
```

- [ ] **Step 4: 添加刻度轴与 Tooltip 样式至 `src/styles.css`**

```css
.chat-timeline-minimap {
  width: 18px;
  flex: 0 0 18px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 7px;
  padding: 10px 0;
  position: relative;
  z-index: 25;
  user-select: none;
}
.timeline-tick {
  width: 12px;
  height: 3px;
  border-radius: 2px;
  background: rgba(100, 116, 139, 0.35);
  border: none;
  padding: 0;
  cursor: pointer;
  position: relative;
  transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);
}
.timeline-tick:hover {
  width: 18px;
  height: 4px;
  background: var(--blue);
  box-shadow: 0 0 8px rgba(37, 99, 235, 0.5);
}
.timeline-tick.active {
  width: 16px;
  height: 4px;
  background: var(--success);
  box-shadow: 0 0 6px rgba(22, 163, 74, 0.4);
}
.timeline-preview-card {
  position: absolute;
  right: 24px;
  top: 50%;
  transform: translateY(-50%) translateX(6px);
  width: 260px;
  padding: 11px 13px;
  background: var(--glass-card);
  backdrop-filter: blur(28px) saturate(190%);
  -webkit-backdrop-filter: blur(28px) saturate(190%);
  border: 1px solid var(--glass-border);
  border-radius: 14px;
  box-shadow: var(--liquid-glass-shadow), 0 16px 36px -6px rgba(15, 23, 42, 0.18);
  pointer-events: none;
  opacity: 0;
  visibility: hidden;
  transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);
  z-index: 100;
  text-align: left;
}
.timeline-tick:hover .timeline-preview-card {
  opacity: 1;
  visibility: visible;
  transform: translateY(-50%) translateX(0);
}
.preview-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 5px;
  font-size: 10px;
  color: var(--muted);
}
.preview-index {
  font-weight: 700;
  color: var(--blue);
}
.preview-question {
  font-size: 12px;
  font-weight: 700;
  color: var(--ink);
  margin-bottom: 4px;
  line-height: 1.35;
}
.preview-snippet {
  font-size: 11px;
  color: var(--muted);
  line-height: 1.45;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
```

- [ ] **Step 5: 运行单元测试验证通过**

Run: `npx vitest run src/components/ChatTimelineNavigator.test.tsx --maxWorkers=1 --fileParallelism=false`
Expected: PASS

- [ ] **Step 6: 提交 Task 2 代码**

```bash
git add src/components/ChatTimelineNavigator.tsx src/components/ChatTimelineNavigator.test.tsx src/styles.css
git commit -m "feat(reader): add ChatTimelineNavigator component with hover preview and jump capability"
```

---

### Task 3: 38px 单行极简多功能顶栏与 Thread 双击重命名重构

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/styles.css`
- Test: `tests/viewNavigation.test.tsx`

**Interfaces:**
- App State: `editingThreadId`, `editingThreadTitle`, `isThreadDropdownOpen`
- Features:
  - Consolidates `.side-mode-tabs`, `.chat-header`, and `.thread-tabs` into single `.compact-rail-header` (38px height).
  - Double clicking thread title activates inline `<input />`, pressing Enter/blur triggers title update and persists to SQLite/state.
  - Arrow click `▾` toggles thread selection popover.

- [ ] **Step 1: 编写顶栏单行化与双击重命名集成测试**

在 `tests/viewNavigation.test.tsx` 中添加对 `compact-rail-header` 与重命名行为的测试断言。

- [ ] **Step 2: 在 `src/App.tsx` 中重构头部与内联编辑逻辑**

1. 将原有的多层头部替换为单行 `.compact-rail-header`：
```tsx
<div className="compact-rail-header">
  <div className="compact-rail-left">
    <div className="compact-tab-group">
      <button
        type="button"
        className={`compact-tab-item ${sideMode === "chat" ? "active" : ""}`}
        onClick={() => setSideMode("chat")}
      >
        讨论
      </button>
      <button
        type="button"
        className={`compact-tab-item ${sideMode === "artifacts" ? "active" : ""}`}
        onClick={() => setSideMode("artifacts")}
      >
        阅读成果 {artifactsCount > 0 && `(${artifactsCount})`}
      </button>
    </div>

    {sideMode === "chat" && (
      <div className="thread-select-wrapper">
        {isEditingThreadTitle ? (
          <input
            type="text"
            className="thread-title-inline-input"
            value={editingTitleText}
            autoFocus
            onChange={(e) => setEditingTitleText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") saveThreadTitle();
              if (e.key === "Escape") cancelEditingThreadTitle();
            }}
            onBlur={saveThreadTitle}
          />
        ) : (
          <button
            type="button"
            className="thread-dropdown-btn"
            onDoubleClick={startEditingThreadTitle}
            onClick={() => setIsThreadDropdownOpen((prev) => !prev)}
            title="单击切换分支，双击重命名"
          >
            <span className="thread-title-text">{activeThreadTitle || "Main discussion"}</span>
            <span className="thread-dropdown-arrow">▾</span>
          </button>
        )}
      </div>
    )}
  </div>

  <div className="compact-rail-right">
    {sideMode === "chat" && (
      <>
        <button
          type="button"
          className="icon-pill-btn"
          onClick={() => void createNewThread()}
          title="新建讨论分支"
        >
          ＋
        </button>
        <button
          type="button"
          className="icon-pill-btn"
          onClick={() => setIsConversationTreeOpen(true)}
          title="讨论树视图"
        >
          🌳
        </button>
        <button
          type="button"
          className="icon-pill-btn"
          onClick={() => void archiveCurrentThread()}
          title="归档当前分支"
        >
          📥
        </button>
      </>
    )}
  </div>
</div>
```

2. 实现分支下拉菜单弹出浮层与双击重命名保存方法：
```tsx
const startEditingThreadTitle = () => {
  setEditingTitleText(activeThread?.title || "Main discussion");
  setIsEditingThreadTitle(true);
  setIsThreadDropdownOpen(false);
};

const saveThreadTitle = async () => {
  setIsEditingThreadTitle(false);
  const trimmed = editingTitleText.trim();
  if (trimmed && activeThread && trimmed !== activeThread.title) {
    // 更新本地状态并通知后端持久化
    await renameThreadTitle(activeThread.id, trimmed);
  }
};
```

- [ ] **Step 3: 在 `src/styles.css` 中添加 `.compact-rail-header` 样式**

```css
.compact-rail-header {
  height: 38px;
  flex: 0 0 38px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 10px;
  background: var(--glass-surface-subtle);
  border-bottom: 1px solid var(--glass-border-subtle);
  backdrop-filter: var(--glass-blur);
  z-index: 20;
}
.compact-rail-left {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.compact-tab-group {
  display: inline-flex;
  background: rgba(15, 23, 42, 0.06);
  border-radius: 999px;
  padding: 2px;
  gap: 2px;
  flex-shrink: 0;
}
.compact-tab-item {
  border: none;
  background: transparent;
  padding: 3px 9px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 600;
  color: var(--muted);
  cursor: pointer;
  transition: all 0.15s ease;
}
.compact-tab-item.active {
  background: var(--paper);
  color: var(--ink);
  box-shadow: var(--liquid-glass-shadow);
}
.thread-dropdown-btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  border: 1px solid var(--glass-border);
  border-radius: 999px;
  padding: 3px 9px;
  background: var(--glass-card);
  box-shadow: var(--liquid-glass-shadow);
  font-size: 11px;
  font-weight: 600;
  color: var(--ink);
  cursor: pointer;
  max-width: 170px;
}
.thread-title-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.thread-title-inline-input {
  height: 24px;
  padding: 0 8px;
  border-radius: 999px;
  border: 1px solid var(--blue);
  background: var(--paper);
  font-size: 11px;
  font-weight: 600;
  color: var(--ink);
  outline: none;
  box-shadow: 0 0 0 2px rgba(37, 99, 235, 0.2);
}
.compact-rail-right {
  display: flex;
  align-items: center;
  gap: 5px;
  flex-shrink: 0;
}
.icon-pill-btn {
  width: 25px;
  height: 25px;
  display: grid;
  place-items: center;
  border: 1px solid var(--glass-border);
  border-radius: 50%;
  background: var(--glass-card);
  box-shadow: var(--liquid-glass-shadow);
  color: var(--muted);
  cursor: pointer;
  font-size: 11.5px;
  transition: all 0.15s ease;
}
.icon-pill-btn:hover {
  color: var(--ink);
  box-shadow: var(--liquid-glass-hover-shadow);
  transform: scale(1.05);
}
```

- [ ] **Step 4: 运行测试验证**

Run: `npx vitest run tests/viewNavigation.test.tsx --maxWorkers=1 --fileParallelism=false`
Expected: PASS

- [ ] **Step 5: 提交 Task 3 代码**

```bash
git add src/App.tsx src/styles.css tests/viewNavigation.test.tsx
git commit -m "feat(reader): consolidate chat headers into 38px compact rail with thread inline renaming"
```

---

### Task 4: 集成 ChatTimelineNavigator、消息光晕脉冲与滚动监听

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/styles.css`
- Test: `tests/viewNavigation.test.tsx`

**Interfaces:**
- Layout: `.chat-body-wrapper` containing `.message-stream` and `<ChatTimelineNavigator />`
- Smooth Scroll: `jumpToMessage(msgId)` scrolling target element to center + adds `.target-highlight` class.
- Scroll Spy: Active tick updates as user scrolls the discussion stream.

- [ ] **Step 1: 在 `src/App.tsx` 中嵌入 `ChatTimelineNavigator` 并实现平滑滚动与光晕高亮**

在 `App.tsx` 的研讨流区域：
```tsx
<div className="chat-body-wrapper">
  <div
    className="message-stream"
    ref={messageStreamRef}
    onScroll={handleMessageStreamScroll}
  >
    {/* 消息渲染列表 */}
  </div>

  <ChatTimelineNavigator
    messages={discussionMessages}
    activeMessageId={activeScrollMessageId}
    onJumpToMessage={handleJumpToMessage}
  />
</div>
```

实现 `handleJumpToMessage`：
```tsx
const handleJumpToMessage = (messageId: string) => {
  const el = document.getElementById(`msg-${messageId}`);
  if (el) {
    el.scrollIntoView({ behavior: "smooth", block: "center" });
    el.classList.remove("target-highlight");
    void el.offsetWidth; // 触发 reflow
    el.classList.add("target-highlight");
    setActiveScrollMessageId(messageId);
  }
};
```

- [ ] **Step 2: 在 `src/styles.css` 中添加 `.chat-body-wrapper` 与 `.target-highlight` 动效**

```css
.chat-body-wrapper {
  flex: 1;
  min-height: 0;
  display: flex;
  position: relative;
  overflow: hidden;
}
.message-item {
  transition: transform 0.3s cubic-bezier(0.16, 1, 0.3, 1), filter 0.3s ease;
}
.message-item.target-highlight {
  animation: pulseMessageHighlight 1.5s ease-out;
}
@keyframes pulseMessageHighlight {
  0% {
    transform: scale(1.02);
    filter: drop-shadow(0 0 12px rgba(37, 99, 235, 0.35));
  }
  100% {
    transform: scale(1);
    filter: none;
  }
}
```

- [ ] **Step 3: 运行全量测试验证**

Run: `npx vitest run --maxWorkers=1 --fileParallelism=false`
Expected: PASS (所有 16 个测试文件全部通过)

- [ ] **Step 4: 提交 Task 4 代码**

```bash
git add src/App.tsx src/styles.css
git commit -m "feat(reader): wire ChatTimelineNavigator with smooth scroll, active spy, and pulse highlight"
```

---

### Task 5: 全量回归、构建验证与文档同步

**Files:**
- Modify: `docs/superpowers/specs/2026-08-17-chat-navigator-and-liquid-glass-refactor-design.md`
- Modify: `docs/agent-onboarding.md`
- Modify: `README.md`

- [ ] **Step 1: 运行所有自动化测试与构建检查**

Run:
```powershell
npx vitest run --maxWorkers=1 --fileParallelism=false
npx tsc -b
npm run build
cd src-tauri
cargo check --locked
cargo test --locked
```
Expected: 全部测试通过，0 报错。

- [ ] **Step 2: 更新 `agent-onboarding.md` 与 `README.md`**

同步新加的 `ChatTimelineNavigator` 组件架构、38px 顶栏重构与双击重命名。

- [ ] **Step 3: 提交最终文档更新**

```bash
git add docs/ README.md
git commit -m "docs: document ChatTimelineNavigator, 38px compact rail header, and enhanced liquid glass in onboarding guide"
```
