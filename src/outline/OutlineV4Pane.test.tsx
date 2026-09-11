import { screen } from "@testing-library/react";
import { renderWithLocale as render } from "../i18n/testUtils";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import OutlinePane from "./OutlinePane";
import type { OutlineGraph, OutlineHeadProjection, OutlinePlan, OutlineProjection } from "../types";

const graph: OutlineGraph = { title: "正式图", summary: "关系概览", nodes: [{ nodeId: "n", title: "设计选择", takeaway: "依据变化调整采样", references: [{ pageNumber: 2, purpose: "设计定义" }] }], edges: [] };
const head: OutlineHeadProjection = { id: "head", kind: "overview", status: "published", protocolVersion: "outline-map-v4", ocrRevisionId: "ocr", catalogDigest: "catalog", coverageWarnings: [], reviewStatus: "reviewed", graph };
const projection: OutlineProjection = { status: "published", revisionId: "rev", ocrRevisionId: "ocr", hasPaperRoot: true, catalog: { entries: [], digest: "catalog", tokenEstimate: 0 }, head, activeJobId: null, coverageWarnings: [] };
const localPlan: OutlinePlan = { revisionId: "rev", ocrRevisionId: "ocr", catalogDigest: "catalog", model: "test-model", rootCalls: 0, extractCalls: 0, composeCalls: 1, maxRepairCalls: 1, pageCount: 8, catalogTokenEstimate: 20, pdfTokenEstimate: 100, estimatedCost: null, hasPaperRoot: true, supportsNativePdf: true, protocolVersion: "outline-deep-dive-v4", workflow: "local_map", planId: "plan", planDigest: "digest", expectedHeadId: "head" };
const props = { projection, plan: null, planError: null, layout: "pdf_outline" as const, onOpenDiscussion: vi.fn(), onShowPdf: vi.fn(), onUseOutlineOnly: vi.fn() };

describe("v4 map publication and local plans", () => {
  it("keeps the published graph visible while explicitly exposing an unchecked candidate", async () => {
    const user = userEvent.setup();
    render(<OutlinePane {...props} projection={{ ...projection, latestAttempt: { ...head, id: "candidate", status: "partial", reviewStatus: "unchecked", graph: { ...graph, title: "未复核候选", nodes: [{ ...graph.nodes[0], title: "候选设计" }] } } }} />);
    expect(await screen.findByText("设计选择")).toBeInTheDocument();
    expect(screen.queryByText("候选设计")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "查看未复核候选图" }));
    expect(screen.getByText("候选图 · 尚未独立复核")).toBeInTheDocument();
    expect(screen.getByText("候选设计")).toBeInTheDocument();
    expect(screen.getByText("设计选择")).toBeInTheDocument();
  });

  it("shows local self-check status and source navigation without a third map layer", async () => {
    const jump = vi.fn(); const generate = vi.fn(); const user = userEvent.setup();
    const localHead = { ...head, id: "local", kind: "deep_dive", protocolVersion: "outline-deep-dive-v4", reviewStatus: "self_checked" };
    render(<OutlinePane {...props} deepDiveHead={localHead} deepDiveGraph={graph} selectedNodeId="n" onJumpEvidence={jump} onGenerateDeepDive={generate} />);
    expect(screen.getByText("局部图 · 单次生成与自检")).toBeInTheDocument();
    await user.click(await screen.findByRole("button", { name: "跳到原文" }));
    expect(jump).toHaveBeenCalledWith({ pageNumber: 2, purpose: "设计定义" });
    expect(screen.queryByRole("button", { name: "生成局部图" })).not.toBeInTheDocument();
    expect(generate).not.toHaveBeenCalled();
  });

  it("requires explicit confirmation of a local plan and lets Escape dismiss it", async () => {
    const user = userEvent.setup(); const confirm = vi.fn(); const cancel = vi.fn();
    const pendingPlan = { plan: localPlan, nodeTitle: "设计选择" };
    const { rerender } = render(<OutlinePane {...props} pendingPlan={pendingPlan} onConfirmPlan={confirm} onCancelPlan={cancel} />);
    expect(screen.getByRole("dialog", { name: "生成局部关系图" })).toBeInTheDocument();
    expect(screen.getByText("局部构图，共 1 次调用，全程最多 1 次修复")).toBeInTheDocument();
    expect(confirm).not.toHaveBeenCalled();
    await user.keyboard("{Escape}"); expect(cancel).toHaveBeenCalledOnce();
    await user.click(screen.getByRole("button", { name: "确认生成" })); expect(confirm).toHaveBeenCalledOnce();
    rerender(<OutlinePane {...props} pendingPlan={pendingPlan} generateBusy onConfirmPlan={confirm} onCancelPlan={cancel} />);
    expect(screen.getByRole("button", { name: "入队中…" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "取消" })).toBeDisabled();
  });
});
