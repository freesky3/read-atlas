import { AuxiliaryMetadata, AuxiliarySources } from "./AuxiliaryMetadata";
import type { AuxiliaryDocumentArtifactKind } from "./types";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import DeleteVersionConfirmModal from "./components/DeleteVersionConfirmModal";
import FontSizeStepper from "./components/FontSizeStepper";
import RegenerateConfirmModal from "./components/RegenerateConfirmModal";
import type { DiscussionFontSize } from "./discussionFont";
import MarkdownBody from "./MarkdownBody";
import {
  ensureDisplayMath,
  ensureInlineMath,
  handleMarkdownCopyEvent,
  hasMathDelimiters,
  wrapBareInlineMath,
} from "./markdown";
import type { PDFDocumentProxy } from "pdfjs-dist";
import BlockCardStream from "./components/BlockCardStream";
import type { BlockArtifactBundle } from "./components/BlockCard";
import type {
  ArtifactProjection,
  BlockAction,
  DocumentKind,
  LensQaProjection,
  OcrBlockProjection,
} from "./types";
import { useLocale, type TranslateFn } from "./i18n/LocaleContext";
import { t as translate } from "./i18n/t";

type ArtifactPanelProps = {
  artifacts: ArtifactProjection[];
  activeArtifactId: string;
  activeBlockId: string | null;
  activeBlockLabel?: string;
  displayCropSrc: string;
  lensQa: LensQaProjection[];
  lensQaBusy: boolean;
  blocks?: OcrBlockProjection[];
  pdfDocument?: PDFDocumentProxy | null;
  rotation?: number;
  onGenerateBlockAction?: (
    block: OcrBlockProjection,
    action: BlockAction,
  ) => void;
  onSelect: (artifact: ArtifactProjection) => void;
  onJump: (page: number, blockId?: string | null) => void;
  onAskLens: (question: string, parentId: string | null) => Promise<void>;
  onTransferLens: (artifact: ArtifactProjection) => void;
  onSetOverride: (
    artifact: ArtifactProjection,
    key: string,
    value: Record<string, unknown>,
  ) => Promise<void>;
  onGenerateBrief: () => void;
  onGenerateDocumentArtifact?: (
    kind: AuxiliaryDocumentArtifactKind,
    useBrief: boolean,
  ) => void;
  busyDocumentArtifacts?: string[];
  onRegenerateLens?: (artifact: ArtifactProjection) => void;
  onDeleteArtifactVersion?: (artifactId: string) => Promise<void> | void;
  modelLabel?: string;
  onOpenOutline?: () => void;
  preferBrief?: boolean;
  orientationBusy?: boolean;
  documentKind?: DocumentKind;
  discussionFontSize?: DiscussionFontSize;
  onDiscussionFontSizeChange?: (value: DiscussionFontSize) => void;
  scopeTab?: "global" | "block";
  onScopeTabChange?: (value: "global" | "block") => void;
  hideScopeTabs?: boolean;
  showAnnotations?: boolean;
  annotationCount?: number;
  annotationsSlot?: ReactNode;
  onShowAnnotations?: () => void;
  onUpdateTags?: (paperId: string, tags: string[]) => Promise<void>;
  onUpdateMetadata?: (
    revisionId: string,
    metadata: Record<string, unknown>,
    pinnedFields: string[],
  ) => Promise<void>;
  onUpdateOrientationTable?: (
    revisionId: string,
    kind: string,
    entries: Record<string, unknown>[],
    pinnedKeys: string[],
    artifactId?: string,
  ) => Promise<void>;
};

export type JsonRecord = Record<string, unknown>;
export function record(value: unknown): JsonRecord {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as JsonRecord)
    : {};
}

export function text(value: unknown): string {
  return typeof value === "string" ? value : "";
}

export function items(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}


const KIND_I18N: Record<string, string> = {
  brief: "artifacts.kinds.brief",
  glossary: "artifacts.kinds.glossary",
  symbol_table: "artifacts.kinds.symbolTable",
  metadata: "artifacts.kinds.metadata",
  translation: "artifacts.kinds.translation",
  explanation: "artifacts.kinds.explanation",
  lens_formula: "artifacts.kinds.lensFormula",
  lens_figure: "artifacts.kinds.lensFigure",
  lens_table: "artifacts.kinds.lensTable",
};

const defaultTranslate: TranslateFn = (key, vars) =>
  translate("zh-CN", key, vars);

export function artifactKindLabel(
  kind: string,
  t: TranslateFn = defaultTranslate,
): string {
  const key = KIND_I18N[kind];
  return key ? t(key) : kind.replaceAll("_", " ");
}

export function artifactLabel(
  artifact: ArtifactProjection,
  t: TranslateFn = defaultTranslate,
): string {
  return artifactKindLabel(artifact.kind, t);
}

export function artifactSubtitle(
  artifact: ArtifactProjection,
  t: TranslateFn = defaultTranslate,
): string {
  if (artifact.objectKey || artifact.evidence.length > 0) {
    const block = record(record(artifact.content).sourceBlock);
    const page = Number(block.pageNumber) || artifact.evidence[0]?.pageNumber;
    return Number.isFinite(page)
      ? t("artifacts.subtitle.selectionWithPage", { page: String(page) })
      : t("artifacts.subtitle.selection");
  }
  return t("artifacts.subtitle.full");
}

function selectionPageLabel(artifact: ArtifactProjection): string {
  if (artifact.objectKey || artifact.evidence.length > 0) {
    const block = record(record(artifact.content).sourceBlock);
    const page = Number(block.pageNumber) || artifact.evidence[0]?.pageNumber;
    return Number.isFinite(page) ? "p." + page : "";
  }
  return "";
}

export function artifactIcon(kind: string): string {
  if (kind === "brief") return "✦";
  if (kind === "glossary") return "📑";
  if (kind === "symbol_table") return "∑";
  if (kind === "metadata") return "ℹ️";
  if (kind === "lens_figure") return "🖼️";
  if (kind === "lens_formula") return "📐";
  if (kind === "lens_table") return "📊";
  if (kind === "translation") return "Aa";
  if (kind === "explanation") return "💡";
  return "📄";
}

export type ArtifactGroup = {
  id: string;
  kind: string;
  pageNumber?: number;
  bbox?: [number, number, number, number] | null;
  versions: ArtifactProjection[];
  latestArtifact: ArtifactProjection;
};

function computeIou(
  bbox1: [number, number, number, number],
  bbox2: [number, number, number, number],
): number {
  const [ax1, ay1, ax2, ay2] = bbox1;
  const [bx1, by1, bx2, by2] = bbox2;
  const ix1 = Math.max(ax1, bx1);
  const iy1 = Math.max(ay1, by1);
  const ix2 = Math.min(ax2, bx2);
  const iy2 = Math.min(ay2, by2);
  if (ix2 <= ix1 || iy2 <= iy1) return 0;
  const interArea = (ix2 - ix1) * (iy2 - iy1);
  const aArea = Math.max(1, (ax2 - ax1) * (ay2 - ay1));
  const bArea = Math.max(1, (bx2 - bx1) * (by2 - by1));
  return interArea / Math.min(aArea, bArea);
}

export function groupArtifacts(
  artifacts: ArtifactProjection[],
): ArtifactGroup[] {
  const groups: ArtifactGroup[] = [];

  for (const artifact of artifacts) {
    const isBlockArtifact =
      artifact.kind.startsWith("lens_") ||
      artifact.kind === "translation" ||
      artifact.kind === "explanation";

    if (!isBlockArtifact) {
      const existing = groups.find((g) => g.kind === artifact.kind);
      if (existing) {
        existing.versions.push(artifact);
      } else {
        groups.push({
          id: `${artifact.kind}:${artifact.objectKey || ""}`,
          kind: artifact.kind,
          versions: [artifact],
          latestArtifact: artifact,
        });
      }
      continue;
    }

    const ev = artifact.evidence[0];
    const pageNumber = ev?.pageNumber;
    const bbox = ev?.bbox;

    let matchedGroup = groups.find((g) => {
      if (g.kind !== artifact.kind) return false;
      if (
        artifact.objectKey &&
        g.versions.some((v) => v.objectKey === artifact.objectKey)
      ) {
        return true;
      }
      if (pageNumber && g.pageNumber === pageNumber && bbox && g.bbox) {
        return computeIou(g.bbox, bbox) > 0.4;
      }
      return false;
    });

    if (matchedGroup) {
      matchedGroup.versions.push(artifact);
      if (pageNumber && !matchedGroup.pageNumber)
        matchedGroup.pageNumber = pageNumber;
      if (bbox && !matchedGroup.bbox) matchedGroup.bbox = bbox;
    } else {
      groups.push({
        id: `${artifact.kind}:${artifact.objectKey || artifact.id}`,
        kind: artifact.kind,
        pageNumber,
        bbox,
        versions: [artifact],
        latestArtifact: artifact,
      });
    }
  }

  for (const group of groups) {
    group.versions.sort((a, b) => {
      const aTime = new Date(a.createdAt).getTime();
      const bTime = new Date(b.createdAt).getTime();
      if (!Number.isNaN(aTime) && !Number.isNaN(bTime) && aTime !== bTime) {
        return aTime - bTime;
      }
      return (a.version || 0) - (b.version || 0);
    });
    group.latestArtifact = group.versions[group.versions.length - 1];
  }

  return groups;
}

export function bundleArtifactsByBlock(
  artifacts: ArtifactProjection[],
  blocks: OcrBlockProjection[] = [],
  activeBlockId: string | null = null,
  activeBlockLabel?: string,
  displayCropSrc?: string,
): BlockArtifactBundle[] {
  const blockArtifacts = artifacts.filter(
    (a) =>
      a.kind.startsWith("lens_") ||
      a.kind === "translation" ||
      a.kind === "explanation",
  );

  const groups = groupArtifacts(blockArtifacts);
  const bundleMap = new Map<string, BlockArtifactBundle>();

  for (const group of groups) {
    const ev = group.latestArtifact.evidence[0];
    const pageNumber = group.pageNumber || ev?.pageNumber || 1;
    const bbox = group.bbox || ev?.bbox || [0, 0, 1000, 1000];

    let matchedOcrBlock = blocks.find((b) => {
      if (
        group.versions.some(
          (v) =>
            v.objectKey === b.id || v.evidence.some((e) => e.blockId === b.id),
        )
      ) {
        return true;
      }
      if (b.pageNumber === pageNumber && b.bbox) {
        return computeIou(b.bbox, bbox) > 0.4;
      }
      return false;
    });

    const blockId =
      matchedOcrBlock?.id ||
      group.latestArtifact.objectKey ||
      ev?.blockId ||
      group.id;
    const blockIndex = matchedOcrBlock?.blockIndex ?? 0;
    const blockType =
      matchedOcrBlock?.blockType || group.kind.replace("lens_", "");
    const textContent = matchedOcrBlock?.textContent || ev?.excerpt || "";

    let bundle = bundleMap.get(blockId);
    if (!bundle) {
      bundle = {
        blockId,
        pageNumber,
        blockIndex,
        blockType,
        textContent,
        bbox,
        cropDataUrl:
          displayCropSrc && activeBlockId === blockId
            ? displayCropSrc
            : undefined,
        groupsByKind: new Map(),
        allArtifacts: [],
      };
      bundleMap.set(blockId, bundle);
    }

    bundle.groupsByKind.set(group.kind, group);
    bundle.allArtifacts.push(...group.versions);
  }

  if (activeBlockId && !bundleMap.has(activeBlockId)) {
    const ocr = blocks.find((b) => b.id === activeBlockId);
    const action: BlockAction =
      activeBlockLabel === "translate"
        ? "translate"
        : activeBlockLabel === "explain"
          ? "explain"
          : "lens";
    if (ocr) {
      bundleMap.set(activeBlockId, {
        blockId: activeBlockId,
        pageNumber: ocr.pageNumber,
        blockIndex: ocr.blockIndex,
        blockType: ocr.blockType,
        textContent: ocr.textContent,
        bbox: ocr.bbox,
        cropDataUrl: displayCropSrc,
        groupsByKind: new Map(),
        allArtifacts: [],
        isGenerating: true,
        generatingAction: action,
      });
    } else {
      bundleMap.set(activeBlockId, {
        blockId: activeBlockId,
        pageNumber: 1,
        blockIndex: 0,
        blockType: "paragraph",
        textContent: "",
        bbox: [0, 0, 1000, 1000],
        cropDataUrl: displayCropSrc,
        groupsByKind: new Map(),
        allArtifacts: [],
        isGenerating: true,
        generatingAction: action,
      });
    }
  }

  const sorted = Array.from(bundleMap.values()).sort((a, b) => {
    if (a.pageNumber !== b.pageNumber) {
      return a.pageNumber - b.pageNumber;
    }
    if (a.blockIndex !== b.blockIndex) {
      return a.blockIndex - b.blockIndex;
    }
    return a.bbox[1] - b.bbox[1];
  });

  return sorted;
}

function Markdown({ children }: { children: string }) {
  return <MarkdownBody>{children}</MarkdownBody>;
}

function StringList({ value }: { value: unknown }) {
  const values = items(value).map(text).filter(Boolean);
  if (values.length === 0) return null;
  return (
    <ul className="artifact-string-list">
      {values.map((entry, index) => (
        <li key={entry + "-" + index}>
          <Markdown>{entry}</Markdown>
        </li>
      ))}
    </ul>
  );
}

function ArtifactSection({
  title,
  value,
  markdown = false,
}: {
  title: string;
  value: unknown;
  markdown?: boolean;
}) {
  const content = text(value);
  if (!content) return null;
  return (
    <section className="artifact-section">
      <h3 className="artifact-section-title">{title}</h3>
      {markdown ? <Markdown>{content}</Markdown> : <p>{content}</p>}
    </section>
  );
}

function formatSymbolLatex(sym: string): string {
  const trimmed = sym.trim();
  if (!trimmed) return "";
  if (hasMathDelimiters(trimmed)) return trimmed;
  return `$${trimmed}$`;
}

function EditableOrientationTableRow({
  artifact,
  row,
  kind,
  isPinned,
  onSaveRow,
  onTogglePin,
  onDeleteRow,
  isNew,
  onCancelNew,
  onJump,
}: {
  artifact: ArtifactProjection;
  row: JsonRecord;
  kind: "glossary" | "symbol_table";
  isPinned: boolean;
  onSaveRow: (oldKey: string, newRow: JsonRecord) => Promise<void>;
  onTogglePin: (key: string) => void;
  onDeleteRow: (key: string) => void;
  isNew?: boolean;
  onCancelNew?: () => void;
  onJump?: (page: number) => void;
}) {
  const { t } = useLocale();
  const isGlossary = kind === "glossary";
  const keyField = isGlossary ? "term" : "symbol";
  const valueField = isGlossary ? "definition" : "meaning";
  const extraField = isGlossary ? "aliases" : "scope";

  const entryKey = text(row[keyField]);
  const entryId = text(row._id) || entryKey;
  const [draftUsage, setDraftUsage] = useState(text(row.usage));
  const [editing, setEditing] = useState(Boolean(isNew));
  const [draftKey, setDraftKey] = useState(entryKey);
  const [draftVal, setDraftVal] = useState(() =>
    wrapBareInlineMath(text(row[valueField])),
  );
  const [draftExtra, setDraftExtra] = useState(() =>
    isGlossary
      ? items(row.aliases).map(text).filter(Boolean).join(", ")
      : text(row.scope),
  );
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  const startEditing = () => {
    setDraftKey(text(row[keyField]));
    setDraftUsage(text(row.usage));
    setDraftVal(wrapBareInlineMath(text(row[valueField])));
    setDraftExtra(
      isGlossary
        ? items(row.aliases).map(text).filter(Boolean).join(", ")
        : text(row.scope),
    );
    setError("");
    setEditing(true);
  };

  const save = async () => {
    if (!draftKey.trim()) {
      setError(isGlossary ? t("artifacts.table.emptyTerm") : t("artifacts.table.emptySymbol"));
      return;
    }
    if (!draftVal.trim()) {
      setError(isGlossary ? t("artifacts.table.emptyDefinition") : t("artifacts.table.emptyMeaning"));
      return;
    }
    setSaving(true);
    setError("");
    try {
      const updatedRow: JsonRecord = {
        ...row,
        ...(isGlossary ? { usage: draftUsage } : {}),
        [keyField]: draftKey.trim(),
        [valueField]: draftVal.trim(),
        [extraField]: isGlossary
          ? draftExtra
              .split(/[,，]/)
              .map((s) => s.trim())
              .filter(Boolean)
          : draftExtra.trim(),
      };
      await onSaveRow(entryId, updatedRow);
      setEditing(false);
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setSaving(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
      e.preventDefault();
      void save();
    } else if (e.key === "Escape") {
      if (isNew && onCancelNew) {
        onCancelNew();
      } else {
        setDraftKey(text(row[keyField]));
        setDraftVal(wrapBareInlineMath(text(row[valueField])));
        setDraftExtra(
          isGlossary
            ? items(row.aliases).map(text).filter(Boolean).join(", ")
            : text(row.scope),
        );
        setEditing(false);
        setError("");
      }
    }
  };

  const aliases = isGlossary
    ? items(row.aliases).map(text).filter(Boolean)
    : [];
  const scope = !isGlossary ? text(row.scope) : "";

  return (
    <tr
      className={
        "orientation-table-row" +
        (isPinned ? " orientation-row-pinned" : "") +
        (editing ? " orientation-row-editing" : "")
      }
    >
      {/* Key / Symbol */}
      <td className="orientation-table-col-key">
        {editing ? (
          <div className="table-cell-edit-wrap">
            <input
              className="artifact-meta-edit-input"
              value={draftKey}
              onChange={(e) => setDraftKey(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder={isGlossary ? t("artifacts.table.termPlaceholder") : t("artifacts.table.symbolPlaceholder")}
              autoFocus={isNew}
            />
          </div>
        ) : (
          <div className="orientation-key-wrapper">
            {kind === "symbol_table" ? (
              <span className="orientation-symbol-badge">
                <Markdown>{formatSymbolLatex(entryKey || "—")}</Markdown>
              </span>
            ) : (
              <strong className="orientation-term-text">
                <Markdown>{wrapBareInlineMath(entryKey || "—")}</Markdown>
              </strong>
            )}
            {isPinned ? (
              <span
                className="override-badge pinned"
                title={t("artifacts.table.pinnedLockTitle")}
              >
                {t("artifacts.table.locked")}
              </span>
            ) : null}
          </div>
        )}
      </td>

      {/* Definition / Meaning */}
      <td
        className="orientation-table-col-def"
        onDoubleClick={() => {
          if (!editing) startEditing();
        }}
        title={editing ? undefined : t("artifacts.table.doubleClickEdit")}
      >
        {editing ? (
          <div className="table-override-editor">
            <textarea
              aria-label={t("artifacts.table.editEntry", { name: entryKey || t("artifacts.table.newEntry") })}
              value={draftVal}
              onChange={(e) => setDraftVal(e.target.value)}
              onKeyDown={handleKeyDown}
              disabled={saving}
              rows={3}
              placeholder={
                isGlossary
                  ? t("artifacts.table.definitionPlaceholder")
                  : t("artifacts.table.meaningPlaceholder")
              }
            />
            {isGlossary && (
              <textarea
                aria-label={t("artifacts.table.editUsage", { name: entryKey || t("artifacts.table.newEntry") })}
                value={draftUsage}
                onChange={(e) => setDraftUsage(e.target.value)}
                rows={3}
              />
            )}
            {error ? <div className="override-error">{error}</div> : null}
            <div className="table-override-actions">
              <button
                type="button"
                aria-label={t("artifacts.table.saveEntry")}
                className="btn-table-save"
                onClick={save}
                disabled={saving}
              >
                {saving ? t("artifacts.table.saving") : t("artifacts.table.save")}
              </button>
              <button
                type="button"
                aria-label={t("artifacts.table.cancelEdit")}
                className="btn-table-cancel"
                onClick={() => {
                  if (isNew && onCancelNew) {
                    onCancelNew();
                  } else {
                    setDraftKey(text(row[keyField]));
                    setDraftVal(wrapBareInlineMath(text(row[valueField])));
                    setDraftExtra(
                      isGlossary
                        ? items(row.aliases)
                            .map(text)
                            .filter(Boolean)
                            .join(", ")
                        : text(row.scope),
                    );
                    setEditing(false);
                    setError("");
                  }
                }}
                disabled={saving}
              >
                {t("artifacts.table.cancel")}
              </button>
              <span className="table-override-hint">
                {t("artifacts.table.editorHint")}
              </span>
            </div>
          </div>
        ) : (
          <div className="orientation-def-content">
            <Markdown>{wrapBareInlineMath(text(row[valueField]))}</Markdown>
            {isGlossary && !!row.usage && (
              <div>
                <strong>{t("artifacts.table.usage")}</strong>
                <Markdown>{text(row.usage)}</Markdown>
              </div>
            )}
            {!!row._manual && <small>{t("artifacts.table.manualNote")}</small>}
            {!!row._unmatched && <p>{t("artifacts.table.unmatched")}</p>}
            <AuxiliarySources sources={row.sources} onJump={onJump} />
          </div>
        )}
      </td>

      {/* Aliases or Scope */}
      <td className="orientation-table-col-extra">
        {editing ? (
          <div className="table-cell-edit-wrap">
            <input
              className="artifact-meta-edit-input"
              value={draftExtra}
              onChange={(e) => setDraftExtra(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder={
                isGlossary ? t("artifacts.table.aliasesPlaceholder") : t("artifacts.table.scopePlaceholder")
              }
            />
          </div>
        ) : isGlossary ? (
          aliases.length > 0 ? (
            <div className="orientation-aliases-list">
              {aliases.map((alias, i) => (
                <span key={alias + "-" + i} className="orientation-alias-pill">
                  <Markdown>{wrapBareInlineMath(alias)}</Markdown>
                </span>
              ))}
            </div>
          ) : (
            <span className="orientation-empty-text">—</span>
          )
        ) : (
          <span className="orientation-scope-text">
            <Markdown>
              {wrapBareInlineMath(scope.trim() ? scope : t("artifacts.table.globalScope"))}
            </Markdown>
          </span>
        )}
      </td>

      {/* Action Column */}
      <td className="orientation-table-col-action">
        {!editing ? (
          <div className="table-row-action-group">
            <button
              type="button"
              aria-label={t("artifacts.table.editNamed", { name: entryKey })}
              className="meta-mini-btn"
              onClick={() => startEditing()}
              title={t("artifacts.table.editInPlace")}
            >
              ✎
            </button>
            <button
              type="button"
              aria-label={t("artifacts.table.pinNamed", { name: entryKey })}
              className={`meta-mini-btn pin ${isPinned ? "pinned" : ""}`}
              onClick={() => onTogglePin(entryId)}
              title={
                isPinned ? t("artifacts.table.pinnedNoOverwrite") : t("artifacts.table.clickToPin")
              }
            >
              📌
            </button>
            <button
              type="button"
              aria-label={t("artifacts.table.deleteNamed", { name: entryKey })}
              className="meta-mini-btn danger"
              onClick={() => onDeleteRow(entryId)}
              title={t("artifacts.table.deleteEntry")}
            >
              🗑️
            </button>
          </div>
        ) : null}
      </td>
    </tr>
  );
}

function MetadataDetail({
  artifact,
  content,
  documentKind,
  onUpdateMetadata,
}: {
  artifact: ArtifactProjection;
  content: JsonRecord;
  documentKind?: DocumentKind;
  onUpdateMetadata?: (
    revisionId: string,
    metadata: Record<string, unknown>,
    pinnedFields: string[],
  ) => Promise<void>;
}) {
  const { t } = useLocale();
  const title = text(content.title);
  const authors = Array.isArray(content.authors)
    ? (content.authors as unknown[]).map(text).filter(Boolean)
    : text(content.authors)
      ? [text(content.authors)]
      : [];
  const publicationYear = content.publicationYear ?? content.year ?? "";
  const venue = text(content.venue);
  const doi = text(content.doi);
  const bookName = text(content.bookName);
  const isbn = text(content.isbn);
  const chapterNumber = text(content.chapterNumber);
  const abstract = text(content.abstract);

  const pinnedFields: string[] = useMemo(() => {
    const raw = content._pinned;
    if (Array.isArray(raw)) return raw.map(text).filter(Boolean);
    return [];
  }, [content._pinned]);

  const [editingField, setEditingField] = useState<string | null>(null);
  const [draftValue, setDraftValue] = useState("");

  const knownKeys = new Set([
    "title",
    "authors",
    "publicationYear",
    "year",
    "venue",
    "doi",
    "abstract",
    "bookName",
    "isbn",
    "chapterNumber",
    "_pinned",
  ]);
  const extraEntries = Object.entries(content).filter(
    ([k]) => !knownKeys.has(k) && !k.startsWith("_"),
  );

  const handleTogglePin = (field: string) => {
    if (!onUpdateMetadata) return;
    const isPinned = pinnedFields.includes(field);
    const nextPinned = isPinned
      ? pinnedFields.filter((f) => f !== field)
      : [...pinnedFields, field];
    const currentMeta = {
      ...content,
      _editBaseArtifactId: artifact.id,
      title,
      authors,
      publicationYear: publicationYear
        ? Number(publicationYear) || publicationYear
        : null,
      venue,
      doi,
      abstract,
      bookName,
      isbn,
      chapterNumber,
    };
    void onUpdateMetadata(artifact.revisionId, currentMeta, nextPinned);
  };

  const handleSaveField = (field: string, newValue: unknown) => {
    if (!onUpdateMetadata) return;
    const nextPinned = pinnedFields.includes(field)
      ? pinnedFields
      : [...pinnedFields, field];
    const currentMeta: Record<string, unknown> = {
      ...content,
      _editBaseArtifactId: artifact.id,
      title,
      authors,
      publicationYear: publicationYear
        ? Number(publicationYear) || publicationYear
        : null,
      venue,
      doi,
      abstract,
      bookName,
      isbn,
      chapterNumber,
    };
    currentMeta[field] = newValue;
    setEditingField(null);
    void onUpdateMetadata(artifact.revisionId, currentMeta, nextPinned);
  };

  const isPinned = (f: string) => pinnedFields.includes(f);

  return (
    <div className="artifact-metadata-container">
      {/* Primary Meta Card */}
      <div className="artifact-meta-header-card">
        <div className="artifact-meta-field-row title-row">
          {editingField === "title" ? (
            <div className="artifact-meta-inline-edit">
              <input
                autoFocus
                className="artifact-meta-edit-input"
                value={draftValue}
                onChange={(e) => setDraftValue(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter")
                    handleSaveField("title", draftValue.trim());
                  if (e.key === "Escape") setEditingField(null);
                }}
                onBlur={() => handleSaveField("title", draftValue.trim())}
              />
            </div>
          ) : (
            <div className="artifact-meta-display-group">
              <h2 className="artifact-meta-title">
                {title ||
                  (documentKind === "textbook" ? t("artifacts.meta.untitledTextbook") : t("artifacts.meta.untitledPaper"))}
              </h2>
              <div className="artifact-meta-action-btns">
                <button
                  type="button"
                  className="meta-action-btn edit"
                  onClick={() => {
                    setDraftValue(title);
                    setEditingField("title");
                  }}
                  title={t("artifacts.meta.editTitle")}
                  aria-label={t("artifacts.meta.editTitle")}
                >
                  ✎
                </button>
                <button
                  type="button"
                  className={`meta-action-btn pin ${isPinned("title") ? "pinned" : ""}`}
                  onClick={() => handleTogglePin("title")}
                  title={
                    isPinned("title")
                      ? t("artifacts.meta.pinnedKeep")
                      : t("artifacts.meta.unpinnedLock")
                  }
                  aria-label={t("artifacts.meta.pin")}
                >
                  📌
                </button>
              </div>
            </div>
          )}
        </div>

        <div className="artifact-meta-badges">
          {/* Year Badge */}
          <div className="artifact-meta-badge-wrap">
            {editingField === "publicationYear" ? (
              <input
                autoFocus
                className="artifact-meta-edit-input small"
                value={draftValue}
                onChange={(e) => setDraftValue(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    const num = parseInt(draftValue.trim(), 10);
                    handleSaveField(
                      "publicationYear",
                      isNaN(num) ? draftValue.trim() : num,
                    );
                  }
                  if (e.key === "Escape") setEditingField(null);
                }}
                onBlur={() => {
                  const num = parseInt(draftValue.trim(), 10);
                  handleSaveField(
                    "publicationYear",
                    isNaN(num) ? draftValue.trim() : num,
                  );
                }}
              />
            ) : (
              <span className="artifact-meta-badge">
                <span className="artifact-meta-badge-icon">📅</span>
                <span className="artifact-meta-badge-label">{t("artifacts.meta.year")}</span>
                <strong>{String(publicationYear || "—")}</strong>
                <button
                  type="button"
                  className="meta-mini-btn"
                  onClick={() => {
                    setDraftValue(String(publicationYear));
                    setEditingField("publicationYear");
                  }}
                  title={t("artifacts.meta.editYear")}
                >
                  ✎
                </button>
                <button
                  type="button"
                  className={`meta-mini-btn pin ${isPinned("publicationYear") ? "pinned" : ""}`}
                  onClick={() => handleTogglePin("publicationYear")}
                  title={isPinned("publicationYear") ? t("artifacts.meta.pinned") : t("artifacts.meta.unpinned")}
                >
                  📌
                </button>
              </span>
            )}
          </div>

          {documentKind === "textbook" ? (
            <>
              <div className="artifact-meta-badge-wrap">
                <span className="artifact-meta-badge">
                  <span className="artifact-meta-badge-icon">📖</span>
                  <span className="artifact-meta-badge-label">{t("artifacts.meta.bookName")}</span>
                  <strong>{bookName || "—"}</strong>
                </span>
              </div>
              <div className="artifact-meta-badge-wrap">
                <span className="artifact-meta-badge">
                  <span className="artifact-meta-badge-icon">🔖</span>
                  <span className="artifact-meta-badge-label">ISBN:</span>
                  <strong>{isbn || "—"}</strong>
                </span>
              </div>
              <div className="artifact-meta-badge-wrap">
                <span className="artifact-meta-badge">
                  <span className="artifact-meta-badge-icon">📑</span>
                  <span className="artifact-meta-badge-label">{t("artifacts.meta.chapter")}</span>
                  <strong>{chapterNumber || "—"}</strong>
                </span>
              </div>
            </>
          ) : null}

          {/* Venue Badge */}
          <div className="artifact-meta-badge-wrap">
            {editingField === "venue" ? (
              <input
                autoFocus
                className="artifact-meta-edit-input small"
                value={draftValue}
                onChange={(e) => setDraftValue(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter")
                    handleSaveField("venue", draftValue.trim());
                  if (e.key === "Escape") setEditingField(null);
                }}
                onBlur={() => handleSaveField("venue", draftValue.trim())}
              />
            ) : (
              <span className="artifact-meta-badge">
                <span className="artifact-meta-badge-icon">🏛️</span>
                <span className="artifact-meta-badge-label">{t("artifacts.meta.venue")}</span>
                <strong>{venue || "—"}</strong>
                <button
                  type="button"
                  className="meta-mini-btn"
                  onClick={() => {
                    setDraftValue(venue);
                    setEditingField("venue");
                  }}
                  title={t("artifacts.meta.editVenue")}
                >
                  ✎
                </button>
                <button
                  type="button"
                  className={`meta-mini-btn pin ${isPinned("venue") ? "pinned" : ""}`}
                  onClick={() => handleTogglePin("venue")}
                  title={isPinned("venue") ? t("artifacts.meta.pinned") : t("artifacts.meta.unpinned")}
                >
                  📌
                </button>
              </span>
            )}
          </div>

          {/* DOI Badge */}
          <div className="artifact-meta-badge-wrap">
            {editingField === "doi" ? (
              <input
                autoFocus
                className="artifact-meta-edit-input small"
                value={draftValue}
                onChange={(e) => setDraftValue(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter")
                    handleSaveField("doi", draftValue.trim());
                  if (e.key === "Escape") setEditingField(null);
                }}
                onBlur={() => handleSaveField("doi", draftValue.trim())}
              />
            ) : (
              <span className="artifact-meta-badge">
                <span className="artifact-meta-badge-icon">🔗</span>
                <span className="artifact-meta-badge-label">DOI:</span>
                {doi ? (
                  doi.startsWith("10.") ? (
                    <a
                      href={`https://doi.org/${doi}`}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="artifact-meta-link"
                      title={t("artifacts.meta.openDoi")}
                    >
                      {doi}
                    </a>
                  ) : (
                    <strong>{doi}</strong>
                  )
                ) : (
                  <strong>—</strong>
                )}
                <button
                  type="button"
                  className="meta-mini-btn"
                  onClick={() => {
                    setDraftValue(doi);
                    setEditingField("doi");
                  }}
                  title={t("artifacts.meta.editDoi")}
                >
                  ✎
                </button>
                <button
                  type="button"
                  className={`meta-mini-btn pin ${isPinned("doi") ? "pinned" : ""}`}
                  onClick={() => handleTogglePin("doi")}
                  title={isPinned("doi") ? t("artifacts.meta.pinned") : t("artifacts.meta.unpinned")}
                >
                  📌
                </button>
              </span>
            )}
          </div>
        </div>

        {/* Authors Section */}
        <div className="artifact-meta-authors-section">
          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <span className="artifact-meta-authors-label">{t("artifacts.meta.authors")}</span>
            <button
              type="button"
              className="meta-mini-btn"
              onClick={() => {
                setDraftValue(authors.join(", "));
                setEditingField("authors");
              }}
              title={t("artifacts.meta.editAuthors")}
            >
              ✎
            </button>
            <button
              type="button"
              className={`meta-mini-btn pin ${isPinned("authors") ? "pinned" : ""}`}
              onClick={() => handleTogglePin("authors")}
              title={isPinned("authors") ? t("artifacts.meta.pinned") : t("artifacts.meta.unpinned")}
            >
              📌
            </button>
          </div>
          {editingField === "authors" ? (
            <div className="artifact-meta-inline-edit" style={{ marginTop: 6 }}>
              <input
                autoFocus
                className="artifact-meta-edit-input"
                placeholder={t("artifacts.meta.authorsPlaceholder")}
                value={draftValue}
                onChange={(e) => setDraftValue(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    const parsed = draftValue
                      .split(/[,，]/)
                      .map((s) => s.trim())
                      .filter(Boolean);
                    handleSaveField("authors", parsed);
                  }
                  if (e.key === "Escape") setEditingField(null);
                }}
                onBlur={() => {
                  const parsed = draftValue
                    .split(/[,，]/)
                    .map((s) => s.trim())
                    .filter(Boolean);
                  handleSaveField("authors", parsed);
                }}
              />
            </div>
          ) : (
            <div className="artifact-meta-authors-list">
              {authors.length > 0 ? (
                authors.map((author, index) => (
                  <span
                    key={author + "-" + index}
                    className="artifact-author-pill"
                  >
                    {author}
                  </span>
                ))
              ) : (
                <span className="artifact-meta-empty">{t("artifacts.meta.noAuthors")}</span>
              )}
            </div>
          )}
        </div>
      </div>

      {/* Abstract Card */}
      <div className="artifact-meta-abstract-card">
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            marginBottom: 6,
          }}
        >
          <h3 className="artifact-section-title" style={{ margin: 0 }}>
            {t("artifacts.meta.abstract")}
          </h3>
          <div style={{ display: "flex", gap: 6 }}>
            <button
              type="button"
              className="meta-mini-btn"
              onClick={() => {
                setDraftValue(abstract);
                setEditingField("abstract");
              }}
              title={t("artifacts.meta.editAbstract")}
            >
              ✎
            </button>
            <button
              type="button"
              className={`meta-mini-btn pin ${isPinned("abstract") ? "pinned" : ""}`}
              onClick={() => handleTogglePin("abstract")}
              title={isPinned("abstract") ? t("artifacts.meta.pinned") : t("artifacts.meta.unpinned")}
            >
              📌
            </button>
          </div>
        </div>
        {editingField === "abstract" ? (
          <textarea
            autoFocus
            className="artifact-meta-edit-textarea"
            rows={6}
            value={draftValue}
            onChange={(e) => setDraftValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") setEditingField(null);
            }}
            onBlur={() => handleSaveField("abstract", draftValue.trim())}
          />
        ) : (
          <p className="artifact-meta-abstract-text">
            {abstract || t("artifacts.meta.noAbstract")}
          </p>
        )}
      </div>

      {/* Extra Metadata if any */}
      {extraEntries.length > 0 ? (
        <div className="artifact-meta-extra-card">
          <h3 className="artifact-section-title">{t("artifacts.meta.otherProps")}</h3>
          <div className="artifact-meta-extra-grid">
            {extraEntries.map(([k, v]) => (
              <div key={k} className="artifact-meta-extra-item">
                <span className="artifact-meta-extra-key">{k}</span>
                <span className="artifact-meta-extra-value">
                  {Array.isArray(v)
                    ? v.map(text).filter(Boolean).join(", ")
                    : typeof v === "object"
                      ? JSON.stringify(v)
                      : String(v ?? "—")}
                </span>
              </div>
            ))}
          </div>
        </div>
      ) : null}
    </div>
  );
}

function orientationContent(artifact: ArtifactProjection): JsonRecord {
  const content = record(artifact.content);
  const overrides = record(artifact.overrides);
  if (
    !Object.keys(overrides).length ||
    !["glossary", "symbol_table"].includes(artifact.kind)
  )
    return content;
  const entries = items(content.entries).map(
    (row, i) =>
      ({
        ...record(row),
        _id: text(record(row)._id) || `legacy-${i}`,
      }) as JsonRecord,
  );
  const pins = items(content._pinned).map(text);
  const unmatched = [...items(content._unmatchedOverrides)];
  for (const [key, value] of Object.entries(overrides)) {
    const matches = entries.filter(
      (row) => row._id === key || row.term === key || row.symbol === key,
    );
    if (matches.length === 1) {
      Object.assign(matches[0], record(value), { _manual: true });
      pins.push(text(matches[0]._id));
    } else unmatched.push({ key, value });
  }
  return {
    ...content,
    entries,
    _pinned: [...new Set(pins)],
    _unmatchedOverrides: unmatched,
  };
}

function OrientationDetail({
  artifact,
  documentKind,
  onSetOverride,
  onUpdateTags,
  onUpdateMetadata,
  onUpdateOrientationTable,
  onJump,
}: {
  artifact: ArtifactProjection;
  documentKind?: DocumentKind;
  onSetOverride: ArtifactPanelProps["onSetOverride"];
  onUpdateTags?: ArtifactPanelProps["onUpdateTags"];
  onUpdateMetadata?: ArtifactPanelProps["onUpdateMetadata"];
  onUpdateOrientationTable?: ArtifactPanelProps["onUpdateOrientationTable"];
  onJump?: (page: number) => void;
}) {
  const { t } = useLocale();
  const content = orientationContent(artifact);
  const [isAddingTag, setIsAddingTag] = useState(false);
  const [newTagInput, setNewTagInput] = useState("");
  const [isAddingNew, setIsAddingNew] = useState(false);

  if (artifact.kind === "brief") {
    const textbookV2 = content.briefProtocol === "textbook-v2";
    const textbook = textbookV2 || documentKind === "textbook" || content.documentKind === "textbook";
    const rawKeywords = items(content.keywords).map(text).filter(Boolean);

    const handleRemoveTag = (tagToRemove: string) => {
      if (!onUpdateTags) return;
      const nextTags = rawKeywords.filter((k) => k !== tagToRemove);
      void onUpdateTags(artifact.paperId, nextTags);
    };

    const handleAddTag = () => {
      const clean = newTagInput.trim();
      if (!clean || !onUpdateTags) {
        setIsAddingTag(false);
        setNewTagInput("");
        return;
      }
      if (!rawKeywords.includes(clean)) {
        void onUpdateTags(artifact.paperId, [...rawKeywords, clean]);
      }
      setNewTagInput("");
      setIsAddingTag(false);
    };

    return (
      <div className="artifact-brief-container">
        {/* Core Takeaway Highlight at the top */}
        {content.takeaway ? (
          <div className="artifact-brief-takeaway-highlight">
            <ArtifactSection
              title={
                textbook
                  ? t("artifacts.brief.takeawayTextbook")
                  : t("artifacts.brief.takeawayPaper")
              }
              value={content.takeaway}
              markdown
            />
          </div>
        ) : content.summary ? (
          <ArtifactSection title={t("completion.summary")} value={content.summary} markdown />
        ) : null}

        {/* Academic Keywords Cloud directly below Takeaway */}
        <div className="artifact-tags-bar" style={{ margin: "12px 0 16px 0" }}>
          <div
            className="artifact-tags-label"
            style={{
              fontSize: 11,
              color: "var(--muted, #94a3b8)",
              marginBottom: 6,
            }}
          >
            {textbook ? t("artifacts.brief.keywordsTextbook") : t("artifacts.brief.keywordsPaper")}
          </div>
          <div
            className="artifact-chip-row"
            style={{
              display: "flex",
              flexWrap: "wrap",
              gap: 6,
              alignItems: "center",
            }}
          >
            {rawKeywords.map((keyword, index) => (
              <span
                key={keyword + "-" + index}
                className="artifact-chip-pill editable"
              >
                <span>{keyword}</span>
                {onUpdateTags ? (
                  <button
                    type="button"
                    className="tag-remove-btn"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleRemoveTag(keyword);
                    }}
                    title={t("artifacts.brief.removeTag", { keyword })}
                    aria-label={t("artifacts.brief.removeTag", { keyword })}
                  >
                    ×
                  </button>
                ) : null}
              </span>
            ))}

            {onUpdateTags ? (
              isAddingTag ? (
                <span className="artifact-tag-input-wrap">
                  <input
                    autoFocus
                    className="artifact-tag-inline-input"
                    placeholder={t("artifacts.brief.tagPlaceholder")}
                    value={newTagInput}
                    onChange={(e) => setNewTagInput(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") handleAddTag();
                      if (e.key === "Escape") {
                        setIsAddingTag(false);
                        setNewTagInput("");
                      }
                    }}
                    onBlur={handleAddTag}
                  />
                </span>
              ) : (
                <button
                  type="button"
                  className="artifact-add-tag-pill"
                  onClick={() => setIsAddingTag(true)}
                  title={t("artifacts.brief.addTag")}
                >
                  {t("artifacts.brief.addTagButton")}
                </button>
              )
            ) : null}
          </div>
        </div>

        {textbookV2 ? <>
          {([
            ["learningScope", t("artifacts.brief.learningScope")], ["motivation", t("artifacts.brief.motivation")],
            ["prerequisites", t("artifacts.brief.prerequisites")], ["knowledgeStructure", t("artifacts.brief.knowledgeStructure")],
            ["coreKnowledge", t("artifacts.brief.coreKnowledge")], ["masteryGoals", t("artifacts.brief.masteryGoals")],
            ["connections", t("artifacts.brief.connections")],
          ] as const).map(([key,label]) => <ArtifactSection key={key} title={label} value={content[key]} markdown />)}
        </> : <>
        {/* Structured Mentor Sections */}
        {content.classification ? (
          <ArtifactSection
            title={textbook ? t("artifacts.brief.classificationTextbook") : t("artifacts.brief.classificationPaper")}
            value={content.classification}
            markdown
          />
        ) : null}
        {content.context ? (
          <ArtifactSection
            title={textbook ? t("artifacts.brief.contextTextbook") : t("artifacts.brief.contextPaper")}
            value={content.context}
            markdown
          />
        ) : null}
        {content.backgroundAndProblem ? (
          <ArtifactSection
            title={t("artifacts.brief.problem")}
            value={content.backgroundAndProblem}
            markdown
          />
        ) : null}
        {content.coreMethod ? (
          <ArtifactSection
            title={textbook ? t("artifacts.brief.methodTextbook") : t("artifacts.brief.methodPaper")}
            value={content.coreMethod}
            markdown
          />
        ) : content.method ? (
          <ArtifactSection title={t("completion.method")} value={content.method} markdown />
        ) : null}
        {content.findings ? (
          <ArtifactSection
            title={textbook ? t("artifacts.brief.findingsTextbook") : t("artifacts.brief.findingsPaper")}
            value={content.findings}
            markdown
          />
        ) : null}
        {content.evaluation ? (
          <ArtifactSection
            title={textbook ? t("artifacts.brief.evalTextbook") : t("artifacts.brief.evalPaper")}
            value={content.evaluation}
            markdown
          />
        ) : content.limitations ? (
          <ArtifactSection
            title={t("completion.limitations")}
            value={content.limitations}
            markdown
          />
        ) : null}
        {content.futureWork ? (
          <ArtifactSection
            title={textbook ? t("artifacts.brief.futureTextbook") : t("artifacts.brief.futurePaper")}
            value={content.futureWork}
            markdown
          />
        ) : null}

        {/* Legacy fallback fields if any */}
        {!content.coreMethod && content.researchQuestion ? (
          <ArtifactSection
            title={t("completion.research_question")}
            value={content.researchQuestion}
            markdown
          />
        ) : null}
        </>}
      </div>
    );
  }
  if (artifact.kind === "glossary" || artifact.kind === "symbol_table") {
    const isGlossary = artifact.kind === "glossary";
    const keyField = isGlossary ? "term" : "symbol";
    const rawEntries = items(content.entries).map(
      (entry, index) =>
        ({
          ...record(entry),
          _id: text(record(entry)._id) || `legacy-${index}`,
        }) as JsonRecord,
    );
    const originalPins = items(content._pinned).map(text).filter(Boolean);
    const pinnedKeys = rawEntries
      .filter(
        (r) =>
          originalPins.includes(text(r._id)) ||
          originalPins.includes(text(r[keyField])),
      )
      .map((r) => text(r._id));

    const saveTable = (
      revisionId: string,
      kind: string,
      entries: JsonRecord[],
      pins: string[],
    ) => {
      if (!onUpdateOrientationTable) return Promise.resolve();
      return onUpdateOrientationTable(
        revisionId,
        kind,
        entries,
        pins,
        artifact.id,
      );
    };
    const handleSaveRow = async (oldKey: string, updatedRow: JsonRecord) => {
      const valueField = isGlossary ? "definition" : "meaning";
      if (!onUpdateOrientationTable) {
        await onSetOverride(
          artifact,
          items(content.entries).some((r) => text(record(r)._id) === oldKey)
            ? oldKey
            : text(updatedRow[keyField]),
          {
            [valueField]: updatedRow[valueField],
          },
        );
        return;
      }
      const newKey = oldKey || crypto.randomUUID();
      updatedRow = { ...updatedRow, _id: newKey };
      let nextEntries = [...rawEntries];
      const existingIdx = rawEntries.findIndex((r) => text(r._id) === oldKey);
      if (existingIdx !== -1) {
        nextEntries[existingIdx] = updatedRow;
      } else {
        nextEntries.push(updatedRow);
      }
      let nextPinned = [...pinnedKeys];
      if (oldKey && oldKey !== newKey) {
        nextPinned = nextPinned.filter((k) => k !== oldKey);
      }
      if (!nextPinned.includes(newKey)) {
        nextPinned.push(newKey);
      }
      await saveTable(
        artifact.revisionId,
        artifact.kind,
        nextEntries,
        nextPinned,
      );
      setIsAddingNew(false);
    };

    const handleTogglePin = (keyToToggle: string) => {
      if (!onUpdateOrientationTable) return;
      const nextPinned = pinnedKeys.includes(keyToToggle)
        ? pinnedKeys.filter((k) => k !== keyToToggle)
        : [...pinnedKeys, keyToToggle];
      void saveTable(
        artifact.revisionId,
        artifact.kind,
        rawEntries,
        nextPinned,
      );
    };

    const handleDeleteRow = (keyToDelete: string) => {
      if (!onUpdateOrientationTable) return;
      const nextEntries = rawEntries.filter((r) => text(r._id) !== keyToDelete);
      const nextPinned = pinnedKeys.filter((k) => k !== keyToDelete);
      void saveTable(
        artifact.revisionId,
        artifact.kind,
        nextEntries,
        nextPinned,
      );
    };

    return (
      <div className="artifact-table-wrapper">
        {items(content._unmatchedOverrides).length > 0 && (
          <details>
            <summary>{t("artifacts.table.oldOverrides")}</summary>
            {items(content._unmatchedOverrides).map((entry, i) => (
              <article key={i}>
                <strong>{text(record(entry).key)}</strong>
                {Object.values(record(record(entry).value)).map((value, j) => (
                  <Markdown key={j}>{text(value)}</Markdown>
                ))}
              </article>
            ))}
          </details>
        )}
        <div className="artifact-table-hint">
          <span>
            {t("artifacts.table.hint")}
          </span>
        </div>
        <table className="artifact-liquid-table">
          <thead>
            <tr>
              <th style={{ width: isGlossary ? "25%" : "20%" }}>
                {isGlossary ? t("artifacts.table.colTerm") : t("artifacts.table.colSymbol")}
              </th>
              <th style={{ width: isGlossary ? "48%" : "50%" }}>
                {isGlossary
                  ? t("artifacts.table.colDefinition")
                  : t("artifacts.table.colMeaning")}
              </th>
              <th style={{ width: isGlossary ? "27%" : "22%" }}>
                {isGlossary
                  ? t("artifacts.table.colAliases")
                  : t("artifacts.table.colScope")}
              </th>
              <th style={{ width: "80px", textAlign: "center" }}>{t("artifacts.table.colActions")}</th>
            </tr>
          </thead>
          <tbody>
            {rawEntries.map((row) => {
              const isPinned = pinnedKeys.includes(text(row._id));
              return (
                <EditableOrientationTableRow
                  key={text(row._id)}
                  artifact={artifact}
                  row={row}
                  kind={artifact.kind as "glossary" | "symbol_table"}
                  isPinned={isPinned}
                  onSaveRow={handleSaveRow}
                  onJump={onJump}
                  onTogglePin={handleTogglePin}
                  onDeleteRow={handleDeleteRow}
                />
              );
            })}
            {isAddingNew ? (
              <EditableOrientationTableRow
                artifact={artifact}
                row={{
                  [keyField]: "",
                  [isGlossary ? "definition" : "meaning"]: "",
                  [isGlossary ? "aliases" : "scope"]: isGlossary ? [] : "",
                }}
                kind={artifact.kind as "glossary" | "symbol_table"}
                isPinned={true}
                onSaveRow={handleSaveRow}
                onJump={onJump}
                onTogglePin={() => undefined}
                onDeleteRow={() => setIsAddingNew(false)}
                isNew={true}
                onCancelNew={() => setIsAddingNew(false)}
              />
            ) : null}
          </tbody>
        </table>
        <div
          style={{
            display: "flex",
            justifyContent: "flex-end",
            marginTop: 12,
          }}
        >
          {onUpdateOrientationTable && !isAddingNew ? (
            <button
              type="button"
              className="artifact-add-tag-pill"
              style={{
                padding: "6px 16px",
                fontSize: 12,
                cursor: "pointer",
                fontWeight: 600,
              }}
              onClick={() => setIsAddingNew(true)}
              title={isGlossary ? t("artifacts.table.addTerm") : t("artifacts.table.addSymbol")}
              aria-label={isGlossary ? t("artifacts.table.addTerm") : t("artifacts.table.addSymbol")}
            >
              {t("artifacts.table.addPrefix")} {isGlossary ? t("artifacts.table.addTerm") : t("artifacts.table.addSymbol")}
            </button>
          ) : null}
        </div>
      </div>
    );
  }
  if (content.document)
    return (
      <AuxiliaryMetadata
        artifact={artifact}
        onUpdate={onUpdateMetadata}
        onJump={onJump}
      />
    );
  return (
    <MetadataDetail
      artifact={artifact}
      content={content}
      onUpdateMetadata={onUpdateMetadata}
    />
  );
}
export function GeneratedDetail({
  artifact,
  displayCropSrc,
}: {
  artifact: ArtifactProjection;
  displayCropSrc: string;
}) {
  const { t } = useLocale();
  const content = record(artifact.content);
  if (artifact.kind === "translation") {
    const statusLabels: Record<string, string> = {
      translated: t("artifacts.translation.translated"),
      unchanged: t("artifacts.translation.unchanged"),
      partial: t("artifacts.translation.partial"),
      unavailable: t("artifacts.translation.unavailable"),
    };
    const languageLabels: Record<string, string> = { und: t("artifacts.translation.und"), mul: t("artifacts.translation.mul") };
    const statusLabel = statusLabels[text(content.status)];

    return (
      <>
        <div className="artifact-language-line">
          <span>{languageLabels[text(content.sourceLanguage)] ?? text(content.sourceLanguage)}</span> <span>→</span>{" "}
          {text(content.targetLanguage)}
          {statusLabel && <span className="status-badge" aria-label={t("artifacts.translation.statusAria")}>{statusLabel}</span>}
        </div>
        <ArtifactSection
          title={t("artifacts.translation.body")}
          value={content.translation}
          markdown
        />
        {items(content.notes).length > 0 && <h4>{t("artifacts.translation.notes")}</h4>}
        <StringList value={content.notes} />
        {items(content.terms).length > 0 && <h4>{t("artifacts.translation.terms")}</h4>}
        <div className="artifact-entry-list compact">
          {items(content.terms).map((term, index) => {
            const row = record(term);
            return (
              <article key={text(row.source) + "-" + index}>
                <strong>
                  <Markdown>{ensureInlineMath(text(row.source))}</Markdown>
                  <span> → </span>
                  <Markdown>{ensureInlineMath(text(row.target))}</Markdown>
                </strong>
                {row.note ? <Markdown>{text(row.note)}</Markdown> : null}
              </article>
            );
          })}
        </div>
      </>
    );
  }
  if (artifact.kind === "explanation") {
    return (
      <>
        <ArtifactSection
          title={t("artifacts.explanation.body")}
          value={content.explanation}
          markdown
        />
        {items(content.keyPoints).map(text).some(Boolean) ? (
          <section className="artifact-section">
            <h3 className="artifact-section-title">{t("artifacts.explanation.keyPoints")}</h3>
            <StringList value={content.keyPoints} />
          </section>
        ) : null}
        <ArtifactSection
          title={t("artifacts.explanation.connection")}
          value={content.paperConnection}
          markdown
        />
      </>
    );
  }

  const quick = record(content.quickTakeaway);
  const specific = record(content.formula ?? content.figure ?? content.table);
  const lensV2 = content.schemaVersion === 2 && content.lensProtocol === "v2";
  const lensStatus = lensV2 ? ({ complete: t("artifacts.lens.statusComplete"), partial: t("artifacts.lens.statusPartial"), unavailable: t("artifacts.lens.statusUnavailable") } as Record<string, string>)[text(content.status)] : undefined;
  const symbolSource: Record<string, string> = { paper_defined: content.documentKind === "textbook" ? t("artifacts.lens.sourceOriginal") : t("artifacts.lens.sourcePaper"), standard: t("artifacts.lens.sourceStandard"), inferred: t("artifacts.lens.sourceInferred"), unresolved: t("artifacts.lens.sourceUnresolved") };
  return (
    <>
      {displayCropSrc ? (
        <figure className="artifact-display-crop">
          <img src={displayCropSrc} alt={t("artifacts.lens.cropAlt")} />
          <figcaption>{t("artifacts.lens.cropCaption")}</figcaption>
        </figure>
      ) : null}
      {lensStatus ? <p aria-label={t("artifacts.lens.statusAria")}>{lensStatus}</p> : null}
      <section className="artifact-quick-take">
        <span>{t("artifacts.lens.quickTake")}</span>
        <h3>{text(quick.title)}</h3>
        <Markdown>{text(quick.markdown)}</Markdown>
      </section>
      {artifact.kind === "lens_formula" ? (
        <>
          <ArtifactSection
            title={t("artifacts.lens.intuit")}
            value={specific.whatItDoesMarkdown}
            markdown
          />
          <ArtifactSection
            title={t("artifacts.lens.readFormula")}
            value={specific.startHereMarkdown}
            markdown
          />
          {text(specific.reconstructedLatex).trim() ? (
            <div className="artifact-formula">
              <Markdown>{ensureDisplayMath(text(specific.reconstructedLatex))}</Markdown>
            </div>
          ) : null}
          <div className="artifact-entry-list compact">
            {items(specific.symbols).map((symbol, index) => {
              const row = record(symbol);
              return (
                <article key={text(row.symbolLatex) + "-" + index}>
                  <strong>
                    <Markdown>
                      {ensureInlineMath(text(row.symbolLatex))}
                    </Markdown>
                  </strong>
                  <Markdown>{text(row.meaningMarkdown)}</Markdown>
                  <small>{symbolSource[text(row.provenance)] ?? text(row.provenance)}</small>
                </article>
              );
            })}
          </div>
        </>
      ) : (
        <>
          <ArtifactSection title={artifact.kind === "lens_table" ? t("artifacts.lens.tableMeans") : t("artifacts.lens.figureMeans")} value={specific.overallMarkdown} markdown />
          {lensV2 ? <ArtifactSection title={artifact.kind === "lens_table" ? t("artifacts.lens.readTable") : t("artifacts.lens.readFigure")} value={specific.readingGuideMarkdown} markdown /> : null}
          {lensV2 && items(specific.focusPoints).length > 0 ? (
            <section className="artifact-section">
              <h3>{t("artifacts.lens.focus")}</h3>
              <ol>
                {items(specific.focusPoints).map((point, index) => {
                  const row = record(point);
                  return <li key={index}>
                    <strong>{text(row.location)}</strong>
                    <Markdown>{text(row.observationMarkdown)}</Markdown>
                    <Markdown>{text(row.meaningMarkdown)}</Markdown>
                  </li>;
                })}
              </ol>
            </section>
          ) : null}
        </>
      )}
      {items(content.sections).map((section, index) => {
        const row = record(section);
        return (
          <ArtifactSection
            key={text(row.sectionId) + "-" + index}
            title={text(row.title) || "Section " + (index + 1)}
            value={row.markdown}
            markdown
          />
        );
      })}
      {lensV2 && items(content.limitations).length > 0 ? (
        <section className="artifact-section" aria-label={t("artifacts.lens.limitsAria")}>
          <h3>{t("artifacts.lens.unconfirmed")}</h3>
          <ul>{items(content.limitations).map((limit, index) => <li key={index}><Markdown>{text(limit)}</Markdown></li>)}</ul>
        </section>
      ) : null}
    </>
  );
}

function BriefGeneratePrompt({
  documentKind,
  orientationBusy,
  onGenerateBrief,
}: {
  orientationBusy?: boolean;
  documentKind?: DocumentKind;
  onGenerateBrief: () => void;
}) {
  const { t } = useLocale();
  return (
    <div className="artifact-empty">
      <div className="artifact-empty-mark">◫</div>
      <span>{t("completion.brief")}</span>
      <h3>{t("artifacts.brief.emptyTitle")}</h3>
      <p>
        {documentKind === "textbook"
          ? t("artifacts.brief.emptyTextbook")
          : t("artifacts.brief.emptyPaper")}
        {t("artifacts.brief.emptyAux")}
        {t("artifacts.brief.emptyCost")}
      </p>
      <button
        type="button"
        onClick={onGenerateBrief}
        disabled={orientationBusy}
      >
        {orientationBusy ? t("artifacts.brief.queued") : t("artifacts.brief.generate")}
      </button>
    </div>
  );
}

export default function ArtifactPanel({
  artifacts,
  activeArtifactId,
  activeBlockId,
  activeBlockLabel,
  displayCropSrc,
  lensQa,
  lensQaBusy,
  blocks = [],
  pdfDocument,
  rotation = 0,
  onGenerateBlockAction,
  onSelect,
  onJump,
  onAskLens,
  onTransferLens,
  onSetOverride,
  onGenerateBrief,
  onGenerateDocumentArtifact,
  busyDocumentArtifacts = [],
  onRegenerateLens,
  onDeleteArtifactVersion,
  modelLabel,
  onOpenOutline,
  preferBrief = false,
  orientationBusy = false,
  documentKind,
  discussionFontSize,
  onDiscussionFontSizeChange,
  scopeTab: scopeTabProp,
  onScopeTabChange: onScopeTabChangeProp,
  hideScopeTabs = false,
  showAnnotations = false,
  annotationCount = 0,
  annotationsSlot,
  onShowAnnotations,
  onUpdateTags,
  onUpdateMetadata,
  onUpdateOrientationTable,
}: ArtifactPanelProps) {
  const { t, dateLocale } = useLocale();
  const [useBriefForGlossary, setUseBriefForGlossary] = useState(false);
  const [question, setQuestion] = useState("");
  const [confirmingArtifact, setConfirmingArtifact] =
    useState<ArtifactProjection | null>(null);
  const [deletingArtifact, setDeletingArtifact] =
    useState<ArtifactProjection | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const generatingAction: BlockAction =
    activeBlockLabel === "translate" || activeBlockLabel === "explain"
      ? activeBlockLabel
      : "lens";
  const generatingLabel = t(`artifacts.actions.${generatingAction}`);
  const generatingIcon =
    generatingAction === "explain"
      ? "💡"
      : generatingAction === "translate"
        ? "Aa"
        : "🔍";
  const generatingDetailTitle = t(
    `artifacts.generating.detail.${generatingAction}`,
  );

  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    const nextHeight = Math.min(el.scrollHeight, 140);
    el.style.height = `${nextHeight}px`;
    el.style.overflowY = el.scrollHeight > 140 ? "auto" : "hidden";
  }, [question]);

  const validArtifacts = useMemo(
    () =>
      artifacts.filter(
        (artifact) =>
          artifact.kind !== "reading_roadmap" && artifact.kind !== "roadmap",
      ),
    [artifacts],
  );

  const brief = useMemo(
    () => validArtifacts.find((artifact) => artifact.kind === "brief"),
    [validArtifacts],
  );
  const auxiliaryActions = onGenerateDocumentArtifact ? (
    <div className="document-artifact-actions" aria-label={t("artifacts.doc.actionsAria")}>
      {(["glossary", "symbol_table", "metadata"] as const).map((kind) => {
        const exists = validArtifacts.some(
          (artifact) => artifact.kind === kind,
        );
        const busy = busyDocumentArtifacts.includes(kind);
        return (
          <button
            key={kind}
            type="button"
            className="artifact-micro-chip"
            disabled={busy}
            onClick={() =>
              onGenerateDocumentArtifact(
                kind,
                kind === "glossary" && Boolean(brief) && useBriefForGlossary,
              )
            }
          >
            {busy
              ? t("artifacts.doc.generating")
              : exists ? t("artifacts.doc.regenerateKind", { kind: artifactKindLabel(kind, t) }) : t("artifacts.doc.generateKind", { kind: artifactKindLabel(kind, t) })}
          </button>
        );
      })}
      <label>
        <input
          type="checkbox"
          checked={Boolean(brief) && useBriefForGlossary}
          disabled={!brief}
          onChange={(event) => setUseBriefForGlossary(event.target.checked)}
        />
        {t("artifacts.doc.glossaryUsesBrief")}
      </label>
    </div>
  ) : null;
  const isGeneratingBlock = Boolean(
    activeBlockId && !validArtifacts.some((a) => a.objectKey === activeBlockId),
  );

  const [userScopeTab, setUserScopeTab] = useState<"global" | "block" | null>(
    null,
  );
  const [lastGlobalId, setLastGlobalId] = useState<string>("");
  const [lastBlockId, setLastBlockId] = useState<string>("");

  const groups = useMemo(
    () => groupArtifacts(validArtifacts),
    [validArtifacts],
  );

  const globalGroups = useMemo(
    () =>
      groups.filter(
        (g) =>
          !g.kind.startsWith("lens_") &&
          g.kind !== "translation" &&
          g.kind !== "explanation",
      ),
    [groups],
  );

  const blockGroups = useMemo(
    () =>
      groups.filter(
        (g) =>
          g.kind.startsWith("lens_") ||
          g.kind === "translation" ||
          g.kind === "explanation",
      ),
    [groups],
  );

  const blockBundles = useMemo(
    () =>
      bundleArtifactsByBlock(
        validArtifacts,
        blocks,
        activeBlockId,
        activeBlockLabel,
        displayCropSrc,
      ),
    [validArtifacts, blocks, activeBlockId, activeBlockLabel, displayCropSrc],
  );

  useEffect(() => {
    if (!activeArtifactId) return;
    const isBlock = blockGroups.some((g) =>
      g.versions.some((v) => v.id === activeArtifactId),
    );
    if (isBlock) {
      setLastBlockId(activeArtifactId);
    } else {
      const isGlobal = globalGroups.some((g) =>
        g.versions.some((v) => v.id === activeArtifactId),
      );
      if (isGlobal) {
        setLastGlobalId(activeArtifactId);
      }
    }
  }, [activeArtifactId, blockGroups, globalGroups]);

  const scopeTab = useMemo(() => {
    if (scopeTabProp) return scopeTabProp;
    if (userScopeTab) return userScopeTab;
    if (preferBrief) return "global";
    if (isGeneratingBlock) return "block";
    if (activeArtifactId) {
      const isBlock = blockGroups.some((g) =>
        g.versions.some((v) => v.id === activeArtifactId),
      );
      return isBlock ? "block" : "global";
    }
    return "global";
  }, [
    scopeTabProp,
    userScopeTab,
    preferBrief,
    isGeneratingBlock,
    activeArtifactId,
    blockGroups,
  ]);

  const activeGroup = useMemo(() => {
    if (preferBrief) return groups.find((g) => g.kind === "brief") ?? null;
    const currentScope = scopeTab;
    const scopeGroups = currentScope === "block" ? blockGroups : globalGroups;
    if (scopeGroups.length === 0) return null;

    // Check if activeArtifactId is within current scope groups
    const inScopeGroup = scopeGroups.find((g) =>
      g.versions.some((v) => v.id === activeArtifactId),
    );
    if (inScopeGroup) return inScopeGroup;

    // Otherwise check if last viewed ID in this scope exists
    const rememberedId = currentScope === "block" ? lastBlockId : lastGlobalId;
    if (rememberedId) {
      const rememberedGroup = scopeGroups.find((g) =>
        g.versions.some((v) => v.id === rememberedId),
      );
      if (rememberedGroup) return rememberedGroup;
    }

    // Default to brief (if global) or first group in current scope
    if (currentScope === "global") {
      return (
        scopeGroups.find((g) => g.kind === "brief") ?? scopeGroups[0] ?? null
      );
    }
    return scopeGroups[0] ?? null;
  }, [
    activeArtifactId,
    groups,
    preferBrief,
    scopeTab,
    blockGroups,
    globalGroups,
    lastBlockId,
    lastGlobalId,
  ]);

  const handleScopeTabChange = (nextTab: "global" | "block") => {
    setUserScopeTab(nextTab);
    onScopeTabChangeProp?.(nextTab);
    if (nextTab === "global" && globalGroups.length > 0) {
      const target =
        globalGroups.find((g) =>
          g.versions.some((v) => v.id === lastGlobalId),
        ) ??
        globalGroups.find((g) => g.kind === "brief") ??
        globalGroups[0];
      if (target) onSelect(target.latestArtifact);
    } else if (nextTab === "block" && blockGroups.length > 0) {
      const target =
        blockGroups.find((g) => g.versions.some((v) => v.id === lastBlockId)) ??
        blockGroups[0];
      if (target) onSelect(target.latestArtifact);
    }
  };

  const active = useMemo(() => {
    if (!activeGroup) return null;
    return (
      activeGroup.versions.find((v) => v.id === activeArtifactId) ??
      activeGroup.latestArtifact ??
      null
    );
  }, [activeArtifactId, activeGroup]);

  const currentVersionIndex = useMemo(() => {
    if (!activeGroup || !active) return 0;
    const index = activeGroup.versions.findIndex((v) => v.id === active.id);
    return index >= 0 ? index : 0;
  }, [active, activeGroup]);

  const totalVersions = activeGroup?.versions.length ?? 1;

  const isLens = active?.kind.startsWith("lens_") ?? false;
  const isGeneratingThisArtifact = Boolean(
    activeBlockId &&
    (active?.objectKey === activeBlockId ||
      activeGroup?.versions.some((v) => v.objectKey === activeBlockId)),
  );
  const latestAssistant =
    [...lensQa].reverse().find((message) => message.role === "assistant") ??
    null;

  const annotationChip = onShowAnnotations ? (
    <button
      type="button"
      className={`artifact-micro-chip ${showAnnotations ? "active" : ""}`}
      onClick={onShowAnnotations}
      title={t("artifacts.notes.title")}
    >
      <i>✎</i>
      <span>{t("artifacts.notes.label")}</span>
      <small>{annotationCount > 0 ? String(annotationCount) : "Notes"}</small>
    </button>
  ) : null;

  if (scopeTab === "block") {
    return (
      <div className="artifact-workspace">
        {!hideScopeTabs ? (
          <div className="artifact-compact-nav">
            <div className="artifact-scope-bar">
              <div className="artifact-scope-tabs">
                <button
                  type="button"
                  className="artifact-scope-btn"
                  onClick={() => handleScopeTabChange("global")}
                >
                  <span>{t("artifacts.scope.global")}</span>
                  <span className="scope-count">{globalGroups.length}</span>
                </button>
                <button
                  type="button"
                  className="artifact-scope-btn active"
                  onClick={() => handleScopeTabChange("block")}
                >
                  <span>{t("artifacts.scope.block")}</span>
                  <span className="scope-count">{blockBundles.length}</span>
                </button>
              </div>

              <div className="artifact-scope-actions">
                {discussionFontSize && onDiscussionFontSizeChange && (
                  <FontSizeStepper
                    value={discussionFontSize}
                    onChange={onDiscussionFontSizeChange}
                  />
                )}
                {onOpenOutline ? (
                  <button
                    type="button"
                    className="artifact-micro-chip outline-chip"
                    onClick={onOpenOutline}
                    title={t("artifacts.outline.open")}
                    style={{ height: 26, padding: "0 8px" }}
                  >
                    <i>🗺️</i>
                    <span>{t("artifacts.outline.label")}</span>
                    <small>{t("completion.outline")}</small>
                  </button>
                ) : null}
                {!brief ? (
                  <button
                    type="button"
                    className="artifact-generate-brief-compact"
                    onClick={onGenerateBrief}
                    disabled={orientationBusy}
                    title={t("artifacts.brief.generateOnly")}
                  >
                    {orientationBusy ? t("artifacts.brief.queuedShort") : t("artifacts.brief.generatePlus")}
                  </button>
                ) : null}
              </div>
            </div>
          </div>
        ) : null}

        {showAnnotations && annotationsSlot ? (
          <div className="artifact-detail artifact-annotations-pane">
            {annotationsSlot}
          </div>
        ) : (
          <BlockCardStream
            bundles={blockBundles}
            activeArtifactId={activeArtifactId}
            activeBlockId={activeBlockId}
            displayCropSrc={displayCropSrc}
            onSelectArtifact={onSelect}
            onJump={onJump}
            onGenerateBlockAction={onGenerateBlockAction}
            onRegenerateLens={onRegenerateLens}
            onDeleteArtifactVersion={onDeleteArtifactVersion}
            onTransferLens={onTransferLens}
            onAskLens={onAskLens}
            lensQa={lensQa}
            lensQaBusy={lensQaBusy}
            pdfDocument={pdfDocument}
            rotation={rotation}
            ocrBlocks={blocks}
            modelLabel={modelLabel}
          />
        )}
      </div>
    );
  }

  if (groups.length === 0 && !isGeneratingBlock) {
    return (
      <div className="artifact-workspace">
        {scopeTab === "global" ? auxiliaryActions : null}
        <div className="artifact-compact-nav">
          <div className="artifact-chip-stream" style={{ padding: "8px 12px" }}>
            {annotationChip}
            {onOpenOutline ? (
              <button
                type="button"
                className="artifact-micro-chip outline-chip"
                onClick={onOpenOutline}
              >
                <i>🗺️</i>
                <span>{t("artifacts.outline.label")}</span>
                <small>{t("completion.outline")}</small>
              </button>
            ) : null}
          </div>
        </div>
        {showAnnotations && annotationsSlot ? (
          <div className="artifact-detail artifact-annotations-pane">
            {annotationsSlot}
          </div>
        ) : (
          <BriefGeneratePrompt
            documentKind={documentKind}
            orientationBusy={orientationBusy}
            onGenerateBrief={onGenerateBrief}
          />
        )}
      </div>
    );
  }

  return (
    <div className="artifact-workspace">
      {scopeTab === "global" ? auxiliaryActions : null}
      <div className="artifact-compact-nav">
        {!hideScopeTabs ? (
          <div className="artifact-scope-bar">
            <div className="artifact-scope-tabs">
              <button
                type="button"
                className="artifact-scope-btn active"
                onClick={() => handleScopeTabChange("global")}
              >
                <span>{t("artifacts.scope.global")}</span>
                <span className="scope-count">{globalGroups.length}</span>
              </button>
              <button
                type="button"
                className="artifact-scope-btn"
                onClick={() => handleScopeTabChange("block")}
              >
                <span>{t("artifacts.scope.block")}</span>
                <span className="scope-count">{blockBundles.length}</span>
              </button>
            </div>

            <div className="artifact-scope-actions">
              {discussionFontSize && onDiscussionFontSizeChange && (
                <FontSizeStepper
                  value={discussionFontSize}
                  onChange={onDiscussionFontSizeChange}
                />
              )}
              {!brief ? (
                <button
                  type="button"
                  className="artifact-generate-brief-compact"
                  onClick={onGenerateBrief}
                  disabled={orientationBusy}
                  title={t("artifacts.brief.generateOnly")}
                >
                  {orientationBusy ? t("artifacts.brief.queuedShort") : t("artifacts.brief.generatePlus")}
                </button>
              ) : null}
            </div>
          </div>
        ) : null}

        <div className="artifact-chip-stream-wrapper">
          <div className="artifact-chip-stream">
            {scopeTab === "global" || !hideScopeTabs ? annotationChip : null}
            {(scopeTab === "global" || !hideScopeTabs) && onOpenOutline ? (
              <button
                type="button"
                className="artifact-micro-chip outline-chip"
                onClick={onOpenOutline}
                title={t("artifacts.outline.open")}
              >
                <i>🗺️</i>
                <span>{t("artifacts.outline.label")}</span>
                <small>{t("completion.outline")}</small>
              </button>
            ) : null}
            {scopeTab === "global" ? (
              <>
                {globalGroups.map((group) => {
                  const isGroupActive =
                    group.id === activeGroup?.id && !isGeneratingBlock;
                  return (
                    <button
                      key={group.id}
                      type="button"
                      className={`artifact-micro-chip ${isGroupActive ? "active" : ""}`}
                      onClick={() =>
                        onSelect(
                          group.id === activeGroup?.id && active
                            ? active
                            : group.latestArtifact,
                        )
                      }
                      title={artifactLabel(group.latestArtifact, t)}
                    >
                      <i>{artifactIcon(group.latestArtifact.kind)}</i>
                      <span>{artifactLabel(group.latestArtifact, t)}</span>
                      {group.versions.length > 1 ? (
                        <span
                          className="version-pill-tiny"
                          title={t("artifacts.versions.countTitle", { count: group.versions.length })}
                        >
                          {t("artifacts.versions.count", { count: group.versions.length })}
                        </span>
                      ) : null}
                    </button>
                  );
                })}
              </>
            ) : (
              <>
                {isGeneratingBlock ? (
                  <button
                    className="artifact-micro-chip active is-generating"
                    type="button"
                  >
                    <i className="status-pip is-busy" />
                    <span>{t("artifacts.generating.chip", { label: generatingLabel })}</span>
                    <small>{t("completion.working")}</small>
                  </button>
                ) : null}
                {blockGroups.length === 0 && !isGeneratingBlock ? (
                  <span className="artifact-chip-empty-hint">
                    {t("artifacts.scope.emptyBlock")}
                  </span>
                ) : (
                  blockGroups.map((group) => {
                    const isThisBusy = Boolean(
                      activeBlockId &&
                      group.versions.some((v) => v.objectKey === activeBlockId),
                    );
                    const isGroupActive =
                      group.id === activeGroup?.id && !isGeneratingBlock;
                    const sub = artifactSubtitle(group.latestArtifact, t);
                    return (
                      <button
                        key={group.id}
                        type="button"
                        className={`artifact-micro-chip ${isGroupActive ? "active" : ""}`}
                        onClick={() =>
                          onSelect(
                            group.id === activeGroup?.id && active
                              ? active
                              : group.latestArtifact,
                          )
                        }
                        title={`${artifactLabel(group.latestArtifact, t)} (${sub})`}
                      >
                        <i>{artifactIcon(group.latestArtifact.kind)}</i>
                        <span>{artifactLabel(group.latestArtifact, t)}</span>
                        <small>{selectionPageLabel(group.latestArtifact)}</small>
                        {group.versions.length > 1 ? (
                          <span
                            className="version-pill-tiny"
                            title={t("artifacts.versions.countTitle", { count: group.versions.length })}
                          >
                            {t("artifacts.versions.count", { count: group.versions.length })}
                          </span>
                        ) : null}
                        {isThisBusy ? <em>⏳</em> : null}
                      </button>
                    );
                  })
                )}
              </>
            )}
          </div>

          <div className="artifact-chip-stream-actions">
            {totalVersions > 1 && activeGroup ? (
              <div
                className="version-switcher-capsule"
                role="navigation"
                aria-label={t("artifacts.versions.nav")}
              >
                <button
                  type="button"
                  className="version-nav-btn"
                  disabled={currentVersionIndex <= 0}
                  onClick={() =>
                    onSelect(activeGroup.versions[currentVersionIndex - 1])
                  }
                  title={t("artifacts.versions.prev")}
                  aria-label={t("artifacts.versions.prev")}
                >
                  ‹
                </button>
                <span className="version-indicator">
                  {t("artifacts.versions.indicator", { current: currentVersionIndex + 1, total: totalVersions })}
                </span>
                <button
                  type="button"
                  className="version-nav-btn"
                  disabled={currentVersionIndex >= totalVersions - 1}
                  onClick={() =>
                    onSelect(activeGroup.versions[currentVersionIndex + 1])
                  }
                  title={t("artifacts.versions.next")}
                  aria-label={t("artifacts.versions.next")}
                >
                  ›
                </button>
                {onDeleteArtifactVersion ? (
                  <button
                    type="button"
                    className="version-delete-btn"
                    onClick={() => setDeletingArtifact(active)}
                    title={t("artifacts.versions.delete")}
                    aria-label={t("artifacts.versions.delete")}
                  >
                    🗑️
                  </button>
                ) : null}
              </div>
            ) : null}
            {isLens && active && onTransferLens ? (
              <button
                type="button"
                className="btn-liquid-pill primary"
                onClick={() => onTransferLens(active)}
                style={{
                  height: 26,
                  padding: "0 9px",
                  fontSize: 11,
                  whiteSpace: "nowrap",
                }}
              >
                {t("artifacts.transferToDiscussion")} <span>→</span>
              </button>
            ) : null}
          </div>
        </div>
      </div>

      {showAnnotations && annotationsSlot ? (
        <div className="artifact-detail artifact-annotations-pane">
          {annotationsSlot}
        </div>
      ) : isGeneratingBlock ? (
        <article className="artifact-detail">
          {!hideScopeTabs ? (
            <header className="artifact-compact-header">
              <div className="artifact-compact-header-meta">
                <span className="artifact-kind-badge">
                  <i>{generatingIcon}</i>
                  <strong>{t("artifacts.generating.chip", { label: generatingLabel })}</strong>
                </span>
                <span className="artifact-meta-timestamp">
                  {t(`artifacts.generating.hint.${generatingAction}`)}
                </span>
              </div>
            </header>
          ) : null}
          <div
            className="artifact-detail-body"
            onCopy={(event) =>
              handleMarkdownCopyEvent(event, event.currentTarget)
            }
          >
            {displayCropSrc ? (
              <div
                className="artifact-crop-preview"
                style={{ marginBottom: 16 }}
              >
                <img
                  src={displayCropSrc}
                  alt={t("completion.selected_block")}
                  style={{
                    maxWidth: "100%",
                    borderRadius: 8,
                    border: "1.5px solid var(--glass-border)",
                    boxShadow: "0 2px 12px rgba(0,0,0,0.08)",
                  }}
                />
              </div>
            ) : null}
            <div
              className="artifact-generating-state"
              style={{
                padding: "36px 16px",
                textAlign: "center",
                color: "var(--muted)",
              }}
            >
              <div
                className="status-pip is-busy"
                style={{
                  width: 10,
                  height: 10,
                  margin: "0 auto 12px",
                  display: "block",
                }}
              />
              <p>{t("artifacts.generating.progress", { title: generatingDetailTitle })}</p>
            </div>
          </div>
        </article>
      ) : preferBrief && !brief ? (
        <BriefGeneratePrompt
            documentKind={documentKind}
          orientationBusy={orientationBusy}
          onGenerateBrief={onGenerateBrief}
        />
      ) : active ? (
        <article className="artifact-detail">
          {!hideScopeTabs ? (
            <header className="artifact-compact-header">
              <div className="artifact-compact-header-meta">
                <span className="artifact-kind-badge">
                  <i>{artifactIcon(active.kind)}</i>
                  <strong>
                    {text(record(active.content).title) ||
                      artifactLabel(active, t)}
                  </strong>
                </span>
              </div>
              <div className="artifact-compact-header-actions">
                {active.kind === "brief" ? (
                  <button
                    type="button"
                    className="artifact-regenerate-brief btn-liquid-pill"
                    onClick={onGenerateBrief}
                    disabled={orientationBusy}
                    style={{ height: 26, padding: "0 10px", fontSize: 11 }}
                  >
                    {orientationBusy ? t("artifacts.brief.queuedShort") : t("artifacts.regenerate")}
                  </button>
                ) : null}
                {isLens && onRegenerateLens ? (
                  <button
                    type="button"
                    className={`artifact-regenerate-lens btn-liquid-pill ${isGeneratingThisArtifact ? "is-busy" : ""}`}
                    onClick={() => setConfirmingArtifact(active)}
                    disabled={isGeneratingThisArtifact}
                    title={
                      isGeneratingThisArtifact
                        ? t("artifacts.generating.inProgress")
                        : t("artifacts.lens.regenerateTitle")
                    }
                  >
                    {isGeneratingThisArtifact
                      ? t("artifacts.generating.inProgressBusy")
                      : t("artifacts.regenerate")}
                  </button>
                ) : null}
              </div>
            </header>
          ) : null}

          <div
            className="artifact-detail-body"
            onCopy={(event) =>
              handleMarkdownCopyEvent(event, event.currentTarget)
            }
          >
            {isGeneratingThisArtifact ? (
              <div
                className="artifact-busy-banner"
                role="status"
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 10,
                  padding: "10px 14px",
                  borderRadius: 12,
                  background: "rgba(59, 130, 246, 0.1)",
                  border: "1px solid rgba(59, 130, 246, 0.25)",
                  color: "var(--ink)",
                  fontSize: 12.5,
                  marginBottom: 16,
                  backdropFilter: "var(--glass-blur)",
                }}
              >
                <span style={{ fontSize: 16 }}>⏳</span>
                <div
                  style={{ display: "flex", flexDirection: "column", gap: 2 }}
                >
                  <strong>{t("artifacts.lens.regenerating")}</strong>
                  <span style={{ fontSize: 11, color: "var(--muted)" }}>
                    {t("artifacts.lens.queuedHint")}
                  </span>
                </div>
              </div>
            ) : null}
            {["brief", "glossary", "symbol_table", "metadata"].includes(
              active.kind,
            ) ? (
              <OrientationDetail
                onJump={onJump}
                artifact={active}
                documentKind={documentKind}
                onSetOverride={onSetOverride}
                onUpdateTags={onUpdateTags}
                onUpdateMetadata={onUpdateMetadata}
                onUpdateOrientationTable={onUpdateOrientationTable}
              />
            ) : (
              <GeneratedDetail
                artifact={active}
                displayCropSrc={displayCropSrc}
              />
            )}

            {active.evidence.length > 0 ? (
              <section className="artifact-evidence">
                <span>{t("completion.evidence")}</span>
                <div className="artifact-evidence-pills">
                  {active.evidence.map((evidence, index) => (
                    <button
                      key={
                        evidence.pageNumber + "-" + (evidence.blockId ?? index)
                      }
                      type="button"
                      className="btn-evidence-pill"
                      title={
                        evidence.excerpt
                          ? t("artifacts.evidence.pageExcerpt", { page: evidence.pageNumber, excerpt: evidence.excerpt })
                          : t("artifacts.evidence.jumpPage", { page: evidence.pageNumber })
                      }
                      onClick={() =>
                        onJump(evidence.pageNumber, evidence.blockId)
                      }
                    >
                      <span>📄 p.{evidence.pageNumber}</span>
                      <i>↗</i>
                    </button>
                  ))}
                </div>
              </section>
            ) : null}

            {isLens ? (
              <section className="lens-qa">
                <div className="lens-qa-head">
                  <span>{t("completion.lens_questions")}</span>
                  <small>{t("completion.isolated_from_discussion")}</small>
                </div>
                {lensQa.map((message) => (
                  <article className={message.role} key={message.id}>
                    <span>{message.role === "user" ? "YOU" : "LENS"}</span>
                    <Markdown>{message.content}</Markdown>
                  </article>
                ))}

                {items(record(active.content).suggestedQuestions).length > 0 ? (
                  <div className="lens-suggested-questions">
                    <span className="lens-suggested-title">{t("artifacts.lens.followups")}</span>
                    <div className="lens-suggested-list">
                      {items(record(active.content).suggestedQuestions)
                        .map(text)
                        .filter(Boolean)
                        .map((q, idx) => (
                          <button
                            key={idx}
                            type="button"
                            className="lens-suggested-chip"
                            onClick={() => {
                              setQuestion(q);
                              setTimeout(() => {
                                textareaRef.current?.focus();
                              }, 0);
                            }}
                            title={t("artifacts.lens.fillQuestion")}
                          >
                            <Markdown>{wrapBareInlineMath(q)}</Markdown>
                          </button>
                        ))}
                    </div>
                  </div>
                ) : null}

                <div className="lens-qa-composer composer-compact-capsule">
                  <textarea
                    ref={textareaRef}
                    className="composer-input-line"
                    value={question}
                    onChange={(event) => setQuestion(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter" && !event.shiftKey) {
                        event.preventDefault();
                        if (lensQaBusy || !question.trim()) return;
                        const next = question.trim();
                        setQuestion("");
                        void onAskLens(next, latestAssistant?.id ?? null);
                      }
                    }}
                    placeholder={t("artifacts.lens.questionPlaceholder")}
                    rows={1}
                  />
                  <button
                    type="button"
                    className="composer-action-btn send"
                    disabled={lensQaBusy || !question.trim()}
                    onClick={() => {
                      const next = question.trim();
                      setQuestion("");
                      void onAskLens(next, latestAssistant?.id ?? null);
                    }}
                    aria-label={t("artifacts.lens.send")}
                    title={t("artifacts.lens.sendEnter")}
                  >
                    {lensQaBusy ? "…" : "↑"}
                  </button>
                </div>
              </section>
            ) : null}

            <div className="artifact-detail-meta-footer">
              <span>
                {modelLabel ? t("artifacts.generatedBy", { model: modelLabel }) : ""}
                {new Date(active.createdAt).toLocaleDateString(dateLocale, {
                  month: "short",
                  day: "numeric",
                })}{" "}
                {new Date(active.createdAt).toLocaleTimeString(dateLocale, {
                  hour: "2-digit",
                  minute: "2-digit",
                })}
              </span>
            </div>
          </div>
        </article>
      ) : null}
      <RegenerateConfirmModal
        open={Boolean(confirmingArtifact)}
        artifactTitle={
          confirmingArtifact
            ? text(record(confirmingArtifact.content).title) ||
              artifactLabel(confirmingArtifact, t)
            : ""
        }
        kindLabel={
          confirmingArtifact
            ? artifactLabel(confirmingArtifact, t)
            : t("artifacts.lens.deepParse")
        }
        busy={isGeneratingBlock}
        onConfirm={() => {
          if (confirmingArtifact && onRegenerateLens) {
            onRegenerateLens(confirmingArtifact);
          }
          setConfirmingArtifact(null);
        }}
        onCancel={() => setConfirmingArtifact(null)}
      />
      <DeleteVersionConfirmModal
        open={Boolean(deletingArtifact)}
        versionNumber={currentVersionIndex + 1}
        totalVersions={totalVersions}
        kindLabel={
          deletingArtifact ? artifactLabel(deletingArtifact, t) : t("artifacts.lens.analysis")
        }
        onConfirm={async () => {
          if (deletingArtifact && onDeleteArtifactVersion) {
            const targetId = deletingArtifact.id;
            setDeletingArtifact(null);
            await onDeleteArtifactVersion(targetId);
          }
        }}
        onCancel={() => setDeletingArtifact(null)}
      />
    </div>
  );
}
