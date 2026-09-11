import { describe, expect, it } from "vitest";
import { reconcileAnnotation, reconcileAnnotations } from "./annotationLocator";
import type { OcrBlockProjection, UserAnnotation } from "./types";

const block = (overrides: Partial<OcrBlockProjection> = {}): OcrBlockProjection => ({
  id: "b-new",
  pageNumber: 2,
  blockIndex: 1,
  blockType: "paragraph",
  textContent: "The model improves calibration.",
  contentDigest: "digest-new",
  bbox: [100, 100, 700, 180],
  ...overrides,
});
const annotation = (overrides: Partial<UserAnnotation> = {}): UserAnnotation => ({
  id: "a-1",
  paperId: "paper-1",
  kind: "note",
  title: null,
  body: "remember this",
  color: null,
  status: "active",
  createdAt: "2026-01-01T00:00:00Z",
  updatedAt: "2026-01-01T00:00:00Z",
  locator: {
    documentRevisionId: "rev-1",
    ocrRevisionId: "ocr-old",
    blockId: "b-old",
    blockIndex: 1,
    blockType: "paragraph",
    contentDigest: "digest-old",
    excerpt: "The model improves calibration.",
    pageNumber: 2,
    bbox: [100, 100, 700, 180],
    coordinateSpace: "normalized_1000",
  },
  ...overrides,
});

describe("annotationLocator", () => {
  it("keeps an exact same-revision block active", () => {
    const current = block({ id: "b-old", contentDigest: "digest-old" });
    expect(reconcileAnnotation(annotation(), [current], "ocr-old").status).toBe("active");
  });
  it("migrates a unique same-page digest match", () => {
    const result = reconcileAnnotation(annotation(), [block()], "ocr-new");
    expect(result.status).toBe("migrated");
    expect(result.locator.blockId).toBe("b-new");
    expect(result.locator.ocrRevisionId).toBe("ocr-new");
  });
  it("marks ambiguous or missing matches orphaned without deleting the snapshot", () => {
    const ambiguous = reconcileAnnotation(annotation(), [block({ id: "b-1" }), block({ id: "b-2" })], "ocr-new");
    expect(ambiguous.status).toBe("orphaned");
    expect(ambiguous.locator.blockId).toBe("b-old");
    const missing = reconcileAnnotation(annotation(), [block({ pageNumber: 9 })], "ocr-new");
    expect(missing.status).toBe("orphaned");
    expect(missing.locator.excerpt).toContain("calibration");
  });
  it("does not match a same id when the OCR revision or digest changed", () => {
    const result = reconcileAnnotation(annotation(), [block({ id: "b-old", contentDigest: "other" })], "ocr-new");
    expect(result.status).toBe("orphaned");
  });
  it("reconciles a collection without mutating the input", () => {
    const original = annotation();
    const result = reconcileAnnotations([original], [block()], "ocr-new");
    expect(result[0].status).toBe("migrated");
    expect(original.status).toBe("active");
  });
});

