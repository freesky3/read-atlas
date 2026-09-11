import { describe, expect, it } from "vitest";
import {
  BulkOperationError,
  createSelectionSnapshot,
  normalizeBulkRequest,
  parseUndoRecord,
  queryDigest,
  serializeUndoRecord,
  summarizeOutcomes,
  validateSelectionSnapshot,
  type UndoRecord,
} from "./batchModel";

describe("batch operation contract", () => {
  it("freezes and de-duplicates immutable selection ids", () => {
    const snapshot = createSelectionSnapshot({
      ids: ["paper-2", "paper-1", "paper-2", " "],
      capturedRevision: 7,
      capturedAt: "2026-08-31T00:00:00Z",
    });
    expect(snapshot.ids).toEqual(["paper-2", "paper-1"]);
    expect(Object.isFrozen(snapshot)).toBe(true);
    expect(Object.isFrozen(snapshot.ids)).toBe(true);
  });

  it("rejects stale revisions and queries before an action can run", () => {
    const snapshot = createSelectionSnapshot({
      ids: ["paper-1"],
      scope: "all_matching",
      queryDigest: queryDigest({ text: "bayes", sort: "recent" }),
      capturedRevision: 3,
    });
    expect(() => validateSelectionSnapshot(snapshot, {
      currentRevision: 4,
      currentQueryDigest: snapshot.queryDigest,
      availableIds: new Set(["paper-1"]),
    })).toThrowError(expect.objectContaining({ code: "stale_revision" }));
    expect(() => validateSelectionSnapshot(snapshot, {
      currentRevision: 3,
      currentQueryDigest: queryDigest({ text: "different" }),
      availableIds: new Set(["paper-1"]),
    })).toThrowError(expect.objectContaining({ code: "stale_query" }));
  });

  it("rejects an explicit snapshot containing a deleted id", () => {
    const snapshot = createSelectionSnapshot({ ids: ["paper-1", "paper-2"], capturedRevision: 1 });
    expect(() => validateSelectionSnapshot(snapshot, {
      currentRevision: 1,
      availableIds: new Set(["paper-1"]),
    })).toThrowError(expect.objectContaining({ code: "invalid_selection" }));
  });

  it("normalizes idempotency keys and keeps request data immutable", () => {
    const selection = createSelectionSnapshot({ ids: ["paper-1"], capturedRevision: 1 });
    const request = normalizeBulkRequest({
      idempotencyKey: "  batch-1  ",
      selection,
      expectedRevision: 1,
      action: { kind: "trash" },
    });
    expect(request.idempotencyKey).toBe("batch-1");
    expect(Object.isFrozen(request.selection.ids)).toBe(true);
    expect(() => normalizeBulkRequest({
      idempotencyKey: " ",
      selection,
      expectedRevision: 1,
      action: { kind: "trash" },
    })).toThrow(BulkOperationError);
  });

  it("round-trips undo records and summarizes partial outcomes", () => {
    const selection = createSelectionSnapshot({ ids: ["a", "b"], capturedRevision: 2 });
    const record: UndoRecord = {
      token: {
        id: "undo-1",
        operationId: "op-1",
        createdAt: "2026-08-31T00:00:00Z",
        expiresAt: null,
        expectedRevision: 3,
      },
      inverse: {
        idempotencyKey: "undo:op-1",
        selection,
        expectedRevision: 3,
        action: { kind: "restore" },
      },
      state: "available",
    };
    expect(parseUndoRecord(serializeUndoRecord(record))).toEqual(record);
    expect(parseUndoRecord("{}")).toBeNull();
    expect(summarizeOutcomes([
      { id: "a", status: "applied" },
      { id: "b", status: "failed", reason: "locked" },
      { id: "c", status: "conflict" },
    ])).toEqual({ total: 3, applied: 1, skipped: 0, failed: 1, conflict: 1 });
  });
});
