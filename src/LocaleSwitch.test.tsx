import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { LocaleProvider } from "./i18n/LocaleContext";
import SettingsWorkbench from "./SettingsWorkbench";
import { createMemoryDesktopClient, desktopClient } from "./desktopClient";
import type { UiLocale } from "./i18n/types";
import { formatOutlinePlanCard } from "./outline/outlinePlan";
import { t } from "./i18n/t";

describe("locale switching", () => {
  it("keeps unsaved drafts per language and binds saves to the displayed bank", async () => {
    const client = createMemoryDesktopClient();
    await client.command("save_prompt_slot", { request: { slot: "outline_extract", kind: "paper", locale: "zh-CN", text: "中文自定义" } });
    await client.command("save_prompt_slot", { request: { slot: "outline_extract", kind: "paper", locale: "en", text: "English custom" } });
    vi.spyOn(desktopClient, "open").mockImplementation(client.open.bind(client));
    const command = vi.spyOn(desktopClient, "command").mockImplementation(client.command.bind(client));
    const modelSettings = await client.open<any>("get_model_settings");
    const view = (locale: UiLocale) => <LocaleProvider locale={locale}><SettingsWorkbench open initialSection="prompts" workspace={null} modelSettings={modelSettings} workspaceBusy={false} onClose={vi.fn()} onChooseWorkspace={vi.fn()} onResetWorkspace={vi.fn()} onOpenWorkspace={vi.fn()} onModelSettingsChange={vi.fn()} locale={locale} onRequestLocaleChange={vi.fn()} /></LocaleProvider>;
    const { rerender } = render(view("zh-CN"));
    fireEvent.change(await screen.findByDisplayValue("中文自定义"), { target: { value: "尚未保存的中文草稿" } });
    await client.command("set_ui_locale", { locale: "en" });
    rerender(view("en"));
    fireEvent.change(await screen.findByDisplayValue("English custom"), { target: { value: "Edited English custom" } });
    // Simulate a second window changing the global preference before this save arrives.
    await client.command("set_ui_locale", { locale: "zh-CN" });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(command).toHaveBeenCalledWith("save_prompt_slot", { request: { slot: "outline_extract", kind: "paper", locale: "en", text: "Edited English custom" } }));
    rerender(view("zh-CN"));
    expect(await screen.findByDisplayValue("尚未保存的中文草稿")).toBeInTheDocument();
    const zh = await client.open<any>("get_prompt_settings", { locale: "zh-CN" });
    const en = await client.open<any>("get_prompt_settings", { locale: "en" });
    expect(zh.slots.outline_extract.paper.text).toBe("中文自定义");
    expect(en.slots.outline_extract.paper.text).toBe("Edited English custom");
  });

  it("reloads the prompt bank when an open settings window changes language", async () => {
    const client = createMemoryDesktopClient();
    const open = vi.spyOn(desktopClient, "open").mockImplementation(client.open.bind(client));
    const settings = await client.open<any>("get_model_settings");
    const workspace = await client.open<any>("get_workspace");
    const view = (locale: UiLocale) => <LocaleProvider locale={locale}>
      <SettingsWorkbench open initialSection="prompts" workspace={workspace} modelSettings={settings}
        workspaceBusy={false} onClose={vi.fn()} onChooseWorkspace={vi.fn()} onResetWorkspace={vi.fn()}
        onOpenWorkspace={vi.fn()} onModelSettingsChange={vi.fn()} locale={locale} onRequestLocaleChange={vi.fn()} />
    </LocaleProvider>;
    const { rerender } = render(view("zh-CN"));
    await waitFor(() => expect(screen.getAllByRole("textbox").length).toBeGreaterThan(0));
    const reads = () => open.mock.calls.filter(([name]) => name === "get_prompt_settings").length;
    expect(reads()).toBe(1);
    await client.command("set_ui_locale", { locale: "en" });
    rerender(view("en"));
    await waitFor(() => expect(reads()).toBe(2));
  });

  it("uses English throughout a new map plan card", () => {
    const card = formatOutlinePlanCard({ workflow: "draft_review", protocolVersion: "outline-map-v4",
      model: "test", pageCount: 8, rootCalls: 1, extractCalls: 1, composeCalls: 1,
      maxRepairCalls: 1, ocrRevisionId: "ocr", catalogDigest: "digest" } as any,
      (key: string, vars?: Record<string, string | number>) => t("en", key, vars));
    expect(JSON.stringify(card)).not.toMatch(/\p{Script=Han}/u);
  });
});
