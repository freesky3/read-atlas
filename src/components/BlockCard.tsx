import { useEffect, useMemo, useRef, useState } from "react";
import { useLocale, type TranslateFn } from "../i18n/LocaleContext";
import type { PDFDocumentProxy } from "pdfjs-dist";
import MarkdownBody from "../MarkdownBody";
import { ensureDisplayMath, handleMarkdownCopyEvent } from "../markdown";
import { createLensCrops } from "../PdfReader";
import { formatQuoteCaption } from "../blockQuotes";
import DeleteVersionConfirmModal from "./DeleteVersionConfirmModal";
import RegenerateConfirmModal from "./RegenerateConfirmModal";
import {
  type ArtifactGroup,
  artifactLabel,
  GeneratedDetail,
  items,
  record,
  text,
} from "../ArtifactPanel";
import type { BlockAction } from "../PdfReader";
import type {
  ArtifactProjection,
  LensQaProjection,
  OcrBlockProjection,
} from "../types";

export type BlockArtifactBundle = {
  blockId: string;
  pageNumber: number;
  blockIndex: number;
  blockType: string;
  textContent: string;
  bbox: [number, number, number, number];
  cropDataUrl?: string | null;
  groupsByKind: Map<string, ArtifactGroup>;
  allArtifacts: ArtifactProjection[];
  isGenerating?: boolean;
  generatingAction?: BlockAction | null;
};

export type BlockActionTab = {
  action: BlockAction;
  kind: string;
  label: string;
  isGenerated: boolean;
  group?: ArtifactGroup;
};

export type BlockCardProps = {
  bundle: BlockArtifactBundle;
  isExpanded: boolean;
  onToggleExpand: (blockId: string) => void;
  activeArtifactId: string | null;
  onSelectArtifact: (artifact: ArtifactProjection) => void;
  onJump: (page: number, blockId?: string | null) => void;
  onGenerateBlockAction?: (block: OcrBlockProjection, action: BlockAction) => void;
  onRegenerateLens?: (artifact: ArtifactProjection) => void;
  onDeleteArtifactVersion?: (artifactId: string) => Promise<void> | void;
  onTransferLens?: (artifact: ArtifactProjection) => void;
  onAskLens?: (question: string, parentId: string | null) => Promise<void>;
  onOpenQa?: (blockId: string, artifact: ArtifactProjection, cropSrc?: string | null) => void;
  lensQa: LensQaProjection[];
  lensQaBusy: boolean;
  pdfDocument?: PDFDocumentProxy | null;
  rotation?: number;
  ocrBlock?: OcrBlockProjection;
  modelLabel?: string;
};

function formatBlockTypeLabel(blockType: string, t: TranslateFn): string {
  const norm = blockType.toLowerCase();
  if (norm.includes("formula") || norm.includes("equation")) return t("blockCard.type.formula");
  if (norm.includes("figure") || norm.includes("image") || norm.includes("picture")) return t("blockCard.type.figure");
  if (norm.includes("table")) return t("blockCard.type.table");
  if (norm.includes("heading") || norm.includes("header") || norm.includes("title")) return t("blockCard.type.heading");
  if (norm.includes("list")) return t("blockCard.type.list");
  return t("blockCard.type.paragraph");
}

function getAvailableTabsForBundle(bundle: BlockArtifactBundle, t: TranslateFn): BlockActionTab[] {
  const norm = bundle.blockType.toLowerCase();
  const tabs: BlockActionTab[] = [];

  if (norm.includes("formula") || norm.includes("equation")) {
    tabs.push({
      action: "lens",
      kind: "lens_formula",
      label: t("blockCard.tab.formulaLens"),
      isGenerated: bundle.groupsByKind.has("lens_formula"),
      group: bundle.groupsByKind.get("lens_formula"),
    });
  } else if (norm.includes("figure") || norm.includes("image") || norm.includes("picture")) {
    tabs.push({
      action: "lens",
      kind: "lens_figure",
      label: t("blockCard.tab.figureLens"),
      isGenerated: bundle.groupsByKind.has("lens_figure"),
      group: bundle.groupsByKind.get("lens_figure"),
    });
  } else if (norm.includes("table")) {
    tabs.push({
      action: "lens",
      kind: "lens_table",
      label: t("blockCard.tab.tableLens"),
      isGenerated: bundle.groupsByKind.has("lens_table"),
      group: bundle.groupsByKind.get("lens_table"),
    });
  } else {
    tabs.push({
      action: "translate",
      kind: "translation",
      label: t("blockCard.tab.translation"),
      isGenerated: bundle.groupsByKind.has("translation"),
      group: bundle.groupsByKind.get("translation"),
    });
    tabs.push({
      action: "explain",
      kind: "explanation",
      label: t("blockCard.tab.explanation"),
      isGenerated: bundle.groupsByKind.has("explanation"),
      group: bundle.groupsByKind.get("explanation"),
    });
  }

  for (const [kind, group] of bundle.groupsByKind.entries()) {
    if (!tabs.some((tab) => tab.kind === kind)) {
      const action: BlockAction = kind.startsWith("lens_")
        ? "lens"
        : kind === "translation"
        ? "translate"
        : kind === "explanation"
        ? "explain"
        : "quote";
      tabs.push({
        action,
        kind,
        label: artifactLabel(group.latestArtifact),
        isGenerated: true,
        group,
      });
    }
  }

  return tabs;
}

export default function BlockCard({
  bundle,
  isExpanded,
  onToggleExpand,
  activeArtifactId,
  onSelectArtifact,
  onJump,
  onGenerateBlockAction,
  onRegenerateLens,
  onDeleteArtifactVersion,
  onTransferLens,
  onAskLens,
  onOpenQa,
  lensQa,
  lensQaBusy,
  pdfDocument,
  rotation = 0,
  ocrBlock,
  modelLabel,
}: BlockCardProps) {
  const { t, dateLocale } = useLocale();
  const normType = bundle.blockType.toLowerCase();
  const isFig = normType.includes("figure") || normType.includes("image") || normType.includes("picture");
  const isTab = normType.includes("table");
  const isFormula = normType.includes("formula") || normType.includes("equation");

  const [cropSrc, setCropSrc] = useState<string | null>(bundle.cropDataUrl ?? null);
  const [enlargedCrop, setEnlargedCrop] = useState<string | null>(null);

  // Lazy load thumbnail crop for Figure / Table
  useEffect(() => {
    if (bundle.cropDataUrl) {
      setCropSrc(bundle.cropDataUrl);
      return;
    }
    if ((isFig || isTab) && pdfDocument) {
      let cancelled = false;
      void createLensCrops(pdfDocument, bundle.pageNumber, bundle.bbox, rotation)
        .then((material) => {
          if (!cancelled && material.displayCropDataUrl) {
            setCropSrc(material.displayCropDataUrl);
            bundle.cropDataUrl = material.displayCropDataUrl;
          }
        })
        .catch(() => undefined);
      return () => {
        cancelled = true;
      };
    }
  }, [bundle, isFig, isTab, pdfDocument, rotation]);

  const availableTabs = useMemo(() => getAvailableTabsForBundle(bundle, t), [bundle, t]);

  // Determine active tab: check if generating, activeArtifactId matches, otherwise pick first generated, else first
  const [selectedKind, setSelectedKind] = useState<string>(() => {
    if (bundle.isGenerating && bundle.generatingAction) {
      const genTab = availableTabs.find((t) => t.action === bundle.generatingAction);
      if (genTab) return genTab.kind;
    }
    if (activeArtifactId) {
      const match = availableTabs.find((t) =>
        t.group?.versions.some((v) => v.id === activeArtifactId),
      );
      if (match) return match.kind;
    }
    const firstGen = availableTabs.find((t) => t.isGenerated);
    return firstGen ? firstGen.kind : availableTabs[0]?.kind || "";
  });

  const prevActiveArtifactIdRef = useRef(activeArtifactId);
  const prevGeneratingActionRef = useRef(bundle.generatingAction);

  // Sync selected tab only when activeArtifactId or generating action changes from outside
  useEffect(() => {
    if (prevGeneratingActionRef.current !== bundle.generatingAction && bundle.isGenerating && bundle.generatingAction) {
      prevGeneratingActionRef.current = bundle.generatingAction;
      const genTab = availableTabs.find((t) => t.action === bundle.generatingAction);
      if (genTab) {
        setSelectedKind(genTab.kind);
        return;
      }
    }
    if (prevActiveArtifactIdRef.current !== activeArtifactId) {
      prevActiveArtifactIdRef.current = activeArtifactId;
      if (activeArtifactId) {
        const match = availableTabs.find((t) =>
          t.group?.versions.some((v) => v.id === activeArtifactId),
        );
        if (match) {
          setSelectedKind(match.kind);
        }
      }
    }
  }, [activeArtifactId, availableTabs, bundle.isGenerating, bundle.generatingAction]);

  const currentTab = availableTabs.find((t) => t.kind === selectedKind) ?? availableTabs[0];

  // Active artifact for the selected tab
  const activeGroup = currentTab?.group;
  const activeArtifact = useMemo(() => {
    if (!activeGroup) return null;
    if (activeArtifactId) {
      const match = activeGroup.versions.find((v) => v.id === activeArtifactId);
      if (match) return match;
    }
    return activeGroup.latestArtifact;
  }, [activeGroup, activeArtifactId]);

  const versionIndex = activeGroup && activeArtifact
    ? activeGroup.versions.findIndex((v) => v.id === activeArtifact.id)
    : -1;
  const totalVersions = activeGroup ? activeGroup.versions.length : 0;

  // Modals
  const [deletingArtifact, setDeletingArtifact] = useState<ArtifactProjection | null>(null);
  const [confirmingArtifact, setConfirmingArtifact] = useState<ArtifactProjection | null>(null);

  const fallbackBlock = useMemo<OcrBlockProjection>(() => {
    if (ocrBlock) return ocrBlock;
    return {
      id: bundle.blockId,
      pageNumber: bundle.pageNumber,
      blockIndex: bundle.blockIndex,
      blockType: bundle.blockType,
      textContent: bundle.textContent,
      contentDigest: "",
      bbox: bundle.bbox,
    };
  }, [bundle, ocrBlock]);

  const handleStartGeneration = (action: BlockAction) => {
    if (onGenerateBlockAction) {
      onGenerateBlockAction(fallbackBlock, action);
    }
  };

  const isTabGenerating = bundle.isGenerating && bundle.generatingAction === currentTab?.action;

  const generatingLabel = bundle.generatingAction === "translate"
    ? t("blockCard.tab.translation")
    : bundle.generatingAction === "explain"
    ? t("blockCard.tab.explanation")
    : "Lens";

  const generatingDetailTitle = bundle.generatingAction === "translate"
    ? t("blockCard.generating.translation")
    : bundle.generatingAction === "explain"
    ? t("blockCard.generating.explanation")
    : t("blockCard.generating.lens");

  return (
    <div
      id={`block-card-${bundle.blockId}`}
      data-testid={`block-card-${bundle.blockId}`}
      className={`block-card ${isExpanded ? "is-expanded" : "is-collapsed"} ${
        bundle.allArtifacts.some((a) => a.id === activeArtifactId) ? "is-active" : ""
      }`}
    >
      {/* Header / Preview Bar */}
      <div
        className="block-card-header"
        onClick={() => onToggleExpand(bundle.blockId)}
        role="button"
        tabIndex={0}
        aria-expanded={isExpanded}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onToggleExpand(bundle.blockId);
          }
        }}
      >
        <div className="block-card-meta-line">
          <button
            type="button"
            className="block-card-jump-badge"
            title={t("blockCard.jumpToPdf")}
            onClick={(e) => {
              e.stopPropagation();
              onJump(bundle.pageNumber, bundle.blockId);
            }}
          >
            <span>p.{bundle.pageNumber} · #{bundle.blockIndex + 1}</span>
            <small>{formatBlockTypeLabel(bundle.blockType, t)}</small>
            <i>↗</i>
          </button>

          <div className="block-card-status-chips">
            {availableTabs
              .filter((tab) => tab.isGenerated)
              .map((tab) => (
                <span key={tab.kind} className="block-status-chip is-done" title={t("blockCard.generatedTitle", { label: tab.label })}>
                  ✓ {tab.label}
                </span>
              ))}
            {bundle.isGenerating ? (
              <span className="block-status-chip is-busy">
                <i className="status-pip is-busy" /> {t("blockCard.generatingStatus", { label: generatingLabel })}
              </span>
            ) : null}
          </div>

          <button
            type="button"
            className="block-card-toggle-btn"
            aria-label={isExpanded ? t("blockCard.collapse") : t("blockCard.expand")}
          >
            {isExpanded ? "▴" : "▾"}
          </button>
        </div>

        {/* Rich Preview Area */}
        <div className="block-card-preview">
          {isFormula ? (
            <div className="block-preview-formula" title={t("blockCard.formulaPreview")}>
              <MarkdownBody>{ensureDisplayMath(bundle.textContent)}</MarkdownBody>
            </div>
          ) : isFig ? (
            <div className="block-preview-figure-row">
              {cropSrc ? (
                <img
                  src={cropSrc}
                  alt={t("completion.figure_preview")}
                  className="block-preview-thumb"
                  onClick={(e) => {
                    e.stopPropagation();
                    setEnlargedCrop(cropSrc);
                  }}
                  title={t("blockCard.enlargeImage")}
                />
              ) : (
                <div className="block-preview-placeholder">🖼️ {t("blockCard.loadingFigure")}</div>
              )}
              <div className="block-preview-caption">
                {formatQuoteCaption(bundle.textContent, `Figure on p.${bundle.pageNumber}`)}
              </div>
            </div>
          ) : isTab ? (
            <div className="block-preview-table-row">
              {cropSrc ? (
                <img
                  src={cropSrc}
                  alt={t("completion.table_preview")}
                  className="block-preview-thumb"
                  onClick={(e) => {
                    e.stopPropagation();
                    setEnlargedCrop(cropSrc);
                  }}
                  title={t("blockCard.enlargeTable")}
                />
              ) : (
                <div className="block-preview-placeholder">📊 {t("blockCard.loadingTable")}</div>
              )}
              <div className="block-preview-caption">
                {formatQuoteCaption(bundle.textContent, `Table on p.${bundle.pageNumber}`)}
              </div>
            </div>
          ) : (
            <div className="block-preview-text">
              <MarkdownBody>
                {bundle.textContent.length > 150
                  ? bundle.textContent.slice(0, 150) + "…"
                  : bundle.textContent || t("blockCard.emptyText")}
              </MarkdownBody>
            </div>
          )}
        </div>
      </div>

      {/* Expanded Content Area */}
      {isExpanded ? (
        <div className="block-card-body">
          {/* Sub-item Tabs */}
          <div className="block-subtabs-bar">
            <div className="block-subtabs-list" role="tablist">
              {availableTabs.map((tab) => {
                const isActive = tab.kind === selectedKind;
                return (
                  <button
                    key={tab.kind}
                    type="button"
                    role="tab"
                    aria-selected={isActive}
                    className={`block-subtab-item ${isActive ? "active" : ""} ${
                      tab.isGenerated ? "is-generated" : "is-ungenerated"
                    }`}
                    onClick={() => {
                      setSelectedKind(tab.kind);
                      if (tab.group?.latestArtifact) {
                        onSelectArtifact(tab.group.latestArtifact);
                      }
                    }}
                  >
                    <span>{tab.isGenerated ? `✓ ${tab.label}` : tab.label}</span>
                    {tab.group && tab.group.versions.length > 1 ? (
                      <span className="version-pill-tiny">{t("blockCard.versionCount", { count: tab.group.versions.length })}</span>
                    ) : null}
                  </button>
                );
              })}
            </div>

            {/* Actions for current tab */}
            {currentTab?.isGenerated && activeArtifact ? (
              <div className="block-subtabs-actions">
                {totalVersions > 1 && activeGroup ? (
                  <div className="version-switcher-capsule" role="navigation" aria-label={t("blockCard.versionSwitch")}>
                    <button
                      type="button"
                      className="version-nav-btn"
                      disabled={versionIndex <= 0}
                      onClick={() => onSelectArtifact(activeGroup.versions[versionIndex - 1])}
                      title={t("blockCard.prevVersion")}
                      aria-label={t("blockCard.prevVersion")}
                    >
                      ‹
                    </button>
                    <span className="version-indicator">
                      {t("blockCard.versionIndex", { current: versionIndex + 1, total: totalVersions })}
                    </span>
                    <button
                      type="button"
                      className="version-nav-btn"
                      disabled={versionIndex >= totalVersions - 1}
                      onClick={() => onSelectArtifact(activeGroup.versions[versionIndex + 1])}
                      title={t("blockCard.nextVersion")}
                      aria-label={t("blockCard.nextVersion")}
                    >
                      ›
                    </button>
                    {onDeleteArtifactVersion ? (
                      <button
                        type="button"
                        className="version-delete-btn"
                        onClick={() => setDeletingArtifact(activeArtifact)}
                        title={t("blockCard.deleteVersion")}
                        aria-label={t("blockCard.deleteVersion")}
                      >
                        🗑️
                      </button>
                    ) : null}
                  </div>
                ) : null}

                {currentTab.kind.startsWith("lens_") && onTransferLens ? (
                  <button
                    type="button"
                    className="btn-liquid-pill primary"
                    onClick={() => onTransferLens(activeArtifact)}
                    style={{ height: 24, padding: "0 8px", fontSize: 11 }}
                  >
                    {t("blockCard.transferToDiscussion")} <span>→</span>
                  </button>
                ) : null}

                {(currentTab.kind.startsWith("lens_") ? Boolean(onRegenerateLens) : Boolean(onGenerateBlockAction)) ? (
                  <button
                    type="button"
                    className="btn-liquid-pill"
                    style={{ height: 24, padding: "0 8px", fontSize: 11 }}
                    aria-label={t("blockCard.regenerate")}
                    title={t("blockCard.regenerateItem")}
                    onClick={() => {
                      if (onRegenerateLens && currentTab.kind.startsWith("lens_")) {
                        setConfirmingArtifact(activeArtifact);
                      } else if (onGenerateBlockAction) {
                        onGenerateBlockAction(fallbackBlock, currentTab.action);
                      }
                    }}
                  >
                    🔄 {t("blockCard.regenerate")}
                  </button>
                ) : null}
              </div>
            ) : null}
          </div>

          {/* Active Tab Content */}
          <div className="block-tab-content">
            {isTabGenerating ? (
              <div className="block-tab-generating">
                <i className="status-pip is-busy" />
                <p>{t("blockCard.generatingNow", { title: generatingDetailTitle })}</p>
              </div>
            ) : !currentTab?.isGenerated || !activeArtifact ? (
              /* Ungenerated Pre-flight Card */
              <div className="block-preflight-card">
                <div className="block-preflight-header">
                  <span className="block-preflight-icon">💡</span>
                  <div className="block-preflight-meta">
                    <h4>{t("blockCard.notGenerated", { label: currentTab.label })}</h4>
                    <p>
                      {currentTab.action === "translate"
                        ? t("blockCard.preflight.translate")
                        : currentTab.action === "explain"
                        ? t("blockCard.preflight.explain")
                        : t("blockCard.preflight.lens")}
                    </p>
                  </div>
                </div>
                <div className="block-preflight-footer">
                  <span className="block-preflight-tip">
                    ✨ {t("blockCard.preflight.tip")}
                  </span>
                  <button
                    type="button"
                    className="btn-liquid-pill primary block-preflight-confirm-btn"
                    onClick={() => handleStartGeneration(currentTab.action)}
                  >
                    <span>{t("blockCard.startGenerate", { label: currentTab.label })}</span>
                  </button>
                </div>
              </div>
            ) : (
              /* Generated Detail */
              <div
                className="block-tab-generated"
                onCopy={(event) =>
                  handleMarkdownCopyEvent(event, event.currentTarget)
                }
              >
                <GeneratedDetail
                  artifact={activeArtifact}
                  displayCropSrc={cropSrc || ""}
                />

                {/* Evidence Pills */}
                {activeArtifact.evidence.length > 0 ? (
                  <section className="artifact-evidence" style={{ marginTop: 12 }}>
                    <span>{t("completion.evidence")}</span>
                    <div className="artifact-evidence-pills">
                      {activeArtifact.evidence.map((ev, idx) => (
                        <button
                          key={`${ev.pageNumber}-${ev.blockId ?? idx}`}
                          type="button"
                          className="btn-evidence-pill"
                          title={t("blockCard.jumpToPage", { page: ev.pageNumber })}
                          onClick={() => onJump(ev.pageNumber, ev.blockId)}
                        >
                          <span>📄 p.{ev.pageNumber}</span>
                          <i>↗</i>
                        </button>
                      ))}
                    </div>
                  </section>
                ) : null}

                {/* Dedicated QA Entry Button */}
                {currentTab.kind.startsWith("lens_") && onOpenQa ? (
                  <div className="block-card-qa-entry-wrap">
                    <button
                      type="button"
                      className="block-card-qa-entry-btn"
                      onClick={(e) => {
                        e.stopPropagation();
                        if (activeArtifact) {
                          onSelectArtifact(activeArtifact);
                          onOpenQa(bundle.blockId, activeArtifact, cropSrc);
                        }
                      }}
                    >
                      <span className="qa-entry-icon">💬</span>
                      <div className="qa-entry-text">
                        <span className="qa-entry-title">{t("blockCard.qaTitle")}</span>
                        <span className="qa-entry-desc">
                          {activeArtifactId === activeArtifact?.id && lensQa.length > 0
                            ? t("blockCard.qaWithHistory", { count: lensQa.length })
                            : t("blockCard.qaEmpty")}
                        </span>
                      </div>
                      {activeArtifactId === activeArtifact?.id && lensQa.length > 0 ? (
                        <span className="qa-entry-count-badge">{lensQa.length}</span>
                      ) : null}
                      <span className="qa-entry-arrow">→</span>
                    </button>
                  </div>
                ) : null}

                {/* Detail Meta Footer */}
                <div className="artifact-detail-meta-footer">
                  <span>
                    {modelLabel ? t("blockCard.generatedBy", { model: modelLabel }) : ""}
                    {new Date(activeArtifact.createdAt).toLocaleDateString(dateLocale, {
                      month: "short",
                      day: "numeric",
                    })}{" "}
                    {new Date(activeArtifact.createdAt).toLocaleTimeString(dateLocale, {
                      hour: "2-digit",
                      minute: "2-digit",
                    })}
                  </span>
                </div>
              </div>
            )}
          </div>
        </div>
      ) : null}

      {/* Lightbox Modal for Thumbnail Image */}
      {enlargedCrop && (
        <div
          className="lightbox-modal"
          role="dialog"
          aria-modal="true"
          onClick={() => setEnlargedCrop(null)}
        >
          <div className="lightbox-content" onClick={(e) => e.stopPropagation()}>
            <button
              type="button"
              className="lightbox-close"
              onClick={() => setEnlargedCrop(null)}
              aria-label={t("blockCard.closePreview")}
            >
              ×
            </button>
            <div className="lightbox-body">
              <img
                src={enlargedCrop}
                alt={t("completion.enlarged_crop_preview")}
                style={{ maxWidth: "100%", maxHeight: "75vh", borderRadius: 8, display: "block", margin: "0 auto" }}
              />
              <p className="lightbox-caption">
                {formatBlockTypeLabel(bundle.blockType, t)} · {t("blockCard.pageCaption", { page: bundle.pageNumber, index: bundle.blockIndex + 1 })}
              </p>
            </div>
          </div>
        </div>
      )}

      {/* Delete Version Modal */}
      {deletingArtifact && onDeleteArtifactVersion && (
        <DeleteVersionConfirmModal
          open={Boolean(deletingArtifact)}
          versionNumber={versionIndex + 1}
          totalVersions={totalVersions}
          onConfirm={() => {
            const id = deletingArtifact.id;
            setDeletingArtifact(null);
            void onDeleteArtifactVersion(id);
          }}
          onCancel={() => setDeletingArtifact(null)}
        />
      )}

      {/* Regenerate Modal */}
      {confirmingArtifact && onRegenerateLens && (
        <RegenerateConfirmModal
          open={Boolean(confirmingArtifact)}
          artifactTitle={artifactLabel(confirmingArtifact)}
          kindLabel={artifactLabel(confirmingArtifact)}
          onConfirm={() => {
            const art = confirmingArtifact;
            setConfirmingArtifact(null);
            onRegenerateLens(art);
          }}
          onCancel={() => setConfirmingArtifact(null)}
        />
      )}
    </div>
  );
}
