import { uiText, zhT, type TranslateFn } from "./i18n/uiText";
import type { ReadingState } from "./types";

/** Schema 8 CHECK：`unread | reading | read`。`done`/`completed` 只作旧投影别名。 */
export const READING_LIFECYCLE_STATUSES = ["unread", "reading", "read"] as const;
export type ReadingLifecycleStatus = (typeof READING_LIFECYCLE_STATUSES)[number];
export type ReadingLifecycleInputStatus = ReadingLifecycleStatus | "done" | "completed";
export type ReadingLifecycle = Readonly<{
  paperId: string;
  status: ReadingLifecycleStatus;
  favorite: boolean;
  priority: 0 | 1 | 2 | 3;
  readLater: boolean;
  reviewAt: string | null;
  statusChangedAt: string;
  completedAt: string | null;
  version: number;
  updatedAt: string;
}>;
export type ReadingEngagement = Readonly<{
  paperId: string;
  revisionId: string;
  furthestPage: number;
  pageCountSnapshot: number;
  firstOpenedAt: string;
  lastOpenedAt: string;
  updatedAt: string;
}>;
export type ReadingProgress = Readonly<{
  page: number;
  pageCount: number | null;
  fraction: number;
  percent: number;
  isAtEnd: boolean;
}>;
/** session is the public name; saved_state remains accepted for prototype callers. */
export type ResumeSource = "session" | "saved_state" | "engagement" | "start";
export type ResumeTarget = Readonly<{
  pageNumber: number;
  pageOffset: number;
  source: ResumeSource;
  staleSession: boolean;
  progress: ReadingProgress;
}>;

const DEFAULT_PAGE = 1;
const finiteNumber = (value: unknown, fallback: number): number =>
  typeof value === "number" && Number.isFinite(value) ? value : fallback;
const positiveInteger = (value: unknown, fallback: number): number => {
  const candidate = finiteNumber(value, fallback);
  return Number.isInteger(candidate) && candidate >= 1 ? candidate : fallback;
};
const clamp = (value: number, min: number, max: number): number =>
  Math.min(max, Math.max(min, value));
const isoNow = (now?: string): string =>
  now ?? new Date().toISOString();

export function normalizeReadingStatus(value: unknown): ReadingLifecycleStatus {
  if (value === "reading") return "reading";
  if (value === "read" || value === "done" || value === "completed") return "read";
  return "unread";
}

export function readingStatusLabel(status: ReadingLifecycleInputStatus, t: TranslateFn = zhT): string {
  return uiText(t, ({ unread: "未读", reading: "阅读中", read: "已读" } as const)[normalizeReadingStatus(status)]);
}

export function defaultReadingLifecycle(paperId: string, now = new Date().toISOString()): ReadingLifecycle {
  const timestamp = isoNow(now);
  return { paperId, status: "unread", favorite: false, priority: 0, readLater: false, reviewAt: null, statusChangedAt: timestamp, completedAt: null, version: 0, updatedAt: timestamp };
}

export function transitionReadingStatus(current: ReadingLifecycle | null | undefined, nextStatus: ReadingLifecycleInputStatus, now = new Date().toISOString()): ReadingLifecycle {
  const base = current ?? defaultReadingLifecycle("", now);
  const timestamp = isoNow(now);
  const status = normalizeReadingStatus(nextStatus);
  if (current && normalizeReadingStatus(current.status) === status) return current;
  return {
    ...base,
    status,
    statusChangedAt: timestamp,
    completedAt: status === "read" ? timestamp : null,
    version: Math.max(0, Math.trunc(base.version)) + 1,
    updatedAt: timestamp,
  };
}

export function readingProgress(page: number, pageCount: number | null | undefined): ReadingProgress {
  const safePage = Math.max(DEFAULT_PAGE, Math.trunc(finiteNumber(page, DEFAULT_PAGE)));
  const safeCount = pageCount == null || pageCount < 1 ? null : positiveInteger(pageCount, DEFAULT_PAGE);
  if (safeCount === null) return { page: safePage, pageCount: null, fraction: 0, percent: 0, isAtEnd: false };
  const boundedPage = clamp(safePage, DEFAULT_PAGE, safeCount);
  const fraction = safeCount <= 1 ? 1 : boundedPage / safeCount;
  return {
    page: boundedPage,
    pageCount: safeCount,
    fraction,
    percent: Math.round(fraction * 100),
    isAtEnd: boundedPage >= safeCount,
  };
}

/** 读到末页才询问，绝不按 90% / 95% 静默完成。 */
export function shouldPromptCompletion(status: ReadingLifecycleInputStatus, progress: ReadingProgress): boolean;
export function shouldPromptCompletion(page: number, pageCount: number | null | undefined, status: ReadingLifecycleInputStatus): boolean;
export function shouldPromptCompletion(
  first: number | ReadingLifecycleInputStatus,
  second: number | null | undefined | ReadingProgress,
  third?: ReadingLifecycleInputStatus,
): boolean {
  const status = typeof first === "number" ? third! : first;
  const progress = typeof first === "number"
    ? readingProgress(first, second as number | null | undefined)
    : second as ReadingProgress;
  return normalizeReadingStatus(status) !== "read" && progress.isAtEnd;
}

const normalizeOffset = (value: unknown): number => clamp(finiteNumber(value, 0), 0, 1);
const normalizePage = (page: unknown, pageCount: number | null | undefined): number => {
  const candidate = positiveInteger(page, DEFAULT_PAGE);
  return pageCount && pageCount >= 1 ? clamp(candidate, DEFAULT_PAGE, Math.trunc(pageCount)) : candidate;
};

/** Prefer same-revision session state, then same-revision engagement, then page one. */
export function resolveResumeTarget(input: {
  savedState?: Pick<ReadingState, "revisionId" | "pageNumber" | "pageOffset"> | null;
  currentRevisionId: string;
  engagement?: Pick<ReadingEngagement, "revisionId" | "furthestPage"> | null;
  pageCount?: number | null;
}): ResumeTarget {
  const pageCount = input.pageCount ?? null;
  const sessionMatches = !!input.savedState && input.savedState.revisionId === input.currentRevisionId;
  const engagementMatches = !!input.engagement && input.engagement.revisionId === input.currentRevisionId;
  const source: ResumeSource = sessionMatches ? "session" : engagementMatches ? "engagement" : "start";
  const rawPage = sessionMatches ? input.savedState?.pageNumber : engagementMatches ? input.engagement?.furthestPage : DEFAULT_PAGE;
  const pageNumber = normalizePage(rawPage, pageCount);
  return {
    pageNumber,
    pageOffset: sessionMatches ? normalizeOffset(input.savedState?.pageOffset) : 0,
    source,
    staleSession: !!input.savedState && !sessionMatches,
    progress: readingProgress(pageNumber, pageCount),
  };
}

export function resumeTargetLabel(target: ResumeTarget): string {
  if (target.source === "session" || target.source === "saved_state") return `Continue on page ${target.pageNumber}`;
  if (target.source === "engagement") return `Resume near page ${target.pageNumber}`;
  return "Start from page 1";
}

/** Record open/activity without overwriting an explicit done or abandoned choice. */
export function recordReadingActivity(input: {
  lifecycle?: ReadingLifecycle | null;
  engagement?: ReadingEngagement | null;
  paperId: string;
  revisionId: string;
  page: number;
  pageCount?: number | null;
  now?: string;
}): { lifecycle: ReadingLifecycle; engagement: ReadingEngagement } {
  const timestamp = isoNow(input.now);
  const existingLifecycle = input.lifecycle ?? defaultReadingLifecycle(input.paperId, timestamp);
  const lifecycle = normalizeReadingStatus(existingLifecycle.status) === "unread"
    ? transitionReadingStatus(existingLifecycle, "reading", timestamp)
    : existingLifecycle;
  const safePage = normalizePage(input.page, input.pageCount);
  const previous = input.engagement?.revisionId === input.revisionId ? input.engagement : null;
  return {
    lifecycle,
    engagement: {
      paperId: input.paperId,
      revisionId: input.revisionId,
      furthestPage: Math.max(previous?.furthestPage ?? DEFAULT_PAGE, safePage),
      pageCountSnapshot: positiveInteger(input.pageCount, previous?.pageCountSnapshot ?? DEFAULT_PAGE),
      firstOpenedAt: previous?.firstOpenedAt ?? timestamp,
      lastOpenedAt: timestamp,
      updatedAt: timestamp,
    },
  };
}

/** Keep transient viewport values in range before persistence. */
export function patchReaderSessionState(
  state: ReadingState,
  patch: {
    pageNumber?: number;
    pageOffset?: number;
    zoom?: number;
    rotation?: ReadingState["rotation"];
    updatedAt?: string;
  },
): ReadingState {
  const pageNumber = positiveInteger(patch.pageNumber, state.pageNumber);
  const pageOffset = normalizeOffset(patch.pageOffset ?? state.pageOffset);
  const zoom = clamp(finiteNumber(patch.zoom, state.zoom), 25, 400);
  const rotation = patch.rotation ?? state.rotation;
  const safeRotation: ReadingState["rotation"] = [0, 90, 180, 270].includes(rotation) ? rotation : 0;
  return { ...state, pageNumber, pageOffset, zoom, rotation: safeRotation, updatedAt: isoNow(patch.updatedAt) };
}

export function lifecycleFromUnknown(value: unknown, paperId: string, now?: string): ReadingLifecycle {
  const raw = (value && typeof value === "object" ? value : {}) as Partial<ReadingLifecycle>;
  const fallback = defaultReadingLifecycle(paperId, now);
  const status = normalizeReadingStatus(raw.status);
  const priority = Number.isInteger(raw.priority) && raw.priority! >= 0 && raw.priority! <= 3
    ? raw.priority as 0 | 1 | 2 | 3
    : fallback.priority;
  return {
    paperId,
    status,
    favorite: raw.favorite === true,
    priority,
    readLater: raw.readLater === true,
    reviewAt: typeof raw.reviewAt === "string" ? raw.reviewAt : null,
    statusChangedAt: typeof raw.statusChangedAt === "string" ? raw.statusChangedAt : fallback.statusChangedAt,
    completedAt: status === "read" && typeof raw.completedAt === "string" ? raw.completedAt : null,
    version: Number.isInteger(raw.version) && raw.version! >= 0 ? raw.version! : 0,
    updatedAt: typeof raw.updatedAt === "string" ? raw.updatedAt : fallback.updatedAt,
  };
}
