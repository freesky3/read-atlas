import { useEffect, useRef, useState } from "react";
import { Bookmark, Highlighter, Pencil, X } from "lucide-react";
import MarkdownBody from "../MarkdownBody";
import type { ReaderAnnotationKind } from "./ReaderAnnotationsPanel";
import { useLocale, type TranslateFn } from "../i18n/LocaleContext";


export type ReaderAnnotationComposerProps = {
  kind: Exclude<ReaderAnnotationKind, "highlight"> | "highlight";
  pageNumber: number;
  excerpt?: string | null;
  initialTitle?: string;
  initialBody?: string;
  busy?: boolean;
  error?: string | null;
  onSubmit: (value: { title: string; body: string }) => void;
  onCancel: () => void;
};

function kindText(kind: ReaderAnnotationKind, t: TranslateFn) {
  return t(`reader.kind.${kind}`);
}

function useDebouncedValue(value: string, delay: number) {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(value), delay);
    return () => window.clearTimeout(timer);
  }, [delay, value]);
  return debounced;
}

export function ReaderAnnotationComposer({
  kind,
  pageNumber,
  excerpt,
  initialTitle = "",
  initialBody = "",
  busy = false,
  error,
  onSubmit,
  onCancel,
}: ReaderAnnotationComposerProps) {
  const { t } = useLocale();
  const [title, setTitle] = useState(initialTitle);
  const [body, setBody] = useState(initialBody);
  const bodyRef = useRef<HTMLTextAreaElement>(null);
  const titleRef = useRef<HTMLInputElement>(null);
  const preview = useDebouncedValue(body, 150);

  useEffect(() => {
    if (kind === "note") bodyRef.current?.focus();
    else titleRef.current?.focus();
  }, [kind]);

  return (
    <div className="reader-annotation-composer-scrim" role="presentation">
      <section
        className={`reader-annotation-composer${kind === "note" ? " is-split" : ""}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby="reader-annotation-composer-title"
      >
        <header>
          <div className="reader-annotation-composer-heading">
            {kind === "bookmark" ? (
              <Bookmark size={17} aria-hidden="true" />
            ) : kind === "note" ? (
              <Pencil size={17} aria-hidden="true" />
            ) : (
              <Highlighter size={17} aria-hidden="true" />
            )}
            <small id="reader-annotation-composer-title">
              {t("reader.composer.heading", { kind: kindText(kind, t), page: pageNumber })}
            </small>
          </div>
          <button
            type="button"
            className="reader-annotation-composer-close"
            onClick={onCancel}
            aria-label={t("reader.cancel")}
            title={t("reader.cancel")}
          >
            <X size={16} aria-hidden="true" />
          </button>
        </header>
        {excerpt ? (
          <blockquote className="reader-annotation-composer-excerpt">
            <MarkdownBody>{excerpt}</MarkdownBody>
          </blockquote>
        ) : null}
        <label className="reader-annotation-composer-title-field">
          <span className="visually-hidden">{t("reader.composer.optionalTitle")}</span>
          <input
            ref={titleRef}
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            placeholder={t("reader.composer.optionalTitle")}
            maxLength={160}
            aria-label={t("reader.composer.optionalTitle")}
          />
        </label>
        {kind === "note" ? (
          <div className="reader-annotation-composer-split">
            <textarea
              ref={bodyRef}
              value={body}
              onChange={(event) => setBody(event.target.value)}
              placeholder={t("reader.composer.notePlaceholder")}
              maxLength={12000}
              aria-label={t("reader.kind.note")}
            />
            <div className="reader-annotation-composer-preview" aria-live="polite">
              {preview.trim() ? (
                <MarkdownBody>{preview}</MarkdownBody>
              ) : (
                <p className="reader-annotation-composer-preview-empty">{t("reader.composer.previewEmpty")}</p>
              )}
            </div>
          </div>
        ) : null}
        {error ? (
          <p className="reader-annotation-composer-error" role="alert">
            {error}
          </p>
        ) : null}
        <footer>
          <button type="button" onClick={onCancel} disabled={busy}>
            {t("reader.cancel")}
          </button>
          <button
            type="button"
            className="primary"
            disabled={busy || (kind === "note" && !body.trim())}
            onClick={() => onSubmit({ title: title.trim(), body: body.trim() })}
          >
            {busy ? t("reader.saving") : t("reader.save")}
          </button>
        </footer>
      </section>
    </div>
  );
}

export default ReaderAnnotationComposer;
