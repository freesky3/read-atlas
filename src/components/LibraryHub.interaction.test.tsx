// D-063 PR 2 Gate 组件测：选择、把手拖拽、键盘替代、目录树与「即将支持」边界。
// 合同：docs/library-workspace-plan-2026-08.md §6.2–§6.5、§12。
import type { ComponentProps } from "react";
import { screen, fireEvent, within, waitFor } from "@testing-library/react";
import { renderWithLocale } from "../i18n/testUtils";
import { describe, it, expect, vi, beforeEach } from "vitest";
import LibraryHub from "./LibraryHub";
import type { CollectionProjection, DocumentCard, WorkspaceInfo } from "../types";
import { COACH_MARK_STORAGE_KEY } from "../library/useLibraryWorkspace";
import { treeStorageKey } from "../library/treeModel";
import { createMemoryLibraryWorkspaceClient, type LibraryWorkspaceClient } from "../library/libraryWorkspaceClient";
import { defaultHubCardLifecycle, type HubPaperCard } from "../library/libraryWorkspaceTypes";

function makeDoc(id: string, title: string, collection: string, importedAt: string, extra: Partial<DocumentCard> = {}): DocumentCard {
  return {
    id,
    revisionId: `rev-${id}`,
    title,
    authors: "A. Author",
    year: "2026",
    pages: 10,
    collection,
    kind: collection.startsWith("Textbooks") ? "textbook" : "paper",
    sha256: `hash-${id}`,
    pdfPath: `${collection}/${id}.pdf`,
    sourceStatus: "ready",
    briefStatus: "ready",
    importedAt,
    fileName: `${id}.pdf`,
    ...extra,
  };
}

const LEAF = "Papers/ML";
const layerDocs: DocumentCard[] = [
  makeDoc("doc-a", "Alpha", LEAF, "2026-08-17T10:00:00Z"),
  makeDoc("doc-b", "Beta", LEAF, "2026-08-17T11:00:00Z"),
  makeDoc("doc-c", "Gamma", LEAF, "2026-08-17T12:00:00Z"),
];
const leafCollection: CollectionProjection = {
  id: "col-ml",
  parentId: "Papers",
  name: "ML",
  relativePath: LEAF,
  sortMode: "manual",
  paperOrder: ["doc-a", "doc-b", "doc-c"],
};

function workspaceWith(rootPath: string): WorkspaceInfo {
  return {
    rootPath,
    databasePath: `${rootPath}/.read-desktop/read-desktop.sqlite3`,
    libraryPath: `${rootPath}/Papers`,
    textbooksPath: `${rootPath}/Textbooks`,
    available: true,
    statusDetail: "ready",
  };
}

function renderHub(overrides: Partial<ComponentProps<typeof LibraryHub>> = {}) {
  const base: ComponentProps<typeof LibraryHub> = {
    documents: layerDocs,
    collections: [leafCollection],
    workspace: workspaceWith("D:/Papers"),
    selectedFolder: LEAF,
    onSelectFolder: vi.fn(),
    onSelectPaper: vi.fn(),
    onImportPdf: vi.fn(),
    onOpenSettings: vi.fn(),
    onOpenOperations: vi.fn(),
  };
  const utils = renderWithLocale(<LibraryHub {...base} {...overrides} />);
  // 首次升级导览是模态：先关掉，让各条测聚焦在被测交互上。
  const dismiss = screen.queryByRole("button", { name: "知道了" });
  if (dismiss) fireEvent.click(dismiss);
  return utils;
}

function cardOf(title: string): HTMLElement {
  return screen.getByText(title).closest("[data-paper-id]") as HTMLElement;
}

function hubCardFromDoc(doc: DocumentCard): HubPaperCard {
  return {
    id: doc.id,
    revisionId: doc.revisionId,
    title: doc.title,
    authors: [doc.authors],
    publicationYear: 2026,
    pageCount: 10,
    collectionPath: doc.collection,
    relativePath: `${doc.collection}/${doc.fileName}`,
    fileName: doc.fileName,
    kind: doc.kind,
    sha256: doc.sha256,
    byteSize: 1,
    importedAt: doc.importedAt,
    tags: doc.keywords ?? [],
    hasOcr: Boolean(doc.hasOcr),
    briefStatus: doc.briefStatus,
    briefTakeaway: doc.briefTakeaway ?? null,
    keywords: doc.keywords ?? [],
    chapterNumber: doc.chapterNumber ?? null,
    sortKey: doc.importedAt,
    ...defaultHubCardLifecycle(),
  };
}

function handleOf(title: string): HTMLElement {
  return screen.getByRole("button", { name: `拖动排序或移动：${title}` });
}

function shellOf(title: string): HTMLElement {
  return cardOf(title).closest(".app-shell") as HTMLElement;
}

/** 把手拖到目标卡片右半区（jsdom 的 rect 全为 0，clientX>0 即 after）。 */
function dragHandleOnto(handle: HTMLElement, target: HTMLElement, shell: HTMLElement) {
  const original = document.elementFromPoint;
  document.elementFromPoint = () => target;
  try {
    fireEvent.pointerDown(handle, { clientX: 0, clientY: 0, button: 0, pointerId: 1 });
    fireEvent.pointerMove(shell, { clientX: 200, clientY: 0, pointerId: 1 });
    fireEvent.pointerUp(shell, { clientX: 200, clientY: 0, pointerId: 1 });
  } finally {
    document.elementFromPoint = original;
  }
}

describe("LibraryHub — 把手拖拽（PR 2 Gate）", () => {
  beforeEach(() => localStorage.clear());

  it("拖到第一张卡左侧空隙放到最前", async () => {
    const onReorderPapers = vi.fn().mockResolvedValue(undefined);
    renderHub({ onReorderPapers });
    const first = cardOf("Alpha");
    const grid = first.closest(".paper-cards-grid") as HTMLElement;
    vi.spyOn(first, "getBoundingClientRect").mockReturnValue({
      x: 120,
      y: 40,
      left: 120,
      top: 40,
      right: 420,
      bottom: 220,
      width: 300,
      height: 180,
      toJSON: () => ({}),
    });
    const original = document.elementFromPoint;
    document.elementFromPoint = () => grid;
    try {
      fireEvent.pointerDown(handleOf("Beta"), { clientX: 0, clientY: 0, button: 0, pointerId: 1 });
      fireEvent.pointerMove(shellOf("Beta"), { clientX: 40, clientY: 80, pointerId: 1 });
      fireEvent.pointerUp(shellOf("Beta"), { clientX: 40, clientY: 80, pointerId: 1 });
    } finally {
      document.elementFromPoint = original;
    }
    await vi.waitFor(() => expect(onReorderPapers).toHaveBeenCalled());
    expect(onReorderPapers).toHaveBeenCalledWith("col-ml", ["doc-b", "doc-a", "doc-c"]);
  });

  it("把手拖到另一张卡之后，提交本层全部 live id 的完整精确置换", async () => {
    const onReorderPapers = vi.fn().mockResolvedValue(undefined);
    renderHub({ onReorderPapers });
    dragHandleOnto(handleOf("Alpha"), cardOf("Beta"), shellOf("Alpha"));
    await vi.waitFor(() => expect(onReorderPapers).toHaveBeenCalled());
    expect(onReorderPapers).toHaveBeenCalledWith("col-ml", ["doc-b", "doc-a", "doc-c"]);
  });

  it("卡片正文的 pointerdown 不启动拖拽，点击正文仍然是打开 Reader", async () => {
    const onReorderPapers = vi.fn();
    const onSelectPaper = vi.fn();
    renderHub({ onReorderPapers, onSelectPaper });
    const card = cardOf("Alpha");
    const shell = shellOf("Alpha");
    const original = document.elementFromPoint;
    document.elementFromPoint = () => cardOf("Beta");
    try {
      fireEvent.pointerDown(card, { clientX: 0, clientY: 0, button: 0, pointerId: 1 });
      fireEvent.pointerMove(shell, { clientX: 200, clientY: 0, pointerId: 1 });
      fireEvent.pointerUp(shell, { clientX: 200, clientY: 0, pointerId: 1 });
    } finally {
      document.elementFromPoint = original;
    }
    expect(onReorderPapers).not.toHaveBeenCalled();
    fireEvent.click(within(card).getByText("Alpha"));
    expect(onSelectPaper).toHaveBeenCalledWith("rev-doc-a", "doc-a");
  });

  it("有效落点画出高对比插入线，落在自己卡片上不画", () => {
    renderHub({ onReorderPapers: vi.fn() });
    const shell = shellOf("Alpha");
    const original = document.elementFromPoint;
    document.elementFromPoint = () => cardOf("Beta");
    try {
      fireEvent.pointerDown(handleOf("Alpha"), { clientX: 0, clientY: 0, button: 0, pointerId: 1 });
      fireEvent.pointerMove(shell, { clientX: 200, clientY: 0, pointerId: 1 });
      expect(cardOf("Beta").className).toContain("insert-after");
      document.elementFromPoint = () => cardOf("Alpha");
      fireEvent.pointerMove(shell, { clientX: 210, clientY: 0, pointerId: 1 });
      expect(cardOf("Beta").className).not.toContain("insert-");
    } finally {
      document.elementFromPoint = original;
      fireEvent.pointerUp(shell, { clientX: 210, clientY: 0, pointerId: 1 });
    }
  });

  it("未超过 8px 阈值的按下松手不算拖拽", () => {
    const onReorderPapers = vi.fn();
    renderHub({ onReorderPapers });
    const handle = handleOf("Alpha");
    const shell = shellOf("Alpha");
    const original = document.elementFromPoint;
    document.elementFromPoint = () => cardOf("Beta");
    try {
      fireEvent.pointerDown(handle, { clientX: 100, clientY: 100, button: 0, pointerId: 1 });
      fireEvent.pointerMove(shell, { clientX: 104, clientY: 100, pointerId: 1 });
      fireEvent.pointerUp(shell, { clientX: 104, clientY: 100, pointerId: 1 });
    } finally {
      document.elementFromPoint = original;
    }
    expect(onReorderPapers).not.toHaveBeenCalled();
  });

  it("结构表格共享同一把手拖拽与完整置换", async () => {
    const onReorderPapers = vi.fn().mockResolvedValue(undefined);
    renderHub({ onReorderPapers });
    fireEvent.click(screen.getByText("≡ 结构表格"));
    dragHandleOnto(handleOf("Alpha"), cardOf("Beta"), shellOf("Alpha"));
    await vi.waitFor(() => expect(onReorderPapers).toHaveBeenCalled());
    expect(onReorderPapers).toHaveBeenCalledWith("col-ml", ["doc-b", "doc-a", "doc-c"]);
  });

  it("搜索时 reorder 被禁止，并把同一原因写进 aria-live 区域", async () => {
    const onReorderPapers = vi.fn();
    renderHub({ onReorderPapers });
    fireEvent.change(screen.getByPlaceholderText(/检索/), { target: { value: "a" } });
    const shell = shellOf("Alpha");
    const original = document.elementFromPoint;
    document.elementFromPoint = () => cardOf("Beta");
    try {
      fireEvent.pointerDown(handleOf("Alpha"), { clientX: 0, clientY: 0, button: 0, pointerId: 1 });
      fireEvent.pointerMove(shell, { clientX: 200, clientY: 0, pointerId: 1 });
      fireEvent.pointerUp(shell, { clientX: 200, clientY: 0, pointerId: 1 });
    } finally {
      document.elementFromPoint = original;
    }
    expect(onReorderPapers).not.toHaveBeenCalled();
    const live = await screen.findByRole("status");
    expect(live.textContent).toContain("搜索结果不是完整顺序");
  });

  it("落在「全部文档」上不是目录：给出原因并标为禁止", async () => {
    const onMovePaper = vi.fn();
    renderHub({ onMovePaper, selectedFolder: "" , documents: [...layerDocs, makeDoc("doc-t", "Delta", "Textbooks/ML", "2026-08-17T09:00:00Z")] });
    const all = screen.getByText("▾ 📁 全部文档").closest("[data-folder]") as HTMLElement;
    const shell = all.closest(".app-shell") as HTMLElement;
    const original = document.elementFromPoint;
    document.elementFromPoint = () => all;
    try {
      fireEvent.pointerDown(handleOf("Alpha"), { clientX: 0, clientY: 0, button: 0, pointerId: 1 });
      fireEvent.pointerMove(shell, { clientX: 200, clientY: 0, pointerId: 1 });
      fireEvent.pointerUp(shell, { clientX: 200, clientY: 0, pointerId: 1 });
    } finally {
      document.elementFromPoint = original;
    }
    expect(onMovePaper).not.toHaveBeenCalled();
    const live = await screen.findByRole("status");
    expect(live.textContent).toContain("请先在左侧选择一个物理目录");
  });

  it("拖到目录行松手即移动到该目录（落点存在 ref 上，不读到过期帧）", async () => {
    const onMovePaper = vi.fn().mockResolvedValue(undefined);
    renderHub({
      onMovePaper,
      documents: [...layerDocs, makeDoc("doc-t", "Delta", "Textbooks/ML", "2026-08-17T09:00:00Z")],
    });
    // 目标必须是 Papers 根下的另一个物理目录：跨根要经过 Move 对话框确认。
    const sibling = screen.getByText("📂 Papers").closest("[data-folder]") as HTMLElement;
    const shell = sibling.closest(".app-shell") as HTMLElement;
    const original = document.elementFromPoint;
    document.elementFromPoint = () => sibling;
    try {
      fireEvent.pointerDown(handleOf("Alpha"), { clientX: 0, clientY: 0, button: 0, pointerId: 1 });
      fireEvent.pointerMove(shell, { clientX: 200, clientY: 0, pointerId: 1 });
      fireEvent.pointerUp(shell, { clientX: 200, clientY: 0, pointerId: 1 });
    } finally {
      document.elementFromPoint = original;
    }
    await vi.waitFor(() => expect(onMovePaper).toHaveBeenCalled());
    expect(onMovePaper).toHaveBeenCalledWith("doc-a", "Papers");
  });
});

describe("LibraryHub — 选择集与键盘替代（PR 2 Gate）", () => {
  beforeEach(() => localStorage.clear());

  it("表格摘要格在有 Brief 时挂完整浮窗", () => {
    renderHub({
      documents: [
        makeDoc("doc-a", "Alpha", LEAF, "2026-08-17T10:00:00Z", { briefTakeaway: "完整核心贡献句。" }),
        ...layerDocs.slice(1),
      ],
    });
    fireEvent.click(screen.getByRole("button", { name: /结构表格/ }));
    expect(screen.getByText("💡 核心贡献 · 完整摘要")).toBeDefined();
    expect(screen.getAllByText("完整核心贡献句。").length).toBeGreaterThanOrEqual(2);
  });

  it("普通单击正文打开 Reader 且不弹出批量工具栏", () => {
    const onSelectPaper = vi.fn();
    renderHub({ onSelectPaper });
    fireEvent.click(screen.getByText("Alpha"));
    expect(onSelectPaper).toHaveBeenCalledWith("rev-doc-a", "doc-a");
    expect(screen.queryByRole("toolbar", { name: "批量操作" })).toBeNull();
  });

  it("复选框、Ctrl+单击与全选都更新同一个选择集（工具栏计数一致）", () => {
    renderHub();
    fireEvent.click(screen.getByLabelText("选择 Alpha"));
    expect(screen.getByText("已选 1 篇")).toBeDefined();
    fireEvent.click(screen.getByLabelText("选择 Beta"), { ctrlKey: true });
    expect(screen.getByText("已选 2 篇")).toBeDefined();
    fireEvent.click(screen.getByRole("button", { name: /全选当前筛选结果/ }));
    expect(screen.getByText(/已选择当前筛选结果 3 篇/)).toBeDefined();
    expect(screen.getByText("逻辑选择")).toBeDefined();
    fireEvent.click(screen.getByRole("button", { name: /清空选择/ }));
    expect(screen.queryByRole("toolbar", { name: "批量操作" })).toBeNull();
  });

  it("Space 切换焦点项、Esc 清空、Ctrl+A 不逐项写入", () => {
    renderHub();
    const list = screen.getByRole("listbox", { name: "文档列表" });
    fireEvent.click(screen.getByText("Alpha"));
    fireEvent.keyDown(list, { key: " " });
    expect(screen.getByText("已选 1 篇")).toBeDefined();
    fireEvent.keyDown(list, { key: "a", ctrlKey: true });
    expect(screen.getByText(/已选择当前筛选结果 3 篇/)).toBeDefined();
    fireEvent.keyDown(list, { key: "Escape" });
    expect(screen.queryByRole("toolbar", { name: "批量操作" })).toBeNull();
  });

  it("Alt+↓ 把焦点项挪一格并提交完整置换", async () => {
    const onReorderPapers = vi.fn().mockResolvedValue(undefined);
    renderHub({ onReorderPapers });
    const list = screen.getByRole("listbox", { name: "文档列表" });
    fireEvent.click(screen.getByText("Alpha"));
    fireEvent.keyDown(list, { key: "ArrowDown", altKey: true });
    await vi.waitFor(() => expect(onReorderPapers).toHaveBeenCalled());
    expect(onReorderPapers).toHaveBeenCalledWith("col-ml", ["doc-b", "doc-a", "doc-c"]);
  });

  it("方向键只移动焦点，Enter 打开焦点项", () => {
    const onSelectPaper = vi.fn();
    renderHub({ onSelectPaper });
    const list = screen.getByRole("listbox", { name: "文档列表" });
    fireEvent.keyDown(list, { key: "ArrowDown" });
    fireEvent.keyDown(list, { key: "ArrowDown" });
    fireEvent.keyDown(list, { key: "Enter" });
    expect(onSelectPaper).toHaveBeenCalledWith("rev-doc-b", "doc-b");
  });

  it("在检索框输入 m / Ctrl+A 不误触快捷键", async () => {
    const onMovePaper = vi.fn();
    renderHub({ onMovePaper });
    const search = screen.getByPlaceholderText(/检索/) as HTMLInputElement;
    fireEvent.click(search);
    fireEvent.keyDown(search, { key: "m" });
    fireEvent.keyDown(search, { key: "a", ctrlKey: true });
    expect(screen.queryByRole("dialog", { name: "移动到……" })).toBeNull();
    expect(screen.queryByRole("toolbar", { name: "批量操作" })).toBeNull();
    expect(onMovePaper).not.toHaveBeenCalled();
  });

  it("切换筛选会清空旧选择并说明原因；Grid/Table 切换保留选择", () => {
    const { rerender } = renderHub();
    fireEvent.click(screen.getByLabelText("选择 Alpha"));
    fireEvent.click(screen.getByLabelText("选择 Beta"), { ctrlKey: true });
    expect(screen.getByText("已选 2 篇")).toBeDefined();
    // 视图切换只是换画法，不能丢选择。
    fireEvent.click(screen.getByText("≡ 结构表格"));
    expect(screen.getByText("已选 2 篇")).toBeDefined();
    fireEvent.click(screen.getByText("田 卡片网格"));
    // 换 collection = 换 scope，旧选择必须清空并给出提示。
    rerender(<LibraryHub
      documents={layerDocs}
      collections={[leafCollection]}
      workspace={workspaceWith("D:/Papers")}
      selectedFolder="Papers"
      onSelectFolder={vi.fn()}
      onSelectPaper={vi.fn()}
      onImportPdf={vi.fn()}
      onOpenSettings={vi.fn()}
      onOpenOperations={vi.fn()}
    />);
    expect(screen.queryByRole("toolbar", { name: "批量操作" })).toBeNull();
    expect(screen.getByText(/筛选已变化/)).toBeDefined();
  });

  it("逻辑选择（Ctrl+A）不能直接执行移动：给出缺 Selection Snapshot 的原因", () => {
    renderHub();
    const list = screen.getByRole("listbox", { name: "文档列表" });
    fireEvent.keyDown(list, { key: "a", ctrlKey: true });
    const toolbar = screen.getByRole("toolbar", { name: "批量操作" });
    const move = within(toolbar).getByRole("button", { name: "移动到……" }) as HTMLButtonElement;
    expect(move.disabled).toBe(true);
    expect(move.title).toContain("Selection Snapshot");
  });

  it("导览可以在设置里重新播放", () => {
    const { rerender } = renderHub({ replayTourSignal: 0 });
    // renderHub 已经关掉首次导览：说明「本版本看过一次」确实被记住了。
    expect(screen.queryByRole("dialog", { name: "文库交互升级" })).toBeNull();
    expect(localStorage.getItem(COACH_MARK_STORAGE_KEY)).toBe("done");
    rerender(<LibraryHub
      documents={layerDocs}
      collections={[leafCollection]}
      workspace={workspaceWith("D:/Papers")}
      selectedFolder={LEAF}
      replayTourSignal={1}
      onSelectFolder={vi.fn()}
      onSelectPaper={vi.fn()}
      onImportPdf={vi.fn()}
      onOpenSettings={vi.fn()}
      onOpenOperations={vi.fn()}
    />);
    expect(screen.getByRole("dialog", { name: "文库交互升级" })).toBeDefined();
    // 重新播放必须清掉已读标记，否则用户没看完就切走会被永久记住。
    expect(localStorage.getItem(COACH_MARK_STORAGE_KEY)).toBeNull();
  });

  it("M 打开 Move 对话框，跨根目标预览 kind change 后才提交", async () => {
    const onMovePaper = vi.fn().mockResolvedValue(undefined);
    renderHub({
      onMovePaper,
      collections: [
        leafCollection,
        { id: "col-tml", parentId: "Textbooks", name: "ML", relativePath: "Textbooks/ML", sortMode: "recent", paperOrder: [] },
      ],
    });
    fireEvent.click(screen.getByText("Alpha"));
    fireEvent.keyDown(window, { key: "m" });
    const dialog = await screen.findByRole("dialog", { name: "移动到……" });
    const crossRoot = within(dialog).getByRole("option", { name: /Textbooks\/ML/ });
    expect(within(dialog).getByText(/当前所在目录/)).toBeDefined();
    fireEvent.click(crossRoot);
    expect(within(dialog).getByText(/论文侧的 Brief/)).toBeDefined();
    fireEvent.click(within(dialog).getByRole("button", { name: "确认跨根移动" }));
    await vi.waitFor(() => expect(onMovePaper).toHaveBeenCalled());
    expect(onMovePaper).toHaveBeenCalledWith("doc-a", "Textbooks/ML");
  });

  it("Move 对话框可以按路径搜索物理目录", async () => {
    renderHub({
      onMovePaper: vi.fn().mockResolvedValue(undefined),
      collections: [
        leafCollection,
        { id: "col-tml", parentId: "Textbooks", name: "ML", relativePath: "Textbooks/ML", sortMode: "recent", paperOrder: [] },
      ],
    });
    fireEvent.click(screen.getByText("Alpha"));
    fireEvent.keyDown(window, { key: "m" });
    const dialog = await screen.findByRole("dialog", { name: "移动到……" });
    fireEvent.change(within(dialog).getByPlaceholderText(/搜索物理目录/), { target: { value: "Textbooks" } });
    const options = within(dialog).getAllByRole("option");
    expect(options.every((option) => (option.textContent ?? "").includes("Textbooks"))).toBe(true);
  });
});

describe("LibraryHub — 目录树、菜单与未接通边界（PR 2 Gate）", () => {
  beforeEach(() => localStorage.clear());

  it("箭头按钮只负责展开/收起，收起状态按 workspace 记忆且不串库", () => {
    const { rerender } = renderHub();
    const papers = screen.getByText("📂 Papers").closest("[data-folder]") as HTMLElement;
    expect(within(papers).getByRole("button", { name: "收起 Papers" })).toBeDefined();
    fireEvent.click(within(papers).getByRole("button", { name: "收起 Papers" }));
    expect(screen.queryByText("📂 ML")).toBeNull();
    expect(JSON.parse(localStorage.getItem(treeStorageKey("D:/Papers")) ?? "[]")).toEqual(["Papers"]);

    rerender(<LibraryHub
      documents={layerDocs}
      collections={[leafCollection]}
      workspace={workspaceWith("E:/Other")}
      selectedFolder={LEAF}
      onSelectFolder={vi.fn()}
      onSelectPaper={vi.fn()}
      onImportPdf={vi.fn()}
      onOpenSettings={vi.fn()}
      onOpenOperations={vi.fn()}
    />);
    expect(screen.queryByText("📂 ML")).not.toBeNull();
  });

  it("目录行选择目录，↑/↓ 与 ←/→ 在树内移动焦点与收起", () => {
    const onSelectFolder = vi.fn();
    renderHub({ onSelectFolder });
    const tree = screen.getByRole("tree", { name: "文库物理目录" });
    fireEvent.keyDown(tree, { key: "ArrowDown" });
    fireEvent.keyDown(tree, { key: "Enter" });
    expect(onSelectFolder).toHaveBeenCalledWith("Papers");
    fireEvent.keyDown(tree, { key: "ArrowLeft" });
    expect(screen.queryByText("📂 ML")).toBeNull();
    expect(JSON.parse(localStorage.getItem(treeStorageKey("D:/Papers")) ?? "[]")).toEqual(["Papers"]);
  });

  it("右键菜单列全 §6.4 的动作，未接通的给出具体阻塞原因", () => {
    renderHub();
    fireEvent.contextMenu(cardOf("Alpha"));
    const menu = screen.getByRole("menu");
    for (const label of ["移动到……", "移到最前", "移到最后", "标签…", "阅读状态…", "导出…", "移入废纸篓"]) {
      expect(within(menu).getByRole("menuitem", { name: new RegExp(label) })).toBeDefined();
    }
    expect((within(menu).getByRole("menuitem", { name: /阅读状态/ }) as HTMLButtonElement).disabled).toBe(true);
    expect(within(menu).getByRole("menuitem", { name: /阅读状态/ }).getAttribute("title")).toContain("library_act");
    // 单篇动作不得把阻塞点说成后端 Batch：能力已在标签芯片与阅读器导出里提供。
    expect(within(menu).getByRole("menuitem", { name: /^标签…$/ }).getAttribute("title")).toContain("标签处编辑");
    expect(within(menu).getByRole("menuitem", { name: /^导出…$/ }).getAttribute("title")).toContain("阅读器");
    // Alpha 已经在本层最前：移到最前必须给出与拖拽同源的原因。
    const front = within(menu).getByRole("menuitem", { name: /移到最前/ }) as HTMLButtonElement;
    expect(front.disabled).toBe(true);
    expect(front.title).toContain("顺序未变化");
  });

  it("右键已选 Paper 时作用于整个选择集，批量项标为未实现", () => {
    renderHub();
    fireEvent.click(screen.getByLabelText("选择 Alpha"));
    fireEvent.click(screen.getByLabelText("选择 Beta"), { ctrlKey: true });
    fireEvent.contextMenu(cardOf("Alpha"));
    const menu = screen.getByRole("menu");
    expect(within(menu).getByRole("menuitem", { name: /移动到……（2 篇）/ })).toBeDefined();
    const trash = within(menu).getByRole("menuitem", { name: /移入废纸篓（2 篇）/ }) as HTMLButtonElement;
    expect(trash.disabled).toBe(true);
    expect(trash.title).toContain("Batch");
    expect(within(menu).getByRole("menuitem", { name: /标签（批量）…/ }).getAttribute("title")).toContain("持久 Batch");
    expect(within(menu).getByRole("menuitem", { name: /导出（批量）…/ })).toBeDefined();
  });

  it("批量动作只以「即将支持」呈现，不伪装成已完成", () => {
    renderHub();
    fireEvent.click(screen.getByLabelText("选择 Alpha"));
    const toolbar = screen.getByRole("toolbar", { name: "批量操作" });
    const soon = within(toolbar).getAllByText("即将支持");
    expect(soon.length).toBeGreaterThanOrEqual(5);
    for (const label of ["标签", "阅读状态", "OCR", "Brief", "导出"]) {
      const button = within(toolbar).getByRole("button", { name: new RegExp(`^${label}`) }) as HTMLButtonElement;
      expect(button.disabled).toBe(true);
    }
    expect(within(toolbar).getByRole("button", { name: /^标签/ }).getAttribute("title")).toContain("Batch");
  });

  it("导览每个版本只显示一次，记住了就不再出现", () => {
    localStorage.clear();
    const { unmount } = renderWithLocale(<LibraryHub
      documents={layerDocs}
      collections={[leafCollection]}
      workspace={workspaceWith("D:/Papers")}
      selectedFolder={LEAF}
      onSelectFolder={vi.fn()}
      onSelectPaper={vi.fn()}
      onImportPdf={vi.fn()}
      onOpenSettings={vi.fn()}
      onOpenOperations={vi.fn()}
    />);
    fireEvent.click(screen.getByRole("button", { name: "知道了" }));
    expect(screen.queryByRole("dialog", { name: "文库交互升级" })).toBeNull();
    expect(localStorage.getItem(COACH_MARK_STORAGE_KEY)).toBe("done");
    unmount();
    renderWithLocale(<LibraryHub
      documents={layerDocs}
      collections={[leafCollection]}
      workspace={workspaceWith("D:/Papers")}
      selectedFolder={LEAF}
      onSelectFolder={vi.fn()}
      onSelectPaper={vi.fn()}
      onImportPdf={vi.fn()}
      onOpenSettings={vi.fn()}
      onOpenOperations={vi.fn()}
    />);
    expect(screen.queryByRole("dialog", { name: "文库交互升级" })).toBeNull();
  });

  it("智能集合列出内置查询，不伪装成目录", () => {
    renderHub();
    expect(screen.getByText("智能集合")).toBeDefined();
    expect(screen.getByText(/阅读中/)).toBeDefined();
    expect(screen.getByText(/稍后阅读/)).toBeDefined();
    expect(screen.getByText(/本周导入但未读/)).toBeDefined();
    expect(screen.getByText(/OCR 失败/)).toBeDefined();
    expect(screen.getAllByTitle("内置查询，不能手排或接收拖放").length).toBeGreaterThan(0);
  });

  it("卡片展示优先级、复习日期和最近打开，不把它们藏进菜单", () => {
    renderHub({
      documents: [
        makeDoc("doc-a", "Alpha", LEAF, "2026-08-17T10:00:00Z", {
          priority: 2,
          reviewAt: "2026-09-10T00:00:00Z",
          lastOpenedAt: "2026-09-01T12:00:00Z",
        }),
        ...layerDocs.slice(1),
      ],
    });
    expect(screen.getByText("优先 2")).toBeDefined();
    expect(screen.getByText("复习 2026-09-10")).toBeDefined();
    expect(screen.getByText(/最近 2026-09-01/)).toBeDefined();
  });
});

describe("LibraryHub — 本地 Batch 接通后（PR 3）", () => {
  beforeEach(() => localStorage.clear());

  it("接通 client 后批量标签可点，不再伪装成即将支持", async () => {
    const client = createMemoryLibraryWorkspaceClient({
      papers: layerDocs.map(hubCardFromDoc),
    });
    renderHub({ libraryClient: client });
    await waitFor(() => expect(screen.getByLabelText("选择 Alpha")).toBeDefined());
    fireEvent.click(screen.getByLabelText("选择 Alpha"));
    fireEvent.click(screen.getByLabelText("选择 Beta"), { ctrlKey: true });
    const toolbar = screen.getByRole("toolbar", { name: "批量操作" });
    const tag = within(toolbar).getByRole("button", { name: "标签" }) as HTMLButtonElement;
    expect(tag.disabled).toBe(false);
    const lifecycle = within(toolbar).getByRole("button", { name: "阅读状态" }) as HTMLButtonElement;
    expect(lifecycle.disabled).toBe(false);
    expect(within(toolbar).queryByText("即将支持")).toBeNull();
    const ocr = within(toolbar).getByRole("button", { name: "OCR" }) as HTMLButtonElement;
    expect(ocr.disabled).toBe(false);
    fireEvent.click(ocr);
    await waitFor(() => expect(screen.getByRole("dialog", { name: "确认批次" })).toBeDefined());
    expect(screen.getByLabelText("费用预览")).toBeDefined();
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    fireEvent.click(tag);
    expect(screen.getByRole("dialog", { name: "批量标签" })).toBeDefined();
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    fireEvent.click(within(toolbar).getByRole("button", { name: "移入回收站" }));
    await waitFor(() => expect(screen.getByRole("dialog", { name: "确认批次" })).toBeDefined());
  });

  it("拖动已选多篇到目录走 Move Batch，不再用空提示静默失败", async () => {
    const inner = createMemoryLibraryWorkspaceClient({ papers: layerDocs.map(hubCardFromDoc) });
    const act = vi.spyOn(inner, "act");
    const onMovePaper = vi.fn();
    renderHub({ libraryClient: inner, onMovePaper });
    await waitFor(() => expect(screen.getByLabelText("选择 Alpha")).toBeDefined());
    fireEvent.click(screen.getByLabelText("选择 Alpha"));
    fireEvent.click(screen.getByLabelText("选择 Beta"), { ctrlKey: true });
    const papersRoot = screen.getByText("📂 Papers").closest("[data-folder]") as HTMLElement;
    const shell = papersRoot.closest(".app-shell") as HTMLElement;
    const original = document.elementFromPoint;
    document.elementFromPoint = () => papersRoot;
    try {
      fireEvent.pointerDown(handleOf("Alpha"), { clientX: 0, clientY: 0, button: 0, pointerId: 1 });
      fireEvent.pointerMove(shell, { clientX: 200, clientY: 0, pointerId: 1 });
      fireEvent.pointerUp(shell, { clientX: 200, clientY: 0, pointerId: 1 });
    } finally {
      document.elementFromPoint = original;
    }
    await waitFor(() => expect(act).toHaveBeenCalled());
    const planned = act.mock.calls.map(([request]) => request).find((request) => request.kind === "plan_batch");
    expect(planned).toMatchObject({
      kind: "plan_batch",
      command: { kind: "move", collectionPath: "Papers" },
      target: { kind: "explicit", paperIds: ["doc-a", "doc-b"] },
    });
    expect(onMovePaper).not.toHaveBeenCalled();
    expect(await screen.findByRole("button", { name: "查看详情" })).toBeDefined();
    expect(screen.getByRole("button", { name: "撤销整批" })).toBeDefined();
  });
});

describe("LibraryHub — 规模与可访问性（PR 6）", () => {
  beforeEach(() => localStorage.clear());

  it("有 client 时先报加载再画卡片，option 带 setsize / posinset", async () => {
    const inner = createMemoryLibraryWorkspaceClient({ papers: layerDocs.map(hubCardFromDoc) });
    let release: (() => void) | undefined;
    const hold = new Promise<void>((resolve) => {
      release = resolve;
    });
    const client: LibraryWorkspaceClient = {
      runtime: "memory",
      async read(request) {
        if (request.kind === "hub_page") await hold;
        return inner.read(request);
      },
      act: (request) => inner.act(request),
      watch: (after, listener) => inner.watch(after, listener),
    };
    renderHub({ libraryClient: client });
    expect(screen.getByText("正在加载文库…")).toBeDefined();
    release?.();
    const alpha = await waitFor(() => screen.getByLabelText("选择 Alpha"));
    const card = alpha.closest("[role='option']") as HTMLElement;
    expect(card.getAttribute("aria-setsize")).toBe("3");
    expect(card.getAttribute("aria-posinset")).toBe("1");
    expect(screen.getByRole("listbox", { name: "文档列表" })).toBeDefined();
  });

  it("键盘全选走 all_matching 的命中篇数，而不是已加载页", async () => {
    const papers = Array.from({ length: 120 }, (_, index) =>
      hubCardFromDoc(
        makeDoc(
          `doc-${index}`,
          `Paper ${index}`,
          LEAF,
          `2026-08-${String((index % 27) + 1).padStart(2, "0")}T10:00:00Z`,
        ),
      ),
    );
    const client = createMemoryLibraryWorkspaceClient({ papers });
    renderHub({
      libraryClient: client,
      documents: papers.map((card) => makeDoc(card.id, card.title, card.collectionPath, card.importedAt)),
    });
    const list = await waitFor(() => screen.getByRole("listbox", { name: "文档列表" }));
    expect(screen.getByRole("button", { name: /加载更多/ })).toBeDefined();
    fireEvent.keyDown(list, { key: "a", ctrlKey: true });
    expect(screen.getByText(/已选择当前筛选结果 120 篇/)).toBeDefined();
    const first = screen.getByText("Paper 0").closest("[role='option']") as HTMLElement;
    expect(first.getAttribute("aria-setsize")).toBe("120");
  });

  it("键盘 Space / Esc 在 hub_page 列表上仍然可用", async () => {
    const client = createMemoryLibraryWorkspaceClient({ papers: layerDocs.map(hubCardFromDoc) });
    renderHub({ libraryClient: client });
    const list = await waitFor(() => screen.getByRole("listbox", { name: "文档列表" }));
    fireEvent.click(screen.getByText("Alpha"));
    fireEvent.keyDown(list, { key: " " });
    expect(screen.getByText("已选 1 篇")).toBeDefined();
    fireEvent.keyDown(list, { key: "Escape" });
    expect(screen.queryByRole("toolbar", { name: "批量操作" })).toBeNull();
  });

  it("右键全部文档、文件夹和单卡片可编辑读者上下文，多选没有", () => {
    const onEditReaderContext = vi.fn();
    renderHub({ onEditReaderContext });
    fireEvent.contextMenu(screen.getByText(/全部文档/));
    fireEvent.click(screen.getByRole("menuitem", { name: "编辑全部文档的读者上下文" }));
    expect(onEditReaderContext).toHaveBeenCalledWith({ scope: "workspace" });

    fireEvent.contextMenu(screen.getByText("📂 ML"));
    fireEvent.click(screen.getByRole("menuitem", { name: "编辑此目录的读者上下文" }));
    expect(onEditReaderContext).toHaveBeenCalledWith({
      scope: "folder",
      collectionPath: "Papers/ML",
    });

    fireEvent.contextMenu(cardOf("Alpha"));
    fireEvent.click(screen.getByRole("menuitem", { name: "编辑此 PDF 的读者上下文" }));
    expect(onEditReaderContext).toHaveBeenCalledWith({ scope: "paper", paperId: "doc-a" });

    fireEvent.click(screen.getByLabelText("选择 Alpha"));
    fireEvent.click(screen.getByLabelText("选择 Beta"), { ctrlKey: true });
    fireEvent.contextMenu(cardOf("Alpha"));
    expect(screen.queryByRole("menuitem", { name: "编辑此 PDF 的读者上下文" })).toBeNull();
  });
});
