// Library Workspace 读取侧的线格式合同（ADR D-063 §4.2 / §10.1）。
//
// 这里的类型是 `src-tauri/src/library_query.rs` 的镜像，不是前端内部模型：
// 字段名、判别 tag 与 `kind` 取值都必须逐字对齐，任何一侧改名都会同时打破
// Rust 与 TS 两侧的 `__fixtures__/library_read_v1.json` 合同测试。
//
// 线格式镜像 `library_query.rs`。`chapter` 排序与 `collection_layer` 属于 PR 6：
// 前者要精确 collection 过滤，后者给出 D-062 整层 live id。Selection Snapshot
// 摘要（`selectionDigest` / `selectionDependencies`）由 `hub_page` 给出。

/** §4.2：每个 read / act 请求都带协议版本，不匹配即拒绝，不猜意思。 */
export const LIBRARY_PROTOCOL_VERSION = 1;
/** 投影只用 UTC 求值相对时间谓词，与 `library_query.rs` 的常量同值。 */
export const EVALUATION_TIMEZONE = "UTC";
/** §10.1：Hub 默认一页 100 张，硬上限 200 张。 */
export const HUB_DEFAULT_PAGE_SIZE = 100;
export const HUB_MAX_PAGE_SIZE = 200;
/** 请求上限与 `library_query.rs` 的常量同值；越界一律 `invalid_query`。 */
export const HUB_MAX_FILTERS = 8;
export const MAX_TEXT_TERM_LENGTH = 200;
export const MAX_TAG_NAME_LENGTH = 64;

export type LibraryRevision = number;

/** 与 Rust `LibraryDomain::as_str()` 一一对应的数据库域名。 */
export type LibraryDomain =
  | "structure"
  | "tags"
  | "lifecycle"
  | "engagement"
  | "artifacts"
  | "jobs"
  | "smart_collections"
  | "sort";

/** `library_domain_revisions` 的合法键集合（schema 8 CHECK 约束）。 */
export const ALL_LIBRARY_DOMAINS: readonly LibraryDomain[] = [
  "artifacts",
  "engagement",
  "jobs",
  "lifecycle",
  "smart_collections",
  "sort",
  "structure",
  "tags",
];

/**
 * `hub_page` 真正读到的域。卡片带 lifecycle / engagement，这两域的写必须让 Hub 失效。
 * `smart_collections` 只影响侧栏查询列表，不进这一页卡片的依赖向量。
 */
export const HUB_PAGE_DEPENDENCIES: readonly LibraryDomain[] = [
  "structure",
  "tags",
  "lifecycle",
  "engagement",
  "artifacts",
  "jobs",
  "sort",
];

/**
 * §5.1：成员集合真正依赖的域，与 `library_query.rs::selection_dependencies` 同规则。
 * 只声明筛选真正读到的域：给纯目录选择挂上 tags / lifecycle，等于让一次无关写入把选择判成漂移。
 */
export function selectionDependencyDomains(
  filters: readonly LibraryQueryFilter[],
): readonly LibraryDomain[] {
  const domains: LibraryDomain[] = ["structure"];
  if (filters.some((filter) => filter.kind === "tag")) domains.push("tags");
  if (
    filters.some(
      (filter) =>
        filter.kind === "status" ||
        filter.kind === "favorite" ||
        filter.kind === "read_later" ||
        filter.kind === "review_due",
    )
  ) {
    domains.push("lifecycle");
  }
  if (filters.some((filter) => filter.kind === "opened"))
    domains.push("engagement");
  if (filters.some((filter) => filter.kind === "ocr_failed")) {
    domains.push("artifacts");
    domains.push("jobs");
  }
  return domains;
}

export type DomainRevision = Readonly<{ domain: LibraryDomain; value: number }>;

export type LibraryDocumentKind = "paper" | "textbook";

export type LibraryHubSort =
  "recent" | "year" | "title" | "manual" | "last_opened" | "chapter";

export type LibraryQueryErrorCode = "invalid_query" | "workspace_unavailable";

/** §10.2：错误是 typed projection，UI 永远拿不到 SQLite / Provider 原文。 */
export type LibraryQueryError = Readonly<{
  code: LibraryQueryErrorCode;
  message: string;
}>;

/** 筛选 AST：每个变体的字段名与 Rust `QueryFilter` 逐字一致。 */
export type LibraryQueryFilter =
  | Readonly<{ kind: "collection"; path: string; recursive: boolean }>
  | Readonly<{ kind: "text"; term: string }>
  | Readonly<{ kind: "tag"; tag: string }>
  | Readonly<{ kind: "document"; value: LibraryDocumentKind }>
  | Readonly<{
      kind: "status";
      value: "unread" | "reading" | "read" | "done" | "completed";
    }>
  | Readonly<{ kind: "favorite"; value: boolean }>
  | Readonly<{ kind: "read_later"; value: boolean }>
  | Readonly<{ kind: "review_due" }>
  | Readonly<{ kind: "imported"; relative: "this_week" }>
  | Readonly<{ kind: "ocr_failed" }>
  | Readonly<{ kind: "opened" }>;

/**
 * Rust 侧 `deny_unknown_fields`：拼错的筛选字段必须失败，静默忽略等于把一次
 * `all_matching` 快照悄悄放大成整个文库。四个字段都显式给出，不留 `undefined`。
 */
export type LibraryHubPageQuery = Readonly<{
  filters: readonly LibraryQueryFilter[];
  sort: LibraryHubSort;
  direction?: "asc" | "desc";
  limit: number | null;
  cursor: string | null;
}>;

export type LibraryReadRequest =
  | Readonly<{
      kind: "hub_page";
      protocolVersion: number;
      page: LibraryHubPageQuery;
    }>
  | Readonly<{
      kind: "watch_handshake";
      protocolVersion: number;
      after: LibraryRevision | null;
    }>
  | Readonly<{ kind: "batch"; protocolVersion: number; batchId: string }>
  | Readonly<{
      kind: "batch_items";
      protocolVersion: number;
      batchId: string;
      afterOrdinal: number | null;
      states: readonly string[] | null;
      limit: number | null;
    }>
  | Readonly<{
      kind: "recent_batches";
      protocolVersion: number;
      limit: number | null;
    }>
  | Readonly<{ kind: "smart_collections"; protocolVersion: number }>
  | Readonly<{
      kind: "reading_context";
      protocolVersion: number;
      paperId: string;
      revisionId: string;
    }>
  | Readonly<{
      kind: "collection_layer";
      protocolVersion: number;
      collectionPath: string;
    }>;

/** 一页卡片：只有相对路径与脱敏摘要，没有绝对路径、Key 或 route（§10.2）。 */
export type HubPaperCard = Readonly<{
  id: string;
  revisionId: string;
  title: string;
  authors: readonly string[];
  publicationYear: number | null;
  displayMetadata?: {
    year: number | null;
    yearLabel: string;
    titleSource?: string;
    authorsSource?: string;
  };
  pageCount: number | null;
  collectionPath: string;
  relativePath: string;
  fileName: string;
  kind: LibraryDocumentKind;
  sha256: string;
  byteSize: number;
  importedAt: string;
  tags: readonly string[];
  hasOcr: boolean;
  /**
   * `artifacts.status` 没有 CHECK 约束，所以线格式上是开放字符串。已知取值：
   * `missing` / `queued` / `ready` / `stale` / `failed`。
   */
  briefStatus: string;
  briefTakeaway: string | null;
  keywords: readonly string[];
  chapterNumber: string | null;
  /** evaluation anchor：本页最后一行的排序键，与 `nextCursor` 同源。 */
  sortKey: string;
  lifecycleStatus: string;
  favorite: boolean;
  priority: number;
  readLater: boolean;
  reviewAt: string | null;
  lifecycleVersion: number;
  furthestPage: number | null;
  lastOpenedAt: string | null;
  ocrFailed: boolean;
}>;

export function defaultHubCardLifecycle(): Pick<
  HubPaperCard,
  | "lifecycleStatus"
  | "favorite"
  | "priority"
  | "readLater"
  | "reviewAt"
  | "lifecycleVersion"
  | "furthestPage"
  | "lastOpenedAt"
  | "ocrFailed"
> {
  return {
    lifecycleStatus: "unread",
    favorite: false,
    priority: 0,
    readLater: false,
    reviewAt: null,
    lifecycleVersion: 0,
    furthestPage: null,
    lastOpenedAt: null,
    ocrFailed: false,
  };
}

export type SmartQuery = Readonly<{
  queryVersion: number;
  filters: readonly LibraryQueryFilter[];
  sort: string;
}>;

export type SmartCollectionProjection = Readonly<{
  id: string;
  name: string;
  builtin: boolean;
  query: SmartQuery;
  createdAt: string | null;
  updatedAt: string | null;
}>;

export type ReadingContextProjection = Readonly<{
  lifecycle: import("../readerReadingState").ReadingLifecycle;
  engagement: import("../readerReadingState").ReadingEngagement | null;
}>;

export const SMART_COLLECTION_PREFIX = "smart:";

export const BUILTIN_SMART_COLLECTIONS: readonly SmartCollectionProjection[] = [
  {
    id: "unread_this_week",
    name: "本周导入但未读",
    builtin: true,
    query: {
      queryVersion: 1,
      filters: [
        { kind: "imported", relative: "this_week" },
        { kind: "status", value: "unread" },
      ],
      sort: "recent",
    },
    createdAt: null,
    updatedAt: null,
  },
  {
    id: "reading",
    name: "阅读中",
    builtin: true,
    query: {
      queryVersion: 1,
      filters: [{ kind: "status", value: "reading" }],
      sort: "recent",
    },
    createdAt: null,
    updatedAt: null,
  },
  {
    id: "read_later",
    name: "稍后阅读",
    builtin: true,
    query: {
      queryVersion: 1,
      filters: [{ kind: "read_later", value: true }],
      sort: "recent",
    },
    createdAt: null,
    updatedAt: null,
  },
  {
    id: "review_due",
    name: "需要复习",
    builtin: true,
    query: {
      queryVersion: 1,
      filters: [{ kind: "review_due" }],
      sort: "recent",
    },
    createdAt: null,
    updatedAt: null,
  },
  {
    id: "ocr_failed",
    name: "OCR 失败",
    builtin: true,
    query: {
      queryVersion: 1,
      filters: [{ kind: "ocr_failed" }],
      sort: "recent",
    },
    createdAt: null,
    updatedAt: null,
  },
  {
    id: "recently_opened",
    name: "最近打开",
    builtin: true,
    query: {
      queryVersion: 1,
      filters: [{ kind: "opened" }],
      sort: "last_opened",
    },
    createdAt: null,
    updatedAt: null,
  },
];

export function smartCollectionIdOf(folder: string): string | null {
  return folder.startsWith(SMART_COLLECTION_PREFIX)
    ? folder.slice(SMART_COLLECTION_PREFIX.length)
    : null;
}

export function smartCollectionFolder(id: string): string {
  return `${SMART_COLLECTION_PREFIX}${id}`;
}

export type HubPageResult = Readonly<{
  protocolVersion: number;
  /** 全局 `library_change_seq`，即 watch 握手用的 revision。 */
  revision: LibraryRevision;
  dependencies: readonly DomainRevision[];
  dependencyRevision: string;
  queryDigest: string;
  sort: LibraryHubSort;
  pageSize: number;
  /** 整个 query 的命中篇数，不随 cursor 变化。 */
  totalCount: number;
  papers: readonly HubPaperCard[];
  nextCursor: string | null;
  hasMore: boolean;
  /**
   * §5.1 Selection Snapshot 的成员摘要：只覆盖 filters。`queryDigest` 标识一次
   * 分页查询（含 sort / limit），换排序不许把已经确认过的 `all_matching` 判成漂移。
   */
  selectionDigest: string;
  selectionDependencies: readonly DomainRevision[];
  /** 相对时间谓词的锚点由后端给出：成员集合在确认前后必须用同一个时刻。 */
  evaluationAnchor: string;
  evaluationTimezone: string;
}>;

export type LibraryReadResult =
  | Readonly<{ kind: "hub_page"; page: HubPageResult }>
  | Readonly<{
      kind: "watch_handshake";
      currentRevision: LibraryRevision;
      invalidated: boolean;
    }>
  | Readonly<{
      kind: "batch";
      batch: import("./libraryActTypes").BatchProjection;
    }>
  | Readonly<{
      kind: "batch_items";
      page: import("./libraryActTypes").BatchItemsPage;
    }>
  | Readonly<{
      kind: "recent_batches";
      page: import("./libraryActTypes").RecentBatchesPage;
    }>
  | Readonly<{
      kind: "smart_collections";
      collections: readonly SmartCollectionProjection[];
    }>
  | Readonly<{ kind: "reading_context"; context: ReadingContextProjection }>
  | Readonly<{
      kind: "collection_layer";
      collectionId: string | null;
      paperIds: readonly string[];
    }>;

export type LibraryWatchHandle = Readonly<{
  currentRevision: LibraryRevision;
  unsubscribe: () => void;
}>;

/** `hub_page` 之外的请求默认值集中在这里，避免调用方各写一套空数组。 */
export function hubPageQuery(
  override: Partial<LibraryHubPageQuery> = {},
): LibraryHubPageQuery {
  return {
    filters: override.filters ?? [],
    sort: override.sort ?? "recent",
    ...(override.direction ? { direction: override.direction } : {}),
    limit: override.limit ?? null,
    cursor: override.cursor ?? null,
  };
}

export function hubPageRequest(
  page: Partial<LibraryHubPageQuery> = {},
): LibraryReadRequest {
  return {
    kind: "hub_page",
    protocolVersion: LIBRARY_PROTOCOL_VERSION,
    page: hubPageQuery(page),
  };
}

export function watchHandshakeRequest(
  after: LibraryRevision | null,
): LibraryReadRequest {
  return {
    kind: "watch_handshake",
    protocolVersion: LIBRARY_PROTOCOL_VERSION,
    after,
  };
}

/** 只有判别联合的 `kind` 才能决定其余字段，禁止 `{ command, payload }` 形状。 */
export function isLibraryReadResult(
  value: unknown,
): value is LibraryReadResult {
  if (typeof value !== "object" || value === null) return false;
  const kind = (value as { kind?: unknown }).kind;
  return (
    kind === "hub_page" ||
    kind === "watch_handshake" ||
    kind === "batch" ||
    kind === "batch_items" ||
    kind === "recent_batches" ||
    kind === "smart_collections" ||
    kind === "reading_context"
  );
}

/** IPC 拒绝值可能是 typed error，也可能是 Tauri 传回的字符串。 */
export function isLibraryQueryError(
  value: unknown,
): value is LibraryQueryError {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as { code?: unknown; message?: unknown };
  return (
    (candidate.code === "invalid_query" ||
      candidate.code === "workspace_unavailable") &&
    typeof candidate.message === "string"
  );
}

export function toLibraryQueryError(value: unknown): LibraryQueryError {
  if (isLibraryQueryError(value)) return value;
  const message =
    typeof value === "string"
      ? value
      : value instanceof Error
        ? value.message
        : "unknown error";
  // 无法识别的拒绝一律按「读不到库」处理，绝不当成「查询结果为空」。
  return { code: "workspace_unavailable", message };
}

/**
 * 依赖向量：`domain=value;…`，按域名字典序、去重。排序规则与 Rust
 * `LibraryRevisions::dependency_vector` 一致——它是跨语言比较的字符串，不能按
 * 枚举声明顺序排。
 */
export function formatDependencyRevision(
  entries: readonly DomainRevision[],
): string {
  const sorted = [...entries]
    .filter((entry) => ALL_LIBRARY_DOMAINS.includes(entry.domain))
    .sort((a, b) => (a.domain < b.domain ? -1 : a.domain > b.domain ? 1 : 0));
  const deduped: DomainRevision[] = [];
  for (const entry of sorted) {
    if (deduped.at(-1)?.domain !== entry.domain) deduped.push(entry);
  }
  return deduped.map((entry) => `${entry.domain}=${entry.value}`).join(";");
}

/** 解析失败（未知域、缺 `=`、非数字）返回 `null`，让调用方显式失效。 */
export function parseDependencyRevision(
  vector: string,
): readonly DomainRevision[] | null {
  if (vector === "") return [];
  const entries: DomainRevision[] = [];
  for (const part of vector.split(";")) {
    const separator = part.lastIndexOf("=");
    if (separator <= 0) return null;
    const domain = part.slice(0, separator);
    const value = Number(part.slice(separator + 1));
    if (!ALL_LIBRARY_DOMAINS.includes(domain as LibraryDomain)) return null;
    if (!Number.isInteger(value) || value < 0) return null;
    entries.push({ domain: domain as LibraryDomain, value });
  }
  if (formatDependencyRevision(entries) !== vector) return null;
  return entries;
}

/** 依赖向量是否逐域相等：任一域前进即代表本页需要重读。 */
export function dependencyRevisionUnchanged(
  previous: string,
  next: string,
  domains: readonly LibraryDomain[] = HUB_PAGE_DEPENDENCIES,
): boolean {
  if (previous === next) return true;
  const before = parseDependencyRevision(previous);
  const after = parseDependencyRevision(next);
  if (!before || !after) return false;
  const lookup = new Map(
    after.map((entry) => [entry.domain, entry.value] as const),
  );
  return domains.every(
    (domain) =>
      lookup.get(domain) ===
      before.find((entry) => entry.domain === domain)?.value,
  );
}

export type LibraryCursorToken = Readonly<{
  digest: string;
  key: string;
  id: string;
}>;

function base64UrlDecode(value: string): string | null {
  const padded = value.replace(/-/g, "+").replace(/_/g, "/");
  const paddedText = padded.padEnd(Math.ceil(padded.length / 4) * 4, "=");
  try {
    const binary = atob(paddedText);
    const bytes = Uint8Array.from(binary, (character) =>
      character.charCodeAt(0),
    );
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    return null;
  }
}

/** cursor 是不透明令牌，但「它属于哪个查询」必须能在前端判定，否则翻页会静默换语义。 */
export function decodeLibraryCursor(cursor: string): LibraryCursorToken | null {
  const text = base64UrlDecode(cursor);
  if (text === null) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null) return null;
  const token = parsed as { digest?: unknown; key?: unknown; id?: unknown };
  if (
    typeof token.digest !== "string" ||
    typeof token.key !== "string" ||
    typeof token.id !== "string" ||
    token.id === ""
  ) {
    return null;
  }
  return { digest: token.digest, key: token.key, id: token.id };
}

export function encodeLibraryCursor(token: LibraryCursorToken): string {
  const bytes = new TextEncoder().encode(
    JSON.stringify({ digest: token.digest, key: token.key, id: token.id }),
  );
  const binary = Array.from(bytes, (byte) => String.fromCharCode(byte)).join(
    "",
  );
  return btoa(binary)
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "");
}

/**
 * cursor 与查询绑定：token 里的 digest 必须等于本页 digest，否则它是「另一个查询」
 * 的游标。Rust 侧同一条规则，前端提前判定只为给出即时提示，不为了省一次 IPC。
 */
export function cursorBelongsToQuery(
  cursor: string | null,
  queryDigest: string,
): boolean {
  if (cursor === null) return true;
  const token = decodeLibraryCursor(cursor);
  return token !== null && token.digest === queryDigest;
}
