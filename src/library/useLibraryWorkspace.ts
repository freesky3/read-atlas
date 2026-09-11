import { uiText, zhT, type TranslateFn } from "../i18n/uiText";
// Library Hub 的 Interaction Controller：把选择、焦点、目录树展开、
// 键盘替代与「完整精确置换」提交收敛到一个 Module 里，`LibraryHub`
// 只消费它的 view / actions。合同见 docs/library-workspace-plan-2026-08.md §4.1 / §6。
//
// 边界：本层只管交互状态。本地 Batch 与 Provider 批量（OCR / Brief）
// 由调用方注入 `libraryClient.act`。

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { CollectionProjection } from "../types";
import {
  applyClickSelection,
  canSubmitSelection,
  clearSelection as clearSelectionSet,
  emptySelection,
  isSelectionEmpty,
  isItemSelected,
  moveFocusedId,
  reconcileWithVisibleIds,
  resolvedSelectionIds,
  selectAllMatching,
  selectionForViewKey,
  setFocusedId,
  toggleId,
  type QuerySnapshot,
  type SelectionSet,
} from "./selectionModel";
import {
  dropLabel,
  evaluateDrop,
  planReorder,
  type DragSubject,
  type DropEvaluation,
  type DropTarget,
  type ReorderContext,
  type ReorderPlan,
} from "./dropPolicy";
import {
  TREE_UI_SCHEMA_VERSION,
  flattenVisible,
  moveTreeFocus,
  pruneCollapsed,
  toggleCollapsed,
  treeStorageKey,
  type CollectionRow,
  type VisibleTreeRow,
} from "./treeModel";

export const COACH_MARK_STORAGE_KEY = `read-desktop.hubCoachMark.v${TREE_UI_SCHEMA_VERSION}`;
export const BULK_DISABLED_REASON = "批量操作需要已打开的 Workspace 与 library_act 持久 Batch";
/** 阅读状态走 library_act；没有 client 时不能伪装成已写入 */
export const LIFECYCLE_DISABLED_REASON = "阅读状态需要 Workspace 的 library_act 接线";
/** 逻辑选择（全选当前筛选结果）没有可提交的 Selection Snapshot，所以不能直接执行动作 */
export const SNAPSHOT_DISABLED_REASON =
  "「全选当前筛选结果」是逻辑选择，提交需要 Selection Snapshot；当前查询还没有 hub_page 快照";

export type ReorderOutcome = { ok: true } | { ok: false; message: string };

export type UseLibraryWorkspaceInput = Readonly<{
  /** 物理 / 智能集合 + 筛选表达式；变化即清空选择 */
  viewKey: string;
  /** Workspace 隔离键（rootPath），用于目录树展开状态记忆 */
  workspaceKey: string;
  /** 当前可见稳定顺序 */
  visibleIds: readonly string[];
  /** 当前可手排物理叶子层的完整 live id（不可手排时为空数组） */
  layerIds: readonly string[];
  layerCollectionId: string | null;
  reorder: ReorderContext;
  collections: readonly CollectionProjection[];
  documentCountOf: (path: string) => number;
  /** 取某个 paper 当前所在物理目录，供跨子目录多选的落点判定使用 */
  collectionOf: (paperId: string) => string;
  onReorderPapers?: (collectionId: string, paperIds: string[]) => void | Promise<void>;
  onOpenPaper?: (paperId: string) => void;
  /** 设置里点「重新播放」时自增；每次变化重新弹出交互导览（§6.5） */
  replayTourSignal?: number;
  /**
   * `hub_page` 给出的成员快照。为 null 时 Ctrl+A 仍构建逻辑选择，但不能提交。
   * 值必须是 selectionDigest，不是带 sort/limit 的 queryDigest。
   */
  querySnapshot?: QuerySnapshot | null;
  /** 当前 query 的命中篇数；all_matching 的计数用它，而不是已加载页的长度。 */
  matchingCount?: number | null;
  /** 本地 Batch（标签 / 移动 / 回收站 / 导入 / 导出）已接通 `library_act` */
  localBatchesEnabled?: boolean;
}>;

const PHYSICAL_ROOTS = ["Papers", "Textbooks"];

function readCollapsed(key: string): Set<string> {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return new Set();
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return new Set();
    return new Set(parsed.filter((item): item is string => typeof item === "string"));
  } catch {
    return new Set();
  }
}

function writeCollapsed(key: string, collapsed: ReadonlySet<string>): void {
  try {
    localStorage.setItem(key, JSON.stringify([...collapsed]));
  } catch {
    /* 记不住展开状态不影响功能 */
  }
}

function hasTextFocus(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null;
  if (!element || !element.tagName) return false;
  return element.tagName === "INPUT" || element.tagName === "TEXTAREA" || element.isContentEditable === true;
}

export function useLibraryWorkspace(input: UseLibraryWorkspaceInput, t: TranslateFn = zhT) {
  const {
    viewKey,
    workspaceKey,
    visibleIds,
    layerIds,
    layerCollectionId,
    reorder,
    collections,
    documentCountOf,
    collectionOf,
    onReorderPapers,
    onOpenPaper,
    replayTourSignal,
    querySnapshot = null,
    matchingCount = null,
    localBatchesEnabled = false,
  } = input;

  const [selection, setSelection] = useState<SelectionSet>(() => emptySelection(viewKey));
  const [clearedHint, setClearedHint] = useState<string | null>(null);
  const [coachMarkOpen, setCoachMarkOpen] = useState<boolean>(() => {
    try {
      return localStorage.getItem(COACH_MARK_STORAGE_KEY) !== "done";
    } catch {
      return false;
    }
  });

  const visibleIdsKey = visibleIds.join("\u0000");
  const orderedVisibleIds = useMemo(
    () => (visibleIdsKey ? visibleIdsKey.split("\u0000") : []),
    [visibleIdsKey],
  );
  const currentSelection = useRef(selection);
  currentSelection.current = selection;

  const showHint = useCallback((message: string) => {
    setClearedHint(message);
  }, []);

  useEffect(() => {
    if (!clearedHint) return;
    const timer = window.setTimeout(() => setClearedHint(null), 5000);
    return () => window.clearTimeout(timer);
  }, [clearedHint]);

  // 切换 collection / Smart Collection / 筛选表达式：清空选择并提示。
  const previousViewKey = useRef(viewKey);
  useEffect(() => {
    if (previousViewKey.current === viewKey) return;
    const transition = selectionForViewKey(currentSelection.current, viewKey);
    previousViewKey.current = viewKey;
    setSelection(transition.selection);
    if (transition.cleared) showHint(uiText(t, "筛选已变化，已清空旧选择"));
  }, [viewKey, showHint, t]);

  // 仅切换 grid/table 或排序：保留所选 ID，只清掉失效的 anchor / focus。
  useEffect(() => {
    setSelection((current) => reconcileWithVisibleIds(current, orderedVisibleIds));
  }, [orderedVisibleIds]);

  const selectedIds = useMemo(
    () => resolvedSelectionIds(selection, orderedVisibleIds),
    [selection, orderedVisibleIds],
  );
  const allMatching = selection.mode === "all_matching";
  const count = allMatching
    ? Math.max(0, (matchingCount ?? orderedVisibleIds.length) - selection.excludedIds.size)
    : selectedIds.length;
  const isSelected = useCallback((id: string) => isItemSelected(selection, id), [selection]);

  /** 拖动已显式选中的 Paper 时移动整个显式选择集；逻辑 all_matching 不能拖。 */
  const dragSubjectFor = useCallback((id: string, fromSelection: boolean): DragSubject => {
    const members = fromSelection && !allMatching ? selectedIds : [id];
    return {
      kind: "paper",
      id,
      collection: collectionOf(id),
      memberIds: members,
      memberCollections: members.map(collectionOf),
      allMatching,
    };
  }, [allMatching, collectionOf, selectedIds]);

  const evaluate = useCallback((subject: DragSubject, target: DropTarget): DropEvaluation => (
    evaluateDrop({ subject, target, reorder: { ...reorder, layerIds } })
  ), [layerIds, reorder]);

  /** 唯一一处提交完整精确置换的入口：拖拽、Alt+↑/↓ 与菜单共用。 */
  const commitReorder = useCallback(async (
    subject: DragSubject,
    target: Extract<DropTarget, { type: "reorder" }>,
  ): Promise<ReorderOutcome> => {
    const plan = planReorder(subject, target, { ...reorder, layerIds });
    if (!plan.allowed) return { ok: false, message: uiText(t, plan.label) };
    if (!layerCollectionId) return { ok: false, message: uiText(t, "当前视图没有可改序的物理目录") };
    if (!onReorderPapers) return { ok: false, message: uiText(t, "改序尚未接通") };
    try {
      await onReorderPapers(layerCollectionId, plan.paperIds);
      return { ok: true };
    } catch (error) {
      return { ok: false, message: uiText(t, "Reorder failed: {error}", { error: String(error) }) };
    }
  }, [layerCollectionId, layerIds, onReorderPapers, reorder, t]);

  const previewReorder = useCallback((
    subject: DragSubject,
    target: Extract<DropTarget, { type: "reorder" }>,
  ): ReorderPlan => planReorder(subject, target, { ...reorder, layerIds }), [layerIds, reorder]);

  /** Alt+↑/↓：把单项或所选块作为一个整体挪一格，块内相对顺序不变。 */
  const nudgeSelection = useCallback((delta: -1 | 1): Promise<ReorderOutcome> => {
    const layerSet = new Set(layerIds);
    const inLayer = selectedIds.filter((id) => layerSet.has(id));
    const moving = inLayer.length > 0
      ? inLayer
      : (selection.focusedId && layerSet.has(selection.focusedId) ? [selection.focusedId] : []);
    if (moving.length === 0) return Promise.resolve({ ok: false, message: uiText(t, "请先用方向键聚焦到本层文档") });
    const block = layerIds.filter((id) => moving.includes(id));
    const boundary = delta < 0 ? layerIds.indexOf(block[0]) : layerIds.indexOf(block[block.length - 1]);
    const targetIndex = boundary + delta;
    if (targetIndex < 0) return Promise.resolve({ ok: false, message: uiText(t, "已经在最前面") });
    if (targetIndex >= layerIds.length) return Promise.resolve({ ok: false, message: uiText(t, "已经在最后面") });
    const subject: DragSubject = {
      kind: "paper",
      id: block[0],
      collection: collectionOf(block[0]),
      memberIds: block,
      memberCollections: block.map(collectionOf),
      allMatching,
    };
    return commitReorder(subject, {
      type: "reorder",
      anchorId: layerIds[targetIndex],
      place: delta < 0 ? "before" : "after",
    });
  }, [allMatching, collectionOf, commitReorder, layerIds, selection.focusedId, selectedIds, t]);

  const onKeyDown = useCallback((event: React.KeyboardEvent) => {
    // 输入框聚焦时 Ctrl+A 必须保持文本全选语义。
    if (hasTextFocus(event.target)) return;
    const current = currentSelection.current;
    const key = event.key;
    if ((event.ctrlKey || event.metaKey) && (key === "a" || key === "A")) {
      event.preventDefault();
      setSelection(selectAllMatching(current, visibleIds, querySnapshot));
      return;
    }
    if (key === "Escape") {
      if (!isSelectionEmpty(current, visibleIds)) {
        event.preventDefault();
        setSelection(clearSelectionSet(current));
      }
      return;
    }
    if (key === " ") {
      if (current.focusedId) {
        event.preventDefault();
        setSelection(toggleId(current, current.focusedId));
      }
      return;
    }
    if (key === "Enter") {
      if (current.focusedId) {
        event.preventDefault();
        onOpenPaper?.(current.focusedId);
      }
      return;
    }
    if (event.altKey && (key === "ArrowUp" || key === "ArrowDown")) {
      event.preventDefault();
      void nudgeSelection(key === "ArrowUp" ? -1 : 1).then((outcome) => {
        if (!outcome.ok) showHint(outcome.message);
      });
      return;
    }
    if (key === "ArrowDown" || key === "ArrowUp") {
      if (visibleIds.length === 0) return;
      event.preventDefault();
      setSelection(moveFocusedId(current, visibleIds, key === "ArrowDown" ? 1 : -1));
    }
  }, [nudgeSelection, onOpenPaper, querySnapshot, showHint, visibleIds]);

  /**
   * 卡片 / 行点击：普通单击正文只移动 anchor/focus 并打开 Reader（不产生选择），
   * Ctrl/Meta 切换、Shift 范围才更新 Selection Set。
   */
  const clickRow = useCallback((id: string, modifiers: { ctrl: boolean; shift: boolean; meta: boolean }): "open" | "select" => {
    const plain = !modifiers.ctrl && !modifiers.meta && !modifiers.shift;
    setSelection((current) => applyClickSelection(current, id, visibleIds, modifiers));
    return plain ? "open" : "select";
  }, [visibleIds]);

  const toggleSelected = useCallback((id: string) => {
    setSelection((current) => toggleId(current, id));
  }, []);
  const clear = useCallback(() => setSelection((current) => clearSelectionSet(current)), []);
  const selectAll = useCallback(
    () => setSelection((current) => selectAllMatching(current, visibleIds, querySnapshot)),
    [querySnapshot, visibleIds],
  );
  const focusRow = useCallback((id: string | null) => {
    setSelection((current) => setFocusedId(current, id));
  }, []);

  // ---- 目录树：展开状态按 workspace + UI schema 版本记忆 --------------
  const collapsedKey = treeStorageKey(workspaceKey || "default", TREE_UI_SCHEMA_VERSION);
  const [collapsed, setCollapsed] = useState<Set<string>>(() => readCollapsed(collapsedKey));
  const loadedKey = useRef(collapsedKey);
  useEffect(() => {
    if (loadedKey.current === collapsedKey) return;
    loadedKey.current = collapsedKey;
    setCollapsed(readCollapsed(collapsedKey));
  }, [collapsedKey]);

  const toggleFolder = useCallback((path: string) => {
    setCollapsed((current) => {
      const next = toggleCollapsed(current, path);
      writeCollapsed(collapsedKey, next);
      return next;
    });
  }, [collapsedKey]);

  const treeRows = useMemo<VisibleTreeRow[]>(() => {
    const rows: CollectionRow[] = collections.map((col) => ({
      id: col.id,
      parentId: col.parentId,
      name: col.name,
      relativePath: col.relativePath,
    }));
    // 根目录即使还没有 collection 行也必须可见（新库 / 浏览器预览）。
    for (const root of PHYSICAL_ROOTS) {
      if (!rows.some((row) => row.relativePath === root)) {
        rows.push({ id: `virtual:${root}`, parentId: null, name: root, relativePath: root });
      }
    }
    return flattenVisible(rows, collapsed, { documentCountOf });
  }, [collections, collapsed, documentCountOf]);

  // 节点消失（删除目录 / 切换 Workspace）后剔除残留展开状态。
  useEffect(() => {
    const existing = new Set<string>(PHYSICAL_ROOTS);
    for (const col of collections) existing.add(col.relativePath);
    setCollapsed((current) => {
      const pruned = pruneCollapsed(current, existing);
      if (pruned.size === current.size) return current;
      writeCollapsed(collapsedKey, pruned);
      return pruned;
    });
  }, [collections, collapsedKey]);

  const [focusedFolder, setFocusedFolder] = useState<string | null>(null);
  const onTreeKeyDown = useCallback((event: React.KeyboardEvent, onSelect: (path: string) => void) => {
    if (hasTextFocus(event.target)) return;
    const move = event.key === "ArrowDown" ? "down"
      : event.key === "ArrowUp" ? "up"
        : event.key === "ArrowRight" ? "expand"
          : event.key === "ArrowLeft" ? "collapse" : null;
    if (!move) {
      if (event.key === "Enter" && focusedFolder) {
        event.preventDefault();
        onSelect(focusedFolder);
      }
      return;
    }
    const result = moveTreeFocus(treeRows, focusedFolder, move);
    if (result.kind === "none") return;
    event.preventDefault();
    if (result.kind === "move") setFocusedFolder(result.path);
    else toggleFolder(result.path);
  }, [focusedFolder, toggleFolder, treeRows]);

  const dismissCoachMark = useCallback(() => {
    setCoachMarkOpen(false);
    try {
      localStorage.setItem(COACH_MARK_STORAGE_KEY, "done");
    } catch {
      /* 记不住就下次再演示一次 */
    }
  }, []);

  // 设置里点「重新播放」：重新弹出并清掉已读标记，避免用户没看完就切走后被永久记住。
  useEffect(() => {
    if (!replayTourSignal || replayTourSignal <= 0) return;
    try {
      localStorage.removeItem(COACH_MARK_STORAGE_KEY);
    } catch {
      /* 清不掉标记不影响本次演示 */
    }
    setCoachMarkOpen(true);
  }, [replayTourSignal]);

  return {
    selection,
    selectedIds,
    count,
    allMatching,
    canSubmit: canSubmitSelection(selection, visibleIds),
    /**
     * 本地 Batch 与 Provider 批量已接通时为 true。
     */
    bulkActionsEnabled: localBatchesEnabled,
    bulkDisabledReason: localBatchesEnabled ? "" : uiText(t, BULK_DISABLED_REASON),
    isSelected,
    focusedId: selection.focusedId,
    focusRow,
    clickRow,
    toggleSelected,
    clear,
    selectAll,
    dragSubjectFor,
    evaluate,
    commitReorder,
    previewReorder,
    nudgeSelection,
    onKeyDown,
    treeRows,
    collapsed,
    toggleFolder,
    focusedFolder,
    setFocusedFolder,
    onTreeKeyDown,
    clearedHint,
    showHint,
    dismissHint: () => setClearedHint(null),
    coachMarkOpen,
    dismissCoachMark,
  };
}

export type LibraryWorkspaceApi = ReturnType<typeof useLibraryWorkspace>;
