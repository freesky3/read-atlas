import { screen } from "@testing-library/react";
import { renderWithLocale as render } from "../i18n/testUtils";
import userEvent from "@testing-library/user-event";
import type { ComponentProps } from "react";
import { describe, expect, it, vi } from "vitest";
import OutlinePane from "./OutlinePane";
import type { JobProjection, OutlinePlan, OutlineProjection } from "../types";

const readyProjection: OutlineProjection = {
  status: "ready_to_plan",
  revisionId: "rev-1",
  ocrRevisionId: "ocr-1",
  hasPaperRoot: true,
  catalog: null,
  head: null,
  activeJobId: null,
  coverageWarnings: [],
};

const readyPlan: OutlinePlan = {
  revisionId: "rev-1",
  ocrRevisionId: "ocr-1",
  catalogDigest: "digest-1",
  model: "gemini-2.5-flash",
  extractCalls: 1,
  composeCalls: 1,
  maxRepairCalls: 1,
  pageCount: 8,
  catalogTokenEstimate: 120,
  pdfTokenEstimate: 2400,
  estimatedCost: null,
  hasPaperRoot: true,
  supportsNativePdf: true,
};

describe("OutlinePane", () => {
  it("shows the OCR empty state", () => {
    render(
      <OutlinePane
        projection={{ ...readyProjection, status: "missing_ocr", hasPaperRoot: false }}
        plan={null}
        planError={null}
        layout="pdf_outline"
        onOpenDiscussion={vi.fn()}
        onUseOutlineOnly={vi.fn()}
        onShowPdf={vi.fn()}
      />,
    );
    expect(screen.getByText("先完成 OCR")).toBeInTheDocument();
    expect(screen.queryByTestId("rf__wrapper")).not.toBeInTheDocument();
  });

  it("renders a local plan card with a disabled generate button", () => {
    render(
      <OutlinePane
        projection={readyProjection}
        plan={readyPlan}
        planError={null}
        layout="pdf_outline"
        onOpenDiscussion={vi.fn()}
        onUseOutlineOnly={vi.fn()}
        onShowPdf={vi.fn()}
      />,
    );
    expect(screen.getByText("生成论证地图")).toBeInTheDocument();
    expect(screen.getByText("2 次调用 + 最多 1 次 repair")).toBeInTheDocument();
    expect(screen.getByText("未知")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "生成地图" })).toBeDisabled();
    expect(screen.queryByTestId("rf__wrapper")).not.toBeInTheDocument();
  });

  it("lets the user leave the map preset", async () => {
    const user = userEvent.setup();
    const onOpenDiscussion = vi.fn();
    const onUseOutlineOnly = vi.fn();
    render(
      <OutlinePane
        projection={readyProjection}
        plan={readyPlan}
        planError={null}
        layout="pdf_outline"
        onOpenDiscussion={onOpenDiscussion}
        onUseOutlineOnly={onUseOutlineOnly}
        onShowPdf={vi.fn()}
      />,
    );
    await user.click(screen.getByRole("button", { name: "返回讨论" }));
    expect(onOpenDiscussion).toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "仅地图" }));
    expect(onUseOutlineOnly).toHaveBeenCalled();
  });

  it("shows live stage and tokens while generating", () => {
    const activeJob: JobProjection = {
      id: "job-outline",
      kind: "outline_overview",
      provider: "gemini",
      paperId: "paper-1",
      revisionId: "rev-1",
      rootKey: null,
      artifactKey: null,
      dedupeKey: "outline",
      state: "running",
      stage: "extracting",
      providerCommitted: true,
      priority: 80,
      payload: {},
      lastError: null,
      createdAt: "2026-08-17T00:00:00Z",
      updatedAt: "2026-08-17T00:00:01Z",
      progress: {
        step: 1,
        steps: 3,
        inputTokens: 900,
        outputTokens: 20,
      },
    };
    render(
      <OutlinePane
        projection={{ ...readyProjection, status: "generating" }}
        plan={readyPlan}
        planError={null}
        layout="pdf_outline"
        activeJob={activeJob}
        onOpenDiscussion={vi.fn()}
        onUseOutlineOnly={vi.fn()}
        onShowPdf={vi.fn()}
      />,
    );
    expect(screen.getByText("论证地图生成中")).toBeInTheDocument();
    expect(screen.getByText(/正在抽取论证单元 · 1 \/ 3/)).toBeInTheDocument();
    expect(screen.getByText(/入 900 · 出 20/)).toBeInTheDocument();
    expect(
      screen.getByText(/返回讨论不会中断生成/),
    ).toBeInTheDocument();
    expect(screen.queryByTestId("rf__wrapper")).not.toBeInTheDocument();
  });

  it("lazy-loads the published canvas without blocking the plan card path", async () => {
    const graph = {
      title: "Map",
      summary: "",
      nodes: [
        {
          nodeId: "n1",
          roleClass: "method_design",
          title: "RNN 模拟",
          takeaway: "用 RNN 做模拟",
          importance: "core",
          sourceUnitIds: [],
          evidenceIds: ["fig-1"],
          confidence: 1,
        },
      ],
      edges: [],
    };
    render(
      <OutlinePane
        projection={{
          ...readyProjection,
          status: "published",
          head: {
            id: "head-1",
            kind: "overview",
            status: "published",
            ocrRevisionId: "ocr-1",
            catalogDigest: "digest-1",
            protocolVersion: "outline-compose-v1",
            coverageWarnings: [],
            graph,
          },
        }}
        plan={readyPlan}
        planError={null}
        layout="pdf_outline"
        selectedNodeId={null}
        onSelectNode={vi.fn()}
        onJumpEvidence={vi.fn()}
        onOpenDiscussion={vi.fn()}
        onUseOutlineOnly={vi.fn()}
        onShowPdf={vi.fn()}
      />,
    );
    expect(
      await screen.findByText("RNN 模拟", undefined, { timeout: 10_000 }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("论证地图")).toBeInTheDocument();
  }, 10000);

  it("keeps the published canvas and shows a progress banner while regenerating", async () => {
    const graph = {
      title: "Map",
      summary: "",
      nodes: [
        {
          nodeId: "n1",
          roleClass: "method_design",
          title: "RNN 模拟",
          takeaway: "用 RNN 做模拟",
          importance: "core",
          sourceUnitIds: [],
          evidenceIds: ["fig-1"],
          confidence: 1,
        },
      ],
      edges: [],
    };
    render(
      <OutlinePane
        projection={{
          ...readyProjection,
          status: "generating",
          head: {
            id: "head-1",
            kind: "overview",
            status: "published",
            ocrRevisionId: "ocr-1",
            catalogDigest: "digest-1",
            protocolVersion: "outline-compose-v2",
            coverageWarnings: [],
            graph,
          },
        }}
        plan={readyPlan}
        planError={null}
        layout="pdf_outline"
        activeJob={{
          id: "job-outline",
          kind: "outline_overview",
          provider: "gemini",
          paperId: "paper-1",
          revisionId: "rev-1",
          rootKey: null,
          artifactKey: null,
          dedupeKey: "outline",
          state: "running",
          stage: "extracting",
          providerCommitted: true,
          priority: 80,
          payload: {},
          lastError: null,
          createdAt: "2026-08-17T00:00:00Z",
          updatedAt: "2026-08-17T00:00:01Z",
          progress: { step: 1, steps: 3, inputTokens: 900, outputTokens: 20 },
        }}
        onOpenDiscussion={vi.fn()}
        onUseOutlineOnly={vi.fn()}
        onShowPdf={vi.fn()}
      />,
    );
    expect(
      await screen.findByText("RNN 模拟", undefined, { timeout: 10_000 }),
    ).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent(/返回讨论不会中断生成/);
  }, 10_000);

  it("remounts the canvas when the overview head or deep-dive graph changes", async () => {
    const graph = {
      title: "Map",
      summary: "",
      nodes: [
        {
          nodeId: "n1",
          roleClass: "method_design",
          title: "RNN 模拟",
          takeaway: "用 RNN 做模拟",
          importance: "core",
          sourceUnitIds: [],
          evidenceIds: ["fig-1"],
          confidence: 1,
        },
      ],
      edges: [],
    };
    const published = {
      ...readyProjection,
      status: "published" as const,
      head: {
        id: "head-1",
        kind: "overview",
        status: "published",
        ocrRevisionId: "ocr-1",
        catalogDigest: "digest-1",
        protocolVersion: "outline-compose-v1",
        coverageWarnings: [],
        graph,
      },
    };
    const pane = (extras: Partial<ComponentProps<typeof OutlinePane>> = {}) => (
      <OutlinePane
        projection={published}
        plan={readyPlan}
        planError={null}
        layout="pdf_outline"
        selectedNodeId={null}
        onSelectNode={vi.fn()}
        onJumpEvidence={vi.fn()}
        onOpenDiscussion={vi.fn()}
        onUseOutlineOnly={vi.fn()}
        onShowPdf={vi.fn()}
        {...extras}
      />
    );

    const { rerender } = render(pane());
    const first = await screen.findByTestId("rf__wrapper", undefined, {
      timeout: 10_000,
    });
    expect(document.querySelector("[data-outline-canvas]")).toHaveAttribute(
      "data-outline-canvas",
      "head-1",
    );

    rerender(
      pane({
        projection: {
          ...published,
          head: { ...published.head, id: "head-2" },
        },
      }),
    );
    const afterHead = await screen.findByTestId("rf__wrapper");
    expect(afterHead).not.toBe(first);
    expect(document.querySelector("[data-outline-canvas]")).toHaveAttribute(
      "data-outline-canvas",
      "head-2",
    );

    rerender(
      pane({
        deepDiveGraph: {
          title: "Local",
          summary: "",
          nodes: [
            {
              nodeId: "d1",
              roleClass: "method_design",
              title: "局部节点",
              takeaway: "只看这一支",
              importance: "core",
              sourceUnitIds: [],
              evidenceIds: [],
              confidence: 1,
            },
          ],
          edges: [],
        },
      }),
    );
    const afterDeep = await screen.findByTestId("rf__wrapper");
    expect(afterDeep).not.toBe(afterHead);
    expect(document.querySelector("[data-outline-canvas]")).toHaveAttribute(
      "data-outline-canvas",
      "deep-dive",
    );
    expect(screen.getByText("局部节点")).toBeInTheDocument();
  });

  const publishedProjection: OutlineProjection = {
    ...readyProjection,
    status: "published",
    head: {
      id: "head-1",
      kind: "overview",
      status: "published",
      ocrRevisionId: "ocr-1",
      catalogDigest: "digest-1",
      protocolVersion: "outline-compose-v2",
      coverageWarnings: [],
      graph: {
        title: "Map",
        summary: "",
        nodes: [
          {
            nodeId: "n1",
            roleClass: "method_design",
            title: "RNN 模拟",
            takeaway: "用 RNN 做模拟",
            importance: "core",
            sourceUnitIds: [],
            evidenceIds: ["fig-1"],
            confidence: 1,
          },
        ],
        edges: [],
      },
    },
  };

  it("asks for confirmation before deleting a published overview", async () => {
    const user = userEvent.setup();
    const onDelete = vi.fn();
    render(
      <OutlinePane
        projection={publishedProjection}
        plan={readyPlan}
        planError={null}
        layout="pdf_outline"
        onOpenDiscussion={vi.fn()}
        onUseOutlineOnly={vi.fn()}
        onShowPdf={vi.fn()}
        onDeleteOverview={onDelete}
        onRegenerateOverview={vi.fn()}
      />,
    );
    await user.click(screen.getByRole("button", { name: "删除" }));
    expect(onDelete).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "确认删除" }));
    expect(onDelete).toHaveBeenCalledTimes(1);
  });

  it("disables regenerate and delete while an overview job is running", () => {
    render(
      <OutlinePane
        projection={publishedProjection}
        plan={readyPlan}
        planError={null}
        layout="pdf_outline"
        overviewBusy
        activeJob={{
          id: "job-outline",
          kind: "outline_overview",
          provider: "gemini",
          paperId: "paper-1",
          revisionId: "rev-1",
          rootKey: null,
          artifactKey: null,
          dedupeKey: "outline",
          state: "running",
          stage: "extracting",
          providerCommitted: true,
          priority: 80,
          payload: {},
          lastError: null,
          createdAt: "2026-08-17T00:00:00Z",
          updatedAt: "2026-08-17T00:00:01Z",
        }}
        onOpenDiscussion={vi.fn()}
        onUseOutlineOnly={vi.fn()}
        onShowPdf={vi.fn()}
        onDeleteOverview={vi.fn()}
        onRegenerateOverview={vi.fn()}
      />,
    );
    expect(screen.getByRole("button", { name: "重生成" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "删除" })).toBeDisabled();
  });
});
