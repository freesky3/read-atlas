import { useRef } from "react";
import type { OutlinePlan } from "../types";
import { useLocale } from "../i18n/LocaleContext";
import { useDialogFocusTrap } from "../useDialogFocusTrap";
import { formatOutlinePlanCard } from "./outlinePlan";

export default function OutlinePlanConfirmation({ plan, nodeTitle, busy, onConfirm, onClose }: {
  plan: OutlinePlan; nodeTitle?: string; busy?: boolean; onConfirm: () => void; onClose: () => void;
}) {
  const { t } = useLocale();
  const ref = useRef<HTMLDivElement>(null);
  const close = () => { if (!busy) onClose(); };
  useDialogFocusTrap({ open: true, containerRef: ref, onClose: close });
  const card = formatOutlinePlanCard(plan, t);
  return <div ref={ref} className="scrim outline-delete-scrim" role="dialog" aria-modal="true" aria-labelledby="outline-plan-confirm-title" onClick={event => { if (event.target === event.currentTarget) close(); }}>
    <section className="outline-delete-dialog outline-plan-card">
      <h2 id="outline-plan-confirm-title">{card.title}</h2>
      {nodeTitle ? <p>{t("outline.plan.localScope", { title: nodeTitle })}</p> : null}
      <dl><div><dt>{t("outline.plan.model")}</dt><dd>{card.model}</dd></div><div><dt>{t("outline.plan.calls")}</dt><dd>{card.calls}</dd></div>
        <div><dt>{t("outline.plan.estimatedCost")}</dt><dd>{card.estimatedCost}</dd></div><div><dt>{t("outline.plan.source")}</dt><dd>{t("outline.plan.sourceValue", { pages: card.pages, ocr: card.ocrRevisionId })}</dd></div></dl>
      <p>{t("outline.plan.confirmHint")}</p>
      <div className="outline-delete-actions"><button type="button" className="liquid-tab-btn" disabled={busy} onClick={close}>{t("outline.plan.cancel")}</button>
        <button type="button" className="btn-liquid-pill primary" disabled={busy} onClick={onConfirm}>{busy ? t("outline.plan.enqueueing") : t("outline.plan.confirmGenerate")}</button></div>
    </section>
  </div>;
}
