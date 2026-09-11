import { describe, expect, it } from "vitest";
import {
  DEFAULT_DISCUSSION_FONT_SIZE,
  parseDiscussionFontSize,
  stepDiscussionFontSize,
} from "./discussionFont";

describe("discussion font size", () => {
  it("accepts only the three allowed sizes", () => {
    expect(parseDiscussionFontSize(14)).toBe(14);
    expect(parseDiscussionFontSize("17")).toBe(17);
    expect(parseDiscussionFontSize(16)).toBe(DEFAULT_DISCUSSION_FONT_SIZE);
  });

  it("steps between 14, 15 and 17", () => {
    expect(stepDiscussionFontSize(15, 1)).toBe(17);
    expect(stepDiscussionFontSize(17, 1)).toBe(17);
    expect(stepDiscussionFontSize(15, -1)).toBe(14);
    expect(stepDiscussionFontSize(14, -1)).toBe(14);
  });
});
