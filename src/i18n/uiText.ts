import { completionEntries } from "./messages/completion";
import { t } from "./t";
import type { TranslateFn } from "./LocaleContext";
export type { TranslateFn } from "./LocaleContext";
export const zhT: TranslateFn = (key, vars) => t("zh-CN", key, vars);
export const enT: TranslateFn = (key, vars) => t("en", key, vars);
const keys = new Map(Object.entries(completionEntries).flatMap(([key, pair]) => pair.map(text => [text, key] as const)));
/** Only application-owned labels belong here; user and model prose is never translated. */
export function uiText(translate: TranslateFn, label: string, vars?: Record<string, string | number>): string {
  const key = keys.get(label);
  return key ? translate("completion." + key, vars) : label;
}
