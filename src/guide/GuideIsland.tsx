import { useEffect, useRef, useState } from "react";
import { useLocale, type TranslateFn } from "../i18n/LocaleContext";
import type { GuideCharacterSettings, GuidePlan, GuideProjection } from "../types";
import { guideNotes, normalizeGuideInks } from "./inks";
import GuideCastPicker from "./GuideCastPicker";

type GuideIslandProps = {
  projection: GuideProjection | null;
  plan: GuidePlan | null;
  layerVisible: boolean;
  busy: boolean;
  confirming: boolean;
  error: string | null;
  suspended?: boolean;
  planning?: boolean;
  canGenerate?: boolean;
  characterSettings?: GuideCharacterSettings | null;
  selectedCharacterIds?: string[];
  onCastChange?: (ids: string[]) => void;
  onOpenCharacterSettings?: () => void;
  onOpen: () => void;
  onConfirmGenerate: () => void;
  onCancelConfirm: () => void;
  onToggleLayer: () => void;
  onRegenerate: () => void;
  onDelete: () => void;
};

function statusLabel(projection: GuideProjection | null, busy: boolean, t: TranslateFn) {
  if (busy) return t("guide.island.statusGenerating");
  if (!projection || projection.status === "missing_ocr") return t("guide.island.statusDefault");
  if (projection.status === "ready_to_plan") return t("guide.island.statusDefault");
  if (projection.status === "stale") return t("guide.island.statusStale");
  const noteCount = guideNotes(normalizeGuideInks(projection.head?.inks)).length;
  const countLabel = noteCount > 0 ? ` · ${noteCount}` : "";
  if (projection.status === "partial") return t("guide.island.statusPartial", { count: countLabel });
  return t("guide.island.statusReady", { count: countLabel });
}

export default function GuideIsland({
  projection,
  plan,
  layerVisible,
  busy,
  confirming,
  error,
  suspended = false,
  planning = false,
  canGenerate = true,
  characterSettings = null,
  selectedCharacterIds = [],
  onCastChange,
  onOpenCharacterSettings,
  onOpen,
  onConfirmGenerate,
  onCancelConfirm,
  onToggleLayer,
  onRegenerate,
  onDelete,
}: GuideIslandProps) {
  const { t } = useLocale();
  const [menuOpen, setMenuOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const hasHead = Boolean(projection?.head);

  useEffect(() => {
    if (suspended || (!menuOpen && !confirming)) return;
    const handlePointerDown = (event: MouseEvent | TouchEvent) => {
      if (
        containerRef.current &&
        !containerRef.current.contains(event.target as Node)
      ) {
        if (menuOpen) setMenuOpen(false);
        if (confirming) onCancelConfirm();
      }
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        if (menuOpen) setMenuOpen(false);
        if (confirming) onCancelConfirm();
      }
    };
    document.addEventListener("mousedown", handlePointerDown);
    document.addEventListener("touchstart", handlePointerDown);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handlePointerDown);
      document.removeEventListener("touchstart", handlePointerDown);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [menuOpen, confirming, onCancelConfirm, suspended]);

  return (
    <div className="guide-island" ref={containerRef} style={{ position: "relative" }}>
      <div className="guide-island-split">
        <button
          type="button"
          className={`btn-liquid-pill ${layerVisible && hasHead ? "primary" : ""} ${busy ? "is-busy" : ""}`}
          style={{ padding: "2px 8px", fontSize: 11 }}
          aria-pressed={layerVisible && hasHead}
          onClick={() => {
            setMenuOpen(false);
            onOpen();
          }}
          onContextMenu={(event) => {
            event.preventDefault();
            if (hasHead || busy) setMenuOpen((open) => !open);
          }}
          title={
            projection?.status === "missing_ocr"
              ? t("guide.island.titleNeedOcr")
              : hasHead
                ? t("guide.island.titleToggle")
                : t("guide.island.titleGenerate")
          }
        >
          {statusLabel(projection, busy, t)}
        </button>
        {hasHead || busy ? (
          <button
            type="button"
            className={`guide-island-caret ${layerVisible && hasHead ? "is-on" : ""}`}
            aria-label={t("guide.island.menu")}
            aria-haspopup="menu"
            aria-expanded={menuOpen}
            title={t("guide.island.menuTitle")}
            onClick={() => setMenuOpen((open) => !open)}
          >
            <svg
              width="10"
              height="10"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2.5"
              strokeLinecap="round"
              strokeLinejoin="round"
              style={{ display: "block", transition: "transform 0.15s ease", transform: menuOpen ? "rotate(180deg)" : "none" }}
            >
              <polyline points="6 9 12 15 18 9" />
            </svg>
          </button>
        ) : null}
      </div>
      {menuOpen ? (
        <div className="guide-island-menu" role="menu">
          <button
            type="button"
            className="guide-menu-item"
            onClick={() => {
              setMenuOpen(false);
              onRegenerate();
            }}
          >
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              className="guide-menu-icon"
            >
              <polyline points="23 4 23 10 17 10" />
              <polyline points="1 20 1 14 7 14" />
              <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15" />
            </svg>
            <span>{t("guide.island.regenerate")}</span>
          </button>
          {hasHead ? (
            <button
              type="button"
              className="guide-menu-item"
              onClick={() => {
                setMenuOpen(false);
                onToggleLayer();
              }}
            >
              {layerVisible ? (
                <>
                  <svg
                    width="14"
                    height="14"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    className="guide-menu-icon"
                  >
                    <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24" />
                    <line x1="1" y1="1" x2="23" y2="23" />
                  </svg>
                  <span>{t("guide.island.hide")}</span>
                </>
              ) : (
                <>
                  <svg
                    width="14"
                    height="14"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    className="guide-menu-icon"
                  >
                    <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
                    <circle cx="12" cy="12" r="3" />
                  </svg>
                  <span>{t("guide.island.show")}</span>
                </>
              )}
            </button>
          ) : null}
          {hasHead ? (
            <button
              type="button"
              className="guide-menu-item is-danger"
              onClick={() => {
                setMenuOpen(false);
                onDelete();
              }}
            >
              <svg
                width="14"
                height="14"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
                className="guide-menu-icon"
              >
                <polyline points="3 6 5 6 21 6" />
                <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
              </svg>
              <span>{t("guide.island.delete")}</span>
            </button>
          ) : null}
        </div>
      ) : null}
      {!suspended && confirming && (plan || characterSettings) ? (
        <div className="guide-plan-card" role="dialog" aria-label={t("guide.island.confirmAria")} aria-describedby="guide-generation-resource-notice">
          <div className="guide-plan-head">
            <span className="guide-plan-icon">✎</span>
            <strong className="guide-plan-title">{t("guide.island.confirmTitle")}</strong>
          </div>
          <p id="guide-generation-resource-notice" className="guide-plan-description">
            <strong>{t("guide.island.resourceNotice")}</strong>
          </p>
          {plan ? <>
          <div className="guide-plan-meta-chips">
            <span className="guide-plan-chip">
              {t("guide.island.pagesBatches", { pages: plan.pageCount, batches: plan.batchCount })}
            </span>
            <span className="guide-plan-chip">
              {plan.reusedOutline ? t("guide.island.reuseOutline") : t("guide.island.readFull")}
            </span>
            <span className="guide-plan-chip">
              {plan.hasPaperRoot ? t("guide.island.rootReady") : t("guide.island.willCreateRoot")}
            </span>
          </div>
          <p className="guide-plan-description">
            {t("guide.island.planCalls", { calls: plan.understandCalls + plan.annotationCalls + (plan.rootCalls ?? 0), repairs: plan.repairCalls })}
            {plan.estimatedCost ? t("guide.island.estimatedCost", { cost: plan.estimatedCost }) : t("guide.island.costUnknown")}
          </p>
          </> : null}
          {plan?.guideProtocol === "reading-guide-desktop-v1" ? (
            <p className="guide-plan-description">{t("guide.island.legacyProtocol")}</p>
          ) : onCastChange && onOpenCharacterSettings ? (
            <GuideCastPicker
              settings={characterSettings}
              selectedIds={selectedCharacterIds}
              onChange={onCastChange}
              onOpenSettings={onOpenCharacterSettings}
            />
          ) : null}
          {planning ? <p role="status">{t("guide.island.updatingPlan")}</p> : null}
          {error ? <p className="guide-plan-error" role="alert">{error}</p> : null}
          <div className="guide-plan-actions">
            <button
              type="button"
              className="btn-liquid-pill"
              onClick={onCancelConfirm}
            >
              {t("guide.island.cancel")}
            </button>
            <button
              type="button"
              className="btn-liquid-pill primary"
              disabled={planning || !canGenerate}
              onClick={onConfirmGenerate}
            >
              {t("guide.island.confirmGenerate")}
            </button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
