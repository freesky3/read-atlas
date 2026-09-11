import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import React, { useState } from "react";

// Mock component simulating the compact rail header interactions
function MockCompactRailHeader({
  initialTitle = "Main discussion",
  isNarrow = false,
  onRename,
}: {
  initialTitle?: string;
  isNarrow?: boolean;
  onRename: (newTitle: string) => void;
}) {
  const [rightTab, setRightTab] = useState<"discussion" | "artifacts">("discussion");
  const [isEditing, setIsEditing] = useState(false);
  const [title, setTitle] = useState(initialTitle);
  const [isDropdownOpen, setIsDropdownOpen] = useState(false);
  const pickerRef = React.useRef<HTMLDivElement>(null);

  React.useEffect(() => {
    if (!isDropdownOpen) return;
    const onPointerDown = (event: MouseEvent | TouchEvent) => {
      if (pickerRef.current && !pickerRef.current.contains(event.target as Node)) {
        setIsDropdownOpen(false);
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setIsDropdownOpen(false);
    };
    document.addEventListener("mousedown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("mousedown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [isDropdownOpen]);

  return (
    <div className="compact-rail-header">
      <div className="compact-rail-left">
        <div className="compact-tab-group">
          <button
            type="button"
            className={`compact-tab-item ${rightTab === "discussion" ? "active" : ""}`}
            onClick={() => setRightTab("discussion")}
            title="讨论 (Discussion)"
            aria-label="讨论"
          >
            💬 {isNarrow ? "" : "讨论"}
          </button>
          <button
            type="button"
            className={`compact-tab-item ${rightTab === "artifacts" ? "active" : ""}`}
            onClick={() => setRightTab("artifacts")}
            title="全篇成果 (3)"
            aria-label="全篇成果 (3)"
          >
            🌐 {isNarrow ? "3" : "全篇成果 (3)"}
          </button>
        </div>

        {rightTab === "discussion" && (
          <div className="thread-title-picker-wrap" ref={pickerRef}>
            {isEditing ? (
              <input
                type="text"
                className="thread-title-inline-input"
                value={title}
                autoFocus
                onChange={(e) => setTitle(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    setIsEditing(false);
                    onRename(title);
                  }
                  if (e.key === "Escape") {
                    setIsEditing(false);
                  }
                }}
                onBlur={() => {
                  setIsEditing(false);
                  onRename(title);
                }}
              />
            ) : (
              <button
                type="button"
                className="thread-dropdown-btn"
                onDoubleClick={() => setIsEditing(true)}
                onClick={() => setIsDropdownOpen((prev) => !prev)}
              >
                <span className="thread-title-text">{title}</span>
                <span className="thread-dropdown-arrow">▾</span>
              </button>
            )}

            {isDropdownOpen && (
              <div className="thread-dropdown-menu">
                <div className="thread-dropdown-header">讨论分支</div>
              </div>
            )}
          </div>
        )}
      </div>
      <div data-testid="outside-area">Outside</div>
    </div>
  );
}

describe("Compact Rail Header and Inline Rename", () => {
  it("renders compact tab switcher and thread title in wide and narrow modes", () => {
    const { unmount } = render(<MockCompactRailHeader isNarrow={false} initialTitle="Neural Manifold Analysis" onRename={() => {}} />);
    expect(screen.getByText(/💬\s+讨论/)).toBeInTheDocument();
    expect(screen.getByText(/🌐\s+全篇成果 \(3\)/)).toBeInTheDocument();
    expect(screen.getByText("Neural Manifold Analysis")).toBeInTheDocument();
    unmount();

    render(<MockCompactRailHeader isNarrow={true} initialTitle="Neural Manifold Analysis" onRename={() => {}} />);
    expect(screen.getByRole("button", { name: "讨论" })).toHaveTextContent("💬");
    expect(screen.getByRole("button", { name: "全篇成果 (3)" })).toHaveTextContent("🌐 3");
  });

  it("enters inline editing on double click and saves on Enter", () => {
    const handleRename = vi.fn();
    render(<MockCompactRailHeader initialTitle="Main discussion" onRename={handleRename} />);

    const threadBtn = screen.getByRole("button", { name: /Main discussion/i });
    fireEvent.doubleClick(threadBtn);

    const input = screen.getByRole("textbox");
    expect(input).toBeInTheDocument();
    expect(input).toHaveValue("Main discussion");

    fireEvent.change(input, { target: { value: "Methodology Derivation" } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(handleRename).toHaveBeenCalledWith("Methodology Derivation");
  });

  it("toggles thread dropdown on single click and closes when clicking outside", () => {
    render(<MockCompactRailHeader onRename={() => {}} />);
    const threadBtn = screen.getByRole("button", { name: /Main discussion/i });
    fireEvent.click(threadBtn);
    expect(screen.getByText("讨论分支")).toBeInTheDocument();

    fireEvent.mouseDown(screen.getByTestId("outside-area"));
    expect(screen.queryByText("讨论分支")).not.toBeInTheDocument();
  });

  it("closes thread dropdown on Escape key", () => {
    render(<MockCompactRailHeader onRename={() => {}} />);
    const threadBtn = screen.getByRole("button", { name: /Main discussion/i });
    fireEvent.click(threadBtn);
    expect(screen.getByText("讨论分支")).toBeInTheDocument();

    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByText("讨论分支")).not.toBeInTheDocument();
  });

  it("handles more menu open and close in narrow mode", () => {
    function MockNarrowRailHeader() {
      const [isMoreOpen, setIsMoreOpen] = useState(false);
      const moreRef = React.useRef<HTMLDivElement>(null);

      React.useEffect(() => {
        if (!isMoreOpen) return;
        const onPointerDown = (event: MouseEvent) => {
          if (moreRef.current && !moreRef.current.contains(event.target as Node)) {
            setIsMoreOpen(false);
          }
        };
        document.addEventListener("mousedown", onPointerDown);
        return () => document.removeEventListener("mousedown", onPointerDown);
      }, [isMoreOpen]);

      return (
        <div className="compact-rail-header">
          <div className="compact-rail-right">
            <div className="rail-more-menu-wrap" ref={moreRef}>
              <button
                type="button"
                className="icon-pill-btn"
                onClick={() => setIsMoreOpen((prev) => !prev)}
                aria-label="更多操作"
              >
                •••
              </button>
              {isMoreOpen && (
                <div className="rail-more-dropdown">
                  <div className="rail-more-dropdown-item">＋ 新建讨论分支</div>
                  <div className="rail-more-dropdown-item">⛶ 全屏讨论树</div>
                </div>
              )}
            </div>
          </div>
          <div data-testid="outside-click-target">Outside</div>
        </div>
      );
    }

    render(<MockNarrowRailHeader />);
    const moreBtn = screen.getByRole("button", { name: /更多操作/i });
    fireEvent.click(moreBtn);

    expect(screen.getByText("＋ 新建讨论分支")).toBeInTheDocument();
    expect(screen.getByText("⛶ 全屏讨论树")).toBeInTheDocument();

    fireEvent.mouseDown(screen.getByTestId("outside-click-target"));
    expect(screen.queryByText("＋ 新建讨论分支")).not.toBeInTheDocument();
  });
});
