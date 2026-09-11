import chitandaZh from "../../src-tauri/prompts/guide-characters/chitanda.json";
import chitandaEn from "../../src-tauri/prompts/guide-characters/en/chitanda.json";
import orekiZh from "../../src-tauri/prompts/guide-characters/oreki.json";
import orekiEn from "../../src-tauri/prompts/guide-characters/en/oreki.json";
import frierenZh from "../../src-tauri/prompts/guide-characters/frieren.json";
import frierenEn from "../../src-tauri/prompts/guide-characters/en/frieren.json";
import jotaroZh from "../../src-tauri/prompts/guide-characters/jotaro.json";
import jotaroEn from "../../src-tauri/prompts/guide-characters/en/jotaro.json";
import conanZh from "../../src-tauri/prompts/guide-characters/conan.json";
import conanEn from "../../src-tauri/prompts/guide-characters/en/conan.json";
import type { GuideCharacter, GuidePresetCast } from "../types";
import type { UiLocale } from "./types";
import { uiText, type TranslateFn } from "./uiText";
const presets = { chitanda: [chitandaZh, chitandaEn], oreki: [orekiZh, orekiEn], frieren: [frierenZh, frierenEn], jotaro: [jotaroZh, jotaroEn], conan: [conanZh, conanEn] };
const fields = ["displayName", "workTitle", "characterVersion", "description", "personality", "readingHabits", "expressionStyle", "avoidances", "exampleNotes"] as const;
const equal = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
/** Only unchanged factory fields are translated. Raw storage and custom text stay intact. */
export function localizedCharacter(character: GuideCharacter, locale: UiLocale): GuideCharacter {
  const pair = presets[character.presetId as keyof typeof presets];
  if (locale !== "en" || !pair) return character;
  const next = { ...character };
  for (const key of fields) if (equal(character[key], pair[0][key])) Object.assign(next, { [key]: pair[1][key] });
  return next;
}
export function canonicalCharacterDraft(draft: GuideCharacter, original: GuideCharacter | null, locale: UiLocale): GuideCharacter {
  if (!original) return draft;
  const shown = localizedCharacter(original, locale), next = { ...draft };
  for (const key of fields) if (equal(draft[key], shown[key])) Object.assign(next, { [key]: original[key] });
  return next;
}
export function localizedCastName(cast: GuidePresetCast, t: TranslateFn): string {
  const originals: Record<string, string> = { daily: "日常阅读", evidence: "实验与证据", classics: "古典部双人" };
  return originals[cast.id] === cast.name ? uiText(t, cast.name) : cast.name;
}
