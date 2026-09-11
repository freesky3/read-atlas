export const OUTLINE_INSPECTOR_MIN = 200;
export const OUTLINE_INSPECTOR_MAX = 420;
export const OUTLINE_INSPECTOR_DEFAULT = 280;

export function clampOutlineInspectorWidth(width: number): number {
  if (!Number.isFinite(width)) return OUTLINE_INSPECTOR_DEFAULT;
  return Math.min(
    OUTLINE_INSPECTOR_MAX,
    Math.max(OUTLINE_INSPECTOR_MIN, Math.round(width)),
  );
}
