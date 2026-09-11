import { describe, expect, it } from "vitest";
import { gridColumnCount, virtualWindow, HUB_VIRTUALIZE_AFTER } from "./virtualWindow";

describe("virtualWindow", () => {
  it("renders everything when the list is small or the viewport is unknown", () => {
    expect(virtualWindow({ count: 10, scrollTop: 0, viewportHeight: 600, itemHeight: 44 }).virtualized).toBe(false);
    expect(virtualWindow({ count: 200, scrollTop: 0, viewportHeight: 0, itemHeight: 44 }).end).toBe(200);
  });

  it("windows a tall list to the viewport plus overscan", () => {
    const window = virtualWindow({
      count: HUB_VIRTUALIZE_AFTER + 200,
      scrollTop: 440,
      viewportHeight: 220,
      itemHeight: 44,
      overscan: 2,
    });
    expect(window.virtualized).toBe(true);
    expect(window.start).toBeGreaterThan(0);
    expect(window.end - window.start).toBeLessThan(HUB_VIRTUALIZE_AFTER + 200);
    expect(window.paddingStart).toBe(window.start * 44);
  });

  it("keeps a dragged-range of grid cells aligned to columns", () => {
    expect(gridColumnCount(0)).toBe(1);
    expect(gridColumnCount(356)).toBe(1);
    expect(gridColumnCount(720)).toBe(2);
    const window = virtualWindow({
      count: 200,
      scrollTop: 196,
      viewportHeight: 400,
      itemHeight: 196,
      columns: 2,
      overscan: 1,
    });
    expect(window.start % 2).toBe(0);
  });

  it("keeps a focused index inside the window so keyboard navigation stays mounted", () => {
    const window = virtualWindow({
      count: 200,
      scrollTop: 0,
      viewportHeight: 220,
      itemHeight: 44,
      overscan: 1,
      ensureIndex: 80,
    });
    expect(window.start).toBeLessThanOrEqual(80);
    expect(window.end).toBeGreaterThan(80);
  });
});
