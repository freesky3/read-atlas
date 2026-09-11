// 跨语言与跨 Adapter 的合同测试（D-063 PR 1 Gate：Tauri 与 Memory Adapter 共享合同测试）。
//
// 三层证据：
// 1. `__fixtures__/library_read_v1.json` 是 Rust `library_read` 真实产出的字节，由
//    `library_query.rs` 的 golden 测试逐字回放守住；本文件读同一份字节。
// 2. 同一组协议断言跑在桌面 Adapter（`invoke` 被替换成回放该字节的假实现）与 Memory
//    Adapter 上：排序、分页、依赖向量与错误码必须一致。
// 3. 字段集合逐项比对，任何一侧改名都会在这里而不是运行时才被发现。

import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  HUB_DEFAULT_PAGE_SIZE,
  HUB_MAX_PAGE_SIZE,
  HUB_PAGE_DEPENDENCIES,
  LIBRARY_PROTOCOL_VERSION,
  EVALUATION_TIMEZONE,
  cursorBelongsToQuery,
  decodeLibraryCursor,
  formatDependencyRevision,
  hubPageQuery,
  hubPageRequest,
  parseDependencyRevision,
  watchHandshakeRequest,
  type HubPageResult,
  type HubPaperCard,
  type LibraryHubPageQuery,
  type LibraryQueryFilter,
  type LibraryReadRequest,
  type LibraryReadResult,
} from "./libraryWorkspaceTypes";
import {
  createMemoryLibraryWorkspaceClient,
  createTauriLibraryWorkspaceClient,
  libraryWorkspaceClient,
  type LibraryWorkspaceClient,
} from "./libraryWorkspaceClient";
import libraryFixture from "./__fixtures__/library_read_v1.json";

// ---------- 字段集合就是合同的一部分 ----------

const PAGE_KEYS = [
  "dependencies",
  "dependencyRevision",
  "evaluationAnchor",
  "evaluationTimezone",
  "hasMore",
  "nextCursor",
  "pageSize",
  "papers",
  "protocolVersion",
  "queryDigest",
  "revision",
  "selectionDependencies",
  "selectionDigest",
  "sort",
  "totalCount",
];

const CARD_KEYS = [
  "authors",
  "briefStatus",
  "briefTakeaway",
  "byteSize",
  "chapterNumber",
  "collectionPath",
  "favorite",
  "fileName",
  "furthestPage",
  "hasOcr",
  "id",
  "importedAt",
  "keywords",
  "kind",
  "lastOpenedAt",
  "lifecycleStatus",
  "lifecycleVersion",
  "ocrFailed",
  "pageCount",
  "priority",
  "publicationYear",
  "readLater",
  "relativePath",
  "reviewAt",
  "revisionId",
  "sha256",
  "sortKey",
  "tags",
  "title",
];

const REQUEST_KEYS: Record<LibraryReadRequest["kind"], readonly string[]> = {
  hub_page: ["kind", "page", "protocolVersion"],
  watch_handshake: ["after", "kind", "protocolVersion"],
  batch: ["batchId", "kind", "protocolVersion"],
  batch_items: ["afterOrdinal", "batchId", "kind", "limit", "protocolVersion", "states"],
  recent_batches: ["kind", "limit", "protocolVersion"],
  smart_collections: ["kind", "protocolVersion"],
  reading_context: ["kind", "paperId", "protocolVersion", "revisionId"],
  collection_layer: ["collectionPath", "kind", "protocolVersion"],
};

const PAGE_QUERY_KEYS = ["cursor", "filters", "limit", "sort"];

const FILTER_KEYS: Record<LibraryQueryFilter["kind"], readonly string[]> = {
  collection: ["kind", "path", "recursive"],
  document: ["kind", "value"],
  tag: ["kind", "tag"],
  text: ["kind", "term"],
  status: ["kind", "value"],
  favorite: ["kind", "value"],
  read_later: ["kind", "value"],
  review_due: ["kind"],
  imported: ["kind", "relative"],
  ocr_failed: ["kind"],
  opened: ["kind"],
};

function keysOf(value: unknown): string[] {
  expect(typeof value, JSON.stringify(value)).toBe("object");
  return Object.keys(value as Record<string, unknown>).sort();
}

/** 键序不参与线格式（serde 按字段名映射），比较前先规范化。 */
function canonicalJson(value: unknown): string {
  return JSON.stringify(sortDeep(value));
}

function sortDeep(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(sortDeep);
  if (value === null || typeof value !== "object") return value;
  const entries = Object.entries(value as Record<string, unknown>).sort(([left], [right]) =>
    compare(left, right),
  );
  return Object.fromEntries(entries.map(([key, entry]) => [key, sortDeep(entry)] as const));
}

function compare(left: string, right: string): number {
  if (left === right) return 0;
  return left < right ? -1 : 1;
}

function asCard(value: unknown): HubPaperCard {
  expect(keysOf(value)).toEqual(CARD_KEYS);
  const card = value as HubPaperCard;
  expect(typeof card.id).toBe("string");
  expect(Array.isArray(card.tags)).toBe(true);
  // §10.2：投影只有库内相对路径。
  for (const path of [card.relativePath, card.collectionPath]) {
    expect(path).not.toContain("\\");
    expect(path.startsWith("/")).toBe(false);
    expect(/^[A-Za-z]:/.test(path)).toBe(false);
  }
  return card;
}

function asPage(value: unknown): HubPageResult {
  expect(keysOf(value)).toEqual(PAGE_KEYS);
  const page = value as HubPageResult;
  for (const card of page.papers) asCard(card);
  for (const entry of page.dependencies) expect(keysOf(entry)).toEqual(["domain", "value"]);
  return page;
}

function asRequest(value: unknown): LibraryReadRequest {
  const request = value as LibraryReadRequest;
  expect(REQUEST_KEYS[request.kind]).toEqual(keysOf(request));
  if (request.kind === "hub_page") expect(keysOf(request.page)).toEqual(PAGE_QUERY_KEYS);
  for (const filter of request.kind === "hub_page" ? request.page.filters : []) {
    expect(Object.keys(filter)).toEqual(FILTER_KEYS[filter.kind]);
  }
  return request;
}

function asResult(value: unknown): LibraryReadResult {
  const result = value as LibraryReadResult;
  expect(keysOf(result)).toEqual(
    result.kind === "hub_page"
      ? ["kind", "page"]
      : result.kind === "watch_handshake"
        ? ["currentRevision", "invalidated", "kind"]
        : result.kind === "batch"
          ? ["batch", "kind"]
          : result.kind === "batch_items"
            ? ["kind", "page"]
            : result.kind === "smart_collections"
              ? ["collections", "kind"]
              : result.kind === "reading_context"
                ? ["context", "kind"]
                : result.kind === "collection_layer"
                  ? ["collectionId", "kind", "paperIds"]
                  : ["kind", "page"],
  );
  if (result.kind === "hub_page") asPage(result.page);
  return result;
}

type FixtureCase = Readonly<{ name: string; request: unknown; result: unknown }>;

const cases = libraryFixture.cases as FixtureCase[];

function fixtureCase(name: string): FixtureCase {
  const found = cases.find((item) => item.name === name);
  if (!found) throw new Error(`fixture case ${name} is missing`);
  return found;
}

function hubCase(name: string): { request: LibraryReadRequest; page: HubPageResult } {
  const item = fixtureCase(name);
  const request = asRequest(item.request);
  const result = asResult(item.result);
  if (request.kind !== "hub_page" || result.kind !== "hub_page") {
    throw new Error(`${name} is not a hub_page case`);
  }
  return { request, page: result.page };
}

/** 直接回放夹具里的那条查询：桌面 Adapter 只认夹具给过的请求。 */
function hubRequest(name: string): LibraryHubPageQuery {
  const { request } = hubCase(name);
  if (request.kind !== "hub_page") throw new Error(`${name} is not a hub_page case`);
  return request.page;
}

/** Memory Adapter 的种子数据就是 Rust 投影出来的那三张卡片。 */
const seedCards = hubCase("hub_page_default").page.papers;

// ---------- 桌面 Adapter 的假 invoke：逐字回放 Rust 产出的字节 ----------

type InvokeArgs = Record<string, unknown> | undefined;

let invokeCalls: { command: string; args: InvokeArgs }[] = [];
let eventHandlers: ((event: { payload: unknown }) => void)[] = [];
let behaviour: "fixture" | "raw-error" | "typed-error" = "fixture";
let invokeImpl: (command: string, args?: InvokeArgs) => Promise<unknown> = fakeInvoke;

async function fakeInvoke(command: string, args?: InvokeArgs): Promise<unknown> {
  invokeCalls.push({ command, args });
  if (command !== "library_read") throw new Error(`unexpected command ${command}`);
  if (behaviour === "raw-error") throw "no such column: papers.sha256";
  if (behaviour === "typed-error") {
    throw { code: "workspace_unavailable", message: "workspace is closed" };
  }
  const request = asRequest((args as { request: unknown }).request);
  const encoded = canonicalJson(request);
  const match = cases.find((item) => canonicalJson(asRequest(item.request)) === encoded);
  if (!match) {
    // `library_read` 对无法回答的请求只给 typed code，这里按同一规则回绝。
    throw { code: "invalid_query", message: `no fixture case for ${encoded}` };
  }
  return asResult(match.result);
}

function emitLibraryEvent(payload: unknown = { kind: "library" }): void {
  for (const handler of [...eventHandlers]) handler({ payload });
}

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: InvokeArgs) => invokeImpl(command, args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: async (_event: string, handler: (event: { payload: unknown }) => void) => {
    eventHandlers.push(handler);
    return () => {
      eventHandlers = eventHandlers.filter((item) => item !== handler);
    };
  },
}));

// ---------- 共享合同 ----------

type Reader = Pick<LibraryWorkspaceClient, "read">;

async function readPage(
  client: Reader,
  page: Partial<LibraryHubPageQuery> = {},
): Promise<HubPageResult> {
  const result = await client.read(hubPageRequest(page));
  if (result.kind !== "hub_page") throw new Error("expected a hub page");
  return result.page;
}

/** 只允许 runtime 相关的值不同：两枚 fingerprint 与挂钟锚点；其余字段必须逐字相同。 */
function withoutRuntimeValues(
  page: HubPageResult,
): Omit<HubPageResult, "queryDigest" | "nextCursor" | "selectionDigest" | "evaluationAnchor"> {
  // 预览端没有 SQLite 也没有 sha256，digest 一律带 `mem-` 前缀；
  // evaluationAnchor 是读取时刻，本来就不可能跨实现相等。
  const {
    queryDigest: _digest,
    nextCursor: _cursor,
    selectionDigest: _selection,
    evaluationAnchor: _anchor,
    ...rest
  } = page;
  return rest;
}

const adapters: readonly [string, () => LibraryWorkspaceClient][] = [
  ["desktop", () => createTauriLibraryWorkspaceClient()],
  ["memory", () => createMemoryLibraryWorkspaceClient({ papers: seedCards })],
];

describe.each(adapters)("library_read contract (%s)", (_name, create) => {
  beforeEach(() => {
    invokeCalls = [];
    eventHandlers = [];
    behaviour = "fixture";
  });

  it("caps the page at the protocol default and declares only real dependencies", async () => {
    const page = await readPage(create());
    expect(page.pageSize).toBe(HUB_DEFAULT_PAGE_SIZE);
    expect(page.protocolVersion).toBe(LIBRARY_PROTOCOL_VERSION);
    expect(page.sort).toBe("recent");
    expect(page.dependencies.map((entry) => entry.domain)).toEqual(
      [...HUB_PAGE_DEPENDENCIES].sort(),
    );
    expect(page.dependencyRevision).toBe(formatDependencyRevision(page.dependencies));
    expect(parseDependencyRevision(page.dependencyRevision)).toEqual(page.dependencies);
    expect(page.hasMore).toBe(false);
    expect(page.nextCursor).toBeNull();
  });

  it("keeps totalCount fixed while the keyset walks without gaps or repeats", async () => {
    const client = create();
    const seen: string[] = [];
    let cursor: string | null = null;
    for (let guard = 0; guard < 5; guard += 1) {
      const page = await readPage(client, { sort: "title", limit: 1, cursor });
      expect(page.totalCount).toBe(3);
      expect(page.papers).toHaveLength(1);
      seen.push(...page.papers.map((card) => card.id));
      cursor = page.hasMore ? page.nextCursor : null;
      if (cursor === null) break;
    }
    expect(seen).toEqual([
      "paper-Papers/Inbox/alpha.pdf",
      "paper-Papers/Inbox/beta.pdf",
      "paper-Textbooks/gamma.pdf",
    ]);
  });

  it("reproduces the Rust projection field by field", async () => {
    const client = create();
    for (const name of ["hub_page_default", "hub_page_title_first", "hub_page_filtered"]) {
      const { request, page: expected } = hubCase(name);
      if (request.kind !== "hub_page") continue;
      const actual = await readPage(client, request.page);
      expect(withoutRuntimeValues(actual), name).toEqual(withoutRuntimeValues(expected));
      if (expected.nextCursor === null) {
        expect(actual.nextCursor).toBeNull();
        continue;
      }
      const mine = decodeLibraryCursor(actual.nextCursor as string);
      const theirs = decodeLibraryCursor(expected.nextCursor);
      // 预览端的 cursor 用自己的 digest，但锚点必须与桌面端逐字相同。
      expect(mine?.key).toBe(theirs?.key);
      expect(mine?.id).toBe(theirs?.id);
      expect(theirs?.digest).toBe(expected.queryDigest);
    }
  });

  it("freezes membership separately from the page query (§5.1)", async () => {
    const client = create();
    const recent = await readPage(client, { sort: "recent" });
    const titled = await readPage(client, { sort: "title", limit: 1 });
    // 换排序 / 换页宽都不改变「这一屏之外还有谁」：快照键不能是 queryDigest。
    expect(titled.selectionDigest).toBe(recent.selectionDigest);
    expect(titled.queryDigest).not.toBe(recent.queryDigest);
    // 纯目录选择只依赖 structure：一次无关打标不该把选择判成漂移。
    expect(recent.selectionDependencies.map((entry) => entry.domain)).toEqual(["structure"]);
    const tagged = await readPage(client, hubRequest("hub_page_tag_only"));
    expect(tagged.selectionDigest).not.toBe(recent.selectionDigest);
    expect(tagged.selectionDependencies.map((entry) => entry.domain)).toEqual([
      "structure",
      "tags",
    ]);
    expect(tagged.evaluationTimezone).toBe(EVALUATION_TIMEZONE);
    expect(tagged.evaluationAnchor.length).toBeGreaterThan(0);
  });

  it("normalizes legacy collection paths to the same page", async () => {
    const client = create();
    const expected = hubCase("hub_page_collection_canonical").page;
    expect(expected.papers.map((card) => card.id)).toEqual([
      "paper-Papers/Inbox/alpha.pdf",
      "paper-Papers/Inbox/beta.pdf",
    ]);
    for (const name of [
      "hub_page_collection_lowercase_root",
      "hub_page_collection_bare_name",
      "hub_page_collection_trailing_slash",
    ]) {
      const { request } = hubCase(name);
      if (request.kind !== "hub_page") continue;
      const actual = await readPage(client, request.page);
      // 归一化规则在 Rust / TS 各有一份：宽松只到根段与分隔符为止。
      expect(withoutRuntimeValues(actual), name).toEqual(withoutRuntimeValues(expected));
    }
    const strict = hubCase("hub_page_collection_leaf_case_matters");
    if (strict.request.kind === "hub_page") {
      const actual = await readPage(client, strict.request.page);
      // 叶子段写错大小写必须是空页，两侧都不许「顺手」做 case folding。
      expect(actual.totalCount).toBe(0);
      expect(actual.papers).toEqual([]);
    }
  });

  it("binds a cursor to the query that produced it", async () => {
    const client = create();
    const page = await readPage(client, { sort: "title", limit: 1 });
    const cursor = page.nextCursor as string;
    expect(cursorBelongsToQuery(cursor, page.queryDigest)).toBe(true);
    expect(cursorBelongsToQuery(cursor, "0".repeat(64))).toBe(false);
    // 换一个 sort 就是另一个查询：同一枚 cursor 不能被复用以「省一次首页读取」。
    await expect(
      readPage(client, { sort: "recent", limit: 1, cursor }),
    ).rejects.toMatchObject({ code: "invalid_query" });
  });

  it("rejects requests the AST cannot answer, with typed codes only", async () => {
    const client = create();
    await expect(readPage(client, { limit: 0 })).rejects.toMatchObject({
      code: "invalid_query",
    });
    await expect(readPage(client, { limit: HUB_MAX_PAGE_SIZE + 1 })).rejects.toMatchObject({
      code: "invalid_query",
    });
    await expect(readPage(client, { sort: "manual" })).rejects.toMatchObject({
      code: "invalid_query",
    });
    await expect(
      readPage(client, { filters: [{ kind: "text", term: "   " }] }),
    ).rejects.toMatchObject({ code: "invalid_query" });
    await expect(
      readPage(client, {
        filters: [{ kind: "collection", path: "../Papers", recursive: false }],
      }),
    ).rejects.toMatchObject({ code: "invalid_query" });
    await expect(
      client.read({ kind: "hub_page", protocolVersion: 99, page: hubPageQuery() }),
    ).rejects.toMatchObject({ code: "invalid_query" });
  });

  it("keeps a current snapshot and drops a stale one", async () => {
    const client = create();
    const page = await readPage(client);
    await expect(client.read(watchHandshakeRequest(page.revision))).resolves.toEqual({
      kind: "watch_handshake",
      currentRevision: page.revision,
      invalidated: false,
    });
    await expect(client.read(watchHandshakeRequest(null))).resolves.toMatchObject({
      kind: "watch_handshake",
      invalidated: true,
    });
  });
});

describe("desktop LibraryWorkspaceClient over library_read", () => {
  beforeEach(() => {
    invokeCalls = [];
    eventHandlers = [];
    behaviour = "fixture";
    invokeImpl = fakeInvoke;
  });

  it("sends the discriminated union as the only argument", async () => {
    const client = createTauriLibraryWorkspaceClient();
    await client.read(hubPageRequest({ sort: "title", limit: 1 }));
    expect(invokeCalls).toHaveLength(1);
    expect(invokeCalls[0].command).toBe("library_read");
    expect(Object.keys(invokeCalls[0].args as object)).toEqual(["request"]);
    expect(invokeCalls[0].args).toEqual({
      request: {
        kind: "hub_page",
        protocolVersion: LIBRARY_PROTOCOL_VERSION,
        page: { filters: [], sort: "title", limit: 1, cursor: null },
      },
    });
  });

  it("never forwards a raw database string to the caller", async () => {
    const client = createTauriLibraryWorkspaceClient();
    behaviour = "raw-error";
    await expect(client.read(hubPageRequest())).rejects.toEqual({
      code: "workspace_unavailable",
      message: "工作区不可用，请在设置中重新选择。",
    });
    behaviour = "typed-error";
    await expect(client.read(hubPageRequest())).rejects.toEqual({
      code: "workspace_unavailable",
      message: "工作区不可用，请在设置中重新选择。",
    });
  });

  it("does not call the backend for a protocol version it cannot speak", async () => {
    const client = createTauriLibraryWorkspaceClient();
    await expect(
      client.read({ kind: "hub_page", protocolVersion: 0, page: hubPageQuery() }),
    ).rejects.toMatchObject({ code: "invalid_query" });
    expect(invokeCalls).toHaveLength(0);
  });

  it("buffers events that arrive during the opening handshake", async () => {
    let releaseHandshake: (() => void) | undefined;
    let holding = false;
    invokeImpl = async (command, args) => {
      const request = (args as { request?: { kind?: string } } | undefined)?.request;
      if (command === "library_read" && request?.kind === "watch_handshake" && !holding) {
        holding = true;
        await new Promise<void>((resolve) => {
          releaseHandshake = resolve;
        });
      }
      return fakeInvoke(command, args);
    };
    const client = createTauriLibraryWorkspaceClient();
    const watching = client.watch(null, () => undefined);
    await vi.waitFor(() => expect(eventHandlers.length).toBe(1));
    emitLibraryEvent({ kind: "job" });
    expect(invokeCalls).toHaveLength(0);
    releaseHandshake?.();
    await watching;
    await vi.waitFor(() => expect(invokeCalls.length).toBeGreaterThanOrEqual(2));
    expect(invokeCalls.every((call) => {
      const request = (call.args as { request?: { kind?: string } } | undefined)?.request;
      return call.command === "library_read" && request?.kind === "watch_handshake";
    })).toBe(true);
  });

  it("subscribes to the library signal and releases it on unsubscribe", async () => {
    const client = createTauriLibraryWorkspaceClient();
    const seen: number[] = [];
    const handle = await client.watch(null, (revision) => seen.push(revision));
    expect(handle.currentRevision).toBe(0);
    const handshakesAfterSubscribe = invokeCalls.length;
    // 作业完成发的是 `job`，标签改动发的是 `library`：任何一条都只是提示。
    emitLibraryEvent({ kind: "job" });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(invokeCalls.length).toBe(handshakesAfterSubscribe + 1);
    // 全局 revision 仍是 0：没有前进就不该通知，否则 Hub 会为一次空事件重读整页。
    expect(seen).toEqual([]);
    emitLibraryEvent("not-a-payload");
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(invokeCalls.length).toBe(handshakesAfterSubscribe + 1);
    handle.unsubscribe();
    expect(eventHandlers).toHaveLength(0);
  });
});

describe("memory LibraryWorkspaceClient writes", () => {
  it("advances only the domains a write touched", async () => {
    const client = createMemoryLibraryWorkspaceClient({ papers: seedCards });
    const before = await readPage(client);
    expect(before.dependencyRevision).toBe("artifacts=0;engagement=0;jobs=0;lifecycle=0;sort=0;structure=0;tags=0");
    client.setManualOrder(
      "Papers/Inbox",
      ["paper-Papers/Inbox/beta.pdf", "paper-Papers/Inbox/alpha.pdf"],
    );
    const after = await readPage(client);
    expect(after.revision).toBe(before.revision + 1);
    expect(after.dependencyRevision).toBe("artifacts=0;engagement=0;jobs=0;lifecycle=0;sort=1;structure=0;tags=0");
    expect(after.totalCount).toBe(before.totalCount);
  });

  it("does not bump on a rejected write", async () => {
    const client = createMemoryLibraryWorkspaceClient({ papers: seedCards });
    const before = await readPage(client);
    expect(() =>
      client.write(["structure"], () => {
        throw new Error("disk write failed");
      }),
    ).toThrow("disk write failed");
    expect((await readPage(client)).revision).toBe(before.revision);
  });

  it("notifies a watcher as soon as the revision advances", async () => {
    const client = createMemoryLibraryWorkspaceClient({ papers: seedCards });
    const seen: number[] = [];
    const handle = await client.watch(null, (revision) => seen.push(revision));
    client.addCards([]);
    expect(seen).toEqual([1]);
    expect(handle.currentRevision).toBe(1);
    handle.unsubscribe();
    client.addCards([]);
    expect(seen).toEqual([1]);
  });

  it("orders manual pages by the persisted position", async () => {
    const client = createMemoryLibraryWorkspaceClient({
      papers: seedCards,
      manualOrders: {
        "Papers/Inbox": ["paper-Papers/Inbox/beta.pdf", "paper-Papers/Inbox/alpha.pdf"],
      },
    });
    const page = await readPage(client, {
      sort: "manual",
      filters: [{ kind: "collection", path: "Papers/Inbox", recursive: false }],
    });
    expect(page.papers.map((card) => card.id)).toEqual([
      "paper-Papers/Inbox/beta.pdf",
      "paper-Papers/Inbox/alpha.pdf",
    ]);
  });

  it("returns every live id in the collection layer for an exact permutation", async () => {
    const client = createMemoryLibraryWorkspaceClient({
      papers: seedCards,
      manualOrders: {
        "Papers/Inbox": ["paper-Papers/Inbox/beta.pdf", "paper-Papers/Inbox/alpha.pdf"],
      },
    });
    const result = await client.read({
      kind: "collection_layer",
      protocolVersion: LIBRARY_PROTOCOL_VERSION,
      collectionPath: "Papers/Inbox",
    });
    expect(result).toEqual({
      kind: "collection_layer",
      collectionId: "Papers/Inbox",
      paperIds: ["paper-Papers/Inbox/beta.pdf", "paper-Papers/Inbox/alpha.pdf"],
    });
  });

  it("orders a chapter page with segmented numbers inside one collection", async () => {
    const early = {
      ...seedCards[0],
      id: "paper-Textbooks/Book/early.pdf",
      collectionPath: "Textbooks/Book",
      relativePath: "Textbooks/Book/early.pdf",
      fileName: "early.pdf",
      chapterNumber: "1.10",
      title: "Early",
      importedAt: "2026-08-01T00:00:00Z",
    };
    const late = {
      ...seedCards[0],
      id: "paper-Textbooks/Book/late.pdf",
      collectionPath: "Textbooks/Book",
      relativePath: "Textbooks/Book/late.pdf",
      fileName: "late.pdf",
      chapterNumber: "1.2",
      title: "Late",
      importedAt: "2026-08-01T00:00:01Z",
    };
    const client = createMemoryLibraryWorkspaceClient({ papers: [early, late] });
    await expect(readPage(client, { sort: "chapter" })).rejects.toMatchObject({
      code: "invalid_query",
    });
    const page = await readPage(client, {
      sort: "chapter",
      filters: [{ kind: "collection", path: "Textbooks/Book", recursive: false }],
    });
    expect(page.papers.map((card) => card.id)).toEqual([late.id, early.id]);
  });

  it("falls back to the browser preview outside the desktop runtime", () => {
    expect(libraryWorkspaceClient.runtime).toBe("memory");
  });
});

describe("library_read fixture bytes", () => {
  it("lists every documented field and nothing else", () => {
    expect(keysOf(libraryFixture)).toEqual(["cases", "protocolVersion"]);
    expect(libraryFixture.protocolVersion).toBe(LIBRARY_PROTOCOL_VERSION);
    expect(cases.map((item) => item.name)).toEqual([
      "hub_page_default",
      "hub_page_title_first",
      "hub_page_title_second",
      "hub_page_title_third",
      "hub_page_filtered",
      "hub_page_tag_only",
      "hub_page_collection_canonical",
      "hub_page_collection_lowercase_root",
      "hub_page_collection_bare_name",
      "hub_page_collection_trailing_slash",
      "hub_page_collection_leaf_case_matters",
      "watch_handshake_stale",
      "watch_handshake_current",
      "smart_collections",
      "reading_context_alpha",
      "collection_layer_inbox",
    ]);
    const bytes = JSON.stringify(libraryFixture);
    expect(bytes).not.toContain("\\");
    expect(bytes).not.toMatch(/[A-Za-z]:\//);
  });

  it("names filter variants by their wire kind", () => {
    const { request } = hubCase("hub_page_filtered");
    expect(request.kind).toBe("hub_page");
    if (request.kind !== "hub_page") return;
    expect(request.page.filters).toEqual([
      { kind: "collection", path: "Papers", recursive: true },
      { kind: "tag", tag: "ML" },
    ]);
  });
});


describe("effective auxiliary metadata in the memory library", () => {
  it("sorts the displayed year and segmented chapters with unknowns last in either direction", async () => {
    const papers = seedCards.slice(0, 3).map((card, index) => ({
      ...card, id: `auxiliary-${index}`, collectionPath: "Textbooks/Sort",
      relativePath: `Textbooks/Sort/${index}.pdf`, kind: "textbook" as const,
      publicationYear: 1900, displayMetadata: { year: [2024, 2025, null][index], yearLabel: "发布" },
      chapterNumber: ["1.2", "1.10", null][index],
    }));
    const client = createMemoryLibraryWorkspaceClient({ papers });
    for (const direction of ["asc", "desc"] as const) {
      const expected = direction === "asc" ? [papers[0].id, papers[1].id, papers[2].id] : [papers[1].id, papers[0].id, papers[2].id];
      const year = await readPage(client, { sort: "year", direction });
      expect(year.papers.map(card => card.id)).toEqual(expected);
      const chapter = await readPage(client, { sort: "chapter", direction, filters: [{ kind: "collection", path: "Textbooks/Sort", recursive: false }] });
      expect(chapter.papers.map(card => card.id)).toEqual(expected);
      expect(year.papers[0].publicationYear).toBe(1900);
      expect(year.papers[0].displayMetadata?.yearLabel).toBe("发布");
    }
  });
});
