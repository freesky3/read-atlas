import type {
  GuideInk,
  GuideLocator,
  GuideNote,
  GuideReply,
  GuideSpeakerId,
} from "../types";

const V1_SPEAKERS = ["alin", "laozhou", "xiaxia"] as const;

export type GuideBlockLike = {
  id: string;
  pageNumber: number;
  bbox: [number, number, number, number];
  blockType?: string;
};

export function normalizeGuideInks(raw: unknown): GuideInk[] {
  const items = inkItems(raw);
  const parsed: GuideInk[] = [];
  for (const item of items) {
    const ink = parseGuideInk(item);
    if (ink) parsed.push(ink);
  }
  return parsed;
}

export function locateGuideInks(
  inks: readonly GuideInk[],
  blocks: readonly GuideBlockLike[] = [],
): GuideInk[] {
  if (blocks.length === 0) return [...inks];
  const byId = new Map(blocks.map((block) => [block.id, block]));
  return inks.map((ink) => {
    if (ink.kind === "reply") return ink;
    const block = byId.get(ink.anchor.blockId);
    if (!block) return ink;
    const pageNumber =
      Number(ink.anchor.pageNumber) >= 1
        ? Number(ink.anchor.pageNumber)
        : block.pageNumber;
    const bbox = hasBbox(ink.anchor.bbox) ? ink.anchor.bbox : block.bbox;
    const blockType =
      ink.anchor.blockType && ink.anchor.blockType !== "paragraph"
        ? ink.anchor.blockType
        : (block.blockType ?? ink.anchor.blockType);
    if (
      pageNumber === ink.anchor.pageNumber &&
      bbox === ink.anchor.bbox &&
      blockType === ink.anchor.blockType
    ) {
      return ink;
    }
    return {
      ...ink,
      anchor: { ...ink.anchor, pageNumber, bbox, blockType },
    };
  });
}

export function guideNotes(inks: readonly GuideInk[]): GuideNote[] {
  return inks.filter((ink): ink is GuideNote => ink.kind === "note");
}

export function notesOnPage(
  inks: readonly GuideInk[],
  pageNumber: number,
  pageBlockIds?: ReadonlySet<string>,
): GuideNote[] {
  return guideNotes(inks).filter((note) => {
    if (Number(note.anchor.pageNumber) === pageNumber) return true;
    return Boolean(pageBlockIds?.has(note.anchor.blockId));
  });
}

export function firstGuideNote(inks: readonly GuideInk[]): GuideNote | null {
  const notes = [...guideNotes(inks)].sort(compareAnchors);
  return notes[0] ?? null;
}

export function firstGuideAnchor(
  inks: readonly GuideInk[],
): { id: string; pageNumber: number } | null {
  const note = firstGuideNote(inks);
  if (note && note.anchor.pageNumber >= 1) {
    return { id: note.id, pageNumber: note.anchor.pageNumber };
  }
  const traces = inks
    .filter(
      (ink): ink is Extract<GuideInk, { kind: "trace" }> =>
        ink.kind === "trace" && ink.anchor.pageNumber >= 1,
    )
    .sort(compareAnchors);
  const trace = traces[0];
  if (trace) return { id: trace.id, pageNumber: trace.anchor.pageNumber };
  if (note) return { id: note.id, pageNumber: note.anchor.pageNumber };
  return null;
}

export function nextNotePage(
  inks: readonly GuideInk[],
  pageNumber: number,
): number | null {
  const later = guideNotes(inks)
    .map((note) => Number(note.anchor.pageNumber))
    .filter((page) => page > pageNumber)
    .sort((left, right) => left - right);
  return later[0] ?? null;
}

export function repliesFor(
  inks: readonly GuideInk[],
  noteId: string,
): GuideReply[] {
  return inks.filter(
    (ink): ink is GuideReply => ink.kind === "reply" && ink.parentId === noteId,
  );
}

export function inksForBlock(inks: readonly GuideInk[], blockId: string): GuideInk[] {
  return inks.filter((ink) => {
    if (ink.kind === "reply" || !ink.anchor) return false;
    return ink.anchor.blockId === blockId;
  });
}

export function primaryNoteForBlock(
  inks: readonly GuideInk[],
  blockId: string,
): GuideNote | null {
  return (
    guideNotes(inks).find((note) => note.anchor.blockId === blockId) ?? null
  );
}

function compareAnchors(
  left: { anchor: GuideLocator },
  right: { anchor: GuideLocator },
) {
  return (
    left.anchor.pageNumber - right.anchor.pageNumber ||
    left.anchor.bbox[1] - right.anchor.bbox[1]
  );
}

function hasBbox(bbox: [number, number, number, number]) {
  return bbox.some((value) => value !== 0);
}

function inkItems(raw: unknown): unknown[] {
  if (Array.isArray(raw)) return raw;
  if (typeof raw === "string") {
    try {
      return inkItems(JSON.parse(raw));
    } catch {
      return [];
    }
  }
  if (raw && typeof raw === "object") {
    const record = raw as Record<string, unknown>;
    if (Array.isArray(record.inks)) return record.inks;
    if (Array.isArray(record.annotations)) return record.annotations;
    if (Array.isArray(record.notes)) return record.notes;
    if (
      record.kind ||
      record.type ||
      record.anchor ||
      record.body ||
      record.blockId ||
      record.Note ||
      record.note
    ) {
      return [record];
    }
  }
  return [];
}

function parseGuideInk(raw: unknown): GuideInk | null {
  const value = unwrapTagged(raw);
  if (!value) return null;
  const speakerId = normalizeSpeaker(
    asString(value.speakerId) ??
      asString(value.speaker_id) ??
      asString(value.speaker) ??
      asString(value.persona) ??
      asString(value.author) ??
      speakerFromObject(value.speaker) ??
      speakerFromObject(value.persona) ??
      speakerFromObject(value.author),
  );
  if (!speakerId) return null;
  const body =
    asString(value.body) ??
    asString(value.text) ??
    asString(value.content) ??
    asString(value.prose) ??
    asString(value.comment) ??
    asString(value.note);
  const parentId =
    asString(value.parentId) ??
    asString(value.parent_id) ??
    asString(value.inReplyTo);
  let kind = normalizeKind(
    asString(value.kind) ?? asString(value.type) ?? asString(value.inkKind),
  );
  if (!kind) {
    if (parentId && body) kind = "reply";
    else if (body) kind = "note";
    else kind = "trace";
  }
  const id = asString(value.id) ?? `${kind}-${speakerId}`;
  if (kind === "reply") {
    if (!parentId || !body) return null;
    return { id, kind: "reply", speakerId, parentId, body };
  }
  const anchor = parseLocator(value);
  if (!anchor) return null;
  if (kind === "trace") {
    return { id, kind: "trace", speakerId, anchor };
  }
  if (!body) return null;
  const weight = asString(value.weight) === "short" ? "short" : "line";
  return { id, kind: "note", speakerId, weight, anchor, body };
}

function unwrapTagged(raw: unknown): Record<string, unknown> | null {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return null;
  const value = raw as Record<string, unknown>;
  if (value.kind || value.type || value.anchor || value.blockId) return value;
  for (const key of ["note", "trace", "reply", "Note", "Trace", "Reply"]) {
    const inner = value[key];
    if (inner && typeof inner === "object" && !Array.isArray(inner)) {
      return { kind: key.toLowerCase(), ...(inner as Record<string, unknown>) };
    }
  }
  return value;
}

function parseLocator(value: Record<string, unknown>): GuideLocator | null {
  const nested =
    asRecord(value.anchor) ??
    asRecord(value.locator) ??
    (typeof value.anchor === "string" ? { blockId: value.anchor } : null);
  const source = nested ?? value;
  const blockId =
    asString(source.blockId) ??
    asString(source.block_id) ??
    asString(source.ref) ??
    asString(source.evidenceId) ??
    asString(source.evidence_id) ??
    (nested ? asString(source.id) : null) ??
    firstListId(value.evidenceIds) ??
    firstListId(value.evidence_ids) ??
    firstListId(value.blockIds) ??
    asString(value.blockId) ??
    asString(value.block_id);
  if (!blockId) return null;
  const pageNumber = Number(
    source.pageNumber ??
      source.page_number ??
      source.page ??
      value.pageNumber ??
      value.page_number ??
      value.page ??
      0,
  );
  const bbox =
    parseBbox(source.bbox) ?? parseBbox(value.bbox) ?? [0, 0, 0, 0];
  return {
    blockId,
    pageNumber: Number.isFinite(pageNumber) ? pageNumber : 0,
    blockType: asString(source.blockType) ?? asString(source.block_type) ?? "paragraph",
    bbox,
  };
}

function parseBbox(raw: unknown): [number, number, number, number] | null {
  if (Array.isArray(raw) && raw.length >= 4) {
    const values = raw.slice(0, 4).map((item) => Number(item));
    if (values.some((item) => !Number.isFinite(item))) return null;
    return [values[0], values[1], values[2], values[3]];
  }
  const record = asRecord(raw);
  if (!record) return null;
  const x0 = Number(record.x0 ?? record.left);
  const y0 = Number(record.y0 ?? record.top);
  const x1 = Number(record.x1 ?? record.right);
  const y1 = Number(record.y1 ?? record.bottom);
  if (![x0, y0, x1, y1].every((item) => Number.isFinite(item))) return null;
  return [x0, y0, x1, y1];
}

function normalizeKind(kind: string | null): "note" | "trace" | "reply" | null {
  if (!kind) return null;
  const compact = kind
    .trim()
    .toLowerCase()
    .replace(/[^a-z]/g, "");
  if (
    ["trace", "mark", "highlight", "underline", "stroke", "color"].includes(
      compact,
    )
  ) {
    return "trace";
  }
  if (["reply", "response", "followup"].includes(compact)) return "reply";
  if (
    [
      "note",
      "notes",
      "annotation",
      "comment",
      "ink",
      "margin",
      "remark",
      "prose",
      "line",
      "short",
    ].includes(compact)
  ) {
    return "note";
  }
  return null;
}

function normalizeSpeaker(value: string | null): GuideSpeakerId | null {
  if (!value) return null;
  const trimmed = value.trim();
  if (!trimmed) return null;
  if (trimmed === "阿林") return "alin";
  if (trimmed === "老周") return "laozhou";
  if (trimmed === "小夏") return "xiaxia";
  const compact = trimmed.toLowerCase().replace(/[^a-z]/g, "");
  const v1 = V1_SPEAKERS.find((id) => id === compact);
  if (v1) return v1;
  return trimmed;
}

function speakerFromObject(raw: unknown): string | null {
  const record = asRecord(raw);
  if (!record) return null;
  return asString(record.id) ?? asString(record.speakerId) ?? asString(record.name);
}

function firstListId(raw: unknown): string | null {
  if (typeof raw === "string" && raw.trim()) return raw.trim();
  if (!Array.isArray(raw)) return null;
  for (const item of raw) {
    if (typeof item === "string" && item.trim()) return item.trim();
  }
  return null;
}

function asString(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const text = value.trim();
  return text || null;
}

function asRecord(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  return value as Record<string, unknown>;
}
