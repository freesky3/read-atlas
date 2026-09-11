import { render, type RenderOptions, type RenderResult } from "@testing-library/react";
import type { ReactElement } from "react";
import { LocaleProvider } from "./LocaleContext";
import type { UiLocale } from "./types";

export function renderWithLocale(
  ui: ReactElement,
  locale: UiLocale = "zh-CN",
  options?: Omit<RenderOptions, "wrapper">,
): RenderResult {
  return render(ui, {
    ...options,
    wrapper: ({ children }) => (
      <LocaleProvider locale={locale}>{children}</LocaleProvider>
    ),
  });
}
