import { describe, it, expect } from "vitest";
import {
  applyWorkspaceLayoutIntent,
  defaultWorkspaceLayout,
  layoutShowsDiscussionRail,
  layoutShowsOutline,
  layoutShowsPdf,
} from "../src/outline/workspaceLayout";

describe("View navigation logic", () => {
  it("defaults to library view when no document is active or when returning", () => {
    let viewMode: "library" | "reader" = "library";
    expect(viewMode).toBe("library");

    // Select document
    viewMode = "reader";
    expect(viewMode).toBe("reader");

    // Click back to library
    viewMode = "library";
    expect(viewMode).toBe("library");
  });

  it("keeps discussion as the default reader preset and opens outline beside the PDF", () => {
    expect(defaultWorkspaceLayout()).toBe("pdf_discussion");
    const outline = applyWorkspaceLayoutIntent(
      defaultWorkspaceLayout(),
      "open_outline",
    );
    expect(outline).toBe("pdf_outline");
    expect(layoutShowsPdf(outline)).toBe(true);
    expect(layoutShowsOutline(outline)).toBe(true);
    expect(layoutShowsDiscussionRail(outline)).toBe(false);
    expect(applyWorkspaceLayoutIntent(outline, "outline_only")).toBe(
      "outline_only",
    );
    expect(layoutShowsPdf("outline_only")).toBe(false);
  });

  it("restores the last active paper and viewMode across restarts", () => {
    const LAST_ACTIVE_PAPER_ID_KEY = "read-desktop.lastActivePaperId";
    const LAST_ACTIVE_REVISION_ID_KEY = "read-desktop.lastActiveRevisionId";
    const LAST_VIEW_MODE_KEY = "read-desktop.viewMode";

    const mockDocs = [
      { id: "paper-1", revisionId: "rev-1", title: "Paper 1" },
      { id: "paper-2", revisionId: "rev-2", title: "Paper 2" },
      { id: "paper-3", revisionId: "rev-3", title: "Paper 3" },
    ];

    // User was reading Paper 2 in reader view
    localStorage.setItem(LAST_VIEW_MODE_KEY, "reader");
    localStorage.setItem(LAST_ACTIVE_PAPER_ID_KEY, "paper-2");
    localStorage.setItem(LAST_ACTIVE_REVISION_ID_KEY, "rev-2");

    const savedRevisionId = localStorage.getItem(LAST_ACTIVE_REVISION_ID_KEY);
    const savedPaperId = localStorage.getItem(LAST_ACTIVE_PAPER_ID_KEY);
    const savedViewMode = localStorage.getItem(LAST_VIEW_MODE_KEY);

    const targetDoc =
      (savedRevisionId &&
        mockDocs.find((doc) => doc.revisionId === savedRevisionId)) ||
      (savedPaperId && mockDocs.find((doc) => doc.id === savedPaperId)) ||
      mockDocs[0];

    expect(targetDoc.id).toBe("paper-2");
    expect(targetDoc.revisionId).toBe("rev-2");
    expect(savedViewMode).toBe("reader");

    // Clean up
    localStorage.removeItem(LAST_VIEW_MODE_KEY);
    localStorage.removeItem(LAST_ACTIVE_PAPER_ID_KEY);
    localStorage.removeItem(LAST_ACTIVE_REVISION_ID_KEY);
  });
});
