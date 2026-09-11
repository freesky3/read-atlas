import { describe, it, expect, vi } from "vitest";
import { screen, fireEvent } from "@testing-library/react";
import { renderWithLocale } from "../i18n/testUtils";
import React from "react";
import { ChatTimelineNavigator } from "./ChatTimelineNavigator";

describe("ChatTimelineNavigator", () => {
  const sampleMessages = [
    { id: "m1", role: "user", text: "What is the core idea of this paper?", createdAt: "2026-08-17T01:15:00Z" },
    { id: "m2", role: "assistant", text: "The paper proposes a low-rank RNN framework linking neural dynamics.", createdAt: "2026-08-17T01:16:00Z" },
    { id: "m3", role: "user", text: "How is the connection matrix constructed?", createdAt: "2026-08-17T01:22:00Z" },
    { id: "m4", role: "assistant", text: "The matrix J is given by the outer product sum.", createdAt: "2026-08-17T01:23:00Z" },
  ];

  it("extracts user turns and renders correct number of ticks", () => {
    renderWithLocale(<ChatTimelineNavigator messages={sampleMessages} onJumpToMessage={() => {}} />);
    const ticks = screen.getAllByRole("button", { name: /跳转至问题/i });
    expect(ticks).toHaveLength(2);
  });

  it("calls onJumpToMessage when clicking a tick", () => {
    const handleJump = vi.fn();
    renderWithLocale(<ChatTimelineNavigator messages={sampleMessages} onJumpToMessage={handleJump} />);
    const ticks = screen.getAllByRole("button", { name: /跳转至问题/i });
    fireEvent.click(ticks[1]);
    expect(handleJump).toHaveBeenCalledWith("m3");
  });

  it("displays preview card content on hover/render", () => {
    renderWithLocale(<ChatTimelineNavigator messages={sampleMessages} onJumpToMessage={() => {}} />);
    expect(screen.getByText("What is the core idea of this paper?")).toBeInTheDocument();
    expect(screen.getByText(/The paper proposes a low-rank RNN framework/)).toBeInTheDocument();
  });

  it("highlights the active tick when activeMessageId is set", () => {
    renderWithLocale(
      <ChatTimelineNavigator
        messages={sampleMessages}
        activeMessageId="m3"
        onJumpToMessage={() => {}}
      />
    );
    const ticks = screen.getAllByRole("button", { name: /跳转至问题/i });
    expect(ticks[0]).not.toHaveClass("active");
    expect(ticks[1]).toHaveClass("active");
  });

  it("renders nothing when there are no user messages", () => {
    const { container } = renderWithLocale(
      <ChatTimelineNavigator messages={[]} onJumpToMessage={() => {}} />
    );
    expect(container.firstChild).toBeNull();
  });
});
