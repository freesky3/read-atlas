import { describe, it, expect, vi, beforeEach } from "vitest";
import { screen, fireEvent, waitFor } from "@testing-library/react";
import React from "react";
import { renderWithLocale as render } from "../src/i18n/testUtils";
import SettingsGuideCharactersPage from "../src/SettingsGuideCharactersPage";
import { desktopClient } from "../src/desktopClient";
import type { GuideCharacterSettings } from "../src/types";

const mockSettings: GuideCharacterSettings = {
  schemaVersion: 1,
  storeRevision: 1,
  defaultCharacterIds: ["preset:chitanda", "preset:jotaro"],
  characters: [
    {
      id: "preset:chitanda",
      revision: 1,
      displayName: "千反田爱瑠",
      workTitle: "冰菓",
      characterVersion: "高中时期",
      description: "真诚、细致、好奇；会对不起眼的异常认真停留。",
      avatarAssetId: null,
      inkColor: "#7653A6",
      personality: "真诚、细致、好奇。",
      readingHabits: "关注作者为何增加某项限制。",
      expressionStyle: "自然完整的短句，语气认真。",
      avoidances: "每条都写「我很好奇」。",
      exampleNotes: ["这里特意固定了样本量。"],
      presetId: "chitanda",
      presetVersion: 1,
      createdAt: "2026-01-01T00:00:00Z",
      updatedAt: "2026-01-01T00:00:00Z",
    },
    {
      id: "preset:jotaro",
      revision: 1,
      displayName: "空条承太郎",
      workTitle: "JOJO的奇妙冒险",
      characterVersion: "第四部",
      description: "冷静、寡言、观察准确；关键位置判断直接。",
      avatarAssetId: "asset-jotaro-avatar",
      inkColor: "#2E4C7A",
      personality: "冷静、寡言、观察准确。",
      readingHabits: "抓住决定性的事实。",
      expressionStyle: "短、稳、有分量。",
      avoidances: "刻意加入战斗台词。",
      exampleNotes: ["条件别漏: 这里保持不变的是参数量，不是计算量。"],
      presetId: "jotaro",
      presetVersion: 1,
      createdAt: "2026-01-01T00:00:00Z",
      updatedAt: "2026-01-01T00:00:00Z",
    },
  ],
  presetCasts: [{ id: "duo", name: "双人试验阵容", characterIds: ["preset:chitanda", "preset:jotaro"] }],
};

describe("SettingsGuideCharactersPage - Persona Dossier", () => {
  beforeEach(() => {
    vi.restoreAllMocks();

    vi.spyOn(desktopClient, "open").mockImplementation(async (projection: string) => {
      if (projection === "get_guide_character_settings") {
        return mockSettings as any;
      }
      return null as any;
    });

    vi.spyOn(desktopClient, "command").mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_guide_character_avatar") {
        const assetId = args?.request?.assetId;
        if (assetId === "asset-jotaro-avatar") {
          return {
            assetId,
            mime: "image/png",
            dataUrl: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
          } as any;
        }
      }
      return mockSettings as any;
    });
  });

  it("renders the persona dossier layout and directory", async () => {
    render(<SettingsGuideCharactersPage />);

    // Wait for data load
    await waitFor(() => {
      expect(screen.getByText("2 位默认阵容")).toBeInTheDocument();
    });

    // Verify left list contains character names
    expect(screen.getAllByText("千反田爱瑠").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("空条承太郎").length).toBeGreaterThanOrEqual(1);

    // Verify dossier sections 01-05
    expect(screen.getByText("基本卷宗档案")).toBeInTheDocument();
    expect(screen.getByText("阅读器页边旁批仿真预览")).toBeInTheDocument();
    expect(screen.getByText("认知心智与阅读视角")).toBeInTheDocument();
    expect(screen.getByText("表达风格与行为边界")).toBeInTheDocument();
    expect(screen.getByText("典范旁批样例")).toBeInTheDocument();
  });

  it("loads and displays character avatar when switching characters", async () => {
    render(<SettingsGuideCharactersPage />);

    await waitFor(() => {
      expect(screen.getByText("2 位默认阵容")).toBeInTheDocument();
    });

    // Switch to 空条承太郎
    const jotaroButton = screen.getAllByText("空条承太郎")[0];
    fireEvent.click(jotaroButton);

    // Verify Jotaro's dossier hero is loaded
    await waitFor(() => {
      expect(screen.getByText("《JOJO的奇妙冒险》")).toBeInTheDocument();
    });

    // Verify avatar query was called for asset-jotaro-avatar
    expect(desktopClient.command).toHaveBeenCalledWith(
      "get_guide_character_avatar",
      expect.objectContaining({ request: { assetId: "asset-jotaro-avatar" } }),
    );

    // Wait for the img with the loaded avatar dataUrl
    await waitFor(() => {
      const images = screen.getAllByRole("img");
      const hasAvatar = images.some((img) =>
        img.getAttribute("src")?.startsWith("data:image/png;base64"),
      );
      expect(hasAvatar).toBe(true);
    });
  });

  it("updates live simulation preview card when typing example notes", async () => {
    render(<SettingsGuideCharactersPage />);

    await waitFor(() => {
      expect(screen.getByText("2 位默认阵容")).toBeInTheDocument();
    });

    // Find sample notes textarea (section 05)
    const textareas = screen.getAllByRole("textbox");
    const notesInput = textareas.find((input) =>
      input.getAttribute("placeholder")?.includes("示例旁批"),
    );
    expect(notesInput).toBeDefined();

    if (notesInput) {
      fireEvent.change(notesInput, {
        target: { value: "新测试旁批：这里的关键推导步骤被省略了。" },
      });

      // Simulation box and textarea should both reflect the new text
      expect(
        screen.getAllByText("新测试旁批：这里的关键推导步骤被省略了。").length,
      ).toBe(2);
    }
  });

  it("keeps unsaved character draft when toggling default cast", async () => {
    render(<SettingsGuideCharactersPage />);
    await waitFor(() => {
      expect(screen.getByText("2 位默认阵容")).toBeInTheDocument();
    });
    const personality = screen.getByDisplayValue("真诚、细致、好奇。");
    fireEvent.change(personality, { target: { value: "未保存的性格草稿" } });
    fireEvent.click(screen.getByTitle("切换是否作为阅读时的默认批注阵容"));
    await waitFor(() => {
      expect(desktopClient.command).toHaveBeenCalledWith(
        "save_guide_default_cast",
        expect.objectContaining({
          request: expect.objectContaining({
            expectedStoreRevision: 1,
          }),
        }),
      );
    });
    expect(screen.getByDisplayValue("未保存的性格草稿")).toBeInTheDocument();
  });

  it("uses the same unsaved-draft guard for keyboard switching", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    render(<SettingsGuideCharactersPage />);
    const field = await screen.findByDisplayValue("真诚、细致、好奇。");
    fireEvent.change(field, { target: { value: "保留我的键盘草稿" } });
    const row = screen.getAllByText("空条承太郎").map((item) => item.closest(".dossier-character-item")).find(Boolean)!;
    fireEvent.keyDown(row, { key: "Enter" });
    expect(confirm).toHaveBeenCalledOnce();
    expect(screen.getByDisplayValue("保留我的键盘草稿")).toBeInTheDocument();
  });

  it("applies a preset without overwriting a dirty character", async () => {
    render(<SettingsGuideCharactersPage />);
    fireEvent.change(await screen.findByDisplayValue("真诚、细致、好奇。"), { target: { value: "预设选择保留草稿" } });
    fireEvent.click(screen.getByRole("button", { name: "双人试验阵容" }));
    await waitFor(() => expect(desktopClient.command).toHaveBeenCalledWith("save_guide_default_cast", {
      request: { expectedStoreRevision: 1, characterIds: ["preset:chitanda", "preset:jotaro"] },
    }));
    expect(screen.getByDisplayValue("预设选择保留草稿")).toBeInTheDocument();
  });

  it("only previews on request, includes unsaved draft and comparison, and adopts explicitly", async () => {
    const invoke = vi.mocked(desktopClient.command).getMockImplementation()!;
    vi.mocked(desktopClient.command).mockImplementation(async (command, args: any) => {
      if (command === "preview_guide_character") return {
        inks: [{ id: "n1", kind: "note", speakerId: "preset:chitanda", weight: "line", body: "本次试写的具体发现", anchor: { pageNumber: 1, blockId: "preview-1", blockType: "Text", bbox: [0,0,1,1] } }],
        usage: { model: "fake-preview", estimatedCost: null, inputTokens: 123, outputTokens: 45 },
      } as any;
      return invoke(command, args);
    });
    render(<SettingsGuideCharactersPage />);
    fireEvent.change(await screen.findByDisplayValue("真诚、细致、好奇。"), { target: { value: "未保存的试写性格" } });
    expect(desktopClient.command).not.toHaveBeenCalledWith("preview_guide_character", expect.anything());
    fireEvent.click(screen.getByRole("checkbox", { name: "空条承太郎" }));
    fireEvent.click(screen.getByRole("button", { name: "试写旁批" }));
    expect(await screen.findByText("本次试写的具体发现")).toBeInTheDocument();
    const call = vi.mocked(desktopClient.command).mock.calls.find(([command]) => command === "preview_guide_character")!;
    expect(call[1]).toEqual({ request: expect.objectContaining({
      requestId: expect.any(String), documentKind: "textbook",
      drafts: [expect.objectContaining({ personality: "未保存的试写性格" }), expect.objectContaining({ id: "preset:jotaro" })],
    }) });
    expect(screen.getByText(/费用：未知/)).toBeInTheDocument();
    expect(screen.queryByDisplayValue(/本次试写的具体发现/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "加入示例旁批" }));
    expect(screen.getByDisplayValue(/本次试写的具体发现/)).toBeInTheDocument();
    expect(desktopClient.command).not.toHaveBeenCalledWith("save_guide_character", expect.anything());
  });

  it("cancels a pending preview and ignores its late response", async () => {
    let resolvePreview!: (value: unknown) => void;
    const invoke = vi.mocked(desktopClient.command).getMockImplementation()!;
    vi.mocked(desktopClient.command).mockImplementation((command, args: any) => {
      if (command === "preview_guide_character") return new Promise((resolve) => { resolvePreview = resolve; }) as any;
      if (command === "cancel_guide_character_preview") return Promise.resolve({ cancelled: true }) as any;
      return invoke(command, args);
    });
    render(<SettingsGuideCharactersPage />);
    await screen.findByDisplayValue("真诚、细致、好奇。");
    fireEvent.click(screen.getByRole("button", { name: "试写旁批" }));
    fireEvent.click(screen.getByRole("button", { name: "取消试写" }));
    await screen.findByText(/试写已取消/);
    const start = vi.mocked(desktopClient.command).mock.calls.find(([command]) => command === "preview_guide_character")![1] as any;
    expect(desktopClient.command).toHaveBeenCalledWith("cancel_guide_character_preview", { request: { requestId: start.request.requestId } });
    resolvePreview({ inks: [], usage: { model: "late" } });
    await waitFor(() => expect(screen.queryByText(/本次模型：late/)).not.toBeInTheDocument());
  });

  it("opens lineup manager modal, creates a custom lineup, and triggers save_guide_preset_casts", async () => {
    render(<SettingsGuideCharactersPage />);
    await screen.findByDisplayValue("真诚、细致、好奇。");

    // Open lineup manager
    fireEvent.click(screen.getByRole("button", { name: "⚙️ 管理阵容" }));
    expect(screen.getByText("⚙️ 批注阵容组合管理")).toBeInTheDocument();
    expect(screen.getAllByText("双人试验阵容").length).toBeGreaterThanOrEqual(2);

    // Create a new custom lineup
    const nameInput = screen.getByPlaceholderText(/新阵容名称/);
    fireEvent.change(nameInput, { target: { value: "论文答辩组" } });
    fireEvent.click(screen.getByRole("button", { name: "＋ 创建阵容" }));

    expect(desktopClient.command).toHaveBeenCalledWith(
      "save_guide_preset_casts",
      expect.objectContaining({
        request: expect.objectContaining({
          presetCasts: expect.arrayContaining([
            expect.objectContaining({ name: "论文答辩组" }),
          ]),
        }),
      }),
    );
  });
});
