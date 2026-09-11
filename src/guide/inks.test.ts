import { describe, expect, it } from "vitest";
import {
  firstGuideAnchor,
  firstGuideNote,
  guideNotes,
  locateGuideInks,
  nextNotePage,
  normalizeGuideInks,
  notesOnPage,
} from "./inks";

describe("normalizeGuideInks", () => {
  it("accepts wire shapes that used to vanish in the reader", () => {
    const inks = normalizeGuideInks([
      {
        kind: "Note",
        speakerId: "alin",
        text: "Hold the manifold.",
        evidenceIds: ["block-1"],
        pageNumber: "2",
      },
      {
        Note: {
          id: "n2",
          speakerId: "xiaxia",
          body: "Like a folded map.",
          anchor: { blockId: "block-2", pageNumber: 3, bbox: [1, 2, 3, 4] },
        },
      },
    ]);
    expect(inks).toHaveLength(2);
    expect(notesOnPage(inks, 2)).toHaveLength(1);
    expect(firstGuideNote(inks)?.anchor.pageNumber).toBe(2);
    expect(nextNotePage(inks, 2)).toBe(3);
  });

  it("does not drop notes when kind is omitted or the payload is wrapped", () => {
    const inks = normalizeGuideInks({
      inks: [
        {
          speaker_id: "laozhou",
          body: "The lemma is doing the work.",
          block_id: "b-9",
          page_number: 4,
          bbox: { x0: 10, y0: 20, x1: 80, y1: 60 },
        },
        {
          speaker: { id: "阿林" },
          content: "Keep the circuit in view.",
          evidenceId: "b-10",
          page: 5,
        },
      ],
    });
    expect(guideNotes(inks)).toHaveLength(2);
    expect(firstGuideNote(inks)?.anchor.pageNumber).toBe(4);
    expect(firstGuideAnchor(inks)?.pageNumber).toBe(4);
  });

  it("fills page 0 locators from OCR blocks so cards land on the PDF", () => {
    const inks = locateGuideInks(
      normalizeGuideInks([
        {
          kind: "note",
          speakerId: "xiaxia",
          body: "This figure is the whole argument.",
          anchor: { blockId: "fig-3", pageNumber: 0, bbox: [0, 0, 0, 0] },
        },
      ]),
      [
        {
          id: "fig-3",
          pageNumber: 7,
          blockType: "figure",
          bbox: [40, 80, 900, 420],
        },
      ],
    );
    expect(notesOnPage(inks, 7)).toHaveLength(1);
    expect(firstGuideNote(inks)?.anchor.pageNumber).toBe(7);
    expect(firstGuideNote(inks)?.anchor.bbox[3]).toBe(420);
  });
});
