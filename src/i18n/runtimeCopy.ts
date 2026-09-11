import { runtimeMessages } from "./runtimeMessages";
import type { TranslateFn } from "./LocaleContext";
const patterns = Object.entries(runtimeMessages).filter(([key]) => key.startsWith("requirement_")).map(([key, pair]) => ({ key, parts: pair[1].split("{count}") }));
/** Translate only known application-generated protocol labels. */
export function runtimeCopy(t: TranslateFn, text: string): string {
  for (const {key, parts} of patterns) {
    if (!text.startsWith(parts[0]) || !text.endsWith(parts[1])) continue;
    const count = text.slice(parts[0].length, text.length - parts[1].length);
    if (/^\d+$/.test(count)) return t("runtime." + key, { count });
  }
  return text;
}
export function stageLabel(t: TranslateFn, stage: string): string {
  const [word, ...rest] = stage.split(" ");
  const key = "stage_" + word;
  return key in runtimeMessages ? [t("runtime." + key), ...rest].join(" ") : stage.replaceAll("_", " ");
}
