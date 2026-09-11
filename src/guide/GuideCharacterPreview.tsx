import { useEffect, useRef, useState } from "react";
import { desktopClient } from "../desktopClient";
import { useLocale } from "../i18n/LocaleContext";
import MarkdownBody from "../MarkdownBody";
import type { GuideCastSnapshot, GuideCharacter, GuideInk, UsageEnvelope } from "../types";

export default function GuideCharacterPreview({ draft, characters, onAdopt }: {
  draft: GuideCharacter;
  characters: GuideCharacter[];
  onAdopt: (body: string) => void;
}) {
  const { t } = useLocale();
  const [comparison, setComparison] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [result, setResult] = useState<{
    inks: GuideInk[]; usage?: UsageEnvelope; castSnapshot?: GuideCastSnapshot; warnings?: string[];
  } | null>(null);
  const [frozenDrafts, setFrozenDrafts] = useState<GuideCharacter[]>([]);
  const pendingId = useRef<string | null>(null);
  const live = useRef(true);
  useEffect(() => {
    live.current = true;
    return () => { live.current = false; };
  }, []);

  async function preview() {
    if (pendingId.current) return;
    const requestId = crypto.randomUUID();
    const drafts = [{ ...draft, id: draft.id || "preview:1" },
      ...characters.filter((item) => comparison.includes(item.id) && item.id !== draft.id)];
    pendingId.current = requestId;
    setFrozenDrafts(drafts);
    setBusy(true);
    setError(null);
    setResult(null);
    setStatus(t("guide.preview.statusWriting"));
    try {
      const response = await desktopClient.command<NonNullable<typeof result>>("preview_guide_character", {
        request: { requestId, drafts, documentKind: "textbook" },
      });
      if (!live.current || pendingId.current !== requestId) return;
      setResult(response);
      setStatus(response.inks.some((ink) => ink.kind === "note") ? t("guide.preview.statusDone") : t("guide.preview.statusEmpty"));
    } catch (reason) {
      if (live.current && pendingId.current === requestId) {
        setError(String(reason));
        setStatus(null);
      }
    } finally {
      if (live.current && pendingId.current === requestId) {
        pendingId.current = null;
        setBusy(false);
      }
    }
  }

  async function cancel() {
    const requestId = pendingId.current;
    if (!requestId) return;
    try {
      await desktopClient.command("cancel_guide_character_preview", { request: { requestId } });
      if (!live.current || pendingId.current !== requestId) return;
      pendingId.current = null;
      setBusy(false);
      setStatus(t("guide.preview.statusCancelled"));
    } catch (reason) {
      if (live.current) setError(t("guide.preview.cancelFailed", { reason: String(reason) }));
    }
  }

  return <section className="dossier-section-card">
    <h3>{t("guide.preview.title")}</h3>
    <p>{t("guide.preview.body")}</p>
    <fieldset disabled={busy}>
      <legend>{t("guide.preview.compareLegend")}</legend>
      {characters.filter((item) => item.id !== draft.id).map((item) => (
        <label key={item.id} className="guide-cast-chip">
          <input type="checkbox" checked={comparison.includes(item.id)}
            disabled={!comparison.includes(item.id) && comparison.length >= 2}
            onChange={() => setComparison((ids) => ids.includes(item.id) ? ids.filter((id) => id !== item.id) : [...ids, item.id])} />
          {item.displayName}
        </label>
      ))}
    </fieldset>
    <button type="button" className="btn-liquid-pill" disabled={busy || !draft.displayName.trim()} onClick={() => void preview()}>{t("guide.preview.tryWrite")}</button>
    {busy ? <button type="button" className="btn-liquid-pill" onClick={() => void cancel()}>{t("guide.preview.cancel")}</button> : null}
    {status ? <p role="status">{status}</p> : null}
    {error ? <p role="alert">{error}</p> : null}
    {result ? <>
      <p>{t("guide.preview.usage", { model: result.usage?.model ?? t("guide.preview.modelMissing"), cost: result.usage?.estimatedCost ?? t("guide.preview.costUnknown") })}
        {result.usage?.inputTokens != null ? t("guide.preview.inputTokens", { n: result.usage.inputTokens }) : ""}
        {result.usage?.outputTokens != null ? t("guide.preview.outputTokens", { n: result.usage.outputTokens }) : ""}</p>
      {result.warnings?.map((warning, index) => <p key={index}>{warning}</p>)}
      {result.inks.filter((ink) => ink.kind !== "trace").map((ink) => {
        const person = result.castSnapshot?.characters.find((item) => item.id === ink.speakerId)
          ?? frozenDrafts.find((item) => item.id === ink.speakerId);
        return <article key={ink.id} className="guide-card" style={{ borderLeftColor: person?.inkColor }}>
          <strong>{person?.displayName ?? ink.speakerId}</strong>
          <MarkdownBody>{ink.body}</MarkdownBody>
          {ink.speakerId === frozenDrafts[0]?.id ? <button type="button" className="btn-liquid-pill" onClick={() => onAdopt(ink.body)}>{t("guide.preview.adopt")}</button> : null}
        </article>;
      })}
    </> : null}
  </section>;
}
