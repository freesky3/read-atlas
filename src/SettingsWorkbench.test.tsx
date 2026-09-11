import { useState } from "react";
import { screen, waitFor, within, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { desktopClient, createMemoryDesktopClient } from "./desktopClient";
import SettingsWorkbench from "./SettingsWorkbench";
import type { ModelSettings, ProviderInstanceView, WorkspaceInfo } from "./types";
import { renderWithLocale } from "./i18n/testUtils";

const mockProvider1: ProviderInstanceView = {
  id: "p-gemini-1",
  name: "Google Gemini",
  kind: "gemini",
  baseUrl: null,
  paperModel: "gemini-2.5-flash",
  translationModel: "gemini-2.5-flash-lite",
  models: [
    {
      id: "gemini-2.5-flash",
      displayName: "Gemini 2.5 Flash",
      description: "Fast multimodal model",
      inputTokenLimit: 1048576,
      outputTokenLimit: 65536,
      supportsGenerateContent: true,
      supportsNativePdf: true,
      supportsInteractions: true,
    },
  ],
  modelsFetchedAt: "2026-08-18T00:00:00.000Z",
  connectionVerifiedAt: "2026-08-18T00:00:00.000Z",
  paperProbe: null,
  credentialConfigured: true,
  paperProbePassed: true,
  isCurrent: true,
  sortOrder: 1,
};

const mockProvider2: ProviderInstanceView = {
  id: "p-openai-1",
  name: "DeepSeek 官方",
  kind: "openai_compatible",
  baseUrl: "https://api.deepseek.com/v1",
  paperModel: "deepseek-chat",
  translationModel: "deepseek-chat",
  models: [],
  modelsFetchedAt: null,
  connectionVerifiedAt: null,
  paperProbe: null,
  credentialConfigured: false,
  paperProbePassed: false,
  isCurrent: false,
  sortOrder: 2,
};

const modelSettings: ModelSettings = {
  currentProviderId: "p-gemini-1",
  providers: [mockProvider1, mockProvider2],
  credentialStore: "Windows Credential Manager",
  mistralCredentialConfigured: false,
  mistralCredentialStore: "Windows Credential Manager",
  ocrModel: "mistral-ocr-latest",
};

const mockWorkspace: WorkspaceInfo = {
  rootPath: "D:\\papers",
  databasePath: "D:\\papers\\.read-desktop\\read-desktop.sqlite3",
  libraryPath: "D:\\papers\\Papers",
  textbooksPath: "D:\\papers\\Textbooks",
  available: true,
  statusDetail: "ready",
};

function SettingsHarness({
  initialSettings = modelSettings,
  initialSection = "models",
  onResetWorkspace = vi.fn().mockResolvedValue(undefined),
  initialProviderId,
  focusNonce,
}: {
  initialSettings?: ModelSettings;
  initialSection?: "models" | "workspace" | "prompts";
  onResetWorkspace?: (previewDigest: string) => Promise<void>;
  initialProviderId?: string | null;
  focusNonce?: number;
}) {
  const [settings, setSettings] = useState(initialSettings);
  return (
    <SettingsWorkbench
      open
      initialSection={initialSection}
      initialProviderId={initialProviderId}
      focusNonce={focusNonce}
      workspace={mockWorkspace}
      modelSettings={settings}
      workspaceBusy={false}
      onClose={vi.fn()}
      onChooseWorkspace={vi.fn()}
      onResetWorkspace={onResetWorkspace}
      onOpenWorkspace={vi.fn()}
      onModelSettingsChange={setSettings}
      locale="zh-CN"
      onRequestLocaleChange={vi.fn()}
    />
  );
}

describe("SettingsWorkbench Suite", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("renders tab rail with provider instances and switches between tabs", async () => {
    const user = userEvent.setup();
    renderWithLocale(<SettingsHarness initialSection="models" />);

    expect(screen.getAllByText("Google Gemini").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("DeepSeek 官方").length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("✓ 正在使用中")).toBeInTheDocument();

    // Switch to Workspace tab
    await user.click(screen.getByRole("button", { name: /工作区与外观/i }));
    expect(screen.getByText("阅读排版与字体风格")).toBeInTheDocument();
    expect(screen.getByText("工作区与本地存储")).toBeInTheDocument();

    // Switch to Prompts tab
    await user.click(screen.getByRole("button", { name: /提示词模板/i }));
    expect(await screen.findByRole("button", { name: "整体构图" })).toBeInTheDocument();
  });

  it("allows adding a new provider instance", async () => {
    const user = userEvent.setup();
    const newInst: ProviderInstanceView = {
      id: "p-grok-1",
      name: "xAI Grok",
      kind: "grok",
      baseUrl: "https://api.x.ai/v1",
      paperModel: "",
      translationModel: "",
      models: [],
      modelsFetchedAt: null,
      connectionVerifiedAt: null,
      paperProbe: null,
      credentialConfigured: false,
      paperProbePassed: false,
      isCurrent: false,
      sortOrder: 3,
    };

    vi.spyOn(desktopClient, "command").mockImplementation(async (command, args: any) => {
      if (command === "add_provider") {
        return {
          ...modelSettings,
          providers: [...modelSettings.providers, newInst],
        };
      }
      if (command === "set_current_provider") {
        return {
          ...modelSettings,
          currentProviderId: args?.request?.id ?? newInst.id,
          providers: [...modelSettings.providers, { ...newInst, isCurrent: true }],
        };
      }
      throw new Error(`Unexpected command ${command}`);
    });

    renderWithLocale(<SettingsHarness initialSection="models" />);

    await user.click(screen.getByRole("button", { name: "+ 添加 Provider" }));

    expect(desktopClient.command).toHaveBeenCalledWith(
      "add_provider",
      expect.objectContaining({
        request: expect.objectContaining({ name: expect.any(String) }),
      }),
    );
    expect((await screen.findAllByText("xAI Grok")).length).toBeGreaterThanOrEqual(1);
  });

  it("allows setting a different provider as current", async () => {
    const user = userEvent.setup();
    vi.spyOn(desktopClient, "command").mockImplementation(async (command) => {
      if (command === "set_current_provider") {
        return {
          ...modelSettings,
          currentProviderId: "p-openai-1",
          providers: modelSettings.providers.map((p) => ({
            ...p,
            isCurrent: p.id === "p-openai-1",
          })),
        };
      }
      throw new Error(`Unexpected command ${command}`);
    });

    renderWithLocale(<SettingsHarness initialSection="models" />);

    // Click on DeepSeek tab
    const tabs = screen.getAllByRole("tab");
    const deepseekTab = tabs.find((t) => t.textContent?.includes("DeepSeek 官方"));
    expect(deepseekTab).toBeDefined();
    await user.click(deepseekTab!);

    expect(screen.getByRole("button", { name: "设为当前使用" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "设为当前使用" }));
    expect(desktopClient.command).toHaveBeenCalledWith(
      "set_current_provider",
      { request: { id: "p-openai-1" } },
    );
  });

  it("allows switching font families independently", async () => {
    const user = userEvent.setup();
    renderWithLocale(<SettingsHarness initialSection="workspace" />);

    expect(screen.getByText(/西文 \/ 英文排版字体/i)).toBeInTheDocument();
    expect(screen.getByText(/中文阅读排版字体/i)).toBeInTheDocument();

    const consolasCard = screen.getByRole("radio", { name: /西文字体 Consolas/i });
    await user.click(consolasCard);
    expect(localStorage.getItem("read-desktop.latinFont")).toBe("consolas");

    const kaitiCard = screen.getByRole("radio", { name: /中文字体 霞鹜文楷/i });
    await user.click(kaitiCard);
    expect(localStorage.getItem("read-desktop.chineseFont")).toBe("kaiti");
  });

  it("requires typing RESET before executing danger workspace reset", async () => {
    const user = userEvent.setup();
    const onReset = vi.fn().mockResolvedValue(undefined);
    vi.spyOn(desktopClient, "command").mockImplementation(async (command) => {
      if (command === "inspect_legacy_reset") {
        return {
          fileCount: 2,
          pdfCount: 1,
          totalBytes: 2048,
          backupPath: "D:\\papers\\.read-desktop-backups\\reset-test",
          legacyDatabasePath: "D:\\papers\\workspace.sqlite3",
          legacyLibraryPath: "D:\\papers\\library",
          warnings: [],
          canProceed: true,
          previewDigest: "digest-test",
        };
      }
      throw new Error(`Unexpected command ${command}`);
    });
    renderWithLocale(<SettingsHarness initialSection="workspace" onResetWorkspace={onReset} />);

    await user.click(screen.getByRole("button", { name: "重置工作区数据" }));
    expect(screen.getByText("⚠️ 重置工作区数据确认")).toBeInTheDocument();

    const confirmBtn = screen.getByRole("button", { name: "确认重置工作区" });
    expect(confirmBtn).toBeDisabled();

    const input = screen.getByPlaceholderText("RESET");
    await user.type(input, "RESET");
    await waitFor(() => expect(confirmBtn).not.toBeDisabled());

    await user.click(confirmBtn);
    await waitFor(() =>
      expect(onReset).toHaveBeenCalledWith("digest-test"),
    );
  });

  it("restores a persisted custom Gemini model when no probed models were saved", () => {
    const customModel = "gemini-private-reading-model";
    renderWithLocale(
      <SettingsHarness
        initialSection="models"
        initialSettings={{
          ...modelSettings,
          providers: [
            {
              ...mockProvider1,
              paperModel: customModel,
              models: [],
            },
          ],
        }}
      />,
    );

    expect(screen.getByDisplayValue(customModel)).toHaveAttribute(
      "placeholder",
      expect.stringContaining("输入模型 ID"),
    );
  });
});

  it("forwards a Provider focus request once without overriding manual selection", async () => {
    const user = userEvent.setup();
    const { rerender } = renderWithLocale(
      <SettingsHarness initialProviderId="p-openai-1" focusNonce={1} />,
    );

    expect(await screen.findByRole("button", { name: "\u8bbe\u4e3a\u5f53\u524d\u4f7f\u7528" })).toBeInTheDocument();
    const geminiTab = screen.getAllByRole("tab").find((tab) =>
      tab.textContent?.includes("Google Gemini"),
    );
    expect(geminiTab).toBeDefined();
    await user.click(geminiTab!);
    expect(screen.getByText("\u2713 \u6b63\u5728\u4f7f\u7528\u4e2d")).toBeInTheDocument();

    rerender(
      <SettingsHarness initialProviderId="p-openai-1" focusNonce={1} />,
    );
    expect(screen.queryByRole("button", { name: "\u8bbe\u4e3a\u5f53\u524d\u4f7f\u7528" })).not.toBeInTheDocument();
  });



it("protects an unsaved character draft when leaving by navigation, back button or Escape", async () => {
  const settings = await createMemoryDesktopClient().open("get_guide_character_settings");
  const onClose = vi.fn();
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
  vi.spyOn(desktopClient, "open").mockImplementation(async (projection) =>
    projection === "get_guide_character_settings" ? settings as any : null as any);
  renderWithLocale(<SettingsWorkbench open initialSection="characters" workspace={mockWorkspace}
    modelSettings={modelSettings} workspaceBusy={false} onClose={onClose}
    onChooseWorkspace={vi.fn()} onResetWorkspace={vi.fn()} onOpenWorkspace={vi.fn()} onModelSettingsChange={vi.fn()}
    locale="zh-CN" onRequestLocaleChange={vi.fn()} />);
  const name = await screen.findByDisplayValue("千反田爱瑠");
  fireEvent.change(name, { target: { value: "尚未保存的人物名" } });
  fireEvent.click(screen.getByRole("button", { name: /返回阅读/ }));
  fireEvent.keyDown(document, { key: "Escape" });
  fireEvent.click(screen.getByRole("button", { name: /AI 模型与 Provider/ }));
  expect(confirm).toHaveBeenCalledTimes(3);
  expect(onClose).not.toHaveBeenCalled();
  expect(screen.getByDisplayValue("尚未保存的人物名")).toBeInTheDocument();
  confirm.mockReturnValue(true);
  fireEvent.click(screen.getByRole("button", { name: /返回阅读/ }));
  expect(onClose).toHaveBeenCalledOnce();
  vi.restoreAllMocks();
});
