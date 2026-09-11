import { localizedCharacter, localizedCastName } from "../i18n/guidePresets";
import { useEffect, useMemo, useRef, useState } from "react";
import { desktopClient } from "../desktopClient";
import { useLocale } from "../i18n/LocaleContext";
import type { GuideCharacterSettings } from "../types";
import GuideAvatar from "./GuideAvatar";

export default function GuideCastPicker({
  settings: rawSettings,
  selectedIds,
  onChange,
  onOpenSettings,
}: {
  settings: GuideCharacterSettings | null;
  selectedIds: string[];
  onChange: (ids: string[]) => void;
  onOpenSettings: () => void;
}) {
  const { t, locale } = useLocale();
  const settings = useMemo(() => rawSettings ? { ...rawSettings, characters: rawSettings.characters.map(character => localizedCharacter(character, locale)) } : null, [rawSettings, locale]);
  const [avatars, setAvatars] = useState<Record<string, string>>({});
  const [dropdownOpen, setDropdownOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;
    for (const id of new Set(settings?.characters.map((item) => item.avatarAssetId).filter(Boolean) ?? [])) {
      if (!id) continue;
      void desktopClient.command<{ dataUrl: string }>("get_guide_character_avatar", { request: { assetId: id } })
        .then((value) => { if (!cancelled) setAvatars((current) => ({ ...current, [id]: value.dataUrl })); })
        .catch(() => undefined);
    }
    return () => { cancelled = true; };
  }, [settings]);

  // Click outside to dismiss dropdown
  useEffect(() => {
    if (!dropdownOpen) return;
    function handleClickOutside(e: MouseEvent) {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setDropdownOpen(false);
      }
    }
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        setDropdownOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [dropdownOpen]);

  if (!settings) {
    return (
      <div className="guide-cast-picker">
        <p className="guide-plan-description">
          {t("guide.cast.needCharacters")}
          <button type="button" className="btn-liquid-pill" onClick={onOpenSettings}>
            {t("guide.cast.openCharacters")}
          </button>
        </p>
      </div>
    );
  }

  const unavailable = selectedIds.filter((id) => !settings.characters.some((item) => item.id === id));
  const selectedCharacters = settings.characters.filter((c) => selectedIds.includes(c.id));
  const nameSeparator = t("guide.cast.nameListSeparator");

  return (
    <div className="guide-cast-picker" ref={containerRef}>
      {/* 1. Presets Pill Row */}
      <div className="guide-cast-presets-bar">
        <span className="guide-cast-presets-label">{t("guide.cast.presets")}</span>
        <div className="guide-cast-presets-pills">
          {settings.presetCasts.map((cast) => {
            const isSelected =
              cast.characterIds.length === selectedIds.length &&
              cast.characterIds.every((id) => selectedIds.includes(id));
            return (
              <button
                type="button"
                className={`btn-liquid-pill ${isSelected ? "primary" : ""}`}
                key={cast.id}
                onClick={() => onChange(cast.characterIds)}
                title={t("guide.cast.applyCast", { name: localizedCastName(cast, t), count: cast.characterIds.length })}
              >
                {localizedCastName(cast, t)}
              </button>
            );
          })}
        </div>
        <button
          type="button"
          className="btn-liquid-pill cast-settings-link"
          onClick={onOpenSettings}
          title={t("guide.cast.manageTitle")}
        >
          {t("guide.cast.manage")}
        </button>
      </div>

      {/* 2. Unavailable Warning */}
      {unavailable.length > 0 && (
        <div className="guide-cast-unavailable" role="alert">
          <span>{t("guide.cast.unavailable", { ids: unavailable.join(", ") })}</span>
          <button
            type="button"
            className="btn-liquid-pill danger-pill"
            onClick={() => onChange(selectedIds.filter((item) => !unavailable.includes(item)))}
          >
            {t("guide.cast.removeUnavailable")}
          </button>
        </div>
      )}

      {/* 3. Dropdown Trigger */}
      <div className="guide-cast-dropdown-wrapper">
        <button
          type="button"
          className={`guide-cast-dropdown-trigger ${dropdownOpen ? "is-open" : ""} ${selectedIds.length === 0 ? "has-error" : ""}`}
          onClick={() => setDropdownOpen((prev) => !prev)}
          aria-haspopup="listbox"
          aria-expanded={dropdownOpen}
          aria-label={t("guide.cast.selectAria")}
        >
          <div className="guide-cast-avatar-stack">
            {selectedCharacters.length > 0 ? (
              selectedCharacters.slice(0, 4).map((char, index) => (
                <div
                  key={char.id}
                  className="stack-avatar-wrap"
                  style={{ zIndex: 10 - index }}
                >
                  <GuideAvatar
                    size={24}
                    persona={{
                      id: char.id,
                      displayName: char.displayName,
                      color: char.inkColor,
                      avatarSrc: char.avatarAssetId ? avatars[char.avatarAssetId] : null,
                    }}
                  />
                </div>
              ))
            ) : (
              <span className="stack-avatar-empty">👤</span>
            )}
            {selectedCharacters.length > 4 && (
              <span className="stack-avatar-more">+{selectedCharacters.length - 4}</span>
            )}
          </div>

          <div className="guide-cast-trigger-info">
            <span className="guide-cast-trigger-count">
              {selectedIds.length === 0 ? t("guide.cast.noneSelected") : t("guide.cast.selectedCount", { count: selectedIds.length })}
            </span>
            <span className="guide-cast-trigger-names" title={selectedCharacters.map((c) => c.displayName).join(nameSeparator)}>
              {selectedIds.length === 0
                ? t("guide.cast.expandHint")
                : selectedCharacters.map((c) => c.displayName).join(nameSeparator)}
            </span>
          </div>

          <span className="guide-cast-trigger-arrow" aria-hidden="true">
            {dropdownOpen ? "▴" : "▾"}
          </span>
        </button>

        {/* 4. Dropdown Menu */}
        {dropdownOpen && (
          <div
            className="guide-cast-dropdown-menu"
            role="listbox"
            aria-label={t("guide.cast.listAria")}
          >
            <div className="guide-cast-menu-header">
              <span className="menu-header-title">
                {t("guide.cast.castCount", { selected: selectedIds.length, total: settings.characters.length })}
              </span>
              <div className="menu-header-actions">
                <button
                  type="button"
                  className="cast-header-action-btn"
                  onClick={() => onChange(settings.characters.map((c) => c.id))}
                >
                  {t("guide.cast.selectAll")}
                </button>
                <button
                  type="button"
                  className="cast-header-action-btn"
                  onClick={() => onChange([])}
                >
                  {t("guide.cast.clear")}
                </button>
              </div>
            </div>

            <div className="guide-cast-dropdown-list">
              {settings.characters.map((character) => {
                const checked = selectedIds.includes(character.id);
                return (
                  <label
                    key={character.id}
                    className={`guide-cast-dropdown-item ${checked ? "is-selected" : ""}`}
                    role="option"
                    aria-selected={checked}
                  >
                    <input
                      type="checkbox"
                      className="dropdown-item-checkbox"
                      checked={checked}
                      onChange={() => {
                        if (checked) {
                          onChange(selectedIds.filter((id) => id !== character.id));
                        } else {
                          onChange([...selectedIds, character.id]);
                        }
                      }}
                      aria-label={character.displayName}
                    />
                    <GuideAvatar
                      size={26}
                      persona={{
                        id: character.id,
                        displayName: character.displayName,
                        color: character.inkColor,
                        avatarSrc: character.avatarAssetId ? avatars[character.avatarAssetId] : null,
                      }}
                    />
                    <div className="dropdown-item-text">
                      <span className="dropdown-item-name">{character.displayName}</span>
                      {character.workTitle && (
                        <span className="dropdown-item-work">{t("guide.cast.workTitle", { title: character.workTitle })}</span>
                      )}
                    </div>
                    <span
                      className="dropdown-item-color-pill"
                      style={{ backgroundColor: character.inkColor }}
                      title={t("guide.cast.inkColor", { color: character.inkColor })}
                    />
                  </label>
                );
              })}
            </div>
          </div>
        )}
      </div>

      {/* 5. Zero Selected Error Hint */}
      {selectedIds.length === 0 && (
        <p className="guide-cast-empty-hint" role="status">
          {t("guide.cast.emptyHint")}
        </p>
      )}
    </div>
  );
}
