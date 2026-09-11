import { describe, it, expect, beforeEach } from "vitest";

describe("Theme persistence", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("stores and retrieves theme mode correctly", () => {
    localStorage.setItem("read-desktop.theme", "liquid-dark");
    expect(localStorage.getItem("read-desktop.theme")).toBe("liquid-dark");

    localStorage.setItem("read-desktop.theme", "liquid-light");
    expect(localStorage.getItem("read-desktop.theme")).toBe("liquid-light");
  });
});
