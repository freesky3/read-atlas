import { describe, expect, it } from "vitest";
import { createMemoryLibraryWorkspaceClient } from "./libraryWorkspaceClient";
import {
  batchOf,
  controlBatchRequest,
  planAndStart,
  planBatchRequest,
  startBatchRequest,
  undoTokenOf,
} from "./libraryActTypes";
import { defaultHubCardLifecycle, type HubPaperCard } from "./libraryWorkspaceTypes";

function card(id: string, collection = "Papers/Inbox"): HubPaperCard {
  return {
    id,
    revisionId: `rev-${id}`,
    title: id,
    authors: ["A"],
    publicationYear: 2026,
    pageCount: 10,
    collectionPath: collection,
    relativePath: `${collection}/${id}.pdf`,
    fileName: `${id}.pdf`,
    kind: collection.startsWith("Textbooks") ? "textbook" : "paper",
    sha256: `hash-${id}`,
    byteSize: 1,
    importedAt: "2026-08-31T00:00:00Z",
    tags: ["inbox"],
    hasOcr: false,
    briefStatus: "missing",
    briefTakeaway: null,
    keywords: [],
    chapterNumber: null,
    sortKey: id,
    ...defaultHubCardLifecycle(),
  };
}

describe("memory library_act", () => {
  it("plan 不改标签，start 才写入，重复 start 不重跑", async () => {
    const client = createMemoryLibraryWorkspaceClient({ papers: [card("a"), card("b")] });
    const planned = await client.act(planBatchRequest(
      { kind: "patch_tags", add: ["reviewed"], remove: [] },
      { kind: "explicit", paperIds: ["a", "b"] },
    ));
    expect(batchOf(planned).state).toBe("planned");
    expect(client.cards().map((item) => item.tags)).toEqual([["inbox"], ["inbox"]]);
    const started = await client.act(startBatchRequest(batchOf(planned)));
    expect(batchOf(started).state).toBe("completed");
    expect(undoTokenOf(started)).toBeTruthy();
    expect(client.cards()[0].tags).toEqual(["inbox", "reviewed"]);
    const again = await client.act(startBatchRequest(batchOf(planned), [], "other-key"));
    expect(undoTokenOf(again)).toBeNull();
    expect(client.cards()[0].tags).toEqual(["inbox", "reviewed"]);
  });

  it("有确认项时 planAndStart 停在预览", async () => {
    const client = createMemoryLibraryWorkspaceClient({ papers: [card("a")] });
    const run = await planAndStart(
      client.act.bind(client),
      { kind: "trash" },
      { kind: "explicit", paperIds: ["a"] },
    );
    expect(run.status).toBe("needs_confirmation");
    expect(client.cards()).toHaveLength(1);
  });

  it("undo 把标签补丁退回去", async () => {
    const client = createMemoryLibraryWorkspaceClient({ papers: [card("a")] });
    const run = await planAndStart(
      client.act.bind(client),
      { kind: "patch_tags", add: ["done"], remove: [] },
      { kind: "explicit", paperIds: ["a"] },
      { acceptRequirements: true },
    );
    if (run.status !== "done") throw new Error("expected start");
    const token = undoTokenOf(run.result);
    expect(token).toBeTruthy();
    await client.act(controlBatchRequest(batchOf(run.result).id, { kind: "undo", token: token! }));
    expect(client.cards()[0].tags).toEqual(["inbox"]);
  });

  it("单项与批量 lifecycle 写入，并列出内置智能集合", async () => {
    const client = createMemoryLibraryWorkspaceClient({ papers: [card("a"), card("b")] });
    const listed = await client.read({ kind: "smart_collections", protocolVersion: 1 });
    expect(listed.kind).toBe("smart_collections");
    if (listed.kind !== "smart_collections") return;
    expect(listed.collections.some((item) => item.id === "reading")).toBe(true);

    const changed = await client.act({
      kind: "change",
      protocolVersion: 1,
      idempotencyKey: "life-1",
      change: {
        kind: "patch_lifecycle",
        paperId: "a",
        expectedVersion: 0,
        patch: { status: "read", favorite: true },
      },
    });
    expect(changed.kind).toBe("change");
    expect(client.cards()[0].lifecycleStatus).toBe("read");
    expect(client.cards()[0].favorite).toBe(true);

    const run = await planAndStart(
      client.act.bind(client),
      { kind: "patch_lifecycle", patch: { readLater: true } },
      { kind: "explicit", paperIds: ["a", "b"] },
    );
    if (run.status !== "done") throw new Error("expected start");
    expect(client.cards().every((item) => item.readLater)).toBe(true);
  });

  it("OCR 计划带费用确认，start 才写入，不提供撤销费用", async () => {
    const client = createMemoryLibraryWorkspaceClient({ papers: [card("a"), card("b")] });
    const preview = await planAndStart(
      client.act.bind(client),
      { kind: "ocr" },
      { kind: "explicit", paperIds: ["a", "b"] },
    );
    expect(preview.status).toBe("needs_confirmation");
    if (preview.status !== "needs_confirmation") return;
    expect(preview.plan.costPreview.marginalEstimates.length).toBeGreaterThan(0);
    expect(preview.plan.undoPolicy).toBe("cancel_only");
    const run = await planAndStart(
      client.act.bind(client),
      { kind: "ocr" },
      { kind: "explicit", paperIds: ["a", "b"] },
      { acceptRequirements: true },
    );
    if (run.status !== "done") throw new Error("expected start");
    expect(undoTokenOf(run.result)).toBeNull();
    expect(client.cards().every((item) => item.hasOcr)).toBe(true);
  });
});
