import { screen, fireEvent } from "@testing-library/react";
import { renderWithLocale as render } from "../i18n/testUtils";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { GuidePlan, GuideProjection } from "../types";
import GuideIsland from "./GuideIsland";

const missing: GuideProjection = {
  status: "missing_ocr",
  revisionId: "rev-1",
  ocrRevisionId: null,
  hasPaperRoot: false,
  head: null,
  activeJobId: null,
};

const ready: GuideProjection = {
  ...missing,
  status: "ready_to_plan",
  ocrRevisionId: "ocr-1",
};

const published: GuideProjection = {
  status: "published",
  revisionId: "rev-1",
  ocrRevisionId: "ocr-1",
  hasPaperRoot: true,
  activeJobId: null,
  head: {
    id: "guide-1",
    status: "published",
    ocrRevisionId: "ocr-1",
    protocolVersion: "reading-guide-desktop-v1",
    promptVersion: "reading-guide-desktop-v1",
    model: "gemini-2.5-flash",
    language: "en",
    coverage: {},
    warnings: [],
    context: {},
    inks: [
      {
        id: "n1",
        kind: "note",
        speakerId: "alin",
        weight: "line",
        body: "Watch the gate.",
        anchor: {
          blockId: "b1",
          pageNumber: 2,
          blockType: "paragraph",
          bbox: [0, 0, 1, 1],
        },
      },
    ],
  },
};

const plan: GuidePlan = {
  revisionId: "rev-1",
  ocrRevisionId: "ocr-1",
  catalogDigest: "digest",
  model: "gemini-2.5-flash",
  pageCount: 12,
  batchCount: 2,
  understandCalls: 1,
  annotationCalls: 2,
  repairCalls: 2,
  reusedOutline: false,
  hasPaperRoot: true,
  supportsNativePdf: true,
  estimatedCost: null,
};

describe("GuideIsland", () => {
  it("asks the host to plan when OCR exists but no head is published", async () => {
    const user = userEvent.setup();
    const onOpen = vi.fn();
    render(
      <GuideIsland
        projection={ready}
        plan={null}
        layerVisible={false}
        busy={false}
        confirming={false}
        error={null}
        onOpen={onOpen}
        onConfirmGenerate={vi.fn()}
        onCancelConfirm={vi.fn()}
        onToggleLayer={vi.fn()}
        onRegenerate={vi.fn()}
        onDelete={vi.fn()}
      />,
    );
    await user.click(screen.getByRole("button", { name: "✎ 旁批" }));
    expect(onOpen).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("toggles the margin layer from the island without touching discussion", async () => {
    const user = userEvent.setup();
    const onOpen = vi.fn();
    const onToggleLayer = vi.fn();
    render(
      <GuideIsland
        projection={published}
        plan={null}
        layerVisible
        busy={false}
        confirming={false}
        error={null}
        onOpen={onOpen}
        onConfirmGenerate={vi.fn()}
        onCancelConfirm={vi.fn()}
        onToggleLayer={onToggleLayer}
        onRegenerate={vi.fn()}
        onDelete={vi.fn()}
      />,
    );
    const island = screen.getByRole("button", { name: /✎ 旁批 · 1/ });
    expect(island).toHaveAttribute("aria-pressed", "true");
    await user.click(island);
    expect(onOpen).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole("button", { name: "菜单" }));
    await user.click(screen.getByRole("button", { name: /隐藏旁批/i }));
    expect(onToggleLayer).toHaveBeenCalledTimes(1);
  });

  it("confirms generation from the plan card and can regenerate from the menu", async () => {
    const user = userEvent.setup();
    const onConfirmGenerate = vi.fn();
    const onRegenerate = vi.fn();
    const onDelete = vi.fn();
    const { rerender } = render(
      <GuideIsland
        projection={ready}
        plan={plan}
        layerVisible={false}
        busy={false}
        confirming
        error={null}
        onOpen={vi.fn()}
        onConfirmGenerate={onConfirmGenerate}
        onCancelConfirm={vi.fn()}
        onToggleLayer={vi.fn()}
        onRegenerate={onRegenerate}
        onDelete={onDelete}
      />,
    );
    expect(screen.getByRole("dialog", { name: "生成旁批确认" })).toHaveTextContent(
      "12 页 · 2 批",
    );
    await user.click(screen.getByRole("button", { name: "确认生成" }));
    expect(onConfirmGenerate).toHaveBeenCalledTimes(1);

    rerender(
      <GuideIsland
        projection={published}
        plan={null}
        layerVisible
        busy={false}
        confirming={false}
        error={null}
        onOpen={vi.fn()}
        onConfirmGenerate={onConfirmGenerate}
        onCancelConfirm={vi.fn()}
        onToggleLayer={vi.fn()}
        onRegenerate={onRegenerate}
        onDelete={onDelete}
      />,
    );
    await user.click(screen.getByRole("button", { name: "菜单" }));
    const regenerate = screen.getByRole("button", { name: /重新生成/i });
    expect(regenerate).toBeVisible();
    await user.click(regenerate);
    expect(onRegenerate).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole("button", { name: "菜单" }));
    await user.click(screen.getByRole("button", { name: /删除旁批/i }));
    expect(onDelete).toHaveBeenCalledTimes(1);
  });

  it("closes the menu on click-outside and on Escape key", async () => {
    const user = userEvent.setup();
    render(
      <div>
        <div data-testid="outside-area">Outside</div>
        <GuideIsland
          projection={published}
          plan={null}
          layerVisible
          busy={false}
          confirming={false}
          error={null}
          onOpen={vi.fn()}
          onConfirmGenerate={vi.fn()}
          onCancelConfirm={vi.fn()}
          onToggleLayer={vi.fn()}
          onRegenerate={vi.fn()}
          onDelete={vi.fn()}
        />
      </div>,
    );
    await user.click(screen.getByRole("button", { name: "菜单" }));
    expect(screen.getByRole("menu")).toBeInTheDocument();

    // Click outside closes menu
    await user.click(screen.getByTestId("outside-area"));
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();

    // Escape closes menu
    await user.click(screen.getByRole("button", { name: "菜单" }));
    expect(screen.getByRole("menu")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("blocks generation copy when OCR is missing", () => {
    render(
      <GuideIsland
        projection={missing}
        plan={null}
        layerVisible={false}
        busy={false}
        confirming={false}
        error={null}
        onOpen={vi.fn()}
        onConfirmGenerate={vi.fn()}
        onCancelConfirm={vi.fn()}
        onToggleLayer={vi.fn()}
        onRegenerate={vi.fn()}
        onDelete={vi.fn()}
      />,
    );
    expect(screen.getByTitle("先完成 OCR 再生成旁批")).toBeInTheDocument();
  });
});


it("keeps the generation draft while role settings are open", () => {
  const cancel = vi.fn();
  const props = { projection: ready, plan, confirming: true, layerVisible: false, busy: false, error: null,
    onOpen: vi.fn(), onConfirmGenerate: vi.fn(), onCancelConfirm: cancel, onToggleLayer: vi.fn(),
    onRegenerate: vi.fn(), onDelete: vi.fn() };
  const { rerender } = render(<GuideIsland {...props} suspended />);
  fireEvent.mouseDown(document.body);
  fireEvent.keyDown(window, { key: "Escape" });
  expect(cancel).not.toHaveBeenCalled();
  rerender(<GuideIsland {...props} suspended={false} />);
  expect(screen.getByRole("dialog", { name: "生成旁批确认" })).toBeInTheDocument();
});
