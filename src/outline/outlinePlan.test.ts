import { describe, expect, it } from "vitest";
import {
  buildDeleteOutlineDeepDiveInvokeArgs,
  buildDeleteOutlineInvokeArgs,
  buildPlanOutlineInvokeArgs,
  buildStartOutlineInvokeArgs,
  classifyOutlinePlanError,
  formatOutlinePlanCard,
  outlineEmptyCopy,
  outlinePaneKind,
} from "./outlinePlan";
import type { OutlinePlan, OutlineProjection } from "../types";

const projection = (
  overrides: Partial<OutlineProjection> = {},
): OutlineProjection => ({
  status: "ready_to_plan",
  revisionId: "rev-1",
  ocrRevisionId: "ocr-1",
  hasPaperRoot: true,
  catalog: null,
  head: null,
  activeJobId: null,
  coverageWarnings: [],
  ...overrides,
});

const plan = (overrides: Partial<OutlinePlan> = {}): OutlinePlan => ({
  revisionId: "rev-1",
  ocrRevisionId: "ocr-1",
  catalogDigest: "abc123",
  model: "gemini-2.5-flash",
  extractCalls: 1,
  composeCalls: 1,
  maxRepairCalls: 1,
  pageCount: 12,
  catalogTokenEstimate: 240,
  pdfTokenEstimate: 3600,
  estimatedCost: null,
  hasPaperRoot: true,
  supportsNativePdf: true,
  ...overrides,
});

describe("plan_outline IPC contract", () => {
  it("wraps the revision in the Tauri request key", () => {
    expect(buildPlanOutlineInvokeArgs("rev-9")).toEqual({
      request: { revisionId: "rev-9" },
    });
    expect(Object.keys(buildPlanOutlineInvokeArgs("rev-9"))).toEqual([
      "request",
    ]);
    expect(buildStartOutlineInvokeArgs("rev-9")).toEqual({
      request: { revisionId: "rev-9" },
    });
    expect(buildStartOutlineInvokeArgs("rev-9", "plan-1")).toEqual({
      request: { revisionId: "rev-9", planId: "plan-1" },
    });
    expect(buildDeleteOutlineInvokeArgs("rev-9")).toEqual({
      request: { revisionId: "rev-9" },
    });
    expect(buildDeleteOutlineDeepDiveInvokeArgs("rev-9", "n1")).toEqual({
      request: { revisionId: "rev-9", nodeId: "n1" },
    });
  });
});

describe("outline pane empty states", () => {
  it("maps missing OCR, missing paper root, and over-window errors", () => {
    expect(
      outlinePaneKind({
        projection: projection({ status: "missing_ocr" }),
        plan: null,
        planError: null,
      }),
    ).toBe("missing_ocr");

    expect(
      outlinePaneKind({
        projection: projection({ hasPaperRoot: false }),
        plan: plan({ hasPaperRoot: false }),
        planError: null,
      }),
    ).toBe("missing_paper_root");

    expect(
      outlinePaneKind({
        projection: projection(),
        plan: null,
        planError: "Outline catalog and PDF exceed the model context window",
      }),
    ).toBe("over_window");

    expect(classifyOutlinePlanError("requires a published OCR revision")).toBe(
      "missing_ocr",
    );
    expect(outlineEmptyCopy("missing_ocr").title).toContain("OCR");
  });

  it("keeps a published map visible while an overview job regenerates", () => {
    expect(
      outlinePaneKind({
        projection: projection({
          status: "generating",
          head: {
            id: "h1",
            kind: "overview",
            status: "published",
            ocrRevisionId: "ocr-1",
            catalogDigest: "abc",
            protocolVersion: "outline-compose-v2",
            coverageWarnings: [],
            graph: { title: "Map", summary: "", nodes: [], edges: [] },
          },
        }),
        plan: plan(),
        planError: null,
        activeJob: {
          id: "job-1",
          kind: "outline_overview",
          provider: "gemini",
          paperId: "p",
          revisionId: "rev-1",
          rootKey: null,
          artifactKey: null,
          dedupeKey: "d",
          state: "running",
          stage: "composing",
          providerCommitted: true,
          priority: 80,
          payload: {},
          lastError: null,
          createdAt: "",
          updatedAt: "",
        },
      }),
    ).toBe("published");
  });

  it("treats an active job as generating even if the plan card is still present", () => {
    expect(
      outlinePaneKind({
        projection: projection(),
        plan: plan(),
        planError: null,
        activeJob: {
          id: "job-1",
          kind: "outline_overview",
          provider: "gemini",
          paperId: "p",
          revisionId: "rev-1",
          rootKey: null,
          artifactKey: null,
          dedupeKey: "d",
          state: "running",
          stage: "composing",
          providerCommitted: true,
          priority: 80,
          payload: {},
          lastError: null,
          createdAt: "",
          updatedAt: "",
        },
      }),
    ).toBe("generating");
  });

  it("formats a local plan card without inventing a cost", () => {
    const card = formatOutlinePlanCard(plan());
    expect(card.calls).toBe("2 次调用 + 最多 1 次 repair");
    expect(card.estimatedCost).toBe("未知");
    expect(card.model).toBe("gemini-2.5-flash");
    expect(card.ocrRevisionId).toBe("ocr-1");
  });
});


it("counts native-root initialization and local v4 calls without requiring an old root", () => {
  const local = plan({ workflow: "local_map", protocolVersion: "outline-deep-dive-v4", rootCalls: 1, extractCalls: 0, composeCalls: 1, hasPaperRoot: false });
  expect(formatOutlinePlanCard(local).calls).toBe("局部构图 + 初始化文档根，共 2 次调用，全程最多 1 次修复");
  expect(outlinePaneKind({ projection: projection({ hasPaperRoot: false }), plan: local, planError: null })).toBe("plan");
  expect(classifyOutlinePlanError("完整材料超过模型输入窗口，请切换模型")).toBe("over_window");
});


it("lets a failed candidate be regenerated from a fresh plan without treating it as published", () => {
  expect(outlinePaneKind({ projection: projection({ status: "partial" }), plan: null, planError: null })).toBe("partial");
  expect(outlinePaneKind({ projection: projection({ status: "partial" }), plan: plan({ protocolVersion: "outline-map-v4" }), planError: null })).toBe("plan");
});
