import { localizedCastName } from "../i18n/guidePresets";
import React, { useState } from "react";
import { useLocale } from "../i18n/LocaleContext";
import type { GuideCharacterSettings, GuidePresetCast } from "../types";
import GuideAvatar from "./GuideAvatar";
export type GuideLineupManagerModalProps = {
  open: boolean;
  store: GuideCharacterSettings;
  avatarCache: Record<string, string>;
  onClose: () => void;
  onSavePresets: (casts: GuidePresetCast[]) => Promise<void>;
  onSaveDefaultCast: (characterIds: string[]) => Promise<void>;
  onRestoreFactoryPresets: () => Promise<void>;
};

export default function GuideLineupManagerModal({
  open,
  store,
  avatarCache,
  onClose,
  onSavePresets,
  onSaveDefaultCast,
  onRestoreFactoryPresets,
}: GuideLineupManagerModalProps) {
  const [editingCastId, setEditingCastId] = useState<string | null>(null);
  const [editingName, setEditingName] = useState("");
  const [newCastName, setNewCastName] = useState("");
  const [newCastMembers, setNewCastMembers] = useState<string[]>(
    store.characters.slice(0, 2).map((c) => c.id),
  );
  const [actionError, setActionError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const { t } = useLocale();

  if (!open) return null;

  const factoryIds = ["daily", "evidence", "classics"];

  async function handleToggleMember(cast: GuidePresetCast, charId: string) {
    if (busy) return;
    setActionError(null);
    const inCast = cast.characterIds.includes(charId);
    if (inCast && cast.characterIds.length <= 1) {
      setActionError(t("guide.lineup.minMembers", { name: cast.name }));
      return;
    }
    const nextIds = inCast
      ? cast.characterIds.filter((id) => id !== charId)
      : [...cast.characterIds, charId];
    const nextCasts = store.presetCasts.map((c) =>
      c.id === cast.id ? { ...c, characterIds: nextIds } : c,
    );
    setBusy(true);
    try {
      await onSavePresets(nextCasts);
    } catch (err) {
      setActionError(String(err));
    } finally {
      setBusy(false);
    }
  }

  function startRename(cast: GuidePresetCast) {
    setEditingCastId(cast.id);
    setEditingName(cast.name);
    setActionError(null);
  }

  async function saveRename(castId: string) {
    const trimmed = editingName.trim();
    if (!trimmed) {
      setActionError(t("guide.lineup.nameEmpty"));
      return;
    }
    const nextCasts = store.presetCasts.map((c) =>
      c.id === castId ? { ...c, name: trimmed } : c,
    );
    setBusy(true);
    try {
      await onSavePresets(nextCasts);
      setEditingCastId(null);
    } catch (err) {
      setActionError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function handleDeleteCast(castId: string) {
    if (busy) return;
    setActionError(null);
    const nextCasts = store.presetCasts.filter((c) => c.id !== castId);
    if (nextCasts.length === 0) {
      setActionError(t("guide.lineup.minCasts"));
      return;
    }
    setBusy(true);
    try {
      await onSavePresets(nextCasts);
    } catch (err) {
      setActionError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function handleCreateNewCast() {
    const trimmed = newCastName.trim();
    if (!trimmed) {
      setActionError(t("guide.lineup.newNameEmpty"));
      return;
    }
    if (newCastMembers.length === 0) {
      setActionError(t("guide.lineup.newMinMembers"));
      return;
    }
    const newId = `cast_${Date.now()}_${Math.random().toString(36).slice(2, 6)}`;
    const nextCasts: GuidePresetCast[] = [
      ...store.presetCasts,
      { id: newId, name: trimmed, characterIds: newCastMembers },
    ];
    setBusy(true);
    try {
      await onSavePresets(nextCasts);
      setNewCastName("");
      setNewCastMembers(store.characters.slice(0, 2).map((c) => c.id));
      setActionError(null);
    } catch (err) {
      setActionError(String(err));
    } finally {
      setBusy(false);
    }
  }

  function toggleNewMember(charId: string) {
    setNewCastMembers((prev) =>
      prev.includes(charId) ? prev.filter((id) => id !== charId) : [...prev, charId],
    );
  }

  return (
    <div className="modal-scrim" onClick={onClose}>
      <div
        className="lineup-modal-box"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label={t("guide.lineup.dialogAria")}
      >
        <div className="modal-header">
          <div className="lineup-header-title">
            <h3>{t("guide.lineup.title")}</h3>
            <span className="lineup-header-badge">{t("guide.lineup.castCount", { count: store.presetCasts.length })}</span>
          </div>
          <button
            type="button"
            className="btn-icon"
            onClick={onClose}
            aria-label={t("guide.lineup.close")}
          >
            ✕
          </button>
        </div>

        <div className="lineup-modal-body">
          <p className="lineup-modal-tip">
            {t("guide.lineup.tip")}
          </p>

          {actionError && (
            <div className="lineup-error-banner" role="alert">
              ⚠️ {actionError}
            </div>
          )}

          {/* Lineups List */}
          <div className="lineup-cards-list">
            {store.presetCasts.map((cast) => {
              const isFactory = factoryIds.includes(cast.id);
              const isDefault =
                cast.characterIds.length === store.defaultCharacterIds.length &&
                cast.characterIds.every((id) => store.defaultCharacterIds.includes(id));
              const isEditing = editingCastId === cast.id;

              return (
                <div key={cast.id} className="lineup-card">
                  <div className="lineup-card-header">
                    <div className="lineup-card-title-col">
                      {isEditing ? (
                        <div className="lineup-rename-form">
                          <input
                            type="text"
                            className="input-text input-sm lineup-rename-input"
                            value={editingName}
                            onChange={(e) => setEditingName(e.target.value)}
                            onKeyDown={(e) => {
                              if (e.key === "Enter") void saveRename(cast.id);
                              if (e.key === "Escape") setEditingCastId(null);
                            }}
                            autoFocus
                          />
                          <button
                            type="button"
                            className="btn-sm btn-primary"
                            onClick={() => void saveRename(cast.id)}
                            disabled={busy}
                          >
                            {t("guide.lineup.save")}
                          </button>
                          <button
                            type="button"
                            className="btn-sm btn-ghost"
                            onClick={() => setEditingCastId(null)}
                          >
                            {t("guide.lineup.cancel")}
                          </button>
                        </div>
                      ) : (
                        <div className="lineup-title-row">
                          <strong className="lineup-name">{localizedCastName(cast, t)}</strong>
                          <button
                            type="button"
                            className="lineup-edit-btn"
                            onClick={() => startRename(cast)}
                            title={t("guide.lineup.renameTitle")}
                            aria-label={t("guide.lineup.renameAria", { name: cast.name })}
                          >
                            ✎
                          </button>
                          {isFactory ? (
                            <span className="item-preset-tag">{t("guide.lineup.factoryTag")}</span>
                          ) : (
                            <span className="item-custom-tag">{t("guide.lineup.customTag")}</span>
                          )}
                          {isDefault && (
                            <span className="lineup-default-active-tag">{t("guide.lineup.currentDefault")}</span>
                          )}
                        </div>
                      )}
                    </div>

                    <div className="lineup-card-actions">
                      {!isDefault && (
                        <button
                          type="button"
                          className="btn-sm btn-outline-primary"
                          onClick={() => void onSaveDefaultCast(cast.characterIds)}
                          disabled={busy}
                          title={t("guide.lineup.setDefaultTitle")}
                        >
                          {t("guide.lineup.setDefault")}
                        </button>
                      )}
                      <button
                        type="button"
                        className="btn-sm btn-ghost text-danger"
                        onClick={() => void handleDeleteCast(cast.id)}
                        disabled={busy}
                        title={t("guide.lineup.deleteTitle")}
                        aria-label={t("guide.lineup.deleteAria", { name: cast.name })}
                      >
                        {t("guide.lineup.delete")}
                      </button>
                    </div>
                  </div>

                  {/* Character Members Selector Chips */}
                  <div className="lineup-card-members-section">
                    <span className="lineup-members-label">
                      {t("guide.lineup.members", { count: cast.characterIds.length })}
                    </span>
                    <div className="lineup-card-members">
                      {store.characters.map((char) => {
                        const inCast = cast.characterIds.includes(char.id);
                        const avatarSrc = char.avatarAssetId
                          ? avatarCache[char.avatarAssetId] ?? null
                          : null;
                        return (
                          <button
                            type="button"
                            key={char.id}
                            className={`lineup-char-chip ${inCast ? "is-selected" : ""}`}
                            onClick={() => void handleToggleMember(cast, char.id)}
                            disabled={busy}
                            title={inCast ? t("guide.lineup.removeMember", { name: char.displayName }) : t("guide.lineup.addMember", { name: char.displayName })}
                          >
                            <GuideAvatar
                              size={18}
                              persona={{
                                id: char.id,
                                displayName: char.displayName,
                                color: char.inkColor,
                                avatarSrc,
                              }}
                            />
                            <span>{char.displayName}</span>
                            <span className="lineup-chip-status">{inCast ? "✓" : "+"}</span>
                          </button>
                        );
                      })}
                    </div>
                  </div>
                </div>
              );
            })}
          </div>

          {/* New Cast Creation Section */}
          <div className="lineup-create-box">
            <h4>{t("guide.lineup.createTitle")}</h4>
            <div className="lineup-create-form">
              <div className="lineup-create-input-row">
                <input
                  type="text"
                  className="input-text"
                  placeholder={t("guide.lineup.newNamePlaceholder")}
                  value={newCastName}
                  onChange={(e) => setNewCastName(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") void handleCreateNewCast();
                  }}
                />
                <button
                  type="button"
                  className="btn-primary"
                  onClick={() => void handleCreateNewCast()}
                  disabled={busy || !newCastName.trim() || newCastMembers.length === 0}
                >
                  {t("guide.lineup.create")}
                </button>
              </div>

              <div className="lineup-create-members-picker">
                <span className="lineup-members-label">
                  {t("guide.lineup.initialMembers", { count: newCastMembers.length })}
                </span>
                <div className="lineup-card-members">
                  {store.characters.map((char) => {
                    const inCast = newCastMembers.includes(char.id);
                    const avatarSrc = char.avatarAssetId
                      ? avatarCache[char.avatarAssetId] ?? null
                      : null;
                    return (
                      <button
                        type="button"
                        key={char.id}
                        className={`lineup-char-chip ${inCast ? "is-selected" : ""}`}
                        onClick={() => toggleNewMember(char.id)}
                      >
                        <GuideAvatar
                          size={18}
                          persona={{
                            id: char.id,
                            displayName: char.displayName,
                            color: char.inkColor,
                            avatarSrc,
                          }}
                        />
                        <span>{char.displayName}</span>
                        <span className="lineup-chip-status">{inCast ? "✓" : "+"}</span>
                      </button>
                    );
                  })}
                </div>
              </div>
            </div>
          </div>
        </div>

        <div className="modal-footer">
          <button
            type="button"
            className="btn-ghost text-danger"
            onClick={async () => {
              if (window.confirm(t("guide.lineup.restoreConfirm"))) {
                await onRestoreFactoryPresets();
              }
            }}
            disabled={busy}
            title={t("guide.lineup.restoreTitle")}
          >
            {t("guide.lineup.restore")}
          </button>
          <button
            type="button"
            className="btn-primary"
            onClick={onClose}
            disabled={busy}
          >
            {t("guide.lineup.done")}
          </button>
        </div>
      </div>
    </div>
  );
}
