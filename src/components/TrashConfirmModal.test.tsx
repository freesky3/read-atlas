import { screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { TrashConfirmModal } from "./TrashConfirmModal";
import { renderWithLocale } from "../i18n/testUtils";
describe("TrashConfirmModal", () => {
  it("renders nothing when closed", () => {
    const { container } = renderWithLocale(
      <TrashConfirmModal
        open={false}
        paperTitle="Attention Is All You Need"
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />
    );
    expect(container.firstChild).toBeNull();
  });

  it("renders paper title and details when open", () => {
    renderWithLocale(
      <TrashConfirmModal
        open={true}
        paperTitle="Attention Is All You Need"
        paperPath="Architectures/attention.pdf"
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />
    );
    expect(screen.getByText("移入废纸篓确认")).toBeDefined();
    expect(screen.getByText("Attention Is All You Need")).toBeDefined();
    expect(screen.getByText("Architectures/attention.pdf")).toBeDefined();
  });

  it("calls onCancel when Cancel button is clicked", () => {
    const onCancel = vi.fn();
    renderWithLocale(
      <TrashConfirmModal
        open={true}
        paperTitle="Attention Is All You Need"
        onConfirm={vi.fn()}
        onCancel={onCancel}
      />
    );
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("calls onConfirm when Confirm button is clicked", () => {
    const onConfirm = vi.fn();
    renderWithLocale(
      <TrashConfirmModal
        open={true}
        paperTitle="Attention Is All You Need"
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />
    );
    fireEvent.click(screen.getByRole("button", { name: "确认移入废纸篓" }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it("calls onCancel on Escape key press", () => {
    const onCancel = vi.fn();
    renderWithLocale(
      <TrashConfirmModal
        open={true}
        paperTitle="Attention Is All You Need"
        onConfirm={vi.fn()}
        onCancel={onCancel}
      />
    );
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(1);
  });
});
