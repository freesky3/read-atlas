import { uiText, zhT, type TranslateFn } from "../i18n/uiText";
// 统一的落点判定（DropEvaluation）：拖拽、右键菜单与键盘替代必须共用它。
// 合同见 docs/library-workspace-plan-2026-08.md §6.3 / §6.4 / §5.5。
//
// 关键不变量（D-062）：任何 reorder 入口提交的都必须是「该物理叶子
// collection 全部 live Paper 的精确置换」，搜索、Smart Collection、「全部」
// 与含子孙的视图一律拒绝。

import type { HubSortMode } from "../hubSort";

export const LIBRARY_ROOTS = ["Papers", "Textbooks"] as const;

export type DropReason =
  | "search_reorder_disabled"
  | "not_manual_mode"
  | "incomplete_collection"
  | "mixed_roots"
  | "same_destination"
  | "descendant_folder"
  | "smart_collection_not_destination"
  | "smart_collection_not_reorderable"
  | "kind_change_requires_dialog"
  | "all_matching_drag_unsupported"
  // 计划列出的九种原因之外的一种：落在「全部文档」上没有任何物理目录
  // 语义可用，既不是 smart collection 也不是同根冲突，必须给出原因。
  | "no_destination";

export type DropAction =
  | { kind: "move_to_folder"; targetFolder: string }
  | { kind: "reorder"; targetId: string | null; place: "before" | "after" | "end" };

export type DropEvaluation =
  | { allowed: true; action: DropAction }
  | { allowed: false; reason: DropReason; label: string };

export type DragSubject = Readonly<{
  kind: "paper" | "folder";
  /** paper id 或 folder relativePath */
  id: string;
  /** paper 当前所在物理目录；folder 即它自己的路径 */
  collection: string;
  /** 本次真正会被移动的 paper id（folder 拖拽为其子孙 paper） */
  memberIds: readonly string[];
  /** 与 memberIds 一一对应的来源物理目录（「全部」视图下可能跨子目录） */
  memberCollections: readonly string[];
  /** 逻辑 all_matching 选择不能被直接拖拽 */
  allMatching: boolean;
}>;

export type DropTarget =
  | { type: "collection"; path: string; smart?: boolean }
  | { type: "reorder"; anchorId: string | null; place: "before" | "after" | "end" }
  | { type: "none" };

export type ReorderContext = Readonly<{
  sortMode: HubSortMode;
  /** hubSortAvailability().manual：本层无子孙 PDF 且不是「全部」 */
  manualAvailable: boolean;
  searching: boolean;
  /** 当前可手排的完整一层 live paper id（顺序为当前展示顺序） */
  layerIds: readonly string[];
  /** 智能集合不是物理层，不能提交 D-062 置换 */
  smartCollection?: boolean;
}>;

export type DropInput = Readonly<{
  subject: DragSubject;
  target: DropTarget;
  reorder: ReorderContext;
}>;

const LABELS: Record<DropReason, string> = {
  search_reorder_disabled: "搜索结果不是完整顺序，请清除搜索后排序",
  not_manual_mode: "切换到「手动」排序后才能拖拽改序",
  incomplete_collection: "当前视图包含子目录文档，不是完整单层顺序，请进入该文件夹",
  mixed_roots: "不能跨 Papers / Textbooks 拖拽，请使用「移动到……」",
  same_destination: "已经在该目录中",
  descendant_folder: "不能把目录移动到自身或其子目录内",
  smart_collection_not_destination: "智能集合由规则生成，不是目录",
  smart_collection_not_reorderable: "智能集合由规则生成，不能手排",
  kind_change_requires_dialog: "跨根移动会改变文献种类，请使用「移动到……」确认",
  all_matching_drag_unsupported: "「全选当前结果」是逻辑选择，请从批量工具栏执行",
  no_destination: "请先在左侧选择一个物理目录再拖放",
};

export function dropLabel(reason: DropReason, t: TranslateFn = zhT): string {
  return uiText(t, LABELS[reason]);
}

function deny(reason: DropReason): DropEvaluation {
  return { allowed: false, reason, label: LABELS[reason] };
}

export function rootOf(path: string): string {
  return path.split("/")[0] ?? "";
}

export function parentOf(path: string): string {
  const idx = path.lastIndexOf("/");
  return idx < 0 ? "" : path.slice(0, idx);
}

function evaluateFolderMove(subject: DragSubject, targetFolder: string): DropEvaluation {
  if (!targetFolder) return deny("no_destination");
  if (targetFolder === subject.id) return deny("same_destination");
  if (targetFolder.startsWith(`${subject.id}/`)) return deny("descendant_folder");
  if (parentOf(subject.id) === targetFolder) return deny("same_destination");
  if (rootOf(subject.id) !== rootOf(targetFolder)) return deny("mixed_roots");
  return { allowed: true, action: { kind: "move_to_folder", targetFolder } };
}

function evaluatePaperMove(subject: DragSubject, targetFolder: string): DropEvaluation {
  if (!targetFolder) return deny("no_destination");
  const origins = subject.memberCollections.length > 0 ? subject.memberCollections : [subject.collection];
  if (origins.some((origin) => rootOf(origin) !== rootOf(targetFolder))) return deny("kind_change_requires_dialog");
  if (origins.every((origin) => origin === targetFolder)) return deny("same_destination");
  return { allowed: true, action: { kind: "move_to_folder", targetFolder } };
}

export function evaluateDrop(input: DropInput): DropEvaluation {
  const { subject, target, reorder } = input;
  if (target.type === "none") return deny("no_destination");
  if (subject.allMatching) return deny("all_matching_drag_unsupported");

  if (target.type === "collection") {
    if (target.smart) return deny("smart_collection_not_destination");
    if (!target.path) return deny("no_destination");
    if (rootOf(target.path) !== "Papers" && rootOf(target.path) !== "Textbooks") return deny("no_destination");
    return subject.kind === "folder"
      ? evaluateFolderMove(subject, target.path)
      : evaluatePaperMove(subject, target.path);
  }

  // reorder 只对手动排序的完整单层物理集合成立。
  if (subject.kind === "folder") return deny("incomplete_collection");
  if (reorder.smartCollection) return deny("smart_collection_not_reorderable");
  if (reorder.searching) return deny("search_reorder_disabled");
  if (reorder.sortMode !== "manual") return deny("not_manual_mode");
  if (!reorder.manualAvailable) return deny("incomplete_collection");
  const members = subject.memberIds.length > 0 ? subject.memberIds : [subject.id];
  if (members.some((id) => !reorder.layerIds.includes(id))) return deny("incomplete_collection");
  if (target.place !== "end" && target.anchorId && members.includes(target.anchorId)) {
    return { allowed: false, reason: "same_destination", label: "顺序未变化" };
  }
  return { allowed: true, action: { kind: "reorder", targetId: target.anchorId, place: target.place } };
}

/**
 * 把 movedIds 作为一个整体搬到 targetId 前/后或末尾，保持块内相对顺序，
 * 返回的数组是 layerIds 的一个排列（调用方必须提交整层，不得提交子集）。
 */
export function moveIdsBlock(
  layerIds: readonly string[],
  movedIds: readonly string[],
  targetId: string | null,
  place: "before" | "after" | "end",
): string[] {
  const moved = new Set(movedIds);
  const ordered = movedIds.filter((id) => layerIds.includes(id));
  if (ordered.length === 0) return [...layerIds];
  const rest = layerIds.filter((id) => !moved.has(id));
  if (place === "end" || targetId === null) return [...rest, ...ordered];
  const anchorIndex = rest.indexOf(targetId);
  // 落在被移动块自身上：保持原序，交由调用方按「未变化」处理。
  if (anchorIndex < 0) return [...layerIds];
  const insertAt = place === "before" ? anchorIndex : anchorIndex + 1;
  return [...rest.slice(0, insertAt), ...ordered, ...rest.slice(insertAt)];
}

export type ReorderPlan =
  | { allowed: true; paperIds: string[] }
  | { allowed: false; reason: DropReason; label: string };

/**
 * 统一的 reorder 提交口：拖拽、Alt+↑/↓、移到最前/最后、多选块移动都走这里，
 * 因此「完整精确置换」只有一处守卫。layerIds 必须是该 collection 全部 live id。
 */
export function planReorder(
  subject: DragSubject,
  target: Extract<DropTarget, { type: "reorder" }>,
  reorder: ReorderContext,
): ReorderPlan {
  const evaluation = evaluateDrop({ subject, target, reorder });
  if (!evaluation.allowed) return { allowed: false, reason: evaluation.reason, label: evaluation.label };
  const members = subject.memberIds.length > 0 ? subject.memberIds : [subject.id];
  const paperIds = moveIdsBlock(reorder.layerIds, members, target.anchorId, target.place);
  if (paperIds.length !== reorder.layerIds.length) {
    return { allowed: false, reason: "incomplete_collection", label: dropLabel("incomplete_collection") };
  }
  const unchanged = paperIds.every((id, index) => id === reorder.layerIds[index]);
  if (unchanged) return { allowed: false, reason: "same_destination", label: "顺序未变化" };
  return { allowed: true, paperIds };
}
