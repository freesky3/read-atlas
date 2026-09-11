import {
  createContext,
  useContext,
  useMemo,
  type ReactNode,
} from "react";
import { t as translate } from "./t";
import type { UiLocale } from "./types";

export type TranslateFn = (
  key: string,
  vars?: Record<string, string | number>,
) => string;

export type LocaleContextValue = {
  locale: UiLocale;
  t: TranslateFn;
  outputLanguage: UiLocale;
  dateLocale: "zh-CN" | "en-US";
};

const LocaleContext = createContext<LocaleContextValue | null>(null);

export function LocaleProvider({
  locale,
  children,
}: {
  locale: UiLocale;
  children: ReactNode;
}) {
  const value = useMemo<LocaleContextValue>(
    () => ({
      locale,
      t: (key, vars) => translate(locale, key, vars),
      outputLanguage: locale,
      dateLocale: locale === "zh-CN" ? "zh-CN" : "en-US",
    }),
    [locale],
  );
  return (
    <LocaleContext.Provider value={value}>{children}</LocaleContext.Provider>
  );
}

export function useLocale(): LocaleContextValue {
  const context = useContext(LocaleContext);
  if (!context) {
    throw new Error("useLocale must be used within LocaleProvider");
  }
  return context;
}
