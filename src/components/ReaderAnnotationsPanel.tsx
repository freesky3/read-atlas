import { useMemo, useState } from "react";
import {
  Bookmark,
  Check,
  ChevronLeft,
  Highlighter,
  MoreHorizontal,
  Pencil,
  Trash2,
} from "lucide-react";
import type {
  AnnotationKind,
  AnnotationStatus,
  UserAnnotation,
} from "../types";
import { useLocale, type TranslateFn } from "../i18n/LocaleContext";


export type ReaderAnnotationKind = AnnotationKind;
export type ReaderAnnotationStatus = AnnotationStatus;
export type ReaderAnnotation = UserAnnotation;

export type ReaderAnnotationsPanelProps = {
  annotations: ReaderAnnotation[];
  activeAnnotationId?: string | null;
  onJump: (annotation: ReaderAnnotation) => void;
  onUpdate?: (
    id: string,
    patch: { body?: string; title?: string; color?: string },
  ) => Promise<void> | void;
  onDelete?: (annotation: ReaderAnnotation) => Promise<void> | void;
  onBack?: () => void;
  backDisabled?: boolean;
  savingIds?: ReadonlySet<string>;
  className?: string;
};

type Filter = "all" | ReaderAnnotationKind;
const FILTERS: Filter[] = ["all", "highlight", "note", "bookmark"];
const COLORS = ["#f5c542", "#71c7ec", "#a7df8d", "#f59ab2", "#c4b5fd"];

function filterLabel(id: Filter, t: TranslateFn) {
  return id === "all"
    ? t("reader.annotations.filter.all")
    : t(`reader.kind.${id}`);
}
function kindLabel(kind: ReaderAnnotationKind, t: TranslateFn) {
  return t(`reader.kind.${kind}`);
}
function kindIcon(kind: ReaderAnnotationKind) {
  return kind === "highlight" ? (
    <Highlighter size={14} aria-hidden="true" />
  ) : kind === "bookmark" ? (
    <Bookmark size={14} aria-hidden="true" />
  ) : (
    <Pencil size={14} aria-hidden="true" />
  );
}
function excerpt(annotation: ReaderAnnotation, t: TranslateFn) {
  const text = (annotation.body || annotation.locator.excerpt || "").trim();
  return text
    ? text.length > 150
      ? text.slice(0, 150) + "…"
      : text
    : t("reader.annotations.noExcerpt");
}
function statusLabel(status: ReaderAnnotationStatus | undefined, t: TranslateFn) {
  return status === "orphan" || status === "orphaned"
    ? t("reader.annotations.status.needsRelocate")
    : status === "migrated"
      ? t("reader.annotations.status.migrated")
      : status === "deleted"
        ? t("reader.annotations.status.deleted")
        : t("reader.annotations.status.saved");
}

export function ReaderAnnotationsPanel({
  annotations,
  activeAnnotationId,
  onJump,
  onUpdate,
  onDelete,
  onBack,
  backDisabled = false,
  savingIds,
  className = "",
}: ReaderAnnotationsPanelProps) {
  const { t } = useLocale();
  const [filter, setFilter] = useState<Filter>("all");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [colorMenuId, setColorMenuId] = useState<string | null>(null);
  const [localSaving, setLocalSaving] = useState<Set<string>>(new Set());
  const counts = useMemo(() => {
    const result: Record<Filter, number> = {
      all: annotations.length,
      highlight: 0,
      note: 0,
      bookmark: 0,
    };
    annotations.forEach((item) => {
      result[item.kind] += 1;
    });
    return result;
  }, [annotations]);
  const visible = useMemo(
    () =>
      annotations
        .filter((item) => filter === "all" || item.kind === filter)
        .filter((item) => item.status !== "deleted")
        .slice()
        .sort(
          (a, b) =>
            a.locator.pageNumber - b.locator.pageNumber ||
            (a.createdAt || "").localeCompare(b.createdAt || ""),
        ),
    [annotations, filter],
  );
  const isSaving = (id: string) =>
    Boolean(savingIds?.has(id) || localSaving.has(id));
  const runUpdate = async (
    id: string,
    patch: { body?: string; title?: string; color?: string },
  ) => {
    if (!onUpdate) return;
    setLocalSaving((current) => new Set(current).add(id));
    try {
      await onUpdate(id, patch);
    } finally {
      setLocalSaving((current) => {
        const next = new Set(current);
        next.delete(id);
        return next;
      });
    }
  };

  return (
    <aside
      className={"reader-annotations-panel " + className}
      aria-label={t("reader.annotations.panelAria")}
    >
      <header className="reader-annotations-header">
        <div className="reader-annotations-heading">
          {onBack ? (
            <button
              type="button"
              className="reader-annotations-back"
              onClick={onBack}
              disabled={backDisabled}
              aria-label={t("reader.annotations.back")}
              title={t("reader.annotations.back")}
            >
              <ChevronLeft size={16} aria-hidden="true" />
            </button>
          ) : null}
          <div>
            <strong>{t("reader.annotations.title")}</strong>
            <small>{t("reader.annotations.count", { count: annotations.length })}</small>
          </div>
        </div>
        <button
          type="button"
          className="reader-annotations-more"
          aria-label={t("reader.annotations.more")}
          title={t("reader.annotations.more")}
        >
          <MoreHorizontal size={16} aria-hidden="true" />
        </button>
      </header>
      <div
        className="reader-annotations-filters"
        role="tablist"
        aria-label={t("reader.annotations.typeAria")}
      >
        {FILTERS.map((id) => (
          <button
            key={id}
            type="button"
            role="tab"
            aria-selected={filter === id}
            className={filter === id ? "active" : ""}
            onClick={() => setFilter(id)}
          >
            {filterLabel(id, t)}
            <span>{counts[id]}</span>
          </button>
        ))}
      </div>
      <div className="reader-annotations-list" role="list" aria-live="polite">
        {visible.length === 0 ? (
          <div className="reader-annotations-empty">
            <Highlighter size={20} aria-hidden="true" />
            <strong>
              {filter === "all"
                ? t("reader.annotations.emptyAll")
                : t("reader.annotations.emptyKind", {
                    kind: filterLabel(filter, t),
                  })}
            </strong>
            <span>
              {t("reader.annotations.emptyHint")}
            </span>
          </div>
        ) : (
          visible.map((annotation) => {
            const editing = editingId === annotation.id;
            const color =
              annotation.color ||
              (annotation.kind === "highlight"
                ? COLORS[0]
                : "var(--blue, #2563eb)");
            return (
              <article
                key={annotation.id}
                role="listitem"
                className={
                  "reader-annotation-item " +
                  (activeAnnotationId === annotation.id ? "is-active " : "") +
                  "status-" +
                  (annotation.status || "active")
                }
                style={
                  annotation.kind === "highlight"
                    ? { borderLeftColor: color }
                    : undefined
                }
              >
                <button
                  type="button"
                  className="reader-annotation-jump"
                  onClick={() => onJump(annotation)}
                  title={t("reader.annotations.jumpTitle", { page: annotation.locator.pageNumber })}
                >
                  <span className="reader-annotation-meta">
                    <span className="reader-annotation-kind">
                      {kindIcon(annotation.kind)} {kindLabel(annotation.kind, t)}
                    </span>
                    <span>p.{annotation.locator.pageNumber}</span>
                    {annotation.status && annotation.status !== "active" ? (
                      <em>{statusLabel(annotation.status, t)}</em>
                    ) : null}
                  </span>
                  {annotation.title ? (
                    <strong className="reader-annotation-title">
                      {annotation.title}
                    </strong>
                  ) : null}
                  <span className="reader-annotation-excerpt">
                    {excerpt(annotation, t)}
                  </span>
                </button>
                {editing && annotation.kind === "note" ? (
                  <div className="reader-annotation-editor">
                    <textarea
                      value={draft}
                      onChange={(event) => setDraft(event.target.value)}
                      aria-label={t("reader.annotations.editNote")}
                      autoFocus
                    />
                    <div className="reader-annotation-editor-actions">
                      <button
                        type="button"
                        onClick={() => {
                          setEditingId(null);
                          setDraft("");
                        }}
                      >
                        {t("reader.cancel")}
                      </button>
                      <button
                        type="button"
                        className="primary"
                        disabled={isSaving(annotation.id)}
                        onClick={() => {
                          void runUpdate(annotation.id, { body: draft });
                          setEditingId(null);
                        }}
                      >
                        {isSaving(annotation.id) ? t("reader.saving") : t("reader.save")}
                      </button>
                    </div>
                  </div>
                ) : null}
                <div className="reader-annotation-actions">
                  {annotation.kind === "highlight" && onUpdate ? (
                    <div className="reader-annotation-color-wrap">
                      <button
                        type="button"
                        className="reader-annotation-color"
                        style={{ background: color }}
                        aria-label={t("reader.annotations.changeColor")}
                        title={t("reader.annotations.changeColor")}
                        onClick={() =>
                          setColorMenuId((current) =>
                            current === annotation.id ? null : annotation.id,
                          )
                        }
                      />
                      {colorMenuId === annotation.id ? (
                        <div
                          className="reader-annotation-colors"
                          role="menu"
                          aria-label={t("reader.annotations.colorMenu")}
                        >
                          {COLORS.map((nextColor) => (
                            <button
                              key={nextColor}
                              type="button"
                              role="menuitem"
                              className="reader-annotation-color"
                              style={{ background: nextColor }}
                              aria-label={t("reader.annotations.selectColor", { color: nextColor })}
                              onClick={() => {
                                setColorMenuId(null);
                                void runUpdate(annotation.id, {
                                  color: nextColor,
                                });
                              }}
                            />
                          ))}
                        </div>
                      ) : null}
                    </div>
                  ) : null}
                  {annotation.kind === "note" && onUpdate ? (
                    <button
                      type="button"
                      className="reader-annotation-icon"
                      aria-label={t("reader.annotations.editNote")}
                      title={t("reader.annotations.editNote")}
                      disabled={isSaving(annotation.id)}
                      onClick={() => {
                        setDraft(annotation.body || "");
                        setEditingId(annotation.id);
                      }}
                    >
                      <Pencil size={13} aria-hidden="true" />
                    </button>
                  ) : null}
                  {onDelete ? (
                    <button
                      type="button"
                      className="reader-annotation-icon danger"
                      aria-label={t("reader.annotations.delete")}
                      title={t("reader.annotations.delete")}
                      disabled={isSaving(annotation.id)}
                      onClick={() => {
                        if (
                          window.confirm(
                            t("reader.annotations.deleteConfirm", {
                              kind: kindLabel(annotation.kind, t),
                            }),
                          )
                        )
                          void onDelete(annotation);
                      }}
                    >
                      <Trash2 size={13} aria-hidden="true" />
                    </button>
                  ) : null}
                  <span
                    className="reader-annotation-save-state"
                    aria-live="polite"
                  >
                    {isSaving(annotation.id) ? (
                      t("reader.saving")
                    ) : (
                      <>
                        <Check size={11} aria-hidden="true" />{" "}
                        {statusLabel(annotation.status, t)}
                      </>
                    )}
                  </span>
                </div>
              </article>
            );
          })
        )}
      </div>
    </aside>
  );
}

export default ReaderAnnotationsPanel;
