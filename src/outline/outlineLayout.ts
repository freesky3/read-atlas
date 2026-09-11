import { uiText, zhT, type TranslateFn } from "../i18n/uiText";
import dagre, { graphlib } from "@dagrejs/dagre";
import type { Edge, Node } from "@xyflow/react";
import { MarkerType } from "@xyflow/react";
import type { OutlineEdge, OutlineGraph, OutlineNode } from "../types";
import { isDirectedEdge, isOutlineMapV4 } from "./outlineDisplay";
export const OUTLINE_NODE_WIDTH = 224;
export const OUTLINE_NODE_HEIGHT = 84;
export const OUTLINE_NODE_SEP = 148;
export const OUTLINE_RANK_SEP = 112;
const VIRTUAL_ROOT = "__outline_virtual_root__";
const MARGIN = 56;

export function narrativeEdgeStyle(tier: string): {
  strokeWidth: number;
  strokeDasharray?: string;
} {
  return {
    strokeWidth: tier === "narrative" ? 1.5 : 1.25,
    strokeDasharray: tier === "cross_link" ? "6 5" : undefined,
  };
}

export function narrativeLabelStyle(): {
  fill: string;
  fontSize: number;
  fontWeight: number;
} {
  return {
    fill: "var(--muted)",
    fontSize: 10,
    fontWeight: 500,
  };
}

export function narrativeLabelBgStyle(): {
  fill: string;
  fillOpacity: number;
} {
  return {
    fill: "var(--glass-surface)",
    fillOpacity: 0.35,
  };
}

export type LaidOutNode = OutlineNode & {
  x: number;
  y: number;
  width: number;
  height: number;
  rank: number;
  readingNumber: number;
};

export type OutlineLayout = {
  nodes: LaidOutNode[];
  bounds: { width: number; height: number };
};

export function narrativeEdges(edges: OutlineEdge[]) {
  return edges.filter((edge) => edge.tier === "narrative");
}

export function readingOrderIds(graph: OutlineGraph): string[] {
  const narrative = narrativeEdges(graph.edges);
  const incoming = new Map(graph.nodes.map((node) => [node.nodeId, 0]));
  const outgoing = new Map(graph.nodes.map((node) => [node.nodeId, [] as string[]]));
  for (const edge of narrative) {
    if (!incoming.has(edge.sourceNodeId) || !incoming.has(edge.targetNodeId)) {
      continue;
    }
    incoming.set(edge.targetNodeId, (incoming.get(edge.targetNodeId) ?? 0) + 1);
    outgoing.get(edge.sourceNodeId)?.push(edge.targetNodeId);
  }
  const queue = graph.nodes
    .filter((node) => (incoming.get(node.nodeId) ?? 0) === 0)
    .map((node) => node.nodeId)
    .sort((left, right) => left.localeCompare(right));
  const ordered: string[] = [];
  const seen = new Set<string>();
  while (queue.length > 0) {
    const current = queue.shift()!;
    if (seen.has(current)) continue;
    seen.add(current);
    ordered.push(current);
    const next = [...(outgoing.get(current) ?? [])].sort((left, right) =>
      left.localeCompare(right),
    );
    for (const id of next) {
      incoming.set(id, (incoming.get(id) ?? 1) - 1);
      if ((incoming.get(id) ?? 0) === 0) queue.push(id);
    }
    queue.sort((left, right) => left.localeCompare(right));
  }
  for (const node of graph.nodes) {
    if (!seen.has(node.nodeId)) ordered.push(node.nodeId);
  }
  return ordered;
}

export function layoutOutlineGraph(graph: OutlineGraph): OutlineLayout {
  if (isOutlineMapV4(graph)) return layoutOpenGraph(graph);
  const ordered = [...graph.nodes].sort((left, right) =>
    left.nodeId.localeCompare(right.nodeId),
  );
  const dagreGraph = new graphlib.Graph({ multigraph: true });
  dagreGraph.setGraph({
    rankdir: "TB",
    ranker: "network-simplex",
    nodesep: OUTLINE_NODE_SEP,
    ranksep: OUTLINE_RANK_SEP,
    marginx: MARGIN,
    marginy: MARGIN,
  });
  dagreGraph.setDefaultEdgeLabel(() => ({}));

  for (const node of ordered) {
    dagreGraph.setNode(node.nodeId, {
      width: OUTLINE_NODE_WIDTH,
      height: OUTLINE_NODE_HEIGHT,
    });
  }

  const narrative = [...narrativeEdges(graph.edges)].sort((left, right) =>
    left.edgeId.localeCompare(right.edgeId),
  );
  for (const edge of narrative) {
    if (!dagreGraph.hasNode(edge.sourceNodeId) || !dagreGraph.hasNode(edge.targetNodeId)) {
      continue;
    }
    dagreGraph.setEdge(
      edge.sourceNodeId,
      edge.targetNodeId,
      { weight: 3, minlen: 1 },
      edge.edgeId,
    );
  }

  const incoming = new Map(ordered.map((node) => [node.nodeId, 0]));
  for (const edge of narrative) {
    if (incoming.has(edge.targetNodeId)) {
      incoming.set(edge.targetNodeId, (incoming.get(edge.targetNodeId) ?? 0) + 1);
    }
  }
  const entries = ordered
    .filter((node) => (incoming.get(node.nodeId) ?? 0) === 0)
    .map((node) => node.nodeId);
  if (entries.length > 1) {
    dagreGraph.setNode(VIRTUAL_ROOT, { width: 1, height: 1 });
    for (const nodeId of entries) {
      dagreGraph.setEdge(
        VIRTUAL_ROOT,
        nodeId,
        { weight: 1, minlen: 1 },
        `${VIRTUAL_ROOT}:${nodeId}`,
      );
    }
  }

  dagre.layout(dagreGraph);

  const numbers = new Map(
    readingOrderIds(graph).map((id, index) => [id, index + 1]),
  );
  const laid: LaidOutNode[] = ordered.map((node) => {
    const position = dagreGraph.node(node.nodeId) as {
      x: number;
      y: number;
      width: number;
      height: number;
    };
    const width = position.width || OUTLINE_NODE_WIDTH;
    const height = position.height || OUTLINE_NODE_HEIGHT;
    return {
      ...node,
      x: position.x - width / 2,
      y: position.y - height / 2,
      width,
      height,
      rank: Math.round(
        (position.y - height / 2 - MARGIN) / Math.max(1, height + OUTLINE_RANK_SEP),
      ),
      readingNumber: numbers.get(node.nodeId) ?? 0,
    };
  });

  const graphBounds = dagreGraph.graph() as { width?: number; height?: number };
  return {
    nodes: laid,
    bounds: {
      width: Math.max(320, graphBounds.width ?? 0),
      height: Math.max(480, graphBounds.height ?? 0),
    },
  };
}

export function boxesOverlap(
  left: { x: number; y: number; width: number; height: number },
  right: { x: number; y: number; width: number; height: number },
  gap = 8,
) {
  return !(
    left.x + left.width + gap <= right.x ||
    right.x + right.width + gap <= left.x ||
    left.y + left.height + gap <= right.y ||
    right.y + right.height + gap <= left.y
  );
}

export function primaryEvidenceId(node: OutlineNode) {
  return (
    node.evidenceIds?.[0] ??
    node.references?.find((item) => item.blockId)?.blockId ??
    null
  );
}

export type OutlineFlowNodeData = {
  node: OutlineNode;
  readingNumber: number;
  selected: boolean;
};

export type OutlineFlowNode = Node<OutlineFlowNodeData, "outlineNode">;
export type OutlineFlowEdge = Edge<{ tier?: string; outlineEdge: OutlineEdge; lane?: number; active?: boolean; onSelect?: (id: string) => void }>;
type FlowHandlePosition = NonNullable<
  NonNullable<OutlineFlowNode["handles"]>[number]["position"]
>;

export function toFlowElements(
  graph: OutlineGraph,
  selectedNodeId: string | null,
  showCrossLinks: boolean,
  t: TranslateFn = zhT,
): { nodes: OutlineFlowNode[]; edges: OutlineFlowEdge[] } {
  const v4 = isOutlineMapV4(graph);
  const laid = layoutOutlineGraph(graph);
  const nodes: OutlineFlowNode[] = laid.nodes.map((item) => ({
    id: item.nodeId,
    type: "outlineNode",
    position: { x: item.x, y: item.y },
    width: item.width,
    height: item.height,
    draggable: true,
    selectable: true,
    focusable: true,
    ariaLabel: uiText(t, "Node: {title}", { title: item.title }),
    data: {
      node: item,
      readingNumber: item.readingNumber,
      selected: item.nodeId === selectedNodeId,
    },
    style: { width: item.width, height: item.height },
    handles: [
      {
        type: "target",
        position: "top" as FlowHandlePosition,
        x: item.width / 2 - 2,
        y: 0,
        width: 4,
        height: 4,
      },
      {
        type: "source",
        position: "bottom" as FlowHandlePosition,
        x: item.width / 2 - 2,
        y: item.height - 4,
        width: 4,
        height: 4,
      },
    ],
  }));
  const pairKey = (edge: OutlineEdge) => JSON.stringify([edge.sourceNodeId, edge.targetNodeId].sort());
  const pairs = new Map<string, string[]>();
  for (const edge of graph.edges) {
    const key = pairKey(edge);
    pairs.set(key, [...(pairs.get(key) ?? []), edge.edgeId].sort());
  }
  const edges: OutlineFlowEdge[] = graph.edges
    .filter((edge) =>
      v4 ? true : edge.tier === "narrative" || showCrossLinks,
    )
    .filter(
      (edge) =>
        laid.nodes.some((item) => item.nodeId === edge.sourceNodeId) &&
        laid.nodes.some((item) => item.nodeId === edge.targetNodeId),
    )
    .map((edge) => {
      const siblings = pairs.get(pairKey(edge)) ?? [edge.edgeId];
      const index = siblings.indexOf(edge.edgeId);
      const selfLoop = edge.sourceNodeId === edge.targetNodeId;
      const directed = isDirectedEdge(edge);
      return {
        id: edge.edgeId,
        type: v4 ? "outlineRelation" : "smoothstep",
        source: edge.sourceNodeId,
        target: edge.targetNodeId,
        label: edge.label.trim() || undefined,
        labelStyle: narrativeLabelStyle(),
        labelBgStyle: narrativeLabelBgStyle(),
        labelBgPadding: [1, 3] as [number, number],
        data: { tier: edge.tier, outlineEdge: edge, lane: selfLoop ? index : index - (siblings.length - 1) / 2 },
        selectable: true,
        focusable: true,
        ariaLabel: uiText(t, "Relation: {label}", { label: edge.label }),
        markerEnd: directed
          ? { type: MarkerType.ArrowClosed, width: 16, height: 16 }
          : undefined,
        style: narrativeEdgeStyle(edge.tier ?? (directed ? "narrative" : "cross_link")),
        pathOptions: index > 0 || selfLoop ? { offset: 18 + index * 12 } : undefined,
      };
    });
  return { nodes, edges };
}

export function narrativePathTo(
  graph: OutlineGraph,
  targetId: string,
): Set<string> {
  const narrative = narrativeEdges(graph.edges);
  const parents = new Map<string, string[]>();
  for (const edge of narrative) {
    const list = parents.get(edge.targetNodeId) ?? [];
    list.push(edge.sourceNodeId);
    parents.set(edge.targetNodeId, list);
  }
  const path = new Set<string>();
  const stack = [targetId];
  while (stack.length > 0) {
    const current = stack.pop()!;
    if (path.has(current)) continue;
    path.add(current);
    stack.push(...(parents.get(current) ?? []));
  }
  return path;
}

function weaklyConnectedIds(nodeIds: string[], edges: OutlineEdge[]): string[][] {
  const parent = new Map(nodeIds.map((id) => [id, id]));
  const find = (id: string): string => {
    const current = parent.get(id) ?? id;
    if (current !== id) parent.set(id, find(current));
    return parent.get(id) ?? id;
  };
  const unite = (a: string, b: string) => {
    const pa = find(a);
    const pb = find(b);
    if (pa !== pb) parent.set(pa, pb);
  };
  for (const edge of edges) {
    if (parent.has(edge.sourceNodeId) && parent.has(edge.targetNodeId)) {
      unite(edge.sourceNodeId, edge.targetNodeId);
    }
  }
  const groups = new Map<string, string[]>();
  for (const id of [...nodeIds].sort((a, b) => a.localeCompare(b))) {
    const root = find(id);
    const list = groups.get(root) ?? [];
    list.push(id);
    groups.set(root, list);
  }
  return [...groups.values()];
}

function stronglyConnectedIds(nodeIds: string[], edges: OutlineEdge[]): string[][] {
  const directed = edges.filter(
    (edge) =>
      isDirectedEdge(edge) &&
      nodeIds.includes(edge.sourceNodeId) &&
      nodeIds.includes(edge.targetNodeId),
  );
  const index = new Map<string, number>();
  const low = new Map<string, number>();
  const stack: string[] = [];
  const onStack = new Set<string>();
  const result: string[][] = [];
  let next = 0;
  const adj = new Map(nodeIds.map((id) => [id, [] as string[]]));
  for (const edge of directed) {
    adj.get(edge.sourceNodeId)?.push(edge.targetNodeId);
  }
  const visit = (id: string) => {
    index.set(id, next);
    low.set(id, next);
    next += 1;
    stack.push(id);
    onStack.add(id);
    for (const nxt of adj.get(id) ?? []) {
      if (!index.has(nxt)) {
        visit(nxt);
        low.set(id, Math.min(low.get(id) ?? 0, low.get(nxt) ?? 0));
      } else if (onStack.has(nxt)) {
        low.set(id, Math.min(low.get(id) ?? 0, index.get(nxt) ?? 0));
      }
    }
    if (low.get(id) === index.get(id)) {
      const scc: string[] = [];
      while (stack.length > 0) {
        const item = stack.pop()!;
        onStack.delete(item);
        scc.push(item);
        if (item === id) break;
      }
      result.push(scc.sort((a, b) => a.localeCompare(b)));
    }
  };
  for (const id of [...nodeIds].sort((a, b) => a.localeCompare(b))) {
    if (!index.has(id)) visit(id);
  }
  return result;
}

function layoutOpenGraph(graph: OutlineGraph): OutlineLayout {
  const numbers = new Map(
    [...graph.nodes]
      .map((node) => node.nodeId)
      .sort((a, b) => a.localeCompare(b))
      .map((id, index) => [id, index + 1]),
  );
  const components = weaklyConnectedIds(
    graph.nodes.map((node) => node.nodeId),
    graph.edges,
  );
  const laid: LaidOutNode[] = [];
  let offsetX = MARGIN;
  let maxHeight = 0;
  for (const component of components) {
    const sccs = stronglyConnectedIds(component, graph.edges);
    const sccOf = new Map<string, string>();
    for (const scc of sccs) {
      const key = JSON.stringify(scc);
      for (const id of scc) sccOf.set(id, key);
    }
    const dagreGraph = new graphlib.Graph({ multigraph: true });
    dagreGraph.setGraph({
      rankdir: "TB",
      nodesep: OUTLINE_NODE_SEP,
      ranksep: OUTLINE_RANK_SEP,
      marginx: 24,
      marginy: 24,
    });
    dagreGraph.setDefaultEdgeLabel(() => ({}));
    for (const scc of sccs) {
      dagreGraph.setNode(JSON.stringify(scc), {
        width: OUTLINE_NODE_WIDTH * scc.length + OUTLINE_NODE_SEP * Math.max(0, scc.length - 1),
        height: OUTLINE_NODE_HEIGHT,
      });
    }
    const seen = new Set<string>();
    for (const edge of graph.edges) {
      if (!isDirectedEdge(edge)) continue;
      const source = sccOf.get(edge.sourceNodeId);
      const target = sccOf.get(edge.targetNodeId);
      if (!source || !target || source === target) continue;
      const key = `${source}->${target}`;
      if (seen.has(key)) continue;
      seen.add(key);
      dagreGraph.setEdge(source, target);
    }
    dagre.layout(dagreGraph);
    let localMaxX = 0;
    let localMaxY = 0;
    for (const scc of sccs) {
      const key = JSON.stringify(scc);
      const position = dagreGraph.node(key) as {
        x: number;
        y: number;
        width: number;
        height: number;
      };
      scc.forEach((id, index) => {
        const node = graph.nodes.find((item) => item.nodeId === id);
        if (!node) return;
        const x =
          offsetX +
          (position?.x ?? 0) -
          ((position?.width ?? OUTLINE_NODE_WIDTH) / 2) +
          index * (OUTLINE_NODE_WIDTH + OUTLINE_NODE_SEP);
        const y = (position?.y ?? 0) - OUTLINE_NODE_HEIGHT / 2;
        laid.push({
          ...node,
          x,
          y,
          width: OUTLINE_NODE_WIDTH,
          height: OUTLINE_NODE_HEIGHT,
          rank: Math.round(y / Math.max(1, OUTLINE_NODE_HEIGHT + OUTLINE_RANK_SEP)),
          readingNumber: numbers.get(id) ?? 0,
        });
        localMaxX = Math.max(localMaxX, x + OUTLINE_NODE_WIDTH);
        localMaxY = Math.max(localMaxY, y + OUTLINE_NODE_HEIGHT);
      });
    }
    offsetX = localMaxX + 96;
    maxHeight = Math.max(maxHeight, localMaxY);
  }
  return {
    nodes: laid,
    bounds: {
      width: Math.max(320, offsetX),
      height: Math.max(480, maxHeight + MARGIN),
    },
  };
}
