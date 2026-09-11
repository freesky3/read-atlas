import { describe, expect, it } from "vitest";
import {
  boxesOverlap,
  layoutOutlineGraph,
  narrativeEdges,
  OUTLINE_NODE_WIDTH,
  readingOrderIds,
  toFlowElements,
} from "./outlineLayout";
import type { OutlineEdge, OutlineGraph, OutlineNode } from "../types";

const node = (
  id: string,
  extras: Partial<OutlineNode> = {},
): OutlineNode => ({
  nodeId: id,
  roleClass: "method_design",
  title: id,
  takeaway: "A reasonably long takeaway that would previously overflow a short card.",
  importance: "core",
  sourceUnitIds: [],
  evidenceIds: [id],
  confidence: 1,
  ...extras,
});

const edge = (
  id: string,
  sourceNodeId: string,
  targetNodeId: string,
  extras: Partial<OutlineEdge> = {},
): OutlineEdge => ({
  edgeId: id,
  sourceNodeId,
  targetNodeId,
  tier: "narrative",
  relationClass: "dependency",
  label: "",
  rationale: "",
  evidenceIds: [],
  ...extras,
});

const graph: OutlineGraph = {
  title: "Map",
  summary: "A then B",
  nodes: [node("a", { title: "Question" }), node("b", { title: "Method" })],
  edges: [
    {
      edgeId: "n1",
      sourceNodeId: "a",
      targetNodeId: "b",
      tier: "narrative",
      relationClass: "dependency",
      label: "then",
      rationale: "",
      evidenceIds: ["b1"],
    },
    {
      edgeId: "c1",
      sourceNodeId: "b",
      targetNodeId: "a",
      tier: "cross_link",
      relationClass: "comparison",
      label: "vs",
      rationale: "",
      evidenceIds: [],
    },
  ],
};

describe("outline layout", () => {
  it("puts narrative edge labels on flow edges by default", () => {
    const { edges } = toFlowElements(graph, null, false);
    const narrative = edges.find((edge) => edge.id === "n1");
    expect(narrative?.label).toBe("then");
    expect(edges.some((edge) => edge.id === "c1")).toBe(false);
  });

  it("uses top-down narrative ranks and ignores cross_link cycles", () => {
    expect(narrativeEdges(graph.edges)).toHaveLength(1);
    const laid = layoutOutlineGraph(graph).nodes;
    const a = laid.find((item) => item.nodeId === "a");
    const b = laid.find((item) => item.nodeId === "b");
    expect(a && b).toBeTruthy();
    expect((a?.y ?? 0) < (b?.y ?? 0)).toBe(true);
    expect(a?.readingNumber).toBe(1);
    expect(b?.readingNumber).toBe(2);
  });

  it("keeps node boxes from overlapping", () => {
    const wide: OutlineGraph = {
      title: "Wide",
      summary: "",
      nodes: [node("a"), node("b"), node("c"), node("d")],
      edges: [
        {
          edgeId: "e1",
          sourceNodeId: "a",
          targetNodeId: "b",
          tier: "narrative",
          relationClass: "dependency",
          label: "",
          rationale: "",
          evidenceIds: [],
        },
        {
          edgeId: "e2",
          sourceNodeId: "a",
          targetNodeId: "c",
          tier: "narrative",
          relationClass: "dependency",
          label: "",
          rationale: "",
          evidenceIds: [],
        },
        {
          edgeId: "e3",
          sourceNodeId: "b",
          targetNodeId: "d",
          tier: "narrative",
          relationClass: "dependency",
          label: "",
          rationale: "",
          evidenceIds: [],
        },
        {
          edgeId: "e4",
          sourceNodeId: "c",
          targetNodeId: "d",
          tier: "narrative",
          relationClass: "dependency",
          label: "",
          rationale: "",
          evidenceIds: [],
        },
      ],
    };
    const laid = layoutOutlineGraph(wide).nodes;
    for (let i = 0; i < laid.length; i += 1) {
      for (let j = i + 1; j < laid.length; j += 1) {
        expect(boxesOverlap(laid[i], laid[j])).toBe(false);
      }
    }
  });

  it("layouts graphs with multiple narrative entries", () => {
    const multi: OutlineGraph = {
      title: "Multi",
      summary: "",
      nodes: [node("start-a"), node("start-b"), node("merge")],
      edges: [
        {
          edgeId: "e1",
          sourceNodeId: "start-a",
          targetNodeId: "merge",
          tier: "narrative",
          relationClass: "dependency",
          label: "",
          rationale: "",
          evidenceIds: [],
        },
        {
          edgeId: "e2",
          sourceNodeId: "start-b",
          targetNodeId: "merge",
          tier: "narrative",
          relationClass: "dependency",
          label: "",
          rationale: "",
          evidenceIds: [],
        },
      ],
    };
    expect(() => layoutOutlineGraph(multi)).not.toThrow();
    const order = readingOrderIds(multi);
    expect(order[order.length - 1]).toBe("merge");
  });

  it("spreads sibling narrative nodes farther than the card width", () => {
    const fork: OutlineGraph = {
      title: "Fork",
      summary: "",
      nodes: [node("q"), node("m1"), node("m2"), node("r")],
      edges: [
        edge("e1", "q", "m1"),
        edge("e2", "q", "m2"),
        edge("e3", "m1", "r"),
        edge("e4", "m2", "r"),
      ],
    };
    const laid = layoutOutlineGraph(fork).nodes;
    const m1 = laid.find((item) => item.nodeId === "m1")!;
    const m2 = laid.find((item) => item.nodeId === "m2")!;
    expect(Math.abs(m1.x - m2.x)).toBeGreaterThan(OUTLINE_NODE_WIDTH);
    expect(boxesOverlap(m1, m2, 24)).toBe(false);
  });

  it("keeps cycles, self-loops, and undirected edges in v4 layout", () => {
    const cyclic: OutlineGraph = {
      title: "Open",
      summary: "",
      nodes: [
        {
          nodeId: "a",
          title: "A",
          takeaway: "a",
          references: [{ blockId: "b1", pageNumber: 1, purpose: "出处" }],
        },
        {
          nodeId: "b",
          title: "B",
          takeaway: "b",
          references: [{ blockId: "b2", pageNumber: 2, purpose: "出处" }],
        },
      ],
      edges: [
        {
          edgeId: "e1",
          sourceNodeId: "a",
          targetNodeId: "b",
          direction: "directed",
          label: "揭示了两种定义的差别",
          rationale: "对照",
          references: [{ blockId: "b1", purpose: "对照" }],
        },
        {
          edgeId: "e2",
          sourceNodeId: "b",
          targetNodeId: "a",
          direction: "directed",
          label: "回指",
          rationale: "回路",
          references: [{ blockId: "b2", purpose: "回指" }],
        },
        {
          edgeId: "e3",
          sourceNodeId: "a",
          targetNodeId: "a",
          direction: "directed",
          label: "自反馈",
          rationale: "有依据",
          references: [{ blockId: "b1", purpose: "自指" }],
        },
      ],
    };
    const laid = layoutOutlineGraph(cyclic);
    expect(laid.nodes).toHaveLength(2);
    const flow = toFlowElements(cyclic, null, false);
    expect(flow.edges.map((edge) => edge.id).sort()).toEqual(["e1", "e2", "e3"]);
    expect(flow.edges.find((edge) => edge.id === "e3")?.source).toBe("a");
    expect(flow.edges.find((edge) => edge.id === "e3")?.target).toBe("a");
  });
});

describe("toFlowElements", () => {
  it("maps dagre boxes onto draggable flow nodes and hides cross_link by default", () => {
    const laid = layoutOutlineGraph(graph);
    const hidden = toFlowElements(graph, "a", false);
    const shown = toFlowElements(graph, "a", true);
    expect(hidden.nodes).toHaveLength(2);
    expect(hidden.nodes.every((item) => item.draggable === true)).toBe(true);
    expect(hidden.nodes.every((item) => item.type === "outlineNode")).toBe(true);
    const a = hidden.nodes.find((item) => item.id === "a");
    const box = laid.nodes.find((item) => item.nodeId === "a");
    expect(a?.position).toEqual({ x: box?.x, y: box?.y });
    expect(a?.data.readingNumber).toBe(1);
    expect(a?.data.selected).toBe(true);
    expect(hidden.edges.map((item) => item.id)).toEqual(["n1"]);
    expect(shown.edges.map((item) => item.id).sort()).toEqual(["c1", "n1"]);
    expect(shown.edges.find((item) => item.id === "c1")?.style?.strokeDasharray).toBe("6 5");
  });

  it("styles narrative labels as thin text instead of a solid card chip", () => {
    const { edges } = toFlowElements(graph, null, false);
    const narrative = edges.find((item) => item.id === "n1");
    expect(narrative?.labelStyle).toMatchObject({ fontSize: 10 });
    expect(String(narrative?.labelBgStyle?.fill ?? "")).not.toContain("--glass-card");
    expect((narrative as { draggable?: boolean } | undefined)?.draggable).not.toBe(
      true,
    );
  });
});
