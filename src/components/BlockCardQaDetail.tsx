import { useEffect, useMemo, useRef, useState } from "react";
import { useLocale } from "../i18n/LocaleContext";
import type { PDFDocumentProxy } from "pdfjs-dist";
import { convertFileSrc } from "@tauri-apps/api/core";
import { desktopClient } from "../desktopClient";
import MarkdownBody from "../MarkdownBody";
import { handleMarkdownCopyEvent } from "../markdown";
import { createLensCrops } from "../PdfReader";
import {
  artifactLabel,
  GeneratedDetail,
  items,
  record,
  text,
} from "../ArtifactPanel";
import type {
  ArtifactProjection,
  LensQaProjection,
} from "../types";
import type { BlockArtifactBundle } from "./BlockCard";

export type BlockCardQaDetailProps = {
  bundle: BlockArtifactBundle;
  artifact: ArtifactProjection;
  lensQa: LensQaProjection[];
  lensQaBusy: boolean;
  onBack: () => void;
  onAskLens: (question: string, parentId: string | null) => Promise<void>;
  onTransferLens?: (artifact: ArtifactProjection) => void;
  onJump: (page: number, blockId?: string | null) => void;
  pdfDocument?: PDFDocumentProxy | null;
  rotation?: number;
  initialCropSrc?: string | null;
  displayCropSrc?: string;
  modelLabel?: string;
};

export default function BlockCardQaDetail({
  bundle,
  artifact,
  lensQa,
  lensQaBusy,
  onBack,
  onAskLens,
  onTransferLens,
  onJump,
  pdfDocument,
  rotation = 0,
  initialCropSrc,
  displayCropSrc,
  modelLabel,
}: BlockCardQaDetailProps) {
  const { t, dateLocale } = useLocale();
  const [question, setQuestion] = useState("");
  const inputRef = useRef<HTMLTextAreaElement>(null);

  const [cropSrc, setCropSrc] = useState<string | null>(() => {
    return initialCropSrc || bundle.cropDataUrl || displayCropSrc || null;
  });
  const [cropLoading, setCropLoading] = useState<boolean>(false);
  const [enlargedCrop, setEnlargedCrop] = useState<string | null>(null);

  // Dynamic textarea auto-height: supports Shift+Enter expansion up to 3 lines without scrollbar
  useEffect(() => {
    const el = inputRef.current;
    if (!el) return;
    el.style.height = "auto";
    // Threshold allowing 3 full lines (approx 75-80px with 14px font & 1.5 line-height + padding) without scrollbar
    const maxHeightThreeLines = 88;
    const scrollHeight = el.scrollHeight;
    if (scrollHeight <= maxHeightThreeLines) {
      el.style.height = `${Math.max(26, scrollHeight)}px`;
      el.style.overflowY = "hidden";
    } else {
      el.style.height = `${maxHeightThreeLines}px`;
      el.style.overflowY = "auto";
    }
  }, [question]);

  // Sync if parent updates initialCropSrc or displayCropSrc
  useEffect(() => {
    if (initialCropSrc && !cropSrc) {
      setCropSrc(initialCropSrc);
    } else if (displayCropSrc && !cropSrc) {
      setCropSrc(displayCropSrc);
    }
  }, [initialCropSrc, displayCropSrc, cropSrc]);

  // Robust multi-tier lazy loader for Lens & Figure/Table crop
  useEffect(() => {
    if (cropSrc) return;

    const normType = bundle.blockType.toLowerCase();
    const isLens = artifact.kind.startsWith("lens_");
    const isFigOrTab =
      isLens ||
      normType.includes("figure") ||
      normType.includes("image") ||
      normType.includes("picture") ||
      normType.includes("table");

    if (!isFigOrTab) return;

    let cancelled = false;
    setCropLoading(true);

    const loadCrop = async () => {
      // 1. Try desktop saved asset from disk
      if (typeof window !== "undefined" && desktopClient.runtime === "desktop") {
        try {
          const path = await desktopClient.open<string | null>(
            "get_artifact_asset_path",
            { artifactId: artifact.id },
          );
          if (!cancelled && path) {
            const url = convertFileSrc(path);
            setCropSrc(url);
            bundle.cropDataUrl = url;
            setCropLoading(false);
            return;
          }
        } catch {
          // fallback to PDF rendering
        }
      }

      // 2. Try PDF Canvas render
      if (pdfDocument) {
        try {
          const ev = artifact.evidence?.[0];
          const pageNumber = ev?.pageNumber ?? bundle.pageNumber ?? 1;
          const bbox = ev?.bbox ?? bundle.bbox ?? [0, 0, 1000, 1000];
          const material = await createLensCrops(
            pdfDocument,
            pageNumber,
            bbox,
            rotation,
          );
          if (!cancelled && material.displayCropDataUrl) {
            setCropSrc(material.displayCropDataUrl);
            bundle.cropDataUrl = material.displayCropDataUrl;
            setCropLoading(false);
            return;
          }
        } catch (err) {
          console.warn("Could not extract crop for BlockCardQaDetail", err);
        }
      }

      if (!cancelled) {
        setCropLoading(false);
      }
    };

    void loadCrop();

    return () => {
      cancelled = true;
    };
  }, [artifact, bundle, cropSrc, pdfDocument, rotation]);

  const content = record(artifact.content);

  const suggestedQuestions = useMemo(() => {
    return items(content.suggestedQuestions)
      .map(text)
      .filter(Boolean);
  }, [content]);

  const latestAssistant = useMemo(() => {
    return [...lensQa].reverse().find((msg) => msg.role === "assistant");
  }, [lensQa]);

  const handleSubmit = async (overrideText?: string) => {
    const q = (overrideText ?? question).trim();
    if (!q || lensQaBusy) return;
    setQuestion("");
    await onAskLens(q, latestAssistant?.id ?? null);
  };

  const targetPage = artifact.evidence[0]?.pageNumber ?? bundle.pageNumber;

  return (
    <div className="block-qa-detail-view">
      {/* Top Header Bar */}
      <header className="block-qa-detail-header">
        <button
          type="button"
          className="block-qa-back-btn"
          onClick={onBack}
          aria-label={t("blockCard.qa.back")}
        >
          <span>←</span>
          <span>{t("blockCard.qa.back")}</span>
        </button>

        <div className="block-qa-header-meta">
          <button
            type="button"
            className="block-card-jump-badge"
            title={t("blockCard.jumpToPage", { page: targetPage })}
            onClick={() => onJump(targetPage, bundle.blockId)}
          >
            <span>📄 p.{targetPage} · #{bundle.blockIndex + 1}</span>
            <i>↗</i>
          </button>
          <span className="block-qa-kind-badge">
            {artifactLabel(artifact)}
          </span>
        </div>

        {onTransferLens ? (
          <button
            type="button"
            className="btn-liquid-pill primary block-qa-transfer-btn"
            onClick={() => onTransferLens(artifact)}
            title={t("blockCard.qa.transferTitle")}
          >
            {t("blockCard.qa.transfer")} <span>→</span>
          </button>
        ) : null}
      </header>

      {/* Main Scrollable Content: Image + Full Lens Results + Evidence + Lens QA */}
      <div
        className="block-qa-detail-body artifact-detail-body"
        onCopy={(event) =>
          handleMarkdownCopyEvent(event, event.currentTarget)
        }
      >
        {/* Source Crop Image at the very top */}
        {cropLoading && !cropSrc ? (
          <div className="block-qa-crop-loading">
            <span className="status-pip is-busy" />
            <span>{t("blockCard.qa.extractingCrop")}</span>
          </div>
        ) : cropSrc ? (
          <figure className="block-qa-crop-card">
            <div
              className="block-qa-crop-wrapper"
              onClick={() => setEnlargedCrop(cropSrc)}
              title={t("blockCard.qa.fullscreenFigure")}
            >
              <img src={cropSrc} alt={t("blockCard.qa.cropAlt")} />
              <div className="block-qa-crop-overlay">
                <span>🔍 {t("blockCard.qa.fullscreenPreview")}</span>
              </div>
            </div>
            <figcaption className="block-qa-crop-caption">
              <span>📷 {t("blockCard.qa.cropCaption", { page: targetPage })}</span>
              <button
                type="button"
                className="block-qa-crop-zoom-btn"
                onClick={() => setEnlargedCrop(cropSrc)}
              >
                {t("blockCard.qa.zoom")} ↗
              </button>
            </figcaption>
          </figure>
        ) : null}

        {/* Full Lens Result displayed naturally below the image */}
        <GeneratedDetail
          artifact={artifact}
          displayCropSrc=""
        />

        {/* Evidence Anchors */}
        {artifact.evidence.length > 0 ? (
          <section className="artifact-evidence">
            <span>{t("completion.evidence")}</span>
            <div className="artifact-evidence-pills">
              {artifact.evidence.map((ev, idx) => (
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

        {/* Lens QA Section (Conversation Messages + Suggested Questions) */}
        <section className="lens-qa">
          <div className="lens-qa-head">
            <span>{t("completion.lens_questions")}</span>
            <small>{t("completion.isolated_from_discussion")}</small>
          </div>

          {lensQa.map((msg) => (
            <article className={msg.role} key={msg.id}>
              <span>{msg.role === "user" ? "YOU" : "LENS"}</span>
              <MarkdownBody>{msg.content}</MarkdownBody>
            </article>
          ))}

          {lensQaBusy && (
            <div className="block-qa-busy-indicator">
              <i className="status-pip is-busy" />
              <span>{t("blockCard.qa.thinking")}</span>
            </div>
          )}

          {/* Suggested Questions */}
          {suggestedQuestions.length > 0 ? (
            <div className="lens-suggested-questions">
              <span className="lens-suggested-title">💡 {t("blockCard.qa.suggestions")}</span>
              <div className="lens-suggested-list">
                {suggestedQuestions.map((sq, idx) => (
                  <button
                    key={`${sq}-${idx}`}
                    type="button"
                    className="lens-suggested-chip"
                    onClick={() => {
                      setQuestion(sq);
                      inputRef.current?.focus();
                    }}
                    title={t("blockCard.qa.fillInput")}
                  >
                    <MarkdownBody>{sq}</MarkdownBody>
                  </button>
                ))}
              </div>
            </div>
          ) : null}
        </section>

        {/* Detail Meta Footer */}
        <div className="artifact-detail-meta-footer">
          <span>
            {modelLabel ? t("blockCard.generatedBy", { model: modelLabel }) : ""}
            {new Date(artifact.createdAt).toLocaleDateString(dateLocale, {
              month: "short",
              day: "numeric",
            })}{" "}
            {new Date(artifact.createdAt).toLocaleTimeString(dateLocale, {
              hour: "2-digit",
              minute: "2-digit",
            })}
          </span>
        </div>
      </div>

      {/* Pinned Bottom Input Bar: Fixed at bottom with compact padding & auto-expanding multi-line textarea */}
      <footer className="block-qa-bottom-bar">
        <div className="lens-qa-composer composer-compact-capsule">
          <textarea
            ref={inputRef}
            className="composer-input-line"
            value={question}
            onChange={(e) => setQuestion(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                void handleSubmit();
              }
            }}
            placeholder={t("blockCard.qa.placeholder")}
            rows={1}
          />
          <button
            type="button"
            className="composer-action-btn send"
            disabled={lensQaBusy || !question.trim()}
            onClick={() => void handleSubmit()}
            aria-label={t("blockCard.qa.send")}
            title={t("blockCard.qa.sendTitle")}
          >
            {lensQaBusy ? "…" : "↑"}
          </button>
        </div>
      </footer>

      {/* Lightbox Modal */}
      {enlargedCrop && (
        <div className="lightbox-modal" onClick={() => setEnlargedCrop(null)}>
          <div className="lightbox-content" onClick={(e) => e.stopPropagation()}>
            <button
              type="button"
              className="lightbox-close"
              onClick={() => setEnlargedCrop(null)}
              aria-label={t("blockCard.qa.closeZoom")}
            >
              ×
            </button>
            <img src={enlargedCrop} alt={t("blockCard.qa.zoomAlt")} />
          </div>
        </div>
      )}
    </div>
  );
}
