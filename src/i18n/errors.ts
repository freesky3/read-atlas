import type { UiLocale } from "./types";
import { errorMessages } from "./errorMessages";
export type OperationDiagnostic = { id: number; operation: string; code: string; detail: string; at: string };
let diagnostics: readonly OperationDiagnostic[] = [];
const listeners = new Set<() => void>();
export const errorDiagnostics = () => diagnostics;
export const subscribeErrors = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; };
export function currentUiLocale(): UiLocale { return typeof document !== "undefined" && document.documentElement.lang === "en" ? "en" : "zh-CN"; }
export function errorCode(reason: unknown): string {
  const value = reason as { code?: string; message?: string } | null;
  if (value?.code && errorMessages[value.code]) return value.code;
  const text = String(value?.message ?? reason);
  if (/超过.*窗口|超出.*窗口|exceed.*(?:window|context)/i.test(text)) return "over_window";
  if (/不支持.*PDF|does not support.*PDF|unsupported.*PDF/i.test(text)) return "unsupported_pdf";
  if (/unauthorized|invalid.*(?:key|credential)|401|凭据.*(?:无效|失效)/i.test(text)) return "unauthorized";
  if (/rate.?limit|429|限流/i.test(text)) return "rate_limited";
  if (/cancelled|canceled|已取消/i.test(text)) return "cancelled";
  if (/显示名称不能为空|display name.*empty/i.test(text)) return "name_required";
  if (/其他窗口|another window|配置已.*更新/i.test(text)) return "settings_changed";
  if (/重新计划|plan.*changed|plan.*stale/i.test(text)) return "stale_plan";
  if (/先.*OCR|OCR.*first|missing.*OCR/i.test(text)) return "missing_ocr";
  return "unknown";
}
export function errorMessage(reason: unknown, locale: UiLocale): string { return errorMessages[errorCode(reason)][locale === "en" ? 1 : 0]; }
/** Localized recovery guidance; keep original provider/OS detail available in Diagnostics. */
export function captureOperationError(operation: string, reason: unknown, locale = currentUiLocale()): any {
  const object = reason && typeof reason === "object" ? reason as Record<string, unknown> : {};
  const detail = String(object.message ?? reason), code = errorCode(reason), message = errorMessage(reason, locale);
  diagnostics = [...diagnostics.slice(-19), { id: Date.now() + Math.random(), operation, code, detail, at: new Date().toISOString() }];
  listeners.forEach(listener => listener());
  const localized = { ...object, message };
  Object.defineProperty(localized, "toString", { value: () => message });
  return localized;
}
