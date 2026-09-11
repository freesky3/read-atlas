import { useMemo, useRef, useState } from "react";
import { useDialogFocusTrap } from "../useDialogFocusTrap";
import { useLocale } from "../i18n/LocaleContext";
import type { LifecyclePatch } from "../library/libraryActTypes";
import type { ReadingLifecycleStatus } from "../readerReadingState";

const STATUSES: ReadingLifecycleStatus[] = ["unread", "reading", "read"];

export default function HubLifecycleDialog({
  count,
  initial,
  onConfirm,
  onClose,
}: Readonly<{
  count: number;
  initial?: LifecyclePatch;
  onConfirm: (patch: LifecyclePatch) => void | Promise<void>;
  onClose: () => void;
}>) {
  const { t } = useLocale();
  const containerRef = useRef<HTMLDivElement>(null);
  const [status, setStatus] = useState<ReadingLifecycleStatus | "">(initial?.status ?? "");
  const [favorite, setFavorite] = useState<boolean | null>(initial?.favorite ?? null);
  const [readLater, setReadLater] = useState<boolean | null>(initial?.readLater ?? null);
  const [priority, setPriority] = useState<number | "">(initial?.priority ?? "");
  const [reviewAt, setReviewAt] = useState(initial?.reviewAt ?? "");
  const [clearReview, setClearReview] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useDialogFocusTrap({ open: true, containerRef, onClose });

  const patch = useMemo((): LifecyclePatch => ({
    ...(status ? { status } : {}),
    ...(favorite !== null ? { favorite } : {}),
    ...(readLater !== null ? { readLater } : {}),
    ...(priority !== "" ? { priority: Number(priority) } : {}),
    ...(clearReview ? { reviewAt: null } : reviewAt.trim() ? { reviewAt: new Date(reviewAt).toISOString() } : {}),
  }), [clearReview, favorite, priority, readLater, reviewAt, status]);

  const submit = async () => {
    if (Object.keys(patch).length === 0) {
      setError(t("hub.lifecycle.needChange"));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await onConfirm(patch);
    } catch (caught) {
      setError(String(caught));
      setBusy(false);
    }
  };

  return (
    <div className="scrim hub-move-scrim" role="presentation" onClick={(event) => { if (event.target === event.currentTarget && !busy) onClose(); }}>
      <div ref={containerRef} className="hub-move-card" role="dialog" aria-modal="true" aria-labelledby="hub-lifecycle-title">
        <header className="hub-move-header">
          <h3 id="hub-lifecycle-title">{t("hub.lifecycle.title")}</h3>
          <p>{t("hub.lifecycle.body", { count })}</p>
        </header>
        <fieldset className="hub-lifecycle-fieldset">
          <legend>{t("hub.lifecycle.statusLegend")}</legend>
          <div className="hub-lifecycle-row">
            {STATUSES.map((value) => (
              <button
                key={value}
                type="button"
                className={`hub-tag-chip ${status === value ? "active" : ""}`}
                aria-pressed={status === value}
                onClick={() => setStatus((current) => (current === value ? "" : value))}
              >
                {t(`hub.status.${value}`)}
              </button>
            ))}
          </div>
        </fieldset>
        <div className="hub-lifecycle-row">
          <button type="button" className={`hub-tag-chip ${favorite === true ? "active" : ""}`} aria-pressed={favorite === true} onClick={() => setFavorite((current) => (current === true ? null : true))}>{t("hub.favorite")}</button>
          <button type="button" className={`hub-tag-chip ${favorite === false ? "active" : ""}`} aria-pressed={favorite === false} onClick={() => setFavorite((current) => (current === false ? null : false))}>{t("hub.lifecycle.unfavorite")}</button>
          <button type="button" className={`hub-tag-chip ${readLater === true ? "active" : ""}`} aria-pressed={readLater === true} onClick={() => setReadLater((current) => (current === true ? null : true))}>{t("hub.lifecycle.readLater")}</button>
          <button type="button" className={`hub-tag-chip ${readLater === false ? "active" : ""}`} aria-pressed={readLater === false} onClick={() => setReadLater((current) => (current === false ? null : false))}>{t("hub.lifecycle.clearReadLater")}</button>
        </div>
        <label className="hub-tag-label" htmlFor="hub-lifecycle-priority">{t("hub.lifecycle.priority")}</label>
        <select id="hub-lifecycle-priority" className="hub-move-search" value={priority} onChange={(event) => setPriority(event.target.value === "" ? "" : Number(event.target.value))}>
          <option value="">{t("hub.lifecycle.unchanged")}</option>
          <option value={0}>0</option>
          <option value={1}>1</option>
          <option value={2}>2</option>
          <option value={3}>3</option>
        </select>
        <label className="hub-tag-label" htmlFor="hub-lifecycle-review">{t("hub.lifecycle.reviewDate")}</label>
        <input
          id="hub-lifecycle-review"
          type="date"
          className="hub-move-search"
          value={reviewAt.slice(0, 10)}
          disabled={clearReview}
          onChange={(event) => setReviewAt(event.target.value)}
        />
        <label className="hub-lifecycle-check">
          <input type="checkbox" checked={clearReview} onChange={(event) => setClearReview(event.target.checked)} />
          {t("hub.lifecycle.clearReview")}
        </label>
        {error ? <p className="hub-move-error">{error}</p> : null}
        <footer className="hub-move-actions">
          <button type="button" className="btn-liquid-pill" onClick={onClose} disabled={busy}>{t("hub.cancel")}</button>
          <button type="button" className="btn-liquid-pill primary" onClick={() => void submit()} disabled={busy}>{t("hub.lifecycle.apply")}</button>
        </footer>
      </div>
    </div>
  );
}
