import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import ArtifactPanel from "./ArtifactPanel";
import type { ArtifactProjection } from "./types";
import { renderWithLocale as render } from "./i18n/testUtils";

const lens: ArtifactProjection = {
  id: "lens-1",
  paperId: "paper-1",
  revisionId: "revision-1",
  ocrRevisionId: "ocr-1",
  kind: "lens_figure",
  objectKey: "block-1",
  version: 2,
  status: "ready",
  content: {
    quickTakeaway: { title: "The main trend", markdown: "Accuracy rises." },
    figure: {
      overallMarkdown: "The figure compares two conditions.",
      roleTags: [],
      panels: [],
      hotspots: [],
    },
    sections: [
      {
        sectionId: "trend",
        title: "Trend",
        markdown: "The blue curve rises.",
        evidenceIds: ["block:block-1"],
      },
    ],
  },
  overrides: {},
  evidence: [
    {
      revisionId: "revision-1",
      pageNumber: 4,
      blockId: "block-1",
      bbox: [100, 100, 800, 700],
      excerpt: "Figure 2",
    },
  ],
  dependencySnapshot: {},
  providerNodeId: "provider-node-1",
  createdAt: "2026-08-16T10:00:00Z",
};

describe("ArtifactPanel", () => {

  it.each(["figure", "table"])("shows the v2 %s reading guide and ordered observation meanings", (kind) => {
    const artifact: ArtifactProjection = { ...lens, kind: `lens_${kind}` as ArtifactProjection["kind"], content: {
      schemaVersion: 2, lensProtocol: "v2", status: "partial", limitations: ["右侧标签无法辨认。"],
      quickTakeaway: { title: "理解当前对象", markdown: "先把握主要比较。" }, sections: [], suggestedQuestions: [],
      [kind]: { overallMarkdown: "这组数据比较了两种条件。", readingGuideMarkdown: "先确认坐标或表头及单位，再比较同一条件。",
        focusPoints: [
          { location: "左侧 A 与 B", observationMarkdown: "A 的读数比 B 大。", meaningMarkdown: "差异只反映当前设置。", evidenceIds: [] },
          { location: "右侧对照", observationMarkdown: "另一组读数接近。", meaningMarkdown: "优势不能推广到全部设置。", evidenceIds: [] },
        ] },
    } };
    render(<ArtifactPanel artifacts={[artifact]} activeArtifactId={artifact.id} activeBlockId="block-1"
      displayCropSrc="" lensQa={[]} lensQaBusy={false} onSelect={vi.fn()} onJump={vi.fn()} onAskLens={vi.fn()}
      onTransferLens={vi.fn()} onSetOverride={vi.fn()} onGenerateBrief={vi.fn()} />);
    expect(screen.getByLabelText("Lens 解析状态")).toHaveTextContent("部分可解释");
    expect(screen.getByText("先确认坐标或表头及单位，再比较同一条件。")).toBeInTheDocument();
    const first = screen.getByText("左侧 A 与 B"); const second = screen.getByText("右侧对照");
    expect(first.compareDocumentPosition(second) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.getByText("A 的读数比 B 大。")).toBeInTheDocument();
    expect(screen.getByText("优势不能推广到全部设置。")).toBeInTheDocument();
    expect(screen.getByLabelText("Lens 材料局限")).toHaveTextContent("右侧标签无法辨认。");
  });

  it("shows an unavailable v2 formula without a fabricated formula or empty explanation headings", () => {
    const artifact: ArtifactProjection = { ...lens, kind: "lens_formula", content: {
      schemaVersion: 2, lensProtocol: "v2", status: "unavailable", limitations: ["公式中段无法辨认。"],
      quickTakeaway: { title: "当前公式暂不能可靠解释", markdown: "关键运算关系缺失。" },
      formula: { whatItDoesMarkdown: "", startHereMarkdown: "", reconstructedLatex: "", symbols: [] }, sections: [], suggestedQuestions: [],
    } };
    const { container, rerender } = render(<ArtifactPanel artifacts={[artifact]} activeArtifactId={artifact.id} activeBlockId="block-1"
      displayCropSrc="" lensQa={[]} lensQaBusy={false} onSelect={vi.fn()} onJump={vi.fn()} onAskLens={vi.fn()}
      onTransferLens={vi.fn()} onSetOverride={vi.fn()} onGenerateBrief={vi.fn()} />);
    expect(screen.getByLabelText("Lens 解析状态")).toHaveTextContent("暂无法解释");
    expect(container.querySelector(".artifact-formula")).toBeNull();
    expect(screen.queryByRole("heading", { name: "先直观理解" })).not.toBeInTheDocument();
    expect(screen.getByText("公式中段无法辨认。")).toBeInTheDocument();
    rerender(<ArtifactPanel artifacts={[lens]} activeArtifactId={lens.id} activeBlockId="block-1"
      displayCropSrc="" lensQa={[]} lensQaBusy={false} onSelect={vi.fn()} onJump={vi.fn()} onAskLens={vi.fn()}
      onTransferLens={vi.fn()} onSetOverride={vi.fn()} onGenerateBrief={vi.fn()} />);
    expect(screen.queryByLabelText("Lens 解析状态")).not.toBeInTheDocument();
    expect(screen.getByText("Accuracy rises.")).toBeInTheDocument();
  });

  it.each([
    ["translated", "已翻译", "译文正文", []],
    ["unchanged", "无需转换", "原有中文内容", []],
    ["partial", "部分完成", "结果 [文本缺损]", ["末尾比较对象无法辨认。"]],
    ["unavailable", "无法翻译", "", ["输入文本无法形成可靠译文。"]],
    [undefined, undefined, "旧版译文", []],
  ])("renders translation status %s without inventing historical status", (status, label, translation, notes) => {
    const artifact: ArtifactProjection = {
      ...lens, id: "translation-1", kind: "translation",
      content: { ...(status ? {status} : {}), sourceLanguage: status === "unavailable" ? "und" : "en", targetLanguage: "zh-CN", translation, notes, terms: [] },
    };
    render(<ArtifactPanel artifacts={[artifact]} activeArtifactId={artifact.id} activeBlockId="block-1"
      displayCropSrc="" lensQa={[]} lensQaBusy={false} onSelect={vi.fn()} onJump={vi.fn()}
      onAskLens={vi.fn()} onTransferLens={vi.fn()} onSetOverride={vi.fn()} onGenerateBrief={vi.fn()} />);
    if (label) expect(screen.getByLabelText("翻译状态")).toHaveTextContent(label);
    else expect(screen.queryByLabelText("翻译状态")).not.toBeInTheDocument();
    if (status === "unavailable") {
      expect(screen.getByText("输入文本无法形成可靠译文。")).toBeInTheDocument();
      expect(screen.getByText("源语言未确定")).toBeInTheDocument();
      expect(screen.queryByRole("heading", {name: "译文"})).not.toBeInTheDocument();
    } else if (status !== "partial") {
      expect(screen.getByText(translation as string)).toBeInTheDocument();
    }
    expect(screen.queryByText("术语对照")).not.toBeInTheDocument();
  });

  it("generates auxiliary artifacts only on explicit clicks and opts into Brief hints", async () => {
    const user = userEvent.setup();
    const generate = vi.fn();
    const generateBrief = vi.fn();
    render(<ArtifactPanel artifacts={[{ ...lens, id: "brief", kind: "brief", objectKey: "", content: { takeaway: "结论" } }]}
      activeArtifactId="brief" activeBlockId={null} displayCropSrc="" lensQa={[]} lensQaBusy={false}
      onSelect={vi.fn()} onJump={vi.fn()} onAskLens={vi.fn()} onTransferLens={vi.fn()} onSetOverride={vi.fn()}
      onGenerateBrief={generateBrief} onGenerateDocumentArtifact={generate} hideScopeTabs scopeTab="global" />);
    expect(generate).not.toHaveBeenCalled();
    expect(generateBrief).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "生成术语表" }));
    expect(generate).toHaveBeenLastCalledWith("glossary", false);
    await user.click(screen.getByRole("checkbox", { name: /术语表参考当前 Brief/ }));
    await user.click(screen.getByRole("button", { name: "生成术语表" }));
    expect(generate).toHaveBeenLastCalledWith("glossary", true);
    await user.click(screen.getByRole("button", { name: "生成符号表" }));
    expect(generate).toHaveBeenLastCalledWith("symbol_table", false);
    await user.click(screen.getByRole("button", { name: "生成元数据" }));
    expect(generate).toHaveBeenLastCalledWith("metadata", false);
    expect(generateBrief).not.toHaveBeenCalled();
  });

  it("keeps Lens QA and transfer as explicit actions in the Artifact tab", async () => {
    const user = userEvent.setup();
    const onAskLens = vi.fn().mockResolvedValue(undefined);
    const onTransferLens = vi.fn();

    render(
      <ArtifactPanel
        artifacts={[lens]}
        activeArtifactId={lens.id}
        activeBlockId={null}
        displayCropSrc=""
        lensQa={[]}
        lensQaBusy={false}
        onSelect={vi.fn()}
        onJump={vi.fn()}
        onAskLens={onAskLens}
        onTransferLens={onTransferLens}
        onSetOverride={vi.fn()}
        onGenerateBrief={vi.fn()}
        onOpenOutline={vi.fn()}
      />,
    );

    expect(screen.getByText("The main trend")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /论证地图/ })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /带到讨论中/ }));
    expect(onTransferLens).toHaveBeenCalledWith(lens);

    await user.click(screen.getByRole("button", { name: /深入探讨此区块/ }));

    await user.type(
      screen.getByPlaceholderText(/关于此 Lens 提问/i),
      "Why does the curve rise?",
    );
    await user.click(screen.getByLabelText("发送提问"));
    expect(onAskLens).toHaveBeenCalledWith("Why does the curve rise?", null);

    // Test Enter key sending
    await user.type(
      screen.getByPlaceholderText(/关于此 Lens 提问/i),
      "Second question{Enter}",
    );
    expect(onAskLens).toHaveBeenCalledWith("Second question", null);
    expect(
      screen.queryByRole("button", { name: /Brief/ }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "重新生成" })).not.toBeInTheDocument();
  });

  it("keeps generate available when other artifacts exist but Brief does not", async () => {
    const user = userEvent.setup();
    const onGenerateBrief = vi.fn();
    render(
      <ArtifactPanel
        artifacts={[lens]}
        activeArtifactId=""
        activeBlockId={null}
        displayCropSrc=""
        lensQa={[]}
        lensQaBusy={false}
        preferBrief
        onSelect={vi.fn()}
        onJump={vi.fn()}
        onAskLens={vi.fn()}
        onTransferLens={vi.fn()}
        onSetOverride={vi.fn()}
        onGenerateBrief={onGenerateBrief}
      />,
    );

    expect(screen.getByText("还没有 Brief")).toBeInTheDocument();
    await user.click(
      screen.getAllByRole("button", { name: "生成 Brief" })[0],
    );
    expect(onGenerateBrief).toHaveBeenCalled();
  });

  it("puts regenerate only inside an open Brief", () => {
    const brief: ArtifactProjection = {
      ...lens,
      id: "brief-1",
      kind: "brief",
      objectKey: "",
      content: {
        summary: "A paper about attention.",
        researchQuestion: "Can attention replace recurrence?",
        method: "Self-attention",
        findings: "It can.",
        limitations: "Data hungry",
        keywords: ["attention"],
      },
      evidence: [],
    };

    render(
      <ArtifactPanel
        artifacts={[brief, lens]}
        activeArtifactId={brief.id}
        activeBlockId={null}
        displayCropSrc=""
        lensQa={[]}
        lensQaBusy={false}
        onSelect={vi.fn()}
        onJump={vi.fn()}
        onAskLens={vi.fn()}
        onTransferLens={vi.fn()}
        onSetOverride={vi.fn()}
        onGenerateBrief={vi.fn()}
      />,
    );

    expect(screen.getByRole("button", { name: /重新生成/i })).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "生成 Brief" }),
    ).not.toBeInTheDocument();
  });

  it("saves glossary wording as an override layer", async () => {
    const user = userEvent.setup();
    const onSetOverride = vi.fn().mockResolvedValue(undefined);
    const glossary: ArtifactProjection = {
      ...lens,
      id: "glossary-1",
      kind: "glossary",
      objectKey: "",
      version: 1,
      content: {
        entries: [{ term: "Cache", definition: "Original wording" }],
      },
      overrides: {},
      evidence: [],
    };

    render(
      <ArtifactPanel
        artifacts={[glossary]}
        activeArtifactId={glossary.id}
        activeBlockId={null}
        displayCropSrc=""
        lensQa={[]}
        lensQaBusy={false}
        onSelect={vi.fn()}
        onJump={vi.fn()}
        onAskLens={vi.fn()}
        onTransferLens={vi.fn()}
        onSetOverride={onSetOverride}
        onGenerateBrief={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("button", { name: "编辑 Cache" }));
    const editor = screen.getByRole("textbox", { name: "编辑 Cache" });
    await user.clear(editor);
    await user.type(editor, "User wording");
    await user.click(screen.getByRole("button", { name: "保存词条" }));

    expect(onSetOverride).toHaveBeenCalledWith(glossary, "Cache", {
      definition: "User wording",
    });
  });

  it("renders structured mentor brief sections when present", () => {
    const brief: ArtifactProjection = {
      ...lens,
      id: "brief-mentor",
      kind: "brief",
      objectKey: "",
      content: {
        takeaway: "Establishes unified theory connecting neural field theory and low-rank RNNs.",
        keywords: ["Neural Manifolds", "RNN", "Topology"],
        classification: "Theoretical Neuroscience and Dynamical Systems",
        context: "Builds on Neural Field Theory and low-rank RNN models",
        backgroundAndProblem: "Bridging microscopic circuitry with macroscopic manifolds",
        coreMethod: "Continuous transfer function in abstract circuit space",
        findings: "Circuit structure bounds intrinsic dimension",
        evaluation: "Assumptions verified via SNN numerical simulations",
        futureWork: "Extending to non-autonomous dynamic inputs",
      },
      evidence: [],
    };

    render(
      <ArtifactPanel
        artifacts={[brief]}
        activeArtifactId={brief.id}
        activeBlockId={null}
        displayCropSrc=""
        lensQa={[]}
        lensQaBusy={false}
        onSelect={vi.fn()}
        onJump={vi.fn()}
        onAskLens={vi.fn()}
        onTransferLens={vi.fn()}
        onSetOverride={vi.fn()}
        onGenerateBrief={vi.fn()}
      />,
    );

    expect(screen.getByText(/Establishes unified theory/)).toBeInTheDocument();
    expect(screen.getByText("Neural Manifolds")).toBeInTheDocument();
    expect(screen.getByText("🧭 论文分类与主题定位")).toBeInTheDocument();
    expect(screen.getByText("🔗 学术脉络")).toBeInTheDocument();
    expect(screen.getByText("❓ 问题与动机")).toBeInTheDocument();
    expect(screen.getByText("🚀 未解问题与后续方向")).toBeInTheDocument();
    expect(screen.getByText("🛠️ 核心方法与设计")).toBeInTheDocument();
  });

  it("renders textbook brief with teaching takeaway title instead of academic breakthrough", () => {
    const brief: ArtifactProjection = {
      ...lens,
      id: "brief-textbook",
      kind: "brief",
      objectKey: "",
      content: {
        takeaway: "这一章介绍线性回归的模型、损失与梯度下降。",
        keywords: ["线性回归", "梯度下降"],
      },
      evidence: [],
    };

    render(
      <ArtifactPanel
        artifacts={[brief]}
        activeArtifactId={brief.id}
        activeBlockId={null}
        displayCropSrc=""
        lensQa={[]}
        lensQaBusy={false}
        onSelect={vi.fn()}
        onJump={vi.fn()}
        onAskLens={vi.fn()}
        onTransferLens={vi.fn()}
        onSetOverride={vi.fn()}
        onGenerateBrief={vi.fn()}
        documentKind="textbook"
      />,
    );

    expect(screen.getByText(/这一章介绍线性回归/)).toBeInTheDocument();
    expect(screen.getByText("📖 核心认识 (Takeaway)")).toBeInTheDocument();
    expect(screen.queryByText("💡 核心贡献与结论 (Takeaway)")).not.toBeInTheDocument();
  });

  it("renders model label and allows regenerating Lens artifact", async () => {
    const user = userEvent.setup();
    const onRegenerateLens = vi.fn();

    render(
      <ArtifactPanel
        artifacts={[lens]}
        activeArtifactId={lens.id}
        activeBlockId={null}
        displayCropSrc=""
        lensQa={[]}
        lensQaBusy={false}
        onSelect={vi.fn()}
        onJump={vi.fn()}
        onAskLens={vi.fn()}
        onTransferLens={vi.fn()}
        onSetOverride={vi.fn()}
        onGenerateBrief={vi.fn()}
        onRegenerateLens={onRegenerateLens}
        modelLabel="Gemini 2.5 Pro"
      />,
    );

    expect(screen.getByText(/由 Gemini 2.5 Pro 生成/)).toBeInTheDocument();
    const regenBtn = screen.getByRole("button", { name: /重新生成/i });
    expect(regenBtn).toBeInTheDocument();
    await user.click(regenBtn);
    expect(screen.getByText("重新生成 图像 Lens")).toBeInTheDocument();
    const confirmBtn = screen.getByRole("button", { name: "确认重新生成" });
    await user.click(confirmBtn);
    expect(onRegenerateLens).toHaveBeenCalledWith(lens);
  });

  it("groups overlapping Lens artifacts into a single group with version switcher and delete action", async () => {
    const user = userEvent.setup();
    const lensV1: ArtifactProjection = {
      ...lens,
      id: "lens-v1",
      objectKey: "block-old",
      version: 1,
      createdAt: "2026-08-16T10:00:00Z",
      content: {
        ...(lens.content as Record<string, unknown>),
        quickTakeaway: { title: "Version 1 Summary", markdown: "Old trend" },
      },
    };
    const lensV2: ArtifactProjection = {
      ...lens,
      id: "lens-v2",
      objectKey: "block-new",
      version: 2,
      createdAt: "2026-08-16T10:05:00Z",
      content: {
        ...(lens.content as Record<string, unknown>),
        quickTakeaway: { title: "Version 2 Summary", markdown: "New improved trend" },
      },
    };

    const onSelect = vi.fn();
    const onDeleteArtifactVersion = vi.fn();

    const { rerender } = render(
      <ArtifactPanel
        artifacts={[lensV1, lensV2]}
        activeArtifactId={lensV2.id}
        activeBlockId={null}
        displayCropSrc=""
        lensQa={[]}
        lensQaBusy={false}
        onSelect={onSelect}
        onJump={vi.fn()}
        onAskLens={vi.fn()}
        onTransferLens={vi.fn()}
        onSetOverride={vi.fn()}
        onGenerateBrief={vi.fn()}
        onDeleteArtifactVersion={onDeleteArtifactVersion}
        modelLabel="Gemini 2.5 Pro"
      />,
    );

    // Index bar should only have 1 card with version pill
    expect(screen.getByText("2 版本")).toBeInTheDocument();
    expect(screen.getByText("Version 2 Summary")).toBeInTheDocument();
    expect(screen.getByText("版本 2/2")).toBeInTheDocument();

    // Previous version button
    const prevBtn = screen.getByRole("button", { name: "查看上一版本" });
    expect(prevBtn).not.toBeDisabled();
    await user.click(prevBtn);
    expect(onSelect).toHaveBeenCalledWith(lensV1);

    // Rerender with v1 active
    rerender(
      <ArtifactPanel
        artifacts={[lensV1, lensV2]}
        activeArtifactId={lensV1.id}
        activeBlockId={null}
        displayCropSrc=""
        lensQa={[]}
        lensQaBusy={false}
        onSelect={onSelect}
        onJump={vi.fn()}
        onAskLens={vi.fn()}
        onTransferLens={vi.fn()}
        onSetOverride={vi.fn()}
        onGenerateBrief={vi.fn()}
        onDeleteArtifactVersion={onDeleteArtifactVersion}
        modelLabel="Gemini 2.5 Pro"
      />,
    );

    expect(screen.getByText("Version 1 Summary")).toBeInTheDocument();
    expect(screen.getByText("版本 1/2")).toBeInTheDocument();

    // Next version button
    const nextBtn = screen.getByRole("button", { name: "查看下一版本" });
    expect(nextBtn).not.toBeDisabled();
    await user.click(nextBtn);
    expect(onSelect).toHaveBeenCalledWith(lensV2);

    // Delete version button
    const deleteBtn = screen.getByRole("button", { name: "删除当前版本" });
    expect(deleteBtn).toBeInTheDocument();
    await user.click(deleteBtn);

    expect(screen.getByText("删除此版本（版本 1/2）")).toBeInTheDocument();
    const confirmDeleteBtn = screen.getByRole("button", { name: "确认删除" });
    await user.click(confirmDeleteBtn);
    expect(onDeleteArtifactVersion).toHaveBeenCalledWith(lensV1.id);
  });

  it("fills in the composer textarea when clicking Lens QA suggested question", async () => {
    const user = userEvent.setup();
    const lensWithSuggestions: ArtifactProjection = {
      ...lens,
      content: {
        ...(lens.content as Record<string, unknown>),
        suggestedQuestions: ["How does this scale with more layers?", "What happens when $z_i > 0$?"],
      },
    };

    render(
      <ArtifactPanel
        artifacts={[lensWithSuggestions]}
        activeArtifactId={lensWithSuggestions.id}
        activeBlockId={null}
        displayCropSrc=""
        lensQa={[]}
        lensQaBusy={false}
        onSelect={vi.fn()}
        onJump={vi.fn()}
        onAskLens={vi.fn()}
        onTransferLens={vi.fn()}
        onSetOverride={vi.fn()}
        onGenerateBrief={vi.fn()}
      />,
    );

    const qaBtn = screen.getByRole("button", { name: /深入探讨此区块/ });
    await user.click(qaBtn);

    expect(screen.getByText("💡 追问建议：")).toBeInTheDocument();
    const chip1 = screen.getByText("How does this scale with more layers?");
    expect(chip1).toBeInTheDocument();

    await user.click(chip1);
    const textarea = screen.getByPlaceholderText(/关于此 Lens 提问/i) as HTMLTextAreaElement;
    expect(textarea.value).toBe("How does this scale with more layers?");
  });

  it("supports Brief tags removal, addition and metadata pinning", async () => {
    const user = userEvent.setup();
    const onUpdateTags = vi.fn().mockResolvedValue(undefined);
    const onUpdateMetadata = vi.fn().mockResolvedValue(undefined);

    const briefArtifact: ArtifactProjection = {
      id: "brief-1",
      paperId: "paper-1",
      revisionId: "revision-1",
      ocrRevisionId: null,
      kind: "brief",
      objectKey: "",
      version: 1,
      status: "ready",
      content: {
        takeaway: "Transformer replaces RNNs with self-attention.",
        keywords: ["Attention", "Transformer", "NLP"],
      },
      overrides: {},
      evidence: [],
      dependencySnapshot: {},
      providerNodeId: null,
      createdAt: "2026-08-16T10:00:00Z",
    };

    const metaArtifact: ArtifactProjection = {
      id: "meta-1",
      paperId: "paper-1",
      revisionId: "revision-1",
      ocrRevisionId: null,
      kind: "metadata",
      objectKey: "",
      version: 1,
      status: "ready",
      content: {
        title: "Attention Is All You Need",
        authors: ["Ashish Vaswani", "Noam Shazeer"],
        publicationYear: 2017,
        venue: "NeurIPS",
        doi: "10.1234/5678",
        abstract: "The dominant sequence transduction models...",
        _pinned: ["venue"],
      },
      overrides: {},
      evidence: [],
      dependencySnapshot: {},
      providerNodeId: null,
      createdAt: "2026-08-16T10:00:00Z",
    };

    const { rerender } = render(
      <ArtifactPanel
        artifacts={[briefArtifact, metaArtifact]}
        activeArtifactId={briefArtifact.id}
        activeBlockId={null}
        displayCropSrc=""
        lensQa={[]}
        lensQaBusy={false}
        onSelect={vi.fn()}
        onJump={vi.fn()}
        onAskLens={vi.fn()}
        onTransferLens={vi.fn()}
        onSetOverride={vi.fn()}
        onGenerateBrief={vi.fn()}
        onUpdateTags={onUpdateTags}
        onUpdateMetadata={onUpdateMetadata}
      />,
    );

    // Brief keywords
    expect(screen.getByText("🏷️ 核心学术标签 (Keywords)：")).toBeInTheDocument();
    expect(screen.getByText("Transformer replaces RNNs with self-attention.")).toBeInTheDocument();
    expect(screen.getByText("Attention")).toBeInTheDocument();

    const removeBtn = screen.getByRole("button", { name: "移除标签 Attention" });
    await user.click(removeBtn);
    expect(onUpdateTags).toHaveBeenCalledWith("paper-1", ["Transformer", "NLP"]);

    // Metadata panel & Pinning
    rerender(
      <ArtifactPanel
        artifacts={[briefArtifact, metaArtifact]}
        activeArtifactId={metaArtifact.id}
        activeBlockId={null}
        displayCropSrc=""
        lensQa={[]}
        lensQaBusy={false}
        onSelect={vi.fn()}
        onJump={vi.fn()}
        onAskLens={vi.fn()}
        onTransferLens={vi.fn()}
        onSetOverride={vi.fn()}
        onGenerateBrief={vi.fn()}
        onUpdateTags={onUpdateTags}
        onUpdateMetadata={onUpdateMetadata}
      />,
    );

    expect(screen.getByRole("heading", { name: "Attention Is All You Need" })).toBeInTheDocument();
    const pinTitleBtn = screen.getByRole("button", { name: "图钉锁定" });
    await user.click(pinTitleBtn);
    expect(onUpdateMetadata).toHaveBeenCalledWith(
      "revision-1",
      expect.objectContaining({ title: "Attention Is All You Need" }),
      ["venue", "title"],
    );
  });

  it("supports Glossary & Symbol Table editing, pinning, deletion and adding rows", async () => {
    const user = userEvent.setup();
    const onUpdateOrientationTable = vi.fn().mockResolvedValue(undefined);

    const glossaryArtifact: ArtifactProjection = {
      id: "glossary-1",
      paperId: "paper-1",
      revisionId: "revision-1",
      ocrRevisionId: null,
      kind: "glossary",
      objectKey: "",
      version: 1,
      status: "ready",
      content: {
        entries: [
          {
            term: "Self-Attention",
            definition: "Mechanism relating different positions.",
            aliases: ["Intra-Attention", "$A_{ij}$"],
          },
        ],
        _pinned: [],
      },
      overrides: {},
      evidence: [],
      dependencySnapshot: {},
      providerNodeId: null,
      createdAt: "2026-08-16T10:00:00Z",
    };

    const { rerender } = render(
      <ArtifactPanel
        artifacts={[glossaryArtifact]}
        activeArtifactId={glossaryArtifact.id}
        activeBlockId={null}
        displayCropSrc=""
        lensQa={[]}
        lensQaBusy={false}
        onSelect={vi.fn()}
        onJump={vi.fn()}
        onAskLens={vi.fn()}
        onTransferLens={vi.fn()}
        onSetOverride={vi.fn()}
        onGenerateBrief={vi.fn()}
        onUpdateOrientationTable={onUpdateOrientationTable}
      />,
    );

    expect(screen.getByText("Self-Attention")).toBeInTheDocument();
    expect(screen.getByText("Mechanism relating different positions.")).toBeInTheDocument();

    // Toggle Pin on Glossary item
    const pinBtn = screen.getByRole("button", { name: "图钉锁定 Self-Attention" });
    await user.click(pinBtn);
    expect(onUpdateOrientationTable).toHaveBeenCalledWith(
      "revision-1",
      "glossary",
      expect.any(Array),
      ["legacy-0"],
      glossaryArtifact.id,
    );

    // Delete Glossary item
    const deleteBtn = screen.getByRole("button", { name: "删除 Self-Attention" });
    await user.click(deleteBtn);
    expect(onUpdateOrientationTable).toHaveBeenCalledWith(
      "revision-1",
      "glossary",
      [],
      [],
      glossaryArtifact.id,
    );

    // Add new row button
    const addBtn = screen.getByRole("button", { name: "添加新学术术语" });
    await user.click(addBtn);
    expect(screen.getByPlaceholderText("术语名...")).toBeInTheDocument();
  });

  it("switches active view and remembers last viewed artifact when switching scope tabs", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    const onScopeTabChange = vi.fn();

    const briefArtifact: ArtifactProjection = {
      id: "brief-test",
      paperId: "paper-1",
      revisionId: "revision-1",
      ocrRevisionId: null,
      kind: "brief",
      objectKey: "",
      version: 1,
      status: "ready",
      content: { takeaway: "Summary of paper." },
      overrides: {},
      evidence: [],
      dependencySnapshot: {},
      providerNodeId: null,
      createdAt: "2026-08-16T10:00:00Z",
    };

    const glossaryArtifact: ArtifactProjection = {
      id: "glossary-test",
      paperId: "paper-1",
      revisionId: "revision-1",
      ocrRevisionId: null,
      kind: "glossary",
      objectKey: "",
      version: 1,
      status: "ready",
      content: {
        entries: [{ term: "Backpropagation", definition: "Gradient descent method" }],
      },
      overrides: {},
      evidence: [],
      dependencySnapshot: {},
      providerNodeId: null,
      createdAt: "2026-08-16T10:00:00Z",
    };

    const lensArtifact: ArtifactProjection = {
      id: "lens-math-test",
      paperId: "paper-1",
      revisionId: "revision-1",
      ocrRevisionId: null,
      kind: "lens_formula",
      objectKey: "block-10",
      version: 1,
      status: "ready",
      content: {
        quickTakeaway: { title: "Softmax Formula", markdown: "Formula calculation" },
        formula: {
          whatItDoesMarkdown: "Computes probabilities",
          startHereMarkdown: "Look at numerator",
          reconstructedLatex: "P(y|x) = \\frac{e^{z_y}}{\\sum e^{z_i}}",
        },
      },
      overrides: {},
      evidence: [],
      dependencySnapshot: {},
      providerNodeId: null,
      createdAt: "2026-08-16T10:00:00Z",
    };

    const { rerender } = render(
      <ArtifactPanel
        artifacts={[briefArtifact, glossaryArtifact, lensArtifact]}
        activeArtifactId={glossaryArtifact.id}
        activeBlockId={null}
        displayCropSrc=""
        lensQa={[]}
        lensQaBusy={false}
        onSelect={onSelect}
        onJump={vi.fn()}
        onAskLens={vi.fn()}
        onTransferLens={vi.fn()}
        onSetOverride={vi.fn()}
        onGenerateBrief={vi.fn()}
        scopeTab="global"
        onScopeTabChange={onScopeTabChange}
        hideScopeTabs={false}
      />,
    );

    // Initial global tab with glossary active
    expect(screen.getByText("Backpropagation")).toBeInTheDocument();
    expect(screen.getByText("Gradient descent method")).toBeInTheDocument();

    // Click Lens scope tab
    const lensTabBtn = screen.getByRole("button", { name: /选区 Lens/ });
    await user.click(lensTabBtn);
    expect(onScopeTabChange).toHaveBeenCalledWith("block");
    expect(onSelect).toHaveBeenCalledWith(lensArtifact);

    // Rerender with block scope active
    rerender(
      <ArtifactPanel
        artifacts={[briefArtifact, glossaryArtifact, lensArtifact]}
        activeArtifactId={lensArtifact.id}
        activeBlockId={null}
        displayCropSrc=""
        lensQa={[]}
        lensQaBusy={false}
        onSelect={onSelect}
        onJump={vi.fn()}
        onAskLens={vi.fn()}
        onTransferLens={vi.fn()}
        onSetOverride={vi.fn()}
        onGenerateBrief={vi.fn()}
        scopeTab="block"
        onScopeTabChange={onScopeTabChange}
        hideScopeTabs={false}
      />,
    );

    expect(screen.getByText("Softmax Formula")).toBeInTheDocument();
    expect(screen.getByText("Computes probabilities")).toBeInTheDocument();

    // Switch back to global scope tab
    const globalTabBtn = screen.getByRole("button", { name: /全篇总览/ });
    await user.click(globalTabBtn);
    expect(onScopeTabChange).toHaveBeenCalledWith("global");
    // Should remember glossary as last viewed global artifact
    expect(onSelect).toHaveBeenCalledWith(glossaryArtifact);
  });

  it("displays custom generating state label when activeBlockLabel is provided", () => {
    render(
      <ArtifactPanel
        artifacts={[]}
        activeArtifactId=""
        activeBlockId="block-99"
        activeBlockLabel="explain"
        displayCropSrc=""
        lensQa={[]}
        lensQaBusy={false}
        onSelect={vi.fn()}
        onJump={vi.fn()}
        onAskLens={vi.fn()}
        onTransferLens={vi.fn()}
        onSetOverride={vi.fn()}
        onGenerateBrief={vi.fn()}
      />,
    );

    expect(
      screen.getAllByText("解释生成中...").length,
    ).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("正在生成段落解释...")).toBeInTheDocument();
  });

  it("copies selected Brief/Lens sections as GFM including ### titles", () => {
    const brief: ArtifactProjection = {
      ...lens,
      id: "brief-copy",
      kind: "brief",
      objectKey: "",
      content: {
        findings: "1. 甲\n2. 乙",
        evaluation: "自测重点。",
      },
      evidence: [],
    };

    render(
      <ArtifactPanel
        artifacts={[brief]}
        activeArtifactId={brief.id}
        activeBlockId={null}
        displayCropSrc=""
        lensQa={[]}
        lensQaBusy={false}
        onSelect={vi.fn()}
        onJump={vi.fn()}
        onAskLens={vi.fn()}
        onTransferLens={vi.fn()}
        onSetOverride={vi.fn()}
        onGenerateBrief={vi.fn()}
      />,
    );

    const body = document.querySelector(".artifact-detail-body") as HTMLElement;
    expect(body).toBeTruthy();
    const range = document.createRange();
    range.selectNodeContents(body);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);

    const data: Record<string, string> = {};
    const event = new Event("copy", { bubbles: true, cancelable: true });
    Object.defineProperty(event, "clipboardData", {
      value: {
        setData: (type: string, value: string) => {
          data[type] = value;
        },
        getData: (type: string) => data[type] ?? "",
      },
    });
    body.dispatchEvent(event);

    expect(event.defaultPrevented).toBe(true);
    expect(data["text/plain"]).toContain("### 📊 主要发现与结论");
    expect(data["text/plain"]).toContain("1. 甲");
    expect(data["text/plain"]).toContain("2. 乙");
    expect(data["text/plain"]).toContain("### ⚖️ 审辨式评估");
    expect(data["text/plain"]).toContain("自测重点。");
    expect(data["text/html"]).toBeUndefined();
    expect(data["text/plain"]).not.toContain("带到讨论");
  });
});
