import { useEffect, useMemo, useRef, useState } from "react";
import { useLocale } from "../i18n/LocaleContext";
import type { PDFDocumentProxy } from "pdfjs-dist";
import BlockCard, {
  type BlockArtifactBundle,
} from "./BlockCard";
import BlockCardQaDetail from "./BlockCardQaDetail";
import type { BlockAction } from "../PdfReader";
import type {
  ArtifactProjection,
  LensQaProjection,
  OcrBlockProjection,
} from "../types";

export type BlockCardStreamProps = {
  bundles: BlockArtifactBundle[];
  activeArtifactId: string | null;
  activeBlockId: string | null;
  onSelectArtifact: (artifact: ArtifactProjection) => void;
  onJump: (page: number, blockId?: string | null) => void;
  onGenerateBlockAction?: (block: OcrBlockProjection, action: BlockAction) => void;
  onRegenerateLens?: (artifact: ArtifactProjection) => void;
  onDeleteArtifactVersion?: (artifactId: string) => Promise<void> | void;
  onTransferLens?: (artifact: ArtifactProjection) => void;
  onAskLens?: (question: string, parentId: string | null) => Promise<void>;
  lensQa: LensQaProjection[];
  lensQaBusy: boolean;
  pdfDocument?: PDFDocumentProxy | null;
  rotation?: number;
  ocrBlocks?: OcrBlockProjection[];
  displayCropSrc?: string;
  emptyMessage?: string;
  modelLabel?: string;
};

type FilterCategory = "all" | "text" | "figure" | "formula" | "table";

function matchCategory(bundle: BlockArtifactBundle, category: FilterCategory): boolean {
  if (category === "all") return true;
  const norm = bundle.blockType.toLowerCase();
  if (category === "figure") {
    return norm.includes("figure") || norm.includes("image") || norm.includes("picture");
  }
  if (category === "formula") {
    return norm.includes("formula") || norm.includes("equation");
  }
  if (category === "table") {
    return norm.includes("table");
  }
  if (category === "text") {
    return (
      !norm.includes("figure") &&
      !norm.includes("image") &&
      !norm.includes("picture") &&
      !norm.includes("formula") &&
      !norm.includes("equation") &&
      !norm.includes("table")
    );
  }
  return true;
}

export default function BlockCardStream({
  bundles,
  activeArtifactId,
  activeBlockId,
  onSelectArtifact,
  onJump,
  onGenerateBlockAction,
  onRegenerateLens,
  onDeleteArtifactVersion,
  onTransferLens,
  onAskLens,
  lensQa,
  lensQaBusy,
  pdfDocument,
  rotation = 0,
  ocrBlocks = [],
  displayCropSrc,
  emptyMessage,
  modelLabel,
}: BlockCardStreamProps) {
  const { t } = useLocale();
  const [filter, setFilter] = useState<FilterCategory>("all");
  const [expandedIds, setExpandedIds] = useState<Set<string>>(() => {
    const initial = new Set<string>();
    if (activeBlockId) {
      initial.add(activeBlockId);
    } else if (bundles.length > 0) {
      // By default expand first bundle or active one
      initial.add(bundles[0].blockId);
    }
    return initial;
  });

  // Automatically expand card when activeBlockId or activeArtifactId changes
  useEffect(() => {
    let targetBlockId = activeBlockId;
    if (!targetBlockId && activeArtifactId) {
      const match = bundles.find((b) =>
        b.allArtifacts.some((a) => a.id === activeArtifactId),
      );
      if (match) targetBlockId = match.blockId;
    }
    if (targetBlockId) {
      setExpandedIds((prev) => {
        if (prev.has(targetBlockId)) return prev;
        const next = new Set(prev);
        next.add(targetBlockId);
        return next;
      });

      // Smooth scroll to card
      const elem = document.getElementById(`block-card-${targetBlockId}`);
      if (elem) {
        elem.scrollIntoView?.({ behavior: "smooth", block: "nearest" });
        elem.classList.add("target-highlight");
        const timer = setTimeout(() => {
          elem.classList.remove("target-highlight");
        }, 1200);
        return () => clearTimeout(timer);
      }
    }
  }, [activeBlockId, activeArtifactId, bundles]);

  const toggleExpand = (blockId: string) => {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(blockId)) {
        next.delete(blockId);
      } else {
        next.add(blockId);
      }
      return next;
    });
  };

  const [focusedQaBlockId, setFocusedQaBlockId] = useState<string | null>(null);
  const [focusedQaArtifact, setFocusedQaArtifact] = useState<ArtifactProjection | null>(null);
  const [focusedCropSrc, setFocusedCropSrc] = useState<string | null>(null);

  const handleOpenQa = (
    blockId: string,
    artifact: ArtifactProjection,
    cropSrc?: string | null,
  ) => {
    onSelectArtifact(artifact);
    setFocusedQaBlockId(blockId);
    setFocusedQaArtifact(artifact);
    if (cropSrc) setFocusedCropSrc(cropSrc);
  };

  const qaScrollTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (qaScrollTimerRef.current) clearTimeout(qaScrollTimerRef.current);
    };
  }, []);

  const handleBackFromQa = (blockId: string) => {
    setFocusedQaBlockId(null);
    setFocusedQaArtifact(null);
    setFocusedCropSrc(null);
    qaScrollTimerRef.current = setTimeout(() => {
      if (typeof document === "undefined") return;
      const elem = document.getElementById(`block-card-${blockId}`);
      if (elem) {
        elem.scrollIntoView?.({ behavior: "smooth", block: "nearest" });
        elem.classList.add("target-highlight");
        setTimeout(() => {
          if (typeof document !== "undefined") {
            elem.classList.remove("target-highlight");
          }
        }, 1200);
      }
    }, 60);
  };

  const ocrBlockMap = useMemo(() => {
    const map = new Map<string, OcrBlockProjection>();
    for (const block of ocrBlocks) {
      map.set(block.id, block);
    }
    return map;
  }, [ocrBlocks]);

  // Counts for filters
  const counts = useMemo(() => {
    let textCount = 0;
    let figureCount = 0;
    let formulaCount = 0;
    let tableCount = 0;
    for (const b of bundles) {
      const norm = b.blockType.toLowerCase();
      if (norm.includes("figure") || norm.includes("image") || norm.includes("picture")) {
        figureCount++;
      } else if (norm.includes("formula") || norm.includes("equation")) {
        formulaCount++;
      } else if (norm.includes("table")) {
        tableCount++;
      } else {
        textCount++;
      }
    }
    return {
      all: bundles.length,
      text: textCount,
      figure: figureCount,
      formula: formulaCount,
      table: tableCount,
    };
  }, [bundles]);

  const filteredBundles = useMemo(
    () => bundles.filter((b) => matchCategory(b, filter)),
    [bundles, filter],
  );

  const allExpanded =
    filteredBundles.length > 0 &&
    filteredBundles.every((b) => expandedIds.has(b.blockId));

  const toggleExpandAll = () => {
    if (allExpanded) {
      setExpandedIds((prev) => {
        const next = new Set(prev);
        for (const b of filteredBundles) {
          next.delete(b.blockId);
        }
        return next;
      });
    } else {
      setExpandedIds((prev) => {
        const next = new Set(prev);
        for (const b of filteredBundles) {
          next.add(b.blockId);
        }
        return next;
      });
    }
  };

  const focusedBundle = focusedQaBlockId
    ? bundles.find((b) => b.blockId === focusedQaBlockId)
    : null;
  const focusedArtifact = focusedQaArtifact || (
    focusedBundle
      ? (focusedBundle.allArtifacts.find((a) => a.id === activeArtifactId) || focusedBundle.allArtifacts[0])
      : null
  );

  if (focusedQaBlockId && focusedBundle && focusedArtifact && onAskLens) {
    const resolvedCrop =
      focusedCropSrc ||
      (focusedArtifact.id === activeArtifactId ? displayCropSrc : undefined) ||
      focusedBundle.cropDataUrl;

    return (
      <BlockCardQaDetail
        bundle={focusedBundle}
        artifact={focusedArtifact}
        lensQa={lensQa}
        lensQaBusy={lensQaBusy}
        onBack={() => handleBackFromQa(focusedQaBlockId)}
        onAskLens={onAskLens}
        onTransferLens={onTransferLens}
        onJump={onJump}
        pdfDocument={pdfDocument}
        rotation={rotation}
        initialCropSrc={resolvedCrop}
        displayCropSrc={displayCropSrc}
        modelLabel={modelLabel}
      />
    );
  }

  return (
    <div className="block-card-stream-container">
      {/* Top Filter Bar */}
      {bundles.length > 0 ? (
        <div className="block-stream-filter-bar" role="toolbar" aria-label={t("blockCard.filterAria")}>
          <div className="block-filter-capsules">
            <button
              type="button"
              className={`block-filter-capsule ${filter === "all" ? "active" : ""}`}
              onClick={() => setFilter("all")}
            >
              <span>{t("blockCard.filter.all")}</span>
              <small className="filter-badge">{counts.all}</small>
            </button>
            <button
              type="button"
              className={`block-filter-capsule ${filter === "text" ? "active" : ""}`}
              onClick={() => setFilter("text")}
            >
              <span>{t("blockCard.filter.text")}</span>
              <small className="filter-badge">{counts.text}</small>
            </button>
            <button
              type="button"
              className={`block-filter-capsule ${filter === "figure" ? "active" : ""}`}
              onClick={() => setFilter("figure")}
            >
              <span>{t("blockCard.filter.figure")}</span>
              <small className="filter-badge">{counts.figure}</small>
            </button>
            <button
              type="button"
              className={`block-filter-capsule ${filter === "formula" ? "active" : ""}`}
              onClick={() => setFilter("formula")}
            >
              <span>{t("blockCard.filter.formula")}</span>
              <small className="filter-badge">{counts.formula}</small>
            </button>
            <button
              type="button"
              className={`block-filter-capsule ${filter === "table" ? "active" : ""}`}
              onClick={() => setFilter("table")}
            >
              <span>{t("blockCard.filter.table")}</span>
              <small className="filter-badge">{counts.table}</small>
            </button>
          </div>
          {filteredBundles.length > 0 ? (
            <div className="block-filter-actions">
              <button
                type="button"
                className="block-filter-toggle-expand-btn"
                onClick={toggleExpandAll}
                title={allExpanded ? t("blockCard.collapseAll") : t("blockCard.expandAll")}
                aria-label={allExpanded ? t("blockCard.collapseAll") : t("blockCard.expandAll")}
              >
                {allExpanded ? (
                  <svg
                    width="13"
                    height="13"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    aria-hidden="true"
                  >
                    <path d="m7 20 5-5 5 5" />
                    <path d="m7 4 5 5 5-5" />
                  </svg>
                ) : (
                  <svg
                    width="13"
                    height="13"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    aria-hidden="true"
                  >
                    <path d="m7 15 5 5 5-5" />
                    <path d="m7 9 5-5 5 5" />
                  </svg>
                )}
              </button>
            </div>
          ) : null}
        </div>
      ) : null}

      {/* Main Stream */}
      <div className="block-card-stream-list">
        {bundles.length === 0 ? (
          <div className="block-stream-empty-state">
            <div className="block-stream-empty-icon">🔍</div>
            <h3>{t("blockCard.empty.title")}</h3>
            <p>
              {emptyMessage ||
                t("blockCard.empty.body")}
            </p>
          </div>
        ) : filteredBundles.length === 0 ? (
          <div className="block-stream-empty-state">
            <div className="block-stream-empty-icon">🔍</div>
            <h3>{t("blockCard.empty.noMatchTitle")}</h3>
            <p>{t("blockCard.empty.noMatchBody")}</p>
          </div>
        ) : (
          filteredBundles.map((bundle) => {
            const isExpanded = expandedIds.has(bundle.blockId);
            const ocr = ocrBlockMap.get(bundle.blockId);
            return (
              <BlockCard
                key={bundle.blockId}
                bundle={bundle}
                isExpanded={isExpanded}
                onToggleExpand={toggleExpand}
                activeArtifactId={activeArtifactId}
                onSelectArtifact={onSelectArtifact}
                onJump={onJump}
                onGenerateBlockAction={onGenerateBlockAction}
                onRegenerateLens={onRegenerateLens}
                onDeleteArtifactVersion={onDeleteArtifactVersion}
                onTransferLens={onTransferLens}
                onAskLens={onAskLens}
                onOpenQa={handleOpenQa}
                lensQa={lensQa}
                lensQaBusy={lensQaBusy}
                pdfDocument={pdfDocument}
                rotation={rotation}
                ocrBlock={ocr}
                modelLabel={modelLabel}
              />
            );
          })
        )}
      </div>
    </div>
  );
}
