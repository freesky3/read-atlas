import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import BlockCardStream from "./BlockCardStream";
import type { BlockArtifactBundle } from "./BlockCard";
import type { ArtifactProjection, OcrBlockProjection } from "../types";
import { renderWithLocale } from "../i18n/testUtils";

const mockParagraphBlock: OcrBlockProjection = {
  id: "para-1",
  pageNumber: 1,
  blockIndex: 0,
  blockType: "paragraph",
  textContent: "The Transformer architecture relies entirely on self-attention mechanisms.",
  bbox: [50, 100, 500, 200],
  contentDigest: "digest-1",
};

const mockFormulaBlock: OcrBlockProjection = {
  id: "formula-1",
  pageNumber: 1,
  blockIndex: 1,
  blockType: "formula",
  textContent: "Attention(Q, K, V) = softmax(\\frac{QK^T}{\\sqrt{d_k}})V",
  bbox: [50, 220, 500, 280],
  contentDigest: "digest-2",
};

const mockTranslationArtifact: ArtifactProjection = {
  id: "trans-1",
  paperId: "paper-1",
  revisionId: "rev-1",
  ocrRevisionId: "ocr-1",
  kind: "translation",
  objectKey: "para-1",
  version: 1,
  status: "ready",
  content: {
    translation: "Transformer 架构完全依赖于自注意力机制，无需循环或卷积操作。",
  },
  overrides: {},
  evidence: [
    {
      revisionId: "rev-1",
      pageNumber: 1,
      blockId: "para-1",
      bbox: [50, 100, 500, 200],
      excerpt: "The Transformer architecture...",
    },
  ],
  dependencySnapshot: {},
  providerNodeId: "provider-1",
  createdAt: "2026-08-16T10:00:00Z",
};

const textBundle: BlockArtifactBundle = {
  blockId: "para-1",
  pageNumber: 1,
  blockIndex: 0,
  blockType: "paragraph",
  textContent: "The Transformer architecture relies entirely on self-attention mechanisms.",
  bbox: [50, 100, 500, 200],
  groupsByKind: new Map([
    [
      "translation",
      {
        id: "translation:para-1",
        kind: "translation",
        versions: [mockTranslationArtifact],
        latestArtifact: mockTranslationArtifact,
      },
    ],
  ]),
  allArtifacts: [mockTranslationArtifact],
};

const formulaBundle: BlockArtifactBundle = {
  blockId: "formula-1",
  pageNumber: 1,
  blockIndex: 1,
  blockType: "formula",
  textContent: "Attention(Q, K, V) = softmax(\\frac{QK^T}{\\sqrt{d_k}})V",
  bbox: [50, 220, 500, 280],
  groupsByKind: new Map(),
  allArtifacts: [],
};

describe("BlockCardStream & BlockCard", () => {
  it("renders card stream with rich preview and category filter pills", async () => {
    const user = userEvent.setup();
    renderWithLocale(
      <BlockCardStream
        bundles={[textBundle, formulaBundle]}
        activeArtifactId={mockTranslationArtifact.id}
        activeBlockId="para-1"
        onSelectArtifact={vi.fn()}
        onJump={vi.fn()}
        lensQa={[]}
        lensQaBusy={false}
        ocrBlocks={[mockParagraphBlock, mockFormulaBlock]}
      />,
    );

    // Filter toolbar pills
    const toolbar = screen.getByRole("toolbar", { name: "区块类型筛选" });
    expect(within(toolbar).getByRole("button", { name: /^全部 \d+/ })).toBeInTheDocument();
    expect(within(toolbar).getByRole("button", { name: /文本/ })).toBeInTheDocument();
    expect(within(toolbar).getByRole("button", { name: /公式/ })).toBeInTheDocument();

    // Rendered text markdown in preview
    expect(
      screen.getByText(/The Transformer architecture relies entirely/),
    ).toBeInTheDocument();

    // Filter to only Formula
    await user.click(within(toolbar).getByRole("button", { name: /公式/ }));
    expect(
      screen.queryByText(/The Transformer architecture relies entirely/),
    ).not.toBeInTheDocument();
    // Formula card should be present
    expect(screen.getByText(/p\.1 · #2/)).toBeInTheDocument();
  });

  it("requires secondary confirmation before generating an ungenerated tab", async () => {
    const user = userEvent.setup();
    const onGenerateBlockAction = vi.fn();

    renderWithLocale(
      <BlockCardStream
        bundles={[textBundle]}
        activeArtifactId={mockTranslationArtifact.id}
        activeBlockId="para-1"
        onSelectArtifact={vi.fn()}
        onJump={vi.fn()}
        onGenerateBlockAction={onGenerateBlockAction}
        lensQa={[]}
        lensQaBusy={false}
        ocrBlocks={[mockParagraphBlock]}
      />,
    );

    // Initial generated tab is active
    expect(screen.getByRole("tab", { name: /翻译/ })).toBeInTheDocument();
    expect(
      screen.getByText(/Transformer 架构完全依赖于自注意力机制/),
    ).toBeInTheDocument();

    // Click ungenerated "解释" tab
    const explainTab = screen.getByRole("tab", { name: "解释" });
    await user.click(explainTab);

    // Pre-flight confirmation card appears
    expect(screen.getByText("尚未生成「解释」")).toBeInTheDocument();
    expect(
      screen.getByText(/点击下方按钮进行二次确认/),
    ).toBeInTheDocument();
    expect(onGenerateBlockAction).not.toHaveBeenCalled();

    // Click confirm generate button
    const confirmBtn = screen.getByRole("button", { name: "开始生成解释" });
    await user.click(confirmBtn);

    expect(onGenerateBlockAction).toHaveBeenCalledWith(
      mockParagraphBlock,
      "explain",
    );
  });

  it("handles jump to PDF block navigation", async () => {
    const user = userEvent.setup();
    const onJump = vi.fn();

    renderWithLocale(
      <BlockCardStream
        bundles={[textBundle]}
        activeArtifactId={mockTranslationArtifact.id}
        activeBlockId="para-1"
        onSelectArtifact={vi.fn()}
        onJump={onJump}
        lensQa={[]}
        lensQaBusy={false}
        ocrBlocks={[mockParagraphBlock]}
      />,
    );

    const jumpBadge = screen.getByTitle("点击跳转至 PDF 原文");
    await user.click(jumpBadge);

    expect(onJump).toHaveBeenCalledWith(1, "para-1");
  });

  it("transitions to dedicated QA page, renders history and suggested questions, and returns to stream", async () => {
    const user = userEvent.setup();
    const onAskLens = vi.fn().mockResolvedValue(undefined);
    const onSelectArtifact = vi.fn();

    const mockLensArtifact: ArtifactProjection = {
      id: "lens-1",
      paperId: "paper-1",
      revisionId: "rev-1",
      ocrRevisionId: "ocr-1",
      kind: "lens_formula",
      objectKey: "formula-1",
      version: 1,
      status: "ready",
      content: {
        quickTakeaway: {
          markdown: "此公式定义了缩放点积注意力，通过除以根号 dk 维持梯度稳定。",
        },
        formula: {
          latex: "Attention(Q, K, V) = softmax(\\frac{QK^T}{\\sqrt{d_k}})V",
        },
        suggestedQuestions: [
          "为什么需要除以根号 dk？",
        ],
      },
      overrides: {},
      evidence: [],
      dependencySnapshot: {},
      providerNodeId: "provider-1",
      createdAt: "2026-08-16T10:05:00Z",
    };

    const generatedFormulaBundle: BlockArtifactBundle = {
      blockId: "formula-1",
      pageNumber: 1,
      blockIndex: 1,
      blockType: "formula",
      textContent: "Attention(Q, K, V) = softmax(\\frac{QK^T}{\\sqrt{d_k}})V",
      bbox: [50, 220, 500, 280],
      groupsByKind: new Map([
        [
          "lens_formula",
          {
            id: "lens_formula:formula-1",
            kind: "lens_formula",
            versions: [mockLensArtifact],
            latestArtifact: mockLensArtifact,
          },
        ],
      ]),
      allArtifacts: [mockLensArtifact],
    };

    const { rerender } = renderWithLocale(
      <BlockCardStream
        bundles={[generatedFormulaBundle]}
        activeArtifactId={mockLensArtifact.id}
        activeBlockId="formula-1"
        displayCropSrc="data:image/png;base64,mockCrop"
        onSelectArtifact={onSelectArtifact}
        onJump={vi.fn()}
        onAskLens={onAskLens}
        lensQa={[]}
        lensQaBusy={false}
        ocrBlocks={[mockFormulaBlock]}
      />
    );

    // Entry button is visible on card
    const qaEntryBtn = screen.getByRole("button", { name: /深入探讨此区块/ });
    expect(qaEntryBtn).toBeInTheDocument();

    // Click to enter dedicated QA page
    await user.click(qaEntryBtn);
    expect(onSelectArtifact).toHaveBeenCalledWith(mockLensArtifact);

    // Dedicated QA page is now rendered
    expect(screen.getByLabelText("返回区块列表")).toBeInTheDocument();
    expect(screen.getByAltText("选区图表片段")).toHaveAttribute(
      "src",
      "data:image/png;base64,mockCrop"
    );
    expect(screen.getByText("此公式定义了缩放点积注意力，通过除以根号 dk 维持梯度稳定。")).toBeInTheDocument();
    expect(screen.getByText("Lens 追问")).toBeInTheDocument();
    expect(screen.getByText("为什么需要除以根号 dk？")).toBeInTheDocument();

    // Click suggested question chip to populate textarea
    const suggestionChip = screen.getByText("为什么需要除以根号 dk？");
    await user.click(suggestionChip);

    // Textarea has the question and is in pinned bottom bar
    const textarea = screen.getByPlaceholderText(/关于此 Lens 提问/);
    expect(textarea).toHaveValue("为什么需要除以根号 dk？");
    expect(textarea.closest("footer")).toHaveClass("block-qa-bottom-bar");
    expect(textarea.style.overflowY).toBe("hidden");

    // Click send
    const sendBtn = screen.getByLabelText("发送提问");
    await user.click(sendBtn);

    expect(onAskLens).toHaveBeenCalledWith("为什么需要除以根号 dk？", null);

    // Re-render with a Q&A message
    rerender(
      <BlockCardStream
        bundles={[generatedFormulaBundle]}
        activeArtifactId={mockLensArtifact.id}
        activeBlockId="formula-1"
        onSelectArtifact={onSelectArtifact}
        onJump={vi.fn()}
        onAskLens={onAskLens}
        lensQa={[
          {
            id: "qa-1",
            lensArtifactId: "lens-1",
            parentId: null,
            role: "user",
            content: "为什么需要除以根号 dk？",
            status: "complete",
            providerNodeId: "provider-1",
            createdAt: "2026-08-16T10:06:00Z",
          },
          {
            id: "qa-2",
            lensArtifactId: "lens-1",
            parentId: "qa-1",
            role: "assistant",
            content: "防止点积数值过大导致 softmax 梯度极小。",
            status: "complete",
            providerNodeId: "provider-1",
            createdAt: "2026-08-16T10:06:05Z",
          },
        ]}
        lensQaBusy={false}
        ocrBlocks={[mockFormulaBlock]}
      />,
    );

    expect(screen.getByText("防止点积数值过大导致 softmax 梯度极小。")).toBeInTheDocument();

    // Click back button to return to stream
    const backBtn = screen.getByLabelText("返回区块列表");
    await user.click(backBtn);

    // Returned to card stream
    expect(screen.getByRole("button", { name: /深入探讨此区块/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "返回区块列表" })).not.toBeInTheDocument();
  });

  it("toggles all cards expanded or collapsed via the toggle button", async () => {
    const user = userEvent.setup();
    renderWithLocale(
      <BlockCardStream
        bundles={[textBundle, formulaBundle]}
        activeArtifactId={null}
        activeBlockId={null}
        onSelectArtifact={vi.fn()}
        onJump={vi.fn()}
        lensQa={[]}
        lensQaBusy={false}
        ocrBlocks={[mockParagraphBlock, mockFormulaBlock]}
      />,
    );

    // Initial state: first card is expanded by default, formula card is not expanded
    const toggleBtn = screen.getByRole("button", { name: /全部展开/ });
    expect(toggleBtn).toBeInTheDocument();

    // Click to expand all
    await user.click(toggleBtn);

    // Now all cards are expanded, button aria-label should switch to "全部收起"
    const collapseBtn = screen.getByRole("button", { name: /全部收起/ });
    expect(collapseBtn).toBeInTheDocument();

    // Click to collapse all
    await user.click(collapseBtn);

    // Now all cards are collapsed, button aria-label switches back to "全部展开"
    expect(screen.getByRole("button", { name: /全部展开/ })).toBeInTheDocument();
  });
});
