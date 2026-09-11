import type { DocumentCard } from "../types";
import type { HubPaperCard } from "./libraryWorkspaceTypes";
import type { ReadingLifecycleStatus } from "../readerReadingState";

function briefStatusOf(value: string): DocumentCard["briefStatus"] {
  if (
    value === "queued" ||
    value === "ready" ||
    value === "stale" ||
    value === "failed"
  )
    return value;
  return "missing";
}

function lifecycleStatusOf(value: string): ReadingLifecycleStatus {
  if (value === "reading" || value === "read") return value;
  return "unread";
}

/** `hub_page` 卡片 → Hub 现有渲染合同。绝对路径不会出现在投影里。 */
export function documentCardFromHub(card: HubPaperCard): DocumentCard {
  return {
    id: card.id,
    revisionId: card.revisionId,
    title: card.title,
    authors: card.authors.join(", "),
    year:
      (card.displayMetadata
        ? card.displayMetadata.year
        : card.publicationYear) != null
        ? String(
            card.displayMetadata
              ? card.displayMetadata.year
              : card.publicationYear,
          )
        : "",
    yearLabel: card.displayMetadata?.yearLabel,
    pages: card.pageCount ?? 0,
    collection: card.collectionPath,
    kind: card.kind,
    sha256: card.sha256,
    pdfPath: card.relativePath,
    sourceStatus: "ready",
    briefStatus: briefStatusOf(card.briefStatus),
    hasOcr: card.hasOcr,
    briefTakeaway: card.briefTakeaway ?? undefined,
    // The legacy render field holds editable library tags, not Brief keywords.
    keywords: [...card.tags],
    chapterNumber: card.chapterNumber ?? undefined,
    importedAt: card.importedAt,
    fileName: card.fileName,
    lifecycleStatus: lifecycleStatusOf(card.lifecycleStatus),
    favorite: card.favorite,
    priority: card.priority as 0 | 1 | 2 | 3,
    readLater: card.readLater,
    reviewAt: card.reviewAt,
    lifecycleVersion: card.lifecycleVersion,
    furthestPage: card.furthestPage,
    lastOpenedAt: card.lastOpenedAt,
    ocrFailed: card.ocrFailed,
  };
}
