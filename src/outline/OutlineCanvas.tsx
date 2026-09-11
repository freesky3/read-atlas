import {
  Component,
  useCallback,
  useEffect,
  useState,
  type CSSProperties,
  type ErrorInfo,
  type ReactNode,
} from "react";
import {
  Background,
  Controls,
  Handle,
  Panel,
  Position,
  ReactFlow,
  useEdgesState,
  useNodesState,
  useReactFlow,
  type NodeProps,
  type ReactFlowInstance,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { Maximize2, RotateCcw } from "lucide-react";
import { useLocale } from "../i18n/LocaleContext";
import OutlineRelationEdge from "./OutlineRelationEdge";
import OutlineGraphList from "./OutlineGraphList";
import type {
  OutlineCatalogEntry,
  OutlineEdge,
  OutlineGraph,
  OutlineNode,
} from "../types";
import {
  clampOutlineInspectorWidth,
  OUTLINE_INSPECTOR_DEFAULT,
  OUTLINE_INSPECTOR_MAX,
  OUTLINE_INSPECTOR_MIN,
} from "./outlineInspectorWidth";
import {
  layoutOutlineGraph,
  OUTLINE_NODE_WIDTH,
  primaryEvidenceId,
  toFlowElements,
  type OutlineFlowEdge,
  type OutlineFlowNode,
} from "./outlineLayout";
import {
  displayRole,
  edgeReferences,
  isOutlineMapV4,
  locatorLabel,
  nodeReferences,
} from "./outlineDisplay";

type OutlineCanvasProps = {
  graph: OutlineGraph;
  catalog: OutlineCatalogEntry[];
  selectedNodeId: string | null;
  canvasKey?: string;
  showCrossLinks?: boolean;
  inspectorWidth?: number;
  onInspectorWidthChange?: (width: number) => void;
  onClearSelection?: () => void;
  onSelectNode: (node: OutlineNode) => void;
  onJumpEvidence: (
    target: string | { blockId?: string | null; pageNumber?: number | null },
  ) => void;
  onGenerateDeepDive?: (node: OutlineNode) => void;
  deepDiveBusy?: boolean;
  hasDeepDive?: boolean;
};

function outlineCanvasInstanceKey(graph: OutlineGraph, canvasKey?: string) {
  return canvasKey ?? graph.nodes.map((node) => node.nodeId).join("\0");
}

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
  const { t } = useLocale();
  const { node, readingNumber, selected } = data;
  const role = displayRole(node, t);
  return (
    <div className="outline-node-shell">
      <Handle type="target" position={Position.Top} className="outline-handle" />
      <div
        className={`outline-node-card ${selected ? "is-selected" : ""}`}
        data-active={selected ? "true" : "false"}
      >
        <span className="outline-reading-number" aria-label={t("outline.canvas.nodeNumber", { n: readingNumber })}>
          {readingNumber}
        </span>
        {role ? <small>{role}</small> : null}
        <strong>{node.title}</strong>
      </div>
      <Handle type="source" position={Position.Bottom} className="outline-handle" />
    </div>
  );
}

const nodeTypes = { outlineNode: OutlineNodeView };
const edgeTypes = { outlineRelation: OutlineRelationEdge };

function OutlineCanvasToolbar({ onResetLayout }: { onResetLayout: () => void }) {
  const { t } = useLocale();
  const { fitView, setCenter, getNodes } = useReactFlow();
  return (
    <div className="outline-flow-toolbar">
      <button
        type="button"
        aria-label={t("outline.canvas.fitView")}
        onClick={() => void fitView({ padding: 0.14, duration: 220 })}
      >
        <Maximize2 size={16} />
      </button>
      <button
        type="button"
        aria-label={t("outline.canvas.resetLayout")}
        onClick={() => {
          onResetLayout();
          window.requestAnimationFrame(() => {
            const current = getNodes();
            if (current.length === 0) return;
            const top = Math.min(...current.map((item) => item.position.y));
            const roots = current.filter(
              (item) => Math.abs(item.position.y - top) < 1,
            );
            const centerX =
              roots.reduce(
                (sum, item) =>
                  sum + item.position.x + (item.width ?? OUTLINE_NODE_WIDTH) / 2,
                0,
              ) / roots.length;
            void setCenter(centerX, top + 100, { zoom: 0.86, duration: 180 });
          });
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
  canvasKey,
  showCrossLinks = false,
  inspectorWidth,
  onInspectorWidthChange,
  onClearSelection,
  onSelectNode,
  onJumpEvidence,
  onGenerateDeepDive,
  deepDiveBusy = false,
  hasDeepDive = false,
}: OutlineCanvasProps) {
  const { t } = useLocale();
  const instanceKey = outlineCanvasInstanceKey(graph, canvasKey);
  const v4 = isOutlineMapV4(graph);
  const [crossLinksOn, setCrossLinksOn] = useState(showCrossLinks);
  const [selectedEdgeId, setSelectedEdgeId] = useState<string | null>(null);
  const [nodes, setNodes, onNodesChange] = useNodesState<OutlineFlowNode>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<OutlineFlowEdge>([]);
  const selected =
    graph.nodes.find((node) => node.nodeId === selectedNodeId) ?? null;
  const selectedEdge =
    graph.edges.find((edge) => edge.edgeId === selectedEdgeId) ?? null;
  const inspectorOpen = Boolean(selected || selectedEdge);
  const width = clampOutlineInspectorWidth(
    inspectorWidth ?? OUTLINE_INSPECTOR_DEFAULT,
  );
  const jumpId = selected ? primaryEvidenceId(selected) : null;

  useEffect(() => {
    setSelectedEdgeId(null);
  }, [instanceKey]);
  useEffect(() => {
    if (selectedNodeId) setSelectedEdgeId(null);
  }, [selectedNodeId]);
  const selectEdge = useCallback((id: string) => {
    onClearSelection?.();
    setSelectedEdgeId(current => current === id ? null : id);
  }, [onClearSelection]);

  useEffect(() => {
    const next = toFlowElements(graph, selectedNodeId, crossLinksOn, t);
    setNodes(next.nodes);
    setEdges(next.edges);
  }, [instanceKey, graph, setEdges, setNodes]);

  useEffect(() => {
    setNodes((current) =>
      current.map((item) => ({
        ...item,
        data: { ...item.data, selected: item.id === selectedNodeId },
      })),
    );
  }, [selectedNodeId, setNodes]);

  useEffect(() => {
    setEdges(toFlowElements(graph, selectedNodeId, crossLinksOn, t).edges.map(edge => ({
      ...edge,
      selected: edge.id === selectedEdgeId,
      data: { ...edge.data!, active: edge.id === selectedEdgeId, onSelect: selectEdge },
    })));
  }, [crossLinksOn, graph, selectedNodeId, selectedEdgeId, selectEdge, setEdges]);

  useEffect(() => {
    const localized = toFlowElements(graph, selectedNodeId, crossLinksOn, t);
    setNodes(current => current.map(node => ({ ...node, ariaLabel: localized.nodes.find(item => item.id === node.id)?.ariaLabel })));
    setEdges(current => current.map(edge => ({ ...edge, ariaLabel: localized.edges.find(item => item.id === edge.id)?.ariaLabel })));
  }, [t, setNodes, setEdges]);

  const handleInit = (
    instance: ReactFlowInstance<OutlineFlowNode, OutlineFlowEdge>,
  ) => {
    const laid = layoutOutlineGraph(graph).nodes;
    if (laid.length === 0) return;
    const top = Math.min(...laid.map((node) => node.y));
    const roots = laid.filter((node) => Math.abs(node.y - top) < 1);
    const centerX =
      roots.reduce((sum, node) => sum + node.x + node.width / 2, 0) /
      Math.max(1, roots.length);
    void instance.setCenter(centerX, top + 100, { zoom: 0.86, duration: 0 });
  };

  const resetLayout = () => {
    const next = toFlowElements(graph, selectedNodeId, crossLinksOn, t);
    setNodes(next.nodes);
    setEdges(next.edges.map(edge => ({
      ...edge,
      selected: edge.id === selectedEdgeId,
      data: { ...edge.data!, active: edge.id === selectedEdgeId, onSelect: selectEdge },
    })));
  };

  const beginResize = (event: React.PointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    const origin = event.clientX;
    const start = width;
    const move = (next: PointerEvent) => {
      onInspectorWidthChange?.(
        clampOutlineInspectorWidth(start + (origin - next.clientX)),
      );
    };
    const stop = (next: PointerEvent) => {
      onInspectorWidthChange?.(
        clampOutlineInspectorWidth(start + (origin - next.clientX)),
      );
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop);
  };

  return (
    <div
      className="outline-canvas-shell"
      data-outline-canvas={instanceKey}
      onKeyDownCapture={(event) => {
        if (event.key === "Escape" && inspectorOpen) {
          event.preventDefault();
          event.stopPropagation();
          setSelectedEdgeId(null);
          onClearSelection?.();
        }
      }}
      data-inspector={inspectorOpen ? "open" : "closed"}
      style={
        inspectorOpen
          ? ({ "--outline-inspector-width": `${width}px` } as CSSProperties)
          : undefined
      }
    >
      <div
        className="outline-flow"
        role="region"
        aria-label={t("outline.canvas.region")}
        style={{ flex: "1 1 0%", minWidth: 0, minHeight: 0 }}
      >
        <ReactFlow
          key={instanceKey}
          nodes={nodes}
          edges={edges}
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          nodeTypes={nodeTypes}
          edgeTypes={edgeTypes}
          onKeyDownCapture={(event) => {
            if (event.key !== "Enter" && event.key !== " ") return;
            const element = (event.target as HTMLElement).closest<HTMLElement>(".react-flow__edge, .react-flow__node");
            if (!element || (event.target as HTMLElement).closest("button")) return;
            const id = element.getAttribute("data-id");
            if (!id) return;
            event.preventDefault();
            if (element.classList.contains("react-flow__edge")) selectEdge(id);
            else {
              setSelectedEdgeId(null);
              const node = graph.nodes.find(item => item.nodeId === id);
              if (node) onSelectNode(node);
            }
          }}
          nodesDraggable
          nodesConnectable={false}
          elementsSelectable
          deleteKeyCode={null}
          panOnDrag
          panOnScroll
          zoomOnScroll
          zoomOnPinch
          minZoom={0.35}
          maxZoom={2}
          defaultViewport={{ x: 0, y: 32, zoom: 0.86 }}
          onInit={handleInit}
          onPaneClick={() => {
            setSelectedEdgeId(null);
            onClearSelection?.();
          }}
          onNodeClick={(_, flowNode) => {
            setSelectedEdgeId(null);
            if (flowNode.id === selectedNodeId) {
              onClearSelection?.();
              return;
            }
            const next = graph.nodes.find((node) => node.nodeId === flowNode.id);
            if (next) onSelectNode(next);
          }}
          onEdgeClick={(_, flowEdge) => selectEdge(flowEdge.id)}
          proOptions={{ hideAttribution: true }}
          className="outline-flow-surface"
          ariaLabelConfig={{
            "controls.zoomIn.ariaLabel": t("outline.canvas.zoomIn"),
            "controls.zoomOut.ariaLabel": t("outline.canvas.zoomOut"),
          }}
        >
          <Background gap={22} size={1} />
          <Controls showInteractive={false} showFitView={false} />
          <Panel position="top-right" className="outline-flow-panel">
            <OutlineCanvasToolbar
              onResetLayout={() => {
                resetLayout();
              }}
            />
          </Panel>
        </ReactFlow>
      </div>
      {inspectorOpen ? (
        <>
          <div
            className="workspace-split outline-inspector-split"
            role="separator"
            tabIndex={0}
            aria-orientation="vertical"
            aria-label={t("outline.canvas.resizeInspector")}
            aria-valuemin={OUTLINE_INSPECTOR_MIN}
            aria-valuemax={OUTLINE_INSPECTOR_MAX}
            aria-valuenow={width}
            onPointerDown={beginResize}
            onDoubleClick={() =>
              onInspectorWidthChange?.(OUTLINE_INSPECTOR_DEFAULT)
            }
            onKeyDown={(event) => {
              const step = event.shiftKey ? 40 : 10;
              if (event.key === "ArrowLeft") {
                event.preventDefault();
                onInspectorWidthChange?.(
                  clampOutlineInspectorWidth(width + step),
                );
              } else if (event.key === "ArrowRight") {
                event.preventDefault();
                onInspectorWidthChange?.(
                  clampOutlineInspectorWidth(width - step),
                );
              } else if (event.key === "Home") {
                event.preventDefault();
                onInspectorWidthChange?.(OUTLINE_INSPECTOR_DEFAULT);
              } else if (event.key === "End") {
                event.preventDefault();
                onInspectorWidthChange?.(OUTLINE_INSPECTOR_MAX);
              }
            }}
          >
            <button
              type="button"
              className="outline-collapse-btn"
              aria-label={t("outline.canvas.collapseDetails")}
              title={t("outline.canvas.collapseDetails")}
              onClick={() => {
                setSelectedEdgeId(null);
                onClearSelection?.();
              }}
              onPointerDown={(event) => event.stopPropagation()}
            >
              {t("outline.canvas.collapseDetailsButton")}
            </button>
          </div>
          <aside className="outline-inspector" role="complementary">
            {selectedEdge ? (
              <>
                <span>{t("outline.canvas.relationDetails")}</span>
                <h3>
                  {(graph.nodes.find((node) => node.nodeId === selectedEdge.sourceNodeId)?.title ??
                    selectedEdge.sourceNodeId)}
                  {" — "}
                  {selectedEdge.label}
                  {" — "}
                  {(graph.nodes.find((node) => node.nodeId === selectedEdge.targetNodeId)?.title ??
                    selectedEdge.targetNodeId)}
                </h3>
                <p>
                  {selectedEdge.direction === "undirected" ? t("outline.canvas.undirected") : t("outline.canvas.directed")} · {selectedEdge.rationale}
                </p>
                {selectedEdge.uncertainty ? <p>{selectedEdge.uncertainty}</p> : null}
                <div className="outline-evidence-pills">
                  {edgeReferences(selectedEdge, t).map((reference, index) => {
                    const label = locatorLabel(reference, catalog, t);
                    return (
                      <button
                        key={`${selectedEdge.edgeId}-${index}`}
                        type="button"
                        onClick={() =>
                          onJumpEvidence({
                            blockId: reference.blockId,
                            pageNumber: reference.pageNumber,
                          })
                        }
                      >
                        {label.text}
                        {reference.purpose ? ` · ${reference.purpose}` : ""}
                      </button>
                    );
                  })}
                </div>
              </>
            ) : selected ? (
              <>
                {displayRole(selected, t) ? <span>{displayRole(selected, t)}</span> : null}
                <h3>{selected.title}</h3>
                <p>{selected.takeaway}</p>
                {selected.uncertainty ? <p>{selected.uncertainty}</p> : null}
                <button
                  type="button"
                  className="btn-liquid-pill"
                  disabled={!jumpId && !nodeReferences(selected, t).some((item) => item.pageNumber)}
                  onClick={() => {
                    const first = nodeReferences(selected, t)[0];
                    if (first) onJumpEvidence(first);
                    else if (jumpId) onJumpEvidence(jumpId);
                  }}
                >
                  {t("outline.canvas.jumpToSource")}
                </button>
                {v4 ? null : (
                  <label className="outline-crosslink-toggle">
                    <input
                      type="checkbox"
                      checked={crossLinksOn}
                      onChange={(event) => setCrossLinksOn(event.target.checked)}
                    />
                    {t("outline.canvas.showCrossLinks")}
                  </label>
                )}
                <div className="outline-evidence-pills">
                  {nodeReferences(selected, t).map((reference, index) => {
                    const label = locatorLabel(reference, catalog, t);
                    return (
                      <button
                        key={`${selected.nodeId}-${index}`}
                        type="button"
                        onClick={() => onJumpEvidence(reference)}
                      >
                        {label.text}
                        {reference.purpose ? ` · ${reference.purpose}` : ""}
                      </button>
                    );
                  })}
                </div>
                {onGenerateDeepDive ? <button
                  type="button"
                  className="btn-liquid-pill"
                  disabled={deepDiveBusy}
                  onClick={() => onGenerateDeepDive?.(selected)}
                >
                  {hasDeepDive ? t("outline.canvas.openLocalMap") : deepDiveBusy ? t("outline.canvas.generating") : t("outline.canvas.generateLocalMap")}
                </button> : null}
              </>
            ) : null}
          </aside>
        </>
      ) : null}
    </div>
  );
}

export default function OutlineCanvas(props: OutlineCanvasProps) {
  const { t } = useLocale();
  const instanceKey = outlineCanvasInstanceKey(props.graph, props.canvasKey);
  const [listView, setListView] = useState(false);
  const fallback = <OutlineGraphList graph={props.graph} catalog={props.catalog} onJump={props.onJumpEvidence} />;
  return <>
    <div className="outline-view-switch" style={{ padding: "6px 12px" }}>
      <button type="button" className="liquid-tab-btn" onClick={() => setListView(value => !value)}>
        {listView ? t("outline.canvas.showCanvas") : t("outline.canvas.nodeRelationList")}
      </button>
    </div>
    {listView ? fallback : <OutlineCanvasBoundary key={instanceKey} fallback={fallback}>
      <OutlineCanvasInner {...props} />
    </OutlineCanvasBoundary>}
  </>;
}
