import { runtimeCopy } from "../i18n/runtimeCopy";
import { useRef } from "react";
import { useDialogFocusTrap } from "../useDialogFocusTrap";
import { useLocale, type TranslateFn } from "../i18n/LocaleContext";
import type { BatchProjection, BatchRequirement, CostPreview } from "../library/libraryActTypes";

const KIND_KEY: Record<BatchRequirement["kind"], string> = {
  kind_change: "hub.batch.kind.kind_change",
  destructive: "hub.batch.kind.destructive",
  conflict: "hub.batch.kind.conflict",
  overwrite: "hub.batch.kind.overwrite",
  cost: "hub.batch.kind.cost",
  unknown_cost: "hub.batch.kind.unknown_cost",
  long_pdf: "hub.batch.kind.long_pdf",
  possible_duplicate_charge: "hub.batch.kind.possible_duplicate_charge",
};

const CONFIDENCE_KEY: Record<"exact" | "upper_bound" | "estimate", string> = {
  exact: "hub.batch.confidence.exact",
  upper_bound: "hub.batch.confidence.upper_bound",
  estimate: "hub.batch.confidence.estimate",
};

function costLines(preview: CostPreview, t: TranslateFn): string[] {
  const lines: string[] = [];
  for (const estimate of preview.marginalEstimates) {
    const range = estimate.minimum === estimate.maximum
      ? `${estimate.currency} ${estimate.maximum}`
      : `${estimate.currency} ${estimate.minimum}–${estimate.maximum}`;
    const confidenceKey = CONFIDENCE_KEY[estimate.confidence] ?? CONFIDENCE_KEY.estimate;
    lines.push(t("hub.batch.costLine", { confidence: t(confidenceKey), range, basis: runtimeCopy(t, estimate.basis) }));
  }
  if (preview.unknownItemCount > 0) {
    lines.push(t("hub.batch.unknownCost", { count: preview.unknownItemCount }));
  }
  if (preview.joinedExistingJobCount > 0) {
    lines.push(t("hub.batch.joinedJobs", { count: preview.joinedExistingJobCount }));
  }
  return lines;
}

export default function HubBatchConfirmDialog({
  plan,
  busy,
  onConfirm,
  onClose,
}: Readonly<{
  plan: BatchProjection;
  busy?: boolean;
  onConfirm: (acceptedIds: readonly string[]) => void | Promise<void>;
  onClose: () => void;
}>) {
  const { t } = useLocale();
  const containerRef = useRef<HTMLDivElement>(null);
  useDialogFocusTrap({ open: true, containerRef, onClose });
  const ids = plan.requirements.map((item) => item.id);
  const cost = costLines(plan.costPreview ?? { marginalEstimates: [], unknownItemCount: 0, unknownReasonCodes: [], joinedExistingJobCount: 0 }, t);
  const canStart = ids.length > 0 || cost.length > 0;

  return (
    <div className="scrim hub-move-scrim" role="presentation" onClick={(event) => { if (event.target === event.currentTarget && !busy) onClose(); }}>
      <div ref={containerRef} className="hub-move-card" role="dialog" aria-modal="true" aria-labelledby="hub-batch-confirm-title">
        <header className="hub-move-header">
          <h3 id="hub-batch-confirm-title">{t("hub.batch.title")}</h3>
          <p>{t("hub.batch.body", { count: plan.totalItems })}</p>
        </header>
        {cost.length > 0 ? (
          <ul className="hub-batch-requirements" aria-label={t("hub.batch.costAria")}>
            {cost.map((line) => (
              <li key={line}><strong>{t("hub.batch.cost")}</strong><span>{line}</span></li>
            ))}
          </ul>
        ) : null}
        <ul className="hub-batch-requirements">
          {plan.requirements.map((requirement) => (
            <li key={requirement.id}>
              <strong>{t(KIND_KEY[requirement.kind])}</strong>
              <span>{runtimeCopy(t, requirement.label)}</span>
              <em>{t("hub.batch.itemCount", { count: requirement.itemCount })}</em>
            </li>
          ))}
        </ul>
        <footer className="hub-move-footer">
          <button type="button" className="btn-liquid-pill" onClick={onClose} disabled={busy}>{t("hub.cancel")}</button>
          <button
            type="button"
            className="btn-liquid-pill primary"
            onClick={() => void onConfirm(ids)}
            disabled={busy || !canStart}
          >
            {busy ? t("hub.batch.running") : t("hub.batch.confirm")}
          </button>
        </footer>
      </div>
    </div>
  );
}
