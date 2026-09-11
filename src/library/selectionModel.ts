// Selection Set（选择集）纯逻辑：只在内存，不落库。
// 合同见 docs/library-workspace-plan-2026-08.md §5.1。
//
// 两种模式：
// - explicit：显式 ID 集合（Ctrl / Shift / 复选框）。
// - all_matching：「全选当前筛选结果」的逻辑选择，成员由 query 解析，
//   逐项取消写入 excludedIds，绝不逐条 setState。
//
// querySnapshot 来自 `hub_page` 的 `selectionDigest` 与依赖 revision 向量。
// 没有 client / 快照为 null 时 Selection Snapshot 无法冻结成员，提交型动作
// 必须拒绝 all_matching，只能显式解析为已加载页的 ID 列表。

export type SelectionMode = "explicit" | "all_matching";

export type QuerySnapshot = Readonly<{
  queryDigest: string;
  dependencyRevisions: Readonly<Record<string, number>>;
  evaluatedAt: string;
  timezone: string;
}>;

export type SelectionSet = Readonly<{
  mode: SelectionMode;
  viewKey: string;
  ids: ReadonlySet<string>;
  excludedIds: ReadonlySet<string>;
  querySnapshot: QuerySnapshot | null;
  anchorId: string | null;
  focusedId: string | null;
}>;

export type SelectionModifiers = Readonly<{
  ctrl: boolean;
  shift: boolean;
  meta: boolean;
}>;

const EMPTY = new Set<string>();

export function emptySelection(viewKey = ""): SelectionSet {
  return {
    mode: "explicit",
    viewKey,
    ids: EMPTY,
    excludedIds: EMPTY,
    querySnapshot: null,
    anchorId: null,
    focusedId: null,
  };
}

export function isSelectionEmpty(selection: SelectionSet, visibleIds: readonly string[] = []): boolean {
  if (selection.mode === "explicit") return selection.ids.size === 0;
  // all_matching 只有在可见项全部被排除时才算空。
  if (visibleIds.length === 0) return false;
  return visibleIds.every((id) => selection.excludedIds.has(id));
}

/** 当前视图下真实被选中的 ID，按可见顺序返回。 */
export function resolvedSelectionIds(selection: SelectionSet, visibleIds: readonly string[]): string[] {
  if (selection.mode === "explicit") {
    const inView = visibleIds.filter((id) => selection.ids.has(id));
    // 允许操作已筛选掉但仍显式选中的项：按选择插入顺序补在末尾。
    if (inView.length === selection.ids.size) return inView;
    const visible = new Set(inView);
    return [...inView, ...[...selection.ids].filter((id) => !visible.has(id))];
  }
  return visibleIds.filter((id) => !selection.excludedIds.has(id));
}

export function selectionCount(selection: SelectionSet, visibleIds: readonly string[]): number {
  return resolvedSelectionIds(selection, visibleIds).length;
}

export function isItemSelected(selection: SelectionSet, id: string): boolean {
  if (selection.mode === "explicit") return selection.ids.has(id);
  return !selection.excludedIds.has(id);
}

/** query 表达式或 scope 变化：清空选择并告知 UI 需要提示。 */
export function selectionForViewKey(selection: SelectionSet, nextViewKey: string): { selection: SelectionSet; cleared: boolean } {
  if (selection.viewKey === nextViewKey) return { selection, cleared: false };
  if (isSelectionEmpty(selection)) return { selection: emptySelection(nextViewKey), cleared: false };
  return { selection: emptySelection(nextViewKey), cleared: true };
}

/**
 * 仅切换 grid/table 或排序：保留所选 ID，清掉已不可见的 anchor。
 */
export function reconcileWithVisibleIds(selection: SelectionSet, visibleIds: readonly string[]): SelectionSet {
  const visible = new Set(visibleIds);
  const nextAnchor = selection.anchorId && visible.has(selection.anchorId) ? selection.anchorId : null;
  const nextFocus = selection.focusedId && visible.has(selection.focusedId) ? selection.focusedId : null;
  if (nextAnchor === selection.anchorId && nextFocus === selection.focusedId) return selection;
  return { ...selection, anchorId: nextAnchor, focusedId: nextFocus };
}

export function setFocusedId(selection: SelectionSet, id: string | null): SelectionSet {
  if (selection.focusedId === id) return selection;
  return { ...selection, focusedId: id };
}

/**
 * 普通单击正文 = 打开 Reader：清空选择但保留 anchor / focus，
 * 这样紧接着的 Shift+单击仍能从这一行构造范围，又不会为导航动作弹出批量工具栏。
 */
export function focusOnly(selection: SelectionSet, id: string): SelectionSet {
  return { ...emptySelection(selection.viewKey), anchorId: id, focusedId: id };
}

/** 罗ving focus：按可见顺序移动焦点，无焦点时落在首项。 */
export function moveFocusedId(selection: SelectionSet, visibleIds: readonly string[], delta: number): SelectionSet {
  if (visibleIds.length === 0) return selection;
  const current = selection.focusedId ? visibleIds.indexOf(selection.focusedId) : -1;
  const nextIndex = current < 0
    ? (delta > 0 ? 0 : visibleIds.length - 1)
    : Math.min(visibleIds.length - 1, Math.max(0, current + delta));
  return setFocusedId(selection, visibleIds[nextIndex]);
}

export function clearSelection(selection: SelectionSet): SelectionSet {
  return emptySelection(selection.viewKey);
}

/** Ctrl/Cmd + 点击或 Space：切换单项，不影响无关 ID。 */
export function toggleId(selection: SelectionSet, id: string): SelectionSet {
  if (selection.mode === "explicit") {
    const ids = new Set(selection.ids);
    if (ids.has(id)) ids.delete(id);
    else ids.add(id);
    return { ...selection, ids, anchorId: id, focusedId: id };
  }
  const excluded = new Set(selection.excludedIds);
  if (excluded.has(id)) excluded.delete(id);
  else excluded.add(id);
  return { ...selection, excludedIds: excluded, anchorId: id, focusedId: id };
}

/** 普通点击正文：单选该项（不切换选择时用），并把它设为 anchor。 */
export function selectOnly(selection: SelectionSet, id: string): SelectionSet {
  return {
    ...emptySelection(selection.viewKey),
    mode: "explicit",
    ids: new Set([id]),
    anchorId: id,
    focusedId: id,
  };
}

/**
 * Shift + 点击：基于当前稳定可见顺序选中 anchor..id 的范围。
 * anchor 缺失时退化为单项选择。all_matching 下只清除范围内的排除项。
 */
export function selectRange(selection: SelectionSet, id: string, visibleIds: readonly string[]): SelectionSet {
  const anchorIndex = selection.anchorId ? visibleIds.indexOf(selection.anchorId) : -1;
  const targetIndex = visibleIds.indexOf(id);
  if (anchorIndex < 0 || targetIndex < 0) return selectOnly(selection, id);
  const [from, to] = anchorIndex <= targetIndex ? [anchorIndex, targetIndex] : [targetIndex, anchorIndex];
  const range = visibleIds.slice(from, to + 1);
  if (selection.mode === "explicit") {
    const ids = new Set(selection.ids);
    range.forEach((rangeId) => ids.add(rangeId));
    return { ...selection, ids, focusedId: id };
  }
  const excluded = new Set(selection.excludedIds);
  range.forEach((rangeId) => excluded.delete(rangeId));
  return { ...selection, excludedIds: excluded, focusedId: id };
}

/**
 * Ctrl+A：进入 all_matching，不逐项 setState。
 * 只有后端给出 snapshot 时才算可提交快照；否则 snapshot 为 null。
 */
export function selectAllMatching(
  selection: SelectionSet,
  visibleIds: readonly string[],
  snapshot: QuerySnapshot | null = null,
): SelectionSet {
  return {
    mode: "all_matching",
    viewKey: selection.viewKey,
    ids: EMPTY,
    excludedIds: EMPTY,
    querySnapshot: snapshot,
    anchorId: selection.anchorId,
    focusedId: selection.focusedId ?? visibleIds[0] ?? null,
  };
}

/** all_matching 是否已经足够可靠到可以提交给批量动作。 */
export function canSubmitSelection(selection: SelectionSet, visibleIds: readonly string[]): boolean {
  if (selection.mode === "explicit") return selection.ids.size > 0;
  return selection.querySnapshot !== null && !isSelectionEmpty(selection, visibleIds);
}

/** 按修饰键派发一次点击语义。普通单击是「打开」，只移动 anchor/focus 不产生选择。 */
export function applyClickSelection(
  selection: SelectionSet,
  id: string,
  visibleIds: readonly string[],
  modifiers: SelectionModifiers,
): SelectionSet {
  if (modifiers.shift) return selectRange(selection, id, visibleIds);
  if (modifiers.ctrl || modifiers.meta) return toggleId(selection, id);
  return focusOnly(selection, id);
}
