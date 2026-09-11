import { describe, expect, it } from "vitest";
import { hubFiltersForView, matchesHubTextFilter, querySnapshotFromPage, thisWeekStartUtc } from "./hubFilters";
import type { HubPageResult } from "./libraryWorkspaceTypes";

describe("hubFilters", () => {
  it("空目录不加 collection 筛选，检索映射为 text", () => {
    expect(hubFiltersForView("", "")).toEqual([]);
    expect(hubFiltersForView("Papers/ML", "Bayes")).toEqual([
      { kind: "collection", path: "Papers/ML", recursive: true },
      { kind: "text", term: "Bayes" },
    ]);
  });

  it("文本筛选与后端一样只打 title / authors / fileName", () => {
    expect(matchesHubTextFilter("alpha", { title: "Alpha", authors: "B", fileName: "x.pdf" })).toBe(true);
    expect(matchesHubTextFilter("x.pdf", { title: "Alpha", authors: "B", fileName: "x.pdf" })).toBe(true);
    expect(matchesHubTextFilter("takeaway", { title: "Alpha", authors: "B", fileName: "x.pdf" })).toBe(false);
  });

  it("快照用 selectionDigest 而不是带 sort 的 queryDigest", () => {
    const page = {
      selectionDigest: "sel-1",
      queryDigest: "q-1",
      selectionDependencies: [{ domain: "structure", value: 4 }],
      evaluationAnchor: "2026-08-31T00:00:00Z",
      evaluationTimezone: "UTC",
    } as unknown as HubPageResult;
    const snapshot = querySnapshotFromPage(page);
    expect(snapshot.queryDigest).toBe("sel-1");
    expect(snapshot.dependencyRevisions).toEqual({ structure: 4 });
  });

  it("本周按锚点所在周的周一起算", () => {
    expect(thisWeekStartUtc("2026-09-02T15:00:00+00:00")).toBe("2026-08-31T00:00:00.000Z");
  });

  it("智能集合视图使用集合自身的 filters，再叠检索", () => {
    expect(hubFiltersForView("smart:reading", "Bayes", {
      query: { queryVersion: 1, filters: [{ kind: "status", value: "reading" }], sort: "recent" },
    })).toEqual([
      { kind: "status", value: "reading" },
      { kind: "text", term: "Bayes" },
    ]);
  });
});
