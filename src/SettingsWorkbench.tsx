import React, { useRef, useState, useEffect } from "react";
import { desktopClient } from "./desktopClient";
import type {
  DocumentKind,
  ModelSettings,
  PromptSettings,
  PromptSlotId,
  StorageReport,
  WorkspaceInfo,
} from "./types";
import type { UiLocale } from "./i18n/types";
import {
  type ChineseFontId,
  type FontPreferences,
  type LatinFontId,
  applyFontPreferences,
  getSavedFontPreferences,
} from "./fontFamily";
import { SettingsModelsPage } from "./SettingsModelsPage";
import { SettingsWorkspacePage } from "./SettingsWorkspacePage";
import PromptCatalogSection from "./PromptCatalogSection";
import SettingsGuideCharactersPage from "./SettingsGuideCharactersPage";
import { useDialogFocusTrap } from "./useDialogFocusTrap";
import { useLocale } from "./i18n/LocaleContext";

export type SettingsSection = "workspace" | "models" | "prompts" | "characters";

type LegacyResetExecution = {
  workspace: WorkspaceInfo;
  backupPath: string;
  fileCount: number;
  pdfCount: number;
  totalBytes: number;
};

export type SettingsWorkbenchProps = {
  open: boolean;
  initialSection: SettingsSection;
  initialProviderId?: string | null;
  focusNonce?: number;
  workspace: WorkspaceInfo | null;
  modelSettings: ModelSettings;
  workspaceBusy: boolean;
  onClose: () => void;
  onChooseWorkspace: () => Promise<void>;
  onResetWorkspace: (
    previewDigest: string,
  ) => Promise<LegacyResetExecution | void>;
  onOpenWorkspace: () => Promise<void>;
  onModelSettingsChange: (settings: ModelSettings) => void;
  /** Replay the library interaction tour (D-063 §6.5); omit when the caller is not in Hub view */
  onReplayHubTour?: () => void;
  locale: UiLocale;
  onRequestLocaleChange: (next: UiLocale) => void;
};

export function SettingsWorkbench({
  open,
  initialSection,
  initialProviderId,
  focusNonce,
  workspace,
  modelSettings,
  workspaceBusy,
  onClose,
  onChooseWorkspace,
  onResetWorkspace,
  onOpenWorkspace,
  onModelSettingsChange,
  onReplayHubTour,
  locale,
  onRequestLocaleChange,
}: SettingsWorkbenchProps) {
  const { t } = useLocale();
  const [charactersDirty, setCharactersDirty] = useState(false);
  const [section, setSection] = useState<SettingsSection>(initialSection);
  const [statusNotice, setStatusNotice] = useState<string | null>(null);
  const [storageReport, setStorageReport] = useState<StorageReport | null>(null);
  const [resetConfirmOpen, setResetConfirmOpen] = useState(false);
  const [resetConfirmText, setResetConfirmText] = useState("");
  const [resetBusy, setResetBusy] = useState(false);
  const [resetPreview, setResetPreview] = useState<null | {
    fileCount: number;
    pdfCount: number;
    totalBytes: number;
    backupPath: string;
    legacyDatabasePath: string;
    legacyLibraryPath: string;
    warnings: string[];
    canProceed: boolean;
    previewDigest: string;
  }>(null);
  const [resetPreviewBusy, setResetPreviewBusy] = useState(false);
  const [resetPreviewError, setResetPreviewError] = useState<string | null>(null);
  const [resetResult, setResetResult] = useState<null | { backupPath: string }>(null);
  const settingsDialogRef = useRef<HTMLElement>(null);
  const resetDialogRef = useRef<HTMLDivElement>(null);
  const resetInputRef = useRef<HTMLInputElement>(null);

  function mayLeaveCharacters() {
    return section !== "characters" || !charactersDirty || window.confirm(t("settings.leaveUnsaved"));
  }
  function closeSettings() {
    if (!mayLeaveCharacters()) return;
    setCharactersDirty(false);
    onClose();
  }
  function navigateSection(next: SettingsSection) {
    if (next === section || !mayLeaveCharacters()) return;
    setCharactersDirty(false);
    setSection(next);
  }
  useDialogFocusTrap({
    open,
    containerRef: settingsDialogRef,
    onClose: closeSettings,
  });
  useDialogFocusTrap({
    open: open && resetConfirmOpen,
    containerRef: resetDialogRef,
    initialFocusRef: resetInputRef,
    onClose: () => {
      if (!resetBusy) setResetConfirmOpen(false);
    },
  });

  // Prompt catalog state
  const [promptSettings, setPromptSettings] = useState<PromptSettings | null>(null);
  const promptDrafts = useRef(new Map<string, string>());
  const currentLocale = useRef(locale);
  currentLocale.current = locale;
  const [promptLoadedLocale, setPromptLoadedLocale] = useState<UiLocale | null>(null);
  const [promptBusy, setPromptBusy] = useState(false);
  const [promptError, setPromptError] = useState<string | null>(null);

  // Font preferences
  const [fontPrefs, setFontPrefs] = useState<FontPreferences>(() =>
    getSavedFontPreferences(),
  );

  useEffect(() => {
    if (open) {
      setSection(initialSection);
    }
  }, [open, initialSection]);

  // Each read is scoped to its requested bank; a late response cannot replace another language.
  useEffect(() => {
    let cancelled = false;
    if (open) {
      setPromptLoadedLocale(null);
      void desktopClient
        .open<StorageReport>("get_storage_report")
        .then(setStorageReport)
        .catch(() => undefined);

      void desktopClient
        .open<PromptSettings>("get_prompt_settings", { locale })
        .then((prompts) => {
          if (cancelled) return;
          setPromptLoadedLocale(locale);
          setPromptSettings(prompts);
          setPromptError(null);
        })
        .catch((e) => { if (!cancelled) setPromptError(String(e)); });
    }
    return () => { cancelled = true; };
  }, [open, locale]);

  // Auto-dismiss status notice after 4 seconds
  useEffect(() => {
    if (statusNotice) {
      const timer = setTimeout(() => setStatusNotice(null), 4000);
      return () => clearTimeout(timer);
    }
  }, [statusNotice]);

  const handleSelectLatinFont = (latin: LatinFontId) => {
    const next = { ...fontPrefs, latin };
    setFontPrefs(next);
    applyFontPreferences(next);
  };

  const handleSelectChineseFont = (chinese: ChineseFontId) => {
    const next = { ...fontPrefs, chinese };
    setFontPrefs(next);
    applyFontPreferences(next);
  };

  const handleRunPromptCommand = async (
    command: "save_prompt_slot" | "restore_prompt_previous" | "restore_prompt_default" | "restore_outline_prompt_bundle",
    request: { slot?: PromptSlotId; text?: string; kind: DocumentKind },
  ) => {
    const requestedLocale = locale;
    setPromptBusy(true);
    setPromptError(null);
    try {
      const next = await desktopClient.command<PromptSettings>(command, {
        request: { ...request, locale: requestedLocale },
      });
      const slots = request.slot ? [request.slot] : ["outline_extract", "outline_compose", "outline_deep_dive"];
      for (const slot of slots) promptDrafts.current.delete(`${requestedLocale}:${request.kind}:${slot}`);
      if (currentLocale.current !== requestedLocale) return;
      setPromptSettings(next);
      setStatusNotice(t("settings.promptsUpdated"));
    } catch (e) {
      if (currentLocale.current === requestedLocale) setPromptError(String(e));
    } finally {
      setPromptBusy(false);
    }
  };

  const handleOpenResetDialog = async () => {
    setResetConfirmOpen(true);
    setResetConfirmText("");
    setResetResult(null);
    setResetPreviewError(null);
    if (!workspace?.rootPath) {
      setResetPreview(null);
      return;
    }
    setResetPreviewBusy(true);
    try {
      const preview = await desktopClient.command<{
        fileCount: number;
        pdfCount: number;
        totalBytes: number;
        backupPath: string;
        legacyDatabasePath: string;
        legacyLibraryPath: string;
        warnings: string[];
        canProceed: boolean;
        previewDigest: string;
      }>("inspect_legacy_reset", { rootPath: workspace.rootPath });
      setResetPreview(preview);
    } catch (e) {
      setResetPreviewError(String(e));
      setResetPreview(null);
    } finally {
      setResetPreviewBusy(false);
    }
  };

  const handleExecuteResetWorkspace = async () => {
    if (resetConfirmText.trim() !== "RESET") return;
    if (!resetPreview?.canProceed || !resetPreview.previewDigest) return;
    setResetBusy(true);
    setResetPreviewError(null);
    try {
      const result = await onResetWorkspace(resetPreview.previewDigest);
      if (result) {
        setResetResult({ backupPath: result.backupPath });
        setStatusNotice(t("settings.reset.archivedTo", { path: result.backupPath }));
        return;
      }
      setResetConfirmOpen(false);
      setResetConfirmText("");
      onClose();
    } catch (e) {
      setStatusNotice(t("settings.reset.failed", { error: String(e) }));
      setResetPreviewError(String(e));
    } finally {
      setResetBusy(false);
    }
  };

  if (!open) return null;

  return (
    <section
      ref={settingsDialogRef}
      className="settings-workbench"
      role="dialog"
      aria-modal="true"
      aria-label={t("settings.dialogAria")}
    >
      {/* 1. Floating Island Header */}
      <header className="settings-floating-island">
        <div className="settings-island-left">
          <button
            type="button"
            className="settings-back-pill"
            onClick={closeSettings}
            title={t("settings.backTitle")}
          >
            <span className="back-arrow">‹</span>
            <span>{t("settings.back")}</span>
            <kbd>Esc</kbd>
          </button>
        </div>

        <div className="settings-title-lockup">
          <span className="settings-seal">R</span>
          <div className="settings-title-text">
            <div className="settings-brand-row">
              <strong>{t("settings.brand")}</strong>
              <span className="settings-section-badge">
                {section === "workspace" && t("settings.badge.workspace")}
                {section === "models" && t("settings.badge.models")}
                {section === "prompts" && t("settings.badge.prompts")}
                {section === "characters" && t("settings.badge.characters")}
              </span>
            </div>
            <small>{t("settings.atlasSubtitle")}</small>
          </div>
        </div>

        <div className="settings-island-right">
          {statusNotice && (
            <div className="settings-status-toast">
              <i className="status-toast-dot" />
              <span>{statusNotice}</span>
            </div>
          )}
          <button
            type="button"
            className="settings-close-btn"
            onClick={closeSettings}
            title={t("settings.closeTitle")}
            aria-label={t("settings.closeAria")}
          >
            ✕
          </button>
        </div>
      </header>

      {/* 2. Full-page Layout: Left Rail + Right Content */}
      <div className="settings-layout">
        {/* Left Navigation Rail */}
        <aside className="settings-rail">
          <div className="settings-rail-heading">
            <span>{t("settings.contents")}</span>
            <small>{t("settings.contentsHint")}</small>
          </div>

          <nav className="settings-rail-nav" aria-label={t("settings.railAria")}>
            <button
              type="button"
              className={`settings-rail-link ${section === "workspace" ? "active" : ""}`}
              onClick={() => navigateSection("workspace")}
            >
              <span className="settings-rail-glyph">1</span>
              <div className="rail-link-text">
                <strong>{t("settings.rail.workspace")}</strong>
                <small>{t("settings.rail.workspaceHint")}</small>
              </div>
              <em className="rail-arrow">›</em>
            </button>

            <button
              type="button"
              className={`settings-rail-link ${section === "models" ? "active" : ""}`}
              onClick={() => navigateSection("models")}
            >
              <span className="settings-rail-glyph">2</span>
              <div className="rail-link-text">
                <div className="rail-link-title-row">
                  <strong>{t("settings.rail.models")}</strong>
                  <span className="rail-count-badge">{modelSettings.providers.length}</span>
                </div>
                <small>{t("settings.rail.modelsHint")}</small>
              </div>
              <em className="rail-arrow">›</em>
            </button>

            <button
              type="button"
              className={`settings-rail-link ${section === "prompts" ? "active" : ""}`}
              onClick={() => navigateSection("prompts")}
            >
              <span className="settings-rail-glyph">3</span>
              <div className="rail-link-text">
                <strong>{t("settings.rail.prompts")}</strong>
                <small>{t("settings.rail.promptsHint")}</small>
              </div>
              <em className="rail-arrow">›</em>
            </button>
            <button
              type="button"
              className={`settings-rail-link ${section === "characters" ? "active" : ""}`}
              onClick={() => navigateSection("characters")}
            >
              <span className="settings-rail-glyph">4</span>
              <div className="rail-link-text">
                <strong>{t("settings.rail.characters")}</strong>
                <small>{t("settings.rail.charactersHint")}</small>
              </div>
              <em className="rail-arrow">›</em>
            </button>
          </nav>

          <div className="settings-rail-note">
            <span>{t("settings.atlas")}</span>
            <p>{t("settings.atlasFoot")}</p>
          </div>
        </aside>

        {/* Right Scrollable Content Pane */}
        <main className={`settings-content ${section === "prompts" ? "settings-content-prompts" : ""}`}>
          {section === "models" && (
            <div className="settings-page">
              <div className="settings-page-head">
                <span className="eyebrow">{t("settings.page.modelsEyebrow")}</span>
                <h1>{t("settings.page.modelsTitle")}</h1>
                <p>
                  {t("settings.page.modelsIntro")}
                </p>
              </div>
              <SettingsModelsPage
                modelSettings={modelSettings}
                initialProviderId={initialProviderId}
                focusNonce={focusNonce}
                onModelSettingsChange={onModelSettingsChange}
                setStatus={setStatusNotice}
              />
            </div>
          )}

          {section === "workspace" && (
            <div className="settings-page">
              <div className="settings-page-head">
                <span className="eyebrow">{t("settings.page.workspaceEyebrow")}</span>
                <h1>{t("settings.page.workspaceTitle")}</h1>
                <p>
                  {t("settings.page.workspaceIntro")}
                </p>
              </div>
              <SettingsWorkspacePage
                workspace={workspace}
                storageReport={storageReport}
                workspaceBusy={workspaceBusy}
                latinFont={fontPrefs.latin}
                chineseFont={fontPrefs.chinese}
                onLatinFontChange={handleSelectLatinFont}
                onChineseFontChange={handleSelectChineseFont}
                onChooseWorkspace={onChooseWorkspace}
                onOpenWorkspace={onOpenWorkspace}
                onResetWorkspaceClick={() => void handleOpenResetDialog()}
                onReplayHubTour={onReplayHubTour}
                setStatus={setStatusNotice}
                locale={locale}
                onRequestLocaleChange={onRequestLocaleChange}
              />
            </div>
          )}

          {section === "prompts" && (
            <div className="settings-page settings-page-prompts-full">
              <div className="settings-page-head">
                <span className="eyebrow">{t("settings.page.promptsEyebrow")}</span>
                <h1>{t("settings.page.promptsTitle")}</h1>
                <p>
                  {t("settings.page.promptsIntro")}
                </p>
              </div>
              {promptSettings && promptLoadedLocale === locale ? (
                <PromptCatalogSection
                  key={locale}
                  drafts={promptDrafts.current}
                  settings={promptSettings}
                  busy={promptBusy}
                  error={promptError}
                  onSave={(slot, text, kind) =>
                    void handleRunPromptCommand("save_prompt_slot", { slot, text, kind })
                  }
                  onRestorePrevious={(slot, kind) =>
                    void handleRunPromptCommand("restore_prompt_previous", { slot, kind })
                  }
                  onRestoreDefault={(slot, kind) =>
                    void handleRunPromptCommand("restore_prompt_default", { slot, kind })
                  }
                  onRestoreOutlineBundle={(kind) =>
                    void handleRunPromptCommand("restore_outline_prompt_bundle", { kind })
                  }
                />
              ) : (
                <div className="loading-state">{t("settings.page.promptsLoading")}</div>
              )}
            </div>
          )}

          {section === "characters" && (
            <div className="settings-page">
              <div className="settings-page-head">
                <span className="eyebrow">{t("settings.page.charactersEyebrow")}</span>
                <h1>{t("settings.page.charactersTitle")}</h1>
                <p>{t("settings.page.charactersIntro")}</p>
              </div>
              <SettingsGuideCharactersPage onDirtyChange={setCharactersDirty} />
            </div>
          )}
        </main>
      </div>

      {/* Danger Zone: Reset Workspace Modal */}
      {resetConfirmOpen && (
        <div
          ref={resetDialogRef}
          className="modal-scrim"
          onClick={() => {
            if (!resetBusy) setResetConfirmOpen(false);
          }}
        >
          <div
            className="confirm-modal-box danger-modal"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="modal-header">
              <h3 className="text-danger">{t("settings.reset.title")}</h3>
              <button
                type="button"
                className="btn-icon"
                disabled={resetBusy}
                onClick={() => setResetConfirmOpen(false)}
              >
                ✕
              </button>
            </div>
            <div className="modal-body">
              {resetPreviewBusy ? (
                <p>{t("settings.reset.inspecting")}</p>
              ) : resetPreviewError ? (
                <p className="text-danger">{t("settings.reset.inspectFailed", { error: resetPreviewError })}</p>
              ) : resetPreview ? (
                <>
                  <p>
                    {t("settings.reset.bodyPreview", {
                      fileCount: resetPreview.fileCount,
                      pdfCount: resetPreview.pdfCount,
                      mb: (resetPreview.totalBytes / 1024 / 1024).toFixed(1),
                      backupPath: resetPreview.backupPath,
                    })}
                  </p>
                  <div style={{ fontSize: 12, color: "var(--muted)", margin: "8px 0", wordBreak: "break-all" }}>
                    <div>{t("settings.reset.legacyDb", { path: resetPreview.legacyDatabasePath })}</div>
                    <div>{t("settings.reset.legacyLibrary", { path: resetPreview.legacyLibraryPath })}</div>
                    <div>{t("settings.reset.backupDir", { path: resetPreview.backupPath })}</div>
                    {resetPreview.warnings.length > 0 && <div className="text-danger">⚠️ {resetPreview.warnings.join(t("settings.listSep"))}</div>}
                  </div>
                  <p style={{ fontSize: 12, color: "var(--muted)" }}>
                    {t("settings.reset.papersKept")}
                  </p>
                  {resetResult && (
                    <div style={{ background: "var(--glass-surface)", padding: 8, borderRadius: 8, marginTop: 8 }}>
                      <strong>{t("settings.reset.done")}</strong>
                      <div style={{ fontSize: 11, wordBreak: "break-all" }}>{t("settings.reset.backupLabel", { path: resetResult.backupPath })}</div>
                      <button
                        type="button"
                        className="btn-sm btn-outline-primary"
                        style={{ marginTop: 6 }}
                        onClick={() => {
                          void navigator.clipboard
                            .writeText(resetResult.backupPath)
                            .then(() => setStatusNotice(t("settings.reset.copied")))
                            .catch((error) =>
                              setStatusNotice(t("settings.reset.copyFailed", { error: String(error) })),
                            );
                        }}
                      >
                        {t("settings.reset.copyBackup")}
                      </button>
                    </div>
                  )}
                </>
              ) : (
                <p>
                  {t("settings.reset.bodyFallback")}
                </p>
              )}
              <div className="confirm-input-wrap">
                <label>{t("settings.reset.typeReset")}</label>
                <input
                  ref={resetInputRef}
                  type="text"
                  className="input-text"
                  placeholder="RESET"
                  value={resetConfirmText}
                  onChange={(e) => setResetConfirmText(e.target.value)}
                />
              </div>
            </div>
            <div className="modal-footer">
              <button
                type="button"
                className="btn-ghost"
                onClick={() => setResetConfirmOpen(false)}
                disabled={resetBusy}
              >
                {t("settings.reset.cancel")}
              </button>
              {resetResult ? (
                <button type="button" className="btn-primary" onClick={() => { setResetConfirmOpen(false); onClose(); }}>
                  {t("settings.reset.doneAndReturn")}
                </button>
              ) : (
                <button
                  type="button"
                  className="btn-danger"
                  disabled={
                    resetConfirmText.trim() !== "RESET" ||
                    resetBusy ||
                    resetPreviewBusy ||
                    !resetPreview?.canProceed ||
                    !resetPreview.previewDigest
                  }
                  onClick={() => void handleExecuteResetWorkspace()}
                >
                  {resetBusy ? t("settings.reset.resetting") : t("settings.reset.confirm")}
                </button>
              )}
            </div>
          </div>
        </div>
      )}
    </section>
  );
}

export default SettingsWorkbench;

