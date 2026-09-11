import { stageLabel } from "./i18n/runtimeCopy";
import type { AuxiliaryDocumentArtifactKind } from "./types";
import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { GitBranch, Pencil, RotateCcw, Square } from "lucide-react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { desktopClient } from "./desktopClient";
import {
  reconcileQuoteBasket,
  snapshotBlock,
  isFigureQuote,
  formatQuoteCaption,
} from "./blockQuotes";
import ConversationTree from "./ConversationTree";
import { layoutDiscussionTree, visibleDiscussionNodes } from "./discussionTree";
import MarkdownBody from "./MarkdownBody";
import { handleMarkdownCopyEvent, parsePaperCitationHref } from "./markdown";
import {
  currentPaperProviderSlot,
  formatModelLabel,
  isPaperProviderReady,
  paperProviderComposerHint,
  paperProviderComposerPlaceholder,
  paperProviderConnectionStatus,
  paperProviderEmptyBody,
  paperProviderEmptyHeading,
  paperProviderNotConfiguredStatus,
  paperProviderSetupTitle,
} from "./modelLabel";
import { useNotice } from "./NoticeCenter";
import { useDialogFocusTrap } from "./useDialogFocusTrap";
import { useGuidePlan } from "./guide/useGuidePlan";
import {
  READER_ZOOM_MIN,
  READER_ZOOM_MAX,
  clampReaderZoom,
  fitReaderZoom,
  normalizeReaderRotation,
} from "./readerZoom";
import { GUIDE_GUTTER_GAP, GUIDE_GUTTER_WIDTH } from "./guide/marginLayout";
import { buildSendChatInvokeArgs } from "./sendChat";
import { createTrailingThrottle } from "./throttle";
import PdfReader, {
  createLensCrops,
  type BlockAction,
  type BlockActionMaterial,
  type PDFDocumentProxy,
} from "./PdfReader";
import OperationsDrawer from "./OperationsDrawer";
import SettingsWorkbench, { type SettingsSection } from "./SettingsWorkbench";
import FontSizeStepper from "./components/FontSizeStepper";
import LibraryHub from "./components/LibraryHub";
import ReaderContextDialog, {
  type ReaderContextTarget,
} from "./components/ReaderContextDialog";
import HubBatchConfirmDialog from "./components/HubBatchConfirmDialog";
import { libraryWorkspaceClient } from "./library/libraryWorkspaceClient";
import {
  batchOf,
  changeRequest,
  controlBatchRequest,
  planAndStart,
  recordReaderActivityRequest,
  startBatchRequest,
  summarizeBatch,
  toLibraryActError,
  undoTokenOf,
} from "./library/libraryActTypes";
import { consumeUndoToken, rememberUndoToken } from "./library/undoTokenStore";
import { LIBRARY_PROTOCOL_VERSION } from "./library/libraryWorkspaceTypes";
import {
  defaultReadingLifecycle,
  readingProgress,
  resolveResumeTarget,
  shouldPromptCompletion,
  type ReadingLifecycle,
} from "./readerReadingState";
import type { BatchProjection } from "./library/libraryActTypes";
import ThreadTabs from "./components/ThreadTabs";
import { ChatTimelineNavigator } from "./components/ChatTimelineNavigator";
import ReaderAnnotationsPanel, {
  type ReaderAnnotation,
} from "./components/ReaderAnnotationsPanel";
import ReaderAnnotationComposer from "./components/ReaderAnnotationComposer";
import {
  canGoBackReader,
  clearReaderHistory,
  popReaderLocation,
  pushReaderLocation,
  type ReaderHistory,
  type ReaderLocation,
} from "./readerNavigation";
import { reconcileAnnotations } from "./annotationLocator";
import {
  applyFontPreferences,
  getSavedFontPreferences,
} from "./fontFamily";
import {
  DISCUSSION_FONT_STORAGE_KEY,
  storedDiscussionFontSize,
  type DiscussionFontSize,
} from "./discussionFont";
import {
  isPlaceholderThreadTitle,
  titleFromFirstQuestion,
} from "./threadTitle";
import type {
  AppStats,
  ArtifactProjection,
  BlockQuoteSnapshot,
  Brief,
  CloseThreadResult,
  DeleteDiscussionTurnResult,
  DiagnosticPreview,
  DocumentCard,
  JobProjection,
  LensQaProjection,
  LensQaTurn,
  OcrBlockProjection,
  OcrProjection,
  OutlineHeadProjection,
  OutlineNode,
  GuidePlan,
  GuideProjection,
  GuideCharacterSettings,
  OutlinePlan,
  OutlineProjection,
  GeminiModelOption,
  Message,
  ModelSettings,
  OutlineView,
  ReadingState,
  RemoteTombstoneProjection,
  StorageReport,
  ThemeMode,
  ReaderContextProjection,
  Thread,
  TrashProjection,
  UsageReceipt,
  WorkspaceInfo,
  UiLocaleProjection,
  WorkspaceLayout,
  ReadingRoadmapProjection,
  RoadmapProgressEntry,
  CollectionProjection,
} from "./types";
import { ReadingRoadmapPanel } from "./ReadingRoadmap";
import { TrashConfirmModal } from "./components/TrashConfirmModal";
import { LanguageGate } from "./components/LanguageGate";
import { LocaleProvider, useLocale } from "./i18n/LocaleContext";
import { LanguageChangeConfirmModal } from "./components/LanguageChangeConfirmModal";
import { t as translate } from "./i18n/t";
import type { UiLocale } from "./i18n/types";
import { CascadeOcrDeleteConfirmModal } from "./components/CascadeOcrDeleteConfirmModal";
import OutlinePane from "./outline/OutlinePane";
import GuideIsland from "./guide/GuideIsland";
import {
  firstGuideAnchor,
  guideNotes,
  locateGuideInks,
  normalizeGuideInks,
} from "./guide/inks";
import {
  clampOutlineInspectorWidth,
  OUTLINE_INSPECTOR_DEFAULT,
} from "./outline/outlineInspectorWidth";
import { planOutlineNodeSelect } from "./outline/outlineSelect";
import {
  buildPlanOutlineInvokeArgs,
  buildDeleteOutlineDeepDiveInvokeArgs,
  buildDeleteOutlineInvokeArgs,
  buildStartOutlineInvokeArgs,
} from "./outline/outlinePlan";
import {
  applyWorkspaceLayoutIntent,
  consumeEscapeForOutline,
  defaultWorkspaceLayout,
  layoutShowsDiscussionRail,
  layoutShowsOutline,
  layoutShowsPdf,
  parseOutlineView,
  parseWorkspaceLayout,
} from "./outline/workspaceLayout";
import {
  isActiveOutlineJob,
  outlineProgressCopy,
} from "./outline/outlineProgress";
import { bundleArtifactsByBlock } from "./ArtifactPanel";

const ArtifactPanel = lazy(() => import("./ArtifactPanel"));
const isTauri = desktopClient.runtime === "desktop";

const defaultModelSettings: ModelSettings = {
  currentProviderId: null,
  providers: [],
  credentialStore: "Windows Credential Manager",
  mistralCredentialConfigured: false,
  mistralCredentialStore: "Windows Credential Manager",
  ocrModel: "mistral-ocr-latest",
};

function formatNumber(value: number | null | undefined, unknownLabel: string) {
  if (value === null || value === undefined) return unknownLabel;
  return value.toLocaleString("en-US");
}

function formatPercent(value: number | null | undefined, unknownLabel: string) {
  if (value === null || value === undefined) return unknownLabel;
  return `${(value * 100).toFixed(1)}%`;
}

function formatCompactTokens(value?: number | null) {
  if (!value || value <= 0) return "0";
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
  if (value >= 1_000) return `${Math.round(value / 1_000)}k`;
  return String(value);
}

function formatTime(value: string, dateLocale: string, nowLabel: string) {
  try {
    return new Intl.DateTimeFormat(dateLocale, {
      hour: "2-digit",
      minute: "2-digit",
    }).format(new Date(value));
  } catch {
    return nowLabel;
  }
}

function storedChatWidth() {
  const saved = Number(localStorage.getItem("read-desktop.chatWidth"));
  return Number.isFinite(saved) && saved >= 300 && saved <= 760 ? saved : 400;
}

function QuoteFigureThumbnail({
  quote,
  pdfDocument,
  rotation = 0,
  onEnlarge,
  className = "quote-figure-thumbnail",
}: {
  quote: BlockQuoteSnapshot;
  pdfDocument: PDFDocumentProxy | null;
  rotation?: number;
  onEnlarge?: () => void;
  className?: string;
}) {
  const { t } = useLocale();
  const [cropSrc, setCropSrc] = useState<string | null>(
    quote.cropDataUrl ?? null,
  );

  useEffect(() => {
    if (quote.cropDataUrl) {
      setCropSrc(quote.cropDataUrl);
      return;
    }
    if (!pdfDocument) return;
    let cancelled = false;
    void createLensCrops(pdfDocument, quote.pageNumber, quote.bbox, rotation)
      .then((material) => {
        if (!cancelled && material.displayCropDataUrl) {
          setCropSrc(material.displayCropDataUrl);
          quote.cropDataUrl = material.displayCropDataUrl;
        }
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [pdfDocument, quote, rotation]);

  return (
    <div
      className={className}
      onClick={(e) => {
        if (onEnlarge) {
          e.stopPropagation();
          onEnlarge();
        }
      }}
      title={t("app.enlargeImage")}
      role={onEnlarge ? "button" : undefined}
      tabIndex={onEnlarge ? 0 : undefined}
      onKeyDown={(e) => {
        if (onEnlarge && (e.key === "Enter" || e.key === " ")) {
          e.preventDefault();
          onEnlarge();
        }
      }}
    >
      {cropSrc ? (
        <img
          src={cropSrc}
          alt={formatQuoteCaption(
            quote.textContent,
            `Figure p.${quote.pageNumber}`,
          )}
          className="quote-figure-img"
        />
      ) : (
        <div className="quote-figure-placeholder">
          <span className="figure-placeholder-icon">🖼️</span>
          <span className="figure-placeholder-loading">{t("app.loadingImage")}</span>
        </div>
      )}
      {onEnlarge ? (
        <span className="quote-figure-zoom-hint" title={t("app.clickToEnlarge")}>
          🔍
        </span>
      ) : null}
    </div>
  );
}

const isLongPdfWarningRequired = (error: unknown) =>
  String(error).includes("long_pdf_warning_required");

const LAST_ACTIVE_PAPER_ID_KEY = "read-desktop.lastActivePaperId";
const LAST_ACTIVE_REVISION_ID_KEY = "read-desktop.lastActiveRevisionId";
const LAST_VIEW_MODE_KEY = "read-desktop.viewMode";
const ANNOTATION_STORAGE_PREFIX = "read-desktop.annotations.";

function annotationStorageKey(paperId: string) {
  return ANNOTATION_STORAGE_PREFIX + paperId;
}

function readLocalAnnotations(paperId: string): ReaderAnnotation[] {
  try {
    const raw = localStorage.getItem(annotationStorageKey(paperId));
    if (!raw) return [];
    const parsed = JSON.parse(raw) as unknown;
    return Array.isArray(parsed) ? (parsed as ReaderAnnotation[]) : [];
  } catch {
    return [];
  }
}

function writeLocalAnnotations(
  paperId: string,
  annotations: ReaderAnnotation[],
) {
  try {
    localStorage.setItem(annotationStorageKey(paperId), JSON.stringify(annotations));
  } catch {
    // Local storage is a resilience cache, never a reason to block a save.
  }
}

function App() {
  const [workspace, setWorkspace] = useState<WorkspaceInfo | null>(null);
  const [uiLocale, setUiLocale] = useState<"zh-CN" | "en" | null | undefined>(
    undefined,
  );
  const [localeBusy, setLocaleBusy] = useState(false);
  const [localeError, setLocaleError] = useState<string | null>(null);
  const [pendingLocale, setPendingLocale] = useState<"zh-CN" | "en" | null>(
    null,
  );
  const [localeChangeBusy, setLocaleChangeBusy] = useState(false);
  const locale: UiLocale = uiLocale === "en" ? "en" : "zh-CN";
  const localeRef = useRef(locale);
  localeRef.current = locale;
  const t = useCallback((key: string, vars?: Record<string, string | number>) =>
    translate(localeRef.current, key, vars), []);
  const [documents, setDocuments] = useState<DocumentCard[]>([]);
  const [collections, setCollections] = useState<CollectionProjection[]>([]);
  const [hubSelectedFolder, setHubSelectedFolder] = useState<string>("");
  const [readerContextTarget, setReaderContextTarget] =
    useState<ReaderContextTarget | null>(null);
  const [selectedRevisionId, setSelectedRevisionId] = useState<string>(() => {
    try {
      return localStorage.getItem(LAST_ACTIVE_REVISION_ID_KEY) || "";
    } catch {
      return "";
    }
  });
  const [threads, setThreads] = useState<Thread[]>([]);
  const [archivedThreads, setArchivedThreads] = useState<Thread[]>([]);
  const [activeThreadId, setActiveThreadId] = useState("");
  const [messages, setMessages] = useState<Message[]>([]);
  const [pdfDocument, setPdfDocument] = useState<PDFDocumentProxy | null>(null);
  const [enlargedQuote, setEnlargedQuote] = useState<BlockQuoteSnapshot | null>(
    null,
  );
  const lightboxRef = useRef<HTMLDivElement>(null);
  const lightboxCloseRef = useRef<HTMLButtonElement>(null);
  useDialogFocusTrap({
    open: Boolean(enlargedQuote),
    containerRef: lightboxRef,
    initialFocusRef: lightboxCloseRef,
    onClose: () => setEnlargedQuote(null),
  });
  const [brief, setBrief] = useState<Brief | null>(null);
  const [ocr, setOcr] = useState<OcrProjection | null>(null);
  const [quoteBasket, setQuoteBasket] = useState<BlockQuoteSnapshot[]>([]);
  const [focusedBlockId, setFocusedBlockId] = useState<string | null>(null);
  const [annotations, setAnnotations] = useState<ReaderAnnotation[]>([]);
  const [showArtifactAnnotations, setShowArtifactAnnotations] = useState(false);
  const [annotationComposer, setAnnotationComposer] = useState<{
    kind: "highlight" | "bookmark" | "note";
    block: OcrBlockProjection;
  } | null>(null);
  const [annotationBusy, setAnnotationBusy] = useState(false);
  const [annotationError, setAnnotationError] = useState<string | null>(null);
  const [selectedAnnotationId, setSelectedAnnotationId] = useState<string | null>(
    null,
  );
  const [annotationSavingIds, setAnnotationSavingIds] = useState<Set<string>>(
    new Set(),
  );
  const readerHistoryRef = useRef<ReaderHistory>([]);
  const [readerHistoryVersion, setReaderHistoryVersion] = useState(0);
  const activeAnnotationId = useMemo(
    () =>
      annotations.find(
        (annotation) =>
          annotation.locator.blockId === focusedBlockId &&
          annotation.status !== "deleted",
      )?.id ?? null,
    [annotations, focusedBlockId],
  );
  const [jobs, setJobs] = useState<JobProjection[]>([]);
  const [libraryBatches, setLibraryBatches] = useState<BatchProjection[]>([]);
  const [importPlan, setImportPlan] = useState<BatchProjection | null>(null);
  const [importPlanBusy, setImportPlanBusy] = useState(false);
  const [artifacts, setArtifacts] = useState<ArtifactProjection[]>([]);
  const [activeArtifactId, setActiveArtifactId] = useState("");
  const [rightTab, setRightTab] =
    useState<ReadingState["rightTab"]>("discussion");
  const [artifactScope, setArtifactScope] = useState<"global" | "block">("global");
  const [lastGlobalArtifactId, setLastGlobalArtifactId] = useState("");
  const [lastBlockArtifactId, setLastBlockArtifactId] = useState("");
  const [workspaceLayout, setWorkspaceLayout] = useState<WorkspaceLayout>(
    defaultWorkspaceLayout(),
  );

  const validArtifacts = useMemo(
    () =>
      artifacts.filter(
        (a) => a.kind !== "reading_roadmap" && a.kind !== "roadmap",
      ),
    [artifacts],
  );

  const globalArtifactCount = useMemo(() => {
    const globalKinds = new Set(
      validArtifacts
        .filter(
          (a) =>
            !a.kind.startsWith("lens_") &&
            a.kind !== "translation" &&
            a.kind !== "explanation",
        )
        .map((a) => a.kind),
    );
    return globalKinds.size;
  }, [validArtifacts]);

  const blockArtifactCount = useMemo(() => {
    return bundleArtifactsByBlock(validArtifacts, ocr?.blocks).length;
  }, [validArtifacts, ocr?.blocks]);

  const [outlineView, setOutlineView] = useState<OutlineView>("overview");
  const [outlineNodeId, setOutlineNodeId] = useState<string | null>(null);
  const [deepDiveNodeId, setDeepDiveNodeId] = useState<string | null>(null);
  const [outlineInspectorWidth, setOutlineInspectorWidth] = useState(
    OUTLINE_INSPECTOR_DEFAULT,
  );
  const [outlineProjection, setOutlineProjection] =
    useState<OutlineProjection | null>(null);
  const [outlinePlan, setOutlinePlan] = useState<OutlinePlan | null>(null);
  const [outlinePlanError, setOutlinePlanError] = useState<string | null>(null);
  const [pendingOutlinePlan, setPendingOutlinePlan] = useState<{
    plan: OutlinePlan; nodeId?: string; nodeTitle?: string;
  } | null>(null);
  const [outlineStarting, setOutlineStarting] = useState(false);
  const outlineRequestId = useRef(0);
  const outlineProjectionRequestId = useRef(0);
  const outlineStartingRef = useRef(false);
  const outlineLocalRequestId = useRef(0);
  const outlineScopeRef = useRef({ revisionId: "", parentHeadId: "", nodeId: "" });
  const [guideProjection, setGuideProjection] =
    useState<GuideProjection | null>(null);
  const [activeGuideInkId, setActiveGuideInkId] = useState<string | null>(null);
  const ocrRef = useRef(ocr);
  ocrRef.current = ocr;
  const jumpedGuideHeadId = useRef<string | null>(null);
  const [deepDiveHead, setDeepDiveHead] =
    useState<OutlineHeadProjection | null>(null);
  const [isEditingThreadTitle, setIsEditingThreadTitle] = useState(false);
  const [editingThreadTitleText, setEditingThreadTitleText] = useState("");
  const [isThreadDropdownOpen, setIsThreadDropdownOpen] = useState(false);
  const threadPickerRef = useRef<HTMLDivElement>(null);
  const [isRailMoreMenuOpen, setIsRailMoreMenuOpen] = useState(false);
  const railMorePickerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!isThreadDropdownOpen) return;
    const onPointerDown = (event: MouseEvent | TouchEvent) => {
      if (
        threadPickerRef.current &&
        !threadPickerRef.current.contains(event.target as Node)
      ) {
        setIsThreadDropdownOpen(false);
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setIsThreadDropdownOpen(false);
      }
    };
    document.addEventListener("mousedown", onPointerDown);
    document.addEventListener("touchstart", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("mousedown", onPointerDown);
      document.removeEventListener("touchstart", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [isThreadDropdownOpen]);

  useEffect(() => {
    if (!isRailMoreMenuOpen) return;
    const onPointerDown = (event: MouseEvent | TouchEvent) => {
      if (
        railMorePickerRef.current &&
        !railMorePickerRef.current.contains(event.target as Node)
      ) {
        setIsRailMoreMenuOpen(false);
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setIsRailMoreMenuOpen(false);
      }
    };
    document.addEventListener("mousedown", onPointerDown);
    document.addEventListener("touchstart", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("mousedown", onPointerDown);
      document.removeEventListener("touchstart", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [isRailMoreMenuOpen]);

  const [activeScrollMessageId, setActiveScrollMessageId] = useState<string | null>(null);
  const messageStreamRef = useRef<HTMLDivElement>(null);
  const discussionTextareaRef = useRef<HTMLTextAreaElement>(null);

  const [artifactBusyBlockId, setArtifactBusyBlockId] = useState<string | null>(
    null,
  );
  const [pendingArtifactJob, setPendingArtifactJob] = useState<{
    id: string;
    paperId: string;
    blockId: string;
    label: string;
  } | null>(null);
  const [pendingDocumentJobs, setPendingDocumentJobs] = useState<Array<{ id: string; label: string }>>([]);
  const [pendingOrientationJob, setPendingOrientationJob] = useState<{
    id: string;
    paperId: string;
    revisionId: string;
  } | null>(null);
  const [lensQa, setLensQa] = useState<LensQaProjection[]>([]);
  const [lensQaBusy, setLensQaBusy] = useState(false);
  const [activeArtifactCropSrc, setActiveArtifactCropSrc] = useState("");
  const [page, setPage] = useState(1);
  const [pageDraft, setPageDraft] = useState("1");
  const [pageError, setPageError] = useState<string | null>(null);
  const [pdfPageCount, setPdfPageCount] = useState(0);
  const [pageOffset, setPageOffset] = useState(0);
  const [restoreOffset, setRestoreOffset] = useState(0);
  const [zoom, setZoom] = useState(100);
  const [readerFitMode, setReaderFitMode] = useState<
    "width" | "page" | null
  >(null);
  const [chatWidth, setChatWidth] = useState(storedChatWidth);
  const [rotation, setRotation] = useState(0);
  const [question, setQuestion] = useState("");

  useEffect(() => { setPageDraft(String(page)); setPageError(null); }, [page]);

  useEffect(() => {
    const el = discussionTextareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    const nextHeight = Math.min(el.scrollHeight, 140);
    el.style.height = `${nextHeight}px`;
    el.style.overflowY = el.scrollHeight > 140 ? "auto" : "hidden";
  }, [question]);
  const [replyTo, setReplyTo] = useState<string | null>(null);
  const [editingMessageId, setEditingMessageId] = useState<string | null>(null);
  const [showTree, setShowTree] = useState(false);
  const [preferBrief, setPreferBrief] = useState(false);
  const [showContext, setShowContext] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  /** Increment to replay the library interaction tour (Settings → Replay). */
  const [hubTourReplayNonce, setHubTourReplayNonce] = useState(0);
  const [showOperations, setShowOperations] = useState(false);
  const [roadmapOpen, setRoadmapOpen] = useState(false);
  const [roadmapPinned, setRoadmapPinned] = useState(() => {
    try {
      return localStorage.getItem("read_desktop_roadmap_pinned") === "true";
    } catch {
      return false;
    }
  });
  const [roadmapProjection, setRoadmapProjection] =
    useState<ReadingRoadmapProjection | null>(null);
  const [roadmapProgress, setRoadmapProgress] = useState<RoadmapProgressEntry[]>([]);
  const [storageReport, setStorageReport] = useState<StorageReport | null>(
    null,
  );
  const [diagnosticPreview, setDiagnosticPreview] =
    useState<DiagnosticPreview | null>(null);
  const [trash, setTrash] = useState<TrashProjection[]>([]);
  const [paperToTrash, setPaperToTrash] = useState<DocumentCard | null>(null);
  const [folderToTrash, setFolderToTrash] = useState<{ relativePath: string; count: number } | null>(null);
  const [remoteTombstones, setRemoteTombstones] = useState<
    RemoteTombstoneProjection[]
  >([]);
  const [operationBusyId, setOperationBusyId] = useState("");
  const [showOcrDeleteModal, setShowOcrDeleteModal] = useState(false);
  const [isCompacting, setIsCompacting] = useState(false);
  const [visibleTurnLimit, setVisibleTurnLimit] = useState(15);
  const [settingsInitialSection, setSettingsInitialSection] =
    useState<SettingsSection>("models");
  const [settingsInitialProviderId, setSettingsInitialProviderId] =
    useState<string | null>(null);
  const [settingsFocusNonce, setSettingsFocusNonce] = useState(0);
  const [operationFocusJobId, setOperationFocusJobId] = useState<string | null>(null);
  const [modelSettings, setModelSettings] =
    useState<ModelSettings>(defaultModelSettings);
  const [stats, setStats] = useState<AppStats | null>(null);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("Ready · local-first workspace");
  const [libraryNotice, setLibraryNotice] = useState<string | null>(null);
  const {
    notices,
    notifyError,
    notifySuccess,
    notifyWarning,
    notifyInfo,
  } = useNotice();
  const [readingStateReadyId, setReadingStateReadyId] = useState("");
  const [readingLifecycle, setReadingLifecycle] = useState<ReadingLifecycle | null>(null);
  const [completionPromptOpen, setCompletionPromptOpen] = useState(false);
  const [completionDismissedKey, setCompletionDismissedKey] = useState("");
  const [exitChoiceOpen, setExitChoiceOpen] = useState(false);
  const [exitChoiceBusy, setExitChoiceBusy] = useState(false);
  const [viewMode, setViewMode] = useState<"library" | "reader">(() => {
    try {
      return (
        (localStorage.getItem(LAST_VIEW_MODE_KEY) as "library" | "reader") ||
        "library"
      );
    } catch {
      return "library";
    }
  });
  const [themeMode, setThemeMode] = useState<ThemeMode>(
    () =>
      (localStorage.getItem("read-desktop.theme") as ThemeMode) ||
      "liquid-light",
  );
  const [isPdfOnly, setIsPdfOnly] = useState(false);
  const [guideLayerVisible, setGuideLayerVisible] = useState(true);
  const applyReaderFit = useCallback(
    async (mode: "width" | "page") => {
      if (!pdfDocument) {
        notifyWarning(t("app.fit.unavailable"), t("app.fit.stillLoading"), {
          source: "reader",
          dedupeKey: "reader-fit-document-loading",
        });
        return;
      }
      const scroller = document.querySelector<HTMLElement>(".pdf-reader");
      if (!scroller) return;
      try {
        const pageProxy = await pdfDocument.getPage(
          Math.min(Math.max(1, page), pdfDocument.numPages),
        );
        const viewport = pageProxy.getViewport({
          scale: 1,
          rotation: normalizeReaderRotation(rotation),
        });
        const guideChrome =
          guideLayerVisible && guideProjection?.head
            ? GUIDE_GUTTER_WIDTH + GUIDE_GUTTER_GAP
            : 0;
        setZoom(
          fitReaderZoom({
            mode,
            containerWidth: scroller.clientWidth,
            containerHeight: scroller.clientHeight,
            pageWidth: viewport.width,
            pageHeight: viewport.height,
            horizontalChrome: 48 + guideChrome,
            verticalChrome: 48,
          }),
        );
      } catch (error) {
        notifyError(t("app.fit.failed"), String(error), {
          source: "reader",
          dedupeKey: `reader-fit-${mode}`,
        });
      }
    },
    [
      guideLayerVisible,
      guideProjection?.head,
      notifyError,
      notifyWarning,
      page,
      pdfDocument,
      rotation,
    ],
  );

  useEffect(() => {
    if (!readerFitMode) return;
    const scroller = document.querySelector<HTMLElement>(".pdf-reader");
    if (!scroller) return;
    void applyReaderFit(readerFitMode);
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      void applyReaderFit(readerFitMode);
    });
    observer.observe(scroller);
    return () => observer.disconnect();
  }, [applyReaderFit, readerFitMode]);

  const [discussionFontSize, setDiscussionFontSize] =
    useState<DiscussionFontSize>(() => storedDiscussionFontSize());
  const [topbarMoreOpen, setTopbarMoreOpen] = useState(false);
  const topbarMoreRef = useRef<HTMLDivElement>(null);
  const topbarMoreButtonRef = useRef<HTMLButtonElement>(null);

  const closeTopbarMore = useCallback((restoreFocus = true) => {
    setTopbarMoreOpen(false);
    if (restoreFocus) {
      window.requestAnimationFrame(() => topbarMoreButtonRef.current?.focus());
    }
  }, []);

  useEffect(() => {
    if (!topbarMoreOpen) return;
    const focusFrame = window.requestAnimationFrame(() => {
      topbarMoreRef.current
        ?.querySelector<HTMLElement>('[role="menuitem"]:not(:disabled)')
        ?.focus();
    });
    const handlePointerDown = (event: MouseEvent | TouchEvent) => {
      if (
        topbarMoreRef.current &&
        !topbarMoreRef.current.contains(event.target as Node)
      ) {
        setTopbarMoreOpen(false);
      }
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        closeTopbarMore();
        return;
      }
      if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
        return;
      }
      const menu = topbarMoreRef.current;
      if (!menu?.contains(document.activeElement)) return;
      const items = Array.from(
        menu.querySelectorAll<HTMLElement>('[role="menuitem"]:not(:disabled)'),
      );
      if (items.length === 0) return;
      event.preventDefault();
      const current = items.indexOf(document.activeElement as HTMLElement);
      const next =
        event.key === "Home"
          ? 0
          : event.key === "End"
            ? items.length - 1
            : event.key === "ArrowDown"
              ? (current + 1 + items.length) % items.length
              : (current - 1 + items.length) % items.length;
      items[next]?.focus();
    };
    document.addEventListener("mousedown", handlePointerDown);
    document.addEventListener("touchstart", handlePointerDown);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.cancelAnimationFrame(focusFrame);
      document.removeEventListener("mousedown", handlePointerDown);
      document.removeEventListener("touchstart", handlePointerDown);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [closeTopbarMore, topbarMoreOpen]);

  useEffect(() => {
    applyFontPreferences(getSavedFontPreferences());
  }, []);

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", themeMode);
    try {
      localStorage.setItem("read-desktop.theme", themeMode);
    } catch {
      // ignore
    }
    void desktopClient.command("set_window_theme", { theme: themeMode }).catch(() => {
      // ignore if non-desktop runtime
    });
  }, [themeMode]);

  useEffect(() => {
    document.documentElement.dataset.discussionFont =
      String(discussionFontSize);
    document.documentElement.style.setProperty(
      "--discussion-font-size",
      `${discussionFontSize}px`,
    );
    try {
      localStorage.setItem(
        DISCUSSION_FONT_STORAGE_KEY,
        String(discussionFontSize),
      );
    } catch {
      /* ignore */
    }
  }, [discussionFontSize]);

  // Bridge legacy status errors into structured notices (P0-04)
  const lastStatusRef = useRef(status);
  useEffect(() => {
    if (status === lastStatusRef.current) return;
    const lower = status.toLowerCase();
    const isError =
      lower.includes("failed") ||
      lower.includes("error") ||
      lower.includes("unable") ||
      status.includes(t("app.statusHint.failed")) ||
      status.includes(t("app.statusHint.error")) ||
      status.includes(t("app.statusHint.unable")) ||
      status.includes(t("app.statusHint.cannot"));
    const isWarning =
      lower.includes("unavailable") ||
      lower.includes("requires") ||
      lower.includes("choose a workspace") ||
      lower.includes("not configured") ||
      lower.includes("no hosted pdf") ||
      status.includes(t("app.statusHint.unavailable")) ||
      status.includes(t("app.statusHint.requires")) ||
      status.includes(t("app.statusHint.maxSupport"));
    if (!isError && !isWarning) {
      lastStatusRef.current = status;
      return;
    }

    // Defer one frame so a flow that already emitted a structured notice wins.
    // This keeps the compatibility bridge without producing duplicate cards.
    const frame = window.requestAnimationFrame(() => {
      lastStatusRef.current = status;
      const alreadyReported = notices.some(
        (notice) =>
          notice.message === status ||
          status.includes(notice.message) ||
          notice.message.includes(status),
      );
      if (alreadyReported) return;
      const notify = isError ? notifyError : notifyWarning;
      notify(isError ? t("app.operationFailed") : t("app.operationNeedsAttention"), status, {
          source: "system",
          dedupeKey: status,
      });
    });
    return () => window.cancelAnimationFrame(frame);
  }, [notices, notifyError, notifyWarning, status]);

  useEffect(() => {
    setVisibleTurnLimit(15);
  }, [activeThreadId]);

  useEffect(() => {
    let unlistenStart: (() => void) | undefined;
    let unlistenFinish: (() => void) | undefined;
    if (desktopClient.runtime === "desktop") {
      void import("@tauri-apps/api/event").then(({ listen }) => {
        void listen("discussion_compaction_started", () => {
          setIsCompacting(true);
        }).then((un) => {
          unlistenStart = un;
        });
        void listen("discussion_compaction_finished", () => {
          setIsCompacting(false);
        }).then((un) => {
          unlistenFinish = un;
        });
      });
    }
    return () => {
      unlistenStart?.();
      unlistenFinish?.();
    };
  }, []);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        // Overlay stack: topmost consumes Escape
        if (exitChoiceOpen) {
          return;
        }
        if (showSettings) {
          // let SettingsWorkbench handle its own Escape; don't swallow
          return;
        }
        if (showOperations) {
          e.preventDefault();
          setShowOperations(false);
          return;
        }
        if (enlargedQuote) {
          e.preventDefault();
          setEnlargedQuote(null);
          return;
        }
        if (showTree || showContext) {
          // let those close themselves
          return;
        }
        if (focusedBlockId) {
          e.preventDefault();
          setFocusedBlockId(null);
          return;
        }
        if (consumeEscapeForOutline(outlineNodeId) === "clear_node") {
          e.preventDefault();
          setOutlineNodeId(null);
          return;
        }
        if (viewMode === "reader") {
          setViewMode("library");
        }
      }
      if (
        (e.ctrlKey || e.metaKey) &&
        (e.key === "\\" || e.key === "b" || e.key === "B")
      ) {
        if (viewMode === "reader") {
          e.preventDefault();
          setIsPdfOnly((prev) => !prev);
        }
      }
      // P0-06 reader hotkeys when focused not in input/dialog
      const target = e.target as HTMLElement | null;
      const isInput = target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable || target.closest('[role="dialog"]'));
      if (
        viewMode === "reader" &&
        !showSettings &&
        !showOperations &&
        !showTree &&
        !showContext &&
        !exitChoiceOpen &&
        !enlargedQuote &&
        !topbarMoreOpen &&
        !isInput
      ) {
        if (e.key === "PageUp") {
          e.preventDefault();
          setPage((p) => Math.max(1, p - 1));
        } else if (e.key === "PageDown") {
          e.preventDefault();
          setPage((p) => Math.min(pdfPageCount || p + 1, p + 1));
        } else if (e.key === "Home") {
          e.preventDefault();
          setPage(1);
        } else if (e.key === "End") {
          e.preventDefault();
          if (pdfPageCount) setPage(pdfPageCount);
        } else if ((e.ctrlKey || e.metaKey) && (e.key === "+" || e.key === "=")) {
          e.preventDefault();
          setReaderFitMode(null);
          setZoom((z) => clampReaderZoom(z + 10));
        } else if ((e.ctrlKey || e.metaKey) && (e.key === "-" || e.key === "_")) {
          e.preventDefault();
          setReaderFitMode(null);
          setZoom((z) => clampReaderZoom(z - 10));
        }
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [
    viewMode,
    showSettings,
    showOperations,
    showTree,
    showContext,
    exitChoiceOpen,
    focusedBlockId,
    outlineNodeId,
    pdfPageCount,
    enlargedQuote,
    topbarMoreOpen,
  ]);

  const currentProvider = useMemo(() => currentPaperProviderSlot(modelSettings), [modelSettings]);
  const paperProviderReady = isPaperProviderReady(modelSettings);
  const needsPaperProvider = isTauri && !paperProviderReady;
  const selectedDoc = useMemo(
    () =>
      documents.find((doc) => doc.revisionId === selectedRevisionId) ??
      documents[0],
    [documents, selectedRevisionId],
  );
  const guidePanel = useGuidePlan(selectedDoc?.id ?? null, selectedDoc?.revisionId ?? null, setGuideProjection, setStatus, t);
  const { plan: guidePlan, confirming: guideConfirming, error: guideError,
    settings: guideCharacterSettings, castIds: guideCastIds } = guidePanel;
  const setGuideError = guidePanel.setError;
  const setGuideConfirming = (value: boolean) => { if (!value) guidePanel.cancel(); };
  const settingsWereOpen = useRef(false);
  useEffect(() => {
    if (settingsWereOpen.current && !showSettings) void guidePanel.refreshCharacters();
    settingsWereOpen.current = showSettings;
  }, [showSettings]);

  const activeThread = useMemo(
    () => threads.find((thread) => thread.id === activeThreadId) ?? threads[0],
    [threads, activeThreadId],
  );
  const activeStreamingMessage = useMemo(
    () =>
      messages.find(
        (message) =>
          message.threadId === activeThread?.id &&
          message.role === "assistant" &&
          message.status === "streaming",
      ),
    [activeThread?.id, messages],
  );
  const activeArtifact = useMemo(
    () =>
      validArtifacts.find((artifact) => artifact.id === activeArtifactId) ??
      validArtifacts[0] ??
      null,
    [activeArtifactId, validArtifacts],
  );
  const readingStateSaver = useMemo(
    () =>
      createTrailingThrottle<ReadingState>(500, (readingState) => {
        if (!isTauri) return;
        void desktopClient
          .command("save_reading_state", { readingState })
          .catch(() => undefined);
      }),
    [selectedDoc?.id],
  );
  const persistReaderActivity = useCallback(
    async (paperId: string, revisionId: string, pageNumber: number, pageCount: number) => {
      try {
        if (desktopClient.runtime === "desktop") {
          const result = await libraryWorkspaceClient.act(
            recordReaderActivityRequest({ paperId, revisionId, pageNumber, pageCount }),
          );
          if (result.kind === "record_reader_activity") {
            setReadingLifecycle(result.context.lifecycle);
          }
        } else {
          const result = await desktopClient.command<{
            lifecycle: ReadingLifecycle;
          }>("record_reading_activity", {
            request: { paperId, revisionId, pageNumber, pageCount },
          });
          setReadingLifecycle(result.lifecycle);
        }
      } catch {
        /* activity is best-effort; viewport restore still works */
      }
    },
    [],
  );
  const activitySaver = useMemo(
    () =>
      createTrailingThrottle<{
        paperId: string;
        revisionId: string;
        pageNumber: number;
        pageCount: number;
      }>(2000, (payload) => {
        void persistReaderActivity(
          payload.paperId,
          payload.revisionId,
          payload.pageNumber,
          payload.pageCount,
        );
      }),
    [persistReaderActivity],
  );
  const persistLifecyclePatch = useCallback(
    async (paperId: string, expectedVersion: number, patch: { status: "read" }) => {
      if (desktopClient.runtime === "desktop") {
        const result = await libraryWorkspaceClient.act(
          changeRequest({
            kind: "patch_lifecycle",
            paperId,
            expectedVersion,
            patch,
          }),
        );
        if (result.kind === "change" && result.result.kind === "lifecycle") {
          setReadingLifecycle(result.result.lifecycle);
        }
        return;
      }
      const next = await desktopClient.command<ReadingLifecycle>("update_reading_lifecycle", {
        request: { paperId, expectedVersion, patch },
      });
      setReadingLifecycle(next);
    },
    [],
  );
  const activeOcrJob = useMemo(
    () =>
      jobs.find(
        (job) =>
          job.kind === "ocr" &&
          job.revisionId === selectedDoc?.revisionId &&
          ["queued", "running", "paused"].includes(job.state),
      ),
    [jobs, selectedDoc?.revisionId],
  );
  const activeOutlineJob = useMemo(
    () =>
      jobs.find(
        (job) =>
          isActiveOutlineJob(job) &&
          job.revisionId === selectedDoc?.revisionId,
      ),
    [jobs, selectedDoc?.revisionId],
  );
  const activeOrientationJob = useMemo(
    () =>
      jobs.find(
        (job) =>
          job.kind === "orientation_pack" &&
          job.revisionId === selectedDoc?.revisionId &&
          ["queued", "running", "paused"].includes(job.state),
      ),
    [jobs, selectedDoc?.revisionId],
  );
  const busyDocumentArtifacts = jobs.filter((job) => job.kind === "document_artifact"
    && job.revisionId === selectedDoc?.revisionId && ["queued", "running", "paused"].includes(job.state))
    .map((job) => String((job.payload as Record<string, unknown> | null)?.documentArtifactKind));
  const activeGuideJob = useMemo(
    () =>
      jobs.find(
        (job) =>
          job.kind === "reading_guide" &&
          job.revisionId === selectedDoc?.revisionId &&
          ["queued", "running", "paused"].includes(job.state),
      ),
    [jobs, selectedDoc?.revisionId],
  );
  const activeRoadmapJob = useMemo(
    () =>
      jobs.find(
        (job) =>
          job.kind === "reading_roadmap" &&
          job.revisionId === selectedDoc?.revisionId &&
          ["queued", "running", "paused"].includes(job.state),
      ),
    [jobs, selectedDoc?.revisionId],
  );
  const discussionNodes = useMemo(
    () => layoutDiscussionTree(messages, activeThread?.activeMessageId ?? null),
    [messages, activeThread?.activeMessageId],
  );
  const visibleDiscussion = useMemo(
    () =>
      visibleDiscussionNodes(
        discussionNodes,
        activeThread?.activeMessageId ?? null,
      ),
    [discussionNodes, activeThread?.activeMessageId],
  );
  const pageCount = pdfPageCount || selectedDoc?.pages || 0;

  const exitWorkSummary = useMemo(
    () => ({
      queued: jobs.filter((job) => job.state === "queued").length,
      running: jobs.filter((job) => job.state === "running").length,
      paused: jobs.filter((job) => job.state === "paused").length,
      streamingDiscussions: messages.filter(
        (message) =>
          message.role === "assistant" && message.status === "streaming",
      ).length,
    }),
    [jobs, messages],
  );

  const pdfReaderSrc = useMemo(() => {
    if (!selectedDoc?.pdfPath) return "";
    try {
      return isTauri
        ? convertFileSrc(selectedDoc.pdfPath)
        : selectedDoc.pdfPath;
    } catch {
      return selectedDoc.pdfPath;
    }
  }, [selectedDoc]);

  const refreshStats = useCallback(async () => {
    try {
      setStats(await desktopClient.open<AppStats>("get_stats"));
    } catch {
      /* workspace may not exist yet */
    }
  }, []);

  const refreshJobs = useCallback(async () => {
    try {
      setJobs(await desktopClient.open<JobProjection[]>("list_jobs"));
    } catch {
      /* workspace may not exist yet */
    }
  }, []);
  const refreshBrief = useCallback(async (revisionId?: string) => {
    if (!revisionId) {
      setBrief(null);
      return;
    }
    try {
      setBrief(
        await desktopClient.open<Brief | null>("get_brief", { revisionId }),
      );
    } catch {
      setBrief(null);
    }
  }, []);
  const refreshMessages = useCallback(async (threadId: string) => {
    if (!threadId) return;
    try {
      setMessages(
        await desktopClient.open<Message[]>("list_messages", { threadId }),
      );
    } catch {
      /* Discussion may have changed while switching papers. */
    }
  }, []);
  const refreshOperations = useCallback(async () => {
    try {
      const [nextJobs, nextStorage, nextTrash, nextTombstones, recent] =
        await Promise.all([
          desktopClient.open<JobProjection[]>("list_jobs"),
          desktopClient.open<StorageReport>("get_storage_report"),
          desktopClient.open<TrashProjection[]>("list_trash"),
          desktopClient.open<RemoteTombstoneProjection[]>(
            "list_remote_tombstones",
          ),
          libraryWorkspaceClient
            .read({ kind: "recent_batches", protocolVersion: 1, limit: 50 })
            .catch(() => null),
        ]);
      setJobs(nextJobs);
      setStorageReport(nextStorage);
      setTrash(nextTrash);
      setRemoteTombstones(nextTombstones);
      if (recent && recent.kind === "recent_batches") setLibraryBatches([...recent.page.batches]);
    } catch (error) {
      setStatus(t("feedback.message0", { v0: String(error) }));
    }
  }, []);

  const refreshOcr = useCallback(async (revisionId?: string) => {
    if (!revisionId) {
      setOcr(null);
      setQuoteBasket([]);
      setFocusedBlockId(null);
      return;
    }
    try {
      const next = await desktopClient.open<OcrProjection | null>(
        "latest_ocr",
        {
          revisionId,
        },
      );
      setOcr(next);
      setQuoteBasket((current) =>
        reconcileQuoteBasket(revisionId, next, current),
      );
      setFocusedBlockId(null);
    } catch {
      setOcr(null);
      setQuoteBasket([]);
      setFocusedBlockId(null);
    }
  }, []);

  const refreshAnnotations = useCallback(async (paperId?: string) => {
    if (!paperId) {
      setAnnotations([]);
      return;
    }
    try {
      const next = await desktopClient.open<ReaderAnnotation[]>(
        "list_annotations" as never,
        { paperId },
      );
      setAnnotations(Array.isArray(next) ? next : []);
    } catch {
      // The browser preview may not have an annotation projection yet.
      setAnnotations([]);
    }
  }, []);
  const refreshArtifacts = useCallback(
    async (paperId?: string, preferredObjectKey?: string) => {
      if (!paperId) {
        setArtifacts([]);
        setActiveArtifactId("");
        return;
      }
      try {
        const next = await desktopClient.open<ArtifactProjection[]>(
          "list_artifacts",
          { paperId },
        );
        setArtifacts(next);
        setActiveArtifactId((current) => {
          const preferred = preferredObjectKey
            ? next.find((artifact) => artifact.objectKey === preferredObjectKey)
            : undefined;
          if (preferred) return preferred.id;
          return next.some((artifact) => artifact.id === current)
            ? current
            : (next.find((artifact) => artifact.kind === "brief")?.id ??
                next[0]?.id ??
                "");
        });
      } catch {
        setArtifacts([]);
        setActiveArtifactId("");
      }
    },
    [],
  );

  const refreshRoadmap = useCallback(
    async (revisionId?: string, paperId?: string) => {
      if (!revisionId) {
        setRoadmapProjection(null);
        setRoadmapProgress([]);
        return;
      }
      try {
        const rm = await desktopClient.open<ReadingRoadmapProjection | null>(
          "get_roadmap",
          { revisionId },
        );
        setRoadmapProjection(rm);
        if (rm && paperId) {
          const prog = await desktopClient.open<RoadmapProgressEntry[]>(
            "list_roadmap_progress",
            { request: { paperId, roadmapId: rm.id } },
          );
          setRoadmapProgress(prog ?? []);
        } else {
          setRoadmapProgress([]);
        }
      } catch {
        setRoadmapProjection(null);
        setRoadmapProgress([]);
      }
    },
    [],
  );

  const refreshDocuments = useCallback(async () => {
    try {
      const [next, cols] = await Promise.all([
        desktopClient.open<DocumentCard[]>("list_documents"),
        desktopClient.open<CollectionProjection[]>("list_collections").catch(() => [] as CollectionProjection[]),
      ]);
      setDocuments(next);
      setCollections(cols as CollectionProjection[]);
      const savedRevisionId = localStorage.getItem(LAST_ACTIVE_REVISION_ID_KEY);
      const savedPaperId = localStorage.getItem(LAST_ACTIVE_PAPER_ID_KEY);
      const targetDoc =
        (savedRevisionId &&
          next.find((doc) => doc.revisionId === savedRevisionId)) ||
        (savedPaperId && next.find((doc) => doc.id === savedPaperId)) ||
        next[0];
      if (
        targetDoc &&
        (!selectedRevisionId ||
          !next.some((doc) => doc.revisionId === selectedRevisionId))
      ) {
        setSelectedRevisionId(targetDoc.revisionId);
      }
      try {
        const current = await desktopClient.open<WorkspaceInfo | null>(
          "get_workspace",
        );
        if (current) {
          setWorkspace(current);
          if (current.unmanagedPdfNotice) {
            setLibraryNotice(current.unmanagedPdfNotice);
          }
        }
      } catch {
        /* workspace projection is optional during library refresh */
      }
    } catch {
      setStatus(t("feedback.message1"));
    }
  }, [selectedRevisionId]);

  const refreshModelSettings = useCallback(async () => {
    try {
      setModelSettings(
        await desktopClient.open<ModelSettings>("get_model_settings"),
      );
    } catch (error) {
      setStatus(t("feedback.message2", { v0: String(error) }));
    }
  }, []);

  const openSettings = useCallback(
    (section?: SettingsSection) => {
      const resolved =
        section ?? (workspace?.available ? "models" : "workspace");
      setSettingsInitialSection(resolved);
      setShowSettings(true);
    },
    [workspace],
  );

  const openProviderSettingsForRecovery = useCallback(
    (providerInstanceId: string | null) => {
      if (!providerInstanceId) {
        setStatus(t("app.noProviderInstance"));
        return;
      }
      setSettingsInitialSection("models");
      setSettingsInitialProviderId(providerInstanceId);
      setSettingsFocusNonce((nonce) => nonce + 1);
      setShowSettings(true);
    },
    [],
  );

  const loadDocument = useCallback(
    async (revisionId: string, knownPaperId?: string) => {
      const paperId =
        knownPaperId ??
        documents.find((document) => document.revisionId === revisionId)?.id;
      setReadingStateReadyId("");
      setPdfPageCount(0);
      setSelectedRevisionId(revisionId);
      try {
        const [
          nextThreads,
          nextArchived,
          nextBrief,
          savedState,
          nextOcr,
          nextGuide,
          nextJobs,
          nextArtifacts,
          nextOutline,
          nextAnnotations,
        ] = await Promise.all([
          desktopClient.open<Thread[]>("list_threads", { revisionId }),
          desktopClient.open<Thread[]>("list_archived_threads", { revisionId }),
          desktopClient.open<Brief | null>("get_brief", { revisionId }),
          paperId
            ? desktopClient.open<ReadingState | null>("get_reading_state", {
                paperId,
              })
            : Promise.resolve(null),
          desktopClient.open<OcrProjection | null>("latest_ocr", {
            revisionId,
          }),
          desktopClient.open<GuideProjection>("get_reading_guide", {
            revisionId,
          }),
          desktopClient.open<JobProjection[]>("list_jobs"),
          paperId
            ? desktopClient.open<ArtifactProjection[]>("list_artifacts", {
                paperId,
              })
            : Promise.resolve([]),
          desktopClient
            .open<OutlineProjection>("get_outline", { revisionId })
            .catch(() => null),
          paperId
            ? desktopClient
                .open<ReaderAnnotation[]>("list_annotations" as never, {
                  paperId,
                })
                .catch(() => readLocalAnnotations(paperId))
            : Promise.resolve([] as ReaderAnnotation[]),
        ]);
        let resolved = nextThreads;
        if (resolved.length === 0) {
          const created = await desktopClient.command<Thread>("create_thread", {
            revisionId,
            title: "Main discussion",
            kind: "global",
          });
          resolved = [created];
        }

        let readingContext: {
          lifecycle: ReadingLifecycle;
          engagement: import("./readerReadingState").ReadingEngagement | null;
        } = {
          lifecycle: defaultReadingLifecycle(paperId ?? ""),
          engagement: null,
        };
        if (paperId) {
          try {
            if (desktopClient.runtime === "desktop") {
              const result = await libraryWorkspaceClient.read({
                kind: "reading_context",
                protocolVersion: LIBRARY_PROTOCOL_VERSION,
                paperId,
                revisionId,
              });
              if (result.kind === "reading_context") readingContext = result.context;
            } else {
              readingContext = await desktopClient.open("get_reading_context", {
                paperId,
                revisionId,
              });
            }
          } catch {
            /* keep unread defaults */
          }
        }
        setReadingLifecycle(readingContext.lifecycle);
        setCompletionPromptOpen(false);
        setCompletionDismissedKey("");

        const restored =
          savedState?.revisionId === revisionId ? savedState : null;
        const resume = resolveResumeTarget({
          savedState,
          currentRevisionId: revisionId,
          engagement: readingContext.engagement,
          pageCount: readingContext.engagement?.pageCountSnapshot ?? null,
        });
        if (restored) {
          setPage(restored.pageNumber);
          setPageOffset(restored.pageOffset);
          setRestoreOffset(restored.pageOffset);
          setZoom(restored.zoom);
          setRotation(restored.rotation);
          setQuestion(restored.discussionDraft);
          setRightTab(restored.rightTab);
          setWorkspaceLayout(parseWorkspaceLayout(restored.workspaceLayout));
          setOutlineView(parseOutlineView(restored.outlineView));
          setOutlineNodeId(restored.activeOutlineNodeId);
          setDeepDiveNodeId(null);
          setOutlineInspectorWidth(
            clampOutlineInspectorWidth(
              restored.outlineInspectorWidth ?? OUTLINE_INSPECTOR_DEFAULT,
            ),
          );
          setGuideLayerVisible(restored.guideLayerVisible !== false);
        } else {
          setPage(resume.pageNumber);
          setPageOffset(0);
          setRestoreOffset(0);
          setZoom(100);
          setRotation(0);
          setQuestion("");
          setRightTab("discussion");
          setWorkspaceLayout(defaultWorkspaceLayout());
          setOutlineView("overview");
          setOutlineNodeId(null);
          setDeepDiveNodeId(null);
          setOutlineInspectorWidth(OUTLINE_INSPECTOR_DEFAULT);
          setGuideLayerVisible(true);
        }

        const restoredThread = restored?.activeDiscussionId
          ? resolved.find((thread) => thread.id === restored.activeDiscussionId)
          : undefined;
        const nextThread = restoredThread ?? resolved[0];
        setThreads(resolved);
        setArchivedThreads(nextArchived);
        setPreferBrief(false);
        setActiveThreadId(nextThread.id);
        setBrief(nextBrief);
        setOcr(nextOcr);
        setAnnotations(
          (Array.isArray(nextAnnotations) && nextAnnotations.length > 0
            ? nextAnnotations
            : paperId
              ? readLocalAnnotations(paperId)
              : []) as ReaderAnnotation[],
        );
        setGuideProjection(nextGuide);
        setActiveGuideInkId(null);
        setGuideConfirming(false);
        setGuideError(null);
        if (outlineScopeRef.current.revisionId === revisionId) {
          setOutlineProjection(nextOutline);
          if (
            nextOutline &&
            nextOutline.status !== "missing_ocr" &&
            !nextOutline.head?.graph
          ) {
            const requestId = ++outlineRequestId.current;
            void desktopClient
              .command<OutlinePlan>(
                "plan_outline",
                buildPlanOutlineInvokeArgs(revisionId),
              )
              .then((planned) => {
                if (requestId !== outlineRequestId.current || outlineScopeRef.current.revisionId !== revisionId) return;
                setOutlinePlan(planned);
                setOutlinePlanError(null);
              })
              .catch((err) => {
                if (requestId !== outlineRequestId.current || outlineScopeRef.current.revisionId !== revisionId) return;
                setOutlinePlan(null);
                setOutlinePlanError(String(err));
              });
          } else {
            setOutlinePlan(null);
            setOutlinePlanError(null);
          }
        }
        setQuoteBasket(
          reconcileQuoteBasket(
            revisionId,
            nextOcr,
            restored?.quoteBasket ?? [],
          ),
        );
        setFocusedBlockId(null);
        setSelectedAnnotationId(null);
        setJobs(nextJobs);
        setArtifacts(nextArtifacts);
        const restoredArtifact = restored?.activeArtifactId
          ? nextArtifacts.find(
              (artifact) => artifact.id === restored.activeArtifactId,
            )
          : undefined;
        setActiveArtifactId(
          restoredArtifact?.id ??
            nextArtifacts.find((artifact) => artifact.kind === "brief")?.id ??
            nextArtifacts[0]?.id ??
            "",
        );
        setLensQa([]);
        const nextMessages = await desktopClient.open<Message[]>(
          "list_messages",
          { threadId: nextThread.id },
        );
        setMessages(nextMessages);
        setReadingStateReadyId(paperId ?? "");
        if (paperId) {
          void persistReaderActivity(
            paperId,
            revisionId,
            resume.pageNumber,
            readingContext.engagement?.pageCountSnapshot ?? 1,
          );
        }
        void refreshRoadmap(revisionId, paperId);
      } catch (error) {
        setStatus(t("app.loadDocumentFailed", { error: String(error) }));
      }
    },
    [documents, persistReaderActivity],
  );

  const bootstrapWorkspace = useCallback(async () => {
    let currentWorkspace: WorkspaceInfo | null = null;
    try {
      let current = await desktopClient.open<WorkspaceInfo | null>(
        "get_workspace",
      );
      const savedRoot = localStorage.getItem("read-desktop.workspaceRoot");
      if (desktopClient.runtime === "desktop" && !current && savedRoot)
        current = await desktopClient.command<WorkspaceInfo>(
          "choose_workspace",
          {
            rootPath: savedRoot,
          },
        );
      if (current) {
        currentWorkspace = current;
        setWorkspace(current);
        localStorage.setItem("read-desktop.workspaceRoot", current.rootPath);
        if (!current.available) {
          const message =
            current.statusDetail === "reset_required"
              ? "This V0.1 Workspace must be reset before V2 can import PDFs."
              : "Workspace unavailable · " + current.statusDetail;
          setLibraryNotice(message);
          setStatus(message);
          setSettingsInitialSection("workspace");
          setShowSettings(true);
        }
      }
    } catch {
      /* first launch or a removed drive */
    }
    if (!currentWorkspace || currentWorkspace.available) {
      await refreshDocuments();
      await refreshStats();
      await refreshJobs();
    }
    await refreshModelSettings();
  }, [refreshDocuments, refreshJobs, refreshStats, refreshModelSettings]);

  const pickUiLocale = async (locale: "zh-CN" | "en") => {
    setLocaleBusy(true);
    setLocaleError(null);
    try {
      await desktopClient.command("set_ui_locale", { locale });
      setUiLocale(locale);
      document.documentElement.lang = locale;
      await bootstrapWorkspace();
    } catch (error) {
      setLocaleError(String(error));
    } finally {
      setLocaleBusy(false);
    }
  };

  const confirmLocaleChange = async () => {
    if (!pendingLocale) return;
    setLocaleChangeBusy(true);
    try {
      await desktopClient.command("set_ui_locale", { locale: pendingLocale });
      setUiLocale(pendingLocale);
      document.documentElement.lang = pendingLocale;
      setPendingLocale(null);
    } catch (error) {
      setStatus(String(error));
    } finally {
      setLocaleChangeBusy(false);
    }
  };

  useEffect(() => {
    (async () => {
      try {
        const projection = await desktopClient.open<UiLocaleProjection>(
          "get_ui_locale",
        );
        if (projection.locale == null) {
          setUiLocale(null);
          return;
        }
        setUiLocale(projection.locale);
        document.documentElement.lang = projection.locale;
        await bootstrapWorkspace();
      } catch (error) {
        setUiLocale(null);
        setLocaleError(String(error));
      }
    })();
  }, [bootstrapWorkspace]);
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void desktopClient
      .watch((event) => {
        if (
          event.kind === "lifecycle" &&
          event.status === "exit_choice_required"
        ) {
          setExitChoiceOpen(true);
          void refreshJobs();
        }
        if (event.kind === "library") {
          if (event.delta) {
            try {
              const notices = JSON.parse(event.delta) as unknown;
              if (
                Array.isArray(notices) &&
                notices.every((item) => typeof item === "string")
              ) {
                setLibraryNotice(notices.join(" "));
              }
            } catch {
              /* not a kind-change notice payload */
            }
          }
          void refreshDocuments();
          void refreshStats();
        }
        if (event.kind === "job") {
          void refreshJobs();
          if (selectedDoc) void refreshGuide(selectedDoc.revisionId);
        }
        if (
          event.kind === "paper" &&
          selectedDoc &&
          (!event.entityId || event.entityId === selectedDoc.revisionId)
        ) {
          void refreshOcr(selectedDoc.revisionId);
          void refreshArtifacts(selectedDoc.id);
          void refreshBrief(selectedDoc.revisionId);
          void refreshOutline(selectedDoc.revisionId);
          void refreshGuide(selectedDoc.revisionId).then((next) => {
            if (event.delta !== "reading_guide" || !next?.head) return;
            setGuideConfirming(false);
            const inks = locateGuideInks(
              normalizeGuideInks(next.head.inks),
              ocrRef.current?.blocks ?? [],
            );
            const first = firstGuideAnchor(inks);
            const noteCount = guideNotes(inks).length;
            setGuideLayerVisible(true);
            if (
              first &&
              first.pageNumber >= 1 &&
              jumpedGuideHeadId.current !== next.head.id
            ) {
              jumpedGuideHeadId.current = next.head.id;
              setPage(first.pageNumber);
              setActiveGuideInkId(first.id);
            }
            setStatus(
              noteCount > 0 && first
                ? t("app.guideReadyFromPage", { count: noteCount, page: first.pageNumber })
                : noteCount > 0
                  ? t("app.guideReadyCount", { count: noteCount })
                  : t("app.guideReady"),
            );
          });
          void refreshRoadmap(selectedDoc.revisionId, selectedDoc.id);
          if (event.delta === "outline") {
            setStatus(t("app.outlineReady"));
          }
          if (outlineNodeId) {
            const parentHeadId = outlineScopeRef.current.parentHeadId;
            const requestId = ++outlineLocalRequestId.current;
            void desktopClient
              .open<OutlineHeadProjection | null>("get_outline_deep_dive", {
                revisionId: selectedDoc.revisionId,
                nodeId: outlineNodeId,
              })
              .then((head) => {
                if (requestId !== outlineLocalRequestId.current || outlineScopeRef.current.parentHeadId !== parentHeadId || outlineScopeRef.current.revisionId !== selectedDoc.revisionId || outlineScopeRef.current.nodeId !== outlineNodeId) return;
                setDeepDiveHead(head);
              })
              .catch(() => undefined);
          }
          void refreshJobs();
        }
        if (event.kind === "generation" && event.entityId) {
          const finished =
            event.status === "complete" ||
            event.status === "cancelled" ||
            event.status === "failed";
          if (finished) {
            if (event.status === "failed") {
              setMessages((current) =>
                current.filter(
                  (message) =>
                    message.id !== event.entityId &&
                    message.status !== "failed",
                ),
              );
              setStatus(
                "Chat failed · " +
                  (event.delta || "the model did not return a reply"),
              );
            }
            void refreshMessages(activeThreadId);
            void refreshStats();
            if (selectedDoc) {
              void desktopClient
                .open<Thread[]>("list_threads", {
                  revisionId: selectedDoc.revisionId,
                })
                .then(setThreads)
                .catch(() => undefined);
            }
          } else if (event.delta) {
            setMessages((current) => {
              const exists = current.some(
                (message) => message.id === event.entityId,
              );
              if (!exists) {
                void refreshMessages(activeThreadId);
                return current;
              }
              return current.map((message) =>
                message.id === event.entityId
                  ? {
                      ...message,
                      content: message.content + event.delta,
                      status: "streaming",
                    }
                  : message,
              );
            });
          } else {
            setMessages((current) => {
              if (!current.some((message) => message.id === event.entityId)) {
                void refreshMessages(activeThreadId);
                return current;
              }
              return current.map((message) =>
                message.id === event.entityId
                  ? {
                      ...message,
                      status:
                        event.status === "streaming" ||
                        event.status === "complete" ||
                        event.status === "cancelled" ||
                        event.status === "failed"
                          ? event.status
                          : message.status,
                    }
                  : message,
              );
            });
          }
        }
      })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [
    refreshArtifacts,
    refreshBrief,
    refreshDocuments,
    refreshJobs,
    refreshMessages,
    refreshOcr,
    refreshStats,
    activeThreadId,
    selectedDoc,
    outlineNodeId,
  ]);
  useEffect(() => {
    if (!pendingArtifactJob) return;
    const job = jobs.find(
      (candidate) => candidate.id === pendingArtifactJob.id,
    );
    if (
      !job ||
      !["completed", "failed", "cancelled", "interrupted_unknown"].includes(
        job.state,
      )
    ) {
      return;
    }
    if (job.state === "completed") {
      setPreferBrief(false);
      if (selectedDoc?.id === pendingArtifactJob.paperId) {
        void refreshArtifacts(
          pendingArtifactJob.paperId,
          pendingArtifactJob.blockId,
        );
      }
      void refreshStats();
      setLensQa([]);
      setStatus(t("feedback.message3", { v0: pendingArtifactJob.label }));
    } else {
      const detail = job.lastError ? ` · ${job.lastError}` : "";
      setStatus(t("feedback.message4", { v0: pendingArtifactJob.label, v1: job.state, v2: detail }));
    }
    setArtifactBusyBlockId(null);
    setPendingArtifactJob(null);
  }, [
    jobs,
    pendingArtifactJob,
    refreshArtifacts,
    refreshStats,
    selectedDoc?.id,
  ]);
  useEffect(() => {
    const done = pendingDocumentJobs.filter((pending) => jobs.some((job) => job.id === pending.id
      && ["completed", "failed", "cancelled", "interrupted_unknown"].includes(job.state)));
    if (!done.length) return;
    setStatus(done.map((pending) => {
      const job = jobs.find((candidate) => candidate.id === pending.id)!;
      return job.state === "completed" ? t("app.jobGenerated", { label: pending.label }) : `${pending.label} · ${job.state}${job.lastError ? ` · ${job.lastError}` : ""}`;
    }).join(t("app.listSep")));
    setPendingDocumentJobs((current) => current.filter((pending) => !done.some((item) => item.id === pending.id)));
  }, [jobs, pendingDocumentJobs]);
  useEffect(() => {
    if (!pendingOrientationJob) return;
    const job = jobs.find(
      (candidate) => candidate.id === pendingOrientationJob.id,
    );
    if (
      !job ||
      !["completed", "failed", "cancelled", "interrupted_unknown"].includes(
        job.state,
      )
    ) {
      return;
    }
    if (job.state === "completed") {
      if (selectedDoc?.revisionId === pendingOrientationJob.revisionId) {
        void refreshBrief(pendingOrientationJob.revisionId);
        void refreshArtifacts(pendingOrientationJob.paperId);
      }
      void refreshDocuments();
      void refreshStats();
      setStatus(
        t("app.briefGenerated"),
      );
    } else {
      const detail = job.lastError ? ` · ${job.lastError}` : "";
      setStatus(t("feedback.message5", { v0: job.state, v1: detail }));
    }
    setPendingOrientationJob(null);
  }, [
    jobs,
    pendingOrientationJob,
    refreshArtifacts,
    refreshBrief,
    refreshDocuments,
    refreshStats,
    selectedDoc?.revisionId,
  ]);
  useEffect(
    () => () => {
      readingStateSaver.dispose(true);
      activitySaver.dispose(true);
    },
    [readingStateSaver],
  );

  useEffect(() => {
    let disposed = false;
    setActiveArtifactCropSrc("");
    if (!isTauri || !activeArtifact?.kind.startsWith("lens_")) return;
    void desktopClient
      .open<string | null>("get_artifact_asset_path", {
        artifactId: activeArtifact.id,
      })
      .then((path) => {
        if (!disposed && path) setActiveArtifactCropSrc(convertFileSrc(path));
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
    };
  }, [activeArtifact]);

  useEffect(() => {
    if (!isTauri || !selectedDoc || readingStateReadyId !== selectedDoc.id) {
      return;
    }
    readingStateSaver.push({
      paperId: selectedDoc.id,
      revisionId: selectedDoc.revisionId,
      pageNumber: page,
      pageOffset,
      zoom,
      rotation: rotation as ReadingState["rotation"],
      rightTab,
      activeArtifactId: activeArtifactId || null,
      activeDiscussionId: activeThreadId || null,
      discussionDraft: question,
      quoteBasket,
      workspaceLayout,
      activeOutlineNodeId: outlineNodeId,
      outlineView,
      outlineInspectorWidth,
      guideLayerVisible,
      updatedAt: new Date().toISOString(),
    });
  }, [
    activeArtifactId,
    activeThreadId,
    guideLayerVisible,
    outlineInspectorWidth,
    outlineNodeId,
    outlineView,
    page,
    pageOffset,
    question,
    quoteBasket,
    readingStateReadyId,
    readingStateSaver,
    rightTab,
    rotation,
    selectedDoc,
    workspaceLayout,
    zoom,
  ]);

  useEffect(() => {
    if (!selectedDoc || readingStateReadyId !== selectedDoc.id) return;
    const count = pdfPageCount || selectedDoc.pages || 1;
    activitySaver.push({
      paperId: selectedDoc.id,
      revisionId: selectedDoc.revisionId,
      pageNumber: page,
      pageCount: count,
    });
    const promptKey = `${selectedDoc.id}:${selectedDoc.revisionId}`;
    if (
      readingLifecycle &&
      shouldPromptCompletion(readingLifecycle.status, readingProgress(page, pdfPageCount || selectedDoc.pages))
    ) {
      if (completionDismissedKey !== promptKey) setCompletionPromptOpen(true);
    } else {
      setCompletionPromptOpen(false);
    }
  }, [
    activitySaver,
    completionDismissedKey,
    page,
    pdfPageCount,
    readingLifecycle,
    readingStateReadyId,
    selectedDoc,
  ]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "," && event.ctrlKey) {
        event.preventDefault();
        openSettings();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [openSettings]);

  useEffect(() => {
    if (selectedDoc && !activeThreadId)
      void loadDocument(selectedDoc.revisionId, selectedDoc.id);
  }, [selectedDoc, activeThreadId, loadDocument]);

  useEffect(() => {
    try {
      localStorage.setItem(LAST_VIEW_MODE_KEY, viewMode);
    } catch {
      /* ignore */
    }
  }, [viewMode]);

  useEffect(() => {
    if (selectedDoc) {
      try {
        localStorage.setItem(LAST_ACTIVE_PAPER_ID_KEY, selectedDoc.id);
        localStorage.setItem(
          LAST_ACTIVE_REVISION_ID_KEY,
          selectedDoc.revisionId,
        );
      } catch {
        /* ignore */
      }
    }
  }, [selectedDoc]);

  useEffect(() => {
    const onBeforeUnload = () => {
      readingStateSaver.dispose(true);
      activitySaver.dispose(true);
    };
    window.addEventListener("beforeunload", onBeforeUnload);
    return () => window.removeEventListener("beforeunload", onBeforeUnload);
  }, [readingStateSaver]);

  useEffect(() => {
    if (selectedDoc && selectedDoc.revisionId !== selectedRevisionId)
      setSelectedRevisionId(selectedDoc.revisionId);
  }, [selectedDoc, selectedRevisionId]);

  const chooseWorkspace = async () => {
    if (!isTauri) {
      setStatus(t("feedback.message6"));
      return;
    }
    const activeJobCount = jobs.filter((job) =>
      ["queued", "running", "paused"].includes(job.state),
    ).length;
    if (activeJobCount > 0) {
      const message = t("app.workspaceSwitchBlocked", { count: activeJobCount });
      setStatus(message);
      notifyWarning(t("app.workspaceSwitchBlockedTitle"), message, {
        source: "workspace",
        dedupeKey: "workspace-switch-active-jobs",
        persistent: true,
      });
      setShowOperations(true);
      return;
    }
    const picked = await open({
      directory: true,
      multiple: false,
      title: "Choose Read Atlas Workspace",
    });
    const path = typeof picked === "string" ? picked : null;
    if (!path) return;
    setBusy(true);
    setStatus(t("feedback.message7"));
    try {
      const info = await desktopClient.command<WorkspaceInfo>(
        "choose_workspace",
        {
          rootPath: path,
        },
      );
      setWorkspace(info);
      localStorage.setItem("read-desktop.workspaceRoot", info.rootPath);
      if (!info.available) {
        const message =
          info.statusDetail === "reset_required"
            ? "This V0.1 Workspace must be reset before V2 can import PDFs."
            : "Workspace unavailable · " + info.statusDetail;
        setLibraryNotice(message);
        setStatus(message);
        setSettingsInitialSection("workspace");
        setShowSettings(true);
        return;
      }
      setLibraryNotice(null);
      await refreshDocuments();
      await refreshStats();
      setStatus(t("feedback.message8"));
    } catch (error) {
      setStatus(t("feedback.message9", { v0: String(error) }));
    } finally {
      setBusy(false);
    }
  };

  const resetWorkspace = async (previewDigest: string) => {
    if (!workspace || workspace.statusDetail !== "reset_required") {
      throw new Error("No legacy Workspace is waiting to be reset");
    }
    if (!previewDigest) {
      throw new Error("Legacy reset preview is missing or expired");
    }
    setBusy(true);
    setStatus(t("feedback.message10"));
    try {
      const result = await desktopClient.command<{
        workspace: WorkspaceInfo;
        backupPath: string;
        fileCount: number;
        pdfCount: number;
        totalBytes: number;
      }>("execute_legacy_reset", {
        request: {
          rootPath: workspace.rootPath,
          previewDigest,
        },
      });
      setWorkspace(result.workspace);
      localStorage.setItem(
        "read-desktop.workspaceRoot",
        result.workspace.rootPath,
      );
      setLibraryNotice(null);
      await refreshDocuments();
      await refreshStats();
      await refreshJobs();
      setStatus(t("feedback.message11"));
      notifySuccess(t("app.workspaceResetOk"), t("app.workspaceResetBackup", { path: result.backupPath }), {
        source: "workspace",
        dedupeKey: `workspace-reset-${result.backupPath}`,
        persistent: true,
      });
      return result;
    } catch (error) {
      const message = "Workspace reset failed · " + String(error);
      setLibraryNotice(message);
      setStatus(message);
      notifyError(t("app.workspaceResetFailed"), String(error), {
        source: "workspace",
        dedupeKey: `workspace-reset-failed-${workspace.rootPath}`,
      });
      throw error;
    } finally {
      setBusy(false);
    }
  };

  const importPdf = async (collection = "Papers/Inbox") => {
    if (!isTauri) {
      setStatus(t("feedback.message12"));
      return;
    }
    if (!workspace) {
      setStatus(t("feedback.message13"));
      openSettings("workspace");
      return;
    }
    if (!workspace.available) {
      const message =
        workspace.statusDetail === "reset_required"
          ? "Reset this V0.1 Workspace before importing PDFs."
          : "Workspace unavailable · " + workspace.statusDetail;
      setLibraryNotice(message);
      setStatus(message);
      openSettings("workspace");
      return;
    }
    const picked = await open({
      multiple: true,
      title: "Import PDFs",
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    const paths = Array.isArray(picked) ? picked : typeof picked === "string" ? [picked] : [];
    if (paths.length === 0) return;
    await importPdfPaths(paths, collection);
  };
  const importPdfPaths = async (paths: string[], collection: string) => {
    if (paths.length > 500) {
      setStatus(t("app.importTooMany"));
      notifyWarning(t("app.importTooLarge"), t("app.importTooLargeBody"), { source: "import" });
      return;
    }
    setBusy(true);
    setLibraryNotice(null);
    setStatus(t("app.importPlanning"));
    try {
      const run = await planAndStart(
        libraryWorkspaceClient.act.bind(libraryWorkspaceClient),
        { kind: "import", collectionPath: collection },
        { kind: "sources", paths },
      );
      if (run.status === "needs_confirmation") {
        setImportPlan(run.plan);
        setStatus(t("app.importNeedsConfirm"));
        return;
      }
      await refreshDocuments();
      await refreshStats();
      announceBatch(batchOf(run.result), undoTokenOf(run.result));
    } catch (error) {
      const message = "Import failed · " + toLibraryActError(error).message;
      setLibraryNotice(message);
      setStatus(message);
      notifyError(t("app.importFailed"), toLibraryActError(error).message, { source: "import" });
    } finally {
      setBusy(false);
    }
  };
  const announceBatch = (batch: BatchProjection, token: string | null) => {
    rememberUndoToken(batch.id, token, batch.undo?.expiresAt);
    const message = summarizeBatch(batch, t);
    setStatus(message);
    notifySuccess(t("app.batchComplete"), token ? t("app.batchCompleteUndo", { message }) : message, {
      source: "import",
      action: { kind: "open_operations", label: t("app.viewDetails") },
    });
  };
  const retryLibraryBatch = (batchId: string) => {
    void libraryWorkspaceClient
      .act({
        kind: "control_batch",
        protocolVersion: 1,
        idempotencyKey: `retry-${batchId}-${Date.now()}`,
        batchId,
        control: { kind: "retry_failed", itemIds: null },
      })
      .then(() => refreshOperations())
      .catch((error) => notifyError(t("app.retryFailed"), toLibraryActError(error).message, { source: "job" }));
  };
  const cancelLibraryBatch = (batchId: string) => {
    void libraryWorkspaceClient
      .act({
        kind: "control_batch",
        protocolVersion: 1,
        idempotencyKey: `cancel-${batchId}-${Date.now()}`,
        batchId,
        control: { kind: "cancel_remaining" },
      })
      .then(() => refreshOperations())
      .catch((error) => notifyError(t("app.cancelFailed"), toLibraryActError(error).message, { source: "job" }));
  };
  const undoLibraryBatch = (batchId: string, token: string) => {
    void libraryWorkspaceClient
      .act(controlBatchRequest(batchId, { kind: "undo", token }))
      .then(async (result) => {
        consumeUndoToken(batchId);
        await refreshDocuments();
        await refreshOperations();
        notifySuccess(t("app.batchUndone"), summarizeBatch(batchOf(result), t), { source: "import" });
      })
      .catch((error) => notifyError(t("app.undoFailed"), toLibraryActError(error).message, { source: "import" }));
  };
  const handleCreateFolder = async (parentPath: string) => {
    try {
      setBusy(true);
      const proj = await desktopClient.command<CollectionProjection>("create_collection", { request: { parentPath } } as unknown as Record<string, unknown>);
      await refreshDocuments();
      setHubSelectedFolder(proj.relativePath);
      setStatus(t("app.createdName", { name: proj.name }));
    } catch (e) { setLibraryNotice(String(e)); setStatus(String(e)); } finally { setBusy(false); }
  };
  const handleRenameFolder = async (relativePath: string, newName: string) => {
    try {
      setBusy(true);
      const proj = await desktopClient.command<CollectionProjection>("rename_collection", { request: { relativePath, newName } } as unknown as Record<string, unknown>);
      await refreshDocuments();
      setHubSelectedFolder(proj.relativePath);
      setStatus(t("app.renamedTo", { name: proj.name }));
    } catch (e) { setLibraryNotice(String(e)); } finally { setBusy(false); }
  };
  const handleMoveFolder = async (relativePath: string, destParentPath: string) => {
    try {
      setBusy(true);
      const proj = await desktopClient.command<CollectionProjection>("move_collection", { request: { relativePath, destParentPath } } as unknown as Record<string, unknown>);
      await refreshDocuments();
      setHubSelectedFolder(proj.relativePath);
      setStatus(t("app.moved"));
    } catch (e) { setLibraryNotice(String(e)); throw e; } finally { setBusy(false); }
  };
  const handleRenamePaper = async (paperId: string, newStem: string) => {
    try {
      setBusy(true);
      await desktopClient.command("rename_paper", { request: { paperId, newFileName: newStem } } as unknown as Record<string, unknown>);
      await refreshDocuments();
      setStatus(t("app.renamed"));
    } catch (e) { setLibraryNotice(String(e)); } finally { setBusy(false); }
  };
  const handleMovePaper = async (paperId: string, destCollection: string) => {
    try {
      setBusy(true);
      const doc = documents.find((d) => d.id === paperId);
      if (!doc) throw new Error("paper not found");
      await desktopClient.command("move_paper", { paperId, collectionPath: destCollection, fileName: doc.fileName, confirmKindChange: false } as unknown as Record<string, unknown>);
      await refreshDocuments();
      setStatus(t("app.moved"));
    } catch (e) { setLibraryNotice(String(e)); throw e; } finally { setBusy(false); }
  };
  const handleTrashPaper = async (paperId: string) => {
    const doc = documents.find((d) => d.id === paperId);
    if (!doc) return;
    setPaperToTrash(doc);
  };
  const handleTrashFolder = async (relativePath: string) => {
    const cnt = documents.filter((d) => d.collection === relativePath || d.collection.startsWith(relativePath + "/")).length;
    setFolderToTrash({ relativePath, count: cnt });
  };
  const handleConfirmFolderTrash = async () => {
    if (!folderToTrash) return;
    const { relativePath, count } = folderToTrash;
    try {
      setBusy(true);
      await desktopClient.command("trash_collection", { request: { relativePath } } as unknown as Record<string, unknown>);
      await refreshDocuments();
      await refreshOperations();
      if (hubSelectedFolder === relativePath || hubSelectedFolder.startsWith(relativePath + "/")) setHubSelectedFolder("");
      setStatus(count > 0 ? t("app.trashedCount", { count }) : t("app.deletedEmptyFolder"));
      setFolderToTrash(null);
    } catch (e) { setLibraryNotice(String(e)); } finally { setBusy(false); }
  };
  const handleOpenResource = async (path: string) => {
    try { await desktopClient.command("open_resource_dir", { path } as unknown as Record<string, unknown>); } catch (e) { setLibraryNotice(String(e)); }
  };
  const handleOpenWorkspace = async () => {
    try { await desktopClient.command("open_workspace_dir"); } catch (e) { setLibraryNotice(String(e)); }
  };
  const handleSetSortMode = async (collectionId: string, sortMode: string) => {
    try {
      await desktopClient.command("set_collection_sort_mode", { request: { collectionId, sortMode } } as unknown as Record<string, unknown>);
      await refreshDocuments();
    } catch (e) { setLibraryNotice(String(e)); }
  };
  const handleReorderPapers = async (collectionId: string, paperIds: string[]) => {
    try {
      await desktopClient.command("reorder_collection_papers", { request: { collectionId, paperIds } } as unknown as Record<string, unknown>);
      await refreshDocuments();
    } catch (e) { setLibraryNotice(String(e)); throw e; }
  };

  const selectThread = async (thread: Thread) => {
    setActiveThreadId(thread.id);
    setReplyTo(null);
    setEditingMessageId(null);
    try {
      setMessages(
        await desktopClient.open<Message[]>("list_messages", {
          threadId: thread.id,
        }),
      );
    } catch (error) {
      setStatus(t("feedback.message14", { v0: String(error) }));
    }
  };

  const closeThread = async (thread: Thread) => {
    if (threads.length <= 1) return;
    try {
      const result = await desktopClient.command<CloseThreadResult>(
        "close_thread",
        { threadId: thread.id },
      );
      const remaining = threads.filter((item) => item.id !== thread.id);
      setThreads(remaining);
      if (result.action === "archived") {
        setArchivedThreads((current) => [
          { ...thread, status: "archived" },
          ...current.filter((item) => item.id !== thread.id),
        ]);
        setStatus(t("app.threadClosed"));
      } else {
        setStatus(t("app.emptyThreadClosed"));
      }
      if (thread.id === activeThreadId) {
        const next = remaining[0];
        if (next) await selectThread(next);
      }
    } catch (error) {
      setStatus(t("app.closeThreadFailed", { error: String(error) }));
    }
  };

  const restoreThread = async (thread: Thread) => {
    try {
      const restored = await desktopClient.command<Thread>("restore_thread", {
        threadId: thread.id,
      });
      setArchivedThreads((current) =>
        current.filter((item) => item.id !== thread.id),
      );
      setThreads((current) => [
        restored,
        ...current.filter((item) => item.id !== restored.id),
      ]);
      await selectThread(restored);
      setStatus(t("app.threadRestored"));
    } catch (error) {
      setStatus(t("app.restoreThreadFailed", { error: String(error) }));
    }
  };

  const deleteArchivedThread = async (thread: Thread) => {
    try {
      await desktopClient.command("delete_thread", { threadId: thread.id });
      setArchivedThreads((current) =>
        current.filter((item) => item.id !== thread.id),
      );
      setStatus(t("app.threadDeletedForever"));
    } catch (error) {
      setStatus(t("app.deleteThreadFailed", { error: String(error) }));
    }
  };

  const renameThread = async (thread: Thread, title: string) => {
    try {
      const renamed = await desktopClient.command<Thread>("rename_thread", {
        threadId: thread.id,
        title,
      });
      setThreads((current) =>
        current.map((item) => (item.id === renamed.id ? renamed : item)),
      );
      setArchivedThreads((current) =>
        current.map((item) => (item.id === renamed.id ? renamed : item)),
      );
    } catch (error) {
      setStatus(t("app.renameFailed", { error: String(error) }));
    }
  };

  const newThread = async () => {
    if (!selectedDoc) return;
    try {
      const thread = await desktopClient.command<Thread>("create_thread", {
        revisionId: selectedDoc.revisionId,
        title: t("app.newThreadTitle"),
        kind: "global",
      });
      setThreads((current) => [thread, ...current]);
      setActiveThreadId(thread.id);
      setMessages([]);
      setReplyTo(null);
      setEditingMessageId(null);
    } catch (error) {
      setStatus(t("app.newThreadFailed", { error: String(error) }));
    }
  };

  const saveThreadTitleInline = async () => {
    setIsEditingThreadTitle(false);
    const trimmed = editingThreadTitleText.trim();
    if (trimmed && activeThread && trimmed !== activeThread.title) {
      await renameThread(activeThread, trimmed);
    }
  };

  const handleJumpToMessage = (messageId: string) => {
    const idx = visibleDiscussion.findIndex((n) => n.message.id === messageId);
    if (idx !== -1) {
      const fromEnd = visibleDiscussion.length - idx;
      const requiredTurns = Math.ceil(fromEnd / 2) + 2;
      if (requiredTurns > visibleTurnLimit) {
        setVisibleTurnLimit(requiredTurns);
      }
    }
    setTimeout(() => {
      const el = document.getElementById(`msg-${messageId}`);
      if (el) {
        el.scrollIntoView({ behavior: "smooth", block: "center" });
        el.classList.remove("target-highlight");
        void el.offsetWidth; // trigger reflow
        el.classList.add("target-highlight");
        setActiveScrollMessageId(messageId);
      }
    }, 40);
  };

  const handleMessageStreamScroll = useCallback(() => {
    if (!messageStreamRef.current) return;
    const container = messageStreamRef.current;
    if (
      container.scrollTop < 30 &&
      visibleDiscussion.length > visibleTurnLimit * 2
    ) {
      setVisibleTurnLimit((prev) => prev + 15);
    }
    const containerMid = container.scrollTop + container.clientHeight / 2;

    const userMsgs = messages.filter((m) => m.role === "user");
    let closestId: string | null = null;
    let minDistance = Infinity;

    for (const msg of userMsgs) {
      const el = document.getElementById(`msg-${msg.id}`);
      if (el) {
        const msgMid = el.offsetTop + el.clientHeight / 2;
        const dist = Math.abs(msgMid - containerMid);
        if (dist < minDistance) {
          minDistance = dist;
          closestId = msg.id;
        }
      }
    }
    if (closestId) {
      setActiveScrollMessageId(closestId);
    }
  }, [messages, visibleDiscussion.length, visibleTurnLimit]);


  const applyChatTurn = (
    turn: { user: Message; assistant: Message },
    nextHeadId: string,
  ) => {
    setMessages((current) => {
      const next = [...current];
      for (const message of [turn.user, turn.assistant]) {
        if (!next.some((candidate) => candidate.id === message.id)) {
          next.push(message);
        }
      }
      return next;
    });
    setThreads((current) =>
      current.map((thread) =>
        thread.id === activeThread?.id
          ? { ...thread, activeMessageId: nextHeadId }
          : thread,
      ),
    );
  };

  const sendQuestion = async (acknowledged = false) => {
    if (
      !selectedDoc ||
      !activeThread ||
      !question.trim() ||
      busy ||
      activeStreamingMessage
    )
      return;
    if (needsPaperProvider) {
      setStatus(paperProviderNotConfiguredStatus(currentProvider?.kind, t));
      openSettings("models");
      return;
    }
    const asked = question.trim();
    const parentId = editingMessageId
      ? (messages.find((message) => message.id === editingMessageId)
          ?.parentId ?? null)
      : replyTo;
    setQuestion("");
    setBusy(true);
    setStatus(t("feedback.message15"));
    let shouldRetryLongPdfWarning = false;
    try {
      const turn = await desktopClient.command<{
        user: Message;
        assistant: Message;
      }>(
        "send_chat",
        buildSendChatInvokeArgs({
          revisionId: selectedDoc.revisionId,
          threadId: activeThread.id,
          parentId,
          question: asked,
          page,
          blockIds: quoteBasket.map((quote) => quote.blockId),
        }),
      );
      applyChatTurn(turn, turn.user.id);
      setStatus(t("feedback.message16"));
      setQuoteBasket([]);
      setReplyTo(null);
      setEditingMessageId(null);
      if (
        activeThread &&
        isPlaceholderThreadTitle(activeThread.title) &&
        !messages.some((message) => message.role === "user")
      ) {
        const nextTitle = titleFromFirstQuestion(asked);
        void desktopClient
          .command<Thread>("rename_thread", {
            threadId: activeThread.id,
            title: nextTitle,
          })
          .then((renamed) => {
            setThreads((current) =>
              current.map((item) =>
                item.id === renamed.id ? renamed : item,
              ),
            );
          })
          .catch(() => undefined);
      }
    } catch (error) {
      if (!acknowledged && isLongPdfWarningRequired(error)) {
        const ok = await askLongPdfWarning(selectedDoc);
        if (ok) {
          shouldRetryLongPdfWarning = true;
        } else {
          setStatus(t("app.cancelledSplitPdf"));
        }
      } else {
        setStatus(t("feedback.message17", { v0: String(error) }));
        setQuestion(asked);
      }
    } finally {
      setBusy(false);
    }
    if (shouldRetryLongPdfWarning) {
      setQuestion(asked);
      void sendQuestion(true);
    }
  };

  const editUserMessage = (message: Message) => {
    if (message.role !== "user") return;
    setEditingMessageId(message.id);
    setReplyTo(message.parentId);
    setQuestion(message.content);
    setQuoteBasket(
      reconcileQuoteBasket(
        selectedDoc?.revisionId ?? "",
        ocr,
        message.blockQuotes,
      ),
    );
    setStatus(t("feedback.message18"));
  };

  const regenerateMessage = async (message: Message) => {
    if (!selectedDoc || !activeThread || busy || activeStreamingMessage) return;
    if (needsPaperProvider) {
      setStatus(paperProviderNotConfiguredStatus(currentProvider?.kind, t));
      openSettings("models");
      return;
    }
    setBusy(true);
    setStatus(t("feedback.message19"));
    try {
      const turn = await desktopClient.command<{
        user: Message;
        assistant: Message;
      }>(
        "send_chat",
        buildSendChatInvokeArgs({
          revisionId: selectedDoc.revisionId,
          threadId: activeThread.id,
          parentId: null,
          question: "",
          page,
          blockIds: [],
          regenerateFromId: message.id,
        }),
      );
      applyChatTurn(turn, turn.user.id);
      setReplyTo(null);
      setEditingMessageId(null);
      setStatus(t("feedback.message20"));
    } catch (error) {
      setStatus(t("feedback.message21", { v0: String(error) }));
    } finally {
      setBusy(false);
    }
  };

  const cancelGeneration = async (message: Message) => {
    try {
      const cancelled = await desktopClient.command<boolean>(
        "cancel_generation",
        { messageId: message.id },
      );
      setStatus(
        cancelled
          ? "Stopping response · saved partial text will be kept"
          : "Response is no longer running",
      );
    } catch (error) {
      setStatus("Unable to stop response · " + String(error));
    }
  };

  const branchFrom = async (message: Message) => {
    setReplyTo(message.id);
    setEditingMessageId(null);
    setStatus(t("feedback.message22"));
    if (activeThread) {
      try {
        await desktopClient.command("create_branch", {
          threadId: activeThread.id,
          messageId: message.id,
        });
      } catch {
        /* local branch point remains useful */
      }
    }
  };

  const selectActiveBranch = async (message: Message) => {
    if (activeThread) {
      try {
        await desktopClient.command("set_active_branch", {
          threadId: activeThread.id,
          messageId: message.id,
        });
      } catch (error) {
        setStatus(t("feedback.message23", { v0: String(error) }));
        return;
      }
    }
    setThreads((current) =>
      current.map((thread) =>
        thread.id === activeThread?.id
          ? { ...thread, activeMessageId: message.id }
          : thread,
      ),
    );
    setReplyTo(null);
    setStatus(t("feedback.message24"));
  };

  const deleteDiscussionTurn = async (user: Message) => {
    if (!activeThread) return;
    try {
      const result = await desktopClient.command<DeleteDiscussionTurnResult>(
        "delete_discussion_turn",
        { threadId: activeThread.id, messageId: user.id },
      );
      setMessages(
        await desktopClient.open<Message[]>("list_messages", {
          threadId: activeThread.id,
        }),
      );
      setThreads((current) =>
        current.map((thread) =>
          thread.id === activeThread.id
            ? { ...thread, activeMessageId: result.nextHeadId }
            : thread,
        ),
      );
      if (replyTo && result.deleted > 0) {
        setReplyTo(null);
        setEditingMessageId(null);
      }
      setStatus(
        result.nextHeadId
          ? t("app.turnDeleted")
          : t("app.turnDeletedEmpty"),
      );
    } catch (error) {
      setStatus(t("app.deleteFailed", { error: String(error) }));
    }
  };

  const askLongPdfWarning = async (doc: DocumentCard): Promise<boolean> => {
    const ok = window.confirm(
      t("app.longPdfWarning", { title: doc.title }),
    );
    if (!ok) {
      setStatus(t("app.cancelledSplitPdf"));
      return false;
    }
    try {
      await desktopClient.command("ack_long_pdf_warning", {
        paperId: doc.id,
        revisionId: doc.revisionId,
      });
      return true;
    } catch (error) {
      setStatus(t("app.confirmFailed", { error: String(error) }));
      return false;
    }
  };

  const startOcr = async (acknowledged = false) => {
    if (!selectedDoc || activeOcrJob) return;
    setStatus(t("feedback.message25"));
    try {
      const job = await desktopClient.command<JobProjection>("start_ocr", {
        revisionId: selectedDoc.revisionId,
      });
      setJobs((current) => [
        ...current.filter((item) => item.id !== job.id),
        job,
      ]);
      setStatus(
        t("feedback.message26"),
      );
    } catch (error) {
      if (!acknowledged && isLongPdfWarningRequired(error)) {
        const ok = await askLongPdfWarning(selectedDoc);
        if (ok) void startOcr(true);
        return;
      }
      setStatus(t("feedback.message27", { v0: String(error) }));
    }
  };

  const deleteOcrCascade = async () => {
    if (!selectedDoc) return;
    try {
      setBusy(true);
      await desktopClient.command("delete_ocr_cascade", {
        revisionId: selectedDoc.revisionId,
      });
      setOcr(null);
      setDocuments((docs) =>
        docs.map((d) =>
          d.id === selectedDoc.id ? { ...d, hasOcr: false } : d,
        ),
      );
      await refreshArtifacts(selectedDoc.id);
      setShowOcrDeleteModal(false);
      setStatus(t("app.ocrCleared"));
    } catch (err) {
      setStatus(t("app.deleteOcrFailed", { error: String(err) }));
    } finally {
      setBusy(false);
    }
  };

  const handleUpdateTags = async (paperId: string, tags: string[]) => {
    try {
      await desktopClient.command("update_paper_tags", {
        paperId,
        tags,
      });
      setDocuments((prev) =>
        prev.map((d) => (d.id === paperId ? { ...d, keywords: tags } : d)),
      );
      if (selectedDoc && selectedDoc.id === paperId) {
        await refreshArtifacts(paperId);
      }
      setStatus(t("app.tagsUpdated"));
    } catch (err) {
      setStatus(t("app.updateTagsFailed", { error: String(err) }));
    }
  };

  const handleUpdateMetadata = async (
    revisionId: string,
    metadata: Record<string, unknown>,
    pinnedFields: string[],
  ) => {
    try {
      await desktopClient.command("update_paper_metadata", {
        revisionId,
        metadata,
        pinnedFields,
      });
      if (selectedDoc && selectedDoc.revisionId === revisionId) {
        const newTitle = String(metadata.title || selectedDoc.title);
        setDocuments((prev) =>
          prev.map((d) =>
            d.revisionId === revisionId ? { ...d, title: newTitle } : d,
          ),
        );
        await refreshArtifacts(selectedDoc.id);
      }
      setStatus(t("app.metadataLocked"));
    } catch (err) {
      setStatus(t("app.updateMetadataFailed", { error: String(err) }));
      throw err;
    }
  };

  const handleUpdateOrientationTable = async (
    revisionId: string,
    kind: string,
    entries: Record<string, unknown>[],
    pinnedKeys: string[],
    artifactId?: string,
  ) => {
    try {
      await desktopClient.command("update_orientation_table", {
        revisionId,
        kind,
        entries,
        pinnedKeys,
        artifactId,
      });
      if (selectedDoc) {
        await refreshArtifacts(selectedDoc.id);
      }
      setStatus(
        t("app.auxLocked", {
          kind: kind === "glossary" ? t("app.kind.glossaryFull") : t("app.kind.symbolTableFull"),
        }),
      );
    } catch (err) {
      setStatus(
        t("app.auxLockFailed", {
          kind: kind === "glossary" ? t("app.kind.glossary") : t("app.kind.symbolTable"),
          error: String(err),
        }),
      );
      throw err;
    }
  };

  const selectArtifact = async (artifact: ArtifactProjection) => {
    setShowArtifactAnnotations(false);
    setPreferBrief(false);
    setActiveArtifactId(artifact.id);
    const isBlock =
      artifact.kind.startsWith("lens_") ||
      artifact.kind === "translation" ||
      artifact.kind === "explanation";
    if (isBlock) {
      setLastBlockArtifactId(artifact.id);
      setArtifactScope("block");
    } else {
      setLastGlobalArtifactId(artifact.id);
      setArtifactScope("global");
    }
    if (artifact.kind.startsWith("lens_")) {
      try {
        setLensQa(
          await desktopClient.open<LensQaProjection[]>("list_lens_qa", {
            lensArtifactId: artifact.id,
          }),
        );
      } catch {
        setLensQa([]);
      }
      if (pdfDocument) {
        const evidence = artifact.evidence[0];
        const block = ocr?.blocks.find((b) => b.id === artifact.objectKey);
        const pageNumber = evidence?.pageNumber ?? block?.pageNumber;
        const bbox = evidence?.bbox ?? block?.bbox;
        if (pageNumber && bbox) {
          try {
            const crops = await createLensCrops(
              pdfDocument,
              pageNumber,
              bbox,
              rotation,
            );
            if (crops.displayCropDataUrl) {
              setActiveArtifactCropSrc(crops.displayCropDataUrl);
            }
          } catch (err) {
            console.warn("Could not extract crop for selected Lens artifact", err);
          }
        }
      }
    } else {
      setLensQa([]);
    }
  };

  const setArtifactOverride = async (
    artifact: ArtifactProjection,
    key: string,
    value: Record<string, unknown>,
  ) => {
    try {
      await desktopClient.command("set_artifact_override", {
        artifactId: artifact.id,
        kind: artifact.kind,
        key,
        value,
      });
      const updated = await desktopClient.open<ArtifactProjection>(
        "get_artifact",
        { artifactId: artifact.id },
      );
      setArtifacts((current) =>
        current.map((candidate) =>
          candidate.id === updated.id ? updated : candidate,
        ),
      );
      setStatus(t("feedback.message28", { v0: key }));
    } catch (error) {
      setStatus(t("feedback.message29", { v0: String(error) }));
      throw error;
    }
  };
  const toggleBlockQuote = (
    block: OcrBlockProjection,
    cropDataUrl?: string,
  ) => {
    if (!selectedDoc || !ocr) return;
    setFocusedBlockId(block.id);
    setPage(block.pageNumber);
    setQuoteBasket((current) => {
      if (current.some((quote) => quote.blockId === block.id)) {
        setStatus(
          t("feedback.message30", { v0: block.blockIndex + 1 }),
        );
        return current.filter((quote) => quote.blockId !== block.id);
      }
      if (current.length >= 32) {
        setStatus(t("feedback.message31"));
        return current;
      }
      setStatus(
        t("feedback.message32", { v0: block.blockIndex + 1, v1: block.pageNumber }),
      );
      return [
        ...current,
        snapshotBlock(selectedDoc.revisionId, ocr.id, block, cropDataUrl),
      ];
    });
  };

  const annotationBlockIds = useMemo(
    () => ({
      highlighted: annotations
        .filter((item) => item.kind === "highlight" && item.status !== "deleted")
        .map((item) => item.locator.blockId)
        .filter((id): id is string => Boolean(id)),
      bookmarked: annotations
        .filter((item) => item.kind === "bookmark" && item.status !== "deleted")
        .map((item) => item.locator.blockId)
        .filter((id): id is string => Boolean(id)),
      noted: annotations
        .filter((item) => item.kind === "note" && item.status !== "deleted")
        .map((item) => item.locator.blockId)
        .filter((id): id is string => Boolean(id)),
    }),
    [annotations],
  );

  const updateAnnotationState = useCallback(
    (next: ReaderAnnotation[]) => {
      setAnnotations(next);
      if (selectedDoc) writeLocalAnnotations(selectedDoc.id, next);
    },
    [selectedDoc],
  );

  const deleteUserAnnotation = useCallback(
    async (annotation: ReaderAnnotation) => {
      if (!selectedDoc) return;
      setAnnotationSavingIds((current) => new Set(current).add(annotation.id));
      try {
        await desktopClient.command(
          "delete_annotation" as never,
          { annotationId: annotation.id },
        );
      } catch {
        // Browser preview and older runtimes use the local resilience cache.
      }
      const next = annotations.filter((item) => item.id !== annotation.id);
      updateAnnotationState(next);
      setStatus(
        (annotation.kind === "note"
          ? t("app.kind.note")
          : annotation.kind === "bookmark"
            ? t("app.kind.bookmark")
            : t("app.kind.highlight")) + t("app.deletedSuffix"),
      );
      setAnnotationSavingIds((current) => {
        const nextIds = new Set(current);
        nextIds.delete(annotation.id);
        return nextIds;
      });
    },
    [annotations, selectedDoc, updateAnnotationState],
  );

  const saveUserAnnotation = useCallback(
    async (
      kind: "highlight" | "bookmark" | "note",
      block: OcrBlockProjection,
      title = "",
      body = "",
    ) => {
      if (!selectedDoc || !ocr) return;
      const existing = annotations.find(
        (item) =>
          item.kind === kind &&
          item.locator.blockId === block.id &&
          item.status !== "deleted",
      );
      if (existing && (kind === "highlight" || kind === "bookmark")) {
        await deleteUserAnnotation(existing);
        return;
      }
      if (kind === "note" && !body.trim()) return;
      const now = new Date().toISOString();
      const optimistic: ReaderAnnotation = {
        id:
          "annotation-" +
          Date.now() +
          "-" +
          Math.random().toString(36).slice(2, 7),
        kind,
        paperId: selectedDoc.id,
        title: title.trim() || null,
        body: body.trim() || null,
        color: kind === "highlight" ? "#f5c542" : null,
        status: "active",
        createdAt: now,
        updatedAt: now,
        locator: {
          pageNumber: block.pageNumber,
          blockId: block.id,
          excerpt: block.textContent,
          bbox: block.bbox,
        },
      };
      const optimisticList = [...annotations, optimistic];
      updateAnnotationState(optimisticList);
      setAnnotationBusy(true);
      setAnnotationError(null);
      try {
        const created = await desktopClient.command<ReaderAnnotation>(
          "create_annotation" as never,
          {
            request: {
              paperId: selectedDoc.id,
              kind,
              title: optimistic.title,
              body: optimistic.body,
              color: optimistic.color,
              locator: {
                documentRevisionId: selectedDoc.revisionId,
                ocrRevisionId: ocr.id,
                blockId: block.id,
                contentDigest: block.contentDigest,
                excerpt: block.textContent,
                pageNumber: block.pageNumber,
                bbox: block.bbox,
                coordinateSpace: "normalized_1000",
              },
            },
          },
        );
        const next = optimisticList.map((item) =>
          item.id === optimistic.id ? created : item,
        );
        updateAnnotationState(next);
        setStatus(
          kind === "highlight"
            ? t("app.highlightSaved")
            : kind === "bookmark"
              ? t("app.bookmarkSaved")
              : t("app.noteSaved"),
        );
        setIsPdfOnly(false);
        setWorkspaceLayout(
          applyWorkspaceLayoutIntent(workspaceLayout, "open_artifacts"),
        );
        setRightTab("artifacts");
        setArtifactScope("global");
        setShowArtifactAnnotations(true);
      } catch (error) {
        setStatus(t("app.savedLocalCache"));
        setAnnotationError(String(error));
      } finally {
        setAnnotationBusy(false);
      }
    },
    [
      annotations,
      deleteUserAnnotation,
      ocr,
      selectedDoc,
      updateAnnotationState,
      workspaceLayout,
    ],
  );

  const handleAnnotationAction = useCallback(
    (block: OcrBlockProjection, kind: "highlight" | "bookmark" | "note") => {
      if (kind === "highlight") {
        void saveUserAnnotation(kind, block);
        return;
      }
      setAnnotationError(null);
      setAnnotationComposer({ kind, block });
    },
    [saveUserAnnotation],
  );

  const jumpToAnnotation = useCallback(
    (annotation: ReaderAnnotation) => {
      rememberReaderLocation();
      setSelectedAnnotationId(annotation.id);
      setPage(annotation.locator.pageNumber);
      if (annotation.locator.blockId) {
        setFocusedBlockId(annotation.locator.blockId);
      } else {
        setFocusedBlockId(null);
      }
      if (!layoutShowsPdf(workspaceLayout)) {
        setWorkspaceLayout(
          applyWorkspaceLayoutIntent(workspaceLayout, "show_pdf"),
        );
      }
      if (annotation.status === "orphan") {
        setStatus(t("app.annotationNeedsRelocate"));
      } else {
        setStatus(t("app.jumpedToPage", { page: annotation.locator.pageNumber }));
      }
    },
    [workspaceLayout],
  );

  const submitAnnotationComposer = (value: {
    title: string;
    body: string;
  }) => {
    const pending = annotationComposer;
    if (!pending) return;
    void saveUserAnnotation(
      pending.kind,
      pending.block,
      value.title,
      value.body,
    ).finally(() => setAnnotationComposer(null));
  };

  const updateUserAnnotation = useCallback(
    async (
      id: string,
      patch: { body?: string; title?: string; color?: string },
    ) => {
      const current = annotations.find((item) => item.id === id);
      if (!current || !selectedDoc) return;
      setAnnotationSavingIds((ids) => new Set(ids).add(id));
      const next = annotations.map((item) =>
        item.id === id
          ? {
              ...item,
              ...patch,
              updatedAt: new Date().toISOString(),
            }
          : item,
      );
      updateAnnotationState(next);
      try {
        await desktopClient.command(
          "update_annotation" as never,
          { request: { id, ...patch } },
        );
      } catch {
        // The local cache remains authoritative until the desktop runtime is available.
      } finally {
        setAnnotationSavingIds((ids) => {
          const nextIds = new Set(ids);
          nextIds.delete(id);
          return nextIds;
        });
      }
    },
    [annotations, selectedDoc, updateAnnotationState],
  );

  const captureReaderLocation = (): ReaderLocation => ({
    paperId: selectedDoc?.id,
    revisionId: selectedDoc?.revisionId,
    page,
    pageOffset,
    zoom,
    rotation,
    focusedBlockId,
    rightTab,
    activeArtifactId: activeArtifactId || null,
    artifactScope,
    workspaceLayout,
    outlineView,
    outlineNodeId,
    activeGuideInkId,
  });

  const rememberReaderLocation = () => {
    readerHistoryRef.current = pushReaderLocation(
      readerHistoryRef.current,
      captureReaderLocation(),
    );
    setReaderHistoryVersion((version) => version + 1);
  };

  const restoreReaderLocation = (location: ReaderLocation) => {
    if (location.paperId && selectedDoc?.id && location.paperId !== selectedDoc.id) {
      return;
    }
    if (location.revisionId && selectedDoc?.revisionId && location.revisionId !== selectedDoc.revisionId) {
      return;
    }
    setPage(location.page);
    setPageOffset(location.pageOffset ?? 0);
    setRestoreOffset(location.pageOffset ?? 0);
    if (location.zoom !== undefined) setZoom(location.zoom);
    if (location.rotation !== undefined) setRotation(location.rotation);
    setFocusedBlockId(location.focusedBlockId ?? null);
    if (location.rightTab === "discussion" || location.rightTab === "artifacts") {
      setRightTab(location.rightTab);
    }
    if (location.activeArtifactId !== undefined) {
      setActiveArtifactId(location.activeArtifactId ?? "");
    }
    if (location.artifactScope === "global" || location.artifactScope === "block") {
      setArtifactScope(location.artifactScope);
    }
    if (location.workspaceLayout) {
      setWorkspaceLayout(parseWorkspaceLayout(location.workspaceLayout));
    }
    if (location.outlineView === "overview" || location.outlineView === "deep_dive") {
      setOutlineView(location.outlineView);
    }
    setOutlineNodeId(location.outlineNodeId ?? null);
    setActiveGuideInkId(location.activeGuideInkId ?? null);
    setStatus(t("app.returnedToPrevious"));
  };

  const goBackReader = () => {
    const popped = popReaderLocation(readerHistoryRef.current);
    if (!popped.location) return;
    readerHistoryRef.current = popped.history;
    setReaderHistoryVersion((version) => version + 1);
    restoreReaderLocation(popped.location);
  };

  // Keep history scoped to one paper; a citation in another document must not
  // unexpectedly return to a stale reader state.
  useEffect(() => {
    readerHistoryRef.current = clearReaderHistory();
    setReaderHistoryVersion((version) => version + 1);
  }, [selectedDoc?.id, selectedDoc?.revisionId]);

  const moveBlockQuote = (blockId: string, offset: -1 | 1) => {
    setQuoteBasket((current) => {
      const index = current.findIndex((quote) => quote.blockId === blockId);
      const target = index + offset;
      if (index < 0 || target < 0 || target >= current.length) return current;
      const next = [...current];
      [next[index], next[target]] = [next[target], next[index]];
      return next;
    });
  };

  const jumpToBlockQuote = (quote: BlockQuoteSnapshot) => {
    rememberReaderLocation();
    setPage(quote.pageNumber);
    setFocusedBlockId(quote.blockId);
    if (!layoutShowsPdf(workspaceLayout)) {
      setWorkspaceLayout(
        applyWorkspaceLayoutIntent(workspaceLayout, "show_pdf"),
      );
    }
  };

  const jumpOutlineEvidence = (
    target:
      | string
      | { blockId?: string | null; pageNumber?: number | null },
  ) => {
    const blockId = typeof target === "string" ? target : target.blockId;
    const pageNumber =
      typeof target === "string" ? undefined : target.pageNumber ?? undefined;
    const block = blockId
      ? ocr?.blocks.find((item) => item.id === blockId)
      : undefined;
    const catalog = blockId
      ? outlineProjection?.catalog?.entries.find((item) => item.id === blockId)
      : undefined;
    if (block) {
      rememberReaderLocation();
      setPage(block.pageNumber);
      setFocusedBlockId(block.id);
      if (!layoutShowsPdf(workspaceLayout)) {
        setWorkspaceLayout(
          applyWorkspaceLayoutIntent(workspaceLayout, "show_pdf"),
        );
      }
      return;
    }
    if (catalog) {
      rememberReaderLocation();
      setPage(catalog.page);
      setFocusedBlockId(catalog.id);
      if (!layoutShowsPdf(workspaceLayout)) {
        setWorkspaceLayout(
          applyWorkspaceLayoutIntent(workspaceLayout, "show_pdf"),
        );
      }
      return;
    }
    if (pageNumber) {
      rememberReaderLocation();
      setPage(pageNumber);
      setFocusedBlockId(null);
      if (!layoutShowsPdf(workspaceLayout)) {
        setWorkspaceLayout(
          applyWorkspaceLayoutIntent(workspaceLayout, "show_pdf"),
        );
      }
    }
  };

  const refreshOutline = async (revisionId: string) => {
    const requestId = ++outlineProjectionRequestId.current;
    const isCurrent = () => requestId === outlineProjectionRequestId.current
      && outlineScopeRef.current.revisionId === revisionId;
    try {
      const next = await desktopClient.open<OutlineProjection>("get_outline", { revisionId });
      if (!isCurrent()) return;
      setOutlineProjection(next);
      // Opening or explicitly regenerating a map obtains a fresh plan. A background
      // publication refresh must not replace a plan the reader is confirming.
      if (next.head?.graph) setOutlinePlan(null);
    } catch {
      /* keep last projection */
    }
  };

  const refreshGuide = async (revisionId: string) => {
    try {
      const next = await desktopClient.open<GuideProjection>("get_reading_guide", {
        revisionId,
      });
      setGuideProjection(next);
      return next;
    } catch {
      /* keep last projection */
      return null;
    }
  };

  const planGuide = guidePanel.open;

  const openGuide = async () => {
    if (!selectedDoc) return;
    try {
      const next = await desktopClient.open<GuideProjection>("get_reading_guide", {
        revisionId: selectedDoc.revisionId,
      });
      setGuideProjection(next);
      if (next.status === "missing_ocr") {
        setStatus(t("app.guideNeedOcr"));
        return;
      }
      if (next.head) {
        setGuideLayerVisible((visible) => !visible);
        return;
      }
      if (next.status === "generating" || next.activeJobId) {
        setStatus(t("app.guideGenerating"));
        return;
      }
      await planGuide();
    } catch (error) {
      setGuideError(String(error));
      setStatus(t("app.guidePlanFailed", { error: String(error) }));
    }
  };

  const startGuide = async () => {
    if (!selectedDoc || !guidePanel.canStart || !guidePlan) return;
    if (needsPaperProvider) {
      setStatus(paperProviderNotConfiguredStatus(currentProvider?.kind, t));
      openSettings("models");
      return;
    }
    try {
      const job = await desktopClient.command<JobProjection>(
        "start_reading_guide",
        {
          request: {
            revisionId: selectedDoc.revisionId,
            characterIds: guideCastIds,
            planId: guidePlan.planId,
            planDigest: guidePlan.planDigest,
          },
        },
      );
      setJobs((current) => [job, ...current.filter((item) => item.id !== job.id)]);
      if (!guidePanel.isCurrent()) return;
      setGuideConfirming(false);
      await refreshGuide(selectedDoc.revisionId);
      setStatus(t("app.guideJobStatus", { state: job.state, stage: job.stage }));
      await refreshJobs();
    } catch (error) {
      if (!guidePanel.isCurrent()) return;
      setGuideError(String(error));
      setStatus(t("app.guideGenerateFailed", { error: String(error) }));
    }
  };

  const deleteGuide = async () => {
    if (!selectedDoc) return;
    if (!window.confirm(t("app.deleteGuideConfirm"))) return;
    try {
      const next = await desktopClient.command<GuideProjection>(
        "delete_reading_guide",
        { request: { revisionId: selectedDoc.revisionId } },
      );
      setGuideProjection(next);
      setGuideLayerVisible(false);
      setActiveGuideInkId(null);
      setStatus(t("app.guideDeleted"));
      await refreshJobs();
    } catch (error) {
      setStatus(t("app.guideDeleteFailed", { error: String(error) }));
    }
  };

  const jumpFromMarkdownCitation = (href: string) => {
    const citation = parsePaperCitationHref(href);
    if (!citation) return;
    if (citation.kind === "page") {
      rememberReaderLocation();
      setPage(citation.page);
      if (!layoutShowsPdf(workspaceLayout)) {
        setWorkspaceLayout(
          applyWorkspaceLayoutIntent(workspaceLayout, "show_pdf"),
        );
      }
      return;
    }
    const quoted = quoteBasket.find(
      (item) => item.blockId === citation.blockId,
    );
    if (quoted) {
      jumpToBlockQuote(quoted);
      return;
    }
    const block = ocr?.blocks.find((item) => item.id === citation.blockId);
    if (block) {
      rememberReaderLocation();
      setPage(block.pageNumber);
      setFocusedBlockId(block.id);
      if (!layoutShowsPdf(workspaceLayout)) {
        setWorkspaceLayout(
          applyWorkspaceLayoutIntent(workspaceLayout, "show_pdf"),
        );
      }
    }
  };

  const handleBlockAction = async (
    block: OcrBlockProjection,
    action: BlockAction,
    material?: BlockActionMaterial,
    forceRegenerate?: boolean,
  ) => {
    if (action === "copy") {
      const canonicalText = block.textContent.trim();
      if (!canonicalText) {
        notifyWarning(t("app.ocrCopyEmptyTitle"), t("app.ocrCopyEmptyBody"), {
          source: "reader",
          entityId: block.id,
          dedupeKey: `ocr-copy-empty-${block.id}`,
        });
        return;
      }
      try {
        await navigator.clipboard.writeText(canonicalText);
        notifySuccess(t("app.ocrCopied"), t("app.ocrCopiedBody", { page: block.pageNumber, index: block.blockIndex + 1 }), {
          source: "reader",
          entityId: block.id,
          dedupeKey: `ocr-copy-${block.id}`,
        });
      } catch (error) {
        notifyError(t("app.ocrCopyFailed"), String(error), {
          source: "reader",
          entityId: block.id,
          dedupeKey: `ocr-copy-failed-${block.id}`,
        });
      }
      return;
    }
    if (!selectedDoc || !ocr) return;
    const actionKey = action === "translate" || action === "explain" ? action : "lens";
    const label = t(`app.action.${actionKey}`);
    setWorkspaceLayout(
      applyWorkspaceLayoutIntent(workspaceLayout, "open_artifacts"),
    );
    setIsPdfOnly(false);
    setPreferBrief(false);
    setRightTab("artifacts");
    setArtifactScope("block");

    // If not forcing regeneration, jump to existing artifact if one already exists for this block
    if (!forceRegenerate) {
      const isMatchingKind = (kind: string) => {
        if (action === "lens") return kind.startsWith("lens_");
        if (action === "translate") return kind === "translation";
        if (action === "explain") return kind === "explanation";
        return false;
      };

      // 1. Match by exact block ID or evidence ID
      let existing = artifacts.find(
        (a) =>
          isMatchingKind(a.kind) &&
          (a.objectKey === block.id ||
            a.evidence.some((e) => e.blockId === block.id)),
      );

      // 2. Match by spatial bounding box overlap on the same page
      if (!existing) {
        existing = artifacts.find((a) => {
          if (!isMatchingKind(a.kind)) return false;
          const ev = a.evidence[0];
          if (!ev || ev.pageNumber !== block.pageNumber || !ev.bbox) return false;
          const [ax1, ay1, ax2, ay2] = ev.bbox;
          const [bx1, by1, bx2, by2] = block.bbox;
          const ix1 = Math.max(ax1, bx1);
          const iy1 = Math.max(ay1, by1);
          const ix2 = Math.min(ax2, bx2);
          const iy2 = Math.min(ay2, by2);
          if (ix2 <= ix1 || iy2 <= iy1) return false;
          const interArea = (ix2 - ix1) * (iy2 - iy1);
          const aArea = Math.max(1, (ax2 - ax1) * (ay2 - ay1));
          const bArea = Math.max(1, (bx2 - bx1) * (by2 - by1));
          const iou = interArea / Math.min(aArea, bArea);
          return iou > 0.45;
        });
      }

      if (existing) {
        void selectArtifact(existing);
        setStatus(t("app.switchedToExisting", { label }));
        return;
      }
    }

    setArtifactBusyBlockId(block.id);
    if (material?.displayCropDataUrl) {
      setActiveArtifactCropSrc(material.displayCropDataUrl);
    }
    setStatus(
      label +
        " · Block " +
        (block.blockIndex + 1) +
        " · page " +
        block.pageNumber,
    );
    try {
      const job = await desktopClient.command<JobProjection>(
        "generate_reading_artifact",
        {
          revisionId: selectedDoc.revisionId,
          ocrRevisionId: ocr.id,
          blockId: block.id,
          action,
          outputLanguage: locale,
          displayCropDataUrl: material?.displayCropDataUrl ?? null,
          modelCropDataUrl: material?.modelCropDataUrl ?? null,
        },
      );
      setJobs((current) => [
        job,
        ...current.filter((candidate) => candidate.id !== job.id),
      ]);
      setPendingArtifactJob({
        id: job.id,
        paperId: selectedDoc.id,
        blockId: block.id,
        label: actionKey,
      });
      setStatus(t("feedback.message33", { v0: label, v1: job.stage }));
      await refreshJobs();
      await refreshOperations();
    } catch (error) {
      setArtifactBusyBlockId(null);
      setPendingArtifactJob(null);
      setStatus(label + " failed · " + String(error));
      console.error("generate_reading_artifact failed", error);
    }
  };

  const askLens = async (nextQuestion: string, parentId: string | null) => {
    if (!activeArtifact || lensQaBusy) return;
    setLensQaBusy(true);
    try {
      const turn = await desktopClient.command<LensQaTurn>("ask_lens", {
        lensArtifactId: activeArtifact.id,
        parentId,
        question: nextQuestion,
      });
      setLensQa((current) => [...current, turn.user, turn.assistant]);
      setStatus(t("feedback.message34"));
      await refreshStats();
    } catch (error) {
      setStatus("Lens question failed · " + String(error));
    } finally {
      setLensQaBusy(false);
    }
  };

  const transferLensToDiscussion = (artifact: ArtifactProjection) => {
    const content =
      artifact.content &&
      typeof artifact.content === "object" &&
      !Array.isArray(artifact.content)
        ? (artifact.content as Record<string, unknown>)
        : {};
    const quick =
      content.quickTakeaway &&
      typeof content.quickTakeaway === "object" &&
      !Array.isArray(content.quickTakeaway)
        ? (content.quickTakeaway as Record<string, unknown>)
        : {};
    const summary =
      typeof quick.markdown === "string" ? quick.markdown : "No summary";
    const evidence = artifact.evidence
      .map(
        (anchor) =>
          "p." + anchor.pageNumber + ": " + (anchor.excerpt ?? "OCR Block"),
      )
      .join("\n");
    const kindLabelMap: Record<string, string> = {
      lens_figure: "Figure Lens",
      lens_table: "Table Lens",
      lens_formula: "Formula Lens",
      translation: t("app.kind.paragraphTranslation"),
      explanation: t("app.kind.paragraphExplanation"),
      brief: t("app.kind.brief"),
      glossary: t("app.kind.glossary"),
      symbol_table: t("app.kind.symbolTable"),
      metadata: t("app.kind.paperMetadata"),
    };
    const friendlyKind = kindLabelMap[artifact.kind] ?? artifact.kind;
    const title =
      typeof content.title === "string" && content.title
        ? `（${content.title}）`
        : "";
    setQuestion(
      [
        t("app.quote.cite", { kind: friendlyKind, title }),
        t("app.quote.summary") + summary,
        t("app.quote.evidence") + "\n" + (evidence || "No evidence"),
        "",
        t("app.quote.continue"),
      ].join("\n"),
    );
    setWorkspaceLayout(
      applyWorkspaceLayoutIntent(workspaceLayout, "open_discussion"),
    );
    setRightTab("discussion");
    setStatus(t("feedback.message35"));
  };
  const regenerateLens = async (artifact: ArtifactProjection) => {
    if (!selectedDoc || !ocr) return;

    // Resolve the current OCR block for this artifact
    let block: OcrBlockProjection | undefined;

    // 1. Match by exact block ID
    block = ocr.blocks.find(
      (b) =>
        b.id === artifact.objectKey ||
        artifact.evidence.some((e) => e.blockId === b.id),
    );

    // 2. Match by spatial bounding box overlap with artifact evidence
    if (!block && artifact.evidence.length > 0) {
      const ev = artifact.evidence[0];
      if (ev.pageNumber && ev.bbox) {
        let bestBlock: OcrBlockProjection | undefined;
        let maxIou = 0;
        const [ax1, ay1, ax2, ay2] = ev.bbox;
        const aArea = Math.max(1, (ax2 - ax1) * (ay2 - ay1));

        for (const b of ocr.blocks) {
          if (b.pageNumber !== ev.pageNumber) continue;
          const [bx1, by1, bx2, by2] = b.bbox;
          const ix1 = Math.max(ax1, bx1);
          const iy1 = Math.max(ay1, by1);
          const ix2 = Math.min(ax2, bx2);
          const iy2 = Math.min(ay2, by2);
          if (ix2 > ix1 && iy2 > iy1) {
            const interArea = (ix2 - ix1) * (iy2 - iy1);
            const bArea = Math.max(1, (bx2 - bx1) * (by2 - by1));
            const iou = interArea / Math.min(aArea, bArea);
            if (iou > maxIou) {
              maxIou = iou;
              bestBlock = b;
            }
          }
        }
        if (bestBlock && maxIou > 0.3) {
          block = bestBlock;
        }
      }
    }

    // 3. Fallback: match by pageNumber and blockType
    if (!block && artifact.evidence.length > 0) {
      const ev = artifact.evidence[0];
      if (ev.pageNumber) {
        const expectedType = artifact.kind.replace("lens_", "");
        const pageBlocks = ocr.blocks.filter((b) => b.pageNumber === ev.pageNumber);
        const typeMatch = pageBlocks.find(
          (b) => b.blockType.toLowerCase() === expectedType.toLowerCase(),
        );
        block = typeMatch || pageBlocks[0];
      }
    }

    if (!block) {
      setStatus(t("app.noMatchingBlock"));
      return;
    }

    let action: BlockAction = "lens";
    if (artifact.kind === "translation") action = "translate";
    else if (artifact.kind === "explanation") action = "explain";

    let material: BlockActionMaterial | undefined;
    if (action === "lens") {
      if (pdfDocument) {
        try {
          material = await createLensCrops(
            pdfDocument,
            block.pageNumber,
            block.bbox,
            rotation,
          );
        } catch (err) {
          console.warn("Failed to create crops for Lens regeneration", err);
        }
      }
      if (!material?.displayCropDataUrl && activeArtifactCropSrc) {
        material = {
          displayCropDataUrl: activeArtifactCropSrc,
          modelCropDataUrl: activeArtifactCropSrc,
        };
      }
    }

    await handleBlockAction(block, action, material, true);
  };
  const handleDeleteArtifactVersion = async (artifactId: string) => {
    if (!selectedDoc) return;
    try {
      await desktopClient.command("delete_artifact", { artifactId });
      setStatus(t("app.versionDeleted"));
      const next = await desktopClient.open<ArtifactProjection[]>(
        "list_artifacts",
        { paperId: selectedDoc.id },
      );
      setArtifacts(next);
      if (activeArtifactId === artifactId) {
        setActiveArtifactId(next[0]?.id ?? "");
      }
    } catch (err) {
      console.error("Failed to delete artifact version", err);
      setStatus(t("app.deleteVersionFailed", { error: err instanceof Error ? err.message : String(err) }));
    }
  };
  outlineScopeRef.current = {
    revisionId: selectedDoc?.revisionId ?? "",
    parentHeadId: outlineProjection?.head?.id ?? "",
    nodeId: outlineNodeId ?? "",
  };
  useEffect(() => {
    outlineRequestId.current += 1;
    outlineLocalRequestId.current += 1;
    setOutlinePlan(null);
    setOutlinePlanError(null);
    setPendingOutlinePlan(null);
    setDeepDiveHead(null);
    setDeepDiveNodeId(null);
    setOutlineView("overview");
  }, [selectedDoc?.revisionId, outlineProjection?.head?.id]);
  useEffect(() => {
    setOutlineProjection(current => current?.revisionId === selectedDoc?.revisionId ? current : null);
  }, [selectedDoc?.revisionId]);

  const planOutlineGeneration = async (node?: OutlineNode, confirm = true) => {
    if (!selectedDoc) return;
    const revisionId = selectedDoc.revisionId;
    const parentHeadId = outlineProjection?.head?.id ?? "";
    const requestId = ++outlineRequestId.current;
    setOutlinePlanError(null);
    setPendingOutlinePlan(null);
    if (!node) setOutlinePlan(null);
    try {
      const planned = await desktopClient.command<OutlinePlan>(
        node ? "plan_outline_deep_dive" : "plan_outline",
        node ? { request: { revisionId, nodeId: node.nodeId } } : buildPlanOutlineInvokeArgs(revisionId),
      );
      if (requestId !== outlineRequestId.current || outlineScopeRef.current.revisionId !== revisionId || (node && (outlineScopeRef.current.parentHeadId !== parentHeadId || outlineScopeRef.current.nodeId !== node.nodeId))) return;
      if (!node) setOutlinePlan(planned);
      if (confirm) setPendingOutlinePlan({ plan: planned, nodeId: node?.nodeId, nodeTitle: node?.title });
    } catch (error) {
      if (requestId !== outlineRequestId.current || outlineScopeRef.current.revisionId !== revisionId) return;
      setOutlinePlanError(String(error));
    }
  };
  const openOutline = async () => {
    if (!selectedDoc) return;
    const revisionId = selectedDoc.revisionId;
    const requestId = ++outlineProjectionRequestId.current;
    setIsPdfOnly(false);
    setWorkspaceLayout(applyWorkspaceLayoutIntent(workspaceLayout, "open_outline"));
    setOutlineView("overview");
    setOutlinePlan(null);
    setPendingOutlinePlan(null);
    setOutlinePlanError(null);
    try {
      const next = await desktopClient.open<OutlineProjection>("get_outline", { revisionId });
      if (requestId !== outlineProjectionRequestId.current || outlineScopeRef.current.revisionId !== revisionId) return;
      setOutlineProjection(next);
      if (next.status !== "missing_ocr" && !next.head?.graph) await planOutlineGeneration(undefined, false);
    } catch (error) {
      if (outlineScopeRef.current.revisionId !== revisionId) return;
      setOutlinePlanError(String(error));
    }
  };
  const startOutline = async () => {
    if (!selectedDoc || outlineStartingRef.current) return;
    if (needsPaperProvider) {
      setStatus(paperProviderNotConfiguredStatus(currentProvider?.kind, t));
      openSettings("models");
      return;
    }
    const frozen = pendingOutlinePlan ?? (outlinePlan ? { plan: outlinePlan } : null);
    if (!frozen || frozen.plan.revisionId !== selectedDoc.revisionId) {
      await planOutlineGeneration();
      return;
    }
    const revisionId = selectedDoc.revisionId;
    const nodeId = "nodeId" in frozen ? frozen.nodeId : undefined;
    if (nodeId && (frozen.plan.expectedHeadId !== outlineProjection?.head?.id || nodeId !== outlineNodeId)) {
      const node = outlineProjection?.head?.graph?.nodes.find(item => item.nodeId === nodeId);
      if (node) await planOutlineGeneration(node);
      else setPendingOutlinePlan(null);
      return;
    }
    outlineStartingRef.current = true;
    setOutlineStarting(true);
    try {
      const args = buildStartOutlineInvokeArgs(revisionId, frozen.plan.planId, frozen.plan.planDigest);
      const job = await desktopClient.command<JobProjection>(
        nodeId ? "start_outline_deep_dive" : "start_outline",
        { request: { ...args.request, ...(nodeId ? { nodeId } : {}) } },
      );
      setJobs(current => [job, ...current.filter(item => item.id !== job.id)]);
      if (outlineScopeRef.current.revisionId !== revisionId) return;
      setPendingOutlinePlan(null);
      setOutlinePlan(null);
      setStatus(t("app.outlineQueued", { kind: nodeId ? t("app.localGraph") : t("app.contentGraph") }));
      const next = await desktopClient.open<OutlineProjection>("get_outline", { revisionId });
      if (outlineScopeRef.current.revisionId === revisionId) setOutlineProjection(next);
      await refreshJobs();
    } catch (error) {
      if (outlineScopeRef.current.revisionId !== revisionId) return;
      setStatus(t("app.outlineNotQueued", { error: String(error) }));
      // Re-plan after rejection; never silently submit changed inputs.
      const node = nodeId ? outlineProjection?.head?.graph?.nodes.find(item => item.nodeId === nodeId) : undefined;
      await planOutlineGeneration(node);
      setOutlinePlanError(t("app.outlinePlanRetry", { error: String(error) }));
    } finally {
      outlineStartingRef.current = false;
      setOutlineStarting(false);
    }
  };
  const deleteOutline = async () => {
    if (!selectedDoc) return;
    const revisionId = selectedDoc.revisionId;
    try {
      const next = await desktopClient.command<OutlineProjection>(
        "delete_outline",
        buildDeleteOutlineInvokeArgs(revisionId),
      );
      if (outlineScopeRef.current.revisionId !== revisionId) return;
      setOutlineProjection(next);
      setDeepDiveHead(null);
      setOutlineView("overview");
      setStatus(t("feedback.message36"));
      await refreshJobs();
    } catch (error) {
      setStatus(t("feedback.message37", { v0: String(error) }));
    }
  };
  const regenerateOutlineDeepDive = async () => {
    const node = outlineProjection?.head?.graph?.nodes.find(item => item.nodeId === outlineNodeId);
    if (node) await planOutlineGeneration(node);
  };
  const deleteOutlineDeepDive = async () => {
    if (!selectedDoc || !outlineNodeId) return;
    const revisionId = selectedDoc.revisionId;
    const nodeId = outlineNodeId;
    try {
      await desktopClient.command(
        "delete_outline_deep_dive",
        buildDeleteOutlineDeepDiveInvokeArgs(revisionId, nodeId),
      );
      if (outlineScopeRef.current.revisionId !== revisionId || outlineScopeRef.current.nodeId !== nodeId) return;
      setDeepDiveHead(null);
      setOutlineView("overview");
      setStatus(t("feedback.message38"));
      await refreshJobs();
    } catch (error) {
      setStatus(t("feedback.message39", { v0: String(error) }));
    }
  };
  const startOutlineDeepDive = async (node: OutlineNode) => {
    if (!selectedDoc) return;
    const revisionId = selectedDoc.revisionId;
    const parentHeadId = outlineProjection?.head?.id ?? "";
    const requestId = ++outlineLocalRequestId.current;
    setOutlineNodeId(node.nodeId);
    try {
      const cached = await desktopClient.open<OutlineHeadProjection | null>(
        "get_outline_deep_dive", { revisionId, nodeId: node.nodeId },
      );
      if (requestId !== outlineLocalRequestId.current || outlineScopeRef.current.revisionId !== revisionId || outlineScopeRef.current.parentHeadId !== parentHeadId) return;
      if (cached?.graph) {
        setDeepDiveHead(cached);
        setDeepDiveNodeId(null);
        setOutlineView("deep_dive");
        return;
      }
      await planOutlineGeneration(node);
    } catch (error) {
      if (requestId === outlineLocalRequestId.current && outlineScopeRef.current.revisionId === revisionId) setStatus(t("app.localGraphFailed", { error: String(error) }));
    }
  };
  const openDiscussionLayout = () => {
    setWorkspaceLayout(
      applyWorkspaceLayoutIntent(workspaceLayout, "open_discussion"),
    );
    setRightTab("discussion");
    if (activeOutlineJob) {
      const copy = outlineProgressCopy(activeOutlineJob, t);
      setStatus(t("app.outlineBackground", { stage: copy.stage, tokens: copy.tokens }));
    }
  };
  const openBrief = async () => {
    if (!selectedDoc) return;
    setIsPdfOnly(false);
    setWorkspaceLayout(
      applyWorkspaceLayoutIntent(workspaceLayout, "open_artifacts"),
    );
    setRightTab("artifacts");
    setArtifactScope("global");
    setPreferBrief(true);
    setLensQa([]);
    try {
      const next = await desktopClient.open<ArtifactProjection[]>(
        "list_artifacts",
        { paperId: selectedDoc.id },
      );
      setArtifacts(next);
      const briefArtifact = next.find((artifact) => artifact.kind === "brief");
      setActiveArtifactId(briefArtifact?.id ?? "");
    } catch {
      setActiveArtifactId("");
    }
  };
  const generateBrief = async (acknowledged = false) => {
    if (!selectedDoc || activeOrientationJob) return;
    if (needsPaperProvider) {
      setStatus(paperProviderNotConfiguredStatus(currentProvider?.kind, t));
      openSettings("models");
      return;
    }
    setStatus(t("feedback.message40"));
    try {
      const job = await desktopClient.command<JobProjection>("generate_brief", {
        revisionId: selectedDoc.revisionId,
      });
      setJobs((current) => [
        job,
        ...current.filter((candidate) => candidate.id !== job.id),
      ]);
      setPendingOrientationJob({
        id: job.id,
        paperId: selectedDoc.id,
        revisionId: selectedDoc.revisionId,
      });
      setDocuments((current) =>
        current.map((doc) =>
          doc.revisionId === selectedDoc.revisionId
            ? { ...doc, briefStatus: "queued" }
            : doc,
        ),
      );
      setPreferBrief(true);
      setWorkspaceLayout(
        applyWorkspaceLayoutIntent(workspaceLayout, "open_artifacts"),
      );
      setRightTab("artifacts");
      setStatus(
        job.stage === "queued" || job.state === "queued"
          ? "Brief queued · reading stays unblocked"
          : `Brief ${job.state} · ${job.stage}`,
      );
    } catch (error) {
      if (!acknowledged && isLongPdfWarningRequired(error)) {
        const ok = await askLongPdfWarning(selectedDoc);
        if (ok) void generateBrief(true);
        return;
      }
      setStatus(t("feedback.message41", { v0: String(error) }));
    }
  };

  const generateDocumentArtifact = async (kind: AuxiliaryDocumentArtifactKind, useBrief: boolean, acknowledged = false) => {
    if (!selectedDoc || busyDocumentArtifacts.includes(kind)) return;
    if (needsPaperProvider) {
      setStatus(paperProviderNotConfiguredStatus(currentProvider?.kind, t));
      openSettings("models");
      return;
    }
    const label = { glossary: t("app.kind.glossary"), symbol_table: t("app.kind.symbolTable"), metadata: t("app.kind.metadata") }[kind];
    try {
      const job = await desktopClient.command<JobProjection>("generate_document_artifact", {
        request: { revisionId: selectedDoc.revisionId, kind, useBrief },
      });
      setJobs((current) => [job, ...current.filter((candidate) => candidate.id !== job.id)]);
      setPendingDocumentJobs((current) => [...current.filter((item) => item.id !== job.id), { id: job.id, label }]);
      setPreferBrief(false);
      setStatus(t("app.kindQueued", { label }));
    } catch (error) {
      if (!acknowledged && isLongPdfWarningRequired(error)) {
        const ok = await askLongPdfWarning(selectedDoc);
        if (ok) void generateDocumentArtifact(kind, useBrief, true);
        return;
      }
      setStatus(t("app.kindGenerateFailed", { label, error: String(error) }));
    }
  };

  const openFolder = async () => {
    if (!selectedDoc?.pdfPath) {
      setStatus(t("feedback.message42"));
      return;
    }
    try {
      await desktopClient.command("open_resource_dir", {
        path: selectedDoc.pdfPath,
      });
    } catch (error) {
      setStatus(t("feedback.message43", { v0: String(error) }));
    }
  };
  const openPdfExternal = async () => {
    if (!selectedDoc?.pdfPath || !isTauri) return;
    try {
      await desktopClient.command("open_pdf_external", {
        path: selectedDoc.pdfPath,
      });
    } catch (error) {
      setStatus(t("feedback.message44", { v0: String(error) }));
    }
  };

  const exportReadingBundle = async () => {
    if (!selectedDoc) return;
    setStatus(t("app.exporting"));
    try {
      const path = await desktopClient.command<string>("export_reading_bundle", {
        revisionId: selectedDoc.revisionId,
      });
      setStatus(t("app.exported", { path }));
    } catch (error) {
      setStatus(t("app.exportFailed", { error: String(error) }));
    }
  };

  const openWorkspaceFolder = async () => {
    if (!workspace) {
      setStatus(t("feedback.message45"));
      return;
    }
    try {
      await desktopClient.command("open_workspace_dir");
      setStatus(t("feedback.message46"));
    } catch (error) {
      setStatus(t("feedback.message47", { v0: String(error) }));
    }
  };

  const openOperations = () => {
    setShowOperations(true);
    void refreshOperations();
  };

  const controlJob = async (
    command: "pause_job" | "resume_job" | "cancel_job",
    jobId: string,
  ) => {
    setOperationBusyId(jobId);
    try {
      await desktopClient.command(command, { jobId });
      await refreshOperations();
      setStatus(t("feedback.message48", { v0: command.replace("_job", ""), v1: jobId.slice(0, 8) }));
    } catch (error) {
      setStatus(t("feedback.message49", { v0: String(error) }));
    } finally {
      setOperationBusyId("");
    }
  };

  const reprioritizeJob = async (jobId: string, priority: number) => {
    setOperationBusyId(jobId);
    try {
      await desktopClient.command("reprioritize_job", { jobId, priority });
      await refreshOperations();
      setStatus(t("feedback.message50", { v0: priority }));
    } catch (error) {
      setStatus(t("feedback.message51", { v0: String(error) }));
    } finally {
      setOperationBusyId("");
    }
  };

  const retryRemoteCleanup = async (tombstoneId: string) => {
    setOperationBusyId(tombstoneId);
    try {
      await desktopClient.command<number>("retry_remote_cleanup", {
        tombstoneId,
      });
      await refreshOperations();
      setStatus(t("feedback.message52"));
    } catch (error) {
      setStatus("Remote cleanup failed · " + String(error));
    } finally {
      setOperationBusyId("");
    }
  };

  const abandonRemoteCleanup = async (tombstoneId: string, confirmed: boolean) => {
    setOperationBusyId(tombstoneId);
    try {
      const abandoned = await desktopClient.command<boolean>("abandon_remote_cleanup", {
        tombstoneId,
        confirmed,
      });
      await refreshOperations();
      setStatus(
        abandoned
          ? t("app.cleanupAbandoned")
          : t("app.cleanupUnchanged"),
      );
    } catch (error) {
      setStatus(t("app.cleanupAbandonFailed", { error: String(error) }));
    } finally {
      setOperationBusyId("");
    }
  };

  const recheckProviderJob = async (jobId: string) => {
    setOperationBusyId(jobId);
    try {
      await desktopClient.command<JobProjection>("recheck_provider_job", { jobId });
      setOperationFocusJobId(jobId);
      await refreshOperations();
      setStatus(t("app.providerRechecked"));
    } catch (error) {
      setStatus(t("app.providerStillNeeds", { error: String(error) }));
    } finally {
      setOperationBusyId("");
    }
  };

  const rebindProviderJob = async (jobId: string, replacementInstanceId: string) => {
    setOperationBusyId(jobId);
    try {
      const outcome = await desktopClient.command<{
        job: JobProjection | null;
        existingJobId: string | null;
      }>("rebind_provider_job", { jobId, replacementInstanceId });
      const focusId = outcome.existingJobId ?? outcome.job?.id ?? jobId;
      setOperationFocusJobId(focusId);
      setShowOperations(true);
      await refreshOperations();
      setStatus(
        outcome.existingJobId
          ? t("app.equivalentJobExists")
          : t("app.boundMatchingProvider"),
      );
    } catch (error) {
      setStatus(t("app.bindProviderFailed", { error: String(error) }));
    } finally {
      setOperationBusyId("");
    }
  };

  const abandonLegacyProviderJob = async (
    jobId: string,
    confirmedPotentialCharge: boolean,
  ) => {
    setOperationBusyId(jobId);
    try {
      await desktopClient.command<JobProjection>("abandon_legacy_provider_job", {
        jobId,
        confirmedPotentialCharge,
      });
      setOperationFocusJobId(jobId);
      await refreshOperations();
      setStatus(t("app.abandonedHistoryJob"));
    } catch (error) {
      setStatus(t("app.abandonHistoryFailed", { error: String(error) }));
    } finally {
      setOperationBusyId("");
    }
  };
  const previewDiagnostics = async () => {
    setOperationBusyId("diagnostics");
    try {
      const preview = await desktopClient.open<DiagnosticPreview>(
        "preview_diagnostics",
      );
      setDiagnosticPreview(preview);
      setStatus(t("feedback.message53"));
    } catch (error) {
      setStatus(t("feedback.message54", { v0: String(error) }));
    } finally {
      setOperationBusyId("");
    }
  };

  const exportDiagnostics = async () => {
    const outputPath = await save({
      title: "Export redacted Read Atlas diagnostics",
      defaultPath: "read-desktop-diagnostics.json",
      filters: [{ name: "JSON diagnostics", extensions: ["json"] }],
    });
    if (!outputPath) return;
    setOperationBusyId("diagnostics");
    try {
      const preview = await desktopClient.command<DiagnosticPreview>(
        "export_diagnostics",
        { outputPath },
      );
      setDiagnosticPreview(preview);
      setStatus(t("feedback.message55"));
    } catch (error) {
      setStatus(t("feedback.message56", { v0: String(error) }));
    } finally {
      setOperationBusyId("");
    }
  };

  const loadReaderContext = useCallback(async (target: ReaderContextTarget) => {
    return desktopClient.command<ReaderContextProjection>("get_reader_context", {
      request: {
        scope: target.scope,
        collectionPath: target.collectionPath,
        paperId: target.paperId,
      },
    });
  }, []);

  const saveReaderContext = useCallback(
    async (target: ReaderContextTarget, text: string) => {
      await desktopClient.command("save_reader_context", {
        request: {
          scope: target.scope,
          collectionPath: target.collectionPath,
          paperId: target.paperId,
          text,
        },
      });
    },
    [],
  );

  const restoreReaderFolder = async (id: string) => {
    setOperationBusyId(id);
    try {
      await desktopClient.command("restore_reader_folder_context", { request: { id } });
      await Promise.all([refreshDocuments(), refreshOperations()]);
      setStatus(t("app.restoredFolderContext"));
    } catch (error) {
      setStatus(t("feedback.message57", { v0: String(error) }));
    } finally {
      setOperationBusyId("");
    }
  };

  const restoreFromTrash = async (paperId: string) => {
    setOperationBusyId(paperId);
    try {
      const restored = await desktopClient.command<DocumentCard>(
        "restore_paper",
        { paperId },
      );
      await Promise.all([refreshDocuments(), refreshOperations()]);
      setSelectedRevisionId(restored.revisionId);
      setStatus(t("feedback.message58", { v0: restored.title }));
    } catch (error) {
      setStatus(t("feedback.message57", { v0: String(error) }));
    } finally {
      setOperationBusyId("");
    }
  };

  const handleConfirmTrash = async () => {
    if (!paperToTrash) return;
    const targetDoc = paperToTrash;
    setBusy(true);
    try {
      const result = await desktopClient.command<{
        deleted: boolean;
        cleanupWarning: string | null;
      }>("delete_revision", {
        revisionId: targetDoc.revisionId,
      });
      const remaining = documents.filter(
        (document) => document.id !== targetDoc.id,
      );
      if (selectedDoc?.id === targetDoc.id) {
        setSelectedRevisionId(remaining[0]?.revisionId ?? "");
      }
      setPaperToTrash(null);
      await Promise.all([refreshDocuments(), refreshOperations()]);
      setStatus(
        result.cleanupWarning
          ? t("app.trashedWithWarning", { title: targetDoc.title })
          : t("app.trashedTitle", { title: targetDoc.title }),
      );
    } catch (error) {
      setStatus(t("app.trashFailed", { error: String(error) }));
    } finally {
      setBusy(false);
    }
  };
  const resolveExitChoice = async (
    mode: "continue_in_tray" | "pause_and_exit" | "cancel_and_exit",
  ) => {
    setExitChoiceBusy(true);
    try {
      await desktopClient.command("resolve_exit_intent", { mode });
      if (mode === "continue_in_tray") {
        setExitChoiceOpen(false);
        setExitChoiceBusy(false);
        setStatus(t("feedback.message59"));
      }
    } catch (error) {
      setStatus("Exit action failed · " + String(error));
      setExitChoiceBusy(false);
    }
  };

  if (uiLocale === undefined) {
    return null;
  }
  if (uiLocale === null) {
    return (
      <LanguageGate
        onPick={pickUiLocale}
        error={localeError}
        busy={localeBusy}
      />
    );
  }

  if (viewMode === "library") {
    return (
      <LocaleProvider locale={uiLocale}>
      <div className="app-shell" data-theme={themeMode}>
        <LibraryHub
          documents={documents}
          collections={collections}
          workspace={workspace}
          selectedFolder={hubSelectedFolder}
          replayTourSignal={hubTourReplayNonce}
          onSelectFolder={setHubSelectedFolder}
          onSelectPaper={(revisionId, paperId) => {
            setViewMode("reader");
            void loadDocument(revisionId, paperId);
          }}
          onImportPdf={importPdf}
          libraryClient={libraryWorkspaceClient}
          onLibraryChanged={() => void Promise.all([refreshDocuments(), refreshStats()])}
          onImportPaths={importPdfPaths}
          onChooseWorkspace={chooseWorkspace}
          onOpenSettings={openSettings}
          onOpenOperations={openOperations}
          libraryNotice={libraryNotice}
          onDismissNotice={() => setLibraryNotice(null)}
          themeMode={themeMode}
          onToggleTheme={setThemeMode}
          importBusy={busy}
          onUpdateTags={handleUpdateTags}
          onCreateFolder={handleCreateFolder}
          onRenameFolder={handleRenameFolder}
          onMoveFolder={handleMoveFolder}
          onRenamePaper={handleRenamePaper}
          onMovePaper={handleMovePaper}
          onTrashPaper={handleTrashPaper}
          onTrashFolder={handleTrashFolder}
          onEditReaderContext={setReaderContextTarget}
          onOpenResource={handleOpenResource}
          onOpenWorkspace={handleOpenWorkspace}
          onSetSortMode={handleSetSortMode}
          onReorderPapers={handleReorderPapers}
        />
        {importPlan ? (
          <HubBatchConfirmDialog
            plan={importPlan}
            busy={importPlanBusy}
            onConfirm={async (ids) => {
              setImportPlanBusy(true);
              try {
                const started = await libraryWorkspaceClient.act(startBatchRequest(importPlan, ids));
                setImportPlan(null);
                await refreshDocuments();
                await refreshStats();
                announceBatch(batchOf(started), undoTokenOf(started));
              } catch (error) {
                notifyError(t("app.importFailed"), toLibraryActError(error).message, { source: "import" });
              } finally {
                setImportPlanBusy(false);
                setBusy(false);
              }
            }}
            onClose={() => { if (!importPlanBusy) { setImportPlan(null); setBusy(false); } }}
          />
        ) : null}
        <OperationsDrawer
          open={showOperations}
          jobs={jobs}
          batches={libraryBatches}
          storage={storageReport}
          diagnostics={diagnosticPreview}
          trash={trash}
          remoteTombstones={remoteTombstones}
          documents={documents}
          busyJobId={operationBusyId}
          onClose={() => setShowOperations(false)}
          onRefresh={() => void refreshOperations()}
          onPause={(jobId) => void controlJob("pause_job", jobId)}
          onResume={(jobId) => void controlJob("resume_job", jobId)}
          onReprioritize={(jobId, priority) =>
            void reprioritizeJob(jobId, priority)
          }
          onCancel={(jobId) => void controlJob("cancel_job", jobId)}
          onRestore={(paperId) => void restoreFromTrash(paperId)}
          onRestoreReaderFolder={(id) => void restoreReaderFolder(id)}
          onRetryRemote={(tombstoneId) => void retryRemoteCleanup(tombstoneId)}
          onPreviewDiagnostics={() => void previewDiagnostics()}
          onExportDiagnostics={() => void exportDiagnostics()}
          providerInstances={modelSettings.providers}
          onOpenProviderSettings={openProviderSettingsForRecovery}
          onRecheckProvider={(jobId) => void recheckProviderJob(jobId)}
          onRebindProvider={(jobId, providerInstanceId) =>
            void rebindProviderJob(jobId, providerInstanceId)
          }
          onAbandonLegacyProviderJob={(jobId, confirmedPotentialCharge) =>
            void abandonLegacyProviderJob(jobId, confirmedPotentialCharge)
          }
          onAbandonRemoteCleanup={(tombstoneId, confirmed) =>
            void abandonRemoteCleanup(tombstoneId, confirmed)
          }
          focusJobId={operationFocusJobId}
          onRetryBatch={retryLibraryBatch}
          onCancelBatch={cancelLibraryBatch}
          onUndoBatch={undoLibraryBatch}
          onRetryJob={(jobId, confirmed) => {
            void (async () => {
              try {
                setOperationBusyId(jobId);
                await desktopClient.command("retry_job", { request: { jobId, confirmedPotentialCharge: confirmed } });
                await refreshJobs();
                notifySuccess(t("app.retried"), t("app.jobRequeued", { id: jobId.slice(0, 8) }), { source: "job" });
              } catch (e) {
                notifyError(t("app.retryFailed"), String(e), { source: "job" });
              } finally {
                setOperationBusyId("");
              }
            })();
          }}
          onOpenSource={(paperId, revisionId) => {
            setShowOperations(false);
            if (revisionId) {
              setViewMode("reader");
              void loadDocument(revisionId, paperId ?? undefined);
            } else if (paperId) {
              const doc = documents.find((d) => d.id === paperId);
              if (doc) {
                setViewMode("reader");
                void loadDocument(doc.revisionId, doc.id);
              }
            }
          }}
        />
        <SettingsWorkbench
          open={showSettings}
          initialSection={settingsInitialSection}
          initialProviderId={settingsInitialProviderId}
          focusNonce={settingsFocusNonce}
          workspace={workspace}
          modelSettings={modelSettings}
          workspaceBusy={busy}
          onClose={() => setShowSettings(false)}
          onChooseWorkspace={chooseWorkspace}
          onResetWorkspace={resetWorkspace}
          onOpenWorkspace={openWorkspaceFolder}
          onModelSettingsChange={(next) => {
            setModelSettings(next);
            setStatus(paperProviderConnectionStatus(next, t));
          }}
          locale={uiLocale}
          onRequestLocaleChange={(next) => {
            if (next === uiLocale) return;
            setPendingLocale(next);
          }}
          onReplayHubTour={() => {
            setShowSettings(false);
            setHubTourReplayNonce((nonce) => nonce + 1);
          }}
        />
        <LanguageChangeConfirmModal
          open={Boolean(pendingLocale)}
          next={pendingLocale ?? "zh-CN"}
          busy={localeChangeBusy}
          onConfirm={() => void confirmLocaleChange()}
          onCancel={() => setPendingLocale(null)}
        />
        <TrashConfirmModal
          open={Boolean(paperToTrash)}
          paperTitle={paperToTrash?.title ?? ""}
          paperPath={paperToTrash?.collection ? `${paperToTrash.collection}/${paperToTrash.title}.pdf` : undefined}
          busy={busy}
          onConfirm={() => void handleConfirmTrash()}
          onCancel={() => setPaperToTrash(null)}
        />
        <TrashConfirmModal
          open={Boolean(folderToTrash)}
          paperTitle={folderToTrash?.relativePath ?? ""}
          busy={busy}
          documentCount={folderToTrash?.count}
          isFolder={true}
          onConfirm={() => void handleConfirmFolderTrash()}
          onCancel={() => setFolderToTrash(null)}
        />
        {exitChoiceOpen ? (
          <div className="scrim exit-choice-scrim">
            <section
              className="exit-choice"
              role="dialog"
              aria-modal="true"
              aria-labelledby="exit-choice-title"
            >
              <span className="eyebrow">{t("completion.active_work_safe_exit")}</span>
              <h2 id="exit-choice-title">{t("completion.tasks_are_still_in_progress")}</h2>
              <p>{t("completion.choose_whether_read_desktop_should_keep_working_preserve_recoverable_queue_state_or_cancel_active_work_before_exiting")}</p>
              <div className="exit-work-summary">
                <span>
                  <b>{exitWorkSummary.running}</b>{t("completion.running")}</span>
                <span>
                  <b>{exitWorkSummary.queued}</b>{t("completion.queued")}</span>
                <span>
                  <b>{exitWorkSummary.paused}</b>{t("completion.paused")}</span>
                <span>
                  <b>{exitWorkSummary.streamingDiscussions}</b>{t("completion.visible_streaming_chat")}</span>
              </div>
              <div className="exit-choice-options">
                <button
                  type="button"
                  disabled={exitChoiceBusy}
                  onClick={() => void resolveExitChoice("continue_in_tray")}
                >
                  <strong>{t("completion.continue_in_tray")}</strong>
                  <small>{t("completion.hide_the_window_and_let_current_work_finish")}</small>
                </button>
                <button
                  type="button"
                  disabled={exitChoiceBusy}
                  onClick={() => void resolveExitChoice("pause_and_exit")}
                >
                  <strong>{t("completion.pause_and_exit")}</strong>
                  <small>{t("completion.pause_safe_queue_work_paid_requests_with_unknown_outcomes_become_interrupted_unknown_and_are_never_retried_silently")}</small>
                </button>
                <button
                  type="button"
                  className="danger"
                  disabled={exitChoiceBusy}
                  onClick={() => void resolveExitChoice("cancel_and_exit")}
                >
                  <strong>{t("completion.cancel_and_exit")}</strong>
                  <small>{t("completion.keep_partial_chat_text_but_cancel_active_tasks")}</small>
                </button>
              </div>
              <button
                type="button"
                className="exit-choice-dismiss"
                disabled={exitChoiceBusy}
                onClick={() => setExitChoiceOpen(false)}
              >{t("completion.keep_the_window_open")}</button>
            </section>
          </div>
        ) : null}
        {readerContextTarget ? (
          <ReaderContextDialog
            target={readerContextTarget}
            load={loadReaderContext}
            save={saveReaderContext}
            onClose={() => setReaderContextTarget(null)}
          />
        ) : null}
      </div>
      </LocaleProvider>
    );
  }

  return (
    <LocaleProvider locale={uiLocale}>
    <div className="app-shell" data-theme={themeMode}>
      <header className="reader-floating-island">
        {/* Left: Document Navigation & Title */}
        <div className="topbar-left-zone">
          <button
            className="btn-liquid-pill"
            style={{ padding: "4px 10px", fontSize: 11.5 }}
            onClick={() => setViewMode("library")}
            title={t("app.backToHubTitle")}
          >
            ‹ {t("app.backToHub")}
          </button>
          <div
            className="topbar-paper-title"
            title={selectedDoc?.title}
          >
            {selectedDoc?.title ?? "Untitled Paper"}
          </div>
          <div className="liquid-pill-group reader-view-controls" style={{ padding: "2px", gap: 2 }} role="group" aria-label={t("completion.page_and_zoom")}>
            <button
              className="liquid-tab-btn reader-fit-control"
              style={{ padding: "3px 7px", fontSize: 11 }}
              onClick={() => setPage((p) => Math.max(1, p - 1))}
              disabled={page <= 1}
              aria-label={t("completion.previous_page")}
              title={t("app.prevPageTitle")}
            >
              ‹
            </button>
            <span className="page-input" style={{ display: "flex", alignItems: "center", gap: 2 }}>
              <input
                aria-label={t("completion.page_number")}
                value={pageDraft}
                aria-invalid={pageError ? "true" : undefined}
                onChange={(e) => {
                  const v = e.target.value;
                  // allow empty draft
                  if (v === "") { setPageDraft(""); setPageError(null); return; }
                  if (!/^\d*$/.test(v)) { setPageError(t("app.pageDigitsOnly")); return; }
                  setPageDraft(v);
                  setPageError(null);
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    const raw = pageDraft.trim();
                    if (raw === "") { setPageError(t("app.pageRequired")); return; }
                    const n = Number(raw);
                    if (!Number.isFinite(n) || n < 1 || (pdfPageCount && n > pdfPageCount)) {
                      setPageError(t("app.pageRange", { max: pdfPageCount || "?" }));
                      return;
                    }
                    setPage(Math.round(n));
                    setPageError(null);
                  } else if (e.key === "Escape") {
                    setPageDraft(String(page));
                    setPageError(null);
                    (e.target as HTMLInputElement).blur();
                  }
                }}
                onBlur={() => {
                  const raw = pageDraft.trim();
                  if (raw === "") { setPageDraft(String(page)); setPageError(null); return; }
                  const n = Number(raw);
                  if (!Number.isFinite(n) || n < 1 || (pdfPageCount && n > pdfPageCount)) {
                    setPageError(t("app.pageRange", { max: pdfPageCount || "?" }));
                    setPageDraft(String(page));
                    return;
                  }
                  setPage(Math.round(n));
                  setPageError(null);
                }}
                style={{ width: 32, textAlign: "center", border: pageError ? "1px solid #dc2626" : 0, background: "transparent", borderRadius: 4 }}
              />
              <span style={{ fontSize: 10, color: "var(--muted)" }}>/ {pdfPageCount || "—"}</span>
            </span>
            <button
              className="liquid-tab-btn reader-fit-control"
              style={{ padding: "3px 7px", fontSize: 11 }}
              onClick={() => setPage((p) => Math.min(pdfPageCount || p + 1, p + 1))}
              disabled={pdfPageCount ? page >= pdfPageCount : false}
              aria-label={t("completion.next_page")}
              title={t("app.nextPageTitle")}
            >
              ›
            </button>
            {pageError && <span style={{ fontSize: 10, color: "#dc2626", marginLeft: 4 }} role="alert">{pageError}</span>}
            <span style={{ width: 1, height: 14, background: "var(--line)", margin: "0 2px" }} aria-hidden />
            <button
              className="liquid-tab-btn reader-fit-control"
              style={{ padding: "3px 7px", fontSize: 11 }}
              onClick={() => {
                setReaderFitMode(null);
                setZoom((z) => clampReaderZoom(z - 10));
              }}
              title={t("app.zoomOutTitle")}
              aria-label={t("completion.zoom_out")}
            >
              −
            </button>
            <span style={{ fontSize: 11, padding: "0 2px", alignSelf: "center", color: "var(--muted)", minWidth: 34, textAlign: "center" }}>
              {zoom}%
            </span>
            <button
              className="liquid-tab-btn"
              style={{ padding: "3px 7px", fontSize: 11 }}
              onClick={() => {
                setReaderFitMode(null);
                setZoom((z) => clampReaderZoom(z + 10));
              }}
              title={t("app.zoomInTitle")}
              aria-label={t("completion.zoom_in")}
            >
              ＋
            </button>
            <button
              className="liquid-tab-btn"
              style={{ padding: "3px 6px", fontSize: 10 }}
              onClick={() => {
                setReaderFitMode("width");
                void applyReaderFit("width");
              }}
              title={t("app.fitWidthTitle")}
              aria-label={t("completion.fit_width")}
            >
              {t("app.fitWidth")}
            </button>
            <button
              className="liquid-tab-btn"
              style={{ padding: "3px 6px", fontSize: 10 }}
              onClick={() => {
                setReaderFitMode("page");
                void applyReaderFit("page");
              }}
              title={t("app.fitPageTitle")}
              aria-label={t("completion.fit_page")}
            >
              {t("app.fitPage")}
            </button>
            <button
              className="liquid-tab-btn"
              style={{ padding: "3px 6px", fontSize: 11 }}
              onClick={() =>
                setRotation((current) =>
                  normalizeReaderRotation(current + 90),
                )
              }
              title={t("app.rotateTitle")}
              aria-label={t("completion.rotate")}
            >
              ↻
            </button>
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
            <button
              className="btn-liquid-pill"
              style={{ padding: "4px 10px", fontSize: 11.5, whiteSpace: "nowrap", flexShrink: 0 }}
              disabled={!selectedDoc || Boolean(activeOcrJob)}
              onClick={() => void startOcr()}
            >
              {activeOcrJob
                ? "OCR · " + activeOcrJob.stage
                : ocr
                  ? t("app.ocrReady")
                  : t("app.ocrParse")}
            </button>
            {ocr && !activeOcrJob ? (
              <button
                type="button"
                className="liquid-icon-btn"
                style={{
                  width: 24,
                  height: 24,
                  padding: 0,
                  fontSize: 11,
                  display: "inline-flex",
                  alignItems: "center",
                  justifyContent: "center",
                  borderRadius: "50%",
                  color: "var(--text-muted, #94a3b8)",
                  background: "rgba(0,0,0,0.04)",
                  border: "1px solid rgba(255,255,255,0.15)",
                }}
                onClick={() => setShowOcrDeleteModal(true)}
                title={t("app.deleteOcrTitle")}
                aria-label={t("app.deleteOcr")}
              >
                🗑️
              </button>
            ) : null}
          </div>
        </div>

        {/* Center: AI Reading Mode Segmented Capsule */}
        <div className="liquid-pill-group topbar-mode-group">
          <button
            className={`liquid-tab-btn ${rightTab === "artifacts" && preferBrief ? "active" : ""}`}
            style={{ padding: "4px 11px", fontSize: 11.5 }}
            onClick={openBrief}
            title={t("app.briefTitle")}
          >
            ✦ {t("app.brief")}
          </button>
          <button
            className={`liquid-tab-btn ${layoutShowsOutline(workspaceLayout) ? "active" : ""} ${activeOutlineJob ? "is-busy" : ""}`}
            style={{ padding: "4px 11px", fontSize: 11.5 }}
            onClick={() => void openOutline()}
            title={
              activeOutlineJob
                ? t("app.outlineGenerating", { stage: outlineProgressCopy(activeOutlineJob, t).stage })
                : t("app.openOutline")
            }
          >
            {activeOutlineJob
              ? `🗺 ${outlineProgressCopy(activeOutlineJob, t).step}`
              : t("app.map")}
          </button>
          <button
            className={`liquid-tab-btn ${roadmapOpen ? "active" : ""} ${activeRoadmapJob ? "is-busy" : ""}`}
            style={{ padding: "4px 11px", fontSize: 11.5 }}
            onClick={() => setRoadmapOpen((prev) => !prev)}
            title={
              activeRoadmapJob
                ? t("app.roadmapGenerating")
                : t("app.roadmapToggle")
            }
          >
            {activeRoadmapJob ? t("app.roadmapBusy") : t("app.roadmap")}
          </button>
          <GuideIsland
            projection={guideProjection}
            plan={guidePlan}
            layerVisible={guideLayerVisible}
            busy={Boolean(activeGuideJob)}
            confirming={guideConfirming}
            error={guideError}
            characterSettings={guideCharacterSettings}
            selectedCharacterIds={guideCastIds}
            suspended={showSettings}
            planning={guidePanel.planning}
            canGenerate={guidePanel.canStart}
            onCastChange={(ids) => void guidePanel.changeCast(ids)}
            onOpenCharacterSettings={() => openSettings("characters")}
            onOpen={() => void openGuide()}
            onConfirmGenerate={() => void startGuide()}
            onCancelConfirm={guidePanel.cancel}
            onToggleLayer={() => setGuideLayerVisible((visible) => !visible)}
            onRegenerate={() => void planGuide()}
            onDelete={() => void deleteGuide()}
          />
        </div>

        {/* Right: View Toggle, Compact Health & More Actions */}
        <div className="topbar-right-zone">
          <button
            className={`btn-liquid-pill ${isPdfOnly ? "primary" : ""}`}
            style={{ padding: "4px 10px", fontSize: 11.5 }}
            onClick={() => setIsPdfOnly((prev) => !prev)}
            title={t("app.pdfOnlyTitle")}
          >
            {isPdfOnly ? t("app.expandDiscussion") : t("app.pdfOnly")}
          </button>
          <button
            className={`btn-liquid-pill ${showArtifactAnnotations ? "primary" : ""}`}
            style={{ padding: "4px 10px", fontSize: 11.5 }}
            onClick={() => {
              setIsPdfOnly(false);
              setWorkspaceLayout(
                applyWorkspaceLayoutIntent(workspaceLayout, "open_artifacts"),
              );
              setRightTab("artifacts");
              setArtifactScope("global");
              setShowArtifactAnnotations(true);
            }}
            aria-pressed={showArtifactAnnotations}
            title={t("app.annotationsTitle")}
          >
            ▤ {t("app.annotations")} {annotations.length > 0 ? annotations.length : ""}
          </button>
          <button
            className="btn-liquid-pill"
            style={{ padding: "4px 10px", fontSize: 11.5 }}
            onClick={goBackReader}
            disabled={!canGoBackReader(readerHistoryRef.current)}
            title={t("app.readerBackTitle")}
            aria-label={t("app.readerBackTitle")}
          >
            ‹ {t("app.back")}
          </button>

          <button
            className="btn-liquid-pill reader-export-control"
            style={{ padding: "4px 10px", fontSize: 11.5, whiteSpace: "nowrap", flexShrink: 0 }}
            disabled={!selectedDoc}
            onClick={() => void exportReadingBundle()}
            title={t("app.exportTitle")}
          >
            ⬇ {t("app.export")}
          </button>

          {/* Compact Health & Token Capsule with Hover Popover - keyboard accessible */}
          <div className="topbar-compact-health-wrapper">
            <button
              type="button"
              className="topbar-compact-health"
              title={t("app.healthTitle")}
              aria-label={t("app.healthAria", { status })}
              aria-haspopup="dialog"
              style={{ cursor: "pointer" }}
            >
              <span className="status-pip" style={{ background: status.toLowerCase().includes("failed") || status.toLowerCase().includes("error") || status.includes(t("app.statusHint.failed")) ? "#dc2626" : status.toLowerCase().includes("warning") || status.includes(t("app.statusHint.requires")) ? "#d97706" : "#16a34a" }} />
              <span className="topbar-compact-token-text">
                {formatCompactTokens((stats?.inputTokens || 0) + (stats?.outputTokens || 0))}
              </span>
            </button>
            <div className="topbar-health-popover">
              <div className="health-popover-row">
                <span className="health-popover-label">{t("app.health.status")}</span>
                <span className="health-popover-val">{status}</span>
              </div>
              <div className="health-popover-divider" />
              <div className="health-popover-row">
                <span className="health-popover-label">{t("app.health.inputTokens")}</span>
                <span className="health-popover-val">{formatNumber(stats?.inputTokens, t("app.unknown"))}</span>
              </div>
              <div className="health-popover-row">
                <span className="health-popover-label">{t("app.health.cachedTokens")}</span>
                <span className="health-popover-val">{formatNumber(stats?.cachedTokens, t("app.unknown"))}</span>
              </div>
              <div className="health-popover-row">
                <span className="health-popover-label">{t("app.health.outputTokens")}</span>
                <span className="health-popover-val">{formatNumber(stats?.outputTokens, t("app.unknown"))}</span>
              </div>
            </div>
          </div>

          <button
            className="micro-action-pill"
            onClick={openOperations}
            title={
              jobs.filter((j) => j.state === "running" || j.state === "queued").length > 0
                ? t("app.operationsBusy", { count: jobs.filter((j) => j.state === "running" || j.state === "queued").length })
                : t("app.operations")
            }
            style={{ position: "relative" }}
          >
            ⚡
            {jobs.filter((j) => j.state === "running" || j.state === "queued").length > 0 ? (
              <span
                style={{
                  position: "absolute",
                  top: -1,
                  right: -1,
                  width: 7,
                  height: 7,
                  borderRadius: "50%",
                  background: "var(--blue, #3b82f6)",
                  boxShadow: "0 0 6px rgba(59, 130, 246, 0.8)",
                }}
              />
            ) : null}
          </button>

          <button
            className="micro-action-pill"
            onClick={() => openSettings()}
            title={t("app.settings")}
          >
            ⚙
          </button>

          {/* More Actions Popover */}
          <div className="topbar-more-container" ref={topbarMoreRef}>
            <button
              ref={topbarMoreButtonRef}
              className={`micro-action-pill ${topbarMoreOpen ? "active" : ""}`}
              onClick={() => setTopbarMoreOpen((o) => !o)}
              title={t("app.moreActions")}
              aria-label={t("app.moreActions")}
              aria-haspopup="menu"
              aria-expanded={topbarMoreOpen}
            >
              ···
            </button>
            {topbarMoreOpen && (
              <div className="topbar-more-popover" role="menu">
                <button
                  type="button"
                  role="menuitem"
                  className="topbar-more-item reader-view-menu-item"
                  onClick={() => {
                    closeTopbarMore();
                    setReaderFitMode("width");
                    void applyReaderFit("width");
                  }}
                >
                  <span aria-hidden>↔</span> {t("app.fitWidthMenu")}
                </button>
                <button
                  type="button"
                  role="menuitem"
                  className="topbar-more-item reader-view-menu-item"
                  onClick={() => {
                    closeTopbarMore();
                    setReaderFitMode("page");
                    void applyReaderFit("page");
                  }}
                >
                  <span aria-hidden>▣</span> {t("app.fitPageMenu")}
                </button>
                <button
                  type="button"
                  role="menuitem"
                  className="topbar-more-item reader-view-menu-item"
                  onClick={() => {
                    closeTopbarMore();
                    setRotation((current) =>
                      normalizeReaderRotation(current + 90),
                    );
                  }}
                >
                  <span aria-hidden>↻</span> {t("app.rotateMenu")}
                </button>
                <button
                  type="button"
                  role="menuitem"
                  className="topbar-more-item reader-export-menu-item"
                  disabled={!selectedDoc}
                  onClick={() => {
                    closeTopbarMore();
                    void exportReadingBundle();
                  }}
                >
                  <span aria-hidden>⬇</span> {t("app.exportMenu")}
                </button>
                <button
                  type="button"
                  role="menuitem"
                  className="topbar-more-item"
                  onClick={() => {
                    closeTopbarMore();
                    openFolder();
                  }}
                  title={t("app.openSourceTitle")}
                >
                  <span>↗</span> {t("app.openSource")}
                </button>
                <button
                  type="button"
                  role="menuitem"
                  className="topbar-more-item danger"
                  disabled={!selectedDoc || busy}
                  onClick={() => {
                    closeTopbarMore();
                    if (selectedDoc) setPaperToTrash(selectedDoc);
                  }}
                  title={t("app.trashMenuTitle")}
                >
                  <span>🗑️</span> {t("app.trashMenu")}
                </button>
              </div>
            )}
          </div>
        </div>
      </header>
      <main
        className="workspace-grid"
        data-discussion-font={discussionFontSize}
        data-workspace-layout={workspaceLayout}
        style={{
          ["--discussion-font-size" as string]: `${discussionFontSize}px`,
          ["--chat-width" as string]:
            isPdfOnly && workspaceLayout === "pdf_discussion"
              ? "0px"
              : `${chatWidth}px`,
          gridTemplateColumns:
            !layoutShowsPdf(workspaceLayout) ||
            (isPdfOnly && workspaceLayout === "pdf_discussion")
              ? "1fr"
              : `1fr 8px var(--chat-width)`,
          padding: "8px 14px 10px 14px",
        }}
      >
        <section
          className="reader-panel panel"
          style={{
            display: layoutShowsPdf(workspaceLayout) ? "flex" : "none",
          }}
        >
          <div className="reader-stage">
            <div className="reader-canvas-wrap">
            {pdfReaderSrc ? (
              <Suspense
                fallback={
                  <div className="reader-loading">{t("completion.loading_pdf_renderer")}</div>
                }
              >
                <PdfReader
                  key={selectedDoc?.revisionId ?? pdfReaderSrc}
                  source={pdfReaderSrc}
                  currentPage={page}
                  zoom={zoom}
                  rotation={rotation}
                  onOpenExternal={() => void openPdfExternal()}
                  ocrBlocks={ocr?.blocks}
                  quotedBlockIds={quoteBasket.map((quote) => quote.blockId)}
                  cachedBlockIds={artifacts
                    .flatMap((a) => a.evidence.map((e) => e.blockId))
                    .filter((id): id is string => Boolean(id))}
                  focusedBlockId={focusedBlockId}
                  onToggleQuote={toggleBlockQuote}
                  onSelectBlock={(block) =>
                    setFocusedBlockId((current) =>
                      current === block.id ? null : block.id,
                    )
                  }
                  onClearBlockSelection={() => setFocusedBlockId(null)}
                  onBlockAction={handleBlockAction}
                  highlightedBlockIds={annotationBlockIds.highlighted}
                  bookmarkedBlockIds={annotationBlockIds.bookmarked}
                  notedBlockIds={annotationBlockIds.noted}
                  onAnnotationAction={handleAnnotationAction}
                  onPageCount={setPdfPageCount}
                  onVisiblePage={setPage}
                  onScrollOffset={(offset) => {
                    setPageOffset(offset);
                    setRestoreOffset(offset);
                  }}
                  initialScrollOffset={pageOffset || restoreOffset}
                  onZoomChange={(nextZoom) => {
                    setReaderFitMode(null);
                    setZoom(clampReaderZoom(nextZoom));
                  }}
                  onDocumentReady={setPdfDocument}
                  guideLayerVisible={
                    guideLayerVisible && Boolean(guideProjection?.head)
                  }
                  guideInks={locateGuideInks(
                    normalizeGuideInks(guideProjection?.head?.inks),
                    ocr?.blocks ?? [],
                  )}
                  guideCastSnapshot={guideProjection?.head?.castSnapshot ?? null}
                  workspaceRoot={workspace?.rootPath ?? null}
                  activeGuideInkId={activeGuideInkId}
                  onActivateGuideInk={setActiveGuideInkId}
                />
              </Suspense>
            ) : (
              <div className="paper-preview">
                <div className="paper-sheet">
                  <div className="paper-running">
                    NEURAL INFORMATION PROCESSING SYSTEMS · 2017
                  </div>
                  <div className="paper-number">01</div>
                  <div className="paper-rule" />
                  <h2>{selectedDoc?.title ?? "Your reading desk"}</h2>
                  <p className="paper-authors">
                    {selectedDoc?.authors ?? "Import a paper to begin"}
                  </p>
                  <p className="paper-abstract">
                    {selectedDoc
                      ? "The native PDF reader will render your hosted source here. Use the page controls to keep the active citation close to the conversation."
                      : "A calm, local-first space for papers, questions, and evidence. Your files stay in the Workspace you choose."}
                  </p>
                  <div className="paper-placeholder-lines">
                    <span />
                    <span />
                    <span />
                    <span />
                    <span />
                  </div>
                  <div className="paper-footer">
                    <span>READ ATLAS</span>
                    <span>{pageCount || selectedDoc?.pages || 0}{" "}{t("completion.pages")}</span>
                  </div>
                </div>
                <div className="preview-note">
                  <span className="pulse-ring" />
                  {selectedDoc
                    ? "PDF preview · import a source to open the reader"
                    : "Start with a Workspace"}
                </div>
              </div>
            )}
            {completionPromptOpen && selectedDoc && readingLifecycle ? (
              <div className="reader-completion-prompt" role="status">
                <div>
                  <strong>{t("app.lastPageTitle")}</strong>
                  <span>{t("app.lastPageBody")}</span>
                </div>
                <div className="reader-completion-actions">
                  <button
                    type="button"
                    className="btn-liquid-pill"
                    onClick={() => {
                      setCompletionPromptOpen(false);
                      setCompletionDismissedKey(`${selectedDoc.id}:${selectedDoc.revisionId}`);
                    }}
                  >{t("app.later")}</button>
                  <button
                    type="button"
                    className="btn-liquid-pill primary"
                    onClick={() => {
                      void persistLifecyclePatch(selectedDoc.id, readingLifecycle.version, { status: "read" })
                        .then(() => {
                          setCompletionPromptOpen(false);
                          setCompletionDismissedKey(`${selectedDoc.id}:${selectedDoc.revisionId}`);
                          void refreshDocuments();
                        })
                        .catch(() => undefined);
                    }}
                  >{t("app.markRead")}</button>
                </div>
              </div>
            ) : null}
            </div>

          </div>
        </section>
        {layoutShowsOutline(workspaceLayout) ||
        (layoutShowsDiscussionRail(workspaceLayout) && !isPdfOnly) ? (
          <>
        {layoutShowsPdf(workspaceLayout) ? (
        <div
          className="workspace-split"
          role="separator"
          aria-orientation="vertical"
          aria-valuemin={300}
          aria-valuemax={760}
          aria-valuenow={chatWidth}
          aria-label={t("completion.resize_discussion_panel")}
          tabIndex={0}
          onKeyDown={(event) => {
            if (event.key === "ArrowLeft") {
              event.preventDefault();
              const step = event.shiftKey ? 40 : 10;
              const next = Math.min(760, Math.max(260, chatWidth + step));
              setChatWidth(next);
              localStorage.setItem("read-desktop.chatWidth", String(next));
            } else if (event.key === "ArrowRight") {
              event.preventDefault();
              const step = event.shiftKey ? 40 : 10;
              const next = Math.min(760, Math.max(260, chatWidth - step));
              setChatWidth(next);
              localStorage.setItem("read-desktop.chatWidth", String(next));
            } else if (event.key === "Home") {
              event.preventDefault();
              setChatWidth(400);
              localStorage.setItem("read-desktop.chatWidth", "400");
            }
          }}
          onPointerDown={(event) => {
            event.preventDefault();
            const origin = event.clientX;
            const start = chatWidth;
            const move = (next: PointerEvent) => {
              setChatWidth(
                Math.min(760, Math.max(260, start - (next.clientX - origin))),
              );
            };
            const stop = (next: PointerEvent) => {
              const width = Math.min(
                760,
                Math.max(260, start - (next.clientX - origin)),
              );
              localStorage.setItem("read-desktop.chatWidth", String(width));
              window.removeEventListener("pointermove", move);
              window.removeEventListener("pointerup", stop);
            };
            window.addEventListener("pointermove", move);
            window.addEventListener("pointerup", stop);
          }}
        />
        ) : null}
        {layoutShowsOutline(workspaceLayout) ? (
          <OutlinePane
            projection={outlineProjection}
            plan={outlinePlan}
            planError={outlinePlanError}
            layout={workspaceLayout}
            onOpenDiscussion={openDiscussionLayout}
            onOpenArtifacts={() => {
              setWorkspaceLayout(
                applyWorkspaceLayoutIntent(workspaceLayout, "open_artifacts"),
              );
              setRightTab("artifacts");
            }}
            onUseOutlineOnly={() =>
              setWorkspaceLayout(
                applyWorkspaceLayoutIntent(workspaceLayout, "outline_only"),
              )
            }
            onShowPdf={() =>
              setWorkspaceLayout(
                applyWorkspaceLayoutIntent(workspaceLayout, "show_pdf"),
              )
            }
            onGenerate={() => void startOutline()}
            generateBusy={outlineStarting || Boolean(activeOutlineJob)}
            pendingPlan={pendingOutlinePlan}
            onConfirmPlan={() => void startOutline()}
            onCancelPlan={() => setPendingOutlinePlan(null)}
            deepDiveHead={outlineView === "deep_dive" ? deepDiveHead : null}
            activeJob={activeOutlineJob}
            onRegenerateOverview={() => void planOutlineGeneration()}
            onDeleteOverview={() => void deleteOutline()}
            overviewBusy={jobs.some(
              (job) =>
                job.kind === "outline_overview" &&
                (job.state === "queued" ||
                  job.state === "running" ||
                  job.state === "paused"),
            )}
            onRegenerateDeepDive={() => void regenerateOutlineDeepDive()}
            onDeleteDeepDive={() => void deleteOutlineDeepDive()}
            selectedNodeId={
              outlineView === "deep_dive" ? deepDiveNodeId : outlineNodeId
            }
            inspectorWidth={outlineInspectorWidth}
            onInspectorWidthChange={setOutlineInspectorWidth}
            onClearSelection={() => {
              if (outlineView === "deep_dive") setDeepDiveNodeId(null);
              else {
                outlineLocalRequestId.current += 1;
                setOutlineNodeId(null);
                setDeepDiveHead(null);
              }
            }}
            onSelectNode={(node) => {
              const currentId =
                outlineView === "deep_dive" ? deepDiveNodeId : outlineNodeId;
              const plan = planOutlineNodeSelect({
                view: outlineView,
                currentId,
                clickedId: node.nodeId,
              });
              if (plan.view === "deep_dive") setDeepDiveNodeId(plan.selectedId);
              else setOutlineNodeId(plan.selectedId);
              if (plan.view !== "deep_dive") setDeepDiveHead(null);
              const requestId = ++outlineLocalRequestId.current;
              if (!plan.loadDeepDiveFor || !selectedDoc) return;
              const revisionId = selectedDoc.revisionId;
              const parentHeadId = outlineProjection?.head?.id ?? "";
              void desktopClient
                .open<OutlineHeadProjection | null>("get_outline_deep_dive", {
                  revisionId: selectedDoc.revisionId,
                  nodeId: plan.loadDeepDiveFor,
                })
                .then(head => {
                  if (requestId === outlineLocalRequestId.current && outlineScopeRef.current.revisionId === revisionId && outlineScopeRef.current.parentHeadId === parentHeadId) setDeepDiveHead(head);
                })
                .catch(() => {
                  if (requestId === outlineLocalRequestId.current) setDeepDiveHead(null);
                });
            }}
            onJumpEvidence={jumpOutlineEvidence}
            onGenerateDeepDive={(node) => void startOutlineDeepDive(node)}
            deepDiveBusy={jobs.some((job) => {
              if (job.kind !== "outline_deep_dive") return false;
              if (
                job.state !== "queued" &&
                job.state !== "running" &&
                job.state !== "paused"
              ) {
                return false;
              }
              const nodeId = (job.payload as { nodeId?: string } | null)?.nodeId;
              return !outlineNodeId || nodeId === outlineNodeId;
            })}
            hasDeepDive={Boolean(deepDiveHead?.graph)}
            deepDiveGraph={
              outlineView === "deep_dive" ? deepDiveHead?.graph ?? null : null
            }
            onBackToOverview={() => {
              setDeepDiveNodeId(null);
              setOutlineView("overview");
            }}
          />
        ) : (
        <aside className="chat-panel panel">
          <div className="compact-rail-header">
            {(() => {
              const isNarrowRail =
                rightTab === "discussion" ? chatWidth < 620 : chatWidth < 480;
              const isUltraNarrowRail = chatWidth < 360;
              return (
                <>
                  <div className="compact-rail-left">
                    <div className="compact-tab-group">
                      <button
                        type="button"
                        className={`compact-tab-item ${rightTab === "discussion" ? "active" : ""}`}
                        onClick={() => {
                          setWorkspaceLayout(
                            applyWorkspaceLayoutIntent(
                              workspaceLayout,
                              "open_discussion",
                            ),
                          );
                          setRightTab("discussion");
                        }}
                        title={t("app.discussionTitle")}
                        aria-label={t("app.discussion")}
                      >
                        💬 {isNarrowRail ? "" : t("app.discussion")}
                      </button>
                      <button
                        type="button"
                        className={`compact-tab-item ${rightTab === "artifacts" && artifactScope === "global" ? "active" : ""}`}
                        onClick={() => {
                          setWorkspaceLayout(
                            applyWorkspaceLayoutIntent(
                              workspaceLayout,
                              "open_artifacts",
                            ),
                          );
                          setRightTab("artifacts");
                          setArtifactScope("global");
                          const globalList = validArtifacts.filter(
                            (a) =>
                              !a.kind.startsWith("lens_") &&
                              a.kind !== "translation" &&
                              a.kind !== "explanation",
                          );
                          if (globalList.length > 0) {
                            const target =
                              globalList.find((a) => a.id === lastGlobalArtifactId) ??
                              globalList.find((a) => a.kind === "brief") ??
                              globalList[0];
                            if (target) {
                              void selectArtifact(target);
                            }
                          }
                        }}
                        title={t("app.globalArtifactsTitle", { count: globalArtifactCount })}
                        aria-label={t("app.globalArtifactsAria", { count: globalArtifactCount > 0 ? ` (${globalArtifactCount})` : "" })}
                      >
                        🌐 {isNarrowRail ? (globalArtifactCount > 0 ? `${globalArtifactCount}` : "") : t("app.globalArtifacts", { suffix: globalArtifactCount > 0 ? ` (${globalArtifactCount})` : "" })}
                      </button>
                      <button
                        type="button"
                        className={`compact-tab-item ${rightTab === "artifacts" && artifactScope === "block" ? "active" : ""}`}
                        onClick={() => {
                          setWorkspaceLayout(
                            applyWorkspaceLayoutIntent(
                              workspaceLayout,
                              "open_artifacts",
                            ),
                          );
                          setRightTab("artifacts");
                          setArtifactScope("block");
                          const blockList = validArtifacts.filter(
                            (a) =>
                              a.kind.startsWith("lens_") ||
                              a.kind === "translation" ||
                              a.kind === "explanation",
                          );
                          if (blockList.length > 0) {
                            const target =
                              blockList.find((a) => a.id === lastBlockArtifactId) ??
                              blockList[0];
                            if (target) {
                              void selectArtifact(target);
                            }
                          }
                        }}
                        title={t("app.lensArtifactsTitle", { count: blockArtifactCount })}
                        aria-label={t("app.lensArtifactsAria", { count: blockArtifactCount > 0 ? ` (${blockArtifactCount})` : "" })}
                      >
                        🔍 {isNarrowRail ? (blockArtifactCount > 0 ? `${blockArtifactCount}` : "") : `Lens${blockArtifactCount > 0 ? ` (${blockArtifactCount})` : ""}`}
                      </button>
                    </div>

                    {rightTab === "discussion" && activeThread && (
                      <div className="thread-title-picker-wrap" ref={threadPickerRef}>
                        {isEditingThreadTitle ? (
                          <input
                            type="text"
                            className="thread-title-inline-input"
                            value={editingThreadTitleText}
                            autoFocus
                            onChange={(e) => setEditingThreadTitleText(e.target.value)}
                            onKeyDown={(e) => {
                              if (e.key === "Enter") void saveThreadTitleInline();
                              if (e.key === "Escape") setIsEditingThreadTitle(false);
                            }}
                            onBlur={() => void saveThreadTitleInline()}
                          />
                        ) : (
                          <button
                            type="button"
                            className="thread-dropdown-btn"
                            onDoubleClick={() => {
                              setEditingThreadTitleText(activeThread.title);
                              setIsEditingThreadTitle(true);
                              setIsThreadDropdownOpen(false);
                            }}
                            onClick={() => setIsThreadDropdownOpen((prev) => !prev)}
                            title={t("app.threadSwitchTitle")}
                          >
                            <span className="thread-title-text">{activeThread.title || "Main discussion"}</span>
                            <span className="thread-dropdown-arrow">▾</span>
                          </button>
                        )}

                        {isThreadDropdownOpen && (
                          <div className="thread-dropdown-menu">
                            <div className="thread-dropdown-header">
                              <span>{t("app.threadBranches", { count: threads.length })}</span>
                              <button
                                type="button"
                                className="thread-dropdown-new-btn"
                                onClick={() => {
                                  setIsThreadDropdownOpen(false);
                                  void newThread();
                                }}
                              >
                                ＋ {t("app.new")}
                              </button>
                            </div>
                            <div className="thread-dropdown-list">
                              {threads.map((thread) => (
                                <div
                                  key={thread.id}
                                  className={`thread-dropdown-item ${thread.id === activeThreadId ? "active" : ""}`}
                                  onClick={() => {
                                    setIsThreadDropdownOpen(false);
                                    void selectThread(thread);
                                  }}
                                >
                                  <span className="thread-item-title">{thread.title}</span>
                                  <div className="thread-item-actions">
                                    <button
                                      type="button"
                                      title={t("app.archiveBranch")}
                                      onClick={(e) => {
                                        e.stopPropagation();
                                        void closeThread(thread);
                                      }}
                                    >
                                      📥
                                    </button>
                                  </div>
                                </div>
                              ))}
                            </div>
                          </div>
                        )}
                      </div>
                    )}
                  </div>

                  <div className="compact-rail-right">
                    <button
                      type="button"
                      className={`rail-model-badge ${needsPaperProvider ? "needs-model" : ""} ${isUltraNarrowRail ? "ultra-compact" : ""}`}
                      onClick={() => openSettings("models")}
                      title={
                        needsPaperProvider
                          ? paperProviderSetupTitle(currentProvider?.kind, t) + t("app.clickToConfigureParen")
                          : t("app.currentReadingModel", { model: formatModelLabel(currentProvider?.paperModel, currentProvider?.models) })
                      }
                      aria-label={
                        needsPaperProvider
                          ? t("app.configureModel")
                          : t("app.currentReadingModelAria", { model: formatModelLabel(currentProvider?.paperModel, currentProvider?.models) })
                      }
                    >
                      <span className={`rail-model-dot ${needsPaperProvider ? "warning" : ""}`} />
                      {!isUltraNarrowRail ? (
                        <span className="rail-model-name">
                          {needsPaperProvider
                            ? paperProviderComposerHint(currentProvider?.kind, t("models.clickToConfigure"), t)
                            : formatModelLabel(currentProvider?.paperModel, currentProvider?.models)}
                        </span>
                      ) : null}
                    </button>

                    {rightTab === "artifacts" && (
                      <>
                        {activeArtifact?.kind === "brief" ? (
                          <button
                            type="button"
                            className={`artifact-regenerate-brief btn-liquid-pill ${isUltraNarrowRail ? "ultra-compact" : ""}`}
                            onClick={() => void generateBrief()}
                            disabled={Boolean(activeOrientationJob)}
                            title={activeOrientationJob ? t("app.briefQueued") : t("app.regenerateBrief")}
                            aria-label={activeOrientationJob ? t("app.briefQueued") : t("app.regenerateBrief")}
                            style={{ height: 22, padding: isUltraNarrowRail ? 0 : "0 8px", fontSize: 11, whiteSpace: "nowrap" }}
                          >
                            {isUltraNarrowRail ? (activeOrientationJob ? "…" : "🔄") : (activeOrientationJob ? t("app.queued") : `🔄 ${t("app.regenerate")}`)}
                          </button>
                        ) : activeArtifact?.kind.startsWith("lens_") ? (
                          <button
                            type="button"
                            className={`artifact-regenerate-lens btn-liquid-pill ${isUltraNarrowRail ? "ultra-compact" : ""}`}
                            onClick={() => void regenerateLens(activeArtifact)}
                            title={t("app.regenerateLens")}
                            aria-label={t("app.regenerateLens")}
                            style={{ height: 22, padding: isUltraNarrowRail ? 0 : "0 8px", fontSize: 11, whiteSpace: "nowrap" }}
                          >
                            {isUltraNarrowRail ? "🔄" : `🔄 ${t("app.regenerate")}`}
                          </button>
                        ) : null}
                        {!isNarrowRail ? (
                          <>
                            <button
                              type="button"
                              className="icon-pill-btn"
                              disabled={!selectedDoc}
                              onClick={() => {
                                if (!selectedDoc) return;
                                setReaderContextTarget({
                                  scope: "paper",
                                  paperId: selectedDoc.id,
                                });
                              }}
                              title={t("app.editReaderContext")}
                              aria-label={t("app.editReaderContext")}
                            >
                              {t("app.reader")}
                            </button>
                            <FontSizeStepper
                              value={discussionFontSize}
                              onChange={setDiscussionFontSize}
                            />
                          </>
                        ) : (
                          <div className="rail-more-menu-wrap" ref={railMorePickerRef}>
                            <button
                              type="button"
                              className={`icon-pill-btn ${isRailMoreMenuOpen ? "active" : ""}`}
                              onClick={() => setIsRailMoreMenuOpen((prev) => !prev)}
                              title={t("app.moreActions")}
                              aria-label={t("app.moreActions")}
                            >
                              •••
                            </button>
                            {isRailMoreMenuOpen && (
                              <div className="rail-more-dropdown">
                                <div
                                  className="rail-more-dropdown-item"
                                  onClick={() => {
                                    if (!selectedDoc) return;
                                    setIsRailMoreMenuOpen(false);
                                    setReaderContextTarget({
                                      scope: "paper",
                                      paperId: selectedDoc.id,
                                    });
                                  }}
                                >
                                  <span>{t("app.editReaderContext")}</span>
                                </div>
                                <div className="rail-more-dropdown-row">
                                  <span className="rail-more-label">{t("app.bodyFontSize")}</span>
                                  <FontSizeStepper
                                    value={discussionFontSize}
                                    onChange={setDiscussionFontSize}
                                  />
                                </div>
                              </div>
                            )}
                          </div>
                        )}
                      </>
                    )}
                    {rightTab === "discussion" && (
                      <>
                        {!isNarrowRail ? (
                          <>
                            <button
                              type="button"
                              className="icon-pill-btn"
                              onClick={() => void newThread()}
                              title={t("app.newDiscussionBranch")}
                            >
                              ＋
                            </button>
                            <button
                              type="button"
                              className="icon-pill-btn"
                              disabled={!selectedDoc}
                              onClick={() => {
                                if (!selectedDoc) return;
                                setReaderContextTarget({
                                  scope: "paper",
                                  paperId: selectedDoc.id,
                                });
                              }}
                              title={t("app.editReaderContext")}
                              aria-label={t("app.editReaderContext")}
                            >
                              {t("app.reader")}
                            </button>
                            <FontSizeStepper
                              value={discussionFontSize}
                              onChange={setDiscussionFontSize}
                            />
                            <button
                              type="button"
                              className="icon-pill-btn"
                              onClick={() => setShowTree(true)}
                              title={t("app.fullscreenTreeTitle")}
                              aria-label={t("app.fullscreenTree")}
                            >
                              <svg
                                width="13"
                                height="13"
                                viewBox="0 0 24 24"
                                fill="none"
                                stroke="currentColor"
                                strokeWidth="2"
                                strokeLinecap="round"
                                strokeLinejoin="round"
                                style={{ display: "block" }}
                              >
                                <path d="M12 3v18" />
                                <path d="M12 8c0-2.5 3-4 6-4" />
                                <path d="M12 14c0-2.5-3-4-6-4" />
                                <circle cx="18" cy="4" r="2" fill="currentColor" />
                                <circle cx="6" cy="10" r="2" fill="currentColor" />
                                <circle cx="12" cy="21" r="2" fill="currentColor" />
                              </svg>
                            </button>
                          </>
                        ) : (
                          <div className="rail-more-menu-wrap" ref={railMorePickerRef}>
                            <button
                              type="button"
                              className={`icon-pill-btn ${isRailMoreMenuOpen ? "active" : ""}`}
                              onClick={() => setIsRailMoreMenuOpen((prev) => !prev)}
                              title={t("app.moreActions")}
                              aria-label={t("app.moreActions")}
                            >
                              •••
                            </button>
                            {isRailMoreMenuOpen && (
                              <div className="rail-more-dropdown">
                                <div
                                  className="rail-more-dropdown-item"
                                  onClick={() => {
                                    setIsRailMoreMenuOpen(false);
                                    void newThread();
                                  }}
                                >
                                  <span>＋ {t("app.newDiscussionBranch")}</span>
                                </div>
                                <div
                                  className="rail-more-dropdown-item"
                                  onClick={() => {
                                    if (!selectedDoc) return;
                                    setIsRailMoreMenuOpen(false);
                                    setReaderContextTarget({
                                      scope: "paper",
                                      paperId: selectedDoc.id,
                                    });
                                  }}
                                >
                                  <span>{t("app.editReaderContext")}</span>
                                </div>
                                <div
                                  className="rail-more-dropdown-item"
                                  onClick={() => {
                                    setIsRailMoreMenuOpen(false);
                                    setShowTree(true);
                                  }}
                                >
                                  <span>⛶ {t("app.fullscreenTree")}</span>
                                </div>
                                <div className="rail-more-dropdown-row">
                                  <span className="rail-more-label">{t("app.bodyFontSize")}</span>
                                  <FontSizeStepper
                                    value={discussionFontSize}
                                    onChange={setDiscussionFontSize}
                                  />
                                </div>
                              </div>
                            )}
                          </div>
                        )}
                      </>
                    )}
                  </div>
                </>
              );
            })()}
          </div>

          {rightTab === "discussion" ? (
            <>
              {activeOutlineJob ? (
                <div className="outline-background-banner" role="status">
                  <div>
                    <strong>{outlineProgressCopy(activeOutlineJob, t).title}</strong>
                    <span>
                      {outlineProgressCopy(activeOutlineJob, t).stage} ·{" "}
                      {outlineProgressCopy(activeOutlineJob, t).step}
                    </span>
                    <small>{outlineProgressCopy(activeOutlineJob, t).tokens}</small>
                  </div>
                  <button type="button" onClick={() => void openOutline()}>
                    {t("app.viewProgress")}
                  </button>
                </div>
              ) : null}
              <div className="chat-body-wrapper">
                <div
                  className="message-stream"
                  ref={messageStreamRef}
                  onScroll={handleMessageStreamScroll}
                  onCopy={(event) =>
                    handleMarkdownCopyEvent(event, event.currentTarget)
                  }
                >
                  {messages.length === 0 ? (
                    <div className="chat-empty">
                      <div className="chat-empty-mark">✦</div>
                      <h3>
                        {needsPaperProvider
                          ? paperProviderEmptyHeading(currentProvider?.kind, t)
                          : "Ask the paper."}
                      </h3>
                      <p>
                        {needsPaperProvider
                          ? paperProviderEmptyBody(currentProvider?.kind, t)
                          : "Questions stay in this discussion. The paper root and current branch history form the stable context."}
                    </p>
                    {needsPaperProvider ? (
                      <div className="model-setup-card">
                        <span>{t("completion.model_connection")}</span>
                        <strong>
                          {paperProviderSetupTitle(currentProvider?.kind, t)}
                        </strong>
                        <small>{t("completion.add_a_key_fetch_the_models_available_to_it_and_choose_the_default_for_this_app")}</small>
                        <button onClick={() => openSettings("models")}>{t("completion.open_model_configuration")}<em>→</em>
                        </button>
                      </div>
                    ) : (
                      <div className="suggestion-list">
                        <button
                          onClick={() =>
                            setQuestion(t("completion.what_problem_does_this_paper_solve"))
                          }
                        >{t("completion.what_problem_does_this_paper_solve")}<span>→</span>
                        </button>
                        <button
                          onClick={() =>
                            setQuestion(t("completion.walk_me_through_the_method"))
                          }
                        >{t("completion.walk_me_through_the_method")}<span>→</span>
                        </button>
                        <button
                          onClick={() =>
                            setQuestion(t("completion.what_should_i_be_skeptical_about"))
                          }
                        >{t("completion.what_should_i_be_skeptical_about")}<span>→</span>
                        </button>
                      </div>
                    )}
                  </div>
                ) : (
                  <>
                    {visibleDiscussion.length > visibleTurnLimit * 2 ? (
                      <div
                        className="discussion-load-earlier-bar"
                        style={{
                          display: "flex",
                          justifyContent: "center",
                          margin: "6px 0 14px 0",
                        }}
                      >
                        <button
                          type="button"
                          className="btn-liquid-pill load-earlier-btn"
                          style={{
                            fontSize: 11.5,
                            padding: "4px 14px",
                            background: "var(--glass-bg)",
                            border: "1px solid var(--glass-border-subtle)",
                            color: "var(--muted)",
                          }}
                          onClick={() =>
                            setVisibleTurnLimit((prev) => prev + 15)
                          }
                        >
                          {t("app.loadEarlierPrefix")}{" "}
                          {Math.ceil(
                            (visibleDiscussion.length -
                              visibleTurnLimit * 2) /
                              2,
                          )}{" "}
                          {t("app.loadEarlierSuffix")}
                        </button>
                      </div>
                    ) : null}
                    {(visibleDiscussion.length > visibleTurnLimit * 2
                      ? visibleDiscussion.slice(-visibleTurnLimit * 2)
                      : visibleDiscussion
                    ).map((node) => (
                      <MessageBubble
                        key={node.message.id}
                        message={node.message}
                        depth={node.depth}
                        paperModel={currentProvider?.paperModel ?? ""}
                        models={currentProvider?.models ?? []}
                        pdfDocument={pdfDocument}
                        rotation={rotation}
                        onBranch={branchFrom}
                        onEdit={editUserMessage}
                        onRegenerate={regenerateMessage}
                        onCancel={cancelGeneration}
                        onMarkdownCitation={jumpFromMarkdownCitation}
                        onBlockQuote={jumpToBlockQuote}
                        onEnlargeQuote={setEnlargedQuote}
                      />
                    ))}
                  </>
                )}
              </div>

              <ChatTimelineNavigator
                messages={messages}
                activeMessageId={activeScrollMessageId}
                onJumpToMessage={handleJumpToMessage}
              />
            </div>
            {isCompacting ? (
              <div
                className="discussion-compaction-pulse-indicator"
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 8,
                  padding: "6px 14px",
                  margin: "0 16px 8px 16px",
                  background: "rgba(59, 130, 246, 0.08)",
                  border: "1px solid rgba(59, 130, 246, 0.25)",
                  borderRadius: 12,
                  fontSize: 12,
                  color: "var(--primary, #3b82f6)",
                }}
              >
                <span style={{ fontSize: 14 }}>⚡</span>
                <span>
                  {t("app.compacting")}
                </span>
              </div>
            ) : null}
            <div className="composer-wrap">
                <div
                  className={`reply-context ${replyTo || editingMessageId ? "visible" : ""}`}
                >
                  <span>
                    {editingMessageId
                      ? "✎ Editing as a new branch · original stays"
                      : "↳ Replying from a branch point"}
                  </span>
                  <button
                    onClick={() => {
                      setReplyTo(null);
                      setEditingMessageId(null);
                    }}
                  >
                    ×
                  </button>
                </div>
                {quoteBasket.length > 0 ? (
                  <div className="quote-basket" aria-label={t("completion.quoted_ocr_blocks")}>
                    <div className="quote-basket-heading">
                      <span>{t("completion.quoted_blocks")}</span>
                      <small>
                        {quoteBasket.length}{t("completion.32_persisted_until_sent")}</small>
                    </div>
                    <div className="quote-basket-list">
                      {quoteBasket.map((quote, index) => {
                        const isFig = isFigureQuote(quote);
                        return (
                          <article
                            className={`quote-card ${isFig ? "is-figure" : ""}`}
                            key={quote.blockId}
                          >
                            <button
                              className="quote-card-jump"
                              type="button"
                              onClick={() => jumpToBlockQuote(quote)}
                              title={t("app.jumpToParagraph")}
                            >
                              <span>
                                p.{quote.pageNumber} · {quote.blockType} · #
                                {quote.blockIndex + 1}
                              </span>
                              {isFig ? (
                                <div className="quote-card-figure-row">
                                  <QuoteFigureThumbnail
                                    quote={quote}
                                    pdfDocument={pdfDocument}
                                    rotation={rotation}
                                    onEnlarge={() => setEnlargedQuote(quote)}
                                    className="quote-card-figure-thumb"
                                  />
                                  <small className="quote-card-figure-caption">
                                    {formatQuoteCaption(
                                      quote.textContent,
                                      `Figure p.${quote.pageNumber}`,
                                    )}
                                  </small>
                                </div>
                              ) : (
                                <small>
                                  {quote.textContent || "Non-text OCR Block"}
                                </small>
                              )}
                            </button>
                            <div className="quote-card-actions">
                              <button
                                type="button"
                                aria-label={t("feedback.message60", { v0: quote.blockIndex + 1 })}
                                disabled={index === 0}
                                onClick={() => moveBlockQuote(quote.blockId, -1)}
                              >
                                ↑
                              </button>
                              <button
                                type="button"
                                aria-label={t("feedback.message61", { v0: quote.blockIndex + 1 })}
                                disabled={index === quoteBasket.length - 1}
                                onClick={() => moveBlockQuote(quote.blockId, 1)}
                              >
                                ↓
                              </button>
                              <button
                                type="button"
                                aria-label={t("feedback.message62", { v0: quote.blockIndex + 1 })}
                                onClick={() =>
                                  setQuoteBasket((current) =>
                                    current.filter(
                                      (candidate) =>
                                        candidate.blockId !== quote.blockId,
                                    ),
                                  )
                                }
                              >
                                ×
                              </button>
                            </div>
                          </article>
                        );
                      })}
                    </div>
                  </div>
                ) : null}
                <div
                  className="composer-compact-capsule"
                  aria-label={t("composer.languageLock")}
                >
                  <textarea
                    ref={discussionTextareaRef}
                    className="composer-input-line"
                    value={question}
                    onChange={(event) => setQuestion(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter" && !event.shiftKey) {
                        event.preventDefault();
                        if (activeStreamingMessage || !question.trim() || needsPaperProvider) return;
                        void sendQuestion();
                      }
                    }}
                    placeholder={
                      needsPaperProvider
                        ? paperProviderComposerPlaceholder(
                            currentProvider?.kind, t
                          )
                        : t("app.composerPlaceholder")
                    }
                    disabled={needsPaperProvider}
                    rows={1}
                  />
                  <button
                    type="button"
                    className={
                      "composer-action-btn" +
                      (activeStreamingMessage ? " stop" : " send")
                    }
                    onClick={() =>
                      activeStreamingMessage
                        ? void cancelGeneration(activeStreamingMessage)
                        : void sendQuestion()
                    }
                    disabled={
                      activeStreamingMessage
                        ? false
                        : busy ||
                          !question.trim() ||
                          needsPaperProvider
                    }
                    aria-label={
                      activeStreamingMessage ? t("app.stopReply") : t("app.sendQuestion")
                    }
                    title={
                      activeStreamingMessage
                        ? t("app.stopReplyTitle")
                        : t("app.sendTitle")
                    }
                  >
                    {activeStreamingMessage ? "■" : busy ? "…" : "↑"}
                  </button>
                </div>
              </div>
            </>
          ) : (
            <Suspense
              fallback={
                <div className="artifact-loading">{t("completion.loading_reading_results")}</div>
              }
            >
              <ArtifactPanel
                artifacts={validArtifacts}
                activeArtifactId={activeArtifactId}
                activeBlockId={artifactBusyBlockId}
                activeBlockLabel={pendingArtifactJob?.label}
                displayCropSrc={activeArtifactCropSrc}
                lensQa={lensQa}
                lensQaBusy={lensQaBusy}
                blocks={ocr?.blocks}
                pdfDocument={pdfDocument}
                rotation={rotation}
                onGenerateBlockAction={handleBlockAction}
                onSelect={(artifact) => void selectArtifact(artifact)}
                onJump={(pageNumber, blockId) => {
                  setPage(pageNumber);
                  if (blockId) setFocusedBlockId(blockId);
                }}
                onAskLens={askLens}
                onTransferLens={transferLensToDiscussion}
                onSetOverride={setArtifactOverride}
                onGenerateBrief={() => void generateBrief()}
                onGenerateDocumentArtifact={(kind, useBrief) => void generateDocumentArtifact(kind, useBrief)}
                busyDocumentArtifacts={busyDocumentArtifacts}
                onRegenerateLens={(artifact) => void regenerateLens(artifact)}
                onDeleteArtifactVersion={(id) => void handleDeleteArtifactVersion(id)}
                modelLabel={formatModelLabel(
                  currentProvider?.paperModel,
                  currentProvider?.models,
                )}
                onOpenOutline={() => {
                  setShowArtifactAnnotations(false);
                  void openOutline();
                }}
                preferBrief={preferBrief}
                orientationBusy={Boolean(activeOrientationJob)}
                documentKind={selectedDoc?.kind}
                discussionFontSize={discussionFontSize}
                onDiscussionFontSizeChange={setDiscussionFontSize}
                scopeTab={artifactScope}
                onScopeTabChange={(value) => {
                  setArtifactScope(value);
                  if (value === "block") setShowArtifactAnnotations(false);
                }}
                hideScopeTabs={true}
                showAnnotations={showArtifactAnnotations}
                annotationCount={annotations.length}
                onShowAnnotations={() => {
                  setArtifactScope("global");
                  setShowArtifactAnnotations(true);
                }}
                annotationsSlot={
                  <ReaderAnnotationsPanel
                    annotations={annotations}
                    activeAnnotationId={selectedAnnotationId || activeAnnotationId}
                    onJump={jumpToAnnotation}
                    onUpdate={updateUserAnnotation}
                    onDelete={deleteUserAnnotation}
                  />
                }
                onUpdateTags={handleUpdateTags}
                onUpdateMetadata={handleUpdateMetadata}
                onUpdateOrientationTable={handleUpdateOrientationTable}
              />
            </Suspense>
          )}
        </aside>
        )}
          </>
        ) : null}
      </main>
      {showContext && (
        <div className="scrim" onClick={() => setShowContext(false)}>
          <aside
            className="drawer context-drawer"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="drawer-head">
              <div>
                <span className="eyebrow">{t("completion.context_inspector")}</span>
                <h2>{t("completion.what_the_model_sees")}</h2>
              </div>
              <button
                className="icon-button"
                onClick={() => setShowContext(false)}
              >
                ×
              </button>
            </div>
            <div className="context-epoch">
              <span>{t("completion.context_epoch")}</span>
              <strong>
                {selectedDoc
                  ? selectedDoc.sha256.slice(0, 12)
                  : "not initialized"}
              </strong>
              <small>{t("completion.stable_prefix_remains_cacheable")}</small>
            </div>
            <ContextBlock
              label="01 / SOURCE"
              title={t("completion.native_pdf")}
              meta={
                selectedDoc
                  ? `${selectedDoc.pages} pages · ${selectedDoc.sha256.slice(0, 16)}…`
                  : "No paper selected"
              }
            />
            <ContextBlock
              label="02 / ORIENTATION"
              title="BriefContextPack"
              meta={brief ? t("app.briefReady", { model: brief.model || "Gemini" }) : t("app.notGenerated")}
            />
            <ContextBlock
              label="03 / PATH"
              title={activeThread?.title ?? "Thread path"}
              meta={`${messages.length} messages · ${replyTo ? "branch point selected" : "main line"}`}
            />
            <ContextBlock
              label="04 / TAIL"
              title={t("feedback.message63", { v0: page })}
              meta="Dynamic context · not part of stable prefix"
            />
            <div className="inspector-note">{t("completion.you_control_the_question_and_the_evidence_anchor_the_stable_instruction_schema_and_cache_checkpoint_are_managed_by_the_adapter")}</div>
          </aside>
        </div>
      )}
      {showTree && (
        <ConversationTree
          title={selectedDoc?.title ?? "Paper"}
          messages={messages}
          activeMessageId={activeThread?.activeMessageId ?? null}
          onClose={() => setShowTree(false)}
          onSelect={(message) => {
            void selectActiveBranch(message);
          }}
          onDelete={(user) => {
            void deleteDiscussionTurn(user);
          }}
        />
      )}
      {exitChoiceOpen ? (
        <div className="scrim exit-choice-scrim">
          <section
            className="exit-choice"
            role="dialog"
            aria-modal="true"
            aria-labelledby="exit-choice-title"
          >
            <span className="eyebrow">{t("completion.active_work_safe_exit")}</span>
            <h2 id="exit-choice-title">{t("completion.tasks_are_still_in_progress")}</h2>
            <p>{t("completion.choose_whether_read_desktop_should_keep_working_preserve_recoverable_queue_state_or_cancel_active_work_before_exiting")}</p>
            <div className="exit-work-summary">
              <span>
                <b>{exitWorkSummary.running}</b>{t("completion.running")}</span>
              <span>
                <b>{exitWorkSummary.queued}</b>{t("completion.queued")}</span>
              <span>
                <b>{exitWorkSummary.paused}</b>{t("completion.paused")}</span>
              <span>
                <b>{exitWorkSummary.streamingDiscussions}</b>{t("completion.visible_streaming_chat")}</span>
            </div>
            <div className="exit-choice-options">
              <button
                type="button"
                disabled={exitChoiceBusy}
                onClick={() => void resolveExitChoice("continue_in_tray")}
              >
                <strong>{t("completion.continue_in_tray")}</strong>
                <small>{t("completion.hide_the_window_and_let_current_work_finish")}</small>
              </button>
              <button
                type="button"
                disabled={exitChoiceBusy}
                onClick={() => void resolveExitChoice("pause_and_exit")}
              >
                <strong>{t("completion.pause_and_exit")}</strong>
                <small>{t("completion.pause_safe_queue_work_paid_requests_with_unknown_outcomes_become_interrupted_unknown_and_are_never_retried_silently")}</small>
              </button>
              <button
                type="button"
                className="danger"
                disabled={exitChoiceBusy}
                onClick={() => void resolveExitChoice("cancel_and_exit")}
              >
                <strong>{t("completion.cancel_and_exit")}</strong>
                <small>{t("completion.keep_partial_chat_text_but_cancel_active_tasks")}</small>
              </button>
            </div>
            <button
              type="button"
              className="exit-choice-dismiss"
              disabled={exitChoiceBusy}
              onClick={() => setExitChoiceOpen(false)}
            >{t("completion.keep_the_window_open")}</button>
          </section>
        </div>
      ) : null}
      {!roadmapOpen &&
      selectedDoc &&
      !showSettings &&
      !showOperations &&
      !showContext &&
      !showTree &&
      !exitChoiceOpen ? (
        <div
          className="roadmap-edge-trigger"
          onMouseEnter={() => setRoadmapOpen(true)}
          aria-label={t("app.roadmapPeekAria")}
        >
          <div
            className="roadmap-edge-peek-pill"
            onClick={() => {
              setRoadmapOpen(true);
              setRoadmapPinned(true);
              try {
                localStorage.setItem("read_desktop_roadmap_pinned", "true");
              } catch {}
            }}
            title={t("app.roadmapPeekTitle")}
          >
            <span>📋</span>
            <small>{t("app.roadmap")}</small>
          </div>
        </div>
      ) : null}
      <ReadingRoadmapPanel
        open={
          roadmapOpen &&
          Boolean(selectedDoc) &&
          !showSettings &&
          !showOperations &&
          !showContext &&
          !showTree &&
          !exitChoiceOpen
        }
        pinned={roadmapPinned}
        projection={roadmapProjection}
        progress={roadmapProgress}
        activeJobId={activeRoadmapJob?.id ?? null}
        isOrientationMissing={!brief}
        onClose={() => setRoadmapOpen(false)}
        onTogglePin={(nextPinned) => {
          setRoadmapPinned(nextPinned);
          try {
            localStorage.setItem(
              "read_desktop_roadmap_pinned",
              String(nextPinned),
            );
          } catch {}
        }}
        onToggleTask={async (taskId, completed) => {
          if (!selectedDoc || !roadmapProjection) return;
          setRoadmapProgress((prev) => {
            const exists = prev.some((p) => p.taskId === taskId);
            if (exists) {
              return prev.map((p) =>
                p.taskId === taskId
                  ? {
                      ...p,
                      completed,
                      completedAt: completed ? new Date().toISOString() : null,
                    }
                  : p,
              );
            }
            return [
              ...prev,
              {
                taskId,
                completed,
                completedAt: completed ? new Date().toISOString() : null,
              },
            ];
          });
          try {
            await desktopClient.command("toggle_roadmap_task", {
              request: {
                paperId: selectedDoc.id,
                roadmapId: roadmapProjection.id,
                taskId,
                completed,
              },
            });
          } catch (error) {
            setStatus(t("app.roadmapProgressFailed", { error: String(error) }));
          }
        }}
        onGenerate={async () => {
          if (!selectedDoc) return;
          if (needsPaperProvider) {
            setStatus(paperProviderNotConfiguredStatus(currentProvider?.kind, t));
            openSettings("models");
            return;
          }
          try {
            await desktopClient.command<JobProjection>("start_roadmap_job", {
              request: { revisionId: selectedDoc.revisionId },
            });
            setStatus(t("app.roadmapQueued"));
            await refreshJobs();
          } catch (error) {
            setStatus(t("app.roadmapStartFailed", { error: String(error) }));
          }
        }}
        onJumpToPage={(targetPage, targetBlockId, label) => {
          setPage(targetPage);
          if (!layoutShowsPdf(workspaceLayout)) {
            setWorkspaceLayout(
              applyWorkspaceLayoutIntent(workspaceLayout, "show_pdf"),
            );
          }
          if (targetBlockId) {
            setFocusedBlockId(targetBlockId);
            return;
          }
          if (ocr?.blocks && ocr.blocks.length > 0) {
            const pageBlocks = ocr.blocks.filter(
              (b) => b.pageNumber === targetPage,
            );
            if (pageBlocks.length > 0) {
              const labelLower = (label || "").toLowerCase();
              let matched = pageBlocks.find((b) => {
                if (
                  (labelLower.includes("figure") ||
                    labelLower.includes("fig") ||
                    labelLower.includes(t("app.content.figureGlyph"))) &&
                  b.blockType.toLowerCase() === "figure"
                ) {
                  return true;
                }
                if (
                  (labelLower.includes("table") ||
                    labelLower.includes(t("app.content.tableGlyph"))) &&
                  b.blockType.toLowerCase() === "table"
                ) {
                  return true;
                }
                return false;
              });

              if (!matched && labelLower) {
                const clean = labelLower
                  .replace(/^(figure|fig\.?|table|section|sec\.?|p\.\d+)\s*/i, "")
                  .trim();
                if (clean.length >= 3) {
                  matched = pageBlocks.find((b) =>
                    b.textContent.toLowerCase().includes(clean),
                  );
                }
              }

              if (matched) {
                setFocusedBlockId(matched.id);
                return;
              }

              const firstContent = pageBlocks.find(
                (b) => b.blockType !== "header" && b.blockType !== "footer",
              );
              if (firstContent) {
                setFocusedBlockId(firstContent.id);
              }
            }
          }
        }}
      />
      <CascadeOcrDeleteConfirmModal
        open={showOcrDeleteModal}
        paperTitle={selectedDoc?.title}
        translationsCount={
          artifacts.filter((a) => a.kind === "translation").length
        }
        lensCount={
          artifacts.filter((a) => a.kind.startsWith("lens_")).length
        }
        guidesCount={
          artifacts.filter((a) => a.kind === "reading_guide").length
        }
        busy={busy}
        onConfirm={() => void deleteOcrCascade()}
        onCancel={() => setShowOcrDeleteModal(false)}
      />
      <TrashConfirmModal
        open={Boolean(paperToTrash)}
        paperTitle={paperToTrash?.title ?? ""}
        paperPath={
          paperToTrash?.collection
            ? `${paperToTrash.collection}/${paperToTrash.title}.pdf`
            : undefined
        }
        busy={busy}
        onConfirm={() => void handleConfirmTrash()}
        onCancel={() => setPaperToTrash(null)}
      />
      <TrashConfirmModal
        open={Boolean(folderToTrash)}
        paperTitle={folderToTrash?.relativePath ?? ""}
        busy={busy}
        documentCount={folderToTrash?.count}
        isFolder={true}
        onConfirm={() => void handleConfirmFolderTrash()}
        onCancel={() => setFolderToTrash(null)}
      />
      <OperationsDrawer
        open={showOperations}
        jobs={jobs}
        batches={libraryBatches}
        storage={storageReport}
        diagnostics={diagnosticPreview}
        trash={trash}
        remoteTombstones={remoteTombstones}
        documents={documents}
        busyJobId={operationBusyId}
        onClose={() => setShowOperations(false)}
        onRefresh={() => void refreshOperations()}
        onPause={(jobId) => void controlJob("pause_job", jobId)}
        onResume={(jobId) => void controlJob("resume_job", jobId)}
        onReprioritize={(jobId, priority) =>
          void reprioritizeJob(jobId, priority)
        }
        onCancel={(jobId) => void controlJob("cancel_job", jobId)}
        onRestore={(paperId) => void restoreFromTrash(paperId)}
        onRestoreReaderFolder={(id) => void restoreReaderFolder(id)}
        onRetryRemote={(tombstoneId) => void retryRemoteCleanup(tombstoneId)}
        onPreviewDiagnostics={() => void previewDiagnostics()}
        onExportDiagnostics={() => void exportDiagnostics()}
        providerInstances={modelSettings.providers}
        onOpenProviderSettings={openProviderSettingsForRecovery}
        onRecheckProvider={(jobId) => void recheckProviderJob(jobId)}
        onRebindProvider={(jobId, providerInstanceId) =>
          void rebindProviderJob(jobId, providerInstanceId)
        }
        onAbandonLegacyProviderJob={(jobId, confirmedPotentialCharge) =>
          void abandonLegacyProviderJob(jobId, confirmedPotentialCharge)
        }
        onAbandonRemoteCleanup={(tombstoneId, confirmed) =>
          void abandonRemoteCleanup(tombstoneId, confirmed)
        }
        focusJobId={operationFocusJobId}
        onRetryJob={(jobId, confirmed) => {
          void (async () => {
            try {
              setOperationBusyId(jobId);
              await desktopClient.command("retry_job", { request: { jobId, confirmedPotentialCharge: confirmed } });
              await refreshJobs();
              notifySuccess(t("app.retried"), t("app.jobRequeued", { id: jobId.slice(0, 8) }), { source: "job" });
            } catch (e) {
              notifyError(t("app.retryFailed"), String(e), { source: "job" });
            } finally {
              setOperationBusyId("");
            }
          })();
        }}
        onRetryBatch={retryLibraryBatch}
        onCancelBatch={cancelLibraryBatch}
        onUndoBatch={undoLibraryBatch}
        onOpenSource={(paperId, revisionId) => {
          setShowOperations(false);
          if (revisionId) {
            setViewMode("reader");
            void loadDocument(revisionId, paperId ?? undefined);
          } else if (paperId) {
            const doc = documents.find((d) => d.id === paperId);
            if (doc) {
              setViewMode("reader");
              void loadDocument(doc.revisionId, doc.id);
            }
          }
        }}
      />{" "}
      {enlargedQuote && (
        <div
          ref={lightboxRef}
          className="lightbox-modal"
          role="dialog"
          aria-modal="true"
          aria-label={t("completion.image_preview")}
          onClick={() => setEnlargedQuote(null)}
          data-testid="lightbox-modal"
        >
          <div
            className="lightbox-content"
            role="document"
            onClick={(e) => e.stopPropagation()}
          >
            <button
              ref={lightboxCloseRef}
              type="button"
              className="lightbox-close"
              onClick={() => setEnlargedQuote(null)}
              aria-label={t("app.closePreview")}
            >
              ×
            </button>
            <div className="lightbox-body">
              <QuoteFigureThumbnail
                quote={enlargedQuote}
                pdfDocument={pdfDocument}
                rotation={rotation}
                className="lightbox-figure-image-wrap"
              />
              <p className="lightbox-caption">
                {formatQuoteCaption(
                  enlargedQuote.textContent,
                  `Figure on page ${enlargedQuote.pageNumber} (#${enlargedQuote.blockIndex + 1})`,
                )}
              </p>
              <button
                type="button"
                className="btn-liquid-pill primary"
                style={{ marginTop: 14 }}
                onClick={() => {
                  setEnlargedQuote(null);
                  jumpToBlockQuote(enlargedQuote);
                }}
              >
                {t("app.jumpToPdfPage", { page: enlargedQuote.pageNumber })} ↗
              </button>
            </div>
          </div>
        </div>
      )}
      {annotationComposer ? (
        <ReaderAnnotationComposer
          kind={annotationComposer.kind}
          pageNumber={annotationComposer.block.pageNumber}
          excerpt={annotationComposer.block.textContent}
          busy={annotationBusy}
          error={annotationError}
          onSubmit={submitAnnotationComposer}
          onCancel={() => {
            if (!annotationBusy) {
              setAnnotationComposer(null);
              setAnnotationError(null);
            }
          }}
        />
      ) : null}
      {readerContextTarget ? (
        <ReaderContextDialog
          target={readerContextTarget}
          load={loadReaderContext}
          save={saveReaderContext}
          onClose={() => setReaderContextTarget(null)}
        />
      ) : null}
      <SettingsWorkbench
        open={showSettings}
        initialSection={settingsInitialSection}
        initialProviderId={settingsInitialProviderId}
        focusNonce={settingsFocusNonce}
        workspace={workspace}
        modelSettings={modelSettings}
        workspaceBusy={busy}
        onClose={() => setShowSettings(false)}
        onChooseWorkspace={chooseWorkspace}
        onResetWorkspace={resetWorkspace}
        onOpenWorkspace={openWorkspaceFolder}
        onModelSettingsChange={(next) => {
          setModelSettings(next);
          setStatus(paperProviderConnectionStatus(next, t));
        }}
        locale={uiLocale}
        onRequestLocaleChange={(next) => {
          if (next === uiLocale) return;
          setPendingLocale(next);
        }}
      />
      <LanguageChangeConfirmModal
        open={Boolean(pendingLocale)}
        next={pendingLocale ?? "zh-CN"}
        busy={localeChangeBusy}
        onConfirm={() => void confirmLocaleChange()}
        onCancel={() => setPendingLocale(null)}
      />
    </div>
    </LocaleProvider>
  );
}

function MessageBubble({
  message,
  depth,
  paperModel,
  models,
  pdfDocument = null,
  rotation = 0,
  onBranch,
  onEdit,
  onRegenerate,
  onCancel,
  onMarkdownCitation,
  onBlockQuote,
  onEnlargeQuote,
}: {
  message: Message;
  depth: number;
  paperModel: string;
  models: GeminiModelOption[];
  pdfDocument?: PDFDocumentProxy | null;
  rotation?: number;
  onBranch: (message: Message) => void;
  onEdit: (message: Message) => void;
  onRegenerate: (message: Message) => void;
  onCancel: (message: Message) => void;
  onMarkdownCitation: (href: string) => void;
  onBlockQuote: (quote: BlockQuoteSnapshot) => void;
  onEnlargeQuote?: (quote: BlockQuoteSnapshot) => void;
}) {
  const { t, dateLocale } = useLocale();
  const isAssistant = message.role === "assistant";
  const [showUsage, setShowUsage] = useState(false);
  const [enlargedQuote, setEnlargedQuote] = useState<BlockQuoteSnapshot | null>(
    null,
  );
  const localLightboxRef = useRef<HTMLDivElement>(null);
  const localLightboxCloseRef = useRef<HTMLButtonElement>(null);
  useDialogFocusTrap({
    open: !onEnlargeQuote && Boolean(enlargedQuote),
    containerRef: localLightboxRef,
    initialFocusRef: localLightboxCloseRef,
    onClose: () => setEnlargedQuote(null),
  });

  const assistantModel = formatModelLabel(
    message.usage?.model,
    models,
    paperModel,
  );

  const equationQuotes =
    !isAssistant && message.blockQuotes?.length
      ? message.blockQuotes.filter(
          (q) =>
            q.blockType === "equation" ||
            q.blockType === "formula" ||
            q.textContent.startsWith("$$") ||
            q.textContent.startsWith("\\begin{equation}"),
        )
      : [];

  const figureQuotes =
    !isAssistant && message.blockQuotes?.length
      ? message.blockQuotes.filter(
          (q) =>
            q.blockType === "figure" ||
            q.blockType === "image" ||
            q.blockType === "picture" ||
            isFigureQuote(q),
        )
      : [];

  const handleEnlarge = (quote: BlockQuoteSnapshot) => {
    if (onEnlargeQuote) {
      onEnlargeQuote(quote);
    } else {
      setEnlargedQuote(quote);
    }
  };

  return (
    <article
      id={`msg-${message.id}`}
      className={
        "message " +
        (isAssistant
          ? "assistant ai-response-frameless"
          : "user user-bubble-right") +
        " " +
        message.status +
        (showUsage ? " show-usage" : "")
      }
      style={{ ["--tree-depth" as string]: String(depth) }}
    >
      <div className="message-meta">
        <span className="message-avatar">{isAssistant ? "✦" : "Y"}</span>
        {isAssistant && message.usage ? (
          <button
            type="button"
            className="message-model-name"
            onClick={() => setShowUsage((value) => !value)}
            aria-expanded={showUsage}
            title={message.usage.model}
          >
            {assistantModel}
          </button>
        ) : (
          <strong title={isAssistant ? paperModel : undefined}>
            {isAssistant ? assistantModel : "You"}
          </strong>
        )}
        <span>{formatTime(message.createdAt, dateLocale, t("app.now"))}</span>
        {message.status !== "complete" && <em>{message.status}</em>}
      </div>

      {figureQuotes.length > 0 && (
        <div className="user-quoted-figures">
          {figureQuotes.map((quote) => (
            <div
              key={quote.blockId}
              className="user-quote-figure-thumbnail"
              onClick={() => handleEnlarge(quote)}
              title={t("app.enlargeImage")}
            >
              <QuoteFigureThumbnail
                quote={quote}
                pdfDocument={pdfDocument}
                rotation={rotation}
                onEnlarge={() => handleEnlarge(quote)}
                className="user-quote-figure-thumb"
              />
              <span className="figure-caption-text">
                {formatQuoteCaption(
                  quote.textContent,
                  `Figure p.${quote.pageNumber}`,
                )}
              </span>
            </div>
          ))}
        </div>
      )}

      {equationQuotes.length > 0 && (
        <div className="user-quoted-formulas">
          {equationQuotes.map((quote) => (
            <div key={quote.blockId} className="user-quote-formula-card">
              <MarkdownBody>
                {quote.textContent.startsWith("$$")
                  ? quote.textContent
                  : `$$\n${quote.textContent}\n$$`}
              </MarkdownBody>
            </div>
          ))}
        </div>
      )}

      <div className="message-content">
        {!message.content && message.status === "streaming" ? (
          <p className="streaming-placeholder">{t("completion.waiting_for_the_first_token")}</p>
        ) : message.status === "failed" ? (
          <p>{message.content}</p>
        ) : (
          <MarkdownBody
            streaming={message.status === "streaming"}
            onCitation={onMarkdownCitation}
          >
            {message.content}
          </MarkdownBody>
        )}
      </div>

      {message.blockQuotes?.length > 0 && (
        <div
          className={
            isAssistant ? "message-block-quotes" : "user-quoted-block-links"
          }
          aria-label={t("completion.quoted_block_snapshots")}
        >
          {message.blockQuotes.map((quote) => (
            <button
              type="button"
              key={quote.blockId}
              className={isAssistant ? "" : "user-quote-pill"}
              onClick={() => onBlockQuote(quote)}
              title={t("app.jumpToPage", { page: quote.pageNumber })}
            >
              <span>
                p.{quote.pageNumber} · {quote.blockType} #{quote.blockIndex + 1}
              </span>
              {isAssistant && (
                <small>{quote.textContent || "Non-text OCR Block"}</small>
              )}
            </button>
          ))}
        </div>
      )}
      {isAssistant && message.usage && (
        <div className="usage-strip">
          <span>{t("completion.in")}<b>{formatNumber(message.usage.inputTokens, t("app.unknown"))}</b>
          </span>
          <span>{t("completion.uncached")}<b>{formatNumber(message.usage.uncachedInputTokens, t("app.unknown"))}</b>
          </span>
          <span className="cached">{t("completion.cached")}<b>{formatNumber(message.usage.cachedInputTokens, t("app.unknown"))}</b>{" "}
            <i>{formatPercent(message.usage.cacheHitRate, t("app.unknown"))}</i>
          </span>
          <span>{t("completion.out")}<b>{formatNumber(message.usage.outputTokens, t("app.unknown"))}</b>
          </span>
          <span>{t("completion.reasoning")}<b>{formatNumber(message.usage.reasoningTokens, t("app.unknown"))}</b>
          </span>
          <span>{message.usage.sessionResume ? "resumed" : "new session"}</span>
          <span>{message.usage.paperRootBranch ? "paper root" : "branch"}</span>
          <span>{message.usage.estimatedCost ?? "—"}</span>
        </div>
      )}
      <div className="message-actions">
        {message.status === "streaming" ? (
          <button
            type="button"
            className="stop-generation"
            onClick={() => onCancel(message)}
            title={t("app.stopKeep")}
            aria-label={t("app.stopKeep")}
          >
            <Square size={13} />
          </button>
        ) : (
          <>
            {isAssistant ? (
              <>
                <button
                  type="button"
                  onClick={() => onBranch(message)}
                  title={t("app.branchFromHere")}
                  aria-label={t("app.branchFromHere")}
                >
                  <GitBranch size={14} />
                </button>
                {(message.status === "complete" ||
                  message.status === "failed" ||
                  message.status === "cancelled") && (
                  <button
                    type="button"
                    onClick={() => onRegenerate(message)}
                    title={
                      message.status === "complete" ? t("app.regenerate") : t("app.retry")
                    }
                    aria-label={
                      message.status === "complete" ? t("app.regenerate") : t("app.retry")
                    }
                  >
                    <RotateCcw size={14} />
                  </button>
                )}
              </>
            ) : (
              <button
                type="button"
                onClick={() => onEdit(message)}
                title={t("app.editAsBranch")}
                aria-label={t("app.editAsBranch")}
              >
                <Pencil size={14} />
              </button>
            )}
          </>
        )}
      </div>

      {/* Local fallback Lightbox Modal if not using parent onEnlargeQuote */}
      {!onEnlargeQuote && enlargedQuote && (
        <div
          ref={localLightboxRef}
          className="lightbox-modal"
          role="dialog"
          aria-modal="true"
          aria-label={t("completion.image_preview")}
          onClick={() => setEnlargedQuote(null)}
          data-testid="lightbox-modal"
        >
          <div
            className="lightbox-content"
            onClick={(e) => e.stopPropagation()}
          >
            <button
              ref={localLightboxCloseRef}
              type="button"
              className="lightbox-close"
              onClick={() => setEnlargedQuote(null)}
              aria-label={t("app.closePreview")}
            >
              ×
            </button>
            <div className="lightbox-body">
              <QuoteFigureThumbnail
                quote={enlargedQuote}
                pdfDocument={pdfDocument}
                rotation={rotation}
                className="lightbox-figure-image-wrap"
              />
              <p className="lightbox-caption">
                {formatQuoteCaption(
                  enlargedQuote.textContent,
                  `Figure on page ${enlargedQuote.pageNumber} (#${enlargedQuote.blockIndex + 1})`,
                )}
              </p>
              <button
                type="button"
                className="btn-liquid-pill primary"
                style={{ marginTop: 14 }}
                onClick={() => {
                  setEnlargedQuote(null);
                  onBlockQuote(enlargedQuote);
                }}
              >
                {t("app.jumpToPdfPage", { page: enlargedQuote.pageNumber })} ↗
              </button>
            </div>
          </div>
        </div>
      )}
    </article>
  );
}

function ContextBlock({
  label,
  title,
  meta,
}: {
  label: string;
  title: string;
  meta: string;
}) {
  return (
    <div className="context-block">
      <span>{label}</span>
      <strong>{title}</strong>
      <small>{meta}</small>
    </div>
  );
}
export default App;
