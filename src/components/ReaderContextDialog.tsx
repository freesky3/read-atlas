import { useEffect, useRef, useState } from "react";
import { useDialogFocusTrap } from "../useDialogFocusTrap";
import MarkdownBody from "../MarkdownBody";
import type { ReaderContextProjection, ReaderContextScope } from "../types";
import { useLocale, type TranslateFn } from "../i18n/LocaleContext";


export type ReaderContextTarget = {
  scope: ReaderContextScope;
  collectionPath?: string;
  paperId?: string;
};

function titleFor(scope: ReaderContextScope, t: TranslateFn): string {
  if (scope === "workspace") return t("reader.context.title.workspace");
  if (scope === "folder") return t("reader.context.title.folder");
  return t("reader.context.title.paper");
}

export default function ReaderContextDialog({
  target,
  load,
  save,
  onClose,
}: Readonly<{
  target: ReaderContextTarget;
  load: (target: ReaderContextTarget) => Promise<ReaderContextProjection>;
  save: (target: ReaderContextTarget, text: string) => Promise<void>;
  onClose: () => void;
}>) {
  const { t } = useLocale();
  const containerRef = useRef<HTMLDivElement>(null);
  const textRef = useRef<HTMLTextAreaElement>(null);
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useDialogFocusTrap({ open: true, containerRef, onClose, initialFocusRef: textRef });

  useEffect(() => {
    let cancelled = false;
    setBusy(true);
    setError(null);
    void load(target)
      .then((projection) => {
        if (!cancelled) setText(projection.text);
      })
      .catch((caught) => {
        if (!cancelled) setError(String(caught));
      })
      .finally(() => {
        if (!cancelled) setBusy(false);
      });
    return () => {
      cancelled = true;
    };
  }, [load, target]);

  const charCount = [...text].length;
  const warn = charCount >= 2000;
  const preview = text.trim();

  const submit = async () => {
    setSaving(true);
    setError(null);
    try {
      await save(target, text);
      onClose();
    } catch (caught) {
      setError(String(caught));
      setSaving(false);
    }
  };

  return (
    <div
      className="reader-context-scrim"
      role="presentation"
      onClick={(event) => {
        if (event.target === event.currentTarget && !saving && !busy) onClose();
      }}
    >
      <div
        ref={containerRef}
        className="reader-context-card"
        role="dialog"
        aria-modal="true"
        aria-labelledby="reader-context-title"
      >
        <header className="reader-context-header">
          <h3 id="reader-context-title">{titleFor(target.scope, t)}</h3>
          <p>{t("reader.context.subtitle")}</p>
        </header>
        <div className="reader-context-split">
          <section className="reader-context-pane" aria-label={t("reader.context.edit")}>
            <span className="reader-context-pane-label">{t("reader.context.edit")}</span>
            <textarea
              ref={textRef}
              className="reader-context-textarea"
              value={text}
              disabled={busy || saving}
              placeholder={t("reader.context.placeholder")}
              onChange={(event) => setText(event.target.value)}
            />
          </section>
          <section className="reader-context-pane" aria-label={t("reader.context.preview")}>
            <span className="reader-context-pane-label">{t("reader.context.preview")}</span>
            <div className="reader-context-preview">
              {preview ? (
                <MarkdownBody>{text}</MarkdownBody>
              ) : (
                <p className="reader-context-preview-empty">{t("reader.context.previewEmpty")}</p>
              )}
            </div>
          </section>
        </div>
        <div className={`reader-context-meta ${warn ? "warn" : ""}`}>
          {t("reader.context.charCount", { count: charCount })}
          {warn ? t("reader.context.warn") : null}
        </div>
        {error ? <p className="reader-context-error">{error}</p> : null}
        <footer className="reader-context-actions">
          <button type="button" className="btn-liquid-pill" disabled={saving} onClick={onClose}>
            {t("reader.cancel")}
          </button>
          <button
            type="button"
            className="btn-liquid-pill primary"
            disabled={busy || saving}
            onClick={() => void submit()}
          >
            {saving ? t("reader.saving") : t("reader.save")}
          </button>
        </footer>
      </div>
    </div>
  );
}
