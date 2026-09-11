import { useEffect, useRef, useState } from "react";
import { useLocale } from "../i18n/LocaleContext";
import { useDialogFocusTrap } from "../useDialogFocusTrap";

export default function GuideAvatarCropper({ file, onApply, onCancel }: {
  file: File; onApply: (dataUrl: string) => Promise<void>; onCancel: () => void;
}) {
  const { t } = useLocale();
  const [source, setSource] = useState<HTMLImageElement | null>(null);
  const [zoom, setZoom] = useState(1);
  const [x, setX] = useState(50);
  const [y, setY] = useState(50);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const dialogRef = useRef<HTMLDivElement>(null);
  useDialogFocusTrap({ open: true, containerRef: dialogRef, onClose: () => { if (!busy) onCancel(); } });
  useEffect(() => {
    const url = URL.createObjectURL(file);
    const image = new Image();
    let cancelled = false;
    image.onload = () => { if (!cancelled) setSource(image); };
    image.onerror = () => { if (!cancelled) setError(t("guide.crop.decodeError")); };
    image.src = url;
    return () => { cancelled = true; URL.revokeObjectURL(url); };
  }, [file, t]);
  useEffect(() => {
    if (!source) return;
    const context = canvasRef.current?.getContext("2d");
    if (!context) return;
    const size = 256;
    const scale = Math.max(size / source.naturalWidth, size / source.naturalHeight) * zoom;
    const width = source.naturalWidth * scale;
    const height = source.naturalHeight * scale;
    context.clearRect(0, 0, size, size);
    context.drawImage(source, (size - width) * x / 100, (size - height) * y / 100, width, height);
  }, [source, zoom, x, y]);
  async function apply() {
    if (!source || !canvasRef.current) return;
    setBusy(true);
    setError(null);
    try { await onApply(canvasRef.current.toDataURL("image/png")); }
    catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  }
  return <div className="modal-scrim"><div ref={dialogRef} role="dialog" aria-modal="true" aria-label={t("guide.crop.dialogAria")} className="confirm-modal-box">
    <h3>{t("guide.crop.title")}</h3>
    <canvas ref={canvasRef} width={256} height={256} role="img" aria-label={t("guide.crop.previewAria")} style={{ maxWidth: "100%" }} />
    <fieldset disabled={!source || busy}>
      <label>{t("guide.crop.zoom")}<input type="range" min="1" max="3" step="0.05" value={zoom} onChange={(event) => setZoom(Number(event.target.value))} /></label>
      <label>{t("guide.crop.panX")}<input type="range" min="0" max="100" value={x} onChange={(event) => setX(Number(event.target.value))} /></label>
      <label>{t("guide.crop.panY")}<input type="range" min="0" max="100" value={y} onChange={(event) => setY(Number(event.target.value))} /></label>
    </fieldset>
    {error ? <p role="alert">{error}</p> : null}
    <button type="button" className="btn-liquid-pill" disabled={busy} onClick={onCancel}>{t("guide.crop.cancel")}</button>
    <button type="button" className="btn-liquid-pill primary" disabled={!source || busy} onClick={() => void apply()}>{busy ? t("guide.crop.importing") : t("guide.crop.useAvatar")}</button>
  </div></div>;
}
