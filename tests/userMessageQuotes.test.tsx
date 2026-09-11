import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import React, { useState } from "react";
import type { BlockQuoteSnapshot, Message } from "../src/types";

// Mock component demonstrating the enhanced user message bubble
function MockUserBubble({
  message,
  onEdit,
  onBlockQuote,
}: {
  message: Message;
  onEdit: (message: Message) => void;
  onBlockQuote: (quote: BlockQuoteSnapshot) => void;
}) {
  const [enlargedQuote, setEnlargedQuote] = useState<BlockQuoteSnapshot | null>(null);

  const equationQuotes = message.blockQuotes.filter(
    (q) => q.blockType === "equation" || q.blockType === "formula" || q.textContent.startsWith("$$")
  );
  const figureQuotes = message.blockQuotes.filter(
    (q) => q.blockType === "figure" || q.blockType === "image" || q.blockType === "picture"
  );

  return (
    <article className="message user user-bubble-right" id={`msg-${message.id}`}>
      <div className="message-meta-compact">
        <span>01:15</span>
        <strong>You</strong>
      </div>

      {/* Figures / Images rendered directly above question */}
      {figureQuotes.length > 0 && (
        <div className="user-quoted-figures" data-testid="user-quoted-figures">
          {figureQuotes.map((quote) => (
            <div
              key={quote.blockId}
              className="user-quote-figure-thumbnail"
              onClick={() => setEnlargedQuote(quote)}
              title="点击放大查看图片"
              data-testid={`figure-thumb-${quote.blockId}`}
            >
              <div className="figure-placeholder-icon">🖼️</div>
              <span className="figure-caption-text">{quote.textContent || `Figure p.${quote.pageNumber}`}</span>
            </div>
          ))}
        </div>
      )}

      {/* Equations rendered directly above question */}
      {equationQuotes.length > 0 && (
        <div className="user-quoted-formulas" data-testid="user-quoted-formulas">
          {equationQuotes.map((quote) => (
            <div key={quote.blockId} className="user-quote-formula-card">
              <code>{quote.textContent}</code>
            </div>
          ))}
        </div>
      )}

      {/* Question Content */}
      <div className="message-content">
        <p>{message.content}</p>
      </div>

      {/* Quoted Block Pills rendered below question in smaller font */}
      {message.blockQuotes.length > 0 && (
        <div className="user-quoted-block-links" data-testid="user-quoted-block-links">
          {message.blockQuotes.map((quote) => (
            <button
              type="button"
              key={quote.blockId}
              className="user-quote-pill"
              onClick={() => onBlockQuote(quote)}
              title={`跳转至第 ${quote.pageNumber} 页`}
            >
              p.{quote.pageNumber} · {quote.blockType} #{quote.blockIndex + 1}
            </button>
          ))}
        </div>
      )}

      <div className="user-bubble-actions">
        <button
          type="button"
          className="user-edit-btn"
          onClick={() => onEdit(message)}
          title="编辑此问题"
          aria-label="编辑此问题"
        >
          ✎
        </button>
      </div>

      {/* Lightbox Modal */}
      {enlargedQuote && (
        <div className="lightbox-modal" data-testid="lightbox-modal" onClick={() => setEnlargedQuote(null)}>
          <div className="lightbox-content" onClick={(e) => e.stopPropagation()}>
            <button
              type="button"
              className="lightbox-close"
              onClick={() => setEnlargedQuote(null)}
              aria-label="关闭预览"
            >
              ×
            </button>
            <div className="lightbox-body">
              <p>{enlargedQuote.textContent}</p>
            </div>
          </div>
        </div>
      )}
    </article>
  );
}

describe("User Message Bubble with Quoted Blocks", () => {
  const sampleMessage: Message = {
    id: "msg-user-1",
    threadId: "t1",
    parentId: null,
    role: "user",
    content: "How does equation 3 connect to figure 1?",
    citations: [],
    blockQuotes: [
      {
        revisionId: "rev-1",
        ocrRevisionId: "ocr-1",
        blockId: "b-eq",
        pageNumber: 3,
        blockIndex: 2,
        blockType: "equation",
        textContent: "J_{ij} = \\sum \\kappa_\\mu u_i v_j",
        contentDigest: "d1",
        bbox: [100, 100, 200, 200],
      },
      {
        revisionId: "rev-1",
        ocrRevisionId: "ocr-1",
        blockId: "b-fig",
        pageNumber: 5,
        blockIndex: 0,
        blockType: "figure",
        textContent: "Figure 1: Recurrent network architecture",
        contentDigest: "d2",
        bbox: [150, 150, 350, 350],
      },
    ],
    status: "complete",
    createdAt: "2026-08-17T01:15:00Z",
  };

  it("renders equation and figure above question and block links below question", () => {
    const handleEdit = vi.fn();
    const handleBlockQuote = vi.fn();

    render(
      <MockUserBubble
        message={sampleMessage}
        onEdit={handleEdit}
        onBlockQuote={handleBlockQuote}
      />
    );

    // Formula above question
    expect(screen.getByTestId("user-quoted-formulas")).toBeInTheDocument();
    expect(screen.getByText(/J_\{ij\}/)).toBeInTheDocument();

    // Figure above question
    expect(screen.getByTestId("user-quoted-figures")).toBeInTheDocument();

    // Main question text
    expect(screen.getByText("How does equation 3 connect to figure 1?")).toBeInTheDocument();

    // Block links below question
    expect(screen.getByTestId("user-quoted-block-links")).toBeInTheDocument();
    expect(screen.getByText("p.3 · equation #3")).toBeInTheDocument();
    expect(screen.getByText("p.5 · figure #1")).toBeInTheDocument();

    // Click block pill triggers jump
    fireEvent.click(screen.getByText("p.3 · equation #3"));
    expect(handleBlockQuote).toHaveBeenCalledWith(sampleMessage.blockQuotes[0]);
  });

  it("opens lightbox modal when clicking figure thumbnail", () => {
    render(
      <MockUserBubble
        message={sampleMessage}
        onEdit={() => {}}
        onBlockQuote={() => {}}
      />
    );

    const figureThumb = screen.getByTestId("figure-thumb-b-fig");
    fireEvent.click(figureThumb);

    const modal = screen.getByTestId("lightbox-modal");
    expect(modal).toBeInTheDocument();
    expect(modal).toHaveTextContent("Figure 1: Recurrent network architecture");

    // Close modal
    fireEvent.click(screen.getByRole("button", { name: /关闭预览/i }));
    expect(screen.queryByTestId("lightbox-modal")).not.toBeInTheDocument();
  });

  it("triggers onEdit when clicking edit button", () => {
    const handleEdit = vi.fn();
    render(
      <MockUserBubble
        message={sampleMessage}
        onEdit={handleEdit}
        onBlockQuote={() => {}}
      />
    );

    fireEvent.click(screen.getByRole("button", { name: /编辑此问题/i }));
    expect(handleEdit).toHaveBeenCalledWith(sampleMessage);
  });
});
