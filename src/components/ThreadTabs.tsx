import { useEffect, useRef, useState } from "react";
import type { Thread } from "../types";
import { useLocale } from "../i18n/LocaleContext";


type ThreadTabsProps = {
  threads: Thread[];
  archivedThreads: Thread[];
  activeThreadId: string;
  onSelect: (thread: Thread) => void;
  onCreate: () => void;
  onClose: (thread: Thread) => void;
  onRename: (thread: Thread, title: string) => void;
  onRestore: (thread: Thread) => void;
  onDelete: (thread: Thread) => void;
};

function formatClosedAt(value: string, dateLocale: "zh-CN" | "en-US") {
  try {
    return new Intl.DateTimeFormat(dateLocale, {
      month: "numeric",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    }).format(new Date(value));
  } catch {
    return value;
  }
}

export default function ThreadTabs({
  threads,
  archivedThreads,
  activeThreadId,
  onSelect,
  onCreate,
  onClose,
  onRename,
  onRestore,
  onDelete,
}: ThreadTabsProps) {
  const { t, dateLocale } = useLocale();
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [archivedOpen, setArchivedOpen] = useState(false);
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);
  const archivedRef = useRef<HTMLDivElement>(null);
  const renameRef = useRef<HTMLInputElement>(null);
  const editingIdRef = useRef<string | null>(null);
  const canClose = threads.length > 1;
  editingIdRef.current = editingId;

  useEffect(() => {
    if (editingId) renameRef.current?.focus();
  }, [editingId]);

  useEffect(() => {
    if (!archivedOpen) return;
    const onPointerDown = (event: PointerEvent) => {
      if (
        archivedRef.current &&
        !archivedRef.current.contains(event.target as Node)
      ) {
        setArchivedOpen(false);
        setConfirmDeleteId(null);
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      setArchivedOpen(false);
      setConfirmDeleteId(null);
    };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown, true);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown, true);
    };
  }, [archivedOpen]);

  const beginRename = (thread: Thread) => {
    editingIdRef.current = thread.id;
    setEditingId(thread.id);
    setDraft(thread.title);
  };

  const commitRename = (thread: Thread) => {
    if (editingIdRef.current !== thread.id) return;
    editingIdRef.current = null;
    const next = draft.trim();
    setEditingId(null);
    if (!next || next === thread.title) return;
    onRename(thread, next);
  };

  return (
    <div className="thread-tabs">
      <div className="thread-tab-list" role="tablist" aria-label={t("reader.threads.tablist")}>
        {threads.map((thread) => (
          <div
            className={`thread-tab ${thread.id === activeThreadId ? "active" : ""}`}
            key={thread.id}
          >
            {editingId === thread.id ? (
              <input
                ref={renameRef}
                className="thread-tab-rename"
                aria-label={t("reader.threads.rename")}
                value={draft}
                onChange={(event) => setDraft(event.target.value)}
                onBlur={() => commitRename(thread)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    commitRename(thread);
                  }
                  if (event.key === "Escape") {
                    event.preventDefault();
                    event.stopPropagation();
                    editingIdRef.current = null;
                    setEditingId(null);
                  }
                }}
              />
            ) : (
              <button
                type="button"
                role="tab"
                aria-selected={thread.id === activeThreadId}
                className="thread-tab-title"
                onClick={() => onSelect(thread)}
                onDoubleClick={() => beginRename(thread)}
                onContextMenu={(event) => {
                  event.preventDefault();
                  beginRename(thread);
                }}
                title={thread.title}
              >
                {thread.title}
              </button>
            )}
            <button
              type="button"
              className="thread-tab-close"
              aria-label={t("reader.threads.closeAria", { title: thread.title })}
              title={canClose ? t("reader.threads.close") : t("reader.threads.keepOne")}
              disabled={!canClose}
              onClick={(event) => {
                event.stopPropagation();
                onClose(thread);
              }}
            >
              ×
            </button>
          </div>
        ))}
      </div>
      <button type="button" className="thread-tab thread-tab-action" onClick={onCreate}>
        {t("reader.threads.new")}
      </button>
      <div className="archived-threads" ref={archivedRef}>
        <button
          type="button"
          className={`thread-tab thread-tab-action ${archivedOpen ? "active" : ""}`}
          aria-label={t("reader.threads.closedAria")}
          aria-expanded={archivedOpen}
          aria-haspopup="dialog"
          onClick={() => {
            setArchivedOpen((open) => !open);
            setConfirmDeleteId(null);
          }}
        >
          {t("reader.threads.closed")}
          {archivedThreads.length > 0 ? (
            <span className="archived-count" aria-hidden="true">
              {archivedThreads.length}
            </span>
          ) : null}
        </button>
        {archivedOpen ? (
          <div className="archived-thread-popover" role="dialog" aria-label={t("reader.threads.closedAria")}>
            {archivedThreads.length === 0 ? (
              <p className="archived-thread-empty">{t("reader.threads.closedEmpty")}</p>
            ) : (
              <ul className="archived-thread-list">
                {archivedThreads.map((thread) => (
                  <li key={thread.id}>
                    <div>
                      <strong>{thread.title}</strong>
                      <small>{formatClosedAt(thread.updatedAt, dateLocale)}</small>
                    </div>
                    <div className="archived-thread-actions">
                      <button type="button" onClick={() => onRestore(thread)}>
                        {t("reader.threads.restore")}
                      </button>
                      {confirmDeleteId === thread.id ? (
                        <>
                          <button
                            type="button"
                            className="danger"
                            onClick={() => {
                              onDelete(thread);
                              setConfirmDeleteId(null);
                            }}
                          >
                            {t("reader.threads.confirmDelete")}
                          </button>
                          <button
                            type="button"
                            onClick={() => setConfirmDeleteId(null)}
                          >
                            {t("reader.cancel")}
                          </button>
                        </>
                      ) : (
                        <button
                          type="button"
                          className="danger"
                          onClick={() => setConfirmDeleteId(thread.id)}
                        >
                          {t("reader.threads.delete")}
                        </button>
                      )}
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </div>
        ) : null}
      </div>
    </div>
  );
}
