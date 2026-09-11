import { useState } from "react";
import type { OutlineCatalogEntry, OutlineGraph, OutlineReference } from "../types";
import { useLocale } from "../i18n/LocaleContext";
import { displayRole, edgeReferences, locatorLabel, nodeReferences } from "./outlineDisplay";

export type OutlineJumpTarget = string | { blockId?: string | null; pageNumber?: number | null };
export function OutlineReferences({ references, catalog, onJump }: {
  references: OutlineReference[]; catalog: OutlineCatalogEntry[]; onJump: (target: OutlineJumpTarget) => void;
}) {
  const { t } = useLocale();
  return <div className="outline-evidence-pills">{references.map((reference, index) =>
    <button key={index} type="button" onClick={() => onJump(reference)}>
      {locatorLabel(reference, catalog, t).text}{reference.purpose ? ` · ${reference.purpose}` : ""}
    </button>)}</div>;
}

/** Same persisted graph, available both on demand and when canvas rendering fails. */
export default function OutlineGraphList({ graph, catalog, onJump }: {
  graph: OutlineGraph; catalog: OutlineCatalogEntry[]; onJump: (target: OutlineJumpTarget) => void;
}) {
  const { t } = useLocale();
  const [query, setQuery] = useState("");
  const matches = (text: string) => text.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase());
  const title = (id: string) => graph.nodes.find(node => node.nodeId === id)?.title ?? id;
  const separator = t("outline.list.separator");
  return <section className="outline-fallback-list" aria-label={t("outline.list.aria")} style={{ overflow: "auto", padding: 16, minHeight: 0 }}>
    <h3>{graph.title}</h3><p>{graph.summary}</p>
    <label>{t("outline.list.search")} <input value={query} onChange={event => setQuery(event.target.value)} /></label>
    <h4>{t("outline.list.nodes")}</h4><ol>{graph.nodes.filter(node => matches(`${node.title} ${node.takeaway} ${displayRole(node, t) ?? ""}`)).map(node =>
      <li key={node.nodeId}><strong>{node.title}</strong>{displayRole(node, t) ? <small> · {displayRole(node, t)}</small> : null}
        <p>{node.takeaway}</p>{node.uncertainty ? <p>{node.uncertainty}</p> : null}
        <OutlineReferences references={nodeReferences(node, t)} catalog={catalog} onJump={onJump} />
      </li>)}</ol>
    <h4>{t("outline.list.relations")}</h4><ul>{graph.edges.filter(edge => matches(`${title(edge.sourceNodeId)} ${edge.label} ${title(edge.targetNodeId)} ${edge.rationale}`)).map(edge =>
      <li key={edge.edgeId}><strong>{title(edge.sourceNodeId)} {edge.direction === "undirected" ? "—" : "→"} {title(edge.targetNodeId)} · {edge.label}</strong>
        <p>{edge.rationale}</p>{edge.uncertainty ? <p>{edge.uncertainty}</p> : null}
        <OutlineReferences references={edgeReferences(edge, t)} catalog={catalog} onJump={onJump} />
      </li>)}</ul>
    {graph.groups?.length ? <><h4>{t("outline.list.groups")}</h4><ul>{graph.groups.map(group => <li key={group.groupId}><strong>{group.title}</strong><p>{group.description}</p><p>{group.nodeIds.map(title).join(separator)}</p></li>)}</ul></> : null}
  </section>;
}
