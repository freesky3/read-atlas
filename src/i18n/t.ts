import { en } from "./messages/en";
import { zhCN } from "./messages/zh-CN";
import type { UiLocale } from "./types";

const catalogs: Record<UiLocale, Record<string, unknown>> = {
  "zh-CN": zhCN,
  en,
};

export function flatten(
  tree: Record<string, unknown>,
  prefix = "",
): string[] {
  const keys: string[] = [];
  for (const [name, value] of Object.entries(tree)) {
    const path = prefix ? `${prefix}.${name}` : name;
    if (value && typeof value === "object" && !Array.isArray(value)) {
      keys.push(...flatten(value as Record<string, unknown>, path));
    } else {
      keys.push(path);
    }
  }
  return keys;
}

export function t(
  locale: UiLocale,
  key: string,
  vars?: Record<string, string | number>,
): string {
  const parts = key.split(".");
  let node: unknown = catalogs[locale];
  for (const part of parts) {
    if (!node || typeof node !== "object" || !(part in node)) {
      throw new Error(`Missing i18n key: ${key}`);
    }
    node = (node as Record<string, unknown>)[part];
  }
  if (typeof node !== "string") {
    throw new Error(`Missing i18n key: ${key}`);
  }
  if (!vars) return node;
  return node.replace(/\{(\w+)\}/g, (match, name: string) => {
    const value = vars[name];
    return value === undefined ? match : String(value);
  });
}
