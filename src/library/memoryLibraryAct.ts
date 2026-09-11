// 浏览器预览 / 组件测用的 `library_act` Adapter。语义对齐桌面端：plan 不写业务、
// start 才生效、requirements 必须按 id 接受、Undo Token 单次且明文只出现在响应里。
// 它不是第二份权威——真实冲突 / 文件系统 journal 永远以 Rust 为准。

import {
  LIBRARY_ACT_PROTOCOL_VERSION,
  MAX_BATCH_ITEMS,
  MAX_IDEMPOTENCY_KEY_CHARS,
  RECENT_BATCHES_DEFAULT,
  RECENT_BATCHES_MAX,
  type BatchCommand,
  type BatchControl,
  type BatchItemProjection,
  type BatchItemsPage,
  type BatchProjection,
  type BatchRequirement,
  type BatchState,
  type CostPreview,
  EMPTY_COST_PREVIEW,
  type BatchTarget,
  type ItemCounts,
  type ItemState,
  type LibraryActError,
  type LibraryActRequest,
  type LibraryActResponse,
  type RecentBatchesPage,
  type UndoPolicy,
} from "./libraryActTypes";
import {
  BUILTIN_SMART_COLLECTIONS,
  defaultHubCardLifecycle,
  selectionDependencyDomains,
  type HubPaperCard,
  type LibraryDomain,
  type LibraryQueryFilter,
  type SmartCollectionProjection,
} from "./libraryWorkspaceTypes";
import { cardMatchesLibraryFilter } from "./hubFilters";
import {
  defaultReadingLifecycle,
  lifecycleFromUnknown,
  recordReadingActivity,
  type ReadingLifecycle,
} from "../readerReadingState";
import type {
  ChangeResult,
  LibraryChange,
  LifecyclePatch,
  ReaderActivity,
} from "./libraryActTypes";

export type MemoryActHost = {
  cards(): HubPaperCard[];
  replaceCards(cards: HubPaperCard[]): void;
  bump(domains: readonly LibraryDomain[]): number;
  selectionDigest(filters: readonly LibraryQueryFilter[]): string;
  domainValue(domain: LibraryDomain): number;
};

type Mutable<T> = { -readonly [K in keyof T]: T[K] };

type MemoryItem = {
  projection: Mutable<BatchItemProjection>;
  beforeTags?: string[];
  beforeCollection?: string;
  beforeRelative?: string;
  beforeFileName?: string;
  beforeLifecycle?: Pick<
    HubPaperCard,
    "lifecycleStatus" | "favorite" | "priority" | "readLater" | "reviewAt" | "lifecycleVersion"
  >;
};

type MemoryBatch = {
  projection: Mutable<BatchProjection>;
  command: BatchCommand;
  items: MemoryItem[];
  undoToken: string | null;
  undoHash: string | null;
};

function fail(code: LibraryActError["code"], message: string): never {
  const error: LibraryActError = { code, message };
  throw error;
}

function emptyCounts(overrides: Partial<ItemCounts> = {}): ItemCounts {
  return {
    planned: 0,
    queued: 0,
    running: 0,
    paused: 0,
    actionRequired: 0,
    interruptedUnknown: 0,
    succeeded: 0,
    failed: 0,
    skipped: 0,
    cancelled: 0,
    ...overrides,
  };
}

function countsFrom(items: readonly MemoryItem[]): ItemCounts {
  const counts: Mutable<ItemCounts> = { ...emptyCounts() };
  for (const item of items) {
    const key = item.projection.state;
    if (key === "action_required") counts.actionRequired += 1;
    else if (key === "interrupted_unknown") counts.interruptedUnknown += 1;
    else counts[key] += 1;
  }
  return counts;
}

function aggregate(counts: ItemCounts, started: boolean): BatchState {
  if (!started) return "planned";
  if (counts.running > 0) return "running";
  if (counts.queued > 0) return "queued";
  if (counts.interruptedUnknown > 0) return "interrupted_unknown";
  if (counts.actionRequired > 0) return "action_required";
  if (counts.paused > 0) return "paused";
  const terminal = counts.succeeded + counts.failed + counts.skipped + counts.cancelled;
  const total = terminal + counts.planned;
  if (total === 0) return "completed";
  if (counts.cancelled === total) return "cancelled";
  if (counts.failed === total) return "failed";
  if ((counts.succeeded > 0 || counts.skipped > 0) && (counts.failed > 0 || counts.cancelled > 0)) {
    return "completed_with_errors";
  }
  return "completed";
}

function nowIso(): string {
  return new Date().toISOString();
}

function rootOf(path: string): string {
  return path.split("/")[0] ?? "";
}

function fileNameOf(path: string): string {
  return path.replace(/\\/g, "/").split("/").pop() || path;
}

function digestOf(request: LibraryActRequest): string {
  return JSON.stringify(request);
}

export function createMemoryActEngine(host: MemoryActHost) {
  const batches = new Map<string, MemoryBatch>();
  const receipts = new Map<string, { digest: string; response: LibraryActResponse }>();
  let seq = 0;

  const nextId = (prefix: string) => {
    seq += 1;
    return `${prefix}-${seq}`;
  };

  const resolvePapers = (target: BatchTarget): HubPaperCard[] => {
    const cards = host.cards();
    if (target.kind === "explicit") {
      if (target.paperIds.length > MAX_BATCH_ITEMS) fail("batch_too_large", "selection exceeds 500");
      return target.paperIds.map((id) => {
        const card = cards.find((item) => item.id === id);
        if (!card) fail("paper_not_found", `paper ${id} is not in the library`);
        return card;
      });
    }
    if (target.kind === "query") {
      const digest = host.selectionDigest(target.filters);
      if (digest !== target.selectionDigest) {
        fail("stale_selection", "the query behind this selection changed; re-select before submitting");
      }
      const domains = selectionDependencyDomains(target.filters);
      const presented = new Map(target.dependencyRevisions.map((entry) => [entry.domain, entry.value]));
      for (const domain of domains) {
        if (presented.get(domain) !== host.domainValue(domain)) {
          fail("stale_selection", "a dependency of this selection moved since it was evaluated; re-select first");
        }
      }
      const excluded = new Set(target.excludedIds);
      const anchor = target.evaluationAnchor ?? nowIso();
      const matched = cards.filter((card) => matchesFilters(card, target.filters, anchor) && !excluded.has(card.id));
      if (matched.length > MAX_BATCH_ITEMS) fail("batch_too_large", "selection exceeds 500");
      return matched;
    }
    fail("invalid_query", "this command works on Papers; it needs paperIds or a query selection");
  };

  const replay = (key: string, digest: string): LibraryActResponse | null => {
    const stored = receipts.get(key);
    if (!stored) return null;
    if (stored.digest !== digest) fail("idempotency_conflict", "the same key was used for a different request");
    return stored.response;
  };

  const store = (key: string, digest: string, response: LibraryActResponse) => {
    receipts.set(key, { digest, response });
    return response;
  };

  const project = (batch: MemoryBatch): BatchProjection => {
    const counts = countsFrom(batch.items);
    const started = batch.projection.startedAt !== null;
    return {
      ...batch.projection,
      counts,
      totalItems: batch.items.length,
      state: aggregate(counts, started),
      updatedAt: nowIso(),
    };
  };

  const plan = (command: BatchCommand, target: BatchTarget): BatchProjection => {
    const timestamp = nowIso();
    const requirements: BatchRequirement[] = [];
    let undoPolicy: UndoPolicy = "full";
    const items: MemoryItem[] = [];

    if (command.kind === "import") {
      if (target.kind !== "sources") {
        fail("invalid_query", "import needs a sources target");
      }
      if (target.paths.length > MAX_BATCH_ITEMS) fail("batch_too_large", "selection exceeds 500");
      if (!command.collectionPath.trim()) fail("invalid_query", "import needs a collection path");
      target.paths.forEach((path, ordinal) => {
        const name = fileNameOf(path);
        const pdf = name.toLowerCase().endsWith(".pdf");
        items.push({
          projection: {
            id: nextId("item"),
            ordinal,
            targetKey: `source:${ordinal}:${name}`,
            paperId: null,
            revisionId: null,
            state: pdf ? "planned" : "skipped",
            outcome: pdf ? null : "skipped_not_pdf",
            attemptCount: 0,
            errorCode: null,
            errorSummary: pdf ? null : "not a PDF",
            retryable: false,
            startedAt: null,
            finishedAt: pdf ? null : timestamp,
            updatedAt: timestamp,
            sourceItemId: null,
          },
        });
      });
    } else {
      const papers = resolvePapers(target);
      if (papers.length === 0) fail("selection_empty", "the selection resolved to no Paper; nothing to do");
      papers.forEach((paper, ordinal) => {
        items.push({
          projection: {
            id: nextId("item"),
            ordinal,
            targetKey: paper.id,
            paperId: paper.id,
            revisionId: paper.revisionId,
            state: "planned",
            outcome: null,
            attemptCount: 0,
            errorCode: null,
            errorSummary: null,
            retryable: false,
            startedAt: null,
            finishedAt: null,
            updatedAt: timestamp,
            sourceItemId: null,
          },
          beforeTags: [...paper.tags],
          beforeCollection: paper.collectionPath,
          beforeRelative: paper.relativePath,
          beforeFileName: paper.fileName,
          beforeLifecycle: {
            lifecycleStatus: paper.lifecycleStatus,
            favorite: paper.favorite,
            priority: paper.priority,
            readLater: paper.readLater,
            reviewAt: paper.reviewAt,
            lifecycleVersion: paper.lifecycleVersion,
          },
        });
      });
      if (command.kind === "trash") {
        requirements.push({
          id: "destructive",
          kind: "destructive",
          itemCount: papers.length,
          label: `${papers.length} Paper(s) move to Trash`,
        });
        undoPolicy = "compensating";
      }
      if (command.kind === "move") {
        const kindChanges = papers.filter((paper) => rootOf(paper.collectionPath) !== rootOf(command.collectionPath)).length;
        if (kindChanges > 0) {
          requirements.push({
            id: "kind_change",
            kind: "kind_change",
            itemCount: kindChanges,
            label: `${kindChanges} Paper(s) change document kind`,
          });
          undoPolicy = "compensating";
        }
      }
      if (command.kind === "ocr" || command.kind === "brief") {
        undoPolicy = "cancel_only";
        const runnable = papers.filter((paper) =>
          command.kind === "ocr" ? !paper.hasOcr : paper.hasOcr && paper.briefStatus !== "ready",
        );
        items.forEach((item, index) => {
          const paper = papers[index];
          if (!paper) return;
          if (command.kind === "ocr" && paper.hasOcr) {
            item.projection.state = "skipped";
            item.projection.outcome = "skipped_existing_ocr";
          } else if (command.kind === "brief" && !paper.hasOcr) {
            item.projection.state = "skipped";
            item.projection.outcome = "skipped_missing_ocr";
          } else if (command.kind === "brief" && paper.briefStatus === "ready") {
            item.projection.state = "skipped";
            item.projection.outcome = "skipped_existing_brief";
          }
        });
        if (runnable.length > 0) {
          requirements.push({
            id: "cost",
            kind: "cost",
            itemCount: runnable.length,
            label: `${runnable.length} item(s) may incur provider charges; preview is not the final bill`,
          });
        }
      }
    }

    const id = nextId("batch");
    const projection: BatchProjection = {
      protocolVersion: LIBRARY_ACT_PROTOCOL_VERSION,
      id,
      commandKind: command.kind,
      parentCommandKind: null,
      parentBatchId: null,
      relation: null,
      state: "planned",
      planDigest: nextId("digest"),
      targetDigest: nextId("target"),
      undoPolicy,
      totalItems: items.length,
      counts: countsFrom(items),
      requirements,
      isCancelling: false,
      planExpiresAt: new Date(Date.now() + 15 * 60 * 1000).toISOString(),
      createdAt: timestamp,
      startedAt: null,
      finishedAt: null,
      updatedAt: timestamp,
      retrySummary: null,
      compensationSummary: null,
      undo: null,
      costPreview: providerCostPreview(command.kind, items.length),
    };
    batches.set(id, { projection, command, items, undoToken: null, undoHash: null });
    return projection;
  };

  function providerCostPreview(kind: BatchCommand["kind"], itemCount: number): CostPreview {
    if (kind !== "ocr" && kind !== "brief") return EMPTY_COST_PREVIEW;
    if (kind === "brief") {
      return {
        ...EMPTY_COST_PREVIEW,
        unknownItemCount: itemCount,
        unknownReasonCodes: itemCount > 0 ? ["custom_endpoint_unpriced"] : [],
      };
    }
    return {
      ...EMPTY_COST_PREVIEW,
      marginalEstimates: itemCount
        ? [{
            confidence: "exact",
            currency: "USD",
            minimum: (itemCount * 0.001).toString(),
            maximum: (itemCount * 0.001).toString(),
            basis: "mistral-ocr-latest · per page from local catalog",
            priceCatalogVersion: "2026-09-01",
          }]
        : [],
    };
  }

  const apply = (batch: MemoryBatch): string | null => {
    const timestamp = nowIso();
    let cards = [...host.cards()];
    const touched = new Set<LibraryDomain>();
    let wrote = false;
    const command = batch.command;

    for (const item of batch.items) {
      if (item.projection.state !== "planned" && item.projection.state !== "queued") continue;
      item.projection.attemptCount += 1;
      item.projection.startedAt = timestamp;
      if (command.kind === "import") {
        const name = item.projection.targetKey.split(":").slice(2).join(":") || "imported.pdf";
        const collection = command.collectionPath;
        const id = nextId("paper");
        const card: HubPaperCard = {
          id,
          revisionId: nextId("rev"),
          title: name.replace(/\.pdf$/i, ""),
          authors: [],
          publicationYear: null,
          pageCount: null,
          collectionPath: collection,
          relativePath: `${collection}/${name}`,
          fileName: name,
          kind: collection.startsWith("Textbooks") ? "textbook" : "paper",
          sha256: `mem-${id}`,
          byteSize: 1,
          importedAt: timestamp,
          tags: [],
          hasOcr: false,
          briefStatus: "missing",
          briefTakeaway: null,
          keywords: [],
          chapterNumber: null,
          sortKey: timestamp,
          ...defaultHubCardLifecycle(),
        };
        cards.push(card);
        item.projection.paperId = id;
        item.projection.revisionId = card.revisionId;
        item.projection.state = "succeeded";
        item.projection.outcome = "created_new";
        item.projection.finishedAt = timestamp;
        touched.add("structure");
        wrote = true;
        continue;
      }
      const paperId = item.projection.paperId;
      const index = cards.findIndex((card) => card.id === paperId);
      if (index < 0) {
        item.projection.state = "failed";
        item.projection.errorCode = "paper_not_found";
        item.projection.errorSummary = "paper is not in the library";
        item.projection.finishedAt = timestamp;
        continue;
      }
      const card = { ...cards[index] };
      if (command.kind === "patch_tags") {
        const tags = new Set(card.tags);
        for (const tag of command.add) tags.add(tag);
        for (const tag of command.remove) {
          for (const current of [...tags]) {
            if (current.toLowerCase() === tag.toLowerCase()) tags.delete(current);
          }
        }
        card.tags = [...tags];
        cards[index] = card;
        touched.add("tags");
        wrote = true;
        item.projection.state = "succeeded";
      } else if (command.kind === "move") {
        card.collectionPath = command.collectionPath;
        card.relativePath = `${command.collectionPath}/${card.fileName}`;
        card.kind = command.collectionPath.startsWith("Textbooks") ? "textbook" : "paper";
        cards[index] = card;
        touched.add("structure");
        wrote = true;
        item.projection.state = "succeeded";
      } else if (command.kind === "trash") {
        cards.splice(index, 1);
        touched.add("structure");
        wrote = true;
        item.projection.state = "succeeded";
      } else if (command.kind === "export") {
        item.projection.state = "succeeded";
        item.projection.outcome = "wrote_new";
        wrote = true;
      } else if (command.kind === "patch_lifecycle") {
        cards[index] = applyLifecyclePatchToCard(card, command.patch, timestamp);
        touched.add("lifecycle");
        wrote = true;
        item.projection.state = "succeeded";
      } else if (command.kind === "ocr") {
        card.hasOcr = true;
        cards[index] = card;
        touched.add("jobs");
        wrote = true;
        item.projection.state = "succeeded";
        item.projection.outcome = "created_job";
      } else if (command.kind === "brief") {
        card.briefStatus = "ready";
        cards[index] = card;
        touched.add("artifacts");
        wrote = true;
        item.projection.state = "succeeded";
        item.projection.outcome = "created_job";
      }
      item.projection.finishedAt = timestamp;
      item.projection.updatedAt = timestamp;
    }

    host.replaceCards(cards);
    if (touched.size > 0) host.bump([...touched]);
    batch.projection.startedAt = timestamp;
    batch.projection.finishedAt = timestamp;
    const token = wrote && (batch.projection.undoPolicy === "full" || batch.projection.undoPolicy === "compensating")
      ? nextId("undo")
      : null;
    batch.undoToken = token;
    batch.undoHash = token ? `hash:${token}` : null;
    batch.projection.undo = token
      ? { state: "available", expiresAt: new Date(Date.now() + 10 * 60 * 1000).toISOString() }
      : null;
    batch.projection = project(batch);
    return token;
  };

  const revert = (batch: MemoryBatch): void => {
    if (!batch.undoToken || batch.projection.undo?.state !== "available") {
      fail("undo_conflict", "undo token is not available");
    }
    let cards = [...host.cards()];
    const touched = new Set<LibraryDomain>();
    const command = batch.command;
    for (const item of [...batch.items].reverse()) {
      if (item.projection.state !== "succeeded") continue;
      if (command.kind === "patch_tags" && item.projection.paperId && item.beforeTags) {
        cards = cards.map((card) =>
          card.id === item.projection.paperId ? { ...card, tags: [...item.beforeTags!] } : card,
        );
        touched.add("tags");
      } else if (command.kind === "move" && item.projection.paperId && item.beforeCollection) {
        cards = cards.map((card) =>
          card.id === item.projection.paperId
            ? {
                ...card,
                collectionPath: item.beforeCollection!,
                relativePath: item.beforeRelative ?? card.relativePath,
                fileName: item.beforeFileName ?? card.fileName,
              }
            : card,
        );
        touched.add("structure");
      } else if (command.kind === "trash" && item.projection.paperId && item.beforeCollection) {
        cards.push({
          id: item.projection.paperId,
          revisionId: item.projection.revisionId ?? item.projection.paperId,
          title: item.beforeFileName?.replace(/\.pdf$/i, "") ?? item.projection.paperId,
          authors: [],
          publicationYear: null,
          pageCount: null,
          collectionPath: item.beforeCollection,
          relativePath: item.beforeRelative ?? `${item.beforeCollection}/${item.beforeFileName ?? "paper.pdf"}`,
          fileName: item.beforeFileName ?? "paper.pdf",
          kind: item.beforeCollection.startsWith("Textbooks") ? "textbook" : "paper",
          sha256: `mem-${item.projection.paperId}`,
          byteSize: 1,
          importedAt: nowIso(),
          tags: item.beforeTags ?? [],
          hasOcr: false,
          briefStatus: "missing",
          briefTakeaway: null,
          keywords: [],
          chapterNumber: null,
          sortKey: nowIso(),
          ...defaultHubCardLifecycle(),
          ...(item.beforeLifecycle ?? {}),
        });
        touched.add("structure");
      } else if (command.kind === "patch_lifecycle" && item.projection.paperId && item.beforeLifecycle) {
        cards = cards.map((card) =>
          card.id === item.projection.paperId ? { ...card, ...item.beforeLifecycle } : card,
        );
        touched.add("lifecycle");
      } else if (command.kind === "import" && item.projection.paperId) {
        cards = cards.filter((card) => card.id !== item.projection.paperId);
        touched.add("structure");
      }
    }
    host.replaceCards(cards);
    if (touched.size > 0) host.bump([...touched]);
    batch.undoToken = null;
    batch.projection.undo = batch.projection.undo
      ? { ...batch.projection.undo, state: "consumed" }
      : null;
    batch.projection = project(batch);
  };

  const act = (request: LibraryActRequest): LibraryActResponse => {
    if (request.protocolVersion !== LIBRARY_ACT_PROTOCOL_VERSION) {
      fail("invalid_query", `unsupported protocolVersion ${request.protocolVersion}`);
    }
    const key = request.idempotencyKey.trim();
    if (!key || key.length > MAX_IDEMPOTENCY_KEY_CHARS) {
      fail("invalid_query", "idempotency key is invalid");
    }
    const digest = digestOf(request);
    const replayed = replay(key, digest);
    if (replayed) return replayed;

    if (request.kind === "change") {
      const result = applyChange(request.change);
      return store(key, digest, { kind: "change", result });
    }
    if (request.kind === "record_reader_activity") {
      const context = applyActivity(request.activity);
      return store(key, digest, { kind: "record_reader_activity", context });
    }
    if (request.kind === "plan_batch") {
      const batch = plan(request.command, request.target);
      return store(key, digest, { kind: "plan_batch", batch });
    }
    if (request.kind === "start_batch") {
      const stored = batches.get(request.batchId);
      if (!stored) fail("batch_not_found", "batch does not exist");
      if (stored.projection.planDigest !== request.planDigest) {
        fail("stale_batch_plan", "planDigest does not match the stored plan; preview again");
      }
      if (stored.projection.state !== "planned") {
        return store(key, digest, { kind: "start_batch", batch: project(stored), undoToken: null });
      }
      const accepted = new Set(request.acceptedRequirementIds);
      for (const requirement of stored.projection.requirements) {
        if (!accepted.has(requirement.id)) {
          fail("confirmation_required", `the plan needs an explicit confirmation: ${requirement.label}`);
        }
      }
      const token = apply(stored);
      return store(key, digest, { kind: "start_batch", batch: project(stored), undoToken: token });
    }
    if (request.kind !== "control_batch") {
      fail("invalid_query", "unsupported library_act request");
    }
    const stored = batches.get(request.batchId);
    if (!stored) fail("batch_not_found", "batch does not exist");
    const control: BatchControl = request.control;
    if (control.kind === "undo") {
      if (stored.undoToken !== control.token) fail("undo_conflict", "undo token does not match");
      revert(stored);
      return store(key, digest, { kind: "control_batch", batch: project(stored), undoToken: null });
    }
    if (control.kind === "cancel_remaining") {
      for (const item of stored.items) {
        if (item.projection.state === "planned" || item.projection.state === "queued") {
          item.projection.state = "cancelled";
          item.projection.finishedAt = nowIso();
        }
      }
      stored.projection.isCancelling = true;
      stored.projection = project(stored);
      return store(key, digest, { kind: "control_batch", batch: project(stored), undoToken: null });
    }
    fail("invalid_query", "retry_failed is not available in the preview adapter");
  };

  const readBatch = (batchId: string): BatchProjection => {
    const stored = batches.get(batchId);
    if (!stored) fail("batch_not_found", "batch does not exist");
    return project(stored);
  };

  const readItems = (batchId: string, afterOrdinal: number | null, limit: number | null): BatchItemsPage => {
    const stored = batches.get(batchId);
    if (!stored) fail("batch_not_found", "batch does not exist");
    const pageSize = limit ?? 100;
    const start = afterOrdinal === null ? -1 : afterOrdinal;
    const items = stored.items
      .map((item) => item.projection)
      .filter((item) => item.ordinal > start)
      .slice(0, pageSize + 1);
    const hasMore = items.length > pageSize;
    const page = items.slice(0, pageSize);
    return {
      protocolVersion: LIBRARY_ACT_PROTOCOL_VERSION,
      batchId,
      pageSize,
      items: page,
      nextOrdinal: hasMore ? page.at(-1)?.ordinal ?? null : null,
      hasMore,
    };
  };

  const readRecent = (limit: number | null): RecentBatchesPage => {
    const pageSize = limit ?? RECENT_BATCHES_DEFAULT;
    if (pageSize < 1 || pageSize > RECENT_BATCHES_MAX) {
      fail("invalid_query", `recent_batches limit must be between 1 and ${RECENT_BATCHES_MAX}`);
    }
    const listed = [...batches.values()]
      .map((batch) => project(batch))
      .sort((left, right) => (left.updatedAt < right.updatedAt ? 1 : -1))
      .slice(0, pageSize);
    return { protocolVersion: LIBRARY_ACT_PROTOCOL_VERSION, pageSize, batches: listed };
  };

  const userSmart: SmartCollectionProjection[] = [];

  const listSmartCollections = (): SmartCollectionProjection[] => [
    ...BUILTIN_SMART_COLLECTIONS,
    ...userSmart,
  ];

  const readingContext = (paperId: string, revisionId: string) => {
    const card = host.cards().find((item) => item.id === paperId);
    const lifecycle = lifecycleFromUnknown(card ?? defaultReadingLifecycle(paperId), paperId);
    if (!card) fail("paper_not_found", `paper ${paperId} is not in the library`);
    const engagement = card.lastOpenedAt
      ? {
          paperId,
          revisionId,
          furthestPage: card.furthestPage ?? 1,
          pageCountSnapshot: card.pageCount ?? 1,
          firstOpenedAt: card.lastOpenedAt,
          lastOpenedAt: card.lastOpenedAt,
          updatedAt: card.lastOpenedAt,
        }
      : null;
    return { lifecycle: { ...lifecycle, paperId, version: card.lifecycleVersion }, engagement };
  };

  const applyChange = (change: LibraryChange): ChangeResult => {
    const timestamp = nowIso();
    if (change.kind === "patch_lifecycle") {
      const cards = [...host.cards()];
      const index = cards.findIndex((card) => card.id === change.paperId);
      if (index < 0) fail("paper_not_found", `paper ${change.paperId} is not in the library`);
      const current = cards[index];
      if (current.lifecycleVersion !== change.expectedVersion) {
        fail("stale_library_snapshot", "the lifecycle version changed; reload before writing");
      }
      cards[index] = applyLifecyclePatchToCard(current, change.patch, timestamp);
      host.replaceCards(cards);
      host.bump(["lifecycle"]);
      return {
        kind: "lifecycle",
        lifecycle: lifecycleFromCard(cards[index]),
      };
    }
    if (change.kind === "create_smart_collection") {
      if (!change.name.trim() || change.filters.length === 0) {
        fail("invalid_query", "a saved smart collection needs a name and at least one filter");
      }
      const collection: SmartCollectionProjection = {
        id: nextId("smart"),
        name: change.name.trim(),
        builtin: false,
        query: { queryVersion: 1, filters: [...change.filters], sort: "recent" },
        createdAt: timestamp,
        updatedAt: timestamp,
      };
      userSmart.unshift(collection);
      host.bump(["smart_collections"]);
      return { kind: "smart_collection", collection };
    }
    if (change.kind === "rename_smart_collection") {
      if (BUILTIN_SMART_COLLECTIONS.some((item) => item.id === change.id)) {
        fail("invalid_query", "built-in smart collections cannot be renamed");
      }
      const index = userSmart.findIndex((item) => item.id === change.id);
      if (index < 0) fail("invalid_query", "smart collection does not exist");
      const collection = { ...userSmart[index], name: change.name.trim(), updatedAt: timestamp };
      userSmart[index] = collection;
      host.bump(["smart_collections"]);
      return { kind: "smart_collection", collection };
    }
    if (BUILTIN_SMART_COLLECTIONS.some((item) => item.id === change.id)) {
      fail("invalid_query", "built-in smart collections cannot be deleted");
    }
    const index = userSmart.findIndex((item) => item.id === change.id);
    if (index < 0) fail("invalid_query", "smart collection does not exist");
    userSmart.splice(index, 1);
    host.bump(["smart_collections"]);
    return { kind: "smart_collection_deleted", id: change.id };
  };

  const applyActivity = (activity: ReaderActivity) => {
    const cards = [...host.cards()];
    const index = cards.findIndex((card) => card.id === activity.paperId);
    if (index < 0) fail("paper_not_found", `paper ${activity.paperId} is not in the library`);
    const card = cards[index];
    const current = lifecycleFromCard(card);
    const previous = card.lastOpenedAt
      ? {
          paperId: activity.paperId,
          revisionId: activity.revisionId,
          furthestPage: card.furthestPage ?? 1,
          pageCountSnapshot: card.pageCount ?? activity.pageCount,
          firstOpenedAt: card.lastOpenedAt,
          lastOpenedAt: card.lastOpenedAt,
          updatedAt: card.lastOpenedAt,
        }
      : null;
    const result = recordReadingActivity({
      lifecycle: current,
      engagement: previous?.revisionId === activity.revisionId ? previous : null,
      paperId: activity.paperId,
      revisionId: activity.revisionId,
      page: activity.pageNumber,
      pageCount: activity.pageCount,
    });
    const touched: LibraryDomain[] = ["engagement"];
    if (result.lifecycle.status !== current.status) touched.push("lifecycle");
    cards[index] = {
      ...card,
      lifecycleStatus: result.lifecycle.status,
      lifecycleVersion: result.lifecycle.version,
      furthestPage: result.engagement.furthestPage,
      lastOpenedAt: result.engagement.lastOpenedAt,
    };
    host.replaceCards(cards);
    host.bump(touched);
    return { lifecycle: result.lifecycle, engagement: result.engagement };
  };

  return { act, readBatch, readItems, readRecent, listSmartCollections, readingContext };
}

function matchesFilters(
  card: HubPaperCard,
  filters: readonly LibraryQueryFilter[],
  evaluationAnchor: string,
): boolean {
  return filters.every((filter) => cardMatchesLibraryFilter(filter, card, evaluationAnchor));
}

function applyLifecyclePatchToCard(
  card: HubPaperCard,
  patch: LifecyclePatch,
  timestamp: string,
): HubPaperCard {
  return {
    ...card,
    lifecycleStatus: patch.status ?? card.lifecycleStatus,
    favorite: patch.favorite ?? card.favorite,
    priority: patch.priority ?? card.priority,
    readLater: patch.readLater ?? card.readLater,
    reviewAt: patch.reviewAt === undefined ? card.reviewAt : patch.reviewAt,
    lifecycleVersion: card.lifecycleVersion + 1,
    lastOpenedAt: card.lastOpenedAt ?? timestamp,
  };
}

function lifecycleFromCard(card: HubPaperCard): ReadingLifecycle {
  return lifecycleFromUnknown(
    {
      paperId: card.id,
      status: card.lifecycleStatus,
      favorite: card.favorite,
      priority: card.priority,
      readLater: card.readLater,
      reviewAt: card.reviewAt,
      version: card.lifecycleVersion,
      updatedAt: card.lastOpenedAt ?? card.importedAt,
    },
    card.id,
    card.lastOpenedAt ?? card.importedAt,
  );
}
