import OperationFailure from "./components/OperationFailure";
import { stageLabel } from "./i18n/runtimeCopy";
import OperationErrorDetails from "./components/OperationErrorDetails";
import React, { useEffect, useMemo, useState } from "react";
import type {
  DiagnosticPreview,
  DocumentCard,
  JobProjection,
  RemoteTombstoneProjection,
  StorageReport,
  TrashProjection,
  ProviderInstanceView,
} from "./types";
import { useDialogFocusTrap } from "./useDialogFocusTrap";
import type { BatchProjection } from "./library/libraryActTypes";
import { summarizeBatch } from "./library/libraryActTypes";
import { peekUndoToken, subscribeUndoTokens } from "./library/undoTokenStore";
import { useLocale, type TranslateFn } from "./i18n/LocaleContext";

type OperationsDrawerProps = {
  open: boolean;
  jobs: JobProjection[];
  batches?: readonly BatchProjection[];
  storage: StorageReport | null;
  diagnostics: DiagnosticPreview | null;
  trash: TrashProjection[];
  remoteTombstones: RemoteTombstoneProjection[];
  documents: DocumentCard[];
  busyJobId: string;
  onClose: () => void;
  onRefresh: () => void;
  onPause: (jobId: string) => void;
  onResume: (jobId: string) => void;
  onReprioritize: (jobId: string, priority: number) => void;
  onCancel: (jobId: string) => void;
  onRestore: (paperId: string) => void;
  onRestoreReaderFolder?: (id: string) => void;
  onRetryRemote: (tombstoneId: string) => void;
  onPreviewDiagnostics: () => void;
  onExportDiagnostics: () => void;
  providerInstances?: ProviderInstanceView[];
  onRebindProvider?: (jobId: string, providerInstanceId: string) => void;
  onAbandonLegacyProviderJob?: (jobId: string, confirmedPotentialCharge: boolean) => void;
  onOpenProviderSettings?: (providerInstanceId: string | null) => void;
  onRecheckProvider?: (jobId: string) => void;
  onAbandonRemoteCleanup?: (tombstoneId: string, confirmed: boolean) => void;
  focusJobId?: string | null;
  onRetryJob?: (jobId: string, confirmedPotentialCharge: boolean) => void;
  onOpenSource?: (paperId: string | null, revisionId: string | null) => void;
  onRetryBatch?: (batchId: string) => void;
  onCancelBatch?: (batchId: string) => void;
  onUndoBatch?: (batchId: string, token: string) => void;
};

type OperationsTab = "tasks" | "storage" | "trash" | "diagnostics";

const ACTIVE_STATES = new Set(["queued", "running", "paused"]);

function formatBytes(value: number) {
  if (!Number.isFinite(value) || value <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const unit = Math.min(
    Math.floor(Math.log(value) / Math.log(1024)),
    units.length - 1,
  );
  const amount = value / 1024 ** unit;
  return `${amount >= 10 || unit === 0 ? amount.toFixed(0) : amount.toFixed(1)} ${units[unit]}`;
}

function formatDate(value: string, dateLocale: string) {
  const date = new Date(value);
  return Number.isNaN(date.valueOf())
    ? "—"
    : new Intl.DateTimeFormat(dateLocale, {
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
      }).format(date);
}

function jobLabel(job: JobProjection, t: TranslateFn) {
  if (job.kind === "ocr") return t("operations.jobs.ocr");
  if (job.kind === "orientation_pack")
    return (job.payload as Record<string, unknown> | null)
      ?.documentArtifactProtocol
      ? t("operations.jobs.brief")
      : t("operations.jobs.orientationLegacy");
  if (job.kind === "document_artifact") {
    const kind = String(
      (job.payload as Record<string, unknown> | null)?.documentArtifactKind,
    );
    const key =
      kind === "glossary"
        ? "operations.jobs.glossary"
        : kind === "symbol_table"
          ? "operations.jobs.symbolTable"
          : kind === "metadata"
            ? "operations.jobs.metadata"
            : "operations.jobs.documentArtifact";
    return t(key);
  }
  if (job.kind === "outline_overview") return t("operations.jobs.outlineOverview");
  if (job.kind === "outline_deep_dive") return t("operations.jobs.outlineDeepDive");
  if (job.kind === "reading_guide") return t("operations.jobs.readingGuide");
  if (job.kind === "reading_artifact") {
    const action = (job.payload as { action?: string } | null)?.action;
    if (action === "translate") return t("operations.jobs.blockTranslation");
    if (action === "explain") return t("operations.jobs.blockExplanation");
    if (action === "lens") return t("operations.jobs.objectLens");
  }
  return job.kind.replaceAll("_", " ");
}

function jobSubTitle(job: JobProjection, t: TranslateFn): string {
  if (job.kind === "ocr") return t("operations.sub.ocr");
  if (job.kind === "orientation_pack")
    return (job.payload as Record<string, unknown> | null)
      ?.documentArtifactProtocol
      ? t("operations.sub.brief")
      : t("operations.sub.orientationLegacy");
  if (job.kind === "document_artifact") return t("operations.sub.documentArtifact");
  if (job.kind === "outline_overview") return t("operations.sub.outlineOverview");
  if (job.kind === "outline_deep_dive") return t("operations.sub.outlineDeepDive");
  if (job.kind === "reading_guide") return t("operations.sub.readingGuide");
  if (job.kind === "reading_artifact") {
    const action = (job.payload as { action?: string } | null)?.action;
    if (action === "translate") return t("operations.sub.translate");
    if (action === "explain") return t("operations.sub.explain");
    if (action === "lens") return t("operations.sub.lens");
  }
  return t("operations.sub.default");
}

function providerBadge(provider: string | null | undefined, t: TranslateFn) {
  if (provider === "mistral") return { label: "Mistral OCR", icon: "⚡", className: "provider-badge-mistral" };
  if (provider === "gemini_proxy") return { label: "Gemini Proxy", icon: "⚡", className: "provider-badge-gemini-proxy" };
  if (provider === "openai_compatible") return { label: "OpenAI", icon: "🤖", className: "provider-badge-openai" };
  if (provider === "grok") return { label: "Grok", icon: "🌌", className: "provider-badge-grok" };
  if (provider === "gemini") return { label: "Gemini", icon: "✨", className: "provider-badge-gemini" };
  return { label: provider || stageLabel(t, "local"), icon: "💻", className: "provider-badge-local" };
}

function stateBadge(state: string, t: TranslateFn) {
  if (state === "completed") return { label: t("operations.state.completed"), icon: "✅", className: "state-badge-completed" };
  if (state === "running") return { label: t("operations.state.running"), icon: "⏳", className: "state-badge-running" };
  if (state === "queued") return { label: t("operations.state.queued"), icon: "🕒", className: "state-badge-queued" };
  if (state === "paused") return { label: t("operations.state.paused"), icon: "⏸️", className: "state-badge-paused" };
  if (state === "failed") return { label: t("operations.state.failed"), icon: "❌", className: "state-badge-failed" };
  return { label: stageLabel(t, state), icon: "●", className: `state-badge-${state}` };
}

function eligibleRebindCandidates(
  job: JobProjection,
  providerInstances: ProviderInstanceView[],
) {
  const requirement = job.providerRequirement;
  const models = job.providerRoute?.models;
  if (
    !requirement?.canRebind ||
    requirement.providerCommitted ||
    job.providerCommitted ||
    !requirement.providerKind ||
    !models?.paper ||
    !models.translation
  ) {
    return [];
  }

  return providerInstances.filter(
    (candidate) =>
      candidate.kind === requirement.providerKind &&
      candidate.paperModel === models.paper &&
      candidate.translationModel === models.translation &&
      candidate.credentialConfigured &&
      candidate.paperProbePassed &&
      Boolean(candidate.connectionVerifiedAt),
  );
}

function rebindCandidateLabel(candidate: ProviderInstanceView) {
  return candidate.name + " | " + candidate.kind + " | " + candidate.paperModel + " / " + candidate.translationModel;
}

function isLegacyProviderRequirement(job: JobProjection) {
  return ["legacy_ambiguous", "legacy_unattributed", "quarantined"].includes(
    job.providerRequirement?.code ?? "",
  );
}

function providerRequirementSummary(job: JobProjection, t: TranslateFn) {
  const requirement = job.providerRequirement;
  if (!requirement) return "";
  if (isLegacyProviderRequirement(job)) {
    return t("operations.req.legacy");
  }
  if (requirement.providerCommitted || job.providerCommitted) {
    return t("operations.req.committed");
  }
  if (requirement.code === "credential_missing") {
    return t("operations.req.credentialMissing");
  }
  if (requirement.code === "provider_missing") {
    return t("operations.req.providerMissing");
  }
  return t("operations.req.paused");
}

export default function OperationsDrawer({
  open,
  jobs,
  batches = [],
  storage,
  diagnostics,
  trash,
  remoteTombstones,
  documents,
  busyJobId,
  onClose,
  onRefresh,
  onPause,
  onResume,
  onReprioritize,
  onCancel,
  onRestore,
  onRestoreReaderFolder,
  onRetryRemote,
  onPreviewDiagnostics,
  onExportDiagnostics,
  providerInstances = [],
  onRebindProvider,
  onAbandonLegacyProviderJob,
  onOpenProviderSettings,
  onRecheckProvider,
  onAbandonRemoteCleanup,
  focusJobId,
  onRetryJob,
  onOpenSource,
  onRetryBatch,
  onCancelBatch,
  onUndoBatch,
}: OperationsDrawerProps) {
  const { t, dateLocale } = useLocale();
  const [tab, setTab] = useState<OperationsTab>("tasks");
  const [, setUndoTick] = useState(0);
  const [selectedRebindInstanceByJob, setSelectedRebindInstanceByJob] =
    useState<Record<string, string>>({});
  const [legacyAbandonConfirmJobId, setLegacyAbandonConfirmJobId] =
    useState<string | null>(null);
  const [legacyAbandonConfirmRemoteId, setLegacyAbandonConfirmRemoteId] =
    useState<string | null>(null);
  const closeBtnRef = React.useRef<HTMLButtonElement>(null);
  const dialogRootRef = React.useRef<HTMLDivElement>(null);
  useDialogFocusTrap({
    open,
    containerRef: dialogRootRef,
    onClose,
    initialFocusRef: closeBtnRef,
  });
  useEffect(() => subscribeUndoTokens(() => setUndoTick((value) => value + 1)), []);
  useEffect(() => {
    if (!open) return;
    const timer = window.setTimeout(() => {
      const card = focusJobId
        ? document.getElementById(`operation-job-${focusJobId}`)
        : null;
      if (card) {
        card.scrollIntoView({ block: "nearest" });
        card.focus({ preventScroll: true });
      } else {
        closeBtnRef.current?.focus();
      }
    }, 0);
    return () => window.clearTimeout(timer);
  }, [focusJobId, open]);
  const activeCount =
    jobs.filter((job) => ACTIVE_STATES.has(job.state)).length +
    remoteTombstones.filter((item) => item.state !== "resolved").length;
  const paperNames = useMemo(
    () => new Map(documents.map((document) => [document.id, document.title])),
    [documents],
  );
  const sortedJobs = useMemo(() => {
    return [...jobs].sort((a, b) => {
      const aTime = new Date(a.updatedAt || a.createdAt).getTime();
      const bTime = new Date(b.updatedAt || b.createdAt).getTime();
      if (!Number.isNaN(aTime) && !Number.isNaN(bTime) && aTime !== bTime) {
        return bTime - aTime;
      }
      return b.id.localeCompare(a.id);
    });
  }, [jobs]);

  if (!open) return null;

  return (
    <div
      ref={dialogRootRef}
      className="scrim operations-scrim open"
      onClick={onClose}
    >
      <aside
        className="operations-drawer"
        role="dialog"
        aria-modal="true"
        aria-label={t("operations.dialogAria")}
        onClick={(event) => event.stopPropagation()}
      >
        <header className="operations-head">
          <div>
            <span className="eyebrow">{t("operations.eyebrow")}</span>
            <h2>{t("operations.ledgerTitle")}</h2>
            <p>{t("operations.activeSummary", { count: activeCount })}</p>
          </div>
          <div className="operations-head-actions">
            <button className="outline-button" onClick={onRefresh}>
              {t("operations.refresh")}
            </button>
            <button
              ref={closeBtnRef}
              className="btn-liquid-close"
              onClick={onClose}
              aria-label={t("operations.closeAria")}
              title={t("operations.closeTitle")}
            >
              ✕
            </button>
          </div>
        </header>

        <div className="operations-nav-container">
          <nav className="operations-tabs" aria-label={t("completion.operations_view")}>
            <button
              aria-label={t("completion.tasks")}
              className={tab === "tasks" ? "active" : ""}
              onClick={() => setTab("tasks")}
            >
              {t("operations.tabTasks")} <span>{jobs.length}</span>
            </button>
            <button
              aria-label={t("completion.storage")}
              className={tab === "storage" ? "active" : ""}
              onClick={() => setTab("storage")}
            >
              {t("operations.tabStorage")}
            </button>
            <button
              aria-label={t("completion.trash")}
              className={tab === "trash" ? "active" : ""}
              onClick={() => setTab("trash")}
            >
              {t("operations.tabTrash")} <span>{trash.length}</span>
            </button>
            <button
              aria-label={t("completion.diagnostics")}
              className={tab === "diagnostics" ? "active" : ""}
              onClick={() => setTab("diagnostics")}
            >
              {t("operations.tabDiagnostics")}
            </button>
          </nav>
        </div>

        <div className="operations-body">
          {tab === "tasks" ? (
            <section className="job-ledger">
              {batches.length > 0 ? (
                <div className="operations-batch-group">
                  <h3 className="operations-batch-heading">{t("operations.batches")}</h3>
                  {batches.map((batch) => (
                    <article key={batch.id} className={`job-card state-${batch.state}`}>
                      <div className="job-card-main">
                        <div className="job-card-title">
                          <strong>{batch.commandKind.replaceAll("_", " ")}</strong>
                          <span className={`state-badge state-badge-${batch.state}`}>{batch.state}</span>
                        </div>
                        <p>{t("operations.batchItems", { summary: summarizeBatch(batch, t), count: batch.totalItems })}</p>
                        <footer className="job-card-actions">
                          {batch.counts.failed > 0 && onRetryBatch ? (
                            <button type="button" className="outline-button" onClick={() => onRetryBatch(batch.id)}>{t("operations.retryFailed")}</button>
                          ) : null}
                          {(batch.state === "queued" || batch.state === "running" || batch.state === "planned") && onCancelBatch ? (
                            <button type="button" className="outline-button" onClick={() => onCancelBatch(batch.id)}>{t("operations.cancelRemaining")}</button>
                          ) : null}
                          {onUndoBatch && peekUndoToken(batch.id) && batch.undo?.state === "available" ? (
                            <button
                              type="button"
                              className="outline-button"
                              onClick={() => {
                                const token = peekUndoToken(batch.id);
                                if (token) onUndoBatch(batch.id, token);
                              }}
                            >{t("operations.undoBatch")}</button>
                          ) : null}
                        </footer>
                      </div>
                    </article>
                  ))}
                </div>
              ) : null}
              {sortedJobs.length === 0 && batches.length === 0 ? (
                <div className="operations-empty">
                  <span>◎</span>
                  <strong>{t("operations.emptyTitle")}</strong>
                  <p>
                    {t("operations.emptyBody")}
                  </p>
                </div>
              ) : sortedJobs.length === 0 ? null : (
                sortedJobs.map((job) => {
                  const pBadge = providerBadge(job.provider, t);
                  const providerRequirement = job.providerRequirement;
                  const retryDisposition =
                    job.retryDisposition ?? "unavailable";
                  const retryReason =
                    job.retryReason ??
                    t("operations.retryUnknown");
                  const sBadge = providerRequirement ? { label: t("operations.actionRequired"), icon: "!", className: "state-badge-action-required" } : stateBadge(job.state, t);
                  const rebindCandidates = eligibleRebindCandidates(job, providerInstances);
                  const selectedRebindInstanceId = selectedRebindInstanceByJob[job.id] ?? "";
                  const legacyProviderRequirement = isLegacyProviderRequirement(job);
                  const paperTitle = job.paperId ? paperNames.get(job.paperId) : null;
                  return (
                    <article
                      id={`operation-job-${job.id}`}
                      tabIndex={-1}
                      className={`job-card state-${job.state}`}
                      key={job.id}
                    >
                      <div className="job-state-rail" />
                      <div className="job-card-main">
                        <div className="job-card-title">
                          <div className="job-card-title-left">
                            <span className={`provider-chip ${pBadge.className}`}>
                              <i>{pBadge.icon}</i> {pBadge.label}
                            </span>
                            <div className="job-title-group">
                              <strong>{jobLabel(job, t)}</strong>
                              {paperTitle ? (
                                <span className="job-paper-tag" title={paperTitle}>
                                  📄 {paperTitle}
                                </span>
                              ) : null}
                            </div>
                          </div>
                          <div className={`job-state-pill ${sBadge.className}`}>
                            <i>{sBadge.icon}</i>
                            <span>{sBadge.label}</span>
                          </div>
                        </div>

                        <div className="job-subtitle-desc">
                          {jobSubTitle(job, t)}
                        </div>

                        <div className="job-stage">
                          <i />
                          <span className="job-stage-name">
                            {t("operations.stage")}: <code>{stageLabel(t, job.stage)}</code>
                            {job.progress?.batch && job.progress?.batches
                              ? t("operations.batchProgress", { batch: job.progress.batch, batches: job.progress.batches })
                              : job.progress?.step && job.progress?.steps
                                ? ` · ${job.progress.step}/${job.progress.steps}`
                                : ""}
                          </span>
                          {retryDisposition === "confirm_possible_charge" ? (
                            <small className="job-commit-tag committed">
                              {t("operations.maybeCharged")}
                            </small>
                          ) : retryDisposition === "safe" ? (
                            <small className="job-commit-tag safe">
                              {t("operations.safeRetry")}
                            </small>
                          ) : (
                            <small className="job-commit-tag committed">
                              {t("operations.unsafeRetry")}
                            </small>
                          )}
                        </div>

                        {providerRequirement ? (
                          <section className="job-provider-requirement" role="alert">
                            <strong>{t("operations.actionRequired")}</strong>
                            <p>{providerRequirementSummary(job, t)}</p>
                            {providerRequirement.code === "credential_missing" &&
                            ((providerRequirement.providerInstanceId && onOpenProviderSettings) ||
                              onRecheckProvider) ? (
                              <div className="job-provider-recovery-actions">
                                {providerRequirement.providerInstanceId && onOpenProviderSettings ? (
                                  <button
                                    disabled={busyJobId === job.id}
                                    onClick={() =>
                                      onOpenProviderSettings(
                                        providerRequirement.providerInstanceId,
                                      )
                                    }
                                  >
                                    {t("operations.openModelSettings")}
                                  </button>
                                ) : null}
                                {onRecheckProvider ? (
                                  <button
                                    disabled={busyJobId === job.id}
                                    onClick={() => onRecheckProvider(job.id)}
                                  >
                                    {t("operations.recheckProvider")}
                                  </button>
                                ) : null}
                              </div>
                            ) : null}
                            {providerRequirement.canRebind &&
                            !providerRequirement.providerCommitted &&
                            !job.providerCommitted ? (
                              rebindCandidates.length > 0 ? (
                                <div className="job-provider-rebind">
                                  <label>
                                    <span>{t("operations.chooseSafeProvider")}</span>
                                    <select
                                      aria-label={t("operations.chooseSafeProviderFor", { label: jobLabel(job, t) })}
                                      value={selectedRebindInstanceId}
                                      onChange={(event) =>
                                        setSelectedRebindInstanceByJob((current) => ({
                                          ...current,
                                          [job.id]: event.target.value,
                                        }))
                                      }
                                    >
                                      <option value="">{t("operations.pickMatchingProvider")}</option>
                                      {rebindCandidates.map((candidate) => (
                                        <option key={candidate.id} value={candidate.id}>
                                          {rebindCandidateLabel(candidate)}
                                        </option>
                                      ))}
                                    </select>
                                  </label>
                                  <button
                                    disabled={
                                      !selectedRebindInstanceId ||
                                      busyJobId === job.id ||
                                      !onRebindProvider
                                    }
                                    onClick={() => {
                                      if (
                                        rebindCandidates.some(
                                          (candidate) =>
                                            candidate.id === selectedRebindInstanceId,
                                        )
                                      ) {
                                        onRebindProvider?.(
                                          job.id,
                                          selectedRebindInstanceId,
                                        );
                                      }
                                    }}
                                  >
                                    {t("operations.bindSelected")}
                                  </button>
                                </div>
                              ) : (
                                <p>{t("operations.noSafeCandidates")}</p>
                              )
                            ) : null}
                            {legacyProviderRequirement &&
                            onAbandonLegacyProviderJob ? (
                              legacyAbandonConfirmJobId === job.id ? (
                                <div className="job-legacy-abandon-confirm">
                                  <p>{t("operations.abandonNoNetwork")}</p>
                                  <button
                                    disabled={busyJobId === job.id}
                                    onClick={() => {
                                      onAbandonLegacyProviderJob(job.id, true);
                                      setLegacyAbandonConfirmJobId(null);
                                    }}
                                  >
                                    {t("operations.confirmAbandonMaybeCharged")}
                                  </button>
                                  <button
                                    disabled={busyJobId === job.id}
                                    onClick={() => setLegacyAbandonConfirmJobId(null)}
                                  >
                                    {t("operations.cancel")}
                                  </button>
                                </div>
                              ) : (
                                <button
                                  disabled={busyJobId === job.id}
                                  onClick={() => setLegacyAbandonConfirmJobId(job.id)}
                                >
                                  {t("operations.abandonLocalJob")}
                                </button>
                              )
                            ) : null}
                          </section>
                        ) : null}

                        {job.lastError ? (
                          <div className="job-error-box">
                            <div className="job-error-header">
                              {t("operations.errorHeader")}
                            </div>
                            <OperationFailure reason={job.lastError} />
                          </div>
                        ) : null}

                        <footer>
                          <div className="job-meta-footer">
                            <code>#{job.id.slice(0, 8)}</code>
                            <span>📅 {formatDate(job.updatedAt, dateLocale)}</span>
                            <span className="job-priority-pill">{t("operations.priority", { value: job.priority })}</span>
                          </div>
                          <div className="job-actions">
                            {!providerRequirement && (job.state === "queued" || job.state === "paused") ? (
                              <>
                                <button
                                  aria-label={t("feedback.message64", { v0: jobLabel(job, t) })}
                                  disabled={busyJobId === job.id}
                                  onClick={() =>
                                    onReprioritize(job.id, job.priority + 10)
                                  }
                                >{t("completion.raise_priority")}</button>
                                <button
                                  aria-label={t("feedback.message65", { v0: jobLabel(job, t) })}
                                  disabled={busyJobId === job.id}
                                  onClick={() =>
                                    onReprioritize(job.id, job.priority - 10)
                                  }
                                >{t("completion.lower_priority")}</button>
                              </>
                            ) : null}
                            {!providerRequirement && job.state === "queued" ? (
                              <button
                                disabled={busyJobId === job.id}
                                onClick={() => onPause(job.id)}
                              >{t("completion.pause")}</button>
                            ) : null}
                            {!providerRequirement && job.state === "paused" ? (
                              <button
                                disabled={busyJobId === job.id}
                                onClick={() => onResume(job.id)}
                              >{t("completion.resume")}</button>
                            ) : null}
                            {ACTIVE_STATES.has(job.state) ? (
                              <button
                                className="danger"
                                disabled={busyJobId === job.id}
                                onClick={() => onCancel(job.id)}
                              >{t("completion.cancel")}</button>
                            ) : null}
                            {(job.state === "failed" || job.state === "interrupted_unknown") && (
                              <>
                                {onOpenSource && job.paperId && (
                                  <button
                                    disabled={busyJobId === job.id}
                                    onClick={() => onOpenSource(job.paperId, job.revisionId)}
                                  >
                                    {t("operations.backToSource")}
                                  </button>
                                )}
                                <button
                                  onClick={() => {
                                    navigator.clipboard?.writeText(job.lastError ?? job.id)?.catch(()=>{});
                                  }}
                                  title={t("operations.copyDetailsTitle")}
                                >
                                  {t("operations.copyDetails")}
                                </button>
                                {retryDisposition === "unavailable" ? (
                                  <span
                                    style={{ fontSize: 11, color: "var(--muted)" }}
                                    title={retryReason}
                                  >
                                    {retryReason}
                                  </span>
                                ) : retryDisposition === "confirm_possible_charge" ? (
                                  onRetryJob ? (
                                    <button
                                      className="danger"
                                      disabled={busyJobId === job.id}
                                      onClick={() => {
                                        if (
                                          window.confirm(
                                            `${retryReason}\n\n${t("operations.confirmRetry")}`,
                                          )
                                        ) {
                                          onRetryJob(job.id, true);
                                        }
                                      }}
                                    >
                                      {t("operations.retryMaybeCharged")}
                                    </button>
                                  ) : null
                                ) : retryDisposition === "safe" && onRetryJob ? (
                                  <button
                                    disabled={busyJobId === job.id}
                                    onClick={() => onRetryJob(job.id, false)}
                                    title={retryReason}
                                  >
                                    {t("operations.retrySafe")}
                                  </button>
                                ) : null}
                              </>
                            )}
                          </div>
                        </footer>
                      </div>
                    </article>
                  );
                })
              )}
              {remoteTombstones.length > 0 ? (
                <div className="remote-cleanup-ledger">
                  <header>
                    <span>{t("completion.remote_cleanup")}</span>
                    <small>{t("completion.provider_files_and_interactions_queued_by_paper_deletion")}</small>
                  </header>
                  {remoteTombstones.map((item) => (
                    <article key={item.id}>
                      <div>
                        <strong>
                          {item.provider} · {item.resourceKind}
                        </strong>
                        <span>
                          {stageLabel(t, item.state)} · {item.attempts} {t("completion.attempt")}
                        </span>
                        {item.lastError ? (
                          <OperationFailure reason={item.lastError} />
                        ) : null}
                      </div>
                      {item.ownershipStatus === "legacy_unattributed" &&
                      item.state !== "resolved" ? (
                        onAbandonRemoteCleanup ? (
                          legacyAbandonConfirmRemoteId === item.id ? (
                            <div className="remote-cleanup-abandon-confirm">
                              <small>{t("operations.abandonCleanupHint")}</small>
                              <button
                                disabled={busyJobId === item.id}
                                onClick={() => {
                                  onAbandonRemoteCleanup(item.id, true);
                                  setLegacyAbandonConfirmRemoteId(null);
                                }}
                              >
                                {t("operations.confirmAbandonCleanup")}
                              </button>
                              <button
                                disabled={busyJobId === item.id}
                                onClick={() => setLegacyAbandonConfirmRemoteId(null)}
                              >
                                {t("operations.cancel")}
                              </button>
                            </div>
                          ) : (
                            <button
                              disabled={busyJobId === item.id}
                              onClick={() => setLegacyAbandonConfirmRemoteId(item.id)}
                            >
                              {t("operations.abandonCleanup")}
                            </button>
                          )
                        ) : (
                          <small>{t("operations.legacyCleanupHint")}</small>
                        )
                      ) : item.state !== "resolved" ? (
                        <button
                          disabled={busyJobId === item.id}
                          onClick={() => onRetryRemote(item.id)}
                        >{t("completion.retry_cleanup")}</button>
                      ) : item.ownershipStatus === "abandoned" ? (
                        <em>{t("operations.abandonedLocally")}</em>
                      ) : (
                        <em>{t("completion.resolved")}</em>
                      )}
                    </article>
                  ))}
                </div>
              ) : null}
            </section>
          ) : null}

          {tab === "storage" ? (
            <section className="storage-ledger">
              <div className="storage-hero">
                <span>{t("completion.workspace_footprint")}</span>
                <strong>{formatBytes(storage?.workspaceBytes ?? 0)}</strong>
                <div>
                  <p>
                    <b>{formatBytes(storage?.papersBytes ?? 0)}</b>
                    <small>{t("completion.source_pdfs")}</small>
                  </p>
                  <p>
                    <b>{formatBytes(storage?.internalBytes ?? 0)}</b>
                    <small>{t("completion.internal_data")}</small>
                  </p>
                </div>
              </div>
              <div className="storage-note">{t("completion.internal_per_paper_values_are_logical_payload_sizes_not_invented_physical_sqlite_page_allocation_workspace_total_is_real_disk_use")}</div>
              <div className="storage-paper-list">
                {(storage?.paperLogicalBytes ?? []).map((paper) => (
                  <article key={paper.paperId}>
                    <div>
                      <strong>
                        {paperNames.get(paper.paperId) ??
                          paper.paperId.slice(0, 8)}
                      </strong>
                      <span>
                        {formatBytes(
                          paper.sourceBytes + paper.logicalDatabaseBytes,
                        )}
                      </span>
                    </div>
                    <dl>
                      <div>
                        <dt>PDF</dt>
                        <dd>{formatBytes(paper.sourceBytes)}</dd>
                      </div>
                      <div>
                        <dt>{t("completion.artifacts")}</dt>
                        <dd>{formatBytes(paper.artifactBytes)}</dd>
                      </div>
                      <div>
                        <dt>{t("completion.discussion")}</dt>
                        <dd>{formatBytes(paper.discussionBytes)}</dd>
                      </div>
                      <div>
                        <dt>OCR</dt>
                        <dd>{formatBytes(paper.ocrBytes)}</dd>
                      </div>
                    </dl>
                    <div className="artifact-size-list">
                      {(storage?.artifactLogicalBytes ?? [])
                        .filter(
                          (artifact) => artifact.paperId === paper.paperId,
                        )
                        .map((artifact) => (
                          <span key={artifact.artifactId}>
                            <code>
                              {artifact.kind.replaceAll("_", " ")} v
                              {artifact.version}
                            </code>
                            <b>{formatBytes(artifact.logicalBytes)}</b>
                          </span>
                        ))}
                    </div>
                  </article>
                ))}
              </div>
            </section>
          ) : null}

          {tab === "diagnostics" ? (
            <section className="diagnostic-ledger">
              <header>
                <span>{t("completion.redacted_diagnostics")}</span>
                <strong>{t("completion.preview_the_scope_before_writing_a_package")}</strong>
                <p>{t("completion.read_desktop_exports_aggregate_state_only_it_does_not_export_paper_content_or_identities")}</p>
                <div>
                  <button
                    disabled={busyJobId === "diagnostics"}
                    onClick={onPreviewDiagnostics}
                  >{t("completion.preview_scope")}</button>
                  <button
                    disabled={
                      busyJobId === "diagnostics" || diagnostics === null
                    }
                    onClick={onExportDiagnostics}
                  >{t("completion.export_json")}</button>
                </div>
              </header>
              <OperationErrorDetails />
              {diagnostics ? (
                <div className="diagnostic-preview">
                  <article>
                    <h3>{t("completion.included")}</h3>
                    <ul>
                      {diagnostics.includedSections.map((entry) => (
                        <li key={entry}>{entry}</li>
                      ))}
                    </ul>
                  </article>
                  <article className="excluded">
                    <h3>{t("completion.explicitly_excluded")}</h3>
                    <ul>
                      {diagnostics.excludedData.map((entry) => (
                        <li key={entry}>{entry}</li>
                      ))}
                    </ul>
                  </article>
                  <pre>{JSON.stringify(diagnostics.summary, null, 2)}</pre>
                  <small>{t("completion.previewed")}{" "}{formatDate(diagnostics.generatedAt, dateLocale)}</small>
                </div>
              ) : (
                <div className="operations-empty">
                  <span>◌</span>
                  <strong>{t("completion.no_diagnostic_data_has_been_read_yet")}</strong>
                  <p>{t("completion.preview_is_explicit_and_does_not_write_a_file")}</p>
                </div>
              )}
            </section>
          ) : null}

          {tab === "trash" ? (
            <section className="trash-ledger">
              {trash.length === 0 ? (
                <div className="operations-empty">
                  <span>⌁</span>
                  <strong>{t("completion.trash_is_empty")}</strong>
                  <p>{t("completion.deleted_papers_remain_recoverable_for_30_days")}</p>
                </div>
              ) : (
                trash.map((entry) => {
                  const folderNote = entry.kind === "reader_folder";
                  return (
                  <article key={entry.id}>
                    <span className="trash-file-mark">{folderNote ? "MD" : "PDF"}</span>
                    <div>
                      <strong>{entry.title}</strong>
                      <code>{entry.originalRelativePath}</code>
                      <small>
                        {formatBytes(entry.sourceBytes)}{" "}{t("completion.purge_after")}{" "}
                        {formatDate(entry.purgeAfter, dateLocale)}
                      </small>
                    </div>
                    <button
                      disabled={busyJobId === (folderNote ? entry.id : entry.paperId)}
                      onClick={() => {
                        if (folderNote) onRestoreReaderFolder?.(entry.id);
                        else onRestore(entry.paperId);
                      }}
                    >{t("completion.restore")}</button>
                  </article>
                  );
                })
              )}
            </section>
          ) : null}
        </div>
      </aside>
    </div>
  );
}
