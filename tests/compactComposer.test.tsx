import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import React, { useState } from "react";
import {
  paperProviderComposerHint,
  paperProviderComposerPlaceholder,
} from "../src/modelLabel";
import type { PaperProvider } from "../src/types";

// Mock component simulating the compact composer behavior
function MockCompactComposer({
  isStreaming = false,
  onSend,
  onCancel,
  quoteCount = 0,
  modelName = "gemini-3.6-flash",
  providerConfigured = true,
  provider = "gemini",
}: {
  isStreaming?: boolean;
  onSend: (text: string) => void;
  onCancel: () => void;
  quoteCount?: number;
  modelName?: string;
  providerConfigured?: boolean;
  provider?: PaperProvider;
}) {
  const [text, setText] = useState("");

  return (
    <div className="composer-compact-wrap">
      <div className="composer-compact-capsule">
        <input
          type="text"
          className="composer-input-line"
          placeholder={
            providerConfigured
              ? "输入你的问题或追问... (Enter 发送)"
              : paperProviderComposerPlaceholder(provider)
          }
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              if (text.trim() && !isStreaming) {
                onSend(text);
                setText("");
              }
            }
          }}
        />
        {isStreaming ? (
          <button
            type="button"
            className="composer-action-btn stop"
            onClick={onCancel}
            title="中断回答 (Stop)"
            aria-label="中断回答"
          >
            ■
          </button>
        ) : (
          <button
            type="button"
            className="composer-action-btn send"
            disabled={!text.trim()}
            onClick={() => {
              if (text.trim()) {
                onSend(text);
                setText("");
              }
            }}
            title="发送 (Enter)"
            aria-label="发送问题"
          >
            ↑
          </button>
        )}
      </div>

      <div className="composer-subline">
        <div className="composer-subline-left">
          <span className="model-indicator-dot" />
          <span className="model-label-text">
            {providerConfigured
              ? modelName
              : paperProviderComposerHint(provider)}
          </span>
        </div>
        {quoteCount > 0 && (
          <div className="composer-subline-quotes" title="已引用 OCR 选区">
            📎 {quoteCount}
          </div>
        )}
      </div>
    </div>
  );
}

describe("Compact Composer and Interrupt Button", () => {
  it("renders input field and sends on Enter", () => {
    const handleSend = vi.fn();
    render(<MockCompactComposer onSend={handleSend} onCancel={() => {}} />);

    const input = screen.getByPlaceholderText(/输入你的问题或追问/i);
    expect(input).toBeInTheDocument();

    fireEvent.change(input, { target: { value: "How does theorem 2 work?" } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(handleSend).toHaveBeenCalledWith("How does theorem 2 work?");
  });

  it("displays interrupt stop button when streaming and triggers onCancel", () => {
    const handleCancel = vi.fn();
    render(<MockCompactComposer isStreaming={true} onSend={() => {}} onCancel={handleCancel} />);

    const stopButton = screen.getByRole("button", { name: /中断回答/i });
    expect(stopButton).toBeInTheDocument();

    fireEvent.click(stopButton);
    expect(handleCancel).toHaveBeenCalledTimes(1);
  });

  it("displays quoted count and model name in subline", () => {
    render(
      <MockCompactComposer
        quoteCount={3}
        modelName="gemini-3.6-flash"
        onSend={() => {}}
        onCancel={() => {}}
      />
    );

    expect(screen.getByText("gemini-3.6-flash")).toBeInTheDocument();
    expect(screen.getByTitle("已引用 OCR 选区")).toHaveTextContent("3");
  });

  it("shows current-provider empty copy when the paper provider is not configured", () => {
    render(
      <MockCompactComposer
        providerConfigured={false}
        provider="grok"
        onSend={() => {}}
        onCancel={() => {}}
      />,
    );

    expect(
      screen.getByPlaceholderText("Configure Grok to ask this paper…"),
    ).toBeInTheDocument();
    expect(screen.getByText("Grok not configured (click to configure)")).toBeInTheDocument();
  });
});
