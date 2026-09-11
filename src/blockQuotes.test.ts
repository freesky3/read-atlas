import { describe, expect, it } from "vitest";
import {
  reconcileQuoteBasket,
  snapshotBlock,
  isFigureQuote,
  formatQuoteCaption,
} from "./blockQuotes";
import type { BlockQuoteSnapshot, OcrProjection } from "./types";

function ocrFixture(): OcrProjection {
  return {
    id: "ocr-current",
    revisionId: "revision-current",
    status: "ready",
    provider: "mistral",
    model: "mistral-ocr-latest",
    createdAt: "2026-08-16T00:00:00Z",
    publishedAt: "2026-08-16T00:00:01Z",
    blocks: [
      {
        id: "block-p1",
        pageNumber: 1,
        blockIndex: 0,
        blockType: "paragraph",
        textContent: "Canonical first Block",
        contentDigest: "digest-p1",
        bbox: [10, 20, 500, 100],
      },
      {
        id: "block-p8",
        pageNumber: 8,
        blockIndex: 3,
        blockType: "formula",
        textContent: "E = mc^2",
        contentDigest: "digest-p8",
        bbox: [200, 300, 800, 600],
      },
      {
        id: "block-p3",
        pageNumber: 3,
        blockIndex: 7,
        blockType: "table",
        textContent: "Canonical table",
        contentDigest: "digest-p3",
        bbox: [100, 250, 900, 850],
      },
    ],
  };
}

function forgedQuote(blockId: string): BlockQuoteSnapshot {
  return {
    revisionId: "revision-current",
    ocrRevisionId: "ocr-current",
    blockId,
    pageNumber: 999,
    blockIndex: 999,
    blockType: "forged",
    textContent: "forged text",
    contentDigest:
      blockId === "block-p8"
        ? "digest-p8"
        : blockId === "block-p3"
          ? "digest-p3"
          : "digest-p1",
    bbox: [0, 0, 1000, 1000],
  };
}

describe("Block quote reconciliation", () => {
  it("restores non-contiguous cross-page order from canonical OCR facts", () => {
    const restored = reconcileQuoteBasket("revision-current", ocrFixture(), [
      forgedQuote("block-p8"),
      forgedQuote("block-p1"),
      forgedQuote("block-p8"),
      forgedQuote("block-p3"),
    ]);

    expect(restored.map((quote) => quote.blockId)).toEqual([
      "block-p8",
      "block-p1",
      "block-p3",
    ]);
    expect(restored[0]).toEqual(
      snapshotBlock("revision-current", "ocr-current", ocrFixture().blocks[1]),
    );
    expect(restored[0].textContent).toBe("E = mc^2");
    expect(restored[0].bbox).toEqual([200, 300, 800, 600]);
  });

  it("drops stale OCR snapshots, changed digests, and revision mismatches", () => {
    const staleOcr = { ...forgedQuote("block-p1"), ocrRevisionId: "ocr-old" };
    const changed = { ...forgedQuote("block-p3"), contentDigest: "old-digest" };
    expect(
      reconcileQuoteBasket("revision-current", ocrFixture(), [
        staleOcr,
        changed,
      ]),
    ).toEqual([]);
    expect(
      reconcileQuoteBasket("revision-other", ocrFixture(), [
        forgedQuote("block-p1"),
      ]),
    ).toEqual([]);
  });

  it("caps a persisted basket at the backend contract limit", () => {
    const ocr = ocrFixture();
    ocr.blocks = Array.from({ length: 35 }, (_, index) => ({
      ...ocr.blocks[0],
      id: `block-${index}`,
      blockIndex: index,
      contentDigest: `digest-${index}`,
    }));
    const saved = ocr.blocks.map((block) =>
      snapshotBlock(ocr.revisionId, ocr.id, block),
    );
    expect(reconcileQuoteBasket(ocr.revisionId, ocr, saved)).toHaveLength(32);
  });

  it("identifies figure quotes from blockType or markdown image syntax", () => {
    expect(isFigureQuote({ blockType: "figure" })).toBe(true);
    expect(isFigureQuote({ blockType: "image" })).toBe(true);
    expect(isFigureQuote({ blockType: "picture" })).toBe(true);
    expect(isFigureQuote({ textContent: "![img-9.jpeg](img-9.jpeg)" })).toBe(true);
    expect(isFigureQuote({ textContent: "![Architecture](fig1.png)" })).toBe(true);
    expect(isFigureQuote({ blockType: "paragraph", textContent: "Regular paragraph text" })).toBe(false);
  });

  it("formats quote captions cleanly for thumbnails and lightboxes", () => {
    expect(formatQuoteCaption("![Figure 1: Recurrent network](fig1.png)", "Figure p.3")).toBe("Figure 1: Recurrent network");
    expect(formatQuoteCaption("![img-9.jpeg](img-9.jpeg)", "Figure p.6")).toBe("Figure p.6");
    expect(formatQuoteCaption("", "Figure p.1")).toBe("Figure p.1");
    expect(formatQuoteCaption("Loss vs Epochs", "Figure p.2")).toBe("Loss vs Epochs");
  });
});
