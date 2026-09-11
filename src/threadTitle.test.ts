import { describe, expect, it } from "vitest";
import {
  isPlaceholderThreadTitle,
  titleFromFirstQuestion,
} from "./threadTitle";

describe("thread titles", () => {
  it("treats factory names as untitled placeholders", () => {
    expect(isPlaceholderThreadTitle("新对话")).toBe(true);
    expect(isPlaceholderThreadTitle("New exploration")).toBe(true);
    expect(isPlaceholderThreadTitle("Main discussion")).toBe(true);
    expect(isPlaceholderThreadTitle("What is attention?")).toBe(false);
  });

  it("uses the first question line as the discussion title", () => {
    expect(titleFromFirstQuestion("  What is attention?\nMore detail")).toBe(
      "What is attention?",
    );
    expect(titleFromFirstQuestion("one   two\tthree")).toBe("one two three");
    expect(titleFromFirstQuestion("   \n")).toBe("新对话");
    expect(titleFromFirstQuestion("x".repeat(40))).toBe(`${"x".repeat(32)}…`);
  });
});
