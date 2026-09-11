import { describe, expect, it } from "vitest";
import {
  defaultReadingLifecycle,
  lifecycleFromUnknown,
  readingProgress,
  recordReadingActivity,
  resolveResumeTarget,
  shouldPromptCompletion,
  transitionReadingStatus,
} from "./readerReadingState";

describe("reader reading state", () => {
  it("normalizes progress and completion threshold", () => {
    expect(readingProgress(99, 10)).toMatchObject({ page: 10, percent: 100, isAtEnd: true });
    expect(readingProgress(Number.NaN, 0)).toMatchObject({ page: 1, pageCount: null, isAtEnd: false });
    expect(shouldPromptCompletion("reading", readingProgress(19, 20))).toBe(false);
    expect(shouldPromptCompletion("reading", readingProgress(20, 20))).toBe(true);
    expect(shouldPromptCompletion("read", readingProgress(20, 20))).toBe(false);
  });

  it("prefers same-revision session, then engagement, and marks stale sessions", () => {
    expect(resolveResumeTarget({
      savedState: { revisionId: "r1", pageNumber: 4, pageOffset: 12 },
      currentRevisionId: "r1",
      engagement: { revisionId: "r1", furthestPage: 8 },
      pageCount: 10,
    })).toMatchObject({ source: "session", pageNumber: 4, staleSession: false });
    expect(resolveResumeTarget({
      savedState: { revisionId: "old", pageNumber: 9, pageOffset: 0 },
      currentRevisionId: "r1",
      engagement: { revisionId: "r1", furthestPage: 8 },
      pageCount: 10,
    })).toMatchObject({ source: "engagement", pageNumber: 8, staleSession: true });
  });

  it("records first open, monotonic furthest page, and unread to reading", () => {
    const initial = defaultReadingLifecycle("p1", "2026-08-31T00:00:00Z");
    const first = recordReadingActivity({
      lifecycle: initial, paperId: "p1", revisionId: "r1", page: 3, pageCount: 10, now: "2026-08-31T00:01:00Z",
    });
    expect(first.lifecycle.status).toBe("reading");
    expect(first.engagement.furthestPage).toBe(3);
    const later = recordReadingActivity({
      lifecycle: first.lifecycle, engagement: first.engagement, paperId: "p1", revisionId: "r1", page: 2, pageCount: 10, now: "2026-08-31T00:02:00Z",
    });
    expect(later.engagement.furthestPage).toBe(3);
  });

  it("keeps read completion timestamp and increments version on transitions", () => {
    const initial = defaultReadingLifecycle("p1", "2026-08-31T00:00:00Z");
    const read = transitionReadingStatus(initial, "read", "2026-08-31T01:00:00Z");
    expect(read.completedAt).toBe("2026-08-31T01:00:00Z");
    expect(read.version).toBe(1);
    expect(transitionReadingStatus(read, "done", "2026-08-31T02:00:00Z")).toBe(read);
  });

  it("tolerates legacy and malformed lifecycle projections", () => {
    expect(lifecycleFromUnknown({ status: "done", priority: 99 }, "p1", "now")).toMatchObject({
      status: "read", priority: 0, paperId: "p1", updatedAt: "now",
    });
    expect(lifecycleFromUnknown(null, "p2", "now").status).toBe("unread");
  });
});
