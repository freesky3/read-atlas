import { isOutlineV4Protocol } from "./outlineDisplay";
import type { OutlineHeadProjection } from "../types";
import { useLocale } from "../i18n/LocaleContext";
import type { OutlineJumpTarget } from "./OutlineGraphList";

export default function OutlineReviewStatus({ head, stale, onJump }: {
  head: OutlineHeadProjection; stale?: boolean; onJump?: (target: OutlineJumpTarget) => void;
}) {
  const { t } = useLocale();
  const v4 = isOutlineV4Protocol(head.protocolVersion);
  const gaps = head.gaps?.length ? head.gaps : head.graph?.gaps ?? [];
  const label = !v4 ? t("outline.review.legacy") : head.reviewStatus === "reviewed" ? t("outline.review.reviewed") : head.reviewStatus === "reviewed_with_gaps" ? t("outline.review.reviewedWithGaps") : head.reviewStatus === "self_checked" ? t("outline.review.selfChecked") : t("outline.review.unchecked");
  return <section className="outline-review-status" aria-label={t("outline.review.aria")} style={{ padding: "8px 14px", maxHeight: 180, overflow: "auto" }}>
    <strong>{label}</strong>{stale ? <p>{t("outline.review.stale")}</p> : null}
    {head.coverageWarnings?.length ? <ul>{head.coverageWarnings.map((warning, index) => <li key={index}>{warning}</li>)}</ul> : null}
    {head.reviewNotes?.length ? <details><summary>{t("outline.review.notes")}</summary><ul>{head.reviewNotes.map((note, index) => <li key={index}>{note}</li>)}</ul></details> : null}
    {gaps.length ? <details open><summary>{t("outline.review.gaps", { count: gaps.length })}</summary><ul>{gaps.map((gap, index) => <li key={index}>{gap.description}
      {gap.suggestedPages?.map(pageNumber => <button type="button" key={pageNumber} onClick={() => onJump?.({ pageNumber })}>p.{pageNumber}</button>)}
    </li>)}</ul></details> : null}
  </section>;
}
