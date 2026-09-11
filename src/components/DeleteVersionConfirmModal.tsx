import React, { useEffect } from "react";
import { useLocale } from "../i18n/LocaleContext";

export interface DeleteVersionConfirmModalProps {
  open: boolean;
  versionNumber: number;
  totalVersions: number;
  kindLabel?: string;
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export const DeleteVersionConfirmModal: React.FC<DeleteVersionConfirmModalProps> = ({
  open,
  versionNumber,
  totalVersions,
  kindLabel,
  busy = false,
  onConfirm,
  onCancel,
}) => {
  const { t } = useLocale();
  const resolvedKind = kindLabel ?? t("modals.deleteVersion.defaultKind");

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

  return (
    <div
      className="scrim trash-modal-scrim open"
      onClick={() => {
        if (!busy) onCancel();
      }}
      role="dialog"
      aria-modal="true"
      aria-labelledby="delete-version-modal-title"
    >
      <div
        className="trash-confirm-card"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="trash-confirm-header">
          <div
            className="trash-confirm-icon-box"
            style={{
              background: "rgba(239, 68, 68, 0.12)",
              borderColor: "rgba(239, 68, 68, 0.25)",
            }}
          >
            <span style={{ fontSize: 20 }}>🗑️</span>
          </div>
          <div>
            <h3 id="delete-version-modal-title" className="trash-confirm-title">
              {t("modals.deleteVersion.title", {
                version: versionNumber,
                total: totalVersions,
              })}
            </h3>
            <p className="trash-confirm-subtitle">
              {t("modals.deleteVersion.subtitle", { kind: resolvedKind })}
            </p>
          </div>
        </header>

        <div className="trash-confirm-body">
          <p className="trash-confirm-detail">
            {t("modals.deleteVersion.detail", { total: totalVersions })}
          </p>
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
            className="btn-liquid-pill"
            style={{
              background: "var(--red, #ef4444)",
              color: "#fff",
              borderColor: "rgba(239, 68, 68, 0.4)",
            }}
            onClick={onConfirm}
            disabled={busy}
          >
            {busy ? t("modals.deleteVersion.busy") : t("modals.deleteVersion.confirm")}
          </button>
        </footer>
      </div>
    </div>
  );
};

export default DeleteVersionConfirmModal;
