import { lazy, Suspense, useState } from "react";
import type {
  JobProjection,
  OutlineNode,
  OutlineHeadProjection,
  OutlinePlan,
  OutlineProjection,
  WorkspaceLayout,
} from "../types";
import { useLocale } from "../i18n/LocaleContext";
import {
  formatOutlinePlanCard,
  outlineEmptyCopy,
  outlinePaneKind,
  type OutlineEmptyKind,
} from "./outlinePlan";
import { outlineProgressCopy } from "./outlineProgress";
import OutlineGraphList, { type OutlineJumpTarget } from "./OutlineGraphList";
import OutlineReviewStatus from "./OutlineReviewStatus";
import OutlinePlanConfirmation from "./OutlinePlanConfirmation";

const OutlineCanvas = lazy(() => import("./OutlineCanvas"));

type OutlinePaneProps = {
  projection: OutlineProjection | null;
  plan: OutlinePlan | null;
  planError: string | null;
  layout: WorkspaceLayout;
  onOpenDiscussion: () => void;
  onOpenArtifacts?: () => void;
  onUseOutlineOnly: () => void;
  onShowPdf: () => void;
  onGenerate?: () => void;
  generateBusy?: boolean;
  activeJob?: JobProjection | null;
  selectedNodeId?: string | null;
  onSelectNode?: (node: OutlineNode) => void;
  onJumpEvidence?: (target: OutlineJumpTarget) => void;
  onGenerateDeepDive?: (node: OutlineNode) => void;
  deepDiveBusy?: boolean;
  hasDeepDive?: boolean;
  deepDiveGraph?: import("../types").OutlineGraph | null;
  deepDiveHead?: OutlineHeadProjection | null;
  pendingPlan?: { plan: OutlinePlan; nodeTitle?: string } | null;
  onConfirmPlan?: () => void;
  onCancelPlan?: () => void;
  onBackToOverview?: () => void;
  onRegenerateOverview?: () => void;
  onDeleteOverview?: () => void;
  overviewBusy?: boolean;
  onRegenerateDeepDive?: () => void;
  onDeleteDeepDive?: () => void;
  inspectorWidth?: number;
  onInspectorWidthChange?: (width: number) => void;
  onClearSelection?: () => void;
};

const EMPTY_KINDS = new Set<OutlineEmptyKind>([
  "missing_ocr",
  "missing_paper_root",
  "over_window",
  "unsupported_pdf",
]);

export default function OutlinePane({
  projection,
  plan,
  planError,
  layout,
  onOpenDiscussion,
  onOpenArtifacts,
  onUseOutlineOnly,
  onShowPdf,
  onGenerate,
  generateBusy = false,
  activeJob = null,
  selectedNodeId = null,
  onSelectNode,
  onJumpEvidence,
  onGenerateDeepDive,
  deepDiveBusy = false,
  hasDeepDive = false,
  deepDiveGraph = null,
  deepDiveHead = null,
  pendingPlan = null,
  onConfirmPlan,
  onCancelPlan,
  onBackToOverview,
  onRegenerateOverview,
  onDeleteOverview,
  overviewBusy = false,
  onRegenerateDeepDive,
  onDeleteDeepDive,
  inspectorWidth,
  onInspectorWidthChange,
  onClearSelection,
}: OutlinePaneProps) {
  const { t } = useLocale();
  const kind = outlinePaneKind({ projection, plan, planError, activeJob });
  const isEmpty = EMPTY_KINDS.has(kind as OutlineEmptyKind);
  const [confirmDelete, setConfirmDelete] = useState<
    null | "overview" | "deep_dive"
  >(null);
  const [visibleAttemptId, setVisibleAttemptId] = useState<string | null>(null);
  const attempt = projection?.latestAttempt;
  const showMapActions =
    (kind === "published" || kind === "stale") &&
    Boolean(projection?.head?.graph);
  const progress = activeJob ? outlineProgressCopy(activeJob, t) : null;

  return (
    <aside className="chat-panel panel outline-pane" data-outline-kind={kind}>
      <div className="compact-rail-header">
        <div className="compact-rail-left">
          <strong className="outline-pane-title">
            {deepDiveGraph ? t("outline.pane.localMap") : t("outline.pane.argumentMap")}
          </strong>
          {deepDiveGraph && onBackToOverview ? (
            <button
              type="button"
              className="liquid-tab-btn"
              onClick={onBackToOverview}
            >
              {t("outline.pane.backToOverview")}
            </button>
          ) : null}
        </div>
        <div className="outline-pane-actions">
          {showMapActions && !deepDiveGraph ? (
            <>
              <button
                type="button"
                className="liquid-tab-btn"
                disabled={overviewBusy || !onRegenerateOverview}
                title={
                  overviewBusy
                    ? t("outline.pane.busyCancelInOps")
                    : t("outline.pane.regenerateOverviewTitle")
                }
                onClick={onRegenerateOverview}
              >
                {t("outline.pane.regenerate")}
              </button>
              <button
                type="button"
                className="liquid-tab-btn"
                disabled={overviewBusy || !onDeleteOverview}
                title={
                  overviewBusy
                    ? t("outline.pane.busyCancelInOps")
                    : t("outline.pane.deleteOverviewTitle")
                }
                onClick={() => setConfirmDelete("overview")}
              >
                {t("outline.pane.delete")}
              </button>
            </>
          ) : null}
          {showMapActions && deepDiveGraph ? (
            <>
              <button
                type="button"
                className="liquid-tab-btn"
                disabled={deepDiveBusy || !onRegenerateDeepDive}
                title={
                  deepDiveBusy
                    ? t("outline.pane.busyCancelInOps")
                    : t("outline.pane.regenerateLocalTitle")
                }
                onClick={onRegenerateDeepDive}
              >
                {t("outline.pane.regenerate")}
              </button>
              <button
                type="button"
                className="liquid-tab-btn"
                disabled={deepDiveBusy || !onDeleteDeepDive}
                title={
                  deepDiveBusy
                    ? t("outline.pane.busyCancelInOps")
                    : t("outline.pane.deleteLocalTitle")
                }
                onClick={() => setConfirmDelete("deep_dive")}
              >
                {t("outline.pane.delete")}
              </button>
            </>
          ) : null}
          {onOpenArtifacts ? (
            <button
              type="button"
              className="liquid-tab-btn"
              onClick={onOpenArtifacts}
              title={t("outline.pane.backToArtifactsTitle")}
            >
              {t("outline.pane.backToArtifacts")}
            </button>
          ) : null}
          <button
            type="button"
            className="liquid-tab-btn"
            onClick={onOpenDiscussion}
            title={t("outline.pane.backToDiscussionTitle")}
          >
            {t("outline.pane.backToDiscussion")}
          </button>
          {layout === "outline_only" ? (
            <button type="button" className="liquid-tab-btn" onClick={onShowPdf}>
              {t("outline.pane.showPdf")}
            </button>
          ) : (
            <button
              type="button"
              className="liquid-tab-btn"
              onClick={onUseOutlineOnly}
            >
              {t("outline.pane.outlineOnly")}
            </button>
          )}
        </div>
      </div>

      {planError ? <p className="outline-progress-hint" role="alert">{planError}</p> : null}
      {isEmpty ? (
        <section className="outline-empty" role="status">
          <h2>{outlineEmptyCopy(kind as OutlineEmptyKind, t).title}</h2>
          <p>{outlineEmptyCopy(kind as OutlineEmptyKind, t).body}</p>
        </section>
      ) : kind === "plan" && plan ? (
        <section className="outline-plan-card">
          <h2>{formatOutlinePlanCard(plan, t).title}</h2>
          <dl>
            <div>
              <dt>{t("outline.plan.model")}</dt>
              <dd>{formatOutlinePlanCard(plan, t).model}</dd>
            </div>
            <div>
              <dt>{t("outline.plan.ocr")}</dt>
              <dd>{formatOutlinePlanCard(plan, t).ocrRevisionId}</dd>
            </div>
            <div>
              <dt>{t("outline.plan.calls")}</dt>
              <dd>{formatOutlinePlanCard(plan, t).calls}</dd>
            </div>
            <div>
              <dt>{t("outline.plan.cost")}</dt>
              <dd>{formatOutlinePlanCard(plan, t).estimatedCost}</dd>
            </div>
          </dl>
          <button
            type="button"
            className="btn-liquid-pill primary"
            disabled={!onGenerate || generateBusy}
            title={onGenerate ? t("outline.plan.generateTitle") : t("outline.plan.waitForPlan")}
            onClick={onGenerate}
          >
            {generateBusy ? t("outline.plan.queuing") : t("outline.plan.generateMap")}
          </button>
        </section>
      ) : kind === "generating" ? (
        <section className="outline-progress-card" role="status">
          <h2>{progress ? progress.title : t("outline.progress.generatingTitle")}</h2>
          <p className="outline-progress-stage">
            {progress
              ? `${progress.stage} · ${progress.step}`
              : t("outline.progress.queuedWaiting")}
          </p>
          <p className="outline-progress-tokens">
            {progress
              ? progress.tokens
              : t("outline.progress.noTokenReceipt")}
          </p>
          {progress?.units ? (
            <p>{progress.units}</p>
          ) : null}
          <p className="outline-progress-hint">
            {t("outline.progress.backgroundHint")}
          </p>
        </section>
      ) : kind === "partial" ? (
        <section className="outline-empty" role="status">
          <h2>{t("outline.partial.title")}</h2>
          <p>{t("outline.partial.body")}</p>
          {onGenerate ? <button type="button" className="btn-liquid-pill" disabled={generateBusy} onClick={onGenerate}>
            {t("outline.partial.replan")}
          </button> : null}
        </section>
      ) : (kind === "published" || kind === "stale") &&
        projection?.head?.graph ? (
        <>
        {activeJob &&
        activeJob.kind === "outline_overview" &&
        (activeJob.state === "queued" || activeJob.state === "running") ? (
          <section className="outline-progress-banner" role="status">
            <p className="outline-progress-stage">
              {`${progress?.stage} · ${progress?.step}`}
            </p>
            <p className="outline-progress-tokens">
              {progress?.tokens}
            </p>
            <p className="outline-progress-hint">
              {t("outline.progress.backgroundHint")}
            </p>
          </section>
        ) : null}
        <OutlineReviewStatus head={deepDiveGraph && deepDiveHead ? deepDiveHead : projection.head} stale={kind === "stale"} onJump={onJumpEvidence} />
        <Suspense fallback={<p className="outline-progress-hint">{t("outline.progress.loadingCanvas")}</p>}>
          <OutlineCanvas
            key={deepDiveGraph ? deepDiveHead?.id ?? "deep-dive" : projection.head.id}
            canvasKey={deepDiveGraph ? deepDiveHead?.id ?? "deep-dive" : projection.head.id}
            graph={deepDiveGraph ?? projection.head.graph}
            catalog={projection.catalog?.entries ?? []}
            selectedNodeId={selectedNodeId}
            onSelectNode={onSelectNode ?? (() => undefined)}
            onJumpEvidence={onJumpEvidence ?? (() => undefined)}
            inspectorWidth={inspectorWidth}
            onInspectorWidthChange={onInspectorWidthChange}
            onClearSelection={onClearSelection}
            onGenerateDeepDive={deepDiveGraph ? undefined : onGenerateDeepDive}
            deepDiveBusy={deepDiveBusy}
            hasDeepDive={hasDeepDive}
          />
        </Suspense>
        </>
      ) : kind === "published" || kind === "stale" ? (
        <section className="outline-empty" role="status">
          <h2>{kind === "stale" ? t("outline.published.staleTitle") : t("outline.published.publishedTitle")}</h2>
          <p>{t("outline.published.noGraph")}</p>
        </section>
      ) : (
        <section className="outline-empty" role="status">
          <h2>{t("outline.plan.fallbackTitle")}</h2>
          <p>{t("outline.plan.fallbackBody")}</p>
          <button
            type="button"
            className="btn-liquid-pill primary"
            disabled
            title={onGenerate ? t("outline.plan.generateTitle") : t("outline.plan.waitForPlan")}
            onClick={onGenerate}
          >
            {generateBusy ? t("outline.plan.queuing") : t("outline.plan.generateMap")}
          </button>
        </section>
      )}
      {attempt?.graph && attempt.id !== projection?.head?.id ? <section className="outline-candidate" style={{ padding: 12, maxHeight: "45%", overflow: "auto" }}>
        <p>{projection?.head?.graph ? t("outline.candidate.keptWithPublished") : t("outline.candidate.keptWithoutPublished")}</p>
        <button type="button" className="liquid-tab-btn" onClick={() => setVisibleAttemptId(value => value === attempt.id ? null : attempt.id)}>
          {visibleAttemptId === attempt.id ? t("outline.candidate.collapse") : t("outline.candidate.view")}
        </button>
        {visibleAttemptId === attempt.id ? <><OutlineReviewStatus head={attempt} onJump={onJumpEvidence} />
          <OutlineGraphList graph={attempt.graph} catalog={projection?.catalog?.entries ?? []} onJump={onJumpEvidence ?? (() => undefined)} /></> : null}
      </section> : null}
      {pendingPlan ? <OutlinePlanConfirmation plan={pendingPlan.plan} nodeTitle={pendingPlan.nodeTitle} busy={generateBusy} onConfirm={onConfirmPlan ?? (() => undefined)} onClose={onCancelPlan ?? (() => undefined)} /> : null}
      {confirmDelete ? (
        <div className="scrim outline-delete-scrim" role="dialog" aria-modal="true">
          <div className="outline-delete-dialog">
            <p>
              {confirmDelete === "overview"
                ? t("outline.delete.overview")
                : t("outline.delete.local")}
            </p>
            <div className="outline-delete-actions">
              <button
                type="button"
                className="liquid-tab-btn"
                onClick={() => setConfirmDelete(null)}
              >
                {t("outline.plan.cancel")}
              </button>
              <button
                type="button"
                className="btn-liquid-pill"
                onClick={() => {
                  if (confirmDelete === "overview") onDeleteOverview?.();
                  else onDeleteDeepDive?.();
                  setConfirmDelete(null);
                }}
              >
                {t("outline.delete.confirm")}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </aside>
  );
}
