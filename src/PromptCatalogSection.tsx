import React, { useEffect, useMemo, useState } from "react";
import {
  PROMPT_SLOTS,
  promptSlotGroups,
  promptMetaForProtocol,
  slotState,
  validatePromptDraft,
  type PromptSlotGroup,
  type PromptSlotDisplay,
} from "./promptCatalog";
import type { DocumentKind, PromptSettings, PromptSlotId } from "./types";
import { useLocale } from "./i18n/LocaleContext";

type PromptCatalogSectionProps = {
  settings: PromptSettings;
  drafts?: Map<string, string>;
  busy: boolean;
  error?: string | null;
  onSave: (slot: PromptSlotId, text: string, kind: DocumentKind) => void;
  onRestorePrevious: (slot: PromptSlotId, kind: DocumentKind) => void;
  onRestoreDefault: (slot: PromptSlotId, kind: DocumentKind) => void;
  onRestoreOutlineBundle?: (kind: DocumentKind) => void;
};

const GROUP_ICONS: Record<PromptSlotGroup, string> = {
  Outline: "🗺️",
  Guide: "📖",
  Lens: "🔍",
  Reading: "✍️",
  Discussion: "💬",
  Roots: "🧭",
};

const GROUP_NAME_KEYS: Record<PromptSlotGroup, string> = {
  Outline: "prompts.groups.outline",
  Guide: "prompts.groups.guide",
  Lens: "prompts.groups.lens",
  Reading: "prompts.groups.reading",
  Discussion: "prompts.groups.discussion",
  Roots: "prompts.groups.roots",
};

function slotCopyKeys(id: PromptSlotId, legacy: boolean) {
  const useLegacyLabel =
    legacy && (id === "outline_extract" || id === "outline_compose");
  return {
    label: useLegacyLabel
      ? `prompts.slots.${id}.legacyLabel`
      : `prompts.slots.${id}.label`,
    description: legacy
      ? `prompts.slots.${id}.legacyDescription`
      : `prompts.slots.${id}.description`,
  };
}

export default function PromptCatalogSection({
  settings,
  drafts,
  busy,
  error = null,
  onSave,
  onRestorePrevious,
  onRestoreDefault,
  onRestoreOutlineBundle,
}: PromptCatalogSectionProps) {
  const { t, locale } = useLocale();
  const [selected, setSelected] = useState<PromptSlotId>("outline_extract");
  const [kind, setKind] = useState<DocumentKind>("paper");
  const [draft, setDraft] = useState(
    drafts?.get(`${locale}:paper:outline_extract`) ?? slotState(settings, "outline_extract", "paper")?.text ?? "",
  );
  const [localError, setLocalError] = useState<string | null>(null);
  const [savedSuccess, setSavedSuccess] = useState(false);

  const selectedMeta = useMemo(
    () => promptMetaForProtocol(PROMPT_SLOTS.find((item) => item.id === selected) ?? PROMPT_SLOTS[0], slotState(settings, selected, kind)?.outputProtocol),
    [selected, settings, kind],
  );
  const currentSlot = slotState(settings, selected, kind);
  const selectedCopy = slotCopyKeys(selectedMeta.id, selectedMeta.useLegacyCopy);

  useEffect(() => {
    setDraft(drafts?.get(`${locale}:${kind}:${selected}`) ?? currentSlot?.text ?? "");
    setLocalError(null);
    setSavedSuccess(false);
  }, [selected, kind, currentSlot?.text, locale, drafts]);

  const grouped = useMemo(() => {
    return promptSlotGroups().map((group) => ({
      group,
      slots: PROMPT_SLOTS.filter((item) => item.group === group).map((item) => promptMetaForProtocol(item, slotState(settings, item.id, kind)?.outputProtocol)),
    }));
  }, [settings, kind]);

  const outlineProtocols = ["outline_extract", "outline_compose", "outline_deep_dive"].map((id) => slotState(settings, id as PromptSlotId, kind)?.outputProtocol ?? "v4");
  const mixedOutline = new Set(outlineProtocols).size > 1;
  const unsupportedOutline = outlineProtocols.some((protocol) => !["v3", "v4"].includes(protocol));
  const isDirty = draft.trim() !== (currentSlot?.text ?? "").trim();
  const isDefault = currentSlot?.isDefault ?? true;

  const handleSave = () => {
    const validation = validatePromptDraft(selected, draft, t);
    if (validation) {
      setLocalError(validation);
      return;
    }
    onSave(selected, draft, kind);
    setSavedSuccess(true);
    setTimeout(() => setSavedSuccess(false), 3000);
  };

  const slotLabel = (slot: PromptSlotDisplay) =>
    t(slotCopyKeys(slot.id, slot.useLegacyCopy).label);
  const slotDescription = (slot: PromptSlotDisplay) =>
    t(slotCopyKeys(slot.id, slot.useLegacyCopy).description);

  const outlineBanner = unsupportedOutline
    ? t("prompts.outline.unsupported")
    : mixedOutline
      ? t("prompts.outline.mixed")
      : outlineProtocols[0] === "v3"
        ? t("prompts.outline.legacyBundle")
        : t("prompts.outline.v4Bundle");

  const protocolBadge = () => {
    if (
      currentSlot?.outputProtocol &&
      ["paper_root", "glossary", "symbol_table", "metadata", "translation"].includes(
        selected,
      )
    ) {
      return currentSlot.outputProtocol === "v2"
        ? t("prompts.protocol.finalized")
        : t("prompts.protocol.legacyCompat");
    }
    if (
      currentSlot?.outputProtocol &&
      ["outline_extract", "outline_compose", "outline_deep_dive"].includes(
        selected,
      )
    ) {
      return currentSlot.outputProtocol === "v4"
        ? t("prompts.protocol.outlineV4")
        : currentSlot.outputProtocol === "v3"
          ? t("prompts.protocol.outlineV3")
          : t("prompts.protocol.unsupportedFlow");
    }
    return null;
  };

  const textbookStructure = () => {
    if (
      kind !== "textbook" ||
      ![
        "orientation_pack",
        "lens_formula",
        "lens_figure",
        "lens_table",
        "lens_repair_formula",
        "lens_repair_figure",
        "lens_repair_table",
      ].includes(selected)
    ) {
      return null;
    }
    const protocol = currentSlot?.outputProtocol;
    const label =
      protocol === "textbook-v2"
        ? t("prompts.protocol.textbookNine")
        : protocol === "v2"
          ? t("prompts.protocol.textbookLensV2")
          : protocol === "v1"
            ? t("prompts.protocol.textbookLegacy")
            : t("prompts.protocol.unsupportedStructure");
    return <span aria-label={t("prompts.protocol.textbookAria")}>{label}</span>;
  };

  return (
    <div className="prompt-catalog-master-detail">
      <aside className="prompt-slots-sidebar">
        <div className="prompt-sidebar-header">
          <h3>{t("prompts.catalogTitle")}</h3>
          <span className="prompt-sidebar-sub">
            {t("prompts.catalogSub", { count: PROMPT_SLOTS.length })}
          </span>
          <div
            className="prompt-kind-toggle"
            role="tablist"
            aria-label={t("prompts.kindAria")}
          >
            <button
              type="button"
              role="tab"
              aria-selected={kind === "paper"}
              className={`liquid-tab-btn ${kind === "paper" ? "active" : ""}`}
              onClick={() => setKind("paper")}
            >
              {t("prompts.kindPaper")}
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={kind === "textbook"}
              className={`liquid-tab-btn ${kind === "textbook" ? "active" : ""}`}
              onClick={() => setKind("textbook")}
            >
              {t("prompts.kindTextbook")}
            </button>
          </div>
        </div>

        <nav className="prompt-groups-scroll" aria-label={t("completion.prompt_slots_list")}>
          {grouped.map(({ group, slots }) => (
            <div key={group} className="prompt-group-section">
              <div className="prompt-group-title">
                <span className="group-icon">{GROUP_ICONS[group]}</span>
                <span>{t(GROUP_NAME_KEYS[group])}</span>
              </div>
              <div className="prompt-slots-column">
                {slots.map((slot) => {
                  const state = slotState(settings, slot.id, kind);
                  const isSelected = selected === slot.id;
                  const itemIsDefault = state?.isDefault ?? true;
                  const label = slotLabel(slot);

                  return (
                    <button
                      key={slot.id}
                      type="button"
                      aria-label={label}
                      className={`prompt-slot-btn ${isSelected ? "active" : ""}`}
                      onClick={() => setSelected(slot.id)}
                    >
                      <div className="slot-btn-left">
                        <strong className="slot-btn-label">{label}</strong>
                        <span className="slot-btn-desc">
                          {slotDescription(slot)}
                        </span>
                      </div>
                      <div className="slot-btn-status">
                        {itemIsDefault ? (
                          <span className="tag-default">{t("prompts.tagDefault")}</span>
                        ) : (
                          <span className="tag-custom">{t("prompts.tagCustom")}</span>
                        )}
                      </div>
                    </button>
                  );
                })}
              </div>
            </div>
          ))}
        </nav>
      </aside>

      <main className="prompt-editor-pane">
        <header className="prompt-editor-header">
          <div className="editor-header-main">
            <div className="editor-title-row">
              <h2>{t(selectedCopy.label)}</h2>
              <code className="slot-id-tag">{selectedMeta.id}</code>
              <span className="group-badge">
                {t(GROUP_NAME_KEYS[selectedMeta.group])}
              </span>
              {protocolBadge() ? <span>{protocolBadge()}</span> : null}
              {textbookStructure()}
              {isDefault ? (
                <span className="status-badge is-default">● {t("prompts.statusDefault")}</span>
              ) : (
                <span className="status-badge is-custom">● {t("prompts.statusCustom")}</span>
              )}
            </div>
            <p className="editor-slot-desc">{t(selectedCopy.description)}</p>
          </div>

          <div className="editor-header-actions">
            <button
              type="button"
              className="btn-secondary btn-sm"
              disabled={busy || !currentSlot?.previousText}
              onClick={() => onRestorePrevious(selected, kind)}
              title={t("prompts.restorePreviousTitle")}
            >
              {t("prompts.restorePrevious")}
            </button>
            <button
              type="button"
              className="btn-ghost btn-sm"
              disabled={busy}
              onClick={() => onRestoreDefault(selected, kind)}
              title={t("prompts.restoreDefaultTitle")}
            >
              {t("prompts.restoreDefault")}
            </button>
            <button
              type="button"
              className="btn-primary btn-sm"
              disabled={busy}
              onClick={handleSave}
            >
              {busy ? t("prompts.saving") : t("prompts.save")}
            </button>
          </div>
        </header>

        {selectedMeta.group === "Outline" && (
          <div className="prompt-alert-banner" role={mixedOutline || unsupportedOutline ? "alert" : "status"}>
            <span>{outlineBanner}</span>
            {onRestoreOutlineBundle && (
              <button type="button" className="btn-secondary btn-sm" disabled={busy}
                onClick={() => {
                  if (isDirty && !window.confirm(t("prompts.outline.unsavedConfirm"))) return;
                  onRestoreOutlineBundle(kind);
                }}>
                {t("prompts.outline.useV4Bundle")}
              </button>
            )}
          </div>
        )}

        {localError || error ? (
          <div className="prompt-alert-banner error" role="alert">
            <span>⚠️ {localError ?? error}</span>
          </div>
        ) : null}

        {savedSuccess ? (
          <div className="prompt-alert-banner success">
            <span>✓ {t("prompts.saved")}</span>
          </div>
        ) : null}

        {selectedMeta.requiresOutputLanguage && (
          <div className="prompt-variables-hint">
            <span>
              ⚠️ {t("prompts.outputLanguageHint")}
            </span>
          </div>
        )}

        <div className="prompt-textarea-wrap">
          <textarea
            className="prompt-monaco-textarea"
            value={draft}
            onChange={(event) => {
              drafts?.set(`${locale}:${kind}:${selected}`, event.target.value);
              setDraft(event.target.value);
              setLocalError(null);
            }}
            placeholder={t("prompts.editorPlaceholder")}
            spellCheck={false}
          />
          <div className="prompt-textarea-footer">
            <span>{t("prompts.charCount", { count: draft.length })}</span>
            <span>{isDirty ? t("prompts.unsaved") : t("prompts.allSaved")}</span>
          </div>
        </div>
      </main>
    </div>
  );
}
