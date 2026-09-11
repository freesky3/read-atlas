import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import PromptCatalogSection from "./PromptCatalogSection";
import { factoryPromptSettings } from "./promptCatalog";
import { renderWithLocale } from "./i18n/testUtils";

const factorySettings = factoryPromptSettings();

describe("PromptCatalogSection", () => {
  it("refuses to save an empty prompt or a translation without the language placeholder", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    renderWithLocale(
      <PromptCatalogSection
        settings={factorySettings}
        busy={false}
        onSave={onSave}
        onRestorePrevious={vi.fn()}
        onRestoreDefault={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("button", { name: /整体构图|抽单元|Extract/i }));
    await user.clear(screen.getByRole("textbox"));
    await user.click(screen.getByRole("button", { name: "保存" }));
    expect(onSave).not.toHaveBeenCalled();
    expect(screen.getByText(/不能为空|cannot be empty/i)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "翻译" }));
    await user.clear(screen.getByRole("textbox"));
    await user.type(screen.getByRole("textbox"), "Translate the block.");
    await user.click(screen.getByRole("button", { name: "保存" }));
    expect(onSave).not.toHaveBeenCalled();
    expect(screen.getByRole("alert")).toHaveTextContent(
      "此提示词必须包含 {output_language}",
    );
  });

  it("saves the selected slot and can restore default", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const onRestoreDefault = vi.fn();
    renderWithLocale(
      <PromptCatalogSection
        settings={factorySettings}
        busy={false}
        onSave={onSave}
        onRestorePrevious={vi.fn()}
        onRestoreDefault={onRestoreDefault}
      />,
    );

    await user.click(screen.getByRole("button", { name: "检查与定稿" }));
    await user.clear(screen.getByRole("textbox"));
    await user.type(screen.getByRole("textbox"), "custom compose");
    await user.click(screen.getByRole("button", { name: "保存" }));
    expect(onSave).toHaveBeenCalledWith(
      "outline_compose",
      "custom compose",
      "paper",
    );

    await user.click(screen.getByRole("button", { name: "恢复默认" }));
    expect(onRestoreDefault).toHaveBeenCalledWith("outline_compose", "paper");
  });

  it("saves the textbook slot independently of the paper slot", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    renderWithLocale(
      <PromptCatalogSection
        settings={factorySettings}
        busy={false}
        onSave={onSave}
        onRestorePrevious={vi.fn()}
        onRestoreDefault={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("tab", { name: "教材" }));
    await user.click(screen.getByRole("button", { name: "检查与定稿" }));
    await user.clear(screen.getByRole("textbox"));
    await user.type(screen.getByRole("textbox"), "textbook compose");
    await user.click(screen.getByRole("button", { name: "保存" }));
    expect(onSave).toHaveBeenCalledWith(
      "outline_compose",
      "textbook compose",
      "textbook",
    );
  });

  it("disables restore previous when no previous text exists", () => {
    renderWithLocale(
      <PromptCatalogSection
        settings={factorySettings}
        busy={false}
        onSave={vi.fn()}
        onRestorePrevious={vi.fn()}
        onRestoreDefault={vi.fn()}
      />,
    );
    expect(screen.getByRole("button", { name: "恢复上一次" })).toBeDisabled();
  });
  it("shows real legacy labels and upgrades only the selected document bundle", async () => {
    const user = userEvent.setup();
    const settings = factoryPromptSettings();
    for (const id of ["outline_extract", "outline_compose", "outline_deep_dive"]) {
      settings.slots[id].paper.outputProtocol = "v3";
    }
    const onRestoreOutlineBundle = vi.fn();
    renderWithLocale(<PromptCatalogSection settings={settings} busy={false} onSave={vi.fn()}
      onRestorePrevious={vi.fn()} onRestoreDefault={vi.fn()} onRestoreOutlineBundle={onRestoreOutlineBundle} />);
    expect(screen.getByRole("button", { name: "抽单元" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "构图" })).toBeInTheDocument();
    expect(screen.getByText(/当前整套使用旧版抽取流程/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "使用新版整套默认" }));
    expect(onRestoreOutlineBundle).toHaveBeenLastCalledWith("paper");
    await user.click(screen.getByRole("tab", { name: "教材" }));
    expect(screen.getByRole("button", { name: "整体构图" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "使用新版整套默认" }));
    expect(onRestoreOutlineBundle).toHaveBeenLastCalledWith("textbook");
  });

  it("explains mixed map protocols and preserves a declined unsaved draft", async () => {
    const user = userEvent.setup();
    const settings = factoryPromptSettings();
    settings.slots.outline_compose.paper.outputProtocol = "v3";
    const onRestoreOutlineBundle = vi.fn();
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    renderWithLocale(<PromptCatalogSection settings={settings} busy={false} onSave={vi.fn()}
      onRestorePrevious={vi.fn()} onRestoreDefault={vi.fn()} onRestoreOutlineBundle={onRestoreOutlineBundle} />);
    expect(screen.getByRole("alert")).toHaveTextContent("协议不一致");
    await user.type(screen.getByRole("textbox"), " important draft");
    await user.click(screen.getByRole("button", { name: "使用新版整套默认" }));
    expect(onRestoreOutlineBundle).not.toHaveBeenCalled();
    expect((screen.getByRole("textbox") as HTMLTextAreaElement).value).toContain("important draft");
    confirm.mockRestore();
  });

  it("does not present an unsupported future bundle as the current generation", () => {
    const settings = factoryPromptSettings();
    for (const slot of ["outline_extract", "outline_compose", "outline_deep_dive"]) {
      settings.slots[slot].paper.outputProtocol = "v9";
    }
    renderWithLocale(<PromptCatalogSection settings={settings} busy={false} onSave={vi.fn()}
      onRestorePrevious={vi.fn()} onRestoreDefault={vi.fn()} />);
    expect(screen.getByRole("alert")).toHaveTextContent("当前应用不支持的协议");
    expect(screen.queryByText(/当前整套使用新版整体构图流程/)).not.toBeInTheDocument();
  });

});
