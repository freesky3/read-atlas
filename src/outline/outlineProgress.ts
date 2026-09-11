import { isOutlineV4Protocol } from "./outlineDisplay";
import type { JobProgress, JobProjection } from "../types";
import { t as translate } from "../i18n/t";
import type { TranslateFn } from "../i18n/LocaleContext";

const ACTIVE = new Set(["queued", "running"]);

const zhT: TranslateFn = (key, vars) => translate("zh-CN", key, vars);

export function isOutlineJob(job: Pick<JobProjection, "kind">) {
  return job.kind === "outline_overview" || job.kind === "outline_deep_dive";
}

export function isActiveOutlineJob(job: JobProjection) {
  return isOutlineJob(job) && ACTIVE.has(job.state);
}

export function outlineStageLabel(
  stage: string,
  t: TranslateFn = zhT,
  kind?: string,
  protocol?: string,
) {
  const v4 = isOutlineV4Protocol(protocol) || protocol === "v4";
  if (kind === "outline_deep_dive") {
    if (stage === "composing") return t("outline.stage.localComposing");
    if (stage === "repairing") return t("outline.stage.localRepairing");
    if (stage === "published") return t("outline.stage.localPublished");
    if (stage === "queued" || stage === "preparing") return t("outline.stage.localQueued");
  }
  switch (stage) {
    case "queued":
    case "preparing":
      return t("outline.stage.queued");
    case "extracting":
      return t("outline.stage.extracting");
    case "extracted":
      return t("outline.stage.extracted");
    case "drafting":
    case "drafted":
      return t("outline.stage.drafting");
    case "reviewing":
      return t("outline.stage.reviewing");
    case "composing":
      return v4 ? t("outline.stage.reviewing") : t("outline.stage.composingLegacy");
    case "repairing":
      return t("outline.stage.repairing");
    case "published":
      return t("outline.stage.published");
    default:
      return stage.replaceAll("_", " ");
  }
}

export function outlineStepLabel(job: JobProjection) {
  const progress = job.progress ?? {};
  const steps = progress.steps ?? (job.kind === "outline_deep_dive" ? 1 : 3);
  const step = progress.step ?? (job.state === "queued" ? 0 : 1);
  return `${Math.min(step, steps)} / ${steps}`;
}

export function formatTokenCount(value: number | null | undefined) {
  if (value === null || value === undefined) return null;
  return value.toLocaleString("en-US");
}

export function outlineTokenLine(
  progress: JobProgress | null | undefined,
  t: TranslateFn = zhT,
) {
  const input = formatTokenCount(progress?.inputTokens);
  const output = formatTokenCount(progress?.outputTokens);
  const cached = formatTokenCount(progress?.cachedInputTokens);
  if (!input && !output && !cached) {
    return t("outline.tokens.pending");
  }
  const parts = [];
  if (input) parts.push(t("outline.tokens.input", { n: input }));
  if (output) parts.push(t("outline.tokens.output", { n: output }));
  if (cached) parts.push(t("outline.tokens.cached", { n: cached }));
  return parts.join(" · ");
}

export function outlineProgressCopy(job: JobProjection, t: TranslateFn = zhT) {
  const payload = job.payload && typeof job.payload === "object"
    ? job.payload as Record<string, unknown> : {};
  const protocol = typeof payload.mapProtocol === "string" ? payload.mapProtocol : "";
  const v4 = isOutlineV4Protocol(protocol);
  const unitCount = job.progress?.unitCount;
  return {
    title:
      job.kind === "outline_deep_dive"
        ? t("outline.progress.titleLocal")
        : v4
          ? t("outline.progress.titleV4")
          : t("outline.progress.titleLegacy"),
    stage: outlineStageLabel(job.stage, t, job.kind, protocol),
    step: outlineStepLabel(job),
    tokens: outlineTokenLine(job.progress, t),
    units:
      !v4 && typeof unitCount === "number"
        ? t("outline.progress.units", { count: unitCount })
        : null,
    backgroundHint: t("outline.progress.backgroundHintHelper"),
  };
}
