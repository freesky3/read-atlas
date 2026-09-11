import { screen, fireEvent } from "@testing-library/react";
import { renderWithLocale } from "../i18n/testUtils";
import { describe, it, expect, vi, beforeEach } from "vitest";
import LibraryHub from "./LibraryHub";
import type { CollectionProjection, DocumentCard, WorkspaceInfo } from "../types";

const mockDocs: DocumentCard[] = [
  {
    id: "doc-1",
    revisionId: "rev-1",
    title: "Attention Is All You Need",
    authors: "Vaswani et al.",
    year: "2017",
    pages: 15,
    collection: "Papers/Architectures/Attention",
    kind: "paper",
    sha256: "hash-1",
    pdfPath: "Papers/Architectures/Attention/attention.pdf",
    sourceStatus: "ready",
    briefStatus: "ready",
    briefTakeaway: "Proposes Transformer based entirely on self-attention mechanisms without recurrent units.",
    keywords: ["Transformer", "Self-Attention", "NLP"],
    lastReadPage: 3,
    importedAt: "2026-08-17T10:00:00Z",
    fileName: "attention.pdf",
  },
  {
    id: "doc-2",
    revisionId: "rev-2",
    title: "DeepSeek-R1",
    authors: "DeepSeek-AI",
    year: "2025",
    pages: 28,
    collection: "Papers/Reasoning/RL",
    kind: "paper",
    sha256: "hash-2",
    pdfPath: "Papers/Reasoning/RL/deepseek.pdf",
    sourceStatus: "ready",
    briefStatus: "ready",
    briefTakeaway: "Demonstrates reasoning capabilities emerging purely via reinforcement learning.",
    keywords: ["Reasoning", "Reinforcement Learning", "LLM"],
    lastReadPage: 1,
    importedAt: "2026-08-17T11:00:00Z",
    fileName: "deepseek.pdf",
  },
];

const mockWorkspace: WorkspaceInfo = {
  rootPath: "D:/Papers",
  databasePath: "D:/Papers/.read-desktop/read-desktop.sqlite3",
  libraryPath: "D:/Papers/Papers",
  textbooksPath: "D:/Papers/Textbooks",
  available: true,
  statusDetail: "ready",
};

describe("LibraryHub", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("renders papers in grid mode with complete takeaway and keyword chips", () => {
    const onSelectPaper = vi.fn();
    renderWithLocale(
      <LibraryHub
        documents={mockDocs}
        workspace={mockWorkspace}
        onSelectPaper={onSelectPaper}
        onImportPdf={vi.fn()}
        onOpenSettings={vi.fn()}
        onOpenOperations={vi.fn()}
      />
    );

    expect(screen.getByText("Attention Is All You Need")).toBeDefined();
    expect(
      screen.getAllByText(/Proposes Transformer based entirely on self-attention/).length,
    ).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("Transformer")).toBeDefined();
    expect(screen.getByText("Self-Attention")).toBeDefined();
  });

  it("renders textbook chapter badge when chapterNumber is present", () => {
    const docs: DocumentCard[] = [
      ...mockDocs,
      {
        id: "doc-3",
        revisionId: "rev-3",
        title: "线性回归",
        authors: "吴恩达",
        year: "",
        pages: 30,
        collection: "Textbooks/ML/linear-regression",
        kind: "textbook",
        sha256: "hash-3",
        pdfPath: "Textbooks/ML/linear-regression/ch3.pdf",
        sourceStatus: "ready",
        briefStatus: "ready",
        chapterNumber: "3",
        importedAt: "2026-08-17T12:00:00Z",
        fileName: "ch3.pdf",
      },
    ];
    renderWithLocale(
      <LibraryHub
        documents={docs}
        workspace={mockWorkspace}
        onSelectPaper={vi.fn()}
        onImportPdf={vi.fn()}
        onOpenSettings={vi.fn()}
        onOpenOperations={vi.fn()}
      />,
    );

    expect(screen.getAllByText(/第 3 章/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("教材").length).toBeGreaterThanOrEqual(1);
  });

  it("triggers onSelectPaper when a paper card is clicked", () => {
    const onSelectPaper = vi.fn();
    renderWithLocale(
      <LibraryHub
        documents={mockDocs}
        workspace={mockWorkspace}
        onSelectPaper={onSelectPaper}
        onImportPdf={vi.fn()}
        onOpenSettings={vi.fn()}
        onOpenOperations={vi.fn()}
      />
    );

    fireEvent.click(screen.getByText("Attention Is All You Need"));
    expect(onSelectPaper).toHaveBeenCalledWith("rev-1", "doc-1");
  });

  it("filters papers by title, authors and file name like hub_page text", () => {
    renderWithLocale(
      <LibraryHub
        documents={mockDocs}
        workspace={mockWorkspace}
        onSelectPaper={vi.fn()}
        onImportPdf={vi.fn()}
        onOpenSettings={vi.fn()}
        onOpenOperations={vi.fn()}
      />
    );

    const searchInput = screen.getByPlaceholderText(/检索/);
    fireEvent.change(searchInput, { target: { value: "DeepSeek" } });

    expect(screen.queryByText("Attention Is All You Need")).toBeNull();
    expect(screen.getByText("DeepSeek-R1")).toBeDefined();

    fireEvent.change(searchInput, { target: { value: "Attention" } });
    expect(screen.getByText("Attention Is All You Need")).toBeDefined();
    expect(screen.queryByText("DeepSeek-R1")).toBeNull();
  });

  it("switches to table view mode", () => {
    renderWithLocale(
      <LibraryHub
        documents={mockDocs}
        workspace={mockWorkspace}
        onSelectPaper={vi.fn()}
        onImportPdf={vi.fn()}
        onOpenSettings={vi.fn()}
        onOpenOperations={vi.fn()}
      />
    );

    const tableSwitchBtn = screen.getByText("≡ 结构表格");
    fireEvent.click(tableSwitchBtn);

    expect(screen.getAllByText("论文标题")[0]).toBeDefined();
    expect(screen.getByText("核心一句话摘要")).toBeDefined();
    expect(screen.getByText("关键词标签")).toBeDefined();
  });

  it("supports removing and adding tags with onUpdateTags callback", () => {
    const onUpdateTags = vi.fn().mockResolvedValue(undefined);
    renderWithLocale(
      <LibraryHub
        documents={mockDocs}
        workspace={mockWorkspace}
        onSelectPaper={vi.fn()}
        onImportPdf={vi.fn()}
        onOpenSettings={vi.fn()}
        onOpenOperations={vi.fn()}
        onUpdateTags={onUpdateTags}
      />
    );

    // Remove tag "Transformer"
    const removeBtn = screen.getByRole("button", { name: "移除标签 Transformer" });
    fireEvent.click(removeBtn);
    expect(onUpdateTags).toHaveBeenCalledWith("doc-1", ["Self-Attention", "NLP"]);

    // Add tag to doc-1 (second card sorted by importedAt)
    const addBtns = screen.getAllByRole("button", { name: "添加标签" });
    fireEvent.click(addBtns[1]);

    const input = screen.getByPlaceholderText("新标签...");
    fireEvent.change(input, { target: { value: "Deep Learning" } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(onUpdateTags).toHaveBeenCalledWith("doc-1", ["Transformer", "Self-Attention", "NLP", "Deep Learning"]);
  });

  it("always shows Papers and Textbooks roots in the folder tree", () => {
    renderWithLocale(
      <LibraryHub
        documents={mockDocs}
        workspace={mockWorkspace}
        onSelectPaper={vi.fn()}
        onImportPdf={vi.fn()}
        onOpenSettings={vi.fn()}
        onOpenOperations={vi.fn()}
      />,
    );

    expect(screen.getByText("▾ 📁 全部文档")).toBeDefined();
    expect(screen.getByText(/Papers/)).toBeDefined();
    expect(screen.getByText(/Textbooks/)).toBeDefined();
    expect(screen.getAllByText("论文").length).toBeGreaterThan(0);
  });

  it("disables manual/chapter sort in the all-documents view", () => {
    renderWithLocale(
      <LibraryHub
        documents={mockDocs}
        workspace={mockWorkspace}
        onSelectPaper={vi.fn()}
        onImportPdf={vi.fn()}
        onOpenSettings={vi.fn()}
        onOpenOperations={vi.fn()}
      />,
    );
    const select = screen.getByTitle("点进该书文件夹才能手排/按章节") as HTMLSelectElement;
    const manual = Array.from(select.options).find((o) => o.value === "manual")!;
    const chapter = Array.from(select.options).find((o) => o.value === "chapter")!;
    expect(manual.disabled).toBe(true);
    expect(chapter.disabled).toBe(true);
  });

  it("enables manual + chapter once a textbook leaf is selected and orders by chapter", () => {
    const docs: DocumentCard[] = [
      {
        id: "doc-3",
        revisionId: "rev-3",
        title: "梯度下降",
        authors: "吴恩达",
        year: "",
        pages: 30,
        collection: "Textbooks/ML",
        kind: "textbook",
        sha256: "hash-3",
        pdfPath: "Textbooks/ML/gd.pdf",
        sourceStatus: "ready",
        briefStatus: "ready",
        chapterNumber: "3.10",
        importedAt: "2026-08-17T12:00:00Z",
        fileName: "gd.pdf",
      },
      {
        id: "doc-1",
        revisionId: "rev-1",
        title: "导论",
        authors: "吴恩达",
        year: "",
        pages: 30,
        collection: "Textbooks/ML",
        kind: "textbook",
        sha256: "hash-1",
        pdfPath: "Textbooks/ML/intro.pdf",
        sourceStatus: "ready",
        briefStatus: "ready",
        chapterNumber: "1",
        importedAt: "2026-08-17T10:00:00Z",
        fileName: "intro.pdf",
      },
      {
        id: "doc-2",
        revisionId: "rev-2",
        title: "线性回归",
        authors: "吴恩达",
        year: "",
        pages: 30,
        collection: "Textbooks/ML",
        kind: "textbook",
        sha256: "hash-2",
        pdfPath: "Textbooks/ML/lr.pdf",
        sourceStatus: "ready",
        briefStatus: "ready",
        chapterNumber: "3",
        importedAt: "2026-08-17T11:00:00Z",
        fileName: "lr.pdf",
      },
    ];
    const collections: CollectionProjection[] = [
      { id: "col-ml", parentId: "Textbooks", name: "ML", relativePath: "Textbooks/ML", sortMode: "chapter", paperOrder: [] },
    ];
    renderWithLocale(
      <LibraryHub
        documents={docs}
        collections={collections}
        workspace={mockWorkspace}
        selectedFolder="Textbooks/ML"
        onSelectFolder={vi.fn()}
        onSelectPaper={vi.fn()}
        onImportPdf={vi.fn()}
        onOpenSettings={vi.fn()}
        onOpenOperations={vi.fn()}
      />,
    );
    const select = screen.getByTitle(/按章节/) as HTMLSelectElement;
    expect(select.value).toBe("chapter");
    const manual = Array.from(select.options).find((o) => o.value === "manual")!;
    expect(manual.disabled).toBe(false);
    // Card order should be chapter-sorted: 1 → 3 → 3.10 (import order, not alphabetical).
    const headings = screen.getAllByText(/导论|线性回归|梯度下降/);
    expect(headings[0].textContent).toBe("导论");
    expect(headings[1].textContent).toBe("线性回归");
    expect(headings[2].textContent).toBe("梯度下降");
  });

  it("reorders collection papers using the persisted manual order", () => {
    const docs: DocumentCard[] = [
      { ...mockDocs[0], id: "doc-1", title: "Attention Paper", collection: "Papers/ML", importedAt: "2026-08-17T10:00:00Z", chapterNumber: "3", keywords: ["Zulu"] },
      { ...mockDocs[0], id: "doc-2", revisionId: "rev-2", title: "DeepSeek Paper", collection: "Papers/ML", importedAt: "2026-08-17T11:00:00Z", chapterNumber: "3", keywords: ["Yankee"] },
    ];
    const collections: CollectionProjection[] = [
      { id: "col-ml", parentId: "Papers", name: "ML", relativePath: "Papers/ML", sortMode: "manual", paperOrder: ["doc-2", "doc-1"] },
    ];
    renderWithLocale(
      <LibraryHub
        documents={docs}
        collections={collections}
        workspace={mockWorkspace}
        selectedFolder="Papers/ML"
        onSelectFolder={vi.fn()}
        onSelectPaper={vi.fn()}
        onImportPdf={vi.fn()}
        onOpenSettings={vi.fn()}
        onOpenOperations={vi.fn()}
      />,
    );
    const deepseekTitle = screen.getByText("DeepSeek Paper");
    const attentionTitle = screen.getByText("Attention Paper");
    const order = (a: Element, b: Element) =>
      a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING;
    expect(order(deepseekTitle, attentionTitle)).toBeTruthy();
  });

  it("does not call onReorderPapers while a search query is active", () => {
    const docs: DocumentCard[] = [
      { ...mockDocs[0], id: "doc-1", title: "Alpha", collection: "Textbooks/ML", importedAt: "2026-08-17T10:00:00Z", chapterNumber: "1" },
      { ...mockDocs[0], id: "doc-2", revisionId: "rev-2", title: "Beta", collection: "Textbooks/ML", importedAt: "2026-08-17T11:00:00Z", chapterNumber: "2" },
    ];
    const collections: CollectionProjection[] = [
      { id: "col-ml", parentId: "Textbooks", name: "ML", relativePath: "Textbooks/ML", sortMode: "manual", paperOrder: ["doc-1", "doc-2"] },
    ];
    const onReorderPapers = vi.fn().mockResolvedValue(undefined);
    renderWithLocale(
      <LibraryHub
        documents={docs}
        collections={collections}
        workspace={mockWorkspace}
        selectedFolder="Textbooks/ML"
        onSelectFolder={vi.fn()}
        onSelectPaper={vi.fn()}
        onImportPdf={vi.fn()}
        onOpenSettings={vi.fn()}
        onOpenOperations={vi.fn()}
        onReorderPapers={onReorderPapers}
      />,
    );
    fireEvent.change(screen.getByPlaceholderText(/检索/), { target: { value: "a" } });
    const from = screen.getByText("Alpha").closest("[data-paper-id]");
    const to = screen.getByText("Beta").closest("[data-paper-id]");
    expect(from).not.toBeNull();
    expect(to).not.toBeNull();
    const shell = from!.closest(".app-shell");
    expect(shell).not.toBeNull();
    const originalHitTest = document.elementFromPoint;
    document.elementFromPoint = () => to;
    try {
      fireEvent.pointerDown(from!, { clientX: 10, clientY: 10, button: 0 });
      fireEvent.pointerMove(shell!, { clientX: 240, clientY: 10 });
      fireEvent.pointerUp(shell!, { clientX: 240, clientY: 10 });
      expect(onReorderPapers).not.toHaveBeenCalled();
    } finally {
      document.elementFromPoint = originalHitTest;
    }
  });

  it("mirrors manual order in table view", () => {
    const docs: DocumentCard[] = [
      { ...mockDocs[0], id: "doc-1", title: "Alpha Paper", collection: "Textbooks/ML", importedAt: "2026-08-17T10:00:00Z", chapterNumber: "1", keywords: ["Zulu"] },
      { ...mockDocs[0], id: "doc-2", revisionId: "rev-2", title: "Beta Paper", collection: "Textbooks/ML", importedAt: "2026-08-17T11:00:00Z", chapterNumber: "2", keywords: ["Yankee"] },
    ];
    const collections: CollectionProjection[] = [
      { id: "col-ml", parentId: "Textbooks", name: "ML", relativePath: "Textbooks/ML", sortMode: "manual", paperOrder: ["doc-2", "doc-1"] },
    ];
    renderWithLocale(
      <LibraryHub
        documents={docs}
        collections={collections}
        workspace={mockWorkspace}
        selectedFolder="Textbooks/ML"
        onSelectFolder={vi.fn()}
        onSelectPaper={vi.fn()}
        onImportPdf={vi.fn()}
        onOpenSettings={vi.fn()}
        onOpenOperations={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByText("≡ 结构表格"));
    const betaTitle = screen.getAllByText("Beta Paper")[0];
    const alphaTitle = screen.getAllByText("Alpha Paper")[0];
    const order = (a: Element, b: Element) =>
      a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING;
    expect(order(betaTitle, alphaTitle)).toBeTruthy();
  });

  it("renders an English settings control under the en catalog", () => {
    renderWithLocale(
      <LibraryHub
        documents={mockDocs}
        workspace={mockWorkspace}
        onSelectPaper={vi.fn()}
        onImportPdf={vi.fn()}
        onOpenSettings={vi.fn()}
        onOpenOperations={vi.fn()}
      />,
      "en",
    );
    expect(
      screen.getByRole("button", { name: "⚙ Settings" }),
    ).toBeInTheDocument();
  });
});
