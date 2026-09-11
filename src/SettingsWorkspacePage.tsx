import React, { useMemo } from "react";
import { desktopClient } from "./desktopClient";
import type { StorageReport, WorkspaceInfo } from "./types";
import {
  CHINESE_FONT_OPTIONS,
  LATIN_FONT_OPTIONS,
  type ChineseFontId,
  type LatinFontId,
  buildCombinedFontFamily,
} from "./fontFamily";
import MarkdownBody from "./MarkdownBody";
import { useLocale } from "./i18n/LocaleContext";
import type { UiLocale } from "./i18n/types";

interface SettingsWorkspacePageProps {
  workspace: WorkspaceInfo | null;
  storageReport: StorageReport | null;
  workspaceBusy?: boolean;
  latinFont: LatinFontId;
  chineseFont: ChineseFontId;
  onLatinFontChange: (font: LatinFontId) => void;
  onChineseFontChange: (font: ChineseFontId) => void;
  onChooseWorkspace?: () => Promise<void>;
  onOpenWorkspace?: () => Promise<void>;
  onResetWorkspaceClick: () => void;
  /** Replay the library interaction tour (D-063 §6.5); App only provides this in Hub view */
  onReplayHubTour?: () => void;
  setStatus: (status: string) => void;
  locale: UiLocale;
  onRequestLocaleChange: (next: UiLocale) => void;
}

function formatBytes(bytes?: number | null): string {
  if (bytes === undefined || bytes === null || bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(2))} ${sizes[i]}`;
}

const SAMPLE_FORMULA = `$$\\mathcal{L}(\\theta) = \\mathbb{E}_{(x, y) \\sim \\mathcal{D}} \\left[ -\\log P_\\theta(y \\mid x) \\right] + \\lambda \\|\\theta\\|_2^2$$`;

const CHINESE_PREVIEW_KEYS: Record<ChineseFontId, string> = {
  default: "fonts.chineseDefaultPreview",
  pingfang: "fonts.chinesePingfangPreview",
  kaiti: "fonts.chineseKaitiPreview",
  songti: "fonts.chineseSongtiPreview",
};

export const SettingsWorkspacePage: React.FC<SettingsWorkspacePageProps> = ({
  workspace,
  storageReport,
  workspaceBusy,
  latinFont,
  chineseFont,
  onLatinFontChange,
  onChineseFontChange,
  onChooseWorkspace,
  onOpenWorkspace,
  onResetWorkspaceClick,
  onReplayHubTour,
  setStatus,
  locale,
  onRequestLocaleChange,
}) => {
  const { t } = useLocale();
  const combinedFontFamily = useMemo(
    () => buildCombinedFontFamily(latinFont, chineseFont),
    [latinFont, chineseFont],
  );

  const handleOpenWorkspace = async () => {
    try {
      if (onOpenWorkspace) await onOpenWorkspace();
      else await desktopClient.command("open_workspace_dir");
    } catch (e) {
      const msg = String(e);
      if (msg.includes("cancelled") || msg.toLowerCase().includes("cancel")) return;
      setStatus(t("settings.workspace.openFailed", { error: msg }));
    }
  };

  const handleChooseWorkspace = async () => {
    if (workspaceBusy) {
      setStatus(t("settings.workspace.busy"));
      return;
    }
    try {
      if (onChooseWorkspace) await onChooseWorkspace();
      else await desktopClient.command("choose_workspace", { rootPath: "" });
    } catch (e) {
      const msg = String(e);
      if (msg.includes("cancelled") || msg.toLowerCase().includes("cancel")) return;
      setStatus(t("settings.workspace.chooseFailed", { error: msg }));
    }
  };

  const isDefaultFont = latinFont === "default" && chineseFont === "default";

  return (
    <div className="settings-workspace-page">
      <section className="settings-card language-settings-card">
        <div className="card-header">
          <div>
            <h3>{t("settings.language.title")}</h3>
            <p className="card-subtitle">{t("settings.language.subtitle")}</p>
          </div>
        </div>
        <div
          className="liquid-pill-group"
          role="group"
          aria-label={t("settings.language.title")}
        >
          <button
            type="button"
            className={`liquid-tab-btn ${locale === "zh-CN" ? "active" : ""}`}
            aria-pressed={locale === "zh-CN"}
            onClick={() => {
              if (locale === "zh-CN") return;
              onRequestLocaleChange("zh-CN");
            }}
          >
            中文
          </button>
          <button
            type="button"
            className={`liquid-tab-btn ${locale === "en" ? "active" : ""}`}
            aria-pressed={locale === "en"}
            onClick={() => {
              if (locale === "en") return;
              onRequestLocaleChange("en");
            }}
          >
            English
          </button>
        </div>
      </section>
      {/* 1. Font Configuration Section */}
      <section className="settings-card font-settings-card">
        <div className="card-header">
          <div>
            <h3>{t("settings.workspace.fontsTitle")}</h3>
            <p className="card-subtitle">
              {t("settings.workspace.fontsSubtitle")}
            </p>
          </div>
          {!isDefaultFont && (
            <button
              type="button"
              className="btn-sm btn-ghost"
              onClick={() => {
                onLatinFontChange("default");
                onChineseFontChange("default");
              }}
            >
              {t("settings.workspace.restoreFonts")}
            </button>
          )}
        </div>

        {/* Latin Font Selection Group */}
        <div className="font-group-block">
          <div className="font-group-header">
            <span className="font-group-label">{t("settings.workspace.latinLabel")}</span>
            <small className="font-group-desc">{t("settings.workspace.latinDesc")}</small>
          </div>
          <div className="font-cards-grid">
            {LATIN_FONT_OPTIONS.map((opt) => {
              const isSelected = latinFont === opt.id;
              const name = t(`settings.fonts.latin.${opt.id}.name`);
              return (
                <button
                  key={opt.id}
                  type="button"
                  role="radio"
                  aria-checked={isSelected}
                  aria-label={t("settings.workspace.latinAria", { name })}
                  className={`font-option-card ${isSelected ? "selected" : ""}`}
                  onClick={() => onLatinFontChange(opt.id)}
                >
                  <div className="font-card-top">
                    <strong style={{ fontFamily: opt.css }}>{name}</strong>
                    {isSelected && <span className="font-card-badge">{t("settings.workspace.active")}</span>}
                  </div>
                  <span className="font-card-desc">{t(`settings.fonts.latin.${opt.id}.description`)}</span>
                  <div className="font-card-sample" style={{ fontFamily: opt.css }}>
                    {t(`settings.fonts.latin.${opt.id}.preview`)}
                  </div>
                </button>
              );
            })}
          </div>
        </div>

        {/* Chinese Font Selection Group */}
        <div className="font-group-block">
          <div className="font-group-header">
            <span className="font-group-label">{t("settings.workspace.chineseLabel")}</span>
            <small className="font-group-desc">{t("settings.workspace.chineseDesc")}</small>
          </div>
          <div className="font-cards-grid">
            {CHINESE_FONT_OPTIONS.map((opt) => {
              const isSelected = chineseFont === opt.id;
              const name = t(`settings.fonts.chinese.${opt.id}.name`);
              return (
                <button
                  key={opt.id}
                  type="button"
                  role="radio"
                  aria-checked={isSelected}
                  aria-label={t("settings.workspace.chineseAria", { name })}
                  className={`font-option-card ${isSelected ? "selected" : ""}`}
                  onClick={() => onChineseFontChange(opt.id)}
                >
                  <div className="font-card-top">
                    <strong style={{ fontFamily: opt.css }}>{name}</strong>
                    {isSelected && <span className="font-card-badge">{t("settings.workspace.active")}</span>}
                  </div>
                  <span className="font-card-desc">{t(`settings.fonts.chinese.${opt.id}.description`)}</span>
                  <div className="font-card-sample" style={{ fontFamily: opt.css }}>
                    {t(CHINESE_PREVIEW_KEYS[opt.id])}
                  </div>
                </button>
              );
            })}
          </div>
        </div>

        {/* Live Font & KaTeX Math Preview Box */}
        <div className="combined-font-preview-wrap">
          <div className="preview-wrap-label">{t("settings.workspace.mixedPreviewLabel")}</div>
          <div
            className="combined-preview-card"
            style={{ fontFamily: combinedFontFamily }}
          >
            <h4 className="preview-heading">
              {t("fonts.mixedHeading")}
            </h4>
            <p className="preview-body">
              {t("fonts.mixedBody")}
            </p>
            <div className="preview-math-box">
              <MarkdownBody children={SAMPLE_FORMULA} />
            </div>
          </div>
        </div>
      </section>

      {/* 2. Workspace Directory & Storage Report */}
      <section className="settings-card workspace-ledger-card">
        <div className="card-header">
          <div>
            <h3>{t("settings.workspace.storageTitle")}</h3>
            <p className="card-subtitle">
              {t("settings.workspace.storageSubtitle")}
            </p>
          </div>
          <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
            {workspace?.available ? (
              <>
                <button
                  type="button"
                  className="btn-sm btn-outline-primary"
                  onClick={() => void handleOpenWorkspace()}
                >
                  {t("settings.workspace.openFolder")}
                </button>
                <button
                  type="button"
                  className="btn-sm btn-ghost"
                  disabled={workspaceBusy}
                  onClick={() => void handleChooseWorkspace()}
                  title={workspaceBusy ? t("settings.workspace.changeBusy") : t("settings.workspace.changeTitle")}
                >
                  {t("settings.workspace.changeWorkspace")}
                </button>
              </>
            ) : workspace?.statusDetail === "reset_required" ? (
              <>
                <button
                  type="button"
                  className="btn-sm btn-primary"
                  onClick={onResetWorkspaceClick}
                >
                  {t("settings.workspace.inspectReset")}
                </button>
                <button
                  type="button"
                  className="btn-sm btn-outline-primary"
                  onClick={() => void handleChooseWorkspace()}
                >
                  {t("settings.workspace.chooseOther")}
                </button>
              </>
            ) : workspace ? (
              <>
                <button
                  type="button"
                  className="btn-sm btn-outline-primary"
                  onClick={() => void handleOpenWorkspace()}
                  disabled={!workspace.rootPath}
                >
                  {t("settings.workspace.openInExplorer")}
                </button>
                <button
                  type="button"
                  className="btn-sm btn-primary"
                  onClick={() => void handleChooseWorkspace()}
                >
                  {t("settings.workspace.reselect")}
                </button>
              </>
            ) : (
              <button
                type="button"
                className="btn-sm btn-primary"
                onClick={() => void handleChooseWorkspace()}
              >
                {t("settings.workspace.choose")}
              </button>
            )}
          </div>
        </div>

        <div className="workspace-status-line" role="status" aria-live="polite" style={{ fontSize: 12, color: "var(--muted)", marginBottom: 8 }}>
          {workspace ? (
            workspace.available ? t("settings.workspace.available") : `⚠️ ${workspace.statusDetail}`
          ) : (
            t("settings.workspace.none")
          )}
          {workspaceBusy && t("settings.workspace.processing")}
        </div>

        <div className="workspace-path-display">
          <span className="path-label">{t("settings.workspace.rootPath")}</span>
          <code className="path-text" style={{ wordBreak: "break-all" }}>{workspace?.rootPath ?? t("settings.workspace.uninitialized")}</code>
        </div>

        {onReplayHubTour ? (
          <div className="workspace-path-display">
            <span className="path-label">{t("settings.workspace.hubTour")}</span>
            <button type="button" className="btn-sm" onClick={onReplayHubTour}>{t("settings.workspace.replayTour")}</button>
          </div>
        ) : null}

        <div className="storage-stats-grid">
          <div className="storage-stat-tile">
            <span className="stat-label">{t("settings.workspace.statTotal")}</span>
            <span className="stat-value">{formatBytes(storageReport?.workspaceBytes)}</span>
          </div>
          <div className="storage-stat-tile">
            <span className="stat-label">{t("settings.workspace.statPapers")}</span>
            <span className="stat-value">{formatBytes(storageReport?.papersBytes)}</span>
          </div>
          <div className="storage-stat-tile">
            <span className="stat-label">{t("settings.workspace.statOcr")}</span>
            <span className="stat-value">{formatBytes(storageReport?.internalBytes)}</span>
          </div>
        </div>
      </section>

      {/* 3. Danger Zone */}
      <section className="settings-card danger-zone-card">
        <div className="danger-zone-header">
          <div>
            <h3 className="text-danger">{t("settings.workspace.dangerTitle")}</h3>
            <p className="card-subtitle">
              {t("settings.workspace.dangerBody")}
            </p>
          </div>
          <button
            type="button"
            className="btn-sm btn-danger"
            onClick={onResetWorkspaceClick}
          >
            {t("settings.workspace.resetData")}
          </button>
        </div>
      </section>
    </div>
  );
};
