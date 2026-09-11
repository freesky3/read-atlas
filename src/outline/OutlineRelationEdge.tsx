import { BaseEdge, EdgeLabelRenderer, type EdgeProps } from "@xyflow/react";
import { useLocale } from "../i18n/LocaleContext";
import type { OutlineFlowEdge } from "./outlineLayout";
import { outlineEdgePath } from "./outlineEdgePath";

export default function OutlineRelationEdge(props: EdgeProps<OutlineFlowEdge>) {
  const { t } = useLocale();
  const { id, data, sourceX, sourceY, targetX, targetY, markerEnd, style } = props;
  if (!data) return null;
  const edge = data.outlineEdge;
  const route = outlineEdgePath({ sourceX, sourceY, targetX, targetY,
    lane: data.lane ?? 0, selfLoop: edge.sourceNodeId === edge.targetNodeId,
    sourceBeforeTarget: edge.sourceNodeId.localeCompare(edge.targetNodeId) < 0 });
  return <>
    <BaseEdge id={id} path={route.path} markerEnd={markerEnd} style={style} interactionWidth={24} />
    <EdgeLabelRenderer>
      <button type="button" className="outline-relation-label nodrag nopan"
        aria-label={t("outline.edge.aria", { label: edge.label })} aria-pressed={data.active ?? false}
        style={{ position: "absolute", transform: `translate(-50%, -50%) translate(${route.labelX}px,${route.labelY}px)`, pointerEvents: "all", maxWidth: 180, whiteSpace: "normal" }}
        onClick={(event) => { event.stopPropagation(); data.onSelect?.(id); }}>
        {edge.label}
      </button>
    </EdgeLabelRenderer>
  </>;
}
