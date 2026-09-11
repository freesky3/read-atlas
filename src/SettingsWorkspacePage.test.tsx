import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { useState } from "react";
import { desktopClient } from "./desktopClient";
import { renderWithLocale } from "./i18n/testUtils";
import { LanguageChangeConfirmModal } from "./components/LanguageChangeConfirmModal";
import { SettingsWorkspacePage } from "./SettingsWorkspacePage";
import type { UiLocale } from "./i18n/types";
import type { WorkspaceInfo } from "./types";

const workspace: WorkspaceInfo = {
  rootPath: "D:\\papers",
  databasePath: "D:\\papers\\.read-desktop\\read-desktop.sqlite3",
  libraryPath: "D:\\papers\\Papers",
  textbooksPath: "D:\\papers\\Textbooks",
  available: true,
  statusDetail: "ready",
};

const pageProps = {
  workspace,
  storageReport: null,
  workspaceBusy: false,
  latinFont: "default" as const,
  chineseFont: "default" as const,
  onLatinFontChange: vi.fn(),
  onChineseFontChange: vi.fn(),
  onResetWorkspaceClick: vi.fn(),
  setStatus: vi.fn(),
};

function LocaleConfirmHarness({
  onCommand = desktopClient.command.bind(desktopClient),
}: {
  onCommand?: typeof desktopClient.command;
}) {
  const [locale, setLocale] = useState<UiLocale>("zh-CN");
  const [pending, setPending] = useState<UiLocale | null>(null);
  return (
    <>
      <SettingsWorkspacePage
        {...pageProps}
        locale={locale}
        onRequestLocaleChange={setPending}
      />
      <LanguageChangeConfirmModal
        open={Boolean(pending)}
        next={pending ?? "en"}
        onConfirm={() => {
          void (async () => {
            if (!pending) return;
            await onCommand("set_ui_locale", { locale: pending });
            setLocale(pending);
            setPending(null);
          })();
        }}
        onCancel={() => setPending(null)}
      />
    </>
  );
}

describe("SettingsWorkspacePage language", () => {
  it("does not request a change when the active pill is clicked", async () => {
    const user = userEvent.setup();
    const onRequestLocaleChange = vi.fn();
    renderWithLocale(
      <SettingsWorkspacePage
        {...pageProps}
        locale="zh-CN"
        onRequestLocaleChange={onRequestLocaleChange}
      />,
    );
    await user.click(screen.getByRole("button", { name: "中文" }));
    expect(onRequestLocaleChange).not.toHaveBeenCalled();
  });

  it("requests a change when the inactive pill is clicked", async () => {
    const user = userEvent.setup();
    const onRequestLocaleChange = vi.fn();
    renderWithLocale(
      <SettingsWorkspacePage
        {...pageProps}
        locale="zh-CN"
        onRequestLocaleChange={onRequestLocaleChange}
      />,
    );
    await user.click(screen.getByRole("button", { name: "English" }));
    expect(onRequestLocaleChange).toHaveBeenCalledWith("en");
  });

  it("invokes set_ui_locale on confirm and not on cancel", async () => {
    const user = userEvent.setup();
    const onCommand = vi.fn().mockResolvedValue({ locale: "en" });
    renderWithLocale(<LocaleConfirmHarness onCommand={onCommand} />);

    await user.click(screen.getByRole("button", { name: "English" }));
    expect(screen.getByRole("dialog", { name: "切换语言？" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "取消" }));
    expect(onCommand).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog", { name: "切换语言？" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "English" }));
    await user.click(screen.getByRole("button", { name: "切换到 English" }));
    expect(onCommand).toHaveBeenCalledWith("set_ui_locale", { locale: "en" });
  });
});
