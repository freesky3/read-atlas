import { describe, expect, it } from "vitest";
import {
  flattenVisible,
  moveTreeFocus,
  parentPathOf,
  pruneCollapsed,
  toggleCollapsed,
  treeStorageKey,
  type CollectionRow,
} from "./treeModel";

// 故意打乱顺序，并让 parentId 指向真实父行，验证确实按 parentId 建树。
const rows: CollectionRow[] = [
  { id: "c5", parentId: "c3", name: "Attention", relativePath: "Papers/Arch/Attention" },
  { id: "c3", parentId: "c1", name: "Arch", relativePath: "Papers/Arch" },
  { id: "c4", parentId: "c1", name: "Bench", relativePath: "Papers/Bench" },
  { id: "c1", parentId: null, name: "Papers", relativePath: "Papers" },
  { id: "c2", parentId: null, name: "Textbooks", relativePath: "Textbooks" },
];

const pathsOf = (list: { path: string }[]) => list.map((row) => row.path);

describe("treeModel — 建树", () => {
  it("深度优先输出，父目录始终排在子目录之前", () => {
    const rows_out = flattenVisible(rows, new Set<string>());
    expect(pathsOf(rows_out)).toEqual([
      "Papers",
      "Papers/Arch",
      "Papers/Arch/Attention",
      "Papers/Bench",
      "Textbooks",
    ]);
  });

  it("缺失的中间祖先会被补齐，不会丢叶子", () => {
    const orphan: CollectionRow[] = [{ id: "x", parentId: null, name: "Deep", relativePath: "Textbooks/Book/Deep" }];
    expect(pathsOf(flattenVisible(orphan, new Set<string>()))).toEqual([
      "Textbooks",
      "Textbooks/Book",
      "Textbooks/Book/Deep",
    ]);
  });

  it("childCount 与 depth 正确", () => {
    const out = flattenVisible(rows, new Set<string>());
    const papers = out.find((row) => row.path === "Papers")!;
    expect(papers.childCount).toBe(2);
    expect(papers.depth).toBe(1);
    expect(out.find((row) => row.path === "Papers/Arch")!.depth).toBe(2);
  });

  it("文档数由调用方注入", () => {
    const out = flattenVisible(rows, new Set<string>(), { documentCountOf: (p) => (p === "Papers" ? 7 : 1) });
    expect(out.find((row) => row.path === "Papers")!.documentCount).toBe(7);
  });

  it("parentPathOf 只切最后一段", () => {
    expect(parentPathOf("Papers/Arch/Attention")).toBe("Papers/Arch");
    expect(parentPathOf("Papers")).toBeNull();
  });
});

describe("treeModel — 展开状态", () => {
  it("折叠父目录时父目录仍在，只隐藏子孙", () => {
    const out = flattenVisible(rows, new Set(["Papers/Arch"]));
    expect(pathsOf(out)).toEqual(["Papers", "Papers/Arch", "Papers/Bench", "Textbooks"]);
    expect(out.find((row) => row.path === "Papers/Arch")!.expanded).toBe(false);
  });

  it("折叠根目录时其子树消失，根本身仍然可选", () => {
    const out = flattenVisible(rows, new Set(["Papers"]));
    expect(pathsOf(out)).toEqual(["Papers", "Textbooks"]);
    expect(out.find((row) => row.path === "Papers")!.expanded).toBe(false);
  });

  it("toggleCollapsed 在展开与折叠间切换", () => {
    const collapsed = toggleCollapsed(new Set<string>(), "Papers/Arch");
    expect(collapsed.has("Papers/Arch")).toBe(true);
    expect(toggleCollapsed(collapsed, "Papers/Arch").has("Papers/Arch")).toBe(false);
  });

  it("pruneCollapsed 剔除已不存在的节点", () => {
    const pruned = pruneCollapsed(new Set(["Papers/Gone", "Papers/Arch"]), ["Papers", "Papers/Arch"]);
    expect([...pruned]).toEqual(["Papers/Arch"]);
  });

  it("展开状态按 workspace 与 UI schema 版本隔离", () => {
    expect(treeStorageKey("D:/ws-a")).toBe("read-desktop.folderTree.v1.D:/ws-a");
    expect(treeStorageKey("D:/ws-a")).not.toBe(treeStorageKey("D:/ws-b"));
    expect(treeStorageKey("D:/ws-a", 2)).not.toBe(treeStorageKey("D:/ws-a", 1));
  });
});

describe("treeModel — 键盘", () => {
  const visible = flattenVisible(rows, new Set<string>());

  it("↑/↓ 在可见行间移动，越界不动", () => {
    expect(moveTreeFocus(visible, null, "down")).toEqual({ kind: "move", path: "Papers" });
    expect(moveTreeFocus(visible, "Papers", "down")).toEqual({ kind: "move", path: "Papers/Arch" });
    expect(moveTreeFocus(visible, "Papers", "up")).toEqual({ kind: "move", path: "Papers" });
    const last = visible[visible.length - 1].path;
    expect(moveTreeFocus(visible, last, "down").kind).toBe("move");
    expect(moveTreeFocus(visible, last, "down")).toEqual({ kind: "move", path: last });
  });

  it("→ 展开、← 收起，且只在有子目录时生效", () => {
    const collapsed = flattenVisible(rows, new Set(["Papers/Arch"]));
    expect(moveTreeFocus(collapsed, "Papers/Arch", "expand")).toEqual({ kind: "expand", path: "Papers/Arch" });
    expect(moveTreeFocus(visible, "Papers/Arch", "collapse")).toEqual({ kind: "collapse", path: "Papers/Arch" });
    expect(moveTreeFocus(visible, "Papers/Bench", "collapse")).toEqual({ kind: "none" });
    expect(moveTreeFocus(visible, "Papers/Bench", "expand")).toEqual({ kind: "none" });
  });

  it("空列表与无焦点时的折叠按键不产生动作", () => {
    expect(moveTreeFocus([], null, "down")).toEqual({ kind: "none" });
    expect(moveTreeFocus(visible, null, "collapse")).toEqual({ kind: "none" });
  });
});
