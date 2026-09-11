import type { AnnotationLocator, OcrBlockProjection, UserAnnotation } from "./types";

const EPSILON = 1e-6;

function normalizeText(value: string | null | undefined) {
  return (value ?? "")
    .toLocaleLowerCase()
    .replace(/\\s+/g, " ")
    .replace(/[^\\p{L}\\p{N}]+/gu, " ")
    .trim();
}

function iou(left: readonly number[] | null | undefined, right: readonly number[]) {
  if (!left || left.length !== 4) return 0;
  const x0 = Math.max(left[0], right[0]);
  const y0 = Math.max(left[1], right[1]);
  const x1 = Math.min(left[2], right[2]);
  const y1 = Math.min(left[3], right[3]);
  const intersection = Math.max(0, x1 - x0) * Math.max(0, y1 - y0);
  const leftArea = Math.max(0, left[2] - left[0]) * Math.max(0, left[3] - left[1]);
  const rightArea = Math.max(0, right[2] - right[0]) * Math.max(0, right[3] - right[1]);
  const union = leftArea + rightArea - intersection;
  return union <= EPSILON ? 0 : intersection / union;
}

function textSimilarity(left: string | null | undefined, right: string) {
  const a = normalizeText(left);
  const b = normalizeText(right);
  if (!a || !b) return 0;
  if (a === b) return 1;
  if (a.length >= 12 && (a.includes(b) || b.includes(a))) return 0.88;
  const aTokens = new Set(a.split(" ").filter(Boolean));
  const bTokens = new Set(b.split(" ").filter(Boolean));
  const shared = [...aTokens].filter((token) => bTokens.has(token)).length;
  const total = new Set([...aTokens, ...bTokens]).size;
  return total === 0 ? 0 : shared / total;
}

function locatorForBlock(previous: AnnotationLocator, block: OcrBlockProjection, ocrRevisionId?: string | null): AnnotationLocator {
  return {
    ...previous,
    ocrRevisionId: ocrRevisionId ?? previous.ocrRevisionId ?? null,
    blockId: block.id,
    blockIndex: block.blockIndex,
    blockType: block.blockType,
    contentDigest: block.contentDigest,
    excerpt: block.textContent,
    pageNumber: block.pageNumber,
    bbox: block.bbox,
  };
}

function migrated(annotation: UserAnnotation, block: OcrBlockProjection, ocrRevisionId?: string | null): UserAnnotation {
  return { ...annotation, status: "migrated", locator: locatorForBlock(annotation.locator, block, ocrRevisionId), updatedAt: new Date().toISOString() };
}

/**
 * Reconcile one annotation against the newly published OCR block catalog.
 * Returns a new object and never mutates the saved snapshot.
 */
export function reconcileAnnotation(
  annotation: UserAnnotation,
  blocks: readonly OcrBlockProjection[],
  ocrRevisionId?: string | null,
): UserAnnotation {
  const locator = annotation.locator;
  const samePage = blocks.filter((block) => block.pageNumber === locator.pageNumber);
  const exactId = locator.blockId ? blocks.find((block) => block.id === locator.blockId) : undefined;
  const sameRevision = Boolean(locator.ocrRevisionId && locator.ocrRevisionId === ocrRevisionId);
  if (exactId && sameRevision && (!locator.contentDigest || locator.contentDigest === exactId.contentDigest)) {
    return { ...annotation, status: "active", locator: locatorForBlock(locator, exactId, ocrRevisionId) };
  }

  // An unchanged block id is not sufficient evidence after OCR has changed.
  // Do not silently fuzzy-migrate a locator whose revision or content digest
  // explicitly disagrees with the block still carrying that id.
  if (
    exactId &&
    ((Boolean(locator.ocrRevisionId) && !sameRevision) ||
      (Boolean(locator.contentDigest) &&
        locator.contentDigest !== exactId.contentDigest))
  ) {
    return { ...annotation, status: "orphaned" };
  }

  if (locator.contentDigest) {
    const digestMatches = samePage.filter((block) => block.contentDigest === locator.contentDigest);
    if (digestMatches.length === 1) return migrated(annotation, digestMatches[0], ocrRevisionId);
    if (digestMatches.length > 1) {
      return { ...annotation, status: "orphaned" };
    }
  }

  const scored = samePage
    .map((block) => ({ block, score: textSimilarity(locator.excerpt, block.textContent) * 0.7 + iou(locator.bbox, block.bbox) * 0.3 }))
    .filter((candidate) => candidate.score >= 0.62)
    .sort((left, right) => right.score - left.score);
  const best = scored[0];
  const second = scored[1];
  if (best && (!second || best.score - second.score > 0.08)) {
    return migrated(annotation, best.block, ocrRevisionId);
  }
  return { ...annotation, status: "orphaned" };
}

export function reconcileAnnotations(
  annotations: readonly UserAnnotation[],
  blocks: readonly OcrBlockProjection[],
  ocrRevisionId?: string | null,
): UserAnnotation[] {
  return annotations.map((annotation) => reconcileAnnotation(annotation, blocks, ocrRevisionId));
}
