import { describe, it, expect } from "vitest";
import fs from "fs";
import path from "path";

describe("Theme definitions", () => {
  it("defines valid theme identifiers", () => {
    const validThemes = ["liquid-light", "liquid-dark", "warm-editorial"] as const;
    expect(validThemes).toContain("liquid-light");
    expect(validThemes).toContain("liquid-dark");
    expect(validThemes).toContain("warm-editorial");
  });

  it("defines Claude Warm Editorial tokens in styles.css", () => {
    const css = fs.readFileSync(path.resolve(__dirname, "../src/styles.css"), "utf-8");
    expect(css).toContain('[data-theme="warm-editorial"]');
    expect(css).toContain("#c15f3e"); // Terracotta accent
    expect(css).toContain("#fbf9f5"); // Warm paper base
    expect(css).toContain("#fdfcf9"); // Ivory card surface
  });
});

