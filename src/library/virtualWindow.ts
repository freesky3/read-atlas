// Hub 列表窗口：只渲染视口附近的行/卡片，避免 1,000+ Paper 同时进 React 树。
// jsdom 没有真实布局时 viewportHeight 为 0，此时退回全量渲染（测试与首帧）。

export const HUB_VIRTUALIZE_AFTER = 48;
export const HUB_GRID_MIN_CARD_WIDTH = 340;
export const HUB_GRID_GAP = 16;
export const HUB_GRID_CARD_HEIGHT = 196;
export const HUB_TABLE_ROW_HEIGHT = 44;
export const HUB_OVERSCAN = 4;

export type VirtualWindow = Readonly<{
  start: number;
  end: number;
  paddingStart: number;
  paddingEnd: number;
  virtualized: boolean;
}>;

export function gridColumnCount(containerWidth: number): number {
  if (containerWidth <= 0) return 1;
  const cell = HUB_GRID_MIN_CARD_WIDTH + HUB_GRID_GAP;
  return Math.max(1, Math.floor((containerWidth + HUB_GRID_GAP) / cell));
}

export function virtualWindow(input: {
  count: number;
  scrollTop: number;
  viewportHeight: number;
  itemHeight: number;
  overscan?: number;
  columns?: number;
  /** 键盘焦点所在项必须留在窗口内，否则方向键会落到未挂载的 option 上。 */
  ensureIndex?: number | null;
}): VirtualWindow {
  const columns = Math.max(1, input.columns ?? 1);
  const overscan = input.overscan ?? HUB_OVERSCAN;
  const count = Math.max(0, input.count);
  if (
    count <= HUB_VIRTUALIZE_AFTER ||
    input.viewportHeight <= 0 ||
    input.itemHeight <= 0
  ) {
    return { start: 0, end: count, paddingStart: 0, paddingEnd: 0, virtualized: false };
  }
  const rowHeight = input.itemHeight;
  const rows = Math.ceil(count / columns);
  let firstRow = Math.max(0, Math.floor(input.scrollTop / rowHeight) - overscan);
  const visibleRows = Math.ceil(input.viewportHeight / rowHeight) + overscan * 2;
  let lastRow = Math.min(rows, firstRow + visibleRows);
  if (
    input.ensureIndex != null &&
    input.ensureIndex >= 0 &&
    input.ensureIndex < count
  ) {
    const ensureRow = Math.floor(input.ensureIndex / columns);
    firstRow = Math.min(firstRow, ensureRow);
    lastRow = Math.max(lastRow, ensureRow + 1);
  }
  const start = firstRow * columns;
  const end = Math.min(count, lastRow * columns);
  return {
    start,
    end,
    paddingStart: firstRow * rowHeight,
    paddingEnd: Math.max(0, (rows - lastRow) * rowHeight),
    virtualized: true,
  };
}
