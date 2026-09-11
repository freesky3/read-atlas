import type { DocumentCard } from "./types";

export type HubSortMode = "recent" | "year" | "title" | "manual" | "chapter" | "last_opened";

export function parseChapterNumber(raw?: string): number[] | null {
  if (!raw) return null;
  const trimmed = raw.trim();
  if (!trimmed) return null;
  const parts = trimmed.split(/[.\-]/);
  const nums: number[] = [];
  for (const p of parts) {
    if (!/^\d+$/.test(p)) return null;
    nums.push(Number(p));
  }
  return nums;
}

/** 与后端 `chapter_sort_key` 同一份键：可解析章节号按段零填充，无法解析的排在后面。 */
export function chapterSortKey(raw?: string | null): string {
  const parts = parseChapterNumber(raw ?? undefined);
  if (!parts) return `~\t${raw ?? ""}`;
  return parts.map((n) => String(n).padStart(8, "0")).join(".");
}

export function compareChapterNumbers(a?: string, b?: string): number {
  const pa = parseChapterNumber(a);
  const pb = parseChapterNumber(b);
  const aHas = pa !== null;
  const bHas = pb !== null;
  if (!aHas && !bHas) return 0;
  if (!aHas) return 1;
  if (!bHas) return -1;
  const na = pa!;
  const nb = pb!;
  const len = Math.min(na.length, nb.length);
  for (let i = 0; i < len; i++) {
    if (na[i] !== nb[i]) return na[i] - nb[i];
  }
  if (na.length === nb.length) return 0;
  return na.length < nb.length ? -1 : 1;
}

export function folderHasDescendantPdfs(documents: DocumentCard[], selectedFolder: string): boolean {
  if (!selectedFolder) return true;
  const prefix = selectedFolder + "/";
  return documents.some((doc) => doc.collection !== selectedFolder && doc.collection.startsWith(prefix));
}

export function hubSortAvailability(
  documents: DocumentCard[],
  selectedFolder: string
): { manual: boolean; chapter: boolean } {
  if (selectedFolder.startsWith("smart:")) return { manual: false, chapter: false };
  const manual = selectedFolder !== "" && !folderHasDescendantPdfs(documents, selectedFolder);
  if (!manual) return { manual: false, chapter: false };
  const exact = documents.filter((d) => d.collection === selectedFolder);
  const hasChapter = exact.some((d) => parseChapterNumber(d.chapterNumber) !== null);
  return { manual, chapter: manual && hasChapter };
}

export function mergeManualOrder(exactLayerDocs: DocumentCard[], paperOrder: string[]): DocumentCard[] {
  if (paperOrder.length === 0) {
    return [...exactLayerDocs].sort((a, b) => (b.importedAt || "").localeCompare(a.importedAt || ""));
  }
  const liveSet = new Set(exactLayerDocs.map((d) => d.id));
  const orderedIds = paperOrder.filter((id) => liveSet.has(id));
  const orderedSet = new Set(orderedIds);
  const orderedDocs: DocumentCard[] = [];
  const docById = new Map(exactLayerDocs.map((d) => [d.id, d] as const));
  for (const id of orderedIds) {
    const doc = docById.get(id);
    if (doc) orderedDocs.push(doc);
  }
  const remaining = exactLayerDocs
    .filter((d) => !orderedSet.has(d.id))
    .sort((a, b) => (a.importedAt || "").localeCompare(b.importedAt || ""));
  return [...orderedDocs, ...remaining];
}

export function applyHubSort(docs: DocumentCard[], mode: HubSortMode, paperOrder: string[]): DocumentCard[] {
  if (mode === "last_opened") {
    return [...docs].sort((a, b) => (b.lastOpenedAt || "").localeCompare(a.lastOpenedAt || ""));
  }
  if (mode === "year") {
    return [...docs].sort((a, b) => (b.year || "").localeCompare(a.year || ""));
  }
  if (mode === "title") {
    return [...docs].sort((a, b) => a.title.localeCompare(b.title));
  }
  if (mode === "chapter") {
    return [...docs].sort((a, b) => {
      const ca = parseChapterNumber(a.chapterNumber);
      const cb = parseChapterNumber(b.chapterNumber);
      const aHas = ca !== null;
      const bHas = cb !== null;
      if (aHas && bHas) {
        const c = compareChapterNumbers(a.chapterNumber, b.chapterNumber);
        if (c !== 0) return c;
        return (b.importedAt || "").localeCompare(a.importedAt || "");
      }
      if (aHas && !bHas) return -1;
      if (!aHas && bHas) return 1;
      return (b.importedAt || "").localeCompare(a.importedAt || "");
    });
  }
  if (mode === "manual") {
    if (paperOrder.length === 0) {
      return [...docs].sort((a, b) => (b.importedAt || "").localeCompare(a.importedAt || ""));
    }
    return mergeManualOrder(docs, paperOrder);
  }
  return [...docs].sort((a, b) => (b.importedAt || "").localeCompare(a.importedAt || ""));
}

export function insertIndexFromRect(
  rect: { left: number; right: number; top: number; bottom: number; width: number; height: number },
  clientX: number,
  clientY: number,
  axis: "x" | "y"
): "before" | "after" {
  if (axis === "x") {
    const mid = rect.left + rect.width / 2;
    return clientX < mid ? "before" : "after";
  }
  const mid = rect.top + rect.height / 2;
  return clientY < mid ? "before" : "after";
}

export function moveItem(
  ids: string[],
  fromId: string,
  targetId: string | null,
  place: "before" | "after" | "end"
): string[] {
  if (!ids.includes(fromId)) return [...ids];
  if (fromId === targetId) return [...ids];
  const filtered = ids.filter((id) => id !== fromId);
  if (place === "end" || targetId === null) {
    return [...filtered, fromId];
  }
  const idx = filtered.indexOf(targetId);
  if (idx === -1) return [...filtered, fromId];
  if (place === "before") {
    filtered.splice(idx, 0, fromId);
  } else {
    filtered.splice(idx + 1, 0, fromId);
  }
  return filtered;
}
