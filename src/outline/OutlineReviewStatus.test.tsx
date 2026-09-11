import { screen } from "@testing-library/react";
import { renderWithLocale as render } from "../i18n/testUtils";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import OutlineReviewStatus from "./OutlineReviewStatus";
import type { OutlineHeadProjection } from "../types";
const head: OutlineHeadProjection = { id: "h", kind: "overview", status: "published", ocrRevisionId: "ocr", catalogDigest: "cat", protocolVersion: "outline-map-v4", coverageWarnings: [] };
describe("map review status", () => {
  it.each([
    ["unchecked", "候选图 · 尚未独立复核"],
    ["reviewed", "已独立复核"],
    ["reviewed_with_gaps", "已独立复核 · 存在缺口"],
    ["self_checked", "局部图 · 单次生成与自检"],
  ])("reports %s without claiming proof", (reviewStatus, label) => {
    render(<OutlineReviewStatus head={{ ...head, reviewStatus, protocolVersion: reviewStatus === "self_checked" ? "outline-deep-dive-v4" : head.protocolVersion }} />);
    expect(screen.getByText(label)).toBeInTheDocument();
  });
  it("retains review notes, stale warnings and navigable gap suggestions", async () => {
    const jump = vi.fn(); const user = userEvent.setup();
    render(<OutlineReviewStatus head={{ ...head, reviewStatus: "reviewed_with_gaps", reviewNotes: ["已修正关系方向"], gaps: [{ description: "附录证明仍需核对", suggestedPages: [8] }] }} stale onJump={jump} />);
    expect(screen.getByText(/基于旧 OCR/)).toBeInTheDocument();
    await user.click(screen.getByText("复核说明"));
    expect(screen.getByText("已修正关系方向")).toBeVisible();
    expect(screen.getByText("附录证明仍需核对")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "p.8" }));
    expect(jump).toHaveBeenCalledWith({ pageNumber: 8 });
  });
});
