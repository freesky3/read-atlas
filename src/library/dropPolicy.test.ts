import { describe, expect, it } from "vitest";
import {
  evaluateDrop,
  moveIdsBlock,
  planReorder,
  type DragSubject,
  type DropTarget,
  type ReorderContext,
} from "./dropPolicy";

function paperSubject(overrides: Partial<DragSubject> = {}): DragSubject {
  return {
    kind: "paper",
    id: "p1",
    collection: "Papers/A",
    memberIds: ["p1"],
    memberCollections: ["Papers/A"],
    allMatching: false,
    ...overrides,
  };
}

function folderSubject(overrides: Partial<DragSubject> = {}): DragSubject {
  return {
    kind: "folder",
    id: "Papers/A",
    collection: "Papers/A",
    memberIds: ["p1", "p2"],
    memberCollections: ["Papers/A", "Papers/A"],
    allMatching: false,
    ...overrides,
  };
}

const manual: ReorderContext = {
  sortMode: "manual",
  manualAvailable: true,
  searching: false,
  layerIds: ["p1", "p2", "p3"],
};

const collectionTarget = (path: string, smart = false): DropTarget => ({ type: "collection", path, smart });
const reorderTarget = (anchorId: string | null, place: "before" | "after" | "end"): DropTarget =>
  ({ type: "reorder", anchorId, place });

describe("dropPolicy — 物理目录落点", () => {
  it("同根跨目录拖 paper 允许移动", () => {
    expect(evaluateDrop({ subject: paperSubject(), target: collectionTarget("Papers/B"), reorder: manual }))
      .toEqual({ allowed: true, action: { kind: "move_to_folder", targetFolder: "Papers/B" } });
  });

  it("跨 Papers / Textbooks 拖 paper 必须走移动到对话框", () => {
    const result = evaluateDrop({ subject: paperSubject(), target: collectionTarget("Textbooks/B"), reorder: manual });
    expect(result.allowed).toBe(false);
    if (!result.allowed) expect(result.reason).toBe("kind_change_requires_dialog");
  });

  it("多选成员里只要有一个跨根就整体要求对话框", () => {
    const subject = paperSubject({
      memberIds: ["p1", "p2"],
      memberCollections: ["Papers/A", "Textbooks/B"],
    });
    const result = evaluateDrop({ subject, target: collectionTarget("Papers/C"), reorder: manual });
    expect(result.allowed).toBe(false);
    if (!result.allowed) expect(result.reason).toBe("kind_change_requires_dialog");
  });

  it("已经在该目录时给出 same_destination 而不是静默无响应", () => {
    const result = evaluateDrop({ subject: paperSubject(), target: collectionTarget("Papers/A"), reorder: manual });
    expect(result.allowed).toBe(false);
    if (!result.allowed) expect(result.reason).toBe("same_destination");
  });

  it("「全部」不是拖放目标", () => {
    for (const target of [collectionTarget(""), { type: "none" } as DropTarget]) {
      const result = evaluateDrop({ subject: paperSubject(), target, reorder: manual });
      expect(result.allowed).toBe(false);
      if (!result.allowed) expect(result.reason).toBe("no_destination");
    }
  });

  it("Smart Collection 永远不是目标，并说明原因", () => {
    const result = evaluateDrop({ subject: paperSubject(), target: collectionTarget("smart:unread", true), reorder: manual });
    expect(result.allowed).toBe(false);
    if (!result.allowed) {
      expect(result.reason).toBe("smart_collection_not_destination");
      expect(result.label).toContain("规则生成");
    }
  });

  it("Smart Collection 不能手排", () => {
    const result = evaluateDrop({
      subject: paperSubject(),
      target: { type: "reorder", anchorId: "p2", place: "after" },
      reorder: { ...manual, smartCollection: true },
    });
    expect(result.allowed).toBe(false);
    if (!result.allowed) expect(result.reason).toBe("smart_collection_not_reorderable");
  });
});

describe("dropPolicy — 目录拖放", () => {
  it("不能拖到自身或自己的父目录", () => {
    for (const path of ["Papers/A", "Papers"]) {
      const result = evaluateDrop({ subject: folderSubject(), target: collectionTarget(path), reorder: manual });
      expect(result.allowed).toBe(false);
      if (!result.allowed) expect(result.reason).toBe("same_destination");
    }
  });

  it("不能拖进自己的子孙", () => {
    const result = evaluateDrop({ subject: folderSubject(), target: collectionTarget("Papers/A/Sub"), reorder: manual });
    expect(result.allowed).toBe(false);
    if (!result.allowed) expect(result.reason).toBe("descendant_folder");
  });

  it("目录不能跨根，也不能参与改序", () => {
    const crossRoot = evaluateDrop({ subject: folderSubject(), target: collectionTarget("Textbooks/B"), reorder: manual });
    expect(!crossRoot.allowed && crossRoot.reason).toBe("mixed_roots");
    const reorder = evaluateDrop({ subject: folderSubject(), target: reorderTarget("p2", "after"), reorder: manual });
    expect(!reorder.allowed && reorder.reason).toBe("incomplete_collection");
  });
});

describe("dropPolicy — 改序落点", () => {
  it("搜索中禁止改序并说明原因", () => {
    const result = evaluateDrop({
      subject: paperSubject(),
      target: reorderTarget("p2", "after"),
      reorder: { ...manual, searching: true },
    });
    expect(result.allowed).toBe(false);
    if (!result.allowed) {
      expect(result.reason).toBe("search_reorder_disabled");
      expect(result.label).toContain("清除搜索");
    }
  });

  it("非手动排序模式要求先切到手动", () => {
    const result = evaluateDrop({
      subject: paperSubject(),
      target: reorderTarget("p2", "after"),
      reorder: { ...manual, sortMode: "recent" },
    });
    expect(!result.allowed && result.reason).toBe("not_manual_mode");
  });

  it("「全部」或含子孙的视图不允许改序", () => {
    const result = evaluateDrop({
      subject: paperSubject(),
      target: reorderTarget("p2", "after"),
      reorder: { ...manual, manualAvailable: false },
    });
    expect(!result.allowed && result.reason).toBe("incomplete_collection");
  });

  it("拖到不在本层的卡片上视为不完整集合，绝不提交子集", () => {
    const result = evaluateDrop({
      subject: paperSubject({ memberIds: ["p1", "outside"], memberCollections: ["Papers/A", "Papers/Z"] }),
      target: reorderTarget("p2", "after"),
      reorder: manual,
    });
    expect(!result.allowed && result.reason).toBe("incomplete_collection");
  });

  it("逻辑 all_matching 选择不能直接拖拽", () => {
    const subject = paperSubject({ allMatching: true, memberIds: ["p1", "p2"], memberCollections: ["Papers/A", "Papers/A"] });
    const reorder = evaluateDrop({ subject, target: reorderTarget("p3", "after"), reorder: manual });
    expect(!reorder.allowed && reorder.reason).toBe("all_matching_drag_unsupported");
    const move = evaluateDrop({ subject, target: collectionTarget("Papers/B"), reorder: manual });
    expect(!move.allowed && move.reason).toBe("all_matching_drag_unsupported");
  });

  it("拖到自己身上不产生写入", () => {
    const result = planReorder(paperSubject(), { type: "reorder", anchorId: "p1", place: "after" }, manual);
    expect(result.allowed).toBe(false);
  });
});

describe("dropPolicy — 精确置换", () => {
  it("块移动保持块内相对顺序且长度等于整层", () => {
    const moved = moveIdsBlock(["p1", "p2", "p3", "p4", "p5"], ["p2", "p4"], "p5", "after");
    expect(moved).toEqual(["p1", "p3", "p5", "p2", "p4"]);
    expect(moved).toHaveLength(5);
    expect([...moved].sort()).toEqual(["p1", "p2", "p3", "p4", "p5"]);
  });

  it("落到末尾", () => {
    expect(moveIdsBlock(["p1", "p2", "p3"], ["p1"], null, "end")).toEqual(["p2", "p3", "p1"]);
    expect(moveIdsBlock(["p1", "p2", "p3"], ["p1"], "p3", "end")).toEqual(["p2", "p3", "p1"]);
  });

  it("落在被移动块自身上不改变顺序", () => {
    const ids = ["p1", "p2", "p3"];
    expect(moveIdsBlock(ids, ["p1", "p2"], "p1", "after")).toEqual(ids);
  });

  it("多选块改序提交整层 id，而不是可见子集", () => {
    const subject = paperSubject({
      memberIds: ["p1", "p3"],
      memberCollections: ["Papers/A", "Papers/A"],
    });
    const plan = planReorder(subject, { type: "reorder", anchorId: "p2", place: "after" }, manual);
    expect(plan.allowed).toBe(true);
    if (plan.allowed) {
      expect(plan.paperIds).toEqual(["p2", "p1", "p3"]);
      expect(plan.paperIds).toHaveLength(manual.layerIds.length);
    }
  });

  it("顺序未变化时拒绝写入", () => {
    const plan = planReorder(paperSubject(), { type: "reorder", anchorId: "p1", place: "before" }, {
      ...manual,
      layerIds: ["p1", "p2", "p3"],
    });
    expect(plan.allowed).toBe(false);
    if (!plan.allowed) expect(plan.reason).toBe("same_destination");
  });

  it("键盘 Alt+↓ 与拖拽共用同一入口并提交完整置换", () => {
    const plan = planReorder(paperSubject(), { type: "reorder", anchorId: "p2", place: "after" }, manual);
    expect(plan.allowed).toBe(true);
    if (plan.allowed) expect(plan.paperIds).toEqual(["p2", "p1", "p3"]);
  });
});
