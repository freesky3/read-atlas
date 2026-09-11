import { fireEvent, screen, waitFor } from "@testing-library/react";
import { renderWithLocale } from "./i18n/testUtils";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import PdfReader, {
  actionsForBlock,
  bboxStyle,
  clampReaderZoom,
  isPinchZoomWheel,
  pageFromScrollOffset,
  pagePitch,
  pageWindow,
  READER_PAGE_GAP,
  rotatedBbox,
  scrollOffsetForPage,
} from "./PdfReader";

const RENDERED_PAGE_HEIGHT = 1000;
const RENDERED_PITCH = RENDERED_PAGE_HEIGHT + READER_PAGE_GAP;

const pdfMocks = vi.hoisted(() => ({
  getDocument: vi.fn(),
  getPage: vi.fn(),
  destroyDocument: vi.fn(),
  destroyLoadingTask: vi.fn(),
  renderTasks: [] as Array<{
    cancel: ReturnType<typeof vi.fn>;
    promise: Promise<void>;
  }>,
}));

vi.mock("pdfjs-dist", () => ({
  GlobalWorkerOptions: { workerSrc: "" },
  getDocument: pdfMocks.getDocument,
}));

vi.mock("pdfjs-dist/build/pdf.worker.min.mjs?url", () => ({
  default: "pdf.worker.min.mjs",
}));

describe("PdfReader", () => {
  beforeEach(() => {
    pdfMocks.renderTasks.length = 0;
    pdfMocks.getPage.mockReset();
    pdfMocks.destroyDocument.mockReset();
    pdfMocks.destroyLoadingTask.mockReset();
    pdfMocks.getDocument.mockReset();

    pdfMocks.getPage.mockImplementation(async (pageNumber: number) => ({
      pageNumber,
      getViewport: ({
        scale,
        rotation,
      }: {
        scale: number;
        rotation: number;
      }) => ({
        width: 600 * scale,
        height: 800 * scale,
        rotation,
      }),
      render: () => {
        const task = { cancel: vi.fn(), promise: Promise.resolve() };
        pdfMocks.renderTasks.push(task);
        return task;
      },
    }));
    pdfMocks.getDocument.mockReturnValue({
      promise: Promise.resolve({
        numPages: 20,
        getPage: pdfMocks.getPage,
        destroy: pdfMocks.destroyDocument,
      }),
      destroy: pdfMocks.destroyLoadingTask,
    });
  });

  it("loads only the current page window instead of decoding the whole PDF", async () => {
    const { rerender } = renderWithLocale(
      <PdfReader
        source="asset://paper.pdf"
        currentPage={1}
        zoom={100}
        rotation={0}
      />,
    );

    await waitFor(() => {
      expect(pdfMocks.getPage.mock.calls.map(([page]) => page)).toEqual([
        1, 2, 3,
      ]);
    });
    expect(pdfMocks.getPage).toHaveBeenCalledTimes(3);

    pdfMocks.getPage.mockClear();
    rerender(
      <PdfReader
        source="asset://paper.pdf"
        currentPage={10}
        zoom={100}
        rotation={0}
      />,
    );

    await waitFor(() => {
      expect(
        pdfMocks.getPage.mock.calls.map(([page]) => page).sort((a, b) => a - b),
      ).toEqual([8, 9, 10, 11, 12]);
    });
  });

  it("cancels obsolete render tasks when zoom changes and destroys the document on close", async () => {
    const { rerender, unmount } = renderWithLocale(
      <PdfReader
        source="asset://paper.pdf"
        currentPage={4}
        zoom={100}
        rotation={0}
      />,
    );
    await waitFor(() => expect(pdfMocks.renderTasks.length).toBe(5));
    const firstWindow = [...pdfMocks.renderTasks];

    rerender(
      <PdfReader
        source="asset://paper.pdf"
        currentPage={4}
        zoom={125}
        rotation={0}
      />,
    );
    await waitFor(() => expect(pdfMocks.renderTasks.length).toBe(10));
    expect(
      firstWindow.every((task) => task.cancel.mock.calls.length === 1),
    ).toBe(true);

    unmount();
    expect(pdfMocks.destroyLoadingTask).toHaveBeenCalledTimes(1);
    // PDF.js 6 owns document/worker cleanup through the loading task.
    expect(pdfMocks.getDocument).toHaveBeenCalledWith({ url: "asset://paper.pdf" });
    expect(pdfMocks.destroyDocument).not.toHaveBeenCalled();
  });

  it("clamps pinch and wheel zoom to the reader range", () => {
    expect(clampReaderZoom(20)).toBe(60);
    expect(clampReaderZoom(240)).toBe(180);
    expect(clampReaderZoom(117.4)).toBe(117);
    expect(isPinchZoomWheel({ ctrlKey: true, metaKey: false })).toBe(true);
    expect(isPinchZoomWheel({ ctrlKey: false, metaKey: false })).toBe(false);
    expect(
      pagePitch(
        new Map([
          [1, 800],
          [2, 820],
        ]),
      ),
    ).toBeCloseTo(838);
  });

  it("applies ctrl-wheel pinch zoom after the PDF loads", async () => {
    const onZoomChange = vi.fn();
    renderWithLocale(
      <PdfReader
        source="asset://paper.pdf"
        currentPage={1}
        zoom={100}
        rotation={0}
        onZoomChange={onZoomChange}
      />,
    );
    const reader = await screen.findByLabelText("PDF 阅读器");
    fireEvent.wheel(reader, { ctrlKey: true, deltaY: -80 });
    expect(onZoomChange).toHaveBeenCalled();
    expect(onZoomChange.mock.calls[0][0]).toBeGreaterThan(100);
  });

  it("still zooms when WebView2 aims the pinch wheel at the document", async () => {
    const onZoomChange = vi.fn();
    renderWithLocale(
      <PdfReader
        source="asset://paper.pdf"
        currentPage={1}
        zoom={100}
        rotation={0}
        onZoomChange={onZoomChange}
      />,
    );
    const reader = await screen.findByLabelText("PDF 阅读器");
    vi.spyOn(reader, "getBoundingClientRect").mockReturnValue({
      x: 0,
      y: 0,
      left: 0,
      top: 0,
      right: 800,
      bottom: 600,
      width: 800,
      height: 600,
      toJSON() {
        return {};
      },
    });
    fireEvent.wheel(window, {
      ctrlKey: true,
      deltaY: -80,
      clientX: 120,
      clientY: 180,
    });
    expect(onZoomChange).toHaveBeenCalled();
  });

  it("accumulates tiny trackpad pinch wheels into a visible zoom change", async () => {
    const onZoomChange = vi.fn();
    renderWithLocale(
      <PdfReader
        source="asset://paper.pdf"
        currentPage={1}
        zoom={100}
        rotation={0}
        onZoomChange={onZoomChange}
      />,
    );
    const reader = await screen.findByLabelText("PDF 阅读器");
    for (let index = 0; index < 50; index += 1) {
      fireEvent.wheel(reader, { ctrlKey: true, deltaY: -0.4 });
    }
    expect(onZoomChange).toHaveBeenCalled();
    expect(onZoomChange.mock.calls.at(-1)?.[0]).toBeGreaterThan(100);
  });

  it("pinches with two mouse pointers because WebView2 trackpads often report that type", async () => {
    const onZoomChange = vi.fn();
    renderWithLocale(
      <PdfReader
        source="asset://paper.pdf"
        currentPage={1}
        zoom={100}
        rotation={0}
        onZoomChange={onZoomChange}
      />,
    );
    const reader = await screen.findByLabelText("PDF 阅读器");
    const dispatchPointer = (
      type: string,
      init: PointerEventInit,
    ) => {
      reader.dispatchEvent(
        new PointerEvent(type, { bubbles: true, cancelable: true, ...init }),
      );
    };
    dispatchPointer("pointerdown", {
      pointerId: 1,
      pointerType: "mouse",
      clientX: 10,
      clientY: 10,
    });
    dispatchPointer("pointerdown", {
      pointerId: 2,
      pointerType: "mouse",
      clientX: 40,
      clientY: 10,
    });
    dispatchPointer("pointermove", {
      pointerId: 2,
      pointerType: "mouse",
      clientX: 100,
      clientY: 10,
    });
    expect(onZoomChange).toHaveBeenCalled();
    expect(onZoomChange.mock.calls[0][0]).toBeGreaterThan(100);
  });

  it("applies a native WebView2 pinch factor forwarded onto the window", async () => {
    const onZoomChange = vi.fn();
    renderWithLocale(
      <PdfReader
        source="asset://paper.pdf"
        currentPage={1}
        zoom={100}
        rotation={0}
        onZoomChange={onZoomChange}
      />,
    );
    await screen.findByLabelText("PDF 阅读器");
    window.dispatchEvent(new CustomEvent("webview-pinch-zoom", { detail: 1.2 }));
    expect(onZoomChange).toHaveBeenCalledWith(120);
  });

  it("maps normalized OCR bboxes through page rotation without using DPI", () => {
    expect(bboxStyle([100, 200, 400, 500], 0)).toEqual({
      left: "10%",
      top: "20%",
      width: "30%",
      height: "30%",
    });
    expect(bboxStyle([100, 200, 400, 500], 90)).toEqual({
      left: "50%",
      top: "10%",
      width: "30%",
      height: "30%",
    });
    expect(bboxStyle([100, 200, 400, 500], 270)).toEqual({
      left: "20%",
      top: "60%",
      width: "30%",
      height: "30%",
    });
    expect(rotatedBbox([100, 200, 400, 500], 90)).toEqual([500, 100, 800, 400]);
    expect(rotatedBbox([100, 200, 400, 500], 180)).toEqual([
      600, 500, 900, 800,
    ]);
  });

  it("keeps formulas and tables out of ordinary explain/translate actions", () => {
    expect(actionsForBlock("formula")).toEqual(["quote", "lens", "copy"]);
    expect(actionsForBlock("table")).toEqual(["quote", "lens", "copy"]);
    expect(actionsForBlock("header")).toEqual(["quote", "copy"]);
    expect(actionsForBlock("footer")).toEqual(["quote", "copy"]);
    expect(actionsForBlock("references")).toEqual(["quote", "copy"]);
    expect(actionsForBlock("paragraph")).toEqual([
      "quote",
      "translate",
      "explain",
      "copy",
    ]);
  });

  it("quotes any OCR Block and exposes its persisted selected state", async () => {
    const onToggleQuote = vi.fn();
    const onSelectBlock = vi.fn();
    const block = {
      id: "block-page-2",
      pageNumber: 2,
      blockIndex: 4,
      blockType: "table",
      textContent: "Ablation results",
      contentDigest: "digest-table",
      bbox: [100, 200, 700, 650] as [number, number, number, number],
    };

    renderWithLocale(
      <PdfReader
        source="asset://paper.pdf"
        currentPage={2}
        zoom={100}
        rotation={0}
        ocrBlocks={[block]}
        quotedBlockIds={[block.id]}
        focusedBlockId={block.id}
        onToggleQuote={onToggleQuote}
        onSelectBlock={onSelectBlock}
      />,
    );

    const quoteButton = await screen.findByRole("button", { name: "引用" });
    expect(quoteButton).toHaveAttribute("aria-pressed", "true");
    expect(document.querySelector(`[data-block-id="${block.id}"]`)).toHaveClass(
      "is-quoted",
    );
    fireEvent.click(quoteButton);
    expect(onToggleQuote).toHaveBeenCalledWith(block);
    expect(onSelectBlock).not.toHaveBeenCalled();
  });

  it("keeps the action bar hidden until a block is selected", async () => {
    const onSelectBlock = vi.fn();
    const onClearBlockSelection = vi.fn();
    const block = {
      id: "block-page-1",
      pageNumber: 1,
      blockIndex: 0,
      blockType: "paragraph",
      textContent: "Abstract",
      contentDigest: "digest-p",
      bbox: [80, 120, 900, 280] as [number, number, number, number],
    };

    const { rerender } = renderWithLocale(
      <PdfReader
        source="asset://paper.pdf"
        currentPage={1}
        zoom={100}
        rotation={0}
        ocrBlocks={[block]}
        onSelectBlock={onSelectBlock}
        onClearBlockSelection={onClearBlockSelection}
      />,
    );

    const hit = await waitFor(() => {
      const node = document.querySelector(`[data-block-id="${block.id}"]`);
      expect(node).toBeTruthy();
      return node as HTMLElement;
    });
    fireEvent.mouseEnter(hit);
    expect(hit).not.toHaveClass("is-focused");
    fireEvent.click(hit);
    expect(onSelectBlock).toHaveBeenCalledWith(block);

    rerender(
      <PdfReader
        source="asset://paper.pdf"
        currentPage={1}
        zoom={100}
        rotation={0}
        ocrBlocks={[block]}
        focusedBlockId={block.id}
        onSelectBlock={onSelectBlock}
        onClearBlockSelection={onClearBlockSelection}
      />,
    );
    expect(hit).toHaveClass("is-focused");
    expect(await screen.findByRole("button", { name: "引用" })).toBeInTheDocument();

    const canvas = document.querySelector("canvas");
    expect(canvas).toBeTruthy();
    fireEvent.click(canvas as HTMLElement);
    expect(onClearBlockSelection).toHaveBeenCalled();
  });

  it("offers keyboard selection, Escape clearing, and canonical text copy", async () => {
    const user = userEvent.setup();
    const onSelectBlock = vi.fn();
    const onClearBlockSelection = vi.fn();
    const onBlockAction = vi.fn();
    const block = {
      id: "block-keyboard",
      pageNumber: 1,
      blockIndex: 2,
      blockType: "paragraph",
      textContent: "Canonical OCR paragraph",
      contentDigest: "digest-keyboard",
      bbox: [80, 120, 900, 280] as [number, number, number, number],
    };
    const { rerender } = renderWithLocale(
      <PdfReader
        source="asset://paper.pdf"
        currentPage={1}
        zoom={100}
        rotation={0}
        ocrBlocks={[block]}
        onSelectBlock={onSelectBlock}
        onClearBlockSelection={onClearBlockSelection}
        onBlockAction={onBlockAction}
      />,
    );
    const target = await screen.findByRole("button", {
      name: /1页 paragraph #3 Canonical OCR paragraph/,
    });
    target.focus();
    await user.keyboard("{Enter}");
    expect(onSelectBlock).toHaveBeenCalledWith(block);

    rerender(
      <PdfReader
        source="asset://paper.pdf"
        currentPage={1}
        zoom={100}
        rotation={0}
        ocrBlocks={[block]}
        focusedBlockId={block.id}
        onSelectBlock={onSelectBlock}
        onClearBlockSelection={onClearBlockSelection}
        onBlockAction={onBlockAction}
      />,
    );
    fireEvent.click(await screen.findByRole("button", { name: "复制" }));
    expect(onBlockAction).toHaveBeenCalledWith(block, "copy");
    fireEvent.keyDown(target, { key: "Escape" });
    expect(onClearBlockSelection).toHaveBeenCalled();
  });

  it("reserves the same gutter on every visible page and keeps block tools", async () => {
    const block = {
      id: "block-page-1",
      pageNumber: 1,
      blockIndex: 0,
      blockType: "paragraph",
      textContent: "Abstract",
      contentDigest: "digest-p",
      bbox: [80, 120, 900, 280] as [number, number, number, number],
    };
    const note = {
      id: "note-1",
      kind: "note" as const,
      speakerId: "alin" as const,
      weight: "line" as const,
      anchor: {
        blockId: block.id,
        pageNumber: 1,
        blockType: "paragraph",
        bbox: block.bbox,
      },
      body: "Watch the gate.",
    };
    const trace = {
      id: "trace-1",
      kind: "trace" as const,
      speakerId: "laozhou" as const,
      anchor: {
        blockId: block.id,
        pageNumber: 1,
        blockType: "paragraph",
        bbox: block.bbox,
      },
    };

    const { rerender } = renderWithLocale(
      <PdfReader
        source="asset://paper.pdf"
        currentPage={1}
        zoom={100}
        rotation={0}
        ocrBlocks={[block]}
        focusedBlockId={block.id}
        guideLayerVisible
        guideInks={[note, trace]}
        activeGuideInkId={null}
      />,
    );

    const page = await screen.findByLabelText("PDF 第 1 页");
    expect(page).toHaveAttribute("data-page-number", "1");
    expect(page).toHaveClass("has-gutter");
    const gutters = await screen.findAllByLabelText(/页 AI 旁批/);
    expect(gutters.length).toBeGreaterThanOrEqual(3);
    gutters.forEach((gutter) => {
      expect(gutter).toHaveStyle({ width: "240px" });
    });
    expect(screen.getByText("阿林")).toBeInTheDocument();
    expect(screen.getByText(/Watch the gate/)).toBeInTheDocument();
    expect(screen.getAllByText("本页无旁批").length).toBeGreaterThan(0);
    expect(document.querySelector(".guide-leaders line")).toBeNull();
    expect(
      document.querySelector(`[data-block-id="${block.id}"]`),
    ).toHaveClass("is-guide-anchored");
    expect(document.querySelectorAll(".guide-color-bar")).toHaveLength(2);
    expect(await screen.findByRole("button", { name: "引用" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "翻译" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "解释" })).toBeInTheDocument();

    fireEvent.mouseEnter(document.querySelector(".guide-gutter-slot") as HTMLElement);
    expect(document.querySelector(".guide-leaders line")).not.toBeNull();

    rerender(
      <PdfReader
        source="asset://paper.pdf"
        currentPage={1}
        zoom={100}
        rotation={0}
        ocrBlocks={[block]}
        focusedBlockId={block.id}
        guideLayerVisible={false}
        guideInks={[note, trace]}
      />,
    );
    expect(screen.queryByLabelText(/页 AI 旁批/)).not.toBeInTheDocument();
    expect(document.querySelector(".has-gutter")).toBeNull();
    expect(await screen.findByRole("button", { name: "引用" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "翻译" })).toBeInTheDocument();
  });

  it("does not draw leader lines for traces", async () => {
    const block = {
      id: "block-page-1",
      pageNumber: 1,
      blockIndex: 0,
      blockType: "paragraph",
      textContent: "Abstract",
      contentDigest: "digest-p",
      bbox: [80, 120, 900, 280] as [number, number, number, number],
    };
    renderWithLocale(
      <PdfReader
        source="asset://paper.pdf"
        currentPage={1}
        zoom={100}
        rotation={0}
        ocrBlocks={[block]}
        guideLayerVisible
        guideInks={[
          {
            id: "trace-1",
            kind: "trace",
            speakerId: "xiaxia",
            anchor: {
              blockId: block.id,
              pageNumber: 1,
              blockType: "paragraph",
              bbox: block.bbox,
            },
          },
        ]}
        activeGuideInkId="trace-1"
      />,
    );
    await screen.findByLabelText("PDF 第 1 页");
    expect(document.querySelector(".guide-card")).toBeNull();
    expect(document.querySelector(".guide-leaders line")).toBeNull();
    expect(document.querySelector(".guide-color-bar")).not.toBeNull();
  });

  it("moves leader lines when the margin gutter scrolls", async () => {
    const block = {
      id: "block-page-1",
      pageNumber: 1,
      blockIndex: 0,
      blockType: "paragraph",
      textContent: "Abstract",
      contentDigest: "digest-p",
      bbox: [80, 120, 900, 280] as [number, number, number, number],
    };
    renderWithLocale(
      <PdfReader
        source="asset://paper.pdf"
        currentPage={1}
        zoom={100}
        rotation={0}
        ocrBlocks={[block]}
        guideLayerVisible
        guideInks={[
          {
            id: "note-1",
            kind: "note",
            speakerId: "alin",
            weight: "line",
            anchor: {
              blockId: block.id,
              pageNumber: 1,
              blockType: "paragraph",
              bbox: block.bbox,
            },
            body: "Watch the gate.",
          },
        ]}
      />,
    );
    const page = await screen.findByLabelText("PDF 第 1 页");
    const slot = await waitFor(() => {
      const node = page.querySelector(".guide-gutter-slot");
      expect(node).not.toBeNull();
      return node as HTMLElement;
    });
    fireEvent.mouseEnter(slot);
    const line = await waitFor(() => {
      const node = page.querySelector(".guide-leaders line");
      expect(node).not.toBeNull();
      expect(Number(node?.getAttribute("y2"))).toBeGreaterThan(0);
      return node as SVGLineElement;
    });
    const y1Before = Number(line.getAttribute("y1"));
    const y2Before = Number(line.getAttribute("y2"));
    const gutter = page.querySelector('[data-guide-gutter="1"]') as HTMLElement;
    Object.defineProperty(gutter, "scrollTop", {
      configurable: true,
      writable: true,
      value: 48,
    });
    fireEvent.scroll(gutter);
    await waitFor(() => {
      const moved = page.querySelector(".guide-leaders line");
      expect(moved).not.toBeNull();
      expect(Number(moved?.getAttribute("y2"))).toBe(y2Before - 48);
      expect(Number(moved?.getAttribute("y1"))).toBe(y1Before);
    });
  });

  it("maps a scrollbar jump onto the destination page instead of leaving a spacer black hole", async () => {
    expect(pageWindow(33, 45)).toEqual([31, 32, 33, 34, 35]);
    expect(pageFromScrollOffset(0, 920, 45)).toBe(1);
    expect(pageFromScrollOffset(scrollOffsetForPage(33, 920), 920, 45)).toBe(33);
    expect(pageFromScrollOffset(scrollOffsetForPage(45, 920) + 400, 920, 45)).toBe(
      45,
    );
    const measured = new Map([
      [1, 1200],
      [2, 1200],
      [3, 900],
    ]);
    expect(
      scrollOffsetForPage(2, 920, 34, measured) -
        scrollOffsetForPage(1, 920, 34, measured),
    ).toBe(1228);
    expect(pageFromScrollOffset(34 + 10, 920, 5, 34, measured)).toBe(1);
    expect(
      pageFromScrollOffset(34 + 1200 + 28 + 10, 920, 5, 34, measured),
    ).toBe(2);

    const onVisiblePage = vi.fn();
    const { rerender } = renderWithLocale(
      <PdfReader
        source="asset://paper.pdf"
        currentPage={1}
        zoom={100}
        rotation={0}
        onVisiblePage={onVisiblePage}
      />,
    );
    const reader = await screen.findByLabelText("PDF 阅读器");
    expect(screen.getByText("页面 1–3 保持加载")).toBeInTheDocument();
    await waitFor(() => {
      const first = screen.getByLabelText("PDF 第 1 页");
      const second = screen.getByLabelText("PDF 第 2 页");
      expect(
        Number.parseFloat((second.parentElement as HTMLElement).style.top) -
          Number.parseFloat((first.parentElement as HTMLElement).style.top),
      ).toBe(RENDERED_PITCH);
    });

    const destination = scrollOffsetForPage(18, RENDERED_PITCH);
    Object.defineProperty(reader, "scrollTop", {
      configurable: true,
      writable: true,
      value: destination,
    });
    await waitFor(() => {
      fireEvent.scroll(reader);
      expect(onVisiblePage).toHaveBeenCalledWith(18);
    });

    rerender(
      <PdfReader
        source="asset://paper.pdf"
        currentPage={18}
        zoom={100}
        rotation={0}
        onVisiblePage={onVisiblePage}
      />,
    );
    await waitFor(() => {
      expect(screen.getByText("页面 16–20 保持加载")).toBeInTheDocument();
    });
    expect(document.querySelector(".pdf-reader-spacer")).toBeNull();
    const page18 = await screen.findByLabelText("PDF 第 18 页");
    const page19 = await screen.findByLabelText("PDF 第 19 页");
    await waitFor(() => {
      const top18 = Number.parseFloat(
        (page18.parentElement as HTMLElement).style.top,
      );
      const top19 = Number.parseFloat(
        (page19.parentElement as HTMLElement).style.top,
      );
      expect(top19 - top18).toBe(RENDERED_PITCH);
    });
  });

  it("does not snap back when the parent only echoes the scroll offset", async () => {
    const onVisiblePage = vi.fn();
    const { rerender } = renderWithLocale(
      <PdfReader
        source="asset://paper.pdf"
        currentPage={18}
        zoom={100}
        rotation={0}
        initialScrollOffset={0}
        onVisiblePage={onVisiblePage}
      />,
    );
    const reader = await screen.findByLabelText("PDF 阅读器");
    await waitFor(() => {
      expect(screen.getByLabelText("PDF 第 18 页")).toBeInTheDocument();
    });
    const destination = scrollOffsetForPage(12, RENDERED_PITCH);
    let scrollTop = destination;
    const assigned: number[] = [];
    Object.defineProperty(reader, "scrollTop", {
      configurable: true,
      get: () => scrollTop,
      set: (value: number) => {
        assigned.push(value);
        scrollTop = value;
      },
    });
    await waitFor(() => {
      fireEvent.scroll(reader);
      expect(onVisiblePage).toHaveBeenCalledWith(12);
    });
    assigned.length = 0;
    rerender(
      <PdfReader
        source="asset://paper.pdf"
        currentPage={18}
        zoom={100}
        rotation={0}
        initialScrollOffset={destination}
        onVisiblePage={onVisiblePage}
      />,
    );
    expect(screen.getByText("页面 10–14 保持加载")).toBeInTheDocument();
    expect(assigned).not.toContain(scrollOffsetForPage(18, RENDERED_PITCH));
  });

  it("caps the margin gutter to the PDF page so cards cannot stretch the virtualizer", async () => {
    const notes = Array.from({ length: 8 }, (_, index) => ({
      id: `note-${index}`,
      kind: "note" as const,
      speakerId: "alin" as const,
      weight: "line" as const,
      anchor: {
        blockId: "block-page-1",
        pageNumber: 1,
        blockType: "paragraph",
        bbox: [80, 80 + index * 40, 900, 140 + index * 40] as [
          number,
          number,
          number,
          number,
        ],
      },
      body: `Stacked margin note ${index} with enough text to wrap.`,
    }));
    renderWithLocale(
      <PdfReader
        source="asset://paper.pdf"
        currentPage={1}
        zoom={100}
        rotation={0}
        guideLayerVisible
        guideInks={notes}
      />,
    );
    const gutter = await screen.findByLabelText("第 1 页 AI 旁批");
    await waitFor(() => {
      expect(gutter.style.height).toBe("1000px");
    });
  });

  it("offers the system PDF application when PDF.js cannot load the source", async () => {
    const openExternal = vi.fn();
    pdfMocks.getDocument.mockReturnValue({
      promise: Promise.reject(new Error("damaged xref")),
      destroy: pdfMocks.destroyLoadingTask,
    });

    renderWithLocale(
      <PdfReader
        source="asset://broken.pdf"
        currentPage={1}
        zoom={100}
        rotation={0}
        onOpenExternal={openExternal}
      />,
    );

    const fallback = await screen.findByRole("button", {
      name: /系统 PDF 应用/i,
    });
    fireEvent.click(fallback);
    expect(openExternal).toHaveBeenCalledTimes(1);
  });
});
