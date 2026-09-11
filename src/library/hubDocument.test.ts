import { describe, expect, it } from "vitest";
import fixture from "./__fixtures__/library_read_v1.json";
import { documentCardFromHub } from "./hubDocument";
import type { HubPaperCard } from "./libraryWorkspaceTypes";
const page = fixture.cases.find(item => item.name === "hub_page_default")!.result as unknown as { page: { papers: HubPaperCard[] } };
const base = page.page.papers[0];
describe("Hub editable tags", () => {
  it("shows saved tags before a Brief exists", () => {
    const card = documentCardFromHub({ ...base, briefStatus: "missing", tags: ["installer-qa"], keywords: [] });
    expect(card.keywords).toEqual(["installer-qa"]);
  });
  it("does not revive removed tags from old generated keywords", () => {
    const card = documentCardFromHub({ ...base, tags: [], keywords: ["old-model-keyword"] });
    expect(card.keywords).toEqual([]);
  });
});
