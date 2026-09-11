import type { UiLocale } from "./types";
export function languageCode(value: string): string {
  const normalized = value.trim().toLowerCase();
  if (["zh", "zh-cn", "zh-hans", "中文", "汉语", "简体中文", "chinese"].includes(normalized)) return "zh-CN";
  if (["en", "en-us", "en-gb", "英文", "英语", "english"].includes(normalized)) return "en";
  return value;
}
export function metadataAbstractIndex(content: Record<string, any>, locale: UiLocale): number | null {
  const abstracts: Record<string, any>[] = Array.isArray(content.document?.abstracts) ? content.document.abstracts : [];
  if (content._abstractSelection) {
    const index = abstracts.findIndex(a => JSON.stringify(a) === JSON.stringify(content._abstractSelection));
    return index < 0 ? null : index;
  }
  const language = languageCode(content._preferredLanguage ?? locale);
  const choices = abstracts.map((a, index) => ({ a, index })).filter(({ a }) => a.segments?.length);
  const matches = (a: Record<string, any>) => (a.languages ?? []).some((v: string) => languageCode(v) === language);
  choices.sort((left, right) => Number(matches(right.a)) - Number(matches(left.a)) || Number(right.a.completeness === "complete") - Number(left.a.completeness === "complete") || left.index - right.index);
  return choices[0]?.index ?? null;
}
