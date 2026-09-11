import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { LanguageGate } from "./LanguageGate";

describe("LanguageGate", () => {
  it("renders Chinese and English choices", () => {
    render(
      <LanguageGate onPick={vi.fn()} error={null} busy={false} />,
    );
    expect(screen.getByRole("button", { name: "中文" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "English" })).toBeInTheDocument();
  });

  it("does not dismiss on Escape or scrim click", () => {
    const onPick = vi.fn();
    render(<LanguageGate onPick={onPick} error={null} busy={false} />);
    fireEvent.keyDown(window, { key: "Escape" });
    fireEvent.click(screen.getByRole("dialog"));
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(onPick).not.toHaveBeenCalled();
  });
});
