import { useMemo, useRef, useState } from "react";
import { useDialogFocusTrap } from "../useDialogFocusTrap";
import { useLocale } from "../i18n/LocaleContext";

export default function HubTagPatchDialog({
  count,
  knownTags,
  onConfirm,
  onClose,
}: Readonly<{
  count: number;
  knownTags: readonly string[];
  onConfirm: (patch: { add: string[]; remove: string[] }) => void | Promise<void>;
  onClose: () => void;
}>) {
  const { t } = useLocale();
  const containerRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const [addInput, setAddInput] = useState("");
  const [remove, setRemove] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useDialogFocusTrap({ open: true, containerRef, onClose, initialFocusRef: inputRef });

  const add = useMemo(
    () => [...new Set(addInput.split(/[,，]/).map((tag) => tag.trim()).filter(Boolean))],
    [addInput],
  );

  const submit = async () => {
    if (add.length === 0 && remove.size === 0) {
      setError(t("hub.tag.needAction"));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await onConfirm({ add, remove: [...remove] });
    } catch (caught) {
      setError(String(caught));
      setBusy(false);
    }
  };

  return (
    <div className="scrim hub-move-scrim" role="presentation" onClick={(event) => { if (event.target === event.currentTarget && !busy) onClose(); }}>
      <div ref={containerRef} className="hub-move-card" role="dialog" aria-modal="true" aria-labelledby="hub-tag-title">
        <header className="hub-move-header">
          <h3 id="hub-tag-title">{t("hub.tag.title")}</h3>
          <p>{t("hub.tag.body", { count })}</p>
        </header>
        <label className="hub-tag-label" htmlFor="hub-tag-add">{t("hub.tag.addLabel")}</label>
        <input
          ref={inputRef}
          id="hub-tag-add"
          type="text"
          className="hub-move-search"
          value={addInput}
          onChange={(event) => setAddInput(event.target.value)}
          placeholder={t("hub.tag.addPlaceholder")}
        />
        {knownTags.length > 0 ? (
          <div className="hub-tag-known">
            <span>{t("hub.tag.removeFrom")}</span>
            <div className="hub-tag-chips">
              {knownTags.map((tag) => {
                const active = remove.has(tag);
                return (
                  <button
                    key={tag}
                    type="button"
                    className={`hub-tag-chip ${active ? "active" : ""}`}
                    aria-pressed={active}
                    onClick={() => {
                      setRemove((current) => {
                        const next = new Set(current);
                        if (next.has(tag)) next.delete(tag);
                        else next.add(tag);
                        return next;
                      });
                    }}
                  >
                    {active ? `− ${tag}` : tag}
                  </button>
                );
              })}
            </div>
          </div>
        ) : <p className="hub-move-empty">{t("hub.tag.empty")}</p>}
        {error ? <p className="hub-inline-error-tip" role="alert">{error}</p> : null}
        <footer className="hub-move-footer">
          <button type="button" className="btn-liquid-pill" onClick={onClose} disabled={busy}>{t("hub.cancel")}</button>
          <button type="button" className="btn-liquid-pill primary" onClick={() => void submit()} disabled={busy}>
            {busy ? t("hub.tag.updating") : t("hub.tag.apply")}
          </button>
        </footer>
      </div>
    </div>
  );
}
