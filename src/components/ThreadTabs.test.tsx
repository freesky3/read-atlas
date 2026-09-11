import { screen } from "@testing-library/react";
import { renderWithLocale } from "../i18n/testUtils";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Thread } from "../types";
import ThreadTabs from "./ThreadTabs";

function thread(partial: Partial<Thread> & Pick<Thread, "id" | "title">): Thread {
  return {
    revisionId: "rev-1",
    kind: "global",
    status: "active",
    activeMessageId: null,
    createdAt: "2026-08-17T10:00:00Z",
    updatedAt: "2026-08-17T10:00:00Z",
    ...partial,
  };
}

const handlers = () => ({
  onSelect: vi.fn(),
  onCreate: vi.fn(),
  onClose: vi.fn(),
  onRename: vi.fn(),
  onRestore: vi.fn(),
  onDelete: vi.fn(),
});

describe("ThreadTabs", () => {
  it("shows every open thread and refuses to close the last one", async () => {
    const user = userEvent.setup();
    const actions = handlers();
    renderWithLocale(
      <ThreadTabs
        threads={[thread({ id: "t1", title: "Main discussion" })]}
        archivedThreads={[]}
        activeThreadId="t1"
        {...actions}
      />,
    );

    expect(screen.getByRole("tab", { name: "Main discussion" })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "关闭对话 Main discussion" }),
    ).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "＋ 新对话" }));
    expect(actions.onCreate).toHaveBeenCalledTimes(1);
  });

  it("closes, restores, and confirms delete from the archived list", async () => {
    const user = userEvent.setup();
    const actions = handlers();
    const closed = thread({
      id: "t2",
      title: "What is attention?",
      status: "archived",
    });
    renderWithLocale(
      <ThreadTabs
        threads={[
          thread({ id: "t1", title: "Main discussion" }),
          thread({ id: "t3", title: "新对话" }),
        ]}
        archivedThreads={[closed]}
        activeThreadId="t1"
        {...actions}
      />,
    );

    await user.click(screen.getByRole("button", { name: "关闭对话 新对话" }));
    expect(actions.onClose).toHaveBeenCalledWith(
      expect.objectContaining({ id: "t3" }),
    );

    await user.click(screen.getByRole("button", { name: "已关闭的对话" }));
    expect(screen.getByText("What is attention?")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "恢复" }));
    expect(actions.onRestore).toHaveBeenCalledWith(closed);

    await user.click(screen.getByRole("button", { name: "删除" }));
    await user.click(screen.getByRole("button", { name: "确认删除" }));
    expect(actions.onDelete).toHaveBeenCalledWith(closed);
  });

  it("renames a thread from a double-click", async () => {
    const user = userEvent.setup();
    const actions = handlers();
    const current = thread({ id: "t1", title: "新对话" });
    renderWithLocale(
      <ThreadTabs
        threads={[current]}
        archivedThreads={[]}
        activeThreadId="t1"
        {...actions}
      />,
    );

    await user.dblClick(screen.getByRole("tab", { name: "新对话" }));
    const editor = screen.getByRole("textbox", { name: "重命名对话" });
    await user.clear(editor);
    await user.type(editor, "Attention paper{enter}");
    expect(actions.onRename).toHaveBeenCalledWith(current, "Attention paper");
  });
});
