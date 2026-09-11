// 把 Hub 当前的物理目录 + 检索框编译成 `hub_page` / Selection Snapshot 用的筛选 AST。
// 必须与 `library_query::filter_predicate` 同一套语义：目录可递归，文本只打
// title / file_name / authors，不能把 Brief 检索假装成后端查询。

import type { DocumentCard } from "../types";
import {
  smartCollectionIdOf,
  type HubPageResult,
  type HubPaperCard,
  type LibraryHubSort,
  type LibraryQueryFilter,
  type SmartCollectionProjection,
} from "./libraryWorkspaceTypes";
import type { QuerySnapshot } from "./selectionModel";

export function hubFiltersForView(
  folder: string,
  search: string,
  smart?: Pick<SmartCollectionProjection, "query"> | null,
): LibraryQueryFilter[] {
  const filters: LibraryQueryFilter[] = smart ? [...smart.query.filters] : [];
  if (!smart) {
    const path = folder.trim();
    if (path && !smartCollectionIdOf(path)) {
      filters.push({ kind: "collection", path, recursive: true });
    }
  }
  const term = search.trim();
  if (term) filters.push({ kind: "text", term });
  return filters;
}

export function hubSortForView(mode: string): LibraryHubSort {
  if (
    mode === "year" ||
    mode === "title" ||
    mode === "manual" ||
    mode === "last_opened" ||
    mode === "chapter"
  ) {
    return mode;
  }
  return "recent";
}

export function hubFiltersForExactCollection(
  folder: string,
  search: string,
  smart?: Pick<SmartCollectionProjection, "query"> | null,
): LibraryQueryFilter[] {
  const filters = hubFiltersForView(folder, search, smart);
  return filters.map((filter) =>
    filter.kind === "collection" ? { ...filter, recursive: false } : filter,
  );
}

export function querySnapshotFromPage(page: HubPageResult): QuerySnapshot {
  const dependencyRevisions: Record<string, number> = {};
  for (const entry of page.selectionDependencies) {
    dependencyRevisions[entry.domain] = entry.value;
  }
  return {
    queryDigest: page.selectionDigest,
    dependencyRevisions,
    evaluatedAt: page.evaluationAnchor,
    timezone: page.evaluationTimezone,
  };
}

export function matchesHubTextFilter(
  term: string,
  fields: { title: string; authors: string; fileName: string },
): boolean {
  const needle = term.trim().toLowerCase();
  if (!needle) return true;
  return [fields.title, fields.authors, fields.fileName]
    .map((value) => value.toLowerCase())
    .some((value) => value.includes(needle));
}

/** 「本周」= evaluation anchor 所在周的周一 00:00 UTC，不在保存查询时固化成某一天。 */
export function thisWeekStartUtc(anchor: string): string {
  const parsed = new Date(anchor);
  if (Number.isNaN(parsed.getTime())) return new Date(0).toISOString();
  const day = parsed.getUTCDay();
  const daysFromMonday = (day + 6) % 7;
  return new Date(
    Date.UTC(parsed.getUTCFullYear(), parsed.getUTCMonth(), parsed.getUTCDate() - daysFromMonday),
  ).toISOString();
}

function normalizeStatus(value: string | undefined): string {
  if (value === "read" || value === "done" || value === "completed") return "read";
  if (value === "reading") return "reading";
  return "unread";
}

export function normalizeLibraryCollectionPath(path: string): string | null {
  const segments = path
    .replaceAll("\\", "/")
    .trim()
    .split("/")
    .filter((segment) => segment !== "");
  const first = segments[0];
  if (!first) return null;
  const root = /^papers$/i.test(first)
    ? "Papers"
    : /^textbooks$/i.test(first)
      ? "Textbooks"
      : null;
  const canonical = root
    ? [root, ...segments.slice(1)].join("/")
    : `Papers/${segments.join("/")}`;
  if (canonical.split("/").some((segment) => segment === "." || segment === "..")) {
    return null;
  }
  return canonical.replace(/\/+$/, "");
}

export function cardMatchesLibraryFilter(
  filter: LibraryQueryFilter,
  card: HubPaperCard,
  evaluationAnchor: string,
): boolean {
  switch (filter.kind) {
    case "collection": {
      const path = normalizeLibraryCollectionPath(filter.path);
      if (path === null) return false;
      return (
        card.collectionPath === path ||
        (filter.recursive && card.collectionPath.startsWith(`${path}/`))
      );
    }
    case "text": {
      const term = filter.term.trim().toLowerCase();
      return [card.title, card.fileName, card.authors.join(", ")]
        .map((value) => value.toLowerCase())
        .some((value) => value.includes(term));
    }
    case "tag": {
      const tag = filter.tag.trim().toLowerCase();
      return card.tags.some((name) => name.toLowerCase() === tag);
    }
    case "document":
      return card.relativePath === filter.value || card.relativePath.startsWith(`${filter.value}/`);
    case "status": {
      const wanted = normalizeStatus(filter.value);
      return normalizeStatus(card.lifecycleStatus) === wanted;
    }
    case "favorite":
      return card.favorite === filter.value;
    case "read_later":
      return card.readLater === filter.value;
    case "review_due":
      return Boolean(card.reviewAt && card.reviewAt <= evaluationAnchor);
    case "imported":
      return card.importedAt >= thisWeekStartUtc(evaluationAnchor);
    case "ocr_failed":
      return card.ocrFailed;
    case "opened":
      return Boolean(card.lastOpenedAt);
  }
}

export function documentMatchesLibraryFilter(
  filter: LibraryQueryFilter,
  doc: DocumentCard,
  evaluationAnchor: string,
): boolean {
  const card: HubPaperCard = {
    id: doc.id,
    revisionId: doc.revisionId,
    title: doc.title,
    authors: doc.authors ? [doc.authors] : [],
    publicationYear: doc.year ? Number(doc.year) : null,
    pageCount: doc.pages,
    collectionPath: doc.collection,
    relativePath: `${doc.collection}/${doc.fileName}`,
    fileName: doc.fileName,
    kind: doc.kind,
    sha256: doc.sha256,
    byteSize: 0,
    importedAt: doc.importedAt,
    tags: doc.keywords ?? [],
    hasOcr: Boolean(doc.hasOcr),
    briefStatus: doc.briefStatus,
    briefTakeaway: doc.briefTakeaway ?? null,
    keywords: doc.keywords ?? [],
    chapterNumber: doc.chapterNumber ?? null,
    sortKey: doc.importedAt,
    lifecycleStatus: doc.lifecycleStatus ?? "unread",
    favorite: doc.favorite === true,
    priority: doc.priority ?? 0,
    readLater: doc.readLater === true,
    reviewAt: doc.reviewAt ?? null,
    lifecycleVersion: doc.lifecycleVersion ?? 0,
    furthestPage: doc.furthestPage ?? null,
    lastOpenedAt: doc.lastOpenedAt ?? null,
    ocrFailed: doc.ocrFailed === true,
  };
  return cardMatchesLibraryFilter(filter, card, evaluationAnchor);
}

export function documentMatchesLibraryFilters(
  doc: DocumentCard,
  filters: readonly LibraryQueryFilter[],
  evaluationAnchor: string,
): boolean {
  return filters.every((filter) => documentMatchesLibraryFilter(filter, doc, evaluationAnchor));
}
