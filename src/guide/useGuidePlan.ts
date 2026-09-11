import { uiText, zhT, type TranslateFn } from "../i18n/uiText";
import { useEffect, useRef, useState } from "react";
import { desktopClient } from "../desktopClient";
import type { GuideCharacterSettings, GuidePlan, GuideProjection } from "../types";

/** A generation panel owns its draft. Only the backend remembers an enqueued cast. */
export function useGuidePlan(
  documentId: string | null,
  revisionId: string | null,
  onProjection: (projection: GuideProjection) => void,
  onStatus: (message: string) => void,
  t: TranslateFn = zhT,
) {
  const [plan, setPlan] = useState<GuidePlan | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [planning, setPlanning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [settings, setSettings] = useState<GuideCharacterSettings | null>(null);
  const [castIds, setCastIds] = useState<string[]>([]);
  const sequence = useRef(0);
  const scope = `${documentId ?? ""}:${revisionId ?? ""}`;
  const currentScope = useRef(scope);
  currentScope.current = scope;
  const callbacks = useRef({ onProjection, onStatus });
  callbacks.current = { onProjection, onStatus };

  useEffect(() => {
    sequence.current += 1;
    setPlan(null);
    setConfirming(false);
    setPlanning(false);
    setError(null);
    setSettings(null);
    setCastIds([]);
    return () => { sequence.current += 1; };
  }, [scope]);

  function cancel() {
    sequence.current += 1;
    setConfirming(false);
    setPlanning(false);
    setPlan(null);
    setError(null);
    setCastIds([]);
  }

  async function buildPlan(ids: string[], request: number, requestScope: string) {
    const next = await desktopClient.command<GuidePlan>("plan_reading_guide", {
      request: { revisionId, characterIds: ids },
    });
    if (sequence.current !== request || currentScope.current !== requestScope) return;
    setPlan(next);
    setPlanning(false);
    setConfirming(true);
  }

  async function open() {
    if (!revisionId) return;
    const request = ++sequence.current;
    const requestScope = scope;
    setPlanning(true);
    setError(null);
    try {
      const [projection, characters] = await Promise.all([
        desktopClient.open<GuideProjection>("get_reading_guide", { revisionId }),
        desktopClient.open<GuideCharacterSettings>("get_guide_character_settings"),
      ]);
      if (sequence.current !== request || currentScope.current !== requestScope) return;
      callbacks.current.onProjection(projection);
      if (projection.status === "missing_ocr" || projection.status === "generating" || projection.activeJobId) {
        callbacks.current.onStatus(projection.status === "missing_ocr"
          ? uiText(t, "Complete OCR before generating margin notes") : uiText(t, "Margin notes are generating; cancel them in the job center"));
        setPlanning(false);
        setConfirming(false);
        return;
      }
      const ids = projection.preferredCharacterIds ?? characters.defaultCharacterIds;
      setSettings(characters);
      setCastIds(ids);
      setConfirming(true);
      await buildPlan(ids, request, requestScope);
    } catch (reason) {
      if (sequence.current !== request || currentScope.current !== requestScope) return;
      setError(String(reason));
      setPlanning(false);
      callbacks.current.onStatus(uiText(t, "Margin-note planning failed · {error}", { error: String(reason) }));
    }
  }

  async function changeCast(ids: string[]) {
    const request = ++sequence.current;
    setCastIds(ids);
    setPlanning(true);
    setError(null);
    try {
      await buildPlan(ids, request, scope);
    } catch (reason) {
      if (sequence.current !== request || currentScope.current !== scope) return;
      setError(String(reason));
      setPlanning(false);
    }
  }

  // Returning from role settings refreshes the frozen plan without resetting this draft.
  async function refreshCharacters() {
    if (!confirming) return;
    const request = ++sequence.current;
    setPlanning(true);
    setError(null);
    try {
      const characters = await desktopClient.open<GuideCharacterSettings>("get_guide_character_settings");
      if (sequence.current !== request || currentScope.current !== scope) return;
      setSettings(characters);
      await buildPlan(castIds, request, scope);
    } catch (reason) {
      if (sequence.current !== request || currentScope.current !== scope) return;
      setError(String(reason));
      setPlanning(false);
    }
  }

  const invalidCast = !castIds.length || castIds.some((id) => !settings?.characters.some((item) => item.id === id));
  const canStart = !planning && !error && !!plan && plan.revisionId === revisionId
    && (plan.guideProtocol === "reading-guide-desktop-v1" || !invalidCast);
  return { plan, confirming, planning, error, settings, castIds, canStart,
    open, changeCast, refreshCharacters, cancel, setError, isCurrent: () => currentScope.current === scope };
}
