# Outline React Flow 画布 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 Full Outline 已发布地图从自制绝对定位画布换成与 `read_addon` 对齐的 dagre TB + `@xyflow/react` 画布：不重叠、可平移缩放、节点不可拖、阅读序号、单击跳 PDF、inspector 出 Deep dive；xyflow/dagre 不进首屏。

**Architecture:** 布局仍由桌面自有 `layoutOutlineGraph` 在主线程算出（10–18 节点，不搬 addon Worker）。`OutlineCanvas` 只负责把 `LaidOutNode` 交给 React Flow 自定义节点 + SmoothStep 边。`OutlinePane` 用 `React.lazy` 载入画布。inspector、Esc、跳 Block、Deep dive Job 继续由现有 `App.tsx` / `OutlineModule` 拥有。不改图合同、不改 Job、不改 Rust。

**Tech Stack:** React 19, TypeScript, Vitest + Testing Library, `@xyflow/react` ^12.11, `@dagrejs/dagre` ^3.1, Vite 6 manual chunk `outline-graph`, 现有 `--glass-*` / `--liquid-glass-*` / `--ink` / `--muted`。

**Spec:** [docs/full-outline-v1.md](../../full-outline-v1.md) §9.3–9.4；产品拍板「对齐参考项目的布局与交互，桌面自有卡片文案 / 阅读序号 / 跳 PDF / Deep dive」。本计划替换 [2026-08-17-full-outline-implementation.md](./2026-08-17-full-outline-implementation.md) Task 5 的自制画布，不重做 Task 1–4 / 6–8。

## Global Constraints

- Vitest 必须串行：`npx vitest run --maxWorkers=1 --fileParallelism=false`。
- 视口锁定：`100vh` / `overflow: hidden`；PDF 与地图栏各自滚动。`zoomHotkeysEnabled` 保持 `false`。
- 样式只用已有 `--glass-*` / `--liquid-glass-*` / `--ink` / `--muted` / `--line`。不要引入 addon 的 `--outline-*` token，不要 Tailwind。
- 节点不可自由拖拽。V1 **不要** MiniMap。V1 **不要** layout Worker。
- 只让 `tier === "narrative"` 参与 dagre 分层。`cross_link` 默认隐藏，inspector 提供开关。
- 单击节点只调用已有 `onSelectNode`；由 `App.tsx` 跳第一条证据并高亮。画布禁止再调 `start_outline` / `start_outline_deep_dive`。
- Esc 仍由 `App.tsx` + `consumeEscapeForOutline` 分层。画布不要 `stopPropagation` Escape，也不要第二套清选中。
- Deep dive 已存在：inspector「生成 / 打开局部图」、面包屑「返回总图」。本计划只换图画法，不改 Job / 白名单 / head。
- lazy chunk，不进 Reader 首屏。本地 ErrorBoundary：布局或画布抛错只退回可读列表，不准空白整个 Reader。
- 禁止改：`src-tauri/**`、`desktopClient.ts` 命令、`outline_module.rs`、图 JSON schema、`workspaceLayout` 预设语义、进度条 / 后台 banner。
- 提交前：串行 Vitest、`npx tsc -b`、`npm run build`（确认 `outline-graph` chunk）、`git diff --check`。本计划不跑 `cargo`，因为不改 Rust。

## Working tree（开工时先核对）

以下可能已经落在未提交工作树里。Executor **先跑 Task 1 测试**；绿则勾掉 Task 1，不要重写布局。

| 项 | 预期 |
| --- | --- |
| `package.json` | 已有 `@xyflow/react` ^12.11.3、`@dagrejs/dagre` ^3.1.1 |
| `src/outline/outlineLayout.ts` | 已是 dagre `rankdir: "TB"` + 虚根 + `readingNumber` |
| `src/outline/outlineLayout.test.ts` | 已有 TB / 不重叠 / 多入口 |
| `src/outline/OutlineCanvas.tsx` | **仍是** `position: absolute` + `<line>`，必须换 |
| `src/outline/OutlinePane.tsx` | **仍是** 静态 `import OutlineCanvas`，必须 lazy |
| `vite.config.ts` | **还没有** `outline-graph` chunk |

## File map

| 文件 | 职责 |
| --- | --- |
| `src/outline/outlineLayout.ts` | dagre TB、阅读序、虚根、`boxesOverlap`、`toFlowElements` |
| `src/outline/outlineLayout.test.ts` | 布局与 flow 元素转换 |
| `src/outline/OutlineCanvas.tsx` | React Flow 画布、自定义节点、inspector、ErrorBoundary |
| `src/outline/OutlineCanvas.test.tsx` | 不拖、序号、选中、跳证据、交叉引用开关、无 MiniMap |
| `src/outline/OutlinePane.tsx` | `React.lazy` + `Suspense` 载入画布 |
| `src/outline/OutlinePane.test.tsx` | 已发布图在 lazy 后仍能点到节点 |
| `src/styles.css` | 玻璃风节点 / Controls / inspector，覆盖 RF 默认白底 |
| `src/test/setup.ts` | `ResizeObserver` polyfill（jsdom 里 RF 需要） |
| `vite.config.ts` | `@xyflow/react` + `@dagrejs/dagre` → `outline-graph` |
| `docs/handoff.md` | 一行：画布已是 lazy React Flow，不再是自制 TB |

不新建 `outlineFlow.ts`、不搬 addon `ArgumentGraphCanvas.tsx`、不搬 Worker。

---

### Task 1: dagre TB 布局 + 阅读序号

**Files:**

- Create or keep: `src/outline/outlineLayout.ts`
- Create or keep: `src/outline/outlineLayout.test.ts`
- Modify if missing: `package.json`（`@dagrejs/dagre`）。**不要**再装 `@types/dagrejs__dagre`（3.1 自带类型，registry 上 404）。

**Interfaces:**

```ts
export const OUTLINE_NODE_WIDTH = 244;
export const OUTLINE_NODE_HEIGHT = 126;

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

export function narrativeEdges(edges: OutlineEdge[]): OutlineEdge[];
export function readingOrderIds(graph: OutlineGraph): string[];
export function layoutOutlineGraph(graph: OutlineGraph): OutlineLayout;
export function boxesOverlap(
  left: { x: number; y: number; width: number; height: number },
  right: { x: number; y: number; width: number; height: number },
  gap?: number,
): boolean;
export function primaryEvidenceId(node: OutlineNode): string | null;
export function narrativePathTo(graph: OutlineGraph, targetId: string): Set<string>;
```

- Consumes: `OutlineGraph` / `OutlineNode` / `OutlineEdge`（`src/types.ts`，不要改字段）
- Produces: 上面的函数。后续 Task 2 只用 `layoutOutlineGraph` 的 `x/y/width/height/readingNumber`。

- [ ] **Step 1: 写失败测试**

`src/outline/outlineLayout.test.ts` 必须包含这三组（可与工作树已有文件对照，缺哪组补哪组）：

```ts
import { describe, expect, it } from "vitest";
import {
  boxesOverlap,
  layoutOutlineGraph,
  narrativeEdges,
  readingOrderIds,
} from "./outlineLayout";
import type { OutlineGraph, OutlineNode } from "../types";

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
});
```

- [ ] **Step 2: 跑测试确认现状**

```powershell
npx vitest run --maxWorkers=1 --fileParallelism=false src/outline/outlineLayout.test.ts
```

Expected：若工作树已实现 → 全绿，勾掉 Step 3–4，进入 Task 2。若红（`layoutOutlineGraph is not a function` 或 `y` 关系反了）→ 做 Step 3。

- [ ] **Step 3: 最小实现**

对齐 `read_addon/src/sidepanel/components/argument-graph-layout.ts`，但有三处桌面差异：

1. 入口序用 `nodeId` 字典序，不要 `sourceOrder.firstPage`（桌面节点没有该字段）。
2. **不要** `validateArgumentGraph` 后 `throw`。服务端已校验；前端布局失败只该由 ErrorBoundary 兜，布局函数本身对缺边/坏端点要 skip。
3. 多入口时加虚根 `__outline_virtual_root__`，布局后丢弃该节点。

```ts
import dagre, { graphlib } from "@dagrejs/dagre";
import type { OutlineEdge, OutlineGraph, OutlineNode } from "../types";

export const OUTLINE_NODE_WIDTH = 244;
export const OUTLINE_NODE_HEIGHT = 126;
const VIRTUAL_ROOT = "__outline_virtual_root__";
const MARGIN = 56;

export function narrativeEdges(edges: OutlineEdge[]) {
  return edges.filter((edge) => edge.tier === "narrative");
}

export function readingOrderIds(graph: OutlineGraph): string[] {
  const narrative = narrativeEdges(graph.edges);
  const incoming = new Map(graph.nodes.map((node) => [node.nodeId, 0]));
  const outgoing = new Map(graph.nodes.map((node) => [node.nodeId, [] as string[]]));
  for (const edge of narrative) {
    if (!incoming.has(edge.sourceNodeId) || !incoming.has(edge.targetNodeId)) continue;
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
  const ordered = [...graph.nodes].sort((left, right) =>
    left.nodeId.localeCompare(right.nodeId),
  );
  const dagreGraph = new graphlib.Graph({ multigraph: true });
  dagreGraph.setGraph({
    rankdir: "TB",
    ranker: "network-simplex",
    nodesep: 86,
    ranksep: 104,
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

  const incoming = new Map(ordered.map((item) => [item.nodeId, 0]));
  for (const edge of narrative) {
    if (incoming.has(edge.targetNodeId)) {
      incoming.set(edge.targetNodeId, (incoming.get(edge.targetNodeId) ?? 0) + 1);
    }
  }
  const entries = ordered
    .filter((item) => (incoming.get(item.nodeId) ?? 0) === 0)
    .map((item) => item.nodeId);
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
  const laid: LaidOutNode[] = ordered.map((item) => {
    const position = dagreGraph.node(item.nodeId) as {
      x: number;
      y: number;
      width: number;
      height: number;
    };
    const width = position.width || OUTLINE_NODE_WIDTH;
    const height = position.height || OUTLINE_NODE_HEIGHT;
    return {
      ...item,
      x: position.x - width / 2,
      y: position.y - height / 2,
      width,
      height,
      rank: Math.round((position.y - height / 2 - MARGIN) / Math.max(1, height + 104)),
      readingNumber: numbers.get(item.nodeId) ?? 0,
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
```

`boxesOverlap` / `primaryEvidenceId` / `narrativePathTo` 保持现有实现。`narrativePathTo` 本计划 UI 不用，但不要删（后续高亮主链会用）。

- [ ] **Step 4: 再跑 Task 1 测试**

```powershell
npx vitest run --maxWorkers=1 --fileParallelism=false src/outline/outlineLayout.test.ts
```

Expected: PASS。

- [ ] **Step 5: Commit**

```powershell
git add package.json package-lock.json src/outline/outlineLayout.ts src/outline/outlineLayout.test.ts
git commit -m "feat(outline): lay out argument maps with dagre TB ranks"
```

**Done when:** `a` 在 `b` 上方、阅读序号 1/2、菱形分支盒子不相交、多入口不抛。

---

### Task 2: React Flow 画布 + 自定义节点 + inspector 交叉引用开关

**Files:**

- Modify: `src/outline/outlineLayout.ts`（追加 `toFlowElements`）
- Modify: `src/outline/outlineLayout.test.ts`
- Modify: `src/outline/OutlineCanvas.tsx`
- Modify: `src/outline/OutlineCanvas.test.tsx`
- Modify: `src/styles.css`（`.outline-canvas*` / `.outline-node*` / 新增 `.outline-flow*`）
- Modify: `src/test/setup.ts`（`ResizeObserver`）

**Interfaces:**

```ts
// outlineLayout.ts
import type { Edge, Node } from "@xyflow/react";

export type OutlineFlowNodeData = {
  node: OutlineNode;
  readingNumber: number;
  selected: boolean;
};

export type OutlineFlowNode = Node<OutlineFlowNodeData, "outlineNode">;
export type OutlineFlowEdge = Edge<{ tier: string }, "smoothstep">;

export function toFlowElements(
  graph: OutlineGraph,
  selectedNodeId: string | null,
  showCrossLinks: boolean,
): { nodes: OutlineFlowNode[]; edges: OutlineFlowEdge[] };
```

```ts
// OutlineCanvas.tsx — props 签名不得缩小
type OutlineCanvasProps = {
  graph: OutlineGraph;
  catalog: OutlineCatalogEntry[];
  selectedNodeId: string | null;
  showCrossLinks?: boolean;
  onSelectNode: (node: OutlineNode) => void;
  onJumpEvidence: (blockId: string) => void;
  onGenerateDeepDive?: (node: OutlineNode) => void;
  deepDiveBusy?: boolean;
  hasDeepDive?: boolean;
};
```

- Consumes: Task 1 `layoutOutlineGraph` / `OUTLINE_NODE_WIDTH` / `OUTLINE_NODE_HEIGHT`
- Produces: default-export 的 `OutlineCanvas`（Task 3 lazy 依赖 default export）。**删除** `export { primaryEvidenceId }`，调用方从 `outlineLayout` 取。

对齐 addon 的交互，**不要**整文件复制 `ArgumentGraphCanvas.tsx`：

| 要 | 不要 |
| --- | --- |
| `ReactFlow` + `Background` + `Controls` | `MiniMap` |
| `nodesDraggable={false}`，`nodesConnectable={false}` | 自由拖节点、连线编辑 |
| 顶栏 Fit / Reset（`fitView` / `setCenter` 到最上一档） | viewport session cache |
| 自定义卡片：序号 + role + title + takeaway | addon 英文 "deep dive" / evidence 计数条 |
| React Flow 自带 `smoothstep` 边 | addon `ViewportPortal` 边覆盖层 |
| inspector 仍在画布右侧（桌面已有） | addon 底部/右侧 overlay inspector |
| inspector 勾选「显示交叉引用」 | 按 `relationClass` 多选过滤器（V1 只要总开关） |
| `proOptions={{ hideAttribution: true }}` | addon i18n / Worker |

- [ ] **Step 1: 测试 setup 加上 ResizeObserver**

`src/test/setup.ts` 在现有 `PointerEvent` / canvas mock 之后追加：

```ts
if (typeof globalThis.ResizeObserver === "undefined") {
  class TestResizeObserver implements ResizeObserver {
    observe(): void {}
    unobserve(): void {}
    disconnect(): void {}
  }
  globalThis.ResizeObserver = TestResizeObserver;
}
```

不要删现有 canvas `getContext` mock（PDF 测试还要用）。

- [ ] **Step 2: 写 `toFlowElements` 失败测试**

追加到 `src/outline/outlineLayout.test.ts`：

```ts
import { toFlowElements } from "./outlineLayout";

describe("toFlowElements", () => {
  it("maps dagre boxes onto non-draggable flow nodes and hides cross_link by default", () => {
    const laid = layoutOutlineGraph(graph);
    const hidden = toFlowElements(graph, "a", false);
    const shown = toFlowElements(graph, "a", true);
    expect(hidden.nodes).toHaveLength(2);
    expect(hidden.nodes.every((item) => item.draggable === false)).toBe(true);
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
});
```

- [ ] **Step 3: 跑转换测试确认失败**

```powershell
npx vitest run --maxWorkers=1 --fileParallelism=false src/outline/outlineLayout.test.ts
```

Expected: FAIL，`toFlowElements is not a function`。

- [ ] **Step 4: 实现 `toFlowElements`**

追加到 `outlineLayout.ts`：

```ts
import type { Edge, Node } from "@xyflow/react";

export type OutlineFlowNodeData = {
  node: OutlineNode;
  readingNumber: number;
  selected: boolean;
};

export type OutlineFlowNode = Node<OutlineFlowNodeData, "outlineNode">;
export type OutlineFlowEdge = Edge<{ tier: string }, "smoothstep">;

export function toFlowElements(
  graph: OutlineGraph,
  selectedNodeId: string | null,
  showCrossLinks: boolean,
): { nodes: OutlineFlowNode[]; edges: OutlineFlowEdge[] } {
  const laid = layoutOutlineGraph(graph);
  const nodes: OutlineFlowNode[] = laid.nodes.map((item) => ({
    id: item.nodeId,
    type: "outlineNode",
    position: { x: item.x, y: item.y },
    width: item.width,
    height: item.height,
    draggable: false,
    selectable: true,
    data: {
      node: item,
      readingNumber: item.readingNumber,
      selected: item.nodeId === selectedNodeId,
    },
    style: { width: item.width, height: item.height },
  }));
  const edges: OutlineFlowEdge[] = graph.edges
    .filter((edge) => edge.tier === "narrative" || showCrossLinks)
    .filter((edge) =>
      laid.nodes.some((item) => item.nodeId === edge.sourceNodeId) &&
      laid.nodes.some((item) => item.nodeId === edge.targetNodeId),
    )
    .map((edge) => ({
      id: edge.edgeId,
      type: "smoothstep",
      source: edge.sourceNodeId,
      target: edge.targetNodeId,
      data: { tier: edge.tier },
      selectable: false,
      style: {
        strokeWidth: edge.tier === "narrative" ? 2 : 1.5,
        strokeDasharray: edge.tier === "cross_link" ? "6 5" : undefined,
      },
    }));
  return { nodes, edges };
}
```

- [ ] **Step 5: 再跑 layout 测试**

```powershell
npx vitest run --maxWorkers=1 --fileParallelism=false src/outline/outlineLayout.test.ts
```

Expected: PASS。

- [ ] **Step 6: 重写 `OutlineCanvas` 失败测试**

替换 `src/outline/OutlineCanvas.test.tsx` 全文：

```tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import OutlineCanvas from "./OutlineCanvas";
import type { OutlineGraph } from "../types";

const graph: OutlineGraph = {
  title: "Map",
  summary: "A",
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
    {
      nodeId: "n2",
      roleClass: "result_finding",
      title: "误差下降",
      takeaway: "验证有效",
      importance: "supporting",
      sourceUnitIds: [],
      evidenceIds: ["p-2"],
      confidence: 1,
    },
  ],
  edges: [
    {
      edgeId: "n1-n2",
      sourceNodeId: "n1",
      targetNodeId: "n2",
      tier: "narrative",
      relationClass: "dependency",
      label: "then",
      rationale: "",
      evidenceIds: [],
    },
    {
      edgeId: "n2-n1",
      sourceNodeId: "n2",
      targetNodeId: "n1",
      tier: "cross_link",
      relationClass: "comparison",
      label: "vs",
      rationale: "",
      evidenceIds: [],
    },
  ],
};

const catalog = [
  {
    id: "fig-1",
    page: 3,
    type: "figure",
    blockIndex: 2,
    bbox: [10, 10, 100, 100] as [number, number, number, number],
  },
];

describe("OutlineCanvas", () => {
  it("renders a non-draggable React Flow map with reading numbers and no MiniMap", async () => {
    render(
      <OutlineCanvas
        graph={graph}
        catalog={catalog}
        selectedNodeId={null}
        onSelectNode={vi.fn()}
        onJumpEvidence={vi.fn()}
      />,
    );
    expect(await screen.findByTestId("rf__wrapper")).toHaveAttribute(
      "role",
      "application",
    );
    expect(screen.getByTestId("rf__node-n1")).not.toHaveClass("draggable");
    expect(screen.getByLabelText("阅读序号 1")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "放大" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "缩小" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "查看整张地图" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "回到可读起点" })).toBeInTheDocument();
    expect(screen.queryByRole("img", { name: "Mini Map" })).not.toBeInTheDocument();
    expect(screen.queryByTestId("rf__edge-n2-n1")).not.toBeInTheDocument();
  });

  it("selects a node, jumps to evidence, and can reveal cross_link edges", async () => {
    const user = userEvent.setup();
    const onSelectNode = vi.fn();
    const onJumpEvidence = vi.fn();
    render(
      <OutlineCanvas
        graph={graph}
        catalog={catalog}
        selectedNodeId="n1"
        onSelectNode={onSelectNode}
        onJumpEvidence={onJumpEvidence}
      />,
    );
    expect(await screen.findByRole("button", { name: "生成局部图" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: /p\.3/ }));
    expect(onJumpEvidence).toHaveBeenCalledWith("fig-1");
    await user.click(screen.getByRole("checkbox", { name: "显示交叉引用" }));
    expect(screen.getByTestId("rf__edge-n2-n1")).toBeInTheDocument();
  });
});
```

注意：RF Controls 默认英文 `Zoom In` / `Zoom Out`。实现里必须把这两个按钮的 `aria-label` 设成「放大」「缩小」（用 CSS 选择器改原生 Controls，或包一层并设置 `aria-label`）。测试按中文查。若 RF 12 不让改原生 label，则测试改查 `Zoom In` / `Zoom Out`，**但 Fit / Reset 必须是中文**（我们自己的 toolbar）。

- [ ] **Step 7: 跑画布测试确认失败**

```powershell
npx vitest run --maxWorkers=1 --fileParallelism=false src/outline/OutlineCanvas.test.tsx
```

Expected: FAIL，找不到 `rf__wrapper`（当前仍是自制 `role="list"`）。

- [ ] **Step 8: 重写 `OutlineCanvas.tsx`**

完整替换该文件。要点：

1. 保留现有 `OutlineCanvasBoundary`。
2. `nodeTypes` **定义在模块顶层**，不要放进组件里（RF 会反复 remount）。
3. `showCrossLinks` 初始取 props，inspector checkbox 可本地覆盖。
4. `onNodeClick` / 卡片按钮都调 `onSelectNode`；不要在画布里 `jumpOutlineEvidence`。
5. 不要监听 `Escape`。
6. `onInit` 把视口放到最上一档中心、`zoom: 0.86`，不要 `fitView` 整图缩到看不清字。
7. `import "@xyflow/react/dist/style.css"` 只出现在本文件，方便 Task 3 打进同一 chunk。

```tsx
import { Component, useMemo, useState, type ErrorInfo, type ReactNode } from "react";
import {
  Background,
  Controls,
  Handle,
  Panel,
  Position,
  ReactFlow,
  useReactFlow,
  type NodeProps,
  type ReactFlowInstance,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { Maximize2, RotateCcw } from "lucide-react";
import type { OutlineCatalogEntry, OutlineGraph, OutlineNode } from "../types";
import {
  OUTLINE_NODE_HEIGHT,
  layoutOutlineGraph,
  toFlowElements,
  type OutlineFlowNode,
} from "./outlineLayout";

type OutlineCanvasProps = {
  graph: OutlineGraph;
  catalog: OutlineCatalogEntry[];
  selectedNodeId: string | null;
  showCrossLinks?: boolean;
  onSelectNode: (node: OutlineNode) => void;
  onJumpEvidence: (blockId: string) => void;
  onGenerateDeepDive?: (node: OutlineNode) => void;
  deepDiveBusy?: boolean;
  hasDeepDive?: boolean;
};

type BoundaryState = { failed: boolean };

class OutlineCanvasBoundary extends Component<
  { fallback: ReactNode; children: ReactNode },
  BoundaryState
> {
  state: BoundaryState = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  componentDidCatch(_error: Error, _info: ErrorInfo) {
    this.setState({ failed: true });
  }
  render() {
    if (this.state.failed) return this.props.fallback;
    return this.props.children;
  }
}

function OutlineNodeView({ data }: NodeProps<OutlineFlowNode>) {
  const { node, readingNumber, selected } = data;
  return (
    <div className="outline-node-shell">
      <Handle type="target" position={Position.Top} className="outline-handle" />
      <div
        className={`outline-node-card ${selected ? "is-selected" : ""}`}
        data-active={selected ? "true" : "false"}
      >
        <span className="outline-reading-number" aria-label={`阅读序号 ${readingNumber}`}>
          {readingNumber}
        </span>
        <small>{node.roleLabel ?? node.roleClass}</small>
        <strong>{node.title}</strong>
        <span>{node.takeaway}</span>
      </div>
      <Handle type="source" position={Position.Bottom} className="outline-handle" />
    </div>
  );
}

const nodeTypes = { outlineNode: OutlineNodeView };

function OutlineCanvasToolbar() {
  const { fitView, setCenter, getNodes } = useReactFlow();
  return (
    <div className="outline-flow-toolbar">
      <button
        type="button"
        aria-label="查看整张地图"
        onClick={() => void fitView({ padding: 0.14, duration: 220 })}
      >
        <Maximize2 size={16} />
      </button>
      <button
        type="button"
        aria-label="回到可读起点"
        onClick={() => {
          const nodes = getNodes();
          if (nodes.length === 0) return;
          const top = Math.min(...nodes.map((item) => item.position.y));
          const roots = nodes.filter((item) => Math.abs(item.position.y - top) < 1);
          const centerX =
            roots.reduce(
              (sum, item) => sum + item.position.x + (item.width ?? 244) / 2,
              0,
            ) / roots.length;
          void setCenter(centerX, top + 100, { zoom: 0.86, duration: 180 });
        }}
      >
        <RotateCcw size={16} />
      </button>
    </div>
  );
}

function OutlineCanvasInner({
  graph,
  catalog,
  selectedNodeId,
  showCrossLinks = false,
  onSelectNode,
  onJumpEvidence,
  onGenerateDeepDive,
  deepDiveBusy = false,
  hasDeepDive = false,
}: OutlineCanvasProps) {
  const [crossLinksOn, setCrossLinksOn] = useState(showCrossLinks);
  const elements = useMemo(
    () => toFlowElements(graph, selectedNodeId, crossLinksOn),
    [graph, selectedNodeId, crossLinksOn],
  );
  const selected =
    graph.nodes.find((node) => node.nodeId === selectedNodeId) ?? null;

  const handleInit = (instance: ReactFlowInstance) => {
    const laid = layoutOutlineGraph(graph).nodes;
    if (laid.length === 0) return;
    const top = Math.min(...laid.map((node) => node.y));
    const roots = laid.filter((node) => Math.abs(node.y - top) < 1);
    const centerX =
      roots.reduce((sum, node) => sum + node.x + node.width / 2, 0) /
      Math.max(1, roots.length);
    void instance.setCenter(centerX, top + 100, { zoom: 0.86, duration: 0 });
  };

  return (
    <div className="outline-canvas-shell">
      <div className="outline-flow" role="region" aria-label="论证地图">
        <ReactFlow
          nodes={elements.nodes}
          edges={elements.edges}
          nodeTypes={nodeTypes}
          nodesDraggable={false}
          nodesConnectable={false}
          elementsSelectable
          panOnDrag
          panOnScroll
          zoomOnScroll
          zoomOnPinch
          minZoom={0.35}
          maxZoom={2}
          defaultViewport={{ x: 0, y: 32, zoom: 0.86 }}
          onInit={handleInit}
          onNodeClick={(_, flowNode) => {
            const next = graph.nodes.find((node) => node.nodeId === flowNode.id);
            if (next) onSelectNode(next);
          }}
          proOptions={{ hideAttribution: true }}
          className="outline-flow-surface"
        >
          <Background gap={22} size={1} />
          <Controls showInteractive={false} showFitView={false} />
          <Panel position="top-right" className="outline-flow-panel">
            <OutlineCanvasToolbar />
          </Panel>
        </ReactFlow>
      </div>
      {selected ? (
        <aside className="outline-inspector">
          <span>{selected.roleLabel ?? selected.roleClass}</span>
          <h3>{selected.title}</h3>
          <p>{selected.takeaway}</p>
          <label className="outline-crosslink-toggle">
            <input
              type="checkbox"
              checked={crossLinksOn}
              onChange={(event) => setCrossLinksOn(event.target.checked)}
            />
            显示交叉引用
          </label>
          <div className="outline-evidence-pills">
            {selected.evidenceIds.map((id) => {
              const entry = catalog.find((item) => item.id === id);
              return (
                <button key={id} type="button" onClick={() => onJumpEvidence(id)}>
                  {entry ? `p.${entry.page} · ${entry.type}` : id}
                </button>
              );
            })}
          </div>
          <button
            type="button"
            className="btn-liquid-pill"
            disabled={!onGenerateDeepDive || deepDiveBusy}
            onClick={() => onGenerateDeepDive?.(selected)}
          >
            {hasDeepDive ? "打开局部图" : deepDiveBusy ? "生成中…" : "生成局部图"}
          </button>
        </aside>
      ) : null}
    </div>
  );
}

export default function OutlineCanvas(props: OutlineCanvasProps) {
  const fallback = (
    <ol className="outline-fallback-list">
      {props.graph.nodes.map((node) => (
        <li key={node.nodeId}>
          <button type="button" onClick={() => props.onSelectNode(node)}>
            {node.title}
          </button>
          <p>{node.takeaway}</p>
        </li>
      ))}
    </ol>
  );
  return (
    <OutlineCanvasBoundary fallback={fallback}>
      <OutlineCanvasInner {...props} />
    </OutlineCanvasBoundary>
  );
}
```

`OUTLINE_NODE_HEIGHT` 若 lint unused，删掉 import。

- [ ] **Step 9: 换掉自制画布 CSS**

在 `src/styles.css` 把 `.outline-canvas-shell` 到 `.outline-fallback-list button` 整段换成：

```css
.outline-pane {
  min-height: 0;
  display: flex;
  flex-direction: column;
}
.outline-canvas-shell {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 220px;
  min-height: 0;
  flex: 1;
}
.outline-flow {
  min-width: 0;
  min-height: 0;
  height: 100%;
}
.outline-flow-surface {
  background: transparent;
}
.outline-flow .react-flow__node-outlineNode {
  background: transparent;
  border: 0;
  padding: 0;
  box-shadow: none;
}
.outline-flow .react-flow__attribution {
  display: none;
}
.outline-flow .react-flow__controls {
  border: 1px solid var(--glass-border);
  border-radius: 12px;
  overflow: hidden;
  box-shadow: var(--liquid-glass-shadow);
  background: var(--glass-surface);
}
.outline-flow .react-flow__controls-button {
  background: var(--glass-card);
  border-bottom: 1px solid var(--glass-border-subtle);
  fill: var(--ink);
}
.outline-handle {
  width: 4px !important;
  height: 4px !important;
  border: 0 !important;
  background: transparent !important;
}
.outline-node-shell {
  width: 100%;
  height: 100%;
}
.outline-node-card {
  display: flex;
  flex-direction: column;
  gap: 4px;
  width: 100%;
  height: 100%;
  padding: 10px 12px 10px 36px;
  border-radius: 14px;
  border: 1px solid var(--glass-border);
  background: var(--glass-card);
  box-shadow: var(--liquid-glass-shadow);
  color: var(--ink);
  text-align: left;
  position: relative;
}
.outline-node-card.is-selected {
  border-color: var(--ink);
}
.outline-node-card small {
  color: var(--muted);
  font-size: 10px;
  text-transform: uppercase;
}
.outline-node-card strong {
  font-size: 13px;
  line-height: 1.3;
}
.outline-node-card span {
  color: var(--muted);
  font-size: 11px;
  line-height: 1.35;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.outline-reading-number {
  position: absolute;
  left: 8px;
  top: 10px;
  width: 20px;
  height: 20px;
  border-radius: 999px;
  display: grid;
  place-items: center;
  font-size: 11px;
  font-weight: 700;
  color: var(--ink);
  background: var(--glass-surface);
  border: 1px solid var(--glass-border);
}
.outline-flow-toolbar {
  display: flex;
  gap: 4px;
  padding: 4px;
  border-radius: 12px;
  border: 1px solid var(--glass-border);
  background: var(--glass-surface);
  box-shadow: var(--liquid-glass-shadow);
}
.outline-flow-toolbar button {
  width: 32px;
  height: 32px;
  border: 0;
  border-radius: 8px;
  background: transparent;
  color: var(--muted);
}
.outline-inspector {
  padding: 14px;
  border-left: 1px solid var(--glass-border);
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-height: 0;
  overflow: auto;
}
.outline-crosslink-toggle {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--ink);
}
.outline-evidence-pills {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.outline-evidence-pills button,
.outline-fallback-list button {
  border: 0;
  border-radius: 999px;
  padding: 4px 8px;
  background: var(--glass-card);
  color: var(--ink);
}
```

删掉旧的 `.outline-canvas` / `.outline-node` / `.outline-edges` 规则，避免绝对定位和 RF 抢同一套 class。

若 RF 12 Controls 的默认 `aria-label` 是 `Zoom In` / `Zoom Out`，在 Step 6 测试里改这两行去匹配实际 DOM，**不要**为了测试去 fork Controls 源码。Fit / Reset 仍必须中文。

- [ ] **Step 10: 跑画布 + 布局测试**

```powershell
npx vitest run --maxWorkers=1 --fileParallelism=false src/outline/OutlineCanvas.test.tsx src/outline/outlineLayout.test.ts
```

Expected: PASS。若 `rf__edge-n2-n1` 找不到，先 `screen.debug()` 看 RF 实际 test id（有的版本是 `react-flow__edge-n2-n1`），测试与实现用同一个 id，不要猜。

- [ ] **Step 11: Commit**

```powershell
git add src/outline/outlineLayout.ts src/outline/outlineLayout.test.ts src/outline/OutlineCanvas.tsx src/outline/OutlineCanvas.test.tsx src/styles.css src/test/setup.ts
git commit -m "feat(outline): render argument maps with React Flow"
```

**Done when:** 测试里能看到 `rf__wrapper`、节点无 `draggable`、有阅读序号、无 MiniMap、默认看不到交叉边、勾选后出现、证据药丸仍跳 `fig-1`、Deep dive 按钮无 handler 时 disabled。

---

### Task 3: lazy import + `outline-graph` chunk

**Files:**

- Modify: `src/outline/OutlinePane.tsx`
- Modify: `src/outline/OutlinePane.test.tsx`
- Modify: `vite.config.ts`

**Interfaces:**

```ts
const OutlineCanvas = lazy(() => import("./OutlineCanvas"));
```

- Consumes: Task 2 default export
- Produces: 未打开发布图时，主 bundle 不含 `@xyflow/react` / `@dagrejs/dagre`

Vite 已有 `pdfjs` 与 `artifact-markdown` chunk。`App.tsx` 已 `lazy` `PdfReader`。`OutlinePane` 仍被 `App.tsx` 静态引入——保持这样，只把重的画布拆出去。计划卡 / 生成中 / 缺 OCR **不得**触发 `import("./OutlineCanvas")`。

- [ ] **Step 1: 给 OutlinePane 补一条已发布图测试**

追加到 `src/outline/OutlinePane.test.tsx`：

```ts
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
  expect(await screen.findByText("RNN 模拟")).toBeInTheDocument();
  expect(screen.getByLabelText("论证地图")).toBeInTheDocument();
});
```

现有「计划卡 / 生成中 / 先 OCR」三测必须继续过，且这些用例 **不准** 出现 `rf__wrapper`（证明没预载画布）。在「渲染计划卡」那条末尾加：

```ts
expect(screen.queryByTestId("rf__wrapper")).not.toBeInTheDocument();
```

- [ ] **Step 2: 跑 Pane 测试确认已发布用例失败或卡住**

```powershell
npx vitest run --maxWorkers=1 --fileParallelism=false src/outline/OutlinePane.test.tsx
```

Expected：计划卡仍绿；新用例 FAIL（现在静态画布没有 `aria-label="论证地图"`，或 lazy 后需要 `findBy`）。

- [ ] **Step 3: OutlinePane 改为 lazy**

`OutlinePane.tsx` 顶部：

```tsx
import { lazy, Suspense } from "react";

const OutlineCanvas = lazy(() => import("./OutlineCanvas"));
```

删掉 `import OutlineCanvas from "./OutlineCanvas"`。

已发布分支包一层：

```tsx
<Suspense fallback={<p className="outline-progress-hint">正在载入地图画布…</p>}>
  <OutlineCanvas
    graph={deepDiveGraph ?? projection.head.graph}
    catalog={projection.catalog?.entries ?? []}
    selectedNodeId={selectedNodeId}
    onSelectNode={onSelectNode ?? (() => undefined)}
    onJumpEvidence={onJumpEvidence ?? (() => undefined)}
    onGenerateDeepDive={deepDiveGraph ? undefined : onGenerateDeepDive}
    deepDiveBusy={deepDiveBusy}
    hasDeepDive={hasDeepDive}
  />
</Suspense>
```

不要把 `lazy()` 写进 render。不要 lazy `OutlinePane` 本身。

- [ ] **Step 4: Vite 拆 chunk**

`vite.config.ts` 的 `manualChunks` 在 `react` 判断之前插入：

```ts
if (
  id.includes("node_modules/@xyflow/react") ||
  id.includes("node_modules/@xyflow/system") ||
  id.includes("node_modules/@dagrejs/dagre") ||
  id.includes("node_modules/@dagrejs/graphlib") ||
  id.includes("/src/outline/OutlineCanvas")
) {
  return "outline-graph";
}
```

必须包含 `OutlineCanvas` 自己，这样 `@xyflow/react/dist/style.css` 跟画布进同一异步块。不要把整个 `src/outline/` 打进去（`outlinePlan.ts` / `outlineProgress.ts` 是首屏计划卡需要的）。

- [ ] **Step 5: 跑 Pane + 画布测试**

```powershell
npx vitest run --maxWorkers=1 --fileParallelism=false src/outline/OutlinePane.test.tsx src/outline/OutlineCanvas.test.tsx
```

Expected: PASS。`findByText("RNN 模拟")` 能等到 lazy resolve。

- [ ] **Step 6: Commit**

```powershell
git add src/outline/OutlinePane.tsx src/outline/OutlinePane.test.tsx vite.config.ts
git commit -m "perf(outline): lazy-load the React Flow argument canvas"
```

**Done when:** 计划卡路径没有 `rf__wrapper`；已发布路径能看到节点标题；chunk 规则已写入。

---

### Task 4: 串行验收 + handoff

**Files:**

- Modify: `docs/handoff.md`（只改「必做下一步」里那句自制画布说明）
- 不改 `docs/full-outline-v1.md`（§9.3 已经写了 `@xyflow/react` + `dagre`）

- [ ] **Step 1: 串行前端测试**

```powershell
npx vitest run --maxWorkers=1 --fileParallelism=false
```

Expected: 全绿。默认并行禁止（本仓库会 OOM）。

- [ ] **Step 2: 类型检查**

```powershell
npx tsc -b
```

Expected: 退出码 0。

- [ ] **Step 3: 生产构建并确认 chunk**

```powershell
npx vite build
```

在 `dist/assets` 里必须出现文件名含 `outline-graph` 的 js（以及可能的 css）。主 `index-*.js` 里用编辑器搜 `@xyflow/react` 应搜不到；`outline-graph-*.js` 里搜得到。

若 chunk 名没出来：检查 `manualChunks` 的 `id` 在 Windows 上是 `\` 还是 `/`。规则改成同时匹配：

```ts
const normalized = id.replaceAll("\\", "/");
```

然后对 `normalized` 做 `includes`。

- [ ] **Step 4: whitespace**

```powershell
git diff --check
```

Expected: 无输出。

- [ ] **Step 5: 改 handoff 一句**

`docs/handoff.md`「必做下一步」第 7 条里「画布是确定性 TB 布局，不是 React Flow」改成：

```text
Full Outline 壳与 Job 已落地。已发布地图用 lazy `@xyflow/react` + dagre TB；计划卡 / 生成中不加载该 chunk。重启 `tauri dev` 后用已 OCR 论文点「地图」。live Gemini 生成需显式付费开关。
```

不要在 handoff 里写 MiniMap、Worker 或「下一步再换画布」。

- [ ] **Step 6: 手工核对清单（无浏览器工具就写进收工说明，不要假装点过）**

1. 计划卡仍在、点「生成地图」仍走现有 `start_outline`。
2. 已发布图：卡片不重叠、可滚轮缩放、不能拖节点。
3. 单击节点：inspector 打开，PDF 跳到第一条证据（`App.tsx` 已有 `jumpOutlineEvidence`）。
4. 勾选「显示交叉引用」才出现虚线边。
5. Deep dive 按钮仍是「生成局部图」/「打开局部图」；返回总图面包屑仍在。
6. 返回讨论时后台 banner 仍在。
7. 打开讨论预设时 Network/构建分析里看不到 `outline-graph` 先被拉下来。

- [ ] **Step 7: Commit**

```powershell
git add docs/handoff.md
git commit -m "docs: record lazy React Flow outline canvas"
```

**Done when:** 串行 Vitest、`tsc`、`vite build` 的 `outline-graph` chunk、`git diff --check` 全过；Rust 零 diff。

---

## Spec coverage

| 规格 | 任务 |
| --- | --- |
| §9.3 `@xyflow/react` + `dagre`，`rankdir: TB`，只 narrative 分层 | Task 1–2 |
| §9.3 节点不可自由拖拽 | Task 2 `nodesDraggable={false}` + `draggable: false` |
| §9.3 `cross_link` 默认隐藏，inspector 过滤器 | Task 2 checkbox |
| §9.3 lazy chunk，不进首屏 | Task 3 |
| §9.3 ErrorBoundary → 可读列表 | Task 2 保留 Boundary |
| §9.3 Deep dive 换图 + 面包屑 | 已有，本计划不改 `OutlinePane` 面包屑 / `App.tsx` view |
| §9.4 单击选中 + inspector + 跳主证据 | Task 2 调 `onSelectNode`；跳转仍在 `App.tsx` |
| §9.4 证据药丸跳该 Block | Task 2 inspector 已有 |
| §9.4 生成 / 打开局部图 | Task 2 保留按钮；不改 Job |
| §9.4 Esc 分层 | 不改 `consumeEscapeForOutline` |
| 阅读序号（产品拍板，规格未写字段） | Task 1 `readingNumber` + Task 2 徽章 |
| 无 MiniMap（产品拍板 V1） | Task 2 测试禁止 Mini Map |

## 非目标（伸手就停）

- MiniMap、layout Worker、按 relationClass 多选、可点击边标签、viewport 缓存
- 改 `src-tauri/**`、Job、catalog、Deep dive 白名单、预设语义
- 把 `OutlinePane` 或 `App.tsx` 整页 lazy
- 把 addon `ArgumentGraphCanvas.tsx` 拷进来
- 通用窗口管理器、三栏、同一表面开两份地图
- 路径高亮（`narrativePathTo` 先留着不用）
