import { uiText } from "../i18n/uiText";
import { useMemo, useState, useEffect, useRef } from "react";
import type { DocumentCard, ThemeMode, WorkspaceInfo, CollectionProjection } from "../types";
import HubContextMenu, { type MenuItem } from "./HubContextMenu";
import HubMoveDialog from "./HubMoveDialog";
import HubTagPatchDialog from "./HubTagPatchDialog";
import HubLifecycleDialog from "./HubLifecycleDialog";
import HubBatchConfirmDialog from "./HubBatchConfirmDialog";
import { useDialogFocusTrap } from "../useDialogFocusTrap";
import { applyHubSort, hubSortAvailability, insertIndexFromRect, type HubSortMode } from "../hubSort";
import { BULK_DISABLED_REASON, LIFECYCLE_DISABLED_REASON, SNAPSHOT_DISABLED_REASON, useLibraryWorkspace } from "../library/useLibraryWorkspace";
import { dropLabel, parentOf, type DragSubject, type DropReason, type DropTarget, type ReorderContext } from "../library/dropPolicy";
import { documentMatchesLibraryFilters, hubFiltersForExactCollection, hubFiltersForView, hubSortForView, matchesHubTextFilter } from "../library/hubFilters";
import { documentCardFromHub } from "../library/hubDocument";
import {
  cssPointFromPhysical,
  decideImportDrop,
  importDestination,
  importDestinationFromHit,
  importHitFromElement,
} from "../library/importDrop";

import { useHubPageList } from "../library/useHubPageList";
import {
  gridColumnCount,
  HUB_GRID_CARD_HEIGHT,
  HUB_TABLE_ROW_HEIGHT,
  virtualWindow,
} from "../library/virtualWindow";
import {
  BUILTIN_SMART_COLLECTIONS,
  LIBRARY_PROTOCOL_VERSION,
  smartCollectionFolder,
  smartCollectionIdOf,
  type SmartCollectionProjection,
} from "../library/libraryWorkspaceTypes";
import { type ReadingLifecycleStatus } from "../readerReadingState";
import { useLocale } from "../i18n/LocaleContext";
import type { LibraryWorkspaceClient } from "../library/libraryWorkspaceClient";
import type { QuerySnapshot } from "../library/selectionModel";
import {
  controlBatchRequest,
  batchOf,
  changeRequest,
  planAndStart,
  startBatchRequest,
  summarizeBatch,
  toLibraryActError,
  type BatchCommand,
  type BatchProjection,
  type BatchTarget,
  type LifecyclePatch,
  undoTokenOf,
} from "../library/libraryActTypes";
import { consumeUndoToken, rememberUndoToken } from "../library/undoTokenStore";

const HUB_DRAG_THRESHOLD_PX = 8;

/** Single-document tag/export live on card chips and the reader top bar; the menu does not duplicate those dialogs. */
const STATUS_KEY: Record<ReadingLifecycleStatus, string> = {
  unread: "hub.status.unread",
  reading: "hub.status.reading",
  read: "hub.status.read",
};

type DragFeedback = Readonly<{
  tone: "info" | "valid" | "invalid";
  text: string;
}>;

function folderMatches(collection: string, selected: string) {
  if (!selected) return true;
  return collection === selected || collection.startsWith(`${selected}/`);
}

function documentCountInFolder(documents: DocumentCard[], folder: string) {
  return documents.filter((doc) => folderMatches(doc.collection, folder)).length;
}

export interface LibraryHubProps {
  documents: DocumentCard[];
  collections?: CollectionProjection[];
  workspace: WorkspaceInfo | null;
  onSelectPaper: (revisionId: string, paperId: string) => void;
  onImportPdf: (collection: string) => void;
  onChooseWorkspace?: () => void;
  onOpenSettings: (section?: "workspace" | "models") => void;
  onOpenOperations: () => void;
  libraryNotice?: string | null;
  onDismissNotice?: () => void;
  /** Incrementing this replays the first-upgrade tour (Settings → Replay, §6.5). */
  replayTourSignal?: number;
  themeMode?: ThemeMode;
  onToggleTheme?: (theme: ThemeMode) => void;
  importBusy?: boolean;
  /** Single-document tag override; bulk add/remove goes through library_act PatchTags. */
  onUpdateTags?: (paperId: string, tags: string[]) => Promise<void>;
  selectedFolder?: string;
  onSelectFolder?: (folder: string) => void;
  onCreateFolder?: (parentPath: string) => void | Promise<void>;
  onRenameFolder?: (relativePath: string, newName: string) => void | Promise<void>;
  onMoveFolder?: (relativePath: string, destParentPath: string) => void | Promise<void>;
  onRenamePaper?: (paperId: string, newStem: string) => void | Promise<void>;
  onMovePaper?: (paperId: string, destCollection: string) => void | Promise<void>;
  onTrashPaper?: (paperId: string) => void;
  onTrashFolder?: (relativePath: string) => void;
  onEditReaderContext?: (target: {
    scope: "workspace" | "folder" | "paper";
    collectionPath?: string;
    paperId?: string;
  }) => void;
  onOpenResource?: (path: string) => void;
  onOpenWorkspace?: () => void;
  onSetSortMode?: (collectionId: string, mode: HubSortMode) => void | Promise<void>;
  onReorderPapers?: (collectionId: string, paperIds: string[]) => void | Promise<void>;
  libraryClient?: LibraryWorkspaceClient;
  onLibraryChanged?: () => void | Promise<void>;
  onImportPaths?: (paths: string[], collection: string) => void | Promise<void>;
}

function isRootFolder(f: string) { return f === "Papers" || f === "Textbooks"; }
function isAllFolder(f: string) { return f === ""; }
function isSmartFolder(f: string) { return Boolean(smartCollectionIdOf(f)); }
function isNormalFolder(f: string) { return !isAllFolder(f) && !isRootFolder(f) && !isSmartFolder(f); }
function isWindowsReserved(name: string) {
  const base = name.split(".")[0].toUpperCase();
  return base === "CON" || base === "PRN" || base === "AUX" || base === "NUL" || /^COM[1-9]$/.test(base) || /^LPT[1-9]$/.test(base);
}

function isActMessage(error: unknown): string {
  return toLibraryActError(error).message;
}

function hubDate(value: string | null | undefined): string | null {
  if (!value) return null;
  const day = value.slice(0, 10);
  return day || null;
}

function isTextEntry(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null;
  if (!element || !element.tagName) return false;
  return element.tagName === "INPUT" || element.tagName === "TEXTAREA" || element.isContentEditable === true;
}

export default function LibraryHub({
  documents,
  collections = [],
  workspace,
  onSelectPaper,
  onImportPdf,
  onChooseWorkspace,
  onOpenSettings,
  onOpenOperations,
  libraryNotice,
  onDismissNotice,
  replayTourSignal,
  themeMode = "liquid-light",
  onToggleTheme,
  importBusy = false,
  onUpdateTags,
  selectedFolder: controlledFolder,
  onSelectFolder,
  onCreateFolder,
  onRenameFolder,
  onMoveFolder,
  onRenamePaper,
  onMovePaper,
  onTrashPaper,
  onTrashFolder,
  onEditReaderContext,
  onOpenResource,
  onOpenWorkspace,
  onSetSortMode,
  onReorderPapers,
  libraryClient,
  onLibraryChanged,
  onImportPaths,
}: LibraryHubProps) {
  const { t } = useLocale();
  const [addingTagDocId, setAddingTagDocId] = useState<string | null>(null);
  const [newTagInput, setNewTagInput] = useState("");
  const [internalFolder, setInternalFolder] = useState<string>("");
  const selectedFolder = controlledFolder ?? internalFolder;
  const setSelectedFolder = (v: string) => {
    if (onSelectFolder) onSelectFolder(v);
    else setInternalFolder(v);
  };

  const [importPickerOpen, setImportPickerOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [viewType, setViewType] = useState<"grid" | "table">(() => {
    const saved = localStorage.getItem("read-desktop.libraryView");
    return saved === "table" ? "table" : "grid";
  });
  // No local "last-picked mode" cache: each folder's preferred sort is
  // sourced from `activeCollection.sortMode` (persisted in SQLite).
  // The all-documents view or any folder without a `CollectionProjection` row
  // resolves to "recent" so manual/chapter state never leaks across folders.

  // inline rename
  const [editingFolder, setEditingFolder] = useState<string | null>(null);
  const [editingFolderValue, setEditingFolderValue] = useState("");
  const [editingPaperId, setEditingPaperId] = useState<string | null>(null);
  const [editingPaperValue, setEditingPaperValue] = useState("");
  const [editingError, setEditingError] = useState<string | null>(null);

  // context menu
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const [moveDialogPaperId, setMoveDialogPaperId] = useState<string | null>(null);
  const [moveDialogInitialPath, setMoveDialogInitialPath] = useState<string | null>(null);
  const [moveDialogSelection, setMoveDialogSelection] = useState(false);
  const [tagDialogOpen, setTagDialogOpen] = useState(false);
  const [lifecycleDialogOpen, setLifecycleDialogOpen] = useState(false);
  const [queriedLayerIds, setQueriedLayerIds] = useState<string[] | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const [listMetrics, setListMetrics] = useState({ scrollTop: 0, height: 0, width: 0 });
  const [lifecyclePaper, setLifecyclePaper] = useState<DocumentCard | null>(null);
  const [smartCollections, setSmartCollections] = useState<readonly SmartCollectionProjection[]>(BUILTIN_SMART_COLLECTIONS);
  const [saveSmartName, setSaveSmartName] = useState("");
  const [saveSmartOpen, setSaveSmartOpen] = useState(false);
  const [confirmPlan, setConfirmPlan] = useState<BatchProjection | null>(null);
  const [confirmBusy, setConfirmBusy] = useState(false);
  const pendingStart = useRef<{ plan: BatchProjection } | null>(null);
  const [querySnapshot, setQuerySnapshot] = useState<QuerySnapshot | null>(null);
  const [importOverlay, setImportOverlay] = useState<string | null>(null);
  const [batchToast, setBatchToast] = useState<{ message: string; batchId: string; token: string | null } | null>(null);
  const batchToastTimer = useRef<number | null>(null);
  const openMoveDialog = (paperId: string, initialPath: string | null = null) => {
    setMoveDialogInitialPath(initialPath);
    setMoveDialogPaperId(paperId);
  };

  // Drag state: the drop target lives on the ref; pointerup only reads the ref (React state is for drawing).
  const dragState = useRef<{
    originX: number;
    originY: number;
    id: string;
    kind: "folder" | "paper";
    collection: string;
    fileName?: string;
    moved: boolean;
    target: DropTarget;
    dropTarget: string | null;
    dropForbidden: string | null;
    dropReason: DropReason | null;
    stageDropActive: boolean;
  } | null>(null);
  const [dragging, setDragging] = useState<{ kind: "folder" | "paper"; label: string; x: number; y: number } | null>(null);
  const [dragFeedback, setDragFeedback] = useState<DragFeedback | null>(null);
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  const [dropForbidden, setDropForbidden] = useState<string | null>(null);
  const [stageDropActive, setStageDropActive] = useState(false);
  const [insert, setInsert] = useState<{ targetId: string | null; place: "before" | "after" | "end" } | null>(null);
  const [toast, setToast] = useState<{ kind: "paper" | "folder"; id: string; fromPath: string; toPath: string; newPath?: string; fileName?: string } | null>(null);
  const toastTimer = useRef<number | null>(null);
  const [undoBusy, setUndoBusy] = useState(false);
  const [moveError, setMoveError] = useState<string | null>(null);
  const suppressNextClickRef = useRef(false);
  const prevColsRef = useRef<Set<string>>(new Set(collections.map((c) => c.relativePath)));
  const pendingCreateParentRef = useRef<string | null>(null);

  // Immediate rename after create: when collections grow, enter edit on the newest new-folder name.
  useEffect(() => {
    const prev = prevColsRef.current;
    const curSet = new Set(collections.map((c) => c.relativePath));
    if (curSet.size > prev.size) {
      const added = collections.filter((c) => !prev.has(c.relativePath));
      const parent = pendingCreateParentRef.current;
      if (parent) {
        const candidate = added.find((c) => c.relativePath.startsWith(parent + "/") && c.name.startsWith(t("hub.newFolderPrefix")));
        if (candidate) {
          setEditingFolder(candidate.relativePath);
          setEditingFolderValue(candidate.name);
          setEditingError(null);
          setSelectedFolder(candidate.relativePath);
          pendingCreateParentRef.current = null;
        } else if (added.length === 1) {
          setEditingFolder(added[0].relativePath);
          setEditingFolderValue(added[0].name);
          setEditingError(null);
          setSelectedFolder(added[0].relativePath);
          pendingCreateParentRef.current = null;
        }
      }
    }
    prevColsRef.current = curSet;
  }, [collections, t]);

  const handleSetViewType = (mode: "grid" | "table") => {
    setViewType(mode);
    try { localStorage.setItem("read-desktop.libraryView", mode); } catch { /* ignore */ }
  };

  const importTarget = () => importDestination(selectedFolder);

  const requestImport = (collection?: string) => {
    const target = collection ?? importTarget();
    if (!target) { setImportPickerOpen(true); return; }
    setImportPickerOpen(false);
    onImportPdf(target);
  };
  const handleAddSubdir = (parent: string) => {
    pendingCreateParentRef.current = parent;
    onCreateFolder?.(parent);
  };

  const activeCollection = useMemo(() => collections.find((c) => c.relativePath === selectedFolder) ?? null, [collections, selectedFolder]);
  const activeSmart = useMemo(() => {
    const id = smartCollectionIdOf(selectedFolder);
    return id ? smartCollections.find((item) => item.id === id) ?? null : null;
  }, [selectedFolder, smartCollections]);
  const availability = useMemo(() => hubSortAvailability(documents, selectedFolder), [documents, selectedFolder]);
  const preferredMode: HubSortMode = activeSmart
    ? hubSortForView(activeSmart.query.sort)
    : activeCollection
      ? ((activeCollection.sortMode as HubSortMode) ?? "recent")
      : "recent";
  const effectiveMode: HubSortMode = (() => {
    if (preferredMode === "last_opened") return "last_opened";
    if (preferredMode === "manual" && !availability.manual) return "recent";
    if (preferredMode === "chapter" && !availability.chapter) return "recent";
    return preferredMode;
  })();
  const pageSort = hubSortForView(effectiveMode);
  const [sortDirections, setSortDirections] = useState<Partial<Record<HubSortMode,"asc"|"desc">>>({});
  const sortDirection = sortDirections[pageSort];
  const pageFilters = useMemo(
    () =>
      pageSort === "manual" || pageSort === "chapter"
        ? hubFiltersForExactCollection(selectedFolder, searchQuery, activeSmart)
        : hubFiltersForView(selectedFolder, searchQuery, activeSmart),
    [activeSmart, pageSort, searchQuery, selectedFolder],
  );
  const hubPage = useHubPageList({
    client: libraryClient,
    filters: pageFilters,
    sort: pageSort,
    direction: sortDirection,
    enabled: Boolean(libraryClient),
  });

  const filteredDocuments = useMemo(() => {
    if (libraryClient) return hubPage.papers.map(documentCardFromHub);
    const anchor = querySnapshot?.evaluatedAt ?? new Date().toISOString();
    const base = documents.filter((doc) => {
      if (activeSmart) {
        return documentMatchesLibraryFilters(
          doc,
          hubFiltersForView(selectedFolder, searchQuery, activeSmart),
          anchor,
        );
      }
      if (selectedFolder && !folderMatches(doc.collection, selectedFolder)) return false;
      if (!searchQuery.trim()) return true;
      return matchesHubTextFilter(searchQuery, { title: doc.title, authors: doc.authors, fileName: doc.fileName });
    });
    const paperOrder = activeCollection?.paperOrder ?? [];
    return applyHubSort(base, effectiveMode, paperOrder);
  }, [activeCollection, activeSmart, documents, effectiveMode, hubPage.papers, libraryClient, querySnapshot?.evaluatedAt, searchQuery, selectedFolder]);

  const visibleIds = useMemo(() => filteredDocuments.map((doc) => doc.id), [filteredDocuments]);
  const paperById = useMemo(() => {
    const map = new Map<string, DocumentCard>();
    for (const doc of documents) map.set(doc.id, doc);
    for (const doc of filteredDocuments) map.set(doc.id, doc);
    return map;
  }, [documents, filteredDocuments]);
  const collectionOf = useMemo(() => {
    const map = new Map(documents.map((doc) => [doc.id, doc.collection] as const));
    for (const doc of filteredDocuments) map.set(doc.id, doc.collection);
    return (paperId: string) => map.get(paperId) ?? "";
  }, [documents, filteredDocuments]);
  const documentCountOf = useMemo(
    () => (path: string) => documentCountInFolder(documents, path),
    [documents],
  );
  /**
   * A hand-sortable layer is every live Paper in the current physical leaf folder.
   * All-documents and folders that include descendants are never a complete layer (D-062 exact permutation).
   */
  const layer = useMemo(() => {
    if (!activeCollection || !availability.manual || !isNormalFolder(selectedFolder)) {
      return { ids: [] as string[], collectionId: null as string | null };
    }
    if (libraryClient && queriedLayerIds) {
      return { ids: queriedLayerIds, collectionId: activeCollection.id };
    }
    const exact = documents.filter((doc) => doc.collection === selectedFolder);
    const ordered = applyHubSort(exact, "manual", activeCollection.paperOrder ?? []);
    return { ids: ordered.map((doc) => doc.id), collectionId: activeCollection.id };
  }, [activeCollection, availability.manual, documents, libraryClient, queriedLayerIds, selectedFolder]);

  const reorderContext: ReorderContext = useMemo(() => ({
    sortMode: effectiveMode,
    manualAvailable: availability.manual && isNormalFolder(selectedFolder),
    searching: Boolean(searchQuery.trim()),
    layerIds: layer.ids,
    smartCollection: Boolean(activeSmart),
  }), [activeSmart, availability.manual, effectiveMode, layer.ids, searchQuery, selectedFolder]);

  const hub = useLibraryWorkspace({
    viewKey: `${selectedFolder}::${searchQuery.trim()}`,
    workspaceKey: workspace?.rootPath ?? "default",
    visibleIds,
    layerIds: layer.ids,
    layerCollectionId: layer.collectionId,
    reorder: reorderContext,
    collections,
    documentCountOf,
    collectionOf,
    onReorderPapers,
    replayTourSignal,
    querySnapshot,
    matchingCount: libraryClient ? hubPage.totalCount : filteredDocuments.length,
    localBatchesEnabled: Boolean(libraryClient),
    onOpenPaper: (paperId) => {
      const doc = paperById.get(paperId);
      if (doc) onSelectPaper(doc.revisionId, doc.id);
    },
  }, t);

  useEffect(() => {
    if (!libraryClient) {
      setQuerySnapshot(null);
      return;
    }
    setQuerySnapshot(hubPage.snapshot);
  }, [hubPage.snapshot, libraryClient]);

  useEffect(() => {
    if (!libraryClient || !availability.manual || !isNormalFolder(selectedFolder)) {
      setQueriedLayerIds(null);
      return;
    }
    let cancelled = false;
    void libraryClient
      .read({
        kind: "collection_layer",
        protocolVersion: LIBRARY_PROTOCOL_VERSION,
        collectionPath: selectedFolder,
      })
      .then((result) => {
        if (!cancelled && result.kind === "collection_layer") setQueriedLayerIds([...result.paperIds]);
      })
      .catch(() => {
        if (!cancelled) setQueriedLayerIds(null);
      });
    return () => {
      cancelled = true;
    };
  }, [availability.manual, hubPage.revision, libraryClient, selectedFolder]);

  useEffect(() => {
    const element = listRef.current;
    if (!element) return;
    const update = () => {
      setListMetrics({
        scrollTop: element.scrollTop,
        height: element.clientHeight,
        width: element.clientWidth,
      });
    };
    update();
    element.addEventListener("scroll", update, { passive: true });
    const observer = typeof ResizeObserver === "function" ? new ResizeObserver(update) : null;
    observer?.observe(element);
    return () => {
      element.removeEventListener("scroll", update);
      observer?.disconnect();
    };
  }, [filteredDocuments.length, viewType]);

  useEffect(() => {
    const element = listRef.current;
    if (!element) return;
    if (typeof element.scrollTo === "function") element.scrollTo(0, 0);
    else element.scrollTop = 0;
  }, [pageFilters, pageSort, viewType]);

  useEffect(() => {
    if (!libraryClient) {
      setSmartCollections(BUILTIN_SMART_COLLECTIONS);
      return;
    }
    let cancelled = false;
    void libraryClient
      .read({ kind: "smart_collections", protocolVersion: LIBRARY_PROTOCOL_VERSION })
      .then((result) => {
        if (!cancelled && result.kind === "smart_collections") setSmartCollections(result.collections);
      })
      .catch(() => {
        if (!cancelled) setSmartCollections(BUILTIN_SMART_COLLECTIONS);
      });
    return () => {
      cancelled = true;
    };
  }, [libraryClient, documents]);

  useEffect(() => {
    if (!libraryClient || !onImportPaths) return;
    const desktop = Boolean((window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__);
    if (!desktop) return;
    let closed = false;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/webview").then(({ getCurrentWebview }) => {
      if (closed) return;
      return getCurrentWebview().onDragDropEvent((event) => {
        const payload = event.payload as { type?: string; paths?: string[]; position?: { x: number; y: number } };
        const point = cssPointFromPhysical(payload.position, window.devicePixelRatio || 1);
        const hit = point ? importHitFromElement(document.elementFromPoint(point.x, point.y)) : null;
        const decision = decideImportDrop(payload, importDestinationFromHit(hit, selectedFolder), t);
        if (decision.action === "clear") {
          setImportOverlay(null);
          return;
        }
        if (decision.action === "overlay") {
          setImportOverlay(decision.message);
          return;
        }
        setImportOverlay(null);
        if (decision.action === "hint") {
          hub.showHint(decision.message);
          return;
        }
        void onImportPaths([...decision.paths], decision.collection);
      }).then((fn) => {
        unlisten = fn;
      });
    }).catch(() => undefined);
    return () => {
      closed = true;
      unlisten?.();
    };
  }, [libraryClient, onImportPaths, selectedFolder, t]);

  const showBatchToast = (batch: BatchProjection, token: string | null) => {
    rememberUndoToken(batch.id, token, batch.undo?.expiresAt);
    if (batchToastTimer.current) window.clearTimeout(batchToastTimer.current);
    setBatchToast({ message: summarizeBatch(batch, t), batchId: batch.id, token });
    batchToastTimer.current = window.setTimeout(() => setBatchToast(null), 8000) as unknown as number;
  };

  const currentTarget = (): BatchTarget => {
    if (hub.allMatching && querySnapshot) {
      return {
        kind: "query",
        filters: hubFiltersForView(selectedFolder, searchQuery, activeSmart),
        excludedIds: [...hub.selection.excludedIds],
        selectionDigest: querySnapshot.queryDigest,
        dependencyRevisions: Object.entries(querySnapshot.dependencyRevisions).map(([domain, value]) => ({
          domain: domain as "structure" | "tags" | "lifecycle" | "engagement" | "artifacts" | "jobs" | "smart_collections" | "sort",
          value,
        })),
        evaluationAnchor: querySnapshot.evaluatedAt,
        evaluationTimezone: querySnapshot.timezone,
      };
    }
    return { kind: "explicit", paperIds: hub.selectedIds };
  };

  const reloadSmartCollections = async () => {
    if (!libraryClient) return;
    try {
      const result = await libraryClient.read({ kind: "smart_collections", protocolVersion: LIBRARY_PROTOCOL_VERSION });
      if (result.kind === "smart_collections") setSmartCollections(result.collections);
    } catch {
      /* keep the last list */
    }
  };

  const applyLifecyclePatch = async (patch: LifecyclePatch, paper?: DocumentCard) => {
    if (!libraryClient) return;
    const single = paper ?? ((hub.count === 1 && !hub.allMatching)
      ? documents.find((doc) => doc.id === hub.selectedIds[0])
      : undefined);
    if (single) {
      await libraryClient.act(changeRequest({
        kind: "patch_lifecycle",
        paperId: single.id,
        expectedVersion: single.lifecycleVersion ?? 0,
        patch,
      }));
      hub.clear();
      await onLibraryChanged?.();
      hub.showHint(t("hub.hint.lifecycleUpdated"));
      return;
    }
    await runCommand({ kind: "patch_lifecycle", patch }, currentTarget());
  };

  const runCommand = async (command: BatchCommand, target: BatchTarget, accept = false) => {
    if (!libraryClient) return;
    try {
      const run = await planAndStart(libraryClient.act.bind(libraryClient), command, target, {
        acceptRequirements: accept,
      });
      if (run.status === "needs_confirmation") {
        pendingStart.current = { plan: run.plan };
        setConfirmPlan(run.plan);
        return;
      }
      hub.clear();
      await onLibraryChanged?.();
      showBatchToast(batchOf(run.result), undoTokenOf(run.result));
    } catch (error) {
      const message = isActMessage(error);
      hub.showHint(message);
      setMoveError(message);
    }
  };

  const commitPaperMove = async (paperIds: readonly string[], targetFolder: string, accept = false) => {
    const ids = [...new Set(paperIds.filter(Boolean))];
    if (ids.length === 0) return;
    if (libraryClient) {
      await runCommand({ kind: "move", collectionPath: targetFolder }, { kind: "explicit", paperIds: ids }, accept);
      return;
    }
    if (ids.length !== 1 || !onMovePaper) {
      hub.showHint(hub.bulkDisabledReason || uiText(t, BULK_DISABLED_REASON));
      return;
    }
    const moving = paperById.get(ids[0]);
    setMoveError(null);
    await onMovePaper(ids[0], targetFolder);
    if (toastTimer.current) window.clearTimeout(toastTimer.current);
    setToast({
      kind: "paper",
      id: ids[0],
      fromPath: moving?.collection ?? "",
      toPath: targetFolder,
      fileName: moving?.fileName,
    });
    toastTimer.current = window.setTimeout(() => setToast(null), 8000) as unknown as number;
  };

  const confirmPending = async (acceptedIds: readonly string[]) => {
    if (!libraryClient || !pendingStart.current) return;
    setConfirmBusy(true);
    try {
      const result = await libraryClient.act(startBatchRequest(pendingStart.current.plan, acceptedIds));
      pendingStart.current = null;
      setConfirmPlan(null);
      hub.clear();
      await onLibraryChanged?.();
      showBatchToast(batchOf(result), undoTokenOf(result));
    } catch (error) {
      hub.showHint(isActMessage(error));
    } finally {
      setConfirmBusy(false);
    }
  };

  const handleSortChange = (mode: HubSortMode) => {
    if (mode === "manual" && !availability.manual) return;
    if (mode === "chapter" && !availability.chapter) return;
    if (!activeCollection) {
      // All-documents view: nothing to persist. The dropdown already shows
      // "recent" because `effectiveMode` resolves without an active
      // collection.
      return;
    }
    onSetSortMode?.(activeCollection.id, mode);
  };

  const sortTitle = (avail: { manual: boolean; chapter: boolean }): string => {
    if (!avail.manual) return t("hub.sortTitle.needFolder");
    if (!avail.chapter) return t("hub.sortTitle.needChapter");
    return t("hub.sortTitle.ok");
  };

  // context menu builders
  const buildPaperMenu = (doc: DocumentCard): MenuItem[] => {
    const actsOnSelection = hub.isSelected(doc.id) && hub.count > 1;
    const subject: DragSubject = {
      kind: "paper",
      id: doc.id,
      collection: doc.collection,
      memberIds: actsOnSelection ? hub.selectedIds : [doc.id],
      memberCollections: actsOnSelection ? hub.selectedIds.map(collectionOf) : [doc.collection],
      allMatching: hub.allMatching,
    };
    const headId = layer.ids[0];
    const frontPlan = hub.previewReorder(subject, { type: "reorder", anchorId: headId ?? null, place: "before" });
    const backPlan = hub.previewReorder(subject, { type: "reorder", anchorId: null, place: "end" });
    const localBulk = hub.bulkActionsEnabled;
    const items: MenuItem[] = [
      { label: t("hub.menu.open"), shortcut: "Enter", onClick: () => onSelectPaper(doc.revisionId, doc.id) },
      {
        label: actsOnSelection ? t("hub.menu.moveToCount", { count: hub.count }) : t("hub.moveTo"),
        shortcut: "M",
        disabledReason: actsOnSelection && !localBulk
          ? hub.bulkDisabledReason
          : (onMovePaper || localBulk ? undefined : t("hub.menu.moveNotWired")),
        onClick: () => {
          setMoveDialogSelection(actsOnSelection && localBulk);
          openMoveDialog(doc.id);
        },
      },
      {
        label: t("hub.menu.moveToFront"),
        disabledReason: frontPlan.allowed ? undefined : uiText(t, frontPlan.label),
        onClick: () => void hub.commitReorder(subject, { type: "reorder", anchorId: headId ?? null, place: "before" }),
      },
      {
        label: t("hub.menu.moveToBack"),
        disabledReason: backPlan.allowed ? undefined : uiText(t, backPlan.label),
        onClick: () => void hub.commitReorder(subject, { type: "reorder", anchorId: null, place: "end" }),
      },
      {
        label: actsOnSelection ? t("hub.menu.tagsBulk") : t("hub.menu.tags"),
        disabledReason: actsOnSelection && !localBulk ? hub.bulkDisabledReason : (actsOnSelection ? undefined : t("hub.disabled.singleTag")),
        onClick: () => { if (actsOnSelection && localBulk) setTagDialogOpen(true); },
      },
      {
        label: actsOnSelection ? t("hub.menu.statusBulk") : t("hub.menu.status"),
        disabledReason: libraryClient ? undefined : uiText(t, LIFECYCLE_DISABLED_REASON),
        onClick: () => {
          setLifecyclePaper(actsOnSelection ? null : doc);
          setLifecycleDialogOpen(true);
        },
      },
      {
        label: actsOnSelection ? t("hub.menu.ocrBulk") : t("hub.menu.ocr"),
        disabledReason: actsOnSelection && !localBulk ? hub.bulkDisabledReason : (libraryClient ? undefined : hub.bulkDisabledReason),
        onClick: () => { if (libraryClient) void runCommand({ kind: "ocr" }, actsOnSelection ? currentTarget() : { kind: "explicit", paperIds: [doc.id] }); },
      },
      {
        label: actsOnSelection ? t("hub.menu.briefBulk") : t("hub.menu.brief"),
        disabledReason: actsOnSelection && !localBulk ? hub.bulkDisabledReason : (libraryClient ? undefined : hub.bulkDisabledReason),
        onClick: () => { if (libraryClient) void runCommand({ kind: "brief" }, actsOnSelection ? currentTarget() : { kind: "explicit", paperIds: [doc.id] }); },
      },
      {
        label: actsOnSelection ? t("hub.menu.exportBulk") : t("hub.menu.export"),
        disabledReason: actsOnSelection && !localBulk ? hub.bulkDisabledReason : (actsOnSelection ? undefined : t("hub.disabled.singleExport")),
        onClick: () => { if (actsOnSelection && localBulk) void runCommand({ kind: "export", format: "reading_bundle" }, currentTarget()); },
      },
    ];
    if (!actsOnSelection) {
      items.push(
        { label: t("hub.menu.editPaperContext"), onClick: () => onEditReaderContext?.({ scope: "paper", paperId: doc.id }) },
        { label: t("hub.menu.renameFile"), shortcut: "F2", onClick: () => { const stem = doc.fileName ? doc.fileName.replace(/\.pdf$/i, "") : doc.title; setEditingPaperId(doc.id); setEditingPaperValue(stem); setEditingError(null); } },
        { label: t("hub.menu.openResource"), onClick: () => onOpenResource?.(doc.pdfPath) },
        { label: t("hub.menu.trash"), shortcut: "Delete", danger: true, onClick: () => onTrashPaper?.(doc.id) },
      );
    } else {
      items.push({
        label: t("hub.menu.trashCount", { count: hub.count }),
        danger: true,
        disabledReason: localBulk ? undefined : hub.bulkDisabledReason,
        onClick: () => { if (localBulk) void runCommand({ kind: "trash" }, currentTarget()); },
      });
    }
    return items;
  };

  const buildFolderMenu = (folder: string): MenuItem[] => {
    const smartId = smartCollectionIdOf(folder);
    if (smartId) {
      const collection = smartCollections.find((item) => item.id === smartId);
      const items: MenuItem[] = [
        { label: t("hub.menu.openSmart"), onClick: () => setSelectedFolder(folder) },
      ];
      if (collection && !collection.builtin && libraryClient) {
        items.push(
          {
            label: t("hub.menu.rename"),
            onClick: () => {
              const name = window.prompt(t("hub.prompt.smartName"), collection.name)?.trim();
              if (!name) return;
              void libraryClient.act(changeRequest({ kind: "rename_smart_collection", id: collection.id, name }))
                .then(() => reloadSmartCollections())
                .catch((error) => hub.showHint(isActMessage(error)));
            },
          },
          {
            label: t("hub.menu.delete"),
            danger: true,
            onClick: () => {
              if (!window.confirm(t("hub.confirm.deleteSmart", { name: collection.name }))) return;
              void libraryClient.act(changeRequest({ kind: "delete_smart_collection", id: collection.id }))
                .then(() => {
                  if (selectedFolder === folder) setSelectedFolder("");
                  return reloadSmartCollections();
                })
                .catch((error) => hub.showHint(isActMessage(error)));
            },
          },
        );
      }
      return items;
    }
    if (isAllFolder(folder)) {
      return [
        { label: t("hub.menu.editAllContext"), onClick: () => onEditReaderContext?.({ scope: "workspace" }) },
        { label: t("hub.menu.importPickKind"), onClick: () => setImportPickerOpen(true) },
        { label: t("hub.menu.openWorkspace"), onClick: () => onOpenWorkspace?.() },
      ];
    }
    if (isRootFolder(folder)) {
      return [
        { label: t("hub.menu.editFolderContext"), onClick: () => onEditReaderContext?.({ scope: "folder", collectionPath: folder }) },
        { label: t("hub.menu.addSubfolder"), onClick: () => handleAddSubdir(folder) },
        { label: t("hub.menu.importPdf"), onClick: () => requestImport(`${folder}/Inbox`) },
        { label: t("hub.menu.openResource"), onClick: () => onOpenResource?.(folder) },
      ];
    }
    return [
      { label: t("hub.menu.editFolderContext"), onClick: () => onEditReaderContext?.({ scope: "folder", collectionPath: folder }) },
      { label: t("hub.menu.renameFolder"), shortcut: "F2", onClick: () => { const name = folder.split("/").pop() || folder; setEditingFolder(folder); setEditingFolderValue(name); setEditingError(null); } },
      { label: t("hub.menu.addSubfolder"), onClick: () => handleAddSubdir(folder) },
      { label: t("hub.menu.importPdf"), onClick: () => requestImport(folder) },
      { label: t("hub.menu.openResource"), onClick: () => onOpenResource?.(folder) },
      { label: t("hub.menu.trash"), shortcut: "Delete", danger: true, onClick: () => onTrashFolder?.(folder) },
    ];
  };

  const handleFolderContext = (e: React.MouseEvent, folder: string) => {
    e.preventDefault();
    setSelectedFolder(folder);
    setMenu({ x: e.clientX, y: e.clientY, items: buildFolderMenu(folder) });
  };

  // On a right-click of an unselected paper, first move the action context to that paper; if already selected, act on the selection.
  const handleDocContext = (e: React.MouseEvent, doc: DocumentCard) => {
    e.preventDefault();
    if (!hub.isSelected(doc.id)) hub.focusRow(doc.id);
    setMenu({ x: e.clientX, y: e.clientY, items: buildPaperMenu(doc) });
  };

  const handleBlankContext = (e: React.MouseEvent) => {
    const t = e.target as HTMLElement;
    if (t.closest(".folder-pill-node") || t.closest(".liquid-paper-card") || t.closest(".liquid-table-row")) return;
    e.preventDefault();
    setMenu({ x: e.clientX, y: e.clientY, items: buildFolderMenu(selectedFolder) });
  };

  const submitFolderRename = () => {
    if (!editingFolder) return;
    const v = editingFolderValue.trim();
    if (!v) { setEditingError(t("hub.error.folderEmpty")); return; }
    if (/[\/\\:\*\?"<>\|]/.test(v) || v === "." || v === ".." || v.endsWith(" ") || v.endsWith(".") || isWindowsReserved(v)) { setEditingError(t("hub.error.folderInvalid")); return; }
    onRenameFolder?.(editingFolder, v);
    setEditingFolder(null);
  };
  const submitPaperRename = () => {
    if (!editingPaperId) return;
    const v = editingPaperValue.trim();
    if (!v) { setEditingError(t("hub.error.fileEmpty")); return; }
    if (/[\/\\:\*\?"<>\|]/.test(v) || v === "." || v === ".." || v.endsWith(" ") || v.endsWith(".") || isWindowsReserved(v)) { setEditingError(t("hub.error.fileInvalid")); return; }
    onRenamePaper?.(editingPaperId, v);
    setEditingPaperId(null);
  };

  // Shortcuts: F2 / Delete / M are handled here; Ctrl+A, Space, Enter, Alt+↑/↓ go through the list container onKeyDown.
  const hubRef = useRef(hub);
  hubRef.current = hub;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (isTextEntry(e.target)) return;
      if (menu || moveDialogPaperId) return;
      const api = hubRef.current;
      if (api.coachMarkOpen) return;
      if (e.key === "F2") {
        e.preventDefault();
        const focusedDoc = documents.find((d) => d.id === api.focusedId);
        if (focusedDoc) {
          const stem = focusedDoc.fileName ? focusedDoc.fileName.replace(/\.pdf$/i, "") : focusedDoc.title;
          setEditingPaperId(focusedDoc.id); setEditingPaperValue(stem); setEditingError(null);
        } else if (isNormalFolder(selectedFolder)) {
          const name = selectedFolder.split("/").pop() || selectedFolder;
          setEditingFolder(selectedFolder); setEditingFolderValue(name); setEditingError(null);
        }
      } else if (e.key === "Delete") {
        const focusedDoc = api.focusedId ? documents.find((d) => d.id === api.focusedId) : null;
        if (focusedDoc && (api.count <= 1)) onTrashPaper?.(focusedDoc.id);
        else if (focusedDoc && api.bulkActionsEnabled && api.canSubmit) void runCommand({ kind: "trash" }, currentTarget());
        else if (focusedDoc) api.showHint(api.bulkDisabledReason);
        else if (isNormalFolder(selectedFolder)) onTrashFolder?.(selectedFolder);
      } else if ((e.key === "m" || e.key === "M") && !e.ctrlKey && !e.metaKey && !e.altKey) {
        const focusedDoc = api.focusedId ?? (api.count === 1 ? api.selectedIds[0] : null);
        if (!focusedDoc) return;
        e.preventDefault();
        if (api.allMatching && !api.canSubmit) { api.showHint(uiText(t, SNAPSHOT_DISABLED_REASON)); return; }
        if (api.count > 1 && !api.bulkActionsEnabled) { api.showHint(api.bulkDisabledReason); return; }
        setMoveDialogSelection(api.count > 1 || api.allMatching);
        openMoveDialog(focusedDoc);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [documents, menu, moveDialogPaperId, onTrashFolder, onTrashPaper, selectedFolder, t]);

  const highlight = (next: {
    dropTarget: string | null;
    dropForbidden: string | null;
    stageDropActive: boolean;
    insert: { targetId: string | null; place: "before" | "after" | "end" } | null;
    target: DropTarget;
    reason?: DropReason;
    feedback: DragFeedback;
  }) => {
    const st = dragState.current;
    if (st) {
      st.dropTarget = next.dropTarget;
      st.dropForbidden = next.dropForbidden;
      st.dropReason = next.reason ?? null;
      st.stageDropActive = next.stageDropActive;
      st.target = next.target;
    }
    setDropTarget(next.dropTarget);
    setDropForbidden(next.dropForbidden);
    setStageDropActive(next.stageDropActive);
    setInsert(next.insert);
    setDragFeedback(next.feedback);
  };

  const handlePointerDown = (e: React.PointerEvent, kind: "folder" | "paper", id: string, collection: string, fileName?: string) => {
    if (e.button !== 0) return;
    if (isRootFolder(id) || isAllFolder(id)) return;
    dragState.current = {
      originX: e.clientX,
      originY: e.clientY,
      id,
      kind,
      collection,
      fileName,
      moved: false,
      target: { type: "none" },
      dropTarget: null,
      dropForbidden: null,
      dropReason: null,
      stageDropActive: false,
    };
    setMoveError(null);
    setDragFeedback({
      tone: "info",
      text: kind === "paper"
        ? t("hub.drag.paperHint")
        : t("hub.drag.folderHint"),
    });
    (e.currentTarget as Element).setPointerCapture?.(e.pointerId);
  };

  const folderSubjectFor = (path: string): DragSubject => ({
    kind: "folder",
    id: path,
    collection: path,
    memberIds: [],
    memberCollections: [],
    allMatching: false,
  });

  const handlePointerMove = (e: React.PointerEvent) => {
    const st = dragState.current;
    if (!st) return;
    const dx = e.clientX - st.originX;
    const dy = e.clientY - st.originY;
    const dist = Math.hypot(dx, dy);
    if (!st.moved && dist < HUB_DRAG_THRESHOLD_PX) return;
    if (!st.moved) {
      st.moved = true;
      suppressNextClickRef.current = true;
    }
    const label = st.kind === "folder" ? st.id.split("/").pop() || st.id : st.fileName || st.id;
    setDragging({ kind: st.kind, label, x: e.clientX, y: e.clientY });
    const el = document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null;
    const pill = el?.closest?.(".folder-pill-node") as HTMLElement | null;
    const subject = hub.dragSubjectFor(st.id, st.kind === "paper" && hub.isSelected(st.id) && hub.count > 1);

    if (pill && typeof pill.dataset.folder === "string") {
      const target = pill.dataset.folder;
      const smart = pill.dataset.smart === "true";
      const evaluation = st.kind === "folder"
        ? hub.evaluate(folderSubjectFor(st.id), { type: "collection", path: target, smart })
        : hub.evaluate(subject, { type: "collection", path: target, smart });
      if (evaluation.allowed && evaluation.action.kind === "move_to_folder") {
        highlight({
          dropTarget: target,
          dropForbidden: null,
          stageDropActive: false,
          insert: null,
          target: { type: "collection", path: target },
          feedback: { tone: "valid", text: st.kind === "folder" ? t("hub.drag.moveFolderTo", { target }) : t("hub.drag.moveTo", { target }) },
        });
      } else {
        highlight({
          dropTarget: null,
          dropForbidden: target,
          stageDropActive: false,
          insert: null,
          target: { type: "none" },
          reason: !evaluation.allowed ? evaluation.reason : undefined,
          feedback: { tone: "invalid", text: !evaluation.allowed ? dropLabel(evaluation.reason, t) : t("hub.drag.cannotDrop") },
        });
      }
    } else {
      const reorderCandidate = st.kind === "paper";
      const card = el?.closest?.(".liquid-paper-card, .liquid-table-row") as HTMLElement | null;
      const anchorId = card?.dataset?.paperId ?? null;
      const onOwnItem = anchorId !== null && anchorId === st.id;
      if (reorderCandidate && anchorId && !onOwnItem) {
        const rect = card!.getBoundingClientRect();
        const axis: "x" | "y" = card!.classList.contains("liquid-paper-card") ? "x" : "y";
        const place = insertIndexFromRect(rect, e.clientX, e.clientY, axis);
        const target: DropTarget = { type: "reorder", anchorId, place };
        const evaluation = hub.evaluate(subject, target);
        if (evaluation.allowed && evaluation.action.kind === "reorder") {
          const anchorTitle = documents.find((doc) => doc.id === anchorId)?.title || t("hub.drag.anchorFallback");
          const placeLabel = place === "before" ? t("hub.drag.placeBefore") : place === "after" ? t("hub.drag.placeAfter") : t("hub.drag.placeEnd");
          highlight({
            dropTarget: null,
            dropForbidden: null,
            stageDropActive: false,
            insert: { targetId: anchorId, place },
            target,
            feedback: { tone: "valid", text: t("hub.drag.reorderAt", { title: anchorTitle, place: placeLabel }) },
          });
        } else {
          highlight({
            dropTarget: null,
            dropForbidden: null,
            stageDropActive: false,
            insert: null,
            target: { type: "none" },
            reason: !evaluation.allowed ? evaluation.reason : undefined,
            feedback: { tone: "invalid", text: !evaluation.allowed ? dropLabel(evaluation.reason, t) : t("hub.drag.cannotReorder") },
          });
        }
      } else if (reorderCandidate && el?.closest?.(".paper-cards-grid, .liquid-table-container") && !onOwnItem) {
        const overItem = Boolean(el.closest(".liquid-paper-card") || el.closest(".liquid-table-row"));
        if (overItem) {
          highlight({
            dropTarget: null,
            dropForbidden: null,
            stageDropActive: false,
            insert: null,
            target: { type: "none" },
            feedback: { tone: "info", text: t("hub.drag.dropOnCards") },
          });
        } else {
          const firstId = visibleIds[0] ?? null;
          const firstEl = firstId
            ? (listRef.current?.querySelector(`[data-paper-id="${firstId}"]`) as HTMLElement | null)
            : null;
          const isGrid = Boolean(el.closest(".paper-cards-grid"));
          const firstRect = firstEl?.getBoundingClientRect();
          const leading = Boolean(
            firstId &&
              firstId !== st.id &&
              firstRect &&
              (isGrid ? e.clientX < firstRect.left : e.clientY < firstRect.top),
          );
          const target: DropTarget = leading && firstId
            ? { type: "reorder", anchorId: firstId, place: "before" }
            : { type: "reorder", anchorId: null, place: "end" };
          const evaluation = hub.evaluate(subject, target);
          if (evaluation.allowed) {
            highlight({
              dropTarget: null,
              dropForbidden: null,
              stageDropActive: false,
              insert: leading && firstId
                ? { targetId: firstId, place: "before" }
                : { targetId: null, place: "end" },
              target,
              feedback: { tone: "valid", text: leading ? t("hub.drag.reorderFront") : t("hub.drag.reorderBack") },
            });
          } else {
            highlight({
              dropTarget: null,
              dropForbidden: null,
              stageDropActive: false,
              insert: null,
              target: { type: "none" },
              reason: !evaluation.allowed ? evaluation.reason : undefined,
              feedback: { tone: "invalid", text: !evaluation.allowed ? dropLabel(evaluation.reason, t) : t("hub.drag.cannotReorder") },
            });
          }
        }
      } else if (
        el?.closest?.(".main-glass-stage") &&
        !el.closest(".liquid-paper-card") &&
        !el.closest(".liquid-table-row") &&
        !isAllFolder(selectedFolder)
      ) {
        const evaluation = st.kind === "folder"
          ? hub.evaluate(folderSubjectFor(st.id), { type: "collection", path: selectedFolder })
          : hub.evaluate(subject, { type: "collection", path: selectedFolder });
        if (evaluation.allowed) {
          highlight({
            dropTarget: null,
            dropForbidden: null,
            stageDropActive: true,
            insert: null,
            target: { type: "collection", path: selectedFolder },
            feedback: { tone: "valid", text: t("hub.drag.moveToCurrent", { folder: selectedFolder }) },
          });
        } else {
          highlight({
            dropTarget: null,
            dropForbidden: null,
            stageDropActive: false,
            insert: null,
            target: { type: "none" },
            reason: !evaluation.allowed ? evaluation.reason : undefined,
            feedback: { tone: "invalid", text: !evaluation.allowed ? dropLabel(evaluation.reason, t) : t("hub.drag.cannotDrop") },
          });
        }
      } else {
        highlight({
          dropTarget: null,
          dropForbidden: null,
          stageDropActive: false,
          insert: null,
          target: { type: "none" },
          feedback: { tone: "info", text: st.kind === "paper" ? t("hub.drag.paperHint") : t("hub.drag.folderHint") },
        });
      }
    }
    const sidebar = document.querySelector(".sidebar-glass-island") as HTMLElement | null;
    if (sidebar && pill) {
      const r = sidebar.getBoundingClientRect();
      if (e.clientY < r.top + 40) sidebar.scrollTop -= 6;
      else if (e.clientY > r.bottom - 40) sidebar.scrollTop += 6;
    }
    const list = listRef.current;
    if (list) {
      const r = list.getBoundingClientRect();
      if (e.clientY < r.top + 40) list.scrollTop -= 6;
      else if (e.clientY > r.bottom - 40) list.scrollTop += 6;
    }
  };

  const clearDragVisuals = () => {
    setDragging(null);
    setDragFeedback(null);
    setDropTarget(null);
    setDropForbidden(null);
    setStageDropActive(false);
    setInsert(null);
  };

  const abortDrag = () => {
    dragState.current = null;
    clearDragVisuals();
    setTimeout(() => { suppressNextClickRef.current = false; }, 350);
  };

  const finishDrag = async () => {
    const st = dragState.current;
    if (!st) {
      clearDragVisuals();
      return;
    }
    const wasDragging = st.moved;
    const currentTarget = st.target;
    const currentDropTarget = st.dropTarget;
    const currentDropForbidden = st.dropForbidden;
    const currentDropReason = st.dropReason;
    const currentStageActive = st.stageDropActive;
    dragState.current = null;
    clearDragVisuals();
    if (!wasDragging) return;
    if (st.kind === "paper") hub.focusRow(st.id);
    else setSelectedFolder(st.id);
    setTimeout(() => { suppressNextClickRef.current = false; }, 350);

    if (currentTarget.type === "reorder" && st.kind === "paper") {
      const subject = hub.dragSubjectFor(st.id, hub.count > 1 && hub.isSelected(st.id));
      const outcome = await hub.commitReorder(subject, currentTarget);
      if (!outcome.ok && outcome.message) hub.showHint(outcome.message);
      return;
    }

    // A cross-root paper drop is a confirmation flow, not a silent no-op.
    if (currentDropReason === "kind_change_requires_dialog" && st.kind === "paper" && currentDropForbidden) {
      const subject = hub.dragSubjectFor(st.id, hub.count > 1 && hub.isSelected(st.id));
      if (subject.memberIds.length > 1 && !libraryClient) {
        hub.showHint(hub.bulkDisabledReason || uiText(t, BULK_DISABLED_REASON));
        return;
      }
      setMoveError(null);
      setMoveDialogSelection(subject.memberIds.length > 1);
      openMoveDialog(st.id, currentDropForbidden);
      return;
    }
    if (currentDropReason) {
      hub.showHint(dropLabel(currentDropReason, t));
      return;
    }

    const targetFolder = currentTarget.type === "collection" && currentStageActive
      ? selectedFolder
      : currentDropTarget;
    if (!targetFolder) return;
    try {
      if (st.kind === "paper") {
        const subject = hub.dragSubjectFor(st.id, hub.count > 1 && hub.isSelected(st.id));
        const movingIds = subject.memberIds.length > 0 ? subject.memberIds : [st.id];
        if (movingIds.length > 1 && !libraryClient) {
          hub.showHint(hub.bulkDisabledReason || uiText(t, BULK_DISABLED_REASON));
          return;
        }
        await commitPaperMove(movingIds, targetFolder);
      } else if (st.kind === "folder" && onMoveFolder) {
        const name = st.id.split("/").pop() || st.id;
        const newPath = `${targetFolder}/${name}`;
        setMoveError(null);
        await onMoveFolder(st.id, targetFolder);
        if (toastTimer.current) window.clearTimeout(toastTimer.current);
        setToast({ kind: "folder", id: st.id, fromPath: st.id, toPath: targetFolder, newPath });
        toastTimer.current = window.setTimeout(() => setToast(null), 8000) as unknown as number;
      }
    } catch (error) {
      const message = t("hub.error.moveFailed", { error: String(error) });
      setMoveError(message);
      hub.showHint(message);
    }
  };

  const handleUndo = async () => {
    if (!toast || undoBusy) return;
    const pending = toast;
    setUndoBusy(true);
    setMoveError(null);
    try {
      if (pending.kind === "paper" && onMovePaper) {
        await onMovePaper(pending.id, pending.fromPath);
      } else if (pending.kind === "folder" && onMoveFolder) {
        const newPath = pending.newPath || pending.id;
        await onMoveFolder(newPath, parentOf(pending.fromPath));
      } else {
        throw new Error(t("hub.error.undoUnavailable"));
      }
      setToast(null);
      if (toastTimer.current) { window.clearTimeout(toastTimer.current); toastTimer.current = null; }
    } catch (error) {
      const message = t("hub.error.undoFailed", { error: String(error) });
      setMoveError(message);
      hub.showHint(message);
    } finally {
      setUndoBusy(false);
    }
  };

  const moveDialogPaper = moveDialogPaperId ? paperById.get(moveDialogPaperId) ?? null : null;

  const listSize = libraryClient ? hubPage.totalCount || filteredDocuments.length : filteredDocuments.length;
  const listColumns = viewType === "grid" ? gridColumnCount(listMetrics.width) : 1;
  const focusedIndex = hub.focusedId ? visibleIds.indexOf(hub.focusedId) : -1;
  const windowed = virtualWindow({
    count: filteredDocuments.length,
    scrollTop: listMetrics.scrollTop,
    viewportHeight: listMetrics.height,
    itemHeight: viewType === "grid" ? HUB_GRID_CARD_HEIGHT : HUB_TABLE_ROW_HEIGHT,
    columns: listColumns,
    ensureIndex: focusedIndex >= 0 ? focusedIndex : null,
  });
  const renderedDocuments = filteredDocuments.slice(windowed.start, windowed.end);

  useEffect(() => {
    if (!hub.focusedId || !listRef.current) return;
    const row = listRef.current.querySelector(`[data-paper-id="${hub.focusedId}"]`);
    if (row instanceof HTMLElement && typeof row.scrollIntoView === "function") {
      row.scrollIntoView({ block: "nearest" });
    }
  }, [hub.focusedId, windowed.start, windowed.end]);

  useEffect(() => {
    if (!libraryClient || !hubPage.hasMore || hubPage.loading || !windowed.virtualized) return;
    if (windowed.end < filteredDocuments.length) return;
    hubPage.loadMore();
  }, [filteredDocuments.length, hubPage.hasMore, hubPage.loading, hubPage.loadMore, libraryClient, windowed.end, windowed.virtualized]);

  const rowTabIndex = (doc: DocumentCard) => (hub.focusedId ? (hub.focusedId === doc.id ? 0 : -1) : (visibleIds[0] === doc.id ? 0 : -1));

  const focused = (doc: DocumentCard) => hub.focusedId === doc.id;

  const lastVisibleId = visibleIds[visibleIds.length - 1] ?? "";
  const renderCard = (doc: DocumentCard, index: number) => {
    const selected = hub.isSelected(doc.id);
    const isEditing = editingPaperId === doc.id;
    const insertClass = insert?.targetId === doc.id
      ? (insert.place === "before" ? "insert-before" : "insert-after")
      : (insert?.targetId === null && insert.place === "end" && doc.id === lastVisibleId ? "insert-after" : "");
    return (
      <div
        key={doc.revisionId}
        data-paper-id={doc.id}
        role="option"
        aria-selected={selected}
        aria-setsize={listSize}
        aria-posinset={index + 1}
        tabIndex={rowTabIndex(doc)}
        className={`liquid-paper-card ${selected ? "selected" : ""} ${focused(doc) ? "focused" : ""} ${dragging?.label === doc.fileName ? "dragging" : ""} ${insertClass}`}
        onFocus={() => hub.focusRow(doc.id)}
        onClick={(e) => {
          if (suppressNextClickRef.current) { suppressNextClickRef.current = false; return; }
          if (isEditing) return;
          const outcome = hub.clickRow(doc.id, { ctrl: e.ctrlKey, shift: e.shiftKey, meta: e.metaKey });
          if (outcome === "open") onSelectPaper(doc.revisionId, doc.id);
        }}
        onContextMenu={(e) => handleDocContext(e, doc)}
        title={t("hub.openForStudy", { title: doc.title })}
      >
        <div className="hub-card-controls">
            <button
              type="button"
              className="hub-drag-handle"
              aria-label={t("hub.dragHandleAria", { title: doc.title })}
              title={t("hub.dragHandle")}
              onPointerDown={(e) => { e.stopPropagation(); handlePointerDown(e, "paper", doc.id, doc.collection, doc.fileName); }}
            onClick={(e) => e.stopPropagation()}
          >⋮⋮</button>
          <label className="hub-select-hit" onClick={(e) => e.stopPropagation()}>
            <input
              type="checkbox"
              className="hub-select-checkbox"
              checked={selected}
              aria-label={t("hub.selectDoc", { title: doc.title || t("hub.untitledDocument") })}
              onChange={() => hub.toggleSelected(doc.id)}
            />
          </label>
        </div>
        <div className="liquid-card-main-content">
          <div className="hub-card-meta-bar" style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 6, flexWrap: "wrap", marginBottom: 6 }}>
            <div className="hub-card-meta-chips" style={{ display: "flex", alignItems: "center", gap: 5, flexWrap: "wrap", minWidth: 0 }}>
              <span style={{ fontSize: 10, fontWeight: 600, padding: "2px 6px", borderRadius: 9999, background: "var(--glass-card)", color: "var(--muted)", border: "1px solid var(--glass-border)", flexShrink: 0 }}>{doc.kind === "textbook" ? t("hub.kind.textbook") : t("hub.kind.paper")}</span>
              {doc.kind === "textbook" && doc.chapterNumber ? (<span style={{ fontSize: 10, fontWeight: 600, padding: "2px 6px", borderRadius: 9999, background: "var(--glass-card)", color: "var(--accent, #4f8cff)", border: "1px solid var(--glass-border)", flexShrink: 0 }}>📑 {doc.chapterNumber.startsWith(t("hub.chapterPrefix")) ? doc.chapterNumber : t("hub.chapterBadge", { n: doc.chapterNumber })}</span>) : null}
              <span className="hub-lifecycle-chip">{t(STATUS_KEY[(doc.lifecycleStatus ?? "unread") as ReadingLifecycleStatus])}</span>
              {doc.favorite ? <span className="hub-lifecycle-chip">{t("hub.favorite")}</span> : null}
              {doc.readLater ? <span className="hub-lifecycle-chip">{t("hub.readLaterShort")}</span> : null}
              {doc.priority ? <span className="hub-lifecycle-chip">{t("hub.priority", { n: doc.priority })}</span> : null}
              {hubDate(doc.reviewAt) ? <span className="hub-lifecycle-chip">{t("hub.review", { date: hubDate(doc.reviewAt) ?? "" })}</span> : null}
              {doc.ocrFailed ? <span className="hub-lifecycle-chip hub-lifecycle-warn">{t("hub.ocrFailed")}</span> : null}
            </div>
            <span style={{ fontSize: 10.5, fontWeight: 600, padding: "2px 8px", borderRadius: 9999, background: doc.hasOcr || doc.briefStatus === "ready" ? "var(--success-bg)" : "rgba(217, 119, 6, 0.1)", color: doc.hasOcr || doc.briefStatus === "ready" ? "var(--success)" : "#d97706", border: `1px solid ${doc.hasOcr || doc.briefStatus === "ready" ? "var(--success)" : "#d97706"}`, flexShrink: 0, marginLeft: "auto" }}>{doc.hasOcr ? t("hub.ocrReady") : doc.briefStatus === "ready" ? t("hub.briefReady") : t("hub.pendingAnalysis")}</span>
          </div>
          <div className="hub-card-title-wrap" style={{ display: "flex", alignItems: "flex-start", width: "100%" }}>
            {isEditing ? (
              <div style={{ display: "flex", alignItems: "center", gap: 4, flex: 1 }} onClick={(e)=>e.stopPropagation()}>
                <input className={`hub-inline-rename ${editingError ? "hub-inline-error" : ""}`} autoFocus value={editingPaperValue} onChange={(e) => setEditingPaperValue(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") submitPaperRename(); if (e.key === "Escape") setEditingPaperId(null); }} onBlur={submitPaperRename} style={{ flex: 1, minWidth: 80 }} />
                <span style={{ fontSize: 12, color: "var(--muted)", userSelect: "none" }}>.pdf</span>
              </div>
            ) : (
              <span className="hub-card-title" style={{ fontSize: 14.5, fontWeight: 700, color: "var(--ink)", lineHeight: 1.35, wordBreak: "break-word" }}>{doc.title || (doc.kind === "textbook" ? t("hub.untitledChapter") : t("hub.untitledPaper"))}</span>
            )}
          </div>
          <div style={{ fontSize: 12, color: "var(--muted)", marginTop: 4 }}>{doc.authors || t("hub.unknownAuthors")} · {doc.year || "—"} {doc.yearLabel ?? ""}</div>
          <div className="liquid-summary-wrapper">
            <div className="liquid-summary-lens"><strong>{doc.kind === "textbook" ? t("hub.takeaway.learning") : t("hub.takeaway.contribution")}</strong><span>{doc.briefTakeaway || (doc.kind === "textbook" ? t("hub.takeaway.textbookPlaceholder") : t("hub.takeaway.paperPlaceholder"))}</span></div>
            {doc.briefTakeaway ? (<div className="liquid-summary-popover"><div className="liquid-summary-popover-header">{doc.kind === "textbook" ? t("hub.takeaway.learningFull") : t("hub.takeaway.contributionFull")}</div><div className="liquid-summary-popover-body">{doc.briefTakeaway}</div></div>) : null}
          </div>
          <div className="liquid-card-keywords" onClick={(e) => e.stopPropagation()}>
            {doc.keywords && doc.keywords.length > 0 ? doc.keywords.map((kw, idx) => (<span key={kw + "-" + idx} className="liquid-keyword-chip" onClick={(e) => { e.stopPropagation(); setSearchQuery(kw); }} title={t("hub.filterByTag", { tag: kw })}><span>{kw}</span>{onUpdateTags ? (<button type="button" className="chip-remove-btn" onClick={(e) => { e.stopPropagation(); const next = (doc.keywords ?? []).filter((k) => k !== kw); void onUpdateTags(doc.id, next); }} title={t("hub.removeTag")} aria-label={t("hub.removeTagAria", { tag: kw })}>×</button>) : null}</span>)) : null}
            {onUpdateTags ? (addingTagDocId === doc.id ? (<span className="liquid-tag-input-wrap" onClick={(e) => e.stopPropagation()}><input autoFocus className="liquid-tag-inline-input" placeholder={t("hub.newTagPlaceholder")} value={newTagInput} onChange={(e) => setNewTagInput(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") { const clean = newTagInput.trim(); if (clean && !(doc.keywords ?? []).includes(clean)) { void onUpdateTags(doc.id, [...(doc.keywords ?? []), clean]); } setAddingTagDocId(null); setNewTagInput(""); } if (e.key === "Escape") { setAddingTagDocId(null); setNewTagInput(""); } }} onBlur={() => { const clean = newTagInput.trim(); if (clean && !(doc.keywords ?? []).includes(clean)) { void onUpdateTags(doc.id, [...(doc.keywords ?? []), clean]); } setAddingTagDocId(null); setNewTagInput(""); }} /></span>) : (<button type="button" className="liquid-add-tag-chip-btn" onClick={(e) => { e.stopPropagation(); setAddingTagDocId(doc.id); setNewTagInput(""); }} title={t("hub.addTag")} aria-label={t("hub.addTag")}>+</button>)) : null}
          </div>
        </div>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", fontSize: 11.5, color: "var(--muted)", borderTop: "1px solid var(--glass-border-subtle)", paddingTop: 8, marginTop: 8 }}>
          <span>{t("hub.pages", { count: doc.pages })} {doc.furthestPage ? t("hub.readToPage", { page: doc.furthestPage }) : doc.lastReadPage ? t("hub.readToPage", { page: doc.lastReadPage }) : ""} {hubDate(doc.lastOpenedAt) ? t("hub.lastOpened", { date: hubDate(doc.lastOpenedAt) ?? "" }) : ""} {doc.collection ? `· 📁 ${doc.collection.split("/").pop()}` : ""}</span>
          <span style={{ fontSize: 11, color: "var(--muted-plus)" }}>{doc.importedAt ? doc.importedAt.slice(0, 10) : ""}</span>
        </div>
      </div>
    );
  };

  const renderRow = (doc: DocumentCard, index: number) => {
    const selected = hub.isSelected(doc.id);
    const isEditing = editingPaperId === doc.id;
    const insertClass = insert?.targetId === doc.id ? (insert.place === "before" ? "insert-before" : "insert-after") : "";
    return (
      <div
        key={doc.revisionId}
        data-paper-id={doc.id}
        role="option"
        aria-selected={selected}
        aria-setsize={listSize}
        aria-posinset={index + 1}
        tabIndex={rowTabIndex(doc)}
        className={`liquid-table-row ${selected ? "selected" : ""} ${focused(doc) ? "focused" : ""} ${insertClass}`}
        onFocus={() => hub.focusRow(doc.id)}
        onClick={(e) => {
          if (suppressNextClickRef.current) { suppressNextClickRef.current = false; return; }
          if (isEditing) return;
          const outcome = hub.clickRow(doc.id, { ctrl: e.ctrlKey, shift: e.shiftKey, meta: e.metaKey });
          if (outcome === "open") onSelectPaper(doc.revisionId, doc.id);
        }}
        onContextMenu={(e) => handleDocContext(e, doc)}
      >
          <button
            type="button"
            className="hub-drag-handle"
            aria-label={t("hub.dragHandleAria", { title: doc.title })}
            title={t("hub.dragHandle")}
            onPointerDown={(e) => { e.stopPropagation(); handlePointerDown(e, "paper", doc.id, doc.collection, doc.fileName); }}
          onClick={(e) => e.stopPropagation()}
        >⋮⋮</button>
        <label className="hub-select-hit" onClick={(e) => e.stopPropagation()}>
          <input
            type="checkbox"
            className="hub-select-checkbox"
            checked={selected}
            aria-label={t("hub.selectDoc", { title: doc.title || t("hub.untitledDocument") })}
            onChange={() => hub.toggleSelected(doc.id)}
          />
        </label>
        {isEditing ? (<div style={{ display: "flex", alignItems: "center", gap: 4, flex: 1 }} onClick={(e)=>e.stopPropagation()}><input className={`hub-inline-rename ${editingError ? "hub-inline-error" : ""}`} autoFocus value={editingPaperValue} onChange={(e) => setEditingPaperValue(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") submitPaperRename(); if (e.key === "Escape") setEditingPaperId(null); }} onBlur={submitPaperRename} style={{ flex: 1 }} /><span style={{ fontSize: 11, color: "var(--muted)" }}>.pdf</span></div>) : (<strong style={{ color: "var(--ink)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{doc.title || (doc.kind === "textbook" ? t("hub.untitledChapter") : t("hub.untitledPaper"))}</strong>)}
        <span className="liquid-summary-wrapper">
          <span className="liquid-summary-lens">{doc.briefTakeaway || t("hub.tableBriefPlaceholder")}</span>
          {doc.briefTakeaway ? (
            <div className="liquid-summary-popover">
              <div className="liquid-summary-popover-header">{doc.kind === "textbook" ? t("hub.takeaway.learningFull") : t("hub.takeaway.contributionFull")}</div>
              <div className="liquid-summary-popover-body">{doc.briefTakeaway}</div>
            </div>
          ) : null}
        </span>
        <span style={{ color: "var(--muted-plus)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontSize: 11.5 }}>{doc.keywords && doc.keywords.length > 0 ? doc.keywords.join(", ") : "—"}</span>
        <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{doc.authors || "—"}</span>
        <span>{doc.year || "—"} {doc.yearLabel ?? ""}</span>
        <span style={{ fontSize: 10.5, fontWeight: 600, color: doc.hasOcr || doc.briefStatus === "ready" ? "var(--success)" : "#d97706" }}>{t(STATUS_KEY[(doc.lifecycleStatus ?? "unread") as ReadingLifecycleStatus])}{doc.favorite ? t("hub.tableFavorite") : ""}{doc.priority ? ` · P${doc.priority}` : ""}{hubDate(doc.reviewAt) ? t("hub.tableReview", { date: hubDate(doc.reviewAt) ?? "" }) : ""} · {doc.hasOcr ? t("hub.ocrReadyCompact") : doc.briefStatus === "ready" ? t("hub.briefReadyCompact") : t("hub.pendingAnalysis")}</span>
      </div>
    );
  };

  return (
    <div className={`app-shell${dragging && dragFeedback?.tone === "invalid" ? " hub-drop-forbidden-cursor" : ""}`} data-theme={themeMode} onPointerMove={handlePointerMove} onPointerUp={finishDrag} onPointerCancel={abortDrag}>
      <header className="hub-floating-island">
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <div style={{ width: 30, height: 30, background: "var(--ink)", color: "var(--paper)", borderRadius: 8, display: "grid", placeItems: "center", fontWeight: 800, fontSize: 15, boxShadow: "0 2px 8px rgba(0,0,0,0.15)" }}>R</div>
          <div style={{ fontWeight: 700, fontSize: 15, letterSpacing: "-0.01em" }}>Read Atlas</div>
          <div className="liquid-capsule" style={{ fontSize: 11.5, fontWeight: 600 }}>{t("hub.paperCount", { count: documents.length })}</div>
        </div>
        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          {onToggleTheme ? (
            <div className="liquid-pill-group">
              <button className={`liquid-tab-btn ${themeMode === "liquid-light" ? "active" : ""}`} onClick={() => onToggleTheme("liquid-light")} title={t("hub.theme.lightTitle")}>{t("hub.theme.light")}</button>
              <button className={`liquid-tab-btn ${themeMode === "liquid-dark" ? "active" : ""}`} onClick={() => onToggleTheme("liquid-dark")} title={t("hub.theme.darkTitle")}>{t("hub.theme.dark")}</button>
              <button className={`liquid-tab-btn ${themeMode === "warm-editorial" ? "active" : ""}`} onClick={() => onToggleTheme("warm-editorial")} title={t("hub.theme.warmTitle")}>{t("hub.theme.warm")}</button>
            </div>
          ) : null}
          <div style={{ position: "relative" }}>
            <button className="btn-liquid-pill primary" onClick={() => requestImport()} disabled={importBusy || Boolean(workspace && !workspace.available)} title={workspace && !workspace.available ? t("hub.workspaceUnavailable") : undefined}><span>＋</span> {t("hub.importPdf")}</button>
            {importPickerOpen ? (
              <div className="liquid-glass-menu" style={{ position: "absolute", right: 0, top: "110%", zIndex: 20, minWidth: 180, padding: 8, display: "flex", flexDirection: "column", gap: 6 }}>
                <button type="button" className="btn-liquid-pill" onClick={() => requestImport("Papers/Inbox")}>{t("hub.importAsPaper")}</button>
                <button type="button" className="btn-liquid-pill" onClick={() => requestImport("Textbooks/Inbox")}>{t("hub.importAsTextbook")}</button>
              </div>
            ) : null}
          </div>
          <button className="btn-liquid-pill" onClick={onOpenOperations}>{t("hub.operations")}</button>
          <button className="btn-liquid-pill" onClick={() => onOpenSettings()}>{t("hub.settings")}</button>
        </div>
      </header>

      {libraryNotice ? (
        <div style={{ margin: "8px 20px 0 20px", padding: "8px 16px", borderRadius: 12, background: "rgba(239, 68, 68, 0.1)", border: "1px solid rgba(239, 68, 68, 0.3)", color: "#dc2626", fontSize: 12, display: "flex", justifyContent: "space-between", alignItems: "center" }} role="alert">
          <span>{libraryNotice}</span>
          <div style={{ display: "flex", gap: 8 }}>
            {workspace && !workspace.available ? (<button className="btn-liquid-pill" style={{ padding: "2px 8px", fontSize: 11 }} onClick={() => onOpenSettings("workspace")}>{t("hub.fixNow")}</button>) : null}
            {onDismissNotice ? (<button style={{ background: "transparent", border: "none", cursor: "pointer", fontSize: 14 }} onClick={onDismissNotice}>×</button>) : null}
          </div>
        </div>
      ) : null}

      <div className="hub-glass-layout">
        <aside className="sidebar-glass-island" onContextMenu={handleBlankContext} onPointerMove={handlePointerMove}>
          <div style={{ fontSize: 11, fontWeight: 700, color: "var(--muted)", textTransform: "uppercase", letterSpacing: "0.05em", padding: "0 6px", display: "flex", justifyContent: "space-between", alignItems: "center" }}>
            <span>{t("hub.physicalFolders")}</span>
            {onChooseWorkspace ? (<button className="micro-action-pill" onClick={onChooseWorkspace} title={t("hub.chooseWorkspace")}>↗</button>) : null}
          </div>
          <div className={`folder-pill-node ${selectedFolder === "" ? "active" : ""} ${dropTarget === "" ? "drop-target" : ""} ${dropForbidden === "" ? "drop-forbidden" : ""}`} data-folder="" onClick={() => setSelectedFolder("")} onContextMenu={(e) => handleFolderContext(e, "")}>
            <span className="folder-pill-label">{t("hub.allDocuments")}</span><span style={{ fontSize: 11, color: "var(--muted)" }}>{documents.length}</span>
          </div>
          <div role="tree" aria-label={t("hub.treeAria")} onKeyDown={(e) => hub.onTreeKeyDown(e, setSelectedFolder)}>
            {hub.treeRows.map((node) => {
              const isEditing = editingFolder === node.path;
              const dropActive = dropTarget === node.path;
              const forbidden = dropForbidden === node.path;
              const canDrag = isNormalFolder(node.path);
              return (
                <div
                  key={node.path}
                  data-folder={node.path}
                  role="treeitem"
                  aria-selected={selectedFolder === node.path}
                  aria-expanded={node.childCount > 0 ? node.expanded : undefined}
                  tabIndex={hub.focusedFolder === node.path ? 0 : -1}
                  className={`folder-pill-node ${selectedFolder === node.path ? "active" : ""} ${dropActive ? "drop-target" : ""} ${forbidden ? "drop-forbidden" : ""} ${dragging?.label === node.name ? "dragging" : ""}`}
                  style={{ paddingLeft: `${node.depth * 14 + 10}px` }}
                  onFocus={() => hub.setFocusedFolder(node.path)}
                  onClick={() => setSelectedFolder(node.path)}
                  onContextMenu={(e) => handleFolderContext(e, node.path)}
                  onPointerDown={canDrag ? (e) => handlePointerDown(e, "folder", node.path, node.path) : undefined}
                >
                  {node.childCount > 0 ? (
                    <button
                      type="button"
                      className="folder-twisty"
                      aria-label={node.expanded ? t("hub.collapseFolder", { name: node.name }) : t("hub.expandFolder", { name: node.name })}
                      aria-expanded={node.expanded}
                      tabIndex={-1}
                      onClick={(e) => { e.stopPropagation(); hub.toggleFolder(node.path); }}
                    >{node.expanded ? "▾" : "▸"}</button>
                  ) : (<span className="folder-twisty-spacer" aria-hidden="true" />)}
                  {isEditing ? (
                    <input
                      className={`hub-inline-rename ${editingError ? "hub-inline-error" : ""}`}
                      autoFocus
                      value={editingFolderValue}
                      onChange={(e) => setEditingFolderValue(e.target.value)}
                      onKeyDown={(e) => { if (e.key === "Enter") submitFolderRename(); if (e.key === "Escape") setEditingFolder(null); }}
                      onBlur={submitFolderRename}
                    />
                  ) : (<><span className="folder-pill-label">📂 {node.name}</span><span style={{ fontSize: 11, color: "var(--muted)" }}>{node.documentCount}</span></>)}
                </div>
              );
            })}
          </div>
          <div className="sidebar-smart-section">
            <span style={{ fontSize: 11, fontWeight: 700, color: "var(--muted)", textTransform: "uppercase", letterSpacing: "0.05em", padding: "10px 6px 0 6px", display: "block" }}>{t("hub.smartCollections")}</span>
            {smartCollections.map((collection) => {
              const folder = smartCollectionFolder(collection.id);
              const active = selectedFolder === folder;
              return (
                <div
                  key={collection.id}
                  aria-current={active ? "true" : undefined}
                  data-folder={folder}
                  data-smart="true"
                  className={`folder-pill-node sidebar-smart-item ${active ? "active" : ""} ${dropForbidden === folder ? "drop-forbidden" : ""}`}
                  onClick={() => setSelectedFolder(folder)}
                  onContextMenu={(e) => handleFolderContext(e, folder)}
                  title={collection.builtin ? t("hub.builtinSmartTitle") : t("hub.savedSmartTitle")}
                >
                  <span className="folder-twisty-spacer" aria-hidden="true" />
                  <span className="folder-pill-label">◇ {collection.builtin ? uiText(t, collection.name) : collection.name}</span>
                  {collection.builtin ? <span className="sidebar-smart-badge">{t("hub.builtinBadge")}</span> : null}
                </div>
              );
            })}
            {libraryClient ? (
              saveSmartOpen ? (
                <form
                  className="sidebar-smart-save"
                  onSubmit={(event) => {
                    event.preventDefault();
                    const name = saveSmartName.trim();
                    const filters = hubFiltersForView(selectedFolder, searchQuery, activeSmart);
                    if (!name || filters.length === 0) {
                      hub.showHint(t("hub.saveSmartNeedName"));
                      return;
                    }
                    void libraryClient.act(changeRequest({ kind: "create_smart_collection", name, filters }))
                      .then(async (response) => {
                        setSaveSmartOpen(false);
                        setSaveSmartName("");
                        await reloadSmartCollections();
                        if (response.kind === "change" && response.result.kind === "smart_collection") {
                          setSelectedFolder(smartCollectionFolder(response.result.collection.id));
                        }
                      })
                      .catch((error) => hub.showHint(isActMessage(error)));
                  }}
                >
                  <input
                    className="hub-inline-rename"
                    autoFocus
                    placeholder={t("hub.collectionNamePlaceholder")}
                    value={saveSmartName}
                    onChange={(event) => setSaveSmartName(event.target.value)}
                    aria-label={t("hub.smartCollectionNameAria")}
                  />
                  <button type="submit" className="btn-liquid-pill">{t("hub.save")}</button>
                  <button type="button" className="btn-liquid-pill" onClick={() => { setSaveSmartOpen(false); setSaveSmartName(""); }}>{t("hub.cancel")}</button>
                </form>
              ) : (
                <button type="button" className="btn-liquid-pill sidebar-smart-save-btn" onClick={() => setSaveSmartOpen(true)}>{t("hub.saveCurrentFilter")}</button>
              )
            ) : (
              <p className="sidebar-smart-empty">{t("hub.smartNeedsWorkspace")}</p>
            )}
          </div>
        </aside>

        <main className={`main-glass-stage ${stageDropActive ? "drop-target" : ""}`} onContextMenu={handleBlankContext}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", gap: 12 }}>
            <div style={{ flex: 1, maxWidth: 420 }}>
              <input type="text" className="liquid-capsule" style={{ width: "100%", fontSize: 13, outline: "none" }} placeholder={t("hub.searchPlaceholder")} value={searchQuery} onChange={(e) => setSearchQuery(e.target.value)} />
            </div>
            <div style={{ display: "flex", alignItems: "center", gap: 12, flexShrink: 0 }}>
              <span style={{ fontSize: 13, color: "var(--muted)", whiteSpace: "nowrap" }}>{t("hub.sortLabel")}</span>
              <select title={sortTitle(availability)} aria-label={t("hub.sortAria")} style={{ background: "var(--glass-surface)", backdropFilter: "var(--glass-blur)", WebkitBackdropFilter: "var(--glass-blur)", border: "1px solid var(--glass-border)", color: "var(--ink)", fontSize: 13, borderRadius: 8, padding: "5px 12px", height: "34px", outline: "none", flexShrink: 0, width: "auto", minWidth: "150px" }} value={effectiveMode} onChange={(e) => handleSortChange(e.target.value as HubSortMode)}>
                <option value="recent">{t("hub.sort.recent")}</option>
                <option value="year">{t("hub.sort.year")}</option>
                <option value="title">{t("hub.sort.title")}</option>
                <option value="manual" disabled={!availability.manual || Boolean(activeSmart)}>{t("hub.sort.manual")}</option>
                <option value="chapter" disabled={!availability.chapter || Boolean(activeSmart)}>{t("hub.sort.chapter")}</option>
                <option value="last_opened" disabled={!activeSmart || activeSmart.query.sort !== "last_opened"}>{t("hub.sort.lastOpened")}</option>
              </select>
              {(pageSort === "year" || pageSort === "chapter") && libraryClient && <button type="button" aria-label={t("hub.sortDirAria")} onClick={() => setSortDirections(current => ({...current,[pageSort]:(sortDirection ?? (pageSort === "year" ? "desc" : "asc")) === "desc" ? "asc" : "desc"}))}>{(sortDirection ?? (pageSort === "year" ? "desc" : "asc")) === "desc" ? t("hub.sortDesc") : t("hub.sortAsc")}</button>}
              <div className="liquid-pill-group" style={{ flexShrink: 0, display: "inline-flex" }}>
                <button type="button" className={`liquid-tab-btn ${viewType === "grid" ? "active" : ""}`} onClick={() => handleSetViewType("grid")}>{t("hub.view.grid")}</button>
                <button type="button" className={`liquid-tab-btn ${viewType === "table" ? "active" : ""}`} onClick={() => handleSetViewType("table")}>{t("hub.view.table")}</button>
              </div>
            </div>
          </div>

          {hub.clearedHint ? (
            <div className="hub-live-hint" role="status" aria-live="polite">
              <span>{hub.clearedHint}</span>
              <button type="button" className="hub-hint-dismiss" onClick={hub.dismissHint} aria-label={t("hub.dismissHint")}>×</button>
            </div>
          ) : null}

          {dragging && dragFeedback ? (
            <div
              className="hub-live-hint"
              role="status"
              aria-live="polite"
              style={dragFeedback.tone === "invalid" ? { borderColor: "rgba(193,95,62,0.45)" } : undefined}
            >
              <span>{dragFeedback.text}</span>
            </div>
          ) : null}

          {hub.count > 0 ? (
            <div className="hub-bulk-toolbar" role="toolbar" aria-label={t("hub.bulk.toolbarAria")}>
              <span className="hub-bulk-count">
                {hub.allMatching ? t("hub.bulk.selectedMatching", { count: hub.count }) : t("hub.bulk.selected", { count: hub.count })}
              </span>
              {hub.allMatching ? <span className="hub-bulk-badge">{t("hub.bulk.logical")}</span> : null}
              {!hub.allMatching ? (
                <button type="button" className="btn-liquid-pill" onClick={hub.selectAll} title="Ctrl+A">{t("hub.bulk.selectAll", { count: listSize })}</button>
              ) : null}
              <button
                type="button"
                className="btn-liquid-pill"
                disabled={!hub.canSubmit || (hub.count > 1 && !hub.bulkActionsEnabled)}
                title={!hub.canSubmit ? uiText(t, SNAPSHOT_DISABLED_REASON) : (!hub.bulkActionsEnabled && hub.count > 1) ? hub.bulkDisabledReason : t("hub.bulk.moveTitle")}
                onClick={() => {
                  if (!hub.canSubmit) return;
                  setMoveDialogSelection(hub.count > 1 || hub.allMatching);
                  openMoveDialog(hub.selectedIds[0] ?? hub.focusedId ?? "");
                }}
              >{t("hub.moveTo")}</button>
              {hub.bulkActionsEnabled ? (
                <button type="button" className="btn-liquid-pill" disabled={!hub.canSubmit} onClick={() => setTagDialogOpen(true)}>{t("hub.tags")}</button>
              ) : (
                <button type="button" className="btn-liquid-pill hub-bulk-soon" disabled title={hub.bulkDisabledReason}>
                  <span>{t("hub.tags")}</span><span className="hub-soon-badge">{t("hub.comingSoon")}</span>
                </button>
              )}
              {hub.bulkActionsEnabled ? (
                <button
                  type="button"
                  className="btn-liquid-pill"
                  disabled={!hub.canSubmit}
                  onClick={() => {
                    setLifecyclePaper(hub.count === 1 && !hub.allMatching
                      ? paperById.get(hub.selectedIds[0]) ?? null
                      : null);
                    setLifecycleDialogOpen(true);
                  }}
                >{t("hub.readingStatus")}</button>
              ) : (
                <button type="button" className="btn-liquid-pill hub-bulk-soon" disabled title={uiText(t, LIFECYCLE_DISABLED_REASON)}>
                  <span>{t("hub.readingStatus")}</span><span className="hub-soon-badge">{t("hub.comingSoon")}</span>
                </button>
              )}
              {hub.bulkActionsEnabled ? (
                <button type="button" className="btn-liquid-pill" disabled={!hub.canSubmit} onClick={() => void runCommand({ kind: "ocr" }, currentTarget())}>{t("hub.ocr")}</button>
              ) : (
                <button type="button" className="btn-liquid-pill hub-bulk-soon" disabled title={hub.bulkDisabledReason}>
                  <span>{t("hub.ocr")}</span><span className="hub-soon-badge">{t("hub.comingSoon")}</span>
                </button>
              )}
              {hub.bulkActionsEnabled ? (
                <button type="button" className="btn-liquid-pill" disabled={!hub.canSubmit} onClick={() => void runCommand({ kind: "brief" }, currentTarget())}>{t("hub.brief")}</button>
              ) : (
                <button type="button" className="btn-liquid-pill hub-bulk-soon" disabled title={hub.bulkDisabledReason}>
                  <span>{t("hub.brief")}</span><span className="hub-soon-badge">{t("hub.comingSoon")}</span>
                </button>
              )}
              {hub.bulkActionsEnabled ? (
                <button type="button" className="btn-liquid-pill" disabled={!hub.canSubmit} onClick={() => void runCommand({ kind: "export", format: "reading_bundle" }, currentTarget())}>{t("hub.export")}</button>
              ) : (
                <button type="button" className="btn-liquid-pill hub-bulk-soon" disabled title={hub.bulkDisabledReason}>
                  <span>{t("hub.export")}</span><span className="hub-soon-badge">{t("hub.comingSoon")}</span>
                </button>
              )}
              <button
                type="button"
                className="btn-liquid-pill hub-bulk-danger"
                disabled={!hub.canSubmit || (hub.count > 1 && !hub.bulkActionsEnabled)}
                title={!hub.canSubmit ? uiText(t, SNAPSHOT_DISABLED_REASON) : (!hub.bulkActionsEnabled && hub.count > 1) ? hub.bulkDisabledReason : undefined}
                onClick={() => {
                  if (!hub.canSubmit) return;
                  if (hub.count === 1 && !hub.allMatching) onTrashPaper?.(hub.selectedIds[0]);
                  else void runCommand({ kind: "trash" }, currentTarget());
                }}
              >{t("hub.moveToTrash")}</button>
              <button type="button" className="btn-liquid-pill" onClick={hub.clear} title="Esc">{t("hub.clearSelection")}</button>
            </div>
          ) : null}

          <div ref={listRef} className="hub-paper-list">
          {filteredDocuments.length === 0 && hubPage.loading ? (
            <div role="status" aria-live="polite" style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--muted)" }}>{t("hub.loadingLibrary")}</div>
          ) : filteredDocuments.length === 0 ? (
            <div style={{ flex: 1, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", background: "var(--glass-card)", borderRadius: 16, border: "1px solid var(--glass-border)", padding: 40, textAlign: "center", color: "var(--muted)" }}>
              <div style={{ fontSize: 36, marginBottom: 8 }}>📚</div>
              <strong style={{ fontSize: 16, color: "var(--ink)", marginBottom: 4 }}>{activeSmart ? t("hub.empty.noMatch") : selectedFolder.startsWith("Textbooks") ? t("hub.empty.noTextbooks") : selectedFolder.startsWith("Papers") ? t("hub.empty.noPapers") : t("hub.empty.noDocuments")}</strong>
              <p style={{ fontSize: 13, maxWidth: 360, margin: "0 0 16px 0" }}>{activeSmart ? t("hub.empty.smartHint") : searchQuery ? t("hub.empty.searchHint") : selectedFolder.startsWith("Textbooks") ? t("hub.empty.textbookHint") : t("hub.empty.paperHint")}</p>
              {!searchQuery && !activeSmart ? (<button className="btn-liquid-pill primary" onClick={() => requestImport()}>{t("hub.importPdfPlus")}</button>) : null}
            </div>
          ) : viewType === "grid" ? (
            <div
              className="paper-cards-grid"
              role="listbox"
              aria-multiselectable="true"
              aria-label={t("hub.documentListAria")}
              aria-busy={hubPage.loading || undefined}
              onKeyDown={hub.onKeyDown}
              style={windowed.virtualized ? { paddingTop: windowed.paddingStart, paddingBottom: windowed.paddingEnd } : undefined}
            >
              {renderedDocuments.map((doc, offset) => renderCard(doc, windowed.start + offset))}
            </div>
          ) : (
            <div
              className={`liquid-table-container ${insert && insert.targetId === null ? "insert-at-end" : ""}`}
              role="listbox"
              aria-multiselectable="true"
              aria-label={t("hub.documentListAria")}
              aria-busy={hubPage.loading || undefined}
              onKeyDown={hub.onKeyDown}
              style={windowed.virtualized ? { paddingTop: windowed.paddingStart, paddingBottom: windowed.paddingEnd } : undefined}
            >
              <div className="liquid-table-row header-row" role="presentation"><span aria-hidden="true" /><span>{t("hub.table.select")}</span><span>{t("hub.table.title")}</span><span>{t("hub.table.summary")}</span><span>{t("hub.table.keywords")}</span><span>{t("hub.table.authors")}</span><span>{t("hub.table.year")}</span><span>{t("hub.table.status")}</span></div>
              {renderedDocuments.map((doc, offset) => renderRow(doc, windowed.start + offset))}
            </div>
          )}
          {hubPage.hasMore ? (
            <button
              type="button"
              className="btn-liquid-pill"
              onClick={() => hubPage.loadMore()}
              disabled={hubPage.loading}
            >
              {hubPage.loading ? t("hub.loading") : t("hub.loadMore", { shown: filteredDocuments.length, total: hubPage.totalCount })}
            </button>
          ) : null}
          </div>
        </main>
      </div>
      {menu ? (<HubContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => setMenu(null)} />) : null}
      {dragging ? (
        <div className="hub-drag-ghost" style={{ left: dragging.x + 12, top: dragging.y + 12 }} aria-hidden="true">
          <strong>{dragging.label}</strong>
          {dragFeedback ? (
            <span style={{ display: "block", marginTop: 3, fontSize: 11, color: dragFeedback.tone === "invalid" ? "#c15f3e" : "var(--muted)" }}>
              {dragFeedback.text}
            </span>
          ) : null}
        </div>
      ) : null}
      {toast ? (
        <div className="hub-move-toast" role="status" aria-live="polite">
          <span>{undoBusy ? t("hub.toast.undoing") : t("hub.toast.moved")}</span>
          <button className="hub-toast-undo" onClick={() => void handleUndo()} disabled={undoBusy}>
            {undoBusy ? t("hub.toast.wait") : t("hub.undo")}
          </button>
        </div>
      ) : null}
      {editingError ? (<div className="hub-inline-error-tip">{editingError}</div>) : null}
      {moveError ? (<div className="hub-inline-error-tip" role="alert">{moveError}</div>) : null}
      {moveDialogPaper ? (
        <HubMoveDialog
          paper={{
            id: moveDialogPaper.id,
            title: moveDialogPaper.title || t("hub.untitledDocument"),
            collection: moveDialogPaper.collection,
            kind: moveDialogPaper.kind,
            hasOcr: moveDialogPaper.hasOcr,
            briefStatus: moveDialogPaper.briefStatus,
            chapterNumber: moveDialogPaper.chapterNumber,
          }}
          count={moveDialogSelection ? hub.count : 1}
          homeCollections={moveDialogSelection ? hub.selectedIds.map(collectionOf) : [moveDialogPaper.collection]}
          candidates={hub.treeRows.map((node) => ({ path: node.path, depth: node.depth, name: node.name, documentCount: node.documentCount }))}
          initialPath={moveDialogInitialPath}
          evaluate={(targetPath) => hub.evaluate(
            {
              kind: "paper",
              id: moveDialogPaper.id,
              collection: moveDialogPaper.collection,
              memberIds: moveDialogSelection ? hub.selectedIds : [moveDialogPaper.id],
              memberCollections: moveDialogSelection ? hub.selectedIds.map(collectionOf) : [moveDialogPaper.collection],
              allMatching: moveDialogSelection && hub.allMatching,
            },
            { type: "collection", path: targetPath, smart: false },
          )}
          onConfirm={async (target) => {
            setMoveDialogPaperId(null);
            setMoveDialogInitialPath(null);
            setMoveError(null);
            try {
              if (libraryClient) {
                const batchTarget = moveDialogSelection
                  ? currentTarget()
                  : { kind: "explicit" as const, paperIds: [moveDialogPaper.id] };
                setMoveDialogSelection(false);
                await runCommand({ kind: "move", collectionPath: target }, batchTarget, true);
                return;
              }
              await commitPaperMove([moveDialogPaper.id], target);
            } catch (error) {
              const message = t("hub.error.moveFailed", { error: String(error) });
              setMoveError(message);
              hub.showHint(message);
            }
          }}
          onClose={() => { setMoveDialogPaperId(null); setMoveDialogInitialPath(null); setMoveDialogSelection(false); }}
        />
      ) : null}
      {tagDialogOpen ? (
        <HubTagPatchDialog
          count={hub.count}
          knownTags={[...new Set(hub.selectedIds.flatMap((id) => paperById.get(id)?.keywords ?? []))]}
          onConfirm={async (patch) => {
            setTagDialogOpen(false);
            await runCommand({ kind: "patch_tags", add: patch.add, remove: patch.remove }, currentTarget(), true);
          }}
          onClose={() => setTagDialogOpen(false)}
        />
      ) : null}
      {lifecycleDialogOpen ? (
        <HubLifecycleDialog
          count={lifecyclePaper ? 1 : hub.count}
          initial={lifecyclePaper ? {
            status: (lifecyclePaper.lifecycleStatus === "reading" || lifecyclePaper.lifecycleStatus === "read")
              ? lifecyclePaper.lifecycleStatus
              : lifecyclePaper.lifecycleStatus === "unread" ? "unread" : undefined,
            favorite: lifecyclePaper.favorite,
            readLater: lifecyclePaper.readLater,
            priority: lifecyclePaper.priority,
            reviewAt: lifecyclePaper.reviewAt ?? undefined,
          } : undefined}
          onConfirm={async (patch) => {
            setLifecycleDialogOpen(false);
            await applyLifecyclePatch(patch, lifecyclePaper ?? undefined);
            setLifecyclePaper(null);
          }}
          onClose={() => { setLifecycleDialogOpen(false); setLifecyclePaper(null); }}
        />
      ) : null}
      {confirmPlan ? (
        <HubBatchConfirmDialog
          plan={confirmPlan}
          busy={confirmBusy}
          onConfirm={(ids) => void confirmPending(ids)}
          onClose={() => { if (!confirmBusy) { setConfirmPlan(null); pendingStart.current = null; } }}
        />
      ) : null}
      {batchToast ? (
        <div className="hub-move-toast" role="status" aria-live="polite">
          <span>{batchToast.message}</span>
          <button type="button" className="hub-toast-undo" onClick={onOpenOperations}>{t("hub.viewDetails")}</button>
          {batchToast.token ? (
            <button
              type="button"
              className="hub-toast-undo"
              onClick={() => {
                if (!libraryClient || !batchToast.token) return;
                void libraryClient.act(controlBatchRequest(batchToast.batchId, { kind: "undo", token: batchToast.token }))
                  .then(async (result) => {
                    consumeUndoToken(batchToast.batchId);
                    setBatchToast(null);
                    await onLibraryChanged?.();
                    hub.showHint(summarizeBatch(batchOf(result), t));
                  })
                  .catch((error) => hub.showHint(isActMessage(error)));
              }}
            >{t("hub.undoBatch")}</button>
          ) : null}
        </div>
      ) : null}
      {importOverlay ? (
        <div className="hub-import-overlay" role="status" aria-live="polite">
          <strong>{t("hub.dropToImport")}</strong>
          <span>{t("hub.importTarget", { path: importOverlay })}</span>
        </div>
      ) : null}
      {hub.coachMarkOpen ? <HubCoachMark onDismiss={hub.dismissCoachMark} /> : null}
    </div>
  );
}

/** Shown once on first upgrade (remembered by UI schema version); Settings can replay it. */
function HubCoachMark({ onDismiss }: Readonly<{ onDismiss: () => void }>) {
  const { t } = useLocale();
  const cardRef = useRef<HTMLDivElement>(null);
  useDialogFocusTrap({ open: true, containerRef: cardRef, onClose: onDismiss });
  return (
    <div className="scrim hub-coach-scrim" role="presentation" onClick={(e) => { if (e.target === e.currentTarget) onDismiss(); }}>
      <div ref={cardRef} className="hub-coach-mark" role="dialog" aria-modal="true" aria-labelledby="hub-coach-title">
        <strong id="hub-coach-title">{t("hub.coach.title")}</strong>
        <ol>
          <li>{t("hub.coach.item1")}</li>
          <li>{t("hub.coach.item2Before")} <b>{t("hub.coach.handle")}</b> {t("hub.coach.item2After")}</li>
          <li>{t("hub.coach.item3")}</li>
          <li>{t("hub.coach.item4")}</li>
        </ol>
        <button type="button" className="btn-liquid-pill primary" onClick={onDismiss}>{t("hub.coach.gotIt")}</button>
      </div>
    </div>
  );
}
