export type ReaderLocation = Readonly<{
  page: number;
  pageOffset?: number;
  zoom?: number;
  rotation?: number;
  focusedBlockId?: string | null;
  rightTab?: string;
  activeArtifactId?: string | null;
  artifactScope?: string;
  workspaceLayout?: string;
  outlineView?: string;
  outlineNodeId?: string | null;
  activeGuideInkId?: string | null;
  selectedAnnotationId?: string | null;
  paperId?: string;
  revisionId?: string;
}>;

export type ReaderHistory = readonly ReaderLocation[];

export const DEFAULT_READER_HISTORY_LIMIT = 50;

const LOCATION_KEYS: ReadonlyArray<keyof ReaderLocation> = [
  "page",
  "pageOffset",
  "zoom",
  "rotation",
  "focusedBlockId",
  "rightTab",
  "activeArtifactId",
  "artifactScope",
  "workspaceLayout",
  "outlineView",
  "outlineNodeId",
  "activeGuideInkId",
  "selectedAnnotationId",
  "paperId",
  "revisionId",
];

export function sameReaderLocation(
  left: ReaderLocation | null | undefined,
  right: ReaderLocation | null | undefined,
): boolean {
  if (left === right) return true;
  if (left == null || right == null) return false;
  return LOCATION_KEYS.every((key) => left[key] === right[key]);
}

function normalizedLimit(limit: number): number {
  if (!Number.isFinite(limit)) return DEFAULT_READER_HISTORY_LIMIT;
  return Math.max(1, Math.floor(limit));
}

export function pushReaderLocation(
  history: ReaderHistory,
  location: ReaderLocation,
  limit = DEFAULT_READER_HISTORY_LIMIT,
): ReaderHistory {
  if (sameReaderLocation(history[history.length - 1], location)) {
    return history;
  }
  const next = [...history, { ...location }];
  return next.slice(-normalizedLimit(limit));
}

export type ReaderJumpResult = {
  history: ReaderHistory;
  changed: boolean;
};

/**
 * Prepares an intentional jump in one operation. The current location is
 * recorded only when the destination differs, which keeps repeated evidence
 * clicks from creating useless back entries.
 */
export function jumpWithHistory(
  history: ReaderHistory,
  current: ReaderLocation,
  destination: ReaderLocation,
  limit = DEFAULT_READER_HISTORY_LIMIT,
): ReaderJumpResult {
  if (sameReaderLocation(current, destination)) {
    return { history, changed: false };
  }
  return {
    history: pushReaderLocation(history, current, limit),
    changed: true,
  };
}

export type ReaderHistoryPop = {
  history: ReaderHistory;
  location: ReaderLocation | null;
};

export function popReaderLocation(history: ReaderHistory): ReaderHistoryPop {
  if (history.length === 0) return { history, location: null };
  const index = history.length - 1;
  return {
    history: history.slice(0, index),
    location: history[index] ?? null,
  };
}

export function canGoBackReader(history: ReaderHistory): boolean {
  return history.length > 0;
}

export function clearReaderHistory(): ReaderHistory {
  return [];
}
