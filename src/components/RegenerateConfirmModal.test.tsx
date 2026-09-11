import { fireEvent, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import RegenerateConfirmModal from "./RegenerateConfirmModal";
import { renderWithLocale } from "../i18n/testUtils";

describe("RegenerateConfirmModal", () => {
  it("renders nothing when closed", () => {
    const { container } = renderWithLocale(
      <RegenerateConfirmModal
        open={false}
        artifactTitle="Figure 1"
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(container.firstChild).toBeNull();
  });

  it("renders title, subtitle and confirmation details when open", () => {
    renderWithLocale(
      <RegenerateConfirmModal
        open={true}
        artifactTitle="Figure 2: Attention Architecture"
        kindLabel="Figure Lens"
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByText("重新生成 Figure Lens")).toBeInTheDocument();
    expect(screen.getByText("Figure 2: Attention Architecture")).toBeInTheDocument();
    expect(screen.getByText("确认重新生成")).toBeInTheDocument();
    expect(screen.getByText("取消")).toBeInTheDocument();
  });

  it("triggers onConfirm when confirmation button is clicked", () => {
    const onConfirm = vi.fn();
    renderWithLocale(
      <RegenerateConfirmModal
        open={true}
        artifactTitle="Formula 1"
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText("确认重新生成"));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it("triggers onCancel when cancel button or scrim is clicked", () => {
    const onCancel = vi.fn();
    renderWithLocale(
      <RegenerateConfirmModal
        open={true}
        artifactTitle="Formula 1"
        onConfirm={vi.fn()}
        onCancel={onCancel}
      />,
    );

    fireEvent.click(screen.getByText("取消"));
    expect(onCancel).toHaveBeenCalledTimes(1);

    const scrim = screen.getByRole("dialog");
    fireEvent.click(scrim);
    expect(onCancel).toHaveBeenCalledTimes(2);
  });

  it("triggers onCancel on Escape key press", () => {
    const onCancel = vi.fn();
    renderWithLocale(
      <RegenerateConfirmModal
        open={true}
        artifactTitle="Formula 1"
        onConfirm={vi.fn()}
        onCancel={onCancel}
      />,
    );

    fireEvent.keyDown(window, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(1);
  });
});
