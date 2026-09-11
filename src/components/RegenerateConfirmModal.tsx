import React, { useEffect } from "react";
import { useLocale } from "../i18n/LocaleContext";

export interface RegenerateConfirmModalProps {
  open: boolean;
  artifactTitle: string;
  kindLabel?: string;
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export const RegenerateConfirmModal: React.FC<RegenerateConfirmModalProps> = ({
  open,
  artifactTitle,
  kindLabel,
  busy = false,
  onConfirm,
  onCancel,
}) => {
  const { t } = useLocale();
  const resolvedKind = kindLabel ?? t("modals.regenerate.defaultKind");

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
      aria-labelledby="regenerate-modal-title"
    >
      <div
        className="trash-confirm-card"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="trash-confirm-header">
          <div
            className="trash-confirm-icon-box"
            style={{
              background: "rgba(59, 130, 246, 0.12)",
              borderColor: "rgba(59, 130, 246, 0.25)",
            }}
          >
            <span style={{ fontSize: 20 }}>🔄</span>
          </div>
          <div>
            <h3 id="regenerate-modal-title" className="trash-confirm-title">
              {t("modals.regenerate.title", { kind: resolvedKind })}
            </h3>
            <p className="trash-confirm-subtitle">
              {t("modals.regenerate.subtitle")}
            </p>
          </div>
        </header>

        <div className="trash-confirm-body">
          <div className="trash-target-paper">
            <span className="target-icon">🔍</span>
            <div className="target-details">
              <strong className="target-title">
                {artifactTitle || t("modals.regenerate.fallbackTitle")}
              </strong>
              <span className="target-path">
                {t("modals.regenerate.targetPath")}
              </span>
            </div>
          </div>

          <div
            className="trash-warning-notice"
            style={{
              background: "rgba(59, 130, 246, 0.08)",
              borderColor: "rgba(59, 130, 246, 0.2)",
              color: "var(--ink)",
            }}
          >
            <span className="warning-bullet">💡</span>
            <p>
              {t("modals.regenerate.warningBefore")}
              <strong>{t("modals.regenerate.warningStrong")}</strong>
              {t("modals.regenerate.warningAfter")}
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
            className="btn-liquid-pill primary"
            onClick={onConfirm}
            disabled={busy}
          >
            {busy ? t("modals.regenerate.busy") : t("modals.regenerate.confirm")}
          </button>
        </footer>
      </div>
    </div>
  );
};

export default RegenerateConfirmModal;
