import { uiText, zhT, type TranslateFn } from "../i18n/uiText";
// Explorer / Webview 拖入的纯判定：与内部 Pointer 拖拽分开。
// 合同见 docs/library-workspace-plan-2026-08.md §6.6。

export const MAX_IMPORT_SOURCES = 500;

export type ImportDropEvent = Readonly<{
  type?: string;
  paths?: readonly string[];
  position?: Readonly<{ x?: number; y?: number }> | null;
}>;

export type ImportDropDecision =
  | Readonly<{ action: "overlay"; message: string }>
  | Readonly<{ action: "clear" }>
  | Readonly<{ action: "hint"; message: string }>
  | Readonly<{ action: "import"; paths: readonly string[]; collection: string }>;

export type ImportHit = Readonly<{
  folder: string | null;
  smart: boolean;
  overList: boolean;
}>;

export function importDestination(folder: string | null | undefined): string | null {
  const selected = folder?.trim() ?? "";
  if (!selected || selected.startsWith("smart:")) return null;
  if (selected === "Papers" || selected === "Textbooks") return `${selected}/Inbox`;
  if (selected.startsWith("Papers/") || selected.startsWith("Textbooks/")) return selected;
  return null;
}

/**
 * 落点：物理目录行用该目录；主列表用当前物理目录；智能集合 / 「全部」不能猜。
 * 没有命中元素时才回退到当前选中目录（没有 pointer 坐标的环境）。
 */
export function importDestinationFromHit(
  hit: ImportHit | null,
  selectedFolder: string | null | undefined,
): string | null {
  if (hit?.smart) return null;
  if (hit && hit.folder != null) return importDestination(hit.folder);
  if (hit?.overList) return importDestination(selectedFolder);
  return importDestination(selectedFolder);
}

export function importHitFromElement(el: Element | null): ImportHit {
  const node = el as HTMLElement | null;
  const pill = node?.closest?.("[data-folder]") as HTMLElement | null;
  if (pill && typeof pill.dataset.folder === "string") {
    return {
      folder: pill.dataset.folder,
      smart: pill.dataset.smart === "true",
      overList: false,
    };
  }
  return {
    folder: null,
    smart: false,
    overList: Boolean(node?.closest?.(".hub-paper-list, .main-glass-stage")),
  };
}

export function cssPointFromPhysical(
  position: ImportDropEvent["position"],
  devicePixelRatio = 1,
): { x: number; y: number } | null {
  if (!position || typeof position.x !== "number" || typeof position.y !== "number") return null;
  const ratio = devicePixelRatio > 0 ? devicePixelRatio : 1;
  return { x: position.x / ratio, y: position.y / ratio };
}

export function decideImportDrop(
  event: ImportDropEvent,
  destination: string | null,
  t: TranslateFn = zhT,
): ImportDropDecision {
  const type = event.type ?? "";
  if (type === "leave") return { action: "clear" };
  if (type === "enter" || type === "over") {
    return {
      action: "overlay",
      message: destination ?? uiText(t, "请先选择 Papers 或 Textbooks 下的目录"),
    };
  }
  if (type !== "drop") return { action: "clear" };
  if (!destination) {
    return {
      action: "hint",
      message: uiText(t, "当前不是物理目录，不能猜测导入位置。请先选中 Papers 或 Textbooks 下的文件夹。"),
    };
  }
  // 目录 / 非 PDF / 重复路径交给 Batch Item 记 skipped，不能在前端滤掉后整批无反馈。
  const paths = [...(event.paths ?? [])];
  if (paths.length === 0) {
    return {
      action: "hint",
      message: uiText(t, "没有可导入的路径。目录和非 PDF 会记为逐项跳过，不会整批消失。"),
    };
  }
  if (paths.length > MAX_IMPORT_SOURCES) {
    return { action: "hint", message: uiText(t, "Import at most {count} source files at a time.", { count: MAX_IMPORT_SOURCES }) };
  }
  return { action: "import", paths, collection: destination };
}
