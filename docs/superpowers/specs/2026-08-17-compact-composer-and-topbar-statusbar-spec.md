# Compact Composer & Topbar Statusbar Integration Spec

## 1. Context & Motivation

In current layout:
1. The discussion composer at the bottom takes up ~120px+ of vertical height (multiline textarea, bottom actions bar, status line, `Context` link).
2. The bottom fixed statusbar (`<footer className="statusbar">`) takes up 28px of height across the entire application window.
3. The `Context` link in the composer is a prompt inspector tool rarely needed during normal reading.

This specification redesigns the bottom composer and statusbar to maximize vertical reading and discussion space:
- Refactor the composer into a sleek, 36px single-line 3D Liquid Glass capsule with inline send/interrupt button.
- Collapse sub-information into a single 12px micro-row (active model label + quoted blocks badge).
- Move workspace status (`● Ready`) and token stats (`INPUT`, `CACHED`, `OUTPUT`, `COST`) into the top floating island (`.reader-floating-island`).
- Completely remove the bottom 28px `<footer className="statusbar">`.
- Net vertical space gain: **~96px+** across the entire window.

---

## 2. Component Specifications

### 2.1 Single-Line Compact Composer (`.composer-compact-capsule`)

- **Structure**:
  ```tsx
  <div className="composer-compact-wrap">
    <div className="composer-compact-capsule">
      <input
        type="text"
        className="composer-input-line"
        placeholder="输入你的问题或追问... (Enter 发送)"
        value={question}
        onChange={(e) => setQuestion(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            void sendQuestion();
          }
        }}
        disabled={replyTo === "busy"}
      />
      {isStreaming ? (
        <button
          type="button"
          className="composer-action-btn stop"
          onClick={cancelGeneration}
          title="中断回答 (Stop)"
        >
          ■
        </button>
      ) : (
        <button
          type="button"
          className="composer-action-btn send"
          disabled={!question.trim()}
          onClick={() => void sendQuestion()}
          title="发送 (Enter)"
        >
          ↑
        </button>
      )}
    </div>
    
    <div className="composer-subline">
      <div className="composer-subline-left">
        <span className="model-indicator-dot" />
        <span className="model-label-text">{assistantModelLabel}</span>
      </div>
      {quoteBasket.length > 0 && (
        <div className="composer-subline-quotes" title="已引用 OCR 选区">
          📎 <span>{quoteBasket.length}</span>
        </div>
      )}
    </div>
  </div>
  ```

- **Visual Styling**:
  - `composer-compact-capsule`:
    - `height: 38px;`
    - `border-radius: 9999px;`
    - `background: var(--glass-card);`
    - `border: 1.5px solid var(--glass-border);`
    - `box-shadow: var(--liquid-glass-shadow);`
    - `backdrop-filter: var(--glass-blur);`
    - `display: flex; align-items: center; padding: 0 4px 0 14px; gap: 8px;`
  - `composer-action-btn`:
    - `width: 30px; height: 30px; border-radius: 50%;`
    - `background: #0f172a; color: #ffffff;`
    - In streaming stop mode: `background: #ef4444; color: #ffffff;`
  - `composer-subline`:
    - `font-size: 11px; color: var(--muted); height: 16px; margin-top: 4px;`

### 2.2 Topbar Statusbar Integration

- **In `.reader-floating-island`**:
  - Right section now includes:
    ```tsx
    <div className="topbar-status-pip" title={status}>
      <span className="status-pip" />
      <span className="topbar-status-text">{status}</span>
    </div>
    <div className="topbar-token-capsule" title="当前会话 Token 统计">
      <span>{formatNumber(stats?.inputTokens)} in</span>
      <span className="sep">·</span>
      <span>{formatNumber(stats?.outputTokens)} out</span>
    </div>
    ```
- **Remove `<footer className="statusbar">`** from `src/App.tsx`.

---

## 3. Verification Plan

- Unit Tests: Verify Vitest tests for compact composer, streaming interrupt state, and topbar token display.
- Build Validation: `tsc -b && vite build` and `cargo test --locked`.
