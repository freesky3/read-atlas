import { describe, expect, it } from "vitest";
import { languageCode, metadataAbstractIndex } from "./language";
import { localizedCharacter, canonicalCharacterDraft } from "./guidePresets";
import original from "../../src-tauri/prompts/guide-characters/chitanda.json";
import type { GuideCharacter } from "../types";
import { captureOperationError, errorDiagnostics } from "./errors";
describe("locale content boundaries", () => {
  it("matches legacy and English language names without rewriting source data", () => {
    const zh = { languages: ["中文"], completeness: "complete", segments: [{ text: "历史中文摘要" }] };
    const en = { languages: ["English"], completeness: "complete", segments: [{ text: "Original English abstract" }] };
    const content = { document: { abstracts: [zh, en] } };
    const before = structuredClone(content);
    expect(metadataAbstractIndex(content, "en")).toBe(1);
    expect(metadataAbstractIndex(content, "zh-CN")).toBe(0);
    expect(metadataAbstractIndex({ ...content, _preferredLanguage: "英文" }, "zh-CN")).toBe(1);
    expect(metadataAbstractIndex({ ...content, _abstractSelection: zh }, "en")).toBe(0);
    expect(content).toEqual(before);
    expect(languageCode("英语")).toBe("en");
  });
  it("shows translated factory fields but saves untouched fields in their original form", () => {
    const raw = { ...original, personality: "用户自定义性格" } as unknown as GuideCharacter;
    const shown = localizedCharacter(raw, "en");
    expect(shown.displayName).toBe("Eru Chitanda");
    expect(shown.personality).toBe("用户自定义性格");
    expect(raw.displayName).toBe("千反田爱瑠");
    const edited = { ...shown, description: "My custom description" };
    expect(canonicalCharacterDraft(edited, raw, "en")).toMatchObject({ displayName: "千反田爱瑠", description: "My custom description", personality: "用户自定义性格" });
  });
  it("keeps structured error codes and diagnostic originals while localizing recovery guidance", () => {
    const error = captureOperationError("library_act", { code: "stale_selection", message: "backend diagnostic" }, "zh-CN");
    expect(error.code).toBe("stale_selection");
    expect(String(error)).toContain("重新选择");
    expect(errorDiagnostics().at(-1)?.detail).toBe("backend diagnostic");
    expect(Object.keys(error)).toEqual(["code", "message"]);
  });
});
