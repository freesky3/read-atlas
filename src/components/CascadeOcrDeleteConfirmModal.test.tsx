import { screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import React from "react";
import { CascadeOcrDeleteConfirmModal } from "./CascadeOcrDeleteConfirmModal";
import { renderWithLocale } from "../i18n/testUtils";

describe("CascadeOcrDeleteConfirmModal", () => {
  it("renders modal with warning details when open", () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();

    renderWithLocale(
      <CascadeOcrDeleteConfirmModal
        open={true}
        paperTitle="Attention Is All You Need"
        translationsCount={4}
        lensCount={2}
        guidesCount={12}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );

    expect(screen.getByText("删除 OCR 识别数据")).toBeInTheDocument();
    expect(screen.getByText("Attention Is All You Need")).toBeInTheDocument();
    expect(
      screen.getByText(/依赖当前 OCR 的翻译与普通解释成果/)
    ).toBeInTheDocument();
    expect(
      screen.getByText(/全部 Lens 深度分析及关联的局部 QA 问答/)
    ).toBeInTheDocument();
    expect(screen.getByText("确认级联删除")).toBeInTheDocument();

    fireEvent.click(screen.getByText("确认级联删除"));
    expect(onConfirm).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByText("取消"));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("does not render when open is false", () => {
    const { container } = renderWithLocale(
      <CascadeOcrDeleteConfirmModal
        open={false}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />
    );
    expect(container).toBeEmptyDOMElement();
  });
});
