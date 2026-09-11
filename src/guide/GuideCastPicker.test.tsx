import { screen, fireEvent } from "@testing-library/react";
import { renderWithLocale as render } from "../i18n/testUtils";
import { describe, it, expect, vi } from "vitest";
import GuideCastPicker from "./GuideCastPicker";
import type { GuideCharacterSettings } from "../types";

describe("GuideCastPicker", () => {
  const settings: GuideCharacterSettings = {
    schemaVersion: 1,
    storeRevision: 1,
    characters: [
      {
        id: "available",
        revision: 1,
        displayName: "现有角色",
        workTitle: "测试作品",
        characterVersion: "",
        description: "",
        avatarAssetId: null,
        inkColor: "#123456",
        personality: "",
        readingHabits: "",
        expressionStyle: "",
        avoidances: "",
        exampleNotes: [],
        presetId: null,
        presetVersion: null,
        createdAt: "",
        updatedAt: "",
      },
      {
        id: "char2",
        revision: 1,
        displayName: "二号角色",
        workTitle: "",
        characterVersion: "",
        description: "",
        avatarAssetId: null,
        inkColor: "#654321",
        personality: "",
        readingHabits: "",
        expressionStyle: "",
        avoidances: "",
        exampleNotes: [],
        presetId: null,
        presetVersion: null,
        createdAt: "",
        updatedAt: "",
      },
    ],
    defaultCharacterIds: ["available"],
    presetCasts: [
      { id: "solo", name: "单人阵容", characterIds: ["available"] },
      { id: "duo", name: "双人阵容", characterIds: ["available", "char2"] },
    ],
  };

  it("shows unavailable saved characters without silently replacing them", () => {
    const onChange = vi.fn();
    render(
      <GuideCastPicker
        settings={settings}
        selectedIds={["deleted"]}
        onChange={onChange}
        onOpenSettings={vi.fn()}
      />,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("不可用角色：deleted");
    fireEvent.click(screen.getByRole("button", { name: "移除不可用角色" }));
    expect(onChange).toHaveBeenLastCalledWith([]);

    // Open dropdown to view checkboxes
    fireEvent.click(screen.getByRole("button", { name: "选择出场批注角色" }));
    expect(screen.getByRole("checkbox", { name: /现有角色/ })).not.toBeChecked();

    fireEvent.click(screen.getByRole("button", { name: "单人阵容" }));
    expect(onChange).toHaveBeenLastCalledWith(["available"]);
  });

  it("toggles dropdown and supports select all, clear, and individual toggle", () => {
    const onChange = vi.fn();
    render(
      <GuideCastPicker
        settings={settings}
        selectedIds={["available"]}
        onChange={onChange}
        onOpenSettings={vi.fn()}
      />,
    );

    const trigger = screen.getByRole("button", { name: "选择出场批注角色" });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByText("已选 1 位角色")).toBeDefined();

    // Open dropdown
    fireEvent.click(trigger);
    expect(trigger).toHaveAttribute("aria-expanded", "true");

    // Select all
    fireEvent.click(screen.getByRole("button", { name: "全选" }));
    expect(onChange).toHaveBeenCalledWith(["available", "char2"]);

    // Clear all
    fireEvent.click(screen.getByRole("button", { name: "清空" }));
    expect(onChange).toHaveBeenCalledWith([]);

    // Toggle char2
    fireEvent.click(screen.getByRole("checkbox", { name: /二号角色/ }));
    expect(onChange).toHaveBeenCalledWith(["available", "char2"]);
  });

  it("shows empty error hint when 0 characters selected", () => {
    render(
      <GuideCastPicker
        settings={settings}
        selectedIds={[]}
        onChange={vi.fn()}
        onOpenSettings={vi.fn()}
      />,
    );
    expect(screen.getByText("未选择出场角色")).toBeDefined();
    expect(screen.getByText(/请至少选择一位角色/)).toBeDefined();
  });
});
