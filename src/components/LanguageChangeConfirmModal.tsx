import React, { useEffect } from "react";
import { useLocale } from "../i18n/LocaleContext";
import type { UiLocale } from "../i18n/types";

export function LanguageChangeConfirmModal({
  open,
  next,
  busy = false,
  onConfirm,
  onCancel,
}: {
  open: boolean;
  next: UiLocale;
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const { t } = useLocale();
  const toEn = next === "en";

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
      aria-labelledby="language-change-title"
    >
      <div
        className="trash-confirm-card"
        onClick={(event) => event.stopPropagation()}
      >
        <header className="trash-confirm-header">
          <div>
            <h3 id="language-change-title" className="trash-confirm-title">
              {t("locale.confirm.title")}
            </h3>
          </div>
        </header>
        <div className="trash-confirm-body">
          <p>
            {t(toEn ? "locale.confirm.bodyEn" : "locale.confirm.bodyZh")}
          </p>
        </div>
        <footer className="trash-confirm-footer">
          <button
            type="button"
            className="btn-liquid-pill"
            onClick={onCancel}
            disabled={busy}
          >
            {t("locale.confirm.cancel")}
          </button>
          <button
            type="button"
            className="btn-liquid-pill primary"
            onClick={onConfirm}
            disabled={busy}
          >
            {t(toEn ? "locale.confirm.okEn" : "locale.confirm.okZh")}
          </button>
        </footer>
      </div>
    </div>
  );
}

export default LanguageChangeConfirmModal;
