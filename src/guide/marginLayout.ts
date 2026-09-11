export const GUIDE_GUTTER_WIDTH = 240;
export const GUIDE_GUTTER_GAP = 16;
export const GUIDE_CARD_GAP = 8;
export const GUIDE_CARD_INSET = 8;
export const GUIDE_LEADER_ATTACH_Y = 18;

export type GuideMarginCardInput = {
  id: string;
  anchorX: number;
  anchorY: number;
  height: number;
};

export type GuideMarginPlacement = GuideMarginCardInput & { top: number };

export type GuideMarginLayout = {
  placements: GuideMarginPlacement[];
  requiredHeight: number;
  contentHeight: number;
};

export function layoutGuideMarginCards(input: {
  pageHeight: number;
  cards: readonly GuideMarginCardInput[];
  gap?: number;
  inset?: number;
}): GuideMarginLayout {
  const gap = Math.max(0, input.gap ?? GUIDE_CARD_GAP);
  const inset = Math.max(0, input.inset ?? GUIDE_CARD_INSET);
  const cards = [...input.cards]
    .map((card) => ({ ...card, height: Math.max(1, card.height) }))
    .sort(
      (left, right) =>
        left.anchorY - right.anchorY ||
        left.anchorX - right.anchorX ||
        left.id.localeCompare(right.id),
    );
  const requiredHeight =
    cards.reduce((sum, card) => sum + card.height, 0) +
    Math.max(0, cards.length - 1) * gap;
  const bottom = Math.max(inset, input.pageHeight - inset);
  const placements: GuideMarginPlacement[] = [];

  for (const card of cards) {
    const desired = Math.min(
      Math.max(inset, card.anchorY - card.height / 2),
      Math.max(inset, bottom - card.height),
    );
    const previous = placements.at(-1);
    const top = previous
      ? Math.max(desired, previous.top + previous.height + gap)
      : desired;
    placements.push({ ...card, top });
  }

  const availableHeight = Math.max(0, input.pageHeight - inset * 2);
  const fitsWithinPage = requiredHeight <= availableHeight;
  if (fitsWithinPage && placements.length > 0) {
    const final = placements.at(-1)!;
    const shift = Math.max(0, final.top + final.height - bottom);
    if (shift > 0) placements.forEach((placement) => {
      placement.top -= shift;
    });
    for (let index = placements.length - 2; index >= 0; index -= 1) {
      const current = placements[index];
      const next = placements[index + 1];
      current.top = Math.min(current.top, next.top - gap - current.height);
    }
    const firstShift = Math.max(0, inset - placements[0].top);
    if (firstShift > 0) placements.forEach((placement) => {
      placement.top += firstShift;
    });
  }

  const final = placements.at(-1);
  const contentHeight = Math.max(
    input.pageHeight,
    final ? final.top + final.height + inset : input.pageHeight,
  );
  return { placements, requiredHeight, contentHeight };
}

export type GuideLeaderLine = {
  id: string;
  x1: number;
  y1: number;
  x2: number;
  y2: number;
};

/** Map a laid-out gutter card onto the page-row SVG after the gutter scrolls. */
export function guideLeaderLine(input: {
  id: string;
  blockRightX: number;
  blockCenterY: number;
  pageWidth: number;
  pageHeight: number;
  cardTop: number;
  cardHeight: number;
  gutterScrollTop: number;
  gutterGap?: number;
  attachOffset?: number;
}): GuideLeaderLine {
  const gap = input.gutterGap ?? GUIDE_GUTTER_GAP;
  const height = Math.max(1, input.cardHeight);
  const attach = Math.min(
    input.attachOffset ?? GUIDE_LEADER_ATTACH_Y,
    height / 2,
  );
  const pageHeight = Math.max(0, input.pageHeight);
  const rawY2 = input.cardTop - input.gutterScrollTop + attach;
  return {
    id: input.id,
    x1: input.blockRightX,
    y1: input.blockCenterY,
    x2: input.pageWidth + gap + 2,
    y2: Math.min(pageHeight, Math.max(0, rawY2)),
  };
}
