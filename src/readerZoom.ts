export const READER_ZOOM_MIN = 60;
export const READER_ZOOM_MAX = 180;
export const READER_BASE_SCALE = 1.25;

export function clampReaderZoom(value: number) {
  return Math.min(READER_ZOOM_MAX, Math.max(READER_ZOOM_MIN, Math.round(value)));
}

export function normalizeReaderRotation(
  value: number,
): 0 | 90 | 180 | 270 {
  const normalized = ((Math.round(value / 90) * 90) % 360 + 360) % 360;
  return normalized as 0 | 90 | 180 | 270;
}

export function fitReaderZoom({
  mode,
  containerWidth,
  containerHeight,
  pageWidth,
  pageHeight,
  horizontalChrome = 48,
  verticalChrome = 48,
  baseScale = READER_BASE_SCALE,
}: {
  mode: "width" | "page";
  containerWidth: number;
  containerHeight: number;
  pageWidth: number;
  pageHeight: number;
  horizontalChrome?: number;
  verticalChrome?: number;
  baseScale?: number;
}) {
  const availableWidth = Math.max(1, containerWidth - horizontalChrome);
  const availableHeight = Math.max(1, containerHeight - verticalChrome);
  const widthZoom = (availableWidth / Math.max(1, pageWidth * baseScale)) * 100;
  const heightZoom =
    (availableHeight / Math.max(1, pageHeight * baseScale)) * 100;
  return clampReaderZoom(mode === "width" ? widthZoom : Math.min(widthZoom, heightZoom));
}

export function isPinchZoomWheel(event: {
  ctrlKey: boolean;
  metaKey: boolean;
}): boolean {
  return event.ctrlKey || event.metaKey;
}

export function wheelDeltaPixels(event: {
  deltaY: number;
  deltaMode?: number;
}) {
  const mode = event.deltaMode ?? 0;
  if (mode === 1) return event.deltaY * 16;
  if (mode === 2) return event.deltaY * 800;
  return event.deltaY;
}

export function applyWheelZoom(current: number, deltaPixels: number) {
  const next = current * Math.exp(-deltaPixels * 0.01);
  return Math.min(READER_ZOOM_MAX, Math.max(READER_ZOOM_MIN, next));
}

export function applyPinchZoom(
  startZoom: number,
  startDistance: number,
  currentDistance: number,
) {
  if (startDistance <= 0) return startZoom;
  return Math.min(
    READER_ZOOM_MAX,
    Math.max(READER_ZOOM_MIN, startZoom * (currentDistance / startDistance)),
  );
}

export function pointerDistance(
  first: { x: number; y: number },
  second: { x: number; y: number },
) {
  return Math.hypot(first.x - second.x, first.y - second.y);
}

export function eventOverElement(
  event: {
    target?: EventTarget | null;
    clientX?: number;
    clientY?: number;
  },
  element: {
    contains(node: Node | null): boolean;
    getBoundingClientRect(): {
      left: number;
      right: number;
      top: number;
      bottom: number;
    };
  },
) {
  if (event.target instanceof Node && element.contains(event.target)) {
    return true;
  }
  if (
    typeof event.clientX !== "number" ||
    typeof event.clientY !== "number" ||
    Number.isNaN(event.clientX) ||
    Number.isNaN(event.clientY)
  ) {
    return false;
  }
  const box = element.getBoundingClientRect();
  return (
    event.clientX >= box.left &&
    event.clientX <= box.right &&
    event.clientY >= box.top &&
    event.clientY <= box.bottom
  );
}

export function subscribeNativePinchZoom(onFactor: (factor: number) => void) {
  const onCustom = (event: Event) => {
    const detail = (event as CustomEvent<number>).detail;
    if (typeof detail === "number" && Number.isFinite(detail) && detail > 0) {
      onFactor(detail);
    }
  };
  window.addEventListener("webview-pinch-zoom", onCustom);

  let disposed = false;
  let unlistenTauri = () => {};
  if ("__TAURI_INTERNALS__" in window) {
    void import("@tauri-apps/api/event")
      .then(({ listen }) =>
        listen<number>("webview-pinch-zoom", ({ payload }) => {
          if (
            !disposed &&
            typeof payload === "number" &&
            Number.isFinite(payload) &&
            payload > 0
          ) {
            onFactor(payload);
          }
        }),
      )
      .then((unlisten) => {
        if (disposed) {
          unlisten();
          return;
        }
        unlistenTauri = unlisten;
      })
      .catch(() => {
        /* browser preview and tests have no Tauri IPC */
      });
  }

  return () => {
    disposed = true;
    window.removeEventListener("webview-pinch-zoom", onCustom);
    unlistenTauri();
  };
}
