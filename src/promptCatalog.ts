import { enT, type TranslateFn } from "./i18n/uiText";
import type {
  DocumentKind,
  PromptSettings,
  PromptSlotId,
  PromptSlotProjection,
} from "./types";

export type PromptSlotGroup =
  | "Outline"
  | "Guide"
  | "Lens"
  | "Reading"
  | "Discussion"
  | "Roots";

export type PromptSlotMeta = {
  id: PromptSlotId;
  group: PromptSlotGroup;
  requiresOutputLanguage: boolean;
};

export type PromptSlotDisplay = PromptSlotMeta & {
  useLegacyCopy: boolean;
};

export const PROMPT_SLOTS: PromptSlotMeta[] = [
  {
    id: "outline_extract",
    group: "Outline",
    requiresOutputLanguage: false,
  },
  {
    id: "outline_compose",
    group: "Outline",
    requiresOutputLanguage: false,
  },
  {
    id: "outline_deep_dive",
    group: "Outline",
    requiresOutputLanguage: false,
  },
  {
    id: "guide_context",
    group: "Guide",
    requiresOutputLanguage: false,
  },
  {
    id: "guide_annotate",
    group: "Guide",
    requiresOutputLanguage: false,
  },
  {
    id: "lens_formula",
    group: "Lens",
    requiresOutputLanguage: true,
  },
  {
    id: "lens_figure",
    group: "Lens",
    requiresOutputLanguage: true,
  },
  {
    id: "lens_table",
    group: "Lens",
    requiresOutputLanguage: true,
  },
  {
    id: "lens_repair_formula",
    group: "Lens",
    requiresOutputLanguage: true,
  },
  {
    id: "lens_repair_figure",
    group: "Lens",
    requiresOutputLanguage: true,
  },
  {
    id: "lens_repair_table",
    group: "Lens",
    requiresOutputLanguage: true,
  },
  {
    id: "lens_qa",
    group: "Lens",
    requiresOutputLanguage: false,
  },
  {
    id: "translation",
    group: "Reading",
    requiresOutputLanguage: true,
  },
  {
    id: "explanation",
    group: "Reading",
    requiresOutputLanguage: false,
  },
  {
    id: "reading_roadmap",
    group: "Reading",
    requiresOutputLanguage: false,
  },
  {
    id: "discussion",
    group: "Discussion",
    requiresOutputLanguage: false,
  },
  {
    id: "discussion_compaction",
    group: "Discussion",
    requiresOutputLanguage: false,
  },
  {
    id: "paper_root",
    group: "Roots",
    requiresOutputLanguage: false,
  },
  {
    id: "orientation_pack",
    group: "Roots",
    requiresOutputLanguage: false,
  },
  { id: "glossary", group: "Roots", requiresOutputLanguage: false },
  { id: "symbol_table", group: "Roots", requiresOutputLanguage: false },
  { id: "metadata", group: "Roots", requiresOutputLanguage: false },
];

export function validatePromptDraft(
  slot: PromptSlotId,
  text: string,
  t: TranslateFn = enT,
): string | null {
  if (!text.trim()) return t("prompts.validate.empty");
  const meta = PROMPT_SLOTS.find((item) => item.id === slot);
  if (meta?.requiresOutputLanguage && !text.includes("{output_language}")) {
    return t("prompts.validate.outputLanguage");
  }
  return null;
}

function factorySlot(slot: PromptSlotMeta): PromptSlotProjection {
  const text = slot.requiresOutputLanguage
    ? `${slot.id} into {output_language}`
    : `${slot.id} factory prompt`;
  return {
    ...(slot.group === "Outline" ? { outputProtocol: "v4" } : {}),
    text,
    previousText: null,
    updatedAt: null,
    isDefault: true,
  };
}

export function factoryPromptSettings(): PromptSettings {
  const slots: Record<string, PromptSettings["slots"][string]> = {};
  for (const slot of PROMPT_SLOTS) {
    const projection = factorySlot(slot);
    slots[slot.id] = {
      paper: { ...projection },
      textbook: { ...projection,
        ...(slot.id === "orientation_pack" ? { outputProtocol: "textbook-v2" } : {}),
        ...(["lens_formula","lens_figure","lens_table","lens_repair_formula","lens_repair_figure","lens_repair_table"].includes(slot.id) ? { outputProtocol: "v2" } : {}),
      },
    };
  }
  return { schemaVersion: 3, slots };
}

export function slotState(
  settings: PromptSettings,
  id: PromptSlotId,
  kind: DocumentKind,
): PromptSlotProjection | undefined {
  return settings.slots[id]?.[kind];
}

export function promptSlotGroups(): PromptSlotGroup[] {
  return ["Outline", "Guide", "Lens", "Reading", "Discussion", "Roots"];
}

export function promptMetaForProtocol(meta: PromptSlotMeta, protocol?: string): PromptSlotDisplay {
  return {
    ...meta,
    useLegacyCopy: protocol === "v3" && meta.group === "Outline",
  };
}
