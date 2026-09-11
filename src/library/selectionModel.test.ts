import { describe, expect, it } from "vitest";
import {
  applyClickSelection,
  canSubmitSelection,
  clearSelection,
  emptySelection,
  isItemSelected,
  isSelectionEmpty,
  moveFocusedId,
  reconcileWithVisibleIds,
  resolvedSelectionIds,
  selectAllMatching,
  selectOnly,
  selectRange,
  selectionCount,
  selectionForViewKey,
  setFocusedId,
  toggleId,
  type QuerySnapshot,
  type SelectionSet,
} from "./selectionModel";

const VISIBLE = ["p1", "p2", "p3", "p4", "p5"];

function explicit(ids: string[], extra: Partial<SelectionSet> = {}): SelectionSet {
  return { ...emptySelection("Papers/A"), ids: new Set(ids), ...extra };
}

const snapshot: QuerySnapshot = {
  queryDigest: "qdigest-1",
  dependencyRevisions: { structure: 3, tags: 1 },
  evaluatedAt: "2026-08-31T00:00:00Z",
  timezone: "Asia/Shanghai",
};

describe("selectionModel — explicit", () => {
  it("Ctrl 切换只影响该项，不改动无关 ID", () => {
    const start = explicit(["p1", "p3"]);
    const added = toggleId(start, "p5");
    expect([...added.ids].sort()).toEqual(["p1", "p3", "p5"]);
    expect(added.anchorId).toBe("p5");
    const removed = toggleId(start, "p1");
    expect([...removed.ids].sort()).toEqual(["p3"]);
    expect(removed.anchorId).toBe("p1");
  });

  it("selectOnly 覆盖为单项并把它设为 anchor", () => {
    const next = selectOnly(explicit(["p1", "p2"]), "p4");
    expect([...next.ids]).toEqual(["p4"]);
    expect(next.anchorId).toBe("p4");
  });

  it("Shift 范围基于当前稳定可见顺序", () => {
    const start = explicit(["p2"], { anchorId: "p2" });
    const range = selectRange(start, "p5", VISIBLE);
    expect([...range.ids].sort()).toEqual(["p2", "p3", "p4", "p5"]);
    const backwards = selectRange(explicit(["p5"], { anchorId: "p5" }), "p1", VISIBLE);
    expect([...backwards.ids].sort()).toEqual(["p1", "p2", "p3", "p4", "p5"]);
  });

  it("Shift 在 anchor 缺失时退化为单项选择", () => {
    const range = selectRange(explicit([]), "p3", VISIBLE);
    expect([...range.ids]).toEqual(["p3"]);
  });

  it("目标不在可见顺序中时退化为只选该单项，不构造假范围", () => {
    const range = selectRange(explicit(["p1"], { anchorId: "p1" }), "p9", VISIBLE);
    expect([...range.ids]).toEqual(["p9"]);
    expect(range.anchorId).toBe("p9");
  });

  it("解析结果按可见顺序，并保留视图外仍被显式选中的项", () => {
    const selection = explicit(["p4", "zz"]);
    expect(resolvedSelectionIds(selection, VISIBLE)).toEqual(["p4", "zz"]);
    expect(selectionCount(selection, VISIBLE)).toBe(2);
  });
});

describe("selectionModel — all_matching", () => {
  it("Ctrl+A 进入逻辑全选，不逐项写入 ids", () => {
    const all = selectAllMatching(explicit(["p1"]), VISIBLE, snapshot);
    expect(all.mode).toBe("all_matching");
    expect(all.ids.size).toBe(0);
    expect(selectionCount(all, VISIBLE)).toBe(VISIBLE.length);
  });

  it("逐项取消写入 excludedIds，重新点击可恢复", () => {
    const all = selectAllMatching(emptySelection("Papers/A"), VISIBLE, snapshot);
    const minusOne = toggleId(all, "p3");
    expect([...minusOne.excludedIds]).toEqual(["p3"]);
    expect(resolvedSelectionIds(minusOne, VISIBLE)).toEqual(["p1", "p2", "p4", "p5"]);
    const restored = toggleId(minusOne, "p3");
    expect(restored.excludedIds.size).toBe(0);
    expect(isItemSelected(restored, "p3")).toBe(true);
  });

  it("可见项全部被排除时选择才算空", () => {
    const all = selectAllMatching(emptySelection("Papers/A"), ["p1"], snapshot);
    expect(isSelectionEmpty(all, ["p1"])).toBe(false);
    expect(isSelectionEmpty(toggleId(all, "p1"), ["p1"])).toBe(true);
  });

  it("all_matching 下的 Shift 只清除范围内的排除项", () => {
    const base = selectAllMatching(emptySelection("Papers/A"), VISIBLE, snapshot);
    const excluded = toggleId(toggleId(base, "p2"), "p3");
    const ranged = selectRange({ ...excluded, anchorId: "p1" }, "p3", VISIBLE);
    expect(ranged.mode).toBe("all_matching");
    expect([...ranged.excludedIds]).toEqual([]);
    expect(resolvedSelectionIds(ranged, VISIBLE)).toEqual(VISIBLE);
  });

  it("没有后端 querySnapshot 时不允许提交 all_matching", () => {
    const unbacked = selectAllMatching(emptySelection("Papers/A"), VISIBLE, null);
    expect(canSubmitSelection(unbacked, VISIBLE)).toBe(false);
    expect(canSubmitSelection({ ...unbacked, querySnapshot: snapshot }, VISIBLE)).toBe(true);
    expect(canSubmitSelection(explicit(["p1"]), VISIBLE)).toBe(true);
  });
});

describe("selectionModel — 视图切换语义", () => {
  it("切换 collection / 筛选表达式清空选择并提示", () => {
    const { selection, cleared } = selectionForViewKey(explicit(["p1", "p2"]), "Papers/B");
    expect(cleared).toBe(true);
    expect(selection.viewKey).toBe("Papers/B");
    expect(isSelectionEmpty(selection)).toBe(true);
  });

  it("viewKey 不变（grid/table 或排序切换）保留所选 ID", () => {
    const { selection, cleared } = selectionForViewKey(explicit(["p1"]), "Papers/A");
    expect(cleared).toBe(false);
    expect([...selection.ids]).toEqual(["p1"]);
  });

  it("从非空切到空视图时不产生多余提示", () => {
    expect(selectionForViewKey(emptySelection("Papers/A"), "").cleared).toBe(false);
  });

  it("可见顺序变化后清除失效 anchor 与 focus，但保留 ID", () => {
    const reconciled = reconcileWithVisibleIds(
      { ...explicit(["p1", "p2"]), anchorId: "gone", focusedId: "gone" },
      VISIBLE,
    );
    expect([...reconciled.ids]).toEqual(["p1", "p2"]);
    expect(reconciled.anchorId).toBeNull();
    expect(reconciled.focusedId).toBeNull();
  });

  it("clearSelection 保留 viewKey", () => {
    const cleared = clearSelection({ ...explicit(["p1"]), viewKey: "Textbooks/C" });
    expect(cleared.viewKey).toBe("Textbooks/C");
    expect(cleared.ids.size).toBe(0);
  });
});

describe("selectionModel — 键盘", () => {
  it("无焦点时 ↓ 落在首项，↑ 落在末项", () => {
    const start = emptySelection("Papers/A");
    expect(moveFocusedId(start, VISIBLE, 1).focusedId).toBe("p1");
    expect(moveFocusedId(start, VISIBLE, -1).focusedId).toBe("p5");
  });

  it("焦点移动不越过边界", () => {
    let selection = setFocusedId(emptySelection("Papers/A"), "p1");
    selection = moveFocusedId(selection, VISIBLE, -1);
    expect(selection.focusedId).toBe("p1");
    selection = moveFocusedId(selection, VISIBLE, 1);
    expect(selection.focusedId).toBe("p2");
  });

  it("焦点为空列表时保持不变", () => {
    expect(moveFocusedId(emptySelection("Papers/A"), [], 1).focusedId).toBeNull();
  });

  it("修饰键派发：Shift 优先于 Ctrl，Meta 视同 Ctrl", () => {
    const start = explicit(["p1"], { anchorId: "p1" });
    expect([...applyClickSelection(start, "p3", VISIBLE, { ctrl: true, shift: true, meta: false }).ids].sort())
      .toEqual(["p1", "p2", "p3"]);
    expect([...applyClickSelection(explicit([]), "p2", VISIBLE, { ctrl: false, shift: false, meta: true }).ids])
      .toEqual(["p2"]);
  });

  it("无修饰键点击是「打开」：清空选择，只留下 anchor 与 focus", () => {
    const next = applyClickSelection(explicit(["p1"]), "p2", VISIBLE, { ctrl: false, shift: false, meta: false });
    expect([...next.ids]).toEqual([]);
    expect(next.anchorId).toBe("p2");
    expect(next.focusedId).toBe("p2");
  });

  it("Shift 目标不在可见顺序时退化为该项单选（anchor 缺失路径不保留旧选择）", () => {
    const next = applyClickSelection(explicit(["p1", "p2"], { anchorId: "p1" }), "zz", VISIBLE,
      { ctrl: false, shift: true, meta: false });
    expect([...next.ids]).toEqual(["zz"]);
  });
});
