import React, { useEffect } from "react";
import { useLocale } from "../i18n/LocaleContext";

export interface TrashConfirmModalProps {
  open: boolean;
  paperTitle: string;
  paperPath?: string;
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
  documentCount?: number;
  isFolder?: boolean;
}

export const TrashConfirmModal: React.FC<TrashConfirmModalProps> = ({
  open,
  paperTitle,
  paperPath,
  busy = false,
  onConfirm,
  onCancel,
  documentCount,
  isFolder,
}) => {
  const { t } = useLocale();

  useEffect(() => {
    if (!open) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) {
        onCancel();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [open, busy, onCancel]);

  if (!open) return null;

  const subtitle = isFolder
    ? documentCount && documentCount > 0
      ? t("modals.trash.subtitleFolder", {
          title: paperTitle,
          count: documentCount,
        })
      : t("modals.trash.subtitleEmptyFolder", { title: paperTitle })
    : t("modals.trash.subtitlePaper");

  return (
    <div
      className="scrim trash-modal-scrim open"
      onClick={() => {
        if (!busy) onCancel();
      }}
      role="dialog"
      aria-modal="true"
      aria-labelledby="trash-modal-title"
    >
      <div
        className="trash-confirm-card"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="trash-confirm-header">
          <div className="trash-confirm-icon-box">
            <svg
              width="22"
              height="22"
              viewBox="0 0 24 24"
              fill="none"
              stroke="#ef4444"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <polyline points="3 6 5 6 21 6" />
              <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
              <line x1="10" y1="11" x2="10" y2="17" />
              <line x1="14" y1="11" x2="14" y2="17" />
            </svg>
          </div>
          <div>
            <h3 id="trash-modal-title" className="trash-confirm-title">
              {t("modals.trash.title")}
            </h3>
            <p className="trash-confirm-subtitle">{subtitle}</p>
          </div>
        </header>

        <div className="trash-confirm-body">
          <div className="trash-target-paper">
            <span className="target-icon">📄</span>
            <div className="target-details">
              <strong className="target-title">{paperTitle}</strong>
              {paperPath ? (
                <span className="target-path">{paperPath}</span>
              ) : null}
            </div>
          </div>

          <div className="trash-warning-notice">
            <span className="warning-bullet">⚠️</span>
            <p>
              {t("modals.trash.warningBefore")}
              <strong>{t("modals.trash.warningStrong")}</strong>
              {t("modals.trash.warningAfter")}
            </p>
          </div>
        </div>

        <footer className="trash-confirm-footer">
          <button
            type="button"
            className="btn-liquid-pill"
            onClick={onCancel}
            disabled={busy}
          >
            {t("modals.cancel")}
          </button>
          <button
            type="button"
            className="btn-liquid-pill btn-danger-pill"
            onClick={onConfirm}
            disabled={busy}
          >
            {busy ? t("modals.trash.busy") : t("modals.trash.confirm")}
          </button>
        </footer>
      </div>
    </div>
  );
};

export default TrashConfirmModal;
