import { describe, expect, it } from "vitest";
import { clampOutlineInspectorWidth } from "./outlineInspectorWidth";

describe("clampOutlineInspectorWidth", () => {
  it("clamps to the supported inspector range", () => {
    expect(clampOutlineInspectorWidth(120)).toBe(200);
    expect(clampOutlineInspectorWidth(500)).toBe(420);
    expect(clampOutlineInspectorWidth(Number.NaN)).toBe(280);
  });
});
