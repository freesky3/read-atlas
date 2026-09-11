import type { OutlineView } from "../types";

export function nextOutlineSelection(
  currentId: string | null,
  clickedId: string,
): string | null {
  return currentId === clickedId ? null : clickedId;
}

export type OutlineSelectPlan = {
  selectedId: string | null;
  view: OutlineView;
  loadDeepDiveFor: string | null;
};

export function planOutlineNodeSelect(input: {
  view: OutlineView;
  currentId: string | null;
  clickedId: string;
}): OutlineSelectPlan {
  const selectedId = nextOutlineSelection(input.currentId, input.clickedId);
  if (input.view === "deep_dive") {
    return { selectedId, view: "deep_dive", loadDeepDiveFor: null };
  }
  return {
    selectedId,
    view: "overview",
    loadDeepDiveFor: selectedId,
  };
}
