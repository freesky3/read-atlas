/** Route parallel/reverse relations separately without changing graph semantics. */
export function outlineEdgePath(input: {
  sourceX: number; sourceY: number; targetX: number; targetY: number;
  lane: number; selfLoop: boolean; sourceBeforeTarget: boolean;
}) {
  const { sourceX: sx, sourceY: sy, targetX: tx, targetY: ty, lane } = input;
  if (input.selfLoop) {
    const reach = 160 + lane * 192;
    const right = Math.max(sx, tx) + reach;
    const bottom = Math.max(sy, ty) + 32 + lane * 16;
    const top = Math.min(sy, ty) - 32 - lane * 16;
    return {
      path: `M ${sx} ${sy} C ${sx} ${bottom}, ${right} ${bottom}, ${right} ${(sy + ty) / 2} C ${right} ${top}, ${tx} ${top}, ${tx} ${ty}`,
      labelX: right, labelY: (sy + ty) / 2,
    };
  }
  const dx = tx - sx, dy = ty - sy;
  const length = Math.max(1, Math.hypot(dx, dy));
  // Canonical normal keeps reverse edges on their assigned physical lane.
  const orientation = input.sourceBeforeTarget ? 1 : -1;
  const bend = lane * 368;
  const cx = (sx + tx) / 2 - dy / length * bend * orientation;
  const cy = (sy + ty) / 2 + dx / length * bend * orientation;
  return {
    path: `M ${sx} ${sy} Q ${cx} ${cy} ${tx} ${ty}`,
    labelX: (sx + 2 * cx + tx) / 4,
    labelY: (sy + 2 * cy + ty) / 4,
  };
}
