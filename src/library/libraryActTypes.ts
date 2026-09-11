import { uiText, zhT, type TranslateFn } from "../i18n/uiText";
// Library Workspace 写侧的线格式合同（ADR D-063 §4.2 / §10.2）。
//
// 字段名、判别 tag 与 `kind` 取值必须与 `src-tauri/src/library_batch.rs` 逐字对齐。
// 禁止 `{ command, payload }` 形状；判别只在 `kind` 上。

import {
  LIBRARY_PROTOCOL_VERSION,
  type DomainRevision,
  type LibraryQueryFilter,
} from "./libraryWorkspaceTypes";

export const LIBRARY_ACT_PROTOCOL_VERSION = LIBRARY_PROTOCOL_VERSION;
export const MAX_BATCH_ITEMS = 500;
export const MAX_IDEMPOTENCY_KEY_CHARS = 64;
export const RECENT_BATCHES_DEFAULT = 50;
export const RECENT_BATCHES_MAX = 100;

export type LibraryActErrorCode =
  | "invalid_query"
  | "workspace_unavailable"
  | "idempotency_conflict"
  | "stale_selection"
  | "stale_batch_plan"
  | "selection_empty"
  | "batch_not_found"
  | "batch_too_large"
  | "paper_not_found"
  | "target_path_conflict"
  | "source_missing"
  | "invalid_source"
  | "source_changed"
  | "confirmation_required"
  | "not_reversible"
  | "undo_expired"
  | "undo_conflict"
  | "unsupported_for_scope"
  | "workspace_busy"
  | "interrupted_unknown"
  | "stale_library_snapshot"
  | "provider_route_unavailable"
  | "cost_confirmation_required"
  | "possible_duplicate_charge";

export type LibraryActError = Readonly<{
  code: LibraryActErrorCode;
  message: string;
}>;

export type BatchState =
  | "planned"
  | "queued"
  | "running"
  | "paused"
  | "action_required"
  | "interrupted_unknown"
  | "completed"
  | "completed_with_errors"
  | "failed"
  | "cancelled";

export type ItemState =
  | "planned"
  | "queued"
  | "running"
  | "paused"
  | "action_required"
  | "interrupted_unknown"
  | "succeeded"
  | "failed"
  | "skipped"
  | "cancelled";

export type UndoPolicy = "none" | "full" | "compensating" | "cancel_only";
export type UndoTokenState = "available" | "consumed" | "expired" | "revoked";

export type BatchCommandKind =
  | "patch_tags"
  | "move"
  | "trash"
  | "import"
  | "export"
  | "patch_lifecycle"
  | "ocr"
  | "brief"
  | "retry"
  | "compensation";

export type BatchRequirementKind =
  | "kind_change"
  | "destructive"
  | "conflict"
  | "overwrite"
  | "cost"
  | "unknown_cost"
  | "long_pdf"
  | "possible_duplicate_charge";

export type CostConfidence = "exact" | "upper_bound" | "estimate";

export type CostEstimate = Readonly<{
  confidence: CostConfidence;
  currency: string;
  minimum: string;
  maximum: string;
  basis: string;
  priceCatalogVersion: string;
}>;

export type CostPreview = Readonly<{
  marginalEstimates: readonly CostEstimate[];
  unknownItemCount: number;
  unknownReasonCodes: readonly string[];
  joinedExistingJobCount: number;
}>;

export const EMPTY_COST_PREVIEW: CostPreview = {
  marginalEstimates: [],
  unknownItemCount: 0,
  unknownReasonCodes: [],
  joinedExistingJobCount: 0,
};

export type ItemCounts = Readonly<{
  planned: number;
  queued: number;
  running: number;
  paused: number;
  actionRequired: number;
  interruptedUnknown: number;
  succeeded: number;
  failed: number;
  skipped: number;
  cancelled: number;
}>;

export type BatchRequirement = Readonly<{
  id: string;
  kind: BatchRequirementKind;
  itemCount: number;
  label: string;
}>;

export type ChildSummary = Readonly<{
  batchId: string;
  state: BatchState;
  totalItems: number;
  succeeded: number;
  failed: number;
  createdAt: string;
}>;

export type UndoSummary = Readonly<{
  state: UndoTokenState;
  expiresAt: string;
}>;

export type BatchProjection = Readonly<{
  protocolVersion: number;
  id: string;
  commandKind: BatchCommandKind;
  parentCommandKind: BatchCommandKind | null;
  parentBatchId: string | null;
  relation: string | null;
  state: BatchState;
  planDigest: string;
  targetDigest: string;
  undoPolicy: UndoPolicy;
  totalItems: number;
  counts: ItemCounts;
  requirements: readonly BatchRequirement[];
  isCancelling: boolean;
  planExpiresAt: string | null;
  createdAt: string;
  startedAt: string | null;
  finishedAt: string | null;
  updatedAt: string;
  retrySummary: ChildSummary | null;
  compensationSummary: ChildSummary | null;
  undo: UndoSummary | null;
  costPreview: CostPreview;
}>;

export type BatchItemProjection = Readonly<{
  id: string;
  ordinal: number;
  targetKey: string;
  paperId: string | null;
  revisionId: string | null;
  state: ItemState;
  outcome: string | null;
  attemptCount: number;
  errorCode: LibraryActErrorCode | null;
  errorSummary: string | null;
  retryable: boolean;
  startedAt: string | null;
  finishedAt: string | null;
  updatedAt: string;
  sourceItemId: string | null;
}>;

export type BatchItemsPage = Readonly<{
  protocolVersion: number;
  batchId: string;
  pageSize: number;
  items: readonly BatchItemProjection[];
  nextOrdinal: number | null;
  hasMore: boolean;
}>;

export type RecentBatchesPage = Readonly<{
  protocolVersion: number;
  pageSize: number;
  batches: readonly BatchProjection[];
}>;

export type ExportFormat = "reading_bundle";

export type LifecyclePatch = Readonly<{
  status?: "unread" | "reading" | "read";
  favorite?: boolean;
  priority?: number;
  readLater?: boolean;
  reviewAt?: string | null;
}>;

export type BatchCommand =
  | Readonly<{ kind: "patch_tags"; add: readonly string[]; remove: readonly string[] }>
  | Readonly<{ kind: "move"; collectionPath: string }>
  | Readonly<{ kind: "trash" }>
  | Readonly<{ kind: "import"; collectionPath: string }>
  | Readonly<{ kind: "export"; format: ExportFormat }>
  | Readonly<{ kind: "patch_lifecycle"; patch: LifecyclePatch }>
  | Readonly<{ kind: "ocr" }>
  | Readonly<{ kind: "brief" }>;

export type BatchTarget =
  | Readonly<{ kind: "explicit"; paperIds: readonly string[] }>
  | Readonly<{
      kind: "query";
      filters: readonly LibraryQueryFilter[];
      excludedIds: readonly string[];
      selectionDigest: string;
      dependencyRevisions: readonly DomainRevision[];
      evaluationAnchor?: string | null;
      evaluationTimezone?: string | null;
    }>
  | Readonly<{ kind: "sources"; paths: readonly string[] }>;

export type BatchControl =
  | Readonly<{ kind: "cancel_remaining" }>
  | Readonly<{ kind: "retry_failed"; itemIds: readonly string[] | null }>
  | Readonly<{ kind: "undo"; token: string }>;

export type LibraryActRequest =
  | Readonly<{
      kind: "plan_batch";
      protocolVersion: number;
      idempotencyKey: string;
      command: BatchCommand;
      target: BatchTarget;
    }>
  | Readonly<{
      kind: "start_batch";
      protocolVersion: number;
      idempotencyKey: string;
      batchId: string;
      planDigest: string;
      acceptedRequirementIds: readonly string[];
      maximumAcceptedEstimateByCurrency: Readonly<Record<string, string>>;
    }>
  | Readonly<{
      kind: "control_batch";
      protocolVersion: number;
      idempotencyKey: string;
      batchId: string;
      control: BatchControl;
    }>
  | Readonly<{
      kind: "change";
      protocolVersion: number;
      idempotencyKey: string;
      change: LibraryChange;
    }>
  | Readonly<{
      kind: "record_reader_activity";
      protocolVersion: number;
      idempotencyKey: string;
      activity: ReaderActivity;
    }>;

export type ReaderActivity = Readonly<{
  paperId: string;
  revisionId: string;
  pageNumber: number;
  pageCount: number;
}>;

export type LibraryChange =
  | Readonly<{
      kind: "patch_lifecycle";
      paperId: string;
      expectedVersion: number;
      patch: LifecyclePatch;
    }>
  | Readonly<{ kind: "create_smart_collection"; name: string; filters: readonly LibraryQueryFilter[] }>
  | Readonly<{ kind: "rename_smart_collection"; id: string; name: string }>
  | Readonly<{ kind: "delete_smart_collection"; id: string }>;

export type LibraryActResponse =
  | Readonly<{ kind: "plan_batch"; batch: BatchProjection }>
  | Readonly<{ kind: "start_batch"; batch: BatchProjection; undoToken: string | null }>
  | Readonly<{ kind: "control_batch"; batch: BatchProjection; undoToken: string | null }>
  | Readonly<{ kind: "change"; result: ChangeResult }>
  | Readonly<{
      kind: "record_reader_activity";
      context: import("./libraryWorkspaceTypes").ReadingContextProjection;
    }>;

export type ChangeResult =
  | Readonly<{
      kind: "lifecycle";
      lifecycle: import("../readerReadingState").ReadingLifecycle;
    }>
  | Readonly<{
      kind: "smart_collection";
      collection: import("./libraryWorkspaceTypes").SmartCollectionProjection;
    }>
  | Readonly<{ kind: "smart_collection_deleted"; id: string }>;

const ACT_ERROR_CODES: readonly LibraryActErrorCode[] = [
  "invalid_query",
  "workspace_unavailable",
  "idempotency_conflict",
  "stale_selection",
  "stale_batch_plan",
  "selection_empty",
  "batch_not_found",
  "batch_too_large",
  "paper_not_found",
  "target_path_conflict",
  "source_missing",
  "invalid_source",
  "source_changed",
  "confirmation_required",
  "not_reversible",
  "undo_expired",
  "undo_conflict",
  "unsupported_for_scope",
  "workspace_busy",
  "interrupted_unknown",
  "stale_library_snapshot",
  "provider_route_unavailable",
  "cost_confirmation_required",
  "possible_duplicate_charge",
];

export function isLibraryActError(value: unknown): value is LibraryActError {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as { code?: unknown; message?: unknown };
  return (
    typeof candidate.code === "string" &&
    ACT_ERROR_CODES.includes(candidate.code as LibraryActErrorCode) &&
    typeof candidate.message === "string"
  );
}

export function toLibraryActError(value: unknown): LibraryActError {
  if (isLibraryActError(value)) return value;
  const message =
    typeof value === "string" ? value : value instanceof Error ? value.message : "unknown error";
  return { code: "workspace_unavailable", message };
}

export function newIdempotencyKey(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `batch-${Date.now().toString(16)}-${Math.random().toString(16).slice(2, 10)}`;
}

export function planBatchRequest(
  command: BatchCommand,
  target: BatchTarget,
  idempotencyKey = newIdempotencyKey(),
): LibraryActRequest {
  return {
    kind: "plan_batch",
    protocolVersion: LIBRARY_ACT_PROTOCOL_VERSION,
    idempotencyKey,
    command,
    target,
  };
}

export function startBatchRequest(
  batch: BatchProjection,
  acceptedRequirementIds: readonly string[] = batch.requirements.map((item) => item.id),
  idempotencyKey = newIdempotencyKey(),
): LibraryActRequest {
  return {
    kind: "start_batch",
    protocolVersion: LIBRARY_ACT_PROTOCOL_VERSION,
    idempotencyKey,
    batchId: batch.id,
    planDigest: batch.planDigest,
    acceptedRequirementIds,
    maximumAcceptedEstimateByCurrency: ceilingsFromCostPreview(batch.costPreview),
  };
}

export function ceilingsFromCostPreview(preview: CostPreview | null | undefined): Record<string, string> {
  const out: Record<string, string> = {};
  for (const estimate of preview?.marginalEstimates ?? []) {
    const current = out[estimate.currency];
    if (current === undefined || Number(estimate.maximum) > Number(current)) {
      out[estimate.currency] = estimate.maximum;
    }
  }
  return out;
}

export function controlBatchRequest(
  batchId: string,
  control: BatchControl,
  idempotencyKey = newIdempotencyKey(),
): LibraryActRequest {
  return {
    kind: "control_batch",
    protocolVersion: LIBRARY_ACT_PROTOCOL_VERSION,
    idempotencyKey,
    batchId,
    control,
  };
}

export function batchOf(response: LibraryActResponse): BatchProjection {
  if (response.kind === "change" || response.kind === "record_reader_activity") {
    throw new Error("this act response is not a batch");
  }
  return response.batch;
}

export function undoTokenOf(response: LibraryActResponse): string | null {
  return response.kind === "start_batch" || response.kind === "control_batch"
    ? response.undoToken
    : null;
}

export function changeRequest(
  change: LibraryChange,
  idempotencyKey = newIdempotencyKey(),
): LibraryActRequest {
  return {
    kind: "change",
    protocolVersion: LIBRARY_ACT_PROTOCOL_VERSION,
    idempotencyKey,
    change,
  };
}

export function recordReaderActivityRequest(
  activity: ReaderActivity,
  idempotencyKey = newIdempotencyKey(),
): LibraryActRequest {
  return {
    kind: "record_reader_activity",
    protocolVersion: LIBRARY_ACT_PROTOCOL_VERSION,
    idempotencyKey,
    activity,
  };
}

export type LibraryActRun =
  | Readonly<{ status: "needs_confirmation"; plan: BatchProjection }>
  | Readonly<{ status: "done"; plan: BatchProjection; result: LibraryActResponse }>;

export async function planAndStart(
  act: (request: LibraryActRequest) => Promise<LibraryActResponse>,
  command: BatchCommand,
  target: BatchTarget,
  options: { acceptRequirements?: boolean; acceptedIds?: readonly string[] } = {},
): Promise<LibraryActRun> {
  const planned = await act(planBatchRequest(command, target));
  const plan = batchOf(planned);
  const accepted = options.acceptedIds ?? (options.acceptRequirements ? plan.requirements.map((item) => item.id) : []);
  if (plan.requirements.length > 0 && accepted.length === 0) {
    return { status: "needs_confirmation", plan };
  }
  const result = await act(startBatchRequest(plan, accepted));
  return { status: "done", plan, result };
}

export function summarizeBatch(batch: BatchProjection, t: TranslateFn = zhT): string {
  const { counts, totalItems, commandKind } = batch;
  const verb =
    commandKind === "patch_tags"
      ? "已更新标签"
      : commandKind === "move"
        ? "已移动"
        : commandKind === "trash"
          ? "已移入回收站"
          : commandKind === "import"
            ? "已导入"
            : commandKind === "export"
              ? "已导出"
              : commandKind === "retry"
                ? "已重试"
                : commandKind === "patch_lifecycle"
                  ? "已更新阅读状态"
                  : commandKind === "ocr"
                    ? "已排队 OCR"
                    : commandKind === "brief"
                      ? "已排队 Brief"
                      : "已撤销";
  const localizedVerb = uiText(t, verb);
  const failed = counts.failed + counts.cancelled;
  if (failed === 0 && counts.skipped === 0) return uiText(t, "{verb} {count} documents", { verb: localizedVerb, count: counts.succeeded });
  if (failed === 0) return uiText(t, "{verb} {count} documents; {skipped} skipped", { verb: localizedVerb, count: counts.succeeded, skipped: counts.skipped });
  return uiText(t, "{verb} {count} documents; {failed} failed ({total} items)", { verb: localizedVerb, count: counts.succeeded, failed: counts.failed, total: totalItems });
}
