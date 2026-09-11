import { uiText, zhT, type TranslateFn } from "../i18n/uiText";
import type {
  OutlineEdge,
  OutlineGraph,
  OutlineNode,
  OutlineReference,
} from "../types";

const ROLE_LABELS: Record<string, string> = {
  question_context: "问题背景",
  claim_hypothesis: "核心主张",
  concept_theory: "概念理论",
  method_design: "方法设计",
  mechanism_process: "机制过程",
  evidence_evaluation: "证据评估",
  result_finding: "结果发现",
  conclusion_implication: "结论启示",
  limitation_boundary: "局限边界",
  concept_intro: "概念引入",
  definition: "定义",
  example_illustration: "例题演示",
  derivation: "推导",
  algorithm_procedure: "算法流程",
  exercise_practice: "练习",
  caution_pitfall: "易错提醒",
  application_example: "应用举例",
  summary_recap: "小结",
};

export function isOutlineMapV4(graph: OutlineGraph): boolean {
  return (
    graph.nodes.some((node) => (node.references?.length ?? 0) > 0) ||
    graph.edges.some((edge) => Boolean(edge.direction)) ||
    (graph.groups?.length ?? 0) > 0 ||
    (graph.gaps?.length ?? 0) > 0
  );
}

export function displayRole(node: OutlineNode, t: TranslateFn = zhT): string | null {
  if (node.roleLabel && node.roleLabel.trim()) return node.roleLabel.trim();
  if (!node.roleClass) return null;
  if (node.roleClass === "other") return null;
  return uiText(t, ROLE_LABELS[node.roleClass] ?? node.roleClass);
}

export function nodeReferences(node: OutlineNode, t: TranslateFn = zhT): OutlineReference[] {
  if (node.references && node.references.length > 0) return node.references;
  return (node.evidenceIds ?? []).map((blockId) => ({
    blockId,
    purpose: uiText(t, "内容出处"),
  }));
}

export function edgeReferences(edge: OutlineEdge, t: TranslateFn = zhT): OutlineReference[] {
  if (edge.references && edge.references.length > 0) return edge.references;
  return (edge.evidenceIds ?? []).map((blockId) => ({
    blockId,
    purpose: uiText(t, "Relation evidence"),
  }));
}

export function isDirectedEdge(edge: OutlineEdge): boolean {
  if (edge.direction === "undirected") return false;
  return true;
}

export function showEdgeInDefaultView(edge: OutlineEdge, showCrossLinks: boolean): boolean {
  if (isOutlineMapV4({ title: "", summary: "", nodes: [], edges: [edge] })) {
    return true;
  }
  return edge.tier === "narrative" || showCrossLinks;
}

export function locatorLabel(
  reference: OutlineReference,
  catalog: { id: string; page: number; type: string }[],
  t: TranslateFn = zhT,
): { text: string; pageOnly: boolean } {
  if (reference.blockId) {
    const entry = catalog.find((item) => item.id === reference.blockId);
    return {
      text: entry
        ? `p.${entry.page} · ${entry.type}`
        : uiText(t, "Block {id}", { id: reference.blockId }),
      pageOnly: false,
    };
  }
  if (reference.pageNumber) {
    return { text: uiText(t, "Page reference · p.{page}", { page: reference.pageNumber }), pageOnly: true };
  }
  return { text: uiText(t, "No location"), pageOnly: false };
}

export function isOutlineV4Protocol(protocol: string | null | undefined): boolean {
  return protocol === "outline-map-v4" || protocol === "outline-draft-v4"
    || protocol === "outline-review-v4" || protocol === "outline-deep-dive-v4";
}
