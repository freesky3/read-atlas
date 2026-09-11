export type WorkspaceLayout =
  | "pdf_discussion"
  | "pdf_outline"
  | "outline_only";

export type WorkspaceLayoutIntent =
  | "open_outline"
  | "open_discussion"
  | "open_artifacts"
  | "outline_only"
  | "show_pdf";

export function defaultWorkspaceLayout(): WorkspaceLayout {
  return "pdf_discussion";
}

export function applyWorkspaceLayoutIntent(
  current: WorkspaceLayout,
  intent: WorkspaceLayoutIntent,
): WorkspaceLayout {
  switch (intent) {
    case "open_outline":
      return "pdf_outline";
    case "open_discussion":
    case "open_artifacts":
      return "pdf_discussion";
    case "outline_only":
      return "outline_only";
    case "show_pdf":
      return current === "outline_only" ? "pdf_outline" : current;
  }
}

export function layoutShowsPdf(layout: WorkspaceLayout): boolean {
  return layout !== "outline_only";
}

export function layoutShowsOutline(layout: WorkspaceLayout): boolean {
  return layout === "pdf_outline" || layout === "outline_only";
}

export function layoutShowsDiscussionRail(layout: WorkspaceLayout): boolean {
  return layout === "pdf_discussion";
}

export function consumeEscapeForOutline(
  selectedNodeId: string | null,
): "clear_node" | "pass" {
  return selectedNodeId ? "clear_node" : "pass";
}

export function parseWorkspaceLayout(value: string | null | undefined): WorkspaceLayout {
  if (
    value === "pdf_discussion" ||
    value === "pdf_outline" ||
    value === "outline_only"
  ) {
    return value;
  }
  return defaultWorkspaceLayout();
}

export function parseOutlineView(
  value: string | null | undefined,
): "overview" | "deep_dive" {
  return value === "deep_dive" ? "deep_dive" : "overview";
}
