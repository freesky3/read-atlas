import type {
  BlockQuoteSnapshot,
  OcrBlockProjection,
  OcrProjection,
} from "./types";

export function isFigureQuote(quote: {
  blockType?: string;
  textContent?: string;
}): boolean {
  const type = (quote.blockType || "").toLowerCase();
  if (
    type === "image" ||
    type === "figure" ||
    type === "picture" ||
    type === "photo" ||
    type === "illustration"
  ) {
    return true;
  }
  const text = (quote.textContent || "").trim();
  return text.startsWith("![") && text.includes("]");
}

export function formatQuoteCaption(
  textContent: string | undefined,
  defaultCaption: string,
): string {
  const trimmed = (textContent || "").trim();
  const match = trimmed.match(/^!\[(.*?)\]\(.*?\)$/);
  if (match) {
    const alt = match[1]?.trim();
    if (alt && !alt.startsWith("img-") && !alt.startsWith("image-")) {
      return alt;
    }
  }
  if (!trimmed || trimmed.startsWith("![")) {
    return defaultCaption;
  }
  return trimmed;
}

export function snapshotBlock(
  revisionId: string,
  ocrRevisionId: string,
  block: OcrBlockProjection,
  cropDataUrl?: string | null,
): BlockQuoteSnapshot {
  return {
    revisionId,
    ocrRevisionId,
    blockId: block.id,
    pageNumber: block.pageNumber,
    blockIndex: block.blockIndex,
    blockType: block.blockType,
    textContent: block.textContent,
    contentDigest: block.contentDigest,
    bbox: block.bbox,
    ...(cropDataUrl ? { cropDataUrl } : {}),
  };
}

export function reconcileQuoteBasket(
  revisionId: string,
  ocr: OcrProjection | null,
  saved: BlockQuoteSnapshot[],
): BlockQuoteSnapshot[] {
  if (!ocr || ocr.revisionId !== revisionId || !Array.isArray(saved)) return [];
  const blocks = new Map(ocr.blocks.map((block) => [block.id, block]));
  const seen = new Set<string>();
  const restored: BlockQuoteSnapshot[] = [];
  for (const quote of saved) {
    const block = blocks.get(quote?.blockId);
    if (!block || seen.has(block.id)) continue;
    if (
      quote.ocrRevisionId !== ocr.id ||
      quote.contentDigest !== block.contentDigest
    ) {
      continue;
    }
    seen.add(block.id);
    restored.push(snapshotBlock(revisionId, ocr.id, block));
  }
  return restored.slice(0, 32);
}
