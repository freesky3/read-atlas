import { describe, expect, it } from "vitest";
import { en } from "./messages/en";
import { zhCN } from "./messages/zh-CN";
import { flatten, t } from "./t";

describe("i18n catalogs", () => {
  it("has identical flattened keys", () => {
    expect(flatten(zhCN).sort()).toEqual(flatten(en).sort());
  });

  it("substitutes vars after lookup", () => {
    expect(
      t("en", "prompts.validate.outputLanguage"),
    ).toBe("This prompt must contain {output_language}");
    expect(t("zh-CN", "hub.settings")).toBe("⚙ 设置");
  });
});
