// Move-to preview dialog. Contract: docs/library-workspace-plan-2026-08.md §6.4:
// search physical folders, show blocked reasons, preview cross-root kind change,
// and never treat smart collections as destinations.
// Blocked reasons must come from the caller's DropEvaluation, shared with drag / menus.

import { useMemo, useRef, useState } from "react";
import { useDialogFocusTrap } from "../useDialogFocusTrap";
import { useLocale, type TranslateFn } from "../i18n/LocaleContext";
import { dropLabel, rootOf, type DropEvaluation } from "../library/dropPolicy";
import type { DocumentKind } from "../types";

export type MoveCandidate = Readonly<{
  path: string;
  depth: number;
  name: string;
  documentCount?: number;
}>;

export type MoveTargetPaper = Readonly<{
  id: string;
  title: string;
  collection: string;
  kind: DocumentKind;
  hasOcr?: boolean;
  briefStatus?: string;
  chapterNumber?: string;
}>;

type Row = Readonly<{
  candidate: MoveCandidate;
  status: "ok" | "kind_change" | "blocked";
  label: string;
}>;

/** Cross-root moves change document kind: spell out which artifacts drop away. */
export function kindChangeEffects(paper: MoveTargetPaper, targetPath: string, t: TranslateFn): string[] {
  if (rootOf(paper.collection) === rootOf(targetPath)) return [];
  const toTextbook = rootOf(targetPath) === "Textbooks";
  const effects = toTextbook
    ? [t("hub.move.effectTextbook")]
    : [t("hub.move.effectPaper")];
  if (paper.hasOcr) effects.push(t("hub.move.effectOcr"));
  if (paper.briefStatus === "ready") effects.push(t("hub.move.effectBrief"));
  return effects;
}

export default function HubMoveDialog({
  paper,
  count = 1,
  homeCollections,
  candidates,
  initialPath,
  evaluate,
  onConfirm,
  onClose,
}: Readonly<{
  paper: MoveTargetPaper;
  count?: number;
  /** Collections the selected documents currently live in. Mark "current folder" only when all already sit there. */
  homeCollections?: readonly string[];
  candidates: readonly MoveCandidate[];
  /** Keep the folder the user originally pointed at on a cross-root drag. */
  initialPath?: string | null;
  /** Same drop evaluation as drag / menus; allowed does not mean the dialog must proceed */
  evaluate: (targetPath: string) => DropEvaluation;
  onConfirm: (targetPath: string) => void | Promise<void>;
  onClose: () => void;
}>) {
  const { t } = useLocale();
  const containerRef = useRef<HTMLDivElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const [picked, setPicked] = useState<string | null>(initialPath ?? null);
  const [busy, setBusy] = useState(false);

  useDialogFocusTrap({ open: true, containerRef, onClose, initialFocusRef: searchRef });

  const homes = homeCollections ?? [paper.collection];
  const rows = useMemo<Row[]>(() => {
    const q = query.trim().toLowerCase();
    return candidates
      .filter((c) => !q || c.path.toLowerCase().includes(q) || c.name.toLowerCase().includes(q))
      .map((candidate) => {
        if (homes.length > 0 && homes.every((home) => home === candidate.path)) {
          return { candidate, status: "blocked" as const, label: t("hub.move.currentFolder") };
        }
        const evaluation = evaluate(candidate.path);
        if (evaluation.allowed) return { candidate, status: "ok" as const, label: t("hub.move.sameRoot") };
        if (evaluation.reason === "kind_change_requires_dialog") {
          const from = t(paper.kind === "textbook" ? "hub.kind.textbook" : "hub.kind.paper");
          const to = t(rootOf(candidate.path) === "Textbooks" ? "hub.kind.textbook" : "hub.kind.paper");
          return {
            candidate,
            status: "kind_change" as const,
            label: `${from} → ${to}`,
          };
        }
        return { candidate, status: "blocked" as const, label: dropLabel(evaluation.reason, t) };
      });
  }, [candidates, evaluate, homes, paper.kind, query, t]);

  const activePath = picked ?? rows.find((r) => r.status !== "blocked")?.candidate.path ?? null;
  const activeRow = rows.find((r) => r.candidate.path === activePath) ?? null;
  const effects = activeRow && activeRow.status === "kind_change"
    ? kindChangeEffects(paper, activeRow.candidate.path, t)
    : [];

  const confirm = async () => {
    if (!activeRow || busy) return;
    setBusy(true);
    try {
      await onConfirm(activeRow.candidate.path);
    } finally {
      setBusy(false);
    }
  };

  const onKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const list = rows.filter((r) => r.status !== "blocked");
      if (list.length === 0) return;
      const index = list.findIndex((r) => r.candidate.path === activePath);
      const next = event.key === "ArrowDown"
        ? Math.min(list.length - 1, index + 1)
        : Math.max(0, index - 1);
      setPicked(list[next < 0 ? 0 : next].candidate.path);
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      void confirm();
    }
  };

  return (
    <div className="scrim hub-move-scrim" role="presentation" onClick={(e) => { if (e.target === e.currentTarget && !busy) onClose(); }}>
      <div
        ref={containerRef}
        className="hub-move-card"
        role="dialog"
        aria-modal="true"
        aria-labelledby="hub-move-title"
        onKeyDown={onKeyDown}
      >
        <header className="hub-move-header">
          <h3 id="hub-move-title">{t("hub.move.title")}</h3>
          <p>{count > 1 ? t("hub.move.previewMany", { count, title: paper.title }) : paper.title}</p>
          <span className="hub-move-current">{t("hub.move.current")}<code>{homes.length === 1 ? (homes[0] || t("hub.move.unknownFolder")) : t("hub.move.differentFolders", { count: homes.length })}</code></span>
        </header>

        <input
          ref={searchRef}
          type="text"
          className="hub-move-search"
          placeholder={t("hub.move.searchPlaceholder")}
          value={query}
          onChange={(e) => { setQuery(e.target.value); setPicked(null); }}
          aria-label={t("hub.move.searchAria")}
        />

        <div className="hub-move-list" role="listbox" aria-label={t("hub.move.listAria")}>
          {rows.length === 0 ? (
            <p className="hub-move-empty">{t("hub.move.empty")}</p>
          ) : rows.map((row) => {
            const isActive = row.candidate.path === activePath;
            return (
              <button
                key={row.candidate.path}
                type="button"
                role="option"
                aria-selected={isActive}
                disabled={row.status === "blocked" || busy}
                title={row.status === "blocked" ? row.label : undefined}
                className={`hub-move-row hub-move-${row.status} ${isActive ? "active" : ""}`}
                style={{ paddingLeft: `${row.candidate.depth * 12 + 10}px` }}
                onClick={() => setPicked(row.candidate.path)}
                onDoubleClick={() => { if (row.status !== "blocked") { setPicked(row.candidate.path); void confirm(); } }}
              >
                <span className="hub-move-name">{row.candidate.name}</span>
                <span className="hub-move-meta">{row.candidate.path}</span>
                <span className="hub-move-tag">
                  {row.status === "kind_change" ? t("hub.move.crossRoot", { label: row.label }) : row.status === "blocked" ? row.label : t("hub.move.sameRootTag")}
                  {typeof row.candidate.documentCount === "number" ? ` · ${row.candidate.documentCount}` : ""}
                </span>
              </button>
            );
          })}
        </div>

        {effects.length > 0 ? (
          <div className="hub-move-effect" role="alert">
            <strong>{t("hub.move.kindChangeTitle")}</strong>
            <ul>{effects.map((text) => (<li key={text}>{text}</li>))}</ul>
          </div>
        ) : null}

        <footer className="hub-move-footer">
          <button type="button" className="btn-liquid-pill" onClick={onClose} disabled={busy}>{t("hub.cancel")}</button>
          <button
            type="button"
            className="btn-liquid-pill primary"
            onClick={() => void confirm()}
            disabled={!activeRow || busy}
            title={activeRow ? t("hub.move.toPath", { path: activeRow.candidate.path }) : t("hub.move.noTarget")}
          >
            {busy ? t("hub.move.moving") : activeRow?.status === "kind_change" ? t("hub.move.confirmCross") : t("hub.move.confirmHere")}
          </button>
        </footer>
      </div>
    </div>
  );
}
