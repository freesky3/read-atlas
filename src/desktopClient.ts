import { captureOperationError } from "./i18n/errors";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { factoryPromptSettings } from "./promptCatalog";
import type {
  AppStats,
  ArtifactProjection,
  Brief,
  ConnectionTestResult,
  CollectionProjection,
  DocumentCard,
  JobProjection,
  LensQaProjection,
  Message,
  ModelSettings,
  GuideProjection,
  GuideCharacterSettings,
  OutlineProjection,
  PromptSettings,
  UiLocaleProjection,
  PromptSlotId,
  ProviderInstanceView,
  ProviderKind,
  OcrProjection,
  RemoteTombstoneProjection,
  ReadingContextProjection,
  ReadingEngagementProjection,
  ReadingLifecycleProjection,
  RecentReadingProjection,
  RecordReadingActivityRequest,
  UpdateReadingLifecycleRequest,
  ReaderContextProjection,
  ReaderContextScope,
  ReadingState,
  StorageReport,
  Thread,
  TrashProjection,
  UserAnnotation,
  UserAnnotationLink,
  UserAnnotationLinkInput,
  UserAnnotationInput,
  UserAnnotationUpdateRequest,
  WorkspaceInfo,
} from "./types";
import {
  defaultReadingLifecycle,
  recordReadingActivity,
  transitionReadingStatus,
  type ReadingEngagement,
  type ReadingLifecycle,
} from "./readerReadingState";
import {
  queryDigest,
  summarizeOutcomes,
  type BulkAction,
  type BulkBatchPlan,
  type BulkBatchProjection,
  type BulkBatchState,
  type BulkControl,
  type BulkOperationRequest,
  type BulkOperationResult,
  type UndoRecord,
  type UndoToken,
} from "./library/batchModel";

export type ProjectionName =
  | "get_workspace"
  | "get_model_settings"
  | "get_prompt_settings"
  | "get_ui_locale"
  | "list_documents"
  | "list_collections"
  | "open_library"
  | "list_threads"
  | "list_archived_threads"
  | "get_reading_state"
  | "get_reading_context"
  | "get_reading_lifecycle"
  | "list_recent_reading"
  | "list_messages"
  | "get_brief"
  | "get_outline"
  | "get_outline_deep_dive"
  | "get_reading_guide"
  | "get_guide_character_settings"
  | "get_stats"
  | "get_storage_report"
  | "list_trash"
  | "list_remote_tombstones"
  | "list_artifacts"
  | "get_artifact"
  | "get_artifact_asset_path"
  | "latest_ocr"
  | "get_ocr"
  | "list_lens_qa"
  | "list_jobs"
  | "preview_diagnostics"
  | "get_roadmap"
  | "list_roadmap_progress"
  | "list_annotations"
  | "list_annotation_links";

export type CommandName =
  | "open_api_key_page"
  | "choose_workspace"
  | "add_provider"
  | "remove_provider"
  | "rename_provider"
  | "duplicate_provider"
  | "reorder_providers"
  | "test_provider_connection"
  | "save_provider_settings"
  | "clear_provider_credential"
  | "set_current_provider"
  | "save_prompt_slot"
  | "restore_prompt_previous"
  | "restore_prompt_default"
  | "restore_outline_prompt_bundle"
  | "get_reader_context"
  | "save_reader_context"
  | "restore_reader_folder_context"
  | "save_mistral_credential"
  | "clear_mistral_credential"
  | "list_collections"
  | "create_collection"
  | "rename_collection"
  | "move_collection"
  | "trash_collection"
  | "reorder_collection_papers"
  | "set_collection_sort_mode"
  | "rename_paper"
  | "import_pdf"
  | "move_paper"
  | "export_reading_bundle"
  | "reconcile_library"
  | "delete_revision"
  | "restore_paper"
  | "retry_remote_cleanup"
  | "abandon_remote_cleanup"
  | "recheck_provider_job"
  | "rebind_provider_job"
  | "abandon_legacy_provider_job"
  | "save_reading_state"
  | "record_reading_activity"
  | "record_reader_activity"
  | "update_reading_lifecycle"
  | "patch_reading_lifecycle"
  | "update_orientation_table"
  | "set_artifact_override"
  | "delete_artifact"
  | "generate_reading_artifact"
  | "ask_lens"
  | "start_ocr"
  | "ack_long_pdf_warning"
  | "pause_job"
  | "resume_job"
  | "reprioritize_job"
  | "cancel_job"
  | "resolve_exit_intent"
  | "export_diagnostics"
  | "inspect_legacy_reset"
  | "execute_legacy_reset"
  | "retry_job"
  | "create_thread"
  | "send_chat"
  | "cancel_generation"
  | "create_branch"
  | "set_active_branch"
  | "delete_discussion_turn"
  | "archive_thread"
  | "close_thread"
  | "restore_thread"
  | "delete_thread"
  | "rename_thread"
  | "generate_brief"
  | "generate_document_artifact"
  | "start_roadmap_job"
  | "toggle_roadmap_task"
  | "plan_outline"
  | "plan_outline_deep_dive"
  | "start_outline"
  | "start_outline_deep_dive"
  | "delete_outline"
  | "delete_outline_deep_dive"
  | "delete_reading_guide"
  | "plan_reading_guide"
  | "start_reading_guide"
  | "save_guide_character"
  | "delete_guide_character"
  | "restore_guide_character_preset"
  | "duplicate_guide_character"
  | "save_guide_default_cast"
  | "save_guide_preset_casts"
  | "restore_guide_factory_preset_casts"
  | "import_guide_character_avatar"
  | "get_guide_character_avatar"
  | "preview_guide_character"
  | "cancel_guide_character_preview"
  | "open_resource_dir"
  | "open_pdf_external"
  | "open_workspace_dir"
  | "delete_ocr_cascade"
  | "update_paper_tags"
  | "update_paper_metadata"
  | "set_window_theme"
  | "set_ui_locale"
  | "create_annotation"
  | "update_annotation"
  | "delete_annotation"
  | "create_annotation_link"
  | "delete_annotation_link"
  | "plan_library_batch"
  | "start_library_batch"
  | "get_library_batch"
  | "control_library_batch"
  | "undo_library_batch";

export type ReadEvent = {
  cursor: string;
  kind: "library" | "paper" | "job" | "generation" | "lifecycle";
  entityId: string | null;
  delta?: string;
  status?:
    | "streaming"
    | "cancelling"
    | "complete"
    | "cancelled"
    | "failed"
    | "exit_choice_required";
  error?: string;
};

export interface DesktopClient {
  readonly runtime: "desktop" | "memory";
  open<T>(
    projection: ProjectionName,
    args?: Record<string, unknown>,
  ): Promise<T>;
  command<T>(command: CommandName, args?: Record<string, unknown>): Promise<T>;
  watch(listener: (event: ReadEvent) => void): Promise<UnlistenFn>;
}

/** A small adapter boundary keeps Hub UI independent from Tauri IPC details. */
export type LibraryBatchRunner = (
  request: BulkOperationRequest,
) => Promise<BulkOperationResult>;

export async function runLibraryBatch(
  client: DesktopClient,
  request: BulkOperationRequest,
): Promise<BulkOperationResult> {
  const plan = await client.command<BulkBatchPlan>("plan_library_batch", {
    request,
  });
  const projection = await client.command<BulkBatchProjection>("start_library_batch", {
    request: {
      batchId: plan.batchId,
      planDigest: plan.planDigest,
      approvals: {},
    },
    // Keep the flat form for older desktop builds while the nested request is
    // the canonical shape used by the current workflow.
    batchId: plan.batchId,
    planDigest: plan.planDigest,
  });
  return {
    operationId: projection.batchId,
    idempotencyKey: request.idempotencyKey,
    revision: projection.revision,
    outcomes: projection.outcomes,
    undoToken: projection.undoToken,
    replayed: projection.replayed ?? false,
  };
}

export function createLibraryBatchRunner(client: DesktopClient): LibraryBatchRunner {
  return (request) => runLibraryBatch(client, request);
}

class TauriDesktopClient implements DesktopClient {
  readonly runtime = "desktop" as const;

  open<T>(projection: ProjectionName, args?: Record<string, unknown>) {
    return invoke<T>(projection, args).catch(error => { throw projection === "get_ui_locale" ? error : captureOperationError(projection, error); });
  }

  command<T>(command: CommandName, args?: Record<string, unknown>) {
    return invoke<T>(command, args).catch(error => { throw command === "set_ui_locale" ? error : captureOperationError(command, error); });
  }

  async watch(listener: (event: ReadEvent) => void) {
    return listen<ReadEvent>("read-event", ({ payload }) => listener(payload));
  }
}

const previewModels = [
  {
    id: "gemini-2.5-flash",
    displayName: "Gemini 2.5 Flash",
    description: "Browser preview catalog entry. No provider request or paper probe is made.",
    inputTokenLimit: 1_048_576,
    outputTokenLimit: 65_536,
    supportsGenerateContent: true,
    supportsNativePdf: false,
    supportsInteractions: false,
  },
];

function requestArg<T>(args?: Record<string, unknown>) {
  return args?.request as T | undefined;
}

const emptyModelSettings: ModelSettings = {
  currentProviderId: null,
  providers: [],
  credentialStore: "Unavailable in browser preview",
  mistralCredentialConfigured: false,
  mistralCredentialStore: "Unavailable in browser preview",
  ocrModel: "mistral-ocr-latest",
};

function clonePromptSettings(value: PromptSettings): PromptSettings {
  return JSON.parse(JSON.stringify(value)) as PromptSettings;
}

// Browser-preview Hub state. The memory adapter must persist the same
// shape as the desktop runtime so UI development can validate the
// reorder / set-sort-mode commands without silently swallowing writes.
interface MemoryCollectionState {
  id: string;
  relativePath: string;
  parentId: string | null;
  name: string;
  sortMode: "recent" | "year" | "title" | "manual" | "chapter";
  paperOrder: string[];
}

export type MemoryDesktopSeed = Readonly<{
  documents?: readonly DocumentCard[];
  collections?: readonly CollectionProjection[];
  /** IDs that should produce a deterministic failed item in batch tests. */
  batchFailureIds?: readonly string[];
}>;
function factoryGuideCharacters(): GuideCharacterSettings {
  const stamp = "2026-09-09T00:00:00Z";
  const make = (
    id: string,
    displayName: string,
    inkColor: string,
    description: string,
    presetId: string,
  ) => ({
    id,
    revision: 1,
    displayName,
    workTitle: "",
    characterVersion: "",
    description,
    avatarAssetId: null,
    inkColor,
    personality: "",
    readingHabits: "",
    expressionStyle: "",
    avoidances: "",
    exampleNotes: [] as string[],
    presetId,
    presetVersion: 1,
    createdAt: stamp,
    updatedAt: stamp,
  });
  const characters = [
    make("preset:chitanda", "千反田爱瑠", "#7653A6", "真诚、细致、好奇", "chitanda"),
    make("preset:oreki", "折木奉太郎", "#64723A", "节能、敏锐、简洁", "oreki"),
    make("preset:frieren", "芙莉莲", "#287C78", "平静、淡然", "frieren"),
    make("preset:jotaro", "空条承太郎", "#35538A", "冷静、寡言", "jotaro"),
    make("preset:conan", "江户川柯南", "#A14F3C", "重视线索与证据", "conan"),
  ];
  return {
    schemaVersion: 1,
    storeRevision: 1,
    characters,
    defaultCharacterIds: ["preset:chitanda", "preset:oreki", "preset:frieren"],
    presetCasts: [
      { id: "daily", name: "日常阅读", characterIds: ["preset:chitanda", "preset:oreki", "preset:frieren"] },
      { id: "evidence", name: "实验与证据", characterIds: ["preset:chitanda", "preset:conan", "preset:jotaro"] },
      { id: "classics", name: "古典部双人", characterIds: ["preset:chitanda", "preset:oreki"] },
    ],
  };
}


export function createMemoryDesktopClient(seed: MemoryDesktopSeed = {}): DesktopClient {
  let settings = emptyModelSettings;
  const promptsByLocale: Record<"zh-CN" | "en", PromptSettings> = {
    "zh-CN": factoryPromptSettings(),
    en: factoryPromptSettings(),
  };
  let uiLocale: "zh-CN" | "en" | null = "zh-CN";
  const promptLocale = (): "zh-CN" | "en" => uiLocale ?? "zh-CN";
  const outlinePreviousProtocols = new Map<string, string>();
  let characterSettings = factoryGuideCharacters();
  const avatarAssets = new Map<string, { mime: string; dataUrl: string }>();
  const annotations = new Map<string, UserAnnotation[]>();
  const annotationLinks = new Map<string, UserAnnotationLink>();
  const lifecycle = new Map<string, ReadingLifecycle>();
  const readerContexts = new Map<string, string>();
  const engagement = new Map<string, ReadingEngagement>();
  const sessions = new Map<string, ReadingState>();
  const documents = new Map<string, DocumentCard>(
    (seed.documents ?? []).map((document) => [document.id, { ...document, keywords: [...(document.keywords ?? [])] }]),
  );
  const trashedIds = new Set<string>();
  const configuredBatchFailures = new Set(seed.batchFailureIds ?? []);
  const batches = new Map<string, BulkBatchProjection>();
  const batchPlans = new Map<string, BulkBatchPlan>();
  const batchByIdempotency = new Map<string, { digest: string; batchId: string }>();
  const undoRecords = new Map<string, UndoRecord>();
  const batchSnapshots = new Map<string, Map<string, {
    collection: string;
    keywords: string[];
    lifecycle: ReadingLifecycle;
    trashed: boolean;
  }>>();
  let libraryRevision = 0;
  let batchSequence = 0;
  let undoSequence = 0;
  // Mirror the desktop `collection_sort_prefs` / `collection_paper_order`
  // tables. `list_collections` projects this in; `reorder_collection_papers`
  // / `set_collection_sort_mode` mutate it so the next refresh reflects
  // the write. The browser preview never receives real drag/drop inputs,
  // so only commands populate this map.
  const collectionMap = new Map<string, MemoryCollectionState>();
  const ensureCollection = (id: string, fallback?: Partial<MemoryCollectionState>): MemoryCollectionState => {
    const existing = collectionMap.get(id);
    if (existing) return existing;
    const seeded: MemoryCollectionState = {
      id,
      relativePath: fallback?.relativePath ?? id,
      parentId: fallback?.parentId ?? null,
      name: fallback?.name ?? id,
      sortMode: "recent",
      paperOrder: [],
    };
    collectionMap.set(id, seeded);
    return seeded;
  };
  const slotFrom = (args?: Record<string, unknown>) => {
    const request = args?.request as
      | { locale?: "zh-CN" | "en";
      slot?: PromptSlotId; text?: string; kind?: "paper" | "textbook" }
      | undefined;
    return request;
  };
  for (const collection of seed.collections ?? []) {
    collectionMap.set(collection.id, {
      id: collection.id,
      relativePath: collection.relativePath,
      parentId: collection.parentId,
      name: collection.name,
      sortMode: collection.sortMode,
      paperOrder: [...collection.paperOrder],
    });
  }
  const lifecycleFor = (paperId: string): ReadingLifecycle => {
    const existing = lifecycle.get(paperId);
    if (existing) return existing;
    const created = defaultReadingLifecycle(paperId);
    lifecycle.set(paperId, created);
    return created;
  };
  const activityKey = (paperId: string, revisionId: string) => `${paperId}:${revisionId}`;
  const requestValue = <T>(args?: Record<string, unknown>): T | undefined =>
    (args?.request ?? args) as T | undefined;
  const cloneRequest = (request: BulkOperationRequest): BulkOperationRequest => ({
    ...request,
    selection: {
      ...request.selection,
      ids: [...request.selection.ids],
    },
    action: request.action.kind === "tag_add" || request.action.kind === "tag_remove"
      ? { ...request.action, tags: [...request.action.tags] }
      : { ...request.action },
  });
  const requestDigest = (request: BulkOperationRequest) => queryDigest({
    idempotencyKey: request.idempotencyKey,
    expectedRevision: request.expectedRevision,
    selection: {
      ids: [...request.selection.ids],
      scope: request.selection.scope,
      queryDigest: request.selection.queryDigest,
      capturedRevision: request.selection.capturedRevision,
    },
    action: request.action,
  });
  const requestFromArgs = (args?: Record<string, unknown>): BulkOperationRequest | null => {
    const value = requestValue<Partial<BulkOperationRequest>>(args);
    if (!value?.selection || !value.action || !value.idempotencyKey) return null;
    return {
      idempotencyKey: String(value.idempotencyKey),
      selection: {
        ids: Array.isArray(value.selection.ids) ? value.selection.ids.map(String) : [],
        scope: value.selection.scope === "all_matching" ? "all_matching" : "explicit",
        queryDigest: value.selection.queryDigest ?? null,
        capturedRevision: Number(value.selection.capturedRevision ?? value.expectedRevision ?? 0),
        capturedAt: value.selection.capturedAt ?? new Date().toISOString(),
      },
      expectedRevision: Number(value.expectedRevision ?? value.selection.capturedRevision ?? 0),
      action: value.action as BulkAction,
    };
  };
  const inverseActionFor = (action: BulkAction, snapshot: Map<string, {
    collection: string;
    keywords: string[];
    lifecycle: ReadingLifecycle;
    trashed: boolean;
  }>): BulkAction => {
    switch (action.kind) {
      case "tag_add":
        return { kind: "tag_remove", tags: [...action.tags] };
      case "tag_remove":
        return { kind: "tag_add", tags: [...action.tags] };
      case "lifecycle": {
        const first = [...snapshot.values()][0];
        return { kind: "lifecycle", status: first ? first.lifecycle.status : "unread" };
      }
      case "trash":
        return { kind: "restore" };
      case "restore":
        return { kind: "trash" };
      case "move": {
        const first = [...snapshot.values()][0];
        return { kind: "move", targetCollection: first?.collection ?? action.targetCollection, place: "end" };
      }
      default:
        return { kind: "trash" };
    }
  };
  const captureBatchSnapshot = (ids: readonly string[]) => {
    const snapshot = new Map<string, {
      collection: string;
      keywords: string[];
      lifecycle: ReadingLifecycle;
      trashed: boolean;
    }>();
    for (const id of ids) {
      const document = documents.get(id);
      snapshot.set(id, {
        collection: document?.collection ?? "",
        keywords: [...(document?.keywords ?? [])],
        lifecycle: { ...lifecycleFor(id) },
        trashed: trashedIds.has(id),
      });
    }
    return snapshot;
  };
  const makeUndoToken = (operationId: string, expectedRevision: number): UndoToken => {
    const now = Date.now();
    return {
      id: `undo-${++undoSequence}`,
      operationId,
      createdAt: new Date(now).toISOString(),
      expiresAt: new Date(now + 10 * 60 * 1000).toISOString(),
      expectedRevision,
    };
  };
  const planBatch = (request: BulkOperationRequest): BulkBatchPlan => {
    const normalized = cloneRequest(request);
    const existing = batchByIdempotency.get(normalized.idempotencyKey);
    const digest = requestDigest(normalized);
    if (existing) {
      if (existing.digest !== digest) throw new Error("idempotency_conflict");
      const existingPlan = batchPlans.get(existing.batchId);
      if (existingPlan) return existingPlan;
      const existingBatch = batches.get(existing.batchId);
      if (existingBatch) {
        return {
          batchId: existingBatch.batchId,
          request: existingBatch.request,
          planDigest: existingBatch.planDigest,
          state: "planned",
          estimatedItems: existingBatch.outcomes.length,
          requiresConfirmation: false,
          createdAt: existingBatch.createdAt,
          expiresAt: null,
        };
      }
    }
    if (!normalized.selection.ids.length) throw new Error("empty_selection");
    if (normalized.expectedRevision !== libraryRevision || normalized.selection.capturedRevision !== libraryRevision) {
      throw new Error("stale_library_revision");
    }
    if (normalized.selection.scope === "all_matching" && !normalized.selection.queryDigest) {
      throw new Error("stale_query");
    }
    const batchId = `batch-${++batchSequence}`;
    const now = new Date().toISOString();
    const plan: BulkBatchPlan = {
      batchId,
      request: normalized,
      planDigest: digest,
      state: "planned",
      estimatedItems: normalized.selection.ids.length,
      requiresConfirmation: false,
      createdAt: now,
      expiresAt: new Date(Date.now() + 15 * 60 * 1000).toISOString(),
    };
    batchPlans.set(batchId, plan);
    batchByIdempotency.set(normalized.idempotencyKey, { digest, batchId });
    return plan;
  };
  const executeBatch = (plan: BulkBatchPlan): BulkOperationResult => {
    const existing = batches.get(plan.batchId);
    if (existing && (existing.state === "completed" || existing.state === "completed_with_errors")) {
      return {
        operationId: existing.batchId,
        idempotencyKey: existing.request.idempotencyKey,
        revision: existing.revision,
        outcomes: existing.outcomes,
        undoToken: existing.undoToken,
        replayed: true,
      };
    }
    if (plan.request.expectedRevision !== libraryRevision) throw new Error("stale_batch_plan");
    const ids = [...plan.request.selection.ids];
    const before = captureBatchSnapshot(ids);
    const outcomes = ids.map((id): { id: string; status: "applied" | "skipped" | "failed" | "conflict"; reason?: string } => {
      if (configuredBatchFailures.has(id)) return { id, status: "failed", reason: "simulated_failure" };
      const action = plan.request.action;
      if ((action.kind === "move" && !action.targetCollection.trim()) ||
          ((action.kind === "tag_add" || action.kind === "tag_remove") && action.tags.length === 0)) {
        return { id, status: "failed", reason: "invalid_action" };
      }
      const document = documents.get(id);
      if (document) {
        if (action.kind === "move") {
          documents.set(id, { ...document, collection: action.targetCollection });
        } else if (action.kind === "tag_add") {
          const next = [...new Set([...(document.keywords ?? []), ...action.tags])];
          documents.set(id, { ...document, keywords: next });
        } else if (action.kind === "tag_remove") {
          const remove = new Set(action.tags);
          documents.set(id, { ...document, keywords: (document.keywords ?? []).filter((tag) => !remove.has(tag)) });
        }
      }
      if (action.kind === "lifecycle") {
        const current = lifecycleFor(id);
        lifecycle.set(id, transitionReadingStatus(current, action.status, new Date().toISOString()));
      } else if (action.kind === "trash") {
        trashedIds.add(id);
      } else if (action.kind === "restore") {
        trashedIds.delete(id);
      }
      return { id, status: "applied" };
    });
    const changed = outcomes.some((outcome) => outcome.status === "applied");
    if (changed) libraryRevision += 1;
    const summary = summarizeOutcomes(outcomes);
    const undoToken = changed ? makeUndoToken(plan.batchId, libraryRevision) : null;
    const inverseRequest: BulkOperationRequest = {
      idempotencyKey: `undo:${plan.batchId}`,
      selection: {
        ids,
        scope: "explicit",
        queryDigest: null,
        capturedRevision: libraryRevision,
        capturedAt: new Date().toISOString(),
      },
      expectedRevision: libraryRevision,
      action: inverseActionFor(plan.request.action, before),
    };
    if (undoToken) undoRecords.set(undoToken.id, { token: undoToken, inverse: inverseRequest, state: "available" });
    const state: BulkBatchState = summary.failed || summary.conflict ? "completed_with_errors" : "completed";
    const now = new Date().toISOString();
    const projection: BulkBatchProjection = {
      batchId: plan.batchId,
      request: plan.request,
      planDigest: plan.planDigest,
      state,
      revision: libraryRevision,
      outcomes,
      undoToken,
      createdAt: plan.createdAt,
      updatedAt: now,
      replayed: false,
    };
    batchSnapshots.set(plan.batchId, before);
    batches.set(plan.batchId, projection);
    return {
      operationId: plan.batchId,
      idempotencyKey: plan.request.idempotencyKey,
      revision: libraryRevision,
      outcomes,
      undoToken,
      replayed: false,
    };
  };
  const resultFromProjection = (projection: BulkBatchProjection, replayed = false): BulkOperationResult => ({
    operationId: projection.batchId,
    idempotencyKey: projection.request.idempotencyKey,
    revision: projection.revision,
    outcomes: projection.outcomes,
    undoToken: projection.undoToken,
    replayed,
  });
  const projectionForBatch = (batchId: string): BulkBatchProjection => {
    const projection = batches.get(batchId);
    if (projection) return projection;
    const plan = batchPlans.get(batchId);
    if (!plan) throw new Error("batch_not_found");
    return {
      batchId: plan.batchId,
      request: plan.request,
      planDigest: plan.planDigest,
      state: "planned",
      revision: libraryRevision,
      outcomes: plan.request.selection.ids.map((id) => ({ id, status: "skipped" as const, reason: "not_started" })),
      undoToken: null,
      createdAt: plan.createdAt,
      updatedAt: plan.createdAt,
    };
  };
  const controlBatch = (batchId: string, control: BulkControl): BulkBatchProjection => {
    const current = projectionForBatch(batchId);
    if (control === "retry_failed") {
      const failedIds = current.outcomes
        .filter((item) => item.status === "failed" || item.status === "conflict")
        .map((item) => item.id);
      if (!failedIds.length) return current;
      const parentPlan = batchPlans.get(batchId);
      if (!parentPlan) throw new Error("batch_not_found");
      const childRequest: BulkOperationRequest = {
        ...cloneRequest(parentPlan.request),
        idempotencyKey: `${parentPlan.request.idempotencyKey}:retry:${Date.now()}`,
        selection: {
          ...parentPlan.request.selection,
          ids: failedIds,
          capturedRevision: libraryRevision,
          capturedAt: new Date().toISOString(),
        },
        expectedRevision: libraryRevision,
      };
      const childPlan = planBatch(childRequest);
      executeBatch(childPlan);
      return projectionForBatch(childPlan.batchId);
    }
    if (current.state === "completed" || current.state === "completed_with_errors" || current.state === "failed" || current.state === "cancelled") {
      return current;
    }
    const now = new Date().toISOString();
    const outcomes = current.outcomes.length > 0
      ? current.outcomes.map((item) => ({ ...item, status: "skipped" as const, reason: "cancelled" }))
      : current.request.selection.ids.map((id) => ({ id, status: "skipped" as const, reason: "cancelled" }));
    const next: BulkBatchProjection = {
      ...current,
      state: "cancelled",
      outcomes,
      updatedAt: now,
    };
    batches.set(batchId, next);
    return next;
  };
  const undoBatch = (tokenId: string): BulkOperationResult => {
    const record = undoRecords.get(tokenId);
    if (!record) throw new Error("undo_not_found");
    if (record.state !== "available") throw new Error(record.state === "used" ? "undo_already_used" : "undo_unavailable");
    if (record.token.expiresAt && Date.parse(record.token.expiresAt) <= Date.now()) {
      undoRecords.set(tokenId, { ...record, state: "expired" });
      throw new Error("undo_expired");
    }
    if (record.token.expectedRevision !== libraryRevision) {
      undoRecords.set(tokenId, { ...record, state: "conflict" });
      throw new Error("undo_conflict");
    }
    const originalBatch = batches.get(record.token.operationId);
    const originalSnapshot = batchSnapshots.get(record.token.operationId);
    if (!originalBatch || !originalSnapshot) throw new Error("undo_not_found");
    const outcomes = record.inverse.selection.ids.map((id) => {
      const before = originalSnapshot.get(id);
      if (!before) return { id, status: "conflict" as const, reason: "missing_precondition" };
      const document = documents.get(id);
      if (document) documents.set(id, { ...document, collection: before.collection, keywords: [...before.keywords] });
      lifecycle.set(id, { ...before.lifecycle });
      if (before.trashed) trashedIds.add(id); else trashedIds.delete(id);
      return { id, status: "applied" as const };
    });
    if (outcomes.some((item) => item.status === "applied")) libraryRevision += 1;
    undoRecords.set(tokenId, { ...record, state: "used" });
    const now = new Date().toISOString();
    const compensationId = `batch-compensation-${++batchSequence}`;
    const compensation: BulkBatchProjection = {
      batchId: compensationId,
      request: record.inverse,
      planDigest: requestDigest(record.inverse),
      state: outcomes.some((item) => item.status === "conflict") ? "completed_with_errors" : "completed",
      revision: libraryRevision,
      outcomes,
      undoToken: null,
      createdAt: now,
      updatedAt: now,
      replayed: false,
    };
    batches.set(compensationId, compensation);
    return resultFromProjection(compensation);
  };
  return {
    runtime: "memory",
    async open<T>(projection: ProjectionName, args?: Record<string, unknown>): Promise<T> {
      if (projection === "get_reading_state") {
        return (sessions.get(String(args?.paperId ?? "")) ?? null) as T;
      }
      if (projection === "get_reading_lifecycle") {
        const paperId = String(args?.paperId ?? "");
        return lifecycleFor(paperId) as T;
      }
      if (projection === "get_reading_context") {
        const paperId = String(args?.paperId ?? "");
        const state = lifecycleFor(paperId);
        const revisionId = String(args?.revisionId ?? "");
        return {
          lifecycle: state,
          engagement: revisionId ? engagement.get(activityKey(paperId, revisionId)) ?? null : null,
          session: sessions.get(paperId) ?? null,
        } as ReadingContextProjection as T;
      }
      if (projection === "list_recent_reading") {
        const recent = [...engagement.values()]
          .sort((a, b) => b.lastOpenedAt.localeCompare(a.lastOpenedAt))
          .map((entry) => ({
            document: null,
            lifecycle: lifecycleFor(entry.paperId),
            engagement: entry,
            session: sessions.get(entry.paperId) ?? null,
          }));
        return recent as unknown as RecentReadingProjection[] as T;
      }
      if (projection === "list_annotations") {
        const paperId = String(args?.paperId ?? "");
        return (annotations.get(paperId) ?? []).filter((item) => item.status !== "deleted") as T;
      }
      if (projection === "list_annotation_links") {
        const paperId = String(args?.paperId ?? "");
        const ids = new Set((annotations.get(paperId) ?? []).map((item) => item.id));
        return [...annotationLinks.values()].filter(
          (link) => ids.has(link.sourceAnnotationId) || ids.has(link.targetAnnotationId),
        ) as T;
      }
      if (projection === "list_documents") {
        return [...documents.values()]
          .filter((document) => !trashedIds.has(document.id))
          .map((document) => {
            const life = lifecycle.get(document.id) ?? defaultReadingLifecycle(document.id);
            const progress = engagement.get(activityKey(document.id, document.revisionId));
            return {
              ...document,
              lifecycleStatus: life.status,
              favorite: life.favorite,
              priority: life.priority,
              readLater: life.readLater,
              reviewAt: life.reviewAt,
              lifecycleVersion: life.version,
              furthestPage: progress?.furthestPage ?? document.furthestPage ?? null,
              lastOpenedAt: progress?.lastOpenedAt ?? document.lastOpenedAt ?? null,
            };
          }) as T;
      }
      if (projection === "list_trash") {
        const now = new Date().toISOString();
        return [...trashedIds].map((paperId): TrashProjection => {
          const document = documents.get(paperId);
          return {
            id: `trash-${paperId}`,
            paperId,
            title: document?.title ?? paperId,
            fileName: document?.fileName ?? `${paperId}.pdf`,
            originalRelativePath: document?.collection ?? "",
            sourceBytes: 0,
            deletedAt: now,
            purgeAfter: new Date(Date.now() + 30 * 24 * 60 * 60 * 1000).toISOString(),
          };
        }) as T;
      }
      const values: Partial<Record<ProjectionName, unknown>> = {
        get_workspace: null satisfies WorkspaceInfo | null,
        get_guide_character_settings: characterSettings,
        get_model_settings: settings,
        get_prompt_settings: clonePromptSettings(promptsByLocale[args?.locale === "en" ? "en" : args?.locale === "zh-CN" ? "zh-CN" : promptLocale()]),
        get_ui_locale: { locale: uiLocale } satisfies UiLocaleProjection,
        get_reading_state: null,
        list_threads: [] satisfies Thread[],
        list_archived_threads: [] satisfies Thread[],
        list_messages: [] satisfies Message[],
        get_brief: null satisfies Brief | null,
        get_outline_deep_dive: null,
        get_reading_guide: {
          status: "missing_ocr",
          revisionId: "",
          ocrRevisionId: null,
          hasPaperRoot: false,
          head: null,
          activeJobId: null,
        } satisfies GuideProjection,
        get_outline: {
          status: "missing_ocr",
          revisionId: "",
          ocrRevisionId: null,
          hasPaperRoot: false,
          catalog: null,
          head: null,
          activeJobId: null,
          coverageWarnings: [],
        } satisfies OutlineProjection,
        list_documents: [...documents.values()].filter((document) => !trashedIds.has(document.id)),
        get_stats: {
          documents: 0,
          threads: 0,
          messages: 0,
          inputTokens: 0,
          cachedTokens: 0,
          outputTokens: 0,
          estimatedCost: null,
        } satisfies AppStats,
        get_storage_report: {
          workspaceBytes: 0,
          papersBytes: 0,
          internalBytes: 0,
          paperLogicalBytes: [],
          artifactLogicalBytes: [],
        } satisfies StorageReport,
        list_trash: [...trashedIds].map((paperId): TrashProjection => {
          const document = documents.get(paperId);
          return {
            id: `trash-${paperId}`,
            paperId,
            title: document?.title ?? paperId,
            fileName: document?.fileName ?? `${paperId}.pdf`,
            originalRelativePath: document?.collection ?? "",
            sourceBytes: 0,
            deletedAt: new Date().toISOString(),
            purgeAfter: new Date(Date.now() + 30 * 24 * 60 * 60 * 1000).toISOString(),
          };
        }),
        list_remote_tombstones: [] satisfies RemoteTombstoneProjection[],
        list_artifacts: [] satisfies ArtifactProjection[],
        get_artifact: null,
        get_artifact_asset_path: null,
        latest_ocr: null satisfies OcrProjection | null,
        get_ocr: null,
        list_lens_qa: [] satisfies LensQaProjection[],
        list_jobs: [] satisfies JobProjection[],
        get_roadmap: null,
        list_roadmap_progress: [],
        list_collections: Array.from(collectionMap.values()).map((c) => ({
          id: c.id,
          parentId: c.parentId,
          name: c.name,
          relativePath: c.relativePath,
          sortMode: c.sortMode,
          paperOrder: c.paperOrder,
        })),
        preview_diagnostics: {
          generatedAt: new Date(0).toISOString(),
          includedSections: [],
          excludedData: [],
          summary: {},
        },
      };
      return values[projection] as T;
    },
    async command<T>(
      command: CommandName,
      args?: Record<string, unknown>,
    ): Promise<T> {
      if (command === "plan_library_batch") {
        const request = requestFromArgs(args);
        if (!request) throw new Error("batch_request_invalid");
        return planBatch(request) as T;
      }
      if (command === "start_library_batch") {
        const request = requestValue<{ batchId?: string; planDigest?: string }>(args);
        const batchId = request?.batchId ?? String(args?.batchId ?? "");
        if (!batchId) throw new Error("batch_id_required");
        const plan = batchPlans.get(batchId);
        if (!plan) throw new Error("batch_not_found");
        const suppliedDigest = request?.planDigest ?? String(args?.planDigest ?? "");
        if (suppliedDigest && suppliedDigest !== plan.planDigest) throw new Error("stale_batch_plan");
        const result = executeBatch(plan);
        const projection = projectionForBatch(batchId);
        return { ...projection, replayed: result.replayed } as T;
      }
      if (command === "get_library_batch") {
        const request = requestValue<{ batchId?: string }>(args);
        const batchId = request?.batchId ?? String(args?.batchId ?? "");
        if (!batchId) throw new Error("batch_id_required");
        return projectionForBatch(batchId) as T;
      }
      if (command === "control_library_batch") {
        const request = requestValue<{ batchId?: string; control?: BulkControl }>(args);
        const batchId = request?.batchId ?? String(args?.batchId ?? "");
        const control = request?.control ?? (args?.control as BulkControl | undefined);
        if (!batchId || !control) throw new Error("batch_control_invalid");
        return controlBatch(batchId, control) as T;
      }
      if (command === "undo_library_batch") {
        const request = requestValue<{ tokenId?: string; undoTokenId?: string }>(args);
        const tokenId = request?.tokenId ?? request?.undoTokenId ?? String(args?.tokenId ?? args?.undoTokenId ?? "");
        if (!tokenId) throw new Error("undo_token_required");
        return undoBatch(tokenId) as T;
      }
      if (command === "save_reading_state") {
        const state = (args?.readingState ?? args?.request) as ReadingState | undefined;
        if (!state?.paperId) throw new Error("Reading state requires paperId");
        sessions.set(state.paperId, { ...state });
        return { ...state } as T;
      }
      if (command === "record_reading_activity" || command === "record_reader_activity") {
        const request = requestValue<RecordReadingActivityRequest>(args);
        if (!request?.paperId || !request.revisionId) throw new Error("Reading activity requires paperId and revisionId");
        const current = lifecycleFor(request.paperId);
        const previous = engagement.get(activityKey(request.paperId, request.revisionId)) ?? null;
        const result = recordReadingActivity({
          lifecycle: current,
          engagement: previous,
          paperId: request.paperId,
          revisionId: request.revisionId,
          page: request.pageNumber,
          pageCount: request.pageCount,
          now: request.occurredAt,
        });
        lifecycle.set(request.paperId, result.lifecycle);
        engagement.set(activityKey(request.paperId, request.revisionId), result.engagement);
        return {
          lifecycle: result.lifecycle,
          engagement: result.engagement,
          session: sessions.get(request.paperId) ?? null,
        } as ReadingContextProjection as T;
      }
      if (command === "update_reading_lifecycle" || command === "patch_reading_lifecycle") {
        const request = requestValue<UpdateReadingLifecycleRequest>(args);
        if (!request?.paperId) throw new Error("Lifecycle request requires paperId");
        const current = lifecycleFor(request.paperId);
        if (request.expectedVersion !== undefined && request.expectedVersion !== current.version) {
          throw new Error("stale_lifecycle_version");
        }
        const patch = request.patch ?? {};
        let next = current;
        if (patch.status !== undefined) next = transitionReadingStatus(next, patch.status, request.occurredAt ?? new Date().toISOString());
        if (next === current && (patch.favorite !== undefined || patch.priority !== undefined || patch.readLater !== undefined || patch.reviewAt !== undefined)) {
          const timestamp = request.occurredAt ?? new Date().toISOString();
          next = {
            ...current,
            favorite: patch.favorite ?? current.favorite,
            priority: patch.priority !== undefined && [0, 1, 2, 3].includes(patch.priority) ? patch.priority as 0 | 1 | 2 | 3 : current.priority,
            readLater: patch.readLater ?? current.readLater,
            reviewAt: patch.reviewAt !== undefined ? patch.reviewAt : current.reviewAt,
            version: current.version + 1,
            updatedAt: timestamp,
          };
        }
        lifecycle.set(request.paperId, next);
        return next as T;
      }
      if (command === "create_annotation") {
        const request = args?.request as UserAnnotationInput | undefined;
        if (!request?.paperId || !request.locator || !request.kind) {
          throw new Error("Annotation request is incomplete");
        }
        const now = new Date().toISOString();
        const item: UserAnnotation = {
          id: `annotation-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
          paperId: request.paperId,
          kind: request.kind,
          title: request.title ?? null,
          body: request.body ?? null,
          color: request.color ?? null,
          locator: { ...request.locator },
          status: request.status ?? "active",
          createdAt: now,
          updatedAt: now,
        };
        annotations.set(item.paperId, [...(annotations.get(item.paperId) ?? []), item]);
        return item as T;
      }
      if (command === "update_annotation") {
        const request = args?.request as UserAnnotationUpdateRequest | undefined;
        if (!request?.id) throw new Error("Annotation id is required");
        let updated: UserAnnotation | null = null;
        for (const [paperId, list] of annotations) {
          const next = list.map((item) => {
            if (item.id !== request.id) return item;
            updated = {
              ...item,
              ...(request.title !== undefined ? { title: request.title } : {}),
              ...(request.body !== undefined ? { body: request.body } : {}),
              ...(request.color !== undefined ? { color: request.color } : {}),
              ...(request.status !== undefined ? { status: request.status } : {}),
              ...(request.locator !== undefined ? { locator: request.locator } : {}),
              updatedAt: new Date().toISOString(),
            };
            return updated;
          });
          if (updated) annotations.set(paperId, next);
        }
        return updated as T;
      }
      if (command === "delete_annotation") {
        const id = String(args?.annotationId ?? "");
        for (const [paperId, list] of annotations) {
          const next = list.map((item) =>
            item.id === id ? { ...item, status: "deleted" as const, updatedAt: new Date().toISOString() } : item,
          );
          annotations.set(paperId, next);
        }
        return Boolean(id) as T;
      }
      if (command === "create_annotation_link") {
        const request = args?.request as UserAnnotationLinkInput | undefined;
        if (!request?.sourceAnnotationId || !request?.targetAnnotationId || request.sourceAnnotationId === request.targetAnnotationId) {
          throw new Error("Annotation link endpoints are invalid");
        }
        const existing = [...annotationLinks.values()].find(
          (link) => link.sourceAnnotationId === request.sourceAnnotationId && link.targetAnnotationId === request.targetAnnotationId && link.linkKind === request.linkKind,
        );
        if (existing) return existing as T;
        const link: UserAnnotationLink = {
          id: `annotation-link-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
          sourceAnnotationId: request.sourceAnnotationId,
          targetAnnotationId: request.targetAnnotationId,
          linkKind: request.linkKind,
          createdAt: new Date().toISOString(),
        };
        annotationLinks.set(link.id, link);
        return link as T;
      }
      if (command === "delete_annotation_link") {
        const id = String(args?.linkId ?? "");
        return annotationLinks.delete(id) as T;
      }
      if (command === "restore_outline_prompt_bundle") {
        const request = requestArg<{ kind?: "paper" | "textbook"; locale?: "zh-CN" | "en" }>(args);
        const kind = request?.kind ?? "paper";
        const factory = factoryPromptSettings();
        const locale = request?.locale === "en" ? "en" : request?.locale === "zh-CN" ? "zh-CN" : promptLocale();
        let prompts = promptsByLocale[locale];
        const slots = { ...prompts.slots };
        for (const slot of ["outline_extract", "outline_compose", "outline_deep_dive"] as const) {
          const current = slots[slot][kind];
          if (current.outputProtocol === "v4" && current.text === factory.slots[slot][kind].text) continue;
          outlinePreviousProtocols.set(`${locale}:${kind}:${slot}`, current.outputProtocol ?? "v4");
          slots[slot] = { ...slots[slot], [kind]: { ...factory.slots[slot][kind], previousText: current.text, updatedAt: new Date().toISOString() } };
        }
        prompts = { ...prompts, slots };
        promptsByLocale[locale] = prompts;
        return clonePromptSettings(prompts) as T;
      }
      if (
        command === "save_prompt_slot" ||
        command === "restore_prompt_previous" ||
        command === "restore_prompt_default"
      ) {
        const request = slotFrom(args);
        const slot = request?.slot;
        const locale = request?.locale === "en" ? "en" : request?.locale === "zh-CN" ? "zh-CN" : promptLocale();
        let prompts = promptsByLocale[locale];
        if (!slot || !prompts.slots[slot]) {
          throw new Error("Unknown prompt slot");
        }
        const kind = request?.kind ?? "paper";
        const pair = prompts.slots[slot];
        const current = pair[kind];
        const outline = ["outline_extract", "outline_compose", "outline_deep_dive"].includes(slot);
        const protocolTracked = outline || (kind === "textbook" && ["orientation_pack","lens_formula","lens_figure","lens_table","lens_repair_formula","lens_repair_figure","lens_repair_table"].includes(slot));
        const historyKey = `${locale}:${kind}:${slot}`;
        const previousProtocol = outlinePreviousProtocols.get(historyKey) ?? current.outputProtocol ?? "v4";
        if (command === "save_prompt_slot") {
          if (protocolTracked) outlinePreviousProtocols.set(historyKey, current.outputProtocol ?? "v4");
          const text = request?.text ?? "";
          if (!text.trim()) throw new Error("Prompt cannot be empty");
          prompts = {
            ...prompts,
            slots: {
              ...prompts.slots,
              [slot]: {
                ...pair,
                [kind]: {
                  ...current,
                  text,
                  previousText: current.text,
                  updatedAt: new Date().toISOString(),
                  isDefault: false,
                },
              },
            },
          };
        } else if (command === "restore_prompt_previous") {
          if (!current.previousText) {
            throw new Error("No previous prompt to restore");
          }
          if (protocolTracked) outlinePreviousProtocols.set(historyKey, current.outputProtocol ?? "v4");
          prompts = {
            ...prompts,
            slots: {
              ...prompts.slots,
              [slot]: {
                ...pair,
                [kind]: {
                  ...current,
                  ...(protocolTracked ? { outputProtocol: previousProtocol } : {}),
                  text: current.previousText,
                  previousText: current.text,
                  updatedAt: new Date().toISOString(),
                  isDefault: false,
                },
              },
            },
          };
        } else {
          const factory = factoryPromptSettings().slots[slot][kind];
          if (protocolTracked && current.outputProtocol === factory.outputProtocol && current.text === factory.text) {
            return clonePromptSettings(prompts) as T;
          }
          if (protocolTracked) outlinePreviousProtocols.set(historyKey, current.outputProtocol ?? "v4");
          prompts = {
            ...prompts,
            slots: {
              ...prompts.slots,
              [slot]: {
                ...pair,
                [kind]: {
                  ...factory,
                  text: factory.text,
                  previousText: current.text,
                  updatedAt: new Date().toISOString(),
                  isDefault: true,
                },
              },
            },
          };
        }
        promptsByLocale[locale] = prompts;
        return clonePromptSettings(prompts) as T;
      }
      if (command === "add_provider") {
        const request = requestArg<{ name?: string; kind?: ProviderKind }>(args);
        const kind = request?.kind ?? "gemini";
        const name = request?.name ?? "Provider";
        const newInst: ProviderInstanceView = {
          id: `p-${Date.now()}`,
          name,
          kind,
          baseUrl:
            kind === "grok"
              ? "https://api.x.ai/v1"
              : kind === "gemini_proxy"
              ? "http://localhost:8045/v1"
              : kind === "openai_compatible"
              ? "https://api.openai.com/v1"
              : null,
          paperModel: kind === "gemini" || kind === "gemini_proxy" ? "gemini-2.5-flash" : "",
          translationModel: kind === "gemini" || kind === "gemini_proxy" ? "gemini-2.5-flash-lite" : "",
          models: previewModels,
          modelsFetchedAt: null,
          connectionVerifiedAt: null,
          paperProbe: null,
          credentialConfigured: false,
          paperProbePassed: false,
          isCurrent: false,
          sortOrder: settings.providers.length + 1,
        };
        settings = {
          ...settings,
          providers: [...settings.providers, newInst],
        };
        return settings as T;
      }
      if (command === "remove_provider") {
        const id = requestArg<{ id?: string }>(args)?.id;
        settings = {
          ...settings,
          currentProviderId: settings.currentProviderId === id ? null : settings.currentProviderId,
          providers: settings.providers.filter((p) => p.id !== id),
        };
        return settings as T;
      }
      if (command === "rename_provider") {
        const request = requestArg<{ id?: string; name?: string }>(args);
        settings = {
          ...settings,
          providers: settings.providers.map((p) =>
            p.id === request?.id ? { ...p, name: request?.name ?? p.name } : p,
          ),
        };
        return settings as T;
      }
      if (command === "duplicate_provider") {
        const id = requestArg<{ id?: string }>(args)?.id;
        const source = settings.providers.find((p) => p.id === id);
        if (source) {
          const dup: ProviderInstanceView = {
            ...source,
            id: `p-${Date.now()}`,
            name: `${source.name} (copy)`,
            credentialConfigured: false,
            paperProbePassed: false,
            paperProbe: null,
            connectionVerifiedAt: null,
            isCurrent: false,
            sortOrder: settings.providers.length + 1,
          };
          settings = {
            ...settings,
            providers: [...settings.providers, dup],
          };
        }
        return settings as T;
      }
      if (command === "reorder_providers") {
        const ids = requestArg<{ ids?: string[] }>(args)?.ids ?? [];
        const ordered = [...settings.providers].sort(
          (a, b) => ids.indexOf(a.id) - ids.indexOf(b.id),
        );
        settings = {
          ...settings,
          providers: ordered.map((p, i) => ({ ...p, sortOrder: i + 1 })),
        };
        return settings as T;
      }
      if (command === "test_provider_connection") {
        return {
          models: previewModels,
          testedAt: new Date().toISOString(),
          usingStoredCredential: false,
          // Browser preview never contacts a provider, so it must not claim that
          // a paper-capability probe (or a usable provider route) succeeded.
          paperProbePassed: null,
          paperProbeError: null,
          modelsFetchError:
            "Browser preview uses a static model catalog; no provider connection was tested.",
        } as T & ConnectionTestResult;
      }
      if (command === "save_provider_settings") {
        const request = requestArg<{
          id?: string;
          apiKey?: string | null;
          baseUrl?: string | null;
          paperModel?: string;
          translationModel?: string;
        }>(args);
        settings = {
          ...settings,
          providers: settings.providers.map((p) => {
            if (p.id !== request?.id) return p;
            return {
              ...p,
              paperModel: request?.paperModel ?? p.paperModel,
              translationModel: request?.translationModel ?? p.translationModel,
              baseUrl: request?.baseUrl !== undefined ? request.baseUrl : p.baseUrl,
              // Preview input is not a persisted credential and has not been
              // tested. Keep the instance explicitly non-ready.
              credentialConfigured: false,
              paperProbePassed: false,
              paperProbe: null,
              connectionVerifiedAt: null,
            };
          }),
        };
        return settings as T;
      }
      if (command === "clear_provider_credential") {
        const id = requestArg<{ id?: string }>(args)?.id;
        settings = {
          ...settings,
          providers: settings.providers.map((p) => {
            if (p.id !== id) return p;
            return {
              ...p,
              credentialConfigured: false,
              paperProbePassed: false,
              paperProbe: null,
              connectionVerifiedAt: null,
            };
          }),
        };
        return settings as T;
      }
      if (command === "set_current_provider") {
        const id = requestArg<{ id?: string }>(args)?.id ?? null;
        settings = {
          ...settings,
          currentProviderId: id,
          providers: settings.providers.map((p) => ({
            ...p,
            isCurrent: p.id === id,
          })),
        };
        return settings as T;
      }
      if (command === "reorder_collection_papers" || command === "set_collection_sort_mode") {
        const request = args?.request as
          | { collectionId?: string; paperIds?: string[]; sortMode?: MemoryCollectionState["sortMode"] }
          | undefined;
        const collectionId = request?.collectionId;
        if (!collectionId) throw new Error("collectionId is required");
        const col = ensureCollection(collectionId);
        if (command === "reorder_collection_papers") {
          const paperIds = request?.paperIds ?? [];
          if (new Set(paperIds).size !== paperIds.length) {
            throw new Error("paperIds must be exact permutation of live papers in collection");
          }
          col.paperOrder = [...paperIds];
        } else {
          const mode = request?.sortMode;
          const allowed: MemoryCollectionState["sortMode"][] = [
            "recent",
            "year",
            "title",
            "manual",
            "chapter",
          ];
          if (!mode || !allowed.includes(mode)) throw new Error("Invalid sort mode");
          col.sortMode = mode;
        }
        return undefined as T;
      }
      if (command === "delete_artifact") {
        return null as T;
      }
      if (command === "ack_long_pdf_warning") {
        return undefined as T;
      }
      if (command === "save_mistral_credential") return settings as T;
      if (command === "clear_mistral_credential") return settings as T;
      if (command === "set_window_theme") return undefined as T;
      if (command === "set_ui_locale") {
        const locale = String(args?.locale ?? "");
        if (locale !== "zh-CN" && locale !== "en") {
          throw new Error("Invalid UI locale");
        }
        uiLocale = locale;
        return { locale: uiLocale } satisfies UiLocaleProjection as T;
      }
      if (command === "resolve_exit_intent") return true as T;
      if (command === "get_reader_context" || command === "save_reader_context") {
        const request = requestArg<{
          scope?: ReaderContextScope;
          collectionPath?: string;
          paperId?: string;
          text?: string;
        }>(args);
        const scope = request?.scope ?? "workspace";
        const key = `${scope}:${request?.collectionPath ?? ""}:${request?.paperId ?? ""}`;
        if (command === "save_reader_context") {
          const text = (request?.text ?? "").trim();
          if (text) readerContexts.set(key, text);
          else readerContexts.delete(key);
        }
        const text = readerContexts.get(key) ?? "";
        return {
          scope,
          collectionPath: request?.collectionPath ?? null,
          paperId: request?.paperId ?? null,
          text,
          charCount: [...text].length,
          warn: [...text].length >= 2000,
          path: key,
        } satisfies ReaderContextProjection as T;
      }
      if (command === "restore_reader_folder_context") {
        return { id: "", originalRelativePath: "", deletedAt: "", purgeAfter: "" } as T;
      }
      if (command === "save_guide_character") {
        const request = requestArg<{
          expectedStoreRevision?: number;
          character?: {
            id?: string | null;
            displayName: string;
            workTitle?: string;
            characterVersion?: string;
            description?: string;
            avatarAssetId?: string | null;
            inkColor: string;
            personality?: string;
            readingHabits?: string;
            expressionStyle?: string;
            avoidances?: string;
            exampleNotes?: string[];
          };
        }>(args);
        if (request?.expectedStoreRevision !== characterSettings.storeRevision) {
          throw new Error("角色配置已被其他窗口更新");
        }
        const draft = request?.character;
        if (!draft?.displayName) throw new Error("显示名称不能为空");
        const now = new Date().toISOString();
        if (draft.id) {
          characterSettings = {
            ...characterSettings,
            storeRevision: characterSettings.storeRevision + 1,
            characters: characterSettings.characters.map((item) =>
              item.id === draft.id
                  ? {
                      ...item,
                      ...draft,
                      id: item.id,
                      exampleNotes: draft.exampleNotes ?? item.exampleNotes,
                      revision: item.revision + 1,
                      updatedAt: now,
                    }
                : item,
            ),
          };
        } else {
          const id = `custom:${now}`;
          characterSettings = {
            ...characterSettings,
            storeRevision: characterSettings.storeRevision + 1,
            characters: [
              ...characterSettings.characters,
              {
                id,
                revision: 1,
                displayName: draft.displayName,
                workTitle: draft.workTitle ?? "",
                characterVersion: draft.characterVersion ?? "",
                description: draft.description ?? "",
                avatarAssetId: draft.avatarAssetId ?? null,
                inkColor: draft.inkColor,
                personality: draft.personality ?? "",
                readingHabits: draft.readingHabits ?? "",
                expressionStyle: draft.expressionStyle ?? "",
                avoidances: draft.avoidances ?? "",
                exampleNotes: draft.exampleNotes ?? [],
                presetId: null,
                presetVersion: null,
                createdAt: now,
                updatedAt: now,
              },
            ],
          };
        }
        return characterSettings as T;
      }
      if (
        command === "delete_guide_character" ||
        command === "duplicate_guide_character" ||
        command === "restore_guide_character_preset"
      ) {
        const request = requestArg<{ characterId?: string; expectedStoreRevision?: number }>(args);
        if (request?.expectedStoreRevision !== characterSettings.storeRevision) {
          throw new Error("角色配置已被其他窗口更新");
        }
        if (command === "delete_guide_character") {
          characterSettings = {
            ...characterSettings,
            storeRevision: characterSettings.storeRevision + 1,
            characters: characterSettings.characters.filter((item) => item.id !== request?.characterId),
            defaultCharacterIds: characterSettings.defaultCharacterIds.filter(
              (id) => id !== request?.characterId,
            ),
          };
        }
        return characterSettings as T;
      }
      if (command === "save_guide_default_cast") {
        const request = requestArg<{ characterIds?: string[]; expectedStoreRevision?: number }>(args);
        if (request?.expectedStoreRevision !== characterSettings.storeRevision) {
          throw new Error("角色配置已被其他窗口更新");
        }
        characterSettings = {
          ...characterSettings,
          storeRevision: characterSettings.storeRevision + 1,
          defaultCharacterIds: request?.characterIds ?? characterSettings.defaultCharacterIds,
        };
        return characterSettings as T;
      }
      if (command === "save_guide_preset_casts") {
        const request = requestArg<{ presetCasts?: { id: string; name: string; characterIds: string[] }[]; expectedStoreRevision?: number }>(args);
        if (request?.expectedStoreRevision !== characterSettings.storeRevision) {
          throw new Error("角色配置已被其他窗口更新");
        }
        characterSettings = {
          ...characterSettings,
          storeRevision: characterSettings.storeRevision + 1,
          presetCasts: request?.presetCasts ?? characterSettings.presetCasts,
        };
        return characterSettings as T;
      }
      if (command === "restore_guide_factory_preset_casts") {
        const request = requestArg<{ expectedStoreRevision?: number }>(args);
        if (request?.expectedStoreRevision !== characterSettings.storeRevision) {
          throw new Error("角色配置已被其他窗口更新");
        }
        characterSettings = {
          ...characterSettings,
          storeRevision: characterSettings.storeRevision + 1,
          presetCasts: [
            { id: "daily", name: "日常阅读", characterIds: ["preset:chitanda", "preset:oreki", "preset:frieren"] },
            { id: "evidence", name: "实验与证据", characterIds: ["preset:chitanda", "preset:conan", "preset:jotaro"] },
            { id: "classics", name: "古典部双人", characterIds: ["preset:chitanda", "preset:oreki"] },
          ],
        };
        return characterSettings as T;
      }
      if (command === "import_guide_character_avatar") {
        const request = requestArg<{ bytesBase64?: string }>(args);
        const assetId = `mem-avatar-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
        const dataUrl = request?.bytesBase64
          ? `data:image/png;base64,${request.bytesBase64}`
          : "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
        avatarAssets.set(assetId, { mime: "image/png", dataUrl });
        return { assetId, mime: "image/png", extension: "png" } as T;
      }
      if (command === "get_guide_character_avatar") {
        const request = requestArg<{ assetId?: string }>(args);
        const asset = request?.assetId ? avatarAssets.get(request.assetId) : undefined;
        return {
          assetId: request?.assetId ?? "",
          mime: asset?.mime ?? "image/png",
          extension: "png",
          dataUrl: asset?.dataUrl ?? "",
        } as T;
      }
      if (command === "preview_guide_character") {
        throw new Error("preview_guide_character requires the Read Atlas runtime");
      }

      if (command === "plan_outline" || command === "plan_outline_deep_dive") {
        throw new Error("plan_outline requires the Read Atlas runtime");
      }
      if (command === "plan_reading_guide") {
        throw new Error("plan_reading_guide requires the Read Atlas runtime");
      }
      if (command === "start_reading_guide") {
        throw new Error("start_reading_guide requires the Read Atlas runtime");
      }
      throw new Error(`${command} requires the Read Atlas runtime`);
    },
    async watch() {
      return () => undefined;
    },
  };
}

const hasTauriRuntime = Boolean(
  (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__,
);

export const desktopClient: DesktopClient = hasTauriRuntime
  ? new TauriDesktopClient()
  : createMemoryDesktopClient();
