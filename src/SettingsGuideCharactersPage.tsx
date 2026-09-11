import { localizedCharacter, canonicalCharacterDraft, localizedCastName } from "./i18n/guidePresets";
import React, { useEffect, useMemo, useRef, useState } from "react";
import { desktopClient } from "./desktopClient";
import type { GuideCharacter, GuideCharacterSettings, GuidePresetCast } from "./types";
import GuideAvatar from "./guide/GuideAvatar";
import GuideCharacterPreview from "./guide/GuideCharacterPreview";
import GuideAvatarCropper from "./guide/GuideAvatarCropper";
import GuideLineupManagerModal from "./guide/GuideLineupManagerModal";
import { useLocale } from "./i18n/LocaleContext";

function emptyDraft(): GuideCharacter {
  return {
    id: "",
    revision: 1,
    displayName: "",
    workTitle: "",
    characterVersion: "",
    description: "",
    avatarAssetId: null,
    inkColor: "#7653A6",
    personality: "",
    readingHabits: "",
    expressionStyle: "",
    avoidances: "",
    exampleNotes: [""],
    presetId: null,
    presetVersion: null,
    createdAt: "",
    updatedAt: "",
  };
}

const PRESET_INK_PALETTE = [
  { id: "chitanda", color: "#7653A6" },
  { id: "oreki", color: "#5B8A64" },
  { id: "frieren", color: "#2E798A" },
  { id: "jotaro", color: "#2E4C7A" },
  { id: "conan", color: "#A63A2A" },
  { id: "amber", color: "#D97706" },
  { id: "ink", color: "#475569" },
  { id: "terracotta", color: "#C15F3E" },
];

export default function SettingsGuideCharactersPage({ onDirtyChange }: { onDirtyChange?: (dirty: boolean) => void } = {}) {
  const { t, locale } = useLocale();
  const [store, setStore] = useState<GuideCharacterSettings | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [draft, setDraft] = useState<GuideCharacter>(emptyDraft());
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [dirty, setDirty] = useState(false);
  const [avatarCache, setAvatarCache] = useState<Record<string, string>>({});
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [isDraggingAvatar, setIsDraggingAvatar] = useState(false);
  const [cropFile, setCropFile] = useState<File | null>(null);
  const [lineupModalOpen, setLineupModalOpen] = useState(false);

  const fileInputRef = useRef<HTMLInputElement>(null);
  const draftRef = useRef(draft);
  draftRef.current = draft;
  useEffect(() => { onDirtyChange?.(dirty); }, [dirty, onDirtyChange]);
  useEffect(() => {
    if (!dirty) return;
    const beforeUnload = (event: BeforeUnloadEvent) => { event.preventDefault(); event.returnValue = ""; };
    window.addEventListener("beforeunload", beforeUnload);
    return () => window.removeEventListener("beforeunload", beforeUnload);
  }, [dirty]);

  function selectCharacter(character: GuideCharacter) {
    if (dirty && !window.confirm(t("settings.characters.confirmSwitch"))) return;
    setSelectedId(character.id);
    setDraft(localizedCharacter(character, locale));
    setDirty(false);
    setConfirmDelete(false);
  }


  useEffect(() => {
    void desktopClient
      .open<GuideCharacterSettings>("get_guide_character_settings")
      .then((next) => {
        setStore(next);
        const first = next.characters[0];
        if (first) {
          setSelectedId(first.id);
          setDraft(localizedCharacter(first, locale));
        }
      })
      .catch((reason) => setError(String(reason)));
  }, []);

  // Fetch any missing avatars in avatarCache
  useEffect(() => {
    if (!store) return;
    const missingAssetIds = store.characters
      .map((c) => c.avatarAssetId)
      .filter((id): id is string => typeof id === "string" && id.length > 0 && !(id in avatarCache));

    if (missingAssetIds.length === 0) return;

    missingAssetIds.forEach((assetId) => {
      void desktopClient
        .command<{ assetId: string; dataUrl: string }>("get_guide_character_avatar", {
          request: { assetId },
        })
        .then((res) => {
          if (res?.dataUrl) {
            setAvatarCache((prev) => ({ ...prev, [assetId]: res.dataUrl }));
          }
        })
        .catch(() => undefined);
    });
  }, [store, avatarCache]);

  const selected = useMemo(
    () => store?.characters.find((item) => item.id === selectedId) ?? null,
    [store, selectedId],
  );

  function applyStore(next: GuideCharacterSettings, keepId?: string) {
    setStore(next);
    const candidate = keepId ?? selectedId;
    const character = next.characters.find((item) => item.id === candidate) ?? next.characters[0];
    setSelectedId(character?.id ?? null);
    setDraft(character ? localizedCharacter(character, locale) : emptyDraft());
    setDirty(false);
    setConfirmDelete(false);
  }

  const previousLocale = useRef(locale);
  useEffect(() => {
    const previous = previousLocale.current;
    previousLocale.current = locale;
    if (previous === locale) return;
    setDraft(current => localizedCharacter(canonicalCharacterDraft(current, selected, previous), locale));
  }, [locale, selected]);

  async function save() {
    if (!store) return;
    if (!draft.displayName.trim()) { setError(t("settings.characters.nameRequired")); return; }
    const submitted = draft;
    const canonical = canonicalCharacterDraft(draft, selected, locale);
    setError(null);
    try {
      const next = await desktopClient.command<GuideCharacterSettings>(
        "save_guide_character",
        {
          request: {
            expectedStoreRevision: store.storeRevision,
            character: {
              id: canonical.id || null,
              displayName: canonical.displayName.trim(),
              workTitle: canonical.workTitle.trim(),
              characterVersion: canonical.characterVersion.trim(),
              description: canonical.description.trim(),
              avatarAssetId: canonical.avatarAssetId,
              inkColor: canonical.inkColor,
              personality: canonical.personality,
              readingHabits: canonical.readingHabits,
              expressionStyle: canonical.expressionStyle,
              avoidances: canonical.avoidances,
              exampleNotes: canonical.exampleNotes.filter((n) => n.trim().length > 0),
            },
          },
        },
      );
      const savedId = submitted.id || next.characters.at(-1)?.id;
      if (draftRef.current === submitted) {
        applyStore(next, savedId);
      } else {
        setStore(next);
        if (!submitted.id && draftRef.current.id === "" && savedId) {
          setSelectedId(savedId);
          setDraft((current) => ({ ...current, id: savedId }));
        }
      }
      setStatus(t("settings.characters.saved"));
      setTimeout(() => setStatus(null), 3000);
    } catch (reason) {
      setError(String(reason));
    }
  }

  async function run(
    command:
      | "delete_guide_character"
      | "restore_guide_character_preset"
      | "duplicate_guide_character",
  ) {
    if (!store || !selected) return;
    if (dirty && !window.confirm(t("settings.characters.confirmContinue"))) return;
    setError(null);
    try {
      const next = await desktopClient.command<GuideCharacterSettings>(command, {
        request: {
          expectedStoreRevision: store.storeRevision,
          characterId: selected.id,
        },
      });
      const newId = command === "duplicate_guide_character"
        ? next.characters.find((item) => !store.characters.some((old) => old.id === item.id))?.id
        : undefined;
      applyStore(next, newId);
      setStatus(t("settings.characters.updated"));
      setTimeout(() => setStatus(null), 3000);
    } catch (reason) {
      setError(String(reason));
    }
  }

  async function saveDefaultCast(ids: string[]) {
    if (!store) return;
    setError(null);
    try {
      const next = await desktopClient.command<GuideCharacterSettings>("save_guide_default_cast", {
        request: { expectedStoreRevision: store.storeRevision, characterIds: ids },
      });
      setStore(next);
      setStatus(t("settings.characters.defaultCastUpdated"));
    } catch (reason) { setError(String(reason)); }
  }

  async function toggleDefault(id: string) {
    if (!store) return;
    const ids = store.defaultCharacterIds.includes(id)
      ? store.defaultCharacterIds.filter((item) => item !== id)
      : [...store.defaultCharacterIds, id];
    await saveDefaultCast(ids);
  }

  async function savePresetCasts(casts: GuidePresetCast[]) {
    if (!store) return;
    setError(null);
    try {
      const next = await desktopClient.command<GuideCharacterSettings>("save_guide_preset_casts", {
        request: { expectedStoreRevision: store.storeRevision, presetCasts: casts },
      });
      setStore(next);
      setStatus(t("settings.characters.castUpdated"));
    } catch (reason) {
      setError(String(reason));
      throw reason;
    }
  }

  async function restoreFactoryPresets() {
    if (!store) return;
    setError(null);
    try {
      const next = await desktopClient.command<GuideCharacterSettings>("restore_guide_factory_preset_casts", {
        request: { expectedStoreRevision: store.storeRevision },
      });
      setStore(next);
      setStatus(t("settings.characters.factoryRestored"));
    } catch (reason) {
      setError(String(reason));
      throw reason;
    }
  }

  async function importAvatar(dataUrl: string) {
    const targetId = draftRef.current.id;
    const imported = await desktopClient.command<{ assetId: string }>("import_guide_character_avatar", {
      request: { bytesBase64: dataUrl.split(",")[1] },
    });
    setAvatarCache((prev) => ({ ...prev, [imported.assetId]: dataUrl }));
    if (draftRef.current.id !== targetId) return;
    setDraft((current) => ({ ...current, avatarAssetId: imported.assetId }));
    setDirty(true);
    setCropFile(null);
    setStatus(t("settings.characters.avatarImported"));
  }

  function handleFileSelected(event: React.ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    if (file) {
      setCropFile(file);
    }
    event.target.value = "";
  }

  function handleRemoveAvatar() {
    setDraft((current) => ({ ...current, avatarAssetId: null }));
    setDirty(true);
  }

  function handleAvatarDrop(e: React.DragEvent) {
    e.preventDefault();
    setIsDraggingAvatar(false);
    const file = e.dataTransfer.files?.[0];
    if (file && file.type.startsWith("image/")) {
      setCropFile(file);
    }
  }

  if (!store) {
    return <p className="settings-note">{error ?? t("settings.characters.loading")}</p>;
  }

  const currentAvatarSrc = draft.avatarAssetId
    ? avatarCache[draft.avatarAssetId] ?? null
    : null;
  const isDefaultSelected = draft.id ? store.defaultCharacterIds.includes(draft.id) : false;

  return (
    <div className="dossier-layout">
      {cropFile ? <GuideAvatarCropper file={cropFile} onCancel={() => setCropFile(null)} onApply={importAvatar} /> : null}
      {/* Hidden file picker input */}
      <input
        type="file"
        ref={fileInputRef}
        style={{ display: "none" }}
        accept="image/png,image/jpeg,image/webp,image/gif"
        onChange={handleFileSelected}
      />

      {/* 1. Left Character Directory / Cast Roster */}
      <aside className="dossier-sidebar">
        <div className="dossier-sidebar-header">
          <div className="dossier-roster-summary">
            <div className="dossier-roster-avatars">
              {store.defaultCharacterIds.map((id) => {
                const char = store.characters.find((c) => c.id === id);
                return (
                  <GuideAvatar
                    key={id}
                    size={20}
                    className="roster-avatar-item"
                    persona={
                      char
                        ? {
                            id: char.id,
                            displayName: char.displayName,
                            color: char.inkColor,
                            avatarSrc: char.avatarAssetId
                              ? avatarCache[char.avatarAssetId] ?? null
                              : null,
                          }
                        : null
                    }
                  />
                );
              })}
            </div>
            <span className="dossier-roster-badge">
              {t("settings.characters.defaultCastCount", { count: store.defaultCharacterIds.length })}
            </span>
          </div>
        </div>

        <div className="dossier-cast-presets" aria-label={t("settings.characters.presetCastsAria")}>
          <div className="dossier-preset-pills-wrap">
            {store.presetCasts.map((cast) => {
              const isCurrentDefault =
                cast.characterIds.length === store.defaultCharacterIds.length &&
                cast.characterIds.every((id) => store.defaultCharacterIds.includes(id));
              return (
                <button
                  type="button"
                  className={`btn-liquid-pill ${isCurrentDefault ? "primary" : ""}`}
                  key={cast.id}
                  onClick={() => void saveDefaultCast(cast.characterIds)}
                  title={t("settings.characters.setDefaultCast", { name: localizedCastName(cast, t), count: cast.characterIds.length })}
                >
                  {localizedCastName(cast, t)}
                </button>
              );
            })}
          </div>
          <button
            type="button"
            className="btn-liquid-pill dossier-lineup-manage-btn"
            onClick={() => setLineupModalOpen(true)}
            title={t("settings.characters.manageCastsTitle")}
          >
            {t("settings.characters.manageCasts")}
          </button>
        </div>
        <div className="dossier-character-list">
          {store.characters.map((rawCharacter) => {
            const character = localizedCharacter(rawCharacter, locale);
            const isActive = character.id === selectedId;
            const isDef = store.defaultCharacterIds.includes(character.id);
            const avatarSrc = character.avatarAssetId
              ? avatarCache[character.avatarAssetId] ?? null
              : null;

            return (
              <div
                key={character.id}
                role="button"
                tabIndex={0}
                className={`dossier-character-item ${isActive ? "is-active" : ""}`}
                style={{
                  borderLeftColor: isActive ? character.inkColor : "transparent",
                }}
                onClick={() => selectCharacter(character)}
                onKeyDown={(e) => {
                  if (e.target !== e.currentTarget) return;
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    selectCharacter(character);
                  }
                }}
              >
                <div className="item-avatar-col">
                  <GuideAvatar
                    size={34}
                    className="item-avatar"
                    persona={{
                      id: character.id,
                      displayName: character.displayName,
                      color: character.inkColor,
                      avatarSrc,
                    }}
                  />
                </div>
                <div className="item-meta-col">
                  <div className="item-title-row">
                    <strong className="item-display-name">{character.displayName}</strong>
                    {character.presetId ? (
                      <span className="item-preset-tag">{t("settings.characters.preset")}</span>
                    ) : (
                      <span className="item-custom-tag">{t("settings.characters.custom")}</span>
                    )}
                  </div>
                  <small className="item-description-text">
                    {character.workTitle
                      ? t("settings.characters.workLine", {
                          title: character.workTitle,
                          version: character.characterVersion ? ` · ${character.characterVersion}` : "",
                        })
                      : character.description || t("settings.characters.noWork")}
                  </small>
                </div>
                <button
                  type="button"
                  aria-label={t(isDef ? "settings.characters.removeFromCastAria" : "settings.characters.addToCastAria", { name: character.displayName })}
                  className="item-default-toggle"
                  title={isDef ? t("settings.characters.removeFromCast") : t("settings.characters.addToCast")}
                  onClick={(e) => {
                    e.stopPropagation();
                    void toggleDefault(character.id);
                  }}
                >
                  <span className={`roster-pill-switch ${isDef ? "is-on" : ""}`}>
                    {isDef ? t("settings.characters.defaultOn") : t("settings.characters.standby")}
                  </span>
                </button>
              </div>
            );
          })}
        </div>

        <button
          type="button"
          className="dossier-new-btn"
          onClick={() => {
            if (dirty) {
              if (!window.confirm(t("settings.characters.confirmNew"))) {
                return;
              }
            }
            setSelectedId(null);
            setDraft(emptyDraft());
            setDirty(true);
            setConfirmDelete(false);
          }}
        >
          <span className="plus-glyph">＋</span> {t("settings.characters.addCharacter")}
        </button>
      </aside>

      {/* 2. Right Dossier View & Editor Pane */}
      <main className="dossier-main">
        {/* Hero Banner / Portrait & Identity */}
        <section className="dossier-hero-card">
          <div
            className={`dossier-portrait-wrap ${isDraggingAvatar ? "is-dragging" : ""}`}
            style={{ borderColor: draft.inkColor }}
            onClick={() => fileInputRef.current?.click()}
            onDragOver={(e) => {
              e.preventDefault();
              setIsDraggingAvatar(true);
            }}
            onDragLeave={() => setIsDraggingAvatar(false)}
            onDrop={handleAvatarDrop}
            title={t("settings.characters.avatarDropTitle")}
          >
            {currentAvatarSrc ? (
              <img
                src={currentAvatarSrc}
                alt={draft.displayName || t("settings.characters.avatarAlt")}
                className="dossier-portrait-img"
              />
            ) : (
              <span
                className="dossier-portrait-fallback"
                style={{ background: draft.inkColor }}
              >
                {(draft.displayName || t("settings.characters.newInitial")).slice(0, 1)}
              </span>
            )}
            <div className="dossier-portrait-overlay">
              <span className="camera-icon">📷</span>
              <span className="overlay-tip">{t("settings.characters.changeAvatar")}</span>
            </div>
          </div>

          <div className="dossier-hero-info">
            <div className="dossier-hero-topline">
              <h2 className="dossier-hero-title">
                {draft.displayName || t("settings.characters.newDossier")}
              </h2>
              <div className="dossier-hero-badges">
                {selected?.presetId ? (
                  <span className="dossier-badge-preset">{t("settings.characters.factoryPreset")}</span>
                ) : (
                  <span className="dossier-badge-custom">{t("settings.characters.customCharacter")}</span>
                )}
                {draft.id && (
                  <button
                    type="button"
                    className={`dossier-cast-toggle-pill ${isDefaultSelected ? "is-active" : ""}`}
                    onClick={() => void toggleDefault(draft.id)}
                    title={t("settings.characters.toggleDefaultTitle")}
                  >
                    {isDefaultSelected ? t("settings.characters.inDefaultGuide") : t("settings.characters.joinDefaultCast")}
                  </button>
                )}
              </div>
            </div>

            <div className="dossier-hero-subline">
              {draft.workTitle && (
                <span className="dossier-hero-work">{t("settings.characters.quotedWork", { title: draft.workTitle })}</span>
              )}
              {draft.characterVersion && (
                <span className="dossier-hero-ver">{t("settings.characters.stage", { version: draft.characterVersion })}</span>
              )}
              <span className="dossier-hero-quote">
                {draft.description ? `“${draft.description}”` : t("settings.characters.noBlurb")}
              </span>
            </div>

            {/* Avatar management actions */}
            <div className="dossier-hero-avatar-actions">
              <button
                type="button"
                className="dossier-btn-micro"
                onClick={() => fileInputRef.current?.click()}
              >
                {t("settings.characters.uploadAvatar")}
              </button>
              {draft.avatarAssetId && (
                <button
                  type="button"
                  className="dossier-btn-micro danger"
                  onClick={handleRemoveAvatar}
                >
                  {t("settings.characters.removeAvatar")}
                </button>
              )}
              <span className="dossier-micro-hint">
                {t("settings.characters.avatarHint")}
              </span>
            </div>

            {/* Ink Color Picker & Swatches */}
            <div className="dossier-color-row">
              <span className="color-row-label">{t("settings.characters.inkLabel")}</span>
              <div className="color-swatches">
                {PRESET_INK_PALETTE.map((p) => (
                  <button
                    key={p.color}
                    type="button"
                    className={`swatch-circle ${draft.inkColor.toLowerCase() === p.color.toLowerCase() ? "is-active" : ""}`}
                    style={{ background: p.color }}
                    title={t(`settings.characters.ink.${p.id}`)}
                    onClick={() => {
                      setDraft({ ...draft, inkColor: p.color });
                      setDirty(true);
                    }}
                  />
                ))}
              </div>
              <div className="color-picker-control">
                <input
                  type="color"
                  className="native-color-picker"
                  value={draft.inkColor}
                  onChange={(e) => {
                    setDraft({ ...draft, inkColor: e.target.value });
                    setDirty(true);
                  }}
                  id="guide-ink-color-input"
                />
                <label htmlFor="guide-ink-color-input" className="color-code-label">
                  <span
                    className="color-dot-inline"
                    style={{ background: draft.inkColor }}
                  />
                  <code>{draft.inkColor.toUpperCase()}</code>
                </label>
              </div>
            </div>
          </div>
        </section>

        {/* Dossier Section 1: Basic Identity Information */}
        <section className="dossier-section-card">
          <div className="dossier-section-header">
            <div className="section-title-wrap">
              <span className="section-num">01</span>
              <h3>{t("settings.characters.sectionIdentity")}</h3>
            </div>
            <span className="section-desc">{t("settings.characters.sectionIdentityDesc")}</span>
          </div>

          <div className="dossier-grid-two">
            <label className="dossier-field">
              <span className="field-label">{t("settings.characters.displayName")} <em className="req">*</em></span>
              <input
                type="text"
                className="dossier-input"
                placeholder={t("settings.characters.displayNamePh")}
                value={draft.displayName}
                onChange={(e) => {
                  setDraft({ ...draft, displayName: e.target.value });
                  setDirty(true);
                }}
              />
            </label>

            <label className="dossier-field">
              <span className="field-label">{t("settings.characters.workSource")}</span>
              <input
                type="text"
                className="dossier-input"
                placeholder={t("settings.characters.workTitlePh")}
                value={draft.workTitle}
                onChange={(e) => {
                  setDraft({ ...draft, workTitle: e.target.value });
                  setDirty(true);
                }}
              />
            </label>

            <label className="dossier-field">
              <span className="field-label">{t("settings.characters.version")}</span>
              <input
                type="text"
                className="dossier-input"
                placeholder={t("settings.characters.versionPh")}
                value={draft.characterVersion}
                onChange={(e) => {
                  setDraft({ ...draft, characterVersion: e.target.value });
                  setDirty(true);
                }}
              />
            </label>

            <label className="dossier-field">
              <span className="field-label">{t("settings.characters.blurb")}</span>
              <input
                type="text"
                className="dossier-input"
                placeholder={t("settings.characters.blurbPh")}
                value={draft.description}
                onChange={(e) => {
                  setDraft({ ...draft, description: e.target.value });
                  setDirty(true);
                }}
              />
            </label>
          </div>
        </section>

        {/* Dossier Section 2: Margin Note Live Simulation */}
        <section className="dossier-section-card dossier-preview-section">
          <div className="dossier-section-header">
            <div className="section-title-wrap">
              <span className="section-num">02</span>
              <h3>{t("settings.characters.sectionPreview")}</h3>
            </div>
            <span className="section-desc">{t("settings.characters.sectionPreviewDesc")}</span>
          </div>

          <div className="dossier-live-preview-box">
            <article
              className="dossier-simulated-card"
              style={{ borderLeftColor: draft.inkColor }}
            >
              <div className="simulated-card-head">
                <span className="simulated-card-who">
                  <GuideAvatar
                    size={20}
                    persona={{
                      id: draft.id || "preview",
                      displayName: draft.displayName || t("settings.characters.newCharacter"),
                      color: draft.inkColor,
                      avatarSrc: currentAvatarSrc,
                    }}
                  />
                  <span className="simulated-name" style={{ color: draft.inkColor }}>
                    {draft.displayName || t("settings.characters.newCharacter")}
                  </span>
                </span>
                <span className="simulated-page-badge">{t("settings.characters.marginBadge")}</span>
              </div>
              <div className="simulated-card-body">
                {draft.exampleNotes[0]?.trim() ||
                  t("settings.characters.previewBody")}
              </div>
            </article>
            <div className="simulated-watermark">
              {t("settings.characters.previewWatermark")} <code>{draft.inkColor}</code>
            </div>
          </div>
        </section>

        {/* Dossier Section 3: Cognitive & Reading Profile */}
        <section className="dossier-section-card">
          <div className="dossier-section-header">
            <div className="section-title-wrap">
              <span className="section-num">03</span>
              <h3>{t("settings.characters.sectionMind")}</h3>
            </div>
            <span className="section-desc">{t("settings.characters.sectionMindDesc")}</span>
          </div>

          <div className="dossier-stack-fields">
            <label className="dossier-field">
              <div className="field-label-row">
                <span className="field-label">{t("settings.characters.personality")}</span>
                <span className="field-tip">{t("settings.characters.personalityTip")}</span>
              </div>
              <textarea
                rows={3}
                className="dossier-textarea"
                placeholder={t("settings.characters.personalityPh")}
                value={draft.personality}
                onChange={(e) => {
                  setDraft({ ...draft, personality: e.target.value });
                  setDirty(true);
                }}
              />
            </label>

            <label className="dossier-field">
              <div className="field-label-row">
                <span className="field-label">{t("settings.characters.habits")}</span>
                <span className="field-tip">{t("settings.characters.habitsTip")}</span>
              </div>
              <textarea
                rows={3}
                className="dossier-textarea"
                placeholder={t("settings.characters.habitsPh")}
                value={draft.readingHabits}
                onChange={(e) => {
                  setDraft({ ...draft, readingHabits: e.target.value });
                  setDirty(true);
                }}
              />
            </label>
          </div>
        </section>

        {/* Dossier Section 4: Voice & Behavioral Guardrails */}
        <section className="dossier-section-card">
          <div className="dossier-section-header">
            <div className="section-title-wrap">
              <span className="section-num">04</span>
              <h3>{t("settings.characters.sectionVoice")}</h3>
            </div>
            <span className="section-desc">{t("settings.characters.sectionVoiceDesc")}</span>
          </div>

          <div className="dossier-stack-fields">
            <label className="dossier-field">
              <div className="field-label-row">
                <span className="field-label">{t("settings.characters.expression")}</span>
                <span className="field-tip">{t("settings.characters.expressionTip")}</span>
              </div>
              <textarea
                rows={3}
                className="dossier-textarea"
                placeholder={t("settings.characters.expressionPh")}
                value={draft.expressionStyle}
                onChange={(e) => {
                  setDraft({ ...draft, expressionStyle: e.target.value });
                  setDirty(true);
                }}
              />
            </label>

            <label className="dossier-field">
              <div className="field-label-row">
                <span className="field-label">{t("settings.characters.avoidances")}</span>
                <span className="field-tip">{t("settings.characters.avoidancesTip")}</span>
              </div>
              <textarea
                rows={3}
                className="dossier-textarea"
                placeholder={t("settings.characters.avoidancesPh")}
                value={draft.avoidances}
                onChange={(e) => {
                  setDraft({ ...draft, avoidances: e.target.value });
                  setDirty(true);
                }}
              />
            </label>
          </div>
        </section>

        {/* Dossier Section 5: Exemplary Annotations */}
        <section className="dossier-section-card">
          <div className="dossier-section-header">
            <div className="section-title-wrap">
              <span className="section-num">05</span>
              <h3>{t("settings.characters.sectionExamples")}</h3>
            </div>
            <span className="section-desc">{t("settings.characters.sectionExamplesDesc")}</span>
          </div>

          <div className="dossier-stack-fields">
            <label className="dossier-field">
              <textarea
                rows={3}
                className="dossier-textarea"
                placeholder={t("settings.characters.examplesPh")}
                value={draft.exampleNotes.join("\n")}
                onChange={(e) => {
                  setDraft({
                    ...draft,
                    exampleNotes: e.target.value.split("\n"),
                  });
                  setDirty(true);
                }}
              />
            </label>
          </div>
        </section>

        <GuideCharacterPreview key={draft.id || "new"} draft={draft} characters={store.characters}
          onAdopt={(body) => {
            setDraft((current) => ({ ...current, exampleNotes: [...current.exampleNotes.filter((note) => note.trim()), body] }));
            setDirty(true);
            setStatus(t("settings.characters.sampleAdopted"));
          }} />

        {/* Bottom Floating Actions Toolbar */}
        <div className="dossier-action-bar">
          <div className="action-bar-status">
            {error && <span className="status-error">⚠️ {error}
              <button type="button" onClick={() => {
                void desktopClient.open<GuideCharacterSettings>("get_guide_character_settings")
                  .then((next) => { setStore(next); setError(null); setStatus(t("settings.characters.reloaded")); })
                  .catch((reason) => setError(String(reason)));
              }}>{t("settings.characters.reloadKeepDraft")}</button>
            </span>}
            {!error && status && <span className="status-success">✓ {status}</span>}
            {!error && !status && dirty && (
              <span className="status-dirty">{t("settings.characters.dirty")}</span>
            )}
            {!error && !status && !dirty && (
              <span className="status-clean">{t("settings.characters.clean")}</span>
            )}
          </div>

          <div className="action-bar-buttons">
            {confirmDelete ? (
              <div className="delete-confirm-group">
                <span className="delete-prompt">{t("settings.characters.confirmRemove")}</span>
                <button
                  type="button"
                  className="btn-liquid-pill danger"
                  onClick={() => void run("delete_guide_character")}
                >
                  {t("settings.characters.confirmDelete")}
                </button>
                <button
                  type="button"
                  className="btn-liquid-pill"
                  onClick={() => setConfirmDelete(false)}
                >
                  {t("settings.characters.cancel")}
                </button>
              </div>
            ) : (
              <>
                {selected && (
                  <button
                    type="button"
                    className="btn-liquid-pill danger-ghost"
                    onClick={() => setConfirmDelete(true)}
                  >
                    {t("settings.characters.removeCharacter")}
                  </button>
                )}
                {selected?.presetId && (
                  <button
                    type="button"
                    className="btn-liquid-pill"
                    onClick={() => void run("restore_guide_character_preset")}
                    title={t("settings.characters.restorePresetTitle")}
                  >
                    {t("settings.characters.restorePreset")}
                  </button>
                )}
                {selected && (
                  <button
                    type="button"
                    className="btn-liquid-pill"
                    onClick={() => void run("duplicate_guide_character")}
                  >
                    {t("settings.characters.duplicate")}
                  </button>
                )}
                <button
                  type="button"
                  className="btn-liquid-pill primary"
                  onClick={() => void save()}
                >
                  {t("settings.characters.save")}
                </button>
              </>
            )}
          </div>
        </div>
      </main>

      {lineupModalOpen && (
        <GuideLineupManagerModal
          open={lineupModalOpen}
          store={store}
          avatarCache={avatarCache}
          onClose={() => setLineupModalOpen(false)}
          onSavePresets={savePresetCasts}
          onSaveDefaultCast={saveDefaultCast}
          onRestoreFactoryPresets={restoreFactoryPresets}
        />
      )}
    </div>
  );
}
