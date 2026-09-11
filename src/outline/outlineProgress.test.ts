import { describe, expect, it } from "vitest";
import { t as translate } from "../i18n/t";
import type { TranslateFn } from "../i18n/LocaleContext";
import {
  isActiveOutlineJob,
  outlineProgressCopy,
  outlineStageLabel,
  outlineTokenLine,
} from "./outlineProgress";
import type { JobProjection } from "../types";

const zh: TranslateFn = (key, vars) => translate("zh-CN", key, vars);

const job = (overrides: Partial<JobProjection> = {}): JobProjection => ({
  id: "job-1",
  kind: "outline_overview",
  provider: "gemini",
  paperId: "paper-1",
  revisionId: "rev-1",
  rootKey: null,
  artifactKey: null,
  dedupeKey: "outline",
  state: "running",
  stage: "extracting",
  providerCommitted: true,
  priority: 80,
  payload: {},
  lastError: null,
  createdAt: "2026-08-17T00:00:00Z",
  updatedAt: "2026-08-17T00:00:01Z",
  progress: {
    step: 1,
    steps: 3,
    inputTokens: 1200,
    outputTokens: 48,
    cachedInputTokens: 400,
  },
  ...overrides,
});

describe("outline generation progress", () => {
  it("formats stage, step, and live token totals", () => {
    expect(outlineStageLabel("extracting", zh)).toBe("正在抽取论证单元");
    const copy = outlineProgressCopy(job(), zh);
    expect(copy.step).toBe("1 / 3");
    expect(copy.tokens).toBe("入 1,200 · 出 48 · 缓存 400");
    expect(copy.backgroundHint).toContain("后台");
    expect(isActiveOutlineJob(job({ state: "running" }))).toBe(true);
    expect(isActiveOutlineJob(job({ state: "completed" }))).toBe(false);
  });

  it("does not invent tokens before a receipt arrives", () => {
    expect(outlineTokenLine(undefined, zh)).toBe("Token 回执尚未返回");
    expect(outlineTokenLine({}, zh)).toBe("Token 回执尚未返回");
  });
});


it("uses v4 review wording and ignores malformed task payloads", () => {
  expect(outlineProgressCopy(job({ payload: { mapProtocol: "outline-map-v4" }, stage: "composing" }), zh).stage).toBe("正在检查与定稿");
  expect(outlineProgressCopy(job({ payload: { mapProtocol: "outline-deep-dive-v4" }, kind: "outline_deep_dive", stage: "composing" }), zh).stage).toBe("正在生成局部关系图");
  expect(outlineProgressCopy(job({ payload: null }), zh).title).toBe("论证地图生成中");
});
