import { describe, expect, it } from "vitest";
import {
  applyWorkspaceLayoutIntent,
  consumeEscapeForOutline,
  defaultWorkspaceLayout,
  layoutShowsDiscussionRail,
  layoutShowsOutline,
  layoutShowsPdf,
  parseWorkspaceLayout,
} from "./workspaceLayout";

describe("workspace layout presets", () => {
  it("defaults to pdf_discussion and opens outline beside the PDF", () => {
    expect(defaultWorkspaceLayout()).toBe("pdf_discussion");
    expect(applyWorkspaceLayoutIntent("pdf_discussion", "open_outline")).toBe(
      "pdf_outline",
    );
    expect(layoutShowsPdf("pdf_outline")).toBe(true);
    expect(layoutShowsOutline("pdf_outline")).toBe(true);
    expect(layoutShowsDiscussionRail("pdf_outline")).toBe(false);
  });

  it("keeps outline_only as an explicit user choice", () => {
    expect(applyWorkspaceLayoutIntent("pdf_outline", "outline_only")).toBe(
      "outline_only",
    );
    expect(layoutShowsPdf("outline_only")).toBe(false);
    expect(applyWorkspaceLayoutIntent("outline_only", "show_pdf")).toBe(
      "pdf_outline",
    );
    expect(applyWorkspaceLayoutIntent("pdf_outline", "open_discussion")).toBe(
      "pdf_discussion",
    );
  });

  it("clears an Outline node on Esc before leaving the reader", () => {
    expect(consumeEscapeForOutline("node-1")).toBe("clear_node");
    expect(consumeEscapeForOutline(null)).toBe("pass");
  });

  it("falls back to the discussion preset for unknown persisted values", () => {
    expect(parseWorkspaceLayout("window-manager")).toBe("pdf_discussion");
    expect(parseWorkspaceLayout("pdf_outline")).toBe("pdf_outline");
  });
});
