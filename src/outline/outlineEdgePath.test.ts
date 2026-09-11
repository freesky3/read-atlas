import { describe, expect, it } from "vitest";
import { outlineEdgePath } from "./outlineEdgePath";

const endpoints = { sourceX: 100, sourceY: 200, targetX: 100, targetY: 500, selfLoop: false, sourceBeforeTarget: true };
describe("free relation routes", () => {
  it("keeps parallel labels apart and reverse relations on distinct physical lanes", () => {
    const left = outlineEdgePath({ ...endpoints, lane: -0.5 });
    const right = outlineEdgePath({ ...endpoints, lane: 0.5 });
    expect(Math.abs(left.labelX - right.labelX)).toBeGreaterThan(180);
    const reverse = outlineEdgePath({ ...endpoints, sourceY: 500, targetY: 200, sourceBeforeTarget: false, lane: 0.5 });
    expect(reverse.labelX).toBe(right.labelX);
    expect(reverse.path).not.toBe(left.path);
  });
  it("preserves and separates multiple self-loops", () => {
    const first = outlineEdgePath({ ...endpoints, selfLoop: true, lane: 0 });
    const second = outlineEdgePath({ ...endpoints, selfLoop: true, lane: 1 });
    expect(first.path).not.toBe(second.path);
    expect(second.labelX - first.labelX).toBeGreaterThan(180);
    expect(first.path).not.toContain("NaN");
  });
});
