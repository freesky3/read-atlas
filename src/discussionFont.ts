export const DISCUSSION_FONT_SIZES = [14, 15, 17] as const;

export type DiscussionFontSize = (typeof DISCUSSION_FONT_SIZES)[number];

export const DEFAULT_DISCUSSION_FONT_SIZE: DiscussionFontSize = 15;
export const DISCUSSION_FONT_STORAGE_KEY = "read-desktop.discussionFontSize";

export function parseDiscussionFontSize(value: unknown): DiscussionFontSize {
  const numeric = Number(value);
  return (DISCUSSION_FONT_SIZES as readonly number[]).includes(numeric)
    ? (numeric as DiscussionFontSize)
    : DEFAULT_DISCUSSION_FONT_SIZE;
}

export function storedDiscussionFontSize(): DiscussionFontSize {
  try {
    return parseDiscussionFontSize(
      localStorage.getItem(DISCUSSION_FONT_STORAGE_KEY),
    );
  } catch {
    return DEFAULT_DISCUSSION_FONT_SIZE;
  }
}

export function stepDiscussionFontSize(
  current: DiscussionFontSize,
  direction: -1 | 1,
): DiscussionFontSize {
  const index = DISCUSSION_FONT_SIZES.indexOf(current);
  const next = DISCUSSION_FONT_SIZES[index + direction];
  return next ?? current;
}
