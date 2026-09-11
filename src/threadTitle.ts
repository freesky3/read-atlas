export const PLACEHOLDER_THREAD_TITLES = [
  "新对话",
  "New exploration",
  "Main discussion",
] as const;

export function isPlaceholderThreadTitle(title: string) {
  return (PLACEHOLDER_THREAD_TITLES as readonly string[]).includes(
    title.trim(),
  );
}

export function titleFromFirstQuestion(question: string, maxLength = 32) {
  const firstLine = question.trim().split(/\r?\n/, 1)[0] ?? "";
  const collapsed = firstLine.replace(/\s+/g, " ").trim();
  if (!collapsed) return "新对话";
  if (collapsed.length <= maxLength) return collapsed;
  return `${collapsed.slice(0, maxLength).trimEnd()}…`;
}
