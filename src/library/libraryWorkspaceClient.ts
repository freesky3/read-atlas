import { captureOperationError } from "../i18n/errors";
// Library Workspace 的桌面 seam（ADR D-063 §4.2）。
//
// Hub 只认识 `LibraryWorkspaceClient`：`read` 是有界投影，`act` 是持久 Batch，
// `watch` 是失效信号。Tauri 事件名仍是 `read-event`。

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  toLibraryActError,
  type LibraryActRequest,
  type LibraryActResponse,
} from "./libraryActTypes";
import { createMemoryActEngine } from "./memoryLibraryAct";
import {
  cardMatchesLibraryFilter,
  normalizeLibraryCollectionPath,
} from "./hubFilters";
import { chapterSortKey } from "../hubSort";
import {
  HUB_DEFAULT_PAGE_SIZE,
  HUB_MAX_FILTERS,
  HUB_MAX_PAGE_SIZE,
  HUB_PAGE_DEPENDENCIES,
  LIBRARY_PROTOCOL_VERSION,
  EVALUATION_TIMEZONE,
  MAX_TAG_NAME_LENGTH,
  MAX_TEXT_TERM_LENGTH,
  decodeLibraryCursor,
  defaultHubCardLifecycle,
  encodeLibraryCursor,
  formatDependencyRevision,
  hubPageQuery,
  selectionDependencyDomains,
  toLibraryQueryError,
  watchHandshakeRequest,
  type DomainRevision,
  type LibraryDomain,
  type LibraryHubPageQuery,
  type LibraryQueryError,
  type LibraryQueryFilter,
  type LibraryReadRequest,
  type LibraryReadResult,
  type LibraryRevision,
  type LibraryWatchHandle,
  type HubPageResult,
  type HubPaperCard,
} from "./libraryWorkspaceTypes";

export interface LibraryWorkspaceClient {
  readonly runtime: "desktop" | "memory";
  read(request: LibraryReadRequest): Promise<LibraryReadResult>;
  act(request: LibraryActRequest): Promise<LibraryActResponse>;
  /**
   * `after` 是调用方手上快照的全局 revision，`null` 表示还没有快照。握手之后
   * 只有全局 revision 真的前进才通知（§10.3）；「本页依赖有没有变」由调用方比较
   * `HubPageResult.dependencyRevision` 决定。
   */
  watch(
    after: LibraryRevision | null,
    listener: (revision: LibraryRevision) => void,
  ): Promise<LibraryWatchHandle>;
}

/** 抛出的就是线格式上的 typed error，不包一层 `Error` 把 code 弄丢。 */
function invalidQuery(message: string): never {
  const error: LibraryQueryError = { code: "invalid_query", message };
  throw error;
}

function assertProtocol(request: LibraryReadRequest): void {
  if (request.protocolVersion !== LIBRARY_PROTOCOL_VERSION) {
    invalidQuery(
      `unsupported protocolVersion ${request.protocolVersion}, expected ${LIBRARY_PROTOCOL_VERSION}`,
    );
  }
}

type ReadEventPayload = Readonly<{ kind?: unknown }>;

export function createTauriLibraryWorkspaceClient(): LibraryWorkspaceClient {
  const read = async (
    request: LibraryReadRequest,
  ): Promise<LibraryReadResult> => {
    assertProtocol(request);
    try {
      // `library_read` 的全部输入就是这个判别联合，没有 `{ command, payload }` 形状。
      return await invoke<LibraryReadResult>("library_read", { request });
    } catch (error) {
      throw captureOperationError("library_read", toLibraryQueryError(error));
    }
  };

  return {
    runtime: "desktop",
    read,
    async act(request) {
      if (request.protocolVersion !== LIBRARY_PROTOCOL_VERSION) {
        invalidQuery(
          `unsupported protocolVersion ${request.protocolVersion}, expected ${LIBRARY_PROTOCOL_VERSION}`,
        );
      }
      try {
        return await invoke<LibraryActResponse>("library_act", { request });
      } catch (error) {
        throw captureOperationError("library_act", toLibraryActError(error));
      }
    },
    async watch(after, listener) {
      let currentRevision: LibraryRevision = after ?? 0;
      let closed = false;
      let armed = false;
      let pending = false;
      // 事件可以连珠炮地到，握手必须一个接一个：否则后到的旧答复会盖掉新 revision。
      let tail = Promise.resolve();

      const refresh = () => {
        tail = tail.then(async () => {
          if (closed) return;
          try {
            const next = revisionOf(
              await read(watchHandshakeRequest(currentRevision)),
            );
            if (closed || next === currentRevision) return;
            currentRevision = next;
            listener(next);
          } catch {
            // 读不到库不是「没有变化」，保持原快照，等下一次事件或用户动作。
          }
        });
      };

      // §10.3：先订阅并缓冲，再握手。listen 之前到达的事件本来就听不见；
      // 握手窗口里到达的事件必须在 snapshot 之后重放，不能丢掉。
      const unlisten: UnlistenFn = await listen<ReadEventPayload>(
        "read-event",
        (event) => {
          if (event.payload === null || typeof event.payload !== "object")
            return;
          if (!armed) {
            pending = true;
            return;
          }
          refresh();
        },
      );

      try {
        const handshake = await read(watchHandshakeRequest(after));
        currentRevision = revisionOf(handshake);
        armed = true;
        if (pending) refresh();
      } catch (error) {
        closed = true;
        unlisten();
        throw error;
      }

      return {
        get currentRevision() {
          return currentRevision;
        },
        unsubscribe: () => {
          closed = true;
          unlisten();
        },
      };
    },
  };
}

function revisionOf(result: LibraryReadResult): LibraryRevision {
  if (result.kind === "hub_page") return result.page.revision;
  if (result.kind === "watch_handshake") return result.currentRevision;
  return 0;
}

export type MemoryLibrarySeed = Readonly<{
  papers?: readonly HubPaperCard[];
  /** 各物理目录的手动顺序，键为 collection 路径；缺失即该目录没有手动顺序。 */
  manualOrders?: Readonly<Record<string, readonly string[]>>;
  domainRevisions?: Readonly<Partial<Record<LibraryDomain, number>>>;
}>;

/**
 * 浏览器预览用的投影：语义与 `hub_page` 对齐（有界分页、keyset、依赖向量、握手
 * 规则），实现刻意简单。它不是第二份权威——真实筛选永远以桌面端为准。
 */
export interface MemoryLibraryWorkspaceClient extends LibraryWorkspaceClient {
  write(domains: readonly LibraryDomain[], mutate: () => void): LibraryRevision;
  addCards(cards: readonly HubPaperCard[]): LibraryRevision;
  setManualOrder(
    collectionPath: string,
    paperIds: readonly string[],
  ): LibraryRevision;
  cards(): readonly HubPaperCard[];
}

const YEAR_SHIFT = 2_147_483_648;
// Rust 的 manual 排序键对「没有 position 的行」用 i64::MAX。
const MAX_POSITION = "9223372036854775807";

function zeroPadded(value: number): string {
  return String(value).padStart(10, "0");
}

/**
 * 逐码点比较：SQLite 的 TEXT 默认 BINARY collation，而 `localeCompare` 会折叠大小写
 * 与变音符，两种 runtime 就会给出不同的插入位置。
 */
function compareBinary(left: string, right: string): number {
  if (left === right) return 0;
  return left < right ? -1 : 1;
}

/**
 * 与 Rust `HubSort::key_expression` 同形的排序键：同样是字符串比较，同样把缺失值
 * 挤到一端。预览端不需要 SQL，但必须和桌面端给出**同一个顺序**，否则拖拽插入线
 * 在两种 runtime 下语义不同。
 */
function memorySortKey(
  sort: LibraryHubPageQuery["sort"],
  card: HubPaperCard,
  manualOrders: Readonly<Record<string, readonly string[]>>,
  descending: boolean,
): string {
  switch (sort) {
    case "year":
      return zeroPadded(
        ((card.displayMetadata
          ? card.displayMetadata.year
          : card.publicationYear) ?? (descending ? -YEAR_SHIFT : YEAR_SHIFT)) +
          YEAR_SHIFT,
      );
    case "title":
      return card.title.toLowerCase();
    case "manual": {
      const position = (manualOrders[card.collectionPath] ?? []).indexOf(
        card.id,
      );
      return position === -1 ? MAX_POSITION : zeroPadded(position);
    }
    case "last_opened":
      return card.lastOpenedAt ?? "";
    case "chapter":
      return descending && chapterSortKey(card.chapterNumber).startsWith("~")
        ? ""
        : chapterSortKey(card.chapterNumber);
    case "recent":
      return card.importedAt;
  }
}

/** `serde_json` 的 Map 是 BTreeMap：规范 JSON 即键升序、无空白。 */
function canonicalQueryJson(
  page: LibraryHubPageQuery,
  pageSize: number,
): string {
  return stableStringify({
    filters: page.filters,
    limit: pageSize,
    protocolVersion: LIBRARY_PROTOCOL_VERSION,
    sort: page.sort,
    ...(page.direction ? { direction: page.direction } : {}),
  });
}

function stableStringify(value: unknown): string {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(stableStringify).join(",")}]`;
  const entries = Object.entries(value as Record<string, unknown>)
    .filter(([, entry]) => entry !== undefined)
    .sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0));
  return `{${entries
    .map(([key, entry]) => `${JSON.stringify(key)}:${stableStringify(entry)}`)
    .join(",")}}`;
}

/**
 * 预览端没有 SQLite，digest 不可能是桌面端那串 sha256，所以刻意带 `mem-` 前缀，
 * 任何跨 runtime 直接比较 digest 值的代码都会立刻看出来。合同测试只断言 digest 的
 * **协议性质**：同一查询稳定、查询 AST 变化即变、cursor 与它绑定。
 */
function memoryDigest(page: LibraryHubPageQuery, pageSize: number): string {
  return memoryFingerprint(canonicalQueryJson(page, pageSize));
}

/** §5.1 的成员摘要：形状与 `library_query.rs::selection_digest` 一致，只有 filters。 */
function memorySelectionDigest(filters: readonly LibraryQueryFilter[]): string {
  return memoryFingerprint(
    stableStringify({ filters, protocolVersion: LIBRARY_PROTOCOL_VERSION }),
  );
}

function memoryFingerprint(canonical: string): string {
  let hash = 0xcbf29ce484222325n;
  for (const character of canonical) {
    hash ^= BigInt(character.codePointAt(0) ?? 0);
    hash = (hash * 0x100000001b3n) & 0xffffffffffffffffn;
  }
  return `mem-${hash.toString(16).padStart(16, "0")}`;
}

function resolveMemoryPageSize(limit: number | null): number {
  const pageSize = limit ?? HUB_DEFAULT_PAGE_SIZE;
  if (pageSize < 1 || pageSize > HUB_MAX_PAGE_SIZE) {
    invalidQuery(
      `limit must be between 1 and ${HUB_MAX_PAGE_SIZE}, default ${HUB_DEFAULT_PAGE_SIZE}`,
    );
  }
  return pageSize;
}

function validateMemoryQuery(query: LibraryHubPageQuery): void {
  if (query.direction && !["asc", "desc"].includes(query.direction))
    invalidQuery("Invalid sort direction");
  if (query.filters.length > HUB_MAX_FILTERS) {
    invalidQuery(`hub_page accepts at most ${HUB_MAX_FILTERS} filters`);
  }
  const hasExactCollection = query.filters.some(
    (filter) => filter.kind === "collection" && !filter.recursive,
  );
  if (
    (query.sort === "manual" || query.sort === "chapter") &&
    !hasExactCollection
  ) {
    invalidQuery(
      "manual and chapter order are only defined inside one collection; pass an exact collection filter",
    );
  }
  for (const filter of query.filters) {
    switch (filter.kind) {
      case "collection":
        if (normalizeLibraryCollectionPath(filter.path) === null) {
          invalidQuery("collection path must stay under Papers or Textbooks");
        }
        break;
      case "text": {
        const term = filter.term.trim();
        if (term === "") invalidQuery("text term cannot be empty");
        if (term.length > MAX_TEXT_TERM_LENGTH) {
          invalidQuery(
            `text term is limited to ${MAX_TEXT_TERM_LENGTH} characters`,
          );
        }
        break;
      }
      case "tag": {
        const tag = filter.tag.trim();
        if (tag === "") invalidQuery("tag cannot be empty");
        if (tag.length > MAX_TAG_NAME_LENGTH) {
          invalidQuery(`tag is limited to ${MAX_TAG_NAME_LENGTH} characters`);
        }
        break;
      }
      case "document":
        break;
      case "status":
        if (
          !["unread", "reading", "read", "done", "completed"].includes(
            filter.value,
          )
        ) {
          invalidQuery("status must be unread, reading, or read");
        }
        break;
      case "favorite":
      case "read_later":
      case "review_due":
      case "ocr_failed":
      case "opened":
        break;
      case "imported":
        if (filter.relative !== "this_week") {
          invalidQuery("imported.relative only accepts this_week");
        }
        break;
    }
  }
}

export function createMemoryLibraryWorkspaceClient(
  seed: MemoryLibrarySeed = {},
): MemoryLibraryWorkspaceClient {
  let cards: HubPaperCard[] = (seed.papers ?? []).map((card) => ({
    ...defaultHubCardLifecycle(),
    ...card,
  }));
  let manualOrders: Record<string, readonly string[]> = {
    ...(seed.manualOrders ?? {}),
  };
  let global: LibraryRevision = 0;
  const counters = new Map<LibraryDomain, number>();
  for (const domain of HUB_PAGE_DEPENDENCIES) {
    counters.set(domain, seed.domainRevisions?.[domain] ?? 0);
  }
  const listeners = new Set<(revision: LibraryRevision) => void>();

  const bump = (touched: readonly LibraryDomain[]): LibraryRevision => {
    global += 1;
    for (const domain of new Set(touched)) {
      counters.set(domain, (counters.get(domain) ?? 0) + 1);
    }
    for (const listener of [...listeners]) listener(global);
    return global;
  };

  const actEngine = createMemoryActEngine({
    cards: () => cards,
    replaceCards: (next) => {
      cards = next;
    },
    bump,
    selectionDigest: memorySelectionDigest,
    domainValue: (domain) => counters.get(domain) ?? 0,
  });

  const hubPage = (query: LibraryHubPageQuery): HubPageResult => {
    // 判定顺序与 `hub_page_on_connection` 一致：分页 → 条数 → manual 适用范围 → 逐条筛选。
    const pageSize = resolveMemoryPageSize(query.limit);
    validateMemoryQuery(query);
    const digest = memoryDigest(query, pageSize);
    const descending = query.direction
      ? query.direction === "desc"
      : query.sort === "recent" ||
        query.sort === "year" ||
        query.sort === "last_opened";
    const keyOf = (card: HubPaperCard) =>
      memorySortKey(query.sort, card, manualOrders, descending);
    const evaluationAnchor = new Date().toISOString();

    const matching = cards.filter((card) =>
      query.filters.every((filter) =>
        cardMatchesLibraryFilter(filter, card, evaluationAnchor),
      ),
    );
    const ordered = [...matching].sort((left, right) => {
      const comparison = compareBinary(keyOf(left), keyOf(right));
      if (comparison !== 0) return descending ? -comparison : comparison;
      return compareBinary(left.id, right.id);
    });

    let rows = ordered;
    if (query.cursor !== null) {
      const token = decodeLibraryCursor(query.cursor);
      if (token === null || token.digest !== digest) {
        invalidQuery(
          "cursor was produced by a different query; re-read from the first page",
        );
      }
      rows = ordered.filter((card) => {
        const comparison = compareBinary(keyOf(card), token.key);
        if (comparison !== 0)
          return descending ? comparison < 0 : comparison > 0;
        // id 决胜永远升序，与 Rust 的 `p.id > ?` 同向。
        return card.id > token.id;
      });
    }

    const fetched = rows.slice(0, pageSize + 1);
    const hasMore = fetched.length > pageSize;
    const papers = fetched.slice(0, pageSize).map((card) => ({
      ...card,
      sortKey: keyOf(card),
    }));
    const last = papers.at(-1);
    const dependencies = [...HUB_PAGE_DEPENDENCIES]
      .sort(compareBinary)
      .map((domain): DomainRevision => ({
        domain,
        value: counters.get(domain) ?? 0,
      }));
    return {
      protocolVersion: LIBRARY_PROTOCOL_VERSION,
      revision: global,
      dependencies,
      dependencyRevision: formatDependencyRevision(dependencies),
      queryDigest: digest,
      sort: query.sort,
      pageSize,
      totalCount: matching.length,
      papers,
      nextCursor:
        hasMore && last
          ? encodeLibraryCursor({ digest, key: keyOf(last), id: last.id })
          : null,
      hasMore,
      selectionDigest: memorySelectionDigest(query.filters),
      selectionDependencies: [...selectionDependencyDomains(query.filters)]
        .sort(compareBinary)
        .map((domain): DomainRevision => ({
          domain,
          value: counters.get(domain) ?? 0,
        })),
      evaluationAnchor,
      evaluationTimezone: EVALUATION_TIMEZONE,
    };
  };

  return {
    runtime: "memory",
    async read(request) {
      assertProtocol(request);
      if (request.kind === "hub_page") {
        return { kind: "hub_page", page: hubPage(hubPageQuery(request.page)) };
      }
      if (request.kind === "watch_handshake") {
        return {
          kind: "watch_handshake",
          currentRevision: global,
          invalidated: !(request.after !== null && request.after >= global),
        };
      }
      if (request.kind === "smart_collections") {
        return {
          kind: "smart_collections",
          collections: actEngine.listSmartCollections(),
        };
      }
      if (request.kind === "reading_context") {
        return {
          kind: "reading_context",
          context: actEngine.readingContext(
            request.paperId,
            request.revisionId,
          ),
        };
      }
      if (request.kind === "collection_layer") {
        const path = normalizeLibraryCollectionPath(request.collectionPath);
        if (path === null)
          invalidQuery("collection path must stay under Papers or Textbooks");
        const matching = cards.filter((card) => card.collectionPath === path);
        const order = manualOrders[path] ?? [];
        const ordered = [...matching].sort((left, right) => {
          const li = order.indexOf(left.id);
          const ri = order.indexOf(right.id);
          if (li === -1 && ri === -1)
            return left.importedAt.localeCompare(right.importedAt);
          if (li === -1) return 1;
          if (ri === -1) return -1;
          return li - ri;
        });
        return {
          kind: "collection_layer",
          collectionId: path,
          paperIds: ordered.map((card) => card.id),
        };
      }
      try {
        if (request.kind === "batch") {
          return { kind: "batch", batch: actEngine.readBatch(request.batchId) };
        }
        if (request.kind === "batch_items") {
          return {
            kind: "batch_items",
            page: actEngine.readItems(
              request.batchId,
              request.afterOrdinal,
              request.limit,
            ),
          };
        }
        return {
          kind: "recent_batches",
          page: actEngine.readRecent(request.limit),
        };
      } catch (error) {
        throw toLibraryQueryError(error);
      }
    },
    async act(request) {
      assertProtocol({
        kind: "watch_handshake",
        protocolVersion: request.protocolVersion,
        after: null,
      });
      return actEngine.act(request);
    },
    async watch(_after, listener) {
      listeners.add(listener);
      return {
        get currentRevision() {
          return global;
        },
        unsubscribe: () => {
          listeners.delete(listener);
        },
      };
    },
    write(touched, mutate) {
      mutate();
      return bump(touched);
    },
    addCards(next) {
      cards = [...cards, ...next];
      return bump(["structure"]);
    },
    setManualOrder(collectionPath, paperIds) {
      manualOrders = { ...manualOrders, [collectionPath]: [...paperIds] };
      return bump(["sort"]);
    },
    cards() {
      return cards;
    },
  };
}

const hasTauriRuntime = Boolean(
  (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__,
);

/** 计划 §4.1 的注入点：`useLibraryWorkspace({ client: libraryWorkspaceClient })`。 */
export const libraryWorkspaceClient: LibraryWorkspaceClient = hasTauriRuntime
  ? createTauriLibraryWorkspaceClient()
  : createMemoryLibraryWorkspaceClient();
