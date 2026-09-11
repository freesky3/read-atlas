import { uiText, zhT, type TranslateFn } from "../i18n/uiText";
import { isOutlineV4Protocol } from "./outlineDisplay";
import type { JobProjection, OutlinePlan, OutlineProjection, OutlineStatus } from "../types";
import { isActiveOutlineJob } from "./outlineProgress";

export function buildPlanOutlineInvokeArgs(revisionId: string) {
  return {
    request: {
      revisionId,
    },
  };
}

export function buildStartOutlineInvokeArgs(
  revisionId: string,
  planId?: string | null,
  planDigest?: string | null,
) {
  return {
    request: {
      revisionId,
      ...(planId ? { planId } : {}),
      ...(planDigest ? { planDigest } : {}),
    },
  };
}

export function buildDeleteOutlineInvokeArgs(revisionId: string) {
  return {
    request: {
      revisionId,
    },
  };
}

export function buildDeleteOutlineDeepDiveInvokeArgs(
  revisionId: string,
  nodeId: string,
) {
  return {
    request: {
      revisionId,
      nodeId,
    },
  };
}

export type OutlineEmptyKind =
  | "missing_ocr"
  | "missing_paper_root"
  | "over_window"
  | "unsupported_pdf";

export type OutlinePaneKind =
  | OutlineEmptyKind
  | "plan"
  | "generating"
  | "partial"
  | "published"
  | "stale";

export function classifyOutlinePlanError(error: string): OutlineEmptyKind | "other" {
  const text = error.toLowerCase();
  if (text.includes("exceed") || text.includes("window") || text.includes("超出") || text.includes("超过") || text.includes("窗口")) return "over_window";
  if (text.includes("native pdf") || text.includes("does not support") || (text.includes("不支持") && text.includes("pdf"))) {
    return "unsupported_pdf";
  }
  if (text.includes("ocr")) return "missing_ocr";
  return "other";
}

export function outlinePaneKind(input: {
  projection: OutlineProjection | null;
  plan: OutlinePlan | null;
  planError: string | null;
  activeJob?: JobProjection | null;
}): OutlinePaneKind {
  const hasGraph = Boolean(input.projection?.head?.graph);
  if (
    input.activeJob &&
    isActiveOutlineJob(input.activeJob) &&
    input.activeJob.kind === "outline_overview" &&
    !hasGraph
  ) {
    return "generating";
  }
  if (hasGraph) return input.projection?.status === "stale" ? "stale" : "published";
  if (input.planError) {
    const classified = classifyOutlinePlanError(input.planError);
    if (classified !== "other") return classified;
  }

  const status: OutlineStatus | null = input.projection?.status ?? null;
  if (status === "missing_ocr") return "missing_ocr";
  if (status === "generating" && !hasGraph) return "generating";
  if (status === "partial") return input.plan ? "plan" : "partial";
  if (status === "stale") return "stale";
  if (status === "published" || hasGraph) return "published";

  const canCreateRoot = isOutlineV4Protocol(input.plan?.protocolVersion);
  if (!canCreateRoot && input.projection && !input.projection.hasPaperRoot) {
    return "missing_paper_root";
  }
  if (!canCreateRoot && input.plan && !input.plan.hasPaperRoot) {
    return "missing_paper_root";
  }
  return "plan";
}

export function formatOutlinePlanCard(plan: OutlinePlan, t: TranslateFn = zhT) {
  const v4 = plan.workflow === "draft_review" || plan.workflow === "local_map" || isOutlineV4Protocol(plan.protocolVersion);
  return {
    title: uiText(t, plan.workflow === "local_map" ? "Generate local map" : v4 ? "Generate content map" : "Generate argument map"),
    model: plan.model,
    ocrRevisionId: plan.ocrRevisionId,
    catalogDigest: plan.catalogDigest,
    calls: v4
      ? uiText(t, "{workflow}{root}, {calls} calls, at most {repairs} repairs", {
        workflow: uiText(t, plan.workflow === "local_map" ? "Local composition" : "Draft + review"),
        root: plan.rootCalls ? uiText(t, " + initialize document context") : "",
        calls: (plan.rootCalls ?? 0) + plan.extractCalls + plan.composeCalls, repairs: plan.maxRepairCalls })
      : uiText(t, "{calls} calls + at most {repairs} repairs", { calls: plan.extractCalls + plan.composeCalls, repairs: plan.maxRepairCalls }),
    estimatedCost: plan.estimatedCost ?? uiText(t, "Unknown"),
    pages: uiText(t, "{count} pages", { count: plan.pageCount }),
  };
}

export function outlineEmptyCopy(kind: OutlineEmptyKind, t: TranslateFn = zhT) {
  switch (kind) {
    case "missing_ocr":
      return {
        title: uiText(t, "先完成 OCR"),
        body: uiText(t, "论证地图需要已发布的 OCR Block 才能定位证据。请先在顶栏运行整篇 OCR。"),
      };
    case "missing_paper_root":
      return {
        title: uiText(t, "需要文档根"),
        body: uiText(t, "地图从文档根旁支生成。请先初始化文档上下文（与首次 Chat / Lens 相同），成功后再回到这里确认计划。"),
      };
    case "over_window":
      return {
        title: uiText(t, "超出模型窗口"),
        body: uiText(t, "当前 PDF 与 OCR 目录估计会超过阅读模型上下文。不会截断中间页，也不会入队付费任务。"),
      };
    case "unsupported_pdf":
      return {
        title: uiText(t, "模型不支持原生 PDF"),
        body: uiText(t, "请更换支持原生 PDF 的阅读模型后再计划 Outline。"),
      };
  }
}
