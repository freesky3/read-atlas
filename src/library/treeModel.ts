// 目录树：按 parentId 构树 + expand/collapse 状态归约。
// 合同见 docs/library-workspace-plan-2026-08.md §6.5。
// 旧实现按路径排序全量平铺，箭头只是装饰；这里树形状与展开状态分离，
// 展开状态按 workspace 记忆，并且必须能剔除已消失的节点。

export const TREE_UI_SCHEMA_VERSION = 1;

export type CollectionRow = Readonly<{
  id: string;
  parentId: string | null;
  name: string;
  relativePath: string;
}>;

export type FolderNode = Readonly<{
  path: string;
  parentPath: string | null;
  name: string;
  depth: number;
  documentCount: number;
  childCount: number;
}>;

export type VisibleTreeRow = FolderNode & Readonly<{ expanded: boolean }>;

export function parentPathOf(path: string): string | null {
  const idx = path.lastIndexOf("/");
  return idx < 0 ? null : path.slice(0, idx);
}

function compareFolders(a: { name: string }, b: { name: string }): number {
  return a.name.localeCompare(b.name, "zh-Hans-CN", { numeric: true });
}

/**
 * 由 collection 行构建 parentPath -> children 的邻接表。
 * 优先信任声明的 parentId；声明的父行不存在（脏数据 / 老库）时退回路径推断，
 * 缺失的祖先目录会被补齐，保证树不会因为断链而丢节点。
 */
function buildAdjacency(rows: readonly CollectionRow[]): { children: Map<string | null, FolderNode[]> } {
  const byId = new Map<string, CollectionRow>();
  for (const row of rows) if (row.id) byId.set(row.id, row);

  const uniqueRows = new Map<string, CollectionRow>();
  for (const row of rows) if (!uniqueRows.has(row.relativePath)) uniqueRows.set(row.relativePath, row);

  const parentOf = new Map<string, string | null>();
  for (const row of uniqueRows.values()) {
    const declared = row.parentId ? byId.get(row.parentId) : undefined;
    parentOf.set(row.relativePath, declared && declared.relativePath !== row.relativePath
      ? declared.relativePath
      : parentPathOf(row.relativePath));
  }

  const paths = new Set<string>(uniqueRows.keys());
  for (const path of [...paths]) {
    let ancestor = parentPathOf(path);
    while (ancestor && !paths.has(ancestor)) {
      paths.add(ancestor);
      parentOf.set(ancestor, parentPathOf(ancestor));
      ancestor = parentPathOf(ancestor);
    }
  }

  const children = new Map<string | null, FolderNode[]>();
  const push = (parentPath: string | null, path: string) => {
    const node: FolderNode = {
      path,
      parentPath,
      name: path.split("/").pop() || path,
      depth: path.split("/").length,
      documentCount: 0,
      childCount: 0,
    };
    const list = children.get(parentPath) ?? [];
    list.push(node);
    children.set(parentPath, list);
  };
  for (const path of paths) push(parentOf.get(path) ?? null, path);
  for (const list of children.values()) list.sort(compareFolders);
  return { children };
}

/**
 * 深度优先铺平为可见行：被折叠的目录本身仍然显示，只有它的子孙因为不递归而隐藏。
 */
export function flattenVisible(
  rows: readonly CollectionRow[],
  collapsed: ReadonlySet<string>,
  options: { documentCountOf?: (path: string) => number } = {},
): VisibleTreeRow[] {
  const { children } = buildAdjacency(rows);
  const countOf = options.documentCountOf ?? (() => 0);
  const result: VisibleTreeRow[] = [];

  const walk = (parentPath: string | null) => {
    const list = children.get(parentPath) ?? [];
    for (const node of list) {
      const expanded = !collapsed.has(node.path);
      result.push({
        ...node,
        documentCount: countOf(node.path),
        childCount: (children.get(node.path) ?? []).length,
        expanded,
      });
      if (expanded) walk(node.path);
    }
  };
  walk(null);
  return result;
}

export type TreeMove = "up" | "down" | "expand" | "collapse";

export type TreeKeyboardResult =
  | { kind: "move"; path: string }
  | { kind: "collapse"; path: string }
  | { kind: "expand"; path: string }
  | { kind: "none" };

/**
 * 键盘导航纯归约：↑/↓ 在可见行间移动焦点，→ 展开，← 收起。
 * 折叠 / 展开只在真的有子目录时生效；焦点保持在原节点上。
 */
export function moveTreeFocus(
  rows: ReadonlyArray<Pick<VisibleTreeRow, "path" | "expanded" | "childCount">>,
  focusedPath: string | null,
  move: TreeMove,
): TreeKeyboardResult {
  const index = focusedPath ? rows.findIndex((row) => row.path === focusedPath) : -1;
  if (move === "up" || move === "down") {
    if (rows.length === 0) return { kind: "none" };
    if (index < 0) return { kind: "move", path: rows[move === "down" ? 0 : rows.length - 1].path };
    const next = move === "down" ? Math.min(rows.length - 1, index + 1) : Math.max(0, index - 1);
    return { kind: "move", path: rows[next].path };
  }
  if (index < 0) return { kind: "none" };
  const row = rows[index];
  if (row.childCount === 0) return { kind: "none" };
  if (move === "collapse" && row.expanded) return { kind: "collapse", path: row.path };
  if (move === "expand" && !row.expanded) return { kind: "expand", path: row.path };
  return { kind: "none" };
}

/** 剔除已不存在的节点，避免切换 Workspace 或删除目录后残留展开状态。 */
export function pruneCollapsed(collapsed: ReadonlySet<string>, existingPaths: Iterable<string>): Set<string> {
  const existing = new Set(existingPaths);
  return new Set([...collapsed].filter((path) => existing.has(path)));
}

export function toggleCollapsed(collapsed: ReadonlySet<string>, path: string): Set<string> {
  const next = new Set(collapsed);
  if (next.has(path)) next.delete(path);
  else next.add(path);
  return next;
}

export function treeStorageKey(workspaceId: string, uiSchemaVersion: number = TREE_UI_SCHEMA_VERSION): string {
  return `read-desktop.folderTree.v${uiSchemaVersion}.${workspaceId}`;
}
