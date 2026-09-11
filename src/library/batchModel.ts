/** Durable, side-effect-free contract for Library Hub batch operations. */
export type SelectionScope = "explicit" | "all_matching";
export type SelectionSnapshot = Readonly<{
  ids: readonly string[];
  scope: SelectionScope;
  queryDigest: string | null;
  capturedRevision: number;
  capturedAt: string;
}>;
export type BulkAction =
  | Readonly<{ kind: "move"; targetCollection: string; anchorId?: string | null; place?: "before" | "after" | "end" }>
  | Readonly<{ kind: "tag_add" | "tag_remove"; tags: readonly string[] }>
  | Readonly<{ kind: "lifecycle"; status: "unread" | "reading" | "read" }>
  | Readonly<{ kind: "trash" | "restore" }>
  | Readonly<{ kind: "export" | "ocr" | "brief" }>;
export type BulkOperationRequest = Readonly<{
  idempotencyKey: string;
  selection: SelectionSnapshot;
  expectedRevision: number;
  action: BulkAction;
}>;
export type BulkItemStatus = "applied" | "skipped" | "failed" | "conflict";
export type BulkItemOutcome = Readonly<{ id: string; status: BulkItemStatus; reason?: string }>;
export type UndoToken = Readonly<{
  id: string;
  operationId: string;
  createdAt: string;
  expiresAt: string | null;
  expectedRevision: number;
}>;
export type BulkOperationResult = Readonly<{
  operationId: string;
  idempotencyKey: string;
  revision: number;
  outcomes: readonly BulkItemOutcome[];
  undoToken: UndoToken | null;
  replayed: boolean;
}>;
/** Client-facing projection for the durable Batch workflow. */
export type BulkBatchState =
  | "planned"
  | "queued"
  | "running"
  | "completed"
  | "completed_with_errors"
  | "cancelled"
  | "failed";
export type BulkBatchPlan = Readonly<{
  batchId: string;
  request: BulkOperationRequest;
  planDigest: string;
  state: "planned";
  estimatedItems: number;
  requiresConfirmation: boolean;
  createdAt: string;
  expiresAt: string | null;
}>;
export type BulkBatchProjection = Readonly<{
  batchId: string;
  request: BulkOperationRequest;
  planDigest: string;
  state: BulkBatchState;
  revision: number;
  outcomes: readonly BulkItemOutcome[];
  undoToken: UndoToken | null;
  createdAt: string;
  updatedAt: string;
  replayed?: boolean;
  error?: string;
}>;
export type BulkControl = "cancel" | "retry_failed" | "cancel_remaining";
export type UndoRecord = Readonly<{
  token: UndoToken;
  inverse: BulkOperationRequest;
  state: "available" | "used" | "expired" | "conflict";
}>;
export type SnapshotValidationContext = Readonly<{
  currentRevision: number;
  currentQueryDigest?: string | null;
  availableIds: ReadonlySet<string>;
}>;
export type SnapshotValidationCode = "empty_selection" | "invalid_selection" | "stale_revision" | "stale_query";
export class BulkOperationError extends Error {
  readonly code: SnapshotValidationCode;
  constructor(code: SnapshotValidationCode, message: string) {
    super(message);
    this.name = "BulkOperationError";
    this.code = code;
  }
}
export function queryDigest(value: unknown): string {
  const canonical = canonicalJson(value);
  let hash = 2166136261;
  for (let index = 0; index < canonical.length; index += 1) {
    hash ^= canonical.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return `fnv1a-${(hash >>> 0).toString(16).padStart(8, "0")}`;
}
function canonicalJson(value: unknown): string {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  const object = value as Record<string, unknown>;
  return `{${Object.keys(object).sort().map((key) => `${JSON.stringify(key)}:${canonicalJson(object[key])}`).join(",")}}`;
}
export function createSelectionSnapshot(input: {
  ids: readonly string[];
  scope?: SelectionScope;
  queryDigest?: string | null;
  capturedRevision: number;
  capturedAt?: string;
}): SelectionSnapshot {
  const ids = [...new Set(input.ids.filter((id) => typeof id === "string" && id.trim().length > 0))];
  if (!ids.length) throw new BulkOperationError("empty_selection", "selection is empty");
  if (!Number.isSafeInteger(input.capturedRevision) || input.capturedRevision < 0) {
    throw new BulkOperationError("invalid_selection", "selection revision is invalid");
  }
  return Object.freeze({
    ids: Object.freeze(ids),
    scope: input.scope ?? "explicit",
    queryDigest: input.queryDigest ?? null,
    capturedRevision: input.capturedRevision,
    capturedAt: input.capturedAt ?? new Date().toISOString(),
  });
}
export function validateSelectionSnapshot(snapshot: SelectionSnapshot, context: SnapshotValidationContext): void {
  if (!snapshot.ids.length) throw new BulkOperationError("empty_selection", "selection is empty");
  if (snapshot.capturedRevision !== context.currentRevision) throw new BulkOperationError("stale_revision", "library changed");
  if (snapshot.scope === "all_matching" && snapshot.queryDigest !== context.currentQueryDigest) throw new BulkOperationError("stale_query", "query changed");
  if (snapshot.scope === "explicit" && snapshot.ids.some((id) => !context.availableIds.has(id))) {
    throw new BulkOperationError("invalid_selection", "some selected items no longer exist");
  }
}
export function normalizeBulkRequest(request: BulkOperationRequest): BulkOperationRequest {
  const key = request.idempotencyKey.trim();
  if (!key || key.length > 160) throw new BulkOperationError("invalid_selection", "idempotency key is invalid");
  return Object.freeze({
    ...request,
    idempotencyKey: key,
    selection: Object.freeze({ ...request.selection, ids: Object.freeze([...request.selection.ids]) }),
  });
}
export function summarizeOutcomes(outcomes: readonly BulkItemOutcome[]) {
  return outcomes.reduce((summary, outcome) => {
    summary.total += 1;
    summary[outcome.status] += 1;
    return summary;
  }, { total: 0, applied: 0, skipped: 0, failed: 0, conflict: 0 });
}
export function serializeUndoRecord(record: UndoRecord): string { return JSON.stringify(record); }
export function parseUndoRecord(raw: string): UndoRecord | null {
  try {
    const value = JSON.parse(raw) as UndoRecord;
    if (!value || !value.token || !value.inverse || !value.state || !Array.isArray(value.inverse.selection?.ids)) return null;
    return value;
  } catch { return null; }
}
