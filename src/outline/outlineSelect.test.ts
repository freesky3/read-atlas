import { describe, expect, it } from "vitest";
import { nextOutlineSelection, planOutlineNodeSelect } from "./outlineSelect";

describe("nextOutlineSelection", () => {
  it("toggles the current node and replaces a different node", () => {
    expect(nextOutlineSelection(null, "n1")).toBe("n1");
    expect(nextOutlineSelection("n1", "n1")).toBeNull();
    expect(nextOutlineSelection("n1", "n2")).toBe("n2");
  });
});

describe("planOutlineNodeSelect", () => {
  it("keeps a deep-dive selection on the local map and does not reload the head", () => {
    expect(
      planOutlineNodeSelect({
        view: "deep_dive",
        currentId: "overview-1",
        clickedId: "local-2",
      }),
    ).toEqual({
      selectedId: "local-2",
      view: "deep_dive",
      loadDeepDiveFor: null,
    });
  });

  it("clears a deep-dive node without leaving the local map", () => {
    expect(
      planOutlineNodeSelect({
        view: "deep_dive",
        currentId: "local-2",
        clickedId: "local-2",
      }),
    ).toEqual({
      selectedId: null,
      view: "deep_dive",
      loadDeepDiveFor: null,
    });
  });

  it("loads the deep-dive head only when selecting on the overview", () => {
    expect(
      planOutlineNodeSelect({
        view: "overview",
        currentId: null,
        clickedId: "n1",
      }),
    ).toEqual({
      selectedId: "n1",
      view: "overview",
      loadDeepDiveFor: "n1",
    });
  });
});
