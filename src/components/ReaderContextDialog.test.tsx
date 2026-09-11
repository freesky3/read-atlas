import { fireEvent, screen, waitFor } from "@testing-library/react";
import { renderWithLocale } from "../i18n/testUtils";
import { describe, expect, it, vi } from "vitest";
import ReaderContextDialog from "./ReaderContextDialog";

const emptyProjection = {
  scope: "workspace" as const,
  collectionPath: null,
  paperId: null,
  text: "",
  charCount: 0,
  warn: false,
  path: "workspace",
};

describe("ReaderContextDialog", () => {
  it("warns after 2000 characters and saves empty text", async () => {
    const save = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();
    renderWithLocale(
      <ReaderContextDialog
        target={{ scope: "workspace" }}
        load={async () => emptyProjection}
        save={save}
        onClose={onClose}
      />,
    );
    const box = await screen.findByPlaceholderText(/已掌握的知识点/);
    fireEvent.change(box, { target: { value: "字".repeat(2000) } });
    expect(screen.getByText(/不要贴论文摘要/)).toBeDefined();
    fireEvent.change(box, { target: { value: "   " } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(save).toHaveBeenCalledWith({ scope: "workspace" }, "   "));
    expect(onClose).toHaveBeenCalled();
  });

  it("centers a large split editor and live-renders markdown", async () => {
    renderWithLocale(
      <ReaderContextDialog
        target={{ scope: "paper" }}
        load={async () => emptyProjection}
        save={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    const dialog = await screen.findByRole("dialog");
    expect(dialog.className).toContain("reader-context-card");
    expect(dialog.parentElement?.className).toContain("reader-context-scrim");
    expect(screen.getByLabelText("编辑")).toBeDefined();
    expect(screen.getByLabelText("预览")).toBeDefined();
    expect(screen.getByText("输入 Markdown 后会在这里实时渲染。")).toBeDefined();
    fireEvent.change(screen.getByPlaceholderText(/已掌握的知识点/), {
      target: { value: "已掌握 **线性代数**" },
    });
    expect(screen.queryByText("输入 Markdown 后会在这里实时渲染。")).toBeNull();
    expect(screen.getByText("线性代数").tagName).toBe("STRONG");
  });
});
