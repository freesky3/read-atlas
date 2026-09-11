import { describe, expect, it } from "vitest";
import {
  GUIDE_GUTTER_GAP,
  GUIDE_LEADER_ATTACH_Y,
  guideLeaderLine,
  layoutGuideMarginCards,
} from "./marginLayout";

describe("layoutGuideMarginCards", () => {
  it("keeps cards from overlapping and can extend the page", () => {
    const layout = layoutGuideMarginCards({
      pageHeight: 200,
      cards: [
        { id: "a", anchorX: 10, anchorY: 40, height: 120 },
        { id: "b", anchorX: 12, anchorY: 50, height: 120 },
      ],
      gap: 8,
      inset: 8,
    });
    expect(layout.placements).toHaveLength(2);
    const [first, second] = layout.placements;
    expect(second.top).toBeGreaterThanOrEqual(first.top + first.height + 8);
    expect(layout.contentHeight).toBeGreaterThan(200);
  });

  it("moves the card-side leader endpoint with gutter scroll", () => {
    const rested = guideLeaderLine({
      id: "n1",
      blockRightX: 500,
      blockCenterY: 120,
      pageWidth: 600,
      pageHeight: 800,
      cardTop: 200,
      cardHeight: 80,
      gutterScrollTop: 0,
    });
    const scrolled = guideLeaderLine({
      id: "n1",
      blockRightX: 500,
      blockCenterY: 120,
      pageWidth: 600,
      pageHeight: 800,
      cardTop: 200,
      cardHeight: 80,
      gutterScrollTop: 40,
    });
    expect(rested.y2).toBe(200 + GUIDE_LEADER_ATTACH_Y);
    expect(scrolled.y2).toBe(200 + GUIDE_LEADER_ATTACH_Y - 40);
    expect(scrolled.y1).toBe(120);
    expect(scrolled.x1).toBe(500);
    expect(scrolled.x2).toBe(600 + GUIDE_GUTTER_GAP + 2);
  });

  it("clamps the card-side endpoint to the page while the slot is scrolled away", () => {
    const above = guideLeaderLine({
      id: "n1",
      blockRightX: 500,
      blockCenterY: 120,
      pageWidth: 600,
      pageHeight: 800,
      cardTop: 10,
      cardHeight: 80,
      gutterScrollTop: 400,
    });
    const below = guideLeaderLine({
      id: "n1",
      blockRightX: 500,
      blockCenterY: 120,
      pageWidth: 600,
      pageHeight: 800,
      cardTop: 900,
      cardHeight: 80,
      gutterScrollTop: 0,
    });
    expect(above.y2).toBe(0);
    expect(below.y2).toBe(800);
  });
});
