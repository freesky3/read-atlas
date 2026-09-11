import React, { useEffect } from "react";
import { useLocale } from "../i18n/LocaleContext";

export interface CascadeOcrDeleteConfirmModalProps {
  open: boolean;
  paperTitle?: string;
  translationsCount?: number;
  lensCount?: number;
  guidesCount?: number;
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export const CascadeOcrDeleteConfirmModal: React.FC<
  CascadeOcrDeleteConfirmModalProps
> = ({
  open,
  paperTitle,
  translationsCount = 0,
  lensCount = 0,
  guidesCount = 0,
  busy = false,
  onConfirm,
  onCancel,
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

  return (
    <div
      className="scrim trash-modal-scrim open"
      onClick={() => {
        if (!busy) onCancel();
      }}
      role="dialog"
      aria-modal="true"
      aria-labelledby="cascade-ocr-delete-modal-title"
    >
      <div
        className="trash-confirm-card"
        style={{ maxWidth: 520 }}
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
            <span style={{ fontSize: 22 }}>🗑️</span>
          </div>
          <div>
            <h3
              id="cascade-ocr-delete-modal-title"
              className="trash-confirm-title"
            >
              {t("modals.ocr.title")}
            </h3>
            <p className="trash-confirm-subtitle">
              {t("modals.ocr.subtitle")}
            </p>
          </div>
        </header>

        <div className="trash-confirm-body">
          {paperTitle ? (
            <div className="trash-target-paper" style={{ marginBottom: 12 }}>
              <span>📄</span>
              <strong>{paperTitle}</strong>
            </div>
          ) : null}

          <div
            className="trash-warning-notice"
            style={{
              marginBottom: 14,
              borderLeft: "3px solid #ef4444",
              background: "rgba(239, 68, 68, 0.06)",
              padding: "10px 14px",
              borderRadius: "0 8px 8px 0",
            }}
          >
            <strong
              style={{
                display: "block",
                marginBottom: 6,
                color: "var(--red, #ef4444)",
                fontSize: 12.5,
              }}
            >
              ⚠️ {t("modals.ocr.clearHeading")}
            </strong>
            <ul
              style={{
                margin: 0,
                paddingLeft: 18,
                fontSize: 12,
                color: "var(--text-secondary, #64748b)",
                lineHeight: 1.6,
              }}
            >
              <li>{t("modals.ocr.clearOcr")}</li>
              <li>
                {t("modals.ocr.clearTranslations")}
                {translationsCount > 0
                  ? t("modals.ocr.aboutCount", { count: translationsCount })
                  : ""}
              </li>
              <li>
                {t("modals.ocr.clearLens")}
                {lensCount > 0
                  ? t("modals.ocr.aboutItems", { count: lensCount })
                  : ""}
              </li>
              <li>
                {t("modals.ocr.clearGuides")}
                {guidesCount > 0
                  ? t("modals.ocr.guideCount", { count: guidesCount })
                  : ""}
              </li>
            </ul>
          </div>

          <div
            style={{
              background: "rgba(34, 197, 94, 0.06)",
              borderLeft: "3px solid #22c55e",
              padding: "10px 14px",
              borderRadius: "0 8px 8px 0",
              fontSize: 12,
              color: "var(--text-secondary, #64748b)",
              lineHeight: 1.6,
            }}
          >
            <strong
              style={{
                display: "block",
                marginBottom: 4,
                color: "var(--green, #22c55e)",
                fontSize: 12.5,
              }}
            >
              🛡️ {t("modals.ocr.keepHeading")}
            </strong>
            <div>
              • {t("modals.ocr.keepPdf")}
              <br />
              • {t("modals.ocr.keepChat")}
              <br />
              • {t("modals.ocr.keepBrief")}
              <br />• {t("modals.ocr.keepTokens")}
            </div>
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
            className="btn-liquid-pill"
            style={{
              background: "var(--red, #ef4444)",
              color: "#fff",
              borderColor: "rgba(239, 68, 68, 0.4)",
            }}
            onClick={onConfirm}
            disabled={busy}
          >
            {busy ? t("modals.ocr.busy") : t("modals.ocr.confirm")}
          </button>
        </footer>
      </div>
    </div>
  );
};
