import {
  Component,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { CSSProperties, ErrorInfo, ReactNode } from "react";
import type { BlockAction, GuideCastSnapshot, GuideInk, OcrBlockProjection } from "./types";
import GuideCard from "./guide/GuideCard";
import {
  GUIDE_GUTTER_GAP,
  GUIDE_GUTTER_WIDTH,
  guideLeaderLine,
  layoutGuideMarginCards,
} from "./guide/marginLayout";
import {
  inksForBlock,
  locateGuideInks,
  nextNotePage,
  normalizeGuideInks,
  notesOnPage,
  primaryNoteForBlock,
  repliesFor,
} from "./guide/inks";
import { guidePersona } from "./guide/personas";
import {
  applyPinchZoom,
  applyWheelZoom,
  clampReaderZoom,
  eventOverElement,
  isPinchZoomWheel,
  pointerDistance,
  subscribeNativePinchZoom,
  wheelDeltaPixels,
} from "./readerZoom";
import {
  getDocument,
  GlobalWorkerOptions,
  type PDFDocumentProxy,
  type RenderTask,
} from "pdfjs-dist";
import workerSource from "pdfjs-dist/build/pdf.worker.min.mjs?url";

import { isFigureQuote } from "./blockQuotes";
import { useLocale } from "./i18n/LocaleContext";


GlobalWorkerOptions.workerSrc = workerSource;

export type PdfReaderProps = {
  source: string;
  currentPage: number;
  zoom: number;
  rotation: number;
  ocrBlocks?: OcrBlockProjection[];
  quotedBlockIds?: string[];
  cachedBlockIds?: string[];
  focusedBlockId?: string | null;
  onToggleQuote?: (block: OcrBlockProjection, cropDataUrl?: string) => void;
  onSelectBlock?: (block: OcrBlockProjection) => void;
  onClearBlockSelection?: () => void;
  onBlockAction?: (
    block: OcrBlockProjection,
    action: BlockAction,
    material?: BlockActionMaterial,
  ) => void;
  /** Persistent user-annotation actions are kept separate from AI artifact actions. */
  highlightedBlockIds?: string[];
  bookmarkedBlockIds?: string[];
  notedBlockIds?: string[];
  onAnnotationAction?: (
    block: OcrBlockProjection,
    action: "highlight" | "bookmark" | "note",
  ) => void;
  onPageCount?: (pageCount: number) => void;
  onVisiblePage?: (pageNumber: number) => void;
  onScrollOffset?: (offset: number) => void;
  initialScrollOffset?: number;
  /** Explicit, one-shot restoration of an intra-document scroll position. */
  scrollRequest?: { offset: number; token: number };
  onZoomChange?: (zoom: number) => void;
  onOpenExternal?: () => void;
  onDocumentReady?: (document: PDFDocumentProxy | null) => void;
  guideLayerVisible?: boolean;
  guideInks?: GuideInk[];
  activeGuideInkId?: string | null;
  onActivateGuideInk?: (id: string | null) => void;
  guideCastSnapshot?: GuideCastSnapshot | null;
  workspaceRoot?: string | null;
};

export { clampReaderZoom, isPinchZoomWheel } from "./readerZoom";
export type { PDFDocumentProxy } from "pdfjs-dist";

const PAGE_WINDOW_RADIUS = 2;
const BASE_SCALE = 1.25;

export const READER_WINDOW_PADDING_TOP = 34;
export const READER_WINDOW_PADDING_BOTTOM = 70;
export const READER_PAGE_GAP = 28;

export function pagePitch(heights: Map<number, number>, fallback = 920) {
  const values = [...heights.values()].filter((height) => height > 50);
  if (values.length === 0) return fallback;
  const average =
    values.reduce((sum, height) => sum + height, 0) / values.length;
  return average + READER_PAGE_GAP;
}

export function pageWindow(currentPage: number, pageCount: number) {
  const normalizedPage = Math.min(
    Math.max(1, currentPage),
    Math.max(1, pageCount),
  );
  const start = Math.max(1, normalizedPage - PAGE_WINDOW_RADIUS);
  const end = Math.min(pageCount, normalizedPage + PAGE_WINDOW_RADIUS);
  return Array.from({ length: end - start + 1 }, (_, index) => start + index);
}

function pageStride(
  pageNumber: number,
  pitch: number,
  heights?: ReadonlyMap<number, number>,
) {
  const measured = heights?.get(pageNumber);
  if (measured && measured > 50) return measured + READER_PAGE_GAP;
  return Math.max(1, pitch);
}

export function scrollOffsetForPage(
  pageNumber: number,
  pitch: number,
  paddingTop = READER_WINDOW_PADDING_TOP,
  heights?: ReadonlyMap<number, number>,
) {
  let top = paddingTop;
  for (let page = 1; page < pageNumber; page += 1) {
    top += pageStride(page, pitch, heights);
  }
  return top;
}

export function pageFromScrollOffset(
  offset: number,
  pitch: number,
  pageCount: number,
  paddingTop = READER_WINDOW_PADDING_TOP,
  heights?: ReadonlyMap<number, number>,
) {
  if (pageCount <= 0) return 1;
  let cursor = paddingTop;
  for (let page = 1; page <= pageCount; page += 1) {
    const stride = pageStride(page, pitch, heights);
    if (offset < cursor + stride * 0.65 || page === pageCount) {
      return page;
    }
    cursor += stride;
  }
  return pageCount;
}

function readerWindowHeight(
  pageCount: number,
  pitch: number,
  heights?: ReadonlyMap<number, number>,
) {
  let total = READER_WINDOW_PADDING_TOP + READER_WINDOW_PADDING_BOTTOM;
  for (let page = 1; page <= Math.max(1, pageCount); page += 1) {
    total += pageStride(page, pitch, heights);
  }
  return total;
}

type BoundaryState = { failed: boolean };

class GuideGutterBoundary extends Component<
  { fallback: ReactNode; children: ReactNode },
  BoundaryState
> {
  state: BoundaryState = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  componentDidCatch(_error: Error, _info: ErrorInfo) {
    this.setState({ failed: true });
  }
  render() {
    if (this.state.failed) return this.props.fallback;
    return this.props.children;
  }
}

export type { BlockAction } from "./types";

export type BlockActionMaterial = {
  displayCropDataUrl: string;
  modelCropDataUrl: string;
};

type PdfPageProps = {
  blocks: OcrBlockProjection[];
  quotedBlockIds: ReadonlySet<string>;
  cachedBlockIds?: ReadonlySet<string>;
  focusedBlockId?: string | null;
  onToggleQuote?: PdfReaderProps["onToggleQuote"];
  onSelectBlock?: PdfReaderProps["onSelectBlock"];
  onClearBlockSelection?: PdfReaderProps["onClearBlockSelection"];
  onBlockAction?: PdfReaderProps["onBlockAction"];
  highlightedBlockIds: ReadonlySet<string>;
  bookmarkedBlockIds: ReadonlySet<string>;
  notedBlockIds: ReadonlySet<string>;
  onAnnotationAction?: PdfReaderProps["onAnnotationAction"];
  document: PDFDocumentProxy;
  pageNumber: number;
  current: boolean;
  zoom: number;
  rotation: number;
  guideLayerVisible?: boolean;
  guideInks?: GuideInk[];
  activeGuideInkId?: string | null;
  onActivateGuideInk?: (id: string | null) => void;
  guideCastSnapshot?: GuideCastSnapshot | null;
  workspaceRoot?: string | null;
  onPageExtent?: (pageNumber: number, height: number) => void;
};

export function isChromeBlock(blockType: string): boolean {
  const normalized = blockType.toLowerCase();
  return (
    normalized.includes("header") ||
    normalized.includes("footer") ||
    normalized.includes("references")
  );
}

export function actionsForBlock(blockType: string): BlockAction[] {
  const normalized = blockType.toLowerCase();
  if (isChromeBlock(normalized)) return ["quote", "copy"];
  if (normalized.includes("formula") || normalized.includes("equation")) {
    return ["quote", "lens", "copy"];
  }
  if (normalized.includes("figure") || normalized.includes("image")) {
    return ["quote", "lens", "copy"];
  }
  if (normalized.includes("table")) return ["quote", "lens", "copy"];
  return ["quote", "translate", "explain", "copy"];
}

export function rotatedBbox(
  bbox: [number, number, number, number],
  rotation: number,
): [number, number, number, number] {
  const [x0, y0, x1, y1] = bbox;
  const normalizedRotation = ((rotation % 360) + 360) % 360;
  if (normalizedRotation === 90) return [1000 - y1, x0, 1000 - y0, x1];
  if (normalizedRotation === 180)
    return [1000 - x1, 1000 - y1, 1000 - x0, 1000 - y0];
  if (normalizedRotation === 270) return [y0, 1000 - x1, y1, 1000 - x0];
  return bbox;
}

function cropCanvas(
  source: HTMLCanvasElement,
  bbox: [number, number, number, number],
  maximumDimension?: number,
) {
  const padding = 24;
  const x0 = Math.max(0, bbox[0] - padding);
  const y0 = Math.max(0, bbox[1] - padding);
  const x1 = Math.min(1000, bbox[2] + padding);
  const y1 = Math.min(1000, bbox[3] + padding);
  const sourceX = Math.floor((x0 / 1000) * source.width);
  const sourceY = Math.floor((y0 / 1000) * source.height);
  const sourceWidth = Math.max(1, Math.ceil(((x1 - x0) / 1000) * source.width));
  const sourceHeight = Math.max(
    1,
    Math.ceil(((y1 - y0) / 1000) * source.height),
  );
  const scale = maximumDimension
    ? Math.min(1, maximumDimension / Math.max(sourceWidth, sourceHeight))
    : 1;
  const target = window.document.createElement("canvas");
  target.width = Math.max(1, Math.round(sourceWidth * scale));
  target.height = Math.max(1, Math.round(sourceHeight * scale));
  const context = target.getContext("2d", { alpha: false });
  if (!context) throw new Error("Canvas 2D is unavailable for Lens crop");
  context.drawImage(
    source,
    sourceX,
    sourceY,
    sourceWidth,
    sourceHeight,
    0,
    0,
    target.width,
    target.height,
  );
  return target;
}

export async function createLensCrops(
  document: PDFDocumentProxy,
  pageNumber: number,
  bbox: [number, number, number, number],
  rotation: number,
): Promise<BlockActionMaterial> {
  const page = await document.getPage(pageNumber);
  const baseViewport = page.getViewport({ scale: 1, rotation });
  const longest = Math.max(baseViewport.width, baseViewport.height);
  const renderScale = Math.max(1.5, Math.min(3, 2800 / longest));
  const viewport = page.getViewport({ scale: renderScale, rotation });
  const canvas = window.document.createElement("canvas");
  canvas.width = Math.max(1, Math.floor(viewport.width));
  canvas.height = Math.max(1, Math.floor(viewport.height));
  const context = canvas.getContext("2d", { alpha: false });
  if (!context) throw new Error("Canvas 2D is unavailable for Lens crop");
  const task = page.render({ canvas, canvasContext: context, viewport });
  await task.promise;
  const normalized = rotatedBbox(bbox, rotation);
  const displayCrop = cropCanvas(canvas, normalized);
  const modelCrop = cropCanvas(canvas, normalized, 1280);
  let displayCropDataUrl = "";
  let modelCropDataUrl = "";
  try {
    displayCropDataUrl = displayCrop.toDataURL("image/png");
  } catch {
    // toDataURL might not be implemented in jsdom
  }
  try {
    modelCropDataUrl = modelCrop.toDataURL("image/png");
  } catch {
    // toDataURL might not be implemented in jsdom
  }
  return {
    displayCropDataUrl,
    modelCropDataUrl,
  };
}

export function bboxStyle(
  bbox: [number, number, number, number],
  rotation: number,
): CSSProperties {
  const [left, top, right, bottom] = rotatedBbox(bbox, rotation);
  return {
    left: left / 10 + "%",
    top: top / 10 + "%",
    width: (right - left) / 10 + "%",
    height: (bottom - top) / 10 + "%",
  };
}
const ACTION_KEYS: Record<BlockAction, string> = {
  quote: "reader.action.quote",
  translate: "reader.action.translate",
  explain: "reader.action.explain",
  lens: "reader.action.lens",
  copy: "reader.action.copy",
};

function PdfPage({
  document,
  pageNumber,
  current,
  zoom,
  rotation,
  blocks,
  quotedBlockIds,
  cachedBlockIds,
  focusedBlockId,
  onToggleQuote,
  onSelectBlock,
  onClearBlockSelection,
  onBlockAction,
  highlightedBlockIds,
  bookmarkedBlockIds,
  notedBlockIds,
  onAnnotationAction,
  guideLayerVisible = false,
  guideInks = [],
  guideCastSnapshot = null,
  workspaceRoot = null,
  activeGuideInkId = null,
  onActivateGuideInk,
  onPageExtent,
}: PdfPageProps) {
  const { t } = useLocale();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [failed, setFailed] = useState(false);
  const [croppingBlockId, setCroppingBlockId] = useState<string | null>(null);
  const [pageSize, setPageSize] = useState({ width: 0, height: 0 });
  const [cardHeights, setCardHeights] = useState<Record<string, number>>({});
  const [hoverInkId, setHoverInkId] = useState<string | null>(null);
  const [gutterScrollTop, setGutterScrollTop] = useState(0);
  const gutterRef = useRef<HTMLElement>(null);
  const resolvedInks = useMemo(
    () => locateGuideInks(normalizeGuideInks(guideInks), blocks),
    [guideInks, blocks],
  );
  const pageBlockIds = useMemo(
    () => new Set(blocks.map((block) => block.id)),
    [blocks],
  );

  const runBlockAction = async (
    block: OcrBlockProjection,
    action: BlockAction,
  ) => {
    if (action === "quote") {
      onToggleQuote?.(block);
      return;
    }
    if (action === "copy") {
      onBlockAction?.(block, action);
      return;
    }
    if (action !== "lens") {
      onBlockAction?.(block, action);
      return;
    }
    setCroppingBlockId(block.id);
    try {
      const material = await createLensCrops(
        document,
        pageNumber,
        block.bbox,
        rotation,
      );
      onBlockAction?.(block, action, material);
    } catch {
      onBlockAction?.(block, action);
    } finally {
      setCroppingBlockId(null);
    }
  };

  useEffect(() => {
    let disposed = false;
    let renderTask: RenderTask | null = null;
    setFailed(false);

    void document
      .getPage(pageNumber)
      .then((page) => {
        if (disposed) return;
        const viewport = page.getViewport({
          scale: BASE_SCALE * (zoom / 100),
          rotation,
        });
        const canvas = canvasRef.current;
        const context = canvas?.getContext("2d", { alpha: false });
        if (!canvas || !context) {
          throw new Error("Canvas 2D is unavailable");
        }

        const outputScale = Math.max(1, window.devicePixelRatio || 1);
        canvas.width = Math.max(1, Math.floor(viewport.width * outputScale));
        canvas.height = Math.max(1, Math.floor(viewport.height * outputScale));
        canvas.style.width = `${Math.floor(viewport.width)}px`;
        canvas.style.height = `${Math.floor(viewport.height)}px`;
        setPageSize({
          width: Math.floor(viewport.width),
          height: Math.floor(viewport.height),
        });
        renderTask = page.render({
          canvas,
          canvasContext: context,
          viewport,
          transform:
            outputScale === 1
              ? undefined
              : [outputScale, 0, 0, outputScale, 0, 0],
        });
        return renderTask.promise;
      })
      .catch((error: unknown) => {
        if (
          disposed ||
          (error instanceof Error &&
            error.name === "RenderingCancelledException")
        ) {
          return;
        }
        setFailed(true);
      });

    return () => {
      disposed = true;
      renderTask?.cancel();
    };
  }, [document, pageNumber, rotation, zoom]);

  useEffect(() => {
    if (pageSize.height > 80) onPageExtent?.(pageNumber, pageSize.height);
  }, [onPageExtent, pageNumber, pageSize.height]);

  const pageNotes = notesOnPage(resolvedInks, pageNumber, pageBlockIds);
  const laterPage = nextNotePage(resolvedInks, pageNumber);
  const layout = layoutGuideMarginCards({
    pageHeight: Math.max(pageSize.height, 1),
    cards: pageNotes.map((note) => ({
      id: note.id,
      anchorX: (note.anchor.bbox[2] / 1000) * Math.max(pageSize.width, 1),
      anchorY: ((note.anchor.bbox[1] + note.anchor.bbox[3]) / 2000) * Math.max(pageSize.height, 1),
      height: cardHeights[note.id] ?? 88,
    })),
  });
  useLayoutEffect(() => {
    const node = gutterRef.current;
    if (!node) return;
    setGutterScrollTop(node.scrollTop);
  }, [layout.contentHeight, pageNumber, pageSize.height]);

  return (
    <article
      className={`pdf-page-row ${guideLayerVisible ? "has-gutter" : ""} ${current ? "is-current" : ""}`}
      data-page-number={pageNumber}
      aria-label={t("reader.page.aria", { page: pageNumber })}
    >
      <div className={`pdf-reader-page ${current ? "is-current" : ""}`}>
      <div
        className="pdf-page-surface"
        onClick={() => {
          onClearBlockSelection?.();
          onActivateGuideInk?.(null);
        }}
      >
        <canvas ref={canvasRef} />
        {blocks.length > 0 ? (
          <div
            className="ocr-block-layer"
            aria-label={t("reader.block.ocrLayer", { page: pageNumber })}
          >
            {blocks.map((block) => {
              const chrome = isChromeBlock(block.blockType);
              const selected = focusedBlockId === block.id;
              const highlighted = highlightedBlockIds.has(block.id);
              const bookmarked = bookmarkedBlockIds.has(block.id);
              const noted = notedBlockIds.has(block.id);
              const actionsBelow = rotatedBbox(block.bbox, rotation)[1] < 70;
              const blockInks = inksForBlock(resolvedInks, block.id);
              const primary = primaryNoteForBlock(resolvedInks, block.id);
              const guideActive =
                guideLayerVisible &&
                Boolean(
                  primary &&
                    (primary.id === activeGuideInkId || primary.id === hoverInkId),
                );
              const a11yLabel = t("reader.block.a11yLabel", {
                page: block.pageNumber,
                type: block.blockType,
                index: block.blockIndex + 1,
                text:
                  block.textContent.slice(0, 40) ||
                  t("reader.block.imageFormulaFallback"),
              });
              const selectThis = () => {
                onSelectBlock?.(block);
                if (primary) onActivateGuideInk?.(primary.id);
              };
              return (
                <div
                  role="group"
                  aria-label={a11yLabel}
                  className={`ocr-block-hit ocr-block-${block.blockType.toLowerCase()} ${chrome ? "is-chrome" : ""} ${quotedBlockIds.has(block.id) ? "is-quoted" : ""} ${highlighted ? "is-highlighted" : ""} ${bookmarked ? "is-bookmarked" : ""} ${noted ? "has-user-note" : ""} ${selected ? "is-focused" : ""} ${actionsBelow ? "actions-below" : ""} ${guideLayerVisible && blockInks.length > 0 ? "is-guide-anchored" : ""} ${guideActive ? "is-guide-current" : ""}`}
                  data-block-id={block.id}
                  key={block.id}
                  style={bboxStyle(block.bbox, rotation)}
                  onMouseEnter={() => {
                    if (primary) setHoverInkId(primary.id);
                  }}
                  onMouseLeave={() => setHoverInkId(null)}
                  onClick={(e) => {
                    e.stopPropagation();
                    selectThis();
                  }}
                >
                  <button
                    type="button"
                    className="ocr-block-target"
                    aria-label={a11yLabel}
                    aria-pressed={selected}
                    onClick={(event) => {
                      event.stopPropagation();
                      selectThis();
                    }}
                    onKeyDown={(event) => {
                      if (event.key === "Escape") {
                        event.stopPropagation();
                        onClearBlockSelection?.();
                      }
                    }}
                    style={{ position: "absolute", inset: 0, border: 0, background: "transparent", cursor: "pointer" }}
                  />
                  {guideLayerVisible
                    ? blockInks
                        .filter((ink) => ink.kind !== "reply")
                        .map((ink, index) => {
                          const color = guidePersona(ink.speakerId, guideCastSnapshot, workspaceRoot)?.color;
                          return (
                            <span
                              key={ink.id}
                              className="guide-color-bar"
                              style={{
                                background: color,
                                left: index * 4,
                              }}
                            />
                          );
                        })
                    : null}
                  {selected ? (
                  <div className="ocr-block-actions" style={{ position: "relative", zIndex: 2 }}>
                    {onAnnotationAction ? (
                      <div className="ocr-block-annotation-actions" aria-label={t("reader.block.annotationActions")}>
                        <button
                          type="button"
                          className="ocr-annotation-action"
                          aria-pressed={highlighted}
                          aria-label={highlighted ? t("reader.block.removeHighlight") : t("reader.block.addHighlight")}
                          title={highlighted ? t("reader.block.removeHighlight") : t("reader.block.addHighlight")}
                          onClick={(event) => {
                            event.stopPropagation();
                            onAnnotationAction(block, "highlight");
                          }}
                        >
                          {highlighted ? "★" : "☆"}
                        </button>
                        <button
                          type="button"
                          className="ocr-annotation-action"
                          aria-pressed={bookmarked}
                          aria-label={bookmarked ? t("reader.block.removeBookmark") : t("reader.block.addBookmark")}
                          title={bookmarked ? t("reader.block.removeBookmark") : t("reader.block.addBookmark")}
                          onClick={(event) => {
                            event.stopPropagation();
                            onAnnotationAction(block, "bookmark");
                          }}
                        >
                          {bookmarked ? "🔖" : "♧"}
                        </button>
                        <button
                          type="button"
                          className="ocr-annotation-action"
                          aria-pressed={noted}
                          aria-label={t("reader.block.addNote")}
                          title={t("reader.block.addNote")}
                          onClick={(event) => {
                            event.stopPropagation();
                            onAnnotationAction(block, "note");
                          }}
                        >
                          ✎
                        </button>
                      </div>
                    ) : null}
                    {actionsForBlock(block.blockType).map((action) => {
                      const isCached =
                        ["translate", "explain", "lens"].includes(action) &&
                        cachedBlockIds?.has(block.id);
                      return (
                        <button
                          key={action}
                          type="button"
                          disabled={croppingBlockId === block.id}
                          aria-pressed={
                            action === "quote"
                              ? quotedBlockIds.has(block.id)
                              : undefined
                          }
                          onClick={(event) => {
                            event.stopPropagation();
                            void runBlockAction(block, action);
                          }}
                        >
                          {croppingBlockId === block.id && action === "lens"
                            ? t("reader.action.cropping")
                            : t(ACTION_KEYS[action])}
                          {isCached ? (
                            <span className="check-dot-green" title={t("reader.action.cached")}>
                              ✓
                            </span>
                          ) : null}
                        </button>
                      );
                    })}
                  </div>
                  ) : null}
                </div>
              );
            })}
          </div>
        ) : null}
      </div>
      {failed ? (
        <span className="pdf-page-error">
          {t("reader.page.renderError", { page: pageNumber })}
        </span>
      ) : null}
      </div>
      {guideLayerVisible ? (
        <>
          <aside
            ref={gutterRef}
            className="guide-gutter"
            data-guide-gutter={pageNumber}
            style={{
              width: GUIDE_GUTTER_WIDTH,
              height: pageSize.height || undefined,
              maxHeight: pageSize.height || undefined,
            }}
            aria-label={t("reader.guide.gutterAria", { page: pageNumber })}
            onScroll={(event) => {
              setGutterScrollTop(event.currentTarget.scrollTop);
            }}
          >
            <GuideGutterBoundary
              fallback={
                <div className="guide-gutter-empty">{t("reader.guide.gutterFailed")}</div>
              }
            >
            <div
              className="guide-gutter-scroller"
              style={{
                height: Math.max(pageSize.height || 0, layout.contentHeight),
              }}
            >
            {pageNotes.length === 0 ? (
              <div className="guide-gutter-empty">
                {laterPage
                  ? t("reader.guide.emptyWithNext", { page: laterPage })
                  : t("reader.guide.empty")}
              </div>
            ) : null}
            {pageNotes.map((note) => {
              const placement = layout.placements.find((item) => item.id === note.id);
              return (
                <div
                  key={note.id}
                  className="guide-gutter-slot"
                  style={{ top: placement?.top ?? 8 }}
                  ref={(node) => {
                    if (!node) return;
                    const next = Math.ceil(node.offsetHeight);
                    setCardHeights((current) => {
                      const prev = current[note.id] ?? 0;
                      if (Math.abs(prev - next) < 2) return current;
                      return { ...current, [note.id]: next };
                    });
                  }}
                  onMouseEnter={() => setHoverInkId(note.id)}
                  onMouseLeave={() => setHoverInkId(null)}
                >
                  <GuideCard
                    note={note}
                    replies={repliesFor(resolvedInks, note.id)}
                    active={note.id === activeGuideInkId || note.id === hoverInkId}
                    onActivate={() => onActivateGuideInk?.(note.id)}
                    snapshot={guideCastSnapshot}
                    workspaceRoot={workspaceRoot}
                  />
                </div>
              );
            })}
            </div>
            </GuideGutterBoundary>
          </aside>
          <svg
            className="guide-leaders"
            width={pageSize.width + GUIDE_GUTTER_GAP + GUIDE_GUTTER_WIDTH}
            height={pageSize.height}
            aria-hidden="true"
          >
            {pageNotes
              .filter(
                (note) =>
                  note.id === activeGuideInkId || note.id === hoverInkId,
              )
              .map((note) => {
                const placement = layout.placements.find((item) => item.id === note.id);
                if (!placement) return null;
                const color = guidePersona(note.speakerId, guideCastSnapshot, workspaceRoot)?.color ?? "#b42318";
                const leader = guideLeaderLine({
                  id: note.id,
                  blockRightX:
                    (note.anchor.bbox[2] / 1000) * Math.max(pageSize.width, 1),
                  blockCenterY:
                    ((note.anchor.bbox[1] + note.anchor.bbox[3]) / 2000) *
                    Math.max(pageSize.height, 1),
                  pageWidth: pageSize.width,
                  pageHeight: pageSize.height,
                  cardTop: placement.top,
                  cardHeight: placement.height,
                  gutterScrollTop,
                });
                return (
                  <line
                    key={leader.id}
                    x1={leader.x1}
                    y1={leader.y1}
                    x2={leader.x2}
                    y2={leader.y2}
                    stroke={color}
                    strokeWidth="1"
                    strokeOpacity="0.55"
                  />
                );
              })}
          </svg>
        </>
      ) : null}
    </article>
  );
}

export default function PdfReader({
  source,
  ocrBlocks = [],
  quotedBlockIds = [],
  cachedBlockIds = [],
  focusedBlockId,
  onToggleQuote,
  onSelectBlock,
  onClearBlockSelection,
  onBlockAction,
  highlightedBlockIds = [],
  bookmarkedBlockIds = [],
  notedBlockIds = [],
  onAnnotationAction,
  currentPage,
  zoom,
  rotation,
  onPageCount,
  onVisiblePage,
  onScrollOffset,
  initialScrollOffset = 0,
  scrollRequest,
  onZoomChange,
  onOpenExternal,
  onDocumentReady,
  guideLayerVisible = false,
  guideInks = [],
  guideCastSnapshot = null,
  workspaceRoot = null,
  activeGuideInkId = null,
  onActivateGuideInk,
}: PdfReaderProps) {
  const { t } = useLocale();
  const [document, setDocument] = useState<PDFDocumentProxy | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const scrollerRef = useRef<HTMLDivElement>(null);
  const programmaticScroll = useRef(false);
  const lastReportedPage = useRef(0);
  const extentsRef = useRef(new Map<number, number>());
  const layoutAnchorRef = useRef<{ page: number; top: number } | null>(null);
  const zoomRef = useRef(zoom);
  const onZoomChangeRef = useRef(onZoomChange);
  const [extentVersion, setExtentVersion] = useState(0);
  const [viewPage, setViewPage] = useState(currentPage);
  zoomRef.current = zoom;
  onZoomChangeRef.current = onZoomChange;
  const pitch = useMemo(
    () => pagePitch(extentsRef.current, 920),
    [extentVersion],
  );
  const rememberPageExtent = useCallback((pageNumber: number, height: number) => {
    if (height < 80) return;
    if (extentsRef.current.get(pageNumber) === height) return;
    extentsRef.current.set(pageNumber, height);
    setExtentVersion((version) => version + 1);
  }, []);

  useEffect(() => {
    let disposed = false;
    setDocument(null);
    setLoadError(null);
    onDocumentReady?.(null);

    const loadingTask = getDocument({ url: source });
    void loadingTask.promise
      .then((nextDocument) => {
        if (disposed) {
          return;
        }
        setDocument(nextDocument);
        onDocumentReady?.(nextDocument);
        onPageCount?.(nextDocument.numPages);
      })
      .catch((error: unknown) => {
        if (!disposed) {
          setLoadError(
            error instanceof Error
              ? error.message
              : "PDF.js could not load this PDF",
          );
        }
      });

    return () => {
      disposed = true;
      onDocumentReady?.(null);
      void loadingTask.destroy();
    };
  }, [onDocumentReady, onPageCount, source]);

  const pages = useMemo(
    () => (document ? pageWindow(viewPage, document.numPages) : []),
    [viewPage, document],
  );
  const quotedIds = useMemo(() => new Set(quotedBlockIds), [quotedBlockIds]);
  const cachedIds = useMemo(
    () => new Set(cachedBlockIds ?? []),
    [cachedBlockIds],
  );
  const highlightedIds = useMemo(
    () => new Set(highlightedBlockIds),
    [highlightedBlockIds],
  );
  const bookmarkedIds = useMemo(
    () => new Set(bookmarkedBlockIds),
    [bookmarkedBlockIds],
  );
  const notedIds = useMemo(() => new Set(notedBlockIds), [notedBlockIds]);

  useEffect(() => {
    if (!focusedBlockId) return;
    const focusedBlock = ocrBlocks.find(
      (block) => block.id === focusedBlockId,
    );
    if (focusedBlock && pages.includes(focusedBlock.pageNumber)) return;
    // A parent can change page and focus in the same render. Keep the focus
    // until the virtual page window catches up instead of clearing a valid
    // distant evidence target.
    if (focusedBlock && focusedBlock.pageNumber === currentPage) return;
    onClearBlockSelection?.();
    window.requestAnimationFrame(() => scrollerRef.current?.focus());
  }, [currentPage, focusedBlockId, ocrBlocks, onClearBlockSelection, pages]);

  const extentsEpoch = useRef({ zoom, rotation, source });
  useLayoutEffect(() => {
    const epoch = extentsEpoch.current;
    if (
      epoch.zoom === zoom &&
      epoch.rotation === rotation &&
      epoch.source === source
    ) {
      return;
    }
    extentsEpoch.current = { zoom, rotation, source };
    extentsRef.current.clear();
    layoutAnchorRef.current = null;
    setExtentVersion((version) => version + 1);
  }, [zoom, rotation, source]);

  useLayoutEffect(() => {
    const scroller = scrollerRef.current;
    if (!scroller) return;
    const page = Math.max(1, lastReportedPage.current || viewPage);
    const nextTop = scrollOffsetForPage(
      page,
      pitch,
      READER_WINDOW_PADDING_TOP,
      extentsRef.current,
    );
    const previous = layoutAnchorRef.current;
    layoutAnchorRef.current = { page, top: nextTop };
    if (!previous || previous.page !== page) return;
    const delta = nextTop - previous.top;
    if (Math.abs(delta) < 1) return;
    programmaticScroll.current = true;
    scroller.scrollTop += delta;
    const timer = window.setTimeout(() => {
      programmaticScroll.current = false;
    }, 0);
    return () => window.clearTimeout(timer);
  }, [extentVersion, pitch]);

  const skipNextPageScroll = useRef(initialScrollOffset > 0);
  const hasRestoredInitialScroll = useRef(false);
  const previousPage = useRef(currentPage);

  useEffect(() => {
    const scroller = scrollerRef.current;
    if (!scroller || !document) return;

    const finishProgrammatic = () => {
      const timer = window.setTimeout(() => {
        programmaticScroll.current = false;
      }, 200);
      return () => window.clearTimeout(timer);
    };

    if (initialScrollOffset > 0 && !hasRestoredInitialScroll.current) {
      hasRestoredInitialScroll.current = true;
      programmaticScroll.current = true;
      scroller.scrollTop = initialScrollOffset;
      lastReportedPage.current = pageFromScrollOffset(
        initialScrollOffset,
        pitch,
        document.numPages,
        READER_WINDOW_PADDING_TOP,
        extentsRef.current,
      );
      previousPage.current = lastReportedPage.current;
      setViewPage(lastReportedPage.current);
      return finishProgrammatic();
    }

    if (skipNextPageScroll.current) {
      skipNextPageScroll.current = false;
      lastReportedPage.current = currentPage;
      previousPage.current = currentPage;
      return;
    }

    const pageChanged = previousPage.current !== currentPage;
    previousPage.current = currentPage;
    const expected = scrollOffsetForPage(
      currentPage,
      pitch,
      READER_WINDOW_PADDING_TOP,
      extentsRef.current,
    );

    if (lastReportedPage.current === 0) {
      programmaticScroll.current = true;
      scroller.scrollTop = expected;
      lastReportedPage.current = currentPage;
      return finishProgrammatic();
    }

    if (!pageChanged || lastReportedPage.current === currentPage) {
      return;
    }

    programmaticScroll.current = true;
    scroller.scrollTop = expected;
    lastReportedPage.current = currentPage;
    setViewPage(currentPage);
    return finishProgrammatic();
  }, [currentPage, document]);

  // `initialScrollOffset` is intentionally consumed only on the first mount.
  // Back navigation needs a separate token so a later restore does not fight
  // ordinary user scrolling or get ignored by the one-shot guard.
  useEffect(() => {
    const scroller = scrollerRef.current;
    if (!scroller || !document || !scrollRequest) return;
    const offset = Number(scrollRequest.offset);
    if (!Number.isFinite(offset) || offset < 0) return;
    programmaticScroll.current = true;
    scroller.scrollTop = offset;
    const restoredPage = pageFromScrollOffset(
      offset,
      pitch,
      document.numPages,
      READER_WINDOW_PADDING_TOP,
      extentsRef.current,
    );
    lastReportedPage.current = restoredPage;
    previousPage.current = restoredPage;
    setViewPage(restoredPage);
    onVisiblePage?.(restoredPage);
    const timer = window.setTimeout(() => {
      programmaticScroll.current = false;
    }, 200);
    return () => window.clearTimeout(timer);
  }, [document, onVisiblePage, pitch, scrollRequest?.offset, scrollRequest?.token]);

  useLayoutEffect(() => {
    const scroller = scrollerRef.current;
    if (!scroller) return;
    const pointers = new Map<number, { x: number; y: number }>();
    let pinchStart = 0;
    let pinchZoom = zoomRef.current;
    let gestureZoom = zoomRef.current;
    let wheelActive = false;
    let wheelIdle = 0;
    let lastWheelAt = 0;
    let lastPointer = { x: 0, y: 0, seen: false };

    const publish = (next: number) => {
      gestureZoom = Math.min(180, Math.max(60, next));
      const rounded = clampReaderZoom(gestureZoom);
      if (rounded === zoomRef.current) return;
      zoomRef.current = rounded;
      onZoomChangeRef.current?.(rounded);
    };

    const beginWheelGesture = () => {
      if (!wheelActive) {
        gestureZoom = zoomRef.current;
        wheelActive = true;
      }
      window.clearTimeout(wheelIdle);
      wheelIdle = window.setTimeout(() => {
        wheelActive = false;
      }, 160);
    };

    const overReader = (event: {
      target?: EventTarget | null;
      clientX?: number;
      clientY?: number;
    }) => eventOverElement(event, scroller);

    const onWheel = (event: WheelEvent) => {
      if (!isPinchZoomWheel(event) || !overReader(event)) return;
      event.preventDefault();
      lastWheelAt = performance.now();
      beginWheelGesture();
      publish(applyWheelZoom(gestureZoom, wheelDeltaPixels(event)));
    };

    const syncPinchOrigin = () => {
      const points = [...pointers.values()];
      if (points.length !== 2) {
        pinchStart = 0;
        return;
      }
      pinchStart = pointerDistance(points[0], points[1]);
      pinchZoom = gestureZoom = zoomRef.current;
    };

    const onPointerDown = (event: PointerEvent) => {
      lastPointer = { x: event.clientX, y: event.clientY, seen: true };
      if (pointers.size === 0 && !overReader(event)) return;
      pointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
      if (event.pointerType !== "mouse") {
        try {
          scroller.setPointerCapture(event.pointerId);
        } catch {
          /* jsdom and some hosts do not implement capture */
        }
      }
      if (pointers.size === 2) syncPinchOrigin();
    };
    const onPointerMove = (event: PointerEvent) => {
      lastPointer = { x: event.clientX, y: event.clientY, seen: true };
      if (!pointers.has(event.pointerId)) return;
      pointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
      if (pointers.size !== 2 || pinchStart <= 0) return;
      event.preventDefault();
      const points = [...pointers.values()];
      publish(
        applyPinchZoom(
          pinchZoom,
          pinchStart,
          pointerDistance(points[0], points[1]),
        ),
      );
    };
    const onPointerUp = (event: PointerEvent) => {
      pointers.delete(event.pointerId);
      if (pointers.size < 2) pinchStart = 0;
    };

    const touchDistance = (touches: TouchList) =>
      pointerDistance(
        { x: touches[0].clientX, y: touches[0].clientY },
        { x: touches[1].clientX, y: touches[1].clientY },
      );

    const onTouchStart = (event: TouchEvent) => {
      if (event.touches.length === 2) {
        pinchStart = touchDistance(event.touches);
        pinchZoom = gestureZoom = zoomRef.current;
      }
    };
    const onTouchMove = (event: TouchEvent) => {
      if (event.touches.length !== 2 || pinchStart <= 0) return;
      event.preventDefault();
      publish(applyPinchZoom(pinchZoom, pinchStart, touchDistance(event.touches)));
    };
    const onTouchEnd = (event: TouchEvent) => {
      if (event.touches.length < 2) pinchStart = 0;
    };

    const onGestureStart = (event: Event) => {
      event.preventDefault();
      pinchZoom = gestureZoom = zoomRef.current;
    };
    const onGesture = (event: Event) => {
      const gesture = event as Event & { scale?: number };
      if (typeof gesture.scale !== "number") return;
      event.preventDefault();
      publish(applyPinchZoom(pinchZoom, 1, gesture.scale));
    };

    const pointerInsideReader = () => {
      if (typeof scroller.matches === "function" && scroller.matches(":hover")) {
        return true;
      }
      if (!lastPointer.seen) return true;
      return eventOverElement(
        { clientX: lastPointer.x, clientY: lastPointer.y },
        scroller,
      );
    };

    const onNativePinch = (factor: number) => {
      if (performance.now() - lastWheelAt < 100) return;
      if (!pointerInsideReader()) return;
      beginWheelGesture();
      publish(gestureZoom * factor);
    };

    const wheelOptions: AddEventListenerOptions = {
      capture: true,
      passive: false,
    };
    const touchOptions: AddEventListenerOptions = { passive: false };
    const pointerOptions: AddEventListenerOptions = { capture: true };
    window.addEventListener("wheel", onWheel, wheelOptions);
    scroller.addEventListener("wheel", onWheel, wheelOptions);
    window.addEventListener("pointerdown", onPointerDown, pointerOptions);
    window.addEventListener("pointermove", onPointerMove, pointerOptions);
    window.addEventListener("pointerup", onPointerUp, pointerOptions);
    window.addEventListener("pointercancel", onPointerUp, pointerOptions);
    scroller.addEventListener("touchstart", onTouchStart, touchOptions);
    scroller.addEventListener("touchmove", onTouchMove, touchOptions);
    scroller.addEventListener("touchend", onTouchEnd);
    scroller.addEventListener("touchcancel", onTouchEnd);
    scroller.addEventListener("gesturestart", onGestureStart, touchOptions);
    scroller.addEventListener("gesturechange", onGesture, touchOptions);
    const stopNative = subscribeNativePinchZoom(onNativePinch);
    return () => {
      window.clearTimeout(wheelIdle);
      window.removeEventListener("wheel", onWheel, wheelOptions);
      scroller.removeEventListener("wheel", onWheel, wheelOptions);
      window.removeEventListener("pointerdown", onPointerDown, pointerOptions);
      window.removeEventListener("pointermove", onPointerMove, pointerOptions);
      window.removeEventListener("pointerup", onPointerUp, pointerOptions);
      window.removeEventListener("pointercancel", onPointerUp, pointerOptions);
      scroller.removeEventListener("touchstart", onTouchStart);
      scroller.removeEventListener("touchmove", onTouchMove);
      scroller.removeEventListener("touchend", onTouchEnd);
      scroller.removeEventListener("touchcancel", onTouchEnd);
      scroller.removeEventListener("gesturestart", onGestureStart);
      scroller.removeEventListener("gesturechange", onGesture);
      stopNative();
    };
  }, [document]);

  if (loadError) {
    return (
      <div className="pdf-reader-fallback" role="alert">
        <span>{t("reader.fallback.badge")}</span>
        <strong>{t("reader.fallback.title")}</strong>
        <p>{loadError}</p>
        {onOpenExternal ? (
          <button type="button" onClick={onOpenExternal}>
            {t("reader.fallback.openExternal")}
          </button>
        ) : null}
      </div>
    );
  }

  if (!document) {
    return (
      <div className="pdf-reader-loading" role="status">
        <span />
        {t("reader.loading")}
      </div>
    );
  }

  return (
    <div
      className={`pdf-reader ${guideLayerVisible ? "has-guide-layer" : ""}`}
      aria-label={t("reader.aria")}
      tabIndex={0}
      ref={scrollerRef}
      onScroll={(event) => {
        const offset = event.currentTarget.scrollTop;
        onScrollOffset?.(offset);
        if (programmaticScroll.current) return;
        const nextPage = pageFromScrollOffset(
          offset,
          pitch,
          document.numPages,
          READER_WINDOW_PADDING_TOP,
          extentsRef.current,
        );
        if (nextPage === lastReportedPage.current) return;
        lastReportedPage.current = nextPage;
        setViewPage(nextPage);
        onVisiblePage?.(nextPage);
      }}
    >
      <div
        className="pdf-reader-window"
        style={{
          height: readerWindowHeight(
            document.numPages,
            pitch,
            extentsRef.current,
          ),
        }}
      >
        {pages.map((pageNumber) => (
          <div
            key={pageNumber}
            className="pdf-reader-slot"
            style={{
              top: scrollOffsetForPage(
                pageNumber,
                pitch,
                READER_WINDOW_PADDING_TOP,
                extentsRef.current,
              ),
            }}
          >
            <PdfPage
              document={document}
              blocks={ocrBlocks.filter(
                (block) => block.pageNumber === pageNumber,
              )}
              quotedBlockIds={quotedIds}
              cachedBlockIds={cachedIds}
              focusedBlockId={focusedBlockId}
              onToggleQuote={onToggleQuote}
              onSelectBlock={onSelectBlock}
              onClearBlockSelection={onClearBlockSelection}
              onBlockAction={onBlockAction}
              highlightedBlockIds={highlightedIds}
              bookmarkedBlockIds={bookmarkedIds}
              notedBlockIds={notedIds}
              onAnnotationAction={onAnnotationAction}
              pageNumber={pageNumber}
              current={pageNumber === viewPage}
              zoom={zoom}
              rotation={rotation}
              guideLayerVisible={guideLayerVisible}
              guideInks={guideInks}
              guideCastSnapshot={guideCastSnapshot}
              workspaceRoot={workspaceRoot}
              activeGuideInkId={activeGuideInkId}
              onActivateGuideInk={onActivateGuideInk}
              onPageExtent={rememberPageExtent}
            />
          </div>
        ))}
      </div>
      <div className="pdf-reader-range" aria-hidden="true">{t("completion.active_pages", { start: pages[0], end: pages[pages.length - 1] })}</div>
    </div>
  );
}
